//! Upload orchestration on top of the Git Data API primitives in
//! `repo.rs`. One `upload_files` call = one atomic commit that creates
//! or updates any number of files under the session-sync repo.
//!
//! The unit of conflict is the branch ref (a commit is atomic), so we
//! detect concurrent uploads by re-checking the ref right before moving
//! it and rebuilding against the new head on a race. See `MAX_ATTEMPTS`.

use std::collections::HashMap;

use crate::github::client::GitHubError;
use crate::github::repo::{
    self, TreeItem,
};

/// Regular file mode for tree entries. GitHub only accepts a fixed set;
/// `100644` is a non-executable blob.
const FILE_MODE: &str = "100644";

/// How many times we rebuild-and-retry when someone else moves the ref
/// out from under us between building the tree and updating the ref.
const MAX_ATTEMPTS: usize = 3;

/// One file to write in the commit: its repo-relative path and raw bytes.
pub struct UploadFile {
    pub path: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    /// repo path -> created blob SHA. The session's own blob SHA is what
    /// we persist as `remote_sha` for conflict detection.
    pub blob_shas: HashMap<String, String>,
    pub commit_sha: String,
    pub default_branch: String,
}

/// Ensure the session-sync repo exists on the user's account, returning
/// its `default_branch`. Creates it (private) on first use.
pub fn ensure_repo(
    token: &str,
    owner: &str,
    repo_name: &str,
) -> Result<String, GitHubError> {
    match repo::get_repo(token, owner, repo_name)? {
        Some(meta) => Ok(meta.default_branch),
        None => Ok(repo::create_repo(token, repo_name)?.default_branch),
    }
}

/// Fetch the bytes of a file at `path` on `branch`, or `None` if absent.
/// Used to merge into an existing per-project `metadata.json` before
/// re-uploading it in the same commit as the session.
pub fn fetch_existing_file(
    token: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    path: &str,
) -> Result<Option<Vec<u8>>, GitHubError> {
    // A branch with no commits yet has no tree — treat as empty repo.
    let tree = match repo::get_branch_head(token, owner, repo, branch)? {
        Some(_) => repo::get_tree_recursive(token, owner, repo, branch)?,
        None => return Ok(None),
    };
    let Some(entry) = tree
        .tree
        .iter()
        .find(|e| e.entry_type == "blob" && e.path == path)
    else {
        return Ok(None);
    };
    Ok(Some(repo::get_blob(token, owner, repo, &entry.sha)?))
}

/// Create/update `files` in a single commit on the repo's default branch.
/// Handles the first-commit (no parent, no base_tree) and subsequent-commit
/// paths, and retries on a concurrent ref move.
pub fn upload_files(
    token: &str,
    owner: &str,
    repo_name: &str,
    message: &str,
    files: &[UploadFile],
) -> Result<UploadResult, GitHubError> {
    let default_branch = ensure_repo(token, owner, repo_name)?;

    // Blobs are content-addressed and don't depend on the parent commit,
    // so we can create them once outside the retry loop.
    let mut blob_shas: HashMap<String, String> = HashMap::new();
    let mut items: Vec<TreeItem> = Vec::with_capacity(files.len());
    for f in files {
        let sha = repo::create_blob(token, owner, repo_name, &f.content)?;
        blob_shas.insert(f.path.clone(), sha.clone());
        items.push(TreeItem {
            path: f.path.clone(),
            mode: FILE_MODE.to_string(),
            entry_type: "blob".to_string(),
            sha,
        });
    }

    let mut last_err: Option<GitHubError> = None;
    for _ in 0..MAX_ATTEMPTS {
        // Snapshot the current head (None => fresh repo, first commit).
        let head = repo::get_branch_head(token, owner, repo_name, &default_branch)?;
        let base_tree = match &head {
            Some(sha) => Some(repo::get_commit_tree_sha(token, owner, repo_name, sha)?),
            None => None,
        };

        let tree_sha = repo::create_tree(
            token,
            owner,
            repo_name,
            base_tree.as_deref(),
            items.clone(),
        )?;

        let parents = head.clone().into_iter().collect::<Vec<_>>();
        let commit_sha =
            repo::create_commit(token, owner, repo_name, message, &tree_sha, parents)?;

        // Move the ref. First commit creates it; otherwise PATCH. On a
        // race (ref already exists / moved), fall through to retry.
        let moved = match &head {
            None => repo::create_ref(token, owner, repo_name, &default_branch, &commit_sha),
            Some(_) => repo::update_ref(token, owner, repo_name, &default_branch, &commit_sha),
        };

        match moved {
            Ok(()) => {
                return Ok(UploadResult {
                    blob_shas,
                    commit_sha,
                    default_branch,
                });
            }
            // 422 on create_ref = ref appeared underneath us; on update_ref
            // = fast-forward rejected because head moved. Both are races we
            // recover from by rebuilding against the new head.
            Err(GitHubError::Http { status: 422, .. }) => {
                last_err = Some(GitHubError::Http {
                    status: 422,
                    body: "ref moved during upload; retrying".into(),
                });
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        GitHubError::Parse("upload failed after retries with no error captured".into())
    }))
}
