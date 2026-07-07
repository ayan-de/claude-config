//! Subscription source — uses the `sessionKey` cookie from claude.ai to
//! fetch the usage page.
//!
//! Use this for `subscription`-kind providers. The user pastes the value
//! of their `sessionKey` cookie from claude.ai (DevTools → Application →
//! Cookies → https://claude.ai → sessionKey).
//!
//! ## Endpoint used
//!
//! `GET https://claude.ai/api/organizations/{org_id}/usage` with a
//! `Cookie: sessionKey=<value>` header. The response is a JSON blob
//! describing the current 5-hour and weekly rate-limit windows. We map
//! that onto `TrackerUsage`.
//!
//! The org id is read from the cookie's parent context — for v1 the user
//! provides it explicitly via the `org_id` field. (We could later read it
//! from `/api/organizations` but the explicit field is simpler and
//! documented.)
//!
//! ## Failure modes
//!
//! The session cookie is a long-lived secret — it can be invalidated by a
//! logout. The UI should surface a clear "session expired" hint when
//! fetch returns 401.

use chrono::Utc;
use serde::Deserialize;

use crate::models::{AppError, AppResult};
use crate::tracker::{SourceId, TrackerField, TrackerSource, TrackerUsage, UsageWindow};

const CLAUDE_AI_BASE: &str = "https://claude.ai";

pub struct SubscriptionSource;

impl TrackerSource for SubscriptionSource {
    fn id(&self) -> SourceId {
        SourceId::Subscription
    }

    fn display_name(&self) -> &'static str {
        "Claude.ai session"
    }

    fn description(&self) -> &'static str {
        "Uses your claude.ai sessionKey cookie to fetch the subscription usage page."
    }

    fn fields(&self) -> Vec<TrackerField> {
        vec![
            TrackerField {
                key: "session_key",
                label: "sessionKey cookie",
                placeholder: "sk-ant-sid01-...",
                secret: true,
                multiline: false,
                required: true,
                hint: Some("Found in DevTools → Application → Cookies → claude.ai → sessionKey. Stored in OS keyring."),
            },
            TrackerField {
                key: "org_id",
                label: "Organization ID",
                placeholder: "00000000-0000-0000-0000-000000000000",
                secret: false,
                multiline: false,
                required: true,
                hint: Some("The UUID shown in the URL when you're logged into claude.ai."),
            },
        ]
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["subscription"]
    }

    fn validate_config(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        let key = config
            .get("session_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("Subscription: missing `session_key` field".into()))?;
        if key.is_empty() {
            return Err(AppError::Validation("Subscription: session_key is empty".into()));
        }
        let org = config
            .get("org_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::Validation("Subscription: missing `org_id` field".into()))?;
        if org.is_empty() {
            return Err(AppError::Validation("Subscription: org_id is empty".into()));
        }
        Ok(())
    }

    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        self.validate_config(config)?;
        let session = config["session_key"].as_str().unwrap().trim();
        let org = config["org_id"].as_str().unwrap().trim();

        // claude.ai's usage endpoint is best-effort. If the schema changes
        // we degrade to "no data" rather than fail the whole refresh.
        let url = format!("{CLAUDE_AI_BASE}/api/organizations/{org}/usage");
        let resp = client
            .get(&url)
            .header("Cookie", format!("sessionKey={session}"))
            .header("Accept", "application/json")
            .send()
            .map_err(|e| AppError::Internal(format!("Subscription: request failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::Validation(
                "Subscription: session key rejected (401/403). Re-export your sessionKey cookie."
                    .into(),
            ));
        }
        let body: SubscriptionUsageResponse = resp
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("Subscription: HTTP {status}: {e}")))?
            .json()
            .map_err(|e| AppError::Internal(format!("Subscription: body parse: {e}")))?;

        // The exact shape of claude.ai's response has shifted historically.
        // We look for the well-known window keys ("five_hour", "seven_day")
        // and surface them as UsageWindow rows. Anything we don't
        // recognize is dropped silently.
        let mut windows = Vec::new();
        if let Some(w) = body.five_hour.as_ref() {
            windows.push(UsageWindow {
                label: "5-hour session".into(),
                used: w.utilization,
                limit: Some(100.0),
                used_percent: w.utilization,
                unit: Some("%".into()),
                resets_at: w.resets_at.clone(),
                reset_label: None,
            });
        }
        if let Some(w) = body.seven_day.as_ref() {
            windows.push(UsageWindow {
                label: "Weekly".into(),
                used: w.utilization,
                limit: Some(100.0),
                used_percent: w.utilization,
                unit: Some("%".into()),
                resets_at: w.resets_at.clone(),
                reset_label: None,
            });
        }

        Ok(TrackerUsage {
            windows,
            models: Vec::new(),
            cost_usd: None,
            fetched_at: Utc::now().to_rfc3339(),
            note: Some("Source: claude.ai usage page".into()),
        })
    }
}

#[derive(Debug, Deserialize, Default)]
struct SubscriptionUsageResponse {
    #[serde(default, alias = "five_hour")]
    five_hour: Option<WindowSlot>,
    #[serde(default, alias = "seven_day", alias = "sevenDay")]
    seven_day: Option<WindowSlot>,
}

#[derive(Debug, Deserialize, Default)]
struct WindowSlot {
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default, alias = "resets_at")]
    resets_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(key: &str, org: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("session_key".into(), json!(key));
        m.insert("org_id".into(), json!(org));
        m
    }

    #[test]
    fn rejects_empty_fields() {
        let s = SubscriptionSource;
        assert!(s.validate_config(&cfg("", "org")).is_err());
        assert!(s.validate_config(&cfg("key", "")).is_err());
    }

    #[test]
    fn rejects_missing_fields() {
        let s = SubscriptionSource;
        assert!(s.validate_config(&serde_json::Map::new()).is_err());
    }

    #[test]
    fn accepts_well_formed() {
        let s = SubscriptionSource;
        s.validate_config(&cfg("sk-ant-sid01-XXXX", "00000000-0000-0000-0000-000000000000"))
            .unwrap();
    }
}
