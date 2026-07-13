//! Unix scheduler backend: reads the user's crontab, replaces only our managed
//! block, and writes it back. Foreign cron lines are preserved verbatim. The
//! managed block is replaced as a unit — we never half-write.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::models::{AppError, AppResult};
use crate::schedule::cron::replace_managed_block;

/// Replace the managed block in the user's crontab with `block` (or strip it
/// when `None`).
pub fn apply_managed_block(block: Option<&str>) -> AppResult<()> {
    let existing = read_crontab()?;
    let updated = replace_managed_block(&existing, block);
    write_crontab(&updated)
}

fn read_crontab() -> AppResult<String> {
    let out = Command::new("crontab")
        .arg("-l")
        .output()
        .map_err(|e| AppError::Internal(format!("could not run `crontab -l`: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        // `crontab -l` exits non-zero with "no crontab for user" when none
        // exists yet — treat as empty rather than an error.
        Ok(String::new())
    }
}

fn write_crontab(content: &str) -> AppResult<()> {
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Internal(format!("could not run `crontab -`: {e}")))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }
    let status = child
        .wait()
        .map_err(|e| AppError::Internal(format!("`crontab -` wait failed: {e}")))?;
    if !status.success() {
        return Err(AppError::Internal(format!(
            "`crontab -` exited with {:?}",
            status.code()
        )));
    }
    Ok(())
}
