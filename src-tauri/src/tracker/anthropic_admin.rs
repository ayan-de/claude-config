//! AnthropicAdmin source — hits api.anthropic.com's Admin API.
//!
//! Use this for `console`-kind providers. The user pastes an admin API key
//! from console.anthropic.com → Settings → Organization → API Keys (the
//! key must have admin scope, not just a regular usage key).
//!
//! ## Endpoints used
//!
//! - `GET /v1/organizations/usage` — daily usage broken down by model.
//!   We collapse the daily rows into a single `TrackerUsage` snapshot,
//!   which is the natural shape for a per-minute refresh.
//! - `GET /v1/organizations/cost_report` — current-month spend.
//!
//! Both endpoints require the `anthropic-version: 2023-06-01` header and
//! the `x-api-key` header set to the admin key.

use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::models::{AppError, AppResult};
use crate::tracker::{ModelUsage, SourceId, TrackerField, TrackerSource, TrackerUsage, UsageWindow};

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicAdminSource;

impl TrackerSource for AnthropicAdminSource {
    fn id(&self) -> SourceId {
        SourceId::AnthropicAdmin
    }

    fn display_name(&self) -> &'static str {
        "Anthropic Console (Admin API)"
    }

    fn description(&self) -> &'static str {
        "Uses an admin API key from console.anthropic.com to fetch daily usage and current-month spend."
    }

    fn fields(&self) -> Vec<TrackerField> {
        vec![TrackerField {
            key: "admin_api_key",
            label: "Admin API key",
            placeholder: "sk-ant-admin-...",
            secret: true,
            multiline: false,
            required: true,
            hint: Some("Create at console.anthropic.com → Settings → Organization → API Keys. Stored in OS keyring."),
        }]
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["console"]
    }

    fn validate_config(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        let key = config
            .get("admin_api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("AnthropicAdmin: missing `admin_api_key` field".into()))?;
        if key.is_empty() {
            return Err(AppError::Validation(
                "AnthropicAdmin: admin_api_key is empty".into(),
            ));
        }
        if !key.starts_with("sk-ant-") {
            // Soft check — Anthropic may issue new prefixes; the API call
            // itself is the real validation. We warn-via-Validation so the
            // user sees a clear hint before the network call fails.
            return Err(AppError::Validation(format!(
                "AnthropicAdmin: key does not start with `sk-ant-` (got {:?})",
                &key[..key.len().min(10)]
            )));
        }
        Ok(())
    }

    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        self.validate_config(config)?;
        let api_key = config["admin_api_key"].as_str().unwrap().trim();

        // 7-day rolling window — cheap and matches the UI's per-minute
        // refresh cadence. Anthropic's bucket_width=1d returns one row per
        // day per model.
        let today = Utc::now().date_naive();
        let start = today - Duration::days(7);
        let usage_url = format!(
            "{ANTHROPIC_API_BASE}/v1/organizations/usage?start_date={start}&end_date={today}&bucket_width=1d&group_by=model"
        );
        let cost_url = format!(
            "{ANTHROPIC_API_BASE}/v1/organizations/cost_report?start_date={start}&end_date={today}&bucket_width=1d"
        );

        let usage_resp = client
            .get(&usage_url)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .map_err(|e| AppError::Internal(format!("AnthropicAdmin: usage request failed: {e}")))?;
        let status = usage_resp.status();
        let usage_body: UsageResponse = usage_resp
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("AnthropicAdmin: usage HTTP {status}: {e}")))?
            .json()
            .map_err(|e| AppError::Internal(format!("AnthropicAdmin: usage body parse: {e}")))?;

        // Aggregate per-model across the 7-day window.
        let mut per_model: HashMap<String, ModelUsage> = HashMap::new();
        for b in &usage_body.data {
            let model = b.model.clone().unwrap_or_else(|| "unknown".to_string());
            let entry = per_model.entry(model).or_insert(ModelUsage {
                model: String::new(),
                input_tokens: Some(0),
                output_tokens: Some(0),
                cost_usd: Some(0.0),
            });
            entry.input_tokens = Some(entry.input_tokens.unwrap_or(0) + b.input_tokens.unwrap_or(0));
            entry.output_tokens = Some(entry.output_tokens.unwrap_or(0) + b.output_tokens.unwrap_or(0));
            entry.cost_usd = Some(entry.cost_usd.unwrap_or(0.0) + b.cost_usd.unwrap_or(0.0));
        }
        // Now stamp the model name on each entry.
        for (k, v) in per_model.iter_mut() {
            v.model = k.clone();
        }
        let mut models: Vec<ModelUsage> = per_model.into_values().collect();
        models.sort_by(|a, b| {
            b.cost_usd
                .unwrap_or(0.0)
                .partial_cmp(&a.cost_usd.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.model.cmp(&b.model))
        });

        // Cost report is best-effort — a 403 (the key has usage scope but
        // not billing scope) should not fail the whole fetch.
        let total_cost = match client
            .get(&cost_url)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
        {
            Ok(r) if r.status().is_success() => r
                .json::<CostResponse>()
                .ok()
                .and_then(|c| c.total_cost)
                .map(|d| d as f64),
            _ => None,
        };

        // Headline "7-day tokens" window for the progress bar.
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

// ---------------------------------------------------------------------------
// Wire types — kept permissive (Option everywhere) so an Anthropic-side
// schema change degrades gracefully instead of breaking the parse.
// ---------------------------------------------------------------------------

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

    fn cfg(key: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("admin_api_key".into(), json!(key));
        m
    }

    #[test]
    fn rejects_empty_key() {
        let s = AnthropicAdminSource;
        assert!(s.validate_config(&cfg("")).is_err());
    }

    #[test]
    fn rejects_missing_key() {
        let s = AnthropicAdminSource;
        assert!(s.validate_config(&serde_json::Map::new()).is_err());
    }

    #[test]
    fn rejects_non_anthropic_prefix() {
        let s = AnthropicAdminSource;
        let err = s.validate_config(&cfg("sk-other-abc")).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn accepts_well_formed_key() {
        let s = AnthropicAdminSource;
        s.validate_config(&cfg("sk-ant-admin-XXXXXX")).unwrap();
    }
}
