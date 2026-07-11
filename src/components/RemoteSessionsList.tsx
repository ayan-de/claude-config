"use client";

import { CheckCircle2, Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { RemoteSessionSummary, SyncAction } from "@/lib/types";

interface Props {
  rows: RemoteSessionSummary[];
  /** Called when the user clicks the per-row Download/Update button. */
  onDownload: (row: RemoteSessionSummary) => void;
  /**
   * Optional: when provided, clicking the row body (NOT the action
   * button) calls this. Use this for the Remote tab's inline preview.
   * The action button still calls onDownload.
   * `expandedRowId` highlights which row is currently expanded.
   */
  onPreview?: (row: RemoteSessionSummary) => void;
  expandedRowId?: string | null;
  /** Optional loading state shown above the list. */
  loading?: boolean;
  /** Optional error message shown above the list. */
  error?: string | null;
  /** Optional empty-state copy when `rows` is empty. */
  emptyMessage?: string;
}

export function RemoteSessionsList({
  rows,
  onDownload,
  onPreview,
  expandedRowId,
  loading,
  error,
  emptyMessage = "No remote sessions yet.",
}: Props) {
  const groups = new Map<string, RemoteSessionSummary[]>();
  for (const s of rows) {
    const arr = groups.get(s.projectSlug) ?? [];
    arr.push(s);
    groups.set(s.projectSlug, arr);
  }

  const sortedSlugs = [...groups.keys()].sort();

  return (
    <div>
      {loading && (
        <div className="flex justify-center p-4">
          <Loader2 className="size-4 animate-spin" />
        </div>
      )}

      {error && <p className="p-4 text-xs text-destructive">{error}</p>}

      {rows.length === 0 && !loading ? (
        <p className="p-4 text-xs text-muted-foreground">{emptyMessage}</p>
      ) : (
        <div className="px-4 py-2">
          {sortedSlugs.map((slug) => {
            const groupRows = groups.get(slug)!;
            return (
              <section key={slug} className="mb-4">
                <h3 className="text-[11px] font-medium text-muted-foreground">
                  {slug}
                </h3>
                <ul className="divide-y">
                  {groupRows.map((r) => {
                    const isExpanded = expandedRowId === r.sessionId;
                    const bodyClass = `min-w-0 ${isExpanded ? "bg-muted/40" : ""}`;
                    const bodyProps = {
                      className: `${bodyClass} ${onPreview ? "flex-1 text-left" : ""}`.trim(),
                      onClick: onPreview ? () => onPreview(r) : undefined,
                      ...(onPreview ? { type: "button" as const } : {}),
                    };

                    const Body = onPreview ? "button" : "div";

                    return (
                      <li
                        key={r.sessionId}
                        className={`flex items-center justify-between py-2 ${isExpanded ? "bg-muted/40" : ""}`}
                      >
                        <Body {...bodyProps}>
                          <div className="truncate text-xs">
                            {r.title ?? r.sessionId.slice(0, 8)}
                          </div>
                          <div className="text-[10px] text-muted-foreground">
                            {r.modified ?? "—"} · {r.messageCount} msgs
                          </div>
                        </Body>
                        <SyncActionButton
                          action={r.syncAction}
                          onClick={() => onDownload(r)}
                        />
                      </li>
                    );
                  })}
                </ul>
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

/**
 * Renders the four states of `row.syncAction` as a single button. The
 * `Conflict` variant keeps the same verb ("Update") as `Update` because
 * the visual affordance is the amber border, not a different label —
 * clicking routes through the existing `SessionDownloadConflict` confirm
 * dialog via `useRemoteSessions.download()`.
 */
function SyncActionButton({
  action,
  onClick,
}: {
  action: SyncAction;
  onClick: () => void;
}) {
  switch (action) {
    case "download":
      return (
        <Button size="sm" onClick={onClick}>
          Download
        </Button>
      );
    case "update":
      return (
        <Button size="sm" onClick={onClick}>
          Update
        </Button>
      );
    case "conflict":
      return (
        <Button
          size="sm"
          onClick={onClick}
          title="This session has local changes since last upload — click to review"
          className={cn(
            "border-amber-500/60 text-amber-700 hover:bg-amber-500/10",
            "dark:text-amber-400 dark:hover:bg-amber-500/10",
          )}
          variant="default"
        >
          Update
        </Button>
      );
    case "in_sync":
      return (
        <Button size="sm" variant="ghost" disabled aria-label="Already synced">
          <CheckCircle2 className="size-3 text-emerald-500" />
          Synced
        </Button>
      );
  }
}
