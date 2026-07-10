/* eslint-disable react-hooks/set-state-in-effect --
 * Resyncing local input state from canonical config in an effect is
 * the same pattern as useSessions.ts: bounded single follow-up render
 * after IPC resolves; ref-guarded microtask adds complexity without UX gain.
 */
"use client";

import { useEffect, useRef, useState } from "react";

import {
  CheckCircle2,
  Copy,
  ExternalLink,
  GitBranch,
  KeyRound,
  Loader2,
  LogOut,
  ShieldCheck,
  X,
} from "lucide-react";

import { githubOpenVerificationUrl } from "@/lib/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useGitHubSync } from "@/hooks/useGitHubSync";
import { cn } from "@/lib/utils";
import type { GitHubPollOutcome } from "@/lib/types";

/**
 * Inline GitHub mark — lucide-react doesn't ship a `Github` icon, so
 * we use the same hand-coded SVG that already lives in Sessions.tsx
 * (the per-row upload affordance). Keep them visually consistent.
 */
function GithubIcon({ className }: { className?: string }) {
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

/**
 * Sidebar entry for the GitHub Sync tab. Mirrors the pattern of the
 * other `*SidebarButton` components — keeps the visual style consistent
 * and gives the global tabs registry a stable handle.
 */
export function GitHubSyncSidebarButton({
  active,
  onSelect,
}: {
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <Button
      type="button"
      variant={active ? "secondary" : "ghost"}
      size="sm"
      onClick={onSelect}
      className={cn("w-full justify-start gap-2", active && "font-medium")}
      aria-pressed={active}
    >
      <GithubIcon className="size-4" />
      GitHub Sync
    </Button>
  );
}

interface GitHubSyncPanelProps {
  className?: string;
  /** Matches the GlobalTabProps contract; not used in v1 — kept so
   *  adding a "Done" button later doesn't require a type change. */
  onClose?: () => void;
}

/**
 * Settings-panel surface for GitHub session sync. Phase 1 scope:
 * connection / disconnect, repo name, privacy consent, and the device
 * flow modal. Phase 2+ adds upload/download + remote-session browser
 * — that UI lives in `Sessions.tsx` (per-row icons) and a sibling
 * modal that opens from this panel.
 */
export function GitHubSyncPanel({ className }: GitHubSyncPanelProps) {
  const {
    config,
    loading,
    phase,
    startDeviceFlow,
    pollOnce,
    reset,
    disconnect,
    setRepoName,
    setPrivacyConsent,
  } = useGitHubSync();

  const [repoNameInput, setRepoNameInput] = useState(config.repoName);
  const [savedRepoName, setSavedRepoName] = useState(false);

  // Resync local input when the canonical config changes (e.g. after
  // a successful refresh). Same pattern as useSessions.ts.
  useEffect(() => {
    setRepoNameInput(config.repoName);
  }, [config.repoName]);

  const handleConnect = () => {
    void startDeviceFlow();
  };

  const handleSaveRepoName = async () => {
    const trimmed = repoNameInput.trim();
    if (!trimmed || trimmed === config.repoName) return;
    try {
      await setRepoName(trimmed);
      setSavedRepoName(true);
      window.setTimeout(() => setSavedRepoName(false), 2000);
    } catch (e) {
      console.error("failed to save repo name", e);
    }
  };

  const modalOpen = phase.kind === "waiting" || phase.kind === "polling";

  return (
    <div
      className={cn(
        "mx-auto flex w-full max-w-2xl flex-col gap-6 p-6",
        className,
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <h2 className="flex items-center gap-2 text-lg font-semibold">
            <GithubIcon className="size-5" />
            GitHub session sync
          </h2>
          <p className="text-sm text-muted-foreground">
            Backup Claude Code conversation transcripts to a private GitHub
            repo. Upload, then resume on any other machine.
          </p>
        </div>
        <ConnectionBadge
          loading={loading}
          isConnected={config.isConnected}
          username={config.username ?? null}
        />
      </div>

      <section className="rounded-lg border bg-card/40 p-4">
        <h3 className="text-sm font-medium">Connection</h3>
        {config.isConnected ? (
          <ConnectedActions
            username={config.username ?? null}
            onDisconnect={() => void disconnect()}
          />
        ) : (
          <DisconnectedActions
            onConnect={handleConnect}
            phaseKind={phase.kind}
            errorMessage={phase.kind === "error" ? phase.message : null}
          />
        )}
      </section>

      <section className="rounded-lg border bg-card/40 p-4">
        <h3 className="text-sm font-medium">Repository</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Private repo on your account where sessions will be stored.
          Created automatically on first upload.
        </p>
        <div className="mt-3 flex gap-2">
          <div className="flex-1">
            <Label htmlFor="repo-name" className="sr-only">
              Repository name
            </Label>
            <Input
              id="repo-name"
              value={repoNameInput}
              onChange={(e) => setRepoNameInput(e.target.value)}
              placeholder="claude-sessions"
              spellCheck={false}
            />
          </div>
          <Button
            type="button"
            variant="secondary"
            onClick={() => void handleSaveRepoName()}
            disabled={
              !repoNameInput.trim() || repoNameInput.trim() === config.repoName
            }
          >
            {savedRepoName ? (
              <>
                <CheckCircle2 className="size-4 text-emerald-500" />
                Saved
              </>
            ) : (
              "Save"
            )}
          </Button>
        </div>
      </section>

      <section className="rounded-lg border bg-card/40 p-4">
        <h3 className="flex items-center gap-2 text-sm font-medium">
          <ShieldCheck className="size-4" />
          Privacy
        </h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Session transcripts can contain file contents, command output, and
          environment variables. You&apos;ll be asked to confirm before any
          transcript leaves this machine.
        </p>
        <label className="mt-3 flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={config.privacyConsentGiven}
            onChange={(e) => void setPrivacyConsent(e.target.checked)}
            className="size-4 rounded border-border"
          />
          <span>
            I understand that uploaded transcripts may contain sensitive data
          </span>
        </label>
      </section>

      <DeviceFlowModal
        open={modalOpen}
        phase={phase}
        onPoll={async () => {
          if (phase.kind === "waiting" || phase.kind === "polling") {
            return await pollOnce(phase.flow.deviceCode);
          }
          return null;
        }}
        onClose={() => reset()}
      />
    </div>
  );
}

function ConnectionBadge({
  loading,
  isConnected,
  username,
}: {
  loading: boolean;
  isConnected: boolean;
  username: string | null;
}) {
  if (loading) {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-full border bg-muted/50 px-2.5 py-1 text-xs text-muted-foreground">
        <Loader2 className="size-3 animate-spin" />
        Loading…
      </span>
    );
  }
  if (!isConnected) {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-full border bg-muted/50 px-2.5 py-1 text-xs text-muted-foreground">
        <span className="size-1.5 rounded-full bg-muted-foreground/60" />
        Not connected
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border bg-emerald-500/10 px-2.5 py-1 text-xs text-emerald-600 dark:text-emerald-400">
      <CheckCircle2 className="size-3" />
      Connected as {username ?? "unknown"}
    </span>
  );
}

function DisconnectedActions({
  onConnect,
  phaseKind,
  errorMessage,
}: {
  onConnect: () => void;
  phaseKind: string;
  errorMessage: string | null;
}) {
  const isStarting = phaseKind === "starting";
  return (
    <div className="mt-3 space-y-2">
      <Button
        type="button"
        onClick={onConnect}
        disabled={isStarting}
        className="w-full sm:w-auto"
      >
        {isStarting ? (
          <>
            <Loader2 className="size-4 animate-spin" />
            Starting…
          </>
        ) : (
          <>
            <GithubIcon className="size-4" />
            Connect GitHub
          </>
        )}
      </Button>
      {errorMessage && (
        <p className="text-xs text-destructive">{errorMessage}</p>
      )}
      <p className="text-xs text-muted-foreground">
        You&apos;ll be shown a one-time code to enter on github.com.
      </p>
    </div>
  );
}

function ConnectedActions({
  username,
  onDisconnect,
}: {
  username: string | null;
  onDisconnect: () => void;
}) {
  return (
    <div className="mt-3 flex items-center justify-between gap-4">
      <div className="text-sm">
        <div className="font-medium">{username ?? "GitHub user"}</div>
        <div className="text-xs text-muted-foreground">
          Sync is active. Click disconnect to remove stored credentials.
        </div>
      </div>
      <Button type="button" variant="outline" onClick={onDisconnect}>
        <LogOut className="size-4" />
        Disconnect
      </Button>
    </div>
  );
}

function DeviceFlowModal({
  open,
  phase,
  onPoll,
  onClose,
}: {
  open: boolean;
  phase: ReturnType<typeof useGitHubSync>["phase"];
  onPoll: () => Promise<GitHubPollOutcome | null>;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const userCode = phase.kind === "waiting" || phase.kind === "polling"
    ? phase.flow.userCode
    : null;
  const verificationUri = phase.kind === "waiting" || phase.kind === "polling"
    ? phase.flow.verificationUri
    : null;

  // Auto-poll loop — start polling on open, respect the server-provided
  // interval. Slows to twice the interval on `slow_down` until reset.
  const intervalRef = useRef<number>(5000);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!open) {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      return;
    }
    intervalRef.current = (phase.kind === "waiting" || phase.kind === "polling" ? (phase.flow.interval || 5) : 5) * 1000;
    let active = true;
    const tick = async () => {
      if (!active) return;
      try {
        const outcome = await onPoll();
        if (outcome && outcome.status === "slow_down") {
          intervalRef.current += 5000;
        }
      } finally {
        if (active) {
          timerRef.current = setTimeout(tick, intervalRef.current);
        }
      }
    };
    timerRef.current = setTimeout(tick, 0);
    return () => {
      active = false;
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
    // onPoll is captured by closure; we deliberately only restart the
    // loop when `open` flips, since pollOnce swaps in the latest state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const handleCopy = async () => {
    if (!userCode) return;
    try {
      await navigator.clipboard.writeText(userCode);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Fall through silently — user can still type it manually.
    }
  };

  const handleOpenBrowser = () => {
    if (verificationUri) void githubOpenVerificationUrl(verificationUri);
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <KeyRound className="size-4" />
            Connect to GitHub
          </DialogTitle>
          <DialogDescription>
            Open the GitHub verification page and enter this one-time code to
            grant access.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="rounded-md border bg-muted/30 p-4 text-center">
            <div className="text-xs uppercase tracking-wide text-muted-foreground">
              One-time code
            </div>
            <div className="mt-2 font-mono text-2xl font-semibold tracking-widest">
              {userCode ?? "—"}
            </div>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="mt-2"
              onClick={() => void handleCopy()}
            >
              <Copy className="size-3" />
              {copied ? "Copied" : "Copy code"}
            </Button>
          </div>

          <Button
            type="button"
            className="w-full"
            onClick={handleOpenBrowser}
            disabled={!verificationUri}
          >
            <ExternalLink className="size-4" />
            Open GitHub verification page
          </Button>

          <div className="flex items-center justify-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            Waiting for authorization…
          </div>
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            <X className="size-4" />
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// Quiet down the unused-import warning while we still keep the icon
// available for future Phase 2 surfaces.
void GitBranch;