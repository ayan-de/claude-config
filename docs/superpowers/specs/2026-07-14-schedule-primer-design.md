# Design: Scheduled Window Primers

**Date:** 2026-07-14
**Status:** Approved (design), pending implementation plan
**Author:** brainstorming session

## Problem

Claude Code's subscription (OAuth) rate limit resets on a rolling **5-hour
window** that starts from your *first* message. Heavy morning users hit the
limit mid-afternoon because their window started whenever they happened to
first message. Users want to **control when the window resets** by priming it
at a chosen time — e.g. kick off a trivial message at 7:30am so a heavy
morning resets at 12:30pm instead of 2:30pm, and repeat for evening sessions.

This app already tracks that exact window (`src-tauri/src/tracker/claude_cli.rs`
reads `~/.claude/.credentials.json` and calls `GET /api/oauth/usage`), so a
feature that *primes* the window belongs alongside the tracker.

## Goal

Let users define recurring **primers** — a local time + weekday set — that fire
a minimal `claude` invocation (Haiku, `"hi"`) to start a fresh subscription
window, **even when the app is closed**, and show clear last-run / next-run
feedback in the UI.

## Non-goals

- No support for priming *metered* (non-subscription) providers in v1.
- No waking a sleeping/offline machine — the OS scheduler only fires when the
  machine is on and the user is logged in.
- No token-refresh implementation — we reuse the CLI's own OAuth, same as the
  existing tracker.

## Decisions (from brainstorming)

| Fork | Decision |
|---|---|
| Firing mechanism | **OS scheduler** — real crontab (Linux/macOS) / Scheduled Tasks (Windows), so primers fire with the app closed. |
| Primer command | **`claude` CLI print mode** — `claude -p "hi" --model <haiku>`. Reuses real Claude Code auth; near-zero code. |
| Target window | **Always the Anthropic subscription** — forced via an isolated `CLAUDE_CONFIG_DIR` so provider `env` overrides don't apply. |
| Recurrence | **Multiple daily schedules with weekday selection** (e.g. 07:30 Mon–Fri, 16:30 daily). No raw cron syntax in the UI. |
| Run feedback | **Last-run status + next-run time** via a small run-log, plus the existing tracker showing the reset window. |
| Primer invocation | **Approach A — generated wrapper script** (`primer.sh` / `primer.cmd`) rather than a compiled sidecar or raw cron line. |

## Grounding facts (verified in codebase)

- Subscription OAuth lives in `~/.claude/.credentials.json` as the
  `claudeAiOauth` blob; the 5-hour window is tied to that access token.
- `src-tauri/src/merge.rs` writes an **empty `env` block** for a Subscription
  provider, so `claude` falls back to `.credentials.json` OAuth. The primer
  must recreate that state even when a *different* provider is loaded.
- Storage helpers already honor `CLAUDE_CONFIG_DIR`
  (`storage::discover_claude_dir`, `read_credentials_oauth`).
- Command naming convention is `<verb>_<noun>_cmd`; all frontend IPC goes
  through `src/lib/api.ts` (never `invoke()` in components).

## Architecture & data flow

```
schedules.json ──► sync_schedules_cmd ──► managed crontab block
                                     └──► primer.sh / primer.cmd (regenerated)
                                     └──► primer-config/ (empty env + OAuth)

        [fire time]
crontab ──► primer.sh <schedule-id>
              └─ CLAUDE_CONFIG_DIR=<app>/primer-config claude -p "hi" --model haiku
              └─ append run-record to runs.jsonl

UI ◄── get_schedule_status_cmd (schedules.json + runs.jsonl) ── "last 07:30 ✅ · next 07:30"
UI ◄── tracker (unchanged) ── shows the freshly-reset window
```

`sync_schedules_cmd` is **idempotent**: it fully regenerates the managed
crontab block (delimited by marker comments), the wrapper scripts, and the
primer config dir from `schedules.json`. It runs after every mutation and at
app launch (to recover from manual edits / partial state).

## Backend (Rust)

New module `src-tauri/src/schedule/`:

- **`store.rs`** — `schedules.json` under app-data, same pattern as
  `providers.json`.
  ```rust
  struct Schedule {
      id: String,           // uuid
      label: Option<String>,
      time: String,         // "HH:MM" local 24h
      days: Vec<Weekday>,   // Mon..Sun; empty is invalid
      enabled: bool,
      created_at: String,   // rfc3339
      updated_at: String,
  }
  ```
- **`cron.rs`** (unix) — render enabled schedules into a block delimited by
  `# >>> claude-config schedules >>>` … `# <<< claude-config schedules <<<`.
  Read via `crontab -l`, replace only the managed block, write via `crontab -`.
  Preserve all other user cron lines verbatim.
- **`task_scheduler.rs`** (windows) — equivalent via `schtasks /Create /F` per
  schedule (task names namespaced `ClaudeConfig\<id>`), `/Delete` for removed
  ones.
- **`primer.rs`** — generate the wrapper script and keep
  `<app-data>/primer-config/` in sync:
  - `settings.json` with an empty `env` block (forces OAuth fallback).
  - OAuth credentials: **symlink** to the real `.credentials.json` on
    unix (always fresh); **refresh-copy** on Windows (best-effort, perms 600).
  - The wrapper sets `CLAUDE_CONFIG_DIR=<app>/primer-config`, runs
    `claude -p "hi" --model <haiku>` with a timeout, and appends a run-record.
- **`runs.rs`** — append/read `runs.jsonl`
  (`{ schedule_id, started_at, exit_code, ok, error? }`).
- **`commands/schedule.rs`** — registered in `lib.rs`:
  - `list_schedules_cmd`, `add_schedule_cmd`, `update_schedule_cmd`,
    `delete_schedule_cmd`, `set_schedule_enabled_cmd`
  - `sync_schedules_cmd` — idempotent regen (also called at launch)
  - `get_schedule_status_cmd` — per-schedule last run + computed next fire time
  - `check_scheduling_available_cmd` — `claude` on PATH? scheduler present?
    subscription OAuth present? → drives UI warnings
  - `run_primer_now_cmd` — fire a primer immediately ("Prime now" test button)

## Frontend

- **`src/lib/types.ts`** — `Schedule`, `ScheduleRun`, `ScheduleStatus`,
  `SchedulingAvailability` (kept aligned with `models.rs`).
- **`src/lib/api.ts`** — thin wrappers for each new command.
- **`src/hooks/useSchedules.ts`** — state + CRUD, mirroring `useTracker.ts`.
- **`src/data/globalTabs.ts`** — add a **Schedules** entry to the global panel.
- **`src/components/Schedules.tsx`** (+ `ScheduleRow.tsx`, `ScheduleForm.tsx`)
  — using existing `src/components/ui/` primitives (`switch`, `select`,
  `button`, `dialog`, `badge`). Each row: label, time, weekday toggles, enable
  switch, last-run badge (✅ / ⚠️ / —), next-run text, "Prime now." Availability
  warnings render at the top when prerequisites are missing.

## Error handling & availability

- **`claude` not on PATH** → scheduling disabled with a fix hint.
- **No subscription OAuth** in `.credentials.json` → warn: run `claude /login`.
- **crontab/schtasks unavailable or write fails** → surface the error; never
  half-write (managed block is replaced atomically as a unit).
- **Primer run failure** (offline, expired token) → recorded in `runs.jsonl`,
  shown as ⚠️ on the row. Non-fatal; self-heals once Claude Code refreshes the
  token.
- **First enable** shows a one-time confirm explaining that an entry will be
  added to the system crontab / Task Scheduler (touching OS-level config).

## Testing

- **Rust unit tests** (pure functions, no real crontab):
  schedules → crontab block rendering, idempotent block replace preserving
  foreign lines, weekday mapping, `schedules.json` round-trip, `runs.jsonl`
  append/parse, next-fire-time computation.
- **`--ignored` tests** (environment-dependent, per keyring convention):
  actual crontab/schtasks write + primer execution.
- **Frontend**: manual verification via `pnpm tauri dev` (no JS test runner in
  this repo).

## Security & safety

- No secrets in `schedules.json`. OAuth stays in `.credentials.json` / keyring.
- The primer-config dir references the same plaintext credentials as the
  default location; the symlink/copy is created with `600` perms.
- The primer sends a real but trivial message (Haiku, `"hi"`) — spending a
  sliver of quota is the intended mechanism, not a side effect.

## Documented risks

1. **OAuth token staleness** in the primer dir (the CLI rotates the token).
   Mitigated by symlink (unix) / refresh-copy at launch (Windows); a failed
   run is visible and self-heals.
2. **macOS cron** may require Full Disk Access; launchd is a possible future
   refinement over crontab.
3. **Machine must be on and logged in** at fire time — inherent to any local
   scheduler; cron does not wake the machine.
4. **Cron entries persist after app uninstall** unless cleaned up — documented;
   "disable all" / delete removes the managed block.

## Out of scope for v1 (possible future work)

- Per-schedule provider targeting (prime GLM/Kimi windows too).
- launchd-native scheduling on macOS.
- Best-effort cron cleanup on uninstall.
