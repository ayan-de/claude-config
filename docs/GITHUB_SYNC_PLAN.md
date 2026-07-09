# GitHub Session Sync Implementation Plan

## Context

Users need to sync Claude Code session transcripts across multiple machines. Currently, sessions live only at `~/.claude/projects/<encoded-project-path>/<session-id>.jsonl` on the local filesystem. This feature adds GitHub-based sync: OAuth device flow authentication, per-session upload to a private GitHub repo, and download with project path remapping for cross-machine compatibility.

The GitHub icon placeholder already exists in `SessionRow` (commit 1269bfd) — this feature wires it to working upload/download functionality.

## Architecture Decisions

### 1. Separate GitHub Auth (Not a Provider)

GitHub auth will be stored separately from the existing `Provider` system. Providers are for Claude API authentication (Subscription, Console, Bedrock, etc.); GitHub is a sync/backup service orthogonal to API access. 

**Storage:**
- Secrets: OS keyring under service `"claude-config"`, account `"github_sync"`
- Metadata: New `github_sync.json` in app data directory
- Path mappings: New `project_path_mappings.json` for cross-machine project path resolution

### 2. GitHub Repo Structure

```
claude-sessions/
├── manifest.json              # Central index: project slugs → metadata
└── sessions/
    ├── home-ayan-de-Projects-claude-config/
    │   ├── <uuid-1>.jsonl
    │   ├── <uuid-2>.jsonl
    │   └── metadata.json      # Per-project metadata
    └── home-ayan-de-Projects-foo/
        ├── <uuid-3>.jsonl
        └── metadata.json
```

**Rationale:**
- Project folders prevent name collisions
- Central manifest enables quick project discovery
- Per-project metadata preserves original paths for remapping
- Slug encoding reuses Claude Code's pattern (replace all non-alphanumeric chars with `-`)

**CRITICAL:** The slug encoding must match Claude Code's actual algorithm: replace ALL non-alphanumeric characters (not just `/`) with `-`. Paths like `/home/ayan.de/my_project` become `-home-ayan-de-my_project`. This encoding is lossy and non-unique (`ayan_de` and `ayan-de` collide), which is why `original_path` must always be stored separately in metadata.

### 3. Per-Session Operations (Bulk Later)

Initial implementation: click GitHub icon on a session row → upload that session. Bulk operations (upload/download all sessions for a project) deferred to future work.

### 4. Project Path Remapping

When downloading a session with `project_path: /home/ayan-de/Projects/foo` to a machine where that path doesn't exist:
1. Show modal with dropdown of existing local projects
2. User selects target project OR creates new directory via file picker
3. Store mapping in `project_path_mappings.json` for future downloads
4. Future downloads with same original path use stored mapping automatically

## Data Models

### Rust Additions (src-tauri/src/models.rs)

```rust
// Stored in OS keyring
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
    pub repo_name: String,  // default: "claude-sessions"
    pub last_sync: Option<String>,
}

// Stored in project_path_mappings.json
pub struct ProjectPathMappings {
    pub version: u32,
    pub mappings: HashMap<String, String>,  // original → local
}

pub struct GitHubDeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub struct RemoteSessionSummary {
    pub session_id: String,
    pub project_slug: String,
    pub original_path: String,
    pub title: Option<String>,
    pub modified: Option<String>,
    pub message_count: u32,
    pub sha: String,  // GitHub blob SHA for conflict detection
}
```

### TypeScript Additions (src/lib/types.ts)

Mirror the Rust types with camelCase serialization for all new interfaces.

## Tauri Commands

All in new `src-tauri/src/commands/github_sync.rs`:

1. `get_github_sync_config_cmd() -> AppResult<GitHubSyncConfig>`
2. `github_start_device_flow_cmd() -> AppResult<GitHubDeviceFlowStart>`
3. `github_poll_device_flow_cmd(device_code: String) -> AppResult<String>` (returns username)
4. `github_disconnect_cmd() -> AppResult<()>`
5. `github_upload_session_cmd(session_id, full_path, project_path) -> AppResult<()>`
6. `github_list_remote_sessions_cmd() -> AppResult<Vec<RemoteSessionSummary>>`
7. `github_download_session_cmd(session_id, project_slug, target_project_path) -> AppResult<String>`
8. `github_get_path_mappings_cmd() -> AppResult<Vec<ProjectPathMapping>>`
9. `github_set_path_mapping_cmd(original_path, local_path) -> AppResult<()>`

## GitHub API Integration

### OAuth Device Flow

**Endpoints:**
- `POST https://github.com/login/device/code` (client_id: register new OAuth App)
- `POST https://github.com/login/oauth/access_token` (poll with device_code)
- `GET https://api.github.com/user` (fetch username after token granted)

**Flow:**
1. Backend initiates device flow, returns `device_code`, `user_code`, `verification_uri`
2. Frontend opens `verification_uri` in browser via `shell.open`
3. Frontend polls `github_poll_device_flow_cmd()` every `interval` seconds
4. On success, backend stores `access_token` in keyring and updates `github_sync.json`

**OAuth App Registration:** Create at https://github.com/settings/developers
- Name: "Claude Config Session Sync"
- Scopes: `repo` (private repo access)
- Device flow doesn't require callback URL

### Repository Operations (Git Data API)

**Use Git Data API** (Contents API has 1MB GET limit and ~50MB practical PUT limit - real sessions exceed both)

**Key endpoints:**
- `GET /repos/{owner}/{repo}` — Check repo existence, get default_branch
- `POST /user/repos` — Create new private repo (body: `{"name": "claude-sessions", "private": true}`)
- `GET /repos/{owner}/{repo}/git/trees/{tree_sha}?recursive=1` — List all files, get blob SHAs
- `GET /repos/{owner}/{repo}/git/blobs/{sha}` — Get file content (supports up to 100MB)
- `POST /repos/{owner}/{repo}/git/blobs` — Create blob (returns SHA)
- `POST /repos/{owner}/{repo}/git/trees` — Create tree with new blobs
- `POST /repos/{owner}/{repo}/git/commits` — Create commit
- `POST /repos/{owner}/{repo}/git/refs` — Create branch ref (first commit only)
- `PATCH /repos/{owner}/{repo}/git/refs/heads/{branch}` — Update existing branch ref

**Auth header:** `Authorization: Bearer {access_token}`

**Upload flow (Git Data API plumbing):**
1. Check if repo exists via `GET /repos/{owner}/{repo}`
   - If not exists: create with `POST /user/repos` body `{"name": "claude-sessions", "private": true}`
   - Store `default_branch` from repo object (don't hardcode "main")
2. Try to get current commit SHA from `refs/heads/{default_branch}`
   - If ref doesn't exist (first commit): set `is_first_commit = true`, no parent/base_tree
   - If ref exists: get commit SHA, then get its tree SHA for base_tree
3. For each file to upload:
   - Create blob: `POST .../git/blobs` with base64-encoded content
   - Collect returned blob SHA
4. Create tree: `POST .../git/trees`
   - First commit: no `base_tree` parameter, full file tree from scratch
   - Subsequent commits: include `base_tree` SHA + only changed blobs
5. Create commit: `POST .../git/commits` with new tree SHA
   - First commit: no `parents` parameter
   - Subsequent commits: `parents: [parent_commit_sha]`
6. Update/create ref:
   - First commit: `POST .../git/refs` with `{"ref": "refs/heads/{default_branch}", "sha": new_commit_sha}`
   - Subsequent commits: `PATCH .../git/refs/heads/{default_branch}` with `{"sha": new_commit_sha}`

**Download flow:**
1. Get tree: `GET .../git/trees/{default_branch}?recursive=1` to find blob SHA for target path
2. Get blob: `GET .../git/blobs/{sha}` (returns base64 content, no size limit up to 100MB)
3. Decode base64 content
4. Check if project path mapping exists; prompt user if not
5. Write to `~/.claude/projects/{encoded_local_path}/{session_id}.jsonl`
6. Add to local `sessions-index.json` with file locking to avoid race with live Claude Code sessions

## UI Components

### New Components (src/components/GitHubSync.tsx)

**GitHubSyncPanel** — Settings panel for GitHub connection
- Connection status (username or "Not connected")
- "Connect GitHub" / "Disconnect" buttons
- Repository name input (default: "claude-sessions")
- "View remote sessions" button

**GitHubDeviceFlowModal** — OAuth flow UI
- Display `user_code` prominently
- "Open GitHub" button → opens verification URL
- Spinner with auto-polling every `interval` seconds
- Success message with username on completion

**RemoteSessionsModal** — Browse GitHub sessions
- List grouped by project slug
- Shows title, modified timestamp, message count per session
- "Download" button per session → triggers ProjectPickerModal if needed

**ProjectPickerModal** — Select target project for download
- Dropdown with existing projects from `~/.claude/projects/`
- "Create new project" option → file picker
- "Remember this mapping" checkbox (default enabled)
- Shows original path for context

### Modified Components

**src/components/Sessions.tsx:**
- Wire GitHub icon click handler in `SessionRow` (lines 418-425)
- Call `github_upload_session_cmd()` with session metadata
- Show upload state: spinner while uploading, checkmark on success
- Add "GitHub" button in `SessionsView` header (opens RemoteSessionsModal, visible only when connected)

**src/components/SettingsMenu.tsx:**
- Add "GitHub Sync" menu item (opens GitHubSyncPanel)

## Files to Create

### Rust (Backend)
- `src-tauri/src/commands/github_sync.rs` — All 9 commands
- `src-tauri/src/storage/github_sync.rs` — Load/save github_sync.json, project_path_mappings.json
- `src-tauri/src/github/mod.rs` — GitHub API client module
- `src-tauri/src/github/device_flow.rs` — Device flow implementation
- `src-tauri/src/github/repo.rs` — Repo operations (upload/download/list)

### TypeScript (Frontend)
- `src/components/GitHubSync.tsx` — All GitHub sync UI components
- `src/hooks/useGitHubSync.ts` — React hooks for GitHub sync state
- `src/lib/github.ts` — Device flow polling helper

## Files to Modify

### Rust
- `src-tauri/src/commands/mod.rs` — Add `pub mod github_sync;`
- `src-tauri/src/lib.rs` — Register 9 new commands in `.invoke_handler()`
- `src-tauri/src/models.rs` — Add 5 new structs (GitHubAuthSecret, etc.)
- `src-tauri/src/storage/mod.rs` — Add `pub mod github_sync;`
- `src-tauri/src/storage/keyring.rs` — Add `const GITHUB_SYNC_ACCOUNT: &str = "github_sync";`

### TypeScript
- `src/lib/api.ts` — Add 9 typed wrappers for new commands
- `src/lib/types.ts` — Add 5 new TypeScript interfaces
- `src/components/Sessions.tsx` — Wire GitHub icon, add header button
- `src/components/SettingsMenu.tsx` — Add GitHub Sync menu item

### Configuration
- `src-tauri/tauri.conf.json` — Verify `shell.open` permission exists (already in `opener:default`)

## Edge Cases

### Session Conflicts
- Compare `modified` timestamps (remote vs local)
- If remote newer: prompt "Remote version is newer. Overwrite local?"
- If local newer: prompt "Local version is newer. Overwrite with remote?"
- If equal (within 1s): skip silently
- Provide "Download as copy" → generate new UUID

### Unindexed Sessions
- After download, add entry to local `sessions-index.json` immediately
- Refresh UI sessions list
- Show toast: "Session downloaded to {path}"

### Keyring Unavailable
- `github_start_device_flow_cmd()` fails early with `AppError::KeyringUnavailable`
- Show user-friendly error: "OS keyring unavailable. Cannot store GitHub credentials."
- Disable all GitHub sync UI

### Network Failures
- Retry OAuth polling up to 3 times on network errors
- Show "Connection error. Retrying..." in device flow modal
- For upload/download: show error toast with "Retry" button

### Token Expiration (401 Unauthorized)
- On any GitHub API 401: clear keyring token, set `is_connected: false`
- Show notification: "GitHub connection expired. Please reconnect."
- Redirect user to reconnect flow

### Repo Doesn't Exist
- First upload auto-creates repo via `POST /user/repos` with body `{"name": "claude-sessions", "private": true}`
- If creation fails (rare): show instructions to create manually at github.com/new

### Large Sessions
- Check file size before upload: `fs::metadata(path)?.len()`
- If > 95 MB: reject with error "Session too large for GitHub (95 MB+)"
- GitHub blob limit is 100 MB; 5 MB buffer for base64 overhead

### Concurrent Uploads
- Implement upload queue in frontend (one per project at a time)
- Backend concurrency detection: since uploads are atomic commits via Git Data API, the unit of conflict is the branch ref (not individual files)
- **Detection:** After building tree from `base_tree` (step 4) but before `PATCH .../git/refs/heads/{branch}` (step 6), re-fetch the ref
- If ref SHA no longer matches the parent commit you built against: someone else committed in between
- **Recovery:** Refetch new commit SHA and its tree SHA as new `base_tree`, rebuild your tree with new base, retry commit + PATCH
- Max 3 retries per upload, then fail with "Conflict error. Try again."

### Privacy Warning (First Upload)
- **CRITICAL:** Transcripts contain full file contents, command output, and potentially credentials/secrets
- Show explicit consent dialog before first upload: "This session may contain sensitive information (file contents, environment variables, command output). Upload to private GitHub repo?"
- Store "consent given" flag in `github_sync.json` to avoid repeated prompts
- Consider adding "review before upload" option showing file/command summary

### Session Index Corruption (Live Claude Code)
- Claude Code writes to `sessions-index.json` continuously while active
- Implement file locking before any read-modify-write on sessions-index.json
- If lock fails or file modified in last 5 seconds: warn user "Claude Code may be running in this project. Close it before syncing."
- Alternative: defer index update to Claude Code's next scan (just write .jsonl, don't touch index)

### Format Versioning
- Claude Code's `.jsonl` and `sessions-index.json` formats are versioned and can change between releases
- Read format version from sessions-index.json before writing
- If version is unknown/unsupported: fail loudly with error rather than corrupting the index
- Document which Claude Code versions this sync feature was tested against

### Download as Copy (UUID Regeneration)
- **WARNING:** Session JSONL may reference its own session_id internally (metadata, hooks, transcript_path fields)
- Before shipping "download as copy", verify against real transcripts whether internal IDs exist
- If internal references exist: implement full transcript rewrite, not just file rename
- If no internal references: simple file rename with new UUID is sufficient

### Path Mapping Staleness
- Stored project_path_mappings.json entries can go stale if user moves project locally
- Before auto-applying a stored mapping: verify the target directory exists and contains a `.jsonl` session
- If mapping target is invalid: re-prompt user with ProjectPickerModal
- Consider TTL or "last verified" timestamp on mappings

### Git Worktree Relationships
- Claude Code resolves sessions across worktrees of the same repo natively
- ProjectPickerModal currently treats projects as flat, independent directories
- Sessions downloaded to the "wrong" worktree may not appear in Claude Code's native picker
- **Future enhancement:** detect worktrees, show relationship in UI
- **V1 workaround:** note in UI "If this project has worktrees, select the main working directory"

### Retention Strategy (Future)
- Sessions accumulate indefinitely; GitHub recommends staying under ~1GB per repo
- No pruning implemented in v1
- **Future phases:** "Delete remote sessions older than N days", manual cleanup UI
- Consider showing repo size in GitHubSyncPanel

## Verification Strategy

### Manual Testing Checklist
1. OAuth flow: complete device flow, verify token in keyring
2. Upload: click icon, verify session in GitHub repo (private)
3. Download: fetch remote session, select project, verify appears in sessions list
4. Path mapping: download session from `/home/user/foo` to different path, verify mapping persists
5. Conflict: modify session locally and remotely, verify prompt on next sync
6. Disconnect: verify token deleted, UI disabled
7. Network timeout: disconnect network during upload, verify error handling
8. Token revoke: revoke on GitHub, verify next upload clears local state

### Test OAuth Flow
- Start flow → modal displays user code + "Open GitHub" button
- Authorize in browser → modal shows success + username
- Cancel flow → polling stops, no token stored
- Wait for expiration → shows "expired" error

### Test Upload
- First upload → repo created (private), session uploaded, manifests created
- Update session → re-upload, verify GitHub SHA updated
- Large session (create dummy 100MB file) → rejected with clear error

### Test Download
- New project (no mapping) → modal prompts for project selection
- Existing mapping → auto-downloads to mapped path, no prompt
- Conflict (local newer) → dialog offers overwrite/keep local/download as copy
- Unindexed project → session appears in list immediately after download

## Implementation Phases

### Phase 1: OAuth + Connection (3-4 days)
- Implement GitHub device flow backend + frontend
- Store token in keyring
- GitHubSyncPanel UI
- Test: complete flow, see username in settings

### Phase 2: Upload Session (3-4 days)
- Repo creation + file upload via GitHub API
- Wire SessionRow GitHub icon
- Manifest/metadata management
- Test: upload session, verify in GitHub web UI

### Phase 3: Download Session (4-5 days)
- List remote sessions + download
- ProjectPickerModal for path mapping
- Conflict detection (timestamps)
- Test: download to new project, resume session in Claude Code

### Phase 4: Edge Cases + Polish (2-3 days)
- Retry logic, token expiration handling
- Large file detection, concurrent upload queue
- Comprehensive error messages
- Documentation

**Total estimate:** 12-16 days for complete implementation + testing

## Key Reused Patterns

- **Keyring storage:** Reuse existing `keyring.rs` API (set_secret, get_secret, delete_token)
- **Atomic writes:** Reuse `settings.rs` write pattern (temp file + fsync + atomic rename) for github_sync.json
- **HTTP client:** Reuse existing `reqwest::blocking::Client` from tracker.rs
- **IPC wrapper:** Reuse `call<T>(cmd, args)` pattern from api.ts
- **Error serialization:** Reuse `AppError` enum and normalization
- **Shell open:** Reuse `tauri-plugin-opener` for browser URLs
- **Project slug encoding:** Reuse Claude Code's encoding pattern (replace all non-alphanumeric characters with `-`)

## Critical Dependencies

- GitHub OAuth App registration (obtain client_id before implementation)
- `reqwest` crate (already in Cargo.toml)
- `keyring` crate (already in use)
- `tauri-plugin-opener` (already configured)
- `base64` crate (add to Cargo.toml for GitHub API base64 encoding)
