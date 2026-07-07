//! Reads/writes per-provider tracker configs and cached usage snapshots.
//!
//! ## Layout
//!
//! One file: `<app-data>/trackers.json`. Schema:
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "trackers": {
//!     "<provider_id>": {
//!       "source": "anthropic_admin",
//!       "fields": { "admin_api_key": "***" },
//!       "last_usage": { ... TrackerUsage ... },
//!       "last_fetched_at": "...",
//!       "last_error": null,
//!       "updated_at": "..."
//!     }
//!   }
//! }
//! ```
//!
//! ## Secrets
//!
//! The `fields` blob may contain sensitive values (api keys, cookies). The
//! caller is responsible for splitting secrets out into the keyring before
//! calling `save_tracker`. The blob stored on disk is whatever the caller
//! hands us — for v1 the public sources' secrets come from the keyring
//! in the command layer, not the blob.
//!
//! ## Atomic write
//!
//! Same tempfile + fsync + rename pattern as `providers.rs` and
//! `settings.rs`. Backups are NOT taken on every save (the file is small
//! and the contents are recoverable from the keyring + last successful
//! usage snapshot).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::models::{AppError, AppResult};
use crate::tracker::{SourceId, TrackerUsage};

#[allow(dead_code)] // Documented constant for future external callers.
pub const TRACKERS_FILENAME: &str = "trackers.json";
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackersFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub trackers: HashMap<String, TrackerConfig>,
}

fn default_schema_version() -> u32 {
    SUPPORTED_SCHEMA_VERSION
}

/// Per-provider tracker state. `fields` is the source-specific config
/// blob — the storage layer doesn't know what's in it. Validation is the
/// adapter's job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackerConfig {
    /// Stable source id (e.g. `"anthropic_admin"`).
    pub source: String,
    /// Source-specific config blob. Sensitive fields are absent — they
    /// live in the keyring under `(KEYRING_SERVICE, "{provider_id}:{field_key}")`.
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
    /// Cached last successful fetch. `None` if we've never successfully
    /// refreshed this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<TrackerUsage>,
    /// ISO-8601 timestamp of the last fetch attempt (success or fail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fetched_at: Option<String>,
    /// Last error string, for surfacing in the UI without re-running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// ISO-8601 timestamp the config was last edited.
    pub updated_at: String,
}

impl TrackerConfig {
    pub fn source_id(&self) -> AppResult<SourceId> {
        SourceId::parse(&self.source)
    }
}

pub fn load_trackers_file(path: &Path) -> AppResult<TrackersFile> {
    if !path.exists() {
        return Ok(TrackersFile {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            trackers: HashMap::new(),
        });
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(TrackersFile {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            trackers: HashMap::new(),
        });
    }
    let file: TrackersFile = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::MalformedSettings {
            path: path.display().to_string(),
            message: format!("trackers.json: {e}"),
        }
    })?;
    if file.schema_version > SUPPORTED_SCHEMA_VERSION {
        return Err(AppError::Internal(format!(
            "trackers.json schema version {} is newer than supported ({}); \
             please update the app",
            file.schema_version, SUPPORTED_SCHEMA_VERSION
        )));
    }
    Ok(file)
}

pub fn save_trackers_file(path: &Path, file: &TrackersFile) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(file)?;
    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!("trackers path has no parent: {}", path.display()))
    })?;
    let mut tmp = NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, &bytes)?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(path) {
        return Err(AppError::Io(e.error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir(name: &str) -> std::path::PathBuf {
        let d = tempfile::tempdir().unwrap().keep().join(name);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn load_returns_default_when_missing() {
        let p = fresh_dir("data").join("trackers.json");
        let f = load_trackers_file(&p).unwrap();
        assert_eq!(f.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert!(f.trackers.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = fresh_dir("data");
        let p = dir.join("trackers.json");
        let mut trackers = HashMap::new();
        trackers.insert(
            "prov-1".to_string(),
            TrackerConfig {
                source: "anthropic_admin".to_string(),
                fields: {
                    let mut m = serde_json::Map::new();
                    m.insert("admin_api_key".into(), serde_json::json!("***"));
                    m
                },
                last_usage: Some(TrackerUsage {
                    windows: vec![],
                    models: vec![],
                    cost_usd: Some(1.5),
                    fetched_at: "2026-07-08T00:00:00Z".into(),
                    note: None,
                }),
                last_fetched_at: Some("2026-07-08T00:00:00Z".into()),
                last_error: None,
                updated_at: "2026-07-08T00:00:00Z".into(),
            },
        );
        let file = TrackersFile {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            trackers,
        };
        save_trackers_file(&p, &file).unwrap();
        let loaded = load_trackers_file(&p).unwrap();
        assert_eq!(loaded.trackers.len(), 1);
        let t = &loaded.trackers["prov-1"];
        assert_eq!(t.source, "anthropic_admin");
        assert_eq!(t.last_usage.as_ref().and_then(|u| u.cost_usd), Some(1.5));
    }

    #[test]
    fn malformed_file_errors() {
        let dir = fresh_dir("data");
        let p = dir.join("trackers.json");
        fs::write(&p, "{not json").unwrap();
        let err = load_trackers_file(&p).unwrap_err();
        assert!(matches!(err, AppError::MalformedSettings { .. }));
    }

    #[test]
    fn future_schema_version_errors() {
        let dir = fresh_dir("data");
        let p = dir.join("trackers.json");
        fs::write(
            &p,
            r#"{"schema_version": 99, "trackers": {}}"#,
        )
        .unwrap();
        let err = load_trackers_file(&p).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[test]
    fn source_id_helper_known() {
        let t = TrackerConfig {
            source: "manual_json".into(),
            fields: Default::default(),
            last_usage: None,
            last_fetched_at: None,
            last_error: None,
            updated_at: "2026-07-08T00:00:00Z".into(),
        };
        assert_eq!(t.source_id().unwrap(), SourceId::ManualJson);
    }

    #[test]
    fn source_id_helper_unknown() {
        let t = TrackerConfig {
            source: "nope".into(),
            fields: Default::default(),
            last_usage: None,
            last_fetched_at: None,
            last_error: None,
            updated_at: "2026-07-08T00:00:00Z".into(),
        };
        assert!(t.source_id().is_err());
    }
}
