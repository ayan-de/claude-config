"use client";

import { History } from "lucide-react";

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
import { GitHubTopBarButton } from "@/components/GitHubTopBarButton";
import { GitHubSyncProvider, useGitHubSyncContext } from "@/hooks/GitHubSyncContext";

import { GLOBAL_TABS } from "@/data/globalTabs";
import { version as appVersion } from "../../package.json";
import { cn } from "@/lib/utils";

export default function Page() {
  // Provider must wrap the consumer. Splitting into an outer wrapper + an
  // inner shell keeps the hook call inside the provider context.
  return (
    <GitHubSyncProvider>
      <PageShell />
    </GitHubSyncProvider>
  );
}

function PageShell() {
  const providers = useProvidersApp();
  const panel = useGlobalPanel();
  const updater = useUpdater();
  const dangerous = useDangerousMode();
  const github = useGitHubSyncContext();

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
            <GitHubTopBarButton
              config={github.config}
              loading={github.loading}
              active={panel.activeTabId === "github-sync"}
              onClick={() =>
                panel.activeTabId === "github-sync"
                  ? panel.close()
                  : panel.openTab("github-sync")
              }
            />
            <button
              type="button"
              onClick={() =>
                panel.activeTabId === "sessions"
                  ? panel.close()
                  : panel.openTab("sessions")
              }
              title={
                panel.activeTabId === "sessions"
                  ? "Close Sessions"
                  : "Open Sessions"
              }
              aria-label="Sessions"
              aria-pressed={panel.activeTabId === "sessions"}
              className={cn(
                "tauri-no-drag flex size-7 items-center justify-center rounded-md transition shrink-0 cursor-pointer",
                panel.activeTabId === "sessions"
                  ? "bg-primary/15 text-primary"
                  : "text-foreground/70 hover:bg-foreground/10 hover:text-foreground",
              )}
            >
              <History className="size-3.5" />
            </button>
            {updater.available && updater.version ? (
              <button
                type="button"
                onClick={() => void updater.installUpdate()}
                disabled={updater.downloading}
                className="rounded-sm border border-foreground/10 bg-foreground/5 px-2 py-0.5 text-xs font-mono text-foreground backdrop-blur-sm transition hover:bg-foreground/10 disabled:opacity-60"
                title="Click to update now"
              >
                Update Now (v{appVersion} → v{updater.version})
              </button>
            ) : (
              <span className="text-xs text-muted-foreground font-mono">
                v{appVersion}
              </span>
            )}
            <SettingsMenu
              appDataDir={providers.appDataDir}
              claudeDir={providers.claudeDir}
              updateAvailable={updater.available}
              updateVersion={updater.version}
              updateDownloading={updater.downloading}
              updateError={updater.error}
              onRevealAppDir={providers.handleRevealAppDir}
              onRevealClaudeDir={providers.handleRevealClaudeDir}
              onExport={providers.handleExport}
              onImport={providers.handleImport}
              onCheckForUpdates={updater.checkNow}
              onInstallUpdate={updater.installUpdate}
              dangerousMode={dangerous.enabled}
              onToggleDangerousMode={dangerous.toggle}
              trackerRefreshInterval={providers.trackerRefreshInterval}
              onTrackerRefreshIntervalChange={providers.setTrackerRefreshInterval}
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
          session5hByProvider={providers.session5hByProvider}
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
