// Token masking helper. Shows first 6 chars + ellipsis + last 4 chars.
// Never returns the full token to the UI — Tauri commands deliberately
// omit it from Provider structs.

export function maskToken(token: string): string {
  if (!token) return "";
  if (token.length <= 12) return "•".repeat(token.length);
  return `${token.slice(0, 6)}…${token.slice(-4)}`;
}

export function isWebEnv(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// Common TLDs we want to skip when deriving a name from a hostname.
// Intentionally not exhaustive — handles the common case (com/io/net/org/
// dev/app/ai/co/lat/xyz/...) and falls back gracefully on misses.
const TLD_SET = new Set([
  "com", "net", "org", "io", "co", "ai", "app", "dev",
  "lat", "xyz", "me", "us", "uk", "de", "jp", "cn", "fr",
  "tech", "cloud", "sh", "gg", "so", "tv",
]);

/**
 * Derive a provider display name from its base URL.
 *
 * Rule: the name is whatever sits between `api.` (or any prefix containing
 * "api", e.g. "capi.") and the next `.` in the hostname. If the hostname
 * has no `api.` segment, fall back to the second-to-last label (skips
 * common TLDs).
 *
 * Examples:
 *   api.minimax.io      → "minimax"
 *   capi.aerolink.lat   → "aerolink"
 *   api.openai.com      → "openai"
 *   api.example.com     → "example"
 *   anthropic.com       → "anthropic"
 *   my-proxy.io         → "my-proxy"
 *
 * Returns "Provider" for invalid URLs or unparseable hosts (caller should
 * treat as a soft fallback, not an error).
 */
export function deriveProviderName(baseUrl: string): string {
  if (!baseUrl) return "Provider";
  let host: string;
  try {
    host = new URL(baseUrl).hostname.toLowerCase();
  } catch {
    return "Provider";
  }
  if (!host) return "Provider";

  // Find "api." anywhere in the host — handles "api.", "capi.", "myapi." etc.
  const idx = host.indexOf("api.");
  if (idx >= 0) {
    const after = host.substring(idx + 4); // skip past "api."
    const dotIdx = after.indexOf(".");
    const candidate = dotIdx >= 0 ? after.substring(0, dotIdx) : after;
    if (candidate) return candidate;
  }

  // Fallback: walk labels right-to-left, skipping TLDs, take the first
  // non-TLD label.
  const labels = host.split(".");
  for (let i = labels.length - 1; i >= 0; i--) {
    const label = labels[i];
    if (!TLD_SET.has(label) && label !== "api" && !label.startsWith("api")) {
      return label;
    }
  }
  return host;
}