//! Cross-platform system commands: discover paths, reveal in file manager.

use std::path::PathBuf;

use tauri_plugin_opener::OpenerExt;

use crate::models::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::claude_md::{claude_md_path, read_claude_md, write_claude_md_atomic};
use crate::storage::{
    discover_claude_dir, scan_marketplaces, scan_mcp_servers, scan_skills, MarketplaceSummary,
    McpServerSummary, SkillSummary,
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
