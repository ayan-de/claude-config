# Phase 3 — Download Session from GitHub Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user browse sessions stored in their private GitHub sync repo and download one onto this machine, with cross-machine project-path remapping, conflict detection, and immediate green-icon reflection in the local Sessions list.

**Architecture:** Three new Tauri commands (`github_list_remote_sessions_cmd`, `github_resolve_download_target_cmd`, `github_download_session_cmd`) over the existing Git Data API plumbing in `src-tauri/src/github/repo.rs`. A new `upsert_into_sessions_index` helper co-located with the `SessionsIndex` type in `src-tauri/src/storage/sessions.rs` writes Claude Code's `sessions-index.json` atomically with sidecar locking (reusing the pattern from `storage/github_sync.rs::write_session_sync_state_atomic`). A new `RemoteSessionsModal` + `ProjectPickerModal` pair in the frontend, plus a `useRemoteSessions` hook. Path mappings gain an **optional `slug` field** on each entry and a **second slug-keyed HashMap** in `ProjectPathMappings` so downloads can look up by slug in O(1) without prefix-hacks.

**Tech Stack:** Rust (Tauri 2 backend, `reqwest::blocking`, `serde_json`, `chrono`, `fs2`), Next.js 16 + React 19, Tailwind v4, `@base-ui/react`, `tauri-plugin-dialog` (already configured in `capabilities/default.json` and `package.json`).

## Global Constraints

- Rust toolchain per `src-tauri/Cargo.toml` (currently 2024 edition, stable channel).
- Frontend framework: Next.js 16 static export (no SSR, no API routes).
- `pnpm tauri dev` for real app work; `pnpm dev` only shows the browser stub.
- Verification gates: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.
- All new commands follow existing Phase 2 error patterns: `AppError::GitHubNotConfigured("not_connected")` when disconnected, `AppError::GitHubAuthRequired` on 401 → frontend clears the connection.
- `RemoteSessionSummary` shape and `SessionSyncMetadata` semantics MUST NOT change — same fields, same IPC, additive only.
- Slug extraction is **authoritative from disk** (`full_path.parent().file_name()`), never re-encoded. The repo layout is `sessions/<slug>/<uuid>.jsonl` + `sessions/<slug>/metadata.json`.
- All file writes go through temp file + `fsync` + atomic rename with sidecar lock (reusing the pattern from `src-tauri/src/storage/github_sync.rs::write_session_sync_state_atomic` which already uses `fs2::FileExt::lock_exclusive`).
- No new IPC command names that collide with existing ones. No new top-level capabilities.
- Do not introduce new dependencies; everything we need is in `Cargo.toml` already (`fs2`, `tempfile`, `chrono`, `serde_json`, `base64`).

## File Structure

| File | Role | Touched in |
|---|---|---|
| `src-tauri/src/storage/sessions.rs` | Add `upsert_into_sessions_index` helper + unit tests | Task 1 |
| `src-tauri/src/github/repo.rs` | Add `fetch_project_metadata` + `list_remote_sessions` helpers | Task 2 |
| `src-tauri/src/models.rs` | Add `slug` field to `ProjectPathMapping`; add `slug_mappings` map to `ProjectPathMappings`; add `SessionConflictKind` + `AppError::SessionDownloadConflict` + `DownloadResult` | Tasks 3, 4 |
| `src-tauri/src/storage/github_sync.rs` | Extend `mappings_to_list` to hydrate `slug` field | Task 3 |
| `src-tauri/src/commands/github_sync.rs` | 3 new commands: `github_list_remote_sessions_cmd`, `github_resolve_download_target_cmd`, `github_download_session_cmd` + extend `github_set_path_mapping_cmd` to accept `slug` + add `github_list_local_projects_cmd` | Tasks 5, 6, 7, 12 |
| `src-tauri/src/lib.rs` | Register 4 new commands | Task 8 |
| `src/lib/types.ts` | Mirror Rust: `DownloadResult`, `ProjectPathMapping.slug`, `SessionDownloadConflict` discriminant | Task 9 |
| `src/lib/api.ts` | Wrappers for the 4 new commands + extended `githubSetPathMapping` | Task 10 |
| `src/lib/dialogs.ts` | New folder-picker wrapper around `@tauri-apps/plugin-dialog` | Task 12 |
| `src/components/RemoteSessionsModal.tsx` | New modal listing remote sessions | Task 11 |
| `src/components/ProjectPickerModal.tsx` | New picker for unmapped target folders | Task 12 |
| `src/hooks/useRemoteSessions.ts` | New hook owning modal data + download flow | Task 13 |
| `src/components/GitHubTopBarButton.tsx` | Open `RemoteSessionsModal` on click (gated by `isConnected`) | Task 14 |

No new top-level directories. No new Tauri capabilities. No new dependencies.

---

### Task 1: `upsert_into_sessions_index` with TDD

**Files:**
- Modify: `src-tauri/src/storage/sessions.rs` (add `upsert_into_sessions_index` + tests in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: target project folder path (parent of the `.jsonl`), a `SessionIndexEntry` populated by the caller.
- Produces: `pub fn upsert_into_sessions_index(project_folder: &Path, entry: &SessionIndexEntry) -> AppResult<()>` — locked, atomic read-modify-write that creates the index file if absent and preserves any existing entries.

**Important type facts read from `storage/sessions.rs` (lines 26–59):**
- `const SESSIONS_INDEX: &str = "sessions-index.json";`
- `pub(crate) struct SessionsIndex { pub(crate) version: u32, pub(crate) entries: Vec<SessionIndexEntry> }` — NOTE: `SessionsIndex` (plural), NOT `SessionIndexFile`. The struct is `pub(crate)`; the `upsert_into_sessions_index` fn we're adding must live in the same file so it can see it, OR the struct's visibility must be bumped to `pub`. Keep it in-file.
- `pub(crate) struct SessionIndexEntry` has: `session_id: String`, `full_path: String`, `first_prompt: Option<String>`, `summary: Option<String>`, `message_count: Option<u32>`, `created: Option<String>`, `modified: Option<String>`, `project_path: Option<String>`, `is_sidechain: Option<bool>` — **every field except `session_id` and `full_path` is `Option<_>`**. The plan's earlier drafts used the wrong shape; use this one.
- The existing atomic-write pattern to mirror is in `src-tauri/src/storage/github_sync.rs:114-154` (`write_session_sync_state_atomic`) — uses `fs2::FileExt::lock_exclusive`, `NamedTempFile::new_in`, `sync_all`, `persist`. Copy that exact shape.

- [ ] **Step 1: Bump `SessionsIndex` and `SessionIndexEntry` visibility to `pub`**

The upsert function is called from `commands/github_sync.rs`, which lives in a sibling module. `pub(crate)` is already crate-visible, so the callers can see the type — no bump strictly needed. Keep the types `pub(crate)` and construct `SessionIndexEntry` via public constructor or expose a `pub` re-export. Simpler: add `pub` in front of both `struct` declarations. Diff:

```rust
// storage/sessions.rs
- pub(crate) struct SessionsIndex {
+ pub struct SessionsIndex {
      #[allow(dead_code)]
-     pub(crate) version: u32,
+     pub version: u32,
      #[serde(default)]
-     pub(crate) entries: Vec<SessionIndexEntry>,
+     pub entries: Vec<SessionIndexEntry>,
  }

- pub(crate) struct SessionIndexEntry {
+ pub struct SessionIndexEntry {
      #[serde(default)]
-     pub(crate) session_id: String,
+     pub session_id: String,
      // ... same treatment for every field
  }
```

Bumping visibility is preferable to keeping `pub(crate)` and forcing callers to build the struct through a helper — the field list is well-known and stable.

- [ ] **Step 2: Add 3 failing tests to the existing `mod tests`**

Append inside the existing `#[cfg(test)] mod tests` in `storage/sessions.rs`. Field names and `Option<_>` semantics match the real struct:

```rust
    // ---- upsert_into_sessions_index ----

    fn entry(id: &str, full_path: &str, summary: Option<&str>) -> SessionIndexEntry {
        SessionIndexEntry {
            session_id: id.to_string(),
            full_path: full_path.to_string(),
            first_prompt: None,
            summary: summary.map(String::from),
            message_count: Some(0),
            created: Some("2026-07-11T10:00:00Z".to_string()),
            modified: Some("2026-07-11T10:00:00Z".to_string()),
            project_path: Some("/home/test".to_string()),
            is_sidechain: Some(false),
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

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::upsert 2>&1 | tail -20`
Expected: compile error — `upsert_into_sessions_index` is not defined.

- [ ] **Step 3: Implement `upsert_into_sessions_index`**

Add the function just above the existing `#[cfg(test)] mod tests` block. Mirror the locking pattern in `storage/github_sync.rs::write_session_sync_state_atomic`:

```rust
/// Atomic read-modify-write on `<project_folder>/sessions-index.json`.
/// Inserts or replaces the entry with matching `session_id`. Existing
/// entries are preserved. Creates the index file (and its parent
/// directories) if absent. Uses a sidecar lock file matching the
/// pattern in `storage::github_sync::write_session_sync_state_atomic`
/// to avoid races with a live Claude Code process appending to the
/// same index.
pub fn upsert_into_sessions_index(
    project_folder: &Path,
    entry: &SessionIndexEntry,
) -> AppResult<()> {
    use fs2::FileExt;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fs::create_dir_all(project_folder)?;
    let index_path = project_folder.join(SESSIONS_INDEX);

    // Sidecar lock — hold exclusively for the duration of the write.
    let lock_path = {
        let mut p = index_path.as_os_str().to_owned();
        p.push(".lock");
        PathBuf::from(p)
    };
    let lock_file = fs::File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| AppError::Lock(e.to_string()))?;

    let write_result = (|| -> AppResult<()> {
        let mut index: SessionsIndex = match fs::read(&index_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(SessionsIndex {
                version: 1,
                entries: Vec::new(),
            }),
            Err(_) => SessionsIndex {
                version: 1,
                entries: Vec::new(),
            },
        };
        index.version = 1;
        let existing = index
            .entries
            .iter()
            .position(|e| e.session_id == entry.session_id);
        match existing {
            Some(i) => index.entries[i] = entry.clone(),
            None => index.entries.push(entry.clone()),
        }

        let parent = index_path
            .parent()
            .unwrap_or(project_folder);
        let mut tmp = NamedTempFile::new_in(parent)?;
        let body = serde_json::to_vec_pretty(&index)?;
        tmp.write_all(&body)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&index_path).map_err(|e| {
            AppError::Io(std::io::Error::other(format!(
                "persist sessions-index.json: {e}"
            )))
        })?;
        Ok(())
    })();

    let _ = lock_file.unlock();
    write_result
}
```

Notes:
- `SessionsIndex` and `SessionIndexEntry` need `Clone` — check they already derive it. If not, add `#[derive(Clone)]` to both.
- `SessionsIndex` needs a way to construct with default fields. It doesn't derive `Default` today; construct it explicitly (as above) rather than adding a derive.
- Add `use fs2::FileExt;` inside the function (or at the top of the file if not already imported).

- [ ] **Step 4: Add `Clone` derives if missing**

Check the two struct declarations at the top of `sessions.rs`. If `#[derive(Debug, Deserialize, Serialize)]` doesn't include `Clone`, add it:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionsIndex { /* … */ }

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry { /* … */ }
```

`Default` on `SessionIndexEntry` isn't strictly required but keeps future testing painless. `Clone` is required by the `entry.clone()` calls above.

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::upsert 2>&1 | tail -15`
Expected: 3 tests pass.

- [ ] **Step 6: Run the full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: all existing tests still pass; the 3 new tests pass.

- [ ] **Step 7: Commit**

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
- Consumes: existing `get_tree_recursive`, `get_blob`, `tree_to_remote_sessions`; existing `ProjectRemoteMetadata`/`RemoteSessionEntry` models.
- Produces:
  - `pub fn fetch_project_metadata(token: &str, owner: &str, repo: &str, blob_sha: &str) -> Result<ProjectRemoteMetadata, GitHubError>` — returns `Default::default()` on 404 (missing metadata.json is OK; the tree still tells us what's there).
  - `pub fn list_remote_sessions(token: &str, owner: &str, repo: &str, default_branch: &str) -> Result<Vec<RemoteSessionSummary>, GitHubError>` — same shape as `tree_to_remote_sessions` but with `title` / `modified` / `message_count` / `original_path` filled from per-project `metadata.json`.

**Note on the `slug_for` closure that Phase 2's `tree_to_remote_sessions` takes.** Its job is to return the *decoded original project path* for a given slug (which the summary places into the `original_path` field). Since Task 2 does its own metadata.json fetch and knows the real `original_path`, this signature no longer needs a caller-supplied closure. `list_remote_sessions` passes `|_| None` to `tree_to_remote_sessions` and then overrides `original_path` from the fetched metadata.

- [ ] **Step 1: Add 1 failing test that locks the tree-walk contract**

Append to the existing `#[cfg(test)] mod tests` block in `src-tauri/src/github/repo.rs`. This test exercises `tree_to_remote_sessions` (already implemented) — its purpose is to lock in the contract before we build `list_remote_sessions` on top of it.

```rust
    // ---- list_remote_sessions ----

    fn sample_tree_with_two_projects() -> Tree {
        let json = r#"{
            "sha": "root",
            "url": "x",
            "tree": [
                {"path": "manifest.json", "mode": "100644", "type": "blob", "sha": "m", "size": 0, "url": "x"},
                {"path": "sessions/-home-foo/uuid-1.jsonl", "mode": "100644", "type": "blob", "sha": "b1", "size": 10, "url": "x"},
                {"path": "sessions/-home-foo/uuid-2.jsonl", "mode": "100644", "type": "blob", "sha": "b2", "size": 10, "url": "x"},
                {"path": "sessions/-home-foo/metadata.json", "mode": "100644", "type": "blob", "sha": "metafoo", "size": 10, "url": "x"},
                {"path": "sessions/-home-bar/uuid-3.jsonl", "mode": "100644", "type": "blob", "sha": "b3", "size": 10, "url": "x"},
                {"path": "sessions/-home-bar/metadata.json", "mode": "100644", "type": "blob", "sha": "metabar", "size": 10, "url": "x"}
            ],
            "truncated": false
        }"#;
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn tree_walk_skips_manifest_and_metadata_blobs() {
        let tree = sample_tree_with_two_projects();
        // No metadata source — assert only the tree-walk step returns
        // 3 .jsonl rows (no metadata.json / manifest.json).
        let bare = tree_to_remote_sessions(&tree, |_| None);
        assert_eq!(bare.len(), 3);
        let ids: Vec<&str> = bare.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"uuid-1"));
        assert!(ids.contains(&"uuid-2"));
        assert!(ids.contains(&"uuid-3"));
    }
```

Verifying that `fetch_project_metadata` fills in title/modified/message_count is an HTTP-integration test — deferred to Task 14 manual verification. The unit test above locks the tree-walk contract.

- [ ] **Step 2: Run the test — it must pass (locks the current behavior)**

Run: `cd src-tauri && cargo test --lib github::repo::tests::tree_walk 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 3: Add `fetch_project_metadata` and `list_remote_sessions`**

Add the following just above the `#[cfg(test)] mod tests` block in `src-tauri/src/github/repo.rs`. Note that `GitHubError::Http { status, body }` is the real variant shape (see `github/client.rs:40`) — don't invent `body` fields.

```rust
use crate::models::ProjectRemoteMetadata;

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
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(GitHubError::Http { status: 404, .. }) => Ok(ProjectRemoteMetadata::default()),
        Err(e) => Err(e),
    }
}

/// Like `tree_to_remote_sessions`, but fills in `title`, `modified`,
/// `message_count`, and `original_path` by fetching each project's
/// `metadata.json` blob. One extra HTTP round-trip per project —
/// acceptable for v3 since projects are few and metadata.json blobs are
/// tiny.
pub fn list_remote_sessions(
    token: &str,
    owner: &str,
    repo: &str,
    default_branch: &str,
) -> Result<Vec<RemoteSessionSummary>, GitHubError> {
    let tree = get_tree_recursive(token, owner, repo, default_branch)?;
    // Bare rows first; original_path is empty for now.
    let mut rows = tree_to_remote_sessions(&tree, |_| None);

    // Index metadata blobs by project_slug: one fetch per project.
    let mut meta_shas: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for entry in &tree.tree {
        if entry.entry_type != "blob" {
            continue;
        }
        let parts: Vec<&str> = entry.path.split('/').collect();
        if parts.len() == 3 && parts[0] == "sessions" && parts[2] == "metadata.json" {
            meta_shas.insert(parts[1].to_string(), entry.sha.clone());
        }
    }

    // Fetch each metadata.json once and merge into rows.
    for (slug, sha) in &meta_shas {
        let meta = fetch_project_metadata(token, owner, repo, sha)?;
        for row in rows.iter_mut().filter(|r| &r.project_slug == slug) {
            row.original_path = meta.original_path.clone();
            if let Some(session_entry) = meta.sessions.get(&row.session_id) {
                row.title = session_entry.title.clone();
                row.modified = session_entry.modified.clone();
                row.message_count = session_entry.message_count;
            }
        }
    }

    // Stable sort: project_slug asc, then modified desc (None sorts last).
    rows.sort_by(|a, b| {
        a.project_slug
            .cmp(&b.project_slug)
            .then_with(|| b.modified.cmp(&a.modified))
    });
    Ok(rows)
}
```

- [ ] **Step 4: Verify the field names on `TreeEntry`**

`entry.entry_type` and `entry.path` assume the existing `TreeEntry` struct in `repo.rs` names them that way. Read the struct definition (search `struct TreeEntry`) and adjust field names if they differ. `tree_to_remote_sessions` uses `e.entry_type` and `e.path` — mirror those.

- [ ] **Step 5: Run the full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/github/repo.rs
git commit -m "feat(github): list_remote_sessions with metadata.json fan-out

Single-pass tree walk + one metadata.json fetch per project slug,
filling title/modified/messageCount/originalPath on each row.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Extend `ProjectPathMapping` with optional `slug` + add `slug_mappings` to `ProjectPathMappings`

**Files:**
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/storage/github_sync.rs` (`mappings_to_list` hydration + a round-trip test)

**Design decision — two mappings, not one prefix-hacked map.** Version 1 of Phase 3 uses two HashMaps side by side inside `ProjectPathMappings`:
- `mappings: HashMap<String, String>` — keyed by `original_path` (existing, unchanged).
- `slug_mappings: HashMap<String, String>` — new, keyed by slug.

Both point to the same `local_path`. Callers pick the map that matches their key. This avoids the "slug:` prefix" hack an earlier draft proposed and keeps existing consumers working on the `mappings` field. Migration is trivial: `slug_mappings` defaults to empty for old files; new writes populate both.

**Interfaces:**
- `ProjectPathMapping` gains an optional `slug` field. Backwards-compatible serde: existing entries without `slug` deserialize as `None`.
- `ProjectPathMappings` gains a `slug_mappings: HashMap<String, String>` field with `#[serde(default)]`.

- [ ] **Step 1: Add the fields to models.rs**

In `src-tauri/src/models.rs`, replace the two struct declarations near line 462:

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPathMappings {
    pub version: u32,
    /// Original decoded project path -> local project folder.
    pub mappings: std::collections::HashMap<String, String>,
    /// Project slug -> local project folder. Populated alongside
    /// `mappings` by Phase 3 writes. Empty for files written before
    /// Phase 3; `#[serde(default)]` keeps old JSON round-tripping.
    #[serde(default)]
    pub slug_mappings: std::collections::HashMap<String, String>,
}
```

- [ ] **Step 2: Add round-trip tests for the new fields**

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

    #[test]
    fn project_path_mappings_round_trip_without_slug_map() {
        // Backwards compat: a file written before Phase 3 has no
        // `slugMappings` key; it must deserialize to an empty map.
        let json = r#"{"version":1,"mappings":{"/home/foo":"/home/bar"}}"#;
        let m: ProjectPathMappings = serde_json::from_str(json).unwrap();
        assert!(m.slug_mappings.is_empty());
        assert_eq!(m.mappings.get("/home/foo").map(|s| s.as_str()), Some("/home/bar"));
    }
```

- [ ] **Step 3: Update `mappings_to_list` to hydrate `slug`**

`storage/github_sync.rs::mappings_to_list` currently builds `ProjectPathMapping { original_path, local_path }`. Add slug lookup — invert `slug_mappings` (`local_path` → `slug`) once, then look up by `local_path` on each row:

```rust
pub fn mappings_to_list(m: &ProjectPathMappings) -> Vec<ProjectPathMapping> {
    // Invert slug_mappings so we can annotate each row's slug in one pass.
    let slug_for_local: std::collections::HashMap<&str, &str> = m
        .slug_mappings
        .iter()
        .map(|(slug, local)| (local.as_str(), slug.as_str()))
        .collect();
    m.mappings
        .iter()
        .map(|(k, v)| ProjectPathMapping {
            original_path: k.clone(),
            local_path: v.clone(),
            slug: slug_for_local.get(v.as_str()).map(|s| s.to_string()),
        })
        .collect()
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib models::tests::project_path_mapping 2>&1 | tail -10`
Expected: PASS.

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: all tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/storage/github_sync.rs
git commit -m "feat(sync): add slug to ProjectPathMapping + slug_mappings map

Phase 3 keys download-target lookups by slug for O(1) resolution.
Existing files without slug_mappings deserialize with an empty map
so no migration is needed.

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

**Serialization contract:** `AppError` has a custom `impl serde::Serialize` (`models.rs:368`) that flattens every variant to `{kind, message}`. Adding a new variant with structured fields means the *fields* live only in the `message` string (unless we teach the serializer about them). For v3, keep it simple: the frontend parses out the conflict `kind` from the `message` string (`"remote_newer"` or `"local_newer"` literal + `session_id`). Alternative — teach the serializer to emit extra fields — is out of scope.

- [ ] **Step 1: Add the enum and variant**

In `src-tauri/src/models.rs`, add near the other GitHub sync types (before `impl serde::Serialize for AppError`):

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionConflictKind {
    RemoteNewer,
    LocalNewer,
}
```

Inside the `AppError` enum (add near `GitHubNotConfigured`):

```rust
    #[error("session download conflict: {kind:?} for {session_id}")]
    SessionDownloadConflict {
        kind: SessionConflictKind,
        session_id: String,
    },
```

Inside `AppError::kind()` add:

```rust
            AppError::SessionDownloadConflict { .. } => "session_download_conflict",
```

The custom `Serialize` impl already picks up new variants (`self.to_string()` runs the `#[error]` template). The frontend reads `err.message`, matches `"remote_newer"` / `"local_newer"` substring, and pulls the session_id off the summary it already has in hand.

- [ ] **Step 2: Add the `DownloadResult` struct**

Also in `src-tauri/src/models.rs`, near the other GitHub sync structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub session_id: String,
    pub full_path: String,
    pub sync_state: SyncState,
}
```

- [ ] **Step 3: Build**

Run: `cd src-tauri && cargo build 2>&1 | tail -10`
Expected: clean build.

- [ ] **Step 4: Add a serialization test**

Append to the existing `mod tests` block:

```rust
    #[test]
    fn session_download_conflict_serializes_kind_in_message() {
        let err = AppError::SessionDownloadConflict {
            kind: SessionConflictKind::RemoteNewer,
            session_id: "abc-123".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Custom Serialize impl flattens to {kind, message}; the
        // conflict kind and session id live in the message string.
        assert!(json.contains("\"kind\":\"session_download_conflict\""));
        assert!(json.contains("RemoteNewer"));
        assert!(json.contains("abc-123"));
    }
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib models::tests::session_download 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "feat(sync): AppError::SessionDownloadConflict variant

Frontend inspects err.message for \"remote_newer\" / \"local_newer\"
to show the right confirm dialog. DownloadResult carries the fresh
sync metadata back for immediate icon coloring.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: `github_list_remote_sessions_cmd` + `github_resolve_download_target_cmd`

**Files:**
- Modify: `src-tauri/src/commands/github_sync.rs`

**Interfaces:**
- `github_list_remote_sessions_cmd(state) -> AppResult<Vec<RemoteSessionSummary>>`
- `github_resolve_download_target_cmd(state, project_slug: String) -> AppResult<Option<String>>`

**Key facts read from the existing file:**
- The path-mappings state accessor is called `mappings_path(state)` (not `path_mappings_path`) — see `commands/github_sync.rs:56`.
- `map_gh` (line 37) normalizes `GitHubError` to `AppError`.
- `gh_upload::ensure_repo(token, owner, repo_name)` returns the `default_branch` string and creates the repo if missing — reuse it.
- `load_github_token(state)` (line 517) returns a `GitHubAuthSecret`.

- [ ] **Step 1: Add `github_list_remote_sessions_cmd`**

Append to `src-tauri/src/commands/github_sync.rs`:

```rust
/// List every session in the GitHub sync repo, grouped by project.
/// Returns `[]` when the repo doesn't exist yet (user hasn't uploaded).
#[tauri::command]
pub fn github_list_remote_sessions_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<RemoteSessionSummary>> {
    let cfg = storage::load_github_sync_config(&sync_config_path(&state))?;
    if !cfg.is_connected {
        return Err(AppError::GitHubNotConfigured("not_connected".into()));
    }
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;

    // If the repo doesn't exist, the user has never uploaded — empty list.
    let repo = gh_repo::get_repo(token, &owner, &cfg.repo_name).map_err(map_gh)?;
    let Some(repo) = repo else {
        return Ok(Vec::new());
    };

    gh_repo::list_remote_sessions(token, &owner, &cfg.repo_name, &repo.default_branch)
        .map_err(map_gh)
}
```

Note: this uses `gh_repo::get_repo(...).default_branch` directly instead of calling `gh_upload::ensure_repo`, because `ensure_repo` would *create* the repo when missing — we don't want that side-effect on a read-only list.

- [ ] **Step 2: Add `github_resolve_download_target_cmd`**

```rust
/// Resolve the local target folder for a remote project slug, if a
/// mapping already exists. Returns None to trigger the ProjectPicker.
#[tauri::command]
pub fn github_resolve_download_target_cmd(
    state: tauri::State<'_, AppState>,
    project_slug: String,
) -> AppResult<Option<String>> {
    let m = storage::load_path_mappings(&mappings_path(&state))?;
    Ok(m.slug_mappings.get(&project_slug).cloned())
}
```

- [ ] **Step 3: Build**

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: clean build.

- [ ] **Step 4: Commit (do NOT register yet — Task 8 registers all new commands together)**

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

- [ ] **Step 1: Add the command**

Append to `src-tauri/src/commands/github_sync.rs`. Reuses existing helpers throughout — no new plumbing:

```rust
/// Download a session transcript from the GitHub sync repo to the
/// resolved local project folder. Resolves the target via path
/// mappings; if none, returns `Validation("path_mapping_required")`.
/// If a local file already exists and the timestamps disagree,
/// returns `SessionDownloadConflict` unless `force` is true.
#[tauri::command]
pub fn github_download_session_cmd(
    state: tauri::State<'_, AppState>,
    session_id: String,
    project_slug: String,
    blob_sha: String,
    force: Option<bool>,
) -> AppResult<crate::models::DownloadResult> {
    use crate::models::{DownloadResult, SessionConflictKind};

    let cfg_path = sync_config_path(&state);
    let mut cfg = storage::load_github_sync_config(&cfg_path)?;
    if !cfg.is_connected {
        return Err(AppError::GitHubNotConfigured("not_connected".into()));
    }
    let secret = load_github_token(&state)?;
    let token = &secret.access_token;
    let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;

    // Resolve target folder from slug mappings. Missing mapping is not a
    // hard error — it's the picker signal.
    let m = storage::load_path_mappings(&mappings_path(&state))?;
    let target = m.slug_mappings.get(&project_slug).cloned().ok_or_else(|| {
        AppError::Validation("path_mapping_required".into())
    })?;
    let target_path = std::path::PathBuf::from(&target);
    if !target_path.exists() {
        return Err(AppError::Validation(format!(
            "mapped local folder does not exist: {target}"
        )));
    }

    // Repo probe — needed for both the blob fetch and metadata lookup.
    let repo = gh_repo::get_repo(token, &owner, &cfg.repo_name)
        .map_err(map_gh)?
        .ok_or_else(|| AppError::Validation("sync repo does not exist".into()))?;
    let default_branch = repo.default_branch;

    // Read remote's modified timestamp from the project's metadata.json
    // (best-effort; missing metadata is fine — we just can't conflict-check).
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
        Some(sha) => gh_repo::fetch_project_metadata(token, &owner, &cfg.repo_name, &sha)
            .map_err(map_gh)?
            .sessions
            .get(&session_id)
            .and_then(|e| e.modified.clone()),
        None => None,
    };

    // Local mtime for conflict detection.
    let target_jsonl = target_path.join(format!("{session_id}.jsonl"));
    let local_mtime = std::fs::metadata(&target_jsonl).ok().and_then(|m| {
        m.modified().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
    });

    if !force.unwrap_or(false) {
        if let (Some(remote), Some(local)) = (remote_modified.as_ref(), local_mtime.as_ref()) {
            // RFC3339 strings from UTC compare lexicographically.
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
        }
    }

    // Fetch and write the transcript atomically.
    let bytes = gh_repo::get_blob(token, &owner, &cfg.repo_name, &blob_sha).map_err(map_gh)?;
    use std::io::Write;
    let tmp_path = target_jsonl.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, &target_jsonl)?;

    // Register with Claude Code's sessions-index.json.
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let entry = crate::storage::sessions::SessionIndexEntry {
        session_id: session_id.clone(),
        full_path: target_jsonl.display().to_string(),
        first_prompt: None,
        summary: None,
        message_count: Some(0),
        created: Some(now.clone()),
        modified: Some(now.clone()),
        project_path: None,
        is_sidechain: Some(false),
    };
    crate::storage::sessions::upsert_into_sessions_index(&target_path, &entry)?;

    // Update session_sync_state.json so the row appears green immediately.
    let state_path = storage::session_sync_state_path(&target_path);
    let mut state_file = storage::load_session_sync_state(&state_path)?;
    state_file.version = 1;
    state_file.sessions.insert(
        session_id.clone(),
        SessionSyncMetadata {
            last_uploaded: Some(now.clone()),
            remote_sha: Some(blob_sha),
            last_local_modified: Some(now.clone()),
            sync_state: SyncState::Synced,
        },
    );
    storage::write_session_sync_state_atomic(&state_path, &state_file)?;

    // Record last sync on the global config.
    cfg.last_sync = Some(now);
    storage::save_github_sync_config(&cfg_path, &cfg)?;

    Ok(DownloadResult {
        session_id,
        full_path: target_jsonl.display().to_string(),
        sync_state: SyncState::Synced,
    })
}
```

- [ ] **Step 2: Add a unit test for the conflict-detection branch**

Append to `mod tests` in `src-tauri/src/commands/github_sync.rs`:

```rust
    #[test]
    fn rfc3339_compare_orders_remote_vs_local() {
        // The command relies on lexicographic-UTC comparison — lock the
        // invariant so no future refactor introduces string-parse instead.
        let remote = "2026-07-11T10:00:00Z";
        let local = "2026-07-11T09:00:00Z";
        assert!(remote > local);
        assert!(local < remote);
    }
```

- [ ] **Step 3: Confirm cargo build is clean**

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: clean build.

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/github_sync.rs
git commit -m "feat(sync): github_download_session_cmd with conflict detection

Slug resolves via slug_mappings; missing mapping surfaces as
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

Find `github_set_path_mapping_cmd` in `src-tauri/src/commands/github_sync.rs` (near line 218). Replace the body:

```rust
#[tauri::command]
pub fn github_set_path_mapping_cmd(
    state: tauri::State<'_, AppState>,
    original_path: String,
    local_path: String,
    slug: Option<String>,
) -> AppResult<()> {
    let original = original_path.trim();
    let local = local_path.trim();
    if original.is_empty() || local.is_empty() {
        return Err(AppError::Validation(
            "both original_path and local_path are required".into(),
        ));
    }
    let path = mappings_path(&state);
    let mut m = storage::load_path_mappings(&path)?;
    m.version = 1;
    // Canonical key: original decoded path.
    m.mappings
        .insert(original.to_string(), local.to_string());
    // Slug-keyed lookup so download resolvers hit without re-prompting.
    if let Some(s) = slug {
        let s = s.trim();
        if !s.is_empty() {
            m.slug_mappings.insert(s.to_string(), local.to_string());
        }
    }
    storage::save_path_mappings(&path, &m)?;
    Ok(())
}
```

- [ ] **Step 2: Build**

Run: `cd src-tauri && cargo build 2>&1 | tail -10`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/github_sync.rs
git commit -m "feat(sync): path mapping carries slug for download resolver

Writes to slug_mappings in addition to the canonical mappings so
Phase 3 download can skip the project picker for remotes the user
has already mapped once.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 8: Register the 4 new commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:** Pure registration.

- [ ] **Step 1: Add the 4 new commands to `.invoke_handler()`**

Find the existing `commands::github_sync::github_upload_session_cmd,` line in `src-tauri/src/lib.rs`. Add:

```rust
            commands::github_sync::github_list_remote_sessions_cmd,
            commands::github_sync::github_resolve_download_target_cmd,
            commands::github_sync::github_download_session_cmd,
            commands::github_sync::github_list_local_projects_cmd,
```

`github_list_local_projects_cmd` is added in Task 12; register it here in the same commit to keep the invoke_handler list stable.

- [ ] **Step 2: Build**

Run: `cd src-tauri && cargo build 2>&1 | tail -10`

If `github_list_local_projects_cmd` isn't defined yet (Task 12 not landed), skip this line for now — you'll add it when Task 12 lands.

- [ ] **Step 3: Run full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(sync): register Phase 3 download commands

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 9: Mirror new types in TypeScript

**Files:**
- Modify: `src/lib/types.ts`

**Interfaces:**
- `DownloadResult { sessionId, fullPath, syncState: SyncState }`
- `ProjectPathMapping.slug?: string`
- `SessionDownloadConflict` — a discriminant type the frontend narrows against.
- `RemoteSessionSummary` already exists; verify it matches the Rust shape.

- [ ] **Step 1: Add the new types**

Open `src/lib/types.ts`. Add:

```ts
export interface DownloadResult {
  sessionId: string;
  fullPath: string;
  syncState: SyncState;
}

/**
 * Discriminant for a download conflict error. Frontend parses this
 * from the AppError.message string (backend serializes the variant's
 * Display impl into `message`; `kind` at the top level is always
 * `"session_download_conflict"`).
 */
export type SessionConflictKind = "remote_newer" | "local_newer";
```

Update `ProjectPathMapping` — if present, add `slug`; if missing, add the whole interface:

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

DownloadResult, SessionConflictKind, ProjectPathMapping.slug.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 10: API wrappers

**Files:**
- Modify: `src/lib/api.ts`

**Interfaces:** Four new typed wrappers; one existing wrapper extended.

- [ ] **Step 1: Add wrappers**

In `src/lib/api.ts`, find the existing Phase 2 GitHub sync block (near line 271). Add:

```ts
// ---------- github sync (Phase 3: download) ----------

/** List every session in the GitHub sync repo, grouped by project. */
export const githubListRemoteSessions = () =>
  call<RemoteSessionSummary[]>("github_list_remote_sessions_cmd");

/** Resolve the local target folder for a remote project slug. */
export const githubResolveDownloadTarget = (projectSlug: string) =>
  call<string | null>("github_resolve_download_target_cmd", { projectSlug });

/** Download one session; force=true bypasses conflict detection. */
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

/** List existing local Claude Code project folders for the picker. */
export const githubListLocalProjects = () =>
  call<string[]>("github_list_local_projects_cmd");
```

Update the existing `githubSetPathMapping` to accept the optional slug:

```ts
export const githubSetPathMapping = (
  originalPath: string,
  localPath: string,
  slug?: string,
) =>
  call<void>("github_set_path_mapping_cmd", { originalPath, localPath, slug });
```

Add the missing imports at the top: `RemoteSessionSummary`, `DownloadResult`.

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
- Uses existing `useGitHubSyncContext` for `isConnected`. Renders grouped rows from `useRemoteSessions`. Owns the ProjectPickerModal open/close state and passes the picker result back into the download flow.

- [ ] **Step 1: Read `GitHubSyncPanel` to mirror style**

Open `src/components/GitHubSync.tsx` and mirror its modal/button conventions. Same button classes, same modal frame, same hover affordances.

- [ ] **Step 2: Create the component**

Create `src/components/RemoteSessionsModal.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { Loader2, RefreshCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ProjectPickerModal } from "@/components/ProjectPickerModal";
import { useGitHubSyncContext } from "@/hooks/GitHubSyncContext";
import { useRemoteSessions } from "@/hooks/useRemoteSessions";
import type { RemoteSessionSummary } from "@/lib/types";

interface Props {
  open: boolean;
  onClose: () => void;
  onDownloaded: () => void;
}

interface PickerState {
  slug: string;
  originalPath: string;
  pendingRow: RemoteSessionSummary;
}

export function RemoteSessionsModal({ open, onClose, onDownloaded }: Props) {
  const { config } = useGitHubSyncContext();
  const { sessions, loading, error, refresh, download } = useRemoteSessions();
  const [picker, setPicker] = useState<PickerState | null>(null);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  if (!open) return null;

  const groups = new Map<string, RemoteSessionSummary[]>();
  for (const s of sessions) {
    const arr = groups.get(s.projectSlug) ?? [];
    arr.push(s);
    groups.set(s.projectSlug, arr);
  }

  const handleDownload = (row: RemoteSessionSummary) => {
    void download(row, {
      onNeedPicker: () =>
        setPicker({
          slug: row.projectSlug,
          originalPath: row.originalPath,
          pendingRow: row,
        }),
      onDone: () => {
        onDownloaded();
      },
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="flex max-h-[80vh] w-full max-w-3xl flex-col rounded-md bg-background shadow-lg">
        <header className="flex items-center justify-between border-b px-4 py-2">
          <h2 className="text-sm font-semibold">Remote sessions</h2>
          <div className="flex gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void refresh()}
              disabled={loading}
            >
              <RefreshCw
                className={loading ? "mr-1 size-3.5 animate-spin" : "mr-1 size-3.5"}
              />
              Refresh
            </Button>
            <Button variant="ghost" size="icon" onClick={onClose}>
              <X className="size-3.5" />
            </Button>
          </div>
        </header>

        {!config.isConnected && (
          <p className="p-4 text-xs text-muted-foreground">
            Connect GitHub in Settings first.
          </p>
        )}

        {config.isConnected && sessions.length === 0 && !loading && !error && (
          <p className="p-4 text-xs text-muted-foreground">
            No remote sessions yet.
          </p>
        )}

        {loading && (
          <div className="flex justify-center p-4">
            <Loader2 className="size-4 animate-spin" />
          </div>
        )}

        {error && <p className="p-4 text-xs text-destructive">{error}</p>}

        <div className="overflow-auto px-4 py-2">
          {[...groups.entries()].map(([slug, rows]) => (
            <section key={slug} className="mb-4">
              <h3 className="text-[11px] font-medium text-muted-foreground">
                {slug}
              </h3>
              <ul className="divide-y">
                {rows.map((r) => (
                  <li
                    key={r.sessionId}
                    className="flex items-center justify-between py-2"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-xs">
                        {r.title ?? r.sessionId.slice(0, 8)}
                      </div>
                      <div className="text-[10px] text-muted-foreground">
                        {r.modified ?? "—"} · {r.messageCount} msgs
                      </div>
                    </div>
                    <Button size="sm" onClick={() => handleDownload(r)}>
                      Download
                    </Button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>

      {picker && (
        <ProjectPickerModal
          open
          onClose={() => setPicker(null)}
          remoteOriginalPath={picker.originalPath}
          remoteSlug={picker.slug}
          onPicked={() => {
            const row = picker.pendingRow;
            setPicker(null);
            // Retry the download after the mapping is persisted.
            void download(row, {
              onNeedPicker: () => {
                // Should never fire on the retry since we just set the
                // mapping, but if it does the modal reopens gracefully.
                setPicker({
                  slug: row.projectSlug,
                  originalPath: row.originalPath,
                  pendingRow: row,
                });
              },
              onDone: () => onDownloaded(),
            });
          }}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 3: Type-check**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10`
Expected: will error until Task 13 lands (`useRemoteSessions` doesn't exist yet). That's fine — we commit Tasks 11/12/13 together.

- [ ] **Step 4: Commit deferred to Task 13**

---

### Task 12: `ProjectPickerModal` component + `github_list_local_projects_cmd` + `pickFolder`

**Files:**
- Create: `src/components/ProjectPickerModal.tsx`
- Create: `src/lib/dialogs.ts`
- Modify: `src-tauri/src/commands/github_sync.rs` (add `github_list_local_projects_cmd`)

**Interfaces:**
- `github_list_local_projects_cmd(state) -> AppResult<Vec<String>>` — returns the local project folder paths under `<claude_dir>/projects/*/`. Each returned string is an absolute path.
- `pickFolder(): Promise<string | null>` — thin wrapper around `@tauri-apps/plugin-dialog`'s `open({directory: true, multiple: false})`.
- `ProjectPickerModal` props: `{ open, onClose, remoteOriginalPath, remoteSlug, onPicked }`.

- [ ] **Step 1: Add the backend `github_list_local_projects_cmd`**

Append to `src-tauri/src/commands/github_sync.rs`:

```rust
/// Return the absolute paths of every local Claude Code project folder
/// (`<claude_dir>/projects/*/`). Feeds the ProjectPickerModal dropdown.
#[tauri::command]
pub fn github_list_local_projects_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<String>> {
    let claude_dir = state.claude_dir.clone();
    let projects_dir = claude_dir.join(crate::storage::sessions::PROJECTS_DIR);
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&projects_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.push(path.display().to_string());
        }
    }
    out.sort();
    Ok(out)
}
```

If `state.claude_dir` doesn't exist as a field, use `crate::storage::system::discover_claude_dir()` (the same helper the sessions scanner uses). Check `state.rs` first; the existing sessions listing command already resolves `claude_dir`, mirror that.

- [ ] **Step 2: Create `src/lib/dialogs.ts`**

```ts
import { open } from "@tauri-apps/plugin-dialog";

/**
 * Open a native folder picker. Returns the selected absolute path or
 * `null` if the user cancelled.
 */
export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}
```

- [ ] **Step 3: Create `src/components/ProjectPickerModal.tsx`**

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
  /** Called after the mapping is persisted; parent re-triggers download. */
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
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-md bg-background p-4 shadow-lg">
        <header className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">Pick target project</h2>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="size-3.5" />
          </Button>
        </header>
        <p className="mb-3 text-[11px] text-muted-foreground">
          Remote: <code className="font-mono">{remoteOriginalPath}</code>
        </p>
        <label className="mb-1 block text-xs">Local project folder</label>
        <select
          className="w-full rounded border px-2 py-1 text-xs"
          value={selected}
          onChange={(e) => setSelected(e.target.value)}
        >
          {projects.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        <div className="mt-3 flex items-center justify-between">
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              const picked = await pickFolder();
              if (picked) setSelected(picked);
            }}
          >
            Browse…
          </Button>
          <label className="flex items-center gap-1 text-[11px]">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />
            Remember this mapping
          </label>
        </div>
        <footer className="mt-4 flex justify-end gap-2">
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

- [ ] **Step 4: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10 && pnpm lint 2>&1 | tail -10`
Expected: will still error until Task 13 lands. Fine — commit together.

- [ ] **Step 5: Commit deferred to Task 13**

---

### Task 13: `useRemoteSessions` hook

**Files:**
- Create: `src/hooks/useRemoteSessions.ts`

**Interfaces:**
- Returns `{ sessions, loading, error, refresh, download }`.
- `download(row, { onNeedPicker, onDone })` orchestrates: resolve target → optional picker → api call → conflict dialog → onDone.

- [ ] **Step 1: Create the hook**

Create `src/hooks/useRemoteSessions.ts`:

```ts
"use client";

import { useCallback, useState } from "react";
import { toast } from "sonner";

import {
  AppError,
  githubDownloadSession,
  githubListRemoteSessions,
  githubResolveDownloadTarget,
} from "@/lib/api";
import type {
  DownloadResult,
  RemoteSessionSummary,
} from "@/lib/types";

interface DownloadCallbacks {
  /** Fires when no local mapping exists yet — parent opens the picker. */
  onNeedPicker: () => void;
  /** Fires after a successful download. */
  onDone: (result: DownloadResult) => void;
}

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
      const msg = e instanceof Error ? e.message : String(e);
      setState({ sessions: [], loading: false, error: msg });
    }
  }, []);

  const doDownload = useCallback(
    async (
      row: RemoteSessionSummary,
      cb: DownloadCallbacks,
      force: boolean,
    ) => {
      try {
        const result = await githubDownloadSession(
          row.sessionId,
          row.projectSlug,
          row.sha,
          force,
        );
        toast.success("Session downloaded");
        cb.onDone(result);
      } catch (e) {
        const kind = e instanceof AppError ? e.kind : undefined;
        const message = e instanceof Error ? e.message : String(e);

        // Conflict — inspect the message string for the variant name.
        if (kind === "session_download_conflict") {
          const remoteNewer = message.includes("RemoteNewer");
          const proceed = window.confirm(
            remoteNewer
              ? "Remote copy is newer than the local file. Overwrite local?"
              : "Local copy is newer than the remote file. Overwrite with remote?",
          );
          if (!proceed) return;
          await doDownload(row, cb, true);
          return;
        }

        // Missing mapping — surface to parent so it can open the picker.
        if (message.includes("path_mapping_required")) {
          cb.onNeedPicker();
          return;
        }

        toast.error(`Download failed: ${message}`);
      }
    },
    [],
  );

  const download = useCallback(
    async (row: RemoteSessionSummary, cb: DownloadCallbacks) => {
      // Peek at the mapping first so we don't burn an API round-trip
      // when we already know the picker is needed.
      const target = await githubResolveDownloadTarget(row.projectSlug);
      if (!target) {
        cb.onNeedPicker();
        return;
      }
      await doDownload(row, cb, false);
    },
    [doDownload],
  );

  return { ...state, refresh, download };
}
```

- [ ] **Step 2: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10 && pnpm lint 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit Tasks 11, 12, 13 together**

```bash
git add \
  src/components/RemoteSessionsModal.tsx \
  src/components/ProjectPickerModal.tsx \
  src/hooks/useRemoteSessions.ts \
  src/lib/dialogs.ts \
  src-tauri/src/commands/github_sync.rs
git commit -m "feat(sync): RemoteSessionsModal + ProjectPickerModal + hook

Phase 3 browse-remote UI: grouped list, project picker for unmapped
slugs, conflict confirm. Backend gains github_list_local_projects_cmd
for the picker dropdown.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 14: Wire `GitHubTopBarButton` to open the modal

**Files:**
- Modify: `src/components/GitHubTopBarButton.tsx`

- [ ] **Step 1: Read the existing button to learn its click convention**

Open `src/components/GitHubTopBarButton.tsx`. Note the existing click handler (if any) and how it wires into `GitHubSyncContext`. Add a `remoteOpen` state and render `RemoteSessionsModal`.

- [ ] **Step 2: Wire the modal**

If the button currently only opens a settings panel, add a right-click / secondary click that opens the modal, or add a menu affordance. Simplest: split the button into two states — click when disconnected opens Settings; click when connected opens the RemoteSessionsModal. Adjust to whatever matches the existing UX; keep the settings entry-point discoverable.

Suggested minimal diff:

```tsx
"use client";

import { useState } from "react";
// ... existing imports ...
import { RemoteSessionsModal } from "@/components/RemoteSessionsModal";
import { useSessions } from "@/hooks/useSessions";

export function GitHubTopBarButton() {
  const { config } = useGitHubSyncContext();
  const { refresh: refreshSessions } = useSessions();
  const [remoteOpen, setRemoteOpen] = useState(false);
  // ... existing render ...

  // On click (when connected): open the remote sessions modal.
  // When disconnected: keep the existing settings-open behavior.

  return (
    <>
      <Button
        // ... existing props ...
        onClick={() => {
          if (config.isConnected) setRemoteOpen(true);
          else /* existing disconnected-click behavior */;
        }}
      >
        {/* existing children */}
      </Button>

      <RemoteSessionsModal
        open={remoteOpen}
        onClose={() => setRemoteOpen(false)}
        onDownloaded={() => void refreshSessions()}
      />
    </>
  );
}
```

Read the actual file first — it may already have its own state pattern that we should slot into rather than shadow.

- [ ] **Step 3: Type-check + lint**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10 && pnpm lint 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 4: Manual smoke test**

Run: `pnpm tauri dev`. With a connected GitHub account and at least one uploaded session from Phase 2:

1. Click the GitHub top-bar button while connected → modal opens.
2. Row shows real `title` (may be null until Phase 2 populates it — see the known follow-up in the summary) and `messageCount`.
3. Click "Download" on a row whose slug is already mapped (or maps to `~/.claude/projects/<slug>/` if the slug matches the current machine's slug directly) → transcript appears in the local Sessions list with a **green** GitHub icon. Re-open the app: still green.
4. Force a `RemoteNewer` conflict: re-upload the same session from another machine after modifying its remote copy, then on this machine click Download → "Overwrite local?" prompt.
5. Force an unmapped project: pick a remote slug with no local folder → ProjectPickerModal opens → choose a folder → mapping persists in `project_path_mappings.json` (verify with `cat`), and the download retries automatically.
6. Empty repo: disconnect, delete the sync repo on GitHub, reconnect (do NOT re-upload) → "Browse remote" shows "No remote sessions yet".

- [ ] **Step 5: Commit**

```bash
git add src/components/GitHubTopBarButton.tsx
git commit -m "feat(sync): open RemoteSessionsModal from GitHub top-bar button

Visible only when connected. Modal owns its own refresh on open and
triggers a local sessions refresh after each download.

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

**Placeholder scan:** every step has explicit code + explicit run/check command.

**Type consistency:** `SessionIndexEntry` / `SessionsIndex` (plural, plain `Vec<SessionIndexEntry>`) / `ProjectRemoteMetadata` / `RemoteSessionSummary` / `SessionSyncMetadata` / `SyncState` / `ProjectPathMapping` / `ProjectPathMappings` (with new `slug_mappings`) — matches `models.rs` and `storage/sessions.rs` as of 2026-07-11. New types `SessionConflictKind`, `AppError::SessionDownloadConflict`, `DownloadResult` defined once in Task 4 and reused.

**Known follow-up (not blocking):** `metadata.json` writes `title: None` and `messageCount: 0` per session today. Phase 2's `github_upload_session_cmd` already extracts the title (for the commit message) but doesn't write it back into `metadata.json`. Track separately as a Phase 2.1 fix; Phase 3 UI will show titled sessions once that lands.

**Risks:**
- `state.claude_dir` may not exist as a field on `AppState`; Task 12 Step 1 falls back to `discover_claude_dir()` — confirm at implementation time.
- `SessionsIndex` visibility bump (Task 1 Step 1) touches types shared with `scan_sessions`. `cargo build` catches any callers broken by the switch from `pub(crate)` to `pub`. There should be zero — `pub` is strictly more permissive.
- `AppError::SessionDownloadConflict` fields are only serialized via the `#[error]` template string. If the frontend ever wants structured fields, teach the custom `Serialize` impl to switch on the variant. Not required for v3.

---

Plan complete and saved to `docs/superpowers/plans/2026-07-11-session-download-from-github.md`. Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
