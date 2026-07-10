/* eslint-disable react-hooks/set-state-in-effect --
 * One-shot IPC fetch on mount. Mirrors useSessions.ts: the cascading
 * render the rule warns about is bounded to a single follow-up render
 * after the IPC resolves; a ref-guarded microtask doesn't improve UX.
 */
"use client";

import { useCallback, useEffect, useState } from "react";

import {
  getGithubSyncConfig,
  githubDisconnect,
  githubPollDeviceFlow,
  githubSetPrivacyConsent,
  githubSetRepoName,
  githubStartDeviceFlow,
} from "@/lib/api";
import { isWebEnv } from "@/lib/utils-app";
import type {
  GitHubDeviceFlowStart,
  GitHubPollOutcome,
  GitHubSyncConfig,
} from "@/lib/types";

const DEFAULT_CONFIG: GitHubSyncConfig = {
  schemaVersion: 1,
  isConnected: false,
  username: null,
  avatarUrl: null,
  repoName: "claude-sessions",
  lastSync: null,
  privacyConsentGiven: false,
};

export type OAuthPhase =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "waiting"; flow: GitHubDeviceFlowStart }
  | { kind: "polling"; flow: GitHubDeviceFlowStart }
  | {
      kind: "success";
      username: string;
      avatarUrl: string | null;
    }
  | { kind: "denied" }
  | { kind: "expired" }
  | { kind: "error"; message: string };

/**
 * Owns the GitHub connection state. Phase 1 surface only — Phase 2 will
 * add upload/download hooks layered on top.
 */
export function useGitHubSync() {
  const [config, setConfig] = useState<GitHubSyncConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(true);
  const [phase, setPhase] = useState<OAuthPhase>({ kind: "idle" });

  const refresh = useCallback(async () => {
    try {
      const cfg = await getGithubSyncConfig();
      setConfig(cfg);
    } catch {
      // Non-fatal — fall back to defaults; the UI will surface the
      // next action that needs backend access.
      setConfig(DEFAULT_CONFIG);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (isWebEnv()) void refresh();
  }, [refresh]);

  const startDeviceFlow = useCallback(async () => {
    setPhase({ kind: "starting" });
    try {
      const flow = await githubStartDeviceFlow();
      setPhase({ kind: "waiting", flow });
      return flow;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setPhase({ kind: "error", message });
      throw e;
    }
  }, []);

  const pollOnce = useCallback(
    async (deviceCode: string): Promise<GitHubPollOutcome> => {
      const outcome = await githubPollDeviceFlow(deviceCode);
      switch (outcome.status) {
        case "authorized":
          setPhase({
            kind: "success",
            username: outcome.username,
            avatarUrl: outcome.avatarUrl,
          });
          await refresh();
          break;
        case "denied":
          setPhase({ kind: "denied" });
          break;
        case "expired":
          setPhase({ kind: "expired" });
          break;
        case "pending":
        case "slow_down":
          // Keep waiting — caller is responsible for the timer.
          break;
      }
      return outcome;
    },
    [refresh],
  );

  const reset = useCallback(() => setPhase({ kind: "idle" }), []);

  const disconnect = useCallback(async () => {
    await githubDisconnect();
    setPhase({ kind: "idle" });
    await refresh();
  }, [refresh]);

  const setRepoName = useCallback(
    async (repoName: string) => {
      await githubSetRepoName(repoName);
      await refresh();
    },
    [refresh],
  );

  const setPrivacyConsent = useCallback(
    async (given: boolean) => {
      await githubSetPrivacyConsent(given);
      await refresh();
    },
    [refresh],
  );

  return {
    config,
    loading,
    phase,
    refresh,
    startDeviceFlow,
    pollOnce,
    reset,
    disconnect,
    setRepoName,
    setPrivacyConsent,
  };
}