"use client";

import { isWebEnv } from "@/lib/utils-app";
import { useProvidersApp } from "@/hooks/useProvidersApp";
import { useDangerousMode } from "@/hooks/useDangerousMode";
import { useGlobalPanel } from "@/hooks/useGlobalPanel";
import { useUpdater } from "@/hooks/useUpdater";

import { SettingsMenu } from "@/components/SettingsMenu";
import { Sidebar } from "@/components/Sidebar";
import { Main } from "@/components/Main";
import { TitleBar } from "@/components/TitleBar";
import { DangerousModeConfirm } from "@/components/DangerousModeConfirm";

import { GLOBAL_TABS } from "@/data/globalTabs";

export default function Page() {
  const providers = useProvidersApp();
  const panel = useGlobalPanel();
  const updater = useUpdater();
  const dangerous = useDangerousMode();

  if (!providers.mounted) {
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

  if (!providers.ready) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  const activeTab =
    GLOBAL_TABS.find((t) => t.id === panel.activeTabId) ?? null;
  const editingProvider =
    providers.mode.kind === "editing" ? providers.mode.provider : null;

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <TitleBar
        left={
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
        }
        actions={
          <div className="flex items-center gap-2 pr-2">
            <SettingsMenu
              appDataDir={providers.appDataDir}
              claudeDir={providers.claudeDir}
              updateAvailable={updater.available}
              updateError={updater.error}
              onRevealAppDir={providers.handleRevealAppDir}
              onRevealClaudeDir={providers.handleRevealClaudeDir}
              onExport={providers.handleExport}
              onImport={providers.handleImport}
              onCheckForUpdates={updater.checkNow}
              dangerousMode={dangerous.enabled}
              onToggleDangerousMode={dangerous.toggle}
            />
          </div>
        }
      />

      <div className="flex min-h-0 flex-1 relative">
        <Sidebar
          collapsed={providers.sidebarCollapsed}
          onToggleCollapse={providers.toggleSidebar}
          providers={providers.providers}
          activeProviderId={providers.active?.id ?? null}
          editingProviderId={editingProvider?.id ?? null}
          loadingProviderId={providers.loadingId}
          showEditor={panel.isOpen}
          onSelectProvider={providers.handleSelect}
          onLoadProvider={providers.handleLoad}
          onDeleteProvider={(id) => {
            const p = providers.providers.find((x) => x.id === id);
            if (p) providers.setDeleteTarget(p);
          }}
          onNewProvider={providers.handleNew}
          panel={{
            activeTabId: panel.activeTabId,
            openTab: panel.openTab,
            close: panel.close,
          }}
        />

        <Main
          mode={providers.mode}
          panel={{ activeTab, closeTab: panel.close }}
          providers={providers.providers}
          activeProvider={providers.active}
          loadingProviderId={providers.loadingId}
          saving={providers.saving}
          deleting={providers.deleting}
          keyring={providers.keyring}
          customEnvKeys={providers.customEnvKeys}
          updateAvailable={updater.available}
          updateVersion={updater.version}
          updateDismissed={updater.dismissed}
          updateDownloading={updater.downloading}
          deleteTarget={providers.deleteTarget}
          setDeleteTarget={providers.setDeleteTarget}
          onCancelProviderForm={providers.handleCancel}
          onSaveProviderForm={providers.handleSave}
          onSubscriptionImported={providers.handleSubscriptionImported}
          onNewProvider={providers.handleNew}
          onSaveCurrentAs={providers.handleSaveCurrentAs}
          onInstallUpdate={updater.installUpdate}
          onDismissUpdate={updater.dismiss}
          onDeleteProvider={providers.handleDeleteConfirm}
        />
      </div>
      <DangerousModeConfirm
        open={dangerous.confirmOpen}
        onConfirm={dangerous.confirm}
        onCancel={dangerous.dismissConfirm}
      />
    </div>
  );
}
