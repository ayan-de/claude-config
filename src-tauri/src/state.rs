//! Shared state held by the Tauri app. Cloned into each `#[tauri::command]`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::github::cache::GitHubCache;
use crate::storage::KeyringStore;

/// All shared state the Tauri commands need. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct AppState {
    pub keyring: KeyringStore,
    pub app_data_dir: Arc<PathBuf>,
    /// In-memory cache for the GitHub connection — owner, default
    /// branch, last seen commit SHA, last tree, last sessions list.
    /// See `docs/superpowers/plans/2026-07-11-remote-sessions-caching.md`.
    pub github_cache: Arc<Mutex<GitHubCache>>,
}

impl AppState {
    pub fn providers_path(&self) -> PathBuf {
        self.app_data_dir.join("providers.json")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.app_data_dir.join("backups")
    }
    /// Pointer file: `{"activeProviderId": "<uuid>"}`. Written on every
    /// successful `load_provider_cmd`; read by `get_active_provider_cmd`.
    pub fn state_path(&self) -> PathBuf {
        self.app_data_dir.join("state.json")
    }
    /// Per-provider tracker configs + cached usage snapshots. Each provider
    /// can have at most one tracker; the file is the source of truth for
    /// non-secret config fields, while secrets live in the OS keyring.
    pub fn trackers_path(&self) -> PathBuf {
        self.app_data_dir.join("trackers.json")
    }
}