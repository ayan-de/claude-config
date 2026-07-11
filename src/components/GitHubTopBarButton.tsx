"use client";

import { useState } from "react";

import { Button } from "@/components/ui/button";
import { RemoteSessionsModal } from "@/components/RemoteSessionsModal";
import { useSessions } from "@/hooks/useSessions";
import { cn } from "@/lib/utils";
import type { GitHubSyncConfig } from "@/lib/types";

interface Props {
  config: GitHubSyncConfig | null;
  loading: boolean;
  active: boolean;
  onClick: () => void;
}

/**
 * Top-bar indicator for the GitHub connection. Mirrors the existing
 * `Sessions` button pattern (icon + toggle, square pill, tauri-no-drag
 * so the OS window drag still works on the rest of the bar).
 *
 * - Disconnected: full "Connect GitHub" pill with the GitHub mark
 * - Connected: round avatar (32px) with a small green dot in the corner
 *   so users can see at a glance whether their sync is wired up
 */
export function GitHubTopBarButton({ config, loading, active, onClick }: Props) {
  const connected = !loading && config?.isConnected;
  const avatarUrl = config?.avatarUrl ?? null;
  const username = config?.username ?? null;
  const { refresh: refreshSessions } = useSessions();
  const [remoteOpen, setRemoteOpen] = useState(false);

  // When connected: open the remote-sessions modal. When disconnected:
  // fall through to the parent-supplied `onClick` so the existing
  // settings-panel flow (OAuth connect) keeps working.
  const handleClick = () => {
    if (connected) {
      setRemoteOpen(true);
    } else {
      onClick();
    }
  };

  if (connected && avatarUrl) {
    return (
      <>
        <button
          type="button"
          onClick={handleClick}
          title={
            username
              ? `GitHub sync: connected as ${username} — click to browse remote sessions`
              : "GitHub sync: connected — click to browse remote sessions"
          }
          aria-label="GitHub sync (connected)"
          aria-pressed={active}
          className={cn(
            "tauri-no-drag relative flex size-7 shrink-0 cursor-pointer items-center justify-center overflow-hidden rounded-full transition",
            active
              ? "ring-2 ring-primary ring-offset-1 ring-offset-card/30"
              : "ring-1 ring-foreground/15 hover:ring-foreground/40",
          )}
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={avatarUrl}
            alt=""
            className="size-full object-cover"
            draggable={false}
          />
          <span
            aria-hidden
            className="absolute right-0 bottom-0 size-2 rounded-full bg-emerald-500 ring-2 ring-card"
          />
        </button>
        <RemoteSessionsModal
          open={remoteOpen}
          onClose={() => setRemoteOpen(false)}
          onDownloaded={() => void refreshSessions()}
        />
      </>
    );
  }

  return (
    <>
      <Button
        type="button"
        onClick={handleClick}
        variant="ghost"
        size="sm"
        title="Connect GitHub to back up sessions"
        aria-label="Connect GitHub"
        aria-pressed={active}
        className={cn(
          "tauri-no-drag h-7 gap-1.5 rounded-md px-2 text-xs font-medium",
          active && "bg-primary/15 text-primary",
        )}
      >
        <GithubIconMark className="size-3.5" />
        <span>Connect</span>
      </Button>
      <RemoteSessionsModal
        open={remoteOpen}
        onClose={() => setRemoteOpen(false)}
        onDownloaded={() => void refreshSessions()}
      />
    </>
  );
}

/**
 * Hand-coded GitHub mark — same path used in GitHubSync.tsx and the
 * session row in Sessions.tsx. lucide-react doesn't ship one.
 */
function GithubIconMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden
      className={className}
      fill="currentColor"
    >
      <path d="M12 .3a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2c-3.3.7-4-1.6-4-1.6-.6-1.4-1.4-1.8-1.4-1.8-1.1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.6-.3-5.4-1.3-5.4-5.9 0-1.3.5-2.4 1.2-3.2-.1-.3-.5-1.5.1-3.2 0 0 1-.3 3.3 1.2a11.5 11.5 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.6 1.7.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.2 0 4.6-2.8 5.6-5.4 5.9.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A12 12 0 0 0 12 .3Z" />
    </svg>
  );
}