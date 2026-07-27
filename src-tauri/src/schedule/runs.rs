//! Append/read the primer run-log `runs.jsonl`. One compact JSON `ScheduleRun`
//! per line. Reads are tolerant of blank or malformed lines so a partial write
//! from a crashed wrapper never breaks status.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::models::{AppResult, ScheduleRun};

/// Append one run record as a single JSON line.
pub fn append_run(path: &Path, run: &ScheduleRun) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(run)?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Read all run records, skipping blank/garbage lines.
pub fn read_runs(path: &Path) -> AppResult<Vec<ScheduleRun>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<ScheduleRun>(trimmed) {
            Ok(run) => out.push(run),
            Err(e) => log::warn!("skipping malformed runs.jsonl line: {e}"),
        }
    }
    Ok(out)
}

/// Reduce a run list to the latest run per schedule id (by `started_at`, which
/// is RFC3339 and therefore lexically sortable).
pub fn last_run_per_schedule(runs: &[ScheduleRun]) -> HashMap<String, ScheduleRun> {
    let mut map: HashMap<String, ScheduleRun> = HashMap::new();
    for run in runs {
        match map.get(&run.schedule_id) {
            Some(existing) if existing.started_at >= run.started_at => {}
            _ => {
                map.insert(run.schedule_id.clone(), run.clone());
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_path(name: &str) -> std::path::PathBuf {
        let d = tempfile::tempdir().unwrap().keep();
        d.join(name)
    }

    fn run(id: &str, at: &str, ok: bool) -> ScheduleRun {
        ScheduleRun {
            schedule_id: id.into(),
            started_at: at.into(),
            exit_code: Some(if ok { 0 } else { 1 }),
            ok,
            error: if ok { None } else { Some("boom".into()) },
        }
    }

    #[test]
    fn append_then_read_roundtrip() {
        let p = fresh_path("runs.jsonl");
        append_run(&p, &run("a", "2026-07-14T07:30:00Z", true)).unwrap();
        append_run(&p, &run("a", "2026-07-14T16:30:00Z", false)).unwrap();
        let runs = read_runs(&p).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].schedule_id, "a");
        assert!(!runs[1].ok);
        assert_eq!(runs[1].error.as_deref(), Some("boom"));
    }

    #[test]
    fn read_skips_blank_and_bad_lines() {
        let p = fresh_path("runs.jsonl");
        std::fs::write(
            &p,
            "\n{not json}\n{\"scheduleId\":\"a\",\"startedAt\":\"2026-07-14T07:30:00Z\",\"ok\":true}\n\n",
        )
        .unwrap();
        let runs = read_runs(&p).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].schedule_id, "a");
    }

    #[test]
    fn read_missing_file_is_empty() {
        let p = fresh_path("nope.jsonl");
        assert!(read_runs(&p).unwrap().is_empty());
    }

    #[test]
    fn last_run_picks_latest_by_started_at() {
        let runs = vec![
            run("a", "2026-07-14T07:30:00Z", true),
            run("a", "2026-07-14T16:30:00Z", false),
            run("b", "2026-07-14T09:00:00Z", true),
        ];
        let map = last_run_per_schedule(&runs);
        assert_eq!(map.len(), 2);
        assert_eq!(map["a"].started_at, "2026-07-14T16:30:00Z");
        assert!(!map["a"].ok);
        assert!(map["b"].ok);
    }
}
