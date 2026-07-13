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

## Why not native Claude Code scheduling?

As of April 2026, Claude Code ships two native scheduling features that overlap
with this design. We evaluated both and still chose custom OS-cron. This
section exists so a reviewer familiar with current Claude Code sees the
reasoning up front.

- **Cloud Routines** ([docs](https://code.claude.com/docs/en/routines)) —
  research preview, launched 2026-04-14. Scheduled/API/GitHub triggers, run on
  Anthropic infrastructure even with the machine off, created via
  `claude.ai/code`, the Desktop app, or the CLI `/schedule` *slash* command.
  Routines "share the same subscription usage quota as interactive sessions."
- **Desktop Scheduled Tasks**
  ([docs](https://code.claude.com/docs/en/desktop-scheduled-tasks)) — task defs
  at `~/.claude/scheduled-tasks/<name>/SKILL.md` (honors `CLAUDE_CONFIG_DIR`),
  fire while the Desktop app is open and awake.

Why they don't replace this feature:

1. **No native *local* scheduler on Linux.** The docs state plainly that Linux
   users must use CLI `/loop`, Cloud Routines, or "traditional cron jobs
   running `claude -p` in headless mode." Desktop Scheduled Tasks are macOS +
   Windows only. This app is Linux-first, so custom cron is the *recommended*
   path there, not reinvention.
2. **`claude -p` on OAuth is the surest primer.** It is a normal interactive
   message on the subscription token, so it deterministically starts the exact
   local 5-hour window the tracker measures. A cloud Routine only "shares the
   same quota" — strongly implied to reset the same window, but an inference,
   not a documented guarantee. Cron sidesteps the question.
3. **No headless create API.** Routines are created via interactive `/schedule`
   or the web; Desktop tasks via the Routines page / natural-language MCP tools,
   and their schedule/enabled state isn't even in the on-disk `SKILL.md`. An app
   "just orchestrating native scheduling" would have to drive an interactive
   session — brittle and undocumented.
4. **Research preview + web-gated.** Routines require Claude Code on the web
   enabled and may change; we can't assume every user has them.

`check_scheduling_available_cmd` should additionally detect an existing native
setup — presence of `~/.claude/scheduled-tasks/` (honoring `CLAUDE_CONFIG_DIR`)
— and surface a one-line note that native Routines/Desktop Tasks are an
alternative, so our schedules don't silently compete with theirs.

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
    subscription OAuth present? native `scheduled-tasks/` dir present? → drives
    UI warnings and the "native alternative" note
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
2. **`CLAUDE_CONFIG_DIR` isolation is imperfect (undocumented feature).**
   - Issue #3833: tool-permission local state (`settings.local.json`) can be
     written to the CWD rather than the config dir. Unlikely to matter for a
     bare `"hi"` prompt with no tool use — covered by a test.
   - On **Windows**, some global state (`~\.claude.json`) has been reported as
     shared across config-dir "profiles" rather than isolated. This weakens the
     Windows refresh-copy isolation trick; verify the primer still targets the
     subscription and not a leaked provider override. If isolation proves
     unreliable on Windows, fall back to running the primer only when the
     Subscription provider is the active one.
3. **macOS cron** may require Full Disk Access; launchd is a possible future
   refinement over crontab.
4. **Machine must be on and logged in** at fire time — inherent to any local
   scheduler; cron does not wake the machine.
5. **Artifacts persist after app uninstall** unless cleaned up — the managed
   cron block / Scheduled Tasks **and** the copied Windows credential file under
   `primer-config/`. "Disable all" / delete removes the managed block and the
   primer-config dir (including the credential copy); document that uninstall
   without first disabling leaves them behind. A stale rotated-token copy is not
   a leak but is an artifact to clean up.

## Out of scope for v1 (possible future work)

- Per-schedule provider targeting (prime GLM/Kimi windows too).
- launchd-native scheduling on macOS.
- Best-effort cron cleanup on uninstall.
