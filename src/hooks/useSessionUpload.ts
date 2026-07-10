/* eslint-disable react-hooks/set-state-in-effect --
 * Seeding sync-state from IPC on mount is the same bounded single
 * follow-up render as useSessions.ts; a ref-guarded microtask adds
 * complexity without changing what the user sees.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import {
  AppError,
  githubGetSessionSyncState,
  githubUploadSession,
} from "@/lib/api";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import { isWebEnv } from "@/lib/utils-app";
import type { SessionSummary, SyncState } from "@/lib/types";

/** Parent directory of a transcript path — the project folder Claude Code
 * created (`<claude_dir>/projects/<slug>/`). Used as the key for the
 * per-project sync-state file. */
function projectFolderOf(fullPath: string): string {
  const idx = fullPath.lastIndexOf("/");
  return idx > 0 ? fullPath.slice(0, idx) : fullPath;
}

interface UseSessionUpload {
  /** Current sync state per session id. Absent = never_uploaded. */
  stateById: Map<string, SyncState>;
  /** Session ids with an upload in flight (icon shows a spinner). */
  uploadingIds: Set<string>;
  upload: (session: SessionSummary) => Promise<void>;
  /** Re-seed states for the given sessions (called on list load/refresh). */
  seed: (sessions: SessionSummary[]) => Promise<void>;
}

/**
 * Owns per-session upload state for the sessions list so individual rows
 * don't each re-implement it. Reads connection + consent from the shared
 * GitHubSync context; seeds each project's sync-state map once, then
 * updates optimistically as uploads resolve.
 */
export function useSessionUpload(sessions: SessionSummary[]): UseSessionUpload {
  const { config, setPrivacyConsent, disconnect } = useGitHubSyncContext();
  const [stateById, setStateById] = useState<Map<string, SyncState>>(
    () => new Map(),
  );
  const [uploadingIds, setUploadingIds] = useState<Set<string>>(
    () => new Set(),
  );

  const seed = useCallback(async (list: SessionSummary[]) => {
    // One sync-state fetch per distinct project folder, then flatten into
    // the id->state map. Failures are non-fatal (unconnected/never-synced
    // projects simply have no state file).
    const folders = new Set<string>();
    for (const s of list) {
      if (s.full_path) folders.add(projectFolderOf(s.full_path));
    }
    const next = new Map<string, SyncState>();
    await Promise.all(
      [...folders].map(async (folder) => {
        try {
          const file = await githubGetSessionSyncState(folder);
          for (const [id, meta] of Object.entries(file.sessions)) {
            next.set(id, meta.syncState);
          }
        } catch {
          // No state file / not connected — leave those rows as
          // never_uploaded (absent from the map).
        }
      }),
    );
    setStateById(next);
  }, []);

  useEffect(() => {
    if (isWebEnv() && config.isConnected) void seed(sessions);
    // Re-seed when connection flips or the session set changes identity.
  }, [seed, sessions, config.isConnected]);

  const doUpload = useCallback(
    async (session: SessionSummary): Promise<void> => {
      const id = session.session_id;
      setUploadingIds((prev) => new Set(prev).add(id));
      try {
        // Up to two attempts: the first may fail with "consent required",
        // in which case we confirm, set the flag, and retry once. A loop
        // (rather than recursion) keeps this a single stable callback.
        for (let attempt = 0; attempt < 2; attempt++) {
          try {
            const meta = await githubUploadSession(
              id,
              session.full_path,
              session.project_path ?? "",
            );
            setStateById((prev) => new Map(prev).set(id, meta.syncState));
            toast.success("Session uploaded to GitHub");
            return;
          } catch (e) {
            const kind = e instanceof AppError ? e.kind : undefined;
            const message = e instanceof Error ? e.message : String(e);

            // Privacy consent required — confirm once, set flag, retry.
            const needsConsent =
              kind === "github_not_configured" ||
              message.includes("privacy_consent_required");
            if (attempt === 0 && needsConsent) {
              const ok = window.confirm(
                "This session may contain sensitive information — file " +
                  "contents, environment variables, and command output. " +
                  "Upload it to your private GitHub repo?",
              );
              if (!ok) return;
              await setPrivacyConsent(true);
              continue;
            }

            // Token revoked/expired — clear connection, prompt reconnect.
            if (kind === "github_auth_required") {
              await disconnect();
              toast.error("GitHub connection expired. Please reconnect.");
              return;
            }

            toast.error(`Upload failed: ${message}`);
            return;
          }
        }
      } finally {
        setUploadingIds((prev) => {
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
      }
    },
    [setPrivacyConsent, disconnect],
  );

  const upload = useCallback(
    async (session: SessionSummary): Promise<void> => {
      if (!config.isConnected) {
        toast.error("Connect GitHub in Settings before uploading sessions.");
        return;
      }
      const current = stateById.get(session.session_id);
      // Out-of-sync means the file changed after the last upload — confirm
      // before overwriting the remote copy.
      if (current === "out_of_sync") {
        const ok = window.confirm(
          "This session has local changes since the last upload. Update the " +
            "remote copy?",
        );
        if (!ok) return;
      }
      await doUpload(session);
    },
    [config.isConnected, stateById, doUpload],
  );

  return { stateById, uploadingIds, upload, seed };
}
