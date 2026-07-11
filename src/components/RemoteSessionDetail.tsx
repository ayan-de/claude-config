"use client";

import { Download, Loader2, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { MessageView } from "@/components/SessionMessageView";
import type { RemoteSessionSummary, SessionMessage } from "@/lib/types";

interface Props {
  row: RemoteSessionSummary;
  messages: SessionMessage[] | null;
  loading: boolean;
  error: string | null;
  /** Closes the preview (parent hides this component). */
  onClose: () => void;
  /** Triggers the existing download flow. */
  onDownload: (row: RemoteSessionSummary) => void;
}

export function RemoteSessionDetail({
  row,
  messages,
  loading,
  error,
  onClose,
  onDownload,
}: Props) {
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-semibold leading-none">
            {row.title ?? row.sessionId.slice(0, 8)}
          </h2>
          <p className="mt-1 truncate text-[11px] font-mono text-muted-foreground">
            {row.sessionId}
            {row.modified ? ` · ${row.modified}` : ""}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => onDownload(row)}
            aria-label="Download session"
          >
            <Download className="size-3.5" />
            Download
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={onClose}
            aria-label="Close preview"
          >
            <X className="size-3.5" />
            Close
          </Button>
        </div>
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
            <p className="max-w-md text-[11px] text-muted-foreground">
              {error}
            </p>
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
          {row.messageCount > 0 && messages.length < row.messageCount
            ? ` (showing first ${messages.length} of ${row.messageCount})`
            : ""}
        </p>
      )}
    </div>
  );
}
