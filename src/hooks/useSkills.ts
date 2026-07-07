/* eslint-disable react-hooks/set-state-in-effect --
 * Initial load runs on mount via useEffect; we deliberately use it instead
 * of useSyncExternalStore for a one-shot IPC fetch.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { listSkills } from "@/lib/api";
import type { SkillSummary } from "@/lib/types";

export interface SkillsState {
  /** Null while loading, empty array when none found. */
  skills: SkillSummary[] | null;
  loading: boolean;
  refresh: () => Promise<void>;
}

export function useSkills(): SkillsState {
  const [skills, setSkills] = useState<SkillSummary[] | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const rows = await listSkills();
      setSkills(rows);
    } catch (e) {
      toast.error(`Failed to load skills: ${(e as Error).message}`);
      // Leave previous state intact; surfaces a recoverable failure
      // without clearing the visible list on transient errors.
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { skills, loading, refresh };
}