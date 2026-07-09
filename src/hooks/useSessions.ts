/* eslint-disable react-hooks/set-state-in-effect --
 * One-shot IPC fetch on mount. The "cascading render" the rule warns
 * about is bounded to a single follow-up render after the IPC resolves;
 * a ref-guarded microtask doesn't improve the user experience here.
 */
"use client";

import { useCallback, useEffect, useState } from "react";

import { listSessions } from "@/lib/api";
import { isWebEnv } from "@/lib/utils-app";
import type { SessionSummary } from "@/lib/types";

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