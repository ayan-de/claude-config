# Remote Sessions Tab — Per-Row Sync Action (Download / Update / Conflict / In-Sync)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current "always Download" button next to every remote session with a four-state action that reflects what clicking the row will actually do on *this* machine: `Download` (first-time pull), `Update` (safe pull, no local edits), `Conflict` (pull will overwrite local edits — prompt first), `InSync` (nothing to pull, button disabled).

**Non-goal:** Change the upload flow, the mtime-based `SyncState` used by the Local tab's per-row icon color, or the on-disk shape of `session_sync_state.json`. This is a read-only reclassification of what the Remote tab shows for the *download* button.

## Background — the current gap

Today `RemoteSessionsList` renders a plain `Download` button next to every row (`components/RemoteSessionsList.tsx`), regardless of whether:
- the session already exists locally with the same content SHA (button should be disabled)
- the session exists locally with a different content SHA and local mtime is unchanged since last upload (safe update — no prompt needed)
- the session exists locally with a different content SHA *and* the local file has been edited since last upload (both sides moved — the user's about to lose local edits)

The write-path already handles the third case: `github_download_session_cmd` runs an RFC3339 mtime compare against the per-project `metadata.json` and returns `SessionDownloadConflict { kind: RemoteNewer | LocalNewer }` before overwriting (`commands/github_sync.rs:793`). But the button label doesn't reflect it, and there's no cheap way to tell the user "this row is fine to click" vs. "clicking this will trip a confirm dialog" without actually clicking.

The right trigger is SHA equality, not mtime equality:
- Every upload persists `remote_sha` (the GitHub blob SHA of the transcript) into `SessionSyncStateFile` at `<project_folder>/session_sync_state.json` (`commands/github_sync.rs:463`).
- Every remote list row carries the same `sha` field from the recursive tree walk (`models.rs:462`).
- Comparing those two SHAs answers "did remote content change since this machine last synced" without a byte-comparison.

Mtime is only involved when we already know SHAs disagree — it's what distinguishes safe-Update from Conflict.

## Architecture — one command, four states

**Decision:** put the classifier on the backend, and surface the result as a new `syncAction` field directly on `RemoteSessionSummary`. Don't add a separate command.

Reasons:
1. **Single source of truth.** The mtime-based conflict guard already lives in `github_download_session_cmd`. If the frontend compares SHAs independently to pick the button label, the button state and the write-guard read from two different places and can drift. One backend classifier means the button label and the write-guard use the same logic.
2. **Four states, not three.** The frontend has access to `remote_row.sha` and (via a separate command) `session_sync_state.json`, but not the transcript's *current* on-disk mtime. Distinguishing `Update` from `Conflict` needs `current_mtime` vs `last_local_modified` — the backend already has both on hand.
3. **Same-call composition.** `sync_action` is always needed alongside the list and never independently — piggybacking it on `github_list_remote_sessions_cmd` sidesteps a stale-pairing bug where two lists fetched at slightly different times don't line up row-for-row.
4. **Zero extra GitHub calls.** The classifier reads local filesystem state only. It composes cleanly with the existing SHA-gated cache from `2026-07-11-remote-sessions-caching.md` — on a warm cache, this stays 1 HTTP call.

Cost:
- One new enum on the wire (`SyncAction`).
- `RemoteSessionSummary` grows one field. Cached JSON without the field must still deserialize — handled by `#[serde(default)]` on Rust + a bumped SWR cache key on the frontend.

**The four states:**

| Local state | Remote state | `SyncAction` | Button UX |
|---|---|---|---|
| No entry in `session_sync_state.json`, or file missing | present | `Download` | primary button, "Download" |
| Entry present, `local.remote_sha == remote.sha` | present | `InSync` | disabled, "Synced" |
| Entry present, SHAs differ, local mtime **==** stored `last_local_modified` | present | `Update` | primary button, "Update" |
| Entry present, SHAs differ, local mtime **!=** stored `last_local_modified` | present | `Conflict` | warning-styled button, "Update"; click routes through the existing `SessionDownloadConflict` confirm prompt |

The `Conflict` variant is deliberately not a separate button label — the user still sees "Update" — because the visual affordance is styling (warning color / icon), not a different verb. The behavior difference is the prompt.

**Tech Stack:** Rust (Tauri 2, existing storage/github_sync module), TypeScript/React 19. No new dependencies.

## Global Constraints

- **Additive to existing IPC.** `RemoteSessionSummary` gains one field; `#[serde(default)]` on Rust and an SWR cache-key bump on TS keep old cached JSON from rendering `Download` for every row after upgrade.
- **No new HTTP calls.** The classifier is a pure local-filesystem read (`session_sync_state.json` + `fs::metadata`). It runs after the list is built, whether the list came from cache or from a fresh tree walk.
- **Reuse `classify_sync_state`.** The mtime-comparison logic that already powers the Local tab's colored icons (`storage/github_sync.rs::classify_sync_state`) is the primitive `Conflict` detection needs. Don't duplicate it. **Contract confirmed** (`storage/github_sync.rs:168`): pure mtime function, no SHA involvement. Returns `NeverUploaded` when metadata is `None`; `OutOfSync` when either `last_local_modified` or `last_uploaded` is `None`, or when mtimes differ by >1s; `Synced` when mtimes match within 1s. That means the `classify_sync_action` mapping `Synced → Update` / `OutOfSync → Conflict` is exactly right — an `OutOfSync` return with a missing anchor timestamp still correctly routes to `Conflict` (we can't prove local is unmodified without the anchor, so treat it as edited).
- **Slug → local folder resolution.** For each row the classifier needs to know where the local project folder is. That's exactly what `slug_mappings` in `ProjectPathMappings` records (`models.rs:489`). A row with no matching `slug_mappings` entry is `Download` by definition — no local folder means nothing local exists yet.
- Verification: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.

## File Structure

| File | Role | Touched in |
|---|---|---|
| `src-tauri/src/models.rs` | Add `SyncAction` enum + `sync_action` field on `RemoteSessionSummary` | Task 1 |
| `src-tauri/src/storage/github_sync.rs` | Add `classify_sync_action(local: Option<&SessionSyncMetadata>, remote_sha: &str, current_mtime: i64) -> SyncAction` next to existing `classify_sync_state` | Task 2 |
| `src-tauri/src/commands/github_sync.rs` | New `annotate_sync_actions(rows: &mut Vec<RemoteSessionSummary>, mappings: &ProjectPathMappings)`; call it at the end of `github_list_remote_sessions_cmd` after the cache write-back | Task 3 |
| `src/lib/types.ts` | Mirror `SyncAction` + `syncAction` field | Task 4 |
| `src/hooks/useRemoteSessions.ts` | Bump SWR cache key `remoteSessions:v1` → `v2` so pre-upgrade cached rows don't render as "Download" for everything | Task 4 |
| `src/components/RemoteSessionsList.tsx` | Render button label + style based on `row.syncAction`; disable for `InSync` | Task 5 |
| `src/hooks/useRemoteSessions.ts` (`download()`) | On `Conflict`, skip the stale-row guard's SHA re-check and go straight into the download call so the existing `SessionDownloadConflict` prompt fires as designed | Task 5 |
| `docs/GITHUB_SYNC_PLAN.md` | Update the "Sync State Workflow" and "UI Components" sections to describe the four states | Task 6 |

## Concerns to Address

**Cache invalidation.** The classifier reads `session_sync_state.json` — that file changes on every local upload, and the current SHA-gated list cache doesn't know about it. Two options:

- (a) Recompute `sync_action` on every list return, even on a cache hit. Cheap (tens of file stats per project); keeps the classifier's inputs always-fresh.
- (b) Invalidate `sessions_list` on every `github_upload_session_cmd` (already done) *and* every `session_sync_state.json` write (not done). More precise but bigger blast radius — every write path becomes a cache-invalidation site.

**Recommendation:** (a). Recomputing `sync_action` costs one `fs::read` per project slug plus one `fs::metadata` per row — negligible next to the network round-trips it's replacing, and it means the button state is always live vs. what's on disk right now (matters if the user uploads from a different app instance and comes back to this tab).

**Missing local folder.** A remote row's slug may not match any `slug_mappings` entry — either the user hasn't downloaded anything for this project on this machine yet, or the mapping was pruned. That's `Download` — same as "no local session file." No special case needed as long as the classifier gates on "does the mapped folder exist AND does the transcript file exist" before reading `session_sync_state.json`.

**Stale `session_sync_state.json` entry for a deleted file.** If the user deletes `<session>.jsonl` by hand but nothing prunes the matching entry from `session_sync_state.json`, `local` is still `Some` with a `remote_sha`. If that stored SHA happens to equal the current remote row's SHA, `classify_sync_action` would return `InSync` and the button would render disabled "Synced" for a session that has no local copy at all — with no path back to `Download` from the disabled button. Fix: check file existence *before* consulting the state file. If `<session_id>.jsonl` doesn't exist on disk, force `SyncAction::Download` and skip the state-file lookup entirely. This parallels the "no mapping" fix one level deeper — mapping exists, state entry exists, but the actual transcript is gone. (Task 3's pseudocode includes this guard; the manual QA covers it explicitly.)

**Stored `remote_sha` is `Option<String>`.** Old entries (pre-Phase 2, or ones written by a bug) may have `remote_sha: None`. Treat that as `Download` — the row can't be `InSync` without knowing what it was synced against.

**Cached list from SWR localStorage on old client.** After deploying this, users with a cached `remoteSessions:v1` payload have rows with no `syncAction` field. TypeScript would default to `undefined`, which would render as neither Download nor Update. Bump the cache key to `v2` so old payloads are ignored and the next mount refetches. (The Rust `#[serde(default)]` covers the same case for the in-memory cache on the backend after a restart.)

**Stale-row guard interaction.** `useRemoteSessions.download()` already has a 60s stale-row guard that force-refetches the list before download. That's still correct for `Download` and `Update` — but for `Conflict`, the stale-row guard should still run (protects against "row was deleted upstream in the last 60s"), and *then* the download call itself will trip `SessionDownloadConflict` and the confirm dialog will fire. No new code path here; the existing `doDownloadRef.current(row, cb, true)` on user-confirm handles the force-overwrite retry.

## Tasks

- [ ] **Task 1 — Add `SyncAction` enum and field.** Edit `src-tauri/src/models.rs`:
   - Add `SyncAction { Download, Update, Conflict, InSync }` (derive `Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq`; `#[serde(rename_all = "snake_case")]`).
   - Add `#[serde(default)] pub sync_action: SyncAction` to `RemoteSessionSummary`. Impl `Default for SyncAction` returning `Download` — plain `#[serde(default)]` picks that up, no separately-named function needed.
   - Add a unit test in `models.rs::tests` that deserializes a `RemoteSessionSummary` JSON payload with no `syncAction` field and asserts `sync_action == SyncAction::Download`.

- [ ] **Task 2 — Add `classify_sync_action` primitive.** Edit `src-tauri/src/storage/github_sync.rs` next to `classify_sync_state`:
   - `pub fn classify_sync_action(local: Option<&SessionSyncMetadata>, remote_sha: &str, current_mtime: i64) -> SyncAction`
   - Logic:
     - `local` is `None` → `Download`
     - `local.remote_sha` is `None` → `Download`
     - `local.remote_sha.as_ref() == Some(&remote_sha.to_string())` → `InSync`
     - SHAs differ; reuse `classify_sync_state(local, current_mtime)`:
       - `Synced` → `Update` (safe pull, local unmodified since last upload)
       - `OutOfSync` → `Conflict` (both sides moved)
       - `NeverUploaded` — unreachable here because `local.remote_sha.is_some()` — fall through to `Download` defensively.
   - Unit tests: one per branch (`Download`/`InSync`/`Update`/`Conflict`), plus the `remote_sha: None` guard.

- [ ] **Task 3 — Wire the classifier into the list command.** Edit `src-tauri/src/commands/github_sync.rs`:
   - New helper `fn annotate_sync_actions(rows: &mut [RemoteSessionSummary], mappings: &ProjectPathMappings)`:
     - Group rows by `project_slug` (single pass).
     - For each slug:
       - Look up `mappings.slug_mappings.get(slug)` — no mapping → set every row in group to `Download`, continue.
       - Load `<local_folder>/session_sync_state.json` via `load_session_sync_state`. On IO error → treat every entry as absent (`Download`), don't surface the error to the UI; log at `warn`.
     - Per row:
       - `let jsonl = local_folder.join(format!("{session_id}.jsonl"));`
       - **File-existence guard:** if `!jsonl.exists()`, set `row.sync_action = SyncAction::Download` and `continue`. Don't read `session_sync_state.json` for this row — a stale entry with a matching SHA would otherwise mis-classify a deleted-locally session as `InSync`.
       - `let current_mtime = fs::metadata(&jsonl).ok().and_then(|m| m.modified().ok())...` — always non-zero here because we just proved the file exists; kept `unwrap_or(0)` defensively.
       - `let local = state_file.sessions.get(session_id);`
       - `row.sync_action = classify_sync_action(local, &row.sha, current_mtime);`
   - Call `annotate_sync_actions(&mut rows, &mappings)` at the end of `github_list_remote_sessions_cmd`, **after** the cache write-back but **before** returning. Load `mappings` once (already needed elsewhere in the command) — same `load_path_mappings(&mappings_path(&state))?` call the download command uses.
   - **Important:** also call `annotate_sync_actions` on the *cached* path (the SHA-gate warm-cache branch that returns `cached_list` verbatim). The cached list may be stale re: local state — see the "Cache invalidation" concern above. Extract the SHA-gate hit into a `let mut rows = cached_list;` binding, run `annotate_sync_actions(&mut rows, &mappings)`, return `Ok(rows)`.
   - Unit test: feed a fake `Vec<RemoteSessionSummary>` + a fake `ProjectPathMappings` + a `tempdir` project folder with a hand-written `session_sync_state.json`, assert each row gets the expected `sync_action`. Cover six fixture cases:
     1. No `slug_mappings` entry → `Download`
     2. Mapping exists, no `session_sync_state.json` → `Download`
     3. Mapping + state entry with `remote_sha == row.sha` and `.jsonl` present → `InSync`
     4. Mapping + state entry, SHAs differ, mtime matches `last_local_modified` → `Update`
     5. Mapping + state entry, SHAs differ, mtime differs → `Conflict`
     6. Mapping + state entry with matching `remote_sha` but `.jsonl` **absent** on disk → `Download` (regression test for the stale-entry-deleted-file trap)

- [ ] **Task 4 — Mirror to TypeScript.** Edit `src/lib/types.ts`:
   - `export type SyncAction = "download" | "update" | "conflict" | "in_sync";` (matches `serde(rename_all = "snake_case")` output).
   - Add `syncAction: SyncAction;` to `RemoteSessionSummary`.

   Edit `src/hooks/useRemoteSessions.ts`:
   - Change `const CACHE_KEY = "remoteSessions:v1"` → `"remoteSessions:v2"`. Old cache is discarded on next open; no migration code needed.

- [ ] **Task 5 — Render the four states.** Edit `src/components/RemoteSessionsList.tsx`:
   - Replace the plain `<Button>Download</Button>` with a `<SyncActionButton row={row} onClick={...}/>` component (co-located in the same file — small enough).
   - Rendering:
     - `Download` → `variant="default"`, label "Download"
     - `Update` → `variant="default"`, label "Update"
     - `Conflict` → `variant="default"` + `className="border-amber-500/60 text-amber-600 dark:text-amber-400"` (or existing warning token), label "Update", `title="This session has local changes since last upload — click to review"`
     - `InSync` → `disabled`, `variant="ghost"`, label "Synced" with a checkmark icon (`CheckCircle2` from lucide-react, already imported elsewhere)
   - The click handler is the same in all clickable states — `useRemoteSessions.download()` already knows how to handle `SessionDownloadConflict`. No new frontend logic for `Conflict`.

- [ ] **Task 6 — Update `docs/GITHUB_SYNC_PLAN.md`.**
   - "Sync State Workflow → On Download" section: describe the four `SyncAction` states and how the button label maps.
   - "UI Components → `RemoteSessionsList`" bullet: mention `SyncActionButton`.
   - "Tauri Commands" section: no new command, but note that `github_list_remote_sessions_cmd` now populates `sync_action` on every row.

## Verification Strategy

**Cargo tests** (`cd src-tauri && cargo test`):
- `classify_sync_action`'s four branches individually.
- `RemoteSessionSummary` deserialization with missing `syncAction` defaults to `Download`.
- `annotate_sync_actions` with a fixture folder covering: no mapping → Download, mapping + no state file → Download, mapping + state file with matching SHA → InSync, mtimes match with differing SHA → Update, mtimes differ with differing SHA → Conflict, and a matching SHA with the `.jsonl` file absent → Download (regression test for the stale-entry-deleted-file trap).

**Manual QA:**
1. Upload session on machine A. On machine A, refresh Remote tab → button reads **Synced** (disabled). No HTTP call beyond the SHA-gate.
2. On machine B (never seen this session), open Remote tab → button reads **Download**. Click → session appears with green icon.
3. On machine B, upload a fresh commit for the same session (edit + upload). On machine A, refresh Remote tab → button reads **Update**. Click → transcript is replaced without a prompt.
4. On machine A, edit `<session>.jsonl` locally (touch is enough — any mtime bump). Then on machine B, upload a fresh version. On machine A refresh → button reads **Update** with warning styling. Click → confirm dialog fires ("Remote copy is newer than the local file. Overwrite local?"). Confirming overwrites; canceling leaves both sides untouched.
5. Delete the local `<session>.jsonl` on machine A → button flips back to **Download** on next refresh.
6. **Stale-entry-deleted-file guard:** delete the local `<session>.jsonl` on machine A *without* touching `session_sync_state.json` (leave the entry with its old matching `remote_sha` in place) → button must read **Download**, not **Synced**. If this renders Synced (disabled), the file-existence guard in `annotate_sync_actions` isn't running before the state-file lookup.
7. Corrupt `session_sync_state.json` (invalid JSON) → button falls back to **Download** for every row in that project; a warning is logged; the tab does not crash.

**Frontend:**
- Delete `localStorage["remoteSessions:v1"]` manually and confirm next mount re-fetches (SWR key bump works).
- On a warm cache, verify that a click on a Synced row is a no-op (button is `disabled`, not just visually).

**Type/lint gates:**
- `pnpm lint`
- `pnpm exec tsc --noEmit`

## Rollout

One PR, six tasks in order. Task 1 and Task 4 introduce the wire shape; Task 3 fills it in; Task 5 renders it. Deploying Tasks 1–4 without Task 5 is safe (button still renders "Download" from the fallback label). Deploying Task 5 without the backend is *not* safe (all rows render "Download" because the field defaults are set client-side too) — Task 5 must land after Task 3 in the same PR.
