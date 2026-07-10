//! Reads/writes the GitHub-sync metadata files.
//!
//! Two files, both atomic via temp-file + rename (no lock required —
//! only the app itself writes them, and we don't run two app instances):
//!   - `<app_data_dir>/github_sync.json` — connection state, repo name,
//!     privacy-consent flag, last-sync timestamp.
//!   - `<app_data_dir>/project_path_mappings.json` — original_path →
//!     local_path remappings so a session downloaded on a second
//!     machine lands in the right project folder.
//!
//! The sync-state file lives at the project level
//! (`<project_folder>/session_sync_state.json`); that one needs the
//! file-lock dance (live Claude Code could be writing to the same
//! folder). See `write_session_sync_state_atomic`.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::models::{
    AppError, AppResult, GitHubSyncConfig, ProjectPathMapping, ProjectPathMappings,
    SessionSyncMetadata, SessionSyncStateFile, SyncState,
};

const GITHUB_SYNC_FILENAME: &str = "github_sync.json";
const PATH_MAPPINGS_FILENAME: &str = "project_path_mappings.json";
const SESSION_SYNC_STATE_FILENAME: &str = "session_sync_state.json";

pub fn github_sync_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(GITHUB_SYNC_FILENAME)
}

pub fn path_mappings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(PATH_MAPPINGS_FILENAME)
}

pub fn session_sync_state_path(project_folder: &Path) -> PathBuf {
    project_folder.join(SESSION_SYNC_STATE_FILENAME)
}

// ---------------- GitHubSyncConfig ----------------

pub fn load_github_sync_config(path: &Path) -> AppResult<GitHubSyncConfig> {
    if !path.exists() {
        return Ok(GitHubSyncConfig::default());
    }
    let bytes = fs::read(path)?;
    let cfg: GitHubSyncConfig = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::Validation(format!(
            "{} is malformed: {e}",
            path.display()
        ))
    })?;
    Ok(cfg)
}

pub fn save_github_sync_config(path: &Path, cfg: &GitHubSyncConfig) -> AppResult<()> {
    write_json_atomic(path, &serde_json::to_value(cfg)?)
}

// ---------------- ProjectPathMappings ----------------

pub fn load_path_mappings(path: &Path) -> AppResult<ProjectPathMappings> {
    if !path.exists() {
        return Ok(ProjectPathMappings::default());
    }
    let bytes = fs::read(path)?;
    let m: ProjectPathMappings = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::Validation(format!(
            "{} is malformed: {e}",
            path.display()
        ))
    })?;
    Ok(m)
}

pub fn save_path_mappings(path: &Path, m: &ProjectPathMappings) -> AppResult<()> {
    write_json_atomic(path, &serde_json::to_value(m)?)
}

pub fn mappings_to_list(m: &ProjectPathMappings) -> Vec<ProjectPathMapping> {
    m.mappings
        .iter()
        .map(|(k, v)| ProjectPathMapping {
            original_path: k.clone(),
            local_path: v.clone(),
        })
        .collect()
}

// ---------------- SessionSyncState ----------------

pub fn load_session_sync_state(path: &Path) -> AppResult<SessionSyncStateFile> {
    if !path.exists() {
        return Ok(SessionSyncStateFile::default());
    }
    let bytes = fs::read(path)?;
    let s: SessionSyncStateFile = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::Validation(format!(
            "{} is malformed: {e}",
            path.display()
        ))
    })?;
    Ok(s)
}

/// Atomic write with sidecar file lock — mirrors `settings::write_settings_atomic`.
/// Used because the same folder may be written to by a live Claude Code
/// process; an exclusive `.lock` sidecar serialises us against that.
pub fn write_session_sync_state_atomic(path: &Path, state: &SessionSyncStateFile) -> AppResult<()> {
    let value = serde_json::to_value(state)?;
    let json_bytes = serde_json::to_vec_pretty(&value)?;

    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!(
            "session sync state path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;

    let lock_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".lock");
        PathBuf::from(p)
    };
    let lock_file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file.lock_exclusive().map_err(|e| AppError::Lock(e.to_string()))?;

    // Write to a temp file in the same directory, fsync, then persist.
    let write_result = (|| -> AppResult<()> {
        let mut tmp = NamedTempFile::new_in(parent)?;
        tmp.write_all(&json_bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| {
            AppError::Io(std::io::Error::other(format!(
                "persist session sync state: {e}"
            )))
        })?;
        Ok(())
    })();

    let _ = lock_file.unlock();
    write_result
}

/// Determine the current sync state for a session file by comparing
/// its on-disk mtime against the stored `last_local_modified`. Returns
/// `NeverUploaded` when there's no entry yet, `Synced` when mtimes
/// match (within 1s of tolerance for filesystem timestamp resolution),
/// `OutOfSync` otherwise.
pub fn classify_sync_state(
    metadata: Option<&SessionSyncMetadata>,
    current_mtime_secs: i64,
) -> SyncState {
    match metadata {
        None => SyncState::NeverUploaded,
        Some(m) => {
            if m.last_local_modified.is_none() || m.last_uploaded.is_none() {
                SyncState::OutOfSync
            } else {
                let stored = m
                    .last_local_modified
                    .as_deref()
                    .and_then(parse_rfc3339_to_unix)
                    .unwrap_or(0);
                if (current_mtime_secs - stored).abs() <= 1 {
                    SyncState::Synced
                } else {
                    SyncState::OutOfSync
                }
            }
        }
    }
}

fn parse_rfc3339_to_unix(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp())
}

// ---------------- internal helpers ----------------

fn write_json_atomic(path: &Path, value: &Value) -> AppResult<()> {
    let json_bytes = serde_json::to_vec_pretty(value)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(&json_bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| {
        AppError::Io(std::io::Error::other(format!(
            "persist {}: {e}",
            path.display()
        )))
    })?;
    Ok(())
}

/// Helper for the "repo path" — stored as `sessions/<slug>/<uuid>.jsonl`
/// so we can keep a consistent layout regardless of how Claude Code
/// evolves its own folder scheme.
pub fn remote_session_path(project_slug: &str, session_id: &str) -> String {
    format!("sessions/{project_slug}/{session_id}.jsonl")
}

/// `manifest.json` lives at the repo root, lists every project we've
/// ever uploaded. Useful for the "browse all remote sessions" UI.
pub fn manifest_path() -> &'static str {
    "manifest.json"
}

/// Per-project metadata file. Tells the download UI what the original
/// project path was (for path-mapping decisions) and keeps a tiny index
/// of session IDs + titles for fast listing.
pub fn project_metadata_path(project_slug: &str) -> String {
    format!("sessions/{project_slug}/metadata.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_handles_missing_entry() {
        assert_eq!(
            classify_sync_state(None, 1000),
            SyncState::NeverUploaded
        );
    }

    #[test]
    fn classify_handles_synced() {
        let m = SessionSyncMetadata {
            last_uploaded: Some("2026-07-09T00:00:00Z".into()),
            remote_sha: Some("abc".into()),
            last_local_modified: Some("2026-07-09T00:00:00Z".into()),
            sync_state: SyncState::Synced,
        };
        let stored_ts = parse_rfc3339_to_unix(m.last_local_modified.as_deref().unwrap())
            .unwrap();
        assert_eq!(classify_sync_state(Some(&m), stored_ts), SyncState::Synced);
    }

    #[test]
    fn classify_handles_out_of_sync() {
        let m = SessionSyncMetadata {
            last_uploaded: Some("2026-07-09T00:00:00Z".into()),
            remote_sha: Some("abc".into()),
            last_local_modified: Some("2026-07-09T00:00:00Z".into()),
            sync_state: SyncState::Synced,
        };
        let stored_ts = parse_rfc3339_to_unix(m.last_local_modified.as_deref().unwrap())
            .unwrap();
        assert_eq!(
            classify_sync_state(Some(&m), stored_ts + 3600),
            SyncState::OutOfSync
        );
    }

    #[test]
    fn remote_session_path_format() {
        assert_eq!(
            remote_session_path("-home-ayan-de-Projects-foo", "abc-uuid"),
            "sessions/-home-ayan-de-Projects-foo/abc-uuid.jsonl"
        );
    }

    #[test]
    fn missing_files_return_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load_github_sync_config(&github_sync_path(dir.path())).unwrap();
        assert!(!cfg.is_connected);
        let m = load_path_mappings(&path_mappings_path(dir.path())).unwrap();
        assert!(m.mappings.is_empty());
    }

    // Smoke test for round-trip JSON.
    #[test]
    fn sync_config_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = github_sync_path(dir.path());
        let cfg = GitHubSyncConfig {
            schema_version: 1,
            is_connected: true,
            username: Some("octocat".into()),
            avatar_url: None,
            repo_name: "claude-sessions".into(),
            last_sync: Some("2026-07-09T12:00:00Z".into()),
            privacy_consent_given: true,
        };
        save_github_sync_config(&path, &cfg).unwrap();
        let loaded = load_github_sync_config(&path).unwrap();
        assert_eq!(loaded.username.as_deref(), Some("octocat"));
        assert!(loaded.privacy_consent_given);
    }
}