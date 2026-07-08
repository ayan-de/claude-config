/* eslint-disable react-hooks/set-state-in-effect --
 * The data-fetching useEffect here triggers an initial provider/keyring
 * load on mount. The recommended alternative (useSyncExternalStore) adds
 * complexity for a one-shot initial fetch.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";

import {
  addProvider,
  deleteProvider,
  discoverClaudeDir,
  exportProviders,
  getActiveProvider,
  getAppDataDir,
  getSettingsEnvKeys,
  importProviders,
  keyringStatus,
  listProviders,
  listTrackerUsage,
  loadProvider,
  refreshTracker,
  revealInFileManager,
  saveCurrentAsProvider,
  updateProvider,
} from "@/lib/api";
import { isWebEnv } from "@/lib/utils-app";
import { resolveProviderLogo } from "@/lib/presetProviders";
import type { KeyringStatus, Provider, ProviderInput } from "@/lib/types";

export type AppMode =
  | { kind: "idle" }
  | { kind: "creating" }
  | { kind: "editing"; provider: Provider };

/**
 * Matches "5h", "5h session", "5-hour session" — the labels our tracker
 * sources use for the rolling-5-hour quota window. Ponytail: regex once,
 * reused on every refresh.
 */
const FIVE_HOUR_RE = /^5[\s-]?h(our)?\s*session?$/i;

export function useProvidersApp() {
  const [mounted, setMounted] = useState(false);
  const [ready, setReady] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [active, setActive] = useState<Provider | null>(null);
  const [customEnvKeys, setCustomEnvKeys] = useState<string[] | null>(null);
  const [keyring, setKeyring] = useState<KeyringStatus | null>(null);
  const [appDataDir, setAppDataDir] = useState<string | null>(null);
  const [claudeDir, setClaudeDir] = useState<string | null>(null);
  const [mode, setMode] = useState<AppMode>({ kind: "idle" });
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<Provider | null>(null);
  const [deleting, setDeleting] = useState(false);
  // Per-provider tracker 5h session percentage for the sidebar progress
  // bar. `null` = no tracker config / no 5h window. Keyed by provider id.
  const [session5hByProvider, setSession5hByProvider] = useState<
    Record<string, number>
  >({});
  const [trackerRefreshInterval, setTrackerRefreshIntervalState] = useState<number>(60000);
  const [trackerProviderIds, setTrackerProviderIds] = useState<string[]>([]);

  const refresh = useCallback(async () => {
    try {
      const [list, activeP, krStatus, appDir, claudeD, envKeys, trackerMap] =
        await Promise.all([
          listProviders(),
          getActiveProvider(),
          keyringStatus(),
          getAppDataDir(),
          discoverClaudeDir(),
          getSettingsEnvKeys(),
          listTrackerUsage().catch(() => ({})),
        ]);

      // Flatten each provider's cached usage into just the 5h session
      // percent. The sidebar only needs this single value to draw the
      // bar; everything else is owned by the Tracker tab.
      const next: Record<string, number> = {};
      for (const [providerId, usage] of Object.entries(trackerMap)) {
        const w = usage.windows.find((x) => FIVE_HOUR_RE.test(x.label));
        if (!w) continue;
        const pct =
          w.used_percent ??
          (w.used !== null && w.limit && w.limit > 0
            ? (w.used / w.limit) * 100
            : null);
        if (pct !== null && Number.isFinite(pct)) {
          next[providerId] = Math.max(0, Math.min(100, pct));
        }
      }
      setSession5hByProvider(next);
      setTrackerProviderIds(Object.keys(trackerMap));

      // Enrich list and active provider with resolved logos if missing
      const enrichedList = await Promise.all(
        list.map(async (p) => {
          if (p.logoSvg) return p;
          const svg = await resolveProviderLogo(p);
          return svg ? { ...p, logoSvg: svg } : p;
        })
      );

      let enrichedActive = activeP;
      if (activeP && !activeP.logoSvg) {
        const svg = await resolveProviderLogo(activeP);
        if (svg) {
          enrichedActive = { ...activeP, logoSvg: svg };
        }
      }

      setProviders(enrichedList);
      setActive(enrichedActive);
      setKeyring(krStatus);
      setAppDataDir(appDir);
      setClaudeDir(claudeD);
      // Custom config = env present but no provider matches.
      if (!activeP && envKeys.length > 0) {
        setCustomEnvKeys(envKeys);
      } else {
        setCustomEnvKeys(null);
      }
    } catch (e) {
      toast.error(`Failed to load: ${(e as Error).message}`);
    } finally {
      setReady(true);
    }
  }, []);

  useEffect(() => {
    // Defer all Tauri-dependent rendering until after mount so server
    // and client output match (avoids hydration mismatch).
    setMounted(true);
    if (isWebEnv()) {
      void refresh();
    }
  }, [refresh]);

  // Load refresh interval on mount
  useEffect(() => {
    if (typeof window !== "undefined") {
      const stored = localStorage.getItem("tracker_refresh_interval");
      if (stored !== null) {
        const val = parseInt(stored, 10);
        if ([60000, 300000, 0].includes(val)) {
          setTrackerRefreshIntervalState(val);
        }
      }
    }
  }, []);

  const setTrackerRefreshInterval = useCallback((interval: number) => {
    setTrackerRefreshIntervalState(interval);
    localStorage.setItem("tracker_refresh_interval", interval.toString());
  }, []);

  // Background polling for trackers
  useEffect(() => {
    if (trackerRefreshInterval <= 0 || trackerProviderIds.length === 0 || !isWebEnv()) {
      return;
    }

    const interval = setInterval(async () => {
      try {
        const results = await Promise.all(
          trackerProviderIds.map(async (id) => {
            try {
              return { id, view: await refreshTracker(id) };
            } catch (e) {
              console.error(`Background refresh failed for provider ${id}`, e);
              return { id, view: null };
            }
          })
        );

        setSession5hByProvider((prev) => {
          const next = { ...prev };
          for (const { id, view } of results) {
            if (!view) continue;
            const w = view.last_usage?.windows.find((x) => FIVE_HOUR_RE.test(x.label));
            if (!w) {
              delete next[id];
              continue;
            }
            const pct =
              w.used_percent ??
              (w.used !== null && w.limit && w.limit > 0
                ? (w.used / w.limit) * 100
                : null);
            if (pct !== null && Number.isFinite(pct)) {
              next[id] = Math.max(0, Math.min(100, pct));
            } else {
              delete next[id];
            }
          }
          return next;
        });
      } catch (err) {
        console.error("Failed to run background tracker refresh", err);
      }
    }, trackerRefreshInterval);

    return () => clearInterval(interval);
  }, [trackerRefreshInterval, trackerProviderIds]);

  // Listen for the custom tracker-changed event to sync updates immediately
  useEffect(() => {
    if (typeof window === "undefined") return;
    const handleTrackerChanged = () => {
      void refresh();
    };
    window.addEventListener("tracker-changed", handleTrackerChanged);
    return () => {
      window.removeEventListener("tracker-changed", handleTrackerChanged);
    };
  }, [refresh]);
  useEffect(() => {
    if (typeof window === "undefined") return;
    const threshold = 960;
    const lastWidth = { current: window.innerWidth };

    const handleResize = () => {
      const width = window.innerWidth;
      const wasWide = lastWidth.current >= threshold;
      const isNarrow = width < threshold;

      if (wasWide && isNarrow) {
        setSidebarCollapsed(true);
      } else if (!wasWide && !isNarrow) {
        setSidebarCollapsed(false);
      }
      lastWidth.current = width;
    };

    // Initial check
    if (window.innerWidth < threshold) {
      setSidebarCollapsed(true);
    }

    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);
  const keyringAvailable = keyring?.status === "available";

  const handleSelect = useCallback((id: string) => {
    setProviders((prev) => {
      const p = prev.find((x) => x.id === id);
      if (p) setMode({ kind: "editing", provider: p });
      return prev;
    });
  }, []);

  const handleNew = useCallback(() => {
    setMode({ kind: "creating" });
  }, []);

  const handleCancel = useCallback(() => {
    setMode({ kind: "idle" });
  }, []);

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed((c) => !c);
  }, []);

  const handleSave = useCallback(async (input: ProviderInput) => {
    if (!keyringAvailable) {
      toast.error("Cannot save: OS keyring is unavailable");
      return;
    }
    setSaving(true);
    try {
      const isEdit = mode.kind === "editing";
      if (isEdit) {
        await updateProvider(input);
        toast.success(`Updated “${input.name}”`);
        // If this is the active provider, reload it into settings.json to sync env changes
        if (input.id && input.id === active?.id) {
          await loadProvider(input.id);
        }
      } else {
        await addProvider(input);
        toast.success(`Created “${input.name}”`);
      }
      await refresh();
      setMode({ kind: "idle" });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setSaving(false);
    }
  }, [keyringAvailable, mode.kind, refresh, active]);

  const handleLoad = useCallback(async (id: string) => {
    setLoadingId(id);
    try {
      await loadProvider(id);
      toast.success("Provider loaded into settings.json");
      await refresh();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setLoadingId(null);
    }
  }, [refresh]);

  const handleDeleteConfirm = useCallback(async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await deleteProvider(deleteTarget.id);
      toast.success(`Deleted “${deleteTarget.name}”`);
      setDeleteTarget(null);
      setMode((currentMode) => {
        if (currentMode.kind === "editing" && currentMode.provider.id === deleteTarget.id) {
          return { kind: "idle" };
        }
        return currentMode;
      });
      await refresh();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setDeleting(false);
    }
  }, [deleteTarget, refresh]);

  const handleSubscriptionImported = useCallback(
    async (p: Provider) => {
      // Snapshot was written by the backend; refresh so the sidebar picks it
      // up, then switch to editing the new provider so the user can add
      // model overrides / label tweaks and hit Save.
      await refresh();
      setMode({ kind: "editing", provider: p });
    },
    [refresh],
  );

  const handleSaveCurrentAs = useCallback(async () => {
    const name = window.prompt("Name for this provider:");
    if (!name) return;
    try {
      const p = await saveCurrentAsProvider(name.trim());
      toast.success(`Saved current settings as “${p.name}”`);
      await refresh();
      setMode({ kind: "editing", provider: p });
      setCustomEnvKeys(null);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, [refresh]);

  const handleRevealAppDir = useCallback(async () => {
    if (!appDataDir) return;
    try {
      await revealInFileManager(appDataDir);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, [appDataDir]);

  const handleRevealClaudeDir = useCallback(async () => {
    if (!claudeDir) return;
    try {
      await revealInFileManager(claudeDir);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, [claudeDir]);

  const handleExport = useCallback(async (includeSecrets: boolean) => {
    if (includeSecrets) {
      const ok = window.confirm(
        "Including secrets will write auth tokens to a sidecar JSON file. " +
          "Share or commit this file carefully. Continue?",
      );
      if (!ok) return;
    }
    try {
      const dest = await saveDialog({
        title: "Export providers",
        defaultPath: "claude-config-providers.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!dest) return;
      await exportProviders(dest, includeSecrets);
      toast.success(
        includeSecrets
          ? "Exported (with secrets sidecar)"
          : "Exported (secrets redacted)",
      );
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, []);

  const handleImport = useCallback(async () => {
    try {
      const src = await openDialog({
        title: "Import providers",
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });
      if (!src || Array.isArray(src)) return;
      let secretsSrc: string | undefined;
      if (
        window.confirm(
          "Do you have a secrets sidecar (.secrets.json) from a previous export?",
        )
      ) {
        const ss = await openDialog({
          title: "Import secrets sidecar (optional)",
          filters: [{ name: "JSON", extensions: ["json"] }],
          multiple: false,
        });
        if (ss && !Array.isArray(ss)) secretsSrc = ss;
      }
      const added = await importProviders(src, secretsSrc);
      toast.success(`Imported ${added} provider${added === 1 ? "" : "s"}`);
      await refresh();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, [refresh]);

  return {
    mounted,
    ready,
    providers,
    active,
    customEnvKeys,
    keyring,
    appDataDir,
    claudeDir,
    mode,
    loadingId,
    saving,
    deleteTarget,
    deleting,
    keyringAvailable,
    session5hByProvider,
    setDeleteTarget,
    sidebarCollapsed,
    toggleSidebar,
    handleSelect,
    handleNew,
    handleCancel,
    handleSave,
    handleLoad,
    handleDeleteConfirm,
    handleSubscriptionImported,
    handleSaveCurrentAs,
    handleRevealAppDir,
    handleRevealClaudeDir,
    handleExport,
    handleImport,
    trackerRefreshInterval,
    setTrackerRefreshInterval,
  };
}
