// Mirrors Rust models in src-tauri/src/models.rs.
// Keep in sync — used by the IPC layer.

export type ProviderKind =
  | "subscription"
  | "console"
  | "custom"
  | "bedrock"
  | "vertex";

/**
 * Fields every Provider has regardless of kind. Kind-specific fields hang
 * off the discriminated variants below.
 */
interface ProviderBase {
  id: string;
  name: string;
  model?: string;
  smallFastModel?: string;
  defaultSonnetModel?: string;
  defaultOpusModel?: string;
  defaultHaikuModel?: string;
  apiTimeoutMs?: number;
  disableNonessentialTraffic?: boolean;
  /**
   * Inline SVG markup for the provider's logo. Theme-aware: SVGs should
   * use `currentColor` on primary shapes so the wrapper's CSS `color`
   * drives the visual. Validated to ≤ 50 KB on save.
   */
  logoSvg?: string;
  created_at: string;
  updated_at: string;
}

export type Provider =
  | (ProviderBase & {
      kind: "subscription";
      subscriptionLabel?: string;
    })
  | (ProviderBase & { kind: "console" })
  | (ProviderBase & {
      kind: "custom";
      base_url: string;
    })
  | (ProviderBase & {
      kind: "bedrock";
      awsRegion?: string;
      awsProfile?: string;
    })
  | (ProviderBase & {
      kind: "vertex";
      vertexProjectId?: string;
      vertexRegion?: string;
      googleApplicationCredentials?: string;
    });

/** Same discriminated shape for the create/edit payload. */
interface ProviderInputBase {
  id?: string;
  name: string;
  model?: string;
  smallFastModel?: string;
  defaultSonnetModel?: string;
  defaultOpusModel?: string;
  defaultHaikuModel?: string;
  apiTimeoutMs?: number;
  disableNonessentialTraffic?: boolean;
  logoSvg?: string;
}

export type ProviderInput =
  | (ProviderInputBase & {
      kind: "subscription";
      subscription_label?: string;
    })
  | (ProviderInputBase & {
      kind: "console";
      api_key?: string;
    })
  | (ProviderInputBase & {
      kind: "custom";
      base_url: string;
      // Required on create. Omit on update to keep the existing keyring token.
      auth_token?: string;
    })
  | (ProviderInputBase & {
      kind: "bedrock";
      aws_region: string;
      aws_profile?: string;
      aws_access_key_id?: string;
      aws_secret_access_key?: string;
      aws_session_token?: string;
    })
  | (ProviderInputBase & {
      kind: "vertex";
      vertex_project_id: string;
      vertex_region: string;
      google_application_credentials?: string;
    });

export type KeyringStatus =
  | { status: "available" }
  | { status: "unavailable"; message: string };

export interface AppError {
  kind:
    | "io"
    | "json"
    | "validation"
    | "not_found"
    | "duplicate_name"
    | "keyring"
    | "keyring_unavailable"
    | "malformed_settings"
    | "malformed_claude_md"
    | "lock"
    | "internal";
  message: string;
}

/** Convenience narrowing helpers so components don't repeat `p.kind === "custom"`. */
export const isCustom = (p: Provider): p is Extract<Provider, { kind: "custom" }> =>
  p.kind === "custom";
export const isBedrock = (p: Provider): p is Extract<Provider, { kind: "bedrock" }> =>
  p.kind === "bedrock";
export const isVertex = (p: Provider): p is Extract<Provider, { kind: "vertex" }> =>
  p.kind === "vertex";
export const isSubscription = (p: Provider): p is Extract<Provider, { kind: "subscription" }> =>
  p.kind === "subscription";
export const isConsole = (p: Provider): p is Extract<Provider, { kind: "console" }> =>
  p.kind === "console";

/**
 * Row in the Marketplace list. Mirrors `MarketplaceSummary` in
 * `src-tauri/src/storage/marketplaces.rs`. Each registered marketplace is
 * one row — read-only display for now; add/remove is deferred.
 */
export interface MarketplaceSummary {
  /** Display name (from manifest, falls back to dir name). */
  name: string;
  /** Author name from manifest.owner.name, empty if absent. */
  owner: string;
  /** Short description from manifest.metadata.description. */
  description: string;
  /** Number of `plugins[]` entries in the manifest. */
  plugin_count: number;
  /** Number of plugins from this marketplace currently enabled in
   *  `settings.json` (`enabledPlugins`). Mirrors `installed_plugins.length`. */
  installed_count: number;
  /** Names of installed plugins from this marketplace. Alphabetical.
   *  Empty when settings.json is missing/malformed. */
  installed_plugins: string[];
  /** Names of manifest plugins not yet installed. Manifest order preserved. */
  available_plugins: string[];
  /** Diagnostic — path of the manifest file we read this row from. */
  source: string;
}

/**
 * Where a skill came from. Mirrors `SkillSource` in
 * `src-tauri/src/storage/skills.rs`. Use the `kind` discriminator to
 * narrow — `plugin` entries carry plugin/marketplace/version for grouping.
 */
export type SkillSource =
  | { kind: "user" }
  | {
      kind: "plugin";
      plugin: string;
      marketplace: string;
      version: string;
    };

/**
 * One row in the Skills list. Mirrors `SkillSummary` in
 * `src-tauri/src/storage/skills.rs`.
 */
export interface SkillSummary {
  /** Folder name (e.g. "graphify"). For plugin skills, the last path
   *  segment of the skill dir inside the plugin. */
  name: string;
  /** First-line description from SKILL.md frontmatter; empty when absent. */
  description: string;
  source: SkillSource;
  /** Absolute path to the SKILL.md file. Drives tooltips and any future
   *  "reveal in file manager" action. */
  path: string;
  /** Always true for user skills. For plugin skills, mirrors
   *  `enabledPlugins["<plugin>@<marketplace>"]` in settings.json —
   *  missing key defaults to true (Claude Code treats absent as enabled). */
  enabled: boolean;
}

/** Transport declared in the MCP server config. Defaults to "stdio"
 *  when the entry has no `type` field — Claude Code's documented default. */
export type McpTransport = "stdio" | "http" | "sse";

/** Health snapshot from `~/.claude/mcp-health-cache.json`. `None` when
 *  no record exists yet (server never checked). */
export type McpHealth =
  | { status: "healthy" }
  | {
      status: "failing";
      last_error: string;
      failure_count: number;
    };

/**
 * One row in the MCP servers list. Mirrors `McpServerSummary` in
 * `src-tauri/src/storage/mcp.rs`. Stdio fields are empty for http/sse
 *  entries; http/sse fields are empty for stdio entries.
 */
export interface McpServerSummary {
  name: string;
  transport: McpTransport;
  command: string | null;
  args: string[];
  /** Env var names → values (stdio only). */
  env: Record<string, string>;
  url: string | null;
  /** HTTP header names → values (http/sse only). */
  headers: Record<string, string>;
  health: McpHealth | null;
  needs_auth: boolean;
  /** Diagnostic — the path the row was read from. */
  source: string;
}

// ---------------------------------------------------------------------------
// Tracker (per-provider usage tracking)
// ---------------------------------------------------------------------------

/**
 * Form field declared by a tracker source. Mirrors `TrackerField` in
 * `src-tauri/src/tracker/mod.rs`. The UI renders one input per field
 * based on these properties — no per-source special-casing.
 */
export interface TrackerField {
  /** JSON key the value is stored under. */
  key: string;
  label: string;
  placeholder: string;
  /** `true` for keys/cookies — value goes to OS keyring, not the JSON
   *  config blob. The UI masks the input and shows a "Stored" hint
   *  when the backend reports a value is already present. */
  secret: boolean;
  /** Multiline text input (ManualJson's `payload` field). */
  multiline: boolean;
  required: boolean;
  hint: string | null;
}

/**
 * One registered tracker source. Mirrors `SourceDescriptor` in
 * `src-tauri/src/tracker/mod.rs`. The UI calls `listTrackerSources` once
 * to get this list and renders the source picker from it.
 */
export interface TrackerSourceDescriptor {
  id: string;
  display_name: string;
  description: string;
  fields: TrackerField[];
  /**
   * Provider kinds this source applies to. The UI uses this to filter
   * the picker and to show a "coming soon" panel when no source
   * matches the current provider's kind. Mirrors
   * `TrackerSource::applicable_kinds` in Rust.
   */
  applicable_kinds: string[];
}

/**
 * One quota window in a usage snapshot. All sources normalize to this
 * shape regardless of their native API.
 */
export interface TrackerUsageWindow {
  label: string;
  used: number | null;
  limit: number | null;
  /** 0..100. The UI uses this directly in the progress bar; when null
   *  it falls back to `used / limit * 100`. */
  used_percent: number | null;
  unit: string | null;
  resets_at: string | null;
  reset_label: string | null;
}

export interface TrackerModelUsage {
  model: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cost_usd: number | null;
}

/**
 * Normalized output every source returns. The UI doesn't care which
 * source produced it.
 */
export interface TrackerUsage {
  windows: TrackerUsageWindow[];
  models: TrackerModelUsage[];
  cost_usd: number | null;
  fetched_at: string;
  note: string | null;
}

/**
 * The saved tracker config returned to the UI. `fields` excludes any
 * secret values (they live in the keyring); `has_secret` tells the UI
 * which keys have a stored value so the form can show "Stored" instead
 * of an empty placeholder.
 */
export interface TrackerConfigView {
  source: string;
  /** Non-secret fields only. */
  fields: Record<string, unknown>;
  last_usage: TrackerUsage | null;
  last_fetched_at: string | null;
  last_error: string | null;
  updated_at: string;
  has_secret: string[];
}

// ---------------------------------------------------------------------------
// Sessions (Claude Code conversation history on this PC)
// ---------------------------------------------------------------------------

/**
 * One row in the sidebar Sessions list. Mirrors `SessionSummary` in
 * `src-tauri/src/storage/sessions.rs`.
 */
export interface SessionSummary {
  session_id: string;
  /** Summary or first prompt, truncated server-side. */
  title: string;
  message_count: number;
  /** RFC 3339 timestamp of last activity; drives the "5m ago" label. */
  modified: string | null;
  /** Last path segment of project_path (e.g. "claude-config"). */
  project_name: string | null;
  /** Absolute path to the `.jsonl` transcript. */
  full_path: string;
}
