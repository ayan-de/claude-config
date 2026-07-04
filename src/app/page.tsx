"use client";

/* eslint-disable react-hooks/set-state-in-effect --
 * The data-fetching useEffect here triggers an initial provider/keyring
 * load on mount. The recommended alternative (useSyncExternalStore) adds
 * complexity for a one-shot initial fetch.
 */

import { useCallback, useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Plus } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
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

import { ActiveBanner } from "@/components/ActiveBanner";
import { CustomConfigBanner } from "@/components/CustomConfigBanner";
import { DeleteDialog } from "@/components/DeleteDialog";
import { EmptyState } from "@/components/EmptyState";
import { KeyringWarning } from "@/components/KeyringWarning";
import { ProviderForm } from "@/components/ProviderForm";
import { ProviderList } from "@/components/ProviderList";
import { SettingsMenu } from "@/components/SettingsMenu";

type Mode =
  | { kind: "idle" }
  | { kind: "creating" }
  | { kind: "editing"; provider: Provider };

export default function Page() {
  const [mounted, setMounted] = useState(false);
  const [ready, setReady] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [active, setActive] = useState<Provider | null>(null);
  const [customEnvKeys, setCustomEnvKeys] = useState<string[] | null>(null);
  const [keyring, setKeyring] = useState<KeyringStatus | null>(null);
  const [appDataDir, setAppDataDir] = useState<string | null>(null);
  const [claudeDir, setClaudeDir] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>({ kind: "idle" });
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

  function handleSelect(id: string) {
    const p = providers.find((x) => x.id === id);
    if (p) setMode({ kind: "editing", provider: p });
  }

  function handleNew() {
    setMode({ kind: "creating" });
  }

  function handleCancel() {
    setMode({ kind: "idle" });
  }

  async function handleSave(input: ProviderInput) {
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
  }

  async function handleLoad(id: string) {
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
  }

  async function handleDeleteConfirm() {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await deleteProvider(deleteTarget.id);
      toast.success(`Deleted “${deleteTarget.name}”`);
      setDeleteTarget(null);
      if (mode.kind === "editing" && mode.provider.id === deleteTarget.id) {
        setMode({ kind: "idle" });
      }
      await refresh();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setDeleting(false);
    }
  }

  async function handleSaveCurrentAs() {
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
  }

  async function handleRevealAppDir() {
    if (!appDataDir) return;
    try {
      await revealInFileManager(appDataDir);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function handleRevealClaudeDir() {
    if (!claudeDir) return;
    try {
      await revealInFileManager(claudeDir);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function handleExport(includeSecrets: boolean) {
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
  }

  async function handleImport() {
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
  }

  if (!mounted) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  if (!isWebEnv()) {
    return (
      <div className="flex h-full items-center justify-center p-12">
        <div className="max-w-md space-y-3 text-center">
          <h1 className="text-lg font-semibold">Run inside Tauri</h1>
          <p className="text-sm text-muted-foreground">
            This is the desktop UI. Launch with{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
              pnpm tauri dev
            </code>
            .
          </p>
        </div>
      </div>
    );
  }

  if (!ready) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  const showForm = mode.kind !== "idle";
  const editingProvider = mode.kind === "editing" ? mode.provider : null;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b bg-card/30 px-4 py-2.5">
        <div className="flex items-center gap-3">
          <div className="size-6 rounded-md bg-foreground" />
          <div>
            <h1 className="text-sm font-semibold leading-none">
              Claude Config
            </h1>
            <p className="mt-0.5 text-[10px] text-muted-foreground">
              Manage Claude Code providers
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" onClick={handleNew} disabled={!keyringAvailable}>
            <Plus className="size-3.5" />
            New provider
          </Button>
          <SettingsMenu
            appDataDir={appDataDir}
            claudeDir={claudeDir}
            onRevealAppDir={handleRevealAppDir}
            onRevealClaudeDir={handleRevealClaudeDir}
            onExport={handleExport}
            onImport={handleImport}
          />
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <ProviderList
          providers={providers}
          activeProviderId={active?.id ?? null}
          selectedId={editingProvider?.id ?? null}
          loadingId={loadingId}
          onSelect={handleSelect}
          onLoad={handleLoad}
          onDelete={(id) => {
            const p = providers.find((x) => x.id === id);
            if (p) setDeleteTarget(p);
          }}
          onNew={handleNew}
        />

        <main className="flex-1 overflow-y-auto p-6">
          <div className="mx-auto max-w-2xl space-y-4">
            <KeyringWarning status={keyring} />

            {active && !showForm && (
              <ActiveBanner provider={active} />
            )}

            {customEnvKeys && !showForm && (
              <CustomConfigBanner
                envKeys={customEnvKeys}
                onSaveAs={handleSaveCurrentAs}
              />
            )}

            {showForm ? (
              <ProviderForm
                editing={editingProvider}
                onCancel={handleCancel}
                onSave={handleSave}
                isSaving={saving}
              />
            ) : (
              <EmptyState hasProviders={providers.length > 0} onNew={handleNew} />
            )}
          </div>
        </main>
      </div>

      <DeleteDialog
        open={!!deleteTarget}
        providerName={deleteTarget?.name ?? ""}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
        onConfirm={handleDeleteConfirm}
        isDeleting={deleting}
      />
    </div>
  );
}