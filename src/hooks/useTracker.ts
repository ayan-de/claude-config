/* eslint-disable react-hooks/set-state-in-effect --
 * The initial config load and the auto-refresh interval both run inside
 * useEffect; the alternative (computing on render) would either fire the
 * IPC twice or force us to use useSyncExternalStore for what's really a
 * one-shot fetch + a setInterval.
 */
"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  deleteTrackerConfig,
  getTrackerConfig,
  refreshTracker,
  saveTrackerConfig,
} from "@/lib/api";
import type { TrackerConfigView } from "@/lib/types";

export interface TrackerState {
  /** Saved config + cached usage. `null` while loading, `undefined` when
   *  no config has been saved yet. */
  config: TrackerConfigView | null | undefined;
  loading: boolean;
  saving: boolean;
  refreshing: boolean;
  /** True while the auto-refresh interval is active. The UI can show a
   *  "Live" badge based on this. */
  autoRefresh: boolean;
  /** Last refresh error, if any. Stays on screen until the next refresh
   *  succeeds. */
  lastError: string | null;
  save: (source: string, fields: Record<string, unknown>) => Promise<void>;
  refresh: () => Promise<void>;
  remove: () => Promise<void>;
  startAutoRefresh: (intervalMs?: number) => void;
  stopAutoRefresh: () => void;
}

/**
 * Owns the state for one provider's tracker tab. Auto-refresh defaults
 * to 60s — the user can pause it by calling `stopAutoRefresh` (the UI
 * toggles this via a "Live" / "Paused" button).
 */
export function useTracker(providerId: string | null): TrackerState {
  const [config, setConfig] = useState<TrackerConfigView | null | undefined>(
    null,
  );
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // Latest refresh fn in a ref so the interval closure always sees the
  // current implementation (avoids stale-closure bugs if `refresh`'s
  // deps change).
  const refreshRef = useRef<() => Promise<void>>(async () => {});

  const refresh = useCallback(async () => {
    if (!providerId) return;
    setRefreshing(true);
    try {
      // The backend now returns the full config view (including updated
      // usage + last_error) in one shot, so we no longer need a second
      // getTrackerConfig round-trip.
      const fresh = await refreshTracker(providerId);
      setConfig(fresh);
      setLastError(fresh.last_error ?? null);
      if (typeof window !== "undefined") {
        window.dispatchEvent(new CustomEvent("tracker-changed"));
      }
    } catch (e) {
      setLastError((e as Error).message);
    } finally {
      setRefreshing(false);
    }
  }, [providerId]);

  // Keep the ref pointed at the latest `refresh`.
  useEffect(() => {
    refreshRef.current = refresh;
  }, [refresh]);

  // Initial load — fetch the saved config (or undefined if none).
  useEffect(() => {
    let cancelled = false;
    if (!providerId) {
      setConfig(undefined);
      return;
    }
    setLoading(true);
    (async () => {
      try {
        const c = await getTrackerConfig(providerId);
        if (!cancelled) {
          setConfig(c);
          setLastError(c.last_error ?? null);
        }
      } catch (e) {
        // `getTrackerConfig` errors with NotFound when no config exists
        // yet — treat that as "unconfigured" rather than an error toast.
        const msg = (e as Error).message;
        if (/not found/i.test(msg) || /no tracker config/i.test(msg)) {
          if (!cancelled) {
            setConfig(undefined);
            setLastError(null);
          }
        } else {
          if (!cancelled) setLastError(msg);
          toast.error(`Failed to load tracker: ${msg}`);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [providerId]);

  const save = useCallback(
    async (source: string, fields: Record<string, unknown>) => {
      if (!providerId) return;
      setSaving(true);
      try {
        const view = await saveTrackerConfig(providerId, source, fields);
        setConfig(view);
        setLastError(null);
        toast.success("Tracker saved");
        if (typeof window !== "undefined") {
          window.dispatchEvent(new CustomEvent("tracker-changed"));
        }
      } catch (e) {
        setLastError((e as Error).message);
        toast.error(`Save failed: ${(e as Error).message}`);
        throw e; // re-throw so the form can keep its error state visible
      } finally {
        setSaving(false);
      }
    },
    [providerId],
  );

  const remove = useCallback(async () => {
    if (!providerId) return;
    setSaving(true);
    try {
      await deleteTrackerConfig(providerId);
      setConfig(undefined);
      setLastError(null);
      toast.success("Tracker removed");
      if (typeof window !== "undefined") {
        window.dispatchEvent(new CustomEvent("tracker-changed"));
      }
    } catch (e) {
      setLastError((e as Error).message);
      toast.error(`Remove failed: ${(e as Error).message}`);
      throw e;
    } finally {
      setSaving(false);
    }
  }, [providerId]);

  const stopAutoRefresh = useCallback(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
    setAutoRefresh(false);
  }, []);

  const startAutoRefresh = useCallback(
    (intervalMs: number = 60_000) => {
      stopAutoRefresh();
      intervalRef.current = setInterval(() => {
        void refreshRef.current();
      }, intervalMs);
      setAutoRefresh(true);
    },
    [stopAutoRefresh],
  );

  // Tear down the interval on unmount so polling stops when the modal
  // closes.
  useEffect(() => {
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, []);

  return {
    config,
    loading,
    saving,
    refreshing,
    autoRefresh,
    lastError,
    save,
    refresh,
    remove,
    startAutoRefresh,
    stopAutoRefresh,
  };
}
