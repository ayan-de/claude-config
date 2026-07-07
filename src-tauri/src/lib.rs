use std::sync::Arc;

use chrono::Utc;
use tauri::Manager;
use uuid::Uuid;

mod commands;
mod merge;
mod models;
mod state;
mod storage;

use commands::settings::write_state;
use merge::derive_provider_name;
use models::{Provider, ProviderKind, ProviderSecret, ProvidersFile, StateFile};
use state::AppState;
use storage::KeyringStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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

            let app_state = AppState {
                keyring,
                app_data_dir: Arc::new(app_data_dir),
            };
            app.manage(app_state);

            // First-launch auto-import: if providers.json doesn't exist yet,
            // capture any config already on disk (Custom env vars in
            // settings.json, and/or Subscription OAuth in .credentials.json)
            // so the user isn't staring at an empty app.
            if let Err(e) = first_launch_import(app) {
                log::warn!("first-launch import skipped: {e}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::discover_claude_dir_cmd,
            commands::system::get_app_data_dir_cmd,
            commands::system::reveal_in_file_manager_cmd,
            commands::system::keyring_status_cmd,
            commands::system::read_claude_md_cmd,
            commands::system::write_claude_md_cmd,
            commands::system::claude_md_exists_cmd,
            commands::system::list_marketplaces_cmd,
            commands::system::list_skills_cmd,
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
            commands::settings::get_dangerous_mode_cmd,
            commands::settings::set_dangerous_mode_cmd,
            commands::subscription::import_current_subscription_cmd,
            commands::transfer::export_providers_cmd,
            commands::transfer::import_providers_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn first_launch_import(app: &tauri::App) -> Result<(), String> {
    let app_state = app.state::<AppState>();
    let providers_path = app_state.providers_path();
    if providers_path.exists() {
        return Ok(());
    }

    let mut file = ProvidersFile::default();
    let now = Utc::now().to_rfc3339();

    // Track which provider (if any) currently corresponds to the *active*
    // env in settings.json. When both a Custom and a Subscription exist,
    // Custom wins because env vars override OAuth.
    let mut active_id: Option<String> = None;

    // (a) Custom-relay import from settings.json.env — today's behaviour.
    if let Some(custom) = try_import_custom(&app_state, &now) {
        active_id = Some(custom.id.clone());
        file.providers.push(custom);
    }

    // (b) Subscription import from .credentials.json. Independent of (a) —
    // both can coexist; active pointer stays on Custom if we already saw one.
    if let Some(sub) = try_import_subscription(&app_state, &now) {
        if active_id.is_none() {
            active_id = Some(sub.id.clone());
        }
        file.providers.push(sub);
    }

    if file.providers.is_empty() {
        return Ok(());
    }

    storage::save_providers_file(&providers_path, &file).map_err(|e| e.to_string())?;

    if let Some(id) = active_id {
        let _ = write_state(
            &app_state.state_path(),
            &StateFile {
                active_provider_id: Some(id),
            },
        );
    }

    log::info!(
        "first-launch auto-import created {} provider(s)",
        file.providers.len()
    );
    Ok(())
}

fn try_import_custom(app_state: &AppState, now: &str) -> Option<Provider> {
    let settings = storage::read_settings(&storage::settings_path()).ok().flatten()?;
    let env_obj = settings.get("env").and_then(|v| v.as_object())?;
    let base_url = env_obj
        .get("ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let token = env_obj
        .get("ANTHROPIC_AUTH_TOKEN")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;

    let id = Uuid::new_v4().to_string();
    let provider = Provider {
        id: id.clone(),
        name: derive_provider_name(base_url),
        kind: ProviderKind::Custom,
        base_url: Some(base_url.to_string()),
        aws_region: None,
        aws_profile: None,
        vertex_project_id: None,
        vertex_region: None,
        google_application_credentials: None,
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
        logo_svg: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    };

    if app_state.keyring.is_available() {
        if let Err(e) = app_state.keyring.set_secret(
            &id,
            &ProviderSecret::Custom {
                auth_token: token.to_string(),
            },
        ) {
            log::warn!("could not stash Custom import token: {e}");
        }
    }

    Some(provider)
}

fn try_import_subscription(app_state: &AppState, now: &str) -> Option<Provider> {
    let oauth = storage::read_credentials_oauth().ok().flatten()?;

    let id = Uuid::new_v4().to_string();
    let name = oauth
        .get("email")
        .and_then(|v| v.as_str())
        .or_else(|| {
            oauth
                .get("account")
                .and_then(|a| a.get("email"))
                .and_then(|v| v.as_str())
        })
        .map(|e| format!("Subscription ({e})"))
        .unwrap_or_else(|| "Subscription".to_string());

    let provider = Provider {
        id: id.clone(),
        name,
        kind: ProviderKind::Subscription,
        base_url: None,
        aws_region: None,
        aws_profile: None,
        vertex_project_id: None,
        vertex_region: None,
        google_application_credentials: None,
        subscription_label: None,
        model: None,
        small_fast_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        default_haiku_model: None,
        api_timeout_ms: None,
        disable_nonessential_traffic: None,
        logo_svg: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    };

    if app_state.keyring.is_available() {
        if let Err(e) = app_state
            .keyring
            .set_secret(&id, &ProviderSecret::Subscription { oauth })
        {
            log::warn!("could not stash Subscription import OAuth: {e}");
            return None;
        }
    } else {
        // Without a keyring we can't safely preserve the OAuth blob across
        // restarts — skip the import rather than lose it silently.
        return None;
    }

    Some(provider)
}

fn string_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}
