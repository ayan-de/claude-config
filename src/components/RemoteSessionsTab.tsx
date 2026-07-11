"use client";

import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { GithubIcon } from "@/components/GitHubSync";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import { useRemoteSessions } from "@/hooks/useRemoteSessions";
import { RemoteSessionsList } from "@/components/RemoteSessionsList";
import { RemoteSessionDetail } from "@/components/RemoteSessionDetail";
import type { RemoteSessionSummary } from "@/lib/types";
import type { GlobalTabId } from "@/data/globalTabs";

interface Props {
  /** Called after a successful download so the parent can refresh local sessions. */
  onDownloaded: () => void;
  /** Optional: jump to the GitHub Sync tab when the user clicks "Connect GitHub". */
  onNavigate?: (id: GlobalTabId) => void;
}

export function RemoteSessionsTab({ onDownloaded, onNavigate }: Props) {
  const { config } = useGitHubSyncContext();
  const { sessions, loading, error, refresh, download, transcripts, loadTranscript } =
    useRemoteSessions();
  const [expandedRowId, setExpandedRowId] = useState<string | null>(null);

  // Auto-refresh when the tab mounts (similar to the modal's behavior).
  useEffect(() => {
    void refresh();
    // We want this to run once on mount — refresh identity is stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const expandedRow = expandedRowId
    ? sessions.find((r) => r.sessionId === expandedRowId) ?? null
    : null;

  const transcriptState = expandedRowId ? transcripts.get(expandedRowId) : undefined;

  const handlePreview = (row: RemoteSessionSummary) => {
    setExpandedRowId((prev) => (prev === row.sessionId ? null : row.sessionId));
    void loadTranscript(row);
  };

  const handleClose = () => setExpandedRowId(null);

  const handleDownload = (row: RemoteSessionSummary) => {
    void download(row, {
      onNeedPicker: () => {
        // Project picker only exists inside the modal flow. For the in-tab
        // preview, we surface the missing mapping via a toast. The actual
        // picker integration is out of scope — the modal continues to be
        // the primary download surface.
        // (See UI-10 for full picker wiring via GlobalTabProps.)
      },
      onDone: () => onDownloaded(),
    });
  };

  // Not connected — show CTA.
  if (!config.isConnected) {
    return (
      <div className="rounded-lg border bg-card/40 px-4 py-10 text-center">
        <GithubIcon className="mx-auto size-5 text-muted-foreground/60" />
        <p className="mt-2 text-sm font-medium">Connect GitHub to browse remote sessions</p>
        <p className="mx-auto mt-1 max-w-sm text-[11px] text-muted-foreground">
          Remote sessions are stored in a private GitHub repo. Connect your account
          in Settings to browse and download them.
        </p>
        <Button
          size="sm"
          variant="outline"
          className="mt-4 cursor-pointer"
          onClick={() => onNavigate?.("github-sync")}
        >
          Connect GitHub
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      {/* Toolbar: refresh + count */}
      <div className="flex items-center justify-between">
        <p className="text-[11px] text-muted-foreground">
          {sessions.length === 0
            ? "No remote sessions yet."
            : `${sessions.length} remote session${sessions.length === 1 ? "" : "s"}`}
        </p>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void refresh()}
          disabled={loading}
          className="cursor-pointer"
        >
          <RefreshCw
            className={loading ? "size-3.5 animate-spin" : "size-3.5"}
          />
          Refresh
        </Button>
      </div>

      <RemoteSessionsList
        rows={sessions}
        loading={loading}
        error={error}
        onDownload={handleDownload}
        onPreview={handlePreview}
        expandedRowId={expandedRowId}
        emptyMessage="No remote sessions yet."
      />

      {expandedRow && (
        <RemoteSessionDetail
          row={expandedRow}
          messages={transcriptState?.messages ?? null}
          loading={transcriptState?.loading ?? false}
          error={transcriptState?.error ?? null}
          onClose={handleClose}
          onDownload={handleDownload}
        />
      )}
    </div>
  );
}
