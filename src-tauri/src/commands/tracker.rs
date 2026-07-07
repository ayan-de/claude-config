//! Tracker commands — per-provider usage tracking.
//!
//! ## Flow
//!
//! 1. UI calls `list_tracker_sources_cmd` once at mount → renders the
//!    source picker + form from the returned field schema.
//! 2. User fills the form, clicks Save → `save_tracker_config_cmd`
//!    validates, splits secrets into keyring, persists the rest to
//!    `trackers.json`.
//! 3. UI auto-polls every 60s (or manual button) → `refresh_tracker_cmd`
//!    reads the config from disk + keyring, asks the adapter to fetch
//!    a usage snapshot, writes it back to `trackers.json`.
//! 4. UI calls `get_tracker_config_cmd` to read the cached snapshot
//!    without paying the network cost.
//!
//! ## Secrets
//!
//! Fields marked `secret: true` in the source descriptor are written to
//! the OS keyring under `{provider_id}::{field_key}`. They never appear
//! in `trackers.json`. On refresh we re-inject them into the adapter's
//! config before calling `fetch_usage`.
//!
//! ## Concurrency
//!
//! `refresh_tracker_cmd` is a per-provider call. There's no global lock
//! — concurrent refreshes of the same provider are coalesced at the
//! UI level (the second one overwrites the first). The file write is
//! atomic (tempfile + rename), so the worst case is a wasted network
//! call.

use std::sync::OnceLock;

use chrono::Utc;
use reqwest::blocking::Client;

use crate::models::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::{
    load_trackers_file, save_trackers_file, TrackerConfig,
};
use crate::tracker::{SourceDescriptor, SourceId, SourceRegistry, TrackerField, TrackerUsage};

/// One reqwest client for the lifetime of the app. Connections are pooled
/// across refreshes, so per-call setup is cheap.
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(concat!(
                "claude-config/",
                env!("CARGO_PKG_VERSION"),
                " (tracker)"
            ))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("reqwest client build should not fail with default config")
    })
}

fn registry() -> &'static SourceRegistry {
    static R: OnceLock<SourceRegistry> = OnceLock::new();
    R.get_or_init(SourceRegistry::new)
}

/// Lists all registered tracker sources. The UI calls this once at mount
/// to render the source picker and the form schema. No provider-specific
/// state is needed.
#[tauri::command]
pub fn list_tracker_sources_cmd() -> Vec<SourceDescriptor> {
    registry().list()
}

/// Reads the saved config + cached usage for a provider. The returned
/// `config.fields` blob excludes secrets — the UI gets `null` for any
/// field that was stored in the keyring, plus a `has_secret: [keys...]`
/// array so the form can show a "Stored" placeholder.
#[tauri::command]
pub fn get_tracker_config_cmd(
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> AppResult<TrackerConfigView> {
    let path = state.trackers_path();
    let file = load_trackers_file(&path)?;
    let Some(mut cfg) = file.trackers.get(&provider_id).cloned() else {
        return Err(AppError::NotFound(format!(
            "no tracker config for provider {provider_id}"
        )));
    };
    // Validate the source id is known — if the user's trackers.json
    // references a source that no longer exists, we surface the error
    // rather than silently returning a row the UI can't render.
    let source = registry()
        .get(cfg.source_id()?)
        .ok_or_else(|| AppError::Validation(format!("unknown source: {}", cfg.source)))?;
    let secret_keys: Vec<&'static str> = source
        .fields()
        .iter()
        .filter(|f| f.secret)
        .map(|f| f.key)
        .collect();
    // Strip the secret fields from the returned blob so the UI doesn't
    // accidentally render a stale copy. The keyring is the source of truth.
    for k in &secret_keys {
        cfg.fields.remove(*k);
    }
    Ok(TrackerConfigView {
        source: cfg.source,
        fields: cfg.fields,
        last_usage: cfg.last_usage,
        last_fetched_at: cfg.last_fetched_at,
        last_error: cfg.last_error,
        updated_at: cfg.updated_at,
        has_secret: secret_keys.into_iter().map(String::from).collect(),
    })
}

/// What `get_tracker_config_cmd` returns to the UI. The `fields` blob
/// has secrets stripped; `has_secret` tells the UI which keys have a
/// stored value in the keyring so the form can show "Stored" instead
/// of a placeholder.
#[derive(Debug, serde::Serialize)]
pub struct TrackerConfigView {
    pub source: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub last_usage: Option<TrackerUsage>,
    pub last_fetched_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
    /// Field keys that have a value in the keyring.
    pub has_secret: Vec<String>,
}

/// Saves the tracker config for a provider. The input has all fields
/// (including secrets) flat — we split them here: `secret: true` fields
/// go to the keyring, the rest go to `trackers.json`.
#[tauri::command]
pub fn save_tracker_config_cmd(
    state: tauri::State<'_, AppState>,
    provider_id: String,
    source: String,
    fields: serde_json::Map<String, serde_json::Value>,
) -> AppResult<TrackerConfigView> {
    let source_id = SourceId::parse(&source)?;
    let src = registry()
        .get(source_id)
        .ok_or_else(|| AppError::Validation(format!("unknown source: {source}")))?;

    // Split secrets. We need the original value from the input (which
    // may be empty for a field that's already in the keyring — the UI
    // sends empty when the user didn't change the field, and we leave
    // the existing keyring entry alone).
    let field_schema: Vec<TrackerField> = src.fields();
    let mut persist_blob: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut secret_keys: Vec<&'static str> = Vec::new();
    for f in &field_schema {
        let incoming = fields.get(f.key).cloned().unwrap_or(serde_json::Value::Null);
        let incoming_str = incoming.as_str().unwrap_or("");
        if f.secret {
            secret_keys.push(f.key);
            if !incoming_str.is_empty() {
                state
                    .keyring
                    .set_tracker_secret(&provider_id, f.key, incoming_str)?;
            }
            // Don't write the secret to the JSON blob even if the user
            // typed one — the keyring is the source of truth.
        } else {
            persist_blob.insert(f.key.to_string(), incoming);
        }
    }

    // Validate using only the non-secret blob + keyring lookups. This
    // mirrors what `refresh_tracker_cmd` will assemble before calling
    // the adapter.
    let mut full_blob = persist_blob.clone();
    for k in &secret_keys {
        if let Some(v) = state.keyring.get_tracker_secret(&provider_id, k)? {
            full_blob.insert((*k).to_string(), serde_json::Value::String(v));
        }
    }
    src.validate_config(&full_blob)?;

    // Read-modify-write trackers.json.
    let path = state.trackers_path();
    let mut file = load_trackers_file(&path)?;
    let now = Utc::now().to_rfc3339();
    let entry = file.trackers.entry(provider_id.clone()).or_insert_with(|| {
        // First save — no cached usage yet. Use the existing struct so
        // we can keep the previous snapshot if we're updating an existing
        // entry.
        TrackerConfig {
            source: source.clone(),
            fields: serde_json::Map::new(),
            last_usage: None,
            last_fetched_at: None,
            last_error: None,
            updated_at: now.clone(),
        }
    });
    entry.source = source.clone();
    entry.fields = persist_blob.clone();
    entry.updated_at = now.clone();
    // If we have a prior cached usage, keep it — saving config shouldn't
    // wipe the snapshot unless explicitly cleared.
    save_trackers_file(&path, &file)?;

    let cfg = file.trackers.get(&provider_id).cloned().unwrap();
    let mut view = TrackerConfigView {
        source: cfg.source,
        fields: cfg.fields,
        last_usage: cfg.last_usage,
        last_fetched_at: cfg.last_fetched_at,
        last_error: cfg.last_error,
        updated_at: cfg.updated_at,
        has_secret: secret_keys.into_iter().map(String::from).collect(),
    };
    // Strip secrets from the returned view, same as the read path.
    for k in std::mem::take(&mut view.has_secret) {
        view.fields.remove(&k);
    }
    Ok(view)
}

/// Deletes the tracker config + all keyring secrets for a provider.
/// Idempotent — succeeds even when no config exists.
#[tauri::command]
pub fn delete_tracker_config_cmd(
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> AppResult<()> {
    let path = state.trackers_path();
    let mut file = load_trackers_file(&path)?;
    // Capture the source id BEFORE removing so we can clean up secrets
    // using the source's field schema.
    let source_keys: Vec<String> = file
        .trackers
        .get(&provider_id)
        .and_then(|c| SourceId::parse(&c.source).ok())
        .and_then(|id| registry().get(id))
        .map(|src| src.fields().iter().filter(|f| f.secret).map(|f| f.key.to_string()).collect())
        .unwrap_or_default();
    file.trackers.remove(&provider_id);
    save_trackers_file(&path, &file)?;
    let key_refs: Vec<&str> = source_keys.iter().map(String::as_str).collect();
    state.keyring.delete_tracker_secrets(&provider_id, &key_refs)?;
    Ok(())
}

/// Triggers a refresh. Returns the new usage snapshot. On failure the
/// last_error is updated in `trackers.json` and the error is also
/// returned to the caller so the UI can show a toast.
#[tauri::command]
pub fn refresh_tracker_cmd(
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> AppResult<TrackerUsage> {
    let path = state.trackers_path();
    let mut file = load_trackers_file(&path)?;
    let cfg = file
        .trackers
        .get(&provider_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("no tracker config for provider {provider_id}")))?;
    let source = registry()
        .get(cfg.source_id()?)
        .ok_or_else(|| AppError::Validation(format!("unknown source: {}", cfg.source)))?;

    // Reassemble the full config blob (secrets injected from keyring).
    let mut full_blob = cfg.fields.clone();
    for f in source.fields() {
        if f.secret {
            if let Some(v) = state.keyring.get_tracker_secret(&provider_id, f.key)? {
                full_blob.insert(f.key.to_string(), serde_json::Value::String(v));
            }
        }
    }

    let now = Utc::now().to_rfc3339();
    let entry = file.trackers.get_mut(&provider_id).unwrap();
    entry.last_fetched_at = Some(now.clone());

    let result = source.fetch_usage(&full_blob, http_client());
    match &result {
        Ok(usage) => {
            entry.last_usage = Some(usage.clone());
            entry.last_error = None;
        }
        Err(e) => {
            entry.last_error = Some(e.to_string());
        }
    }
    save_trackers_file(&path, &file)?;
    result
}

/// Returns the cached usage without re-fetching. Used by the UI on mount
/// to populate immediately, before the first auto-refresh fires.
#[tauri::command]
pub fn get_tracker_usage_cmd(
    state: tauri::State<'_, AppState>,
    provider_id: String,
) -> AppResult<Option<TrackerUsage>> {
    let path = state.trackers_path();
    let file = load_trackers_file(&path)?;
    Ok(file
        .trackers
        .get(&provider_id)
        .and_then(|c| c.last_usage.clone()))
}
