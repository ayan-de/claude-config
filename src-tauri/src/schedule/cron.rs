//! Pure scheduling logic: no I/O. Renders schedules into a managed crontab
//! block, replaces that block idempotently while preserving foreign cron
//! lines, and computes next-fire / catch-up times. Exhaustively unit-tested.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone};

use crate::models::{Schedule, Weekday};

/// Marker comments delimiting the block this app owns in the user's crontab.
pub const BLOCK_START: &str = "# >>> claude-config schedules >>>";
pub const BLOCK_END: &str = "# <<< claude-config schedules <<<";

/// Parse "HH:MM" (24h) into `(hour, minute)`; `None` if malformed / out of range.
pub fn parse_hhmm(time: &str) -> Option<(u32, u32)> {
    let (h, m) = time.split_once(':')?;
    let hour: u32 = h.parse().ok()?;
    let minute: u32 = m.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

/// Render a single cron line for one schedule, or `None` if the schedule is
/// malformed (bad time or empty days). Format:
/// `M H * * <days-csv> "<wrapper>" <schedule-id>`.
pub fn render_cron_line(schedule: &Schedule, wrapper: &str) -> Option<String> {
    let (hour, minute) = parse_hhmm(&schedule.time)?;
    if schedule.days.is_empty() {
        return None;
    }
    let mut nums: Vec<u32> = schedule.days.iter().map(|d| d.cron_num()).collect();
    nums.sort_unstable();
    nums.dedup();
    let days = nums
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    Some(format!(
        "{minute} {hour} * * {days} \"{wrapper}\" {}",
        schedule.id
    ))
}

/// Render the managed block for all *enabled* schedules, or `None` when there
/// are no valid enabled lines (so the caller strips the block entirely).
pub fn render_managed_block(schedules: &[Schedule], wrapper: &str) -> Option<String> {
    let lines: Vec<String> = schedules
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| render_cron_line(s, wrapper))
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!("{BLOCK_START}\n{}\n{BLOCK_END}\n", lines.join("\n")))
}

/// Strip any existing managed block from `existing`, preserve all foreign
/// lines verbatim, then append `new_block` (if any). Idempotent.
pub fn replace_managed_block(existing: &str, new_block: Option<&str>) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut in_block = false;
    for line in existing.lines() {
        let t = line.trim();
        if t == BLOCK_START {
            in_block = true;
            continue;
        }
        if t == BLOCK_END {
            in_block = false;
            continue;
        }
        if !in_block {
            kept.push(line);
        }
    }
    let body = kept.join("\n");
    let body = body.trim_end_matches('\n');
    let mut result = String::new();
    if !body.is_empty() {
        result.push_str(body);
        result.push('\n');
    }
    if let Some(nb) = new_block {
        result.push_str(nb);
    }
    result
}

/// The next time this schedule fires strictly after `now`, or `None` if it has
/// no valid time/days.
pub fn next_fire_time(schedule: &Schedule, now: DateTime<Local>) -> Option<DateTime<Local>> {
    let (hour, minute) = parse_hhmm(&schedule.time)?;
    if schedule.days.is_empty() {
        return None;
    }
    for add in 0..=7 {
        let date = (now + Duration::days(add)).date_naive();
        let wd = Weekday::from_chrono(date.weekday());
        if !schedule.days.contains(&wd) {
            continue;
        }
        let naive = date.and_hms_opt(hour, minute, 0)?;
        let cand = Local.from_local_datetime(&naive).single()?;
        if cand > now {
            return Some(cand);
        }
    }
    None
}

/// The most recent time this schedule was expected to fire at or before `now`,
/// or `None` if it has no valid time/days.
pub fn last_expected_fire(schedule: &Schedule, now: DateTime<Local>) -> Option<DateTime<Local>> {
    let (hour, minute) = parse_hhmm(&schedule.time)?;
    if schedule.days.is_empty() {
        return None;
    }
    for sub in 0..=7 {
        let date = (now - Duration::days(sub)).date_naive();
        let wd = Weekday::from_chrono(date.weekday());
        if !schedule.days.contains(&wd) {
            continue;
        }
        let naive = date.and_hms_opt(hour, minute, 0)?;
        let cand = Local.from_local_datetime(&naive).single()?;
        if cand <= now {
            return Some(cand);
        }
    }
    None
}

/// Whether a missed run should be fired on launch: the schedule is enabled, its
/// most recent expected fire is *today*, and no run has happened since it. Only
/// today's miss is caught up — older windows are moot.
pub fn needs_catchup(
    schedule: &Schedule,
    now: DateTime<Local>,
    last_run: Option<DateTime<Local>>,
) -> bool {
    if !schedule.enabled {
        return false;
    }
    let Some(expected) = last_expected_fire(schedule, now) else {
        return false;
    };
    if expected.date_naive() != now.date_naive() {
        return false;
    }
    match last_run {
        Some(r) => r < expected,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn sched(id: &str, time: &str, days: Vec<Weekday>, enabled: bool) -> Schedule {
        Schedule {
            id: id.into(),
            label: None,
            time: time.into(),
            days,
            enabled,
            created_at: "2026-07-14T00:00:00Z".into(),
            updated_at: "2026-07-14T00:00:00Z".into(),
        }
    }

    #[test]
    fn parse_hhmm_valid_and_invalid() {
        assert_eq!(parse_hhmm("07:30"), Some((7, 30)));
        assert_eq!(parse_hhmm("23:59"), Some((23, 59)));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("7:60"), None);
        assert_eq!(parse_hhmm("bogus"), None);
    }

    #[test]
    fn cron_line_weekdays() {
        let s = sched(
            "abc",
            "07:30",
            vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri],
            true,
        );
        let line = render_cron_line(&s, "/x/primer.sh").unwrap();
        assert_eq!(line, "30 7 * * 1,2,3,4,5 \"/x/primer.sh\" abc");
    }

    #[test]
    fn cron_line_dedups_and_sorts_days() {
        let s = sched("abc", "16:05", vec![Weekday::Sun, Weekday::Mon, Weekday::Sun], true);
        let line = render_cron_line(&s, "/w").unwrap();
        assert_eq!(line, "5 16 * * 0,1 \"/w\" abc");
    }

    #[test]
    fn cron_line_none_for_empty_days_or_bad_time() {
        assert!(render_cron_line(&sched("a", "07:30", vec![], true), "/w").is_none());
        assert!(render_cron_line(&sched("a", "nope", vec![Weekday::Mon], true), "/w").is_none());
    }

    #[test]
    fn managed_block_only_enabled() {
        let schedules = vec![
            sched("a", "07:30", vec![Weekday::Mon], true),
            sched("b", "16:30", vec![Weekday::Sun], false),
        ];
        let block = render_managed_block(&schedules, "/w").unwrap();
        assert!(block.starts_with(BLOCK_START));
        assert!(block.trim_end().ends_with(BLOCK_END));
        assert!(block.contains("30 7 * * 1 \"/w\" a"));
        assert!(!block.contains(" b"));
    }

    #[test]
    fn managed_block_none_when_all_disabled() {
        let schedules = vec![sched("b", "16:30", vec![Weekday::Sun], false)];
        assert!(render_managed_block(&schedules, "/w").is_none());
    }

    #[test]
    fn replace_preserves_foreign_lines() {
        let existing = "0 0 * * * /home/me/backup.sh\n# my note\n";
        let block = render_managed_block(&[sched("a", "07:30", vec![Weekday::Mon], true)], "/w").unwrap();
        let result = replace_managed_block(existing, Some(&block));
        assert!(result.contains("0 0 * * * /home/me/backup.sh"));
        assert!(result.contains("# my note"));
        assert!(result.contains(BLOCK_START));
        assert!(result.contains("30 7 * * 1 \"/w\" a"));
    }

    #[test]
    fn replace_is_idempotent() {
        let block = render_managed_block(&[sched("a", "07:30", vec![Weekday::Mon], true)], "/w").unwrap();
        let existing = "0 0 * * * /backup.sh\n";
        let once = replace_managed_block(existing, Some(&block));
        let twice = replace_managed_block(&once, Some(&block));
        assert_eq!(once, twice);
    }

    #[test]
    fn replace_none_strips_block() {
        let existing = format!(
            "0 0 * * * /backup.sh\n{BLOCK_START}\n30 7 * * 1 \"/w\" a\n{BLOCK_END}\n"
        );
        let result = replace_managed_block(&existing, None);
        assert!(result.contains("/backup.sh"));
        assert!(!result.contains(BLOCK_START));
        assert!(!result.contains("primer"));
        assert_eq!(result, "0 0 * * * /backup.sh\n");
    }

    #[test]
    fn next_fire_same_day_future() {
        // Tue 2026-07-14 06:00 local; schedule fires 07:30 Tue.
        let now = Local.with_ymd_and_hms(2026, 7, 14, 6, 0, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Tue], true);
        let next = next_fire_time(&s, now).unwrap();
        assert_eq!(next.hour(), 7);
        assert_eq!(next.minute(), 30);
        assert_eq!(next.day(), 14);
    }

    #[test]
    fn next_fire_rolls_to_next_matching_day() {
        // Tue 08:00, already past 07:30; next Mon-only fire is next Monday (20th).
        let now = Local.with_ymd_and_hms(2026, 7, 14, 8, 0, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Mon], true);
        let next = next_fire_time(&s, now).unwrap();
        assert_eq!(next.day(), 20);
    }

    #[test]
    fn last_expected_today_when_past() {
        let now = Local.with_ymd_and_hms(2026, 7, 14, 8, 5, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Tue], true);
        let last = last_expected_fire(&s, now).unwrap();
        assert_eq!(last.day(), 14);
        assert_eq!(last.hour(), 7);
    }

    #[test]
    fn needs_catchup_true_when_missed_today() {
        let now = Local.with_ymd_and_hms(2026, 7, 14, 8, 5, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Tue], true);
        assert!(needs_catchup(&s, now, None));
        // A run from yesterday doesn't count as covering today's fire.
        let yday = Local.with_ymd_and_hms(2026, 7, 13, 7, 30, 0).unwrap();
        assert!(needs_catchup(&s, now, Some(yday)));
    }

    #[test]
    fn needs_catchup_false_when_already_ran() {
        let now = Local.with_ymd_and_hms(2026, 7, 14, 8, 5, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Tue], true);
        let ran = Local.with_ymd_and_hms(2026, 7, 14, 7, 31, 0).unwrap();
        assert!(!needs_catchup(&s, now, Some(ran)));
    }

    #[test]
    fn needs_catchup_false_before_fire_time() {
        // 06:00, today's fire (07:30) hasn't happened yet → last expected was
        // a previous day, not today → no catch-up.
        let now = Local.with_ymd_and_hms(2026, 7, 14, 6, 0, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Tue], true);
        assert!(!needs_catchup(&s, now, None));
    }

    #[test]
    fn needs_catchup_false_when_disabled() {
        let now = Local.with_ymd_and_hms(2026, 7, 14, 8, 5, 0).unwrap();
        let s = sched("a", "07:30", vec![Weekday::Tue], false);
        assert!(!needs_catchup(&s, now, None));
    }
}
