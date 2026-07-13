"use client";

import { useState } from "react";
import {
  AlarmClockCheck,
  AlarmClockOff,
  ArrowLeft,
  CalendarClock,
  CircleAlert,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  Zap,
} from "lucide-react";

import type { GlobalTabProps, SidebarTabButtonProps } from "@/data/globalTabs";
import { useSchedules } from "@/hooks/useSchedules";
import type {
  Schedule,
  ScheduleInput,
  ScheduleStatus,
  SchedulingAvailability,
  Weekday,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

const WEEKDAYS: { key: Weekday; short: string }[] = [
  { key: "mon", short: "Mon" },
  { key: "tue", short: "Tue" },
  { key: "wed", short: "Wed" },
  { key: "thu", short: "Thu" },
  { key: "fri", short: "Fri" },
  { key: "sat", short: "Sat" },
  { key: "sun", short: "Sun" },
];

const CONSENT_KEY = "schedules-crontab-consent";

export function SchedulesSidebarButton({
  active,
  onSelect,
}: SidebarTabButtonProps) {
  return (
    <button
      onClick={onSelect}
      className={cn(
        "w-full flex items-center gap-2 px-3 py-2 rounded-lg border text-left text-xs font-medium transition-all cursor-pointer group",
        active
          ? "bg-primary/10 border-primary/20 text-primary shadow-2xs"
          : "bg-card/50 border-border/60 text-muted-foreground hover:bg-card hover:border-foreground/20 hover:text-foreground",
      )}
    >
      <CalendarClock
        className={cn(
          "size-3.5 shrink-0",
          active
            ? "text-primary"
            : "text-muted-foreground group-hover:text-foreground",
        )}
      />
      <span className="flex-1 truncate">Schedules</span>
    </button>
  );
}

export function SchedulesView({ onClose }: GlobalTabProps) {
  const s = useSchedules();
  const initialLoad = s.schedules === null && s.loading;

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<Schedule | null>(null);
  const [deleting, setDeleting] = useState<Schedule | null>(null);
  // Holds an action awaiting first-run crontab consent.
  const [pendingConsent, setPendingConsent] = useState<null | (() => void)>(
    null,
  );

  const hasConsent = () =>
    typeof window !== "undefined" &&
    window.localStorage.getItem(CONSENT_KEY) === "1";

  /** Gate the first OS-touching enable behind a one-time confirm. */
  const withConsent = (action: () => void) => {
    if (hasConsent()) {
      action();
    } else {
      setPendingConsent(() => action);
    }
  };

  const openAdd = () => {
    setEditing(null);
    setFormOpen(true);
  };
  const openEdit = (schedule: Schedule) => {
    setEditing(schedule);
    setFormOpen(true);
  };

  const submitForm = async (input: ScheduleInput) => {
    const run = async () => {
      if (editing) {
        await s.update({ ...input, id: editing.id });
      } else {
        await s.create(input);
      }
      setFormOpen(false);
      setEditing(null);
    };
    // Only the OS scheduler is touched when the schedule is enabled.
    if (input.enabled) {
      withConsent(() => void run());
    } else {
      await run();
    }
  };

  const confirmDelete = async () => {
    if (!deleting) return;
    await s.remove(deleting.id);
    setDeleting(null);
  };

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <Button size="sm" variant="ghost" onClick={onClose}>
            <ArrowLeft className="size-3.5" />
          </Button>
          <CalendarClock className="size-4 text-primary" />
          <div>
            <h2 className="text-sm font-semibold leading-none">Schedules</h2>
            <p className="mt-1 text-[11px] text-muted-foreground">
              Fire a tiny primer at a chosen time to reset your Claude
              subscription&apos;s 5-hour window — even when this app is closed.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={() => void s.refresh()}
            disabled={s.loading}
            aria-label="Refresh schedules"
          >
            {s.loading ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <RefreshCw className="size-3.5" />
            )}
            Refresh
          </Button>
          <Button size="sm" onClick={openAdd}>
            <Plus className="size-3.5" />
            Add
          </Button>
        </div>
      </div>

      <AvailabilityWarnings availability={s.availability} />

      <ScheduleList
        schedules={s.schedules}
        statusById={s.statusById}
        initialLoad={initialLoad}
        busy={s.busy}
        onToggle={(id, enabled) => {
          if (enabled) {
            withConsent(() => void s.toggle(id, true));
          } else {
            void s.toggle(id, false);
          }
        }}
        onPrimeNow={(id) => void s.primeNow(id)}
        onEdit={openEdit}
        onDelete={setDeleting}
      />

      <ScheduleFormDialog
        open={formOpen}
        editing={editing}
        busy={s.busy}
        onOpenChange={(open) => {
          setFormOpen(open);
          if (!open) setEditing(null);
        }}
        onSubmit={submitForm}
      />

      <DeleteScheduleDialog
        schedule={deleting}
        busy={s.busy}
        onOpenChange={(open) => {
          if (!open) setDeleting(null);
        }}
        onConfirm={confirmDelete}
      />

      <ConsentDialog
        open={pendingConsent !== null}
        schedulerKind={s.availability?.schedulerKind ?? "crontab"}
        onOpenChange={(open) => {
          if (!open) setPendingConsent(null);
        }}
        onConfirm={() => {
          window.localStorage.setItem(CONSENT_KEY, "1");
          const action = pendingConsent;
          setPendingConsent(null);
          action?.();
        }}
      />
    </div>
  );
}

function AvailabilityWarnings({
  availability,
}: {
  availability: SchedulingAvailability | null;
}) {
  if (!availability) return null;
  const warnings: string[] = [];
  if (!availability.claudeOnPath) {
    warnings.push(
      "The `claude` CLI isn't on your PATH. Primers can't run until it is installed and on PATH.",
    );
  }
  if (!availability.schedulerAvailable) {
    warnings.push(
      "No OS scheduler was found. Schedules can't be installed on this machine.",
    );
  }
  if (!availability.subscriptionOauthPresent) {
    warnings.push(
      "No Claude subscription login found in ~/.claude/.credentials.json. Run `claude /login` first.",
    );
  }

  if (warnings.length === 0 && !availability.nativeSchedulingPresent) {
    return null;
  }

  return (
    <div className="flex flex-col gap-2">
      {warnings.map((w) => (
        <div
          key={w}
          className="flex items-start gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[11px] text-amber-700 dark:text-amber-300"
        >
          <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
          <span>{w}</span>
        </div>
      ))}
      {availability.nativeSchedulingPresent && (
        <div className="flex items-start gap-2 rounded-lg border border-border/60 bg-card/40 px-3 py-2 text-[11px] text-muted-foreground">
          <CalendarClock className="mt-0.5 size-3.5 shrink-0" />
          <span>
            Native Claude Code scheduling (Routines / Desktop Tasks) is set up
            on this machine. These primers run independently — mind overlap.
          </span>
        </div>
      )}
    </div>
  );
}

function ScheduleList({
  schedules,
  statusById,
  initialLoad,
  busy,
  onToggle,
  onPrimeNow,
  onEdit,
  onDelete,
}: {
  schedules: Schedule[] | null;
  statusById: Record<string, ScheduleStatus>;
  initialLoad: boolean;
  busy: boolean;
  onToggle: (id: string, enabled: boolean) => void;
  onPrimeNow: (id: string) => void;
  onEdit: (s: Schedule) => void;
  onDelete: (s: Schedule) => void;
}) {
  if (initialLoad) {
    return (
      <div className="flex items-center justify-center gap-2 rounded-xl border bg-card/45 p-8 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        Loading schedules…
      </div>
    );
  }
  const rows = schedules ?? [];
  if (rows.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed bg-card/30 p-8 text-center">
        <CalendarClock className="size-5 text-muted-foreground/60" />
        <div>
          <p className="text-xs font-medium">No schedules yet</p>
          <p className="mt-1 text-[11px] text-muted-foreground">
            Add a primer at, say, 07:30 on weekdays so your morning window
            resets at 12:30 instead of whenever you first message.
          </p>
        </div>
      </div>
    );
  }
  return (
    <div className="flex flex-col divide-y divide-border/40 overflow-hidden rounded-xl border bg-card/45">
      {rows.map((s) => (
        <ScheduleRow
          key={s.id}
          schedule={s}
          status={statusById[s.id]}
          busy={busy}
          onToggle={onToggle}
          onPrimeNow={onPrimeNow}
          onEdit={onEdit}
          onDelete={onDelete}
        />
      ))}
    </div>
  );
}

function ScheduleRow({
  schedule,
  status,
  busy,
  onToggle,
  onPrimeNow,
  onEdit,
  onDelete,
}: {
  schedule: Schedule;
  status: ScheduleStatus | undefined;
  busy: boolean;
  onToggle: (id: string, enabled: boolean) => void;
  onPrimeNow: (id: string) => void;
  onEdit: (s: Schedule) => void;
  onDelete: (s: Schedule) => void;
}) {
  const dayset = new Set(schedule.days);
  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <span className="font-mono text-sm font-semibold tabular-nums">
            {schedule.time}
          </span>
          {schedule.label && (
            <span className="truncate text-xs text-muted-foreground">
              {schedule.label}
            </span>
          )}
          <LastRunPill status={status} />
        </div>
        <div className="flex items-center gap-1.5">
          <Switch
            checked={schedule.enabled}
            onCheckedChange={(checked) => onToggle(schedule.id, checked)}
            aria-label={schedule.enabled ? "Disable schedule" : "Enable schedule"}
          />
          <Button
            size="icon-xs"
            variant="ghost"
            disabled={busy}
            onClick={() => onPrimeNow(schedule.id)}
            aria-label="Prime now"
            title="Fire this primer now"
          >
            <Zap className="size-3.5" />
          </Button>
          <Button
            size="icon-xs"
            variant="ghost"
            onClick={() => onEdit(schedule)}
            aria-label="Edit schedule"
          >
            <Pencil className="size-3.5" />
          </Button>
          <Button
            size="icon-xs"
            variant="ghost"
            onClick={() => onDelete(schedule)}
            aria-label="Delete schedule"
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </div>
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-wrap gap-1">
          {WEEKDAYS.map((d) => (
            <span
              key={d.key}
              className={cn(
                "rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider",
                dayset.has(d.key)
                  ? "bg-primary/10 text-primary"
                  : "bg-muted/50 text-muted-foreground/50",
              )}
            >
              {d.short}
            </span>
          ))}
        </div>
        {schedule.enabled && status?.nextFire && (
          <span className="shrink-0 text-[11px] text-muted-foreground">
            next {formatNextFire(status.nextFire)}
          </span>
        )}
      </div>
    </div>
  );
}

function LastRunPill({ status }: { status: ScheduleStatus | undefined }) {
  const run = status?.lastRun;
  if (!run) {
    return (
      <span className="inline-flex items-center gap-1 shrink-0 rounded-full bg-muted/60 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
        never run
      </span>
    );
  }
  if (run.ok) {
    return (
      <span
        title={`Last ran ${formatNextFire(run.startedAt)}`}
        className="inline-flex items-center gap-1 shrink-0 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-emerald-600 dark:text-emerald-400"
      >
        <AlarmClockCheck className="size-3" />
        ok
      </span>
    );
  }
  return (
    <span
      title={run.error ?? "Primer failed"}
      className="inline-flex items-center gap-1 shrink-0 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-amber-600 dark:text-amber-400"
    >
      <AlarmClockOff className="size-3" />
      failed
    </span>
  );
}

function ScheduleFormDialog({
  open,
  editing,
  busy,
  onOpenChange,
  onSubmit,
}: {
  open: boolean;
  editing: Schedule | null;
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: ScheduleInput) => Promise<void>;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        {open && (
          <ScheduleFormBody
            editing={editing}
            busy={busy}
            onSubmit={onSubmit}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function ScheduleFormBody({
  editing,
  busy,
  onSubmit,
}: {
  editing: Schedule | null;
  busy: boolean;
  onSubmit: (input: ScheduleInput) => Promise<void>;
}) {
  const [time, setTime] = useState(editing?.time ?? "07:30");
  const [label, setLabel] = useState(editing?.label ?? "");
  const [days, setDays] = useState<Set<Weekday>>(
    new Set(editing?.days ?? ["mon", "tue", "wed", "thu", "fri"]),
  );
  const [enabled, setEnabled] = useState(editing?.enabled ?? true);

  const timeValid = /^([01]\d|2[0-3]):[0-5]\d$/.test(time);
  const canSave = timeValid && days.size > 0 && !busy;

  const toggleDay = (d: Weekday) => {
    setDays((prev) => {
      const next = new Set(prev);
      if (next.has(d)) next.delete(d);
      else next.add(d);
      return next;
    });
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSave) return;
    // Preserve weekday order (Mon..Sun) regardless of click order.
    const orderedDays = WEEKDAYS.map((w) => w.key).filter((k) => days.has(k));
    await onSubmit({
      id: editing?.id,
      label: label.trim() ? label.trim() : undefined,
      time,
      days: orderedDays,
      enabled,
    });
  };

  return (
    <form onSubmit={handleSubmit}>
      <DialogHeader>
        <DialogTitle>{editing ? "Edit schedule" : "New schedule"}</DialogTitle>
        <DialogDescription>
          A primer fires <code className="font-mono text-[11px]">claude -p
          &quot;hi&quot;</code> on the chosen days at the chosen local time.
        </DialogDescription>
      </DialogHeader>

      <div className="flex flex-col gap-4 py-2">
        <div className="flex items-end gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="schedule-time">Time (24h)</Label>
            <Input
              id="schedule-time"
              type="time"
              value={time}
              onChange={(e) => setTime(e.target.value)}
              className="w-32"
            />
          </div>
          <div className="flex flex-1 flex-col gap-1.5">
            <Label htmlFor="schedule-label">Label (optional)</Label>
            <Input
              id="schedule-label"
              value={label}
              placeholder="Morning"
              onChange={(e) => setLabel(e.target.value)}
            />
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label>Days</Label>
          <div className="flex flex-wrap gap-1.5">
            {WEEKDAYS.map((d) => (
              <button
                key={d.key}
                type="button"
                onClick={() => toggleDay(d.key)}
                className={cn(
                  "rounded-md border px-2.5 py-1 text-xs font-medium transition-colors",
                  days.has(d.key)
                    ? "border-primary/30 bg-primary/10 text-primary"
                    : "border-border/60 bg-card/50 text-muted-foreground hover:text-foreground",
                )}
              >
                {d.short}
              </button>
            ))}
          </div>
          {days.size === 0 && (
            <p className="text-[11px] text-amber-600 dark:text-amber-400">
              Pick at least one day.
            </p>
          )}
        </div>

        <label className="flex items-center gap-2 text-xs">
          <Switch checked={enabled} onCheckedChange={setEnabled} />
          Enabled (installs into your system scheduler)
        </label>
      </div>

      <DialogFooter>
        <DialogClose render={<Button variant="ghost" type="button" disabled={busy} />}>
          Cancel
        </DialogClose>
        <Button type="submit" disabled={!canSave}>
          {busy ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <CalendarClock className="size-3.5" />
          )}
          {editing ? "Save changes" : "Add schedule"}
        </Button>
      </DialogFooter>
    </form>
  );
}

function DeleteScheduleDialog({
  schedule,
  busy,
  onOpenChange,
  onConfirm,
}: {
  schedule: Schedule | null;
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => Promise<void>;
}) {
  return (
    <Dialog open={schedule !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete this schedule?</DialogTitle>
          <DialogDescription>
            The {schedule?.time} primer will be removed from your system
            scheduler. Run history is kept.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose render={<Button variant="ghost" disabled={busy} />}>
            Cancel
          </DialogClose>
          <Button variant="destructive" onClick={onConfirm} disabled={busy}>
            {busy ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Trash2 className="size-3.5" />
            )}
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ConsentDialog({
  open,
  schedulerKind,
  onOpenChange,
  onConfirm,
}: {
  open: boolean;
  schedulerKind: string;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Install into your system scheduler?</DialogTitle>
          <DialogDescription>
            Enabling a schedule adds an entry to your{" "}
            <code className="font-mono text-[11px]">{schedulerKind}</code>{" "}
            so primers fire even when this app is closed. It touches OS-level
            config; other cron/task entries are preserved. You can disable or
            delete a schedule at any time to remove it.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose render={<Button variant="ghost" />}>Cancel</DialogClose>
          <Button onClick={onConfirm}>
            <CalendarClock className="size-3.5" />
            Install
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatNextFire(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return rfc3339;
  return d.toLocaleString(undefined, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
