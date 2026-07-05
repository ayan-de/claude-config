//! Reads/writes Claude Code's `~/.claude/.credentials.json`.
//!
//! This file holds OAuth session state for `claude /login` — specifically the
//! `claudeAiOauth` object with access/refresh tokens for a Claude
//! subscription. It may also contain a `mcpOAuth` map for MCP-server OAuth
//! sessions; we preserve that top-level key verbatim on every write.
//!
//! Writes are atomic (tempfile + fsync + rename), guarded by a sidecar file
//! lock to prevent two app instances from racing. The pattern mirrors
//! `storage::settings::write_settings_atomic` — we don't share the code
//! because the file matters enough that clarity beats DRY.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use fs2::FileExt;
use serde_json::{Map, Value};
use tempfile::NamedTempFile;

use crate::models::{AppError, AppResult};
use crate::storage::settings::discover_claude_dir;

const CREDENTIALS_FILENAME: &str = ".credentials.json";
const OAUTH_KEY: &str = "claudeAiOauth";

/// Path Claude Code reads its OAuth credentials from. Honours
/// `CLAUDE_CONFIG_DIR` via `discover_claude_dir`.
pub fn credentials_path() -> PathBuf {
    discover_claude_dir().join(CREDENTIALS_FILENAME)
}

/// Read the `claudeAiOauth` object from `.credentials.json`. Returns:
/// - `Ok(None)` if the file is absent, empty, or has no `claudeAiOauth` key.
/// - `Err(MalformedSettings)` if the file exists but doesn't parse.
/// - `Ok(Some(oauth))` on success.
pub fn read_credentials_oauth() -> AppResult<Option<Value>> {
    read_credentials_oauth_at(&credentials_path())
}

/// Path-taking variant for tests (avoids racing on `CLAUDE_CONFIG_DIR`).
fn read_credentials_oauth_at(path: &std::path::Path) -> AppResult<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let raw: Value = serde_json::from_slice(&bytes).map_err(|e| AppError::MalformedSettings {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(raw.get(OAUTH_KEY).cloned())
}

/// Atomically write the `claudeAiOauth` object into `.credentials.json`,
/// preserving every other top-level key that was already there (notably
/// `mcpOAuth`). Creates the file if missing.
pub fn write_credentials_oauth(oauth: &Value) -> AppResult<()> {
    write_credentials_oauth_at(&credentials_path(), oauth)
}

/// Path-taking variant for tests.
fn write_credentials_oauth_at(path: &std::path::Path, oauth: &Value) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!("credentials path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent)?;

    // Sidecar lock — mirrors settings.rs.
    let lock_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".lock");
        PathBuf::from(p)
    };
    let lock_file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock_file
        .lock_exclusive()
        .map_err(|e| AppError::Lock(e.to_string()))?;

    // Merge into existing structure so mcpOAuth and any other top-level keys survive.
    let mut root: Map<String, Value> = if path.exists() {
        let bytes = fs::read(path)?;
        if bytes.is_empty() {
            Map::new()
        } else {
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(Value::Object(m)) => m,
                Ok(_) => {
                    // Root isn't an object — treat as corrupted and rewrite fresh
                    // rather than crash. The previous non-object content is lost;
                    // that's fine because Claude Code always writes an object.
                    log::warn!(
                        ".credentials.json root was not an object; rewriting from scratch"
                    );
                    Map::new()
                }
                Err(e) => {
                    return Err(AppError::MalformedSettings {
                        path: path.display().to_string(),
                        message: e.to_string(),
                    });
                }
            }
        }
    } else {
        Map::new()
    };

    root.insert(OAUTH_KEY.into(), oauth.clone());

    let json_bytes = serde_json::to_vec_pretty(&Value::Object(root))?;
    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(&json_bytes)?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(path) {
        return Err(AppError::Io(e.error));
    }

    let _ = lock_file.unlock();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn fresh_creds_path(name: &str) -> PathBuf {
        let tmp = tempfile::tempdir().unwrap().keep();
        let dir = tmp.join(name);
        fs::create_dir_all(&dir).unwrap();
        dir.join(".credentials.json")
    }

    #[test]
    fn read_returns_none_when_missing() {
        let p = fresh_creds_path("no-creds");
        assert!(read_credentials_oauth_at(&p).unwrap().is_none());
    }

    #[test]
    fn read_returns_none_when_oauth_key_missing() {
        let p = fresh_creds_path("only-mcp");
        fs::write(&p, r#"{"mcpOAuth": {}}"#).unwrap();
        assert!(read_credentials_oauth_at(&p).unwrap().is_none());
    }

    #[test]
    fn read_extracts_oauth_object() {
        let p = fresh_creds_path("with-oauth");
        fs::write(
            &p,
            r#"{"claudeAiOauth": {"accessToken": "abc", "refreshToken": "xyz"}, "mcpOAuth": {}}"#,
        )
        .unwrap();
        let oauth = read_credentials_oauth_at(&p).unwrap().unwrap();
        assert_eq!(oauth["accessToken"], "abc");
        assert_eq!(oauth["refreshToken"], "xyz");
    }

    #[test]
    fn write_preserves_other_top_level_keys() {
        let p = fresh_creds_path("preserve");
        fs::write(&p, r#"{"mcpOAuth": {"figma": {"clientId": "abc"}}}"#).unwrap();
        let new_oauth = json!({"accessToken": "new", "refreshToken": "new-r"});
        write_credentials_oauth_at(&p, &new_oauth).unwrap();

        let raw = fs::read_to_string(&p).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["claudeAiOauth"]["accessToken"], "new");
        assert_eq!(parsed["mcpOAuth"]["figma"]["clientId"], "abc");
    }

    #[test]
    fn write_creates_file_when_missing() {
        let p = fresh_creds_path("create");
        assert!(!p.exists());
        let oauth = json!({"accessToken": "a"});
        write_credentials_oauth_at(&p, &oauth).unwrap();
        assert!(p.exists());
        let raw = fs::read_to_string(&p).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["claudeAiOauth"]["accessToken"], "a");
    }

    #[test]
    fn write_overwrites_existing_oauth_but_keeps_other_keys() {
        let p = fresh_creds_path("overwrite");
        fs::write(
            &p,
            r#"{"claudeAiOauth": {"accessToken": "old"}, "mcpOAuth": {"x": 1}}"#,
        )
        .unwrap();
        let new_oauth = json!({"accessToken": "new"});
        write_credentials_oauth_at(&p, &new_oauth).unwrap();
        let parsed: Value =
            serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(parsed["claudeAiOauth"]["accessToken"], "new");
        assert_eq!(parsed["mcpOAuth"]["x"], 1);
    }
}
