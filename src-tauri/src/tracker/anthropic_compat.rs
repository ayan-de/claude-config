//! AnthropicCompat source — points at a third-party relay that exposes the
//! same `/v1/organizations/usage` Admin API as Anthropic itself.
//!
//! Use this for `custom`-kind providers (third-party Anthropic-compatible
//! relays). The user pastes:
//!  - `base_url` — the relay's API root (e.g. `https://api.manyclaw.com`)
//!  - `admin_api_key` — a key with admin/usage scope on the relay
//!
//! The same wire types as `AnthropicAdmin` are reused; the only difference
//! is the URL.

use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::Deserialize;
use url::Url;

use crate::models::{AppError, AppResult};
use crate::tracker::{ModelUsage, SourceId, TrackerField, TrackerSource, TrackerUsage, UsageWindow};

pub struct AnthropicCompatSource;

impl TrackerSource for AnthropicCompatSource {
    fn id(&self) -> SourceId {
        SourceId::AnthropicCompat
    }

    fn display_name(&self) -> &'static str {
        "Anthropic-compatible relay"
    }

    fn description(&self) -> &'static str {
        "For third-party relays that expose the Anthropic admin API at {base_url}/v1/organizations/usage."
    }

    fn fields(&self) -> Vec<TrackerField> {
        vec![
            TrackerField {
                key: "base_url",
                label: "Base URL",
                placeholder: "https://api.example.com",
                secret: false,
                multiline: false,
                required: true,
                hint: Some("The relay's API root. Trailing slashes are stripped."),
            },
            TrackerField {
                key: "admin_api_key",
                label: "Admin API key",
                placeholder: "sk-...",
                secret: true,
                multiline: false,
                required: true,
                hint: Some("Key with admin/usage scope on the relay. Stored in OS keyring."),
            },
        ]
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["custom"]
    }

    fn validate_config(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        let base = config
            .get("base_url")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("AnthropicCompat: missing `base_url` field".into()))?;
        if base.is_empty() {
            return Err(AppError::Validation("AnthropicCompat: base_url is empty".into()));
        }
        let parsed = Url::parse(base)
            .map_err(|e| AppError::Validation(format!("AnthropicCompat: base_url is not a valid URL: {e}")))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(AppError::Validation(format!(
                "AnthropicCompat: base_url must use http or https (got {:?})",
                parsed.scheme()
            )));
        }

        let key = config
            .get("admin_api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("AnthropicCompat: missing `admin_api_key` field".into()))?;
        if key.is_empty() {
            return Err(AppError::Validation("AnthropicCompat: admin_api_key is empty".into()));
        }
        Ok(())
    }

    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        self.validate_config(config)?;
        let base_raw = config["base_url"].as_str().unwrap().trim();
        let api_key = config["admin_api_key"].as_str().unwrap().trim();

        // Normalize: strip trailing slash so `Url::join` doesn't drop the
        // `/v1/...` path segment.
        let base = base_raw.trim_end_matches('/');
        let today = Utc::now().date_naive();
        let start = today - Duration::days(7);
        let usage_url = format!(
            "{base}/v1/organizations/usage?start_date={start}&end_date={today}&bucket_width=1d&group_by=model"
        );
        let cost_url = format!(
            "{base}/v1/organizations/cost_report?start_date={start}&end_date={today}&bucket_width=1d"
        );

        let usage_resp = client
            .get(&usage_url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .map_err(|e| AppError::Internal(format!("AnthropicCompat: usage request failed: {e}")))?;
        let status = usage_resp.status();
        let usage_body: UsageResponse = usage_resp
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("AnthropicCompat: usage HTTP {status}: {e}")))?
            .json()
            .map_err(|e| AppError::Internal(format!("AnthropicCompat: usage body parse: {e}")))?;

        let mut per_model: HashMap<String, ModelUsage> = HashMap::new();
        for b in &usage_body.data {
            let model = b.model.clone().unwrap_or_else(|| "unknown".to_string());
            let entry = per_model.entry(model.clone()).or_insert(ModelUsage {
                model,
                input_tokens: Some(0),
                output_tokens: Some(0),
                cost_usd: Some(0.0),
            });
            entry.input_tokens = Some(entry.input_tokens.unwrap_or(0) + b.input_tokens.unwrap_or(0));
            entry.output_tokens = Some(entry.output_tokens.unwrap_or(0) + b.output_tokens.unwrap_or(0));
            entry.cost_usd = Some(entry.cost_usd.unwrap_or(0.0) + b.cost_usd.unwrap_or(0.0));
        }
        let mut models: Vec<ModelUsage> = per_model.into_values().collect();
        models.sort_by(|a, b| {
            b.cost_usd
                .unwrap_or(0.0)
                .partial_cmp(&a.cost_usd.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.model.cmp(&b.model))
        });

        let total_cost = match client
            .get(&cost_url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
        {
            Ok(r) if r.status().is_success() => r
                .json::<CostResponse>()
                .ok()
                .and_then(|c| c.total_cost)
                .map(|d| d as f64),
            _ => None,
        };

        let total_in: u64 = models.iter().filter_map(|m| m.input_tokens).sum();
        let total_out: u64 = models.iter().filter_map(|m| m.output_tokens).sum();
        let mut windows = Vec::new();
        if !models.is_empty() {
            windows.push(UsageWindow {
                label: "Last 7 days tokens".into(),
                used: Some((total_in + total_out) as f64),
                limit: None,
                used_percent: None,
                unit: Some("tokens".into()),
                resets_at: None,
                reset_label: None,
            });
        }

        Ok(TrackerUsage {
            windows,
            models,
            cost_usd: total_cost,
            fetched_at: Utc::now().to_rfc3339(),
            note: Some("7-day rolling window. Refresh to update.".into()),
        })
    }
}

#[derive(Debug, Deserialize, Default)]
struct UsageResponse {
    #[serde(default)]
    data: Vec<UsageBucket>,
}

#[derive(Debug, Deserialize, Default)]
struct UsageBucket {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct CostResponse {
    #[serde(default)]
    total_cost: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(base: &str, key: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("base_url".into(), json!(base));
        m.insert("admin_api_key".into(), json!(key));
        m
    }

    #[test]
    fn rejects_empty_base_or_key() {
        let s = AnthropicCompatSource;
        assert!(s.validate_config(&cfg("", "k")).is_err());
        assert!(s.validate_config(&cfg("https://x", "")).is_err());
    }

    #[test]
    fn rejects_missing_fields() {
        let s = AnthropicCompatSource;
        assert!(s.validate_config(&serde_json::Map::new()).is_err());
    }

    #[test]
    fn rejects_invalid_url() {
        let s = AnthropicCompatSource;
        assert!(s.validate_config(&cfg("not a url", "k")).is_err());
    }

    #[test]
    fn rejects_non_http_scheme() {
        let s = AnthropicCompatSource;
        assert!(s.validate_config(&cfg("ftp://example.com", "k")).is_err());
    }

    #[test]
    fn accepts_well_formed() {
        let s = AnthropicCompatSource;
        s.validate_config(&cfg("https://api.example.com", "sk-abc")).unwrap();
        s.validate_config(&cfg("https://api.example.com/", "sk-abc")).unwrap(); // trailing slash OK
    }
}
