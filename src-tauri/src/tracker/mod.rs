//! Tracker source registry.
//!
//! Each "source" is a self-contained adapter that knows how to:
//!  1. Render its form fields to the UI (declared statically, not built from
//!     per-provider kind — different custom relays have different shapes).
//!  2. Validate a user-supplied config blob.
//!  3. Fetch a usage snapshot for a given config (HTTP or no-op).
//!
//! Adding a new source = one new file in this directory + a single
//! `register_source(...)` line in the `sources` initializer below. Nothing
//! in the storage, command, or UI layers changes — the registry is the
//! only thing that knows which sources exist.
//!
//! ## Source identity
//!
//! Sources are identified by a stable string id (e.g. `"anthropic_admin"`).
//! That id is what's persisted in `trackers.json` and what the frontend
//! uses to look up the field schema. Renaming a source id is a breaking
//! change — old configs would orphan.
//!
//! ## Config blob
//!
//! The user's pasted fields are stored as a `serde_json::Map<String, Value>`
//! rather than a per-source struct. This keeps the storage layer source-
//! agnostic and means new fields can be added to an existing source
//! without a migration. Validation happens at adapter level.

use std::collections::HashMap;
use std::sync::Arc;

use crate::models::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub mod anthropic_admin;
pub mod anthropic_compat;
pub mod claude_cli;
pub mod freemodel;
pub mod manual_json;
pub mod minimax;
pub mod subscription;

pub use anthropic_admin::AnthropicAdminSource;
pub use anthropic_compat::AnthropicCompatSource;
pub use claude_cli::ClaudeCliSource;
pub use freemodel::FreeModelSource;
pub use manual_json::ManualJsonSource;
pub use minimax::MiniMaxSource;
pub use subscription::SubscriptionSource;

/// Stable identifier for a source. Persisted in `trackers.json`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SourceId {
    AnthropicAdmin,
    AnthropicCompat,
    ClaudeCli,
    Subscription,
    ManualJson,
    MiniMax,
    FreeModel,
}

impl SourceId {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceId::AnthropicAdmin => "anthropic_admin",
            SourceId::AnthropicCompat => "anthropic_compat",
            SourceId::ClaudeCli => "claude_cli",
            SourceId::Subscription => "subscription",
            SourceId::ManualJson => "manual_json",
            SourceId::MiniMax => "minimax",
            SourceId::FreeModel => "freemodel",
        }
    }

    /// Parse from the on-disk string. Returns `Validation` on unknown id so
    /// a typo in `trackers.json` surfaces as a clear error rather than
    /// silently dropping the row.
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "anthropic_admin" => Ok(SourceId::AnthropicAdmin),
            "anthropic_compat" => Ok(SourceId::AnthropicCompat),
            "claude_cli" => Ok(SourceId::ClaudeCli),
            "subscription" => Ok(SourceId::Subscription),
            "manual_json" => Ok(SourceId::ManualJson),
            "minimax" => Ok(SourceId::MiniMax),
            "freemodel" => Ok(SourceId::FreeModel),
            other => Err(AppError::Validation(format!(
                "unknown tracker source id: {other:?}"
            ))),
        }
    }
}

/// One form input. Drives the dynamic form in the Tracker tab — the UI
/// calls `get_tracker_sources_cmd` and renders one input per field.
#[derive(Debug, Clone, Serialize)]
pub struct TrackerField {
    /// JSON key the value is stored under.
    pub key: &'static str,
    /// Human label for the input.
    pub label: &'static str,
    /// Placeholder text shown when empty.
    pub placeholder: &'static str,
    /// `true` for keys/cookies — value goes to OS keyring, not the JSON
    /// config blob. The UI also masks the input.
    pub secret: bool,
    /// Multiline text input. Useful for the ManualJson source where the
    /// user pastes a multi-line JSON blob.
    pub multiline: bool,
    /// Required for a valid config. The form disables Save until all
    /// required fields are filled.
    pub required: bool,
    /// Short hint shown under the field. Markdown is NOT supported.
    pub hint: Option<&'static str>,
}

/// A window in the usage snapshot (e.g. "5-hour", "weekly"). All sources
/// normalize to this shape regardless of their native API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageWindow {
    /// Display label, e.g. "Session", "Weekly", "Opus".
    pub label: String,
    /// Used portion, 0..=limit when both are known.
    pub used: Option<f64>,
    /// Total quota, when known.
    pub limit: Option<f64>,
    /// 0..100, computed when used/limit are both known. The UI uses this
    /// directly in the progress bar.
    pub used_percent: Option<f64>,
    /// "requests", "tokens", "usd", etc. — falls back to "units" in the UI
    /// when None.
    pub unit: Option<String>,
    /// ISO-8601 timestamp the window resets, if the source knows.
    pub resets_at: Option<String>,
    /// Human-readable reset hint ("in 2h 14m"), when the source provides
    /// one. The UI prefers this over the raw timestamp.
    pub reset_label: Option<String>,
}

/// Per-model usage row. Sources that don't break down by model can return
/// an empty list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

/// The normalized output every source returns. The UI doesn't care which
/// source produced it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TrackerUsage {
    /// Quota windows (session/weekly/etc). Empty when the source has no
    /// quota concept.
    #[serde(default)]
    pub windows: Vec<UsageWindow>,
    /// Per-model breakdown. Empty when not applicable.
    #[serde(default)]
    pub models: Vec<ModelUsage>,
    /// Total spend over the source's reporting period, when known.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// ISO-8601 timestamp the snapshot was taken at.
    #[serde(default)]
    pub fetched_at: String,
    /// Free-form note from the source — surfaced under the windows.
    #[serde(default)]
    pub note: Option<String>,
}

/// A single, immutable source adapter. Sources are stateless and cheap to
/// share — the registry holds them as `Arc<dyn TrackerSource>`.
pub trait TrackerSource: Send + Sync {
    fn id(&self) -> SourceId;
    fn display_name(&self) -> &'static str;
    /// One-line description shown in the source picker.
    fn description(&self) -> &'static str;
    /// Form schema. Order in the returned Vec is the render order in the UI.
    fn fields(&self) -> Vec<TrackerField>;
    /// Which provider kinds this source applies to. The Tracker tab
    /// uses this to filter the picker and to show a "coming soon"
    /// panel when no source matches the current provider's kind.
    ///
    /// "custom" sources (AnthropicCompat, MiniMax, FreeModel) are
    /// intentionally scoped to `["custom"]` — the auto-pick narrows
    /// further based on name/URL matching. Sources that should be
    /// offered for *every* kind (e.g. ManualJson) list all five.
    fn applicable_kinds(&self) -> &'static [&'static str];
    /// Validate a config blob. Called before save and on every refresh.
    fn validate_config(&self, config: &serde_json::Map<String, serde_json::Value>) -> AppResult<()>;
    /// Fetch a usage snapshot. Implementations may do HTTP I/O; the
    /// command handler invokes this on a blocking thread.
    fn fetch_usage(
        &self,
        config: &serde_json::Map<String, serde_json::Value>,
        client: &reqwest::blocking::Client,
    ) -> AppResult<TrackerUsage>;
}

/// Static registry. Built once at startup; never mutated.
pub struct SourceRegistry {
    by_id: HashMap<SourceId, Arc<dyn TrackerSource>>,
}

impl SourceRegistry {
    /// Build the registry with the v1 sources. New sources get one line
    /// here.
    pub fn new() -> Self {
        let sources: Vec<Arc<dyn TrackerSource>> = vec![
            Arc::new(AnthropicAdminSource),
            Arc::new(AnthropicCompatSource),
            Arc::new(ClaudeCliSource),
            Arc::new(SubscriptionSource),
            Arc::new(MiniMaxSource),
            Arc::new(FreeModelSource),
            Arc::new(ManualJsonSource),
        ];
        let mut by_id = HashMap::with_capacity(sources.len());
        for s in sources {
            by_id.insert(s.id(), s);
        }
        Self { by_id }
    }

    /// All sources, in deterministic order. The UI uses this to render the
    /// source picker.
    pub fn list(&self) -> Vec<SourceDescriptor> {
        let mut ids = [
            SourceId::AnthropicAdmin,
            SourceId::AnthropicCompat,
            SourceId::ClaudeCli,
            SourceId::Subscription,
            SourceId::MiniMax,
            SourceId::FreeModel,
            SourceId::ManualJson,
        ];
        ids.sort_by_key(|i| i.as_str());
        ids.into_iter()
            .filter_map(|id| self.by_id.get(&id).map(|s| s.descriptor()))
            .collect()
    }

    pub fn get(&self, id: SourceId) -> Option<Arc<dyn TrackerSource>> {
        self.by_id.get(&id).cloned()
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight description of a source — what the UI needs to render the
/// picker. Includes the field schema so the UI gets everything in one
/// command.
#[derive(Debug, Clone, Serialize)]
pub struct SourceDescriptor {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub fields: Vec<TrackerField>,
    /// Provider kinds this source applies to. The UI uses this to
    /// filter the picker and to show a "coming soon" panel when the
    /// current provider has no supported source.
    pub applicable_kinds: Vec<String>,
}

impl dyn TrackerSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id().as_str().to_string(),
            display_name: self.display_name().to_string(),
            description: self.description().to_string(),
            fields: self.fields(),
            applicable_kinds: self.applicable_kinds().iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_all_v1_sources() {
        let r = SourceRegistry::new();
        let ids: Vec<String> = r.list().into_iter().map(|s| s.id).collect();
        assert!(ids.iter().any(|s| s == "anthropic_admin"));
        assert!(ids.iter().any(|s| s == "anthropic_compat"));
        assert!(ids.iter().any(|s| s == "claude_cli"));
        assert!(ids.iter().any(|s| s == "subscription"));
        assert!(ids.iter().any(|s| s == "manual_json"));
        assert!(ids.iter().any(|s| s == "minimax"));
        assert!(ids.iter().any(|s| s == "freemodel"));
    }

    #[test]
    fn registry_get_returns_matching_source() {
        let r = SourceRegistry::new();
        let s = r.get(SourceId::AnthropicAdmin).unwrap();
        assert_eq!(s.id(), SourceId::AnthropicAdmin);
    }

    #[test]
    fn source_id_round_trips() {
        for id in [
            SourceId::AnthropicAdmin,
            SourceId::AnthropicCompat,
            SourceId::ClaudeCli,
            SourceId::Subscription,
            SourceId::ManualJson,
            SourceId::MiniMax,
            SourceId::FreeModel,
        ] {
            assert_eq!(SourceId::parse(id.as_str()).unwrap(), id);
        }
    }

    #[test]
    fn every_source_declares_applicable_kinds() {
        // Sanity: no source forgot to implement applicable_kinds (the
        // filter would otherwise drop the source from every picker).
        let r = SourceRegistry::new();
        for s in r.list() {
            assert!(
                !s.applicable_kinds.is_empty(),
                "source {} has no applicable_kinds",
                s.id
            );
        }
    }

    #[test]
    fn kind_to_sources_mapping_covers_what_the_app_supports() {
        // Each provider kind in the main `ProviderKind` enum should
        // have at least one applicable source. Bedrock/Vertex fall
        // through to manual_json, which lists every kind.
        let r = SourceRegistry::new();
        for kind in ["subscription", "console", "custom", "bedrock", "vertex"] {
            let matching: Vec<String> = r
                .list()
                .into_iter()
                .filter(|s| s.applicable_kinds.iter().any(|k| k == kind))
                .map(|s| s.id)
                .collect();
            assert!(
                !matching.is_empty(),
                "no tracker source for provider kind {kind}"
            );
        }
    }

    #[test]
    fn source_id_parse_rejects_unknown() {
        let err = SourceId::parse("nope").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
