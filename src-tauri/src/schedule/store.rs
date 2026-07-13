//! Reads/writes `schedules.json` in the app's data dir. Same lock-free atomic
//! write pattern as `storage::providers` (tempfile + fsync + rename). Contains
//! no secrets.

use std::fs;
use std::path::Path;

use tempfile::NamedTempFile;

use crate::models::{AppError, AppResult, SchedulesFile};

#[allow(dead_code)] // Documented constant for external callers.
pub const SCHEDULES_FILENAME: &str = "schedules.json";

/// Load `schedules.json`. Returns `SchedulesFile::default()` when the file is
/// missing or empty (first launch).
pub fn load_schedules_file(path: &Path) -> AppResult<SchedulesFile> {
    if !path.exists() {
        return Ok(SchedulesFile::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(SchedulesFile::default());
    }
    let file: SchedulesFile =
        serde_json::from_slice(&bytes).map_err(|e| AppError::MalformedSettings {
            path: path.display().to_string(),
            message: format!("schedules.json: {e}"),
        })?;
    Ok(file)
}

/// Atomic write of `schedules.json`.
pub fn save_schedules_file(path: &Path, file: &SchedulesFile) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(file)?;
    let parent = path.parent().ok_or_else(|| {
        AppError::Validation(format!("schedules path has no parent: {}", path.display()))
    })?;
    let mut tmp = NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, &bytes)?;
    tmp.as_file().sync_all()?;
    if let Err(e) = tmp.persist(path) {
        return Err(AppError::Io(e.error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Schedule, Weekday};

    fn fresh_dir(name: &str) -> std::path::PathBuf {
        let d = tempfile::tempdir().unwrap().keep().join(name);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample(id: &str) -> Schedule {
        Schedule {
            id: id.into(),
            label: Some("Morning".into()),
            time: "07:30".into(),
            days: vec![Weekday::Mon, Weekday::Fri],
            enabled: true,
            created_at: "2026-07-14T00:00:00Z".into(),
            updated_at: "2026-07-14T00:00:00Z".into(),
        }
    }

    #[test]
    fn load_returns_default_when_missing() {
        let p = fresh_dir("data").join("schedules.json");
        let f = load_schedules_file(&p).unwrap();
        assert_eq!(f.schema_version, 1);
        assert!(f.schedules.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let p = fresh_dir("data").join("schedules.json");
        let file = SchedulesFile {
            schema_version: 1,
            schedules: vec![sample("abc"), sample("def")],
        };
        save_schedules_file(&p, &file).unwrap();
        let loaded = load_schedules_file(&p).unwrap();
        assert_eq!(loaded.schedules.len(), 2);
        assert_eq!(loaded.schedules[0].id, "abc");
        assert_eq!(loaded.schedules[1].days, vec![Weekday::Mon, Weekday::Fri]);
    }
}
