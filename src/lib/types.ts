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
