"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import {
  AppError,
  githubDownloadSession,
  githubFetchRemoteTranscript,
  githubListRemoteSessions,
  githubResolveDownloadTarget,
} from "@/lib/api";
import type {
  DownloadResult,
  RemoteSessionSummary,
  SessionMessage,
} from "@/lib/types";

interface DownloadCallbacks {
  /** Fires when no local mapping exists yet — parent opens the picker. */
  onNeedPicker: () => void;
  /** Fires after a successful download. */
  onDone: (result: DownloadResult) => void;
}

interface State {
  sessions: RemoteSessionSummary[];
  loading: boolean;
  error: string | null;
}

interface TranscriptState {
  messages: SessionMessage[] | null;
  loading: boolean;
  error: string | null;
}

type DoDownload = (
  row: RemoteSessionSummary,
  cb: DownloadCallbacks,
  force: boolean,
) => Promise<void>;

export function useRemoteSessions() {
  const [state, setState] = useState<State>({
    sessions: [],
    loading: true,
    error: null,
  });

  const refresh = useCallback(async () => {
    setState((s) => ({ ...s, loading: true, error: null }));
    try {
      const sessions = await githubListRemoteSessions();
      setState({ sessions, loading: false, error: null });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setState({ sessions: [], loading: false, error: msg });
    }
  }, []);

  // The recursive force-resubmit path inside `doDownload` needs a stable
  // self-reference; resolve it through a ref to keep `doDownload` itself
  // stable across renders (so `download`'s dep array stays empty).
  const doDownloadRef = useRef<DoDownload>(() => Promise.resolve());
  const doDownload = useCallback<DoDownload>(
    async (row, cb, force) => {
      try {
        const result = await githubDownloadSession(
          row.sessionId,
          row.projectSlug,
          row.sha,
          force,
        );
        toast.success("Session downloaded");
        cb.onDone(result);
      } catch (e) {
        const kind = e instanceof AppError ? e.kind : undefined;
        const message = e instanceof Error ? e.message : String(e);

        // Conflict — inspect the message string for the variant name.
        if (kind === "session_download_conflict") {
          const remoteNewer = message.includes("RemoteNewer");
          const proceed = window.confirm(
            remoteNewer
              ? "Remote copy is newer than the local file. Overwrite local?"
              : "Local copy is newer than the remote file. Overwrite with remote?",
          );
          if (!proceed) return;
          await doDownloadRef.current(row, cb, true);
          return;
        }

        // Missing mapping — surface to parent so it can open the picker.
        if (message.includes("path_mapping_required")) {
          cb.onNeedPicker();
          return;
        }

        toast.error(`Download failed: ${message}`);
      }
    },
    [],
  );
  useEffect(() => {
    doDownloadRef.current = doDownload;
  }, [doDownload]);

  const download = useCallback(
    async (row: RemoteSessionSummary, cb: DownloadCallbacks) => {
      // Peek at the mapping first so we don't burn an API round-trip
      // when we already know the picker is needed.
      const target = await githubResolveDownloadTarget(row.projectSlug);
      if (!target) {
        cb.onNeedPicker();
        return;
      }
      await doDownload(row, cb, false);
    },
    [doDownload],
  );

  // Per-row transcript cache. Looked up by `sessionId`; populated lazily
  // by `loadTranscript` when the user opens a row in the Remote tab.
  const [transcripts, setTranscripts] = useState<Map<string, TranscriptState>>(
    () => new Map(),
  );
  // In-flight promises live alongside the state map so duplicate
  // `loadTranscript` calls for the same id share a single fetch.
  const inFlightRef = useRef<Map<string, Promise<SessionMessage[] | null>>>(
    new Map(),
  );
  const loadTranscript = useCallback(
    async (row: RemoteSessionSummary): Promise<SessionMessage[] | null> => {
      const id = row.sessionId;
      const cached = transcripts.get(id);
      // Already resolved — return cached messages.
      if (cached && cached.messages !== null) {
        return cached.messages;
      }
      // In flight — await the existing promise so concurrent callers
      // share a single fetch.
      if (cached && cached.loading) {
        const existing = inFlightRef.current.get(id);
        if (existing) return existing;
      }
      setTranscripts((prev) => {
        const next = new Map(prev);
        next.set(id, { messages: null, loading: true, error: null });
        return next;
      });
      const promise = (async (): Promise<SessionMessage[] | null> => {
        try {
          const messages = await githubFetchRemoteTranscript(
            row.sessionId,
            row.sha,
          );
          setTranscripts((prev) => {
            const next = new Map(prev);
            next.set(id, { messages, loading: false, error: null });
            return next;
          });
          return messages;
        } catch (e) {
          const message = e instanceof Error ? e.message : String(e);
          setTranscripts((prev) => {
            const next = new Map(prev);
            next.set(id, { messages: null, loading: false, error: message });
            return next;
          });
          return null;
        } finally {
          inFlightRef.current.delete(id);
        }
      })();
      inFlightRef.current.set(id, promise);
      return promise;
    },
    [transcripts],
  );

  return { ...state, refresh, download, transcripts, loadTranscript };
}