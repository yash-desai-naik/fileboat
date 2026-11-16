//! Grouping a folder's rows: by type, date modified (Today, Yesterday, Last week…), size (Tiny,
//! Small…) or first letter. Dates use the viewer's local time, which the view passes in.

use super::entry::{Entry, Kind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GroupBy {
    #[default]
    None,
    /// Folders, then files by extension.
    Type,
    Modified,
    Size,
    /// First letter (digits together, other characters together).
    Letter,
    /// Git status: conflicts, changed, untracked, unchanged, ignored.
    Git,
}

/// Where an entry goes: groups sort by `rank`, then `sub`; `key` identifies the group (for
/// collapsing) and `label` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupOf {
    pub rank: i64,
    pub sub: String,
    pub key: String,
    pub label: String,
}

fn g(rank: i64, key: impl Into<String>, label: impl Into<String>) -> GroupOf {
    GroupOf { rank, sub: String::new(), key: key.into(), label: label.into() }
}

/// The local time the dates are grouped against.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    /// Milliseconds since 1970 (UTC).
    pub now: i64,
    /// Minutes to add to UTC for local time (+330 in India, -300 in New York in winter).
    pub tz_offset_min: i32,
}

const DAY_MS: i64 = 86_400_000;

/// (year, month 1-12) of a day number since 1970 (Howard Hinnant's civil-from-days).
fn year_month(days: i64) -> (i64, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m)
}

fn local_day(ms: i64, clock: &Clock) -> i64 {
    (ms + i64::from(clock.tz_offset_min) * 60_000).div_euclid(DAY_MS)
}

fn by_date(ms: f64, clock: &Clock) -> GroupOf {
    if ms <= 0.0 {
        return g(1000, "unknown", "Unknown date");
    }
    let today = local_day(clock.now, clock);
    let day = local_day(ms as i64, clock);
    // 1970-01-01 was a Thursday: with Monday as 0, its weekday is 3.
    let week_start = today - (today + 3).rem_euclid(7);
    let (y, m) = year_month(day);
    let (ty, tm) = year_month(today);
    let (lm_y, lm) = if tm == 1 { (ty - 1, 12) } else { (ty, tm - 1) };
    if day > today {
        g(0, "future", "Later than today")
    } else if day == today {
        g(1, "today", "Today")
    } else if day == today - 1 {
        g(2, "yesterday", "Yesterday")
    } else if day >= week_start {
        g(3, "this-week", "Earlier this week")
    } else if day >= week_start - 7 {
        g(4, "last-week", "Last week")
    } else if (y, m) == (ty, tm) {
        g(5, "this-month", "Earlier this month")
    } else if (y, m) == (lm_y, lm) {
        g(6, "last-month", "Last month")
    } else if y == ty {
        g(7, "this-year", "Earlier this year")
    } else {
        // Older years one group each, newest first.
        g(8 + (ty - y), format!("year-{y}"), y.to_string())
    }
}

fn by_size(e: &Entry) -> GroupOf {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if e.kind == Kind::Folder {
        return g(0, "folders", "Folders");
    }
    match e.size {
        0 => g(1, "empty", "Empty (0 KB)"),
        s if s < 16 * KB => g(2, "tiny", "Tiny (under 16 KB)"),
        s if s < MB => g(3, "small", "Small (16 KB – 1 MB)"),
        s if s < 128 * MB => g(4, "medium", "Medium (1 – 128 MB)"),
        s if s < GB => g(5, "large", "Large (128 MB – 1 GB)"),
        s if s < 4 * GB => g(6, "huge", "Huge (1 – 4 GB)"),
        _ => g(7, "gigantic", "Gigantic (over 4 GB)"),
    }
}

fn by_git(e: &Entry) -> GroupOf {
    use crate::git::Mark;
    match e.git {
        Some(Mark::Conflicted) => g(0, "git:conflicted", "Conflicts"),
        Some(Mark::Modified | Mark::Added | Mark::Deleted | Mark::Renamed | Mark::Changed) => g(1, "git:changed", "Changed"),
        Some(Mark::Untracked) => g(2, "git:untracked", "Untracked"),
        None => g(3, "git:clean", "Unchanged"),
        Some(Mark::Ignored) => g(4, "git:ignored", "Ignored"),
    }
}

fn by_letter(e: &Entry) -> GroupOf {
    // Leading dots don't count (".env" goes under E), as in most file managers.
    let first = e.key.trim_start_matches('.').chars().next();
    match first {
        Some(c) if c.is_ascii_digit() => g(0, "0-9", "0 – 9"),
        Some(c) if c.is_alphabetic() => {
            let up: String = c.to_uppercase().collect();
            GroupOf { rank: 1, sub: up.clone(), key: format!("letter-{up}"), label: up }
        }
        _ => g(2, "other", "Other"),
    }
}

fn by_type(e: &Entry) -> GroupOf {
    match e.kind {
        Kind::Folder => g(0, "folder", "Folders"),
        _ if e.ext.is_empty() => g(2, "no-ext", "No extension"),
        // The label is the extension; the view names it ("Rust source").
        _ => GroupOf { rank: 1, sub: e.ext.clone(), key: format!("ext:{}", e.ext), label: e.ext.clone() },
    }
}

/// The group an entry belongs to (None when not grouping).
pub fn group_of(e: &Entry, by: GroupBy, clock: &Clock) -> Option<GroupOf> {
    Some(match by {
        GroupBy::None => return None,
        GroupBy::Type => by_type(e),
        GroupBy::Modified => by_date(e.modified, clock),
        GroupBy::Size => by_size(e),
        GroupBy::Letter => by_letter(e),
        GroupBy::Git => by_git(e),
    })
}

/// A group as the view shows it: a header, then `count` rows from `start` (none if collapsed).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub key: String,
    pub label: String,
    /// The first row of the group in the view (where its rows would be when collapsed).
    pub start: usize,
    /// Items in the group (shown or not).
    pub count: usize,
    /// Total size of its files.
    pub bytes: u64,
    pub collapsed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;

    /// Wednesday 2026-11-18 15:00 in India (UTC+5:30): day 20,775 since 1970, 09:30 UTC.
    fn clock() -> Clock {
        Clock { now: 20_775 * DAY_MS + 9 * H + 30 * 60_000, tz_offset_min: 330 }
    }

    fn label(ms: i64) -> String {
        by_date(ms as f64, &clock()).label
    }

    #[test]
    fn calendar() {
        assert_eq!(year_month(0), (1970, 1));
        assert_eq!(year_month(20_775), (2026, 11)); // 2026-11-18
        assert_eq!(year_month(11_016), (2000, 2)); // 2000-02-29
    }

    #[test]
    fn dates_group_in_local_time() {
        let c = clock();
        let today_local_midnight = local_day(c.now, &c) * DAY_MS - 330 * 60_000;
        assert_eq!(label(today_local_midnight + H), "Today");
        assert_eq!(label(today_local_midnight - H), "Yesterday");
        // Monday of this week is two days before Wednesday.
        assert_eq!(label(today_local_midnight - 2 * 24 * H + H), "Earlier this week");
        assert_eq!(label(today_local_midnight - 5 * 24 * H), "Last week");
        assert_eq!(label(today_local_midnight - 14 * 24 * H), "Earlier this month");
        assert_eq!(label(today_local_midnight - 30 * 24 * H), "Last month");
        assert_eq!(label(today_local_midnight - 120 * 24 * H), "Earlier this year");
        assert_eq!(label(today_local_midnight - 400 * 24 * H), "2025");
        assert_eq!(label(today_local_midnight + 3 * 24 * H), "Later than today");
        assert_eq!(by_date(0.0, &c).label, "Unknown date");
        // Older years come after this year, newest first.
        assert!(by_date((today_local_midnight - 400 * 24 * H) as f64, &c).rank < by_date((today_local_midnight - 800 * 24 * H) as f64, &c).rank);
    }
}
