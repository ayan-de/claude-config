use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical Claude Code env keys across every supported provider kind.
/// `merge_env` uses this set to decide what to unset when swapping providers:
/// any canonical key the new provider doesn't emit is stripped from
/// `settings.json`, giving effortless mode-switching for free.
pub const CANONICAL_ENV_KEYS: &[&str] = &[
    // Anthropic direct + custom relay
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    // Model overrides (any kind)
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    // Misc
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    // Bedrock
    "CLAUDE_CODE_USE_BEDROCK",
    "AWS_REGION",
    "AWS_PROFILE",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    // Vertex
    "CLAUDE_CODE_USE_VERTEX",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLOUD_ML_REGION",
    "GOOGLE_APPLICATION_CREDENTIALS",
];

/// Which login method a provider represents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// OAuth via a Claude subscription (Pro/Max/Team/Enterprise). Session token
    /// lives in `~/.claude/.credentials.json`, not in env vars.
    Subscription,
    /// Anthropic Console API key. `ANTHROPIC_API_KEY` in env.
    Console,
    /// Third-party relay / proxy exposing the Anthropic API shape.
    /// `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` in env.
    Custom,
    /// Amazon Bedrock. `CLAUDE_CODE_USE_BEDROCK=1` + AWS credentials.
    Bedrock,
    /// Google Vertex AI. `CLAUDE_CODE_USE_VERTEX=1` + project id + region.
    Vertex,
}

/// Metadata for a saved provider. Secrets (OAuth blobs, API keys, AWS access
/// keys) live separately in the OS keyring under
/// (service, account) = (KEYRING_SERVICE, id) and are never persisted here.
///
/// Which of the optional kind-specific fields are populated depends on `kind`;
/// see the wizard in `ProviderForm` and validation in `commands::providers`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: String,
    pub name: String,

    /// Discriminator. Required in schema v2. When migrating a schema-v1 file,
    /// missing kinds are coerced to `Custom` (that was the only supported
    /// login method before this change).
    #[serde(default = "default_kind_custom")]
    pub kind: ProviderKind,

    // -- Custom / Console relay --
    /// Anthropic-compatible endpoint. Required for `Custom`. Ignored for other
    /// kinds (Console defaults to api.anthropic.com; Bedrock/Vertex/Subscription
    /// don't route through a base URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    // -- Bedrock --
    #[serde(rename = "awsRegion", default, skip_serializing_if = "Option::is_none")]
    pub aws_region: Option<String>,
    #[serde(rename = "awsProfile", default, skip_serializing_if = "Option::is_none")]
    pub aws_profile: Option<String>,

    // -- Vertex --
    #[serde(rename = "vertexProjectId", default, skip_serializing_if = "Option::is_none")]
    pub vertex_project_id: Option<String>,
    #[serde(rename = "vertexRegion", default, skip_serializing_if = "Option::is_none")]
    pub vertex_region: Option<String>,
    #[serde(
        rename = "googleApplicationCredentials",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub google_application_credentials: Option<String>,

    // -- Subscription --
    /// Optional friendly label ("Work Max", "Personal Pro"). Used to
    /// disambiguate multiple subscription profiles.
    #[serde(
        rename = "subscriptionLabel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub subscription_label: Option<String>,

    // -- Model overrides (any kind) --
    #[serde(rename = "model", skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,

    #[serde(
        rename = "smallFastModel",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub small_fast_model: Option<String>,

    #[serde(
        rename = "defaultSonnetModel",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub default_sonnet_model: Option<String>,

    #[serde(
        rename = "defaultOpusModel",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub default_opus_model: Option<String>,

    #[serde(
        rename = "defaultHaikuModel",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub default_haiku_model: Option<String>,

    #[serde(
        rename = "apiTimeoutMs",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub api_timeout_ms: Option<u64>,

    #[serde(
        rename = "disableNonessentialTraffic",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub disable_nonessential_traffic: Option<bool>,

    /// Inline SVG markup for the provider's logo. Stored as a string (no
    /// base64) so it survives export/import as part of `providers.json`.
    /// Theme-aware: SVGs should use `currentColor` on their primary shapes —
    /// the renderer wraps the SVG in an element with `color: var(--muted-foreground)`.
    /// Validated to ≤ 50 KB on write to keep `providers.json` small.
    #[serde(rename = "logoSvg", skip_serializing_if = "Option::is_none", default)]
    pub logo_svg: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

fn default_kind_custom() -> ProviderKind {
    ProviderKind::Custom
}

/// What the frontend posts when saving a new or edited provider.
/// Secrets live in a separate field because they go to the keyring, not
/// `providers.json`.
///
/// Which secret is required depends on `kind`; see `commands::providers::validate_input`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: ProviderKind,

    // -- Custom relay --
    #[serde(default)]
    pub base_url: Option<String>,
    /// Custom-kind bearer token (`ANTHROPIC_AUTH_TOKEN`).
    #[serde(default)]
    pub auth_token: Option<String>,

    // -- Console --
    /// Anthropic Console API key (`ANTHROPIC_API_KEY`).
    #[serde(default)]
    pub api_key: Option<String>,

    // -- Bedrock --
    #[serde(default)]
    pub aws_region: Option<String>,
    #[serde(default)]
    pub aws_profile: Option<String>,
    #[serde(default)]
    pub aws_access_key_id: Option<String>,
    #[serde(default)]
    pub aws_secret_access_key: Option<String>,
    #[serde(default)]
    pub aws_session_token: Option<String>,

    // -- Vertex --
    #[serde(default)]
    pub vertex_project_id: Option<String>,
    #[serde(default)]
    pub vertex_region: Option<String>,
    #[serde(default)]
    pub google_application_credentials: Option<String>,

    // -- Subscription --
    #[serde(default)]
    pub subscription_label: Option<String>,

    // -- Model / misc overrides --
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub small_fast_model: Option<String>,
    #[serde(default)]
    pub default_sonnet_model: Option<String>,
    #[serde(default)]
    pub default_opus_model: Option<String>,
    #[serde(default)]
    pub default_haiku_model: Option<String>,
    #[serde(default)]
    pub api_timeout_ms: Option<u64>,
    #[serde(default)]
    pub disable_nonessential_traffic: Option<bool>,

    /// Inline SVG markup for the provider's logo. Omitted from payloads when
    /// the user hasn't set one — see `Provider::logo_svg` for storage details.
    #[serde(default)]
    pub logo_svg: Option<String>,
}
/// tag must match the provider's `kind`; a mismatch indicates data corruption
/// or a partial write, and should be surfaced as an internal error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderSecret {
    /// Full `claudeAiOauth` object as read from `.credentials.json`.
    Subscription { oauth: serde_json::Value },
    Console { api_key: String },
    Custom { auth_token: String },
    Bedrock {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_token: Option<String>,
    },
    /// Vertex relies on `GOOGLE_APPLICATION_CREDENTIALS` (a file path on disk),
    /// not a secret we manage. Present as a variant so `set_secret` still
    /// writes *something* per provider (helps consistency checks).
    Vertex {},
}

impl ProviderSecret {
    pub fn kind(&self) -> ProviderKind {
        match self {
            ProviderSecret::Subscription { .. } => ProviderKind::Subscription,
            ProviderSecret::Console { .. } => ProviderKind::Console,
            ProviderSecret::Custom { .. } => ProviderKind::Custom,
            ProviderSecret::Bedrock { .. } => ProviderKind::Bedrock,
            ProviderSecret::Vertex {} => ProviderKind::Vertex,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersFile {
    pub schema_version: u32,
    pub providers: Vec<Provider>,
}

impl Default for ProvidersFile {
    fn default() -> Self {
        Self {
            schema_version: 3,
            providers: Vec::new(),
        }
    }
}

/// Persisted pointer to the currently-active provider. Lives at
/// `<app-data>/state.json`. The pointer is the source of truth for
/// `get_active_provider_cmd`; `settings.json` env matching is only a fallback
/// for first-launch cases where `state.json` doesn't exist yet.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateFile {
    #[serde(rename = "activeProviderId", default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
}

/// AppError is the single error type returned to the frontend.
/// We tag each variant so the UI can branch on `kind` for friendlier
/// messages (e.g. "Keyring unavailable" vs "Settings file is malformed").
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation: {0}")]
    Validation(String),

    #[error("Provider not found: {0}")]
    NotFound(String),

    #[error("Provider name already exists: {0}")]
    DuplicateName(String),

    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error("Keyring unavailable: {0}")]
    KeyringUnavailable(String),

    #[error("Settings file at {path} is malformed: {message}")]
    MalformedSettings { path: String, message: String },

    #[error("Failed to acquire lock on settings file")]
    Lock(String),

    #[error("Internal: {0}")]
    Internal(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Validation(_) => "validation",
            AppError::NotFound(_) => "not_found",
            AppError::DuplicateName(_) => "duplicate_name",
            AppError::Keyring(_) => "keyring",
            AppError::KeyringUnavailable(_) => "keyring_unavailable",
            AppError::MalformedSettings { .. } => "malformed_settings",
            AppError::Lock(_) => "lock",
            AppError::Internal(_) => "internal",
        }
    }
}

// Serialize as tagged {kind, message} so the frontend can pattern-match.
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = ser.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_custom() -> Provider {
        Provider {
            id: "abc".into(),
            name: "test".into(),
            kind: ProviderKind::Custom,
            base_url: Some("https://x".into()),
            aws_region: None,
            aws_profile: None,
            vertex_project_id: None,
            vertex_region: None,
            google_application_credentials: None,
            subscription_label: None,
            model: Some("claude-sonnet-4-6".into()),
            small_fast_model: None,
            default_sonnet_model: Some("sonnet".into()),
            default_opus_model: None,
            default_haiku_model: None,
            api_timeout_ms: Some(60_000),
            disable_nonessential_traffic: Some(true),
            logo_svg: None,
            created_at: "2026-07-04T00:00:00Z".into(),
            updated_at: "2026-07-04T00:00:00Z".into(),
        }
    }

    #[test]
    fn provider_uses_camel_case_in_json() {
        let p = sample_custom();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"custom\""));
        assert!(json.contains("\"model\":\"claude-sonnet-4-6\""));
        assert!(json.contains("\"defaultSonnetModel\":\"sonnet\""));
        assert!(json.contains("\"apiTimeoutMs\":60000"));
        assert!(json.contains("\"disableNonessentialTraffic\":true"));
        // None fields are omitted
        assert!(!json.contains("smallFastModel"));
        assert!(!json.contains("defaultOpusModel"));
        assert!(!json.contains("awsRegion"));
        assert!(!json.contains("vertexProjectId"));
        assert!(!json.contains("logoSvg"));
    }

    #[test]
    fn provider_logo_svg_round_trips() {
        let mut p = sample_custom();
        p.logo_svg = Some(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"/></svg>"#.into());
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"logoSvg\":\"<svg"));
        let back: Provider = serde_json::from_str(&json).unwrap();
        assert_eq!(back.logo_svg, p.logo_svg);
    }

    #[test]
    fn provider_missing_kind_deserializes_as_custom() {
        // Simulates a schema-v1 provider — everything worked before `kind`
        // existed, so an old blob must load as Custom.
        let raw = r#"{
            "id": "old",
            "name": "legacy",
            "base_url": "https://x",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }"#;
        let p: Provider = serde_json::from_str(raw).unwrap();
        assert_eq!(p.kind, ProviderKind::Custom);
        assert_eq!(p.base_url.as_deref(), Some("https://x"));
    }

    #[test]
    fn provider_secret_kind_matches_variant() {
        let s = ProviderSecret::Custom {
            auth_token: "t".into(),
        };
        assert_eq!(s.kind(), ProviderKind::Custom);
        let s = ProviderSecret::Subscription {
            oauth: serde_json::json!({"accessToken": "abc"}),
        };
        assert_eq!(s.kind(), ProviderKind::Subscription);
    }

    #[test]
    fn error_serializes_with_kind_tag() {
        let e = AppError::Validation("name is empty".into());
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "validation");
        assert!(json["message"].as_str().unwrap().contains("name is empty"));
    }

    #[test]
    fn canonical_env_keys_covers_all_kinds() {
        // Sanity: presence checks for each kind's marker key.
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_BASE_URL"));
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_AUTH_TOKEN"));
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_API_KEY"));
        assert!(CANONICAL_ENV_KEYS.contains(&"CLAUDE_CODE_USE_BEDROCK"));
        assert!(CANONICAL_ENV_KEYS.contains(&"AWS_REGION"));
        assert!(CANONICAL_ENV_KEYS.contains(&"CLAUDE_CODE_USE_VERTEX"));
        assert!(CANONICAL_ENV_KEYS.contains(&"ANTHROPIC_VERTEX_PROJECT_ID"));
        // Exact count so a careless edit is caught. Update this when adding
        // Foundry in the follow-up PR.
        assert_eq!(CANONICAL_ENV_KEYS.len(), 20);
    }
}
