"use client";

import { createContext, useContext } from "react";

import type { SessionSummary, SyncState } from "@/lib/types";

/**
 * Per-row upload state for the sessions list. Provided once at the
 * `SessionsView` level and consumed by each `SessionRow`, so the
 * upload handler and sync-state map don't have to be prop-drilled
 * through the accordion/group layers.
 *
 * Null when the sessions list renders outside a provider (e.g. GitHub
 * not connected); rows fall back to a non-interactive mark.
 */
export interface SessionUploadValue {
  stateById: Map<string, SyncState>;
  uploadingIds: Set<string>;
  upload: (session: SessionSummary) => void;
  connected: boolean;
}

const SessionUploadContext = createContext<SessionUploadValue | null>(null);

export function SessionUploadProvider({
  value,
  children,
}: {
  value: SessionUploadValue;
  children: React.ReactNode;
}) {
  return (
    <SessionUploadContext.Provider value={value}>
      {children}
    </SessionUploadContext.Provider>
  );
}

/** Returns the upload context, or null when there's no provider. */
export function useSessionUploadContext(): SessionUploadValue | null {
  return useContext(SessionUploadContext);
}
