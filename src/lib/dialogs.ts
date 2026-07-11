import { open } from "@tauri-apps/plugin-dialog";

/**
 * Open a native folder picker. Returns the selected absolute path or
 * `null` if the user cancelled.
 */
export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}