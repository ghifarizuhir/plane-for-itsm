//! Preset schedules: validation, defaults, and next-occurrence math.
//!
//! Presets: hourly / daily / weekly / monthly, plus "HH:MM" time and an IANA
//! timezone. Monthly clamps to the last day of short months; nonexistent local
//! times (DST spring-forward) move forward to the next valid wall time.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const FREQUENCIES: [&str; 4] = ["hourly", "daily", "weekly", "monthly"];
pub const DEFAULT_TIME: &str = "09:00";
pub const DEFAULT_HOURLY_TIME: &str = "00:00";
pub const DEFAULT_TIMEZONE: &str = "UTC";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleProposal {
    pub name: String,
    pub prompt: String,
    pub frequency: String,
    pub time: String,
    pub day_of_week: Option<i16>,
    pub day_of_month: Option<i16>,
    pub timezone: String,
}

fn parse_time(time: &str) -> Result<(u32, u32), String> {
    let (h, m) = time
        .split_once(':')
        .ok_or_else(|| "time must be HH:MM".to_string())?;
    if h.len() != 2 || m.len() != 2 {
        return Err("time must be HH:MM".to_string());
    }
    let hour: u32 = h.parse().map_err(|_| "time must be HH:MM".to_string())?;
    let minute: u32 = m.parse().map_err(|_| "time must be HH:MM".to_string())?;
    if hour > 23 || minute > 59 {
        return Err("time must be HH:MM (24h)".to_string());
    }
    Ok((hour, minute))
}

impl ScheduleProposal {
    pub fn new(
        name: &str,
        prompt: &str,
        frequency: &str,
        time: Option<&str>,
        day_of_week: Option<i16>,
        day_of_month: Option<i16>,
        timezone: Option<&str>,
    ) -> Result<Self, String> {
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("name must be 1-120 characters".to_string());
        }
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() || prompt.chars().count() > 2000 {
            return Err("prompt must be 1-2000 characters".to_string());
        }
        let frequency = frequency.trim().to_ascii_lowercase();
        if !FREQUENCIES.contains(&frequency.as_str()) {
            return Err(format!(
                "frequency must be one of: {}",
                FREQUENCIES.join(", ")
            ));
        }
        let fallback = if frequency == "hourly" {
            DEFAULT_HOURLY_TIME
        } else {
            DEFAULT_TIME
        };
        let time = time
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback)
            .to_string();
        parse_time(&time)?;
        match frequency.as_str() {
            "weekly" if !matches!(day_of_week, Some(1..=7)) => {
                return Err("day_of_week must be 1 (Monday) to 7 (Sunday)".to_string());
            }
            "monthly" if !matches!(day_of_month, Some(1..=31)) => {
                return Err("day_of_month must be 1-31".to_string());
            }
            _ => {}
        }
        let timezone = timezone
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_TIMEZONE)
            .to_string();
        timezone
            .parse::<Tz>()
            .map_err(|_| format!("unknown IANA timezone: {timezone}"))?;
        Ok(Self {
            name,
            prompt,
            frequency,
            time,
            day_of_week,
            day_of_month,
            timezone,
        })
    }

    pub fn tz(&self) -> Tz {
        self.timezone.parse().expect("validated timezone")
    }

    /// Next fire time strictly after `from`, in UTC.
    pub fn next_occurrence(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        let (hour, minute) = parse_time(&self.time).expect("validated time");
        let local = from.with_timezone(&self.tz());
        let candidate = match self.frequency.as_str() {
            "hourly" => next_hourly(local, minute),
            "daily" => next_daily(local, hour, minute),
            "weekly" => next_weekly(local, self.day_of_week.unwrap_or(1) as u32, hour, minute),
            _ => next_monthly(local, self.day_of_month.unwrap_or(1) as u32, hour, minute),
        };
        candidate.with_timezone(&Utc)
    }
}

/// Resolve a local wall time, moving forward past DST gaps; ambiguity takes
/// the earliest occurrence.
fn local_at(tz: Tz, mut date: NaiveDate, mut hour: u32, minute: u32) -> DateTime<Tz> {
    loop {
        match tz.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => return dt,
            LocalResult::None => {
                hour += 1;
                if hour > 23 {
                    hour = 0;
                    date += Duration::days(1);
                }
            }
        }
    }
}

fn next_hourly(from: DateTime<Tz>, minute: u32) -> DateTime<Tz> {
    let mut candidate = local_at(from.timezone(), from.date_naive(), from.hour(), minute);
    if candidate <= from {
        let bumped = from + Duration::hours(1);
        candidate = local_at(
            bumped.timezone(),
            bumped.date_naive(),
            bumped.hour(),
            minute,
        );
        if candidate <= from {
            candidate = local_at(
                bumped.timezone(),
                bumped.date_naive(),
                bumped.hour() + 1,
                minute,
            );
        }
    }
    candidate
}

fn next_daily(from: DateTime<Tz>, hour: u32, minute: u32) -> DateTime<Tz> {
    let tz = from.timezone();
    let mut candidate = local_at(tz, from.date_naive(), hour, minute);
    if candidate <= from {
        candidate = local_at(tz, from.date_naive() + Duration::days(1), hour, minute);
    }
    candidate
}

fn next_weekly(from: DateTime<Tz>, day_of_week: u32, hour: u32, minute: u32) -> DateTime<Tz> {
    let tz = from.timezone();
    let current = from.weekday().number_from_monday();
    let delta = (day_of_week as i64 - current as i64).rem_euclid(7);
    let mut candidate = local_at(tz, from.date_naive() + Duration::days(delta), hour, minute);
    if candidate <= from {
        candidate = local_at(
            tz,
            from.date_naive() + Duration::days(delta + 7),
            hour,
            minute,
        );
    }
    candidate
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid month");
    (next - first).num_days() as u32
}

fn next_monthly(from: DateTime<Tz>, day_of_month: u32, hour: u32, minute: u32) -> DateTime<Tz> {
    let tz = from.timezone();
    let clamp = |year: i32, month: u32| {
        NaiveDate::from_ymd_opt(year, month, day_of_month.min(days_in_month(year, month)))
            .expect("valid date")
    };
    let (mut year, mut month) = (from.year(), from.month());
    let mut candidate = local_at(tz, clamp(year, month), hour, minute);
    if candidate <= from {
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }
        candidate = local_at(tz, clamp(year, month), hour, minute);
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn proposal(
        frequency: &str,
        time: &str,
        dow: Option<i16>,
        dom: Option<i16>,
        tz: &str,
    ) -> ScheduleProposal {
        ScheduleProposal::new(
            "Report",
            "Summarize overdue work items",
            frequency,
            Some(time),
            dow,
            dom,
            Some(tz),
        )
        .expect("valid proposal")
    }

    #[test]
    fn rejects_bad_frequency_time_and_timezone() {
        assert!(ScheduleProposal::new("n", "p", "often", None, None, None, Some("UTC")).is_err());
        assert!(
            ScheduleProposal::new("n", "p", "daily", Some("25:00"), None, None, Some("UTC"))
                .is_err()
        );
        assert!(
            ScheduleProposal::new("n", "p", "daily", Some("9:00"), None, None, Some("UTC"))
                .is_err()
        );
        assert!(
            ScheduleProposal::new("n", "p", "daily", None, None, None, Some("Mars/Olympus"))
                .is_err()
        );
        assert!(ScheduleProposal::new("n", "p", "weekly", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", "p", "monthly", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("", "p", "daily", None, None, None, Some("UTC")).is_err());
        assert!(ScheduleProposal::new("n", " ", "daily", None, None, None, Some("UTC")).is_err());
    }

    #[test]
    fn applies_defaults() {
        let p = ScheduleProposal::new("n", "p", "daily", None, None, None, None).unwrap();
        assert_eq!(p.time, "09:00");
        assert_eq!(p.timezone, "UTC");
        let h = ScheduleProposal::new("n", "p", "hourly", None, None, None, None).unwrap();
        assert_eq!(h.time, "00:00");
    }

    #[test]
    fn hourly_advances_to_next_minute() {
        let p = proposal("hourly", "00:30", None, None, "UTC");
        let from = Utc.with_ymd_and_hms(2026, 9, 24, 10, 5, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2026, 9, 24, 10, 30, 0).unwrap()
        );
        let from_after = Utc.with_ymd_and_hms(2026, 9, 24, 10, 45, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from_after),
            Utc.with_ymd_and_hms(2026, 9, 24, 11, 30, 0).unwrap()
        );
    }

    #[test]
    fn daily_uses_proposal_timezone() {
        // 09:00 Asia/Jakarta == 02:00 UTC.
        let p = proposal("daily", "09:00", None, None, "Asia/Jakarta");
        let from = Utc.with_ymd_and_hms(2026, 9, 24, 10, 0, 0).unwrap(); // 17:00 WIB
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2026, 9, 25, 2, 0, 0).unwrap()
        );
    }

    #[test]
    fn weekly_picks_monday_and_rolls_forward() {
        let p = proposal("weekly", "09:00", Some(1), None, "UTC");
        // 2026-09-24 is a Thursday; next Monday is 2026-09-28.
        let from = Utc.with_ymd_and_hms(2026, 9, 24, 10, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2026, 9, 28, 9, 0, 0).unwrap()
        );
        // Monday before 09:00 stays the same day.
        let from_monday = Utc.with_ymd_and_hms(2026, 9, 28, 8, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from_monday),
            Utc.with_ymd_and_hms(2026, 9, 28, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn monthly_clamps_short_months() {
        let p = proposal("monthly", "09:00", None, Some(31), "UTC");
        let from = Utc.with_ymd_and_hms(2026, 1, 31, 10, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2026, 2, 28, 9, 0, 0).unwrap()
        );
        let from_early = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from_early),
            Utc.with_ymd_and_hms(2026, 2, 28, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn dst_gap_moves_forward() {
        // 2026-03-08 02:30 does not exist in America/New_York (spring forward).
        let p = proposal("daily", "02:30", None, None, "America/New_York");
        let from = Utc.with_ymd_and_hms(2026, 3, 8, 0, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2026, 3, 8, 7, 30, 0).unwrap()
        );
    }
}
