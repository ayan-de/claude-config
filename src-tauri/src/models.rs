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
    // The TS ProviderInputBase sends these as camelCase (matching the
    // `Provider` saved-shape struct). Per-field renames keep the two sides
    // in sync; without them, every model override + apiTimeoutMs + logoSvg
    // silently deserializes to None and the form appears to "save nothing".
    #[serde(rename = "model", default)]
    pub model: Option<String>,
    #[serde(rename = "smallFastModel", default)]
    pub small_fast_model: Option<String>,
    #[serde(rename = "defaultSonnetModel", default)]
    pub default_sonnet_model: Option<String>,
    #[serde(rename = "defaultOpusModel", default)]
    pub default_opus_model: Option<String>,
    #[serde(rename = "defaultHaikuModel", default)]
    pub default_haiku_model: Option<String>,
    #[serde(rename = "apiTimeoutMs", default)]
    pub api_timeout_ms: Option<u64>,
    #[serde(rename = "disableNonessentialTraffic", default)]
    pub disable_nonessential_traffic: Option<bool>,

    /// Inline SVG markup for the provider's logo. Omitted from payloads when
    /// the user hasn't set one — see `Provider::logo_svg` for storage details.
    #[serde(rename = "logoSvg", default)]
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

    #[error("CLAUDE.md file at {path} is malformed: {message}")]
    MalformedClaudeMd { path: String, message: String },

    #[error("Failed to acquire lock on settings file")]
    Lock(String),

    #[error("GitHub API error ({status}): {message}")]
    GitHub { status: u16, message: String },

    #[error("GitHub authentication required")]
    GitHubAuthRequired,

    #[error("GitHub sync not configured: {0}")]
    GitHubNotConfigured(String),

    #[error("session download conflict: {kind:?} for {session_id}")]
    SessionDownloadConflict {
        kind: SessionConflictKind,
        session_id: String,
    },

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
            AppError::MalformedClaudeMd { .. } => "malformed_claude_md",
            AppError::Lock(_) => "lock",
            AppError::GitHub { .. } => "github_api",
            AppError::GitHubAuthRequired => "github_auth_required",
            AppError::GitHubNotConfigured(_) => "github_not_configured",
            AppError::SessionDownloadConflict { .. } => "session_download_conflict",
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

// ===================================================================
// GitHub session sync
// ===================================================================

/// Account name used in the OS keyring for the GitHub access token.
/// Stored under (KEYRING_SERVICE, GITHUB_KEYRING_ACCOUNT).
pub const GITHUB_KEYRING_ACCOUNT: &str = "github_sync";

/// Default repo name for session storage. User-configurable in settings.
pub const DEFAULT_GITHUB_REPO: &str = "claude-sessions";

/// GitHub OAuth device-flow start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubDeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// GitHub OAuth access token (the only secret we keep). Stored in OS
/// keyring as a JSON-serialized `GitHubAuthSecret`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAuthSecret {
    pub access_token: String,
    pub username: Option<String>,
    pub created_at: String,
}

/// Non-secret metadata for GitHub sync. Stored at
/// `<app_data_dir>/github_sync.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSyncConfig {
    pub schema_version: u32,
    pub is_connected: bool,
    pub username: Option<String>,
    /// User's GitHub avatar URL — captured once on auth, shown in the
    /// top bar so users can see at a glance that they're connected.
    pub avatar_url: Option<String>,
    pub repo_name: String,
    pub last_sync: Option<String>,
    /// Set once after first upload — confirms user accepted the
    /// "this transcript may contain sensitive content" prompt.
    pub privacy_consent_given: bool,
}

impl Default for GitHubSyncConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            is_connected: false,
            username: None,
            avatar_url: None,
            repo_name: DEFAULT_GITHUB_REPO.to_string(),
            last_sync: None,
            privacy_consent_given: false,
        }
    }
}

/// Remote session summary returned to the UI when listing the GitHub
/// repo's contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionSummary {
    pub session_id: String,
    pub project_slug: String,
    pub original_path: String,
    pub title: Option<String>,
    pub modified: Option<String>,
    pub message_count: u32,
    pub sha: String,
    /// What the download button should do for this row *on this machine*.
    /// `#[serde(default)]` so old serialized rows (or rows produced by a
    /// `github_list_remote_sessions_cmd` caller that hasn't been updated)
    /// deserialize as `Download` rather than something else — the
    /// pre-Phase-4 wiring always showed Download anyway.
    #[serde(default)]
    pub sync_action: SyncAction,
}

/// Per-row action the Remote tab's download button renders. Populated by
/// `annotate_sync_actions` from local filesystem state — never computed
/// on the frontend.
///
/// `Update` means "safe pull, no local edits since last upload" — clicking
/// does not prompt. `Conflict` means "both sides changed" — clicking shows
/// the existing `SessionDownloadConflict` confirm dialog. `InSync` is
/// disabled. `Download` is the first-pull case (no local file or no
/// stored `remote_sha` to compare against).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    Download,
    Update,
    Conflict,
    InSync,
}

impl Default for SyncAction {
    fn default() -> Self {
        SyncAction::Download
    }
}

/// Maps an original project path (where the session was created) to a
/// local path on this machine. Persisted in `<app_data_dir>/project_path_mappings.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPathMapping {
    pub original_path: String,
    pub local_path: String,
    /// Project slug as encoded by Claude Code (e.g. `-home-foo-Projects-bar`).
    /// Optional for v3 backward-compat with entries persisted before
    /// Phase 3. When present, slug-keyed lookups skip the project picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPathMappings {
    pub version: u32,
    /// Original decoded project path -> local project folder.
    pub mappings: std::collections::HashMap<String, String>,
    /// Project slug -> local project folder. Populated alongside
    /// `mappings` by Phase 3 writes. Empty for files written before
    /// Phase 3; `#[serde(default)]` keeps old JSON round-tripping.
    #[serde(default)]
    pub slug_mappings: std::collections::HashMap<String, String>,
}

/// Per-project sync state. One file lives at
/// `<project_folder>/session_sync_state.json` (next to sessions-index.json).
/// Records the last-uploaded timestamp, remote blob SHA, and the file
/// mtime we captured at upload — enough to detect "local changed since
/// upload" without scanning the whole `.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncMetadata {
    pub last_uploaded: Option<String>,
    pub remote_sha: Option<String>,
    pub last_local_modified: Option<String>,
    pub sync_state: SyncState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    NeverUploaded,
    Synced,
    OutOfSync,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncStateFile {
    pub version: u32,
    pub sessions: std::collections::HashMap<String, SessionSyncMetadata>,
}

/// Per-project metadata stored in the repo at
/// `sessions/<slug>/metadata.json`. Its whole reason for existing is
/// `original_path`: the slug is lossy (see the module docs on encoding),
/// so the decoded project path must be carried explicitly for Phase 3
/// path remapping on download. The per-session map is a tiny index the
/// browse-remote UI reads without downloading every transcript.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRemoteMetadata {
    pub version: u32,
    pub original_path: String,
    pub sessions: std::collections::HashMap<String, RemoteSessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionEntry {
    pub title: Option<String>,
    pub modified: Option<String>,
    pub message_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionConflictKind {
    RemoteNewer,
    LocalNewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub session_id: String,
    pub full_path: String,
    pub sync_state: SyncState,
}

// ===================================================================
// Scheduled window primers
// ===================================================================

/// Weekday for a schedule's recurrence set. Serialized lowercase
/// (`"mon".."sun"`) to keep `schedules.json` and the TS mirror compact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// Cron day-of-week number. Cron uses 0=Sunday .. 6=Saturday.
    pub fn cron_num(self) -> u32 {
        match self {
            Weekday::Sun => 0,
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
        }
    }

    pub fn from_chrono(w: chrono::Weekday) -> Self {
        match w {
            chrono::Weekday::Mon => Weekday::Mon,
            chrono::Weekday::Tue => Weekday::Tue,
            chrono::Weekday::Wed => Weekday::Wed,
            chrono::Weekday::Thu => Weekday::Thu,
            chrono::Weekday::Fri => Weekday::Fri,
            chrono::Weekday::Sat => Weekday::Sat,
            chrono::Weekday::Sun => Weekday::Sun,
        }
    }
}

/// A recurring primer: a local time on a set of weekdays. When enabled it is
/// rendered into the OS scheduler (crontab / Scheduled Tasks) which fires the
/// generated wrapper script. No secrets live here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Local 24h "HH:MM".
    pub time: String,
    /// Recurrence weekdays; empty is invalid (rejected on write).
    pub days: Vec<Weekday>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// What the frontend posts to create or edit a schedule. `id` is `None` for a
/// create, `Some` for an edit.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    pub time: String,
    pub days: Vec<Weekday>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchedulesFile {
    pub schema_version: u32,
    pub schedules: Vec<Schedule>,
}

impl Default for SchedulesFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            schedules: Vec::new(),
        }
    }
}

/// One primer execution, appended to `runs.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRun {
    pub schedule_id: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Per-schedule status shown in the UI: last recorded run + computed next fire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    pub schedule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<ScheduleRun>,
    /// Next fire time as RFC3339 local, or `None` if disabled / no days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_fire: Option<String>,
}

/// Prerequisites for scheduling to work, surfaced as UI warnings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulingAvailability {
    /// `claude` binary is on PATH.
    pub claude_on_path: bool,
    /// The OS scheduler binary (crontab / schtasks) is present.
    pub scheduler_available: bool,
    /// Subscription OAuth exists in `.credentials.json`.
    pub subscription_oauth_present: bool,
    /// A native Claude Code `scheduled-tasks/` dir exists (Routines / Desktop
    /// Tasks) — used to surface the "native alternative" note.
    pub native_scheduling_present: bool,
    /// Human label for the scheduler: "crontab", "schtasks", or "none".
    pub scheduler_kind: String,
}

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
    fn provider_input_round_trips_camel_case_form_payload() {
        // Regression: the TS ProviderForm hook posts camelCase model override
        // fields (smallFastModel, defaultSonnetModel, apiTimeoutMs, …).
        // If ProviderInput loses those renames, the Rust side silently gets
        // None for every model override and the form appears to "save nothing".
        let raw = r#"{
            "id": "abc",
            "name": "My Provider",
            "kind": "custom",
            "base_url": "https://api.example.com",
            "auth_token": "tok",
            "model": "claude-sonnet-4-6",
            "smallFastModel": "claude-haiku-4-5",
            "defaultSonnetModel": "claude-sonnet-4-6",
            "defaultOpusModel": "claude-opus-4-7",
            "defaultHaikuModel": "claude-haiku-4-5",
            "apiTimeoutMs": 120000,
            "disableNonessentialTraffic": true,
            "logoSvg": "<svg/>"
        }"#;
        let input: ProviderInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(input.small_fast_model.as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(input.default_sonnet_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(input.default_opus_model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(input.default_haiku_model.as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(input.api_timeout_ms, Some(120_000));
        assert_eq!(input.disable_nonessential_traffic, Some(true));
        assert_eq!(input.logo_svg.as_deref(), Some("<svg/>"));
        // Snake_case form (kind-specific union variants) still works.
        assert_eq!(input.base_url.as_deref(), Some("https://api.example.com"));
        assert_eq!(input.auth_token.as_deref(), Some("tok"));
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

    #[test]
    fn project_path_mapping_round_trip_with_slug() {
        let m = ProjectPathMapping {
            original_path: "/home/foo/Projects/bar".to_string(),
            local_path: "/home/baz/Projects/bar".to_string(),
            slug: Some("-home-foo-Projects-bar".to_string()),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ProjectPathMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back.slug.as_deref(), Some("-home-foo-Projects-bar"));
    }

    #[test]
    fn project_path_mapping_round_trip_without_slug() {
        // Backwards compat: an entry written before Phase 3 has no slug.
        let json = r#"{"originalPath":"/home/foo","localPath":"/home/bar"}"#;
        let m: ProjectPathMapping = serde_json::from_str(json).unwrap();
        assert_eq!(m.slug, None);
    }

    #[test]
    fn project_path_mappings_round_trip_without_slug_map() {
        // Backwards compat: a file written before Phase 3 has no
        // `slugMappings` key; it must deserialize to an empty map.
        let json = r#"{"version":1,"mappings":{"/home/foo":"/home/bar"}}"#;
        let m: ProjectPathMappings = serde_json::from_str(json).unwrap();
        assert!(m.slug_mappings.is_empty());
        assert_eq!(m.mappings.get("/home/foo").map(|s| s.as_str()), Some("/home/bar"));
    }

    #[test]
    fn session_download_conflict_serializes_kind_in_message() {
        let err = AppError::SessionDownloadConflict {
            kind: SessionConflictKind::RemoteNewer,
            session_id: "abc-123".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Custom Serialize impl flattens to {kind, message}; the
        // conflict kind and session id live in the message string.
        assert!(json.contains("\"kind\":\"session_download_conflict\""));
        assert!(json.contains("RemoteNewer"));
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn remote_session_summary_missing_sync_action_defaults_to_download() {
        // Regression: pre-Phase-4 cached payloads (or any JSON emitted by
        // a non-updated backend) have no `syncAction` field. The `#[serde(default)]`
        // attribute + `Default for SyncAction` must produce Download, not panic
        // and not surface InSync by accident.
        let raw = r#"{
            "sessionId": "abc",
            "projectSlug": "-home-foo",
            "originalPath": "/home/foo",
            "title": null,
            "modified": null,
            "messageCount": 0,
            "sha": "0123456789abcdef0123456789abcdef01234567"
        }"#;
        let s: RemoteSessionSummary = serde_json::from_str(raw).unwrap();
        assert_eq!(s.sync_action, SyncAction::Download);
    }

    #[test]
    fn weekday_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Weekday::Mon).unwrap(), "\"mon\"");
        assert_eq!(serde_json::to_string(&Weekday::Sun).unwrap(), "\"sun\"");
        let w: Weekday = serde_json::from_str("\"fri\"").unwrap();
        assert_eq!(w, Weekday::Fri);
    }

    #[test]
    fn weekday_cron_num_maps_sun_zero() {
        assert_eq!(Weekday::Sun.cron_num(), 0);
        assert_eq!(Weekday::Mon.cron_num(), 1);
        assert_eq!(Weekday::Sat.cron_num(), 6);
        assert_eq!(Weekday::from_chrono(chrono::Weekday::Wed), Weekday::Wed);
    }

    #[test]
    fn schedule_round_trips_camel_case() {
        let s = Schedule {
            id: "abc".into(),
            label: Some("Morning".into()),
            time: "07:30".into(),
            days: vec![Weekday::Mon, Weekday::Fri],
            enabled: true,
            created_at: "2026-07-14T00:00:00Z".into(),
            updated_at: "2026-07-14T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"createdAt\""), "json: {json}");
        assert!(json.contains("\"time\":\"07:30\""));
        assert!(json.contains("[\"mon\",\"fri\"]"));
        let back: Schedule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn schedule_label_omitted_when_none() {
        let s = Schedule {
            id: "abc".into(),
            label: None,
            time: "16:30".into(),
            days: vec![Weekday::Sun],
            enabled: false,
            created_at: "2026-07-14T00:00:00Z".into(),
            updated_at: "2026-07-14T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("label"), "json: {json}");
    }

    #[test]
    fn schedules_file_default_is_v1_empty() {
        let f = SchedulesFile::default();
        assert_eq!(f.schema_version, 1);
        assert!(f.schedules.is_empty());
    }

    #[test]
    fn schedule_input_accepts_missing_id_and_label() {
        let raw = r#"{"time":"07:30","days":["mon","tue"],"enabled":true}"#;
        let input: ScheduleInput = serde_json::from_str(raw).unwrap();
        assert!(input.id.is_none());
        assert!(input.label.is_none());
        assert_eq!(input.days, vec![Weekday::Mon, Weekday::Tue]);
        assert!(input.enabled);
    }

    #[test]
    fn schedule_run_round_trips() {
        let r = ScheduleRun {
            schedule_id: "abc".into(),
            started_at: "2026-07-14T07:30:00Z".into(),
            exit_code: Some(0),
            ok: true,
            error: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"scheduleId\":\"abc\""));
        assert!(json.contains("\"exitCode\":0"));
        assert!(!json.contains("error"));
        let back: ScheduleRun = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
