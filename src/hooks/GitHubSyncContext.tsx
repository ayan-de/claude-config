"use client";

import { createContext, useContext } from "react";

import { useGitHubSync } from "@/hooks/useGitHubSync";

/**
 * Shared GitHub sync state for the top-bar button and the Connection
 * panel. Without this, each consumer instantiates its own
 * `useGitHubSync()` and a connect in the panel never refreshes the
 * top bar until full reload.
 *
 * `phase` (modal/polling) is also shared: only one panel renders at a
 * time, and putting it in the provider keeps the contract single.
 */
type GitHubSyncContextValue = ReturnType<typeof useGitHubSync>;

const GitHubSyncContext = createContext<GitHubSyncContextValue | null>(null);

export function GitHubSyncProvider({ children }: { children: React.ReactNode }) {
  const value = useGitHubSync();
  return (
    <GitHubSyncContext.Provider value={value}>
      {children}
    </GitHubSyncContext.Provider>
  );
}

export function useGitHubSyncContext(): GitHubSyncContextValue {
  const ctx = useContext(GitHubSyncContext);
  if (!ctx) {
    throw new Error(
      "useGitHubSyncContext must be used inside <GitHubSyncProvider>",
    );
  }
  return ctx;
}
