// Typed wrappers around `invoke()` so component code never touches
// raw command names. All errors are normalized to AppError shape.

import { invoke } from "@tauri-apps/api/core";
import type {
  GitHubDeviceFlowStart,
  GitHubPollOutcome,
  GitHubSyncConfig,
  KeyringStatus,
  MarketplaceSummary,
  McpServerSummary,
  ProjectPathMapping,
  Provider,
  ProviderInput,
  RepoProbeResult,
  SessionMessage,
  SessionSummary,
  SessionSyncMetadata,
  SessionSyncStateFile,
  SkillSummary,
  SyncState,
  TrackerConfigView,
  TrackerSourceDescriptor,
  TrackerUsage,
} from "./types";

interface RawError {
  kind?: string;
  message?: string;
}

/**
 * Error carrying the backend's `kind` discriminant (see `AppError` in
 * `models.rs`, serialized as `{kind, message}`). Callers that want to
 * branch on a specific failure — e.g. `github_auth_required` vs
 * `github_not_configured` — read `.kind`; everyone else just uses
 * `.message` as before.
 */
export class AppError extends Error {
  readonly kind?: string;
  constructor(message: string, kind?: string) {
    super(message);
    this.name = "AppError";
    this.kind = kind;
  }
}

function normalizeError(e: unknown): Error {
  if (typeof e === "object" && e !== null) {
    const r = e as RawError;
    if (typeof r.message === "string") {
      return new AppError(r.message, r.kind);
    }
  }
  if (typeof e === "string") return new AppError(e);
  return new AppError("Unknown error");
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw normalizeError(e);
  }
}

// ---------- system ----------

export const keyringStatus = () => call<KeyringStatus>("keyring_status_cmd");
export const discoverClaudeDir = () => call<string>("discover_claude_dir_cmd");
export const getAppDataDir = () => call<string>("get_app_data_dir_cmd");
export const revealInFileManager = (path: string) =>
  call<void>("reveal_in_file_manager_cmd", { path });
export const readClaudeMd = () => call<string | null>("read_claude_md_cmd");
export const writeClaudeMd = (content: string) =>
  call<void>("write_claude_md_cmd", { content });
export const claudeMdExists = () => call<boolean>("claude_md_exists_cmd");

/**
 * Lists registered marketplaces from
 * &lt;claude_dir&gt;/plugins/marketplaces/<name>/.claude-plugin/marketplace.json.
 * Returns an empty array when no marketplaces are installed yet.
 */
export const listMarketplaces = () =>
  call<MarketplaceSummary[]>("list_marketplaces_cmd");

/**
 * Lists user-authored skills (the SKILL.md files under the user's
 * claude config dir's skills/ tree) plus skills bundled with installed
 * plugins. Mirrors `scan_skills` in `src-tauri/src/storage/skills.rs`.
 * Returns an empty array when no skills are installed yet.
 */
export const listSkills = () => call<SkillSummary[]>("list_skills_cmd");

/**
 * Lists MCP servers Claude Code connects to, from the top-level
 * `mcpServers` object in `~/.claude.json`, enriched with the health
 * and needs-auth caches. Mirrors `scan_mcp_servers` in
 * `src-tauri/src/storage/mcp.rs`. Returns an empty array when no
 * servers are configured.
 */
export const listMcpServers = () =>
  call<McpServerSummary[]>("list_mcp_servers_cmd");

/**
 * Lists Claude Code conversation sessions on this PC. Scans the
 * per-project sessions index files under &lt;claude_dir&gt;/projects
 * plus a jsonl fallback for transcripts not yet indexed. Mirrors
 * `scan_sessions` in `src-tauri/src/storage/sessions.rs`. Returns an
 * empty array when no sessions exist yet.
 */
export const listSessions = () => call<SessionSummary[]>("list_sessions_cmd");

/**
 * Parses a Claude Code `.jsonl` transcript at `path` into a flat list
 * of messages. Used by the in-app transcript viewer.
 */
export const parseSession = (path: string) =>
  call<SessionMessage[]>("parse_session_cmd", { path });

/**
 * Deletes a single Claude Code session: moves the `.jsonl` to OS Trash
 * and strips the entry from `sessions-index.json`. Local-only; the
 * GitHub-synced copy (if any) is untouched.
 */
export const deleteSession = (fullPath: string) =>
  call<void>("delete_session_cmd", { fullPath });

// ---------- providers ----------

export const listProviders = () => call<Provider[]>("list_providers_cmd");
export const getProvider = (id: string) =>
  call<Provider>("get_provider_cmd", { id });
export const addProvider = (input: ProviderInput) =>
  call<Provider>("add_provider_cmd", { input });
export const updateProvider = (input: ProviderInput) =>
  call<Provider>("update_provider_cmd", { input });
export const deleteProvider = (id: string) =>
  call<void>("delete_provider_cmd", { id });
export const validateProvider = (input: ProviderInput) =>
  call<void>("validate_provider_cmd", { input });

// ---------- settings ----------

export const getActiveProvider = () =>
  call<Provider | null>("get_active_provider_cmd");
export const loadProvider = (id: string) =>
  call<void>("load_provider_cmd", { id });
export const saveCurrentAsProvider = (name: string) =>
  call<Provider>("save_current_as_provider_cmd", { name });
export const previewProviderEnv = (id: string) =>
  call<Record<string, unknown>>("preview_provider_env_cmd", { id });
export const getSettingsEnvKeys = () =>
  call<string[]>("get_settings_env_keys_cmd");
export const getDangerousMode = () =>
  call<boolean>("get_dangerous_mode_cmd");
export const setDangerousMode = (enabled: boolean) =>
  call<void>("set_dangerous_mode_cmd", { enabled });

// ---------- subscription ----------

/**
 * Snapshot the current `claude /login` OAuth session (from
 * `~/.claude/.credentials.json`) as a saved Subscription provider.
 *
 * `label` is optional — used to disambiguate multiple subscription profiles
 * (e.g. "Work Max", "Personal Pro"). When omitted, the display name is
 * derived from the email in the OAuth blob when available.
 */
export const importCurrentSubscription = (label?: string) =>
  call<Provider>("import_current_subscription_cmd", { label: label ?? null });

// ---------- transfer ----------

export const exportProviders = (
  dest: string,
  includeSecrets: boolean,
) => call<void>("export_providers_cmd", { dest, includeSecrets });
export const importProviders = (src: string, secretsSrc?: string) =>
  call<number>("import_providers_cmd", { src, secretsSrc });

// ---------- tracker ----------
//
// Per-provider usage tracking. The flow is:
//   1. listTrackerSources() — fetch the source catalog once at mount.
//   2. getTrackerConfig(providerId) — read saved config + cached usage.
//   3. saveTrackerConfig(providerId, source, fields) — validate + persist.
//   4. refreshTracker(providerId) — fetch a fresh usage snapshot.
//   5. getTrackerUsage(providerId) — read the cached snapshot only.
//   6. deleteTrackerConfig(providerId) — clean up.
//
// Secrets are split out by the backend into the OS keyring; the UI never
// sees them on read. On save the UI sends whatever the user typed (empty
// = "leave existing keyring entry alone").

export const listTrackerSources = () =>
  call<TrackerSourceDescriptor[]>("list_tracker_sources_cmd");

export const getTrackerConfig = (providerId: string) =>
  call<TrackerConfigView>("get_tracker_config_cmd", { providerId });

export const saveTrackerConfig = (
  providerId: string,
  source: string,
  fields: Record<string, unknown>,
) =>
  call<TrackerConfigView>("save_tracker_config_cmd", {
    providerId,
    source,
    fields,
  });

export const deleteTrackerConfig = (providerId: string) =>
  call<void>("delete_tracker_config_cmd", { providerId });

export const refreshTracker = (providerId: string) =>
  call<TrackerConfigView>("refresh_tracker_cmd", { providerId });

export const getTrackerUsage = (providerId: string) =>
  call<TrackerUsage | null>("get_tracker_usage_cmd", { providerId });

/**
 * Bulk-fetch the cached usage snapshot for every provider that has one.
 * Used by the sidebar to render per-provider progress bars without
 * fanning out N IPC calls. Providers with no tracker config are simply
 * absent from the map.
 */
export const listTrackerUsage = () =>
  call<Record<string, TrackerUsage>>("list_tracker_usage_cmd");

// ---------- github sync (Phase 1: OAuth + connection) ----------

export const getGithubSyncConfig = () =>
  call<GitHubSyncConfig>("get_github_sync_config_cmd");

export const githubStartDeviceFlow = () =>
  call<GitHubDeviceFlowStart>("github_start_device_flow_cmd");

export const githubPollDeviceFlow = (deviceCode: string) =>
  call<GitHubPollOutcome>("github_poll_device_flow_cmd", { deviceCode });

export const githubOpenVerificationUrl = (verificationUri: string) =>
  call<void>("github_open_verification_url_cmd", { verificationUri });

export const githubDisconnect = () =>
  call<void>("github_disconnect_cmd");

export const githubSetPrivacyConsent = (given: boolean) =>
  call<void>("github_set_privacy_consent_cmd", { given });

export const githubSetRepoName = (repoName: string) =>
  call<void>("github_set_repo_name_cmd", { repoName });

export const githubGetPathMappings = () =>
  call<ProjectPathMapping[]>("github_get_path_mappings_cmd");

export const githubSetPathMapping = (originalPath: string, localPath: string) =>
  call<void>("github_set_path_mapping_cmd", { originalPath, localPath });

export const githubRemovePathMapping = (originalPath: string) =>
  call<void>("github_remove_path_mapping_cmd", { originalPath });

/**
 * Probe whether the configured session-sync repo exists on the user's
 * GitHub account. Used during Phase 2 upload setup; exposed here so the
 * settings UI can also show "Repo: connected / not found".
 */
export const githubCheckRepo = () =>
  call<RepoProbeResult | null>("github_check_repo_cmd");

// ---------- github sync (Phase 2: upload) ----------

/**
 * Upload one transcript (+ per-project metadata) to the private sync repo
 * in a single commit. Returns the fresh sync metadata so the caller can
 * recolor the row without a refetch. Rejects with `github_not_configured`
 * (`privacy_consent_required`) until the consent flag is set.
 */
export const githubUploadSession = (
  sessionId: string,
  fullPath: string,
  projectPath: string,
) =>
  call<SessionSyncMetadata>("github_upload_session_cmd", {
    sessionId,
    fullPath,
    projectPath,
  });

/** Full per-project sync-state map (parent folder of the transcripts). */
export const githubGetSessionSyncState = (projectFolder: string) =>
  call<SessionSyncStateFile>("github_get_session_sync_state_cmd", {
    projectFolder,
  });

/** Re-classify one session against its current on-disk mtime. */
export const githubCheckSessionSyncStatus = (
  sessionId: string,
  fullPath: string,
) =>
  call<SyncState>("github_check_session_sync_status_cmd", {
    sessionId,
    fullPath,
  });