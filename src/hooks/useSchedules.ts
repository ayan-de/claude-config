/* eslint-disable react-hooks/set-state-in-effect --
 * The initial load runs inside useEffect; computing on render would fire the
 * IPC calls twice. Same one-shot-fetch pattern as useTracker/useMcpServers.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  addSchedule,
  checkSchedulingAvailable,
  deleteSchedule,
  getScheduleStatus,
  listSchedules,
  runPrimerNow,
  setScheduleEnabled,
  updateSchedule,
} from "@/lib/api";
import type {
  Schedule,
  ScheduleInput,
  ScheduleStatus,
  SchedulingAvailability,
} from "@/lib/types";

export interface SchedulesState {
  /** `null` while the first load is in flight. */
  schedules: Schedule[] | null;
  /** Per-schedule status keyed by schedule id. */
  statusById: Record<string, ScheduleStatus>;
  availability: SchedulingAvailability | null;
  loading: boolean;
  busy: boolean;
  lastError: string | null;
  refresh: () => Promise<void>;
  create: (input: ScheduleInput) => Promise<void>;
  update: (input: ScheduleInput) => Promise<void>;
  remove: (id: string) => Promise<void>;
  toggle: (id: string, enabled: boolean) => Promise<void>;
  primeNow: (id: string) => Promise<void>;
}

export function useSchedules(): SchedulesState {
  const [schedules, setSchedules] = useState<Schedule[] | null>(null);
  const [statusById, setStatusById] = useState<Record<string, ScheduleStatus>>(
    {},
  );
  const [availability, setAvailability] =
    useState<SchedulingAvailability | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [list, statuses, avail] = await Promise.all([
        listSchedules(),
        getScheduleStatus().catch(() => [] as ScheduleStatus[]),
        checkSchedulingAvailable().catch(() => null),
      ]);
      setSchedules(list);
      setStatusById(
        Object.fromEntries(statuses.map((s) => [s.scheduleId, s])),
      );
      setAvailability(avail);
      setLastError(null);
    } catch (e) {
      const msg = (e as Error).message;
      setLastError(msg);
      toast.error(`Failed to load schedules: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const create = useCallback(
    async (input: ScheduleInput) => {
      setBusy(true);
      try {
        await addSchedule(input);
        toast.success("Schedule added");
        await refresh();
      } catch (e) {
        setLastError((e as Error).message);
        toast.error(`Add failed: ${(e as Error).message}`);
        throw e;
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const update = useCallback(
    async (input: ScheduleInput) => {
      setBusy(true);
      try {
        await updateSchedule(input);
        toast.success("Schedule updated");
        await refresh();
      } catch (e) {
        setLastError((e as Error).message);
        toast.error(`Update failed: ${(e as Error).message}`);
        throw e;
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const remove = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await deleteSchedule(id);
        toast.success("Schedule deleted");
        await refresh();
      } catch (e) {
        setLastError((e as Error).message);
        toast.error(`Delete failed: ${(e as Error).message}`);
        throw e;
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const toggle = useCallback(
    async (id: string, enabled: boolean) => {
      setBusy(true);
      try {
        await setScheduleEnabled(id, enabled);
        await refresh();
      } catch (e) {
        setLastError((e as Error).message);
        toast.error(`Toggle failed: ${(e as Error).message}`);
        throw e;
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const primeNow = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        const run = await runPrimerNow(id);
        if (run.ok) {
          toast.success("Primer fired");
        } else {
          toast.error(`Primer failed: ${run.error ?? "unknown error"}`);
        }
        await refresh();
      } catch (e) {
        setLastError((e as Error).message);
        toast.error(`Prime now failed: ${(e as Error).message}`);
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  return {
    schedules,
    statusById,
    availability,
    loading,
    busy,
    lastError,
    refresh,
    create,
    update,
    remove,
    toggle,
    primeNow,
  };
}
