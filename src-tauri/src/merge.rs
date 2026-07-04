//! Pure env-merge logic. The single source of truth for what `load_provider`
//! writes into `settings.json.env`. Has no I/O so it is exhaustively unit-tested.

use serde_json::{Map, Value};

use crate::models::CANONICAL_ENV_KEYS;

/// Common TLDs we skip when deriving a provider name from a hostname.
/// Not exhaustive — covers the common case and falls back gracefully.
const TLD_SET: &[&str] = &[
    "com", "net", "org", "io", "co", "ai", "app", "dev", "lat", "xyz", "me", "us", "uk", "de",
    "jp", "cn", "fr", "tech", "cloud", "sh", "gg", "so", "tv",
];

/// Derive a provider display name from its base URL.
///
/// Rule: take what sits between any `api.` segment (e.g. `api.`, `capi.`)
/// and the next `.` in the hostname. Falls back to the first non-TLD
/// label right-to-left for hosts without an `api.` segment.
///
/// Examples:
///   api.minimax.io    -> "minimax"
///   capi.aerolink.lat -> "aerolink"
///   api.openai.com    -> "openai"
///   anthropic.com     -> "anthropic"
///
/// Returns "Provider" for invalid URLs or unparseable hosts.
pub fn derive_provider_name(base_url: &str) -> String {
    if base_url.is_empty() {
        return "Provider".to_string();
    }
    let Ok(url) = url::Url::parse(base_url) else {
        return "Provider".to_string();
    };
    let Some(host) = url.host_str() else {
        return "Provider".to_string();
    };
    let host = host.to_lowercase();
    if host.is_empty() {
        return "Provider".to_string();
    }

    if let Some(name) = name_after_api_prefix(&host) {
        if !name.is_empty() {
            return name.to_string();
        }
    }

    // Fallback: walk labels right-to-left, skipping TLDs and "api*"
    for label in host.split('.').rev() {
        if label.is_empty() {
            continue;
        }
        if !TLD_SET.contains(&label) && label != "api" && !label.starts_with("api") {
            return label.to_string();
        }
    }
    // No usable label found (e.g. "api.", "api.io") — return generic name.
    "Provider".to_string()
}

fn name_after_api_prefix(host: &str) -> Option<&str> {
    let idx = host.find("api.")?;
    let after = &host[idx + 4..];
    let dot = after.find('.');
    let candidate = match dot {
        Some(d) => &after[..d],
        None => after,
    };
    if candidate.is_empty() {
        None
    } else {
        Some(candidate)
    }
}

/// Merge a provider's env block into an existing settings.json env block.
///
/// Provider-authoritative semantics:
///
/// 1. Every **canonical** key (the 9 known ANTHROPIC_* / API_* / CLAUDE_CODE_*
///    keys) present in `provider_env` is set in the result. If its value is
///    `null`, it is treated as absent (not written). This means a provider
///    that doesn't include a field actively *unsets* it from the result —
///    no stale keys accumulate across loads.
///
/// 2. Canonical keys present in `existing` but absent from `provider_env`
///    are removed from the result.
///
/// 3. **Unknown** (non-canonical) keys in `existing` are preserved verbatim.
///    They are user-authored additions we don't understand; deleting them
///    silently would be hostile.
///
/// 4. Unknown keys in `provider_env` are passed through to the result
///    (forward-compat if a future Provider struct adds a new env field
///    before this function is updated).
pub fn merge_env(existing: Option<&Value>, provider_env: &Value) -> Value {
    let mut result: Map<String, Value> = existing
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter(|(k, _)| !is_canonical(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();

    for &key in CANONICAL_ENV_KEYS {
        match provider_env.get(key) {
            Some(value) if !value.is_null() => {
                result.insert(key.to_string(), value.clone());
            }
            _ => {
                // Absent from provider or explicit null → ensure removed.
                result.remove(key);
            }
        }
    }

    if let Some(extra) = provider_env.as_object() {
        for (k, v) in extra {
            if !is_canonical(k) {
                result.insert(k.clone(), v.clone());
            }
        }
    }

    Value::Object(result)
}

/// Convert a `Provider` + token into the env block that would be written
/// to settings.json. None fields are simply omitted; their absence is what
/// triggers the unset behavior in `merge_env`.
pub fn provider_env_block(
    provider: &crate::models::Provider,
    token: &str,
) -> Map<String, Value> {
    let mut env = Map::new();
    env.insert("ANTHROPIC_BASE_URL".into(), Value::String(provider.base_url.clone()));
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), Value::String(token.to_string()));
    insert_opt_str(&mut env, "ANTHROPIC_MODEL", provider.model.as_deref());
    insert_opt_str(
        &mut env,
        "ANTHROPIC_SMALL_FAST_MODEL",
        provider.small_fast_model.as_deref(),
    );
    insert_opt_str(
        &mut env,
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        provider.default_sonnet_model.as_deref(),
    );
    insert_opt_str(
        &mut env,
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        provider.default_opus_model.as_deref(),
    );
    insert_opt_str(
        &mut env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        provider.default_haiku_model.as_deref(),
    );
    if let Some(ms) = provider.api_timeout_ms {
        env.insert(
            "API_TIMEOUT_MS".into(),
            Value::String(ms.to_string()),
        );
    }
    if let Some(b) = provider.disable_nonessential_traffic {
        env.insert(
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
            Value::String(if b { "1".into() } else { "0".into() }),
        );
    }
    env
}

fn insert_opt_str(env: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        if !v.is_empty() {
            env.insert(key.into(), Value::String(v.to_string()));
        }
    }
}

fn is_canonical(key: &str) -> bool {
    CANONICAL_ENV_KEYS.contains(&key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn empty_merge_uses_provider_env() {
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
        });
        let result = merge_env(None, &provider);
        assert_eq!(result, provider);
    }

    #[test]
    fn replace_existing_with_provider_env() {
        let existing = json!({
            "ANTHROPIC_BASE_URL": "https://old",
            "ANTHROPIC_AUTH_TOKEN": "old-tok",
            "ANTHROPIC_MODEL": "claude-sonnet-4-6",
        });
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://new",
            "ANTHROPIC_AUTH_TOKEN": "new-tok",
        });
        let result = merge_env(Some(&existing), &provider);
        assert_eq!(
            result,
            json!({
                "ANTHROPIC_BASE_URL": "https://new",
                "ANTHROPIC_AUTH_TOKEN": "new-tok",
            })
        );
    }

    #[test]
    fn partial_provider_env_unsets_missing_canonical_keys() {
        let existing = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "ANTHROPIC_MODEL": "claude-opus-4-7",
            "API_TIMEOUT_MS": "3000000",
        });
        // Provider only sets base_url + token + a model. opus model is unset,
        // timeout is unset.
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "ANTHROPIC_MODEL": "claude-sonnet-4-6",
        });
        let result = merge_env(Some(&existing), &provider);
        assert_eq!(result.get("ANTHROPIC_MODEL").unwrap(), "claude-sonnet-4-6");
        assert!(result.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
        assert!(result.get("API_TIMEOUT_MS").is_none());
    }

    #[test]
    fn explicit_null_unsets_key() {
        let existing = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "ANTHROPIC_MODEL": "claude-opus-4-7",
        });
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "ANTHROPIC_MODEL": null,
        });
        let result = merge_env(Some(&existing), &provider);
        assert!(result.get("ANTHROPIC_MODEL").is_none());
        assert_eq!(result.get("ANTHROPIC_BASE_URL").unwrap(), "https://x");
    }

    #[test]
    fn preserves_unknown_keys_from_existing() {
        let existing = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "CUSTOM_TOOL_KEY": "user-set",
            "MY_PLUGIN_CONFIG": {"nested": true},
        });
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
        });
        let result = merge_env(Some(&existing), &provider);
        assert_eq!(result.get("CUSTOM_TOOL_KEY").unwrap(), "user-set");
        assert_eq!(result.get("MY_PLUGIN_CONFIG").unwrap(), &json!({"nested": true}));
    }

    #[test]
    fn merge_is_idempotent() {
        let existing = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "API_TIMEOUT_MS": "3000000",
            "CUSTOM_KEY": "keep",
        });
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
        });
        let once = merge_env(Some(&existing), &provider);
        let twice = merge_env(Some(&once), &provider);
        assert_eq!(once, twice);
    }

    #[test]
    fn unknown_keys_in_provider_env_are_passed_through() {
        let existing = json!({"ANTHROPIC_BASE_URL": "https://x", "ANTHROPIC_AUTH_TOKEN": "tok"});
        let provider = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_AUTH_TOKEN": "tok",
            "FUTURE_FIELD": "value",
        });
        let result = merge_env(Some(&existing), &provider);
        assert_eq!(result.get("FUTURE_FIELD").unwrap(), "value");
    }

    #[test]
    fn empty_string_in_existing_canonical_key_is_kept_when_provider_absent() {
        // Provider doesn't set API_TIMEOUT_MS; if existing has empty string,
        // it's treated as a canonical key the user put there. Per semantics,
        // it should be removed (no stale keys).
        let existing = json!({
            "ANTHROPIC_BASE_URL": "https://x",
            "API_TIMEOUT_MS": "",
        });
        let provider = json!({"ANTHROPIC_BASE_URL": "https://x"});
        let result = merge_env(Some(&existing), &provider);
        assert!(result.get("API_TIMEOUT_MS").is_none());
    }

    #[test]
    fn provider_env_block_includes_only_set_fields() {
        use crate::models::Provider;
        let p = Provider {
            id: "x".into(),
            name: "test".into(),
            base_url: "https://api.example.com".into(),
            model: Some("claude-sonnet-4-6".into()),
            small_fast_model: None,
            default_sonnet_model: Some("claude-sonnet-4-6".into()),
            default_opus_model: None,
            default_haiku_model: Some("claude-haiku-4-5".into()),
            api_timeout_ms: Some(120_000),
            disable_nonessential_traffic: Some(true),
            created_at: "2026-07-04T00:00:00Z".into(),
            updated_at: "2026-07-04T00:00:00Z".into(),
        };
        let env = provider_env_block(&p, "secret-token");
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://api.example.com"
        );
        assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "secret-token");
        assert_eq!(env.get("ANTHROPIC_MODEL").unwrap(), "claude-sonnet-4-6");
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").unwrap(),
            "claude-sonnet-4-6"
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").unwrap(),
            "claude-haiku-4-5"
        );
        assert_eq!(env.get("API_TIMEOUT_MS").unwrap(), "120000");
        assert_eq!(
            env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC").unwrap(),
            "1"
        );
        // None fields are absent (not serialized as null)
        assert!(env.get("ANTHROPIC_SMALL_FAST_MODEL").is_none());
        assert!(env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
    }

    #[test]
    fn canonical_keys_list_matches_plan() {
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_BASE_URL"));
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_AUTH_TOKEN"));
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_MODEL"));
        assert!(CANONICAL_ENV_KEYS.contains(&"API_TIMEOUT_MS"));
        assert!(CANONICAL_ENV_KEYS.contains(&"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"));
        // exact count so a careless edit is caught
        assert_eq!(CANONICAL_ENV_KEYS.len(), 9);
        // helper used by parser
        let _ = parse(r#"{"a":1}"#);
    }

    #[test]
    fn derive_provider_name_from_api_subdomain() {
        assert_eq!(
            derive_provider_name("https://api.minimax.io/anthropic"),
            "minimax"
        );
        assert_eq!(
            derive_provider_name("https://capi.aerolink.lat/"),
            "aerolink"
        );
        assert_eq!(
            derive_provider_name("https://api.openai.com/v1"),
            "openai"
        );
        assert_eq!(
            derive_provider_name("https://api.anthropic.com"),
            "anthropic"
        );
    }

    #[test]
    fn derive_provider_name_falls_back_for_plain_hosts() {
        assert_eq!(derive_provider_name("https://anthropic.com"), "anthropic");
        assert_eq!(derive_provider_name("https://my-proxy.io"), "my-proxy");
        assert_eq!(
            derive_provider_name("https://api.example.co.uk"),
            "example"
        );
    }

    #[test]
    fn derive_provider_name_handles_edge_cases() {
        assert_eq!(derive_provider_name(""), "Provider");
        assert_eq!(derive_provider_name("not a url"), "Provider");
        assert_eq!(
            derive_provider_name("https://api./v1"),
            "Provider" // empty candidate
        );
    }
}