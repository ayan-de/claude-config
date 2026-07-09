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
use serde::Deserialize;
use serde_json::Value;

use crate::models::{AppError, AppResult};

const PROJECTS_DIR: &str = "projects";
const SESSIONS_INDEX: &str = "sessions-index.json";

/// Schema of Claude Code's per-project `sessions-index.json`. All fields
/// optional except `version` + `entries` so an older index still parses.
#[derive(Debug, Deserialize)]
struct SessionsIndex {
    #[allow(dead_code)]
    version: u32,
    #[serde(default)]
    entries: Vec<SessionIndexEntry>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SessionIndexEntry {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    full_path: String,
    #[serde(default)]
    first_prompt: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    message_count: Option<u32>,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    modified: Option<String>,
    #[serde(default)]
    project_path: Option<String>,
    #[serde(default)]
    is_sidechain: Option<bool>,
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
        out.push(SessionSummary {
            session_id: entry.session_id,
            title: pick_title(entry.summary.as_deref(), entry.first_prompt.as_deref()),
            message_count: entry.message_count.unwrap_or(0),
            modified: entry.modified.or(entry.created),
            project_name: entry
                .project_path
                .as_deref()
                .and_then(last_path_segment),
            full_path: entry.full_path,
        });
    }
}

/// Stat-only summary for a jsonl transcript not yet in the index. We
/// avoid parsing the file — modified time + filename is enough for a
/// placeholder row.
fn summary_from_jsonl_stat(path: &Path) -> Option<SessionSummary> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0))
        .flatten()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));

    let session_id = file_stem(path);
    Some(SessionSummary {
        title: format!("(unindexed) {}", session_id),
        session_id,
        message_count: 0,
        modified,
        project_name: path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string()),
        full_path: path.display().to_string(),
    })
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
}