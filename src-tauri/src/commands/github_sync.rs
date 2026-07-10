//! Tauri commands for GitHub session sync.
//!
//! Phase 1 scope (this file):
//!   - OAuth device flow (start + poll) and storage of access token
//!   - Read/clear connection state (`github_sync.json`)
//!   - Read/clear project-path mappings
//!   - Privacy-consent flag (set once before first upload)
//!
//! Phase 2 will add upload (repo creation + Git Data API plumbing).
//! Phase 3 will add list-remote + download. Those commands are
//! stubbed-but-absent here on purpose — adding them now would mean
//! running a Rust-only half-feature that the UI can't yet call.

use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::github::client::GitHubError;
use crate::github::device_flow::{
    poll_device_flow as poll_df, start_device_flow as start_df, DeviceFlowOutcome,
};
use crate::github::repo as gh_repo;
use crate::models::{
    AppError, AppResult, GITHUB_KEYRING_ACCOUNT, GitHubAuthSecret, GitHubDeviceFlowStart,
    GitHubSyncConfig, ProjectPathMapping, ProjectPathMappings, ProviderSecret,
};
use crate::state::AppState;
use crate::storage::github_sync as storage;

fn map_gh(e: GitHubError) -> AppError {
    match e {
        GitHubError::Http { status, .. } if status == 401 => AppError::GitHubAuthRequired,
        GitHubError::Http { status, body } => {
            AppError::GitHub { status, message: body }
        }
        GitHubError::RateLimited { retry_after_secs } => AppError::GitHub {
            status: 429,
            message: format!("rate limited; retry in {retry_after_secs}s"),
        },
        GitHubError::Network(m) => AppError::Internal(format!("network: {m}")),
        GitHubError::Parse(m) => AppError::Internal(format!("parse: {m}")),
    }
}

fn sync_config_path(state: &AppState) -> PathBuf {
    storage::github_sync_path(state.app_data_dir.as_ref())
}

fn mappings_path(state: &AppState) -> PathBuf {
    storage::path_mappings_path(state.app_data_dir.as_ref())
}

// ===================================================================
// OAuth flow
// ===================================================================

#[tauri::command]
pub fn github_start_device_flow_cmd() -> AppResult<GitHubDeviceFlowStart> {
    start_df().map_err(map_gh)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum GitHubPollOutcome {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Authorized {
        username: String,
        avatar_url: Option<String>,
    },
}

#[tauri::command]
pub fn github_poll_device_flow_cmd(
    state: tauri::State<'_, AppState>,
    device_code: String,
) -> AppResult<GitHubPollOutcome> {
    if !state.keyring.is_available() {
        return Err(AppError::KeyringUnavailable(
            "OS keyring is unavailable; cannot store GitHub access token".into(),
        ));
    }

    let outcome = poll_df(&device_code).map_err(map_gh)?;
    match outcome {
        DeviceFlowOutcome::Pending => Ok(GitHubPollOutcome::Pending),
        DeviceFlowOutcome::SlowDown => Ok(GitHubPollOutcome::SlowDown),
        DeviceFlowOutcome::Denied => Ok(GitHubPollOutcome::Denied),
        DeviceFlowOutcome::Expired => Ok(GitHubPollOutcome::Expired),
        DeviceFlowOutcome::Authorized {
            access_token,
            username,
            avatar_url,
        } => {
            // Persist token + update metadata. We use `ProviderSecret::Custom`
            // because the keyring helper expects that enum; the GitHub
            // token is just an opaque bearer string.
            let secret = ProviderSecret::Custom {
                auth_token: access_token,
            };
            state
                .keyring
                .set_secret(GITHUB_KEYRING_ACCOUNT, &secret)
                .map_err(AppError::from)?;

            let cfg_path = sync_config_path(&state);
            let mut cfg = storage::load_github_sync_config(&cfg_path)?;
            cfg.is_connected = true;
            cfg.username = Some(username.clone());
            cfg.avatar_url = avatar_url.clone();
            storage::save_github_sync_config(&cfg_path, &cfg)?;
            Ok(GitHubPollOutcome::Authorized {
                username,
                avatar_url,
            })
        }
    }
}

// ===================================================================
// Connection state
// ===================================================================

#[tauri::command]
pub fn get_github_sync_config_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<GitHubSyncConfig> {
    storage::load_github_sync_config(&sync_config_path(&state))
}

#[tauri::command]
pub fn github_disconnect_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    let _ = state.keyring.delete_token(GITHUB_KEYRING_ACCOUNT);

    let cfg_path = sync_config_path(&state);
    let mut cfg = storage::load_github_sync_config(&cfg_path)?;
    cfg.is_connected = false;
    cfg.username = None;
    cfg.avatar_url = None;
    cfg.last_sync = None;
    // Privacy-consent flag stays — user already accepted once and
    // we'd otherwise nag them every time they reconnect.
    storage::save_github_sync_config(&cfg_path, &cfg)?;
    Ok(())
}

#[tauri::command]
pub fn github_set_privacy_consent_cmd(
    state: tauri::State<'_, AppState>,
    given: bool,
) -> AppResult<()> {
    let cfg_path = sync_config_path(&state);
    let mut cfg = storage::load_github_sync_config(&cfg_path)?;
    cfg.privacy_consent_given = given;
    storage::save_github_sync_config(&cfg_path, &cfg)?;
    Ok(())
}

#[tauri::command]
pub fn github_set_repo_name_cmd(
    state: tauri::State<'_, AppState>,
    repo_name: String,
) -> AppResult<()> {
    let trimmed = repo_name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("repo name cannot be empty".into()));
    }
    if trimmed.contains('/') || trimmed.contains(' ') {
        return Err(AppError::Validation(
            "repo name must be a single segment (no slashes or spaces)".into(),
        ));
    }
    let cfg_path = sync_config_path(&state);
    let mut cfg = storage::load_github_sync_config(&cfg_path)?;
    cfg.repo_name = trimmed.to_string();
    storage::save_github_sync_config(&cfg_path, &cfg)?;
    Ok(())
}

/// Open the GitHub device-flow verification URL in the user's browser.
/// Frontend calls this immediately after `github_start_device_flow_cmd`
/// so the user doesn't have to copy-paste the URL.
#[tauri::command]
pub fn github_open_verification_url_cmd(
    app: AppHandle,
    verification_uri: String,
) -> AppResult<()> {
    app.opener()
        .open_url(&verification_uri, None::<&str>)
        .map_err(|e| AppError::Internal(format!("opener: {e}")))?;
    Ok(())
}

// ===================================================================
// Path mappings (used in Phase 3, exposed now so settings UI can edit)
// ===================================================================

#[tauri::command]
pub fn github_get_path_mappings_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<ProjectPathMapping>> {
    let m = storage::load_path_mappings(&mappings_path(&state))?;
    Ok(storage::mappings_to_list(&m))
}

#[tauri::command]
pub fn github_set_path_mapping_cmd(
    state: tauri::State<'_, AppState>,
    original_path: String,
    local_path: String,
) -> AppResult<()> {
    let original = original_path.trim();
    let local = local_path.trim();
    if original.is_empty() || local.is_empty() {
        return Err(AppError::Validation(
            "both original_path and local_path are required".into(),
        ));
    }
    let path = mappings_path(&state);
    let mut m = storage::load_path_mappings(&path)?;
    m.version = 1;
    m.mappings
        .insert(original.to_string(), local.to_string());
    storage::save_path_mappings(&path, &m)?;
    Ok(())
}

#[tauri::command]
pub fn github_remove_path_mapping_cmd(
    state: tauri::State<'_, AppState>,
    original_path: String,
) -> AppResult<()> {
    let path = mappings_path(&state);
    let mut m = storage::load_path_mappings(&path)?;
    m.mappings.remove(&original_path);
    storage::save_path_mappings(&path, &m)?;
    Ok(())
}

// ===================================================================
// Probe (used in Phase 2 setup; included now to validate the wiring)
// ===================================================================

/// Verifies that the configured repo exists, returns its default_branch.
/// Used during the upload flow's "first run" path. Frontend doesn't need
/// to call this directly yet — Phase 2 will.
#[tauri::command]
pub fn github_check_repo_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Option<RepoProbeResult>> {
    let secret = load_github_token(&state)?;
    let cfg = storage::load_github_sync_config(&sync_config_path(&state))?;
    let owner = gh_repo::get_authenticated_user(&secret.access_token).map_err(map_gh)?;
    let repo = gh_repo::get_repo(&secret.access_token, &owner, &cfg.repo_name).map_err(map_gh)?;
    Ok(repo.map(|r| RepoProbeResult {
        full_name: r.full_name,
        default_branch: r.default_branch,
    }))
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoProbeResult {
    pub full_name: String,
    pub default_branch: String,
}

fn load_github_token(state: &AppState) -> AppResult<GitHubAuthSecret> {
    if !state.keyring.is_available() {
        return Err(AppError::KeyringUnavailable(
            "OS keyring is unavailable".into(),
        ));
    }
    // We store as `ProviderSecret::Custom { auth_token }` so we can
    // reuse the existing keyring JSON-blob machinery. (Storing under
    // GITHUB_KEYRING_ACCOUNT gives us namespace isolation from
    // providers.)
    use crate::models::ProviderSecret;
    let secret = state
        .keyring
        .get_secret(GITHUB_KEYRING_ACCOUNT)
        .map_err(AppError::from)?;
    match secret {
        ProviderSecret::Custom { auth_token } => Ok(GitHubAuthSecret {
            access_token: auth_token,
            username: None,
            created_at: String::new(),
        }),
        _ => Err(AppError::Internal(
            "unexpected secret variant for github_sync".into(),
        )),
    }
}

// `ProjectPathMappings` isn't constructed here, but it's re-exported
// from the storage layer and a stray `_` import would silently rot.
// Keep this trait-bound reference so the type is referenced.
#[allow(dead_code)]
fn _types_used(_: ProjectPathMappings, _: GitHubSyncConfig) {}