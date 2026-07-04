//! Settings.json commands: detect active provider, load a provider into
//! `~/.claude/settings.json`, save current config as a new provider.

use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::merge::{derive_provider_name, merge_env, provider_env_block};
use crate::models::{AppError, AppResult, Provider};
use crate::state::AppState;
use crate::storage::{
    load_providers_file, read_settings, save_providers_file, settings_path,
    write_settings_atomic,
};

#[tauri::command]
pub fn get_active_provider_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Option<Provider>> {
    let settings = read_settings(&settings_path())?;
    let env_block = settings
        .as_ref()
        .and_then(|v| v.get("env"))
        .cloned()
        .unwrap_or(Value::Null);

    if env_block.is_null() || !env_block.is_object() || env_block.as_object().unwrap().is_empty()
    {
        return Ok(None);
    }

    let providers = load_providers_file(&state.providers_path())?.providers;

    for p in providers {
        let token = match state.keyring.get_token(&p.id) {
            Ok(t) => t,
            Err(_) => continue, // skip providers whose token we can't read
        };
        let candidate = Value::Object(provider_env_block(&p, &token));
        if env_blocks_equal(&env_block, &candidate) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

#[tauri::command]
pub fn load_provider_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let providers = load_providers_file(&state.providers_path())?.providers;
    let provider = providers
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    let token = state.keyring.get_token(&id)?;

    let path = settings_path();
    let mut current = read_settings(&path)?.unwrap_or_else(|| {
        // settings.json doesn't exist yet — start with empty object
        Value::Object(Map::new())
    });

    let provider_env = Value::Object(provider_env_block(&provider, &token));
    let existing_env = current.get("env").cloned();

    let new_env = merge_env(existing_env.as_ref(), &provider_env);
    if let Value::Object(map) = &mut current {
        map.insert("env".into(), new_env);
    } else {
        return Err(AppError::Validation(
            "settings.json root is not an object".into(),
        ));
    }

    write_settings_atomic(&path, &current, &state.backups_dir())?;
    Ok(())
}

#[tauri::command]
pub fn save_current_as_provider_cmd(
    state: tauri::State<'_, AppState>,
    name: String,
) -> AppResult<Provider> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    let mut file = load_providers_file(&state.providers_path())?;
    if file.providers.iter().any(|p| p.name == name) {
        return Err(AppError::DuplicateName(name));
    }

    let settings = read_settings(&settings_path())?;
    let env_block = settings
        .as_ref()
        .and_then(|v| v.get("env"))
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    let env_obj = env_block
        .as_object()
        .ok_or_else(|| AppError::Validation("settings.json env is not an object".into()))?;

    let base_url = string_field(env_obj, "ANTHROPIC_BASE_URL")
        .ok_or_else(|| AppError::Validation("settings.json is missing ANTHROPIC_BASE_URL".into()))?;
    let token = string_field(env_obj, "ANTHROPIC_AUTH_TOKEN")
        .ok_or_else(|| AppError::Validation("settings.json is missing ANTHROPIC_AUTH_TOKEN".into()))?;

    let now = Utc::now().to_rfc3339();
    let provider = Provider {
        id: Uuid::new_v4().to_string(),
        name: derive_provider_name(&base_url),
        base_url,
        model: string_field(env_obj, "ANTHROPIC_MODEL"),
        small_fast_model: string_field(env_obj, "ANTHROPIC_SMALL_FAST_MODEL"),
        default_sonnet_model: string_field(env_obj, "ANTHROPIC_DEFAULT_SONNET_MODEL"),
        default_opus_model: string_field(env_obj, "ANTHROPIC_DEFAULT_OPUS_MODEL"),
        default_haiku_model: string_field(env_obj, "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        api_timeout_ms: env_obj
            .get("API_TIMEOUT_MS")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok()),
        disable_nonessential_traffic: env_obj
            .get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "1" | "true" => Some(true),
                "0" | "false" => Some(false),
                _ => None,
            }),
        created_at: now.clone(),
        updated_at: now,
    };

    state.keyring.set_token(&provider.id, &token)?;
    file.providers.push(provider.clone());
    save_providers_file(&state.providers_path(), &file)?;
    Ok(provider)
}

#[tauri::command]
pub fn preview_provider_env_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> AppResult<Map<String, Value>> {
    let providers = load_providers_file(&state.providers_path())?.providers;
    let provider = providers
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    let token = state.keyring.get_token(&id)?;
    Ok(provider_env_block(&provider, &token))
}

/// Returns the canonical env keys present in `~/.claude/settings.json`.
/// Used by the UI to detect "custom configuration" — env present but no
/// saved provider matches.
#[tauri::command]
pub fn get_settings_env_keys_cmd() -> AppResult<Vec<String>> {
    let settings = read_settings(&settings_path())?;
    let Some(value) = settings else {
        return Ok(Vec::new());
    };
    let Some(env_obj) = value.get("env").and_then(|v| v.as_object()) else {
        return Ok(Vec::new());
    };
    Ok(env_obj.keys().cloned().collect())
}

/// Compare two env blocks for equality ignoring key order. Used by
/// `get_active_provider` to match the live settings.json env to a
/// saved provider's reconstructed env.
fn env_blocks_equal(a: &Value, b: &Value) -> bool {
    let a_map = match a.as_object() {
        Some(m) => m,
        None => return false,
    };
    let b_map = match b.as_object() {
        Some(m) => m,
        None => return false,
    };
    let a_sorted: BTreeMap<&str, &Value> = a_map.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let b_sorted: BTreeMap<&str, &Value> = b_map.iter().map(|(k, v)| (k.as_str(), v)).collect();
    a_sorted.len() == b_sorted.len()
        && a_sorted
            .iter()
            .zip(b_sorted.iter())
            .all(|((ak, av), (bk, bv))| ak == bk && av == bv)
}

fn string_field(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn env_blocks_equal_ignores_order() {
        let a = json!({"A": "1", "B": "2"});
        let b = json!({"B": "2", "A": "1"});
        assert!(env_blocks_equal(&a, &b));
    }

    #[test]
    fn env_blocks_equal_detects_value_diff() {
        let a = json!({"A": "1"});
        let b = json!({"A": "2"});
        assert!(!env_blocks_equal(&a, &b));
    }

    #[test]
    fn env_blocks_equal_detects_extra_key() {
        let a = json!({"A": "1"});
        let b = json!({"A": "1", "B": "2"});
        assert!(!env_blocks_equal(&a, &b));
    }
}