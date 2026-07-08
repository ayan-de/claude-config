"use client";

import { CustomConfigBanner } from "@/components/CustomConfigBanner";
import { DeleteDialog } from "@/components/DeleteDialog";
import { EmptyState } from "@/components/EmptyState";
import { KeyringWarning } from "@/components/KeyringWarning";
import { ProviderForm } from "@/components/ProviderForm";
import { ProviderLogo } from "@/components/ProviderLogo";
import { UpdateBanner } from "@/components/UpdateBanner";
import type { GlobalTab } from "@/data/globalTabs";
import type { KeyringStatus, Provider, ProviderInput } from "@/lib/types";
import { kindLabel, providerSubtitle } from "@/lib/utils-app";
import { cn } from "@/lib/utils";

export interface MainPanel {
  activeTab: GlobalTab | null;
  closeTab: () => void;
}

interface Props {
  mode:
    | { kind: "idle" }
    | { kind: "creating" }
    | { kind: "editing"; provider: Provider };
  panel: MainPanel;
  providers: Provider[];
  activeProvider: Provider | null;
  loadingProviderId: string | null;
  saving: boolean;
  deleting: boolean;
  keyring: KeyringStatus | null;
  customEnvKeys: string[] | null;
  updateAvailable: boolean;
  updateVersion: string | null;
  updateDismissed: boolean;
  updateDownloading: boolean;
  deleteTarget: Provider | null;
  setDeleteTarget: (p: Provider | null) => void;
  onCancelProviderForm: () => void;
  onSaveProviderForm: (input: ProviderInput) => Promise<void>;
  onSubscriptionImported: (p: Provider) => Promise<void>;
  onNewProvider: () => void;
  onSaveCurrentAs: () => void;
  onInstallUpdate: () => Promise<void>;
  onDismissUpdate: () => void;
  onDeleteProvider: () => Promise<void>;
}

/**
 * Main content area: top banners, the active-provider card, the form/editor/
 * empty-state switch, and the delete-confirmation dialog. Composition is
 * data-driven off `mode` and `panel` so the page stays a pure orchestrator.
 */
export function Main({
  mode,
  panel,
  providers,
  activeProvider,
  loadingProviderId,
  saving,
  deleting,
  keyring,
  customEnvKeys,
  updateAvailable,
  updateVersion,
  updateDismissed,
  updateDownloading,
  deleteTarget,
  setDeleteTarget,
  onCancelProviderForm,
  onSaveProviderForm,
  onSubscriptionImported,
  onNewProvider,
  onSaveCurrentAs,
  onInstallUpdate,
  onDismissUpdate,
  onDeleteProvider,
}: Props) {
  const showForm = mode.kind === "creating" || mode.kind === "editing";
  const showEditor = panel.activeTab !== null;
  const editingProvider = mode.kind === "editing" ? mode.provider : null;
  const loadingProvider =
    providers.find((p) => p.id === loadingProviderId) ?? null;
  const displayProvider = loadingProvider || activeProvider;
  const isCentered = !showForm && !showEditor;

  return (
    <>
      <main
        className={cn(
          "flex-1 overflow-y-auto p-6",
          isCentered && "flex flex-col justify-center",
        )}
      >
        <div
          className={cn(
            "mx-auto w-full",
            showEditor ? "max-w-4xl" : "max-w-2xl",
            isCentered
              ? "flex-1 flex flex-col justify-center space-y-6"
              : "space-y-4",
          )}
        >
          {!showEditor && <KeyringWarning status={keyring} />}

          {updateAvailable &&
            updateVersion &&
            !updateDismissed &&
            !showForm &&
            !showEditor && (
              <UpdateBanner
                version={updateVersion}
                downloading={updateDownloading}
                onInstall={() => void onInstallUpdate()}
                onDismiss={onDismissUpdate}
              />
            )}

          {customEnvKeys && !showForm && !showEditor && (
            <CustomConfigBanner
              envKeys={customEnvKeys}
              onSaveAs={onSaveCurrentAs}
            />
          )}

          {!showForm && !showEditor && displayProvider && (
            <div className="rounded-xl border bg-card/45 p-5 mb-2">
              <p className="text-xs uppercase tracking-wider text-muted-foreground mb-3 select-none">
                Active provider
              </p>
              <div className="flex items-center justify-between gap-4">
                <div className="flex min-w-0 items-center gap-3">
                  <ProviderLogo
                    svg={displayProvider.logoSvg}
                    size={32}
                    className="rounded"
                  />
                  <div className="min-w-0">
                    <h3 className="text-sm font-semibold truncate leading-none flex items-center gap-1.5">
                      <span>{displayProvider.name}</span>
                      {!loadingProviderId && (
                        // eslint-disable-next-line @next/next/no-img-element
                        <img
                          src="/tick.svg"
                          alt="Active"
                          className="size-3.5 object-contain shrink-0"
                        />
                      )}
                    </h3>
                    <p className="mt-2 truncate font-mono text-[10px] text-muted-foreground/80 leading-none">
                      {providerSubtitle(displayProvider)}
                    </p>
                  </div>
                </div>
                <div className="flex flex-col items-end gap-1 shrink-0">
                  <span
                    className={cn(
                      "text-[10px] font-medium px-2.5 py-0.5 rounded-sm border select-none transition-all duration-150",
                      loadingProviderId
                        ? "bg-amber-500/10 text-amber-400 border-amber-500/20 animate-pulse"
                        : "bg-primary/10 text-primary border-primary/20",
                    )}
                  >
                    {loadingProviderId ? "switching…" : "connected"}
                  </span>
                  <span className="text-[9px] uppercase tracking-wider text-muted-foreground/70 select-none">
                    {kindLabel(displayProvider.kind)}
                  </span>
                </div>
              </div>
            </div>
          )}

          {panel.activeTab ? (
            <panel.activeTab.Component onClose={panel.closeTab} />
          ) : showForm ? (
            <ProviderForm
              key={editingProvider?.id ?? "new"}
              editing={editingProvider}
              onCancel={onCancelProviderForm}
              onSave={onSaveProviderForm}
              onSubscriptionImported={onSubscriptionImported}
              onDelete={() => {
                if (editingProvider) setDeleteTarget(editingProvider);
              }}
              isSaving={saving}
            />
          ) : (
            <EmptyState
              hasProviders={providers.length > 0}
              onNew={onNewProvider}
            />
          )}
        </div>
      </main>

      <DeleteDialog
        open={!!deleteTarget}
        providerName={deleteTarget?.name ?? ""}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
        onConfirm={onDeleteProvider}
        isDeleting={deleting}
      />
    </>
  );
}
