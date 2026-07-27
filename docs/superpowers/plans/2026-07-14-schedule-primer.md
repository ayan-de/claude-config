# Scheduled Window Primers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users define recurring primers (local time + weekday set) that fire a minimal `claude -p "hi"` invocation via the OS scheduler to reset the Anthropic subscription's 5-hour window, even when the app is closed.

**Architecture:** A new `src-tauri/src/schedule/` module owns a `schedules.json` store, a `runs.jsonl` run-log, pure cron-rendering / next-fire logic, a generated wrapper script, and an isolated `primer-config/` dir. `sync_schedules` idempotently regenerates the managed crontab block (unix) / Scheduled Tasks (windows), the wrapper, and the primer config from `schedules.json`; it runs after every mutation and at launch (with catch-up). A `Schedules` global-panel tab drives it through `commands/schedule.rs`.

**Tech Stack:** Rust (Tauri 2), chrono, uuid, serde/serde_json, tempfile; Next.js 16 static export, React, `@base-ui/react` primitives, sonner.

## Global Constraints

- Next.js 16 static SPA — no SSR/API routes. Frontend IPC only via `src/lib/api.ts` (never `invoke()` in components).
- Keep `src/lib/types.ts` aligned with `src-tauri/src/models.rs`. UI never receives secrets.
- Command naming: `<verb>_<noun>_cmd`; commands return `AppResult<T>` and take `state: tauri::State<'_, AppState>`.
- App-data JSON stores use the lock-free atomic write pattern from `save_providers_file` (tempfile + `sync_all` + `persist`).
- Timestamps are RFC3339 via `chrono::Utc::now().to_rfc3339()`; IDs are `uuid::Uuid::new_v4().to_string()`.
- Primer always targets the subscription via an isolated `CLAUDE_CONFIG_DIR=<app-data>/primer-config` with an empty `env` block.
- No new crates — chrono/uuid/serde_json/tempfile are already in `Cargo.toml`.
- Verification: `cd src-tauri && cargo test`; `pnpm exec tsc --noEmit`; `pnpm lint`. OS-touching tests are `#[ignore]` per the keyring convention.

---

## File Structure

**Backend (new):**
- `src-tauri/src/schedule/mod.rs` — module facade + re-exports.
- `src-tauri/src/schedule/store.rs` — `schedules.json` load/save (lock-free atomic).
- `src-tauri/src/schedule/runs.rs` — `runs.jsonl` append/read + last-run-per-schedule.
- `src-tauri/src/schedule/cron.rs` — pure: weekday↔cron mapping, `render_cron_line`, `render_managed_block`, `replace_managed_block`, `next_fire_time`, `last_expected_fire`, `needs_catchup`.
- `src-tauri/src/schedule/primer.rs` — wrapper-script generation, `primer-config/` sync, `run_primer` execution.
- `src-tauri/src/schedule/os_cron.rs` (unix) / `src-tauri/src/schedule/os_task.rs` (windows) — apply the managed block to the real scheduler.
- `src-tauri/src/schedule/sync.rs` — `sync_schedules` orchestration + launch catch-up.
- `src-tauri/src/commands/schedule.rs` — the IPC commands.

**Backend (modified):**
- `src-tauri/src/models.rs` — add `Weekday`, `Schedule`, `ScheduleInput`, `SchedulesFile`, `ScheduleRun`, `ScheduleStatus`, `SchedulingAvailability`.
- `src-tauri/src/state.rs` — `schedules_path`, `runs_path`, `primer_config_dir`, `scripts_dir`, `wrapper_path`.
- `src-tauri/src/storage/mod.rs` — re-export the schedule store/runs if needed (or keep under `schedule::`).
- `src-tauri/src/lib.rs` — `mod schedule;`, register commands, run launch sync + catch-up in `.setup`.
- `src-tauri/src/commands/mod.rs` — `pub mod schedule;`.

**Frontend (new):**
- `src/hooks/useSchedules.ts`, `src/components/Schedules.tsx`.

**Frontend (modified):**
- `src/lib/types.ts`, `src/lib/api.ts`, `src/data/globalTabs.ts`.

---

## Task 1: Schedule data model

**Files:**
- Modify: `src-tauri/src/models.rs`

**Interfaces:**
- Produces: `Weekday` (enum `Mon..Sun`, serde `lowercase`, `cron_num()`, `from_chrono()`), `Schedule { id, label: Option<String>, time: String, days: Vec<Weekday>, enabled: bool, created_at, updated_at }`, `ScheduleInput { id: Option<String>, label, time, days, enabled }`, `SchedulesFile { schema_version: u32, schedules: Vec<Schedule> }` (Default v1), `ScheduleRun { schedule_id, started_at, exit_code: Option<i32>, ok: bool, error: Option<String> }`, `ScheduleStatus { schedule_id, last_run: Option<ScheduleRun>, next_fire: Option<String> }`, `SchedulingAvailability { claude_on_path, scheduler_available, subscription_oauth_present, native_scheduling_present, scheduler_kind: String }`.

- [ ] **Step 1: Write failing tests** in `models.rs` `tests` module: `weekday_serializes_lowercase`, `weekday_cron_num_maps_sun_zero`, `schedule_round_trips_camel_and_snake` (id/time/days/enabled + created_at/updated_at snake_case), `schedules_file_default_is_v1_empty`, `schedule_run_round_trips`.
- [ ] **Step 2: Run** `cd src-tauri && cargo test schedule 2>&1 | head -40` — Expected FAIL (types missing).
- [ ] **Step 3: Implement** the structs/enums. `Weekday` derives `Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash` + `#[serde(rename_all = "lowercase")]`; `cron_num`: Sun=0, Mon=1..Sat=6; `from_chrono(chrono::Weekday)`. `Schedule`/`ScheduleRun`/`ScheduleStatus`/`SchedulingAvailability` derive `Debug, Clone, Serialize, Deserialize, PartialEq`; use `#[serde(rename_all = "camelCase")]` on the new structs so TS mirrors are camelCase (except keep it consistent — document in types.ts). `ScheduleInput` is `Deserialize` only.
- [ ] **Step 4: Run** `cd src-tauri && cargo test schedule 2>&1 | head -40` — Expected PASS.
- [ ] **Step 5: Commit** `feat(schedule): add schedule data model`.

## Task 2: schedules.json store

**Files:**
- Create: `src-tauri/src/schedule/mod.rs`, `src-tauri/src/schedule/store.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod schedule;`)

**Interfaces:**
- Consumes: `SchedulesFile` (Task 1).
- Produces: `load_schedules_file(&Path) -> AppResult<SchedulesFile>`, `save_schedules_file(&Path, &SchedulesFile) -> AppResult<()>`.

- [ ] **Step 1:** Write tests `load_returns_default_when_missing`, `save_then_load_roundtrip` mirroring `providers.rs` tests.
- [ ] **Step 2:** Run tests → FAIL.
- [ ] **Step 3:** Implement `store.rs` copying `save_providers_file`'s lock-free atomic pattern; `load_schedules_file` returns `SchedulesFile::default()` when missing/empty. Add `mod schedule;` to `lib.rs` and `pub mod store;` in `schedule/mod.rs`.
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(schedule): add schedules.json store`.

## Task 3: runs.jsonl run-log

**Files:**
- Create: `src-tauri/src/schedule/runs.rs`

**Interfaces:**
- Consumes: `ScheduleRun` (Task 1).
- Produces: `append_run(&Path, &ScheduleRun) -> AppResult<()>`, `read_runs(&Path) -> AppResult<Vec<ScheduleRun>>` (tolerant of blank/garbage lines), `last_run_per_schedule(&[ScheduleRun]) -> HashMap<String, ScheduleRun>`.

- [ ] **Step 1:** Tests: `append_then_read_roundtrip`, `read_skips_blank_and_bad_lines`, `last_run_picks_latest_by_started_at`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implement append (open create+append, write one compact JSON line + `\n`), read (split lines, `serde_json::from_str` each, skip errors), and last-run reducer (max by `started_at` string compare — RFC3339 sorts lexically).
- [ ] **Step 4:** PASS.
- [ ] **Step 5:** Commit `feat(schedule): add runs.jsonl log`.

## Task 4: pure cron rendering + next-fire logic

**Files:**
- Create: `src-tauri/src/schedule/cron.rs`

**Interfaces:**
- Consumes: `Schedule`, `Weekday`.
- Produces: `BLOCK_START`/`BLOCK_END` consts, `render_cron_line(&Schedule, wrapper: &str) -> Option<String>`, `render_managed_block(&[Schedule], wrapper: &str) -> Option<String>`, `replace_managed_block(existing: &str, new_block: Option<&str>) -> String`, `parse_hhmm(&str) -> Option<(u32,u32)>`, `next_fire_time(&Schedule, now: DateTime<Local>) -> Option<DateTime<Local>>`, `last_expected_fire(&Schedule, now) -> Option<DateTime<Local>>`, `needs_catchup(&Schedule, now, last_run: Option<DateTime<Local>>) -> bool`.

- [ ] **Step 1:** Tests: cron line `30 7 * * 1,2,3,4,5 "<wrapper>" <id>`; block wrapped in markers, only enabled schedules; `replace_managed_block` preserves foreign lines and replaces old block; empty (all disabled) → `None` block and stripped markers; `next_fire_time` picks next matching weekday/time strictly after now; `last_expected_fire` picks most recent ≤ now; `needs_catchup` true when today's expected fire missed and no run since, false when a run exists after it or expected not today.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implement per the design in the code below (see plan appendix — full function bodies provided at execution time from the spec's grounding facts).
- [ ] **Step 4:** PASS.
- [ ] **Step 5:** Commit `feat(schedule): cron rendering and next-fire logic`.

## Task 5: primer wrapper + primer-config

**Files:**
- Create: `src-tauri/src/schedule/primer.rs`
- Modify: `src-tauri/src/state.rs` (paths)

**Interfaces:**
- Produces: `render_wrapper_unix(config_dir, runs_path, model) -> String`, `render_wrapper_windows(...) -> String`, `render_primer_settings() -> serde_json::Value` (`{"env": {}}`), `sync_primer_config(config_dir) -> AppResult<()>` (writes settings.json, symlinks/copies `.credentials.json`), `write_wrapper(path, contents) -> AppResult<()>` (chmod 755 unix), `run_primer(state, schedule_id) -> ScheduleRun` (exec wrapper, append run).
- state.rs adds `schedules_path`, `runs_path`, `primer_config_dir`, `scripts_dir`, `wrapper_path`.

- [ ] **Step 1:** Tests (pure): wrapper contains `CLAUDE_CONFIG_DIR`, `claude -p "hi"`, `--model`, appends to runs path; `render_primer_settings` is `{"env":{}}`; state paths join correctly.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implement generators + state paths. `#[ignore]` test for `sync_primer_config` real symlink + `run_primer` real exec.
- [ ] **Step 4:** PASS (ignored tests skipped).
- [ ] **Step 5:** Commit `feat(schedule): primer wrapper and config`.

## Task 6: OS scheduler apply + sync orchestration

**Files:**
- Create: `src-tauri/src/schedule/os_cron.rs`, `src-tauri/src/schedule/os_task.rs`, `src-tauri/src/schedule/sync.rs`

**Interfaces:**
- Produces: `apply_managed_block(block: Option<&str>) -> AppResult<()>` (unix: `crontab -l`, `replace_managed_block`, `crontab -`), windows equivalent via `schtasks`; `sync_schedules(state) -> AppResult<()>` (regen primer-config + wrapper + scheduler block); `remove_all(state) -> AppResult<()>`; `catch_up_on_launch(state) -> AppResult<()>`.

- [ ] **Step 1:** Pure tests only where possible; `#[ignore]` tests for real crontab write/read/cleanup.
- [ ] **Step 2/3/4:** Implement; gate windows behind `#[cfg(windows)]`, unix behind `#[cfg(unix)]`.
- [ ] **Step 5:** Commit `feat(schedule): OS scheduler apply + sync`.

## Task 7: commands + lib.rs wiring

**Files:**
- Create: `src-tauri/src/commands/schedule.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces commands: `list_schedules_cmd`, `add_schedule_cmd(input)`, `update_schedule_cmd(input)`, `delete_schedule_cmd(id)`, `set_schedule_enabled_cmd(id, enabled)`, `sync_schedules_cmd`, `get_schedule_status_cmd -> Vec<ScheduleStatus>`, `check_scheduling_available_cmd -> SchedulingAvailability`, `run_primer_now_cmd(id)`.

- [ ] Implement validation (time `HH:MM`, days non-empty), read-modify-write on `schedules.json`, call `sync_schedules` after mutations. Register each in `lib.rs` handler list. Add launch `sync_schedules` + `catch_up_on_launch` in `.setup` (best-effort, logged). Commit `feat(schedule): commands and wiring`.

## Task 8: Frontend types + api

**Files:**
- Modify: `src/lib/types.ts`, `src/lib/api.ts`

- [ ] Add `Weekday`, `Schedule`, `ScheduleInput`, `ScheduleRun`, `ScheduleStatus`, `SchedulingAvailability` mirroring the Rust camelCase serde. Add `// ---------- schedules ----------` api wrappers for all 9 commands. `pnpm exec tsc --noEmit` PASS. Commit `feat(schedule): frontend types and api`.

## Task 9: Frontend hook + component + tab

**Files:**
- Create: `src/hooks/useSchedules.ts`, `src/components/Schedules.tsx`
- Modify: `src/data/globalTabs.ts`

- [ ] `useSchedules` mirrors `useTracker`/`useMcpServers`: list + status + availability, CRUD actions, `toast`, tri-state loading. `Schedules.tsx` exports `SchedulesView` + `SchedulesSidebarButton`: header, availability warnings, rows (label/time/weekday toggles/enable switch/last-run pill/next-run/Prime now), add/edit form dialog, first-enable confirm. Register in `globalTabs.ts`. `pnpm lint` + `tsc` PASS. Commit `feat(schedule): schedules UI tab`.

## Self-Review notes

- Spec coverage: store (T2), cron.rs (T4), task_scheduler (T6), primer.rs (T5), runs.rs (T3), all commands (T7), all frontend files (T8–9), catch-up (T6/T7), availability + native-detection note (T7), first-enable confirm + status pills (T9). ✓
- Windows refresh-copy isolation caveat and macOS Full Disk Access are documented risks; implemented best-effort with `#[ignore]` coverage.
