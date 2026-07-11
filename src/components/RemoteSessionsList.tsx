"use client";

import { Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { RemoteSessionSummary } from "@/lib/types";

interface Props {
  rows: RemoteSessionSummary[];
  /** Called when the user clicks the Download button. */
  onDownload: (row: RemoteSessionSummary) => void;
  /**
   * Optional: when provided, clicking the row body (NOT the Download
   * button) calls this. Use this for the Remote tab's inline preview.
   * The Download button still calls onDownload.
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
                        <Button size="sm" onClick={() => onDownload(r)}>
                          Download
                        </Button>
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
