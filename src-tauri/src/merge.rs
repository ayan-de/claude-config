//! Pure env-merge logic. The single source of truth for what `load_provider`
//! writes into `settings.json.env`. Has no I/O so it is exhaustively unit-tested.

use serde_json::{Map, Value};

use crate::models::{Provider, ProviderKind, ProviderSecret, CANONICAL_ENV_KEYS};

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
/// 1. Every **canonical** key present in `provider_env` is set in the result.
///    If its value is `null`, it is treated as absent (not written).
///
/// 2. Canonical keys present in `existing` but absent from `provider_env`
///    are removed from the result. This is what makes mode-switching self-
///    cleaning: when you activate a Bedrock provider after a Custom one, the
///    Bedrock env block has no `ANTHROPIC_AUTH_TOKEN`, so it disappears.
///
/// 3. **Unknown** (non-canonical) keys in `existing` are preserved verbatim.
///    They are user-authored additions we don't understand; deleting them
///    silently would be hostile.
///
/// 4. Unknown keys in `provider_env` are passed through to the result
///    (forward-compat if a future kind adds a new env field before this
///    function's canonical list is updated).
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

/// Build the env block that represents `provider` (with its keyring `secret`)
/// in `settings.json`. What's included depends on `provider.kind`:
///
/// - Subscription: empty. OAuth in `.credentials.json` is authoritative;
///   any leftover `ANTHROPIC_AUTH_TOKEN` here would override and disable it.
/// - Console: `ANTHROPIC_API_KEY`.
/// - Custom: today's behaviour — `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN`.
/// - Bedrock: `CLAUDE_CODE_USE_BEDROCK=1` + AWS_* from provider metadata
///   (region, profile) and secret (access key + secret key + session token).
/// - Vertex: `CLAUDE_CODE_USE_VERTEX=1` + project id + region +
///   optional `GOOGLE_APPLICATION_CREDENTIALS` path.
///
/// Model overrides + `API_TIMEOUT_MS` + `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`
/// apply to any kind (Claude Code respects them regardless of backend).
///
/// If `secret.kind()` doesn't match `provider.kind`, this function still
/// produces a syntactically-valid env block from whatever fields it can find,
/// but the caller (`load_provider_cmd`) should validate before write.
pub fn provider_env_block(provider: &Provider, secret: &ProviderSecret) -> Map<String, Value> {
    let mut env = Map::new();

    match provider.kind {
        ProviderKind::Subscription => {
            // Empty — falls back to .credentials.json OAuth.
        }
        ProviderKind::Console => {
            if let ProviderSecret::Console { api_key } = secret {
                env.insert("ANTHROPIC_API_KEY".into(), Value::String(api_key.clone()));
            }
        }
        ProviderKind::Custom => {
            if let Some(url) = &provider.base_url {
                env.insert("ANTHROPIC_BASE_URL".into(), Value::String(url.clone()));
            }
            if let ProviderSecret::Custom { auth_token } = secret {
                env.insert(
                    "ANTHROPIC_AUTH_TOKEN".into(),
                    Value::String(auth_token.clone()),
                );
            }
        }
        ProviderKind::Bedrock => {
            env.insert("CLAUDE_CODE_USE_BEDROCK".into(), Value::String("1".into()));
            insert_opt_str(&mut env, "AWS_REGION", provider.aws_region.as_deref());
            if let Some(profile) = &provider.aws_profile {
                env.insert("AWS_PROFILE".into(), Value::String(profile.clone()));
            } else if let ProviderSecret::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            } = secret
            {
                env.insert(
                    "AWS_ACCESS_KEY_ID".into(),
                    Value::String(access_key_id.clone()),
                );
                env.insert(
                    "AWS_SECRET_ACCESS_KEY".into(),
                    Value::String(secret_access_key.clone()),
                );
                if let Some(tok) = session_token {
                    env.insert("AWS_SESSION_TOKEN".into(), Value::String(tok.clone()));
                }
            }
        }
        ProviderKind::Vertex => {
            env.insert("CLAUDE_CODE_USE_VERTEX".into(), Value::String("1".into()));
            insert_opt_str(
                &mut env,
                "ANTHROPIC_VERTEX_PROJECT_ID",
                provider.vertex_project_id.as_deref(),
            );
            insert_opt_str(
                &mut env,
                "CLOUD_ML_REGION",
                provider.vertex_region.as_deref(),
            );
            insert_opt_str(
                &mut env,
                "GOOGLE_APPLICATION_CREDENTIALS",
                provider.google_application_credentials.as_deref(),
            );
        }
    }

    // Model overrides + misc — apply to any kind Claude Code understands them for.
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
        env.insert("API_TIMEOUT_MS".into(), Value::String(ms.to_string()));
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

    fn base_provider(kind: ProviderKind) -> Provider {
        Provider {
            id: "x".into(),
            name: "test".into(),
            kind,
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
            created_at: "2026-07-04T00:00:00Z".into(),
            updated_at: "2026-07-04T00:00:00Z".into(),
        }
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
        assert_eq!(
            result.get("MY_PLUGIN_CONFIG").unwrap(),
            &json!({"nested": true})
        );
    }

    #[test]
    fn env_block_subscription_is_empty() {
        let p = base_provider(ProviderKind::Subscription);
        let s = ProviderSecret::Subscription {
            oauth: json!({"accessToken": "abc"}),
        };
        let env = provider_env_block(&p, &s);
        // Empty save for model overrides / timeout — none set here.
        assert!(env.is_empty(), "expected empty env, got {env:?}");
    }

    #[test]
    fn env_block_console_sets_api_key() {
        let p = base_provider(ProviderKind::Console);
        let s = ProviderSecret::Console {
            api_key: "sk-ant-abc".into(),
        };
        let env = provider_env_block(&p, &s);
        assert_eq!(env.get("ANTHROPIC_API_KEY").unwrap(), "sk-ant-abc");
        assert!(env.get("ANTHROPIC_BASE_URL").is_none());
        assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none());
    }

    #[test]
    fn env_block_custom_sets_base_and_token() {
        let mut p = base_provider(ProviderKind::Custom);
        p.base_url = Some("https://api.example.com".into());
        let s = ProviderSecret::Custom {
            auth_token: "sk-cp-xyz".into(),
        };
        let env = provider_env_block(&p, &s);
        assert_eq!(env.get("ANTHROPIC_BASE_URL").unwrap(), "https://api.example.com");
        assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "sk-cp-xyz");
        assert!(env.get("ANTHROPIC_API_KEY").is_none());
    }

    #[test]
    fn env_block_bedrock_with_profile() {
        let mut p = base_provider(ProviderKind::Bedrock);
        p.aws_region = Some("us-east-1".into());
        p.aws_profile = Some("work".into());
        // Even though secret has static creds, profile takes precedence.
        let s = ProviderSecret::Bedrock {
            access_key_id: "AKIA...".into(),
            secret_access_key: "shhh".into(),
            session_token: None,
        };
        let env = provider_env_block(&p, &s);
        assert_eq!(env.get("CLAUDE_CODE_USE_BEDROCK").unwrap(), "1");
        assert_eq!(env.get("AWS_REGION").unwrap(), "us-east-1");
        assert_eq!(env.get("AWS_PROFILE").unwrap(), "work");
        assert!(env.get("AWS_ACCESS_KEY_ID").is_none());
        assert!(env.get("AWS_SECRET_ACCESS_KEY").is_none());
    }

    #[test]
    fn env_block_bedrock_static_creds() {
        let mut p = base_provider(ProviderKind::Bedrock);
        p.aws_region = Some("eu-west-1".into());
        let s = ProviderSecret::Bedrock {
            access_key_id: "AKIAABC".into(),
            secret_access_key: "sec".into(),
            session_token: Some("tok".into()),
        };
        let env = provider_env_block(&p, &s);
        assert_eq!(env.get("AWS_REGION").unwrap(), "eu-west-1");
        assert_eq!(env.get("AWS_ACCESS_KEY_ID").unwrap(), "AKIAABC");
        assert_eq!(env.get("AWS_SECRET_ACCESS_KEY").unwrap(), "sec");
        assert_eq!(env.get("AWS_SESSION_TOKEN").unwrap(), "tok");
        assert!(env.get("AWS_PROFILE").is_none());
    }

    #[test]
    fn env_block_vertex_sets_project_and_region() {
        let mut p = base_provider(ProviderKind::Vertex);
        p.vertex_project_id = Some("my-project".into());
        p.vertex_region = Some("us-central1".into());
        p.google_application_credentials = Some("/home/me/sa.json".into());
        let s = ProviderSecret::Vertex {};
        let env = provider_env_block(&p, &s);
        assert_eq!(env.get("CLAUDE_CODE_USE_VERTEX").unwrap(), "1");
        assert_eq!(env.get("ANTHROPIC_VERTEX_PROJECT_ID").unwrap(), "my-project");
        assert_eq!(env.get("CLOUD_ML_REGION").unwrap(), "us-central1");
        assert_eq!(env.get("GOOGLE_APPLICATION_CREDENTIALS").unwrap(), "/home/me/sa.json");
    }

    #[test]
    fn model_overrides_apply_regardless_of_kind() {
        let mut p = base_provider(ProviderKind::Console);
        p.model = Some("claude-sonnet-4-6".into());
        p.api_timeout_ms = Some(120_000);
        let s = ProviderSecret::Console {
            api_key: "k".into(),
        };
        let env = provider_env_block(&p, &s);
        assert_eq!(env.get("ANTHROPIC_MODEL").unwrap(), "claude-sonnet-4-6");
        assert_eq!(env.get("API_TIMEOUT_MS").unwrap(), "120000");
    }

    #[test]
    fn switch_custom_to_bedrock_strips_anthropic_keys() {
        // Simulates load_provider_cmd calling merge_env twice: once for the
        // Custom provider currently active, then again for the incoming
        // Bedrock provider. The Bedrock env has no ANTHROPIC_AUTH_TOKEN, and
        // canonical-key semantics say it must be stripped.
        let mut custom = base_provider(ProviderKind::Custom);
        custom.base_url = Some("https://api.custom".into());
        let custom_env = provider_env_block(
            &custom,
            &ProviderSecret::Custom {
                auth_token: "cus-tok".into(),
            },
        );

        let mut bedrock = base_provider(ProviderKind::Bedrock);
        bedrock.aws_region = Some("us-east-1".into());
        bedrock.aws_profile = Some("work".into());
        let bedrock_env = provider_env_block(
            &bedrock,
            &ProviderSecret::Bedrock {
                access_key_id: "".into(),
                secret_access_key: "".into(),
                session_token: None,
            },
        );

        let after_custom = merge_env(None, &Value::Object(custom_env));
        let after_bedrock = merge_env(Some(&after_custom), &Value::Object(bedrock_env));

        // Anthropic keys are gone
        assert!(after_bedrock.get("ANTHROPIC_BASE_URL").is_none());
        assert!(after_bedrock.get("ANTHROPIC_AUTH_TOKEN").is_none());
        // Bedrock keys are present
        assert_eq!(after_bedrock.get("CLAUDE_CODE_USE_BEDROCK").unwrap(), "1");
        assert_eq!(after_bedrock.get("AWS_REGION").unwrap(), "us-east-1");
        assert_eq!(after_bedrock.get("AWS_PROFILE").unwrap(), "work");
    }

    #[test]
    fn switch_bedrock_to_subscription_strips_all_canonical_keys() {
        let mut bedrock = base_provider(ProviderKind::Bedrock);
        bedrock.aws_region = Some("us-east-1".into());
        bedrock.aws_profile = Some("work".into());
        let bedrock_env = provider_env_block(
            &bedrock,
            &ProviderSecret::Bedrock {
                access_key_id: "".into(),
                secret_access_key: "".into(),
                session_token: None,
            },
        );

        let sub = base_provider(ProviderKind::Subscription);
        let sub_env = provider_env_block(
            &sub,
            &ProviderSecret::Subscription {
                oauth: json!({"accessToken": "x"}),
            },
        );

        let after_bedrock = merge_env(None, &Value::Object(bedrock_env));
        let after_sub = merge_env(Some(&after_bedrock), &Value::Object(sub_env));

        // All Bedrock env vars gone; env block is now empty apart from any
        // user-authored unknown keys (there are none in this test).
        assert!(after_sub.get("CLAUDE_CODE_USE_BEDROCK").is_none());
        assert!(after_sub.get("AWS_REGION").is_none());
        assert!(after_sub.get("AWS_PROFILE").is_none());
        assert!(
            after_sub.as_object().unwrap().is_empty(),
            "expected empty env after switching to subscription, got {after_sub:?}"
        );
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
    fn derive_provider_name_from_api_subdomain() {
        assert_eq!(
            derive_provider_name("https://api.minimax.io/anthropic"),
            "minimax"
        );
        assert_eq!(derive_provider_name("https://capi.aerolink.lat/"), "aerolink");
        assert_eq!(derive_provider_name("https://api.openai.com/v1"), "openai");
        assert_eq!(derive_provider_name("https://api.anthropic.com"), "anthropic");
    }

    #[test]
    fn derive_provider_name_falls_back_for_plain_hosts() {
        assert_eq!(derive_provider_name("https://anthropic.com"), "anthropic");
        assert_eq!(derive_provider_name("https://my-proxy.io"), "my-proxy");
        assert_eq!(derive_provider_name("https://api.example.co.uk"), "example");
    }

    #[test]
    fn derive_provider_name_handles_edge_cases() {
        assert_eq!(derive_provider_name(""), "Provider");
        assert_eq!(derive_provider_name("not a url"), "Provider");
        assert_eq!(derive_provider_name("https://api./v1"), "Provider");
    }
}
