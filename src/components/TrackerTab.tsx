/* eslint-disable react-hooks/set-state-in-effect --
 * Two intentional setState-in-effect sites:
 *   1. Locking the source picker to the loaded config's source.
 *   2. Pre-filling the form values when the source's field set or the
 *      saved view changes.
 * Both are sync-from-external-state and follow the same pattern used in
 * `useMcpServers.ts` and `useSkills.ts`.
 */
"use client";

import { useEffect, useMemo, useState } from "react";
import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  Eye,
  EyeOff,
  Loader2,
  Pause,
  Play,
  RefreshCw,
  Save,
  Sparkles,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useTracker } from "@/hooks/useTracker";
import { listTrackerSources } from "@/lib/api";
import type {
  TrackerConfigView,
  TrackerField,
  TrackerSourceDescriptor,
  TrackerUsage,
  TrackerUsageWindow,
} from "@/lib/types";
import { cn } from "@/lib/utils";

const AUTO_REFRESH_MS = 60_000;

/**
 * Custom Anthropic-compatible relays we know exist but haven't built
 * a tracker for yet. Substring-matched against (name + baseUrl) in a
 * case-insensitive way. When a provider matches one of these, the
 * Tracker tab shows a "coming soon" panel instead of offering
 * `anthropic_compat` (which would silently fail because these relays
 * have their own auth/admin endpoints we haven't wired up).
 *
 * Add new entries here as we add more relay types — and remove them
 * once a dedicated source ships.
 */
const COMING_SOON_RELAYS: ReadonlyArray<string> = [
  "aerolink",
  "zenmux",
];

function isComingSoonRelay(
  name: string,
  baseUrl: string | null | undefined,
): boolean {
  if (!name && !baseUrl) return false;
  const hay = `${name} ${baseUrl ?? ""}`.toLowerCase();
  return COMING_SOON_RELAYS.some((needle) => hay.includes(needle));
}

/**
 * Best-effort source picker. Pure heuristic — given a provider's name,
 * kind, and (optionally) base URL, suggest the most likely tracker
 * source so the user lands on a sensible default. The order matters:
 * the first matching rule wins.
 *
 * Only considers sources that apply to the provider's kind — the
 * caller has already filtered the catalog, but we re-check defensively
 * so the helper stays correct if reused with the unfiltered list.
 *
 * The lookup uses substring matches on lowercased inputs. The rules
 * are intentionally conservative — they only fire on clear signals
 * (e.g. "minimax" in the URL or name) so we never silently mis-pick a
 * source for a generic Anthropic-style relay.
 */
function pickSourceForProvider(
  sources: TrackerSourceDescriptor[],
  name: string,
  kind?: string,
  baseUrl?: string | null,
): string | null {
  // Filter to applicable sources — never suggest a source that the
  // picker would hide.
  const applicable = kind
    ? sources.filter((s) => s.applicable_kinds.includes(kind))
    : sources;
  const ids = new Set(applicable.map((s) => s.id));
  const hay = [name, baseUrl ?? ""]
    .join(" ")
    .toLowerCase();

  // Provider-name / URL substrings → source. Listed in the order
  // "most specific first" so a "minimax-anthropic-compat" relay
  // (hypothetical) still wins on the minimax rule.
  if (hay.includes("minimax") && ids.has("minimax")) return "minimax";
  if (hay.includes("freemodel") && ids.has("freemodel")) return "freemodel";
  if (hay.includes("kiro") && ids.has("anthropic_compat")) return "anthropic_compat";

  // Kind-based defaults.
  if (kind === "console" && ids.has("anthropic_admin")) return "anthropic_admin";
  if (kind === "subscription" && ids.has("claude_cli")) return "claude_cli";
  if (kind === "subscription" && ids.has("subscription")) return "subscription";
  if (kind === "custom" && ids.has("anthropic_compat")) return "anthropic_compat";

  // Manual JSON is the universal fallback — use it when nothing else
  // matched but it's available for this kind.
  if (ids.has("manual_json")) return "manual_json";

  return null;
}

interface Props {
  providerId: string;
  providerName: string;
  /**
   * Optional hints for auto-picking a source when no config has been
   * saved yet. `kind` is the provider kind (e.g. "custom"); `baseUrl`
   * is the custom-relay base URL when present. The auto-pick is a
   * best-effort UX nudge — the user can always pick something else.
   */
  providerKind?: string;
  providerBaseUrl?: string | null;
}

export function TrackerTab({
  providerId,
  providerName,
  providerKind,
  providerBaseUrl,
}: Props) {
  const [sources, setSources] = useState<TrackerSourceDescriptor[] | null>(
    null,
  );
  const [selectedSourceId, setSelectedSourceId] = useState<string | null>(
    null,
  );

  // Fetch the source catalog once. Cheap (in-memory), and the result
  // is shared across providers in the same session.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await listTrackerSources();
        if (!cancelled) {
          setSources(s);
          // Default selection = a hint-matched source. Once a saved
          // config loads, the hook's `config.source` will override.
          if (s.length > 0) {
            const hint = pickSourceForProvider(
              s,
              providerName,
              providerKind,
              providerBaseUrl,
            );
            setSelectedSourceId((prev) => prev ?? hint ?? s[0].id);
          }
        }
      } catch (e) {
        // Toast handled by the hook on the next user action; the
        // picker just stays empty.
        console.error("listTrackerSources failed", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [providerName, providerKind, providerBaseUrl]);

  const tracker = useTracker(providerId);

  // When the saved config loads, lock the picker to its source so the
  // form below renders the right field schema. Intentional sync from
  // an external snapshot — see the file-header disable.
  useEffect(() => {
    if (tracker.config?.source) {
      setSelectedSourceId(tracker.config.source);
    }
  }, [tracker.config?.source]);

  // Filter the source catalog to sources that apply to the current
  // provider's kind. When nothing matches (e.g. Bedrock, Vertex) the
  // tab renders a "coming soon" panel instead of the picker.
  const applicableSources = useMemo(() => {
    if (!sources) return null;
    if (!providerKind) return sources;
    return sources.filter((s) => s.applicable_kinds.includes(providerKind));
  }, [sources, providerKind]);

  // "Coming soon" for known third-party relays we don't have a tracker
  // for yet. Computed eagerly (no hooks) since it only depends on the
  // props — this lets the early-return below stay above the hooks
  // block.
  const comingSoonRelay = isComingSoonRelay(
    providerName,
    providerBaseUrl,
  );

  const selectedSource = useMemo(
    () => sources?.find((s) => s.id === selectedSourceId) ?? null,
    [sources, selectedSourceId],
  );

  // Start auto-refresh on mount once we have a config. Stop on unmount.
  // We delay the first auto-poll to the first manual refresh, so the
  // user isn't bombarded with a network call the moment they open the
  // tab.
  useEffect(() => {
    if (tracker.config && !tracker.autoRefresh) {
      tracker.startAutoRefresh(AUTO_REFRESH_MS);
    }
    return () => {
      tracker.stopAutoRefresh();
    };
    // We intentionally don't include `tracker.autoRefresh` so we don't
    // re-arm the interval on every state change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tracker.config?.source, tracker.config?.updated_at]);

  // "Coming soon" state — the provider kind has no implemented tracker
  // (e.g. Bedrock, Vertex), OR the relay is on our "not yet supported"
  // list (AeroLink, Zenmux, etc.). We still show the header so the
  // user knows the tab exists, but skip the picker + form + usage panel.
  if (sources !== null && (comingSoonRelay || (applicableSources && applicableSources.length === 0))) {
    return (
      <ComingSoonPanel
        providerName={providerName}
        kind={providerKind}
        reason={comingSoonRelay ? "relay-not-implemented" : "kind-not-implemented"}
      />
    );
  }

  return (
    <div className="space-y-5">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2.5">
          <Sparkles className="mt-0.5 size-4 text-primary" />
          <div>
            <p className="text-sm font-semibold leading-none">
              Track model usage
            </p>
            <p className="mt-1 text-[11px] text-muted-foreground">
              Pick a source, paste the info it asks for, and we&apos;ll
              refresh every minute.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <RefreshControls tracker={tracker} hasConfig={!!tracker.config} />
        </div>
      </div>

      <SourcePicker
        sources={applicableSources}
        value={selectedSourceId}
        onChange={setSelectedSourceId}
      />

      {selectedSource && (
        <ConfigForm
          source={selectedSource}
          view={tracker.config}
          saving={tracker.saving}
          onSave={(fields) =>
            tracker.save(selectedSource.id, fields).then(() => undefined)
          }
          onRemove={() => tracker.remove().then(() => undefined)}
        />
      )}

      <UsagePanel
        usage={tracker.config?.last_usage ?? null}
        refreshing={tracker.refreshing}
        lastFetchedAt={tracker.config?.last_fetched_at ?? null}
        lastError={tracker.lastError}
        providerName={providerName}
      />
    </div>
  );
}

function ComingSoonPanel({
  providerName,
  kind,
  reason,
}: {
  providerName: string;
  kind?: string;
  reason: "relay-not-implemented" | "kind-not-implemented";
}) {
  const message =
    reason === "relay-not-implemented"
      ? `Usage tracking for ${providerName || "this relay"} isn't wired up yet.`
      : `Usage tracking for ${kindLabel(kind)} providers isn&apos;t wired up yet.`;
  return (
    <div className="rounded-xl border border-dashed bg-card/30 p-6 text-center">
      <Sparkles className="mx-auto size-5 text-muted-foreground/60" />
      <p className="mt-3 text-sm font-medium">Coming soon</p>
      <p className="mt-1 text-[11px] text-muted-foreground">{message}</p>
      <p className="mt-3 text-[10px] text-muted-foreground/80">
        Anthropic Console, claude.ai subscription, MiniMax, FreeModel, and
        Anthropic-compatible relays are supported today. Bedrock, Vertex,
        AeroLink, Zenmux, and OpenAI-compatible relays are next.
      </p>
    </div>
  );
}

function kindLabel(kind: string | undefined): string {
  switch (kind) {
    case "bedrock":
      return "AWS Bedrock";
    case "vertex":
      return "Google Vertex";
    case "console":
      return "Anthropic Console";
    case "subscription":
      return "Claude subscription";
    case "custom":
      return "custom relay";
    default:
      return kind ?? "this";
  }
}

// ---------------------------------------------------------------------------
// Refresh controls
// ---------------------------------------------------------------------------

function RefreshControls({
  tracker,
  hasConfig,
}: {
  tracker: TrackerState;
  hasConfig: boolean;
}) {
  if (!hasConfig) return null;
  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="outline"
        onClick={() => void tracker.refresh()}
        disabled={tracker.refreshing}
        aria-label="Refresh now"
      >
        {tracker.refreshing ? (
          <Loader2 className="size-3.5 animate-spin" />
        ) : (
          <RefreshCw className="size-3.5" />
        )}
        Refresh
      </Button>
      <Button
        type="button"
        size="sm"
        variant={tracker.autoRefresh ? "secondary" : "ghost"}
        onClick={() =>
          tracker.autoRefresh
            ? tracker.stopAutoRefresh()
            : tracker.startAutoRefresh(AUTO_REFRESH_MS)
        }
        aria-label={tracker.autoRefresh ? "Pause auto-refresh" : "Resume auto-refresh"}
        title={
          tracker.autoRefresh
            ? "Auto-refresh every minute"
            : "Auto-refresh paused"
        }
      >
        {tracker.autoRefresh ? (
          <Pause className="size-3.5" />
        ) : (
          <Play className="size-3.5" />
        )}
        {tracker.autoRefresh ? "Live" : "Paused"}
      </Button>
    </>
  );
}

// Subset of `TrackerState` we need here — avoids re-declaring the full type.
type TrackerState = ReturnType<typeof useTracker>;

// ---------------------------------------------------------------------------
// Source picker
// ---------------------------------------------------------------------------

function SourcePicker({
  sources,
  value,
  onChange,
}: {
  sources: TrackerSourceDescriptor[] | null;
  value: string | null;
  onChange: (id: string) => void;
}) {
  if (sources === null) {
    return (
      <div className="flex items-center gap-2 rounded-md border bg-muted/20 p-3 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        Loading sources…
      </div>
    );
  }
  if (sources.length === 0) {
    return (
      <div className="rounded-md border border-dashed bg-muted/10 p-3 text-xs text-muted-foreground">
        No tracker sources registered.
      </div>
    );
  }
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">Source</Label>
      <Select
        value={value ?? ""}
        onValueChange={(v) => v && onChange(v)}
      >
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Choose a source…" />
        </SelectTrigger>
        <SelectContent>
          {sources.map((s) => (
            <SelectItem key={s.id} value={s.id}>
              {s.display_name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {value && (() => {
        const s = sources.find((x) => x.id === value);
        return s ? (
          <p className="text-[10px] text-muted-foreground">{s.description}</p>
        ) : null;
      })()}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Config form — one input per field declared by the source
// ---------------------------------------------------------------------------

function ConfigForm({
  source,
  view,
  saving,
  onSave,
  onRemove,
}: {
  source: TrackerSourceDescriptor;
  view: TrackerConfigView | null | undefined;
  saving: boolean;
  onSave: (fields: Record<string, unknown>) => Promise<void>;
  onRemove: () => Promise<void>;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [showSecrets, setShowSecrets] = useState<Record<string, boolean>>({});

  // Pre-fill from the saved view. Secrets intentionally come back empty
  // (the backend strips them) so the placeholder reads "Stored". When
  // the user types a new value, it overrides the stored one. Intentional
  // sync — useState initializer can't react to dep changes.
  useEffect(() => {
    const next: Record<string, string> = {};
    for (const f of source.fields) {
      const v = view?.fields[f.key];
      next[f.key] = typeof v === "string" ? v : "";
    }
    setValues(next);
  }, [source, view]);

  const missing = useMemo(
    () =>
      source.fields.filter(
        (f) => f.required && !values[f.key]?.trim() && !view?.has_secret.includes(f.key),
      ),
    [source.fields, values, view],
  );
  const canSave = missing.length === 0 && !saving;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSave) return;
    // Coerce non-string values to strings — the backend expects a string
    // for every field key. (For non-text sources this would need a
    // richer schema, but every v1 source has string fields.)
    const payload: Record<string, unknown> = {};
    for (const f of source.fields) {
      payload[f.key] = values[f.key] ?? "";
    }
    await onSave(payload);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      {source.fields.map((f) => (
        <FieldInput
          key={f.key}
          field={f}
          value={values[f.key] ?? ""}
          onChange={(v) => setValues((prev) => ({ ...prev, [f.key]: v }))}
          showSecret={!!showSecrets[f.key]}
          onToggleSecret={() =>
            setShowSecrets((prev) => ({ ...prev, [f.key]: !prev[f.key] }))
          }
          stored={view?.has_secret.includes(f.key) ?? false}
        />
      ))}
      <div className="flex items-center justify-end gap-2 pt-1">
        {view && (
          <Button
            type="button"
            variant="ghost"
            onClick={() => void onRemove()}
            disabled={saving}
            aria-label="Remove tracker"
          >
            <Trash2 className="size-3.5" />
            Remove
          </Button>
        )}
        <Button type="submit" disabled={!canSave}>
          {saving ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Save className="size-3.5" />
          )}
          {view ? "Save changes" : "Save & start tracking"}
        </Button>
      </div>
      {missing.length > 0 && (
        <p className="text-[10px] text-destructive">
          Fill required fields: {missing.map((m) => m.label).join(", ")}
        </p>
      )}
    </form>
  );
}

function FieldInput({
  field,
  value,
  onChange,
  showSecret,
  onToggleSecret,
  stored,
}: {
  field: TrackerField;
  value: string;
  onChange: (v: string) => void;
  showSecret: boolean;
  onToggleSecret: () => void;
  stored: boolean;
}) {
  const placeholder = stored
    ? `••••• stored — type to replace`
    : field.placeholder;
  return (
    <div className="space-y-1.5">
      <Label htmlFor={`tracker-${field.key}`} className="text-xs">
        {field.label}
        {field.required && <span className="text-destructive"> *</span>}
      </Label>
      <div className="relative">
        {field.multiline ? (
          <textarea
            id={`tracker-${field.key}`}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={placeholder}
            rows={6}
            className={cn(
              "w-full rounded-md border bg-background px-3 py-2 font-mono text-xs",
              "focus:outline-none focus:ring-2 focus:ring-ring",
              "placeholder:text-muted-foreground/60",
            )}
          />
        ) : (
          <Input
            id={`tracker-${field.key}`}
            type={field.secret && !showSecret ? "password" : "text"}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={placeholder}
            className={cn("font-mono text-xs", field.secret && "pr-10")}
            autoComplete="off"
            spellCheck={false}
          />
        )}
        {field.secret && !field.multiline && (
          <button
            type="button"
            onClick={onToggleSecret}
            className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:text-foreground"
            aria-label={showSecret ? "Hide" : "Show"}
          >
            {showSecret ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
          </button>
        )}
      </div>
      {field.hint && (
        <p className="text-[10px] text-muted-foreground">{field.hint}</p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Usage panel — renders whatever the last successful snapshot contained
// ---------------------------------------------------------------------------

function UsagePanel({
  usage,
  refreshing,
  lastFetchedAt,
  lastError,
  providerName,
}: {
  usage: TrackerUsage | null;
  refreshing: boolean;
  lastFetchedAt: string | null;
  lastError: string | null;
  providerName: string;
}) {
  if (!usage) {
    return (
      <div className="rounded-lg border border-dashed bg-card/30 p-5 text-center">
        {lastError ? (
          <div className="space-y-1">
            <CircleAlert className="mx-auto size-4 text-destructive" />
            <p className="text-xs font-medium text-destructive">
              Couldn&apos;t fetch usage
            </p>
            <p className="text-[11px] text-muted-foreground break-words">
              {lastError}
            </p>
          </div>
        ) : (
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground">
              No usage data yet for {providerName}.
            </p>
            <p className="text-[11px] text-muted-foreground">
              Click <span className="font-medium text-foreground">Refresh</span>{" "}
              to fetch the first snapshot.
            </p>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-3 rounded-lg border bg-card/40 p-4">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <CheckCircle2 className="size-3.5 text-emerald-500" />
          <span className="text-xs font-medium">Latest snapshot</span>
          {refreshing && (
            <Loader2 className="size-3 animate-spin text-muted-foreground" />
          )}
        </div>
        {lastFetchedAt && (
          <span className="text-[10px] text-muted-foreground tabular-nums">
            {formatRelative(lastFetchedAt)}
          </span>
        )}
      </div>

      {usage.windows.length > 0 && (
        <div className="space-y-2">
          {usage.windows.map((w, i) => (
            <WindowRow key={`${w.label}-${i}`} window={w} />
          ))}
        </div>
      )}

      {usage.cost_usd !== null && (
        <div className="flex items-center justify-between rounded-md bg-muted/30 px-3 py-2 text-xs">
          <span className="text-muted-foreground">Total cost</span>
          <span className="font-mono font-medium tabular-nums">
            ${usage.cost_usd.toFixed(2)}
          </span>
        </div>
      )}

      {usage.models.length > 0 && (
        <ModelBreakdown models={usage.models} />
      )}

      {usage.note && (
        <p className="text-[10px] italic text-muted-foreground/80">
          {usage.note}
        </p>
      )}
    </div>
  );
}

function WindowRow({ window: w }: { window: TrackerUsageWindow }) {
  const pct = w.used_percent ?? computePercent(w.used, w.limit);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-[11px]">
        <span className="font-medium text-foreground/90">{w.label}</span>
        <span className="font-mono tabular-nums text-muted-foreground">
          {formatUsed(w)} {w.unit ?? ""}
          {pct !== null && (
            <span className="ml-1.5 text-foreground/70">
              ({pct.toFixed(0)}%)
            </span>
          )}
        </span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full transition-all duration-500",
            pct === null
              ? "bg-muted-foreground/30"
              : pct >= 90
                ? "bg-red-500/80"
                : pct >= 70
                  ? "bg-amber-500/80"
                  : "bg-emerald-500/80",
          )}
          style={{ width: pct === null ? "100%" : `${Math.min(100, pct)}%` }}
        />
      </div>
      {w.resets_at && (
        <p className="text-[10px] text-muted-foreground/80">
          Resets {formatRelative(w.resets_at)}
        </p>
      )}
    </div>
  );
}

function ModelBreakdown({
  models,
}: {
  models: TrackerUsage["models"];
}) {
  const [open, setOpen] = useState(true);
  return (
    <div className="rounded-md border bg-background/40">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between px-3 py-2 text-left text-[11px] font-medium text-foreground/90 hover:bg-muted/40"
        aria-expanded={open}
      >
        <span className="inline-flex items-center gap-1.5">
          {open ? (
            <ChevronDown className="size-3" />
          ) : (
            <ChevronRight className="size-3" />
          )}
          Models ({models.length})
        </span>
      </button>
      {open && (
        <ul className="divide-y divide-border/40">
          {models.map((m) => (
            <li
              key={m.model}
              className="flex items-center justify-between px-3 py-1.5 text-[11px] font-mono"
            >
              <span className="truncate">{m.model}</span>
              <span className="flex items-center gap-3 tabular-nums text-muted-foreground">
                {m.input_tokens !== null && (
                  <span title="input tokens">
                    in {m.input_tokens.toLocaleString()}
                  </span>
                )}
                {m.output_tokens !== null && (
                  <span title="output tokens">
                    out {m.output_tokens.toLocaleString()}
                  </span>
                )}
                {m.cost_usd !== null && (
                  <span className="text-foreground/80">
                    ${m.cost_usd.toFixed(2)}
                  </span>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

function computePercent(used: number | null, limit: number | null): number | null {
  if (used === null || limit === null || limit === 0) return null;
  return (used / limit) * 100;
}

function formatUsed(w: TrackerUsageWindow): string {
  if (w.used !== null && w.limit !== null) {
    return `${formatNumber(w.used)} / ${formatNumber(w.limit)}`;
  }
  if (w.used !== null) return formatNumber(w.used);
  if (w.used_percent !== null) return `${w.used_percent.toFixed(0)}%`;
  return "—";
}

function formatNumber(n: number): string {
  if (Number.isInteger(n)) return n.toLocaleString();
  return n.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

function formatRelative(iso: string): string {
  const ts = Date.parse(iso);
  if (Number.isNaN(ts)) return iso;
  const diffMs = Date.now() - ts;
  const abs = Math.abs(diffMs);
  const future = diffMs < 0;
  const sec = Math.round(abs / 1000);
  const min = Math.round(sec / 60);
  const hr = Math.round(min / 60);
  const day = Math.round(hr / 24);
  let body: string;
  if (sec < 60) body = `${sec}s`;
  else if (min < 60) body = `${min}m`;
  else if (hr < 24) body = `${hr}h`;
  else body = `${day}d`;
  return future ? `in ${body}` : `${body} ago`;
}
