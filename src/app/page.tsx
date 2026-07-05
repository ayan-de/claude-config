"use client";


import { PanelLeftOpen } from "lucide-react";

import { Button } from "@/components/ui/button";
import { isWebEnv } from "@/lib/utils-app";
import { useProvidersApp } from "@/hooks/useProvidersApp";
import { cn } from "@/lib/utils";


import { CustomConfigBanner } from "@/components/CustomConfigBanner";
import { DeleteDialog } from "@/components/DeleteDialog";
import { EmptyState } from "@/components/EmptyState";
import { KeyringWarning } from "@/components/KeyringWarning";
import { ProviderForm } from "@/components/ProviderForm";
import { ProviderList } from "@/components/ProviderList";
import { SettingsMenu } from "@/components/SettingsMenu";
import { TitleBar } from "@/components/TitleBar";

export default function Page() {
  const {
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
  } = useProvidersApp();

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
  const loadingProvider = providers.find((p) => p.id === loadingId) ?? null;
  const displayProvider = loadingProvider || active;

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <TitleBar
        left={
          <div className="flex items-center gap-3 shrink-0">
            {sidebarCollapsed && (
              <Button
                variant="ghost"
                className="h-7 w-7 p-0 tauri-no-drag shrink-0"
                onClick={toggleSidebar}
                title="Expand sidebar"
              >
                <PanelLeftOpen className="size-3.5" />
              </Button>
            )}
            <div className="flex items-center gap-3 shrink-0">
              <div className="flex size-8 shrink-0 items-center justify-center rounded-sm bg-[#f4f3ee]">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img
                  src="/logo.png"
                  alt="Claude Config"
                  className="size-7 object-contain shrink-0"
                />
              </div>
              <div className="flex flex-col justify-center shrink-0">
                <h1 className="text-sm font-semibold leading-none shrink-0">
                  Claude Config
                </h1>
                <p className="mt-0.5 text-[10px] text-muted-foreground hidden sm:block shrink-0">
                  Manage Claude Code providers
                </p>
              </div>
            </div>
          </div>
        }
        actions={
          <div className="flex items-center gap-2 pr-2">
            <SettingsMenu
              appDataDir={appDataDir}
              claudeDir={claudeDir}
              onRevealAppDir={handleRevealAppDir}
              onRevealClaudeDir={handleRevealClaudeDir}
              onExport={handleExport}
              onImport={handleImport}
            />
          </div>
        }
      />

      <div className="flex min-h-0 flex-1">
        <ProviderList
          collapsed={sidebarCollapsed}
          onToggleCollapse={toggleSidebar}
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

        <main className={cn("flex-1 overflow-y-auto p-6", !showForm && "flex flex-col justify-center")}>
          <div className={cn("mx-auto max-w-2xl w-full", !showForm ? "flex-1 flex flex-col justify-center space-y-6" : "space-y-4")}>
            <KeyringWarning status={keyring} />

            {customEnvKeys && !showForm && (
              <CustomConfigBanner
                envKeys={customEnvKeys}
                onSaveAs={handleSaveCurrentAs}
              />
            )}

            {!showForm && displayProvider && (
              <div className="rounded-xl border bg-card/45 p-5 mb-2">
                <p className="text-xs uppercase tracking-wider text-muted-foreground mb-3 select-none">
                  Active provider
                </p>
                <div className="flex items-center justify-between gap-4">
                  <div className="min-w-0">
                    <h3 className="text-sm font-semibold truncate leading-none flex items-center gap-1.5">
                      <span>{displayProvider.name}</span>
                      {!loadingId && (
                        // eslint-disable-next-line @next/next/no-img-element
                        <img
                          src="/tick.svg"
                          alt="Active"
                          className="size-3.5 object-contain shrink-0"
                        />
                      )}
                    </h3>
                    <p className="mt-2 truncate font-mono text-[10px] text-muted-foreground/80 leading-none">
                      {(() => {
                        try {
                          return new URL(displayProvider.base_url).host;
                        } catch {
                          return displayProvider.base_url;
                        }
                      })()}
                    </p>
                  </div>
                  <span
                    className={cn(
                      "text-[10px] font-medium px-2.5 py-0.5 rounded-full shrink-0 border select-none transition-all duration-150",
                      loadingId
                        ? "bg-amber-500/10 text-amber-400 border-amber-500/20 animate-pulse"
                        : "bg-[#c15f3c]/10 text-[#c15f3c] border-[#c15f3c]/20",
                    )}
                  >
                    {loadingId ? "switching…" : "connected"}
                  </span>
                </div>
              </div>
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