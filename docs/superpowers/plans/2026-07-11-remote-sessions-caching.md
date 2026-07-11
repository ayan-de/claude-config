# Remote Sessions Tab — Fetch Caching & SHA-Gated Refresh

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the redundant HTTP work that happens every time the user opens (or refreshes) the Remote sessions tab and every time they preview a transcript. Steady state — nothing changed remotely — must cost **1 HTTP call**, not `3 + N`. When something did change, only refetch the projects that actually changed.

**Non-goal:** Change any user-visible IPC shape. `RemoteSessionSummary`, `SessionSyncMetadata`, `DownloadResult` stay identical — additive backend changes only.

## Background — current cost per action

For each `github_list_remote_sessions_cmd` call today (`src-tauri/src/commands/github_sync.rs:546`):

1. `GET /user` — resolve owner (`src-tauri/src/github/repo.rs:144`)
2. `GET /repos/{owner}/{repo}` — probe repo + default branch (`repo.rs:155`)
3. `GET /git/trees/{branch}?recursive=1` — full tree walk (`repo.rs:190`)
4. `GET /git/blobs/{sha}` **× N projects** — one per `metadata.json` (`repo.rs:398`)

For each preview (`github_fetch_remote_transcript_cmd`, `commands/github_sync.rs:741`):

1. `GET /user`
2. `GET /repos/{owner}/{repo}`
3. `GET /git/blobs/{sha}`

`useRemoteSessions.refresh()` fires on every mount of `RemoteSessionsTab` (`components/RemoteSessionsTab.tsx:40`) with no gating.

Blob content is content-addressed by SHA-1 — a blob fetched once is byte-identical forever, but is refetched every preview today.

## Architecture — the four moves

1. **In-`AppState` cache for owner + default_branch.** Both are tied to the token, both change ~never for a connected user. Cleared on `github_disconnect_cmd`. Cuts every command by 2 HTTP calls.

2. **Commit-SHA-gated refresh.** Replaces the "ETag / If-None-Match" idea from the earlier draft. `GET /repos/{owner}/{repo}/git/refs/heads/{branch}` returns the current HEAD commit SHA in a tiny payload. Cache the last commit SHA alongside the last successful `Vec<RemoteSessionSummary>`. On each list call:
   - Fetch current commit SHA (1 call).
   - If it matches the cached SHA → return the cached list. **Total: 1 call.**
   - If it differs → do the full tree walk, but diff the new tree against the cached tree and only refetch `metadata.json` for projects whose slug's tree entries changed. **Total: 2 + M** where M is the projects that changed.

   Strictly better than ETags: no header plumbing, no 304-body ambiguity, and the same call naturally tells us which blobs are new — which composes with (3) below.

3. **Content-addressed blob disk cache.** Store fetched blobs under `${app_data_dir}/github_cache/blobs/{sha}` keyed by SHA-1. Blob SHA is the content hash so entries are eternally correct.

   **Security caveat (non-negotiable):** these blobs are session transcripts. We already flagged in Phase 2 that they can contain file contents, environment variables, and shell output. An indefinite plaintext disk cache means sensitive content persists outside the encrypted keyring, in a location `find ~/.local/share/com.claudeconfig.app` will surface. Two mitigations, do both:
   - **Clear the cache in `github_disconnect_cmd`** — matches how we already delete the token.
   - **Size cap** with LRU eviction (default 200 MB). Correctness doesn't need it, but it prevents unbounded growth.

4. **Frontend stale-while-revalidate.** Persist the last successful sessions list to `localStorage`. On tab mount: paint cached list immediately, fire the SHA-gated refresh in the background, reconcile in place if new data arrives. Handle the edge case: if the background refresh removes a row the user was mid-download on, the download must fail with a clear "no longer on remote" message rather than 404 silently.

**Tech Stack:** Rust (Tauri 2 backend, `reqwest::blocking`, `serde_json`, `parking_lot::Mutex` or `std::sync::Mutex`), Next.js 16 + React 19. No new dependencies.

## Global Constraints

- **No changes to existing IPC shapes.** `RemoteSessionSummary`, `SessionSyncMetadata`, `DownloadResult` stay identical. New commands may be added (`github_invalidate_remote_cache_cmd`, `github_get_blob_cache_stats_cmd`) but existing ones keep their signatures.
- Cache mutations use existing atomic-write pattern (`storage/github_sync.rs::write_session_sync_state_atomic`) — temp file + `fsync` + rename, no naked `fs::write`.
- New in-memory caches live on `AppState` as `Arc<Mutex<...>>`. `AppState` is `Clone` and shared across commands; caches must survive across command invocations.
- `github_disconnect_cmd` must invalidate every cache tier (owner, default_branch, commit_sha, sessions_list, blob disk cache).
- No new Tauri capabilities. No new top-level directories.
- Verification: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.

## File Structure

| File | Role | Touched in |
|---|---|---|
| `src-tauri/src/state.rs` | Add `Arc<Mutex<GitHubCache>>` field to `AppState` | Task 1 |
| `src-tauri/src/github/cache.rs` | **New.** `GitHubCache` struct (owner, default_branch, commit_sha, sessions_list, tree). Blob-cache disk helpers. | Tasks 1, 4 |
| `src-tauri/src/github/repo.rs` | Add `get_branch_ref_sha` (single-call HEAD lookup). Extend `list_remote_sessions` to accept a "reuse cached metadata for unchanged slugs" callback. | Tasks 2, 3 |
| `src-tauri/src/commands/github_sync.rs` | Thread cache through `github_list_remote_sessions_cmd`, `github_fetch_remote_transcript_cmd`, `github_download_session_cmd`. Wire disconnect to clear cache. | Tasks 2, 3, 4, 5 |
| `src/hooks/useRemoteSessions.ts` | Read/write `localStorage` for last-known list; background refresh; stale-row handling | Task 6 |
| `src/components/RemoteSessionsTab.tsx` | Show cached list on mount without spinner; small badge while background refresh runs | Task 6 |

No new IPC command names. No new frontend components.

---

### Task 1: `GitHubCache` on `AppState` + owner/default_branch caching

**Files:**
- New: `src-tauri/src/github/cache.rs`
- Modify: `src-tauri/src/state.rs`, `src-tauri/src/github/mod.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct GitHubCache {
      pub owner: Option<String>,
      pub default_branch: Option<String>,
      pub commit_sha: Option<String>,
      pub sessions_list: Option<Vec<RemoteSessionSummary>>,
      pub tree: Option<Tree>, // for slug-diff on invalidation
  }
  impl GitHubCache {
      pub fn clear(&mut self);
  }
  ```
- Field on `AppState`: `pub github_cache: Arc<Mutex<GitHubCache>>`.

**Steps:**

- [ ] **Step 1: Create `src-tauri/src/github/cache.rs`** with the `GitHubCache` struct and `Default`/`clear` impls. Add `pub mod cache;` to `github/mod.rs`.

- [ ] **Step 2: Add `github_cache: Arc<Mutex<GitHubCache>>` to `AppState`.** Initialize in `lib.rs:49` with `Arc::new(Mutex::new(GitHubCache::default()))`.

- [ ] **Step 3: Introduce a `resolve_owner_and_branch` helper in `commands/github_sync.rs`** that reads/writes the cache:
  ```rust
  fn resolve_owner_and_branch(state: &AppState, token: &str, repo_name: &str)
      -> AppResult<(String, String)> {
      let mut c = state.github_cache.lock();
      if let (Some(o), Some(b)) = (&c.owner, &c.default_branch) {
          return Ok((o.clone(), b.clone()));
      }
      // NOTE: we drop the lock before the HTTP calls so a stuck request can't
      // block every other command. Two concurrent misses will therefore each
      // fire the pair of HTTP calls — this is not single-flight. Acceptable
      // for this feature's usage pattern (human-driven tab clicks don't
      // race), but do not assume dedupe if a future caller starts firing
      // this concurrently from a background task.
      drop(c);
      let owner = gh_repo::get_authenticated_user(token).map_err(map_gh)?;
      let repo = gh_repo::get_repo(token, &owner, repo_name)
          .map_err(map_gh)?
          .ok_or_else(|| AppError::Validation("sync repo does not exist".into()))?;
      let mut c = state.github_cache.lock();
      c.owner = Some(owner.clone());
      c.default_branch = Some(repo.default_branch.clone());
      Ok((owner, repo.default_branch))
  }
  ```
  Replace the `get_authenticated_user` + `get_repo` sequences in `github_list_remote_sessions_cmd`, `github_download_session_cmd`, and `github_fetch_remote_transcript_cmd` with this helper.

- [ ] **Step 4: Wire `github_disconnect_cmd` to clear the cache** — `state.github_cache.lock().clear()` immediately after the keyring delete. Also clear on `github_poll_device_flow_cmd::Authorized` to avoid a stale owner from a previous login lingering.

- [ ] **Step 5: Seed `cache.owner` from the device-flow response.** `github_poll_device_flow_cmd`'s `Authorized` branch already receives the authenticated `username` from GitHub — that value is byte-identical to what `get_authenticated_user` would return (GitHub's `login` field). After the `clear()` from Step 4, immediately re-populate `cache.owner = Some(username.clone())` under lock. `default_branch` cannot be seeded here because the sync repo may not exist yet at OAuth time; `resolve_owner_and_branch`'s first call after connect will still make the repo probe (1 call), but will skip the `/user` call — so first tab-open pays `2 + N` instead of `3 + N`, matching the metrics table.

- [ ] **Step 6: Unit tests in `github/cache.rs`** — `clear` empties every field; `Default` returns all `None`; seeding just `owner` without `default_branch` leaves `default_branch` at `None`.

**Verification:** `cargo test`, then manually: launch app, open Remote tab twice back-to-back, confirm second open makes 1 fewer `/user` call in devtools network log.

---

### Task 2: `get_branch_ref_sha` — 1-call ref probe

**Files:**
- Modify: `src-tauri/src/github/repo.rs`

**Interfaces:**
- Produces: `pub fn get_branch_ref_sha(token: &str, owner: &str, repo: &str, branch: &str) -> Result<Option<String>, GitHubError>` — returns `Ok(Some(sha))` on 200, `Ok(None)` on 404 (branch missing), errors otherwise.

**Steps:**

- [ ] **Step 1: Add failing tests** in `repo.rs::tests`:
  ```rust
  #[test] fn ref_sha_deserializes_from_gh_payload() { ... }
  #[test] fn ref_sha_returns_none_on_404() { ... } // struct-level, doesn't hit network
  ```
  Payload shape:
  ```json
  { "ref": "refs/heads/main", "node_id": "...",
    "object": { "sha": "abc123...", "type": "commit", "url": "..." } }
  ```
  We only need `object.sha`.

- [ ] **Step 2: Implement** the function using `GitHubClient::get_json` on `{GITHUB_API_BASE}/repos/{owner}/{repo}/git/ref/heads/{branch}`. Note the endpoint is `git/ref/...` (singular) not `git/refs/...` — the plural form redirects and 302 can trip `reqwest`.

- [ ] **Step 3: Confirm tests pass.**

---

### Task 3: SHA-gated `github_list_remote_sessions_cmd` with tree-diff

**Files:**
- Modify: `src-tauri/src/github/repo.rs` (extend `list_remote_sessions`)
- Modify: `src-tauri/src/commands/github_sync.rs`

**Behavior contract:**

1. Fetch current commit SHA via `get_branch_ref_sha`.
2. If it matches `cache.commit_sha` **and** `cache.sessions_list` is `Some` → return the cached list. **1 HTTP call total.**
3. Otherwise: fetch tree, compute two slug sets against `cache.tree` (if any):
   - `changed` — slugs whose tree entries in the new tree differ from the old tree (added or modified projects).
   - `removed` — slugs present in the old tree but absent from the new tree (fully-deleted projects).
4. Refetch `metadata.json` only for `changed` slugs; splice previously-cached `RemoteSessionSummary` rows in unchanged for every slug that is neither `changed` nor `removed`; drop every row whose slug is in `removed`.
5. Update `cache.commit_sha`, `cache.tree`, `cache.sessions_list` atomically.

**Steps:**

- [ ] **Step 1: Extract slug-set diff helper** in `github/repo.rs` (pure fn, easy to test). Returns both `changed` and `removed` in one pass so callers can't forget one:
  ```rust
  pub struct SlugDiff {
      pub changed: HashSet<String>,
      pub removed: HashSet<String>,
  }
  pub fn diff_slugs(old: Option<&Tree>, new: &Tree) -> SlugDiff {
      let new_slugs: HashSet<String> = new.tree.iter().filter_map(slug_of).collect();
      let Some(old) = old else {
          // First run: every project is "changed" (needs a metadata fetch),
          // and nothing has been removed.
          return SlugDiff { changed: new_slugs, removed: HashSet::new() };
      };
      let old_map: HashMap<&str, &str> = old.tree.iter()
          .filter_map(|e| Some((e.path.as_str(), e.sha.as_str()))).collect();
      let old_slugs: HashSet<String> = old.tree.iter().filter_map(slug_of).collect();
      let changed: HashSet<String> = new.tree.iter()
          .filter_map(|e| {
              let slug = slug_of(e)?;
              if old_map.get(e.path.as_str()) == Some(&e.sha.as_str()) { None }
              else { Some(slug) }
          })
          .collect();
      let removed: HashSet<String> = old_slugs.difference(&new_slugs).cloned().collect();
      SlugDiff { changed, removed }
  }
  fn slug_of(e: &TreeEntry) -> Option<String> {
      let parts: Vec<&str> = e.path.split('/').collect();
      if parts.len() >= 2 && parts[0] == "sessions" { Some(parts[1].into()) } else { None }
  }
  ```

- [ ] **Step 2: Refactor `list_remote_sessions`** to accept `previous: Option<&[RemoteSessionSummary]>` and `diff: &SlugDiff`. For every slug in neither `diff.changed` nor `diff.removed`, splice rows from `previous` instead of refetching. For every slug in `diff.removed`, drop the row entirely. Existing signature stays available as a thin wrapper (`list_remote_sessions(t, o, r, b) = list_remote_sessions_with_diff(t, o, r, b, None, &SlugDiff { changed: all_slugs, removed: empty })`).

- [ ] **Step 3: Rewrite `github_list_remote_sessions_cmd`** to:
  - Call `resolve_owner_and_branch` (Task 1).
  - Call `get_branch_ref_sha` (Task 2).
  - Read cache under lock; if SHA matches, return cached list immediately.
  - Otherwise fetch tree, compute `changed_slugs`, call `list_remote_sessions_with_diff`, write cache under lock, return list.
  - Preserve `Ok(Vec::new())` fast path when the repo doesn't exist (repo probe is inside `resolve_owner_and_branch`; distinguish via error and short-circuit).

- [ ] **Step 4: Add integration-level Rust tests** for `diff_slugs`:
  - Empty previous tree → every slug is in `changed`, `removed` is empty.
  - Same tree → both `changed` and `removed` empty.
  - One session added in one project → only that slug in `changed`; `removed` empty.
  - Two projects change → both slugs in `changed`; `removed` empty.
  - `manifest.json` changes at root → does not appear in either set (no `sessions/` prefix).
  - **Whole project deleted** — old tree has `sessions/-home-foo/uuid.jsonl` + `sessions/-home-foo/metadata.json`; new tree has neither → `-home-foo` appears in `removed`, not in `changed`. Also verify at the caller level that `list_remote_sessions_with_diff` drops the corresponding rows from the spliced output rather than leaving stale entries that would 404 on download.

- [ ] **Step 5: Optional but recommended** — add a `#[tauri::command] fn github_invalidate_remote_cache_cmd(state)` that just clears the cache. Bind it to the Refresh button so users have an explicit "force full refetch" escape hatch. Frontend calls it before `refresh()` when the user clicks refresh while holding Shift, matching browser hard-refresh convention.

**Verification:**
- Open Remote tab, click Refresh 5×. First call is `2 + N`, subsequent are `1`. Confirm via network log.
- Push a new session from another machine, click Refresh once. Confirm exactly `2 + 1` calls (ref + tree + 1 metadata.json).

---

### Task 4: Content-addressed blob disk cache

**Files:**
- Modify: `src-tauri/src/github/cache.rs`
- Modify: `src-tauri/src/commands/github_sync.rs` (`github_fetch_remote_transcript_cmd`, `github_download_session_cmd`)

**Interfaces:**
- Produces:
  ```rust
  pub fn blob_cache_dir(app_data_dir: &Path) -> PathBuf { /* ${app_data_dir}/github_cache/blobs */ }
  pub fn get_cached_blob(app_data_dir: &Path, sha: &str) -> Option<Vec<u8>>;
  pub fn put_cached_blob(app_data_dir: &Path, sha: &str, bytes: &[u8]) -> AppResult<()>;
  pub fn clear_blob_cache(app_data_dir: &Path) -> AppResult<()>;
  pub fn enforce_blob_cache_size(app_data_dir: &Path, max_bytes: u64) -> AppResult<()>;
  ```

**Safety requirements:**
- SHA must match `^[0-9a-f]{40}$` before being used as a file path. Reject anything else with `AppError::Validation` — untrusted input.
- Writes go through `NamedTempFile::new_in` + `persist`, matching the existing atomic pattern.
- `enforce_blob_cache_size` sorts by mtime ascending and deletes oldest until total ≤ cap. Default cap: 200 MB. Configurable via `${GITHUB_BLOB_CACHE_MAX_MB}` env var for testability.

**Steps:**

- [ ] **Step 1: Add tests first** in `github/cache.rs`:
  - `get_cached_blob` returns `None` for missing SHA
  - `put_cached_blob` + `get_cached_blob` roundtrips bytes
  - `put_cached_blob` rejects non-hex SHA
  - `enforce_blob_cache_size` deletes oldest files first, keeps newest within cap
  - `clear_blob_cache` removes every blob but leaves the directory

- [ ] **Step 2: Implement the helpers.** Use `chrono::Utc::now` for mtime tie-breaking in the eviction sort so tests are deterministic.

- [ ] **Step 3: Wrap the `get_blob` call** in `github_fetch_remote_transcript_cmd` and `github_download_session_cmd`:
  ```rust
  let bytes = match get_cached_blob(&state.app_data_dir, &blob_sha) {
      Some(b) => b,
      None => {
          let b = gh_repo::get_blob(token, &owner, &cfg.repo_name, &blob_sha).map_err(map_gh)?;
          let _ = put_cached_blob(&state.app_data_dir, &blob_sha, &b); // best-effort
          let _ = enforce_blob_cache_size(&state.app_data_dir, cache_cap_bytes()); // best-effort
          b
      }
  };
  ```
  Cache write failures never fail the parent operation — the source of truth is GitHub.

- [ ] **Step 4: Extend `github_disconnect_cmd`** to call `clear_blob_cache` after the keyring delete and cache clear. Silent-fail on IO errors (log only). Matches the "token is gone, sensitive data should be gone" invariant.

- [ ] **Step 5: Add a `github_get_blob_cache_stats_cmd`** returning `{ file_count, total_bytes }` so a future settings-page toggle can surface the cache size to the user. Ship the command now, wire the UI later.

**Verification:**
- Open a session preview twice. First open makes a `get_blob` call; second makes none.
- `pnpm tauri dev`, disconnect via the UI, then `ls ${app_data_dir}/github_cache/blobs/` — empty.

---

### Task 5: Refresh button behavior + cache invalidation semantics

**Files:**
- Modify: `src-tauri/src/commands/github_sync.rs`
- Modify: `src/lib/api.ts`, `src/components/RemoteSessionsTab.tsx`

**Steps:**

- [ ] **Step 1: Regular Refresh does not invalidate.** The SHA-gated path from Task 3 already re-checks the ref on every call, so a normal Refresh either confirms "nothing new" for free or refetches the delta. No user-visible regression.

- [ ] **Step 2: Force refresh (Shift+Click, or long-press on touch)** invokes `githubInvalidateRemoteCache()` before `refresh()`. Add a subtle "hold Shift to force" tooltip only in the dev build.

- [ ] **Step 3: After a successful upload** (`github_upload_session_cmd` completes), invalidate `cache.commit_sha` and `cache.sessions_list` **from the backend** — the upload just changed HEAD, so any cached list is stale. The next list call will refetch. No frontend action needed.

---

### Task 6: Frontend stale-while-revalidate

**Files:**
- Modify: `src/hooks/useRemoteSessions.ts`
- Modify: `src/components/RemoteSessionsTab.tsx`

**Behavior contract:**

1. On hook mount, read `localStorage['remoteSessions:v1']` — an object of shape `{ sessions: RemoteSessionSummary[], savedAt: string }`. If present and less than 24h old, seed `state.sessions` with it and set `loading: false` immediately.
2. Fire `refresh()` in the background regardless. If it returns a different list, patch state and rewrite `localStorage`.
3. If `refresh()` errors and we have cached data, show the existing "Showing cached results — {error}" banner without evicting cached rows.
4. **Stale-row protection during download:** in `download()`, if the row's `sha` no longer matches any row in the freshly-refreshed list (i.e. the remote deleted it between our last list and now), surface `AppError::Validation("session removed from remote")` and refresh the list.

**Steps:**

- [ ] **Step 1: Add a `readCachedSessions()` / `writeCachedSessions()` pair** at the top of `useRemoteSessions.ts`. Namespace the key with a schema version — bump it if `RemoteSessionSummary` ever changes shape so we don't crash on a bad decode.

- [ ] **Step 2: Change the initial state.** Instead of `loading: true, sessions: []`, seed from cache if available:
  ```ts
  const cached = readCachedSessions();
  const [state, setState] = useState<State>({
      sessions: cached?.sessions ?? [],
      loading: cached === null, // only spin if we have nothing to show
      error: null,
  });
  ```

- [ ] **Step 3: In `refresh()`,** on success, call `writeCachedSessions(sessions)` before setState. On error with cached data present, keep the cached rows and set `error` — the existing error banner already handles this UX.

- [ ] **Step 4: Update the `initialLoad` guard in `RemoteSessionsTab.tsx:75`** — currently `loading && sessions.length === 0 && error === null`. With cached seed data, `sessions.length` will be > 0 immediately, so the spinner naturally hides. Add a subtle inline "Refreshing…" badge when `loading === true && sessions.length > 0` so users know the background refresh is in flight.

- [ ] **Step 5: Stale-row protection in `download()`.** Before firing `githubDownloadSession`, `await refresh()` if the last refresh was > 60s ago (`Date.now() - lastRefreshAt`). If after refresh the row's SHA no longer appears, `toast.error("Session no longer on remote — refresh to see latest list")` and bail.

- [ ] **Step 6: Reset cache on disconnect.** Subscribe to the GitHub sync context's `isConnected` flag; on transition to `false`, `localStorage.removeItem('remoteSessions:v1')` and reset hook state.

**Verification:**
- Load Remote tab, then reload the app. Tab paints instantly with cached rows before any network activity. Confirm with devtools throttling set to Slow 3G.
- Disconnect GitHub. Reconnect. Confirm cached rows do not leak between sessions (should be blank on reconnect).

---

## Non-goals & explicitly out of scope

- **React Query / SWR integration.** Overkill for one endpoint. If we add a second remote data source, revisit.
- **ETag / conditional GETs.** The commit-SHA approach in Task 3 supersedes this cleanly. Adding both would be redundant surface area.
- **Encrypted blob cache.** Blobs are already sensitive but the OS keyring's per-secret limits make it impractical for MB-scale transcripts. Rely on filesystem permissions + `clear_blob_cache` on disconnect. Revisit if we ship this to shared machines.
- **Server-push invalidation.** GitHub doesn't push; polling on tab open is fine at this cadence.

## Success metrics (measure before/after)

Log HTTP request counts via a small `#[cfg(debug_assertions)]` counter in `github/client.rs`. Snapshot before starting and after Task 4:

| Action | Before | After (steady state) | After (change on remote) |
|---|---|---|---|
| Open Remote tab, first time after connecting (Task 1 Step 5 seeded owner) | `3 + N` | `2 + N` | `2 + N` |
| Open Remote tab, first time after app restart (owner cache empty) | `3 + N` | `3 + N` | `3 + N` |
| Open Remote tab, subsequent | `3 + N` | `1` | `2 + M` |
| Preview a transcript, first time | `3` | `1` | `1` |
| Preview same transcript again | `3` | `0` | `0` |
| Refresh button | `3 + N` | `1` | `2 + M` |

N = total projects. M = projects that changed remotely since last cache write. In typical usage M ≈ 0–1. Owner caching is in-memory only (Task 1) — it does not survive an app restart. If we later persist `cache.owner` to `${app_data_dir}/github_cache/state.json`, the first-open-after-restart row collapses to `2 + N` too.
