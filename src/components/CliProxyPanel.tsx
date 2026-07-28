"use client";

/**
 * Setup panel for the managed CLIProxyAPI instance, shown when the
 * `cliproxyapi` preset is selected in the provider form.
 *
 * CLIProxyAPI turns *subscriptions* (Claude, ChatGPT, Gemini, Grok, Kimi) into
 * an Anthropic-compatible endpoint, so Claude Code can run on any of them. The
 * app owns the binary, the config, and the process, which reduces setup to:
 * Install → Connect a subscription → pick models.
 *
 * ponytail: status is refreshed on mount and after each action, not polled —
 * nothing changes it except the buttons in this panel. Add polling only if the
 * proxy gains a way to die that the user cares about seeing live.
 */

import { useCallback, useEffect, useState } from "react";
import { Check, Download, Loader2, Play, Square } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  installProxy,
  proxyLogin,
  proxyLoginFlows,
  proxyStatus,
  startProxy,
  stopProxy,
} from "@/lib/api";
import type { ProxyLoginFlow, ProxyStatus } from "@/lib/types";

export function CliProxyPanel() {
  const [status, setStatus] = useState<ProxyStatus | null>(null);
  const [flows, setFlows] = useState<ProxyLoginFlow[]>([]);
  /** Id of the action in flight — also used to label the spinner. */
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await proxyStatus());
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, []);

  // Initial load. Both calls are independent, so they run together; on
  // failure the empty defaults are already the right thing to render.
  useEffect(() => {
    let alive = true;
    void (async () => {
      const [s, fl] = await Promise.allSettled([
        proxyStatus(),
        proxyLoginFlows(),
      ]);
      if (!alive) return;
      if (s.status === "fulfilled") setStatus(s.value);
      if (fl.status === "fulfilled") setFlows(fl.value);
    })();
    return () => {
      alive = false;
    };
  }, []);

  /** Run one action with a shared busy lock, then resync status. */
  async function run(id: string, fn: () => Promise<unknown>, ok?: string) {
    setBusy(id);
    try {
      await fn();
      if (ok) toast.success(ok);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(null);
      await refresh();
    }
  }

  if (!status) {
    return (
      <div className="rounded-lg border border-border/60 bg-muted/10 p-3 text-xs text-muted-foreground">
        Checking CLIProxyAPI…
      </div>
    );
  }

  const anyBusy = busy !== null;

  return (
    <div className="space-y-3 rounded-lg border border-border/60 bg-muted/10 p-3">
      {/* Header: what it is + live state */}
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h4 className="text-xs font-semibold">CLIProxyAPI</h4>
          <p className="mt-0.5 text-[10px] text-muted-foreground">
            Run Claude Code on a Gemini, Codex, Grok, or Kimi subscription.
          </p>
        </div>
        <span
          className={
            status.running
              ? "shrink-0 rounded-full border border-emerald-500/40 bg-emerald-500/10 px-2 py-0.5 text-[9px] font-medium uppercase tracking-wider text-emerald-600 dark:text-emerald-400"
              : "shrink-0 rounded-full border border-border/60 bg-muted/30 px-2 py-0.5 text-[9px] font-medium uppercase tracking-wider text-muted-foreground"
          }
        >
          {status.running ? `running :${status.port}` : "stopped"}
        </span>
      </div>

      {/* Lifecycle controls */}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={anyBusy}
          onClick={() =>
            run("install", installProxy, "CLIProxyAPI installed")
          }
        >
          {busy === "install" ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Download className="size-3.5" />
          )}
          {status.installed ? "Update" : "Install"}
        </Button>

        {status.installed &&
          (status.running ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              // A proxy we didn't start isn't ours to kill.
              disabled={anyBusy || !status.managed}
              title={
                status.managed
                  ? undefined
                  : "This instance was started outside Claude Config"
              }
              onClick={() => run("stop", stopProxy)}
            >
              {busy === "stop" ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Square className="size-3.5" />
              )}
              Stop
            </Button>
          ) : (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              disabled={anyBusy}
              onClick={() => run("start", startProxy)}
            >
              {busy === "start" ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Play className="size-3.5" />
              )}
              Start
            </Button>
          ))}

        {status.version && (
          <span className="text-[10px] text-muted-foreground">
            {status.version}
          </span>
        )}
      </div>

      {/* Subscriptions. Logins need the binary but not a running server. */}
      {status.installed && (
        <div className="space-y-1.5 border-t border-border/60 pt-2.5">
          <p className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            Subscriptions
          </p>
          <div className="grid grid-cols-1 gap-1 sm:grid-cols-2">
            {flows.map((flow) => {
              const connected = status.connected.includes(flow.id);
              return (
                <div
                  key={flow.id}
                  className="flex items-center justify-between gap-2 rounded-md border border-border/50 bg-background/40 px-2 py-1"
                >
                  <span className="truncate text-xs">{flow.label}</span>
                  {connected ? (
                    <span className="inline-flex shrink-0 items-center gap-1 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                      <Check className="size-3" />
                      connected
                    </span>
                  ) : (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-6 shrink-0 px-2 text-[10px]"
                      disabled={anyBusy}
                      onClick={() =>
                        run(
                          flow.id,
                          () => proxyLogin(flow.id),
                          `${flow.label} connected`,
                        )
                      }
                    >
                      {busy === flow.id ? (
                        <Loader2 className="size-3 animate-spin" />
                      ) : (
                        "Log in"
                      )}
                    </Button>
                  )}
                </div>
              );
            })}
          </div>
          <p className="text-[10px] text-muted-foreground">
            Opens your browser. Nothing is stored by Claude Config — credentials
            stay with the proxy.
          </p>
        </div>
      )}

      {/* Next step, once there's something to talk to. */}
      {status.running && status.connected.length > 0 && (
        <p className="border-t border-border/60 pt-2.5 text-[10px] text-muted-foreground">
          Now open <span className="font-medium text-foreground">Models</span>{" "}
          below and hit <span className="font-medium text-foreground">Discover models</span>{" "}
          to pick which one Claude Code should use.
        </p>
      )}
    </div>
  );
}
