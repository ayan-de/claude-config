//! In-memory + disk caches for GitHub API responses.
//!
//! Split into two tiers:
//!
//! 1. `GitHubCache` (in-memory, on `AppState`) — owner login, default
//!    branch, the last seen commit SHA on the default branch, the last
//!    successful tree, and the last successful list of remote sessions.
//!    All read-mostly, all per-token, all change-at-most-once-per-login.
//!    Cleared on `github_disconnect_cmd` and on a fresh OAuth login.
//!
//! 2. Blob disk cache (`${app_data_dir}/github_cache/blobs/{sha}`) —
//!    content-addressed by SHA-1, byte-correct forever because GitHub's
//!    blob API is content-addressed. Subject to a size cap so
//!    "immutable, so cache forever" doesn't quietly become "sensitive
//!    plaintext lives on this disk indefinitely."
//!
//! See `docs/superpowers/plans/2026-07-11-remote-sessions-caching.md`.

use std::path::{Path, PathBuf};

use crate::github::repo::Tree;
use crate::models::{AppError, AppResult, RemoteSessionSummary};

/// Default blob-cache size cap. Override with `GITHUB_BLOB_CACHE_MAX_MB`.
const DEFAULT_CACHE_MAX_MB: u64 = 200;

/// SHA-1 is 40 hex chars. We never let any user-controlled string reach
/// the filesystem as a path without this match.
const SHA1_RE: &str = r"^[0-9a-f]{40}$";

/// All non-secret state we keep about the GitHub connection in memory.
/// `Arc<Mutex<GitHubCache>>` lives on `AppState`.
#[derive(Default)]
pub struct GitHubCache {
    /// GitHub `login` for the connected user. Seeded from the OAuth
    /// device-flow response so the first tab open after connecting
    /// doesn't pay the `GET /user` round-trip.
    pub owner: Option<String>,
    /// Default branch of the sync repo (almost always `main`). Cached
    /// alongside owner; they are invalidated together.
    pub default_branch: Option<String>,
    /// Last seen SHA of the default branch's HEAD ref. The SHA-gate in
    /// `github_list_remote_sessions_cmd` compares the live ref SHA
    /// against this and skips the full tree walk on a match.
    pub commit_sha: Option<String>,
    /// Last successful full tree walk, kept around so the tree-diff
    /// path can compute per-slug changes when the SHA shifts.
    pub tree: Option<Tree>,
    /// Last successful `Vec<RemoteSessionSummary>`. Returned verbatim
    /// when the SHA-gate matches.
    pub sessions_list: Option<Vec<RemoteSessionSummary>>,
}

impl GitHubCache {
    pub fn clear(&mut self) {
        self.owner = None;
        self.default_branch = None;
        self.commit_sha = None;
        self.tree = None;
        self.sessions_list = None;
    }
}

pub fn blob_cache_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("github_cache").join("blobs")
}

fn is_valid_sha(s: &str) -> bool {
    // Anchor-and-length regex is overkill, so use a manual scan — avoids
    // pulling `regex` into the dep tree for one 40-char check.
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub fn get_cached_blob(app_data_dir: &Path, sha: &str) -> Option<Vec<u8>> {
    if !is_valid_sha(sha) {
        return None;
    }
    std::fs::read(blob_cache_dir(app_data_dir).join(sha)).ok()
}

pub fn put_cached_blob(app_data_dir: &Path, sha: &str, bytes: &[u8]) -> AppResult<()> {
    if !is_valid_sha(sha) {
        return Err(AppError::Validation(format!(
            "blob sha must match {SHA1_RE}: {sha}"
        )));
    }
    let dir = blob_cache_dir(app_data_dir);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(sha);
    // Match the project's standard atomic pattern (see
    // `storage/github_sync.rs::write_session_sync_state_atomic`): write
    // to a sibling temp file in the same directory, fsync, then rename.
    // Avoids leaving a half-written blob file the next crash would
    // happily reuse.
    let tmp = tempfile::NamedTempFile::new_in(&dir)?;
    use std::io::Write;
    {
        let mut f = tmp.as_file();
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    tmp.persist(&dest).map_err(|e| {
        AppError::Internal(format!("blob cache persist failed: {e}"))
    })?;
    Ok(())
}

/// Best-effort wipe. Disconnect path swallows IO errors and logs them
/// upstream so the parent command still succeeds.
pub fn clear_blob_cache(app_data_dir: &Path) -> AppResult<()> {
    let dir = blob_cache_dir(app_data_dir);
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Per-blob budget. Reads `GITHUB_BLOB_CACHE_MAX_MB` if set, otherwise
/// `DEFAULT_CACHE_MAX_MB`.
pub fn cache_cap_bytes() -> u64 {
    match std::env::var("GITHUB_BLOB_CACHE_MAX_MB") {
        Ok(s) => s.parse::<u64>().ok().map(|mb| mb * 1024 * 1024).unwrap_or(DEFAULT_CACHE_MAX_MB * 1024 * 1024),
        Err(_) => DEFAULT_CACHE_MAX_MB * 1024 * 1024,
    }
}

/// LRU eviction by mtime. Deletes oldest first until the directory is
/// back under `max_bytes`. No-op when already under cap.
pub fn enforce_blob_cache_size(app_data_dir: &Path, max_bytes: u64) -> AppResult<()> {
    let dir = blob_cache_dir(app_data_dir);
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<(PathBuf, std::time::SystemTime, u64)> = Vec::new();
    let mut total: u64 = 0;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();
        let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        total += size;
        entries.push((path, mtime, size));
    }
    if total <= max_bytes {
        return Ok(());
    }
    // Oldest first.
    entries.sort_by_key(|(_, mtime, _)| *mtime);
    for (path, _, size) in entries {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

/// Counts and total bytes for surfacing to a future settings-page UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobCacheStats {
    pub file_count: u64,
    pub total_bytes: u64,
}

pub fn blob_cache_stats(app_data_dir: &Path) -> BlobCacheStats {
    let dir = blob_cache_dir(app_data_dir);
    let mut file_count: u64 = 0;
    let mut total_bytes: u64 = 0;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    file_count += 1;
                    total_bytes += meta.len();
                }
            }
        }
    }
    BlobCacheStats { file_count, total_bytes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let c = GitHubCache::default();
        assert!(c.owner.is_none());
        assert!(c.default_branch.is_none());
        assert!(c.commit_sha.is_none());
        assert!(c.tree.is_none());
        assert!(c.sessions_list.is_none());
    }

    #[test]
    fn clear_empties_every_field() {
        let mut c = GitHubCache::default();
        c.owner = Some("octocat".into());
        c.default_branch = Some("main".into());
        c.commit_sha = Some("a".repeat(40));
        c.clear();
        assert!(c.owner.is_none());
        assert!(c.default_branch.is_none());
        assert!(c.commit_sha.is_none());
    }

    #[test]
    fn seeding_owner_only_leaves_default_branch_none() {
        let mut c = GitHubCache::default();
        c.owner = Some("octocat".into());
        assert!(c.default_branch.is_none());
        assert!(c.commit_sha.is_none());
    }

    #[test]
    fn get_cached_blob_returns_none_for_missing_sha() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(get_cached_blob(tmp.path(), &"b".repeat(40)).is_none());
    }

    #[test]
    fn put_then_get_roundtrips_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let payload = b"hello world";
        put_cached_blob(tmp.path(), sha, payload).unwrap();
        let got = get_cached_blob(tmp.path(), sha).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn put_rejects_non_hex_sha() {
        let tmp = tempfile::tempdir().unwrap();
        // 41 chars — wrong length.
        let bad = "z".repeat(41);
        assert!(put_cached_blob(tmp.path(), &bad, b"x").is_err());
    }

    #[test]
    fn put_rejects_uppercase_hex_sha() {
        // Repo blob SHAs are always lowercase; uppercase is either
        // user error or an injection attempt — both rejected.
        let tmp = tempfile::tempdir().unwrap();
        let bad = "A".repeat(40);
        assert!(put_cached_blob(tmp.path(), &bad, b"x").is_err());
    }

    #[test]
    fn clear_blob_cache_removes_every_file_but_leaves_dir() {
        let tmp = tempfile::tempdir().unwrap();
        for sha in ["a".repeat(40).as_str(), "b".repeat(40).as_str()] {
            put_cached_blob(tmp.path(), sha, b"x").unwrap();
        }
        assert_eq!(blob_cache_stats(tmp.path()).file_count, 2);
        clear_blob_cache(tmp.path()).unwrap();
        assert_eq!(blob_cache_stats(tmp.path()).file_count, 0);
        assert!(blob_cache_dir(tmp.path()).exists());
    }

    #[test]
    fn enforce_cache_size_deletes_oldest_first() {
        let tmp = tempfile::tempdir().unwrap();
        // Three blobs, 10 bytes each. Cap = 15 → oldest gets evicted,
        // two newest survive (20 bytes total).
        let shas = ["a".repeat(40), "b".repeat(40), "c".repeat(40)];
        // Base time shifted well into the past so subsequent bumps are
        // strictly increasing regardless of filesystem mtime granularity.
        let base = std::time::SystemTime::now() - std::time::Duration::from_secs(10);
        for (i, sha) in shas.iter().enumerate() {
            put_cached_blob(tmp.path(), sha, &vec![b'x'; 10]).unwrap();
            // Force mtime to a deterministic value per file.
            let target = base + std::time::Duration::from_secs(i as u64);
            std::fs::OpenOptions::new()
                .write(true)
                .open(blob_cache_dir(tmp.path()).join(sha))
                .unwrap()
                .set_modified(target)
                .unwrap();
        }
        enforce_blob_cache_size(tmp.path(), 15).unwrap();
        let stats = blob_cache_stats(tmp.path());
        // 30 bytes total, cap 15 → eviction loop deletes until <=15,
        // so exactly one 10-byte file survives.
        assert_eq!(stats.total_bytes, 10, "newest survives");
        assert_eq!(stats.file_count, 1);
        assert!(get_cached_blob(tmp.path(), &shas[0]).is_none());
        assert!(get_cached_blob(tmp.path(), &shas[1]).is_none());
        assert!(get_cached_blob(tmp.path(), &shas[2]).is_some());
    }

    #[test]
    fn enforce_cache_size_is_noop_under_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = "a".repeat(40);
        put_cached_blob(tmp.path(), &sha, &vec![b'x'; 10]).unwrap();
        enforce_blob_cache_size(tmp.path(), 1024).unwrap();
        assert_eq!(blob_cache_stats(tmp.path()).file_count, 1);
    }
}