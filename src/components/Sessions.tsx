"use client";

import { useEffect, useMemo, useState, Suspense, lazy } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  ChevronDown,
  ChevronUp,
  Folder,
  History,
  Loader2,
  MessageSquare,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { GithubIcon } from "@/components/GitHubSync";
import { SessionDeleteDialog } from "@/components/SessionDeleteDialog";
import { MessageView } from "@/components/SessionMessageView";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { useSessionTranscript, useSessions } from "@/hooks/useSessions";
import { useSessionUpload } from "@/hooks/useSessionUpload";
import {
  SessionUploadProvider,
  useSessionUploadContext,
} from "@/hooks/SessionUploadContext";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import { deleteSession } from "@/lib/api";
import { SessionsTabs } from "@/components/SessionsTabs";

const RemoteSessionsTab = lazy(() =>
  import("@/components/RemoteSessionsTab").then((m) => ({
    default: m.RemoteSessionsTab,
  }))
);

function RemoteSessionsLoadingFallback() {
  return (
    <div className="flex flex-col items-center gap-2 rounded-lg border bg-card/40 px-4 py-10">
      <Loader2 className="size-4 animate-spin text-muted-foreground" />
      <p className="text-[11px] text-muted-foreground">
        Loading remote sessions…
      </p>
    </div>
  );
}

import type { SessionSummary, SyncState } from "@/lib/types";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
  GlobalTabId,
} from "@/data/globalTabs";
import { cn } from "@/lib/utils";

/**
 * Sidebar entry — same visual shape as SkillsSidebarButton (icon +
 * label, pill highlight when active). No "+ Add" affordance because
 * sessions are an output of Claude Code, not a thing the user authors
 * from this app.
 */
export function SessionsSidebarButton({
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
      <History
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">Sessions</span>
    </button>
  );
}

/**
 * Group key for sessions that didn't resolve to a project (orphan
 * jsonl files outside any Claude project dir). Falls out as a single
 * "(unindexed)" bucket at the bottom of the list.
 */
const UNGROUPED_KEY = "__ungrouped__";

/**
 * Main-space Sessions view. Groups sessions by project path into
 * collapsible accordions, so one project with many conversations
 * doesn't drown the rest. Server still caps at 1000 rows on the
 * Rust side.
 *
 * ponytail: server-side sorted + capped, grouping handles what
 * pagination handled before. Per-group pagination when a single
 * project has hundreds of sessions — re-add if it shows up.
 */
export function SessionsView({ onClose, onNavigate }: GlobalTabProps) {
  const { sessions, loading, refresh } = useSessions();
  const { config } = useGitHubSyncContext();
  const { stateById, uploadingIds, upload, seed } = useSessionUpload(sessions);
  const [selected, setSelected] = useState<SessionSummary | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<SessionSummary | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [activeTab, setActiveTab] = useState<"local" | "remote">("local");

  const onConfirmDelete = async () => {
    const target = deleteTarget;
    if (!target) return;
    setDeleting(true);
    try {
      await deleteSession(target.full_path);
      const refreshed = await refresh();
      // Re-seed the upload state map so the deleted session drops out of
      // stateById. useSessionUpload's built-in re-seed effect is gated on
      // isWebEnv() and never fires in the Tauri desktop app, so this
      // explicit call is required.
      await seed(refreshed);
      if (selected?.session_id === target.session_id) setSelected(null);
      toast.success("Session deleted");
      setDeleteTarget(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.error(`Delete failed: ${msg}`);
    } finally {
      setDeleting(false);
    }
  };

  const initialLoad = sessions.length === 0 && loading;

  // Group by project_path; sort groups by most-recent activity so the
  // "where am I working today" project lands at the top.
  const groups = useMemo(
    () => groupByProject(sessions),
    [sessions],
  );

  if (selected) {
    return (
      <SessionDetail
        session={selected}
        onBack={() => setSelected(null)}
        onClose={onClose}
      />
    );
  }

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <Button size="sm" variant="ghost" onClick={onClose}>
            <ArrowLeft className="size-3.5" />
          </Button>
          <History className="size-4 text-primary" />
          <div>
            <h2 className="text-sm font-semibold leading-none">Sessions</h2>
            <p className="mt-1 text-[11px] text-muted-foreground">
              {activeTab === "local"
                ? "Claude Code conversations stored on this PC. Click a row to read the transcript."
                : "Sessions synced to your private GitHub repo. Click a row to preview."}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <SessionsTabs active={activeTab} onChange={setActiveTab} />
          {activeTab === "local" && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => void refresh()}
              disabled={loading}
              aria-label="Refresh sessions list"
              className="cursor-pointer"
            >
              {loading ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <RefreshCw className="size-3.5" />
              )}
              Refresh
            </Button>
          )}
        </div>
      </div>

      <div className={activeTab === "local" ? "" : "hidden"}>
        {initialLoad ? (
        <div className="rounded-lg border bg-card/40 px-4 py-6 text-center text-xs text-muted-foreground">
          Loading sessions…
        </div>
      ) : sessions.length === 0 ? (
        <div className="flex flex-col items-center gap-2 rounded-lg border bg-card/40 px-4 py-10 text-center">
          <MessageSquare className="size-5 text-muted-foreground/60" />
          <p className="text-sm font-medium">No Claude Code sessions found</p>
          <p className="max-w-sm text-[11px] text-muted-foreground">
            Run a conversation with Claude Code and it will appear here.
          </p>
        </div>
      ) : (
        <>
          {config.isConnected && (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-md border bg-card/30 px-3 py-1.5 text-[11px] text-muted-foreground">
              <span className="font-medium text-foreground/80">GitHub sync:</span>
              <span className="inline-flex items-center gap-1.5">
                <GithubIcon className="size-3 text-muted-foreground/50" />
                Not yet uploaded
              </span>
              <span className="inline-flex items-center gap-1.5">
                <GithubIcon className="size-3 text-amber-600 dark:text-amber-400" />
                Local changes since upload
              </span>
              <span className="inline-flex items-center gap-1.5">
                <GithubIcon className="size-3 text-primary" />
                Uploaded to GitHub
              </span>
            </div>
          )}
          <SessionUploadProvider
            value={{
              stateById,
              uploadingIds,
              upload,
              connected: config.isConnected,
            }}
          >
            <ProjectAccordion
              groups={groups}
              onSelect={setSelected}
              onRequestDelete={setDeleteTarget}
            />
          </SessionUploadProvider>
        </>
      )}

      {!initialLoad && sessions.length > 0 && (
        <p className="text-center text-[10px] text-muted-foreground">
          {sessions.length} session{sessions.length === 1 ? "" : "s"} across{" "}
          {groups.length} project{groups.length === 1 ? "" : "s"}
        </p>
      )}
      </div>

      {activeTab === "remote" && (
        <ErrorBoundary
          fallback={(err, reset) => {
            const cta = (err as { cta?: { label: string; navigateTo: GlobalTabId } }).cta;
            return (
              <div className="rounded-lg border bg-card/40 px-4 py-8 text-center">
                <AlertTriangle className="mx-auto size-5 text-destructive" />
                <p className="mt-2 text-sm font-medium text-destructive">Failed to load remote sessions</p>
                <p className="mx-auto mt-1 max-w-sm text-[11px] text-muted-foreground">
                  {err.message}
                </p>
                <div className="mt-4 flex justify-center gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={reset}
                    className="mt-1 cursor-pointer"
                  >
                    <RefreshCw className="size-3.5 mr-1" />
                    Retry
                  </Button>
                  {cta && (
                    <Button
                      size="sm"
                      variant="default"
                      onClick={() => onNavigate?.(cta.navigateTo)}
                      className="mt-1 cursor-pointer"
                    >
                      {cta.label}
                    </Button>
                  )}
                </div>
              </div>
            );
          }}
        >
          <Suspense fallback={<RemoteSessionsLoadingFallback />}>
            <RemoteSessionsTab
              onDownloaded={async () => {
                await refresh();
              }}
              onNavigate={onNavigate}
            />
          </Suspense>
        </ErrorBoundary>
      )}

      <SessionDeleteDialog
        open={!!deleteTarget}
        sessionTitle={deleteTarget?.title ?? ""}
        projectName={deleteTarget?.project_name ?? null}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
        onConfirm={onConfirmDelete}
        isDeleting={deleting}
      />
    </div>
  );
}

/**
 * One accordion slice — a project header with `FolderOpen`/`FolderClosed`
 * affordance and a per-row transcript below it. Tracks its own
 * expanded state; the top-level group expands by default so the
 * first project lands visible.
 */
function ProjectAccordion({
  groups,
  onSelect,
  onRequestDelete,
}: {
  groups: ProjectGroup[];
  onSelect: (s: SessionSummary) => void;
  onRequestDelete: (s: SessionSummary) => void;
}) {
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set(groups[0] ? [groups[0].key] : []),
  );

  // Keep stale keys from accumulating when a refresh removes a project.
  // Bounded to one follow-up render after `groups` change; the
  // identity-return short-circuit keeps it a no-op on unrelated rerenders.
  const validKeys = useMemo(() => new Set(groups.map((g) => g.key)), [groups]);
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setExpanded((prev) => {
      let hasStale = false;
      for (const k of prev) {
        if (!validKeys.has(k)) {
          hasStale = true;
          break;
        }
      }
      if (!hasStale) return prev;
      const next = new Set<string>();
      for (const k of prev) if (validKeys.has(k)) next.add(k);
      return next;
    });
  }, [validKeys]);

  const toggle = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  return (
    <div className="flex flex-col gap-2">
      {groups.map((group) => (
        <SessionGroup
          key={group.key}
          group={group}
          open={expanded.has(group.key)}
          onToggle={() => toggle(group.key)}
          onSelect={onSelect}
          onRequestDelete={onRequestDelete}
        />
      ))}
    </div>
  );
}

interface ProjectGroup {
  key: string;
  /** Decoded path like "/home/ayande/Project/claude-config" or the
   * ungrouped label when no project resolved. */
  label: string;
  /** Sort key; ISO timestamp or "" for never-modified. */
  latestModified: string;
  sessions: SessionSummary[];
}

/**
 * Bucket sessions by their full project_path. Sessions whose
 * `project_path` is null (orphan jsonls) collapse into one synthetic
 * "Unindexed" group, sorted last.
 */
function groupByProject(sessions: SessionSummary[]): ProjectGroup[] {
  const buckets = new Map<string, SessionSummary[]>();
  for (const s of sessions) {
    const key = s.project_path ?? UNGROUPED_KEY;
    const list = buckets.get(key);
    if (list) list.push(s);
    else buckets.set(key, [s]);
  }

  const groups: ProjectGroup[] = [];
  for (const [key, list] of buckets) {
    groups.push({
      key,
      label: key === UNGROUPED_KEY ? "(unindexed)" : key,
      latestModified: list.reduce<string>(
        (acc, s) => (s.modified && s.modified > acc ? s.modified : acc),
        "",
      ),
      sessions: list,
    });
  }

  // Project groups: most recently active first. Unindexed always
  // dead-last so it doesn't compete for attention.
  groups.sort((a, b) => {
    if (a.key === UNGROUPED_KEY) return 1;
    if (b.key === UNGROUPED_KEY) return -1;
    return b.latestModified.localeCompare(a.latestModified);
  });

  return groups;
}

function SessionGroup({
  group,
  open,
  onToggle,
  onSelect,
  onRequestDelete,
}: {
  group: ProjectGroup;
  open: boolean;
  onToggle: () => void;
  onSelect: (s: SessionSummary) => void;
  onRequestDelete: (s: SessionSummary) => void;
}) {
  const segments = useMemo(
    () => splitBreadcrumbs(group.label),
    [group.label],
  );
  const uploadCtx = useSessionUploadContext();
  const syncCounts = useMemo(() => {
    if (!uploadCtx) return null;
    let never = 0,
      dirty = 0,
      synced = 0;
    for (const s of group.sessions) {
      const st = uploadCtx.stateById.get(s.session_id) ?? "never_uploaded";
      if (st === "synced") synced++;
      else if (st === "out_of_sync") dirty++;
      else never++;
    }
    return { never, dirty, synced };
  }, [uploadCtx, group.sessions]);
  const isUngrouped = group.label === "(unindexed)";

  return (
    <section className="rounded-lg border bg-card/40 overflow-hidden">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className={cn(
          "flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors cursor-pointer",
          "hover:bg-muted/40",
        )}
      >
        {open ? (
          <ChevronUp className="size-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
        )}
        <Folder
          className={cn(
            "size-3.5 shrink-0",
            open ? "text-primary" : "text-muted-foreground/70",
          )}
        />
        <div className="min-w-0 flex-1 truncate text-xs">
          {isUngrouped ? (
            <span className="font-medium text-muted-foreground">
              {group.label}
            </span>
          ) : (
            <BreadcrumbPath segments={segments} />
          )}
        </div>
        {syncCounts && (
          <span
            className="hidden items-center gap-2 sm:inline-flex text-[10px] tabular-nums"
            aria-label="GitHub sync breakdown"
          >
            <span className="inline-flex items-center gap-0.5 text-muted-foreground/70">
              <GithubIcon className="size-2.5 text-muted-foreground/50" />
              {syncCounts.never}
            </span>
            <span className="inline-flex items-center gap-0.5 text-amber-600 dark:text-amber-400">
              <GithubIcon className="size-2.5" />
              {syncCounts.dirty}
            </span>
            <span className="inline-flex items-center gap-0.5 text-primary">
              <GithubIcon className="size-2.5" />
              {syncCounts.synced}
            </span>
          </span>
        )}
      </button>
      {open && (
        <ul className="divide-y divide-border/60 border-t border-border/40">
          {group.sessions.map((s) => (
            <SessionRow
              key={s.session_id}
              session={s}
              onSelect={onSelect}
              onRequestDelete={onRequestDelete}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

/** Split "/home/ayande/Project/claude-config" into its path segments,
 * dropping the leading empty piece from the leading slash. */
function splitBreadcrumbs(path: string): string[] {
  return path.split("/").filter(Boolean);
}

function BreadcrumbPath({ segments }: { segments: string[] }) {
  return (
    <span className="font-mono text-[11px]">
      {segments.map((seg, i) => (
        <span key={`${i}-${seg}`}>
          {i > 0 && (
            <span className="text-muted-foreground/50">/</span>
          )}
          <span
            className={cn(
              i === segments.length - 1
                ? "font-medium text-foreground"
                : "text-muted-foreground/80",
            )}
          >
            {seg}
          </span>
        </span>
      ))}
    </span>
  );
}

interface RowProps {
  session: SessionSummary;
  onSelect: (s: SessionSummary) => void;
  onRequestDelete: (s: SessionSummary) => void;
}

function SessionRow({ session, onSelect, onRequestDelete }: RowProps) {
  const uploadCtx = useSessionUploadContext();
  return (
    <li
      className={cn(
        "group flex items-start gap-2 pr-3 transition-colors",
        "hover:bg-muted/40",
      )}
    >
      <button
        type="button"
        onClick={() => onSelect(session)}
        title={session.full_path}
        className="flex min-w-0 flex-1 items-start gap-3 px-4 py-2.5 text-left cursor-pointer"
      >
        <div className="mt-0.5 flex shrink-0 items-center gap-1.5">
          <MessageSquare className="size-3.5 text-muted-foreground/70 group-hover:text-foreground" />
        </div>
        <div className="min-w-0 flex-1">
          <p className="line-clamp-2 text-xs font-medium text-foreground/90 group-hover:text-foreground">
            {session.title}
          </p>
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground">
            <span>
              {session.message_count > 0
                ? `${session.message_count} msg${session.message_count === 1 ? "" : "s"}`
                : "unindexed"}
            </span>
            {session.modified && (
              <>
                <span className="text-muted-foreground/50">·</span>
                <span className="tabular-nums">
                  <TimeAgo iso={session.modified} />
                </span>
              </>
            )}
            {session.project_name && (
              <>
                <span className="text-muted-foreground/50">·</span>
                <span className="inline-flex items-center gap-0.5">
                  <Folder className="size-2.5" />
                  {session.project_name}
                </span>
              </>
            )}
          </div>
        </div>
      </button>
      <SessionSyncButton session={session} ctx={uploadCtx} />
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onRequestDelete(session);
        }}
        aria-label="Delete session"
        title="Delete session"
        className="mt-2 shrink-0 rounded p-1 text-muted-foreground/50 opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
      >
        <Trash2 className="size-3.5" />
      </button>
    </li>
  );
}

/**
 * Per-row GitHub upload affordance. Color encodes sync state:
 * gray = never uploaded, green = synced, amber = local changes since
 * upload, spinner = upload in flight. Renders a static muted mark when
 * GitHub isn't connected (no provider or `connected` false).
 */
function SessionSyncButton({
  session,
  ctx,
}: {
  session: SessionSummary;
  ctx: ReturnType<typeof useSessionUploadContext>;
}) {
  if (!ctx || !ctx.connected) {
    return (
      <GithubIcon className="mt-2.5 shrink-0 size-3.5 text-muted-foreground/30" />
    );
  }

  const uploading = ctx.uploadingIds.has(session.session_id);
  const state: SyncState =
    ctx.stateById.get(session.session_id) ?? "never_uploaded";

  const { color, tooltip } = syncIconMeta(state);

  return (
    <Tooltip>
      <TooltipTrigger
        disabled={uploading}
        aria-label={tooltip}
        onClick={(e) => {
          e.stopPropagation();
          ctx.upload(session);
        }}
        className={cn(
          "mt-1.5 shrink-0 rounded-md p-1 transition-colors",
          uploading ? "cursor-default" : "cursor-pointer hover:bg-muted",
        )}
      >
        {uploading ? (
          <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
        ) : (
          <GithubIcon className={cn("size-3.5", color)} />
        )}
      </TooltipTrigger>
      <TooltipContent side="left">{tooltip}</TooltipContent>
    </Tooltip>
  );
}

function syncIconMeta(state: SyncState): { color: string; tooltip: string } {
  switch (state) {
    case "synced":
      return {
        color: "text-primary",
        tooltip: "Uploaded to GitHub — click to update",
      };
    case "out_of_sync":
      return {
        color: "text-amber-600 dark:text-amber-400",
        tooltip: "Local changes since upload — click to update",
      };
    case "never_uploaded":
    default:
      return {
        color: "text-muted-foreground/50",
        tooltip: "Upload to GitHub",
      };
  }
}

// ---------------------------------------------------------------------------
// Session detail view
// ---------------------------------------------------------------------------

interface DetailProps {
  session: SessionSummary;
  onBack: () => void;
  onClose: () => void;
}

function SessionDetail({ session, onBack, onClose }: DetailProps) {
  const { messages, loading, error } = useSessionTranscript(session.full_path);

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <Button size="sm" variant="ghost" onClick={onBack} title="Back to sessions">
            <ArrowLeft className="size-3.5" />
          </Button>
          <History className="size-4 text-primary shrink-0" />
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold leading-none">
              {session.title}
            </h2>
            <p className="mt-1 truncate text-[11px] text-muted-foreground font-mono">
              {session.session_id}
              {session.project_name ? ` · ${session.project_name}` : ""}
              {session.modified ? ` · ${formatAbsolute(session.modified)}` : ""}
            </p>
          </div>
        </div>
        <Button
          size="sm"
          variant="ghost"
          onClick={onClose}
          aria-label="Close sessions tab"
          className="cursor-pointer"
        >
          Close
        </Button>
      </div>

      <div className="rounded-lg border bg-card/40">
        {loading ? (
          <p className="flex items-center justify-center gap-2 px-4 py-10 text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            Loading transcript…
          </p>
        ) : error ? (
          <div className="flex flex-col items-center gap-1 px-4 py-10 text-center">
            <p className="text-sm font-medium text-destructive">
              Failed to read transcript
            </p>
            <p className="max-w-md text-[11px] text-muted-foreground">{error}</p>
          </div>
        ) : !messages || messages.length === 0 ? (
          <p className="px-4 py-10 text-center text-xs text-muted-foreground">
            (empty session)
          </p>
        ) : (
          <ol className="divide-y divide-border/40">
            {messages.map((m, i) => (
              <li key={i}>
                <MessageView message={m} />
              </li>
            ))}
          </ol>
        )}
      </div>

      {!loading && !error && messages && messages.length > 0 && (
        <p className="text-center text-[10px] text-muted-foreground">
          {messages.length} message{messages.length === 1 ? "" : "s"}
          {session.message_count > 0 &&
          messages.length < session.message_count
            ? ` (showing first ${messages.length} of ${session.message_count})`
            : ""}
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// List-view helpers
// ---------------------------------------------------------------------------

/**
 * Compact relative-time label ("just now", "5m ago", "3d ago"). Avoids
 * pulling a date library — Date.parse + a small ladder is enough for
 * this view. Falls back to the raw ISO string on parse failure.
 */
function TimeAgo({ iso }: { iso: string }) {
  const now = useNowMinute();
  return <>{formatRelative(iso, now)}</>;
}

/** Re-renders once a minute so "5m ago" → "6m ago" updates on its own. */
function useNowMinute(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(id);
  }, []);
  return now;
}

function formatRelative(iso: string, now: number): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  const diffSec = Math.max(0, Math.floor((now - then) / 1000));
  if (diffSec < 45) return "just now";
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86_400) return `${Math.floor(diffSec / 3600)}h ago`;
  if (diffSec < 86_400 * 30) return `${Math.floor(diffSec / 86_400)}d ago`;
  return new Date(then).toLocaleDateString();
}

/** Absolute timestamp for the detail-view header. */
function formatAbsolute(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  return new Date(then).toLocaleString();
}