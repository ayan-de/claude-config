//! FreeModel (freemodel.dev) tracker source.
//!
//! Adapted from `codexbar-desktop/backend/src/providers/freemodel/mod.rs`.
//! Uses a `bm_session` cookie as the auth credential.
//!
//! ## Endpoints
//!
//! 1. `GET /api/usage` — returns total requests, total tokens, and the
//!    5h + weekly rate-limit windows. This is the primary call.
//! 2. `GET /api/billing` — returns the credit balance + subscription info
//!    (used to label the "note" line). Best-effort.
//!
//! Both endpoints are authenticated via a `Cookie: bm_session=<value>`
//! header.

use chrono::{TimeZone, Utc};
use serde::Deserialize;

use crate::models::{AppError, AppResult};
use crate::tracker::{SourceId, TrackerField, TrackerSource, TrackerUsage, UsageWindow};

const FREEMODEL_API_BASE: &str = "https://freemodel.dev";

pub struct FreeModelSource;

impl TrackerSource for FreeModelSource {
    fn id(&self) -> SourceId {
        SourceId::FreeModel
    }

    fn display_name(&self) -> &'static str {
        "FreeModel"
    }

    fn description(&self) -> &'static str {
        "Tracks FreeModel.dev 5h + weekly rate limits using a bm_session cookie."
    }

    fn fields(&self) -> Vec<TrackerField> {
        vec![TrackerField {
            key: "bm_session",
            label: "bm_session cookie",
            placeholder: "bm_session=...",
            secret: true,
            multiline: false,
            required: true,
            hint: Some("DevTools → Application → Cookies → freemodel.dev → bm_session. Stored in OS keyring."),
        }]
    }

    /// FreeModel is configured as a `custom` provider (with a
    /// `cc.freemodel.dev` base URL). The auto-pick narrows further by
    /// name/URL match.
    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["custom"]
    }

    fn validate_config(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        let raw = config
            .get("bm_session")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("FreeModel: missing `bm_session` field".into()))?;
        if raw.is_empty() {
            return Err(AppError::Validation("FreeModel: bm_session is empty".into()));
        }
        Ok(())
    }

    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        self.validate_config(config)?;
        // The FreeModel API expects the cookie VALUE only — we wrap it
        // in `Cookie: bm_session=<value>` server-side.
        let bm_session = config["bm_session"].as_str().unwrap().trim();

        // Primary call. Failures here abort the whole fetch — without
        // rate-limit data the tracker has nothing to show.
        let usage = fetch_usage_data(client, bm_session)?;
        // Billing is best-effort. A failure here degrades to a smaller
        // snapshot (no plan name) rather than an error.
        let billing = fetch_billing_data(client, bm_session).unwrap_or_default();

        let mut windows = Vec::new();

        // 5h window. limit_cents is the per-window spend cap; used/limit
        // gives a 0..1 fraction that we surface as 0..100%.
        if usage.window_5h.limit_cents > 0.0 {
            let pct = (usage.window_5h.used_cents / usage.window_5h.limit_cents) * 100.0;
            let resets_at = seconds_to_iso(usage.window_5h.resets_at);
            windows.push(UsageWindow {
                label: "5h session".into(),
                used: Some(pct),
                limit: Some(100.0),
                used_percent: Some(pct),
                unit: Some("%".into()),
                resets_at,
                reset_label: None,
            });
        }

        // Weekly window — same shape.
        if usage.window_week.limit_cents > 0.0 {
            let pct = (usage.window_week.used_cents / usage.window_week.limit_cents) * 100.0;
            let resets_at = seconds_to_iso(usage.window_week.resets_at);
            windows.push(UsageWindow {
                label: "Weekly".into(),
                used: Some(pct),
                limit: Some(100.0),
                used_percent: Some(pct),
                unit: Some("%".into()),
                resets_at,
                reset_label: None,
            });
        }

        // Fall back to lifetime totals if no windows are present. This
        // happens for free / unlimited plans.
        if windows.is_empty() {
            if usage.total_tokens > 0 {
                windows.push(UsageWindow {
                    label: "Total tokens".into(),
                    used: Some(usage.total_tokens as f64),
                    limit: None,
                    used_percent: None,
                    unit: Some("tokens".into()),
                    resets_at: None,
                    reset_label: None,
                });
            }
            if usage.total_requests > 0 {
                windows.push(UsageWindow {
                    label: "Total requests".into(),
                    used: Some(usage.total_requests as f64),
                    limit: None,
                    used_percent: None,
                    unit: Some("requests".into()),
                    resets_at: None,
                    reset_label: None,
                });
            }
        }

        // Credit-balance window — informational, not a progress bar.
        if billing.credit_cents > 0.0 {
            windows.push(UsageWindow {
                label: "Credit balance".into(),
                used: Some(billing.credit_cents / 100.0),
                limit: None,
                used_percent: None,
                unit: Some("USD".into()),
                resets_at: None,
                reset_label: None,
            });
        }

        if windows.is_empty() {
            return Err(AppError::Validation(
                "FreeModel: no usage data — check that the bm_session cookie is valid".into(),
            ));
        }

        let note = billing
            .subscription
            .as_ref()
            .map(|s| s.plan_id.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| Some("FreeModel".into()));

        Ok(TrackerUsage {
            windows,
            models: Vec::new(),
            cost_usd: Some(billing.credit_cents / 100.0),
            fetched_at: Utc::now().to_rfc3339(),
            note,
        })
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn fetch_usage_data(client: &reqwest::blocking::Client, bm_session: &str) -> AppResult<UsageResponse> {
    let url = format!("{FREEMODEL_API_BASE}/api/usage");
    let resp = client
        .get(&url)
        .header("Cookie", format!("bm_session={bm_session}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| AppError::Internal(format!("FreeModel: usage request failed: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::Validation(
            "FreeModel: bm_session rejected (401/403). Re-export the cookie.".into(),
        ));
    }
    let bytes = resp
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("FreeModel: usage HTTP {status}: {e}")))?
        .bytes()
        .map_err(|e| AppError::Internal(format!("FreeModel: read usage body: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Validation(format!("FreeModel: usage parse: {e}")))
}

fn fetch_billing_data(client: &reqwest::blocking::Client, bm_session: &str) -> AppResult<BillingResponse> {
    let url = format!("{FREEMODEL_API_BASE}/api/billing");
    let resp = client
        .get(&url)
        .header("Cookie", format!("bm_session={bm_session}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| AppError::Internal(format!("FreeModel: billing request failed: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::Validation(
            "FreeModel: bm_session rejected (401/403)".into(),
        ));
    }
    let bytes = resp
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("FreeModel: billing HTTP {status}: {e}")))?
        .bytes()
        .map_err(|e| AppError::Internal(format!("FreeModel: read billing body: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Validation(format!("FreeModel: billing parse: {e}")))
}

fn seconds_to_iso(secs: u64) -> Option<String> {
    if secs == 0 {
        return None;
    }
    Utc.timestamp_opt(secs as i64, 0).single().map(|d| d.to_rfc3339())
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct UsageResponse {
    #[serde(default)]
    total_requests: u64,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    window_5h: WindowData,
    #[serde(default)]
    window_week: WindowData,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct WindowData {
    #[serde(default)]
    used_cents: f64,
    #[serde(default)]
    limit_cents: f64,
    #[serde(default)]
    resets_at: u64,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BillingResponse {
    #[serde(default)]
    credit_cents: f64,
    #[serde(default)]
    subscription: Option<Subscription>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Subscription {
    #[serde(default)]
    plan_id: String,
    /// Subscription status ("active", "trialing", etc). Parsed for
    /// parity with the upstream codexbar schema; the Tracker UI
    /// currently shows the plan name only.
    #[serde(default)]
    #[allow(dead_code)]
    status: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(s: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("bm_session".into(), json!(s));
        m
    }

    #[test]
    fn rejects_empty_or_missing() {
        let s = FreeModelSource;
        assert!(s.validate_config(&cfg("")).is_err());
        assert!(s.validate_config(&serde_json::Map::new()).is_err());
    }

    #[test]
    fn accepts_well_formed() {
        let s = FreeModelSource;
        s.validate_config(&cfg("bm_session=abc")).unwrap();
        s.validate_config(&cfg("abc")).unwrap(); // user might paste just the value
    }

    #[test]
    fn parses_usage_response_camel_case() {
        let raw = r#"{
            "totalRequests": 110,
            "totalTokens": 4607446,
            "window5h": {"usedCents": 184, "limitCents": 1000, "resetsAt": 1782506376},
            "windowWeek": {"usedCents": 1189, "limitCents": 6667, "resetsAt": 1783022779}
        }"#;
        let u: UsageResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(u.total_requests, 110);
        assert_eq!(u.total_tokens, 4607446);
        assert_eq!(u.window_5h.used_cents, 184.0);
        assert_eq!(u.window_5h.limit_cents, 1000.0);
        // 1189/6667 ≈ 17.83%
        let pct = (u.window_week.used_cents / u.window_week.limit_cents) * 100.0;
        assert!((pct - 17.83).abs() < 0.1);
    }

    #[test]
    fn parses_billing_response_with_subscription() {
        let raw = r#"{
            "creditCents": 500,
            "subscription": {"planId": "pro-monthly", "status": "active"}
        }"#;
        let b: BillingResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(b.credit_cents, 500.0);
        assert_eq!(b.subscription.unwrap().plan_id, "pro-monthly");
    }

    #[test]
    fn seconds_to_iso_zero_is_none() {
        assert!(seconds_to_iso(0).is_none());
        let iso = seconds_to_iso(1_782_506_376).unwrap();
        assert!(iso.starts_with("2026")); // unix ~mid-2026
    }
}
