"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import {
  AppError,
  githubDownloadSession,
  githubFetchRemoteTranscript,
  githubInvalidateRemoteCache,
  githubListRemoteSessions,
  githubResolveDownloadTarget,
} from "@/lib/api";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import type {
  DownloadResult,
  RemoteSessionSummary,
  SessionMessage,
} from "@/lib/types";

const CACHE_KEY = "remoteSessions:v2";
const CACHE_TTL_MS = 24 * 60 * 60 * 1000;
/** Force a fresh list before any download if the last refresh was older than this. */
const STALE_BEFORE_DOWNLOAD_MS = 60_000;

interface CachedSessions {
  sessions: RemoteSessionSummary[];
  savedAt: number;
}

function readCachedSessions(): CachedSessions | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CachedSessions;
    if (!parsed || !Array.isArray(parsed.sessions)) return null;
    if (typeof parsed.savedAt !== "number") return null;
    if (Date.now() - parsed.savedAt > CACHE_TTL_MS) return null;
    return parsed;
  } catch {
    return null;
  }
}

function writeCachedSessions(sessions: RemoteSessionSummary[]): void {
  if (typeof window === "undefined") return;
  try {
    const payload: CachedSessions = { sessions, savedAt: Date.now() };
    window.localStorage.setItem(CACHE_KEY, JSON.stringify(payload));
  } catch {
    // Quota / private-mode failures are non-fatal — the in-memory copy
    // still works for this session.
  }
}

export function clearCachedRemoteSessions(): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(CACHE_KEY);
  } catch {
    // ignore
  }
}

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
  const { config } = useGitHubSyncContext();
  // Seed initial state from localStorage so the tab paints instantly on
  // remount. `loading` stays false because we already have something to
  // show — a background refresh will fire after mount.
  const [state, setState] = useState<State>(() => {
    const cached = readCachedSessions();
    return {
      sessions: cached?.sessions ?? [],
      loading: cached === null,
      error: null,
    };
  });

  const lastRefreshAtRef = useRef<number>(
    readCachedSessions()?.savedAt ?? 0,
  );

  const refresh = useCallback(async () => {
    setState((s) => ({ ...s, loading: true, error: null }));
    try {
      const sessions = await githubListRemoteSessions();
      writeCachedSessions(sessions);
      lastRefreshAtRef.current = Date.now();
      setState({ sessions, loading: false, error: null });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      // Don't blow away the cached rows if the network failed —
      // `RemoteSessionsTab` already shows the slim "Showing cached
      // results" banner when `error` is set alongside non-empty
      // `sessions`.
      setState((prev) => ({
        sessions: prev.sessions,
        loading: false,
        error: msg,
      }));
    }
  }, []);

  // Always refresh on mount. When we seeded from localStorage, the
  // cached rows are already on screen and this is a "background"
// reconcile. On a cold mount with no cache, this is what unsticks the
  // loading spinner (refresh's own setState eventually flips loading
  // back to false).
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- refresh() resolves asynchronously; setState happens in a microtask, not synchronously inside the effect body.
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Disconnect → wipe the local cache tier (side effect) so a stale
  // list from the previous user doesn't bleed into the next session.
  // The visible rows are derived from `config.isConnected` below so
  // we don't need setState-in-effect here — the parent re-renders on
  // the context change and we naturally show the empty-state.
  useEffect(() => {
    if (!config.isConnected) {
      clearCachedRemoteSessions();
      lastRefreshAtRef.current = 0;
    }
  }, [config.isConnected]);

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
      // Stale-row protection: if our last list is older than
      // STALE_BEFORE_DOWNLOAD_MS, force a fresh refetch first. If the
      // row's SHA no longer matches anything in the updated list, the
      // session was deleted upstream and we'd 404 on download.
      if (Date.now() - lastRefreshAtRef.current > STALE_BEFORE_DOWNLOAD_MS) {
        await githubInvalidateRemoteCache();
        await refresh();
      }
      const fresh = readCachedSessions()?.sessions ?? state.sessions;
      if (!fresh.some((r) => r.sessionId === row.sessionId && r.sha === row.sha)) {
        toast.error(
          "Session no longer on remote — refresh to see latest list",
        );
        return;
      }

      // Peek at the mapping first so we don't burn an API round-trip
      // when we already know the picker is needed.
      const target = await githubResolveDownloadTarget(row.projectSlug);
      if (!target) {
        cb.onNeedPicker();
        return;
      }
      await doDownload(row, cb, false);
    },
    [doDownload, refresh, state.sessions],
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

  return {
    sessions: config.isConnected ? state.sessions : [],
    loading: config.isConnected ? state.loading : true,
    error: config.isConnected ? state.error : null,
    refresh,
    download,
    transcripts,
    loadTranscript,
  };
}