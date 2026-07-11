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
    let mut rows = tree_to_remote_sessions(&tree, |_| None);

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
}