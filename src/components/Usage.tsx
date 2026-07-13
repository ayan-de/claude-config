"use client";

/* eslint-disable react-hooks/set-state-in-effect --
 * One intentional setState-in-effect site: the initial usage fetch that runs
 * when the view mounts. Sync-from-external-state pattern matching the other
 * global tabs (useMcpServers / useMarketplaces). */

import { useCallback, useEffect, useState } from "react";
import {
  ArrowLeft,
  BarChart3,
  ChevronDown,
  ChevronRight,
  Loader2,
  RefreshCw,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { ProviderLogo } from "@/components/ProviderLogo";
import { listProviders, listTrackerUsage, refreshTracker } from "@/lib/api";
import { resolveProviderLogo } from "@/lib/presetProviders";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
} from "@/data/globalTabs";
import type { Provider, TrackerUsage, TrackerUsageWindow } from "@/lib/types";
import { cn } from "@/lib/utils";

/**
 * Sidebar entry — same visual shape as McpSidebarButton / MarketplaceSidebarButton
 * (icon + label, pill highlight when active). No "+ Add" affordance here because
 * the tracker add/edit flow lives inside each provider's form.
 */
export function UsageSidebarButton({
  active,
  onSelect,
}: SidebarTabButtonProps) {
  return (
    <button
      onClick={onSelect}
      className={cn(
        "w-full flex items-center gap-2 px-3 py-2 rounded-lg border text-left text-xs font-medium transition-all cursor-pointer group",
        active
          ? "bg-primary/10 border-primary/20 text-primary shadow-2xs"
          : "bg-card/50 border-border/60 text-muted-foreground hover:bg-card hover:border-foreground/20 hover:text-foreground",
      )}
    >
      <BarChart3
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">Usage</span>
    </button>
  );
}

interface RefreshState {
  [providerId: string]: boolean | undefined;
}

export function UsageView({ onClose }: GlobalTabProps) {
  const [usageByProvider, setUsageByProvider] = useState<
    Record<string, TrackerUsage>
  >({});
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState<RefreshState>({});
  const [providers, setProviders] = useState<Provider[]>([]);
  // Per-provider display collapse for the model breakdown list.
  const [modelsOpen, setModelsOpen] = useState<Record<string, boolean>>({});

  const providerById = new Map(providers.map((p) => [p.id, p]));

  const loadUsage = useCallback(async () => {
    setLoading(true);
    try {
      const [map, rawProviders] = await Promise.all([
        listTrackerUsage(),
        listProviders(),
      ]);
      // Same enrichment as `useProvidersApp` — resolve missing logos so each
      // card shows the right avatar instead of the gray placeholder dot.
      const enriched = await Promise.all(
        rawProviders.map(async (p) => {
          if (p.logoSvg) return p;
          const svg = await resolveProviderLogo(p);
          return svg ? { ...p, logoSvg: svg } : p;
        }),
      );
      setUsageByProvider(map);
      setProviders(enriched);
    } catch (e) {
      toast.error(`Failed to load usage: ${(e as Error).message}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadUsage();
  }, [loadUsage]);

  const handleRefreshOne = useCallback(async (providerId: string) => {
    setRefreshing((s) => ({ ...s, [providerId]: true }));
    try {
      const view = await refreshTracker(providerId);
      if (view?.last_usage) {
        setUsageByProvider((prev) => ({
          ...prev,
          [providerId]: view.last_usage as TrackerUsage,
        }));
      } else {
        setUsageByProvider((prev) => {
          const next = { ...prev };
          delete next[providerId];
          return next;
        });
      }
      // Sync the sidebar's `useProvidersApp` cache with the fresh usage.
      if (typeof window !== "undefined") {
        window.dispatchEvent(new CustomEvent("tracker-changed"));
      }
    } catch (e) {
      toast.error(`Refresh failed: ${(e as Error).message}`);
    } finally {
      setRefreshing((s) => ({ ...s, [providerId]: false }));
    }
  }, []);

  const handleRefreshAll = useCallback(async () => {
    const tracked = Object.keys(usageByProvider);
    if (tracked.length === 0) {
      await loadUsage();
      return;
    }
    setRefreshing(Object.fromEntries(tracked.map((id) => [id, true])));
    const results = await Promise.allSettled(
      tracked.map(async (id) => ({ id, view: await refreshTracker(id) })),
    );
    setUsageByProvider((prev) => {
      const next = { ...prev };
      for (const r of results) {
        if (r.status !== "fulfilled") continue;
        const { id, view } = r.value;
        if (view?.last_usage) {
          next[id] = view.last_usage as TrackerUsage;
        } else {
          delete next[id];
        }
      }
      return next;
    });
    // Sync the sidebar's `useProvidersApp` cache with the fresh usage.
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("tracker-changed"));
    }
    setRefreshing({});
  }, [usageByProvider, loadUsage]);

  const trackedIds = Object.keys(usageByProvider);
  const hasAnyUsage = trackedIds.length > 0;
  const anyRefreshing = Object.values(refreshing).some(Boolean);

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <Button size="sm" variant="ghost" onClick={onClose}>
            <ArrowLeft className="size-3.5" />
          </Button>
          <BarChart3 className="size-4 text-primary" />
          <div>
            <h2 className="text-sm font-semibold leading-none">Usage</h2>
            <p className="mt-1 text-[11px] text-muted-foreground">
              The latest cached snapshot for every provider with a tracker
              configured.
            </p>
          </div>
        </div>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void handleRefreshAll()}
          disabled={loading || anyRefreshing}
          className="cursor-pointer"
          title="Refresh all"
        >
          {loading || anyRefreshing ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <RefreshCw className="size-3.5" />
          )}
          Refresh all
        </Button>
      </div>

      {loading && !hasAnyUsage ? (
        <LoadingState />
      ) : !hasAnyUsage ? (
        <EmptyState hasProviders={providers.length > 0} />
      ) : (
        <div className="space-y-3">
          {trackedIds.map((id) => {
            const provider = providerById.get(id);
            const usage = usageByProvider[id];
            const isRefreshing = !!refreshing[id];
            return (
              <ProviderUsageCard
                key={id}
                providerName={provider?.name ?? id}
                providerLogoSvg={provider?.logoSvg ?? null}
                usage={usage}
                isRefreshing={isRefreshing}
                modelsOpen={!!modelsOpen[id]}
                onToggleModels={() =>
                  setModelsOpen((m) => ({ ...m, [id]: !m[id] }))
                }
                onRefresh={() => void handleRefreshOne(id)}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

function ProviderUsageCard({
  providerName,
  providerLogoSvg,
  usage,
  isRefreshing,
  modelsOpen,
  onToggleModels,
  onRefresh,
}: {
  providerName: string;
  providerLogoSvg: string | null;
  usage: TrackerUsage;
  isRefreshing: boolean;
  modelsOpen: boolean;
  onToggleModels: () => void;
  onRefresh: () => void;
}) {
  return (
    <div className="space-y-2.5 rounded-lg border bg-card/40 p-3.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <div className="size-7 shrink-0 rounded-md border bg-muted/20 flex items-center justify-center overflow-hidden">
            <ProviderLogo svg={providerLogoSvg} size={16} className="rounded" />
          </div>
          <p className="text-xs font-semibold truncate">{providerName}</p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span className="text-[10px] text-muted-foreground tabular-nums">
            {formatRelative(usage.fetched_at)}
          </span>
          <Button
            size="icon-xs"
            variant="ghost"
            onClick={onRefresh}
            disabled={isRefreshing}
            className="cursor-pointer"
            title="Refresh"
          >
            {isRefreshing ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <RefreshCw className="size-3" />
            )}
          </Button>
        </div>
      </div>

      {usage.note && (
        <p className="text-[10px] italic text-muted-foreground/80">
          {usage.note}
        </p>
      )}

      {usage.windows.length > 0 && (
        <div className="space-y-2">
          {usage.windows.map((w, i) => (
            <WindowRow key={`${w.label}-${i}`} window={w} />
          ))}
        </div>
      )}

      {usage.cost_usd !== null && (
        <div className="flex items-center justify-between rounded-md bg-muted/30 px-3 py-1.5 text-xs">
          <span className="text-muted-foreground">Total cost</span>
          <span className="font-mono font-medium tabular-nums">
            ${usage.cost_usd.toFixed(2)}
          </span>
        </div>
      )}

      {usage.models.length > 0 && (
        <ModelBreakdown
          models={usage.models}
          open={modelsOpen}
          onToggle={onToggleModels}
        />
      )}
    </div>
  );
}

function WindowRow({ window: w }: { window: TrackerUsageWindow }) {
  const pct = w.used_percent ?? computePercent(w.used, w.limit);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-[11px]">
        <span className="font-medium text-foreground/90">{w.label}</span>
        <span className="font-mono tabular-nums text-muted-foreground">
          {formatUsed(w)} {w.unit ?? ""}
          {pct !== null && (
            <span className="ml-1.5 text-foreground/70">
              ({pct.toFixed(0)}%)
            </span>
          )}
        </span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full transition-all duration-500",
            pct === null
              ? "bg-muted-foreground/30"
              : pct >= 90
                ? "bg-red-500/80"
                : pct >= 70
                  ? "bg-amber-500/80"
                  : "bg-emerald-500/80",
          )}
          style={{ width: pct === null ? "100%" : `${Math.min(100, pct)}%` }}
        />
      </div>
      {w.resets_at && (
        <p className="text-[10px] text-muted-foreground/80">
          Resets {formatRelative(w.resets_at)}
        </p>
      )}
    </div>
  );
}

function ModelBreakdown({
  models,
  open,
  onToggle,
}: {
  models: TrackerUsage["models"];
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="rounded-md border bg-background/40">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between px-3 py-1.5 text-left text-[11px] font-medium text-foreground/90 hover:bg-muted/40 cursor-pointer"
        aria-expanded={open}
      >
        <span className="inline-flex items-center gap-1.5">
          {open ? (
            <ChevronDown className="size-3" />
          ) : (
            <ChevronRight className="size-3" />
          )}
          Models ({models.length})
        </span>
      </button>
      {open && (
        <ul className="divide-y divide-border/40">
          {models.map((m) => (
            <li
              key={m.model}
              className="flex items-center justify-between px-3 py-1.5 text-[11px] font-mono"
            >
              <span className="truncate">{m.model}</span>
              <span className="flex items-center gap-3 tabular-nums text-muted-foreground">
                {m.input_tokens !== null && (
                  <span title="input tokens">
                    in {m.input_tokens.toLocaleString()}
                  </span>
                )}
                {m.output_tokens !== null && (
                  <span title="output tokens">
                    out {m.output_tokens.toLocaleString()}
                  </span>
                )}
                {m.cost_usd !== null && (
                  <span className="text-foreground/80">
                    ${m.cost_usd.toFixed(2)}
                  </span>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function LoadingState() {
  return (
    <div className="flex items-center justify-center gap-2 rounded-lg border border-dashed bg-card/30 py-8 text-xs text-muted-foreground">
      <Loader2 className="size-3.5 animate-spin" />
      Loading usage…
    </div>
  );
}

function EmptyState({ hasProviders }: { hasProviders: boolean }) {
  return (
    <div className="rounded-lg border border-dashed bg-card/30 p-8 text-center">
      <div className="mx-auto flex max-w-sm flex-col items-center gap-2">
        <BarChart3 className="size-5 text-muted-foreground/60" />
        <p className="text-xs font-medium">No usage trackers configured</p>
        <p className="text-[11px] text-muted-foreground">
          {hasProviders
            ? "Open any provider and configure a tracker in its Tracker tab to see usage here."
            : "Add a provider and configure a tracker to see usage here."}
        </p>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// helpers — duplicated from TrackerTab; keeping them private avoids a cross-file
// refactor just to expose them.
// ---------------------------------------------------------------------------

function computePercent(
  used: number | null,
  limit: number | null,
): number | null {
  if (used === null || limit === null || limit === 0) return null;
  return (used / limit) * 100;
}

function formatUsed(w: TrackerUsageWindow): string {
  if (w.used !== null && w.limit !== null) {
    return `${formatNumber(w.used)} / ${formatNumber(w.limit)}`;
  }
  if (w.used !== null) return formatNumber(w.used);
  if (w.used_percent !== null) return `${w.used_percent.toFixed(0)}%`;
  return "—";
}

function formatNumber(n: number): string {
  if (Number.isInteger(n)) return n.toLocaleString();
  return n.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

function formatRelative(iso: string): string {
  const ts = Date.parse(iso);
  if (Number.isNaN(ts)) return iso;
  const diffMs = Date.now() - ts;
  const abs = Math.abs(diffMs);
  const future = diffMs < 0;
  const sec = Math.round(abs / 1000);
  const min = Math.round(sec / 60);
  const hr = Math.round(min / 60);
  const day = Math.round(hr / 24);
  let value: number;
  let unit: string;
  if (sec < 60) {
    value = sec;
    unit = "s";
  } else if (min < 60) {
    value = min;
    unit = "m";
  } else if (hr < 24) {
    value = hr;
    unit = "h";
  } else {
    value = day;
    unit = "d";
  }
  return future ? `in ${value}${unit}` : `${value}${unit} ago`;
}
