// Typed wrappers around `invoke()` so component code never touches
// raw command names. All errors are normalized to AppError shape.

import { invoke } from "@tauri-apps/api/core";
import type {
  KeyringStatus,
  MarketplaceSummary,
  Provider,
  ProviderInput,
  SkillSummary,
} from "./types";

interface RawError {
  kind?: string;
  message?: string;
}

function normalizeError(e: unknown): Error {
  if (typeof e === "object" && e !== null) {
    const r = e as RawError;
    if (typeof r.message === "string") {
      return new Error(r.message);
    }
  }
  if (typeof e === "string") return new Error(e);
  return new Error("Unknown error");
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