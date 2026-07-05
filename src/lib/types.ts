// Mirrors Rust models in src-tauri/src/models.rs.
// Keep in sync — used by the IPC layer.

export interface Provider {
  id: string;
  name: string;
  base_url: string;
  model?: string;
  smallFastModel?: string;
  defaultSonnetModel?: string;
  defaultOpusModel?: string;
  defaultHaikuModel?: string;
  apiTimeoutMs?: number;
  disableNonessentialTraffic?: boolean;
  created_at: string;
  updated_at: string;
}

export interface ProviderInput {
  id?: string;
  name: string;
  base_url: string;
  // Required on create. Omit (or leave undefined) on update to keep the
  // existing keyring token; supply a new value only to rotate it.
  auth_token?: string;
  model?: string;
  smallFastModel?: string;
  defaultSonnetModel?: string;
  defaultOpusModel?: string;
  defaultHaikuModel?: string;
  apiTimeoutMs?: number;
  disableNonessentialTraffic?: boolean;
}

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

export type EditableProvider = ProviderInput & {
  /** True when editing existing (vs creating new). Drives header + button text. */
  isEditing: boolean;
};