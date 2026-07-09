"use client";

import { useEffect, useState } from "react";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  Folder,
  History,
  Loader2,
  MessageSquare,
  RefreshCw,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { useSessions } from "@/hooks/useSessions";
import { revealInFileManager } from "@/lib/api";
import type { SessionSummary } from "@/lib/types";
import type {
  GlobalTabProps,
  SidebarTabButtonProps,
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

const PAGE_SIZE = 20;

/**
 * Main-space Sessions view. Paginates the full list returned by the
 * backend (capped at 1000 rows on the Rust side). Clicking a row
 * reveals the `.jsonl` transcript in the OS file manager — the only
 * safe action without a viewer wired up yet.
 *
 * ponytail: server-side sorted + capped, client paginates. Add search
 * when the page count crosses ~20.
 */
export function SessionsView({ onClose }: GlobalTabProps) {
  const { sessions, loading, refresh } = useSessions();
  const [page, setPage] = useState(0);

  const initialLoad = sessions.length === 0 && loading;
  const totalPages = Math.max(1, Math.ceil(sessions.length / PAGE_SIZE));
  const safePage = Math.min(page, totalPages - 1);
  const pageStart = safePage * PAGE_SIZE;
  const pageRows = sessions.slice(pageStart, pageStart + PAGE_SIZE);

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
              Claude Code conversations stored on this PC. Click a row to
              reveal the transcript in your file manager.
            </p>
          </div>
        </div>
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
      </div>

      <div className="rounded-lg border bg-card/40">
        {initialLoad ? (
          <p className="px-4 py-6 text-center text-xs text-muted-foreground">
            Loading sessions…
          </p>
        ) : sessions.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-10 text-center">
            <MessageSquare className="size-5 text-muted-foreground/60" />
            <p className="text-sm font-medium">No Claude Code sessions found</p>
            <p className="max-w-sm text-[11px] text-muted-foreground">
              Run a conversation with Claude Code and it will appear here.
            </p>
          </div>
        ) : (
          <ul className="divide-y divide-border/60">
            {pageRows.map((s) => (
              <SessionRow key={s.session_id} session={s} />
            ))}
          </ul>
        )}
      </div>

      {sessions.length > PAGE_SIZE && (
        <Pagination
          page={safePage}
          totalPages={totalPages}
          onChange={setPage}
        />
      )}

      {!initialLoad && sessions.length > 0 && (
        <p className="text-center text-[10px] text-muted-foreground">
          {sessions.length} session{sessions.length === 1 ? "" : "s"} total
        </p>
      )}
    </div>
  );
}

interface RowProps {
  session: SessionSummary;
}

function SessionRow({ session }: RowProps) {
  const [revealing, setRevealing] = useState(false);

  const handleReveal = async () => {
    if (revealing) return;
    setRevealing(true);
    try {
      await revealInFileManager(session.full_path);
    } catch {
      // Silent — file manager either opens or it doesn't. Re-add a
      // toast if users report silent failures.
    } finally {
      setRevealing(false);
    }
  };

  return (
    <li>
      <button
        type="button"
        onClick={handleReveal}
        disabled={revealing}
        title={session.full_path}
        className={cn(
          "group flex w-full items-start gap-3 px-4 py-2.5 text-left transition-colors",
          "hover:bg-muted/40 cursor-pointer",
          "disabled:opacity-60 disabled:cursor-not-allowed",
        )}
      >
        <div className="mt-0.5 shrink-0">
          <MessageSquare className="size-3.5 text-muted-foreground/70 group-hover:text-foreground" />
        </div>
        <div className="min-w-0 flex-1">
          <p className="line-clamp-2 text-xs font-medium text-foreground/90 group-hover:text-foreground">
            {session.title}
          </p>
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground">
            <span className="font-mono tabular-nums">{session.session_id.slice(0, 8)}</span>
            <span className="text-muted-foreground/50">·</span>
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
    </li>
  );
}

interface PaginationProps {
  page: number;
  totalPages: number;
  onChange: (page: number) => void;
}

function Pagination({ page, totalPages, onChange }: PaginationProps) {
  return (
    <div className="flex items-center justify-center gap-2">
      <Button
        size="sm"
        variant="outline"
        disabled={page === 0}
        onClick={() => onChange(page - 1)}
        className="cursor-pointer"
      >
        <ChevronLeft className="size-3.5" />
        Prev
      </Button>
      <span className="text-[11px] text-muted-foreground tabular-nums">
        Page {page + 1} of {totalPages}
      </span>
      <Button
        size="sm"
        variant="outline"
        disabled={page >= totalPages - 1}
        onClick={() => onChange(page + 1)}
        className="cursor-pointer"
      >
        Next
        <ChevronRight className="size-3.5" />
      </Button>
    </div>
  );
}

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