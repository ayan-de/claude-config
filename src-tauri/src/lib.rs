use std::sync::Arc;

use chrono::Utc;
use tauri::Manager;
use uuid::Uuid;

mod commands;
mod merge;
mod models;
mod state;
mod storage;

use models::ProvidersFile;
use state::AppState;
use storage::KeyringStore;
use merge::derive_provider_name;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("could not resolve app data dir: {e}"))?;
            std::fs::create_dir_all(&app_data_dir)?;
            std::fs::create_dir_all(app_data_dir.join("backups"))?;

            let keyring = KeyringStore::new();
            if !keyring.is_available() {
                log::warn!(
                    "OS keyring unavailable; auth tokens cannot be persisted: {:?}",
                    keyring.status()
                );
            }

            let state = AppState {
                keyring,
                app_data_dir: Arc::new(app_data_dir),
            };
            app.manage(state);

            // First-launch auto-import: if providers.json doesn't exist and
            // settings.json has a usable env block, snapshot it as an
            // "Imported" provider so the user isn't staring at an empty app.
            if let Err(e) = first_launch_import(&app) {
                log::warn!("first-launch import skipped: {e}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::discover_claude_dir_cmd,
            commands::system::get_app_data_dir_cmd,
            commands::system::reveal_in_file_manager_cmd,
            commands::system::keyring_status_cmd,
            commands::providers::list_providers_cmd,
            commands::providers::get_provider_cmd,
            commands::providers::add_provider_cmd,
            commands::providers::update_provider_cmd,
            commands::providers::delete_provider_cmd,
            commands::providers::validate_provider_cmd,
            commands::settings::get_active_provider_cmd,
            commands::settings::load_provider_cmd,
            commands::settings::save_current_as_provider_cmd,
            commands::settings::preview_provider_env_cmd,
            commands::settings::get_settings_env_keys_cmd,
            commands::transfer::export_providers_cmd,
            commands::transfer::import_providers_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn first_launch_import(app: &tauri::App) -> Result<(), String> {
    let state = app.state::<AppState>();
    let providers_path = state.providers_path();
    if providers_path.exists() {
        return Ok(());
    }

    let settings = storage::read_settings(&storage::settings_path())
        .map_err(|e| e.to_string())?;
    let Some(value) = settings else { return Ok(()) };
    let Some(env_obj) = value.get("env").and_then(|v| v.as_object()) else {
        return Ok(());
    };
    let Some(base_url) = env_obj.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    let Some(token) = env_obj.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    if base_url.is_empty() || token.is_empty() {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let provider = models::Provider {
        id: id.clone(),
        name: derive_provider_name(base_url),
        base_url: base_url.into(),
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

    if state.keyring.is_available() {
        state
            .keyring
            .set_token(&id, token)
            .map_err(|e| e.to_string())?;
    }

    let file = ProvidersFile {
        schema_version: 1,
        providers: vec![provider],
    };
    storage::save_providers_file(&providers_path, &file).map_err(|e| e.to_string())?;
    log::info!("first-launch auto-import created 'Imported' provider");
    Ok(())
}

fn string_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}