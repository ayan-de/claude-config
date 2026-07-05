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
