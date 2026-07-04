//! Cross-platform system commands: discover paths, reveal in file manager.

use std::path::PathBuf;

use tauri_plugin_opener::OpenerExt;

use crate::models::AppResult;
use crate::state::AppState;
use crate::storage::discover_claude_dir;

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