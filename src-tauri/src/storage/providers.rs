//! Reads/writes the saved providers file in the app's data dir.
//!
//! Schema versioned. On unrecognized future versions we back up the existing
//! file and start fresh — never silently lose data.
//!
//! ## Silent v1 → v2 → v3 migration
//!
//! v1 files have no `kind` field on providers (only Custom-relay was
//! supported). When we read a v1 file, serde defaults the missing `kind` to
//! `Custom` (see `models::default_kind_custom`), and we bump the in-memory
//! `schema_version` to 3 so the next save re-serializes as v3.
//!
//! v2 files have no `logoSvg` field; serde defaults it to `None`. Same
//! schema_version bump.

use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::NamedTempFile;

use crate::models::{AppError, AppResult, ProvidersFile};

#[allow(dead_code)] // Documented constant for future external callers.
pub const PROVIDERS_FILENAME: &str = "providers.json";
pub const SUPPORTED_SCHEMA_VERSION: u32 = 3;

#[allow(dead_code)] // Reserved for surfacing schema errors back to UI explicitly.
#[derive(Debug)]
pub enum ProvidersFileError {
    UnsupportedVersion {
        found: u32,
        supported: u32,
        raw_path: std::path::PathBuf,
    },
}

impl std::fmt::Display for ProvidersFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvidersFileError::UnsupportedVersion {
                found, supported, ..
            } => {
                write!(
                    f,
                    "unsupported providers.json schema version {found}; this app supports up to {supported}"
                )
            }
        }
    }
}
impl std::error::Error for ProvidersFileError {}

/// Load providers.json from disk. Returns `Ok(ProvidersFile::default())`
/// if the file doesn't exist yet (first launch).
///
/// v1 files are auto-migrated in memory: missing `kind` fields default to
/// `Custom` and `schema_version` is bumped to 2 so the next `save_providers_file`
/// persists the migration.
pub fn load_providers_file(path: &Path) -> AppResult<ProvidersFile> {
    if !path.exists() {
        return Ok(ProvidersFile::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(ProvidersFile::default());
    }
    let raw: Value =
        serde_json::from_slice(&bytes).map_err(|e| AppError::MalformedSettings {
            path: path.display().to_string(),
            message: format!("providers.json: {e}"),
        })?;

    // Schema-version pre-check. Real files use snake_case `schema_version`
    // (matches the struct field). Fall back to camelCase for any historical
    // divergence — cheap and future-proof.
    let version = raw
        .get("schema_version")
        .or_else(|| raw.get("schemaVersion"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if version > SUPPORTED_SCHEMA_VERSION {
        // Back up and reset rather than panic; surface via error so caller
        // can decide whether to show a dialog or proceed.
        let backup = path.with_extension(format!("json.unsupported-v{version}.bak"));
        let _ = fs::copy(path, &backup);
        return Err(AppError::Internal(format!(
            "providers.json schema version {version} is newer than supported ({SUPPORTED_SCHEMA_VERSION}); backed up to {}",
            backup.display()
        )));
    }

    let mut file: ProvidersFile = serde_json::from_value(raw)?;

    // Silent auto-migration: force in-memory version to current so the next
    // save writes the new schema. Existing providers already have kind=Custom
    // via serde default when the field was missing.
    if file.schema_version < SUPPORTED_SCHEMA_VERSION {
        log::info!(
            "migrating providers.json from schema v{} to v{SUPPORTED_SCHEMA_VERSION}",
            file.schema_version
        );
        file.schema_version = SUPPORTED_SCHEMA_VERSION;
    }

    Ok(file)
}

/// Atomic write of providers.json. Backups are the responsibility of the
/// caller (settings.json writes are backed up automatically; providers.json
/// has schema versioning + manual export as its safety net).
pub fn save_providers_file(path: &Path, file: &ProvidersFile) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(file)?;
    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!("providers path has no parent: {}", path.display()))
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
    use crate::models::{Provider, ProviderKind};

    fn fresh_dir(name: &str) -> std::path::PathBuf {
        let d = tempfile::tempdir().unwrap().keep().join(name);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_custom(id: &str, name: &str) -> Provider {
        Provider {
            id: id.into(),
            name: name.into(),
            kind: ProviderKind::Custom,
            base_url: Some("https://x".into()),
            aws_region: None,
            aws_profile: None,
            vertex_project_id: None,
            vertex_region: None,
            google_application_credentials: None,
            subscription_label: None,
            model: Some("claude-sonnet-4-6".into()),
            small_fast_model: None,
            default_sonnet_model: None,
            default_opus_model: None,
            default_haiku_model: None,
            api_timeout_ms: None,
            disable_nonessential_traffic: None,
            logo_svg: None,
            created_at: "2026-07-04T00:00:00Z".into(),
            updated_at: "2026-07-04T00:00:00Z".into(),
        }
    }

    #[test]
    fn load_returns_default_when_missing() {
        let p = fresh_dir("data").join("providers.json");
        let f = load_providers_file(&p).unwrap();
        assert_eq!(f.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert!(f.providers.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip_v3() {
        let dir = fresh_dir("data");
        let p = dir.join("providers.json");
        let mut provider = sample_custom("abc", "test");
        provider.logo_svg = Some(r#"<svg viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"/></svg>"#.into());
        let file = ProvidersFile {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            providers: vec![provider],
        };
        save_providers_file(&p, &file).unwrap();
        let loaded = load_providers_file(&p).unwrap();
        assert_eq!(loaded.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].name, "test");
        assert_eq!(loaded.providers[0].kind, ProviderKind::Custom);
        assert_eq!(loaded.providers[0].model.as_deref(), Some("claude-sonnet-4-6"));
        // logo_svg survives the round-trip.
        assert!(loaded.providers[0]
            .logo_svg
            .as_deref()
            .map(|s| s.contains("<svg"))
            .unwrap_or(false));
    }

    #[test]
    fn v2_file_loads_with_logo_svg_none() {
        // A pre-v3 providers.json file (schema_version: 2, no logoSvg on
        // providers) must load successfully and surface logo_svg as None.
        // The schema_version is bumped in-memory so the next save is v3.
        let dir = fresh_dir("data");
        let p = dir.join("providers.json");
        let v2_json = r#"{
            "schema_version": 2,
            "providers": [
                {
                    "id": "7ad6c1f5",
                    "name": "minimax",
                    "kind": "custom",
                    "base_url": "https://api.minimax.io/anthropic",
                    "model": "minimax",
                    "created_at": "2026-07-04T17:07:06Z",
                    "updated_at": "2026-07-04T17:07:06Z"
                }
            ]
        }"#;
        fs::write(&p, v2_json).unwrap();
        let loaded = load_providers_file(&p).unwrap();
        assert_eq!(loaded.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert_eq!(loaded.providers.len(), 1);
        assert!(loaded.providers[0].logo_svg.is_none());
    }

    #[test]
    fn migrates_v1_file_silently() {
        // Mimics a v1 providers.json: no `kind` field on providers, schema
        // version 1. Deserialization must succeed (with kind=Custom), and the
        // loaded schema_version must be bumped to 2 so the next save is v2.
        let dir = fresh_dir("data");
        let p = dir.join("providers.json");
        let v1_json = r#"{
            "schema_version": 1,
            "providers": [
                {
                    "id": "7ad6c1f5-d79b-49bd-a6cb-29fa6ea01602",
                    "name": "minimax",
                    "base_url": "https://api.minimax.io/anthropic",
                    "model": "MiniMax-M3[1m]",
                    "smallFastModel": "MiniMax-M3[1m]",
                    "created_at": "2026-07-04T17:07:06Z",
                    "updated_at": "2026-07-04T17:07:06Z"
                },
                {
                    "id": "aa776292-8215-4a92-a3b7-b12b1518534d",
                    "name": "freemodel",
                    "base_url": "https://cc.freemodel.dev",
                    "created_at": "2026-07-04T20:53:30Z",
                    "updated_at": "2026-07-04T20:53:30Z"
                }
            ]
        }"#;
        fs::write(&p, v1_json).unwrap();
        let loaded = load_providers_file(&p).unwrap();
        assert_eq!(loaded.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert_eq!(loaded.providers.len(), 2);
        assert!(loaded.providers.iter().all(|p| p.kind == ProviderKind::Custom));
        assert_eq!(
            loaded.providers[0].base_url.as_deref(),
            Some("https://api.minimax.io/anthropic")
        );
    }

    #[test]
    fn migration_persists_after_save() {
        let dir = fresh_dir("data");
        let p = dir.join("providers.json");
        let v1_json = r#"{
            "schema_version": 1,
            "providers": [
                {"id": "a", "name": "n", "base_url": "https://x",
                 "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"}
            ]
        }"#;
        fs::write(&p, v1_json).unwrap();
        let loaded = load_providers_file(&p).unwrap();
        // Round-trip: save the loaded (now v3) file, re-read, should be clean v3.
        save_providers_file(&p, &loaded).unwrap();
        let reloaded = load_providers_file(&p).unwrap();
        assert_eq!(reloaded.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert_eq!(reloaded.providers[0].kind, ProviderKind::Custom);
        // Verify the raw JSON now contains the kind field.
        let raw = fs::read_to_string(&p).unwrap();
        assert!(raw.contains("\"kind\": \"custom\""), "raw json: {raw}");
        assert!(
            raw.contains(&format!("\"schema_version\": {SUPPORTED_SCHEMA_VERSION}")),
            "raw json: {raw}"
        );
    }

    #[test]
    fn unsupported_version_errors_and_backs_up() {
        let dir = fresh_dir("data");
        let p = dir.join("providers.json");
        fs::write(&p, r#"{"schema_version": 99, "providers": []}"#).unwrap();
        let err = load_providers_file(&p).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
        // .bak file created
        let mut entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        entries.sort();
        assert!(
            entries.iter().any(|n| n.contains("unsupported-v99")),
            "expected backup file, got {entries:?}"
        );
    }
}
