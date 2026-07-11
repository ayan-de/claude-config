"use client";

import { useEffect, useState } from "react";
import { RefreshCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ProjectPickerModal } from "@/components/ProjectPickerModal";
import { RemoteSessionsList } from "@/components/RemoteSessionsList";
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

  const handleDownload = (row: RemoteSessionSummary) => {
    void download(row, {
      onNeedPicker: () =>
        setPicker({
          slug: row.projectSlug,
          originalPath: row.originalPath,
          pendingRow: row,
        }),
      onDone: () => onDownloaded(),
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

        <RemoteSessionsList
          rows={sessions}
          loading={loading}
          error={error}
          onDownload={handleDownload}
          emptyMessage="No remote sessions yet."
        />
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
            void download(row, {
              onNeedPicker: () =>
                setPicker({
                  slug: row.projectSlug,
                  originalPath: row.originalPath,
                  pendingRow: row,
                }),
              onDone: () => onDownloaded(),
            });
          }}
        />
      )}
    </div>
  );
}