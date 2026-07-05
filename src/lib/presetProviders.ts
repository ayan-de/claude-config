import type { Provider } from "./types";

/**
 * Built-in presets for the Custom relay provider kind. Picking one from the
 * dropdown auto-fills `name` + `base_url` and loads the bundled SVG logo.
 *
 * Adding a new preset:
 *   1. Append an entry to PRESET_PROVIDERS.
 *   2. Drop the matching SVG at `public/logos/<id>.svg` using
 *      `currentColor` on primary shapes so it themes via the wrapper.
 *   3. If the provider exposes a self-serve dashboard for grabbing an API
 *      key, set `apiKeyUrl` so the form can show a "Grab your free API key
 *      from here" hint under the auth-token field.
 *
 * The "+ Custom" option in the dropdown uses CUSTOM_SENTINEL as its value —
 * it tells the form to clear preset state and let the user define a provider
 * not in the list (with an optional SVG upload).
 */
export interface PresetProvider {
  /** Stable identifier. Matches the SVG filename under /public/logos. */
  id: string;
  /** Display name shown in the dropdown and used as the default name. */
  name: string;
  /** Anthropic-compatible base URL the preset resolves to. */
  baseUrl: string;
  /** Path under /public for the bundled SVG asset. */
  logoPath: string;
  /** Link to a self-serve dashboard where the user can grab an API key.
   *  Surfaced in the form under the auth-token field when set. */
  apiKeyUrl?: string;
}

export const PRESET_PROVIDERS: readonly PresetProvider[] = [
  {
    id: "zai",
    name: "Z.ai (Zhipu GLM)",
    baseUrl: "https://api.z.ai/api/anthropic",
    logoPath: "/logos/zai.svg",
  },
  {
    id: "minimax",
    name: "MiniMax",
    baseUrl: "https://api.minimax.io/anthropic",
    logoPath: "/logos/minimax.svg",
  },
  {
    id: "moonshot",
    name: "Moonshot Kimi",
    baseUrl: "https://api.moonshot.ai/anthropic",
    logoPath: "/logos/moonshot.svg",
  },
  {
    id: "kimi-code",
    name: "Kimi Code Plan",
    baseUrl: "https://api.kimi.com/coding/",
    logoPath: "/logos/kimi-ai.svg",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/anthropic",
    logoPath: "/logos/deepseek.svg",
  },
  {
    id: "freemodel",
    name: "freemodel",
    baseUrl: "https://cc.freemodel.dev",
    logoPath: "/logos/freemodel.svg",
    apiKeyUrl: "https://freemodel.dev/dashboard/keys",
  },
  {
    id: "aerolink",
    name: "aerolink",
    baseUrl: "https://capi.aerolink.lat/",
    logoPath: "/logos/aerolink.svg",
    apiKeyUrl: "https://aerolink.lat/dashboard/api-keys",
  },
  {
    id: "zenmux",
    name: "zenmux",
    baseUrl: "https://zenmux.ai/api/anthropic",
    logoPath: "/logos/zenmux.svg",
    apiKeyUrl: "https://zenmux.ai/platform/pay-as-you-go",
  },
] as const;

/** Look up the API-key dashboard URL for a preset id. Returns undefined
 *  when the preset has no self-serve link or doesn't exist. */
export function getPresetApiKeyUrl(id: string | null): string | undefined {
  if (!id || id === CUSTOM_SENTINEL) return undefined;
  return PRESET_PROVIDERS.find((p) => p.id === id)?.apiKeyUrl;
}

/** Sentinel value for the "+ Custom" option in the preset dropdown. */
export const CUSTOM_SENTINEL = "__custom__";

/** Path to the universal fallback logo, used when a preset's own SVG is
 *  missing. The fallback is also themed via currentColor. */
export const FALLBACK_LOGO_PATH = "/logos/fallback.svg";

/** Map a base URL back to its preset id, if any. Useful for highlighting
 *  the active preset on edit. */
export function findPresetByBaseUrl(baseUrl: string): PresetProvider | undefined {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  return PRESET_PROVIDERS.find((p) => {
    const preset = p.baseUrl.trim().replace(/\/+$/, "");
    return trimmed === preset;
  });
}

/**
 * Dynamically resolve a logo SVG for a provider if it is not already set.
 * Returns the resolved SVG string, or null if it cannot be resolved.
 */
export async function resolveProviderLogo(provider: Provider): Promise<string | null> {
  if (provider.logoSvg) {
    return provider.logoSvg;
  }

  // 1. Resolve for Custom providers based on preset URL
  if (provider.kind === "custom" && provider.base_url) {
    const preset = findPresetByBaseUrl(provider.base_url);
    if (preset) {
      return fetchPresetLogo(preset.id);
    }
  }

  // 2. Resolve for Subscription and Console kinds (use Claude logo)
  if (provider.kind === "subscription" || provider.kind === "console") {
    return fetchSvgAt("/logos/claude.svg");
  }

  return null;
}

/** Module-level cache so repeated form opens don't re-fetch the SVG. */
const svgCache = new Map<string, string>();

/**
 * Fetch and cache the SVG markup for a path. Returns `null` only if the
 * fetch and the fallback (FALLBACK_LOGO_PATH) both fail.
 */
async function fetchSvgAt(path: string): Promise<string | null> {
  try {
    const res = await fetch(path);
    if (!res.ok) return null;
    const text = await res.text();
    if (!text.includes("<svg")) return null;
    return text;
  } catch {
    return null;
  }
}

/**
 * Fetch and cache the SVG markup for a preset. On 404 of the preset's own
 * logo, transparently falls back to `fallback.svg` so the UI always has
 * something to render. Returns `null` only if both fetches fail.
 */
export async function fetchPresetLogo(id: string): Promise<string | null> {
  const cached = svgCache.get(id);
  if (cached !== undefined) return cached;

  const preset = PRESET_PROVIDERS.find((p) => p.id === id);
  if (!preset) return null;

  const own = await fetchSvgAt(preset.logoPath);
  if (own) {
    svgCache.set(id, own);
    return own;
  }

  const fallback = await fetchSvgAt(FALLBACK_LOGO_PATH);
  if (fallback) {
    svgCache.set(id, fallback);
    return fallback;
  }
  return null;
}

/** Fetch the universal fallback logo. Cached separately so the lookup is
 *  fast even when no preset has been picked. */
export async function fetchFallbackLogo(): Promise<string | null> {
  return fetchSvgAt(FALLBACK_LOGO_PATH);
}