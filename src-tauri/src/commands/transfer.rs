//! Export/import the providers list. By default, secrets are NOT exported —
//! they remain in the user's OS keyring. The `include_secrets` flag must be
//! opted-in, and the UI must confirm before invoking it.
//!
//! In v2 the sidecar secrets file stores full `ProviderSecret` JSON blobs
//! (keyed by provider id), so any kind — Subscription OAuth, AWS creds,
//! Console API key, custom bearer — round-trips cleanly.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::models::{AppError, AppResult, Provider, ProviderSecret};
use crate::state::AppState;
use crate::storage::{load_providers_file, save_providers_file};

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportPayload {
    pub schema_version: u32,
    pub exported_at: String,
    pub source_app: String,
    pub providers: Vec<Provider>,
}

#[tauri::command]
pub fn export_providers_cmd(
    state: tauri::State<'_, AppState>,
    dest: String,
    include_secrets: bool,
) -> AppResult<()> {
    let file = load_providers_file(&state.providers_path())?;
    let payload = ExportPayload {
        schema_version: 2,
        exported_at: Utc::now().to_rfc3339(),
        source_app: "claude-config".into(),
        providers: file.providers,
    };
    let json = serde_json::to_vec_pretty(&payload)?;

    if !include_secrets {
        write_atomic(&dest, &json)?;
    } else {
        // include_secrets: sidecar JSON contains a map<id, ProviderSecret>.
        // Two-file export keeps secrets out of accidental shares of the main
        // file.
        let secrets = collect_secrets(&state, &payload.providers)?;
        write_atomic(&dest, &json)?;
        let secrets_dest = format!("{dest}.secrets.json");
        write_atomic(&secrets_dest, &serde_json::to_vec_pretty(&secrets)?)?;
    }
    Ok(())
}

#[tauri::command]
pub fn import_providers_cmd(
    state: tauri::State<'_, AppState>,
    src: String,
    secrets_src: Option<String>,
) -> AppResult<usize> {
    let bytes = fs::read(&src).map_err(AppError::Io)?;
    let payload: ExportPayload =
        serde_json::from_slice(&bytes).map_err(|e| AppError::MalformedSettings {
            path: src.clone(),
            message: format!("import: {e}"),
        })?;
    let secrets: BTreeMap<String, ProviderSecret> = if let Some(path) = secrets_src {
        let bytes = fs::read(&path).map_err(AppError::Io)?;
        // Try the v2 shape first (ProviderSecret map). Fall back to v1 raw-token
        // map so pre-v2 exports still import.
        match serde_json::from_slice::<BTreeMap<String, ProviderSecret>>(&bytes) {
            Ok(m) => m,
            Err(_) => {
                let legacy: BTreeMap<String, String> =
                    serde_json::from_slice(&bytes).map_err(AppError::Json)?;
                legacy
                    .into_iter()
                    .map(|(k, v)| (k, ProviderSecret::Custom { auth_token: v }))
                    .collect()
            }
        }
    } else {
        BTreeMap::new()
    };
    let mut file = load_providers_file(&state.providers_path())?;
    let mut added = 0;
    let now = Utc::now().to_rfc3339();
    for p in payload.providers {
        // Skip duplicates (by name) so re-import doesn't dupe.
        if file.providers.iter().any(|existing| existing.name == p.name) {
            continue;
        }
        let new_id = Uuid::new_v4().to_string();
        if let Some(secret) = secrets.get(&p.id) {
            state.keyring.set_secret(&new_id, secret)?;
        }
        let mut new_provider = p;
        new_provider.id = new_id;
        new_provider.created_at = now.clone();
        new_provider.updated_at = now.clone();
        file.providers.push(new_provider);
        added += 1;
    }
    save_providers_file(&state.providers_path(), &file)?;
    Ok(added)
}

fn collect_secrets(
    state: &AppState,
    providers: &[Provider],
) -> AppResult<Map<String, Value>> {
    let mut map = Map::new();
    for p in providers {
        match state.keyring.get_secret(&p.id) {
            Ok(s) => {
                map.insert(p.id.clone(), serde_json::to_value(&s)?);
            }
            Err(e) => {
                log::warn!("could not read secret for provider {}: {e}", p.id);
            }
        }
    }
    Ok(map)
}

fn write_atomic(path: &str, bytes: &[u8]) -> AppResult<()> {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let parent = p
        .parent()
        .filter(|x| !x.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, bytes)?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(p) {
        return Err(AppError::Io(e.error));
    }
    Ok(())
}
