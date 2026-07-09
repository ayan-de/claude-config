"use client";

import { useCallback, useMemo } from "react";
import {
  BarChart3,
  Globe,
  Layers,
  PanelLeftOpen,
  Plus,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { ProviderCard } from "@/components/ProviderCard";
import { ProviderList, type SidebarSection } from "@/components/ProviderList";
import { GLOBAL_TABS } from "@/data/globalTabs";
import type { Provider } from "@/lib/types";
import { cn } from "@/lib/utils";

export interface SidebarPanel {
  activeTabId: string | null;
  openTab: (id: string) => void;
  close: () => void;
}

interface Props {
  collapsed: boolean;
  onToggleCollapse: () => void;
  providers: Provider[];
  /** ID of the provider currently loaded into settings.json (highlight chip). */
  activeProviderId: string | null;
  /** ID of the provider being edited in <ProviderForm>. Drives the selected state. */
  editingProviderId: string | null;
  loadingProviderId: string | null;
  /** Per-provider 5h-session usage percent (0..100) for the sidebar bar.
   *  Absent keys mean "no tracker configured". */
  session5hByProvider: Record<string, number>;
  /** True when a global tab (CLAUDE.md etc.) is open — de-highlights providers. */
  showEditor: boolean;
  onSelectProvider: (id: string) => void;
  onLoadProvider: (id: string) => void;
  onDeleteProvider: (id: string) => void;
  onNewProvider: () => void;
  panel: SidebarPanel;
}

/**
 * Owns the workspace sidebar: floating expand-when-collapsed toggle button and
 * the registry-driven <ProviderList>. Adding a new global tab = edit this file
 * (one new section) and `data/globalTabs.ts` (one new entry).
 */
export function Sidebar({
  collapsed,
  onToggleCollapse,
  providers,
  activeProviderId,
  editingProviderId,
  loadingProviderId,
  session5hByProvider,
  showEditor,
  onSelectProvider,
  onLoadProvider,
  onDeleteProvider,
  onNewProvider,
  panel,
}: Props) {
  // Hoist panel primitives so the memo dep list sees stable refs (the parent
  // re-creates the `panel` object every render, which would defeat memo).
  const { activeTabId, openTab, close } = panel;

  // Clicking a provider while a global tab is open must close the tab — the
  // user is choosing to navigate to that provider, not stay on CLAUDE.md /
  // Marketplace. Without this, Main keeps rendering the tab content because
  // the `panel.activeTab` branch wins over the form branch.
  const handleSelect = useCallback(
    (id: string) => {
      if (activeTabId !== null) close();
      onSelectProvider(id);
    },
    [activeTabId, close, onSelectProvider],
  );

  const handleLoad = useCallback(
    (id: string) => {
      if (activeTabId !== null) close();
      onLoadProvider(id);
    },
    [activeTabId, close, onLoadProvider],
  );

  const handleNew = useCallback(() => {
    if (activeTabId !== null) close();
    onNewProvider();
  }, [activeTabId, close, onNewProvider]);

  const handleUsage = useCallback(() => {
    if (activeTabId === "usage") close();
    else openTab("usage");
  }, [activeTabId, openTab, close]);

  const sections = useMemo<SidebarSection[]>(
    () => [
      {
        id: "providers",
        label: "Providers",
        icon: Layers,
        content: (
          <div className="flex flex-col gap-2.5">
            {providers.length === 0 ? (
              <div className="flex flex-col items-center gap-2.5 rounded-lg border border-dashed p-4 text-center">
                <p className="text-[10px] text-muted-foreground">
                  No providers configured
                </p>
                <div className="flex items-center gap-2">
                  <Button
                    size="xs"
                    variant="outline"
                    onClick={handleUsage}
                    title="View usage"
                    className="cursor-pointer"
                  >
                    <BarChart3 className="size-3" />
                    Usage
                  </Button>
                  <Button
                    size="xs"
                    variant="default"
                    onClick={handleNew}
                    title="New provider"
                    className="cursor-pointer dark:bg-secondary dark:text-secondary-foreground dark:hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)]"
                  >
                    <Plus className="size-3" />
                    New
                  </Button>
                </div>
              </div>
            ) : (
              <>
                {providers.map((p) => (
                  <ProviderCard
                    key={p.id}
                    provider={p}
                    isActive={p.id === activeProviderId}
                    isSelected={!showEditor && p.id === editingProviderId}
                    isLoading={p.id === loadingProviderId}
                    session5hPct={session5hByProvider[p.id] ?? null}
                    onSelect={() => handleSelect(p.id)}
                    onLoad={() => handleLoad(p.id)}
                    onDelete={() => onDeleteProvider(p.id)}
                  />
                ))}
                <div className="flex items-center justify-center gap-2">
                  <Button
                    size="xs"
                    variant="outline"
                    onClick={handleUsage}
                    title="View usage"
                    className="cursor-pointer"
                  >
                    <BarChart3 className="size-3" />
                    Usage
                  </Button>
                  <Button
                    size="xs"
                    variant="default"
                    onClick={handleNew}
                    title="New provider"
                    className="cursor-pointer dark:bg-secondary dark:text-secondary-foreground dark:hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)]"
                  >
                    <Plus className="size-3" />
                    New
                  </Button>
                </div>
              </>
            )}
          </div>
        ),
      },
      {
        id: "global",
        label: "Global Config",
        icon: Globe,
        headerTooltip:
          "Manage global config files shared across all providers.",
        content: (
          <div className="flex flex-col gap-1.5">
            {GLOBAL_TABS.filter((t) => !t.hideInSidebar).map((tab) => (
              <tab.SidebarButton
                key={tab.id}
                active={activeTabId === tab.id}
                onSelect={() =>
                  activeTabId === tab.id ? close() : openTab(tab.id)
                }
              />
            ))}
          </div>
        ),
      },
    ],
    [
      providers,
      activeProviderId,
      editingProviderId,
      loadingProviderId,
      showEditor,
      session5hByProvider,
      activeTabId,
      openTab,
      close,
      onDeleteProvider,
      handleSelect,
      handleLoad,
      handleNew,
      handleUsage,
    ],
  );

  return (
    <>
      {collapsed && (
        <button
          onClick={onToggleCollapse}
          title="Expand sidebar"
          className={cn(
            "absolute left-4 top-4 z-40 size-8 rounded-full border bg-popover hover:bg-muted shadow-md flex items-center justify-center cursor-pointer transition-all duration-200 hover:scale-105 active:scale-95 text-muted-foreground hover:text-foreground",
          )}
        >
          <PanelLeftOpen className="size-4" />
        </button>
      )}

      <ProviderList
        collapsed={collapsed}
        onToggleCollapse={onToggleCollapse}
        sections={sections}
      />
    </>
  );
}
