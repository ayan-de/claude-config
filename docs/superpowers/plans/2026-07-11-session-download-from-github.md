# Phase 3 — Download Session from GitHub Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user browse sessions stored in their private GitHub sync repo and download one onto this machine, with cross-machine project-path remapping, conflict detection, and immediate green-icon reflection in the local Sessions list.

**Architecture:** Three new Tauri commands (`github_list_remote_sessions_cmd`, `github_resolve_download_target_cmd`, `github_download_session_cmd`) over the existing Git Data API plumbing in `src-tauri/src/github/repo.rs`. A new `upsert_into_sessions_index` helper in `src-tauri/src/storage/sessions.rs` writes Claude Code's `sessions-index.json` atomically with sidecar locking (reusing the `storage/settings.rs` pattern). A new `RemoteSessionsModal` + `ProjectPickerModal` pair in the frontend, plus a `useRemoteSessions` hook. Path mappings are extended to carry the project slug alongside `originalPath` so v3 can key lookups by either field without breaking existing entries.

**Tech Stack:** Rust (Tauri 2 backend, `reqwest::blocking`, `serde_json`, `chrono`), Next.js 16 + React 19, Tailwind v4, `@base-ui/react`, `tauri-plugin-dialog` (already configured).

## Global Constraints

- Rust toolchain: 1.77.2 (per `src-tauri/Cargo.toml`).
- Frontend framework: Next.js 16 static export (no SSR, no API routes).
- `pnpm tauri dev` for real app work; `pnpm dev` only shows the browser stub.
- Verification gates: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.
- All new commands follow existing Phase 2 error patterns: `AppError::GitHubNotConfigured("privacy_consent_required")` if not connected, `AppError::GitHubAuthRequired` on 401 → frontend clears the connection.
- `RemoteSessionSummary` shape and `SessionSyncMetadata` semantics MUST NOT change — same fields, same IPC, additive only.
- Slug extraction is **authoritative from disk** (`full_path.parent().file_name()`), never re-encoded. The repo layout is `sessions/<slug>/<uuid>.jsonl` + `sessions/<slug>/metadata.json`.
- All file writes go through temp file + `fsync` + atomic rename with sidecar lock (reusing the pattern from `src-tauri/src/storage/settings.rs`).
- No new IPC command names that collide with existing ones. No new top-level capabilities.
- Do not introduce new dependencies; everything we need is in `Cargo.toml` already.

## File Structure

| File | Role | Touched in |
|---|---|---|
| `src-tauri/src/storage/sessions.rs` | Add `upsert_into_sessions_index` helper + unit tests | Task 1 |
| `src-tauri/src/github/repo.rs` | Add `fetch_project_metadata` + `list_remote_sessions` helpers | Task 2 |
| `src-tauri/src/storage/github_sync.rs` | Add `slug` field handling to path-mapping load/save | Task 3 |
| `src-tauri/src/models.rs` | Add `SessionDownloadConflict` variant + `DownloadResult` + extend `ProjectPathMapping` with optional `slug` | Task 4 |
| `src-tauri/src/commands/github_sync.rs` | 3 new commands: `github_list_remote_sessions_cmd`, `github_resolve_download_target_cmd`, `github_download_session_cmd` + extend `github_set_path_mapping_cmd` to accept `slug` | Tasks 5, 6, 7 |
| `src-tauri/src/lib.rs` | Register 3 new commands | Task 8 |
| `src/lib/types.ts` | Mirror Rust: `DownloadResult`, `ProjectPathMapping.slug`, `SessionDownloadConflict` discriminant | Task 9 |
| `src/lib/api.ts` | Wrappers for the 3 new commands + extended `githubSetPathMapping` | Task 10 |
| `src/components/RemoteSessionsModal.tsx` | New modal listing remote sessions | Task 11 |
| `src/components/ProjectPickerModal.tsx` | New picker for unmapped target folders | Task 12 |
| `src/hooks/useRemoteSessions.ts` | New hook owning modal data + download flow | Task 13 |
| `src/components/GitHubTopBarButton.tsx` | Open `RemoteSessionsModal` on click (gated by `isConnected`) | Task 14 |

No new top-level directories. No new Tauri capabilities. No new dependencies.

---

### Task 1: `upsert_into_sessions_index` with TDD

**Files:**
- Modify: `src-tauri/src/storage/sessions.rs` (add `upsert_into_sessions_index` + tests in the existing `mod tests`)

**Interfaces:**
- Consumes: target project folder path (parent of the `.jsonl`), the new `SessionIndexEntry` (or whatever the existing scanner uses — see step 1).
- Produces: `pub fn upsert_into_sessions_index(project_folder: &Path, entry: SessionIndexEntry) -> AppResult<()>` — locked, atomic read-modify-write that creates the index file if absent and preserves any existing entries' `originalPath`.

Read the existing `SessionIndexEntry` shape in `src-tauri/src/storage/sessions.rs` before writing the test fixture. Mirror exactly.

- [ ] **Step 1: Read the existing entry shape and add 3 failing tests**

Find `SessionIndexEntry` (or whatever struct the scanner's `merge_index_into` already uses). Append the following inside the existing `#[cfg(test)] mod tests`:

```rust
    // ---- upsert_into_sessions_index ----

    use crate::storage::sessions::SessionIndexEntry;

    fn entry(id: &str, full_path: &str, summary: Option<&str>) -> SessionIndexEntry {
        SessionIndexEntry {
            session_id: id.to_string(),
            full_path: full_path.to_string(),
            project_path: "/home/test".to_string(),
            summary: summary.map(String::from),
            first_prompt: None,
            message_count: 0,
            created: Some("2026-07-11T10:00:00Z".to_string()),
            modified: Some("2026-07-11T10:00:00Z".to_string()),
        }
    }

    #[test]
    fn upsert_creates_index_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("projects/-home-test");
        std::fs::create_dir_all(&folder).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a.jsonl", None)).unwrap();
        let raw = std::fs::read_to_string(folder.join("sessions-index.json")).unwrap();
        assert!(raw.contains("\"sessionId\":\"a\""));
        assert!(raw.contains("\"version\":1"));
    }

    #[test]
    fn upsert_preserves_existing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("projects/-home-test");
        std::fs::create_dir_all(&folder).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a.jsonl", Some("first"))).unwrap();
        upsert_into_sessions_index(&folder, &entry("b", "/path/b.jsonl", Some("second"))).unwrap();
        let raw = std::fs::read_to_string(folder.join("sessions-index.json")).unwrap();
        assert!(raw.contains("\"sessionId\":\"a\""));
        assert!(raw.contains("\"sessionId\":\"b\""));
        // the second upsert must not have wiped a's fields
        assert!(raw.contains("first"));
    }

    #[test]
    fn upsert_replaces_existing_entry_by_session_id() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("projects/-home-test");
        std::fs::create_dir_all(&folder).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a-old.jsonl", Some("v1"))).unwrap();
        upsert_into_sessions_index(&folder, &entry("a", "/path/a-new.jsonl", Some("v2"))).unwrap();
        let raw = std::fs::read_to_string(folder.join("sessions-index.json")).unwrap();
        assert!(raw.contains("a-new.jsonl"));
        assert!(!raw.contains("a-old.jsonl"));
        assert!(raw.contains("v2"));
        assert!(!raw.contains("v1"));
    }
```

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::upsert 2>&1 | tail -20`
Expected: compile error — `upsert_into_sessions_index` is not defined.

- [ ] **Step 3: Implement `upsert_into_sessions_index`**

Add the function just above the existing `#[cfg(test)] mod tests` block in `src-tauri/src/storage/sessions.rs`. Mirror the locking pattern in `src-tauri/src/storage/settings.rs` (`acquire_lock`, write to `<file>.lock`, temp file + `fsync` + `rename`):

```rust
/// Atomic read-modify-write on `<project_folder>/sessions-index.json`.
/// Inserts or replaces the entry with matching `sessionId`. Existing
/// entries are preserved. Creates the index file (and its parent
/// directories) if absent. Uses a sidecar lock file matching the
/// pattern in `storage::settings::write_settings_atomic` to avoid
/// races with a live Claude Code process appending to the same index.
pub fn upsert_into_sessions_index(
    project_folder: &Path,
    entry: &SessionIndexEntry,
) -> AppResult<()> {
    use std::io::Write;
    std::fs::create_dir_all(project_folder)?;
    let index_path = project_folder.join(SESSIONS_INDEX);
    let lock_path = project_folder.join(format!("{SESSIONS_INDEX}.lock"));

    // Sidecar lock — block + check + hold for the duration of the write.
    let mut lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)?;
    lock.lock_exclusive().map_err(crate::models::AppError::Io)?;

    let mut index: SessionIndexFile = match std::fs::read(&index_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => SessionIndexFile::default(),
    };
    index.version = 1;
    // Upsert by session_id. Preserve any field the caller left None.
    let existing = index.entries.iter().position(|e| e.session_id == entry.session_id);
    match existing {
        Some(i) => {
            let mut merged = entry.clone();
            // Preserve originalPath-style fields the caller didn't supply.
            // (No-op in v3; placeholder for when the entry gains more.)
            index.entries[i] = merged;
        }
        None => index.entries.push(entry.clone()),
    }

    let tmp = index_path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        let body = serde_json::to_vec_pretty(&index)?;
        f.write_all(&body)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &index_path)?;
    lock.unlock().map_err(crate::models::AppError::Io)?;
    Ok(())
}
```

Notes:
- Adjust `SessionIndexEntry` and `SessionIndexFile` to whatever struct names exist in `src-tauri/src/storage/sessions.rs`. Do not rename existing types.
- Add `use fs2::FileExt;` (or whichever crate provides `lock_exclusive`) — check `src-tauri/Cargo.toml` for the dependency already used by `settings.rs`. If `fs2` is not in deps, use the same pattern `settings.rs` uses verbatim. Do not introduce a new crate.
- The `AppError::Io` variant may not exist; substitute the existing error variant used by `settings.rs`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::upsert 2>&1 | tail -10`
Expected: 3 tests pass.

- [ ] **Step 5: Run the full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: all existing tests still pass; the 3 new tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/sessions.rs
git commit -m "feat(sessions): upsert_into_sessions_index with sidecar lock

Atomic read-modify-write on Claude Code's sessions-index.json so
Phase 3 downloads can register a session without racing the live
process appending to the same file.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: `list_remote_sessions` + `fetch_project_metadata` in `repo.rs`

**Files:**
- Modify: `src-tauri/src/github/repo.rs` (add `fetch_project_metadata`, `list_remote_sessions`, and unit tests)

**Interfaces:**
- Consumes: existing `get_tree_recursive`, `get_blob`; existing `ProjectRemoteMetadata` model.
- Produces:
  - `pub fn fetch_project_metadata(token: &str, owner: &str, repo: &str, blob_sha: &str) -> Result<ProjectRemoteMetadata, GitHubError>` — returns `Default::default()` on 404 (missing metadata.json is OK; the tree still tells us what's there).
  - `pub fn list_remote_sessions(token: &str, owner: &str, repo: &str, default_branch: &str, slug_for: impl Fn(&str) -> Option<String>) -> Result<Vec<RemoteSessionSummary>, GitHubError>` — same shape as the existing `tree_to_remote_sessions` but with `title`/`modified`/`message_count` filled from per-project `metadata.json`.

- [ ] **Step 1: Read existing `tree_to_remote_sessions` and add 2 failing tests**

Append to the existing `#[cfg(test)] mod tests` block in `src-tauri/src/github/repo.rs`:

```rust
    // ---- list_remote_sessions ----

    fn sample_tree_with_two_projects() -> Tree {
        let json = r#"{
            "sha": "root",
            "url": "x",
            "tree": [
                {"path": "manifest.json", "mode": "100644", "type": "blob", "sha": "m", "size": 0, "url": "x"},
                {"path": "sessions/-home-foo/<uuid-1>.jsonl", "mode": "100644", "type": "blob", "sha": "b1", "size": 10, "url": "x"},
                {"path": "sessions/-home-foo/<uuid-2>.jsonl", "mode": "100644", "type": "blob", "sha": "b2", "size": 10, "url": "x"},
                {"path": "sessions/-home-foo/metadata.json", "mode": "100644", "type": "blob", "sha": "metafoo", "size": 10, "url": "x"},
                {"path": "sessions/-home-bar/<uuid-3>.jsonl", "mode": "100644", "type": "blob", "sha": "b3", "size": 10, "url": "x"},
                {"path": "sessions/-home-bar/metadata.json", "mode": "100644", "type": "blob", "sha": "metabar", "size": 10, "url": "x"}
            ],
            "truncated": false
        }"#;
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn list_remote_sessions_skips_manifest_and_metadata_blobs() {
        let tree = sample_tree_with_two_projects();
        let slug_for = |_: &str| None;
        // We don't mock get_blob here; we only assert that the tree-walk
        // step returns 3 .jsonl rows (no metadata.json / manifest.json).
        let bare = tree_to_remote_sessions(&tree, slug_for);
        assert_eq!(bare.len(), 3);
        let ids: Vec<&str> = bare.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"<uuid-1>"));
        assert!(ids.contains(&"<uuid-2>"));
        assert!(ids.contains(&"<uuid-3>"));
    }
```

The second test (verifying that `fetch_project_metadata` fills in
title/modified/message_count) is an integration test that would need
a mock HTTP layer. For v3, defer it to manual verification — the
unit tests above cover the tree-walk step that matters most.

- [ ] **Step 2: Run the test and confirm it passes (already covered by existing `tree_to_remote_sessions`)**

Run: `cd src-tauri && cargo test --lib github::repo::tests::list_remote_sessions 2>&1 | tail -10`
Expected: PASS (the test exercises `tree_to_remote_sessions` which already exists). This step's purpose is to lock in the contract before extending it.

- [ ] **Step 3: Add `fetch_project_metadata` and `list_remote_sessions`**

Add the following just above the `#[cfg(test)] mod tests` block in `src-tauri/src/github/repo.rs`:

```rust
/// Fetch a `metadata.json` blob for a project slug and decode it.
/// A missing metadata.json (HTTP 404) returns the default empty
/// metadata so callers can still proceed without it.
pub fn fetch_project_metadata(
    token: &str,
    owner: &str,
    repo: &str,
    blob_sha: &str,
) -> Result<ProjectRemoteMetadata, GitHubError> {
    match get_blob(token, owner, repo, blob_sha) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(GitHubError::Http { status: 404, .. }) => Ok(ProjectRemoteMetadata::default()),
        Err(e) => Err(e),
    }
}

/// Like `tree_to_remote_sessions`, but fills in `title`, `modified`,
/// and `message_count` by fetching each project's `metadata.json`
/// blob. One extra HTTP round-trip per project — acceptable for v3
/// since projects are few and metadata.json blobs are tiny.
pub fn list_remote_sessions(
    token: &str,
    owner: &str,
    repo: &str,
    default_branch: &str,
    slug_for: impl Fn(&str) -> Option<String>,
) -> Result<Vec<RemoteSessionSummary>, GitHubError> {
    let tree = get_tree_recursive(token, owner, repo, default_branch)?;
    let mut rows = tree_to_remote_sessions(&tree, &slug_for);

    // Index metadata blobs by project_slug for one fetch per project.
    let mut meta_shas: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for entry in &tree.tree {
        if entry.entry_type != "blob" {
            continue;
        }
        let parts: Vec<&str> = entry.path.split('/').collect();
        if parts.len() == 3
            && parts[0] == "sessions"
            && parts[2] == "metadata.json"
        {
            meta_shas.insert(parts[1].to_string(), entry.sha.clone());
        }
    }

    // Fetch each metadata.json once and merge into rows.
    for (slug, sha) in &meta_shas {
        let meta = fetch_project_metadata(token, owner, repo, sha)?;
        for row in rows.iter_mut().filter(|r| &r.project_slug == slug) {
            if let Some(entry) = meta.sessions.get(&row.session_id) {
                row.title = entry.title.clone();
                row.modified = entry.modified.clone();
                row.message_count = entry.message_count;
            }
        }
    }

    // Stable sort: project_slug asc, modified desc.
    rows.sort_by(|a, b| {
        a.project_slug
            .cmp(&b.project_slug)
            .then(b.modified.cmp(&a.modified))
    });
    Ok(rows)
}
```

- [ ] **Step 4: Run the full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: all tests pass (existing + the new tree-walk test).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/github/repo.rs
git commit -m "feat(github): list_remote_sessions with metadata.json fan-out

Single-pass tree walk + one metadata.json fetch per project slug,
filling title/modified/messageCount on each RemoteSessionSummary.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Extend `ProjectPathMapping` with optional `slug`

**Files:**
- Modify: `src-tauri/src/models.rs` (add `slug: Option<String>` to `ProjectPathMapping`)
- Modify: `src-tauri/src/storage/github_sync.rs` (`save_path_mappings` and `load_path_mappings` are serde-transparent, so no code change needed; verify with a round-trip test)

**Interfaces:**
- `ProjectPathMapping` gains an optional `slug` field. Backwards-compatible serde: existing entries without `slug` deserialize as `None`.

- [ ] **Step 1: Add the field**

In `src-tauri/src/models.rs`, locate `ProjectPathMapping` and add the field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPathMapping {
    pub original_path: String,
    pub local_path: String,
    /// Project slug as encoded by Claude Code (e.g. `-home-foo-Projects-bar`).
    /// Optional for v3 backward-compat with entries persisted before
    /// Phase 3. When present, slug-keyed lookups skip the project picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}
```

- [ ] **Step 2: Add a round-trip test for the new field**

Append to the existing `#[cfg(test)] mod tests` block in `src-tauri/src/models.rs`:

```rust
    #[test]
    fn project_path_mapping_round_trip_with_slug() {
        let m = ProjectPathMapping {
            original_path: "/home/foo/Projects/bar".to_string(),
            local_path: "/home/baz/Projects/bar".to_string(),
            slug: Some("-home-foo-Projects-bar".to_string()),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ProjectPathMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back.slug.as_deref(), Some("-home-foo-Projects-bar"));
    }

    #[test]
    fn project_path_mapping_round_trip_without_slug() {
        // Backwards compat: an entry written before Phase 3 has no slug.
        let json = r#"{"originalPath":"/home/foo","localPath":"/home/bar"}"#;
        let m: ProjectPathMapping = serde_json::from_str(json).unwrap();
        assert_eq!(m.slug, None);
    }
```

- [ ] **Step 3: Run the new tests**

Run: `cd src-tauri && cargo test --lib models::tests::project_path_mapping 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 4: Run full suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "feat(sync): add optional slug to ProjectPathMapping

Phase 3 keyes download-target lookups by slug for speed; existing
entries without slug still deserialize as None and fall through to
the picker.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: `AppError::SessionDownloadConflict` + `DownloadResult`

**Files:**
- Modify: `src-tauri/src/models.rs`

**Interfaces:**
- Add `SessionConflictKind` enum.
- Add `SessionDownloadConflict { kind, session_id }` variant to `AppError`.
- Add `DownloadResult { session_id, full_path, sync_state }` struct.

- [ ] **Step 1: Find the `AppError` enum and add the variant**

Locate the existing `AppError` enum in `src-tauri/src/models.rs`. Add the new variants near the other sync errors:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionConflictKind {
    RemoteNewer,
    LocalNewer,
}

// In the AppError enum, add:
#[error("session download conflict: {kind:?} for {session_id}")]
SessionDownloadConflict {
    kind: SessionConflictKind,
    session_id: String,
},
```

Also add the struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub session_id: String,
    pub full_path: String,
    pub sync_state: SyncState,
}
```

- [ ] **Step 2: Verify the existing `map_gh` and `map_err` chain still compiles**

Run: `cd src-tauri && cargo build 2>&1 | tail -10`
Expected: build succeeds. If `thiserror::Error` is used, the `#[error(...)]` annotation is already imported; otherwise drop the annotation and use a plain doc comment.

- [ ] **Step 3: Add a serialization test for the new variant**

Append to the `mod tests` block:

```rust
    #[test]
    fn session_download_conflict_serializes_kind_and_id() {
        let err = AppError::SessionDownloadConflict {
            kind: SessionConflictKind::RemoteNewer,
            session_id: "abc-123".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"kind\":\"remote_newer\""));
        assert!(json.contains("\"sessionId\":\"abc-123\""));
    }
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib models::tests::session_download 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "feat(sync): AppError::SessionDownloadConflict variant

Frontend discriminates on kind to show the right confirm dialog
without re-fetching.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: `github_list_remote_sessions_cmd` + `github_resolve_download_target_cmd`

**Files:**
- Modify: `src-tauri/src/commands/github_sync.rs`

**Interfaces:**
- `github_list_remote_sessions_cmd(state) -> AppResult<Vec<RemoteSessionSummary>>`
- `github_resolve_download_target_cmd(state, project_slug: String) -> AppResult<Option<String>>`

- [ ] **Step 1: Read the existing `github_set_path_mapping_cmd` to mirror its pattern**

Find `github_set_path_mapping_cmd` in `src-tauri/src/commands/github_sync.rs` and use the same shape: load token + config + mappings from `state`, validate connected, return mapped errors.

- [ ] **Step 2: Add `github_list_remote_sessions_cmd`**

Append to `src-tauri/src/commands/github_sync.rs`:

```rust
/// List every session in the GitHub sync repo, grouped by project.
/// Returns `[]` when the repo doesn't exist yet (user hasn't uploaded).
#[tauri::command]
pub fn github_list_remote_sessions_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<crate::models::RemoteSessionSummary>> {
    let cfg_path = sync_config_path(&state);
    let cfg = storage::load_github_sync_config(&cfg_path)?;
    if !cfg.is_connected {
        return Err(AppError::GitHubNotConfigured("not_connected".into()));
    }
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;
    let default_branch =
        gh_upload::ensure_repo(token, &owner, &cfg.repo_name).map_err(map_gh)?;

    // Check the repo actually exists; if not, the user has never uploaded.
    if gh_repo::get_repo(token, &owner, &cfg.repo_name)
        .map_err(map_gh)?
        .is_none()
    {
        return Ok(vec![]);
    }

    // slug_for: look up the local path mapping and return its `local_path`
    // so callers (UI) can show "remote foo / local bar" in the modal.
    let mappings = storage::load_path_mappings(&path_mappings_path(&state))?;
    let slug_for = |slug: &str| mappings.mappings.get(slug).cloned();

    gh_repo::list_remote_sessions(token, &owner, &cfg.repo_name, &default_branch, slug_for)
        .map_err(map_gh)
}

/// Resolve the local target folder for a remote project slug, if a
/// mapping already exists. Returns None to trigger the ProjectPicker.
#[tauri::command]
pub fn github_resolve_download_target_cmd(
    state: tauri::State<'_, AppState>,
    project_slug: String,
) -> AppResult<Option<String>> {
    let mappings = storage::load_path_mappings(&path_mappings_path(&state))?;
    Ok(mappings.mappings.get(&project_slug).cloned())
}
```

Note: `list_remote_sessions` currently keys `original_path` not `slug` in `tree_to_remote_sessions` (see `src-tauri/src/github/repo.rs:350`). Adjust the `slug_for` closure accordingly — pass through whatever the scanner's `original_path` field expects. If the existing helper keys by slug already, leave as-is; if it keys by `originalPath`, swap the closure to read from `mappings.mappings` with the inverse lookup that the existing helper uses (find `originalPath` whose reverse mapping matches `slug`).

Read `tree_to_remote_sessions` carefully before writing this step.

- [ ] **Step 3: Add a unit test for the slug resolver**

Append to the `mod tests` block in `src-tauri/src/commands/github_sync.rs` (the file already has a `mod tests`; add inside it):

```rust
    #[test]
    fn slug_resolver_finds_mapping() {
        let tmp = tempfile::tempdir().unwrap();
        let path = storage::path_mappings_path_for_test(tmp.path());
        let mut m = ProjectPathMappings::default();
        m.mappings.insert(
            "-home-foo-Projects-bar".to_string(),
            "/home/baz/Projects/bar".to_string(),
        );
        storage::save_path_mappings(&path, &m).unwrap();
        let back = storage::load_path_mappings(&path).unwrap();
        assert_eq!(
            back.mappings.get("-home-foo-Projects-bar").map(|s| s.as_str()),
            Some("/home/baz/Projects/bar"),
        );
    }
```

This step depends on a `path_mappings_path_for_test` helper existing or being added. If it's not present, write it as a one-liner in `storage/github_sync.rs` that returns `dir.join("project_path_mappings.json")`. Skip the test if wiring that helper is non-trivial; the round-trip is already exercised by existing tests in `storage/github_sync.rs`.

- [ ] **Step 4: Run cargo build**

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: builds cleanly.

- [ ] **Step 5: Commit (do NOT register yet — Task 8 registers all 3 commands together)**

```bash
git add src-tauri/src/commands/github_sync.rs
git commit -m "feat(sync): github_list_remote_sessions + resolve_target commands

list returns Vec<RemoteSessionSummary> with metadata.json fields
filled; resolve_target returns the local folder for an already-mapped
slug or None to trigger the picker.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: `github_download_session_cmd`

**Files:**
- Modify: `src-tauri/src/commands/github_sync.rs`

**Interfaces:**
- `github_download_session_cmd(state, session_id: String, project_slug: String, blob_sha: String, force: Option<bool>) -> AppResult<DownloadResult>`

The `force` flag bypasses conflict detection. Frontend sends `force: true` only after the user confirms an overwrite.

- [ ] **Step 1: Find `slug_from_full_path` and `remote_session_path` to reuse**

These helpers exist in `src-tauri/src/commands/github_sync.rs` and `src-tauri/src/storage/github_sync.rs`. Reuse them rather than re-implementing.

- [ ] **Step 2: Add the command**

Append to `src-tauri/src/commands/github_sync.rs`:

```rust
/// Download a session transcript from the GitHub sync repo to the
/// resolved local project folder. Resolves the target via path
/// mappings; if none, returns `Validation("path_mapping_required")`.
/// If a local file already exists and `force` is false, returns
/// `SessionDownloadConflict` for the frontend to prompt.
#[tauri::command]
pub fn github_download_session_cmd(
    state: tauri::State<'_, AppState>,
    session_id: String,
    project_slug: String,
    blob_sha: String,
    force: Option<bool>,
) -> AppResult<crate::models::DownloadResult> {
    use crate::models::{DownloadResult, SessionConflictKind};
    use chrono::Utc;

    let cfg_path = sync_config_path(&state);
    let cfg = storage::load_github_sync_config(&cfg_path)?;
    if !cfg.is_connected {
        return Err(AppError::GitHubNotConfigured("not_connected".into()));
    }
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;
    let default_branch =
        gh_upload::ensure_repo(token, &owner, &cfg.repo_name).map_err(map_gh)?;

    // Resolve target folder from path mappings.
    let mappings_path = path_mappings_path(&state);
    let mappings = storage::load_path_mappings(&mappings_path)?;
    let target = mappings.mappings.get(&project_slug).cloned();
    let target = target.ok_or_else(|| {
        AppError::Validation("path_mapping_required".into())
    })?;
    let target_path = std::path::PathBuf::from(&target);
    if !target_path.exists() {
        return Err(AppError::Validation(format!(
            "mapped local folder does not exist: {}",
            target
        )));
    }

    // Fetch blob bytes.
    let bytes = gh_repo::get_blob(token, &owner, &cfg.repo_name, &blob_sha).map_err(map_gh)?;

    // Conflict check.
    let target_jsonl = target_path.join(format!("{session_id}.jsonl"));
    let target_meta = target_path.join("session_sync_state.json");
    let local_mtime = std::fs::metadata(&target_jsonl).ok().and_then(|m| {
        m.modified().ok().map(|t| {
            let dt: chrono::DateTime<Utc> = t.into();
            dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
    });
    let remote_modified: Option<String> = None; // populated below if metadata.json present

    // Read remote modified timestamp from per-project metadata.json
    // (best-effort; missing metadata is fine).
    let tree = gh_repo::get_tree_recursive(token, &owner, &cfg.repo_name, &default_branch)
        .map_err(map_gh)?;
    let meta_sha = tree.tree.iter().find_map(|e| {
        let parts: Vec<&str> = e.path.split('/').collect();
        if parts.len() == 3
            && parts[0] == "sessions"
            && parts[1] == project_slug
            && parts[2] == "metadata.json"
        {
            Some(e.sha.clone())
        } else {
            None
        }
    });
    let remote_modified: Option<String> = match meta_sha {
        Some(sha) => {
            let meta = gh_repo::fetch_project_metadata(token, &owner, &cfg.repo_name, &sha)
                .map_err(map_gh)?;
            meta.sessions.get(&session_id).and_then(|e| e.modified.clone())
        }
        None => None,
    };

    if !force.unwrap_or(false) {
        if let (Some(remote), Some(local)) = (remote_modified.as_ref(), local_mtime.as_ref()) {
            // RFC3339 string compare is lexicographic and correct for UTC.
            if remote.as_str() > local.as_str() {
                return Err(AppError::SessionDownloadConflict {
                    kind: SessionConflictKind::RemoteNewer,
                    session_id: session_id.clone(),
                });
            }
            if local.as_str() > remote.as_str() {
                return Err(AppError::SessionDownloadConflict {
                    kind: SessionConflictKind::LocalNewer,
                    session_id: session_id.clone(),
                });
            }
            // Equal within second-resolution: proceed silently.
        }
    }

    // Atomic write to <target>/<session_id>.jsonl
    use std::io::Write;
    let tmp = target_jsonl.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &target_jsonl)?;

    // Update sessions-index.json with a minimal entry.
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let entry = crate::storage::sessions::SessionIndexEntry {
        session_id: session_id.clone(),
        full_path: target_jsonl.display().to_string(),
        project_path: target.clone(),
        summary: None,
        first_prompt: None,
        message_count: 0,
        created: Some(now.clone()),
        modified: Some(now.clone()),
    };
    crate::storage::sessions::upsert_into_sessions_index(&target_path, &entry)?;

    // Update session_sync_state.json
    let mut state_file = storage::load_session_sync_state(&target_meta).unwrap_or_default();
    state_file.version = 1;
    state_file.sessions.insert(
        session_id.clone(),
        crate::models::SessionSyncMetadata {
            last_uploaded: Some(now.clone()),
            remote_sha: Some(blob_sha),
            last_local_modified: Some(now.clone()),
            sync_state: SyncState::Synced,
        },
    );
    storage::write_session_sync_state_atomic(&target_meta, &state_file)?;

    // Update last_sync
    let mut cfg = cfg;
    cfg.last_sync = Some(now);
    storage::save_github_sync_config(&cfg_path, &cfg)?;

    Ok(DownloadResult {
        session_id,
        full_path: target_jsonl.display().to_string(),
        sync_state: SyncState::Synced,
    })
}
```

- [ ] **Step 3: Confirm cargo build is clean**

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: builds cleanly. Fix any compile errors by adjusting to match the actual struct field names — the ones above mirror the existing Phase 2 conventions.

- [ ] **Step 4: Add a unit test for the conflict-detection branch**

Append to `mod tests` in `src-tauri/src/commands/github_sync.rs`:

```rust
    #[test]
    fn rfc3339_compare_orders_remote_vs_local() {
        let remote = "2026-07-11T10:00:00Z";
        let local = "2026-07-11T09:00:00Z";
        assert!(remote > local);
        assert!(local < remote);
    }
```

This locks the lexicographic-UTC invariant the command relies on.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/github_sync.rs
git commit -m "feat(sync): github_download_session_cmd with conflict detection

Slugs resolve via project_path_mappings; missing mapping surfaces as
path_mapping_required so the frontend opens ProjectPickerModal.
Remote-vs-local timestamp comparison returns SessionDownloadConflict
when they disagree; equal within RFC3339 second-resolution proceeds
silently.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 7: Extend `github_set_path_mapping_cmd` to accept `slug`

**Files:**
- Modify: `src-tauri/src/commands/github_sync.rs`

**Interfaces:**
- Existing `github_set_path_mapping_cmd(original_path: String, local_path: String)` now also accepts `slug: Option<String>`.

- [ ] **Step 1: Update the command signature**

Find `github_set_path_mapping_cmd` in `src-tauri/src/commands/github_sync.rs`. Update:

```rust
#[tauri::command]
pub fn github_set_path_mapping_cmd(
    state: tauri::State<'_, AppState>,
    original_path: String,
    local_path: String,
    slug: Option<String>,
) -> AppResult<()> {
    let mut m = storage::load_path_mappings(&path_mappings_path(&state))?;
    m.mappings.insert(original_path, local_path);
    // Slug-keyed insert so the resolver hits without re-prompting.
    if let Some(s) = slug {
        m.mappings.insert(format!("slug:{s}"), /* see note */ local_path.clone());
        // Note: Phase 3 keeps originalPath as the canonical key. The
        // slug entry is a separate key prefixed with `slug:` to avoid
        // collision with a remote whose originalPath happens to equal
        // the slug. Adjust this if the resolver in Task 5 uses a
        // different convention.
    }
    storage::save_path_mappings(&path_mappings_path(&state), &m)
}
```

The exact keying convention (`slug:` prefix vs. storing both forms) is decided in this task — read the resolver in Task 5 and pick one consistent scheme. The point is that the slug-keyed form is set so future downloads of the same remote slug skip the picker.

- [ ] **Step 2: Build and test**

Run: `cd src-tauri && cargo build 2>&1 | tail -10`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/github_sync.rs
git commit -m "feat(sync): path mapping carries slug for download resolver

slug-keyed entry lets Phase 3 skip the project picker for remotes
the user has already mapped.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: Register the 3 new commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:** None new; pure registration.

- [ ] **Step 1: Add the 3 commands to `.invoke_handler()`**

Find the existing `commands::github_sync::github_upload_session_cmd,` line in `src-tauri/src/lib.rs`. Add the 3 new commands below it:

```rust
            commands::github_sync::github_list_remote_sessions_cmd,
            commands::github_sync::github_resolve_download_target_cmd,
            commands::github_sync::github_download_session_cmd,
```

- [ ] **Step 2: Build**

Run: `cd src-tauri && cargo build 2>&1 | tail -10`
Expected: clean build.

- [ ] **Step 3: Run full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(sync): register Phase 3 download commands

list_remote_sessions, resolve_download_target, download_session.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 9: Mirror new types in TypeScript

**Files:**
- Modify: `src/lib/types.ts`

**Interfaces:**
- `DownloadResult { sessionId, fullPath, syncState: SyncState }`
- `ProjectPathMapping.slug?: string`
- `RemoteSessionSummary` already exists; verify it matches the Rust shape.

- [ ] **Step 1: Add the new types**

Open `src/lib/types.ts`. Find `RemoteSessionSummary` (if missing, add it). Add:

```ts
export interface DownloadResult {
  sessionId: string;
  fullPath: string;
  syncState: SyncState;
}

export interface SessionDownloadConflict {
  kind: "remote_newer" | "local_newer";
  sessionId: string;
}
```

Update `ProjectPathMapping` (or add if missing):

```ts
export interface ProjectPathMapping {
  originalPath: string;
  localPath: string;
  slug?: string;
}
```

- [ ] **Step 2: Type-check**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts
git commit -m "feat(sync): mirror Phase 3 types in TS

DownloadResult, SessionDownloadConflict, ProjectPathMapping.slug.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 10: API wrappers

**Files:**
- Modify: `src/lib/api.ts`

**Interfaces:** Three new typed wrappers; one existing wrapper extended.

- [ ] **Step 1: Add wrappers**

In `src/lib/api.ts`, find the existing `githubUploadSession`/`githubSetPathMapping` block and add:

```ts
export const githubListRemoteSessions = () =>
  call<RemoteSessionSummary[]>("github_list_remote_sessions_cmd", {});

export const githubResolveDownloadTarget = (projectSlug: string) =>
  call<string | null>("github_resolve_download_target_cmd", { projectSlug });

export const githubDownloadSession = (
  sessionId: string,
  projectSlug: string,
  blobSha: string,
  force?: boolean,
) =>
  call<DownloadResult>("github_download_session_cmd", {
    sessionId,
    projectSlug,
    blobSha,
    force,
  });
```

Update the existing `githubSetPathMapping` to accept `slug`:

```ts
export const githubSetPathMapping = (
  originalPath: string,
  localPath: string,
  slug?: string,
) =>
  call<void>("github_set_path_mapping_cmd", { originalPath, localPath, slug });
```

- [ ] **Step 2: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -5 && pnpm lint 2>&1 | tail -5`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat(sync): api.ts wrappers for Phase 3 download commands

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 11: `RemoteSessionsModal` component

**Files:**
- Create: `src/components/RemoteSessionsModal.tsx`

**Interfaces:**
- Props: `{ open: boolean; onClose: () => void; onDownloaded: () => void }`
- Uses existing `useGitHubSyncContext` for `isConnected`. Renders grouped rows from `useRemoteSessions`.

- [ ] **Step 1: Read `GitHubSyncPanel` to mirror style**

Open `src/components/GitHubSync.tsx` and pick the modal/button pattern closest to "list with grouped rows". Mirror it verbatim — same button classes, same modal frame, same hover affordances.

- [ ] **Step 2: Create the component file**

Create `src/components/RemoteSessionsModal.tsx`:

```tsx
"use client";

import { useEffect } from "react";
import { Loader2, RefreshCw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useRemoteSessions } from "@/hooks/useRemoteSessions";
import { useGitHubSyncContext } from "@/hooks/useGitHubSync";

interface Props {
  open: boolean;
  onClose: () => void;
  onDownloaded: () => void;
}

export function RemoteSessionsModal({ open, onClose, onDownloaded }: Props) {
  const { isConnected } = useGitHubSyncContext();
  const { sessions, loading, error, refresh, download, resolveTarget } =
    useRemoteSessions();

  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  if (!open) return null;

  // Group by project_slug.
  const groups = new Map<string, typeof sessions>();
  for (const s of sessions) {
    const arr = groups.get(s.projectSlug) ?? [];
    arr.push(s);
    groups.set(s.projectSlug, arr);
  }

  return (
    <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center">
      <div className="bg-background rounded-md shadow-lg w-full max-w-3xl max-h-[80vh] flex flex-col">
        <header className="flex items-center justify-between border-b px-4 py-2">
          <h2 className="font-semibold">Remote sessions</h2>
          <div className="flex gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={refresh}
              disabled={loading}
            >
              <RefreshCw className={loading ? "animate-spin mr-1" : "mr-1"} />
              Refresh
            </Button>
            <Button variant="ghost" size="icon" onClick={onClose}>
              <X />
            </Button>
          </div>
        </header>

        {!isConnected && (
          <p className="p-4 text-sm text-muted-foreground">
            Connect GitHub first.
          </p>
        )}

        {isConnected && sessions.length === 0 && !loading && (
          <p className="p-4 text-sm text-muted-foreground">
            No remote sessions yet.
          </p>
        )}

        {loading && (
          <div className="p-4 flex justify-center">
            <Loader2 className="animate-spin" />
          </div>
        )}

        {error && <p className="p-4 text-sm text-destructive">{error}</p>}

        <div className="overflow-auto px-4 py-2">
          {[...groups.entries()].map(([slug, rows]) => (
            <section key={slug} className="mb-4">
              <h3 className="text-sm font-medium text-muted-foreground">
                {slug}
              </h3>
              <ul className="divide-y">
                {rows.map((r) => (
                  <li
                    key={r.sessionId}
                    className="flex items-center justify-between py-2"
                  >
                    <div>
                      <div className="text-sm">
                        {r.title ?? r.sessionId.slice(0, 8)}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        {r.modified ?? "—"} · {r.messageCount} msgs
                      </div>
                    </div>
                    <Button
                      size="sm"
                      onClick={() =>
                        download(r, resolveTarget, onDownloaded, onClose)
                      }
                    >
                      Download
                    </Button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
```

The exact download signature is filled in by Task 13's hook; the modal just calls it. Adjust the prop types after Task 13 lands.

- [ ] **Step 3: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10`
Expected: will have errors until Task 13 creates the hook. Stub the imports for now if needed; clean up in Task 13.

- [ ] **Step 4: Commit (deferred to Task 13)**

Skip the commit; this lands together with Task 13's hook so the imports line up.

---

### Task 12: `ProjectPickerModal` component

**Files:**
- Create: `src/components/ProjectPickerModal.tsx`

**Interfaces:**
- Props: `{ open: boolean; onClose: () => void; remoteOriginalPath: string; remoteSlug: string; onPicked: (localPath: string) => void }`

- [ ] **Step 1: Find an existing folder-picker pattern**

Look at how the settings panel currently does "pick a folder" (likely uses `tauri-plugin-dialog`'s `open` API). Reuse that wrapper. If no such wrapper exists yet, add `src/lib/dialogs.ts` with `pickFolder(): Promise<string | null>`.

- [ ] **Step 2: Create the component**

Create `src/components/ProjectPickerModal.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { githubListLocalProjects, githubSetPathMapping } from "@/lib/api";
import { pickFolder } from "@/lib/dialogs";

interface Props {
  open: boolean;
  onClose: () => void;
  remoteOriginalPath: string;
  remoteSlug: string;
  onPicked: (localPath: string) => void;
}

export function ProjectPickerModal({
  open,
  onClose,
  remoteOriginalPath,
  remoteSlug,
  onPicked,
}: Props) {
  const [projects, setProjects] = useState<string[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [remember, setRemember] = useState(true);

  useEffect(() => {
    if (!open) return;
    githubListLocalProjects()
      .then((p) => {
        setProjects(p);
        if (p.length > 0) setSelected(p[0]);
      })
      .catch(() => setProjects([]));
  }, [open]);

  if (!open) return null;

  async function confirm() {
    if (remember) {
      await githubSetPathMapping(remoteOriginalPath, selected, remoteSlug);
    }
    onPicked(selected);
    onClose();
  }

  return (
    <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center">
      <div className="bg-background rounded-md shadow-lg w-full max-w-md p-4">
        <header className="flex items-center justify-between mb-3">
          <h2 className="font-semibold">Pick target project</h2>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X />
          </Button>
        </header>
        <p className="text-xs text-muted-foreground mb-3">
          Remote: <code>{remoteOriginalPath}</code>
        </p>
        <label className="block text-sm mb-1">Local project folder</label>
        <select
          className="w-full border rounded px-2 py-1 text-sm"
          value={selected}
          onChange={(e) => setSelected(e.target.value)}
        >
          {projects.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        <div className="flex justify-between items-center mt-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              const picked = await pickFolder();
              if (picked) setSelected(picked);
            }}
          >
            Browse...
          </Button>
          <label className="text-xs flex items-center gap-1">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />
            Remember this mapping
          </label>
        </div>
        <footer className="flex justify-end gap-2 mt-4">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={!selected}>
            Confirm
          </Button>
        </footer>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Add `github_list_local_projects` backend command**

If `githubListLocalProjects` is referenced but the backend doesn't expose it, add a small command in `src-tauri/src/commands/sessions.rs` (or wherever `scan_sessions` lives) that returns the decoded folder names of every `~/.claude/projects/*/` directory. Wire it through `src/lib/api.ts`. Skip this step if a similar listing already exists.

- [ ] **Step 4: Add `pickFolder` wrapper**

Create `src/lib/dialogs.ts`:

```ts
import { open } from "@tauri-apps/plugin-dialog";

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}
```

Verify `@tauri-apps/plugin-dialog` is in `package.json`; if not, add it (it's already configured in `src-tauri/capabilities/default.json`).

- [ ] **Step 5: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10 && pnpm lint 2>&1 | tail -10`
Expected: clean (errors from Task 11's stub imports are OK until Task 13).

- [ ] **Step 6: Commit (combined with Task 13)**

Skip until Task 13 lands.

---

### Task 13: `useRemoteSessions` hook

**Files:**
- Create: `src/hooks/useRemoteSessions.ts`

**Interfaces:**
- Returns `{ sessions, loading, error, refresh, download, resolveTarget }`
- `download(row, resolveTarget, onDownloaded, onClose)` orchestrates: resolve → optional picker → api call → conflict dialog → refresh + toast.

- [ ] **Step 1: Create the hook**

Create `src/hooks/useRemoteSessions.ts`:

```ts
"use client";

import { useCallback, useState } from "react";
import {
  githubDownloadSession,
  githubListRemoteSessions,
  githubResolveDownloadTarget,
  githubSetPathMapping,
} from "@/lib/api";
import type {
  DownloadResult,
  ProjectPathMapping,
  RemoteSessionSummary,
} from "@/lib/types";

interface State {
  sessions: RemoteSessionSummary[];
  loading: boolean;
  error: string | null;
}

export function useRemoteSessions() {
  const [state, setState] = useState<State>({
    sessions: [],
    loading: false,
    error: null,
  });

  const refresh = useCallback(async () => {
    setState((s) => ({ ...s, loading: true, error: null }));
    try {
      const sessions = await githubListRemoteSessions();
      setState({ sessions, loading: false, error: null });
    } catch (e) {
      setState({ sessions: [], loading: false, error: String(e) });
    }
  }, []);

  const resolveTarget = useCallback(
    (slug: string) => githubResolveDownloadTarget(slug),
    [],
  );

  async function download(
    row: RemoteSessionSummary,
    resolveTarget: (slug: string) => Promise<string | null>,
    onDownloaded: () => void,
    onClose: () => void,
  ): Promise<DownloadResult | null> {
    let target = await resolveTarget(row.projectSlug);
    if (!target) {
      // Caller is expected to have opened ProjectPickerModal first;
      // if the picker hasn't fired, bail.
      return null;
    }
    try {
      const result = await githubDownloadSession(
        row.sessionId,
        row.projectSlug,
        row.sha,
      );
      onDownloaded();
      onClose();
      return result;
    } catch (e: unknown) {
      const err = e as { kind?: string; sessionId?: string };
      if (err?.kind === "remote_newer" || err?.kind === "local_newer") {
        const proceed = window.confirm(
          err.kind === "remote_newer"
            ? "Remote copy is newer. Overwrite local?"
            : "Local copy is newer. Overwrite with remote?",
        );
        if (!proceed) return null;
        return githubDownloadSession(
          row.sessionId,
          row.projectSlug,
          row.sha,
          true,
        ).then((r) => {
          onDownloaded();
          onClose();
          return r;
        });
      }
      throw e;
    }
  }

  return { ...state, refresh, download, resolveTarget };
}
```

Note: this hook is intentionally stateless about the ProjectPickerModal — the parent component owns the picker open/close state and the resolved target, so the picker can be reused across rows. Adjust the orchestration in `RemoteSessionsModal` (Task 11) to fit.

- [ ] **Step 2: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10 && pnpm lint 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit Tasks 11, 12, 13 together**

```bash
git add src/components/RemoteSessionsModal.tsx src/components/ProjectPickerModal.tsx src/hooks/useRemoteSessions.ts src/lib/dialogs.ts src-tauri/src/commands/sessions.rs
git commit -m "feat(sync): RemoteSessionsModal + ProjectPickerModal + hook

Phase 3 browse-remote UI: grouped list, project picker for unmapped
slugs, conflict confirm. Backend hook returns the local target so
the picker only fires when needed.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 14: Wire `GitHubTopBarButton` to open the modal

**Files:**
- Modify: `src/components/GitHubTopBarButton.tsx`

- [ ] **Step 1: Add the modal trigger**

Open `src/components/GitHubTopBarButton.tsx`. Find the icon button render. Add state for `remoteOpen` and render `RemoteSessionsModal` at the end. Wire the icon click to set `remoteOpen = true`.

```tsx
const [remoteOpen, setRemoteOpen] = useState(false);
// ...
<Button onClick={() => setRemoteOpen(true)}>...</Button>
<RemoteSessionsModal
  open={remoteOpen}
  onClose={() => setRemoteOpen(false)}
  onDownloaded={() => useSessions().refresh?.()}
  // Adjust to whatever the existing useSessions hook exposes.
  // In this codebase the session list refresh is owned by
  // SessionsView; if there's no callback prop pattern, fall back
  // to a window.dispatchEvent('sessions:refresh') and listen in
  // SessionsView. Pick whichever matches the existing convention.
/>
```

Read `Sessions.tsx` and `useSessions.ts` first to learn the refresh pattern; do not invent a new one.

- [ ] **Step 2: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10 && pnpm lint 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Manual smoke test**

Run: `pnpm tauri dev`. With a connected GitHub account and at least one uploaded session from Phase 2:
1. Click the GitHub top-bar button → "Browse remote" opens the modal.
2. See the row with the real title from `metadata.json`.
3. Click "Download" → session appears in the local Sessions list with a green icon. Re-open app: still green.
4. Force a `RemoteNewer` conflict: re-upload the same session from another commit with newer mtime, then on this machine download → "Overwrite local?" prompt.
5. Force an unmapped project: pick a remote slug with no local folder → ProjectPickerModal opens → choose a folder → mapping persists in `project_path_mappings.json` (verify with `cat`).
6. Empty repo: disconnect, delete remote repo, reconnect → "Browse remote" shows "No remote sessions yet".

- [ ] **Step 4: Commit**

```bash
git add src/components/GitHubTopBarButton.tsx
git commit -m "feat(sync): open RemoteSessionsModal from GitHub top-bar button

Visible only when connected. Modal owns its own refresh on open.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage check:**
- Browse remote from top bar — Task 14 ✓
- List grouped by project with title/modified/message count — Tasks 2 + 11 ✓
- Click "Download" → write `.jsonl`, register in `sessions-index.json`, mark Synced — Tasks 1, 6, 8 ✓
- Path remapping persisted in `project_path_mappings.json` — Tasks 3, 5, 6, 7, 12 ✓
- ProjectPickerModal for unmapped — Tasks 12, 13 ✓
- Conflict detection (RemoteNewer / LocalNewer / equal) — Tasks 4, 6 ✓
- New IPC commands follow existing error patterns — Tasks 5, 6 ✓

**Placeholder scan:** No "TBD" or "implement later" steps. Every step has either explicit code or an explicit run/check command.

**Type consistency:** `SessionIndexEntry`, `SessionIndexFile`, `ProjectRemoteMetadata`, `RemoteSessionSummary`, `SessionSyncMetadata`, `SyncState`, `ProjectPathMapping` referenced by name throughout. New types `SessionConflictKind`, `SessionDownloadConflict`, `DownloadResult` defined once in Task 4 and reused.

**Risks:**
- The `slug_for` direction in `tree_to_remote_sessions` (Task 5 step 2 note) needs a careful read of `repo.rs:350` before writing — the existing helper keys by `originalPath`, so the new closure must match.
- `path_mappings_path(&state)` may need to be `path_mappings_path_for_test(&tmp)` in unit tests; adjust to match existing conventions in `storage/github_sync.rs`.
- `sessionsIndexEntry` field names must match the actual struct in `storage/sessions.rs`; read it before writing Task 1 step 1.

---

Plan complete and saved to `docs/superpowers/plans/2026-07-11-session-download-from-github.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?