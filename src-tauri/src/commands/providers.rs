//! Provider CRUD commands. Secrets (OAuth blobs, API keys, tokens, AWS
//! creds) go to the OS keyring as `ProviderSecret` JSON blobs; metadata
//! (everything else, including the `kind` discriminator) lives in
//! `providers.json`.

use chrono::Utc;
use uuid::Uuid;

use crate::models::{
    AppError, AppResult, Provider, ProviderInput, ProviderKind, ProviderSecret,
};
use crate::state::AppState;
use crate::storage::{load_providers_file, save_providers_file};

#[tauri::command]
pub fn list_providers_cmd(state: tauri::State<'_, AppState>) -> AppResult<Vec<Provider>> {
    let file = load_providers_file(&state.providers_path())?;
    Ok(file.providers)
}

#[tauri::command]
pub fn get_provider_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> AppResult<Provider> {
    let file = load_providers_file(&state.providers_path())?;
    file.providers
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(id))
}

#[tauri::command]
pub fn add_provider_cmd(
    state: tauri::State<'_, AppState>,
    input: ProviderInput,
) -> AppResult<Provider> {
    validate_input(&input, /* creating */ true)?;
    let secret = build_secret(&input, /* creating */ true)?;

    let mut file = load_providers_file(&state.providers_path())?;
    if file.providers.iter().any(|p| p.name == input.name) {
        return Err(AppError::DuplicateName(input.name));
    }

    let now = Utc::now().to_rfc3339();
    let provider = provider_from_input(&input, Uuid::new_v4().to_string(), now.clone(), now);

    if let Some(secret) = secret {
        state.keyring.set_secret(&provider.id, &secret)?;
    }
    file.providers.push(provider.clone());
    save_providers_file(&state.providers_path(), &file)?;
    Ok(provider)
}

#[tauri::command]
pub fn update_provider_cmd(
    state: tauri::State<'_, AppState>,
    input: ProviderInput,
) -> AppResult<Provider> {
    validate_input(&input, /* creating */ false)?;
    let id = input
        .id
        .clone()
        .ok_or_else(|| AppError::Validation("update requires provider id".into()))?;

    let mut file = load_providers_file(&state.providers_path())?;
    let pos = file
        .providers
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(id.clone()))?;

    // Unique-name check (excluding self)
    if file
        .providers
        .iter()
        .any(|p| p.name == input.name && p.id != id)
    {
        return Err(AppError::DuplicateName(input.name));
    }

    // Changing kind on an existing provider is not supported in v1 — the
    // secret schema would need to be entirely different. Reject cleanly.
    if file.providers[pos].kind != input.kind {
        return Err(AppError::Validation(format!(
            "cannot change provider kind after creation (was {:?}, now {:?}); \
             delete and recreate instead",
            file.providers[pos].kind, input.kind
        )));
    }

    let now = Utc::now().to_rfc3339();
    let created_at = file.providers[pos].created_at.clone();
    let updated = provider_from_input(&input, id.clone(), created_at, now);

    // Only rotate the secret when the input included fresh secret material for
    // this kind. Otherwise leave the keyring entry alone (e.g. editing the
    // model overrides shouldn't require re-entering the token).
    if let Some(secret) = build_secret(&input, /* creating */ false)? {
        state.keyring.set_secret(&id, &secret)?;
    }
    file.providers[pos] = updated.clone();
    save_providers_file(&state.providers_path(), &file)?;
    Ok(updated)
}

#[tauri::command]
pub fn delete_provider_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let mut file = load_providers_file(&state.providers_path())?;
    let before = file.providers.len();
    file.providers.retain(|p| p.id != id);
    if file.providers.len() == before {
        return Err(AppError::NotFound(id));
    }
    save_providers_file(&state.providers_path(), &file)?;
    // Best-effort keyring delete; if it's already gone, that's fine.
    if let Err(e) = state.keyring.delete_token(&id) {
        log::warn!("keyring delete for {id} failed: {e}");
    }
    // Clean up any tracker config + tracker keyring entries. Best-effort
    // — a stale row doesn't break anything, but we shouldn't leave it
    // around to confuse the user if they re-create the provider.
    let trackers_path = state.trackers_path();
    if let Ok(mut tfile) = crate::storage::load_trackers_file(&trackers_path) {
        if let Some(cfg) = tfile.trackers.remove(&id) {
            if let Ok(source) = cfg.source_id() {
                if let Some(src) = crate::tracker::SourceRegistry::new().get(source) {
                    let secret_keys: Vec<&str> = src
                        .fields()
                        .iter()
                        .filter(|f| f.secret)
                        .map(|f| f.key)
                        .collect();
                    if let Err(e) = state.keyring.delete_tracker_secrets(&id, &secret_keys) {
                        log::warn!("tracker keyring cleanup for {id} failed: {e}");
                    }
                }
            }
            if let Err(e) = crate::storage::save_trackers_file(&trackers_path, &tfile) {
                log::warn!("tracker file cleanup for {id} failed: {e}");
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn validate_provider_cmd(input: ProviderInput) -> AppResult<()> {
    // Treat an input without an id as a create — secret material required.
    let creating = input.id.is_none();
    validate_input(&input, creating)
}

fn validate_input(input: &ProviderInput, creating: bool) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if let Some(ms) = input.api_timeout_ms {
        if ms == 0 {
            return Err(AppError::Validation(
                "api_timeout_ms must be greater than 0".into(),
            ));
        }
    }
    if let Some(svg) = input.logo_svg.as_deref() {
        // SVGs are stored inline in providers.json. Cap size to keep the file
        // small — the renderer already rejects oversized uploads at 50 KB,
        // but re-validate here in case a payload comes from elsewhere.
        if svg.len() > 50 * 1024 {
            return Err(AppError::Validation(
                "logo_svg exceeds 50 KB limit".into(),
            ));
        }
    }

    match input.kind {
        ProviderKind::Subscription => {
            // OAuth secret arrives via import_current_subscription_cmd, not
            // via this endpoint. Nothing kind-specific to require here.
        }
        ProviderKind::Console => {
            if creating && is_blank(&input.api_key) {
                return Err(AppError::Validation("api_key is required for Console".into()));
            }
        }
        ProviderKind::Custom => {
            let url = input
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    AppError::Validation("base_url is required for Custom".into())
                })?;
            let parsed = url::Url::parse(url).map_err(|e| {
                AppError::Validation(format!("base_url is not a valid URL: {e}"))
            })?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err(AppError::Validation(
                    "base_url must use http or https".into(),
                ));
            }
            if creating && is_blank(&input.auth_token) {
                return Err(AppError::Validation(
                    "auth_token is required for Custom".into(),
                ));
            }
        }
        ProviderKind::Bedrock => {
            if is_blank(&input.aws_region) {
                return Err(AppError::Validation(
                    "aws_region is required for Bedrock".into(),
                ));
            }
            let has_profile = !is_blank(&input.aws_profile);
            let has_static =
                !is_blank(&input.aws_access_key_id) && !is_blank(&input.aws_secret_access_key);
            if creating && !has_profile && !has_static {
                return Err(AppError::Validation(
                    "Bedrock requires either aws_profile or (aws_access_key_id + aws_secret_access_key)".into(),
                ));
            }
        }
        ProviderKind::Vertex => {
            if is_blank(&input.vertex_project_id) {
                return Err(AppError::Validation(
                    "vertex_project_id is required for Vertex".into(),
                ));
            }
            if is_blank(&input.vertex_region) {
                return Err(AppError::Validation(
                    "vertex_region is required for Vertex".into(),
                ));
            }
        }
    }

    Ok(())
}

/// Build a `ProviderSecret` from input. Returns `Ok(None)` when the input
/// doesn't include fresh secret material — used by `update_provider_cmd` to
/// distinguish "leave existing keyring entry alone" from "rotate secret".
///
/// For `creating == true`, callers rely on `validate_input` having already
/// rejected blank secrets — so a `None` return here means the kind
/// genuinely has no secret (Subscription, Vertex).
fn build_secret(input: &ProviderInput, creating: bool) -> AppResult<Option<ProviderSecret>> {
    match input.kind {
        ProviderKind::Subscription => {
            // OAuth blob is imported via a separate command, not fields on the form.
            Ok(None)
        }
        ProviderKind::Console => {
            if is_blank(&input.api_key) {
                if creating {
                    // Should have been caught by validate; belt-and-braces:
                    return Err(AppError::Validation("api_key is required".into()));
                }
                Ok(None)
            } else {
                Ok(Some(ProviderSecret::Console {
                    api_key: input.api_key.clone().unwrap().trim().to_string(),
                }))
            }
        }
        ProviderKind::Custom => {
            if is_blank(&input.auth_token) {
                if creating {
                    return Err(AppError::Validation("auth_token is required".into()));
                }
                Ok(None)
            } else {
                Ok(Some(ProviderSecret::Custom {
                    auth_token: input.auth_token.clone().unwrap().trim().to_string(),
                }))
            }
        }
        ProviderKind::Bedrock => {
            let has_profile = !is_blank(&input.aws_profile);
            let has_static = !is_blank(&input.aws_access_key_id)
                && !is_blank(&input.aws_secret_access_key);
            if has_static {
                Ok(Some(ProviderSecret::Bedrock {
                    access_key_id: input.aws_access_key_id.clone().unwrap().trim().to_string(),
                    secret_access_key: input
                        .aws_secret_access_key
                        .clone()
                        .unwrap()
                        .trim()
                        .to_string(),
                    session_token: input
                        .aws_session_token
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                }))
            } else if has_profile {
                // Profile-based Bedrock has no secret to store in the keyring
                // (creds come from ~/.aws/credentials). Persist an empty
                // Bedrock variant so lookups stay consistent per-provider.
                Ok(Some(ProviderSecret::Bedrock {
                    access_key_id: String::new(),
                    secret_access_key: String::new(),
                    session_token: None,
                }))
            } else if creating {
                Err(AppError::Validation("Bedrock requires credentials".into()))
            } else {
                Ok(None)
            }
        }
        ProviderKind::Vertex => Ok(Some(ProviderSecret::Vertex {})),
    }
}

fn provider_from_input(
    input: &ProviderInput,
    id: String,
    created_at: String,
    updated_at: String,
) -> Provider {
    Provider {
        id,
        name: input.name.clone(),
        kind: input.kind,
        base_url: non_empty(input.base_url.clone()),
        aws_region: non_empty(input.aws_region.clone()),
        aws_profile: non_empty(input.aws_profile.clone()),
        vertex_project_id: non_empty(input.vertex_project_id.clone()),
        vertex_region: non_empty(input.vertex_region.clone()),
        google_application_credentials: non_empty(input.google_application_credentials.clone()),
        subscription_label: non_empty(input.subscription_label.clone()),
        model: non_empty(input.model.clone()),
        small_fast_model: non_empty(input.small_fast_model.clone()),
        default_sonnet_model: non_empty(input.default_sonnet_model.clone()),
        default_opus_model: non_empty(input.default_opus_model.clone()),
        default_haiku_model: non_empty(input.default_haiku_model.clone()),
        api_timeout_ms: input.api_timeout_ms,
        disable_nonessential_traffic: input.disable_nonessential_traffic,
        logo_svg: non_empty(input.logo_svg.clone()),
        created_at,
        updated_at,
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.and_then(|v| if v.trim().is_empty() { None } else { Some(v) })
}

fn is_blank(s: &Option<String>) -> bool {
    s.as_deref().map(str::trim).unwrap_or("").is_empty()
}
