//! Export/import the providers list. By default, auth tokens are NOT
//! exported — they remain in the user's OS keyring. The `include_secrets`
//! flag must be opted-in, and the UI must confirm before invoking it.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::models::{AppError, AppResult, Provider};
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
        schema_version: 1,
        exported_at: Utc::now().to_rfc3339(),
        source_app: "claude-config".into(),
        providers: file.providers,
    };
    let json = serde_json::to_vec_pretty(&payload)?;

    if !include_secrets {
        // The Payload itself never embeds tokens (Provider struct doesn't
        // have one), but we still re-serialize with explicit redaction to
        // future-proof against a struct change.
        let mut redacted: Map<String, Value> = serde_json::from_slice(&json)?;
        if let Some(Value::Array(arr)) = redacted.get_mut("providers") {
            for p in arr.iter_mut() {
                if let Some(obj) = p.as_object_mut() {
                    obj.remove("auth_token");
                    obj.insert("auth_token".into(), Value::String("[redacted]".into()));
                }
            }
        }
        let redacted_bytes = serde_json::to_vec_pretty(&Value::Object(redacted))?;
        write_atomic(&dest, &redacted_bytes)?;
    } else {
        // include_secrets: write a sidecar JSON with tokens that pairs with
        // the redacted main file. Two-file export keeps secrets out of
        // accidental shares of the main file.
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
    let payload: ExportPayload = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::MalformedSettings {
            path: src.clone(),
            message: format!("import: {e}"),
        }
    })?;
    let mut secrets: BTreeMap<String, String> = BTreeMap::new();
    if let Some(path) = secrets_src {
        let bytes = fs::read(&path).map_err(AppError::Io)?;
        secrets = serde_json::from_slice(&bytes).map_err(AppError::Json)?;
    }
    let mut file = load_providers_file(&state.providers_path())?;
    let mut added = 0;
    let now = Utc::now().to_rfc3339();
    for p in payload.providers {
        // Skip duplicates (by name) so re-import doesn't dupe.
        if file.providers.iter().any(|existing| existing.name == p.name) {
            continue;
        }
        let new_id = Uuid::new_v4().to_string();
        let token = secrets
            .get(&p.id)
            .cloned()
            .unwrap_or_default();
        if !token.is_empty() {
            state.keyring.set_token(&new_id, &token)?;
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
        match state.keyring.get_token(&p.id) {
            Ok(t) => {
                map.insert(p.id.clone(), Value::String(t));
            }
            Err(e) => {
                log::warn!("could not read token for provider {}: {e}", p.id);
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
    let parent = p.parent().filter(|x| !x.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, bytes)?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(p) {
        return Err(AppError::Io(e.error));
    }
    Ok(())
}