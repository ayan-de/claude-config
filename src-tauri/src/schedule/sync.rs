//! Idempotent orchestration: regenerate the primer config, wrapper script, and
//! the OS scheduler entries from `schedules.json`. Runs after every mutation
//! and at launch (plus catch-up for missed runs).

use chrono::{DateTime, Local};

use crate::models::AppResult;
use crate::schedule::{cron, primer, runs, store};
use crate::state::AppState;

/// Is `cmd` an executable on `PATH`?
pub fn command_on_path(cmd: &str) -> bool {
    #[cfg(windows)]
    let names = vec![format!("{cmd}.exe"), format!("{cmd}.cmd"), cmd.to_string()];
    #[cfg(not(windows))]
    let names = vec![cmd.to_string()];

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for n in &names {
                if dir.join(n).is_file() {
                    return true;
                }
            }
        }
    }
    false
}

/// Is the OS scheduler binary present?
pub fn scheduler_available() -> bool {
    #[cfg(windows)]
    {
        command_on_path("schtasks")
    }
    #[cfg(not(windows))]
    {
        command_on_path("crontab")
    }
}

/// Human-readable scheduler name for the availability payload.
pub fn scheduler_kind() -> &'static str {
    if !scheduler_available() {
        return "none";
    }
    #[cfg(windows)]
    {
        "schtasks"
    }
    #[cfg(not(windows))]
    {
        "crontab"
    }
}

/// Fully regenerate all scheduling artifacts from `schedules.json`. Idempotent.
pub fn sync_schedules(state: &AppState) -> AppResult<()> {
    let file = store::load_schedules_file(&state.schedules_path())?;

    // No schedules at all → strip the scheduler block and clean artifacts
    // (including the Windows credential copy).
    if file.schedules.is_empty() {
        #[cfg(unix)]
        crate::schedule::os_cron::apply_managed_block(None)?;
        #[cfg(windows)]
        crate::schedule::os_task::remove_all()?;
        let _ = std::fs::remove_dir_all(state.primer_config_dir());
        let _ = std::fs::remove_file(state.wrapper_path());
        return Ok(());
    }

    // Regenerate primer config + wrapper.
    primer::sync_primer_config(&state.primer_config_dir())?;
    let wrapper_path = state.wrapper_path();
    let contents = primer::render_wrapper(
        &state.primer_config_dir(),
        &state.runs_path(),
        primer::PRIMER_MODEL,
    );
    primer::write_wrapper(&wrapper_path, &contents)?;

    // Apply to the OS scheduler.
    #[cfg(unix)]
    {
        let wrapper_str = wrapper_path.to_string_lossy();
        let block = cron::render_managed_block(&file.schedules, &wrapper_str);
        crate::schedule::os_cron::apply_managed_block(block.as_deref())?;
    }
    #[cfg(windows)]
    {
        crate::schedule::os_task::apply_schedules(&file.schedules, &wrapper_path)?;
    }

    Ok(())
}

/// At launch, fire any enabled schedule whose most recent expected fire (today)
/// was missed — at most one catch-up per schedule.
pub fn catch_up_on_launch(state: &AppState) -> AppResult<()> {
    let file = store::load_schedules_file(&state.schedules_path())?;
    let all_runs = runs::read_runs(&state.runs_path())?;
    let last = runs::last_run_per_schedule(&all_runs);
    let now = Local::now();

    for s in &file.schedules {
        let last_run_dt: Option<DateTime<Local>> = last
            .get(&s.id)
            .and_then(|r| DateTime::parse_from_rfc3339(&r.started_at).ok())
            .map(|dt| dt.with_timezone(&Local));
        if cron::needs_catchup(s, now, last_run_dt) {
            log::info!("catch-up: firing missed schedule {}", s.id);
            let _ = primer::run_primer(state, &s.id);
        }
    }
    Ok(())
}
