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

use crate::github::cache as gh_cache;
use crate::github::client::GitHubError;
use crate::github::device_flow::{
    poll_device_flow as poll_df, start_device_flow as start_df, DeviceFlowOutcome,
};
use crate::github::repo as gh_repo;
use crate::github::upload as gh_upload;
use crate::models::{
    AppError, AppResult, GITHUB_KEYRING_ACCOUNT, GitHubAuthSecret, GitHubDeviceFlowStart,
    GitHubSyncConfig, ProjectPathMapping, ProjectPathMappings, ProjectRemoteMetadata,
    ProviderSecret, RemoteSessionEntry, RemoteSessionSummary, SessionSyncMetadata,
    SessionSyncStateFile, SyncState,
};
use crate::state::AppState;
use crate::storage::sessions::PROJECTS_DIR;
use crate::storage::{discover_claude_dir, github_sync as storage, SessionMessage};

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

/// Resolve `(owner, default_branch)` for the connected sync repo, hitting
/// GitHub only on a cold cache. Both fields are tied to the access token
/// and change at most once per login, so caching them across commands is
/// safe — we invalidate the whole cache on `github_disconnect_cmd` and on
/// every fresh OAuth login.
fn resolve_owner_and_branch(
    state: &AppState,
    token: &str,
    repo_name: &str,
) -> AppResult<(String, String)> {
    {
        let c = state.github_cache.lock().expect("github_cache poisoned");
        if let (Some(o), Some(b)) = (&c.owner, &c.default_branch) {
            return Ok((o.clone(), b.clone()));
        }
    }
    // NOTE: we drop the lock before the HTTP calls so a stuck request
    // can't block every other command. Two concurrent misses will
    // therefore each fire the pair of HTTP calls — this is not
    // single-flight. Acceptable for this feature's usage pattern
    // (human-driven tab clicks don't race), but do not assume dedupe if
    // a future caller starts firing this concurrently from a
    // background task.
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;
    let repo = gh_repo::get_repo(token, &owner, repo_name)
        .map_err(map_gh)?
        .ok_or_else(|| AppError::Validation("sync repo does not exist".into()))?;
    {
        let mut c = state.github_cache.lock().expect("github_cache poisoned");
        c.owner = Some(owner.clone());
        c.default_branch = Some(repo.default_branch.clone());
    }
    Ok((owner, repo.default_branch))
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

            // Seed the in-memory cache. `username` is GitHub's `login`
            // and is byte-identical to what `get_authenticated_user`
            // would return — this skips the GET /user call on the first
            // tab open after connecting. `default_branch` cannot be
            // seeded here because the sync repo may not exist yet at
            // OAuth time; `resolve_owner_and_branch` will fill it on
            // first list call.
            {
                let mut c = state.github_cache.lock().expect("github_cache poisoned");
                c.clear();
                c.owner = Some(username.clone());
            }

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

    // Wipe every cache tier in lockstep with the token: in-memory
    // owner/branch/SHA/list/tree AND the on-disk blob cache. Token
    // gone, sensitive data gone — same invariant.
    state.github_cache.lock().expect("github_cache poisoned").clear();
    if let Err(e) = gh_cache::clear_blob_cache(state.app_data_dir.as_ref()) {
        log::warn!("blob cache clear on disconnect failed: {e}");
    }

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
    slug: Option<String>,
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
    // Canonical key: original decoded path.
    m.mappings
        .insert(original.to_string(), local.to_string());
    // Slug-keyed lookup so download resolvers hit without re-prompting.
    if let Some(s) = slug {
        let s = s.trim();
        if !s.is_empty() {
            m.slug_mappings.insert(s.to_string(), local.to_string());
        }
    }
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

    // Invalidate the remote-list cache: we just moved the default
    // branch's HEAD forward, so the cached list/tree/commit_sha are
    // stale. Dropping them forces the next `github_list_remote_sessions_cmd`
    // to refetch. Owner + default_branch are still correct.
    {
        let mut cache = state.github_cache.lock().expect("github_cache poisoned");
        cache.commit_sha = None;
        cache.tree = None;
        cache.sessions_list = None;
    }

    Ok(sync_meta)
}

/// Return the full per-project sync-state map. `project_folder` is the
/// on-disk directory that holds the transcripts (parent of the `.jsonl`).
/// The sessions list calls this once per project to color every row.
///
/// The `sync_state` field on each entry is re-classified against the
/// transcript's current on-disk mtime before returning, so a session
/// that was edited after its last upload flips to `OutOfSync` without
/// waiting for the next upload. The persisted value is only ever
/// authoritative right after an upload; anything downstream must trust
/// this recomputed view.
#[tauri::command]
pub fn github_get_session_sync_state_cmd(
    _state: tauri::State<'_, AppState>,
    project_folder: String,
) -> AppResult<SessionSyncStateFile> {
    let folder = PathBuf::from(&project_folder);
    let path = storage::session_sync_state_path(&folder);
    let file = storage::load_session_sync_state(&path)?;
    Ok(reclassify_state_file(&folder, file))
}

/// Recompute every entry's `sync_state` against the transcript's
/// current on-disk mtime. Extracted so the reclassification is
/// unit-testable without a Tauri state harness.
fn reclassify_state_file(
    project_folder: &Path,
    mut file: SessionSyncStateFile,
) -> SessionSyncStateFile {
    for (session_id, meta) in file.sessions.iter_mut() {
        let jsonl = project_folder.join(format!("{session_id}.jsonl"));
        let current_mtime = std::fs::metadata(&jsonl)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let fresh = storage::classify_sync_state(Some(&*meta), current_mtime);
        meta.sync_state = fresh;
    }
    file
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

// ===================================================================
// Phase 3: list + download target
// ===================================================================

/// List every session in the GitHub sync repo, grouped by project.
/// Returns `[]` when the repo doesn't exist yet (user hasn't uploaded).
///
/// Async + `spawn_blocking` so the blocking HTTP calls to GitHub don't
/// freeze the WebView's main thread.
///
/// SHA-gated cache: on a warm cache with no upstream changes, this is
/// a single `GET /git/ref/heads/{branch}` call. On a SHA shift we fall
/// through to a tree walk + metadata fetches only for the slugs whose
/// tree entries actually changed.
#[tauri::command]
pub async fn github_list_remote_sessions_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<RemoteSessionSummary>> {
    let state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cfg = storage::load_github_sync_config(&sync_config_path(&state))?;
        if !cfg.is_connected {
            return Err(AppError::GitHubNotConfigured("not_connected".into()));
        }
        let secret = load_github_token(&state)?;
        let token = &secret.access_token;

        // Repo probe — distinguishes "never uploaded" (no repo) from
        // an actual GitHub failure. Returns Ok(None) for the empty-
        // repo fast path.
        let owner = {
            let c = state.github_cache.lock().expect("github_cache poisoned");
            c.owner.clone()
        };
        let owner = match owner {
            Some(o) => o,
            None => gh_repo::get_authenticated_user(token).map_err(map_gh)?,
        };
        let repo = gh_repo::get_repo(token, &owner, &cfg.repo_name).map_err(map_gh)?;
        let Some(repo) = repo else {
            return Ok(Vec::new());
        };
        // Cold-cache fill: owner is now known; default_branch can be
        // cached alongside it for next time.
        {
            let mut c = state.github_cache.lock().expect("github_cache poisoned");
            c.owner = Some(owner.clone());
            c.default_branch = Some(repo.default_branch.clone());
        }

        // SHA gate: 1 call. If the ref SHA matches our cache, return
        // the cached list verbatim and we're done.
        let current_ref_sha =
            gh_repo::get_branch_ref_sha(token, &owner, &cfg.repo_name, &repo.default_branch)
                .map_err(map_gh)?;
        let (cached_sha, cached_list) = {
            let c = state.github_cache.lock().expect("github_cache poisoned");
            (c.commit_sha.clone(), c.sessions_list.clone())
        };
        if let (Some(cached_sha), Some(cached_list)) = (cached_sha, cached_list) {
            if Some(&cached_sha) == current_ref_sha.as_ref() {
                return Ok(cached_list);
            }
        }

        // Cold or shifted SHA — full tree walk + per-slug diff.
        let tree = gh_repo::get_tree_recursive(token, &owner, &cfg.repo_name, &repo.default_branch)
            .map_err(map_gh)?;
        let (previous_list, previous_tree) = {
            let c = state.github_cache.lock().expect("github_cache poisoned");
            (c.sessions_list.clone(), c.tree.clone())
        };
        let diff = gh_repo::diff_slugs(previous_tree.as_ref(), &tree);
        let rows = gh_repo::list_remote_sessions_with_diff(
            token,
            &owner,
            &cfg.repo_name,
            &tree,
            previous_list.as_deref(),
            &diff,
        )
        .map_err(map_gh)?;

        // Write-back the cache. Note: we persist the *current* tree,
        // not a transformed one, so the next diff has the same source
        // format (paths + SHAs).
        {
            let mut c = state.github_cache.lock().expect("github_cache poisoned");
            c.tree = Some(tree);
            c.commit_sha = current_ref_sha;
            c.sessions_list = Some(rows.clone());
        }
        Ok(rows)
    })
    .await
    .map_err(|e| AppError::Internal(format!("list_remote_sessions task panicked: {e}")))?
}

/// Resolve the local target folder for a remote project slug, if a
/// mapping already exists. Returns None to trigger the ProjectPicker.
#[tauri::command]
pub fn github_resolve_download_target_cmd(
    state: tauri::State<'_, AppState>,
    project_slug: String,
) -> AppResult<Option<String>> {
    let m = storage::load_path_mappings(&mappings_path(&state))?;
    Ok(m.slug_mappings.get(&project_slug).cloned())
}

/// Download a session transcript from the GitHub sync repo to the
/// resolved local project folder. Resolves the target via path
/// mappings; if none, returns `Validation("path_mapping_required")`.
/// If a local file already exists and the timestamps disagree,
/// returns `SessionDownloadConflict` unless `force` is true.
#[tauri::command]
pub fn github_download_session_cmd(
    state: tauri::State<'_, AppState>,
    session_id: String,
    project_slug: String,
    blob_sha: String,
    force: Option<bool>,
) -> AppResult<crate::models::DownloadResult> {
    use crate::models::{DownloadResult, SessionConflictKind};

    let cfg_path = sync_config_path(&state);
    let mut cfg = storage::load_github_sync_config(&cfg_path)?;
    if !cfg.is_connected {
        return Err(AppError::GitHubNotConfigured("not_connected".into()));
    }
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;

    // Resolve target folder from slug mappings. Missing mapping is not a
    // hard error — it's the picker signal.
    let m = storage::load_path_mappings(&mappings_path(&state))?;
    let target = m.slug_mappings.get(&project_slug).cloned().ok_or_else(|| {
        AppError::Validation("path_mapping_required".into())
    })?;
    let target_path = std::path::PathBuf::from(&target);
    if !target_path.exists() {
        return Err(AppError::Validation(format!(
            "mapped local folder does not exist: {target}"
        )));
    }

    // Repo probe — needed for both the blob fetch and metadata lookup.
    let repo = gh_repo::get_repo(token, &owner, &cfg.repo_name)
        .map_err(map_gh)?
        .ok_or_else(|| AppError::Validation("sync repo does not exist".into()))?;
    let default_branch = repo.default_branch;

    // Read remote's modified timestamp from the project's metadata.json
    // (best-effort; missing metadata is fine — we just can't conflict-check).
    let tree = gh_repo::get_tree_recursive(token, &owner, &cfg.repo_name, &default_branch)
        .map_err(map_gh)?;
    let meta_sha = tree.tree.iter().find_map(|e| {
        let parts: Vec<&str> = e.path.split('/').collect();
        if parts.len() == 3
            && parts[0] == "sessions"
            && parts[1] == project_slug
            && parts[2] == "metadata.json"
        {
            Some(e.sha.clone())
        } else {
            None
        }
    });
    let remote_modified: Option<String> = match meta_sha {
        Some(sha) => gh_repo::fetch_project_metadata(token, &owner, &cfg.repo_name, &sha)
            .map_err(map_gh)?
            .sessions
            .get(&session_id)
            .and_then(|e| e.modified.clone()),
        None => None,
    };

    // Local mtime for conflict detection.
    let target_jsonl = target_path.join(format!("{session_id}.jsonl"));
    let local_mtime = std::fs::metadata(&target_jsonl).ok().and_then(|m| {
        m.modified().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
    });

    if !force.unwrap_or(false) {
        if let (Some(remote), Some(local)) = (remote_modified.as_ref(), local_mtime.as_ref()) {
            // RFC3339 strings from UTC compare lexicographically.
            if remote.as_str() > local.as_str() {
                return Err(AppError::SessionDownloadConflict {
                    kind: SessionConflictKind::RemoteNewer,
                    session_id: session_id.clone(),
                });
            }
            if local.as_str() > remote.as_str() {
                return Err(AppError::SessionDownloadConflict {
                    kind: SessionConflictKind::LocalNewer,
                    session_id: session_id.clone(),
                });
            }
        }
    }

    // Fetch and write the transcript atomically.
    let bytes = fetch_blob_cached(&state, token, &owner, &cfg.repo_name, &blob_sha)?;
    use std::io::Write;
    let tmp_path = target_jsonl.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, &target_jsonl)?;

    // Register with Claude Code's sessions-index.json.
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let entry = crate::storage::sessions::SessionIndexEntry {
        session_id: session_id.clone(),
        full_path: target_jsonl.display().to_string(),
        first_prompt: None,
        summary: None,
        message_count: Some(0),
        created: Some(now.clone()),
        modified: Some(now.clone()),
        project_path: None,
        is_sidechain: Some(false),
    };
    crate::storage::sessions::upsert_into_sessions_index(&target_path, &entry)?;

    // Update session_sync_state.json so the row appears green immediately.
    let state_path = storage::session_sync_state_path(&target_path);
    let mut state_file = storage::load_session_sync_state(&state_path)?;
    state_file.version = 1;
    state_file.sessions.insert(
        session_id.clone(),
        SessionSyncMetadata {
            last_uploaded: Some(now.clone()),
            remote_sha: Some(blob_sha),
            last_local_modified: Some(now.clone()),
            sync_state: SyncState::Synced,
        },
    );
    storage::write_session_sync_state_atomic(&state_path, &state_file)?;

    // Record last sync on the global config.
    cfg.last_sync = Some(now);
    storage::save_github_sync_config(&cfg_path, &cfg)?;

    Ok(DownloadResult {
        session_id,
        full_path: target_jsonl.display().to_string(),
        sync_state: SyncState::Synced,
    })
}

/// Fetch a remote session transcript from the GitHub sync repo, decode it,
/// and return the parsed `SessionMessage` list so the frontend can render
/// a preview without downloading the file to disk. The bytes are written
/// to a `NamedTempFile` solely because `parse_session_transcript` takes a
/// `&Path`; the temp file is auto-deleted when this scope returns.
#[tauri::command]
pub fn github_fetch_remote_transcript_cmd(
    state: tauri::State<'_, AppState>,
    session_id: String,
    blob_sha: String,
) -> AppResult<Vec<SessionMessage>> {
    let cfg = storage::load_github_sync_config(&sync_config_path(&state))?;
    if !cfg.is_connected {
        return Err(AppError::GitHubNotConfigured("not_connected".into()));
    }
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;

    // Probe the repo — distinguish "never uploaded" (no repo) from
    // "blob_sha stale/missing" (blob fetch error) by failing fast here.
    let _repo = gh_repo::get_repo(token, &owner, &cfg.repo_name)
        .map_err(map_gh)?
        .ok_or_else(|| AppError::Validation("sync repo does not exist".into()))?;

    let bytes = fetch_blob_cached(&state, token, &owner, &cfg.repo_name, &blob_sha)?;
    let tmp = tempfile::NamedTempFile::new()?;
    std::fs::write(tmp.path(), &bytes)?;

    // Reuse the exact same parser the local-file path uses. Don't
    // duplicate the JSONL parsing logic — it lives in the storage layer.
    let messages = crate::storage::sessions::parse_session_transcript(tmp.path())?;

    // `session_id` and `blob_sha` are accepted by the signature for future
    // use (e.g. caching, telemetry) but the current parser only needs the
    // file bytes. Drop the temp on scope exit.
    let _ = (session_id, blob_sha);
    Ok(messages)
}

/// Fetch a session transcript blob, hitting the on-disk cache first.
/// Cache writes are best-effort and never fail the parent command —
/// GitHub remains the source of truth.
fn fetch_blob_cached(
    state: &AppState,
    token: &str,
    owner: &str,
    repo: &str,
    blob_sha: &str,
) -> AppResult<Vec<u8>> {
    if let Some(bytes) = gh_cache::get_cached_blob(state.app_data_dir.as_ref(), blob_sha) {
        return Ok(bytes);
    }
    let bytes = gh_repo::get_blob(token, owner, repo, blob_sha).map_err(map_gh)?;
    if let Err(e) = gh_cache::put_cached_blob(state.app_data_dir.as_ref(), blob_sha, &bytes) {
        log::warn!("blob cache put failed for {blob_sha}: {e}");
    } else if let Err(e) =
        gh_cache::enforce_blob_cache_size(state.app_data_dir.as_ref(), gh_cache::cache_cap_bytes())
    {
        log::warn!("blob cache eviction failed: {e}");
    }
    Ok(bytes)
}

/// Return blob cache size for a future settings-page UI.
#[tauri::command]
pub fn github_get_blob_cache_stats_cmd(
    state: tauri::State<'_, AppState>,
) -> gh_cache::BlobCacheStats {
    gh_cache::blob_cache_stats(state.app_data_dir.as_ref())
}

/// Drop the in-memory cache. Frontend calls this for the "force
/// refresh" escape hatch (Shift+Click on the Refresh button) so the
/// next list call pays the full `2 + N` cost.
#[tauri::command]
pub fn github_invalidate_remote_cache_cmd(state: tauri::State<'_, AppState>) {
    state.github_cache.lock().expect("github_cache poisoned").clear();
}

fn load_github_token(state: &AppState) -> AppResult<GitHubAuthSecret> {
    if !state.keyring.is_available() {
        return Err(AppError::KeyringUnavailable(
            "OS keyring is unavailable".into(),
        ));
    }
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

/// Return the absolute paths of every local Claude Code project folder
/// (`<claude_dir>/projects/*/`). Feeds the ProjectPickerModal dropdown.
#[tauri::command]
pub fn github_list_local_projects_cmd(
    _state: tauri::State<'_, AppState>,
) -> AppResult<Vec<String>> {
    let projects_dir = discover_claude_dir().join(PROJECTS_DIR);
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&projects_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.push(path.display().to_string());
        }
    }
    out.sort();
    Ok(out)
}

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

    // ---- reclassify_state_file ----

    fn make_state_file(session_id: &str, last_local_modified: &str) -> SessionSyncStateFile {
        let mut file = SessionSyncStateFile::default();
        file.version = 1;
        file.sessions.insert(
            session_id.to_string(),
            SessionSyncMetadata {
                last_uploaded: Some("2026-07-09T00:00:00Z".into()),
                remote_sha: Some("deadbeef".into()),
                last_local_modified: Some(last_local_modified.into()),
                // Persisted value is deliberately stale — the reclassify
                // step should overwrite it based on real mtime.
                sync_state: SyncState::Synced,
            },
        );
        file
    }

    fn write_transcript(dir: &Path, session_id: &str) -> PathBuf {
        let p = dir.join(format!("{session_id}.jsonl"));
        std::fs::write(&p, b"{}\n").unwrap();
        p
    }

    fn file_mtime_rfc3339(p: &Path) -> String {
        let meta = std::fs::metadata(p).unwrap();
        let secs = meta
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        DateTime::<Utc>::from_timestamp(secs, 0)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    #[test]
    fn reclassify_flips_synced_to_out_of_sync_when_mtime_drifts() {
        // File on disk has whatever mtime the OS gave it; we lie in the
        // metadata by claiming the last upload captured an mtime an hour
        // earlier. Reclassify must notice and flip the row to amber even
        // though the persisted `sync_state` still says Synced.
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "abc-uuid";
        write_transcript(tmp.path(), session_id);
        let file = make_state_file(session_id, "2020-01-01T00:00:00Z");

        let reclassified = reclassify_state_file(tmp.path(), file);
        let entry = reclassified.sessions.get(session_id).unwrap();
        assert_eq!(entry.sync_state, SyncState::OutOfSync);
    }

    #[test]
    fn reclassify_keeps_synced_when_mtime_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "abc-uuid";
        let path = write_transcript(tmp.path(), session_id);
        // Pull the real mtime off disk so classify_sync_state's <=1s
        // tolerance is satisfied without touching the filesystem clock.
        let mtime = file_mtime_rfc3339(&path);
        let file = make_state_file(session_id, &mtime);

        let reclassified = reclassify_state_file(tmp.path(), file);
        let entry = reclassified.sessions.get(session_id).unwrap();
        assert_eq!(entry.sync_state, SyncState::Synced);
    }

    #[test]
    fn reclassify_reports_out_of_sync_when_transcript_is_missing() {
        // A transcript that vanished (user moved the file / renamed the
        // project) should not silently keep the green icon.
        let tmp = tempfile::tempdir().unwrap();
        let session_id = "missing-uuid";
        let file = make_state_file(session_id, "2026-07-09T00:00:00Z");

        let reclassified = reclassify_state_file(tmp.path(), file);
        let entry = reclassified.sessions.get(session_id).unwrap();
        // Missing file -> current_mtime falls to 0 -> classify returns
        // OutOfSync (mtimes disagree).
        assert_eq!(entry.sync_state, SyncState::OutOfSync);
    }

    #[test]
    fn rfc3339_compare_orders_remote_vs_local() {
        // The command relies on lexicographic-UTC comparison — lock the
        // invariant so no future refactor introduces string-parse instead.
        let remote = "2026-07-11T10:00:00Z";
        let local = "2026-07-11T09:00:00Z";
        assert!(remote > local);
        assert!(local < remote);
    }
}