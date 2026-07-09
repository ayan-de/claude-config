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
  Terminal,
  Wrench,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";

import { Button } from "@/components/ui/button";
import { useSessionTranscript, useSessions } from "@/hooks/useSessions";
import type { SessionMessage, SessionSummary } from "@/lib/types";
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
 * swaps the list for a read-only transcript view (`SessionDetail`);
 * the back button in the detail view returns here.
 *
 * ponytail: server-side sorted + capped, client paginates. Add search
 * when the page count crosses ~20.
 */
export function SessionsView({ onClose }: GlobalTabProps) {
  const { sessions, loading, refresh } = useSessions();
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<SessionSummary | null>(null);

  // Reset to page 1 if a refresh shrinks the total below the current page.
  const initialLoad = sessions.length === 0 && loading;
  const totalPages = Math.max(1, Math.ceil(sessions.length / PAGE_SIZE));
  const safePage = Math.min(page, totalPages - 1);
  const pageStart = safePage * PAGE_SIZE;
  const pageRows = sessions.slice(pageStart, pageStart + PAGE_SIZE);

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
              Claude Code conversations stored on this PC. Click a row to
              read the transcript.
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
              <SessionRow
                key={s.session_id}
                session={s}
                onSelect={setSelected}
              />
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
  onSelect: (s: SessionSummary) => void;
}

function SessionRow({ session, onSelect }: RowProps) {
  return (
    <li>
      <button
        type="button"
        onClick={() => onSelect(session)}
        title={session.full_path}
        className={cn(
          "group flex w-full items-start gap-3 px-4 py-2.5 text-left transition-colors",
          "hover:bg-muted/40 cursor-pointer",
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

function MessageView({ message }: { message: SessionMessage }) {
  if (message.is_tool_result) {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2">
          <Terminal className="mt-0.5 size-3 shrink-0 text-muted-foreground/60" />
          <pre className="min-w-0 flex-1 whitespace-pre-wrap break-words rounded-md border border-border/40 bg-muted/20 px-3 py-2 font-mono text-[11px] text-muted-foreground">
            {message.content}
          </pre>
        </div>
      </div>
    );
  }

  if (message.role === "tool") {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2">
          <Wrench className="mt-0.5 size-3 shrink-0 text-amber-600 dark:text-amber-400" />
          <div className="min-w-0 flex-1">
            <p className="text-[10px] font-mono font-medium uppercase tracking-wider text-amber-600 dark:text-amber-400">
              {message.tool_name ?? "tool"}
            </p>
            {message.content && (
              <pre className="mt-1 whitespace-pre-wrap break-words rounded-md border border-border/40 bg-muted/20 px-3 py-2 font-mono text-[11px] text-muted-foreground">
                {message.content}
              </pre>
            )}
          </div>
        </div>
      </div>
    );
  }

  if (message.role === "user") {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2.5">
          <span
            aria-hidden
            className="mt-0.5 select-none font-mono text-sm font-semibold leading-snug text-blue-600 dark:text-blue-400"
          >
            ›
          </span>
          <p className="min-w-0 flex-1 whitespace-pre-wrap break-words rounded-md bg-blue-500/10 px-3 py-2 text-xs text-foreground/90">
            {message.content}
          </p>
        </div>
      </div>
    );
  }

  if (message.role === "assistant") {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2.5">
          <span
            aria-hidden
            className="mt-0.5 select-none font-mono text-sm font-semibold leading-snug text-primary"
          >
            ✦
          </span>
          <div className="min-w-0 flex-1 rounded-md bg-primary/5 px-3 py-2 text-xs text-foreground/90 [&_p]:my-1.5 [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 [&_h1]:mt-3 [&_h1]:mb-1 [&_h1]:text-sm [&_h1]:font-semibold [&_h2]:mt-2.5 [&_h2]:mb-1 [&_h2]:text-xs [&_h2]:font-semibold [&_h3]:mt-2 [&_h3]:mb-1 [&_h3]:text-xs [&_h3]:font-semibold [&_ul]:my-1.5 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-1.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-0.5 [&_a]:text-primary [&_a]:underline [&_strong]:font-semibold [&_em]:italic [&_blockquote]:border-l-2 [&_blockquote]:border-border/60 [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_code]:rounded [&_code]:bg-muted/60 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.9em] [&_pre]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border/40 [&_pre]:bg-muted/30 [&_pre]:p-3 [&_pre]:font-mono [&_pre]:text-[11px] [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_table]:my-2 [&_table]:w-full [&_table]:border-collapse [&_th]:border [&_th]:border-border/40 [&_th]:bg-muted/20 [&_th]:px-2 [&_th]:py-1 [&_th]:text-left [&_th]:font-semibold [&_td]:border [&_td]:border-border/40 [&_td]:px-2 [&_td]:py-1 [&_hr]:my-3 [&_hr]:border-border/40">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeSanitize]}
            >
              {message.content}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    );
  }

  // thinking marker the parser prefixes onto thinking-block content
  if (message.content.startsWith("[thinking]")) {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2.5">
          <span
            aria-hidden
            className="mt-0.5 select-none font-mono text-sm font-semibold leading-snug text-muted-foreground/60"
          >
            ·
          </span>
          <p className="min-w-0 flex-1 whitespace-pre-wrap break-words rounded-md border border-dashed border-border/40 px-3 py-2 text-[11px] italic text-muted-foreground/80">
            {message.content.slice("[thinking]".length).trim()}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="px-4 py-2.5">
      <p className="whitespace-pre-wrap break-words text-xs text-foreground/90">
        {message.content}
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Existing list-view helpers
// ---------------------------------------------------------------------------

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

/** Absolute timestamp for the detail-view header. */
function formatAbsolute(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  return new Date(then).toLocaleString();
}