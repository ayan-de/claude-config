//! Reads/writes the global `CLAUDE.md` file in Claude Code's config dir.
//!
//! Mirrors the `write_settings_atomic` pattern: tempfile + fsync + rename,
//! exclusive sidecar lock across the read-modify-write, and a timestamped
//! backup before every successful write. CLAUDE.md is more precious than
//! settings.json — `settings.json` can be rebuilt from the active provider,
//! but a half-written CLAUDE.md corrupts the user's global instructions with
//! no recovery path except git or a backup.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use tempfile::NamedTempFile;

use crate::models::{AppError, AppResult};

pub const CLAUDE_MD_FILENAME: &str = "CLAUDE.md";

/// Returns `<claude_dir>/CLAUDE.md`. Honors `CLAUDE_CONFIG_DIR`.
pub fn claude_md_path() -> PathBuf {
    crate::storage::discover_claude_dir().join(CLAUDE_MD_FILENAME)
}

/// Reads CLAUDE.md as a UTF-8 string.
///
/// - `Ok(None)` if the file does not exist.
/// - `Err(MalformedClaudeMd)` if it exists but contains invalid UTF-8
///   (`std::fs::read_to_string` returns an `InvalidData` io error in that case).
/// - `Ok(Some(content))` otherwise.
pub fn read_claude_md(path: &Path) -> AppResult<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    Ok(Some(content))
}

/// Atomic write with file-lock and backup. Same guarantees as
/// `write_settings_atomic`: tempfile + fsync + rename in the same dir, an
/// exclusive `.lock` sidecar held across the write, and a best-effort
/// timestamped backup under `<backups_dir>`.
pub fn write_claude_md_atomic(
    path: &Path,
    content: &str,
    backups_dir: &Path,
) -> AppResult<ClaudeMdBackup> {
    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!(
            "CLAUDE.md path has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;

    // Sidecar lock file so we don't hold a handle on the destination during
    // the rename — Windows MoveFileEx fails on locked targets. Same trick
    // write_settings_atomic uses.
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

    let backup_path = if path.exists() {
        match backup_current(path, backups_dir) {
            Ok(p) => Some(p),
            Err(e) => {
                log::warn!("CLAUDE.md backup failed (continuing): {e}");
                None
            }
        }
    } else {
        None
    };

    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(path) {
        return Err(AppError::Io(e.error));
    }

    let _ = lock_file.unlock();

    Ok(ClaudeMdBackup { backup_path })
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Public field for future "last backup" UI surfacing.
pub struct ClaudeMdBackup {
    pub backup_path: Option<PathBuf>,
}

fn backup_current(path: &Path, backups_dir: &Path) -> AppResult<PathBuf> {
    fs::create_dir_all(backups_dir)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dest = backups_dir.join(format!("CLAUDE.md-{ts}.bak"));
    fs::copy(path, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fresh_dir(name: &str) -> PathBuf {
        let d = tempfile::tempdir().unwrap().keep().join(name);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn read_claude_md_returns_none_when_missing() {
        let dir = fresh_dir("claude");
        let p = dir.join("CLAUDE.md");
        assert!(read_claude_md(&p).unwrap().is_none());
    }

    #[test]
    fn write_claude_md_atomic_creates_file_and_backup() {
        let dir = fresh_dir("claude");
        let p = dir.join("CLAUDE.md");
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"# old instructions\n").unwrap();
        drop(f);

        let backups = fresh_dir("backups");
        let res = write_claude_md_atomic(&p, "# new instructions\n", &backups).unwrap();

        let new_content = fs::read_to_string(&p).unwrap();
        assert_eq!(new_content, "# new instructions\n");
        assert!(res.backup_path.is_some());
        assert!(res.backup_path.unwrap().exists());
    }

    #[test]
    fn write_claude_md_atomic_writes_when_no_existing_file() {
        let dir = fresh_dir("claude");
        let p = dir.join("CLAUDE.md");
        let backups = fresh_dir("backups");
        let res = write_claude_md_atomic(&p, "fresh\n", &backups).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "fresh\n");
        // No existing file means nothing to back up.
        assert!(res.backup_path.is_none());
    }

    #[test]
    fn write_claude_md_atomic_overwrites_in_place() {
        // Second write should reflect the latest content, not append.
        let dir = fresh_dir("claude");
        let p = dir.join("CLAUDE.md");
        let backups = fresh_dir("backups");
        write_claude_md_atomic(&p, "first version that is longer", &backups).unwrap();
        write_claude_md_atomic(&p, "v2", &backups).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "v2");
    }

    #[test]
    fn backup_filename_includes_unix_ms() {
        // Sanity: the backup filename is recoverable for "when did this happen".
        let dir = fresh_dir("claude");
        let p = dir.join("CLAUDE.md");
        fs::write(&p, "old").unwrap();
        let backups = fresh_dir("backups");
        let res = write_claude_md_atomic(&p, "new", &backups).unwrap();
        let name = res.backup_path.unwrap().file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("CLAUDE.md-"));
        assert!(name.ends_with(".bak"));
    }
}
