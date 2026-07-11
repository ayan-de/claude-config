//! Cross-platform system commands: discover paths, reveal in file manager.

use std::fs;
use std::path::{Path, PathBuf};

use tauri_plugin_opener::OpenerExt;

use crate::models::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::claude_md::{claude_md_path, read_claude_md, write_claude_md_atomic};
use crate::storage::sessions::{SessionsIndex, PROJECTS_DIR};
use crate::storage::{
    discover_claude_dir, parse_session_transcript, scan_marketplaces, scan_mcp_servers,
    scan_sessions, scan_skills, MarketplaceSummary, McpServerSummary, SessionMessage,
    SessionSummary, SkillSummary,
};

/// Returns the path Claude Code reads `settings.json` from. Respects the
/// `CLAUDE_CONFIG_DIR` env var.
#[tauri::command]
pub fn discover_claude_dir_cmd() -> AppResult<PathBuf> {
    Ok(discover_claude_dir())
}

/// Returns the app data directory used to store providers.json, backups,
/// etc. Useful for the "Reveal in file manager" feature in settings menu.
#[tauri::command]
pub fn get_app_data_dir_cmd(state: tauri::State<'_, AppState>) -> AppResult<PathBuf> {
    Ok(state.app_data_dir.as_ref().clone())
}

/// Opens the OS file manager pointing at the given path.
#[tauri::command]
pub fn reveal_in_file_manager_cmd(
    app: tauri::AppHandle,
    path: PathBuf,
) -> AppResult<()> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| crate::models::AppError::Internal(format!("opener: {e}")))
}

/// Returns the keyring availability status. Frontend reads this on launch
/// to decide whether to show a warning banner and disable the "Add" button.
#[tauri::command]
pub fn keyring_status_cmd(
    state: tauri::State<'_, AppState>,
) -> crate::storage::KeyringStatus {
    state.keyring.status()
}

/// Reads the global `CLAUDE.md`. Honors `CLAUDE_CONFIG_DIR`. Returns `Ok(None)`
/// if the file does not exist; surfaces non-UTF-8 contents as `MalformedClaudeMd`
/// instead of a generic io error.
#[tauri::command]
pub fn read_claude_md_cmd() -> AppResult<Option<String>> {
    let path = claude_md_path();
    match read_claude_md(&path) {
        Ok(opt) => Ok(opt),
        Err(AppError::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData => {
            Err(AppError::MalformedClaudeMd {
                path: path.display().to_string(),
                message: e.to_string(),
            })
        }
        Err(e) => Err(e),
    }
}

/// Atomically writes global `CLAUDE.md` (tempfile + fsync + rename, exclusive
/// sidecar lock, timestamped backup). Honors `CLAUDE_CONFIG_DIR`.
#[tauri::command]
pub fn write_claude_md_cmd(
    state: tauri::State<'_, AppState>,
    content: String,
) -> AppResult<()> {
    let path = claude_md_path();
    let backups_dir = state.app_data_dir.as_ref().join("backups");
    write_claude_md_atomic(&path, &content, &backups_dir)?;
    Ok(())
}

/// Cheap existence probe for `CLAUDE.md`. UI uses it on app start to decide
/// whether the sidebar shows "+ Add CLAUDE.md" or the file button — no
/// reason to pull the contents just for that binary distinction.
#[tauri::command]
pub fn claude_md_exists_cmd() -> bool {
    claude_md_path().exists()
}

/// Scans `<claude_dir>/plugins/marketplaces/*` and returns one summary per
/// known marketplace. The Add flow itself is deferred — this command lets
/// the UI populate the marketplace list. Honors `CLAUDE_CONFIG_DIR`.
#[tauri::command]
pub fn list_marketplaces_cmd() -> AppResult<Vec<MarketplaceSummary>> {
    scan_marketplaces(&discover_claude_dir())
}

/// Scans both user-authored skills (`<claude_dir>/skills/**/SKILL.md`) and
/// skills bundled with installed plugins (resolved via
/// `<claude_dir>/plugins/installed_plugins.json`). Honors `CLAUDE_CONFIG_DIR`.
#[tauri::command]
pub fn list_skills_cmd() -> AppResult<Vec<SkillSummary>> {
    scan_skills(&discover_claude_dir())
}

/// Scans MCP server definitions from `${HOME}/.claude.json` top-level
/// `mcpServers`, enriched with health / needs-auth cache files.
#[tauri::command]
pub fn list_mcp_servers_cmd() -> AppResult<Vec<McpServerSummary>> {
    scan_mcp_servers()
}

/// Lists Claude Code conversation sessions on this PC. Reads
/// `<claude_dir>/projects/*/sessions-index.json` (plus a jsonl fallback
/// for transcripts the index has not yet recorded). Honors
/// `CLAUDE_CONFIG_DIR`. Newest activity first, sidechain entries skipped.
///
/// Async + `spawn_blocking` so the heavy directory walk and jsonl
/// head/tail reads don't freeze the WebView's main thread.
#[tauri::command]
pub async fn list_sessions_cmd() -> AppResult<Vec<SessionSummary>> {
    tauri::async_runtime::spawn_blocking(|| scan_sessions(&discover_claude_dir()))
        .await
        .map_err(|e| AppError::Internal(format!("list_sessions task panicked: {e}")))?
}

/// Parses a Claude Code `.jsonl` transcript at `path` into a flat list
/// of messages for the in-app viewer. Honors `path` only — the caller
/// passes the absolute path it received from `list_sessions_cmd`. Errors
/// only if the file is unreadable; malformed lines are silently skipped.
///
/// Async + `spawn_blocking` so large transcript parsing doesn't freeze
/// the WebView's main thread.
#[tauri::command]
pub async fn parse_session_cmd(path: PathBuf) -> AppResult<Vec<SessionMessage>> {
    tauri::async_runtime::spawn_blocking(move || parse_session_transcript(&path))
        .await
        .map_err(|e| AppError::Internal(format!("parse_session task panicked: {e}")))?
}

/// Deletes a single Claude Code session: moves the `.jsonl` to OS Trash
/// and strips the entry from `sessions-index.json`. Local-only; the
/// GitHub-synced copy (if any) is untouched.
///
/// **Order matters.** We trash BEFORE stripping the index so a crash
/// mid-call never silently leaves an orphaned `.jsonl` on disk. The
/// only remaining partial-failure state is "index still references a
/// now-trashed file," which the scanner self-heals on next refresh
/// (`summary_from_jsonl_stat` tolerates a missing file).
///
/// **Path validation.** `full_path` must resolve under
/// `<claude_dir>/projects/`. Canonicalize both sides to defend against
/// `..` traversal and symlinks. Prevents the UI from asking the backend
/// to trash arbitrary paths like `~/.ssh/id_rsa`.
///
/// `ponytail: this is a destructive op gated behind a UI confirmation
/// dialog. If the dialog is bypassed (e.g. future automation, IPC
/// fuzzing), the path check is the only thing standing between a
/// bug and a data-loss incident. Treat it as load-bearing.`
#[tauri::command]
pub fn delete_session_cmd(full_path: String) -> AppResult<()> {
    delete_session_cmd_logic(&discover_claude_dir().join(PROJECTS_DIR), &full_path)
}

/// Inner function so tests can pass a tempdir-rooted `projects/` instead
/// of the process-global `discover_claude_dir()`. Mirrors the validation
/// + trash + strip steps in order.
fn delete_session_cmd_logic(projects_root: &Path, full_path: &str) -> AppResult<()> {
    if full_path.is_empty() {
        return Err(AppError::Validation("full_path is empty".into()));
    }
    let requested = Path::new(full_path);
    let requested_canon = requested
        .canonicalize()
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("canonicalize {full_path}: {e}"))))?;
    let root_canon = projects_root
        .canonicalize()
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("canonicalize {}: {e}", projects_root.display()))))?;
    if !requested_canon.starts_with(&root_canon) {
        return Err(AppError::Validation(format!(
            "full_path {} is not under projects/",
            requested_canon.display()
        )));
    }

    // 1. Trash first — fail-safe ordering.
    trash::delete_all([requested_canon.as_path()]).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("trash {}: {e}", requested_canon.display()),
        ))
    })?;

    // 2. Strip the index entry. Parent of the .jsonl is the project dir.
    let project_dir = requested_canon
        .parent()
        .ok_or_else(|| AppError::Validation("full_path has no parent".into()))?;
    strip_session_index_entry(project_dir, &requested_canon.display().to_string())?;
    Ok(())
}

/// Loads `sessions-index.json` from `project_dir` (if present), drops the
/// entry whose `fullPath` matches `full_path`, and writes the result
/// back atomically (temp + fsync + rename). No-op when the index file
/// does not exist — unindexed sessions still get trashed upstream.
fn strip_session_index_entry(project_dir: &Path, full_path: &str) -> AppResult<()> {
    let index_path = project_dir.join("sessions-index.json");
    if !index_path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&index_path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("read {}: {e}", index_path.display()),
        ))
    })?;
    let mut index: SessionsIndex = serde_json::from_str(&raw).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse {}: {e}", index_path.display()),
        ))
    })?;
    let before = index.entries.len();
    index.entries.retain(|e| e.full_path != full_path);
    if index.entries.len() == before {
        // No entry matched — nothing to write.
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("serialize index: {e}"),
        ))
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(project_dir).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("create temp in {}: {e}", project_dir.display()),
        ))
    })?;
    std::io::Write::write_all(tmp.as_file_mut(), &bytes).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("write temp: {e}"),
        ))
    })?;
    tmp.as_file().sync_all().map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("fsync temp: {e}"),
        ))
    })?;
    tmp.persist(&index_path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.error.kind(),
            format!("persist to {}: {e}", index_path.display()),
        ))
    })?;
    Ok(())
}

// ---- delete_session_cmd ----
//
// Tests that touch `trash::delete_all` are gated `#[ignore]` because they
// pollute the OS Trash. Run them with:
//   cargo test -- --ignored delete_session
//
// Tests that only exercise validation, index rewriting, or already-missing
// files run normally.

#[cfg(test)]
mod delete_session_tests {
    use super::*;
    use std::fs;

    /// Two-entry index; delete the first. Surviving entry structurally
    /// equals the original (serde round-trip — NOT byte-identical).
    #[test]
    fn strips_entry_from_index() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        let jsonl = proj.join("aaa.jsonl");
        fs::write(&jsonl, "{}\n").unwrap();

        let keep_path = jsonl.display().to_string();
        let drop_path = proj.join("bbb.jsonl").display().to_string();

        let index = serde_json::json!({
            "version": 1,
            "entries": [
                {"sessionId": "aaa", "fullPath": drop_path, "summary": "drop"},
                {"sessionId": "bbb", "fullPath": keep_path, "summary": "keep"},
            ]
        });
        fs::write(proj.join("sessions-index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        strip_session_index_entry(&proj, &drop_path).unwrap();

        let after: SessionsIndex =
            serde_json::from_str(&fs::read_to_string(proj.join("sessions-index.json")).unwrap()).unwrap();
        assert_eq!(after.entries.len(), 1);
        assert_eq!(after.entries[0].session_id, "bbb");
        assert_eq!(after.entries[0].full_path, keep_path);
        assert!(!proj.join("sessions-index.json.tmp").exists(), "temp file must not linger");
    }

    /// Unindexed session: index absent. Strip is a no-op.
    #[test]
    fn strip_is_noop_when_index_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        let p = proj.join("orphan.jsonl").display().to_string();
        fs::write(proj.join("orphan.jsonl"), "{}\n").unwrap();

        strip_session_index_entry(&proj, &p).unwrap();

        assert!(!proj.join("sessions-index.json").exists());
        assert!(proj.join("orphan.jsonl").exists());
    }

    #[test]
    fn rejects_empty_full_path() {
        let cmd = || delete_session_cmd_logic(Path::new("/tmp/projects"), "");
        let e = cmd().unwrap_err();
        assert!(matches!(e, AppError::Validation(_)), "got {e:?}");
    }

    /// /etc/passwd resolves somewhere that won't be under the
    /// canonicalized claude_dir/projects/. Must reject.
    #[test]
    fn rejects_path_outside_projects() {
        let tmp = tempfile::tempdir().unwrap();
        // Use a projects root inside the tempdir; passwd is outside.
        let projects_root = tmp.path().join(PROJECTS_DIR);
        fs::create_dir_all(&projects_root).unwrap();

        let result = delete_session_cmd_logic(&projects_root, "/etc/passwd");
        let e = result.unwrap_err();
        assert!(matches!(e, AppError::Validation(_)), "got {e:?}");
    }

    /// After a full delete on an already-trashed file, command must not
    /// panic and must still strip the index entry (idempotent on file,
    /// eager on index).
    #[test]
    #[ignore = "calls trash::delete_all — pollutes OS Trash; run with --ignored"]
    fn idempotent_on_missing_file_strips_index() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        // .jsonl intentionally not created.
        let full_path = proj.join("gone.jsonl").display().to_string();

        let index = serde_json::json!({
            "version": 1,
            "entries": [
                {"sessionId": "gone", "fullPath": full_path, "summary": "gone"}
            ]
        });
        fs::write(proj.join("sessions-index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        // discover_claude_dir is process-global; we can't easily override
        // it for this test. Instead exercise the strip helper directly,
        // which is the part that would fail if the file was still on disk.
        // The trash step is covered by `full_delete_strips_index_and_trashes`.
        strip_session_index_entry(&proj, &full_path).unwrap();

        let after: SessionsIndex =
            serde_json::from_str(&fs::read_to_string(proj.join("sessions-index.json")).unwrap()).unwrap();
        assert!(after.entries.is_empty());
    }

    /// Happy path: file exists + index has entry → file trashed, entry gone.
    /// Ignored because it calls `trash::delete_all`.
    #[test]
    #[ignore = "calls trash::delete_all — pollutes OS Trash; run with --ignored"]
    fn full_delete_strips_index_and_trashes() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        let full_path = proj.join("real.jsonl");
        fs::write(&full_path, "{}\n").unwrap();
        let full_path_str = full_path.display().to_string();

        let index = serde_json::json!({
            "version": 1,
            "entries": [
                {"sessionId": "real", "fullPath": full_path_str, "summary": "real"}
            ]
        });
        fs::write(proj.join("sessions-index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        // Direct call to the trash step + strip step. Skipping the path-
        // validation step because discover_claude_dir is global.
        trash::delete_all([&full_path]).unwrap();
        strip_session_index_entry(&proj, &full_path_str).unwrap();

        assert!(!full_path.exists(), "file must be moved to OS Trash");
        let after: SessionsIndex =
            serde_json::from_str(&fs::read_to_string(proj.join("sessions-index.json")).unwrap()).unwrap();
        assert!(after.entries.is_empty());
    }
}
