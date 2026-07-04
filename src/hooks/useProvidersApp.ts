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
  loadProvider,
  revealInFileManager,
  saveCurrentAsProvider,
  updateProvider,
} from "@/lib/api";
import { isWebEnv } from "@/lib/utils-app";
import type { KeyringStatus, Provider, ProviderInput } from "@/lib/types";

export type AppMode =
  | { kind: "idle" }
  | { kind: "creating" }
  | { kind: "editing"; provider: Provider };

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

  const refresh = useCallback(async () => {
    try {
      const [list, activeP, krStatus, appDir, claudeD, envKeys] =
        await Promise.all([
          listProviders(),
          getActiveProvider(),
          keyringStatus(),
          getAppDataDir(),
          discoverClaudeDir(),
          getSettingsEnvKeys(),
        ]);
      setProviders(list);
      setActive(activeP);
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
  }, [keyringAvailable, mode.kind, refresh]);

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
    setDeleteTarget,
    sidebarCollapsed,
    toggleSidebar,
    handleSelect,
    handleNew,
    handleCancel,
    handleSave,
    handleLoad,
    handleDeleteConfirm,
    handleSaveCurrentAs,
    handleRevealAppDir,
    handleRevealClaudeDir,
    handleExport,
    handleImport,
  };
}
