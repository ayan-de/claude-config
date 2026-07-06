/* eslint-disable react-hooks/set-state-in-effect --
 * Initial load runs on mount via useEffect; we deliberately use it instead
 * of useSyncExternalStore for a one-shot IPC fetch.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { listMarketplaces } from "@/lib/api";
import type { MarketplaceSummary } from "@/lib/types";

export interface MarketplacesState {
  /** Null while loading, empty array when none registered. */
  marketplaces: MarketplaceSummary[] | null;
  loading: boolean;
  refresh: () => Promise<void>;
}

export function useMarketplaces(): MarketplacesState {
  const [marketplaces, setMarketplaces] = useState<MarketplaceSummary[] | null>(
    null,
  );
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const rows = await listMarketplaces();
      setMarketplaces(rows);
    } catch (e) {
      toast.error(`Failed to load marketplaces: ${(e as Error).message}`);
      // Leave previous state intact; surfaces a recoverable failure
      // without clearing the visible list on transient errors.
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { marketplaces, loading, refresh };
}
