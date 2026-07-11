//! Lists Claude Code conversation sessions stored on this PC.
//!
//! Claude Code stores transcripts under `<claude_dir>/projects/<encoded-dir>/`.
//! Each project directory has a `sessions-index.json` (cheap pre-scan
//! metadata Claude Code maintains) plus zero or more `<uuid>.jsonl`
//! transcripts. We surface them as a single list sorted by most recent
//! activity, skipping sidechain entries. Honors `CLAUDE_CONFIG_DIR` via
//! `discover_claude_dir()` at the call site.
//!
//! ponytail: single read pass over the index files, no jsonl tail-walking
//! unless the index is missing a transcript. Upgrade to incremental
//! stat-only scan (mtime + size) when the session count climbs past ~500.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{AppError, AppResult};

pub(crate) const PROJECTS_DIR: &str = "projects";
const SESSIONS_INDEX: &str = "sessions-index.json";

/// Schema of Claude Code's per-project `sessions-index.json`. All fields
/// optional except `version` + `entries` so an older index still parses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionsIndex {
    #[allow(dead_code)]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<SessionIndexEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub full_path: String,
    #[serde(default)]
    pub first_prompt: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub message_count: Option<u32>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
}

/// One row for the sidebar Sessions list. Slimmed to what the UI actually
/// renders — keeps the IPC payload small even with hundreds of sessions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    /// `summary` if present, else truncated `first_prompt`, else a
    /// placeholder. Already truncated server-side so the UI doesn't
    /// repeat the work.
    pub title: String,
    pub message_count: u32,
    /// ISO-8601 string from Claude's index. Drives the "5m ago" label
    /// and the sort key.
    pub modified: Option<String>,
    /// Last path segment of `project_path`, e.g. "claude-config" — used
    /// as the row footer.
    pub project_name: Option<String>,
    /// Full decoded project path, e.g. "/home/ayande/Project/claude-config".
    /// Sourced from the transcript's own `cwd` field for unindexed rows
    /// (authoritative), or from the index entry directly. Drives the
    /// accordion grouping. `None` only when both sources fail (empty
    /// transcript + relative slug).
    pub project_path: Option<String>,
    /// Absolute path to the `.jsonl` transcript. Drives tooltips + a
    /// future "Reveal in file manager" action.
    pub full_path: String,
}

/// Cap how many rows we surface. The main view paginates over the full
/// set, so the cap just bounds IPC payload size. Bump if a real user
/// reports the cap biting.
const MAX_ROWS: usize = 1000;
/// Truncate titles to this many chars before they hit the wire. The
/// main view truncates again for display; this ceiling protects the
/// IPC payload when a session's first prompt is huge.
const TITLE_MAX_CHARS: usize = 200;

/// Scans `<claude_dir>/projects/*/sessions-index.json` (plus a jsonl
/// fallback for entries the index missed) and returns the most recent
/// `MAX_ROWS` summaries, newest activity first.
pub fn scan_sessions(claude_dir: &Path) -> AppResult<Vec<SessionSummary>> {
    let projects_dir = claude_dir.join(PROJECTS_DIR);
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = match fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(AppError::Io(std::io::Error::new(
                e.kind(),
                format!("reading projects dir {}: {e}", projects_dir.display()),
            )))
        }
    };

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut out: Vec<SessionSummary> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::warn!("sessions: skipping unreadable project entry: {e}");
                continue;
            }
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let index_path = path.join(SESSIONS_INDEX);
        if index_path.exists() {
            merge_index_into(&index_path, &mut seen_ids, &mut out);
        }

        // Fallback: scan jsonl files in the project dir that the index
        // didn't already account for. Picks up sessions Claude Code
        // started but hasn't yet flushed to the index.
        for jsonl in collect_jsonl_files(&path) {
            let id = file_stem(&jsonl);
            if !seen_ids.insert(id.clone()) {
                continue;
            }
            if let Some(summary) = summary_from_jsonl_stat(&jsonl) {
                out.push(summary);
            }
        }
    }

    // Newest first; entries with no modified fall to the bottom.
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out.truncate(MAX_ROWS);
    Ok(out)
}

fn merge_index_into(
    index_path: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<SessionSummary>,
) {
    let raw = match fs::read_to_string(index_path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("sessions: cannot read {}: {e}", index_path.display());
            return;
        }
    };
    let index: SessionsIndex = match serde_json::from_str(&raw) {
        Ok(i) => i,
        Err(e) => {
            log::warn!("sessions: malformed {}: {e}", index_path.display());
            return;
        }
    };

    for entry in index.entries {
        if entry.is_sidechain.unwrap_or(false) {
            continue;
        }
        if entry.session_id.is_empty() {
            continue;
        }
        if !seen.insert(entry.session_id.clone()) {
            continue;
        }
        let title_from_jsonl = if entry.full_path.is_empty() {
            None
        } else {
            extract_title_from_jsonl(Path::new(&entry.full_path))
        };
        let title = title_from_jsonl
            .unwrap_or_else(|| pick_title(entry.summary.as_deref(), entry.first_prompt.as_deref()));
        out.push(SessionSummary {
            session_id: entry.session_id,
            title,
            message_count: entry.message_count.unwrap_or(0),
            modified: entry.modified.or(entry.created),
            project_name: entry
                .project_path
                .as_deref()
                .and_then(last_path_segment),
            project_path: entry.project_path,
            full_path: entry.full_path,
        });
    }
}

/// Stat-only-ish summary for a jsonl transcript not yet in the index.
/// We do read a few lines to recover the authoritative `cwd` — the
/// folder-slug decoding is ambiguous when a directory name contains `-`
/// (see [`decode_project_slug`]), and the transcript itself carries the
/// unambiguous answer.
fn summary_from_jsonl_stat(path: &Path) -> Option<SessionSummary> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0))
        .flatten()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));

    let project_folder_slug = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());
    // Prefer the `cwd` written into the transcript itself — it's the
    // ground truth. Fall back to filesystem-aware slug decoding when
    // the transcript has no usable `cwd` (empty file, corrupt lines,
    // or an old Claude Code version).
    let project_path = read_cwd_from_transcript(path)
        .or_else(|| project_folder_slug.as_deref().and_then(decode_project_slug));
    let session_id = file_stem(path);
    let title = extract_title_from_jsonl(path)
        .unwrap_or_else(|| format!("(unindexed) {}", session_id));
    Some(SessionSummary {
        title,
        session_id,
        message_count: 0,
        modified,
        project_name: project_folder_slug,
        project_path,
        full_path: path.display().to_string(),
    })
}

/// Extract the authoritative project working directory from a Claude Code
/// transcript. Every user/assistant record carries a `cwd` field, so we
/// only need to scan a handful of leading lines. Bounded by
/// `CWD_SCAN_LINES` to keep this cheap even on transcripts that lead
/// with meta records (`summary`, `file-history-snapshot`, …) before the
/// first real turn.
const CWD_SCAN_LINES: usize = 32;

fn read_cwd_from_transcript(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(CWD_SCAN_LINES) {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
            if !cwd.is_empty() {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

/// Claude Code encodes project paths as on-disk folder names with every
/// `/` replaced by `-`. The mapping is lossy when folder names themselves
/// contain `-` (`/home/ayan-de/Projects/x` →
/// `-home-ayan-de-Projects-x`), so we resolve the ambiguity by walking
/// the filesystem from `/` and treating a `-` as a separator only when
/// the accumulated prefix is a real directory. Prefer
/// [`read_cwd_from_transcript`] when the transcript is available — this
/// is only a display-only fallback for empty/corrupt transcripts.
///
/// Returns `None` when the slug does not start with `-` (i.e. was a
/// relative path).
fn decode_project_slug(slug: &str) -> Option<String> {
    let rest = slug.strip_prefix('-')?;
    let mut path = PathBuf::from("/");
    let mut current = String::new();

    for ch in rest.chars() {
        if ch == '-' {
            let candidate = path.join(&current);
            if candidate.is_dir() {
                path = candidate;
                current.clear();
            } else {
                current.push(ch);
            }
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        path.push(&current);
    }

    Some(path.to_string_lossy().into_owned())
}

fn collect_jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            out.push(p);
        }
    }
    out
}

fn pick_title(summary: Option<&str>, first_prompt: Option<&str>) -> String {
    let raw = summary
        .filter(|s| !s.trim().is_empty())
        .or_else(|| first_prompt.filter(|s| !s.trim().is_empty()))
        .unwrap_or("(untitled session)");
    truncate_chars(raw, TITLE_MAX_CHARS)
}

fn truncate_chars(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Reads the first and last 64KB of a transcript and returns the
/// user-set or auto-generated title. Type-first priority: the user's
/// rename always wins over the AI title, even when the rename has
/// been pushed out of the tail 64KB window by later activity.
///
/// Priority: customTitle (tail) → customTitle (head) → aiTitle (tail)
/// → aiTitle (head) → None.
///
/// `ponytail: head+tail 64KB windows. Differs from upstream /resume's
/// bucket-first order (tail→head, types interleaved) by checking
/// customTitle across both buffers before falling through to aiTitle.
/// Fixes the silent-rename-loss case where a /rename gets tail-evicted
/// and a later aiTitle is in the tail. Same I/O, strictly better.`
pub fn extract_title_from_jsonl(path: &Path) -> Option<String> {
    const WINDOW: u64 = 64 * 1024;
    let head_bytes = read_head_bytes(path, WINDOW).ok()?;
    let tail_bytes = read_tail_bytes(path, WINDOW).ok()?;
    let head = std::str::from_utf8(&head_bytes).ok()?;
    let tail = std::str::from_utf8(&tail_bytes).ok()?;
    let title = extract_last_string_field(tail, "customTitle")
        .or_else(|| extract_last_string_field(head, "customTitle"))
        .or_else(|| extract_last_string_field(tail, "aiTitle"))
        .or_else(|| extract_last_string_field(head, "aiTitle"));
    title.map(|s| truncate_chars(&s, TITLE_MAX_CHARS))
}

/// Returns the first `max_bytes` of the file. Always line-aligned at
/// the start: if the file is smaller than `max_bytes`, returns it
/// whole; if a partial first line would result from a windowed read,
/// the partial line is discarded. (The head read here is always from
/// byte 0, so no skip is needed — but we keep the helper symmetric
/// with [`read_tail_bytes`] for readability.)
fn read_head_bytes(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; max_bytes as usize];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// Returns the last `max_bytes` of the file. When the read does not
/// start at byte 0, the first (partial) line is dropped so callers
/// see line-aligned data.
fn read_tail_bytes(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf)?;
    if start > 0 {
        if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            buf.drain(..=nl);
        }
    }
    Ok(buf)
}

/// Finds the LAST `"<key>":"…"` or `"<key>": "…"` substring in `text`
/// and returns the unescaped value. Returns `None` if no complete
/// match exists in the buffer. Mirrors upstream
/// `extractLastJsonStringField` in spirit (last-one-wins via
/// left-to-right walk) but iterates by char so UTF-8 multibyte
/// continuation bytes can never be mistaken for `\\` or `"`.
fn extract_last_string_field(text: &str, key: &str) -> Option<String> {
    let pat1 = format!("\"{}\":\"", key);
    let pat2 = format!("\"{}\": \"", key);
    let mut last: Option<String> = None;
    for pat in [&pat1, &pat2] {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(pat.as_str()) {
            let val_start = from + rel + pat.len();
            // Walk chars to find the closing quote, skipping
            // JSON-escaped chars (\\, \", \n, \uXXXX).
            let mut chars = text[val_start..].char_indices().peekable();
            let mut close_off: Option<usize> = None;
            while let Some((off, c)) = chars.next() {
                if c == '\\' {
                    if chars.next().is_none() {
                        break;
                    }
                    continue;
                }
                if c == '"' {
                    close_off = Some(off);
                    break;
                }
            }
            match close_off {
                Some(off) => {
                    let raw = &text[val_start..val_start + off];
                    last = Some(unescape_json_string(raw));
                    from = val_start + off + '"'.len_utf8();
                }
                None => {
                    // Truncated buffer; no complete value here.
                    from = text.len();
                    break;
                }
            }
        }
    }
    last
}

/// Parses a JSON string literal body and returns the unescaped value.
/// Falls back to the raw input on parse error (shouldn't happen for
/// well-formed input — defensive only).
fn unescape_json_string(raw: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{}\"", raw)).unwrap_or_else(|_| raw.to_string())
}

fn last_path_segment(p: &str) -> Option<String> {
    let trimmed = p.trim_end_matches('/');
    let last = trimmed.rsplit('/').next()?;
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// One message in a parsed session transcript. Mirrors jcode's
/// `PreviewMessage` shape (see `crates/jcode-tui-session-picker/src/lib.rs:159`)
/// but flattened — the React renderer decides how to style each role.
///
/// ponytail: text-only payload, no markdown flag, no tool_use input JSON.
/// Renderer can show `tool_name` as a header and `is_tool_result` as a
/// dim box. Add structured blocks (thinking / tool_use / tool_result)
/// when the renderer needs to differentiate them.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub tool_name: Option<String>,
    pub is_tool_result: bool,
}

/// Cap messages per transcript. Long sessions (1k+ turns) still fit;
/// this just protects the IPC payload from absurd outliers.
const MAX_TRANSCRIPT_MESSAGES: usize = 5000;

/// Reads an entire Claude Code `.jsonl` transcript and returns a flat
/// list of messages. Unlike the picker-style tail-read, this reads the
/// full file — the viewer is the natural place to show everything.
///
/// Lines that fail to parse are silently skipped (jcode's `loading.rs:1894`
/// pattern: never abort the preview on one malformed record).
pub fn parse_session_transcript(path: &Path) -> AppResult<Vec<SessionMessage>> {
    let file = fs::File::open(path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("opening transcript {}: {e}", path.display()),
        ))
    })?;
    let reader = BufReader::new(file);

    let mut out: Vec<SessionMessage> = Vec::new();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let entry_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        // Drop system / summary / file-history-snapshot / queue-operation
        // entries — jcode keeps only user + assistant.
        if entry_type != "user" && entry_type != "assistant" {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or(entry_type)
            .to_string();
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string);

        // Iterate content blocks once, emitting one row per block that
        // carries text. tool_use blocks → row with tool_name set and
        // content from the input's `description`/`command`/`file_path`
        // when present. tool_result blocks → row with is_tool_result=true.
        if let Some(content) = message.get("content") {
            if content.is_string() {
                if let Some(text) = content.as_str() {
                    push_message(
                        &mut out,
                        role.clone(),
                        text.to_string(),
                        timestamp.clone(),
                        None,
                        false,
                    );
                }
            } else if let Some(arr) = content.as_array() {
                for block in arr {
                    let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
                    match kind {
                        "text" | "input_text" | "output_text" => {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                push_message(
                                    &mut out,
                                    role.clone(),
                                    text.to_string(),
                                    timestamp.clone(),
                                    None,
                                    false,
                                );
                            }
                        }
                        "thinking" => {
                            if let Some(text) =
                                block.get("thinking").and_then(Value::as_str)
                            {
                                // ponytail: thinking is folded into the
                                // assistant prose with a leading marker so
                                // the renderer can dim it; add a separate
                                // `kind` discriminator when a real viewer
                                // needs to collapse it.
                                push_message(
                                    &mut out,
                                    role.clone(),
                                    format!("[thinking] {text}"),
                                    timestamp.clone(),
                                    None,
                                    false,
                                );
                            }
                        }
                        "tool_use" => {
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_string();
                            let summary = summarize_tool_input(
                                block.get("input").unwrap_or(&Value::Null),
                            );
                            push_message(
                                &mut out,
                                "tool".to_string(),
                                summary,
                                timestamp.clone(),
                                Some(name),
                                false,
                            );
                        }
                        "tool_result" => {
                            let content = block
                                .get("content")
                                .map(tool_result_to_text)
                                .unwrap_or_default();
                            push_message(
                                &mut out,
                                "tool".to_string(),
                                content,
                                timestamp.clone(),
                                None,
                                true,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        if out.len() >= MAX_TRANSCRIPT_MESSAGES {
            break;
        }
    }
    Ok(out)
}

fn push_message(
    out: &mut Vec<SessionMessage>,
    role: String,
    content: String,
    timestamp: Option<String>,
    tool_name: Option<String>,
    is_tool_result: bool,
) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    out.push(SessionMessage {
        role,
        content: trimmed.to_string(),
        timestamp,
        tool_name,
        is_tool_result,
    });
}

/// One-line summary for a tool_use block. Picks the most identifying
/// field so the renderer doesn't need to render raw JSON.
fn summarize_tool_input(input: &Value) -> String {
    let obj = match input.as_object() {
        Some(o) => o,
        None => return String::new(),
    };
    for key in [
        "command",
        "file_path",
        "path",
        "description",
        "query",
        "url",
        "pattern",
    ] {
        if let Some(v) = obj.get(key).and_then(Value::as_str) {
            return v.to_string();
        }
    }
    String::new()
}

/// tool_result.content is a string OR an array of content blocks.
fn tool_result_to_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(arr) = value.as_array() {
        let mut out: Vec<String> = Vec::new();
        for block in arr {
            let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
            match kind {
                "text" => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        out.push(t.to_string());
                    }
                }
                "image" => out.push("[image]".to_string()),
                _ => {}
            }
        }
        return out.join("\n");
    }
    String::new()
}

/// Atomic read-modify-write on `<project_folder>/sessions-index.json`.
/// Inserts or replaces the entry with matching `session_id`. Existing
/// entries are preserved. Creates the index file (and its parent
/// directories) if absent. Uses a sidecar lock file matching the
/// pattern in `storage::github_sync::write_session_sync_state_atomic`
/// to avoid races with a live Claude Code process appending to the
/// same index.
pub fn upsert_into_sessions_index(
    project_folder: &Path,
    entry: &SessionIndexEntry,
) -> AppResult<()> {
    use fs2::FileExt;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fs::create_dir_all(project_folder)?;
    let index_path = project_folder.join(SESSIONS_INDEX);

    // Sidecar lock — hold exclusively for the duration of the write.
    let lock_path = {
        let mut p = index_path.as_os_str().to_owned();
        p.push(".lock");
        PathBuf::from(p)
    };
    let lock_file = fs::File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| AppError::Lock(e.to_string()))?;

    let write_result = (|| -> AppResult<()> {
        let mut index: SessionsIndex = match fs::read(&index_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(SessionsIndex {
                version: 1,
                entries: Vec::new(),
            }),
            Err(_) => SessionsIndex {
                version: 1,
                entries: Vec::new(),
            },
        };
        index.version = 1;
        let existing = index
            .entries
            .iter()
            .position(|e| e.session_id == entry.session_id);
        match existing {
            Some(i) => index.entries[i] = entry.clone(),
            None => index.entries.push(entry.clone()),
        }

        let parent = index_path
            .parent()
            .unwrap_or(project_folder);
        let mut tmp = NamedTempFile::new_in(parent)?;
        let body = serde_json::to_vec_pretty(&index)?;
        tmp.write_all(&body)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&index_path).map_err(|e| {
            AppError::Io(std::io::Error::other(format!(
                "persist sessions-index.json: {e}"
            )))
        })?;
        Ok(())
    })();

    let _ = lock_file.unlock();
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn missing_projects_dir_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let out = scan_sessions(tmp.path()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn reads_index_and_skips_sidechains() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "projects/-home-ayande-claude-config/sessions-index.json",
            r#"{
                "version": 1,
                "entries": [
                    {
                        "sessionId": "abc",
                        "fullPath": "/tmp/abc.jsonl",
                        "summary": "Refactor sidebar",
                        "firstPrompt": "refactor the sidebar",
                        "messageCount": 12,
                        "modified": "2026-07-09T10:00:00Z",
                        "created": "2026-07-09T09:50:00Z",
                        "projectPath": "/home/ayande/claude-config"
                    },
                    {
                        "sessionId": "side",
                        "fullPath": "/tmp/side.jsonl",
                        "firstPrompt": "ignore me",
                        "isSidechain": true
                    }
                ]
            }"#,
        );

        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "abc");
        assert_eq!(out[0].title, "Refactor sidebar");
        assert_eq!(out[0].message_count, 12);
        assert_eq!(out[0].project_name.as_deref(), Some("claude-config"));
        assert_eq!(out[0].modified.as_deref(), Some("2026-07-09T10:00:00Z"));
    }

    #[test]
    fn sorts_by_modified_desc() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "projects/-home-x/sessions-index.json",
            r#"{
                "version": 1,
                "entries": [
                    {"sessionId": "old", "fullPath": "/x/old.jsonl", "summary": "Old", "modified": "2026-01-01T00:00:00Z"},
                    {"sessionId": "new", "fullPath": "/x/new.jsonl", "summary": "New", "modified": "2026-07-09T00:00:00Z"},
                    {"sessionId": "mid", "fullPath": "/x/mid.jsonl", "summary": "Mid", "modified": "2026-03-01T00:00:00Z"}
                ]
            }"#,
        );
        let out = scan_sessions(tmp.path()).unwrap();
        let ids: Vec<_> = out.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["new", "mid", "old"]);
    }

    #[test]
    fn title_falls_back_to_first_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "projects/-home-x/sessions-index.json",
            r#"{
                "version": 1,
                "entries": [
                    {"sessionId": "a", "fullPath": "/x/a.jsonl", "firstPrompt": "Help me write tests"}
                ]
            }"#,
        );
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out[0].title, "Help me write tests");
    }

    #[test]
    fn title_truncates_long_strings() {
        let tmp = tempfile::tempdir().unwrap();
        let long = "x".repeat(500);
        let payload = format!(
            r#"{{"version":1,"entries":[{{"sessionId":"a","fullPath":"/x/a.jsonl","summary":"{long}"}}]}}"#
        );
        write(tmp.path(), "projects/-home-x/sessions-index.json", &payload);
        let out = scan_sessions(tmp.path()).unwrap();
        assert!(out[0].title.chars().count() <= TITLE_MAX_CHARS);
        assert!(out[0].title.ends_with('…'));
    }

    #[test]
    fn jsonl_files_outside_index_become_placeholder_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        // No index, just one transcript file.
        fs::write(proj.join("orphan.jsonl"), "{}\n").unwrap();
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "orphan");
        assert!(out[0].title.contains("unindexed"));
        assert!(out[0].modified.is_some());
    }

    #[test]
    fn unindexed_session_recovers_cwd_from_transcript() {
        // Ground truth is written into every user/assistant record, so
        // the slug-decoding ambiguity around `-` in folder names doesn't
        // reach the UI when the transcript is readable.
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp
            .path()
            .join("projects/-home-ayan-de-Projects-githubProjects-jcode");
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            proj.join("s.jsonl"),
            concat!(
                "{\"type\":\"user\",\"cwd\":\"/home/ayan-de/Projects/githubProjects/jcode\",",
                "\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
            ),
        )
        .unwrap();
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].project_path.as_deref(),
            Some("/home/ayan-de/Projects/githubProjects/jcode"),
        );
    }

    #[test]
    fn unindexed_session_scans_past_meta_records_for_cwd() {
        // Meta records (summary / file-history-snapshot) can appear
        // before the first user turn — cwd is on the user record.
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-ayan-de-x");
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            proj.join("s.jsonl"),
            concat!(
                "{\"type\":\"summary\",\"summary\":\"a\"}\n",
                "{\"type\":\"file-history-snapshot\"}\n",
                "{\"type\":\"user\",\"cwd\":\"/home/ayan-de/x\",",
                "\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
            ),
        )
        .unwrap();
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out[0].project_path.as_deref(), Some("/home/ayan-de/x"));
    }

    #[test]
    fn duplicate_session_ids_in_two_indexes_are_collapsed() {
        // Defensive: a user with two claude_dirs and a symlinked project
        // could see the same id twice. We dedupe by id.
        let tmp = tempfile::tempdir().unwrap();
        let idx = r#"{"version":1,"entries":[{"sessionId":"dup","fullPath":"/x.jsonl","summary":"S"}]}"#;
        write(tmp.path(), "projects/-a/sessions-index.json", idx);
        write(tmp.path(), "projects/-b/sessions-index.json", idx);
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
    }

    fn write_transcript(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn parse_session_returns_user_and_assistant_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "happy.jsonl",
            concat!(
                "{\"type\":\"user\",\"timestamp\":\"2026-07-09T10:00:00Z\",",
                "\"message\":{\"role\":\"user\",\"content\":\"Fix the flaky test\"}}\n",
                "{\"type\":\"assistant\",\"timestamp\":\"2026-07-09T10:00:05Z\",",
                "\"message\":{\"role\":\"assistant\",\"content\":[",
                "{\"type\":\"text\",\"text\":\"I found the race condition\"}]}}\n",
            ),
        );
        let out = parse_session_transcript(&path).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content, "Fix the flaky test");
        assert_eq!(out[0].timestamp.as_deref(), Some("2026-07-09T10:00:00Z"));
        assert_eq!(out[1].role, "assistant");
        assert_eq!(out[1].content, "I found the race condition");
    }

    #[test]
    fn parse_session_skips_malformed_and_unrelated_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "messy.jsonl",
            concat!(
                "{not valid json\n",
                "\n",
                "{\"type\":\"summary\",\"message\":{\"role\":\"system\"}}\n",
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
                "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",",
                "\"content\":\"hi back\"}}\n",
            ),
        );
        let out = parse_session_transcript(&path).unwrap();
        let roles: Vec<_> = out.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, vec!["user", "assistant"]);
    }

    #[test]
    fn parse_session_extracts_tool_use_and_tool_result() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "tools.jsonl",
            concat!(
                "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",",
                "\"content\":[",
                "{\"type\":\"tool_use\",\"id\":\"1\",\"name\":\"Bash\",",
                "\"input\":{\"command\":\"ls -la\"}}]}}\n",
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",",
                "\"content\":[",
                "{\"type\":\"tool_result\",\"tool_use_id\":\"1\",",
                "\"content\":[{\"type\":\"text\",\"text\":\"file.txt\"}]}]}}\n",
            ),
        );
        let out = parse_session_transcript(&path).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].role, "tool");
        assert_eq!(out[0].tool_name.as_deref(), Some("Bash"));
        assert_eq!(out[0].content, "ls -la");
        assert!(!out[0].is_tool_result);
        assert!(out[1].is_tool_result);
        assert_eq!(out[1].content, "file.txt");
    }

    #[test]
    fn parse_session_returns_empty_for_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(tmp.path(), "empty.jsonl", "");
        let out = parse_session_transcript(&path).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_session_marks_thinking_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "thinking.jsonl",
            concat!(
                "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",",
                "\"content\":[",
                "{\"type\":\"thinking\",\"thinking\":\"pondering\"},",
                "{\"type\":\"text\",\"text\":\"answer\"}]}}\n",
            ),
        );
        let out = parse_session_transcript(&path).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out[0].content.starts_with("[thinking]"));
        assert_eq!(out[1].content, "answer");
    }

    // ---- extract_title_from_jsonl ----

    #[test]
    fn title_from_jsonl_returns_custom_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n\
             {\"type\":\"custom-title\",\"customTitle\":\"My Renamed Session\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("My Renamed Session"),
        );
    }

    #[test]
    fn title_from_jsonl_returns_ai_title_when_no_custom_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n\
             {\"type\":\"ai-title\",\"aiTitle\":\"Auto generated title\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("Auto generated title"),
        );
    }

    #[test]
    fn title_from_jsonl_prefers_custom_over_ai() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"ai-title\",\"aiTitle\":\"Auto\"}\n\
             {\"type\":\"custom-title\",\"customTitle\":\"Manual\"}\n",
        );
        assert_eq!(extract_title_from_jsonl(&path).as_deref(), Some("Manual"));
    }

    #[test]
    fn title_from_jsonl_uses_last_custom_title_when_renamed_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"custom-title\",\"customTitle\":\"First rename\"}\n\
             {\"type\":\"custom-title\",\"customTitle\":\"Second rename\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("Second rename"),
        );
    }

    /// The bucket-order fix: customTitle is in the head 64KB only
    /// (tail-evicted), aiTitle is in the tail. Type-first priority
    /// must return the customTitle regardless of position.
    #[test]
    fn title_from_jsonl_prefers_head_custom_title_over_tail_ai_title() {
        let tmp = tempfile::tempdir().unwrap();
        // Build a file large enough that head and tail don't overlap.
        // 80KB of padding (between 64KB and 128KB) forces any 64KB
        // window to see only one of the two entries.
        let mut content = String::new();
        content.push_str("{\"type\":\"custom-title\",\"customTitle\":\"User rename\"}\n");
        // Pad to push the aiTitle past the head 64KB window.
        let pad_lines = 80 * 1024 / 64; // 1280 lines of ~64 bytes
        for i in 0..pad_lines {
            content.push_str(&format!(
                "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"message {} that is long enough to push the line past a boundary\"}}}}\n",
                i
            ));
        }
        content.push_str("{\"type\":\"ai-title\",\"aiTitle\":\"Auto title\"}\n");
        let path = write_transcript(tmp.path(), "s.jsonl", &content);

        // Sanity: file is > 64KB so the two windows don't fully overlap.
        let len = fs::metadata(&path).unwrap().len();
        assert!(len > 64 * 1024, "test fixture should exceed 64KB; got {} bytes", len);

        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("User rename"),
        );
    }

    #[test]
    fn title_from_jsonl_returns_none_when_no_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        );
        assert_eq!(extract_title_from_jsonl(&path), None);
    }

    #[test]
    fn title_from_jsonl_unescapes_quoted_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"custom-title\",\"customTitle\":\"He said \\\"hi\\\"\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("He said \"hi\""),
        );
    }

    #[test]
    fn indexed_session_uses_jsonl_title_over_summary() {
        let tmp = tempfile::tempdir().unwrap();
        // Create the transcript first so we can reference its real path.
        let session_id = "11111111-2222-3333-4444-555555555555";
        let transcript = write_transcript(
            tmp.path(),
            &format!("{}.jsonl", session_id),
            "{\"type\":\"custom-title\",\"customTitle\":\"Jsonl Wins\"}\n",
        );
        let full_path = transcript.display().to_string();

        write(
            tmp.path(),
            "projects/-home-test/sessions-index.json",
            &format!(
                r#"{{
                    "version": 1,
                    "entries": [{{
                        "sessionId": "{sid}",
                        "fullPath": "{fp}",
                        "summary": "Index summary loses",
                        "firstPrompt": "first prompt loses",
                        "messageCount": 3,
                        "modified": "2026-07-09T10:00:00Z",
                        "projectPath": "/home/test"
                    }}]
                }}"#,
                sid = session_id,
                fp = full_path,
            ),
        );

        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, session_id);
        assert_eq!(out[0].title, "Jsonl Wins");
    }

    #[test]
    fn unindexed_session_uses_jsonl_title_not_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        // A transcript under projects/... with NO sessions-index.json
        // nearby. The scanner will fall through to summary_from_jsonl_stat.
        write(
            tmp.path(),
            "projects/-home-test/abcdef.jsonl",
            "{\"type\":\"custom-title\",\"customTitle\":\"Unindexed but titled\"}\n",
        );
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "abcdef");
        assert_eq!(out[0].title, "Unindexed but titled");
    }

    #[test]
    fn unindexed_session_falls_back_to_placeholder_when_no_title() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "projects/-home-test/no-title-here.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        );
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].title,
            "(unindexed) no-title-here",
            "untitled unindexed sessions should keep the uuid placeholder"
        );
    }

    // ---- upsert_into_sessions_index ----

    fn entry(id: &str, full_path: &str, summary: Option<&str>) -> SessionIndexEntry {
        SessionIndexEntry {
            session_id: id.to_string(),
            full_path: full_path.to_string(),
            first_prompt: None,
            summary: summary.map(String::from),
            message_count: Some(0),
            created: Some("2026-07-11T10:00:00Z".to_string()),
            modified: Some("2026-07-11T10:00:00Z".to_string()),
            project_path: Some("/home/test".to_string()),
            is_sidechain: Some(false),
        }
    }

    #[test]
    fn upsert_creates_index_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("projects/-home-test");
        std::fs::create_dir_all(&folder).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a.jsonl", None)).unwrap();
        let raw = std::fs::read_to_string(folder.join("sessions-index.json")).unwrap();
        assert!(raw.contains("\"sessionId\": \"a\""), "raw was:\n{raw}");
        assert!(raw.contains("\"version\": 1"), "raw was:\n{raw}");
    }

    #[test]
    fn upsert_preserves_existing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("projects/-home-test");
        std::fs::create_dir_all(&folder).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a.jsonl", Some("first"))).unwrap();
        upsert_into_sessions_index(&folder, &entry("b", "/path/b.jsonl", Some("second"))).unwrap();
        let raw = std::fs::read_to_string(folder.join("sessions-index.json")).unwrap();
        assert!(raw.contains("\"sessionId\": \"a\""), "raw was:\n{raw}");
        assert!(raw.contains("\"sessionId\": \"b\""), "raw was:\n{raw}");
        assert!(raw.contains("first"), "raw was:\n{raw}");
    }

    #[test]
    fn upsert_replaces_existing_entry_by_session_id() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("projects/-home-test");
        std::fs::create_dir_all(&folder).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a-old.jsonl", Some("v1"))).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a-new.jsonl", Some("v2"))).unwrap();
        let raw = std::fs::read_to_string(folder.join("sessions-index.json")).unwrap();
        assert!(raw.contains("a-new.jsonl"), "raw was:\n{raw}");
        assert!(!raw.contains("a-old.jsonl"), "raw was:\n{raw}");
        assert!(raw.contains("v2"), "raw was:\n{raw}");
        assert!(!raw.contains("v1"), "raw was:\n{raw}");
    }
}