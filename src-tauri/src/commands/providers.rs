//! Provider CRUD commands. Auth tokens are written to the OS keyring;
//! metadata (everything else) lives in `providers.json`.

use chrono::Utc;
use uuid::Uuid;

use crate::models::{AppError, AppResult, Provider, ProviderInput};
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
    validate_input(&input, /* require_token */ true)?;
    let token = input
        .auth_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::Validation("auth_token is required".into()))?
        .to_string();

    let mut file = load_providers_file(&state.providers_path())?;
    if file.providers.iter().any(|p| p.name == input.name) {
        return Err(AppError::DuplicateName(input.name));
    }

    let now = Utc::now().to_rfc3339();
    let provider = Provider {
        id: Uuid::new_v4().to_string(),
        name: input.name,
        base_url: input.base_url,
        model: non_empty(input.model),
        small_fast_model: non_empty(input.small_fast_model),
        default_sonnet_model: non_empty(input.default_sonnet_model),
        default_opus_model: non_empty(input.default_opus_model),
        default_haiku_model: non_empty(input.default_haiku_model),
        api_timeout_ms: input.api_timeout_ms,
        disable_nonessential_traffic: input.disable_nonessential_traffic,
        created_at: now.clone(),
        updated_at: now,
    };

    state.keyring.set_token(&provider.id, &token)?;
    file.providers.push(provider.clone());
    save_providers_file(&state.providers_path(), &file)?;
    Ok(provider)
}

#[tauri::command]
pub fn update_provider_cmd(
    state: tauri::State<'_, AppState>,
    input: ProviderInput,
) -> AppResult<Provider> {
    validate_input(&input, /* require_token */ false)?;
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

    let now = Utc::now().to_rfc3339();
    let updated = Provider {
        id: id.clone(),
        name: input.name,
        base_url: input.base_url,
        model: non_empty(input.model),
        small_fast_model: non_empty(input.small_fast_model),
        default_sonnet_model: non_empty(input.default_sonnet_model),
        default_opus_model: non_empty(input.default_opus_model),
        default_haiku_model: non_empty(input.default_haiku_model),
        api_timeout_ms: input.api_timeout_ms,
        disable_nonessential_traffic: input.disable_nonessential_traffic,
        created_at: file.providers[pos].created_at.clone(),
        updated_at: now,
    };

    // Only rotate the token when a non-empty one was supplied; otherwise
    // leave the existing keyring entry alone.
    if let Some(token) = input
        .auth_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        state.keyring.set_token(&id, token)?;
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
    Ok(())
}

#[tauri::command]
pub fn validate_provider_cmd(input: ProviderInput) -> AppResult<()> {
    // Treat an input without an id as a create — token required.
    let require_token = input.id.is_none();
    validate_input(&input, require_token)
}

fn validate_input(input: &ProviderInput, require_token: bool) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if input.base_url.trim().is_empty() {
        return Err(AppError::Validation("base_url is required".into()));
    }
    let parsed = url::Url::parse(&input.base_url).map_err(|e| {
        AppError::Validation(format!("base_url is not a valid URL: {e}"))
    })?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::Validation(
            "base_url must use http or https".into(),
        ));
    }
    if require_token
        && input
            .auth_token
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(AppError::Validation("auth_token is required".into()));
    }
    if let Some(ms) = input.api_timeout_ms {
        if ms == 0 {
            return Err(AppError::Validation(
                "api_timeout_ms must be greater than 0".into(),
            ));
        }
    }
    Ok(())
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.and_then(|v| if v.trim().is_empty() { None } else { Some(v) })
}