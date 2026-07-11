//! Repository operations via the Git Data API.
//!
//! The Contents API is capped at 1MB on GET and ~50MB on PUT in practice,
//! which is too small for real Claude Code transcripts. We use the Git
//! Data API (blob → tree → commit → ref) for everything, so the same
//! code path handles small and large files uniformly.

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::github::client::{b64_decode, b64_encode, GitHubClient, GitHubError, GITHUB_API_BASE};
use crate::models::{ProjectRemoteMetadata, RemoteSessionSummary};

pub const DEFAULT_REPO_DESCRIPTION: &str =
    "Claude Code session transcripts synced from claude-config. Private — do not share.";

#[derive(Debug, Clone, Deserialize)]
pub struct RepoMetadata {
    pub name: String,
    pub full_name: String,
    pub default_branch: String,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub mode: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub sha: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tree {
    pub sha: String,
    #[serde(default)]
    pub tree: Vec<TreeEntry>,
    #[serde(default)]
    pub truncated: bool,
}

/// Single file in the repo, with decoded content. Used by the download
/// path: we list files via tree-recursive, then GET each blob.
#[derive(Debug, Clone)]
pub struct RepoFile {
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBlobResponse {
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTreeResponse {
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCommitResponse {
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRefResponse {
    #[serde(rename = "ref")]
    pub ref_field: String,
    pub object: RefObject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefObject {
    pub sha: String,
    #[serde(rename = "type")]
    pub object_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateBlobRequest<'a> {
    pub content: &'a str,
    pub encoding: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateTreeRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_tree: Option<&'a str>,
    pub tree: Vec<TreeItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeItem {
    pub path: String,
    pub mode: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub sha: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateCommitRequest<'a> {
    pub message: &'a str,
    pub tree: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateRefRequest<'a> {
    #[serde(rename = "ref")]
    pub ref_field: &'a str,
    pub sha: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchRefRequest<'a> {
    pub sha: &'a str,
    #[serde(default, skip_serializing_if = "bool_is_false")]
    pub force: bool,
}

fn bool_is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateRepoRequest<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub private: bool,
    pub auto_init: bool,
}

/// Returns the authenticated user's `login`. Used as the owner of the
/// session-sync repo (it's always under the user's own account).
pub fn get_authenticated_user(token: &str) -> Result<String, GitHubError> {
    #[derive(Deserialize)]
    struct User {
        login: String,
    }
    let u: User = GitHubClient::get_json(token, &format!("{GITHUB_API_BASE}/user"))?;
    Ok(u.login)
}

/// Check whether the repo exists, and capture its `default_branch`.
/// Returns `Ok(None)` if the repo is absent (404).
pub fn get_repo(token: &str, owner: &str, repo: &str) -> Result<Option<RepoMetadata>, GitHubError> {
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}");
    match GitHubClient::get_json::<RepoMetadata>(token, &url) {
        Ok(meta) => Ok(Some(meta)),
        Err(GitHubError::Http { status: 404, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Shortcut: returns `default_branch` if repo exists, `None` otherwise.
pub fn get_default_branch(token: &str, owner: &str, repo: &str) -> Result<Option<String>, GitHubError> {
    Ok(get_repo(token, owner, repo)?.map(|m| m.default_branch))
}

/// Create the private session-sync repo on the authenticated user's
/// account. Returns the new repo's metadata.
pub fn create_repo(token: &str, repo_name: &str) -> Result<RepoMetadata, GitHubError> {
    let req = CreateRepoRequest {
        name: repo_name,
        description: DEFAULT_REPO_DESCRIPTION,
        private: true,
        // ponytail: must be `true` here. GitHub's blob API returns
        // `409 Git Repository is empty` against a repo with no commits
        // and no branch, so `auto_init:false` would deadlock the upload
        // path on first use. Seeding a README + default branch lets us
        // commit on top as a normal parent (upload_files already handles
        // `head = Some(...)` and PATCHes the existing ref).
        auto_init: true,
    };
    let url = format!("{GITHUB_API_BASE}/user/repos");
    GitHubClient::post_json(token, &url, &req)
}

/// Returns the recursive tree of the default branch's HEAD. Each entry
/// has a `sha` we can pass to `get_blob` to download the file.
pub fn get_tree_recursive(
    token: &str,
    owner: &str,
    repo: &str,
    default_branch: &str,
) -> Result<Tree, GitHubError> {
    let url = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/git/trees/{default_branch}?recursive=1"
    );
    GitHubClient::get_json(token, &url)
}

/// Returns the current HEAD commit SHA for the default branch.
/// Returns `Ok(None)` when the branch has no commits yet (fresh repo).
pub fn get_branch_head(
    token: &str,
    owner: &str,
    repo: &str,
    default_branch: &str,
) -> Result<Option<String>, GitHubError> {
    // We only need `sha`. The shared `RefObject` requires a `type` field
    // that the `branches/{branch}` endpoint doesn't include in `commit`,
    // so a local struct with `#[serde(default)]` for everything else keeps
    // us tolerant of GitHub adding fields.
    #[derive(Deserialize)]
    struct BranchHead {
        #[serde(default)]
        commit: Option<CommitHead>,
    }
    #[derive(Deserialize)]
    struct CommitHead {
        sha: String,
    }
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/branches/{default_branch}");
    match GitHubClient::get_json::<BranchHead>(token, &url) {
        Ok(b) => Ok(b.commit.map(|c| c.sha)),
        Err(GitHubError::Http { status: 404, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Cheap SHA-only HEAD probe for the default branch, used by the cache
/// layer to gate `github_list_remote_sessions_cmd`. Hits
/// `GET /repos/{owner}/{repo}/git/ref/heads/{branch}` (singular) which
/// returns the current commit SHA in a tiny payload — no recursive
/// tree walk, no metadata blob fetches. Returns `Ok(None)` on 404
/// (branch missing or repo empty).
pub fn get_branch_ref_sha(
    token: &str,
    owner: &str,
    repo: &str,
    default_branch: &str,
) -> Result<Option<String>, GitHubError> {
    #[derive(Deserialize)]
    struct RefObjectHead {
        sha: String,
    }
    #[derive(Deserialize)]
    struct Ref {
        #[serde(default)]
        object: Option<RefObjectHead>,
    }
    let url = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/git/ref/heads/{default_branch}"
    );
    match GitHubClient::get_json::<Ref>(token, &url) {
        Ok(r) => Ok(r.object.map(|o| o.sha)),
        Err(GitHubError::Http { status: 404, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Fetch a commit's tree SHA. Used as `base_tree` when building the next
/// commit so we only send changed blobs and inherit everything else.
pub fn get_commit_tree_sha(
    token: &str,
    owner: &str,
    repo: &str,
    commit_sha: &str,
) -> Result<String, GitHubError> {
    #[derive(Deserialize)]
    struct TreeRef {
        sha: String,
    }
    #[derive(Deserialize)]
    struct Commit {
        tree: TreeRef,
    }
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/commits/{commit_sha}");
    let c: Commit = GitHubClient::get_json(token, &url)?;
    Ok(c.tree.sha)
}

/// Fetch a blob by SHA, returning decoded bytes. Supports up to 100MB.
pub fn get_blob(token: &str, owner: &str, repo: &str, sha: &str) -> Result<Vec<u8>, GitHubError> {
    #[derive(Deserialize)]
    struct Blob {
        content: String,
        encoding: String,
    }
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/blobs/{sha}");
    let blob = GitHubClient::get_json::<Blob>(token, &url)?;
    match blob.encoding.as_str() {
        "base64" => b64_decode(&blob.content),
        other => Err(GitHubError::Parse(format!(
            "unexpected blob encoding: {other}"
        ))),
    }
}

pub fn create_blob(token: &str, owner: &str, repo: &str, content: &[u8]) -> Result<String, GitHubError> {
    let req = CreateBlobRequest {
        content: &b64_encode(content),
        encoding: "base64",
    };
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/blobs");
    let resp: CreateBlobResponse = GitHubClient::post_json(token, &url, &req)?;
    Ok(resp.sha)
}

/// Build a tree from `base_tree` (the parent's tree SHA, or `None` for
/// first commit) and a list of file changes. Any path not in `tree`
/// but in `base_tree` is left untouched.
pub fn create_tree(
    token: &str,
    owner: &str,
    repo: &str,
    base_tree: Option<&str>,
    items: Vec<TreeItem>,
) -> Result<String, GitHubError> {
    let req = CreateTreeRequest { base_tree, tree: items };
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/trees");
    let resp: CreateTreeResponse = GitHubClient::post_json(token, &url, &req)?;
    Ok(resp.sha)
}

pub fn create_commit(
    token: &str,
    owner: &str,
    repo: &str,
    message: &str,
    tree_sha: &str,
    parents: Vec<String>,
) -> Result<String, GitHubError> {
    let req = CreateCommitRequest {
        message,
        tree: tree_sha,
        parents,
    };
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/commits");
    let resp: CreateCommitResponse = GitHubClient::post_json(token, &url, &req)?;
    Ok(resp.sha)
}

/// First-commit path: create the ref pointing at a commit with no parents.
pub fn create_ref(
    token: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    commit_sha: &str,
) -> Result<(), GitHubError> {
    let req = CreateRefRequest {
        ref_field: &format!("refs/heads/{branch}"),
        sha: commit_sha,
    };
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/refs");
    let _resp: CreateRefResponse = GitHubClient::post_json(token, &url, &req)?;
    Ok(())
}

/// Subsequent-commit path: move the existing ref.
pub fn update_ref(
    token: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    commit_sha: &str,
) -> Result<(), GitHubError> {
    let req = PatchRefRequest {
        sha: commit_sha,
        force: false,
    };
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/git/refs/heads/{branch}");
    GitHubClient::patch_json::<serde_json::Value, _>(token, &url, &req)?;
    Ok(())
}

/// Convert a recursive tree listing into `RemoteSessionSummary` records.
/// Skips `manifest.json` and per-project `metadata.json` — UI only needs
/// the session files themselves.
pub fn tree_to_remote_sessions(tree: &Tree, slug_for: impl Fn(&str) -> Option<String>) -> Vec<RemoteSessionSummary> {
    tree.tree
        .iter()
        .filter(|e| e.entry_type == "blob" && e.path.ends_with(".jsonl"))
        .filter_map(|e| {
            // expected layout: sessions/<project_slug>/<session_id>.jsonl
            let parts: Vec<&str> = e.path.split('/').collect();
            if parts.len() != 3 || parts[0] != "sessions" {
                return None;
            }
            let project_slug = parts[1].to_string();
            let filename = parts[2];
            let session_id = filename.trim_end_matches(".jsonl").to_string();
            let original_path = slug_for(&project_slug).unwrap_or_default();
            Some(RemoteSessionSummary {
                session_id,
                project_slug,
                original_path,
                title: None,           // populated from per-project metadata.json later
                modified: None,
                message_count: 0,
                sha: e.sha.clone(),
            })
        })
        .collect()
}

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

/// Per-slug delta between two trees. `changed` slugs need a fresh
/// `metadata.json` fetch; `removed` slugs had their last blob deleted
/// upstream and must be dropped from the spliced result rather than
/// silently left behind (otherwise download would 404 on a row the UI
/// thinks still exists).
#[derive(Debug, Default, Clone)]
pub struct SlugDiff {
    pub changed: std::collections::HashSet<String>,
    pub removed: std::collections::HashSet<String>,
}

/// Pull the project slug out of a tree entry's `sessions/<slug>/...`
/// path. Returns `None` for `manifest.json`, `README.md`, and anything
/// outside the `sessions/` prefix — diff_slugs deliberately ignores them.
fn slug_of(e: &TreeEntry) -> Option<String> {
    let parts: Vec<&str> = e.path.split('/').collect();
    if parts.len() >= 2 && parts[0] == "sessions" {
        Some(parts[1].to_string())
    } else {
        None
    }
}

/// Compute the per-slug delta between the previous successful tree and
/// the freshly-fetched tree. Both `changed` and `removed` are returned
/// in one pass so callers can't accidentally drop one of them.
pub fn diff_slugs(old: Option<&Tree>, new: &Tree) -> SlugDiff {
    let new_slugs: std::collections::HashSet<String> =
        new.tree.iter().filter_map(slug_of).collect();
    let Some(old) = old else {
        return SlugDiff {
            changed: new_slugs,
            removed: std::collections::HashSet::new(),
        };
    };
    let old_map: std::collections::HashMap<&str, &str> = old
        .tree
        .iter()
        .map(|e| (e.path.as_str(), e.sha.as_str()))
        .collect();
    let old_slugs: std::collections::HashSet<String> =
        old.tree.iter().filter_map(slug_of).collect();
    let changed: std::collections::HashSet<String> = new
        .tree
        .iter()
        .filter_map(|e| {
            let slug = slug_of(e)?;
            if old_map.get(e.path.as_str()) == Some(&e.sha.as_str()) {
                None
            } else {
                Some(slug)
            }
        })
        .collect();
    let removed: std::collections::HashSet<String> =
        old_slugs.difference(&new_slugs).cloned().collect();
    SlugDiff { changed, removed }
}

/// Thin wrapper kept for the no-cache call site: every slug is "changed"
/// (so each `metadata.json` gets fetched) and nothing is "removed."
pub fn list_remote_sessions(
    token: &str,
    owner: &str,
    repo: &str,
    default_branch: &str,
) -> Result<Vec<RemoteSessionSummary>, GitHubError> {
    let tree = get_tree_recursive(token, owner, repo, default_branch)?;
    let all_slugs: std::collections::HashSet<String> =
        tree.tree.iter().filter_map(slug_of).collect();
    let diff = SlugDiff {
        changed: all_slugs,
        removed: std::collections::HashSet::new(),
    };
    list_remote_sessions_with_diff(token, owner, repo, &tree, None, &diff)
}

/// Diff-aware variant used by the SHA-gated cache path. For slugs in
/// neither `diff.changed` nor `diff.removed`, the rows from `previous`
/// are spliced in unchanged — those `metadata.json` blobs don't need
/// to be refetched. `diff.removed` slugs are dropped from the spliced
/// result entirely so the UI doesn't surface a row that would 404 on
/// download.
pub fn list_remote_sessions_with_diff(
    token: &str,
    owner: &str,
    repo: &str,
    tree: &Tree,
    previous: Option<&[RemoteSessionSummary]>,
    diff: &SlugDiff,
) -> Result<Vec<RemoteSessionSummary>, GitHubError> {
    let mut rows = tree_to_remote_sessions(tree, |_| None);

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

    // Refetch metadata.json only for changed slugs. Unchanged slugs
    // are spliced from `previous` below.
    for (slug, sha) in &meta_shas {
        if !diff.changed.contains(slug) {
            continue;
        }
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

    // Drop rows whose slug was removed upstream — never splice stale
    // entries back in.
    rows.retain(|r| !diff.removed.contains(&r.project_slug));

    // For slugs that weren't changed AND weren't removed, splice the
    // previously-known row back in. Only do this when we actually have
    // a previous list to splice from.
    if let Some(prev) = previous {
        let splice_slugs: Vec<String> = meta_shas
            .keys()
            .filter(|s| !diff.changed.contains(*s))
            .cloned()
            .collect();
        for slug in splice_slugs {
            for prev_row in prev.iter().filter(|r| r.project_slug == slug) {
                if !rows.iter().any(|r| r.session_id == prev_row.session_id) {
                    rows.push(prev_row.clone());
                }
            }
        }
    }

    rows.sort_by(|a, b| {
        a.project_slug
            .cmp(&b.project_slug)
            .then_with(|| b.modified.cmp(&a.modified))
    });
    Ok(rows)
}

/// Encode JSON content as base64 for storage in blob API requests.
pub fn encode_blob_content(content: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `GET /repos/.../branches/{branch}` returns a 4KB+
    /// payload whose inner `commit` object has `sha` but no `type` field
    /// at that level (only the *git object's* `type` exists, nested under
    /// `commit.commit.tree`). The previous `RefObject`-based struct
    /// required `type` and blew up with a JSON parse error.
    #[test]
    fn branch_head_payload_with_no_commit_type_parses() {
        let payload = r#"
        {
            "name": "main",
            "commit": {
                "sha": "abcdef1234567890abcdef1234567890abcdef12",
                "node_id": "C_abc",
                "commit": {
                    "tree": {"sha": "t", "url": "u"},
                    "author": null,
                    "committer": null,
                    "message": "init"
                },
                "url": "https://api.github.com/repos/o/r/commits/abcdef",
                "html_url": "https://github.com/o/r/commit/abcdef",
                "comments_url": "https://api.github.com/repos/o/r/commits/abcdef/comments",
                "author": null,
                "committer": null,
                "parents": []
            },
            "_links": {"self": "x", "html": "y"},
            "protected": false,
            "protection": {"enabled": false, "required_status_checks": null},
            "protection_url": "https://api.github.com/repos/o/r/branches/main/protection"
        }
        "#;

        #[derive(serde::Deserialize)]
        struct CommitHead {
            sha: String,
        }
        #[derive(serde::Deserialize)]
        struct BranchHead {
            #[serde(default)]
            commit: Option<CommitHead>,
        }
        let b: BranchHead = serde_json::from_str(payload).expect("must parse");
        assert_eq!(
            b.commit.unwrap().sha,
            "abcdef1234567890abcdef1234567890abcdef12"
        );
    }

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
        let bare = tree_to_remote_sessions(&tree, |_| None);
        assert_eq!(bare.len(), 3);
        let ids: Vec<&str> = bare.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"uuid-1"));
        assert!(ids.contains(&"uuid-2"));
        assert!(ids.contains(&"uuid-3"));
    }

    // ---- get_branch_ref_sha payload shape ----
    //
    // The function is a thin HTTP wrapper, but the SHA-gate in
    // commands::github_sync depends on the response shape being stable.
    // Lock it here so a GitHub API drift shows up as a unit-test failure
    // rather than a silent cache always-invalidating.

    #[derive(Deserialize)]
    struct RefObjectHead {
        sha: String,
    }
    #[derive(Deserialize)]
    struct Ref {
        #[serde(default)]
        object: Option<RefObjectHead>,
    }

    #[test]
    fn ref_sha_payload_deserializes_sha_from_object() {
        let payload = r#"{
            "ref": "refs/heads/main",
            "node_id": "MDM6UmVmMjM0NTY3ODk6bWFpbg==",
            "url": "https://api.github.com/repos/o/r/git/refs/heads/main",
            "object": {
                "sha": "abc1234567890abcdef1234567890abcdef12345",
                "type": "commit",
                "url": "https://api.github.com/repos/o/r/git/commits/abc1234567890abcdef1234567890abcdef12345"
            }
        }"#;
        let r: Ref = serde_json::from_str(payload).expect("must parse");
        assert_eq!(
            r.object.unwrap().sha,
            "abc1234567890abcdef1234567890abcdef12345"
        );
    }

    #[test]
    fn ref_sha_payload_missing_object_is_none() {
        // Defensive: a malformed payload should not panic — `object`
        // is Option so we degrade to None and the SHA-gate falls
        // through to a full refetch.
        let payload = r#"{ "ref": "refs/heads/main", "node_id": "x", "url": "u" }"#;
        let r: Ref = serde_json::from_str(payload).expect("must parse");
        assert!(r.object.is_none());
    }

    // ---- diff_slugs ----

    fn parse_tree(json: &str) -> Tree {
        serde_json::from_str(json).expect("tree parses")
    }

    fn tree_with(paths: &[&str]) -> Tree {
        // Build a tree where each path's SHA is derived from the path
        // bytes themselves (so two trees containing the same path at
        // the same logical position produce the same SHA — which is
        // what we want for "no diff" tests — while still being unique
        // per path). This makes diff tests deterministic without
        // depending on input ordering.
        let entries: Vec<TreeEntry> = paths
            .iter()
            .map(|p| TreeEntry {
                path: (*p).to_string(),
                mode: "100644".into(),
                entry_type: "blob".into(),
                sha: {
                    // Cheap deterministic SHA — first 40 hex chars of a
                    // simple FNV-1a-style fold. Test-only.
                    let mut h: u64 = 1469598103934665603;
                    for b in p.bytes() {
                        h ^= b as u64;
                        h = h.wrapping_mul(1099511628211);
                    }
                    format!("{h:016x}{:024x}", h.wrapping_mul(31))
                },
                size: Some(10),
            })
            .collect();
        Tree {
            sha: "root".into(),
            tree: entries,
            truncated: false,
        }
    }

    #[test]
    fn diff_no_old_tree_every_slug_changed_nothing_removed() {
        let new = tree_with(&[
            "sessions/-home-foo/uuid-1.jsonl",
            "sessions/-home-foo/metadata.json",
            "sessions/-home-bar/uuid-2.jsonl",
            "sessions/-home-bar/metadata.json",
        ]);
        let diff = diff_slugs(None, &new);
        assert_eq!(diff.changed.len(), 2);
        assert!(diff.changed.contains("-home-foo"));
        assert!(diff.changed.contains("-home-bar"));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_identical_trees_no_change_no_remove() {
        let tree = tree_with(&[
            "sessions/-home-foo/uuid-1.jsonl",
            "sessions/-home-foo/metadata.json",
        ]);
        let diff = diff_slugs(Some(&tree), &tree);
        assert!(diff.changed.is_empty());
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_new_session_in_one_project_only_that_slug_changed() {
        let old = tree_with(&["sessions/-home-foo/uuid-1.jsonl", "sessions/-home-foo/metadata.json"]);
        let new = tree_with(&[
            "sessions/-home-foo/uuid-1.jsonl",
            "sessions/-home-foo/uuid-2.jsonl",
            "sessions/-home-foo/metadata.json",
        ]);
        let diff = diff_slugs(Some(&old), &new);
        assert_eq!(diff.changed.len(), 1);
        assert!(diff.changed.contains("-home-foo"));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_two_projects_change_both_in_changed() {
        let old = tree_with(&[
            "sessions/-home-foo/uuid-1.jsonl",
            "sessions/-home-foo/metadata.json",
            "sessions/-home-bar/uuid-2.jsonl",
            "sessions/-home-bar/metadata.json",
        ]);
        let new = tree_with(&[
            "sessions/-home-foo/uuid-1.jsonl",
            "sessions/-home-foo/uuid-99.jsonl",
            "sessions/-home-foo/metadata.json",
            "sessions/-home-bar/uuid-2.jsonl",
            "sessions/-home-bar/uuid-100.jsonl",
            "sessions/-home-bar/metadata.json",
        ]);
        let diff = diff_slugs(Some(&old), &new);
        assert_eq!(diff.changed.len(), 2);
        assert!(diff.changed.contains("-home-foo"));
        assert!(diff.changed.contains("-home-bar"));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_root_manifest_change_does_not_appear_in_diff() {
        let old = tree_with(&["manifest.json", "sessions/-home-foo/uuid-1.jsonl"]);
        let new = tree_with(&["manifest.json", "sessions/-home-foo/uuid-1.jsonl"]);
        // Mutate just the manifest SHA so the SHA comparison flips.
        let mut new_mut = new;
        new_mut.tree[0].sha = "ff".repeat(20);
        let diff = diff_slugs(Some(&old), &new_mut);
        assert!(diff.changed.is_empty(), "manifest.json is outside sessions/");
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_whole_project_removed_appears_in_removed_not_changed() {
        let old = tree_with(&[
            "manifest.json",
            "sessions/-home-foo/uuid-1.jsonl",
            "sessions/-home-foo/metadata.json",
            "sessions/-home-bar/uuid-2.jsonl",
            "sessions/-home-bar/metadata.json",
        ]);
        let new = tree_with(&[
            "manifest.json",
            "sessions/-home-bar/uuid-2.jsonl",
            "sessions/-home-bar/metadata.json",
        ]);
        let diff = diff_slugs(Some(&old), &new);
        assert!(diff.changed.is_empty(), "no project was modified, only one was deleted");
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.removed.contains("-home-foo"));
    }

    #[test]
    fn diff_splice_previous_drops_removed_project_rows() {
        // Caller-level test: list_remote_sessions_with_diff with a
        // `removed` slug must not include those rows in the result,
        // even if `previous` contained them.
        let prev = vec![
            RemoteSessionSummary {
                session_id: "uuid-1".into(),
                project_slug: "-home-foo".into(),
                original_path: String::new(),
                title: None,
                modified: None,
                message_count: 0,
                sha: "a".repeat(40),
            },
            RemoteSessionSummary {
                session_id: "uuid-2".into(),
                project_slug: "-home-bar".into(),
                original_path: String::new(),
                title: None,
                modified: None,
                message_count: 0,
                sha: "b".repeat(40),
            },
        ];
        let new = tree_with(&["sessions/-home-bar/uuid-2.jsonl"]);
        let mut diff = SlugDiff::default();
        diff.removed.insert("-home-foo".into());
        let rows =
            list_remote_sessions_with_diff("", "o", "r", &new, Some(&prev), &diff).unwrap();
        let slugs: Vec<&str> = rows.iter().map(|r| r.project_slug.as_str()).collect();
        assert!(!slugs.contains(&"-home-foo"), "removed slug rows must not appear");
        assert!(slugs.contains(&"-home-bar"));
    }
}