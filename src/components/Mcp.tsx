"use client";

import {
  ArrowLeft,
  CircleCheck,
  CircleAlert,
  CircleHelp,
  Loader2,
  PlugZap,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { useMcpServers } from "@/hooks/useMcpServers";
import type {
  McpHealth,
  McpServerSummary,
  McpTransport,
} from "@/lib/types";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
} from "@/data/globalTabs";
import { cn } from "@/lib/utils";

// Sidebar entry — same visual shape as MarketplaceSidebarButton and
// SkillsSidebarButton (icon + label, pill highlight when active). No
// "+ Add" affordance because MCP server CRUD lives in `~/.claude.json`
// which is shared with Claude Code's runtime state — read-only for v1.
export function McpSidebarButton({
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
      <PlugZap
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">MCP</span>
    </button>
  );
}

export function McpView({ onClose }: GlobalTabProps) {
  const { servers, loading, refresh } = useMcpServers();

  // servers === null means first load still in flight.
  const initialLoad = servers === null && loading;

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <Button size="sm" variant="ghost" onClick={onClose}>
            <ArrowLeft className="size-3.5" />
          </Button>
          <PlugZap className="size-4 text-primary" />
          <div>
            <h2 className="text-sm font-semibold leading-none">MCP servers</h2>
            <p className="mt-1 text-[11px] text-muted-foreground">
              External tools and data sources Claude Code can connect to.
            </p>
          </div>
        </div>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void refresh()}
          disabled={loading}
          aria-label="Refresh MCP server list"
        >
          {loading ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <RefreshCw className="size-3.5" />
          )}
          Refresh
        </Button>
      </div>

      <McpServerList servers={servers} initialLoad={initialLoad} />
    </div>
  );
}

function McpServerList({
  servers,
  initialLoad,
}: {
  servers: McpServerSummary[] | null;
  initialLoad: boolean;
}) {
  if (initialLoad) {
    return (
      <div className="flex items-center justify-center gap-2 rounded-xl border bg-card/45 p-8 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        Loading MCP servers…
      </div>
    );
  }

  const rows = servers ?? [];
  if (rows.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed bg-card/30 p-8 text-center">
        <PlugZap className="size-5 text-muted-foreground/60" />
        <div className="space-y-1">
          <p className="text-sm font-medium">No MCP servers configured</p>
          <p className="text-[11px] text-muted-foreground">
            Add an entry to the <code className="font-mono">mcpServers</code>{" "}
            object in <code className="font-mono">~/.claude.json</code>.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col divide-y divide-border/40 overflow-hidden rounded-xl border bg-card/45">
      {rows.map((s) => (
        <McpServerRow key={s.name} server={s} />
      ))}
    </div>
  );
}

function McpServerRow({ server }: { server: McpServerSummary }) {
  return (
    <div className="flex flex-col gap-2 p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 truncate">
          <h3 className="truncate text-sm font-semibold font-mono">
            {server.name}
          </h3>
          <TransportPill transport={server.transport} />
          <HealthPill health={server.health} />
          {server.needs_auth && <NeedsAuthPill />}
        </div>
      </div>

      {server.transport === "stdio" ? (
        <StdioDetails server={server} />
      ) : (
        <HttpDetails server={server} />
      )}

      <p
        className="truncate font-mono text-[10px] text-muted-foreground/50"
        title={server.source}
      >
        {server.source}
      </p>
    </div>
  );
}

function TransportPill({ transport }: { transport: McpTransport }) {
  // Distinct background per transport so the user can scan a list of
  // mixed servers at a glance. Colors are picked for AA contrast on the
  // card background.
  const classes: Record<McpTransport, string> = {
    stdio: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    http: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
    sse: "bg-violet-500/10 text-violet-600 dark:text-violet-400",
  };
  return (
    <span
      className={cn(
        "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider",
        classes[transport],
      )}
    >
      {transport}
    </span>
  );
}

function HealthPill({ health }: { health: McpHealth | null }) {
  if (health === null) {
    return (
      <span
        title="Health not yet recorded"
        className="inline-flex items-center gap-1 shrink-0 rounded-full bg-muted/60 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground"
      >
        <CircleHelp className="size-3" />
        Not checked
      </span>
    );
  }
  if (health.status === "healthy") {
    return (
      <span
        title="Last health check succeeded"
        className="inline-flex items-center gap-1 shrink-0 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-emerald-600 dark:text-emerald-400"
      >
        <CircleCheck className="size-3" />
        Healthy
      </span>
    );
  }
  return (
    <span
      title={
        health.last_error
          ? `Last error: ${health.last_error}`
          : `Failed ${health.failure_count} time${health.failure_count === 1 ? "" : "s"}`
      }
      className="inline-flex items-center gap-1 shrink-0 rounded-full bg-red-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-red-600 dark:text-red-400"
    >
      <CircleAlert className="size-3" />
      Failing
    </span>
  );
}

function NeedsAuthPill() {
  return (
    <span
      title="This server needs re-authentication"
      className="inline-flex items-center shrink-0 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-amber-600 dark:text-amber-400"
    >
      Needs auth
    </span>
  );
}

function StdioDetails({ server }: { server: McpServerSummary }) {
  const cmdline = [server.command, ...server.args]
    .filter(Boolean)
    .join(" ");
  const envEntries = Object.entries(server.env);

  return (
    <div className="space-y-1.5">
      {cmdline ? (
        <p
          className="truncate font-mono text-xs text-foreground/80"
          title={cmdline}
        >
          {cmdline}
        </p>
      ) : (
        <p className="text-[11px] italic text-muted-foreground/60">
          No command configured.
        </p>
      )}
      {envEntries.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {envEntries.map(([k, v]) => (
            <span
              key={k}
              title={`${k}=${v}`}
              className="rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
            >
              {k}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function HttpDetails({ server }: { server: McpServerSummary }) {
  const headerEntries = Object.entries(server.headers);

  return (
    <div className="space-y-1.5">
      {server.url ? (
        <p
          className="truncate font-mono text-xs text-foreground/80"
          title={server.url}
        >
          {server.url}
        </p>
      ) : (
        <p className="text-[11px] italic text-muted-foreground/60">
          No URL configured.
        </p>
      )}
      {headerEntries.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {headerEntries.map(([k, v]) => (
            <span
              key={k}
              title={`${k}: ${v}`}
              className="rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
            >
              {k}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}