//! Windows scheduler backend: one Scheduled Task per enabled schedule,
//! namespaced under `ClaudeConfig\<id>`. Best-effort — Windows is a secondary
//! target for this feature (see the design's isolation caveats).

use std::path::Path;
use std::process::Command;

use crate::models::{AppError, AppResult, Schedule, Weekday};
use crate::schedule::cron::parse_hhmm;

const TASK_PREFIX: &str = "ClaudeConfig";

fn schtasks_day(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "MON",
        Weekday::Tue => "TUE",
        Weekday::Wed => "WED",
        Weekday::Thu => "THU",
        Weekday::Fri => "FRI",
        Weekday::Sat => "SAT",
        Weekday::Sun => "SUN",
    }
}

/// Delete every existing managed task, then create one per enabled schedule.
pub fn apply_schedules(schedules: &[Schedule], wrapper: &Path) -> AppResult<()> {
    remove_all()?;
    for s in schedules.iter().filter(|s| s.enabled) {
        let Some((hour, minute)) = parse_hhmm(&s.time) else {
            continue;
        };
        if s.days.is_empty() {
            continue;
        }
        let days = s
            .days
            .iter()
            .map(|d| schtasks_day(*d))
            .collect::<Vec<_>>()
            .join(",");
        let tr = format!("\"{}\" {}", wrapper.display(), s.id);
        let tn = format!("{TASK_PREFIX}\\{}", s.id);
        let st = format!("{hour:02}:{minute:02}");
        let status = Command::new("schtasks")
            .args([
                "/Create", "/F", "/SC", "WEEKLY", "/D", &days, "/TN", &tn, "/TR", &tr, "/ST",
                &st,
            ])
            .status()
            .map_err(|e| AppError::Internal(format!("could not run schtasks /Create: {e}")))?;
        if !status.success() {
            return Err(AppError::Internal(format!(
                "schtasks /Create for {} exited with {:?}",
                s.id,
                status.code()
            )));
        }
    }
    Ok(())
}

/// Delete the entire managed task folder. Ignores "not found".
pub fn remove_all() -> AppResult<()> {
    // Query the folder; delete each task we own. `/TN <folder>\*` isn't
    // universally supported, so enumerate via query and delete individually.
    let out = Command::new("schtasks")
        .args(["/Query", "/FO", "LIST"])
        .output()
        .map_err(|e| AppError::Internal(format!("could not run schtasks /Query: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("TaskName:") {
            let name = rest.trim();
            if name.contains(&format!("\\{TASK_PREFIX}\\")) || name.starts_with(&format!("\\{TASK_PREFIX}\\")) {
                let _ = Command::new("schtasks")
                    .args(["/Delete", "/F", "/TN", name])
                    .status();
            }
        }
    }
    Ok(())
}
