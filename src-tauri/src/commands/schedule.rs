//! Scheduled window primer commands.
//!
//! ## Flow
//!
//! 1. UI calls `check_scheduling_available_cmd` at mount → renders warnings
//!    (missing `claude`, no OAuth) and the "native alternative" note.
//! 2. CRUD (`add`/`update`/`delete`/`set_enabled`) mutates `schedules.json`
//!    then calls `sync_schedules` to regenerate the primer config, wrapper
//!    script, and OS scheduler block idempotently.
//! 3. `get_schedule_status_cmd` joins `schedules.json` with `runs.jsonl` and
//!    computes each schedule's next fire time.
//! 4. `run_primer_now_cmd` fires a primer immediately ("Prime now" button).
//!
//! No secrets live in `schedules.json`; the primer reuses `.credentials.json`
//! OAuth via the isolated `primer-config/` dir (empty `env` block).

use chrono::{Local, Utc};
use uuid::Uuid;

use crate::models::{
    AppError, AppResult, Schedule, ScheduleInput, ScheduleRun, ScheduleStatus, SchedulesFile,
    SchedulingAvailability,
};
use crate::schedule::store::{load_schedules_file, save_schedules_file};
use crate::schedule::{cron, primer, runs, sync};
use crate::state::AppState;

/// Reject a malformed schedule before it reaches the store / scheduler.
fn validate_input(input: &ScheduleInput) -> AppResult<()> {
    if cron::parse_hhmm(&input.time).is_none() {
        return Err(AppError::Validation(format!(
            "invalid time \"{}\"; expected 24h HH:MM",
            input.time
        )));
    }
    if input.days.is_empty() {
        return Err(AppError::Validation(
            "a schedule needs at least one weekday".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn list_schedules_cmd(state: tauri::State<'_, AppState>) -> AppResult<Vec<Schedule>> {
    Ok(load_schedules_file(&state.schedules_path())?.schedules)
}

#[tauri::command]
pub async fn add_schedule_cmd(
    state: tauri::State<'_, AppState>,
    input: ScheduleInput,
) -> AppResult<Schedule> {
    validate_input(&input)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut file = load_schedules_file(&state.schedules_path())?;
        let now = Utc::now().to_rfc3339();
        let schedule = Schedule {
            id: Uuid::new_v4().to_string(),
            label: input.label.filter(|s| !s.trim().is_empty()),
            time: input.time,
            days: input.days,
            enabled: input.enabled,
            created_at: now.clone(),
            updated_at: now,
        };
        file.schedules.push(schedule.clone());
        save_schedules_file(&state.schedules_path(), &file)?;
        sync::sync_schedules(&state)?;
        Ok(schedule)
    })
    .await
    .map_err(|e| AppError::Internal(format!("add_schedule task panicked: {e}")))?
}

#[tauri::command]
pub async fn update_schedule_cmd(
    state: tauri::State<'_, AppState>,
    input: ScheduleInput,
) -> AppResult<Schedule> {
    validate_input(&input)?;
    let id = input
        .id
        .clone()
        .ok_or_else(|| AppError::Validation("update requires a schedule id".into()))?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut file = load_schedules_file(&state.schedules_path())?;
        let pos = file
            .schedules
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| AppError::NotFound(id.clone()))?;
        let existing = &file.schedules[pos];
        let updated = Schedule {
            id: existing.id.clone(),
            label: input.label.filter(|s| !s.trim().is_empty()),
            time: input.time,
            days: input.days,
            enabled: input.enabled,
            created_at: existing.created_at.clone(),
            updated_at: Utc::now().to_rfc3339(),
        };
        file.schedules[pos] = updated.clone();
        save_schedules_file(&state.schedules_path(), &file)?;
        sync::sync_schedules(&state)?;
        Ok(updated)
    })
    .await
    .map_err(|e| AppError::Internal(format!("update_schedule task panicked: {e}")))?
}

#[tauri::command]
pub async fn delete_schedule_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut file = load_schedules_file(&state.schedules_path())?;
        let before = file.schedules.len();
        file.schedules.retain(|s| s.id != id);
        if file.schedules.len() == before {
            return Err(AppError::NotFound(id));
        }
        save_schedules_file(&state.schedules_path(), &file)?;
        sync::sync_schedules(&state)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("delete_schedule task panicked: {e}")))?
}

#[tauri::command]
pub async fn set_schedule_enabled_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
    enabled: bool,
) -> AppResult<Schedule> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut file = load_schedules_file(&state.schedules_path())?;
        let pos = file
            .schedules
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| AppError::NotFound(id.clone()))?;
        file.schedules[pos].enabled = enabled;
        file.schedules[pos].updated_at = Utc::now().to_rfc3339();
        let updated = file.schedules[pos].clone();
        save_schedules_file(&state.schedules_path(), &file)?;
        sync::sync_schedules(&state)?;
        Ok(updated)
    })
    .await
    .map_err(|e| AppError::Internal(format!("set_schedule_enabled task panicked: {e}")))?
}

#[tauri::command]
pub async fn sync_schedules_cmd(state: tauri::State<'_, AppState>) -> AppResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sync::sync_schedules(&state))
        .await
        .map_err(|e| AppError::Internal(format!("sync task panicked: {e}")))?
}

#[tauri::command]
pub fn get_schedule_status_cmd(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<ScheduleStatus>> {
    let file: SchedulesFile = load_schedules_file(&state.schedules_path())?;
    let all_runs = runs::read_runs(&state.runs_path())?;
    let last = runs::last_run_per_schedule(&all_runs);
    let now = Local::now();

    let statuses = file
        .schedules
        .iter()
        .map(|s| ScheduleStatus {
            schedule_id: s.id.clone(),
            last_run: last.get(&s.id).cloned(),
            next_fire: if s.enabled {
                cron::next_fire_time(s, now).map(|dt| dt.to_rfc3339())
            } else {
                None
            },
        })
        .collect();
    Ok(statuses)
}

#[tauri::command]
pub fn check_scheduling_available_cmd(
    _state: tauri::State<'_, AppState>,
) -> AppResult<SchedulingAvailability> {
    let subscription_oauth_present = crate::storage::read_credentials_oauth()
        .ok()
        .flatten()
        .is_some();
    let native_scheduling_present = crate::storage::discover_claude_dir()
        .join("scheduled-tasks")
        .is_dir();
    Ok(SchedulingAvailability {
        claude_on_path: sync::command_on_path("claude"),
        scheduler_available: sync::scheduler_available(),
        subscription_oauth_present,
        native_scheduling_present,
        scheduler_kind: sync::scheduler_kind().to_string(),
    })
}

#[tauri::command]
pub async fn run_primer_now_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> AppResult<ScheduleRun> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || Ok(primer::run_primer(&state, &id)))
        .await
        .map_err(|e| AppError::Internal(format!("run_primer task panicked: {e}")))?
}
