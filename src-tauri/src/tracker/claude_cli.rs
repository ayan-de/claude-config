//! Claude Code CLI source — reads the CLI's own OAuth login from
//! `~/.claude/.credentials.json` (via the existing storage helper, which
//! honors `CLAUDE_CONFIG_DIR`) and fetches subscription usage from
//! Anthropic's OAuth usage API. Zero configuration: nothing to paste.
//!
//! ## Endpoint used
//!
//! `GET https://api.anthropic.com/api/oauth/usage` with
//! `Authorization: Bearer <accessToken>` and the
//! `anthropic-beta: oauth-2025-04-20` header. The response carries
//! utilization percentages for the 5-hour and weekly rate-limit windows.
//!
//! ## Failure modes
//!
//! - No CLI login → tell the user to run `claude /login`.
//! - Expired access token → the CLI refreshes it itself; tell the user to
//!   open Claude Code. We deliberately do not implement token refresh.

use chrono::Utc;
use serde::Deserialize;

use crate::models::{AppError, AppResult};
use crate::storage::read_credentials_oauth;
use crate::tracker::{SourceId, TrackerField, TrackerSource, TrackerUsage, UsageWindow};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

pub struct ClaudeCliSource;

impl TrackerSource for ClaudeCliSource {
    fn id(&self) -> SourceId {
        SourceId::ClaudeCli
    }

    fn display_name(&self) -> &'static str {
        "Claude Code CLI"
    }

    fn description(&self) -> &'static str {
        "Reads your Claude Code login and fetches subscription usage from the OAuth API. No configuration needed."
    }

    fn fields(&self) -> Vec<TrackerField> {
        Vec::new()
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["subscription"]
    }

    fn validate_config(
        &self,
        _config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        Ok(())
    }

    fn fetch_usage(
        &self,
        _config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        let oauth = read_credentials_oauth()?.ok_or_else(|| {
            AppError::Validation(
                "Claude Code CLI: no OAuth login found — run `claude /login` in a terminal first"
                    .into(),
            )
        })?;
        let token = access_token(&oauth)?;

        let resp = client
            .get(USAGE_URL)
            .header("Authorization", format!("Bearer {token}"))
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("Accept", "application/json")
            .send()
            .map_err(|e| AppError::Internal(format!("Claude Code CLI: request failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::Validation(
                "Claude Code CLI: token rejected (401/403). Re-login with `claude /login`.".into(),
            ));
        }
        let body: OauthUsageResponse = resp
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("Claude Code CLI: HTTP {status}: {e}")))?
            .json()
            .map_err(|e| AppError::Internal(format!("Claude Code CLI: body parse: {e}")))?;

        Ok(usage_from_response(body))
    }
}

/// Pull a usable access token out of the `claudeAiOauth` blob, rejecting
/// missing/empty/expired tokens with actionable messages.
fn access_token(oauth: &serde_json::Value) -> AppResult<String> {
    let token = oauth
        .get("accessToken")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            AppError::Validation(
                "Claude Code CLI: credentials file has no access token — run `claude /login`"
                    .into(),
            )
        })?;
    // `expiresAt` is milliseconds since epoch. The CLI refreshes its own
    // token; we just refuse to use a stale one.
    if let Some(expires_ms) = oauth.get("expiresAt").and_then(serde_json::Value::as_f64) {
        if expires_ms <= Utc::now().timestamp_millis() as f64 {
            return Err(AppError::Validation(
                "Claude Code CLI: access token expired — open Claude Code to refresh it".into(),
            ));
        }
    }
    Ok(token.to_string())
}

fn usage_from_response(body: OauthUsageResponse) -> TrackerUsage {
    let mut windows = Vec::new();
    let mut push = |label: &str, slot: &Option<WindowSlot>| {
        if let Some(w) = slot {
            windows.push(UsageWindow {
                label: label.into(),
                used: w.utilization,
                limit: Some(100.0),
                used_percent: w.utilization,
                unit: Some("%".into()),
                resets_at: w.resets_at.clone(),
                reset_label: None,
            });
        }
    };
    push("5-hour session", &body.five_hour);
    push("Weekly", &body.seven_day);
    push("Weekly (Sonnet)", &body.seven_day_sonnet);
    push("Weekly (Opus)", &body.seven_day_opus);

    TrackerUsage {
        windows,
        models: Vec::new(),
        cost_usd: body.extra_usage.as_ref().and_then(|e| e.used_credits),
        fetched_at: Utc::now().to_rfc3339(),
        note: Some("Source: Claude Code CLI login".into()),
    }
}

#[derive(Debug, Deserialize, Default)]
struct OauthUsageResponse {
    #[serde(default, rename = "fiveHour", alias = "five_hour")]
    five_hour: Option<WindowSlot>,
    #[serde(default, rename = "sevenDay", alias = "seven_day")]
    seven_day: Option<WindowSlot>,
    #[serde(default, rename = "sevenDaySonnet", alias = "seven_day_sonnet")]
    seven_day_sonnet: Option<WindowSlot>,
    #[serde(default, rename = "sevenDayOpus", alias = "seven_day_opus")]
    seven_day_opus: Option<WindowSlot>,
    #[serde(default, rename = "extraUsage", alias = "extra_usage")]
    extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize, Default)]
struct WindowSlot {
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default, rename = "resetsAt", alias = "resets_at")]
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ExtraUsage {
    #[serde(default, rename = "usedCredits", alias = "used_credits")]
    used_credits: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_camel_case_windows() {
        let body: OauthUsageResponse = serde_json::from_value(json!({
            "fiveHour": { "utilization": 42.5, "resetsAt": "2026-07-13T10:00:00Z" },
            "sevenDay": { "utilization": 12.0 },
            "sevenDayOpus": { "utilization": 3.0 },
            "extraUsage": { "usedCredits": 1.25 }
        }))
        .unwrap();
        let usage = usage_from_response(body);
        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].label, "5-hour session");
        assert_eq!(usage.windows[0].used_percent, Some(42.5));
        assert_eq!(
            usage.windows[0].resets_at.as_deref(),
            Some("2026-07-13T10:00:00Z")
        );
        assert_eq!(usage.windows[1].label, "Weekly");
        assert_eq!(usage.windows[2].label, "Weekly (Opus)");
        assert_eq!(usage.cost_usd, Some(1.25));
    }

    #[test]
    fn maps_snake_case_windows() {
        let body: OauthUsageResponse = serde_json::from_value(json!({
            "five_hour": { "utilization": 10.0, "resets_at": "2026-07-13T10:00:00Z" },
            "seven_day_sonnet": { "utilization": 5.0 }
        }))
        .unwrap();
        let usage = usage_from_response(body);
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[1].label, "Weekly (Sonnet)");
    }

    #[test]
    fn unknown_keys_dropped_silently() {
        let body: OauthUsageResponse =
            serde_json::from_value(json!({ "somethingNew": { "utilization": 1.0 } })).unwrap();
        let usage = usage_from_response(body);
        assert!(usage.windows.is_empty());
        assert!(usage.cost_usd.is_none());
    }

    #[test]
    fn rejects_missing_token() {
        let err = access_token(&json!({ "scopes": ["user:profile"] })).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn rejects_empty_token() {
        let err = access_token(&json!({ "accessToken": "  " })).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn rejects_expired_token() {
        let err = access_token(&json!({
            "accessToken": "sk-ant-oat01-x",
            "expiresAt": 1_000_i64
        }))
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn accepts_live_token() {
        let future_ms = (Utc::now().timestamp_millis() + 3_600_000) as f64;
        let token = access_token(&json!({
            "accessToken": "sk-ant-oat01-x",
            "expiresAt": future_ms
        }))
        .unwrap();
        assert_eq!(token, "sk-ant-oat01-x");
    }
}
