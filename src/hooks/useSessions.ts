/* eslint-disable react-hooks/set-state-in-effect --
 * One-shot IPC fetch on mount. The "cascading render" the rule warns
 * about is bounded to a single follow-up render after the IPC resolves;
 * a ref-guarded microtask doesn't improve the user experience here.
 */
"use client";

import { useCallback, useEffect, useState } from "react";

import { listSessions, parseSession } from "@/lib/api";
import { isWebEnv } from "@/lib/utils-app";
import type { SessionMessage, SessionSummary } from "@/lib/types";

/**
 * Loads Claude Code conversation sessions stored on this PC.
 *
 * Single fetch on mount — the sidebar refreshes when the underlying
 * filesystem changes only on app restart; per-edit mtime polling is
 * out of scope for v1. Returns `[]` while loading and on error so the
 * UI can render an empty state instead of crashing.
 *
 * ponytail: one fetch, no retry, no cache — add a TTL or a manual
 * refresh button when stale data becomes a real complaint.
 */
export function useSessions() {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const list = await listSessions();
      setSessions(list);
    } catch {
      // Surfaces empty in the UI; non-fatal — listSessions errors are
      // almost always "no ~/.claude/projects dir yet".
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (isWebEnv()) void refresh();
  }, [refresh]);

  return { sessions, loading, refresh };
}

/**
 * Loads a single session's parsed transcript on demand. Re-fetches
 * whenever `path` changes (e.g. user picks a different row).
 *
 * Returns `{ messages: null, loading: true }` until the first fetch
 * resolves; the detail view renders a spinner in that state.
 */
export function useSessionTranscript(path: string | null) {
  const [messages, setMessages] = useState<SessionMessage[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!path) {
      setMessages(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    parseSession(path)
      .then((m) => {
        if (cancelled) return;
        setMessages(m);
        setLoading(false);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  return { messages, loading, error };
}