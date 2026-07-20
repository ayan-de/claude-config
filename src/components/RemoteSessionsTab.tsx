"use client";

import { useMemo, useState } from "react";
import { AlertTriangle, Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { GithubIcon } from "@/components/GitHubSync";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import { useRemoteSessions } from "@/hooks/useRemoteSessions";
import { RemoteSessionsList } from "@/components/RemoteSessionsList";
import { RemoteSessionDetail } from "@/components/RemoteSessionDetail";
import { ProjectPickerModal } from "@/components/ProjectPickerModal";
import type { RemoteSessionSummary } from "@/lib/types";
import { AppError } from "@/lib/api";
import type { GlobalTabId } from "@/data/globalTabs";

export class RemoteSessionsError extends Error {
  cta?: { label: string; navigateTo: GlobalTabId };
  constructor(message: string, cta?: { label: string; navigateTo: GlobalTabId }) {
    super(message);
    this.name = "RemoteSessionsError";
    this.cta = cta;
  }
}

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
  const [pickerRow, setPickerRow] = useState<RemoteSessionSummary | null>(null);

  // The hook's own mount effect handles the initial refresh — including
// the localStorage-seeded instant-paint + background reconcile path.
  // (No useEffect needed here anymore.)

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
      onNeedPicker: () => setPickerRow(row),
      onDone: () => onDownloaded(),
    });
  };

  const classified = useMemo(
    () => (error ? classifyError(error) : null),
    [error],
  );
  const initialLoad = loading && sessions.length === 0 && error === null;

  // Throw error when sessions.length === 0 so that parent ErrorBoundary catches it
  if (!initialLoad && error && sessions.length === 0 && classified) {
    throw new RemoteSessionsError(classified.message, classified.cta);
  }

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
      {/* Toolbar — keep as-is, but only show when not in initial-load */}
      {!initialLoad && (
        <div className="flex items-center justify-between">
          <p className="flex items-center gap-2 text-[11px] text-muted-foreground">
            {sessions.length === 0
              ? "No remote sessions yet."
              : `${sessions.length} remote session${sessions.length === 1 ? "" : "s"}`}
            {/* Background refresh indicator — shown when we have cached
                rows on screen but a fresh list is still in flight. */}
            {loading && sessions.length > 0 && (
              <span className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px]">
                <Loader2 className="size-2.5 animate-spin" />
                Refreshing
              </span>
            )}
          </p>
          <Button
            size="sm"
            variant="outline"
            onClick={(e) => {
              // Shift+Click forces a full refetch by invalidating the
              // backend cache first — same UX convention as browser
              // hard-refresh. Plain click uses the SHA-gated path.
              if (e.shiftKey) {
                void import("@/lib/api").then(({ githubInvalidateRemoteCache }) =>
                  githubInvalidateRemoteCache().then(() => refresh()),
                );
              } else {
                void refresh();
              }
            }}
            disabled={loading}
            title="Hold Shift to force-refresh (bypasses cache)"
            className="cursor-pointer"
          >
            <RefreshCw
              className={loading ? "size-3.5 animate-spin" : "size-3.5"}
            />
            Refresh
          </Button>
        </div>
      )}

      {/* Initial load */}
      {initialLoad && (
        <div className="flex flex-col items-center gap-2 rounded-lg border bg-card/40 px-4 py-10">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
          <p className="text-[11px] text-muted-foreground">
            Loading remote sessions…
          </p>
        </div>
      )}

      {/* Error with cached data — slim banner above list */}
      {!initialLoad && error && sessions.length > 0 && classified && (
        <div className="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-1.5 text-[11px]">
          <span className="text-destructive/90">
            Showing cached results — {classified.message}
          </span>
          {classified.retryable && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void refresh()}
              className="cursor-pointer"
            >
              Retry
            </Button>
          )}
        </div>
      )}

      {/* Happy path: list (no error or with cached sessions) */}
      {!initialLoad && (!error || sessions.length > 0) && (
        <RemoteSessionsList
          rows={sessions}
          loading={loading}
          error={null}
          onDownload={handleDownload}
          onPreview={handlePreview}
          expandedRowId={expandedRowId}
          emptyMessage="No remote sessions yet."
        />
      )}

      {/* Detail wrapped in ErrorBoundary */}
      {expandedRow && (
        <ErrorBoundary
          fallback={(err, reset) => (
            <div className="rounded-lg border bg-card/40 px-4 py-6 text-center">
              <AlertTriangle className="mx-auto size-5 text-destructive" />
              <p className="mt-2 text-sm font-medium">Preview failed</p>
              <p className="mx-auto mt-1 max-w-sm text-[11px] text-muted-foreground">
                {err.message}
              </p>
              <div className="mt-3 flex justify-center gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    reset();
                    handleClose();
                  }}
                  className="cursor-pointer"
                >
                  Close
                </Button>
                <Button
                  size="sm"
                  variant="default"
                  onClick={reset}
                  className="cursor-pointer"
                >
                  Retry
                </Button>
              </div>
            </div>
          )}
        >
          <RemoteSessionDetail
            row={expandedRow}
            messages={transcriptState?.messages ?? null}
            loading={transcriptState?.loading ?? false}
            error={transcriptState?.error ?? null}
            onClose={handleClose}
            onDownload={handleDownload}
          />
        </ErrorBoundary>
      )}

      {pickerRow && (
        <ProjectPickerModal
          open
          onClose={() => setPickerRow(null)}
          remoteOriginalPath={pickerRow.originalPath}
          remoteSlug={pickerRow.projectSlug}
          onPicked={() => {
            const row = pickerRow;
            setPickerRow(null);
            void download(row, {
              onNeedPicker: () => setPickerRow(row),
              onDone: () => onDownloaded(),
            });
          }}
        />
      )}
    </div>
  );
}

interface ClassifiedError {
  message: string;
  retryable: boolean;
  cta?: { label: string; navigateTo: GlobalTabId };
}

function classifyError(e: unknown): ClassifiedError {
  const fallback = e instanceof Error ? e.message : String(e);
  if (e instanceof AppError) {
    switch (e.kind) {
      case "github_auth_required":
        return {
          message: "GitHub authentication expired.",
          retryable: false,
          cta: { label: "Reconnect GitHub", navigateTo: "github-sync" },
        };
      case "github_not_configured":
        return {
          message: "GitHub sync isn't configured.",
          retryable: false,
          cta: { label: "Open settings", navigateTo: "github-sync" },
        };
      case "keyring_unavailable":
        return {
          message:
            "OS keyring is unavailable — cannot read GitHub credentials.",
          retryable: false,
        };
      case "validation":
        return { message: e.message, retryable: false };
      case "github_api":
      case "internal":
      default:
        return { message: e.message || fallback, retryable: true };
    }
  }
  return { message: fallback, retryable: true };
}
