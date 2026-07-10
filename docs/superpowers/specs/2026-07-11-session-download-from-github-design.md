# Download sessions from GitHub (Phase 3)

Date: 2026-07-11

## Problem

Phases 1 + 2 let the user push a session to a private GitHub repo. There is
no path back: sessions on another machine, or pushed before a fresh install,
are stranded in the repo. We need an in-app browser that lists the remote
sessions, lets the user pick one, resolves the cross-machine project-path
problem, writes the `.jsonl` to the right local folder, registers it with
Claude Code's `sessions-index.json`, and immediately reflects it in the
local Sessions list with a green "synced" icon.

The repo layout already shipped in Phase 2:

```
claude-sessions/
└── sessions/
    └── <project-slug>/
        ├── <uuid>.jsonl          # the transcript
        └── metadata.json         # { originalPath, sessions: { uuid → {title, modified, messageCount} } }
```

Phase 3 consumes this; it does not change the upload shape.

## Goals

- A new "Browse remote" affordance in the global top bar (next to the
  GitHub icon already shipped) opens a modal that lists every remote
  session grouped by project.
- Each row shows the same title the upload side wrote (read from
  `metadata.json`), the project slug + decoded `originalPath`, the modified
  time, and message count.
- Clicking "Download" on a row writes the `.jsonl` to
  `~/.claude/projects/<local-slug>/<uuid>.jsonl`, adds an entry to that
  project's `sessions-index.json`, writes a green `SessionSyncMetadata`
  entry, and closes the modal. The session appears in the local Sessions
  list immediately.
- Path remapping: when `originalPath` doesn't match any local project
  folder, show a ProjectPickerModal so the user picks the target. The
  choice is persisted in `project_path_mappings.json` and remembered
  for future downloads with the same `originalPath`.
- Conflict handling: if a local `.jsonl` with the same `session_id`
  already exists, compare remote `modified` to local file mtime.
  Newer-remote → "Overwrite?"; newer-local → "Discard remote?";
  within 1s → no prompt.
- No new IPC command shape surprises: every new command has the same
  pattern as the Phase 2 commands (sync-state returns,
  `AppError::GitHubNotConfigured` for not-connected,
  `AppError::GitHubAuthRequired` on 401 → frontend clears connection).

## Non-goals

- Bulk download (whole project / whole repo). One row at a time for v3;
  bulk lands in a later phase once single-row download is rock-solid.
- Sync directionality other than download. Upload is Phase 2, download
  is Phase 3. Two-way sync is a separate, much larger feature.
- "Download as copy" with regenerated UUID. Deferred — flagged as a v3+
  concern in `GITHUB_SYNC_PLAN.md` (line ~387). The internal-id concern
  (transcripts referencing their own session_id) needs empirical
  investigation first.
- Worktree awareness in ProjectPickerModal. V1 just shows flat
  `~/.claude/projects/*/` directories; users pick the main worktree.
- Live refetch of remote list while modal is open. The list is fetched
  once when the modal opens and once when the user clicks "Refresh".
- Live diffing against `messages[]` inside the `.jsonl`. We compare
  RFC3339 `modified` only.
- Retention/pruning. Out of scope until Phase 4.

## Design

### Backend

**1. `src-tauri/src/storage/sessions.rs`** — add `upsert_into_sessions_index`

A new helper that performs a locked, atomic read-modify-write on a
project's `sessions-index.json`. Must:

1. Take the project's absolute folder path (parent of the `.jsonl`) and
   the new entry's `(session_id, full_path, project_path, summary?,
   first_prompt?, message_count, modified, created)`.
2. Read the existing index or create one with `version: 1`.
3. Upsert by `sessionId`. If the entry already exists, preserve
   `originalPath` and any fields the caller didn't pass.
4. Write to a temp file + `fsync` + atomic rename. Use a sidecar lock
   file like `storage/settings.rs` does (existing pattern, no new
   abstraction).
5. Return the updated `SessionIndexFile`.

This is the only way Claude Code learns about the downloaded session; the
Phase 1 scanner reads `sessions-index.json` exclusively.

**2. `src-tauri/src/github/repo.rs`** — add `list_remote_sessions`

A new helper that turns the recursive tree + per-project `metadata.json`
fan-out into `Vec<RemoteSessionSummary>`. The existing
`tree_to_remote_sessions` produces rows with `title/modified/message_count`
all empty/zero; `list_remote_sessions` fills them in by fetching each
project's `metadata.json` blob via `get_blob` and decoding the
`ProjectRemoteMetadata`. One HTTP round-trip per project is acceptable
for v3 — projects are few, blobs are tiny. Add a small
`fetch_project_metadata(token, owner, repo, sha) -> Result<ProjectRemoteMetadata, GitHubError>`
helper to keep this tidy.

`list_remote_sessions` takes the same `slug_for: impl Fn(&str) -> Option<String>`
so it can reuse the existing pattern that decodes the slug via
`project_path_mappings.json` lookup.

**3. `src-tauri/src/commands/github_sync.rs`** — 3 new commands

- **`github_list_remote_sessions_cmd() -> AppResult<Vec<RemoteSessionSummary>>`**
  Loads token + config + path mappings. Calls `get_repo` (404 → return
  empty list with a `repo_not_found` error variant? No — return empty
  list, that's friendlier). Calls
  `get_tree_recursive(token, owner, repo, default_branch)`. For each
  unique `project_slug` in the tree, fetches its `metadata.json` and
  merges fields into the matching `RemoteSessionSummary` entries.
  Returns `Vec<RemoteSessionSummary>` sorted by `(project_slug, modified desc)`.

- **`github_download_session_cmd(session_id, project_slug, blob_sha) -> AppResult<DownloadResult>`**
  Where `DownloadResult` is a small struct
  `{ session_id: String, full_path: String, sync_state: SyncState }`
  so the frontend can refresh just the affected row.

  Flow:
  1. Load token + config + path mappings.
  2. **Resolve target project path:**
     - Look up `mappings[project_slug]` (keyed by slug, not originalPath,
       because the mapping is "remote slug → local project folder").
     - If absent, look up `mappings[originalPath]` and convert to the
       corresponding local folder (we'll switch to slug-keyed in Task 2;
       Phase 3 stores both forms so existing keying still works).
     - If neither: return `AppError::Validation("path_mapping_required")`
       so the frontend opens the ProjectPickerModal.
  3. Fetch the blob with `get_blob(token, owner, repo, blob_sha)`.
  4. **Conflict detection:** if `<target>/<session_id>.jsonl` exists,
     compare remote `metadata.sessions[session_id].modified` to local
     file mtime. Return a typed `AppError::Conflict(...)` with a
     structured `{ kind: "remote_newer" | "local_newer" | "equal" }`
     discriminant so the frontend can show the right dialog without
     re-fetching.
  5. Write the blob bytes to `<target>/<session_id>.jsonl` atomically
     (temp + fsync + rename).
  6. Build a minimal `SessionIndexEntry` (file mtime + created time =
     now; `summary` and `firstPrompt` we don't know — leave None; the
     next `scan_sessions` will fill title from the `.jsonl` itself via
     `extract_title_from_jsonl`).
  7. Call `storage::sessions::upsert_into_sessions_index(target, entry)`.
  8. Write a fresh `SessionSyncMetadata` (Synced, last_uploaded = now,
     remote_sha = blob_sha, last_local_modified = mtime) into
     `session_sync_state.json` for this project folder.
  9. Update `github_sync.json` `last_sync = now`.
  10. Return `DownloadResult`.

  The frontend's `useSessions` hook needs to know the new session
  exists. Either the frontend reloads `useSessions` after success
  (cheap; current pattern) or the backend returns enough info for a
  targeted refresh. We pick reload-on-success for v3 — fewer moving
  parts and the modal closes anyway.

- **`github_resolve_download_target_cmd(project_slug) -> AppResult<Option<String>>`**
  Pure helper for the frontend: given a remote project_slug, returns
  the local target folder if a mapping exists, else `None`. Lets the
  UI show "Already mapped to /home/foo/projects/bar" in the modal
  before the user clicks Download, instead of discovering it inside
  the error path.

**4. Conflict typing**

Add a new variant to `AppError`:

```rust
SessionDownloadConflict {
    kind: SessionConflictKind,
    session_id: String,
}
```

with `SessionConflictKind` enum: `RemoteNewer`, `LocalNewer`. Keep the
"equal" case inside `github_download_session_cmd` — when the timestamps
match we proceed silently, no prompt needed. The frontend maps
`RemoteNewer` → "Overwrite local copy?" / `LocalNewer` → "Discard
remote copy? Or download as copy?". The "download as copy" path is
intentionally not implemented in v3; the dialog tells the user "skip"
as the safe default.

**5. Path-mapping keying (Phase 3 backward-compat)**

`ProjectPathMappings` currently maps `originalPath → localPath`. Phase 3
needs the inverse lookup by slug too. Two options:

- **A) Switch the key to slug.** Breaks existing user files; needs a
  one-shot migration that reads `originalPath` from the matching
  remote `metadata.json` (we already have to fetch that). Avoids
  ambiguity when two remotes share an `originalPath`.
- **B) Keep `originalPath` as key; also store the slug.** Two
  writes per upload, but no migration, and it matches the plan
  ("Path mappings: New `project_path_mappings.json` for cross-machine
  project path resolution").

We pick **B** for v3: add `slug` to `ProjectPathMapping` as an
optional field. The frontend asks "which local folder?" once;
we persist both `originalPath` and `slug` so future downloads
of the same project find the mapping by slug without re-firing
the picker.

### Frontend

**6. `src/components/GitHubSync.tsx` — extend or add `RemoteSessionsModal`**

A modal that lists remote sessions. New file is cleaner than
extending the 511-line `GitHubSync.tsx`. Lives at
`src/components/RemoteSessionsModal.tsx`. Surface:

- Header: "Remote sessions" + close button + "Refresh" button.
- Body: grouped by `projectSlug`. Each group header shows the slug and,
  if known, the decoded `originalPath` (decoded = the mapping we just
  resolved from `project_path_mappings.json` server-side). Each row:
  title (from `RemoteSessionSummary.title`), modified (relative time),
  message count, and a "Download" button.
- If `githubResolveDownloadTargetCmd` returns `None` for the row's
  slug: show a "Pick target…" button that opens
  `ProjectPickerModal`. After the user picks, the row's "Download"
  becomes active.
- Loading + error states (toast on error).

**7. `ProjectPickerModal.tsx`**

A modal with:
- The remote `originalPath` shown at the top (e.g. "Remote project:
  /home/old-laptop/Projects/foo").
- A select with all local `~/.claude/projects/*/` folder names
  (decoded: strip the leading `-` and replace `-` with `/`, with the
  caveat documented in CLAUDE.md about the lossy encoding).
- A "Browse..." button that opens the OS folder picker via
  `tauri-plugin-dialog` (already configured in
  `src-tauri/capabilities/default.json`). Use the existing wrapper
  pattern from `useGlobalPanel` or wherever the settings folder
  picker lives.
- A "Remember this mapping" checkbox, default-on, disabled if the
  user picks the matching local folder (no need to store a self-mapping).
- Confirm/Cancel buttons. Confirm calls
  `githubSetPathMapping({ originalPath, slug, localPath })` (extended
  signature; the existing `github_set_path_mapping_cmd` needs a small
  Task to accept `slug`).

**8. `useRemoteSessions.ts` (new hook)**

Owns the modal's data:
- `sessions: RemoteSessionSummary[]`
- `loading`, `error`
- `refresh()`
- `download(session) → Promise<DownloadResult>`:
  - Calls `githubResolveDownloadTargetCmd(slug)` first.
  - If `None`: opens ProjectPickerModal (controlled by parent).
  - On confirm: `githubSetPathMapping(...)` then retry download.
  - Calls `githubDownloadSessionCmd(...)`.
  - On `SessionDownloadConflict`: opens a confirm dialog
    (use existing `dialog.confirm` from `useGlobalPanel`? — there's a
    runtime error in the previous session about `dialog.confirm not
    allowed`, so use the app's own ConfirmDialog component instead,
    which is what we use elsewhere).
  - On success: refresh `useSessions` (call its refresh function),
    close the modal, toast "Downloaded <title>".
- Returns `{ sessions, loading, error, refresh, download }`.

**9. Wire the entry point**

`GitHubTopBarButton` already shows the GitHub sync state. Extend it
(or add a sibling button) to open `RemoteSessionsModal`. Only visible
when `config.isConnected`. Clicking opens the modal; the modal fetches
its own data on mount.

## Data flow after the change

```
[User clicks "Browse remote"]
  └─► useRemoteSessions.refresh()
        └─► invoke("github_list_remote_sessions_cmd")
              ├─► gh_repo::get_repo (returns owner, default_branch)
              ├─► gh_repo::get_tree_recursive
              └─► For each unique project_slug in tree:
                    └─► gh_repo::get_blob(<metadata.json sha>)
                          └─► decode ProjectRemoteMetadata, merge into rows

[User clicks "Download" on row R]
  └─► useRemoteSessions.download(R)
        ├─► invoke("github_resolve_download_target_cmd", {slug: R.project_slug})
        │     └─► if None → open ProjectPickerModal → githubSetPathMapping → retry
        ├─► invoke("github_download_session_cmd", {session_id, project_slug, blob_sha})
        │     ├─► gh_repo::get_blob(token, owner, repo, blob_sha)
        │     ├─► conflict check (remote vs local mtime)
        │     ├─► atomic write to <target>/<session_id>.jsonl
        │     ├─► storage::sessions::upsert_into_sessions_index(target, entry)
        │     ├─► write SessionSyncMetadata (Synced) to session_sync_state.json
        │     └─► update github_sync.json last_sync
        ├─► on conflict error → ConfirmDialog → re-call with `force: true` flag
        ├─► on success → useSessions.refresh() → toast → close modal
```

## Edge cases

- **No path mapping yet:** the download command returns
  `path_mapping_required`; frontend opens ProjectPickerModal;
  on success the picker writes the mapping AND retries the download.
- **Remote `metadata.json` missing** (corrupt repo, manual edit):
  `list_remote_sessions` falls back to the bare `tree_to_remote_sessions`
  shape — title/modified/message_count all empty. Download still works;
  the local Sessions view fills the title via `extract_title_from_jsonl`.
- **Local `.jsonl` already exists, equal timestamps:**
  `github_download_session_cmd` proceeds silently, overwrites with
  the same content, and the sync metadata stays Synced. No prompt.
- **Local `.jsonl` already exists, remote newer:** command returns
  `SessionDownloadConflict { kind: RemoteNewer }`. Frontend shows
  "Overwrite local copy?" → confirm → retry with `force: true`.
- **Local `.jsonl` already exists, local newer:** command returns
  `SessionDownloadConflict { kind: LocalNewer }`. Frontend shows
  "Local copy is newer. Skip download?" → cancel skips, confirm
  proceeds to overwrite anyway. **v3 deliberately does not support
  "download as copy"** — the dialog says "Cancel to keep local".
- **`session_sync_state.json` doesn't exist for the target project:**
  create it on the fly with `version: 1, sessions: { ... }`.
- **`sessions-index.json` doesn't exist for the target project:**
  `upsert_into_sessions_index` creates it (matches the existing
  pattern in `storage/sessions.rs` where `write` is the canonical
  "create or replace" path).
- **Repo is empty (no `sessions/<slug>/` entries):**
  `github_list_remote_sessions_cmd` returns `[]`. Modal shows
  "No remote sessions yet".
- **Repo doesn't exist at all:** `get_repo` returns `None`. The
  command returns an empty list (friendlier than 404). Modal shows
  "No remote repo. Upload a session first."
- **Cross-machine, same `originalPath`, different slug:** two
  machines with different folder layouts may produce different slugs
  for the same path. The path-mapping key is `originalPath` (Phase 3
  preserves the existing keying), so both resolve to the same local
  folder. **This is a deliberate v3 simplification**; if the user
  has two distinct local folders they want to map to one
  `originalPath`, they can't in v3. Document this as a known limit.
- **Two `originalPath`s that map to the same slug** (lossy encoding
  collision): the mapping is `originalPath → localPath`, so two
  remotes with the same slug but different `originalPath` can each
  declare their own mapping. Slug-keyed resolution looks up by
  `project_slug` first; the second write overrides the first only if
  the slug also matches. Acceptable for v3.

## Out of scope (Phase 3: deliberately deferred)

- Bulk download / "download whole project" / "download all".
- "Download as copy" with regenerated session UUID — needs empirical
  investigation of whether `.jsonl` files reference their own UUID
  internally (CLAUDE.md / hooks / transcript_path fields).
- Worktree-aware ProjectPickerModal.
- Live remote list refresh while the modal is open.
- Retention / repo size awareness.
- Two-way sync (downloaded session edits get pushed back on next
  upload). Already works for sessions uploaded from this machine;
  the question is sessions first downloaded then edited locally.
  For v3 we treat any downloaded session like any other local
  session: it can be re-uploaded on click, and the existing
  OutOfSync detection handles it.

## Verification

- Backend: `cd src-tauri && cargo test` (existing storage tests stay
  green; add unit tests for `upsert_into_sessions_index`,
  `list_remote_sessions` with mocked `get_blob`, and the slug-keyed
  resolver).
- Frontend: `pnpm exec tsc --noEmit && pnpm lint`.
- Manual (`pnpm tauri dev`):
  1. With at least one uploaded session from Phase 2, click
     "Browse remote" → see the row with the real title from
     `metadata.json`.
  2. Click "Download" → session appears in local Sessions list with
     green icon. Re-open app: still green.
  3. Force a `RemoteNewer` conflict: upload the same session from
     another commit, then on this machine delete the local `.jsonl`
     and re-upload with newer mtime. Click Download on the row →
     "Overwrite local copy?" prompt.
  4. Force an unmapped project: pick a remote slug that has no local
     folder → ProjectPickerModal opens → pick a folder → mapping
     persists in `project_path_mappings.json` (verify with
     `cat`).
  5. Empty repo: disconnect, delete the remote repo, reconnect, click
     "Browse remote" → "No remote sessions yet" message.