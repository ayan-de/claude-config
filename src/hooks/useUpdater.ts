/* eslint-disable react-hooks/set-state-in-effect --
 * The mount-time useEffect runs a one-shot check against the
 * tauri-plugin-updater endpoint. The recommended alternative
 * (useSyncExternalStore) is overkill for a single initial fetch.
 */
"use client";

// Auto-update hook. Uses tauri-plugin-updater's JS API directly.
// Mounts a silent background check; emits toast on detection; the
// banner + settings red dot are driven by `available`. Manual
// re-check via `checkNow()` (wired into SettingsMenu).

import { useCallback, useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { toast } from "sonner";

import { isWebEnv } from "@/lib/utils-app";

export type UpdaterState = {
  available: boolean;
  version: string | null;
  downloading: boolean;
  dismissed: boolean;
  error: string | null;
};

export type UpdaterActions = {
  checkNow: () => Promise<void>;
  installUpdate: () => Promise<void>;
  dismiss: () => void;
};

export function useUpdater(): UpdaterState & UpdaterActions {
  const [available, setAvailable] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const updateRef = useRef<Update | null>(null);

  const runCheck = useCallback(async (showToastOnError: boolean) => {
    if (!isWebEnv()) {
      try {
        const u = await check();
        if (u) {
          updateRef.current = u;
          setVersion(u.version);
          setAvailable(true);
          setDismissed(false);
          toast.info(`Update available: v${u.version}`, {
            description: "Click Update to install.",
            duration: 8000,
          });
        } else {
          setAvailable(false);
          setVersion(null);
          updateRef.current = null;
          if (showToastOnError) {
            toast.success("You're up to date");
          }
        }
        setError(null);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
        if (showToastOnError) {
          toast.error(`Update check failed: ${msg}`);
        }
      }
    }
  }, []);

  useEffect(() => {
    void runCheck(false);
  }, [runCheck]);

  const checkNow = useCallback(() => runCheck(true), [runCheck]);

  const installUpdate = useCallback(async () => {
    const u = updateRef.current;
    if (!u) {
      toast.error("No update available");
      return;
    }
    setDownloading(true);
    try {
      await u.download();
      await u.install();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      toast.error(`Update failed: ${msg}`);
      setDownloading(false);
    }
  }, []);

  const dismiss = useCallback(() => setDismissed(true), []);

  return {
    available,
    version,
    downloading,
    dismissed,
    error,
    checkNow,
    installUpdate,
    dismiss,
  };
}