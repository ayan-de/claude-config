// Platform / runtime detection for the renderer.
//
// Tauri WebView forwards the host OS to navigator.userAgent / userAgentData,
// so we can detect macOS without pulling in @tauri-apps/plugin-os.

const TAURI_INTERNALS = "__TAURI_INTERNALS__" as const;

function detectTauri(): boolean {
  return typeof window !== "undefined" && TAURI_INTERNALS in window;
}

function detectMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const uaData = (navigator as Navigator & {
    userAgentData?: { platform?: string };
  }).userAgentData;
  if (uaData?.platform) {
    return uaData.platform === "macOS";
  }
  return /Mac/i.test(navigator.platform || navigator.userAgent || "");
}

export const isTauri = detectTauri();
export const isMac = detectMac();
