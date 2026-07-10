//! Tauri commands for GitHub session sync.
//!
//! Phase 1 scope (this file):
//!   - OAuth device flow (start + poll) and storage of access token
//!   - Read/clear connection state (`github_sync.json`)
//!   - Read/clear project-path mappings
//!   - Privacy-consent flag (set once before first upload)
//!
//! Phase 2 adds upload (repo creation + Git Data API plumbing) plus the
//! two sync-state read commands the sessions list uses to color each
//! row's GitHub icon. Phase 3 will add list-remote + download.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::github::client::GitHubError;
use crate::github::device_flow::{
    poll_device_flow as poll_df, start_device_flow as start_df, DeviceFlowOutcome,
};
use crate::github::repo as gh_repo;
use crate::github::upload as gh_upload;
use crate::models::{
    AppError, AppResult, GITHUB_KEYRING_ACCOUNT, GitHubAuthSecret, GitHubDeviceFlowStart,
    GitHubSyncConfig, ProjectPathMapping, ProjectPathMappings, ProjectRemoteMetadata,
    ProviderSecret, RemoteSessionEntry, SessionSyncMetadata, SessionSyncStateFile, SyncState,
};
use crate::state::AppState;
use crate::storage::github_sync as storage;

/// Largest session we'll upload. GitHub's hard blob limit is 100 MB;
/// base64 inflates payloads ~33%, so we cap the raw file well under it.
const MAX_UPLOAD_BYTES: u64 = 95 * 1024 * 1024;

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
// Upload (Phase 2)
// ===================================================================

/// The project slug is the on-disk folder name that Claude Code created
/// (`<claude_dir>/projects/<slug>/<uuid>.jsonl`). We read it straight from
/// the transcript's parent directory rather than re-encoding `project_path`
/// — the encoding is lossy/ambiguous (e.g. `.claude` -> `--claude`), so the
/// authoritative slug is the one already on disk.
fn slug_from_full_path(full_path: &Path) -> AppResult<String> {
    full_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AppError::Validation(format!(
                "cannot derive project slug from path: {}",
                full_path.display()
            ))
        })
}

/// File mtime as an RFC3339 string, matching the format stored by the
/// sessions scanner so `classify_sync_state`'s comparison lines up.
fn mtime_rfc3339(meta: &std::fs::Metadata) -> Option<String> {
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    DateTime::<Utc>::from_timestamp(dur.as_secs() as i64, 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Merge this session into the per-project `metadata.json` living in the
/// repo, returning the serialized bytes to commit. Fetches the existing
/// file (if any) so we preserve other sessions' entries and don't clobber
/// `original_path`.
fn build_project_metadata(
    token: &str,
    owner: &str,
    repo_name: &str,
    default_branch: &str,
    slug: &str,
    session_id: &str,
    project_path: &str,
    modified: Option<String>,
) -> AppResult<Vec<u8>> {
    let meta_path = storage::project_metadata_path(slug);
    let existing =
        gh_upload::fetch_existing_file(token, owner, repo_name, default_branch, &meta_path)
            .map_err(map_gh)?;

    let mut meta: ProjectRemoteMetadata = match existing {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        None => ProjectRemoteMetadata::default(),
    };
    meta.version = 1;
    if meta.original_path.is_empty() {
        meta.original_path = project_path.to_string();
    }
    meta.sessions.insert(
        session_id.to_string(),
        RemoteSessionEntry {
            title: None,
            modified,
            message_count: 0,
        },
    );
    Ok(serde_json::to_vec_pretty(&meta)?)
}

/// Upload a single session transcript (plus refreshed per-project metadata)
/// to the private sync repo in one atomic commit. Returns the resulting
/// sync metadata so the frontend can color the row without a refetch.
#[tauri::command]
pub fn github_upload_session_cmd(
    state: tauri::State<'_, AppState>,
    session_id: String,
    full_path: String,
    project_path: String,
) -> AppResult<SessionSyncMetadata> {
    let full_path = PathBuf::from(&full_path);
    let cfg_path = sync_config_path(&state);
    let mut cfg = storage::load_github_sync_config(&cfg_path)?;

    // Privacy gate: the frontend shows the consent dialog, calls
    // `github_set_privacy_consent_cmd`, then retries. Until then we refuse
    // to push a transcript that may hold secrets.
    if !cfg.privacy_consent_given {
        return Err(AppError::GitHubNotConfigured("privacy_consent_required".into()));
    }

    // Large-file guard before we read anything into memory.
    let meta = std::fs::metadata(&full_path)?;
    if meta.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::Validation(format!(
            "session is {} MB; GitHub sync caps at {} MB",
            meta.len() / (1024 * 1024),
            MAX_UPLOAD_BYTES / (1024 * 1024)
        )));
    }
    let modified = mtime_rfc3339(&meta);
    let content = std::fs::read(&full_path)?;

    let slug = slug_from_full_path(&full_path)?;
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;

    // Ensure the repo exists first so metadata's fetch-existing sees the
    // right default branch.
    let default_branch = gh_upload::ensure_repo(token, &owner, &cfg.repo_name).map_err(map_gh)?;

    let session_repo_path = storage::remote_session_path(&slug, &session_id);
    let metadata_bytes = build_project_metadata(
        token,
        &owner,
        &cfg.repo_name,
        &default_branch,
        &slug,
        &session_id,
        &project_path,
        modified.clone(),
    )?;

    let files = vec![
        gh_upload::UploadFile {
            path: session_repo_path.clone(),
            content,
        },
        gh_upload::UploadFile {
            path: storage::project_metadata_path(&slug),
            content: metadata_bytes,
        },
    ];

    let title = crate::storage::sessions::extract_title_from_jsonl(&full_path)
        .unwrap_or_else(|| session_id.clone());
    let message = format!("sync: {} ({})", title, slug);
    let result =
        gh_upload::upload_files(token, &owner, &cfg.repo_name, &message, &files).map_err(map_gh)?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let remote_sha = result.blob_shas.get(&session_repo_path).cloned();

    let sync_meta = SessionSyncMetadata {
        last_uploaded: Some(now.clone()),
        remote_sha,
        last_local_modified: modified,
        sync_state: SyncState::Synced,
    };

    // Persist to the project's session_sync_state.json (locked write —
    // Claude Code may be writing to the same folder).
    let project_folder = full_path.parent().ok_or_else(|| {
        AppError::Validation(format!(
            "transcript path has no parent folder: {}",
            full_path.display()
        ))
    })?;
    let state_path = storage::session_sync_state_path(project_folder);
    let mut state_file = storage::load_session_sync_state(&state_path)?;
    state_file.version = 1;
    state_file
        .sessions
        .insert(session_id.clone(), sync_meta.clone());
    storage::write_session_sync_state_atomic(&state_path, &state_file)?;

    // Record the last successful sync on the global config.
    cfg.last_sync = Some(now);
    storage::save_github_sync_config(&cfg_path, &cfg)?;

    Ok(sync_meta)
}

/// Return the full per-project sync-state map. `project_folder` is the
/// on-disk directory that holds the transcripts (parent of the `.jsonl`).
/// The sessions list calls this once per project to color every row.
#[tauri::command]
pub fn github_get_session_sync_state_cmd(
    _state: tauri::State<'_, AppState>,
    project_folder: String,
) -> AppResult<SessionSyncStateFile> {
    let path = storage::session_sync_state_path(Path::new(&project_folder));
    storage::load_session_sync_state(&path)
}

/// Re-classify a single session by comparing its current on-disk mtime
/// against the last-uploaded snapshot. Used after an edit to flip a row
/// from green (synced) to amber (out-of-sync) without a full rescan.
#[tauri::command]
pub fn github_check_session_sync_status_cmd(
    _state: tauri::State<'_, AppState>,
    session_id: String,
    full_path: String,
) -> AppResult<SyncState> {
    let full_path = PathBuf::from(&full_path);
    let project_folder = full_path.parent().ok_or_else(|| {
        AppError::Validation(format!(
            "transcript path has no parent folder: {}",
            full_path.display()
        ))
    })?;
    let path = storage::session_sync_state_path(project_folder);
    let state_file = storage::load_session_sync_state(&path)?;
    let entry = state_file.sessions.get(&session_id);

    let current_mtime = std::fs::metadata(&full_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    Ok(storage::classify_sync_state(entry, current_mtime))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_the_on_disk_parent_folder_name() {
        // The slug is read verbatim from the transcript's parent dir, so
        // even the lossy/ambiguous encodings (`.claude` -> `--claude`)
        // round-trip perfectly — we never re-derive them.
        let p = PathBuf::from(
            "/home/ayande/.claude/projects/-home-ayande--claude/abc-uuid.jsonl",
        );
        assert_eq!(slug_from_full_path(&p).unwrap(), "-home-ayande--claude");
    }

    #[test]
    fn slug_handles_underscores_and_dashes_in_path() {
        let p = PathBuf::from(
            "/x/projects/-home-ayan_de-Projects-my-project/s.jsonl",
        );
        assert_eq!(
            slug_from_full_path(&p).unwrap(),
            "-home-ayan_de-Projects-my-project"
        );
    }

    #[test]
    fn slug_errors_when_no_parent() {
        let p = PathBuf::from("s.jsonl");
        // No parent directory component -> validation error rather than a
        // silently-wrong empty slug.
        assert!(slug_from_full_path(&p).is_err());
    }
}