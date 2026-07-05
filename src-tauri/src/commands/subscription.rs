//! Subscription-specific commands: capture the current `claude /login`
//! session as a saved provider so it can be swapped back in later without
//! re-running the OAuth flow.

use chrono::Utc;
use uuid::Uuid;

use crate::models::{
    AppError, AppResult, Provider, ProviderKind, ProviderSecret,
};
use crate::state::AppState;
use crate::storage::{load_providers_file, read_credentials_oauth, save_providers_file};

/// Read the current `claudeAiOauth` blob from `~/.claude/.credentials.json`
/// and save it as a Subscription provider.
///
/// - `label` is an optional user-supplied string ("Work Max", "Personal Pro")
///   used to disambiguate multiple subscription profiles. When `None`, we
///   attempt to pull an email from the OAuth blob; otherwise we fall back to
///   a generic name.
/// - Fails with `Validation` if there's no OAuth session to import.
/// - Fails with `KeyringUnavailable` if we can't stash the OAuth blob.
#[tauri::command]
pub fn import_current_subscription_cmd(
    state: tauri::State<'_, AppState>,
    label: Option<String>,
) -> AppResult<Provider> {
    let oauth = read_credentials_oauth()?.ok_or_else(|| {
        AppError::Validation(
            "no OAuth session found in ~/.claude/.credentials.json — \
             run `claude /login` in a terminal first"
                .into(),
        )
    })?;

    if !state.keyring.is_available() {
        return Err(AppError::KeyringUnavailable(
            "cannot save subscription without keyring".into(),
        ));
    }

    let display_name = derive_subscription_name(label.as_deref(), &oauth);
    let subscription_label = label.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });

    let mut file = load_providers_file(&state.providers_path())?;
    // Uniqueness by name — bump with a counter if a subscription with this
    // name already exists (e.g. re-import of the same email).
    let name = unique_name(&file.providers, &display_name);

    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
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
        subscription_label,
        model: None,
        small_fast_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        default_haiku_model: None,
        api_timeout_ms: None,
        disable_nonessential_traffic: None,
        logo_svg: None,
        created_at: now.clone(),
        updated_at: now,
    };

    state
        .keyring
        .set_secret(&id, &ProviderSecret::Subscription { oauth })?;
    file.providers.push(provider.clone());
    save_providers_file(&state.providers_path(), &file)?;
    Ok(provider)
}

fn derive_subscription_name(label: Option<&str>, oauth: &serde_json::Value) -> String {
    if let Some(l) = label.map(str::trim).filter(|s| !s.is_empty()) {
        return format!("Subscription ({l})");
    }
    if let Some(email) = oauth
        .get("email")
        .and_then(|v| v.as_str())
        .or_else(|| oauth.get("account").and_then(|a| a.get("email")).and_then(|v| v.as_str()))
    {
        return format!("Subscription ({email})");
    }
    "Subscription".to_string()
}

fn unique_name(providers: &[Provider], base: &str) -> String {
    if !providers.iter().any(|p| p.name == base) {
        return base.to_string();
    }
    for i in 2..999 {
        let candidate = format!("{base} ({i})");
        if !providers.iter().any(|p| p.name == candidate) {
            return candidate;
        }
    }
    base.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn name_prefers_explicit_label() {
        let oauth = json!({"email": "a@b.com"});
        assert_eq!(
            derive_subscription_name(Some("Work Max"), &oauth),
            "Subscription (Work Max)"
        );
    }

    #[test]
    fn name_falls_back_to_email() {
        let oauth = json!({"email": "person@example.com"});
        assert_eq!(
            derive_subscription_name(None, &oauth),
            "Subscription (person@example.com)"
        );
    }

    #[test]
    fn name_falls_back_to_nested_account_email() {
        let oauth = json!({"account": {"email": "nested@example.com"}});
        assert_eq!(
            derive_subscription_name(None, &oauth),
            "Subscription (nested@example.com)"
        );
    }

    #[test]
    fn name_falls_back_to_generic() {
        let oauth = json!({"accessToken": "abc"});
        assert_eq!(derive_subscription_name(None, &oauth), "Subscription");
    }
}
