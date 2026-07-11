"use client";

import { useEffect, useState } from "react";
import { Loader2, RefreshCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ProjectPickerModal } from "@/components/ProjectPickerModal";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import { useRemoteSessions } from "@/hooks/useRemoteSessions";
import type { RemoteSessionSummary } from "@/lib/types";

interface Props {
  open: boolean;
  onClose: () => void;
  onDownloaded: () => void;
}

interface PickerState {
  slug: string;
  originalPath: string;
  pendingRow: RemoteSessionSummary;
}

export function RemoteSessionsModal({ open, onClose, onDownloaded }: Props) {
  const { config } = useGitHubSyncContext();
  const { sessions, loading, error, refresh, download } = useRemoteSessions();
  const [picker, setPicker] = useState<PickerState | null>(null);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  if (!open) return null;

  const groups = new Map<string, RemoteSessionSummary[]>();
  for (const s of sessions) {
    const arr = groups.get(s.projectSlug) ?? [];
    arr.push(s);
    groups.set(s.projectSlug, arr);
  }

  const handleDownload = (row: RemoteSessionSummary) => {
    void download(row, {
      onNeedPicker: () =>
        setPicker({
          slug: row.projectSlug,
          originalPath: row.originalPath,
          pendingRow: row,
        }),
      onDone: () => {
        onDownloaded();
      },
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="flex max-h-[80vh] w-full max-w-3xl flex-col rounded-md bg-background shadow-lg">
        <header className="flex items-center justify-between border-b px-4 py-2">
          <h2 className="text-sm font-semibold">Remote sessions</h2>
          <div className="flex gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void refresh()}
              disabled={loading}
            >
              <RefreshCw
                className={loading ? "mr-1 size-3.5 animate-spin" : "mr-1 size-3.5"}
              />
              Refresh
            </Button>
            <Button variant="ghost" size="icon" onClick={onClose}>
              <X className="size-3.5" />
            </Button>
          </div>
        </header>

        {!config.isConnected && (
          <p className="p-4 text-xs text-muted-foreground">
            Connect GitHub in Settings first.
          </p>
        )}

        {config.isConnected && sessions.length === 0 && !loading && !error && (
          <p className="p-4 text-xs text-muted-foreground">
            No remote sessions yet.
          </p>
        )}

        {loading && (
          <div className="flex justify-center p-4">
            <Loader2 className="size-4 animate-spin" />
          </div>
        )}

        {error && <p className="p-4 text-xs text-destructive">{error}</p>}

        <div className="overflow-auto px-4 py-2">
          {[...groups.entries()].map(([slug, rows]) => (
            <section key={slug} className="mb-4">
              <h3 className="text-[11px] font-medium text-muted-foreground">
                {slug}
              </h3>
              <ul className="divide-y">
                {rows.map((r) => (
                  <li
                    key={r.sessionId}
                    className="flex items-center justify-between py-2"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-xs">
                        {r.title ?? r.sessionId.slice(0, 8)}
                      </div>
                      <div className="text-[10px] text-muted-foreground">
                        {r.modified ?? "—"} · {r.messageCount} msgs
                      </div>
                    </div>
                    <Button size="sm" onClick={() => handleDownload(r)}>
                      Download
                    </Button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>

      {picker && (
        <ProjectPickerModal
          open
          onClose={() => setPicker(null)}
          remoteOriginalPath={picker.originalPath}
          remoteSlug={picker.slug}
          onPicked={() => {
            const row = picker.pendingRow;
            setPicker(null);
            // Retry the download after the mapping is persisted.
            void download(row, {
              onNeedPicker: () => {
                // Should never fire on the retry since we just set the
                // mapping, but if it does the modal reopens gracefully.
                setPicker({
                  slug: row.projectSlug,
                  originalPath: row.originalPath,
                  pendingRow: row,
                });
              },
              onDone: () => onDownloaded(),
            });
          }}
        />
      )}
    </div>
  );
}
