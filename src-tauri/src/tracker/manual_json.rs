//! ManualJson source — no network, user pastes a JSON usage payload.
//!
//! Useful as a universal fallback for any provider we don't have a real
//! adapter for, and as a way to prototype the Tracker tab UI without
//! needing API credentials.
//!
//! ## Accepted JSON shape
//!
//! The pasted JSON must deserialize to a `TrackerUsage` directly. Example:
//!
//! ```json
//! {
//!   "windows": [
//!     {"label": "5h session", "used": 12, "limit": 100, "used_percent": 12, "unit": "requests", "resets_at": "2026-07-08T05:00:00Z"},
//!     {"label": "Weekly", "used": 240, "limit": 1000, "used_percent": 24, "unit": "messages"}
//!   ],
//!   "models": [
//!     {"model": "claude-sonnet-4-6", "input_tokens": 12345, "output_tokens": 6789, "cost_usd": 0.42}
//!   ],
//!   "cost_usd": 12.34,
//!   "note": "Pasted 2026-07-08 by user"
//! }
//! ```
//!
//! `used_percent` is optional; if missing on a window, the UI computes it
//! from `used` / `limit`. `fetched_at` is overwritten with the current
//! timestamp at paste time.

use crate::models::{AppError, AppResult};
use crate::tracker::{TrackerField, TrackerSource, TrackerUsage, SourceId};
use chrono::Utc;

pub struct ManualJsonSource;

impl TrackerSource for ManualJsonSource {
    fn id(&self) -> SourceId {
        SourceId::ManualJson
    }

    fn display_name(&self) -> &'static str {
        "Manual JSON"
    }

    fn description(&self) -> &'static str {
        "Paste a usage JSON snapshot. No network call — useful as a universal fallback."
    }

    fn fields(&self) -> Vec<TrackerField> {
        vec![TrackerField {
            key: "payload",
            label: "Usage JSON",
            placeholder: r#"{"windows":[{"label":"5h","used":12,"limit":100}],"models":[]}"#,
            secret: false,
            multiline: true,
            required: true,
            hint: Some("Must be a JSON object with optional `windows[]`, `models[]`, `cost_usd`, and `note` keys."),
        }]
    }

    /// Manual JSON works for every provider kind — it's a universal
    /// fallback. The UI prefers a kind-specific source when one matches,
    /// but offers this as the last resort.
    fn applicable_kinds(&self) -> &'static [&'static str] {
        &[
            "subscription",
            "console",
            "custom",
            "bedrock",
            "vertex",
        ]
    }

    fn validate_config(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
    ) -> AppResult<()> {
        let raw = config
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AppError::Validation("ManualJson: missing `payload` field".into()))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(AppError::Validation(
                "ManualJson: payload is empty".into(),
            ));
        }
        // Parse but don't keep — just surface a shape error if the JSON is bad.
        let _: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| AppError::Validation(format!("ManualJson: not valid JSON: {e}")))?;
        Ok(())
    }

    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        _client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage> {
        let raw = config
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AppError::Validation("ManualJson: missing `payload` field".into()))?;
        let mut usage: TrackerUsage = serde_json::from_str(raw.trim())
            .map_err(|e| AppError::Validation(format!("ManualJson: cannot parse as TrackerUsage: {e}. Expected keys: windows, models, cost_usd, note.")))?;
        // Stamp the fetch time so the UI can show "last pasted at" honestly.
        if usage.fetched_at.is_empty() {
            usage.fetched_at = Utc::now().to_rfc3339();
        }
        Ok(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(payload: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        m.insert("payload".into(), json!(payload));
        m
    }

    #[test]
    fn rejects_empty_payload() {
        let s = ManualJsonSource;
        assert!(s.validate_config(&cfg("")).is_err());
        assert!(s.validate_config(&cfg("   \n  ")).is_err());
    }

    #[test]
    fn rejects_invalid_json() {
        let s = ManualJsonSource;
        let err = s.validate_config(&cfg("{not json")).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn rejects_missing_payload_key() {
        let s = ManualJsonSource;
        let err = s.validate_config(&serde_json::Map::new()).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn accepts_valid_payload() {
        let s = ManualJsonSource;
        let payload = r#"{"windows":[{"label":"5h","used":10,"limit":100,"used_percent":10}]}"#;
        s.validate_config(&cfg(payload)).unwrap();
    }

    #[test]
    fn fetch_returns_usage_with_fetched_at() {
        let s = ManualJsonSource;
        let payload = r#"{"windows":[{"label":"5h","used":10,"limit":100,"used_percent":10}],"cost_usd":1.5}"#;
        let u = s.fetch_usage(&cfg(payload), &noop_client()).unwrap();
        assert_eq!(u.windows.len(), 1);
        assert_eq!(u.cost_usd, Some(1.5));
        assert!(!u.fetched_at.is_empty());
    }

    fn noop_client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::new()
    }
}
