use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical Claude Code env keys (in declared order; covers the 8 fields
/// a Provider may set). Order is not semantically meaningful but makes
/// serialization stable for tests and diffing.
pub const CANONICAL_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
];

/// Metadata for a saved provider. The auth token is stored separately
/// in the OS keyring under (service, account) = (KEYRING_SERVICE, id)
/// and is never persisted in this struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,

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

    pub created_at: String,
    pub updated_at: String,
}

/// What the frontend posts when saving a new or edited provider.
/// Token lives in a separate field because it goes to the keyring.
/// `auth_token` is optional: required when creating (`id: None`), and
/// treated as "keep existing" when updating without a new value.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub auth_token: Option<String>,

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersFile {
    pub schema_version: u32,
    pub providers: Vec<Provider>,
}

impl Default for ProvidersFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)] // Reserved for a future active-pointer cache; not used yet.
pub struct StateFile {
    #[serde(rename = "activeProviderId", skip_serializing_if = "Option::is_none")]
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

    #[test]
    fn provider_uses_camel_case_in_json() {
        let p = Provider {
            id: "abc".into(),
            name: "test".into(),
            base_url: "https://x".into(),
            model: Some("claude-sonnet-4-6".into()),
            small_fast_model: None,
            default_sonnet_model: Some("sonnet".into()),
            default_opus_model: None,
            default_haiku_model: None,
            api_timeout_ms: Some(60_000),
            disable_nonessential_traffic: Some(true),
            created_at: "2026-07-04T00:00:00Z".into(),
            updated_at: "2026-07-04T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"model\":\"claude-sonnet-4-6\""));
        assert!(json.contains("\"defaultSonnetModel\":\"sonnet\""));
        assert!(json.contains("\"apiTimeoutMs\":60000"));
        assert!(json.contains("\"disableNonessentialTraffic\":true"));
        // None fields are omitted
        assert!(!json.contains("smallFastModel"));
        assert!(!json.contains("defaultOpusModel"));
    }

    #[test]
    fn error_serializes_with_kind_tag() {
        let e = AppError::Validation("name is empty".into());
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "validation");
        assert!(json["message"].as_str().unwrap().contains("name is empty"));
    }
}