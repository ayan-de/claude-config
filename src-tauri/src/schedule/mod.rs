//! Scheduled window primers: recurring `claude -p "hi"` invocations fired by
//! the OS scheduler (crontab / Scheduled Tasks) to reset the Anthropic
//! subscription's 5-hour usage window even when the app is closed.
//!
//! See `docs/superpowers/specs/2026-07-14-schedule-primer-design.md`.

pub mod cron;
pub mod primer;
pub mod runs;
pub mod store;
pub mod sync;

#[cfg(unix)]
pub mod os_cron;
#[cfg(windows)]
pub mod os_task;
