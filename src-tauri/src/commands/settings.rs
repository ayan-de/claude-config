//! Settings.json commands: detect active provider, load a provider into
//! `~/.claude/settings.json`, save current config as a new provider.
//!
//! The active-provider pointer lives in `<app-data>/state.json` (the
//! `activeProviderId` field). `load_provider_cmd` maintains it, and
//! `get_active_provider_cmd` reads it as the primary source of truth.
//! Env-block equality matching is only used as a first-launch fallback.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::Utc;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::merge::{derive_provider_name, merge_env, provider_env_block};
use crate::models::{
    AppError, AppResult, Provider, ProviderKind, ProviderSecret, StateFile,
};
use crate::state::AppState;
use crate::storage::{
    load_providers_file, read_credentials_oauth, read_settings, save_providers_file,
    settings_path, write_credentials_oauth, write_settings_atomic,
};

#[tauri::command]
pub fn get_active_provider_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Option<Provider>> {
    let providers = load_providers_file(&state.providers_path())?.providers;

    // Preferred path: state.json pointer.
    if let Some(id) = read_state(&state.state_path())?.active_provider_id {
        if let Some(p) = providers.iter().find(|p| p.id == id).cloned() {
            return Ok(Some(p));
        }
    }

    // Fallback for first-launch cases where state.json doesn't exist yet.
    // Only meaningful for kinds that write to settings.json.env (i.e. not
    // Subscription); env matching can't identify a Subscription session.
    let settings = read_settings(&settings_path())?;
    let env_block = settings
        .as_ref()
        .and_then(|v| v.get("env"))
        .cloned()
        .unwrap_or(Value::Null);
    if env_block.is_null() || !env_block.is_object() {
        return Ok(None);
    }

    for p in providers {
        if p.kind == ProviderKind::Subscription {
            continue;
        }
        let secret = match state.keyring.get_secret(&p.id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let candidate = Value::Object(provider_env_block(&p, &secret));
        if env_blocks_equal(&env_block, &candidate) {
            // Opportunistically persist the pointer so future calls are cheap.
            let _ = write_state(
                &state.state_path(),
                &StateFile {
                    active_provider_id: Some(p.id.clone()),
                },
            );
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
    let new_provider = providers
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(id.clone()))?;

    // Step 1: if a Subscription provider is currently active, re-snapshot its
    // OAuth blob from ~/.claude/.credentials.json so refresh-token rotation
    // isn't lost across switches.
    let state_before = read_state(&state.state_path())?;
    if let Some(prev_id) = &state_before.active_provider_id {
        if let Some(prev) = providers.iter().find(|p| &p.id == prev_id) {
            if prev.kind == ProviderKind::Subscription && prev.id != new_provider.id {
                if let Some(oauth) = read_credentials_oauth()? {
                    let snapshot = ProviderSecret::Subscription { oauth };
                    if let Err(e) = state.keyring.set_secret(&prev.id, &snapshot) {
                        log::warn!(
                            "could not re-snapshot subscription OAuth for previous active provider {}: {e}",
                            prev.id
                        );
                    }
                }
            }
        }
    }

    // Step 2: load the new provider's secret.
    let secret = state.keyring.get_secret(&new_provider.id)?;
    if secret.kind() != new_provider.kind {
        return Err(AppError::Internal(format!(
            "keyring secret kind ({:?}) does not match provider kind ({:?}) for {}",
            secret.kind(),
            new_provider.kind,
            new_provider.id
        )));
    }

    // Step 3: if switching TO a Subscription provider, restore its OAuth blob
    // into ~/.claude/.credentials.json.
    if let ProviderSecret::Subscription { oauth } = &secret {
        write_credentials_oauth(oauth)?;
    }

    // Step 4: merge and atomic-write settings.json env.
    let path = settings_path();
    let mut current = read_settings(&path)?.unwrap_or_else(|| Value::Object(Map::new()));
    let provider_env = Value::Object(provider_env_block(&new_provider, &secret));
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

    // Step 5: persist the new active-provider pointer.
    write_state(
        &state.state_path(),
        &StateFile {
            active_provider_id: Some(new_provider.id.clone()),
        },
    )?;

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

    // Detect the kind from what's actually in the env block (and, for
    // Subscription, from .credentials.json).
    let (kind, secret_opt) = detect_current_kind(env_obj, state.keyring.is_available())?;

    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    // Reasonable auto-name per kind if the caller passed a blank sentinel.
    // (Today the frontend always passes a real name; kept for parity with
    // first-launch import.)
    let display_name = if name.trim().is_empty() {
        auto_name(&kind, env_obj)
    } else {
        name
    };

    let provider = Provider {
        id: id.clone(),
        name: display_name,
        kind,
        base_url: string_field(env_obj, "ANTHROPIC_BASE_URL"),
        aws_region: string_field(env_obj, "AWS_REGION"),
        aws_profile: string_field(env_obj, "AWS_PROFILE"),
        vertex_project_id: string_field(env_obj, "ANTHROPIC_VERTEX_PROJECT_ID"),
        vertex_region: string_field(env_obj, "CLOUD_ML_REGION"),
        google_application_credentials: string_field(env_obj, "GOOGLE_APPLICATION_CREDENTIALS"),
        subscription_label: None,
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

    if let Some(secret) = secret_opt {
        state.keyring.set_secret(&id, &secret)?;
    }
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
    let secret = state.keyring.get_secret(&id)?;
    Ok(provider_env_block(&provider, &secret))
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

/// Inspect the live env block (and, for Subscription, `.credentials.json`)
/// and decide which `ProviderKind` describes it. Returns the kind plus the
/// secret we should stash for it (None for Vertex, since GOOGLE_APPLICATION_CREDENTIALS
/// is just a file path).
fn detect_current_kind(
    env_obj: &Map<String, Value>,
    keyring_available: bool,
) -> AppResult<(ProviderKind, Option<ProviderSecret>)> {
    let has = |k: &str| env_obj.get(k).and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty());
    let get = |k: &str| env_obj.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();

    if has("CLAUDE_CODE_USE_BEDROCK") {
        let secret = ProviderSecret::Bedrock {
            access_key_id: get("AWS_ACCESS_KEY_ID"),
            secret_access_key: get("AWS_SECRET_ACCESS_KEY"),
            session_token: env_obj
                .get("AWS_SESSION_TOKEN")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        };
        return Ok((ProviderKind::Bedrock, Some(secret)));
    }
    if has("CLAUDE_CODE_USE_VERTEX") {
        return Ok((ProviderKind::Vertex, Some(ProviderSecret::Vertex {})));
    }
    if has("ANTHROPIC_API_KEY") && !has("ANTHROPIC_BASE_URL") {
        return Ok((
            ProviderKind::Console,
            Some(ProviderSecret::Console {
                api_key: get("ANTHROPIC_API_KEY"),
            }),
        ));
    }
    if has("ANTHROPIC_BASE_URL") && has("ANTHROPIC_AUTH_TOKEN") {
        return Ok((
            ProviderKind::Custom,
            Some(ProviderSecret::Custom {
                auth_token: get("ANTHROPIC_AUTH_TOKEN"),
            }),
        ));
    }

    // Env block doesn't identify a known kind. Try Subscription:
    // OAuth in .credentials.json + empty env is the signature.
    if env_obj.is_empty() || !env_obj.keys().any(|k| is_canonical_marker(k)) {
        if let Some(oauth) = read_credentials_oauth()? {
            if !keyring_available {
                return Err(AppError::KeyringUnavailable(
                    "cannot save subscription without keyring".into(),
                ));
            }
            return Ok((
                ProviderKind::Subscription,
                Some(ProviderSecret::Subscription { oauth }),
            ));
        }
    }

    Err(AppError::Validation(
        "settings.json env doesn't describe a known provider kind, and no OAuth session was found"
            .into(),
    ))
}

fn is_canonical_marker(k: &str) -> bool {
    matches!(
        k,
        "ANTHROPIC_BASE_URL"
            | "ANTHROPIC_AUTH_TOKEN"
            | "ANTHROPIC_API_KEY"
            | "CLAUDE_CODE_USE_BEDROCK"
            | "CLAUDE_CODE_USE_VERTEX"
    )
}

fn auto_name(kind: &ProviderKind, env_obj: &Map<String, Value>) -> String {
    match kind {
        ProviderKind::Custom => env_obj
            .get("ANTHROPIC_BASE_URL")
            .and_then(|v| v.as_str())
            .map(derive_provider_name)
            .unwrap_or_else(|| "Custom".to_string()),
        ProviderKind::Console => "Anthropic Console".into(),
        ProviderKind::Subscription => "Subscription".into(),
        ProviderKind::Bedrock => "Amazon Bedrock".into(),
        ProviderKind::Vertex => "Google Vertex AI".into(),
    }
}

/// Compare two env blocks for equality ignoring key order. Used by
/// `get_active_provider` fallback to match the live settings.json env to a
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

fn read_state(path: &Path) -> AppResult<StateFile> {
    if !path.exists() {
        return Ok(StateFile::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(StateFile::default());
    }
    let file: StateFile = serde_json::from_slice(&bytes).map_err(|e| AppError::MalformedSettings {
        path: path.display().to_string(),
        message: format!("state.json: {e}"),
    })?;
    Ok(file)
}

pub(crate) fn write_state(path: &Path, s: &StateFile) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(s)?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Validation("state path has no parent".into()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
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

    #[test]
    fn detect_kind_prefers_bedrock_when_marker_present() {
        let env = json!({
            "CLAUDE_CODE_USE_BEDROCK": "1",
            "AWS_REGION": "us-east-1",
            "AWS_ACCESS_KEY_ID": "AKIA",
            "AWS_SECRET_ACCESS_KEY": "sec"
        });
        let (kind, secret) =
            detect_current_kind(env.as_object().unwrap(), true).unwrap();
        assert_eq!(kind, ProviderKind::Bedrock);
        match secret.unwrap() {
            ProviderSecret::Bedrock {
                access_key_id, ..
            } => assert_eq!(access_key_id, "AKIA"),
            other => panic!("expected Bedrock secret, got {other:?}"),
        }
    }

    #[test]
    fn detect_kind_prefers_vertex_when_marker_present() {
        let env = json!({
            "CLAUDE_CODE_USE_VERTEX": "1",
            "ANTHROPIC_VERTEX_PROJECT_ID": "p",
            "CLOUD_ML_REGION": "us-central1"
        });
        let (kind, secret) =
            detect_current_kind(env.as_object().unwrap(), true).unwrap();
        assert_eq!(kind, ProviderKind::Vertex);
        assert!(matches!(secret.unwrap(), ProviderSecret::Vertex {}));
    }

    #[test]
    fn detect_kind_picks_console_when_only_api_key() {
        let env = json!({"ANTHROPIC_API_KEY": "sk-ant-abc"});
        let (kind, secret) =
            detect_current_kind(env.as_object().unwrap(), true).unwrap();
        assert_eq!(kind, ProviderKind::Console);
        assert!(matches!(secret.unwrap(), ProviderSecret::Console { .. }));
    }

    #[test]
    fn detect_kind_picks_custom_when_base_and_token() {
        let env = json!({
            "ANTHROPIC_BASE_URL": "https://api.custom",
            "ANTHROPIC_AUTH_TOKEN": "tok"
        });
        let (kind, secret) =
            detect_current_kind(env.as_object().unwrap(), true).unwrap();
        assert_eq!(kind, ProviderKind::Custom);
        assert!(matches!(secret.unwrap(), ProviderSecret::Custom { .. }));
    }

    #[test]
    fn detect_kind_errors_on_ambiguous_env() {
        let env = json!({"ANTHROPIC_BASE_URL": "https://x"}); // no token, no marker
        let err = detect_current_kind(env.as_object().unwrap(), true).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
