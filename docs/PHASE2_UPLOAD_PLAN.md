# Phase 2 — Upload Session (Implementation Plan)

## Where Phase 1 left us (more is done than the plan assumes)

Phase 1 landed not just OAuth/connection but **all the low-level upload plumbing**. What already exists:

- `github/repo.rs` — every Git Data API primitive: `get_repo`, `create_repo`, `get_branch_head`, `get_tree_recursive`, `create_blob`, `create_tree`, `create_commit`, `create_ref`, `update_ref`, `tree_to_remote_sessions`, `get_authenticated_user`.
- `github/client.rs` — pooled `reqwest::blocking` client with `get/post/patch/put/delete` + typed `GitHubError`, base64 helpers.
- `storage/github_sync.rs` — `session_sync_state.json` load + **atomic locked write** (`write_session_sync_state_atomic`), `classify_sync_state(metadata, mtime)`, path helpers (`remote_session_path`, `project_metadata_path`, `manifest_path`).
- `models.rs` — `SessionSyncStateFile`, `SessionSyncMetadata`, `SyncState`, `RemoteSessionSummary`, `GitHubSyncConfig` (has `privacy_consent_given`, `last_sync`).
- Commands already registered: config/connect/disconnect/consent/repo-name/path-mappings/`github_check_repo_cmd`.
- Frontend: `useGitHubSync` + `GitHubSyncContext`, `GitHubSyncPanel`, device-flow modal, `api.ts` wrappers, TS types.
- The GitHub icon placeholder in `SessionRow` (`Sessions.tsx:418-425`) is a decorative SVG — not yet a button.

So Phase 2 is **orchestration + 3 commands + frontend wiring**, not building the API layer.

## Critical design decision: derive the slug, don't re-encode

The plan flags slug encoding as CRITICAL because `/home/ayan.de/x` → `-home-ayan-de-x` is lossy and ambiguous (verified against real folders: `.claude` → `--claude`). **We avoid the problem entirely on upload:** a session's `full_path` is already `<claude_dir>/projects/<slug>/<uuid>.jsonl`. The slug is literally the parent directory name — read it, never compute it. `project_path` (the decoded cwd) is stored separately in per-project `metadata.json` as `original_path` for Phase 3 remapping.

- `project_slug` = `full_path.parent().file_name()` (authoritative, on-disk).
- `session_id` = provided by caller (matches file stem).
- Remote path = `sessions/<slug>/<uuid>.jsonl` via existing `remote_session_path`.

## Backend work

### 1. `github/upload.rs` (new) — orchestration over existing primitives

`pub fn upload_session(token, owner, repo_name, files: Vec<(String /*repo_path*/, Vec<u8>)>) -> Result<UploadResult, GitHubError>`

Flow (matches plan §"Upload flow", reuses repo.rs fns):
1. `get_repo` → if `None`, `create_repo`. Capture `default_branch` (never hardcode "main").
2. `get_branch_head(default_branch)` → `Some(head_sha)` or `None` (fresh repo, first commit).
   - If `Some`: fetch that commit's tree SHA for `base_tree` (add small `get_commit` helper in repo.rs, or reuse tree via `get_tree_recursive` root sha).
3. For each file: `create_blob` → collect `TreeItem { path, mode: "100644", type: "blob", sha }`.
4. `create_tree(base_tree, items)`.
5. `create_commit(message, tree_sha, parents)` — parents `[]` on first commit else `[head_sha]`.
6. First commit: `create_ref`; else `update_ref` (PATCH, `force:false`).
7. Return `UploadResult { blob_shas: HashMap<session_path, sha>, commit_sha, default_branch }`.

**Concurrency retry (plan §Concurrent Uploads):** wrap steps 2→6 in a loop, max 3 attempts. Before `update_ref`, re-fetch `get_branch_head`; if it moved off the parent we built against, rebuild tree/commit against the new head and retry. `create_ref` 422 (ref exists) → treat as race, refetch and retry as update.

### 2. Commands in `commands/github_sync.rs` (3 new)

Delete the "Phase 2 will add…" comment. Add:

**`github_upload_session_cmd(state, session_id, full_path, project_path) -> AppResult<SessionSyncMetadata>`**
1. `load_github_token` (already exists). Load config.
2. **Privacy gate:** if `!cfg.privacy_consent_given` → return `AppError::GitHubNotConfigured("privacy_consent_required")` so the frontend shows the consent dialog first. (Frontend calls `githubSetPrivacyConsent(true)` then retries; matches existing consent flow.)
3. **Large-file guard:** `fs::metadata(full_path).len()` > 95 MB → `AppError::Validation("Session too large for GitHub (95 MB max)")`.
4. Read file bytes + capture mtime (`fs::metadata(...).modified()` → RFC3339 via chrono).
5. Derive `slug` from `full_path` parent dir name. Build the session blob at `remote_session_path(slug, session_id)`.
6. Build/refresh per-project `metadata.json` (see §3) and include it in the same commit (one atomic commit = session + metadata).
7. `owner = get_authenticated_user(token)`; call `upload::upload_session(...)`.
8. On success write `session_sync_state.json` (project folder = `full_path.parent()`) via `write_session_sync_state_atomic`, upserting this session's `SessionSyncMetadata { last_uploaded: now, remote_sha: <blob sha>, last_local_modified: <mtime>, sync_state: Synced }`.
9. Update `github_sync.json` `last_sync = now`.
10. Map `GitHubError` via existing `map_gh` (401 → `GitHubAuthRequired` → frontend clears connection). Return the metadata so the row updates without a full refetch.

**`github_get_session_sync_state_cmd(state, project_path) -> AppResult<SessionSyncStateFile>`**
- `project_path` here is the on-disk project folder (parent of the jsonl). Load `session_sync_state.json`; return default if absent. Used to color all rows on list load.

**`github_check_session_sync_status_cmd(state, session_id, full_path) -> AppResult<SyncState>`**
- Load state file from `full_path.parent()`, look up `session_id`, `fs::metadata` mtime, return `classify_sync_state(entry, mtime)` (already implemented + tested).

Register all 3 in `lib.rs` `.invoke_handler()`.

### 3. Per-project `metadata.json` (repo side)

Small struct (new, in models or upload.rs), serialized to `sessions/<slug>/metadata.json`:
```
{ "version": 1, "original_path": "<decoded project_path>",
  "sessions": { "<uuid>": { "title": null, "modified": "<rfc3339>", "messageCount": 0 } } }
```
On upload: `get_blob` the existing metadata if present (via tree lookup), merge this session's entry, re-upload in the same commit. `original_path` is what Phase 3 uses for path remapping — this is why we store the decoded path even though the slug is authoritative for layout. Keep it minimal in v2; `tree_to_remote_sessions` already skips `metadata.json`/`manifest.json`.

(Root `manifest.json` deferred — `tree_to_remote_sessions` reconstructs the project list from the tree in Phase 3, so a central manifest isn't required for upload to be correct. Note this as a deferred optimization.)

## Frontend work

### 4. `api.ts` — 3 wrappers
```ts
export const githubUploadSession = (sessionId, fullPath, projectPath) =>
  call<SessionSyncMetadata>("github_upload_session_cmd", { sessionId, fullPath, projectPath });
export const githubGetSessionSyncState = (projectPath) =>
  call<SessionSyncStateFile>("github_get_session_sync_state_cmd", { projectPath });
export const githubCheckSessionSyncStatus = (sessionId, fullPath) =>
  call<SyncState>("github_check_session_sync_status_cmd", { sessionId, fullPath });
```

### 5. `types.ts` — mirror Rust
Add `SyncState` (`"never_uploaded" | "synced" | "out_of_sync"`), `SessionSyncMetadata`, `SessionSyncStateFile` (camelCase per convention).

### 6. `useSessionUpload.ts` (new hook)
Owns per-session upload state so rows don't each re-implement it:
- `stateBySession: Map<sessionId, SyncState>` seeded from `githubGetSessionSyncState` per project folder on mount/refresh.
- `uploadingIds: Set<sessionId>` for the spinner.
- `upload(session)`:
  - if not connected → toast "Connect GitHub first" (read from `useGitHubSyncContext`).
  - if backend returns `github_not_configured` (`privacy_consent_required`) → open consent dialog; on accept, `setPrivacyConsent(true)` then retry once.
  - `out_of_sync` → confirm dialog "Local changes detected. Update remote copy?" before uploading.
  - on success set state `synced`, `toast.success`; on `github_auth_required` → context disconnect + toast "GitHub connection expired. Reconnect."; other errors → `toast.error` with retry.

### 7. `Sessions.tsx` — wire the icon (replace SVG at 418-425)
- Extract a `<GithubIcon>` (already exists in GitHubSync.tsx — export & reuse, keep one copy).
- New `SessionSyncIcon` button inside `SessionRow`, `onClick` stops propagation (don't open the transcript), calls `upload(session)`. Color by state:
  - gray/muted — `never_uploaded` — tooltip "Upload to GitHub"
  - green/primary — `synced` — tooltip "Uploaded {relative}"
  - amber/warning — `out_of_sync` — tooltip "Local changes — click to update"
  - `Loader2` spin — uploading — not clickable
- Only render the icon when `config.isConnected`; otherwise keep the current static muted mark (or hide).
- Thread `syncState` + `upload` + `uploadingIds` down from `SessionsView` → `ProjectAccordion` → `SessionGroup` → `SessionRow` (or consume the hook via context to avoid prop drilling — prefer a small context like the existing `GitHubSyncContext`).
- Optional per plan: a sub-line "Uploaded {t} · Modified {t}" when `out_of_sync`. Keep minimal for v2.

## Files touched

**New (Rust):** `src-tauri/src/github/upload.rs`
**New (TS):** `src/hooks/useSessionUpload.ts` (+ optional `SessionUploadContext.tsx`)
**Modified (Rust):** `github/mod.rs` (add `pub mod upload;`), `github/repo.rs` (add `get_commit` helper for tree SHA), `commands/github_sync.rs` (3 cmds), `lib.rs` (register 3), possibly `models.rs` (project metadata struct).
**Modified (TS):** `lib/api.ts`, `lib/types.ts`, `components/Sessions.tsx`, export `GithubIcon` from `components/GitHubSync.tsx`.

## Edge cases handled in v2 (vs deferred)

Handled: privacy-consent gate, 401→disconnect, large-file reject, concurrent-upload ref-retry, out-of-sync confirm, first-commit vs update-ref.
Deferred to Phase 3/4: download/remapping, root `manifest.json`, conflict resolution on download, retention, worktree awareness, "download as copy".

## Verification

- `cd src-tauri && cargo test` (existing storage tests stay green; add unit test for slug-from-path extraction and upload retry logic where mockable).
- `pnpm lint && pnpm exec tsc --noEmit`.
- Manual (`pnpm tauri dev`): connect → upload a session → confirm private repo + `sessions/<slug>/<uuid>.jsonl` + `metadata.json` on GitHub → re-open app, icon green → edit session locally, icon amber → re-upload → green.
