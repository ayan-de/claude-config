//! Reads/writes Claude Code's `~/.claude/settings.json` (or wherever
//! `CLAUDE_CONFIG_DIR` points). All writes are atomic: temp file + fsync +
//! rename, with an exclusive file lock held across the read-modify-write to
//! guard against two app instances racing.
//!
//! A timestamped backup is created before every successful write so users
//! can recover if something later corrupts the file (not their fault; Claude
//! Code bugs, OS crash, etc.).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::models::{AppError, AppResult};

const SETTINGS_FILENAME: &str = "settings.json";

/// Returns the directory where Claude Code reads `settings.json` from.
///
/// Precedence:
/// 1. `CLAUDE_CONFIG_DIR` env var, if set and non-empty.
/// 2. `$HOME/.claude`
pub fn discover_claude_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claude")
}

pub fn settings_path() -> PathBuf {
    discover_claude_dir().join(SETTINGS_FILENAME)
}

/// Read settings.json. Returns:
/// - `Ok(None)` if file does not exist.
/// - `Err(MalformedSettings)` if it exists but cannot be parsed.
/// - `Ok(Some(value))` on success.
pub fn read_settings(path: &Path) -> AppResult<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(Some(Value::Object(Default::default())));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::MalformedSettings {
            path: path.display().to_string(),
            message: e.to_string(),
        }
    })?;
    Ok(Some(value))
}

/// Atomic write with file-lock and backup.
///
/// Steps:
/// 1. Open existing file (if any) for read; acquire `lock_exclusive`.
/// 2. Copy current contents to `<backups_dir>/<unix-ms>.json`.
/// 3. Serialize value to a NamedTempFile in the same directory.
/// 4. fsync the temp file; then `persist` (atomic rename).
/// 5. Release lock (drop file handle).
///
/// If `value` cannot be serialized as JSON, returns Err early and never
/// touches the file. If `path` doesn't exist, we still create it but skip
/// the backup step (nothing to back up).
pub fn write_settings_atomic(
    path: &Path,
    value: &Value,
    backups_dir: &Path,
) -> AppResult<SettingsBackup> {
    let json_bytes = serde_json::to_vec_pretty(value)?;

    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!(
            "settings path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;

    // Lock a sidecar file, not settings.json itself. On Windows, MoveFileEx
    // (which backs NamedTempFile::persist) fails with ERROR_ACCESS_DENIED
    // if the destination is open — including a read/lock handle held by
    // this same process. Locking a separate file avoids that entirely and
    // still serialises concurrent writers.
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

    // Backup current contents (best-effort: if read fails or path is missing,
    // we still proceed with the write — the user's settings.json wasn't
    // readable anyway, so a "backup of garbage" isn't useful).
    let backup_path = if path.exists() {
        match backup_current(path, backups_dir) {
            Ok(p) => Some(p),
            Err(e) => {
                log::warn!("backup failed (continuing): {e}");
                None
            }
        }
    } else {
        None
    };

    // Atomic write into same directory so rename is atomic on all platforms.
    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(&json_bytes)?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(path) {
        return Err(AppError::Io(e.error));
    }

    let _ = lock_file.unlock();

    Ok(SettingsBackup { backup_path })
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Public field for future "last backup" UI surfacing.
pub struct SettingsBackup {
    pub backup_path: Option<PathBuf>,
}

fn backup_current(path: &Path, backups_dir: &Path) -> AppResult<PathBuf> {
    fs::create_dir_all(backups_dir)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dest = backups_dir.join(format!("settings-{ts}.json"));
    fs::copy(path, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fresh_dir(name: &str) -> PathBuf {
        let d = tempfile::tempdir().unwrap().keep().join(name);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn read_settings_returns_none_when_missing() {
        let dir = fresh_dir("claude");
        let p = dir.join("settings.json");
        assert!(read_settings(&p).unwrap().is_none());
    }

    #[test]
    fn read_settings_returns_empty_object_for_zero_byte_file() {
        let dir = fresh_dir("claude");
        let p = dir.join("settings.json");
        fs::write(&p, "").unwrap();
        assert_eq!(read_settings(&p).unwrap(), Some(json!({})));
    }

    #[test]
    fn read_settings_reports_parse_error_for_malformed() {
        let dir = fresh_dir("claude");
        let p = dir.join("settings.json");
        fs::write(&p, "{not json").unwrap();
        let err = read_settings(&p).unwrap_err();
        match err {
            AppError::MalformedSettings { path, .. } => {
                assert!(path.contains("settings.json"));
            }
            other => panic!("expected MalformedSettings, got {other:?}"),
        }
    }

    #[test]
    fn write_settings_atomic_creates_file_and_backup() {
        let dir = fresh_dir("claude");
        let p = dir.join("settings.json");
        fs::write(
            &p,
            serde_json::to_string_pretty(&json!({
                "env": {"ANTHROPIC_BASE_URL": "https://old"},
                "hooks": {"Stop": []}
            }))
            .unwrap(),
        )
        .unwrap();
        let backups = fresh_dir("backups");
        let v = json!({
            "env": {"ANTHROPIC_BASE_URL": "https://new"},
            "hooks": {"Stop": []}
        });
        let res = write_settings_atomic(&p, &v, &backups).unwrap();
        let new_content = fs::read_to_string(&p).unwrap();
        assert!(new_content.contains("\"ANTHROPIC_BASE_URL\": \"https://new\""));
        assert!(new_content.contains("\"hooks\"")); // preserved
        assert!(res.backup_path.is_some());
        assert!(res.backup_path.unwrap().exists());
    }

    #[test]
    fn write_settings_atomic_writes_when_no_existing_file() {
        let dir = fresh_dir("claude");
        let p = dir.join("settings.json");
        let backups = fresh_dir("backups");
        let v = json!({"env": {"ANTHROPIC_BASE_URL": "https://x"}});
        let res = write_settings_atomic(&p, &v, &backups).unwrap();
        assert!(p.exists());
        assert!(res.backup_path.is_none());
    }

    #[test]
    fn discover_claude_dir_honors_env_var() {
        let prev = std::env::var("CLAUDE_CONFIG_DIR").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CLAUDE_CONFIG_DIR", tmp.path());
        let got = discover_claude_dir();
        match prev {
            Some(v) => std::env::set_var("CLAUDE_CONFIG_DIR", v),
            None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
        }
        assert_eq!(got, tmp.path());
    }
}