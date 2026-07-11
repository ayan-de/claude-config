# GitHub Session Sync Implementation Plan

> **Status:** Phases 1–3 shipped. Phase 4 (edge-case polish, retention, bulk operations) deferred. See `Implementation Status` at the end.

## Context

Users need to sync Claude Code session transcripts across multiple machines. Sessions live at `~/.claude/projects/<encoded-project-path>/<session-id>.jsonl` on the local filesystem. This feature adds GitHub-based sync: OAuth device flow authentication, per-session upload to a private GitHub repo, and download with project path remapping for cross-machine compatibility.

The GitHub icon placeholder already exists in `SessionRow` — this feature wires it to working upload/download functionality.

## Architecture Decisions

### 1. Separate GitHub Auth (Not a Provider)

GitHub auth is stored separately from the existing `Provider` system. Providers are for Claude API authentication (Subscription, Console, Bedrock, etc.); GitHub is a sync/backup service orthogonal to API access.

**Storage:**
- Secrets: OS keyring under service `"claude-config"`, account `"github_sync"` (stored as `ProviderSecret::Custom { auth_token }` — reuses the existing keyring plumbing without inventing a parallel API)
- Metadata: `<app_data_dir>/github_sync.json`
- Path mappings: `<app_data_dir>/project_path_mappings.json`
- Per-project sync state: `<project_folder>/session_sync_state.json` (lives alongside Claude Code's `sessions-index.json`, so we can write it under a sidecar lock — see `storage/github_sync.rs::write_session_sync_state_atomic`)

### 2. GitHub Repo Structure

```
claude-sessions/
├── manifest.json              # Central index (reserved for future use — not yet read or written)
└── sessions/
    ├── home-ayan-de-Projects-claude-config/
    │   ├── <uuid-1>.jsonl
    │   └── metadata.json      # Per-project metadata: original_path + per-session title/modified
    └── home-ayan-de-Projects-foo/
        ├── <uuid-3>.jsonl
        └── metadata.json
```

**Rationale:**
- Project folders prevent name collisions
- Per-project `metadata.json` preserves `original_path` (for cross-machine remapping) and per-session title/modified (for the Remote tab UI)
- Slug encoding reuses Claude Code's pattern (replace all non-alphanumeric chars with `-`)

**Slug encoding note:** the encoding is lossy and non-unique (`ayan_de` and `ayan-de` collide). `original_path` is always stored separately in `metadata.json`, and the upload pipeline reads the slug straight from the transcript's parent directory (`slug_from_full_path`) — never re-derives it — so round-tripping is byte-exact.

### 3. Per-Session Operations (Bulk Later)

Initial implementation: click GitHub icon on a session row → upload that session. Bulk operations (upload/download all sessions for a project) deferred to future work.

### 4. Project Path Remapping

When downloading a session with `project_path: /home/ayan-de/Projects/foo` to a machine where that path doesn't exist:
1. Show modal with dropdown of existing local projects (`github_list_local_projects_cmd` enumerates `<claude_dir>/projects/*/`)
2. User selects target project OR creates new directory via file picker
3. Store mapping in `project_path_mappings.json` for future downloads
4. Future downloads with same slug use the stored mapping automatically (slug-keyed lookup, see `github_resolve_download_target_cmd`)
5. Mapped folder is re-validated on every download — stale mappings that point at deleted dirs return `Validation("mapped local folder does not exist: ...")`

`project_path_mappings.json` keeps **two** indices to avoid forcing the picker on every download:
- `mappings: HashMap<original_path, local_path>` — for display/edit UI
- `slug_mappings: HashMap<slug, local_path>` — for download resolver

## Data Models

### Rust (`src-tauri/src/models.rs`)

```rust
// Stored in OS keyring as ProviderSecret::Custom { auth_token }
pub struct GitHubAuthSecret {
    pub access_token: String,
    pub username: Option<String>,
    pub created_at: String,
}

// Stored in github_sync.json
pub struct GitHubSyncConfig {
    pub schema_version: u32,
    pub is_connected: bool,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub repo_name: String,                  // default: "claude-sessions"
    pub privacy_consent_given: bool,
    pub last_sync: Option<String>,          // RFC3339
}

// Stored in project_path_mappings.json
pub struct ProjectPathMappings {
    pub version: u32,
    pub mappings: HashMap<String, String>,             // original_path → local
    pub slug_mappings: HashMap<String, String>,        // slug → local (download hot path)
}

pub struct ProjectPathMapping { /* serialized view of one entry */ pub original_path, pub local_path, pub slug }

// Stored in <project_folder>/session_sync_state.json
pub struct SessionSyncStateFile {
    pub version: u32,
    pub sessions: HashMap<String, SessionSyncMetadata>, // session_id → metadata
}

pub struct SessionSyncMetadata {
    pub last_uploaded: Option<String>,        // RFC3339
    pub remote_sha: Option<String>,            // GitHub blob SHA for conflict detection
    pub last_local_modified: Option<String>,   // File mtime at upload time (RFC3339)
    pub sync_state: SyncState,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState { NeverUploaded, Synced, OutOfSync }

// Per-project metadata stored in the repo (committable alongside sessions)
pub struct ProjectRemoteMetadata {
    pub version: u32,
    pub original_path: String,
    pub sessions: HashMap<String, RemoteSessionEntry>,  // session_id → entry
}

pub struct RemoteSessionEntry {
    pub title: Option<String>,
    pub modified: Option<String>,
    pub message_count: u32,
}

// OAuth device flow
pub struct GitHubDeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

// Remote tab summary (built from a recursive tree walk)
pub struct RemoteSessionSummary {
    pub session_id: String,
    pub project_slug: String,
    pub original_path: String,
    pub title: Option<String>,
    pub modified: Option<String>,
    pub message_count: u32,
    pub sha: String,
}

pub enum SessionConflictKind { RemoteNewer, LocalNewer }
pub struct DownloadResult { session_id, full_path, sync_state }

// Cache stats for a future settings UI
pub struct BlobCacheStats { file_count: u64, total_bytes: u64 }
```

### Error Model (`src-tauri/src/models.rs::AppError`)

```rust
pub enum AppError {
    GitHub { status: u16, message: String },         // generic GitHub failure
    GitHubAuthRequired,                              // HTTP 401 — token expired/revoked
    GitHubNotConfigured(String),                     // "not_connected" | "privacy_consent_required"
    SessionDownloadConflict { kind: SessionConflictKind, session_id: String },
    KeyringUnavailable(String),
    Validation(String),
    Internal(String),
    // ... existing variants ...
}
```

The frontend's `AppError` class mirrors these `kind` strings for `instanceof` branching in UI.

### TypeScript (`src/lib/types.ts`)

Mirror the Rust types with `camelCase` serialization.

## Tauri Commands

All in `src-tauri/src/commands/github_sync.rs` (currently **21 commands**, up from the 11 listed in the original plan):

**OAuth / connection (7)**
1. `get_github_sync_config_cmd() -> GitHubSyncConfig`
2. `github_start_device_flow_cmd() -> GitHubDeviceFlowStart`
3. `github_poll_device_flow_cmd(device_code) -> GitHubPollOutcome` (Pending/SlowDown/Denied/Expired/Authorized{username,avatar_url})
4. `github_disconnect_cmd() -> ()` — wipes in-memory cache + on-disk blob cache + keyring token + `is_connected` flag (privacy consent is preserved across reconnects)
5. `github_set_privacy_consent_cmd(given: bool)`
6. `github_set_repo_name_cmd(repo_name: String)` — validates no slashes/spaces/empty
7. `github_open_verification_url_cmd(verification_uri)` — wraps `tauri-plugin-opener`

**Path mappings (3)**
8. `github_get_path_mappings_cmd() -> Vec<ProjectPathMapping>`
9. `github_set_path_mapping_cmd(original_path, local_path, slug?)` — populates both `mappings` and `slug_mappings`
10. `github_remove_path_mapping_cmd(original_path)`

**Repo probe + upload (2)**
11. `github_check_repo_cmd() -> Option<RepoProbeResult { full_name, default_branch }>`
12. `github_upload_session_cmd(session_id, full_path, project_path) -> SessionSyncMetadata` — single atomic commit, refreshes per-project `metadata.json` in the same commit, invalidates `sessions_list` cache

**Sync state (2)**
13. `github_get_session_sync_state_cmd(project_folder) -> SessionSyncStateFile` — reclassifies every entry's `sync_state` against current mtime before returning
14. `github_check_session_sync_status_cmd(session_id, full_path) -> SyncState` — single-row reclassification

**Remote list / download / preview (4)**
15. `github_list_remote_sessions_cmd() -> Vec<RemoteSessionSummary>` — **async**, SHA-gated (see `Caching Layer`)
16. `github_resolve_download_target_cmd(project_slug) -> Option<String>` — slug-keyed lookup; `None` triggers the picker
17. `github_download_session_cmd(session_id, project_slug, blob_sha, force?) -> DownloadResult` — runs conflict check against per-project `metadata.json`, writes via temp + fsync + atomic rename, registers with `sessions-index.json`
18. `github_fetch_remote_transcript_cmd(session_id, blob_sha) -> Vec<SessionMessage>` — preview without disk write; uses `tempfile::NamedTempFile` only because `parse_session_transcript` takes a `&Path`

**Local helper (1)**
19. `github_list_local_projects_cmd() -> Vec<String>` — feeds the ProjectPicker dropdown

**Cache management (2)**
20. `github_invalidate_remote_cache_cmd()` — drops in-memory cache (frontend calls on Shift+Refresh)
21. `github_get_blob_cache_stats_cmd() -> BlobCacheStats` — surfaces size for a future settings UI

## GitHub API Integration

### Shared HTTP Client (`src-tauri/src/github/client.rs`)

- One `reqwest::blocking::Client` per app (`OnceLock`), 5s connect / 15s total timeout
- Token passed per-request (not stored on the client) so `github_disconnect_cmd` doesn't need to rebuild it
- Typed `GitHubError` enum: `Http { status, body }`, `RateLimited { retry_after_secs }` (parsed from `Retry-After`), `Network(String)`, `Parse(String)`
- `parse()` maps 401 → `GitHubAuthRequired` (so frontend can branch on `AppError.kind === "github_auth_required"`), 429 → `RateLimited`, anything else → `Http`
- `b64_encode` / `b64_decode` exported here (GitHub wraps blobs at 60 chars; decoder strips whitespace)

### OAuth Device Flow (`src-tauri/src/github/device_flow.rs`)

**Endpoints (unauthenticated):**
- `POST https://github.com/login/device/code` (client_id from OAuth App)
- `POST https://github.com/login/oauth/access_token` (poll with device_code)

**Flow:**
1. Backend initiates device flow → returns `device_code`, `user_code`, `verification_uri`
2. Frontend opens `verification_uri` via `github_open_verification_url_cmd` (uses `tauri-plugin-opener`)
3. Frontend polls `github_poll_device_flow_cmd()` every `interval` seconds, with `slow_down` doubling the interval
4. On `Authorized`, backend stores `access_token` in keyring as `ProviderSecret::Custom { auth_token }`, writes `github_sync.json`, **seeds the in-memory `GitHubCache.owner`** from the OAuth response so the first tab open after connecting skips the `GET /user` round-trip

### Repository Operations (`src-tauri/src/github/repo.rs` + `upload.rs`)

Uses Git Data API exclusively (Contents API's ~50MB PUT limit doesn't fit real sessions).

**Helpers (`repo.rs`):** `get_repo`, `create_repo`, `get_authenticated_user`, `get_branch_ref_sha`, `get_branch_head`, `get_tree_recursive`, `create_blob`, `create_tree`, `create_commit`, `create_ref`, `update_ref`, `get_blob`, `tree_to_remote_sessions`, `diff_slugs` (per-slug delta between two trees), `list_remote_sessions_with_diff` (diff-aware splice), `fetch_project_metadata` (404-tolerant)

**Upload orchestration (`upload.rs`):**
- `ensure_repo(token, owner, repo_name)` — get-or-create the private repo, returns its `default_branch`
- `fetch_existing_file(token, owner, repo, branch, path)` — for merging into existing `metadata.json` before re-commit
- `upload_files(token, owner, repo_name, message, files)` — single commit that creates/updates any number of files:
  1. Create all blobs upfront (content-addressed, independent of parent)
  2. Loop up to `MAX_ATTEMPTS = 3`:
     - Snapshot current head (`None` ⇒ first commit)
     - Build tree (with `base_tree` on subsequent commits, without on first)
     - Create commit (no `parents` on first commit)
     - Move the ref (`POST .../git/refs` on first commit, `PATCH .../git/refs/heads/{branch}` otherwise)
     - On `422` from either call (race), rebuild against the new head and retry

**Download flow:**
1. Look up `slug_mappings[project_slug]` to get target folder (else `path_mapping_required` triggers picker)
2. Verify target dir exists on disk
3. Recursive tree walk → find `sessions/<slug>/metadata.json` blob SHA → fetch and decode (best-effort: 404 returns `ProjectRemoteMetadata::default()`)
4. Compare remote `modified` (from `metadata.json`) vs local mtime (RFC3339 string compare). Disagreement ⇒ `SessionDownloadConflict { kind: RemoteNewer | LocalNewer, session_id }` unless `force` is true
5. Fetch session blob (cached via `fetch_blob_cached`), write via `NamedTempFile` + fsync + atomic rename to `<target>/<session_id>.jsonl`
6. Register with Claude Code's `sessions-index.json` via `upsert_into_sessions_index`
7. Update per-project `session_sync_state.json` so the row appears green immediately
8. Record `cfg.last_sync` on `github_sync.json`

## Caching Layer (`src-tauri/src/github/cache.rs`)

Two-tier cache, designed to make the Remote tab paint without paying the full tree-walk cost on every open.

### Tier 1 — In-Memory (`GitHubCache` on `AppState`)

`Arc<Mutex<GitHubCache>>`, lives on `AppState`, **all fields are per-token** (cleared on disconnect and on every fresh OAuth login):

| Field | Purpose |
|---|---|
| `owner: Option<String>` | Seeded from OAuth response; skips `GET /user` on first tab open |
| `default_branch: Option<String>` | Cached alongside owner; filled on first list call (can't seed at OAuth time — repo may not exist yet) |
| `commit_sha: Option<String>` | The SHA-gate — `get_branch_ref_sha()` matches this and short-circuits |
| `tree: Option<Tree>` | Last successful recursive tree; used as the "before" side of `diff_slugs` |
| `sessions_list: Option<Vec<RemoteSessionSummary>>` | The spliced result; returned verbatim on SHA-gate hit |

Invalidation triggers:
- `github_disconnect_cmd` → `cache.clear()` (drops every field)
- `github_poll_device_flow_cmd` Authorized branch → `cache.clear()` then seed `owner`
- `github_upload_session_cmd` → clear `commit_sha`, `tree`, `sessions_list` (owner + default_branch still valid)
- `github_invalidate_remote_cache_cmd` → `cache.clear()` (Shift+Refresh escape hatch)

**Not single-flight:** the lock is dropped before any HTTP call so a stuck request can't block every other command. Two concurrent misses both fire the request — acceptable for human-driven tab clicks.

### Tier 2 — On-Disk Blob Cache

`${app_data_dir}/github_cache/blobs/{sha}` — content-addressed by SHA-1 (GitHub blob API is content-addressed, so byte-correct forever).

- **Atomic writes:** `tempfile::NamedTempFile` + `sync_all` + `persist` (matches `write_session_sync_state_atomic` pattern)
- **SHA validation:** accepts only 40-char lowercase hex (rejects uppercase + wrong length; manual scan instead of pulling `regex` into the dep tree)
- **Size cap:** default 200 MB, override with `GITHUB_BLOB_CACHE_MAX_MB` env var. LRU eviction by mtime when over cap
- **Cleared on disconnect** (same invariant: token gone, sensitive data gone)
- **Best-effort writes:** cache misses that successfully fetch from GitHub log a warning on cache-put failure but never fail the parent command. GitHub stays the source of truth

### Tier 3 — localStorage SWR (`useRemoteSessions.ts`)

Frontend tier, independent of the Rust caches:
- Key `remoteSessions:v1`, TTL 24h
- `useState` initial value is seeded from `readCachedSessions()` so the tab paints instantly on remount
- `loading` is `false` when we have cached rows — a background `refresh()` reconciles after mount
- Cold mount with no cache shows the spinner until `refresh()` resolves
- `clearCachedRemoteSessions()` runs on disconnect so the next user doesn't inherit the previous user's list
- **Stale-row guard on download:** if `Date.now() - lastRefreshAt > 60s`, invalidate the in-memory cache and refetch before downloading — catches the "row was deleted upstream" case before we 404 on the download call

## SHA-Gated List Algorithm

`github_list_remote_sessions_cmd` (`commands/github_sync.rs:619`) is the hot path. Cost breakdown:

| Cache state | Network calls |
|---|---|
| Warm cache, ref SHA unchanged | **1 call** (`GET /git/ref/heads/{branch}`) — return cached list verbatim |
| Cold cache or ref SHA shifted | `2 + N` calls: `GET /user` (if owner not cached) + `GET /repos/.../git/ref/heads/{branch}` + `GET /repos/.../git/trees/{branch}?recursive=1` + `GET /repos/.../git/blobs/{sha}` per **changed** slug's `metadata.json` |
| Repo doesn't exist | 2 calls (owner probe + repo probe), returns `[]` |

Per-slug diff (`repo.rs::diff_slugs`): one pass over both trees, returns `{ changed, removed }`. Slugs in `changed` get a fresh `metadata.json` fetch; slugs in `removed` are dropped from the spliced result (otherwise the UI surfaces a row that would 404 on download); unchanged slugs are spliced verbatim from the previous list.

The cache is written back with the **current** tree (not a transformed one), so the next diff has the same source format.

## UI Components

### Layout: Local/Remote Tabs in `SessionsView`

The Sessions tab has a `SessionsTabs` Local/Remote control (`src/components/SessionsTabs.tsx`). The Remote tab is `lazy()`-loaded with a Suspense fallback showing the same spinner the initial-load path uses.

**Why a tab, not a separate panel:** the original plan said the remote UI lives in per-row icons + a sibling modal. In practice, users want to browse remote sessions in the same surface as their local sessions — and the data shapes are similar enough that one list component can render both. The original modal (`RemoteSessionsModal`) still exists as the download surface opened from `RemoteSessionsTab`.

**Why lazy-load:** the Remote tab pulls in `RemoteSessionsList`, `RemoteSessionDetail`, `useRemoteSessions`, and a chunk of `api.ts` for cache invalidation. Keeping it out of the initial bundle means the Local tab (the common case) doesn't pay the parse cost.

### Components

- `SessionsTabs.tsx` — Local/Remote segmented control
- `RemoteSessionsTab.tsx` — Remote pane: toolbar (count + Refresh button) + initial-load spinner / not-connected CTA / cached-data-with-error banner / list / detail. Uses `useRemoteSessions` for state, `ErrorBoundary` for the detail, and a custom `RemoteSessionsError` class with a `cta` field so the boundary's fallback can deep-link the user to GitHub Sync settings on auth-required errors
- `RemoteSessionsList.tsx` — extracted, reused by `RemoteSessionsTab` and `RemoteSessionsModal`
- `RemoteSessionDetail.tsx` — per-row preview pane (lazy-loaded messages via `github_fetch_remote_transcript_cmd`)
- `RemoteSessionsModal.tsx` — sibling download surface still opened from some legacy call sites
- `ProjectPickerModal.tsx` — dropdown of `github_list_local_projects_cmd` results + "create new" file picker + "remember this mapping" checkbox
- `ErrorBoundary.tsx` — reusable boundary with `fallback` render-prop. Used around the Remote tab (catches `RemoteSessionsError` thrown by `RemoteSessionsTab`) and around the detail pane
- `GitHubSync.tsx` — `GitHubSyncPanel` (settings), `GitHubSyncSidebarButton` (sidebar), `GithubIcon` (exported for reuse)
- `GitHubTopBarButton.tsx` — **Both connected (avatar) and disconnected ("Connect") states delegate to the parent's `onClick`, which opens the `github-sync` settings tab.** (Earlier behavior opened the Remote modal from the avatar; flipped because the settings tab is the canonical entry point for managing the connection.)

### Modified Components

- `Sessions.tsx` — wires the Local/Remote tabs, the GitHub sync legend (gray/amber/primary swatches), and the lazy Remote pane. `SessionUploadProvider` lives on the Local side; the Remote side is stateless wrt uploads.
- `Sessions.tsx` row — wires the GitHub icon click for upload, colors per `SyncState`:
  - **Gray** (muted): `NeverUploaded` — tooltip "Upload to GitHub"
  - **Amber**: `OutOfSync` (local mtime != `last_local_modified`) — tooltip "Local changes since upload"
  - **Primary** (green): `Synced` — tooltip "Uploaded {relative_time}"
  - **Spinner**: uploading — not clickable

### Top-bar wiring (`src/app/page.tsx:107`)

`<GitHubTopBarButton>` always opens the `github-sync` panel (no special-case for connected state). The user's title-bar `<button>` pattern (no `Button` component, custom sizing for the avatar) is preserved.

## Sync State Workflow

### On Session Upload
1. Read mtime, check ≤95 MB (GitHub blob limit is 100MB; base64 inflates ~33%)
2. Privacy gate: `cfg.privacy_consent_given` must be true (`AppError::GitHubNotConfigured("privacy_consent_required")` otherwise)
3. Build session blob + refreshed per-project `metadata.json` (merged with existing entries so other sessions are preserved)
4. `upload_files()` → atomic commit on default branch, with 422-retry up to 3× for concurrent ref moves
5. Persist `SessionSyncMetadata { last_uploaded, remote_sha, last_local_modified, sync_state: Synced }` to `session_sync_state.json`
6. Update `cfg.last_sync`
7. Drop `commit_sha`/`tree`/`sessions_list` from `GitHubCache`

### On Sync State Read
`github_get_session_sync_state_cmd` recomputes every entry's `sync_state` against current mtime before returning. Persisted value is only authoritative right after upload; anything downstream must trust the recomputed view. Same pattern in `github_check_session_sync_status_cmd` for single-row lookups.

`classify_sync_state(Some(&meta), current_mtime)` (in `storage/github_sync.rs`):
- `meta.last_local_modified.is_none()` → `NeverUploaded`
- `current_mtime != parsed_meta.last_local_modified` → `OutOfSync`
- else → `Synced`

### On Upload Click (Out of Sync)
No separate confirmation — clicking the amber icon uploads directly. UI changes color as soon as the next `get_session_sync_state_cmd` round-trip lands.

### On Download
1. `useRemoteSessions.download()` checks if `lastRefreshAt > 60s` old → invalidate + refetch
2. Re-check the row still exists with same SHA in the fresh list (catches upstream delete)
3. `github_resolve_download_target_cmd(slug)` → if `None`, surface `path_mapping_required` and the modal opens the picker
4. `github_download_session_cmd(...)` runs the conflict check, writes atomically, registers with `sessions-index.json`
5. On `SessionDownloadConflict`, the hook shows a `window.confirm()` and retries with `force: true` if the user agrees

### Privacy Warning (First Upload)

`GitHubSync.tsx` shows the consent UI before the first upload. Once the user checks "I understand..." and clicks Save, `cfg.privacy_consent_given` becomes true and stays true across reconnects (deliberate: we don't nag).

## Files to Create

### Rust (Backend)
- `src-tauri/src/commands/github_sync.rs` — all 21 commands
- `src-tauri/src/storage/github_sync.rs` — load/save `github_sync.json`, `project_path_mappings.json`, `session_sync_state.json` (atomic), `classify_sync_state`, `project_metadata_path`, `remote_session_path`
- `src-tauri/src/github/mod.rs` — module wiring
- `src-tauri/src/github/client.rs` — shared `reqwest::blocking::Client`, typed `GitHubError`, `b64_encode`/`b64_decode`
- `src-tauri/src/github/device_flow.rs` — start + poll device flow, `DeviceFlowOutcome` enum
- `src-tauri/src/github/repo.rs` — `get_repo`, `create_repo`, `get_tree_recursive`, `create_blob`/`tree`/`commit`/`ref`, `get_blob`, `diff_slugs`, `list_remote_sessions_with_diff`, `fetch_project_metadata`
- `src-tauri/src/github/upload.rs` — `ensure_repo`, `upload_files` (with 422-retry), `UploadFile`/`UploadResult` structs
- `src-tauri/src/github/cache.rs` — `GitHubCache` (in-memory, on `AppState`), `get_cached_blob`/`put_cached_blob`/`clear_blob_cache`/`enforce_blob_cache_size` (on-disk), `BlobCacheStats`

### TypeScript (Frontend)
- `src/components/GitHubSync.tsx` — `GitHubSyncPanel`, `GitHubSyncSidebarButton`, `GithubIcon`
- `src/components/GitHubTopBarButton.tsx` — top-bar avatar/Connect (always opens sync settings)
- `src/components/Sessions.tsx` — Local/Remote tabs, SessionGroup legend, lazy-loaded Remote pane
- `src/components/SessionsTabs.tsx` — Local/Remote segmented control
- `src/components/RemoteSessionsTab.tsx` — Remote pane (toolbar, list, detail, error classification)
- `src/components/RemoteSessionsList.tsx` — extracted row list (reused by tab + modal)
- `src/components/RemoteSessionDetail.tsx` — per-row preview with messages
- `src/components/RemoteSessionsModal.tsx` — sibling download surface (legacy call sites)
- `src/components/ProjectPickerModal.tsx` — local-project dropdown + file picker
- `src/components/ErrorBoundary.tsx` — reusable boundary with `fallback` render-prop
- `src/hooks/useGitHubSync.ts`, `src/hooks/GitHubSyncContext.tsx` — connection state + polling
- `src/hooks/useRemoteSessions.ts` — SHA-gated list + SWR localStorage cache + download + transcript preview
- `src/hooks/useSessionUpload.ts` — per-row upload state, optimistic updates

## Files to Modify

### Rust
- `src-tauri/src/commands/mod.rs` — `pub mod github_sync;`
- `src-tauri/src/lib.rs` — register all 21 commands in `.invoke_handler()`; seed `github_cache` on `AppState` construction
- `src-tauri/src/state.rs` — `pub github_cache: Arc<Mutex<GitHubCache>>` field
- `src-tauri/src/models.rs` — add all data models + `AppError` variants (`GitHub`, `GitHubAuthRequired`, `GitHubNotConfigured`, `SessionDownloadConflict`)
- `src-tauri/src/storage/mod.rs` — `pub mod github_sync;`
- `src-tauri/src/storage/keyring.rs` — `const GITHUB_KEYRING_ACCOUNT: &str = "github_sync";`
- `src-tauri/Cargo.toml` — `base64` (added), `chrono` (added), `tempfile` (added), `reqwest` (already in use), `keyring` (already in use), `tauri-plugin-opener` (already configured)

### TypeScript
- `src/lib/api.ts` — typed wrappers for all 21 commands + `AppError` class with `kind` discriminant
- `src/lib/types.ts` — mirror Rust data models with `camelCase`
- `src/components/Sessions.tsx` — Local/Remote tabs, GitHub legend, lazy Remote pane, row sync-state coloring
- `src/data/globalTabs.ts` — register `github-sync` tab (sidebar button + panel)

### Configuration
- `src-tauri/tauri.conf.json` — `tauri-plugin-opener` for browser URLs

## Edge Cases

### Concurrent Uploads
- Atomic per-commit via Git Data API; unit of conflict is the branch ref
- Detection: rebuild against the new head on 422 from `create_ref` / `update_ref`
- Recovery: max 3 retries (`MAX_ATTEMPTS` in `upload.rs`), then return the captured error

### Privacy Warning (First Upload)
- Consent dialog in `GitHubSync.tsx`; `privacy_consent_given` flag in `github_sync.json`
- Persists across reconnects (deliberate)

### Keyring Unavailable
- `github_poll_device_flow_cmd` and `load_github_token` both check `state.keyring.is_available()` up front
- Returns `AppError::KeyringUnavailable`; UI shows the error via `ErrorBoundary` and offers no CTA (re-enabling keyring is OS-level)

### Token Expiration (401)
- `map_gh` maps 401 → `AppError::GitHubAuthRequired`
- `RemoteSessionsTab.classifyError` shows "GitHub authentication expired." with CTA "Reconnect GitHub" → deep-links to `github-sync` tab
- **Not auto-revoked:** the local token is not cleared on 401 (next successful call would clear it; for now the user reconnects manually)

### Network Failures
- `reqwest` timeouts (5s connect, 15s total) bubble as `GitHubError::Network` → `AppError::Internal`
- Frontend keeps cached rows visible and shows the slim "Showing cached results — ..." banner with a Retry button (instead of throwing into the boundary)

### Repo Doesn't Exist
- `ensure_repo` creates on first use via `POST /user/repos` `{"name": "claude-sessions", "private": true}`
- `get_repo` returning `Ok(None)` from `list_remote_sessions_cmd` short-circuits to `[]`

### Large Sessions
- `MAX_UPLOAD_BYTES = 95 * 1024 * 1024` (5MB buffer for base64 overhead)
- Frontend is told to retry with a smaller session

### Session Conflicts (Download)
- Remote newer → `SessionDownloadConflict { kind: RemoteNewer }` → `window.confirm("Overwrite local?")` → retry with `force: true`
- Local newer → `SessionDownloadConflict { kind: LocalNewer }` → `window.confirm("Overwrite with remote?")` → retry with `force: true`
- Equal (within RFC3339 second precision) → silent download

### Unindexed Sessions
- After download, `upsert_into_sessions_index` registers the new `.jsonl` so Claude Code's own picker sees it
- Local sessions list refreshes via `onDownloaded` callback

### Format Versioning
- Reuses `storage::sessions::parse_session_transcript` for both local and remote files — single source of truth for the JSONL shape

### Path Mapping Staleness
- Stored `slug_mappings` are validated against the filesystem on every download — `mapped local folder does not exist: ...` triggers the picker again
- `mappings` (original_path-keyed) are display-only and not validated

### Git Worktree Relationships
- Same caveat as the original plan: the picker treats projects as flat directories. Future enhancement: detect worktrees.

### Retention Strategy (Future)
- Not implemented; `BlobCacheStats` is exposed for a future settings UI but no eviction UI exists yet

## Implementation Status

| Phase | Status | Notes |
|---|---|---|
| Phase 1 — OAuth + Connection | ✅ Shipped | All 7 commands + `GitHubSyncPanel` working |
| Phase 2 — Upload Session | ✅ Shipped | Single-session upload via row icon, conflict-free per-commit |
| Phase 3 — Download Session | ✅ Shipped | `RemoteSessionsTab` + `ProjectPickerModal` + `github_download_session_cmd` |
| Phase 4 — Edge Cases + Polish | 🟡 Partial | SHA-gate, blob cache, SWR localStorage, Shift+Refresh, error boundaries all done. Still open: retention/pruning, bulk operations, automatic 401-revocation, "download as copy" UUID regeneration |

**Deliberately deferred:**
- Bulk operations (upload/download all sessions per project)
- Repo size warning UI (stats are exposed via `github_get_blob_cache_stats_cmd` and `BlobCacheStats`)
- Git worktree awareness in the picker
- "Download as copy" (UUID regeneration needs transcript-rewrite plumbing)
- Using `manifest.json` — reserved, not yet written or read
- Removing legacy `RemoteSessionsModal` (still referenced from old call sites; harmless)

## Verification Strategy

### Manual Testing Checklist
1. OAuth flow: complete device flow, verify token in keyring
2. Upload: click icon, verify session + per-project `metadata.json` in GitHub repo (private)
3. Download: fetch remote session, select project, verify appears in sessions list with green icon
4. Path mapping: download from `/home/user/foo` to a different path, verify `slug_mappings` persists
5. Conflict: modify session locally and remotely, verify `window.confirm` prompt
6. Disconnect: verify keyring cleared, in-memory cache cleared, blob cache cleared, UI updates
7. Network timeout: disconnect network during upload, verify error handling
8. Token revoke: revoke on GitHub, verify next list call surfaces "GitHub authentication expired." with Reconnect CTA
9. **SHA-gate warm:** open Remote tab, switch tabs, switch back → second open should be 1 API call (verified by toggling DevTools network panel)
10. **Shift+Refresh:** open Remote tab → click Refresh with Shift held → 2+N calls (cache fully invalidated)
11. **Blob cache:** download the same session twice → second call should hit the on-disk cache (no `GET /git/blobs/{sha}` network call)
12. **Blob cache eviction:** write 200 MB of blobs with `GITHUB_BLOB_CACHE_MAX_MB=50` → verify LRU eviction by mtime
13. **SWR localStorage:** load Remote tab → reload the window → should paint instantly with cached rows + show "Refreshing" badge

### Cargo Tests
```bash
cd src-tauri && cargo test
```
Unit tests live next to the code: `cache.rs` (default/clear/seed/SHA validation/roundtrip/eviction), `repo.rs` (diff_slugs, list_remote_sessions_with_diff splice semantics), `client.rs` (b64_decode line-wrap), `upload.rs` (retry behavior with mock), `commands/github_sync.rs` (slug_from_full_path, reclassify_state_file, conflict classification).

## Key Reused Patterns

- **Keyring storage:** Reuses `ProviderSecret::Custom { auth_token }` enum so `keyring.set_secret` / `get_secret` / `delete_token` work unchanged
- **Atomic writes:** `storage::github_sync::write_session_sync_state_atomic` mirrors the temp-file + fsync + rename pattern from `storage::settings.rs`
- **HTTP client:** Shared `OnceLock<Client>` in `client.rs` — connection-pooled across calls
- **IPC wrapper:** `call<T>(cmd, args)` in `api.ts` (typed discriminated unions for `AppError`)
- **Error serialization:** `AppError` enum normalized to `{ kind, ... }` JSON; `AppError` class on the frontend mirrors the `kind` strings
- **Shell open:** `tauri-plugin-opener` for `verification_uri`
- **Project slug encoding:** Read straight from the on-disk parent folder (`slug_from_full_path`) rather than re-deriving — sidesteps the lossy-encoding bug entirely
- **GitHub API base64 wrapping:** `b64_decode` strips whitespace before decoding

## Critical Dependencies

- `reqwest` crate (already in `Cargo.toml`)
- `keyring` crate (already in use)
- `tauri-plugin-opener` (already configured)
- `base64` crate (added for GitHub API encoding)
- `chrono` crate (added for RFC3339 timestamp formatting/comparison)
- `tempfile` crate (added for atomic blob-cache writes + remote-transcript temp file)
- GitHub OAuth App registration (obtain `client_id` before implementation — already done for the working setup)