/* eslint-disable react-hooks/set-state-in-effect --
 * Initial load runs on mount via useEffect; we deliberately use it instead
 * of useSyncExternalStore for a one-shot IPC fetch.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { listMcpServers } from "@/lib/api";
import type { McpServerSummary } from "@/lib/types";

export interface McpServersState {
  /** Null while loading, empty array when none configured. */
  servers: McpServerSummary[] | null;
  loading: boolean;
  refresh: () => Promise<void>;
}

export function useMcpServers(): McpServersState {
  const [servers, setServers] = useState<McpServerSummary[] | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const rows = await listMcpServers();
      setServers(rows);
    } catch (e) {
      toast.error(`Failed to load MCP servers: ${(e as Error).message}`);
      // Leave previous state intact; surfaces a recoverable failure
      // without clearing the visible list on transient errors.
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { servers, loading, refresh };
}