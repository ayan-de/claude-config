//! MiniMax (minimax) tracker source.
//!
//! Adapted from `codexbar-desktop/backend/src/providers/minimax/mod.rs`.
//! Same two endpoints and headers, trimmed to the TrackerSource trait.
//!
//! ## Endpoints
//!
//! 1. **Coding Plan** (the `sk-cp-…` keys): `GET {api_base}/v1/api/openplatform/coding_plan/remains?GroupId={group_id}`
//!    Returns a 5-hour session window plus an optional weekly window.
//! 2. **General billing**: `GET {api_base}/v1/billing/usage?group_id={group_id}`
//!    Returns `used_amount` / `total_quota` (a 0-100% quota) plus a plan
//!    name (we surface it as the "note" line).
//!
//! For non-coding-plan keys we go straight to the billing endpoint. The
//! coding plan endpoint returns AuthRequired on those, so we fall through
//! silently.
//!
//! ## Region
//!
//! `minimax` is the global domain (`platform.minimax.io` / `api.minimax.io`).
//! `minimaxi.com` is the China-mainland mirror. The user picks via a
//! `region` field — defaults to `global`. Region values other than `cn`
//! (case-insensitive) are treated as global.

use std::collections::HashMap;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::models::{AppError, AppResult};
use crate::tracker::{ModelUsage, SourceId, TrackerField, TrackerSource, TrackerUsage, UsageWindow};

pub struct MiniMaxSource;

/// Region selector for the MiniMax endpoints. `Global` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniMaxRegion {
    Global,
    ChinaMainland,
}

impl MiniMaxRegion {
    pub fn from_settings_value(value: Option<&str>) -> Self {
        match value.unwrap_or("").trim().to_lowercase().as_str() {
            "cn" | "china" | "china-mainland" | "china_mainland" | "mainland" => {
                Self::ChinaMainland
            }
            _ => Self::Global,
        }
    }

    pub fn api_base_url(self) -> &'static str {
        match self {
            Self::Global => "https://api.minimax.io",
            Self::ChinaMainland => "https://api.minimaxi.com",
        }
    }
}

impl TrackerSource for MiniMaxSource {
    fn id(&self) -> SourceId {
        SourceId::MiniMax
    }

    fn display_name(&self) -> &'static str {
        "MiniMax"
    }

    fn description(&self) -> &'static str {
        "Tracks MiniMax Coding Plan (5h + weekly) and billing quota via api.minimax.io."
    }

    fn fields(&self) -> Vec<TrackerField> {
        vec![
            TrackerField {
                key: "api_key",
                label: "API key",
                placeholder: "sk-cp-...",
                secret: true,
                multiline: false,
                required: true,
                hint: Some("Create at platform.minimax.io → API Keys. Stored in OS keyring."),
            },
            TrackerField {
                key: "group_id",
                label: "Group ID",
                placeholder: "1234567890",
                secret: false,
                multiline: false,
                required: true,
                hint: Some("The numeric group id shown on the MiniMax dashboard."),
            },
            TrackerField {
                key: "region",
                label: "Region",
                placeholder: "global",
                secret: false,
                multiline: false,
                required: false,
                hint: Some("`global` (api.minimax.io) or `cn` (api.minimaxi.com). Defaults to global."),
            },
        ]
    }

    /// MiniMax relays are configured as `custom` providers in the main
    /// form (with a MiniMax base URL), so this source is scoped to
    /// `custom`. The auto-pick narrows further by name/URL match.
    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["custom"]
    }

    fn validate_config(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        let key = config
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("MiniMax: missing `api_key` field".into()))?;
        if key.is_empty() {
            return Err(AppError::Validation("MiniMax: api_key is empty".into()));
        }
        let group = config
            .get("group_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("MiniMax: missing `group_id` field".into()))?;
        if group.is_empty() {
            return Err(AppError::Validation("MiniMax: group_id is empty".into()));
        }
        // region is optional; anything goes.
        Ok(())
    }

    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        self.validate_config(config)?;
        let api_key = config["api_key"].as_str().unwrap().trim();
        let group_id = config["group_id"].as_str().unwrap().trim();
        let region = MiniMaxRegion::from_settings_value(config.get("region").and_then(serde_json::Value::as_str));
        let base = region.api_base_url();

        let mut windows: Vec<UsageWindow> = Vec::new();
        let mut note: Option<String> = None;
        let mut cost_usd: Option<f64> = None;
        let mut models: Vec<ModelUsage> = Vec::new();

        // Try the Coding Plan endpoint first if the key looks like one.
        // On AuthRequired (non-CP keys) we fall through to billing.
        if api_key.starts_with("sk-cp-") {
            match fetch_coding_plan(client, base, group_id, api_key) {
                Ok((primary, weekly)) => {
                    windows.push(primary);
                    if let Some(w) = weekly {
                        windows.push(w);
                    }
                    note = Some("MiniMax Coding Plan".into());
                }
                Err(AppError::Validation(msg)) if msg.contains("rejected") => {
                    // AuthRequired falls through to billing.
                }
                Err(e) => return Err(e),
            }
        }

        // Always try billing — it gives us a quota percentage and a
        // plan name we can show even when the coding plan endpoint
        // worked. Non-fatal if it fails.
        if let Ok((quota_pct, plan, top_models, spend)) = fetch_billing(client, base, group_id, api_key) {
            if plan.is_some() {
                note = plan;
            }
            // If we don't have a primary window yet, surface the billing
            // quota as one.
            if windows.is_empty() {
                windows.push(UsageWindow {
                    label: "Quota".into(),
                    used: Some(quota_pct),
                    limit: Some(100.0),
                    used_percent: Some(quota_pct),
                    unit: Some("%".into()),
                    resets_at: None,
                    reset_label: None,
                });
            }
            cost_usd = spend;
            // Top models as a lightweight breakdown.
            models = top_models
                .into_iter()
                .map(|(name, tokens)| ModelUsage {
                    model: name,
                    input_tokens: Some(tokens),
                    output_tokens: Some(0),
                    cost_usd: None,
                })
                .collect();
        }

        if windows.is_empty() {
            return Err(AppError::Validation(
                "MiniMax: no usage data — check that the API key + group id are correct"
                    .into(),
            ));
        }

        Ok(TrackerUsage {
            windows,
            models,
            cost_usd,
            fetched_at: Utc::now().to_rfc3339(),
            note,
        })
    }
}

/// Hits `/v1/api/openplatform/coding_plan/remains`. Returns (primary
/// 5h window, optional weekly window).
fn fetch_coding_plan(
    client: &reqwest::blocking::Client,
    base: &str,
    group_id: &str,
    api_key: &str,
) -> AppResult<(UsageWindow, Option<UsageWindow>)> {
    let url = format!(
        "{}/v1/api/openplatform/coding_plan/remains?GroupId={}",
        base, group_id
    );
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("MM-API-Source", "claude-config")
        .send()
        .map_err(|e| AppError::Internal(format!("MiniMax: coding_plan request failed: {e}")))?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::Validation(
            "MiniMax: API key rejected (401/403). Re-export your key.".into(),
        ));
    }
    let body: serde_json::Value = resp
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("MiniMax: coding_plan HTTP {status}: {e}")))?
        .json()
        .map_err(|e| AppError::Internal(format!("MiniMax: coding_plan body parse: {e}")))?;

    // base_resp.status_code != 0 is a logical error, not a transport
    // one — surface it as Validation so the UI shows a friendly message.
    if let Some(base_resp) = body.get("base_resp") {
        if let Some(code) = base_resp.get("status_code").and_then(|v| v.as_i64()) {
            if code != 0 {
                let msg = base_resp
                    .get("status_msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("MiniMax API error");
                return Err(AppError::Validation(format!("MiniMax: {msg}")));
            }
        }
    }

    let remains_list = body
        .get("model_remains")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| AppError::Validation("MiniMax: missing model_remains".into()))?;
    let remains = remains_list
        .iter()
        .find(|item| {
            item.get("model_name")
                .and_then(serde_json::Value::as_str)
                .map(|s| s == "general")
                .unwrap_or(false)
        })
        .or_else(|| remains_list.first())
        .ok_or_else(|| AppError::Validation("MiniMax: no remains items".into()))?;

    let primary_pct_remaining = remains
        .get("current_interval_remaining_percent")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(100.0);
    let primary_used_pct = (100.0 - primary_pct_remaining).clamp(0.0, 100.0);
    let primary_resets_at = remains
        .get("end_time")
        .and_then(serde_json::Value::as_i64)
        .and_then(|ms| Utc.timestamp_opt(ms / 1000, 0).single());

    let primary = UsageWindow {
        label: "5h session".into(),
        used: Some(primary_used_pct),
        limit: Some(100.0),
        used_percent: Some(primary_used_pct),
        unit: Some("%".into()),
        resets_at: primary_resets_at.map(to_iso),
        reset_label: None,
    };

    let weekly = if let Some(weekly_pct_remaining) = remains
        .get("current_weekly_remaining_percent")
        .and_then(serde_json::Value::as_f64)
    {
        let weekly_used_pct = (100.0 - weekly_pct_remaining).clamp(0.0, 100.0);
        let weekly_resets_at = remains
            .get("weekly_end_time")
            .and_then(serde_json::Value::as_i64)
            .and_then(|ms| Utc.timestamp_opt(ms / 1000, 0).single());
        Some(UsageWindow {
            label: "Weekly".into(),
            used: Some(weekly_used_pct),
            limit: Some(100.0),
            used_percent: Some(weekly_used_pct),
            unit: Some("%".into()),
            resets_at: weekly_resets_at.map(to_iso),
            reset_label: None,
        })
    } else {
        None
    };

    Ok((primary, weekly))
}

/// Hits `/v1/billing/usage`. Returns (used percent, plan name, top models, spend).
fn fetch_billing(
    client: &reqwest::blocking::Client,
    base: &str,
    group_id: &str,
    api_key: &str,
) -> AppResult<(f64, Option<String>, Vec<(String, u64)>, Option<f64>)> {
    let url = format!("{}/v1/billing/usage?group_id={}", base, group_id);
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("MM-API-Source", "claude-config")
        .send()
        .map_err(|e| AppError::Internal(format!("MiniMax: billing request failed: {e}")))?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::Validation(
            "MiniMax: API key rejected (401/403)".into(),
        ));
    }
    let body: serde_json::Value = resp
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("MiniMax: billing HTTP {status}: {e}")))?
        .json()
        .map_err(|e| AppError::Internal(format!("MiniMax: billing body parse: {e}")))?;

    if let Some(base_resp) = body.get("base_resp") {
        if let Some(code) = base_resp.get("status_code").and_then(|v| v.as_i64()) {
            if code != 0 {
                let msg = base_resp
                    .get("status_msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("MiniMax API error");
                return Err(AppError::Validation(format!("MiniMax: {msg}")));
            }
        }
    }

    let used = body
        .get("used_amount")
        .or_else(|| body.get("total_amount"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let limit = body
        .get("total_quota")
        .or_else(|| body.get("quota"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(100.0);
    let used_pct = if limit > 0.0 { (used / limit) * 100.0 } else { 0.0 };

    let plan = body
        .get("plan_name")
        .or_else(|| body.get("current_plan_title"))
        .or_else(|| body.get("current_subscribe_title"))
        .or_else(|| body.get("combo_title"))
        .or_else(|| body.pointer("/current_combo_card/title"))
        .or_else(|| body.get("plan_type"))
        .or_else(|| body.get("type"))
        .and_then(serde_json::Value::as_str)
        .map(String::from);

    // Aggregate records if present, otherwise return an empty model list.
    // Records have shape: { consume_token, consume_input_token, consume_output_token,
    // consume_cash, consume_cash_after_voucher, ymd, method, model, result }.
    let records = body
        .get("charge_records")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut model_totals: HashMap<String, u64> = HashMap::new();
    let mut total_spend: f64 = 0.0;
    let mut have_spend = false;
    for raw in records {
        let r: MiniMaxBillingRecord = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !billing_record_succeeded(&r) {
            continue;
        }
        if let Some(model) = r.model.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let tokens = record_token_count(&r);
            *model_totals.entry(model.to_string()).or_insert(0) += tokens;
        }
        if let Some(cash) = record_cash(&r) {
            total_spend += cash;
            have_spend = true;
        }
    }
    let mut top: Vec<(String, u64)> = model_totals.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(5);
    let spend = if have_spend { Some(total_spend) } else { None };

    Ok((used_pct, plan, top, spend))
}

// -- Wire types for the billing records (permissive — MiniMax occasionally
//    ships a new field that breaks a strict deserializer). --

#[derive(Clone, Debug, Deserialize, Default)]
#[allow(non_snake_case)]
struct MiniMaxBillingRecord {
    #[serde(default)]
    consume_token: Option<serde_json::Value>,
    #[serde(default)]
    consume_input_token: Option<serde_json::Value>,
    #[serde(default)]
    consume_output_token: Option<serde_json::Value>,
    #[serde(default)]
    consume_cash: Option<serde_json::Value>,
    #[serde(default)]
    consume_cash_after_voucher: Option<serde_json::Value>,
    /// Method label (e.g. "chat"). Parsed for parity with the upstream
    /// codexbar schema; not currently surfaced in the Tracker UI.
    #[serde(default)]
    #[allow(dead_code)]
    method: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    status: Option<serde_json::Value>,
}

fn billing_record_succeeded(r: &MiniMaxBillingRecord) -> bool {
    let status = scalar_string(r.result.as_ref()).or_else(|| scalar_string(r.status.as_ref()));
    match status
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        None => true,
        Some(v) => v.eq_ignore_ascii_case("success") || v == "0" || v.eq_ignore_ascii_case("succeeded"),
    }
}

fn record_token_count(r: &MiniMaxBillingRecord) -> u64 {
    if let Some(n) = value_u64(r.consume_token.as_ref()) {
        if n > 0 {
            return n;
        }
    }
    value_u64(r.consume_input_token.as_ref()).unwrap_or(0)
        + value_u64(r.consume_output_token.as_ref()).unwrap_or(0)
}

fn record_cash(r: &MiniMaxBillingRecord) -> Option<f64> {
    value_f64(r.consume_cash_after_voucher.as_ref()).or_else(|| value_f64(r.consume_cash.as_ref()))
}

fn value_u64(v: Option<&serde_json::Value>) -> Option<u64> {
    match v? {
        serde_json::Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f as u64)),
        serde_json::Value::String(s) => s.trim().replace(',', "").parse().ok(),
        _ => None,
    }
}

fn value_f64(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().replace(',', "").parse().ok(),
        _ => None,
    }
}

fn scalar_string(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn to_iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(key: &str, group: &str, region: Option<&str>) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("api_key".into(), json!(key));
        m.insert("group_id".into(), json!(group));
        if let Some(r) = region {
            m.insert("region".into(), json!(r));
        }
        m
    }

    #[test]
    fn rejects_empty_or_missing() {
        let s = MiniMaxSource;
        assert!(s.validate_config(&cfg("", "g", None)).is_err());
        assert!(s.validate_config(&cfg("k", "", None)).is_err());
        assert!(s.validate_config(&serde_json::Map::new()).is_err());
    }

    #[test]
    fn accepts_well_formed() {
        let s = MiniMaxSource;
        s.validate_config(&cfg("sk-cp-XYZ", "123", None)).unwrap();
        s.validate_config(&cfg("sk-cp-XYZ", "123", Some("cn"))).unwrap();
        s.validate_config(&cfg("sk-cp-XYZ", "123", Some("GLOBAL"))).unwrap();
    }

    #[test]
    fn region_parses_aliases() {
        for v in ["cn", "china", "china-mainland", "china_mainland", "mainland"] {
            assert_eq!(MiniMaxRegion::from_settings_value(Some(v)), MiniMaxRegion::ChinaMainland);
        }
        for v in ["global", "io", "anything-else", ""] {
            assert_eq!(MiniMaxRegion::from_settings_value(Some(v)), MiniMaxRegion::Global);
        }
        assert_eq!(MiniMaxRegion::from_settings_value(None), MiniMaxRegion::Global);
    }

    #[test]
    fn region_urls_are_correct() {
        assert_eq!(MiniMaxRegion::Global.api_base_url(), "https://api.minimax.io");
        assert_eq!(MiniMaxRegion::ChinaMainland.api_base_url(), "https://api.minimaxi.com");
    }

    #[test]
    fn billing_record_filters_failures() {
        let ok = MiniMaxBillingRecord {
            consume_token: Some(json!(1000)),
            consume_input_token: None,
            consume_output_token: None,
            consume_cash: Some(json!(0.5)),
            consume_cash_after_voucher: None,
            method: Some("chat".into()),
            model: Some("MiniMax-M1".into()),
            result: Some(json!("SUCCESS")),
            status: None,
        };
        let failed = MiniMaxBillingRecord {
            result: Some(json!("FAILED")),
            ..ok.clone()
        };
        assert!(billing_record_succeeded(&ok));
        assert!(!billing_record_succeeded(&failed));
    }

    #[test]
    fn token_count_falls_back_to_in_plus_out() {
        let r = MiniMaxBillingRecord {
            consume_token: None,
            consume_input_token: Some(json!(300)),
            consume_output_token: Some(json!(500)),
            consume_cash: None,
            consume_cash_after_voucher: None,
            method: None,
            model: None,
            result: None,
            status: None,
        };
        assert_eq!(record_token_count(&r), 800);
    }
}
