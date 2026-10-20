//! Preset schedules: validation, defaults, and next-occurrence math.
//!
//! Presets: hourly / daily / weekly / monthly, plus "HH:MM" time and an IANA
//! timezone. Monthly clamps to the last day of short months; nonexistent local
//! times (DST spring-forward) move forward to the next valid wall time. Hourly
//! uses only the minutes of `time`; the hour part is ignored.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const FREQUENCIES: [&str; 4] = ["hourly", "daily", "weekly", "monthly"];
pub const DEFAULT_TIME: &str = "09:00";
pub const DEFAULT_HOURLY_TIME: &str = "00:00";
pub const DEFAULT_TIMEZONE: &str = "UTC";

pub const SPEC_VERSION: u8 = 1;
pub const SPEC_TOOLS: [&str; 23] = [
    "list_projects",
    "count_work_items",
    "search_work_items",
    "get_work_item",
    "list_work_item_comments",
    "list_work_item_relations",
    "list_members",
    "list_states",
    "list_labels",
    "list_work_item_types",
    "list_services",
    "get_service",
    "list_intake_items",
    "get_intake_item",
    "count_intake_items",
    "list_sprints",
    "get_sprint",
    "list_sprint_work_items",
    "list_tracks",
    "get_track",
    "list_track_work_items",
    "search_articles",
    "get_article",
];
pub const SPEC_DESCRIPTION_MAX: usize = 500;
pub const SPEC_STEPS_MAX: usize = 10;
pub const SPEC_STEP_MAX: usize = 500;
pub const SPEC_EXPECTED_OUTPUT_MAX: usize = 1000;
pub const SPEC_PROMPT_MAX: usize = 8000;
pub const LEGACY_PROMPT_MAX: usize = 2000;

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
    if !h.bytes().all(|b| b.is_ascii_digit()) || !m.bytes().all(|b| b.is_ascii_digit()) {
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
        Self::with_prompt_limit(
            name,
            prompt,
            frequency,
            time,
            day_of_week,
            day_of_month,
            timezone,
            LEGACY_PROMPT_MAX,
        )
    }

    /// Same as [`ScheduleProposal::new`] but with a caller-chosen prompt cap:
    /// rendered schedule recipes may exceed the legacy 2000 character limit.
    #[allow(clippy::too_many_arguments)]
    pub fn with_prompt_limit(
        name: &str,
        prompt: &str,
        frequency: &str,
        time: Option<&str>,
        day_of_week: Option<i16>,
        day_of_month: Option<i16>,
        timezone: Option<&str>,
        prompt_max: usize,
    ) -> Result<Self, String> {
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("name must be 1-120 characters".to_string());
        }
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() || prompt.chars().count() > prompt_max {
            return Err(format!("prompt must be 1-{prompt_max} characters"));
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
        let (hour, minute) = parse_time(&time)?;
        let time = format!("{hour:02}:{minute:02}");
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
        let day_of_week = if frequency == "weekly" {
            day_of_week
        } else {
            None
        };
        let day_of_month = if frequency == "monthly" {
            day_of_month
        } else {
            None
        };
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
        debug_assert!(candidate > local, "next_occurrence must be strictly future");
        candidate.with_timezone(&Utc)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleSpec {
    pub version: u8,
    pub description: String,
    pub how_to: Vec<String>,
    pub tools: Vec<String>,
    pub expected_output: String,
}

fn bounded_text(field: &str, value: &str, max: usize) -> Result<String, String> {
    let trimmed = value.trim();
    let length = trimmed.chars().count();
    if length == 0 || length > max {
        return Err(format!("{field} must be 1-{max} characters"));
    }
    Ok(trimmed.to_string())
}

impl ScheduleSpec {
    pub fn new(
        description: &str,
        how_to: &[String],
        tools: &[String],
        expected_output: &str,
    ) -> Result<Self, String> {
        Self {
            version: SPEC_VERSION,
            description: description.to_string(),
            how_to: how_to.to_vec(),
            tools: tools.to_vec(),
            expected_output: expected_output.to_string(),
        }
        .validated()
    }

    /// Re-check a spec parsed from storage: version, bounds, and the tool
    /// allowlist; returns the canonical form (trimmed, deduped tools in
    /// `SPEC_TOOLS` order).
    pub fn validated(mut self) -> Result<Self, String> {
        if self.version != SPEC_VERSION {
            return Err(format!("unsupported spec version: {}", self.version));
        }
        self.description = bounded_text("description", &self.description, SPEC_DESCRIPTION_MAX)?;
        if self.how_to.is_empty() || self.how_to.len() > SPEC_STEPS_MAX {
            return Err(format!("how_to must have 1-{SPEC_STEPS_MAX} steps"));
        }
        self.how_to = self
            .how_to
            .iter()
            .map(|step| bounded_text("how_to step", step, SPEC_STEP_MAX))
            .collect::<Result<Vec<_>, _>>()?;
        if self.tools.is_empty() {
            return Err(format!(
                "tools must include at least one of: {}",
                SPEC_TOOLS.join(", ")
            ));
        }
        if let Some(unknown) = self
            .tools
            .iter()
            .map(|tool| tool.trim())
            .find(|tool| !SPEC_TOOLS.contains(tool))
        {
            return Err(format!(
                "tools must be a subset of: {} (unknown: {unknown})",
                SPEC_TOOLS.join(", ")
            ));
        }
        self.tools = SPEC_TOOLS
            .iter()
            .filter(|known| self.tools.iter().any(|tool| tool.trim() == **known))
            .map(|known| (*known).to_string())
            .collect();
        self.expected_output = bounded_text(
            "expected_output",
            &self.expected_output,
            SPEC_EXPECTED_OUTPUT_MAX,
        )?;
        Ok(self)
    }
}

/// Canonical subset of `SPEC_TOOLS` in declaration order.
pub fn allowed_read_tools(allowed: &[String]) -> Vec<&'static str> {
    SPEC_TOOLS
        .iter()
        .copied()
        .filter(|name| allowed.iter().any(|tool| tool == name))
        .collect()
}

/// A partial schedule update: every field is optional and only the provided
/// fields change. Shape validation lives here; merging with the stored row and
/// re-rendering the prompt happen in the API route.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScheduleChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_to: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_week: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_month: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl ScheduleChanges {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.how_to.is_none()
            && self.tools.is_none()
            && self.expected_output.is_none()
            && self.prompt.is_none()
            && self.frequency.is_none()
            && self.time.is_none()
            && self.day_of_week.is_none()
            && self.day_of_month.is_none()
            && self.timezone.is_none()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err("at least one change is required".to_string());
        }
        if self.prompt.is_some()
            && (self.description.is_some()
                || self.how_to.is_some()
                || self.tools.is_some()
                || self.expected_output.is_some())
        {
            return Err(
                "prompt cannot be combined with structured recipe fields".to_string(),
            );
        }
        if let Some(name) = self.name.as_deref() {
            bounded_text("name", name, 120)?;
        }
        if let Some(description) = self.description.as_deref() {
            bounded_text("description", description, SPEC_DESCRIPTION_MAX)?;
        }
        if let Some(how_to) = self.how_to.as_ref() {
            if how_to.is_empty() || how_to.len() > SPEC_STEPS_MAX {
                return Err(format!("how_to must have 1-{SPEC_STEPS_MAX} steps"));
            }
            for step in how_to {
                bounded_text("how_to step", step, SPEC_STEP_MAX)?;
            }
        }
        if let Some(tools) = self.tools.as_ref() {
            if tools.is_empty() {
                return Err("tools must include at least one read tool".to_string());
            }
            if let Some(unknown) = tools
                .iter()
                .map(|tool| tool.trim())
                .find(|tool| !SPEC_TOOLS.contains(tool))
            {
                return Err(format!("unknown tool: {unknown}"));
            }
        }
        if let Some(prompt) = self.prompt.as_deref() {
            bounded_text("prompt", prompt, LEGACY_PROMPT_MAX)?;
        }
        if let Some(frequency) = self.frequency.as_deref() {
            let normalized = frequency.trim().to_ascii_lowercase();
            if !FREQUENCIES.contains(&normalized.as_str()) {
                return Err(format!(
                    "frequency must be one of: {}",
                    FREQUENCIES.join(", ")
                ));
            }
        }
        if let Some(time) = self.time.as_deref() {
            parse_time(time)?;
        }
        if let Some(day_of_week) = self.day_of_week {
            if !(1..=7).contains(&day_of_week) {
                return Err("day_of_week must be 1 (Monday) to 7 (Sunday)".to_string());
            }
        }
        if let Some(day_of_month) = self.day_of_month {
            if !(1..=31).contains(&day_of_month) {
                return Err("day_of_month must be 1-31".to_string());
            }
        }
        if let Some(timezone) = self.timezone.as_deref() {
            timezone
                .trim()
                .parse::<Tz>()
                .map_err(|_| format!("unknown IANA timezone: {}", timezone.trim()))?;
        }
        Ok(())
    }
}

/// Merge a structured schedule's recipe fields and render its canonical prompt.
///
/// `name`, `frequency`, `time`, `day_of_week`, `day_of_month`, and `timezone`
/// are the merged effective values (patch over stored row); only the spec
/// fields are taken from `changes`. `legacy` edits of the raw prompt are
/// rejected here.
#[allow(clippy::too_many_arguments)]
pub fn updated_recipe(
    name: &str,
    spec: &ScheduleSpec,
    changes: &ScheduleChanges,
    frequency: &str,
    time: &str,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: &str,
) -> Result<ScheduleRecipe, String> {
    if changes.prompt.is_some() {
        return Err("prompt is only editable on legacy schedules".to_string());
    }
    let description = changes.description.as_deref().unwrap_or(&spec.description);
    let how_to = changes.how_to.as_deref().unwrap_or(&spec.how_to);
    let tools = changes.tools.as_deref().unwrap_or(&spec.tools);
    let expected_output = changes
        .expected_output
        .as_deref()
        .unwrap_or(&spec.expected_output);
    ScheduleRecipe::new(
        name,
        description,
        how_to,
        tools,
        expected_output,
        frequency,
        Some(time),
        day_of_week,
        day_of_month,
        Some(timezone),
    )
}

/// Merge a legacy schedule's raw prompt and cadence. Structured recipe fields
/// are rejected: legacy schedules keep their raw prompt shape.
#[allow(clippy::too_many_arguments)]
pub fn updated_legacy(
    name: &str,
    prompt: &str,
    changes: &ScheduleChanges,
    frequency: &str,
    time: &str,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: &str,
) -> Result<ScheduleProposal, String> {
    if changes.description.is_some()
        || changes.how_to.is_some()
        || changes.tools.is_some()
        || changes.expected_output.is_some()
    {
        return Err(
            "structured recipe fields can only be edited on recipe schedules; recreate the schedule to convert it"
                .to_string(),
        );
    }
    let prompt = changes.prompt.as_deref().unwrap_or(prompt);
    let name = changes.name.as_deref().unwrap_or(name);
    ScheduleProposal::new(
        name,
        prompt,
        frequency,
        Some(time),
        day_of_week,
        day_of_month,
        Some(timezone),
    )
}

/// Deterministic prompt rendering for a structured recipe.
pub fn render_schedule_prompt(name: &str, spec: &ScheduleSpec) -> String {
    let steps = spec
        .how_to
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {step}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Task: {}\n\nDescription:\n{}\n\nSteps:\n{}\n\nExpected output:\n{}\n\nAllowed tools: {}",
        name.trim(),
        spec.description,
        steps,
        spec.expected_output,
        spec.tools.join(", ")
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleRecipe {
    #[serde(flatten)]
    pub spec: ScheduleSpec,
    #[serde(flatten)]
    pub proposal: ScheduleProposal,
}

impl ScheduleRecipe {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        description: &str,
        how_to: &[String],
        tools: &[String],
        expected_output: &str,
        frequency: &str,
        time: Option<&str>,
        day_of_week: Option<i16>,
        day_of_month: Option<i16>,
        timezone: Option<&str>,
    ) -> Result<Self, String> {
        let spec = ScheduleSpec::new(description, how_to, tools, expected_output)?;
        let prompt = render_schedule_prompt(name, &spec);
        let proposal = ScheduleProposal::with_prompt_limit(
            name,
            &prompt,
            frequency,
            time,
            day_of_week,
            day_of_month,
            timezone,
            SPEC_PROMPT_MAX,
        )?;
        Ok(Self { spec, proposal })
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
    let tz = from.timezone();
    let mut candidate = local_at(tz, from.date_naive(), from.hour(), minute);
    // Ambiguous or repeated local times (DST fall-back) can resolve at or
    // before `from`; advance absolute time until the result is future.
    let mut guard = 0;
    while candidate <= from && guard < 4 {
        let bumped = (candidate + Duration::hours(1)).with_timezone(&tz);
        candidate = local_at(tz, bumped.date_naive(), bumped.hour(), minute);
        guard += 1;
    }
    if candidate <= from {
        // Pathological tz data: never return a past instant.
        candidate = from + Duration::hours(1);
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
    use serde_json::json;

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
    fn irrelevant_day_fields_are_normalized() {
        let daily = proposal("daily", "09:00", Some(3), Some(5), "UTC");
        assert_eq!(daily.day_of_week, None);
        assert_eq!(daily.day_of_month, None);
        let monthly = proposal("monthly", "09:00", None, Some(15), "UTC");
        assert_eq!(monthly.day_of_week, None);
        assert_eq!(monthly.day_of_month, Some(15));
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

    #[test]
    fn hourly_fall_back_stays_future_troll() {
        // Antarctica/Troll ends DST on 2026-10-25, repeating local wall times
        // around the transition; the result must never be in the past.
        let p = proposal("hourly", "00:30", None, None, "Antarctica/Troll");
        let from = Utc.with_ymd_and_hms(2026, 10, 25, 0, 31, 0).unwrap();
        let next = p.next_occurrence(from);
        assert!(next > from, "next occurrence {next} must be after {from}");
        assert!(
            next <= from + Duration::hours(2),
            "next occurrence {next} unexpectedly far after {from}"
        );
    }

    #[test]
    fn hourly_fall_back_stays_future_new_york() {
        // America/New_York falls back on 2026-11-01: local 01:30 occurs twice.
        let p = proposal("hourly", "01:30", None, None, "America/New_York");
        let from = Utc.with_ymd_and_hms(2026, 11, 1, 5, 45, 0).unwrap();
        let next = p.next_occurrence(from);
        assert!(next > from, "next occurrence {next} must be after {from}");
        assert!(
            next <= from + Duration::hours(2),
            "next occurrence {next} unexpectedly far after {from}"
        );
    }

    #[test]
    fn rejects_non_canonical_time() {
        assert!(
            ScheduleProposal::new("n", "p", "daily", Some("+9:+9"), None, None, Some("UTC"))
                .is_err()
        );
        assert!(
            ScheduleProposal::new("n", "p", "daily", Some("+09:+09"), None, None, Some("UTC"))
                .is_err()
        );
    }

    #[test]
    fn monthly_rolls_over_year_and_handles_leap() {
        let p = proposal("monthly", "09:00", None, Some(31), "UTC");
        let from = Utc.with_ymd_and_hms(2026, 12, 31, 10, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2027, 1, 31, 9, 0, 0).unwrap()
        );
        let from = Utc.with_ymd_and_hms(2027, 1, 31, 10, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2027, 2, 28, 9, 0, 0).unwrap()
        );
        let from = Utc.with_ymd_and_hms(2028, 1, 31, 10, 0, 0).unwrap();
        assert_eq!(
            p.next_occurrence(from),
            Utc.with_ymd_and_hms(2028, 2, 29, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn spec_validation_bounds_and_trim() {
        let spec = ScheduleSpec::new(
            "  Do things  ",
            &[" Step one ".to_string()],
            &["list_projects".to_string()],
            " A report ",
        )
        .unwrap();
        assert_eq!(spec.version, SPEC_VERSION);
        assert_eq!(spec.description, "Do things");
        assert_eq!(spec.how_to, vec!["Step one".to_string()]);
        assert_eq!(spec.expected_output, "A report");

        assert!(
            ScheduleSpec::new("", &["s".to_string()], &["list_projects".to_string()], "o").is_err()
        );
        assert!(ScheduleSpec::new("d", &[], &["list_projects".to_string()], "o").is_err());
        let too_many: Vec<String> = (0..11).map(|index| format!("step {index}")).collect();
        assert!(ScheduleSpec::new("d", &too_many, &["list_projects".to_string()], "o").is_err());
        assert!(ScheduleSpec::new("d", &["s".to_string()], &[], "o").is_err());
        assert!(
            ScheduleSpec::new("d", &["s".to_string()], &["list_projects".to_string()], "").is_err()
        );
    }

    #[test]
    fn spec_tools_are_canonical_and_deduped() {
        let spec = ScheduleSpec::new(
            "d",
            &["s".to_string()],
            &[
                "search_work_items".to_string(),
                "list_projects".to_string(),
                "list_projects".to_string(),
            ],
            "o",
        )
        .unwrap();
        assert_eq!(
            spec.tools,
            vec!["list_projects".to_string(), "search_work_items".to_string()]
        );

        let err = ScheduleSpec::new("d", &["s".to_string()], &["drop_tables".to_string()], "o")
            .unwrap_err();
        assert!(err.contains("subset"), "unexpected error: {err}");
    }

    #[test]
    fn spec_version_is_rejected_when_unknown() {
        let mut spec =
            ScheduleSpec::new("d", &["s".to_string()], &["list_projects".to_string()], "o")
                .unwrap();
        spec.version = 2;
        assert!(spec.validated().unwrap_err().contains("version"));
    }

    #[test]
    fn render_prompt_is_deterministic() {
        let spec = ScheduleSpec::new(
            "Summarize overdue",
            &[
                "Count overdue items".to_string(),
                "List the top 5".to_string(),
            ],
            &["list_projects".to_string(), "search_work_items".to_string()],
            "A short markdown list",
        )
        .unwrap();
        assert_eq!(
            render_schedule_prompt("Daily overdue", &spec),
            "Task: Daily overdue\n\nDescription:\nSummarize overdue\n\nSteps:\n1. Count overdue items\n2. List the top 5\n\nExpected output:\nA short markdown list\n\nAllowed tools: list_projects, search_work_items"
        );
    }

    #[test]
    fn recipe_flattens_spec_and_preset() {
        let recipe = ScheduleRecipe::new(
            "Daily",
            "desc",
            &["step".to_string()],
            &["list_projects".to_string()],
            "out",
            "daily",
            Some("09:00"),
            None,
            None,
            Some("UTC"),
        )
        .unwrap();
        let value = serde_json::to_value(&recipe).unwrap();
        assert_eq!(value["version"], json!(1));
        assert_eq!(value["description"], json!("desc"));
        assert_eq!(value["how_to"][0], json!("step"));
        assert_eq!(value["tools"][0], json!("list_projects"));
        assert_eq!(value["expected_output"], json!("out"));
        assert_eq!(value["name"], json!("Daily"));
        assert_eq!(value["frequency"], json!("daily"));
        assert_eq!(
            value["prompt"],
            json!("Task: Daily\n\nDescription:\ndesc\n\nSteps:\n1. step\n\nExpected output:\nout\n\nAllowed tools: list_projects")
        );
    }

    #[test]
    fn recipe_prompt_limit_is_enforced() {
        let long = "x".repeat(SPEC_PROMPT_MAX + 1);
        let err = ScheduleProposal::with_prompt_limit(
            "n",
            &long,
            "daily",
            None,
            None,
            None,
            Some("UTC"),
            SPEC_PROMPT_MAX,
        )
        .unwrap_err();
        assert!(err.contains("8000"), "unexpected error: {err}");
    }

    #[test]
    fn allowed_read_tools_follows_canonical_order() {
        assert_eq!(
            allowed_read_tools(&["search_work_items".to_string(), "list_projects".to_string()]),
            vec!["list_projects", "search_work_items"]
        );
        assert!(allowed_read_tools(&[]).is_empty());
    }

    #[test]
    fn spec_accepts_every_spec_tool() {
        let all: Vec<String> = SPEC_TOOLS.iter().map(|tool| tool.to_string()).collect();
        let spec = ScheduleSpec::new("d", &["s".to_string()], &all, "o").unwrap();
        assert_eq!(spec.tools.len(), SPEC_TOOLS.len());
        assert_eq!(SPEC_TOOLS.len(), 23);
    }

    fn base_spec() -> ScheduleSpec {
        ScheduleSpec::new(
            "Summarize overdue items",
            &["Count overdue items".to_string()],
            &["count_work_items".to_string()],
            "A short list",
        )
        .unwrap()
    }

    #[test]
    fn update_changes_validates_bounds_and_mixing() {
        let empty = ScheduleChanges::default();
        assert!(empty.validate().unwrap_err().contains("at least one change"));

        let mixed = ScheduleChanges {
            prompt: Some("legacy".to_string()),
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        assert!(mixed.validate().unwrap_err().contains("cannot be combined"));

        let blank_name = ScheduleChanges {
            name: Some("  ".to_string()),
            ..Default::default()
        };
        assert!(blank_name.validate().unwrap_err().contains("name"));

        let bad_day = ScheduleChanges {
            day_of_week: Some(8),
            ..Default::default()
        };
        assert!(bad_day.validate().unwrap_err().contains("day_of_week"));

        let bad_tz = ScheduleChanges {
            timezone: Some("Mars/Olympus".to_string()),
            ..Default::default()
        };
        assert!(bad_tz.validate().unwrap_err().contains("timezone"));

        let valid = ScheduleChanges {
            name: Some(" Weekly report ".to_string()),
            frequency: Some("weekly".to_string()),
            day_of_week: Some(1),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn updated_recipe_merges_fields_and_renders_a_new_prompt() {
        let spec = base_spec();
        let changes = ScheduleChanges {
            name: Some("Weekly overdue report".to_string()),
            how_to: Some(vec![
                "Count overdue items".to_string(),
                "Group by owner".to_string(),
            ]),
            tools: Some(vec!["search_work_items".to_string(), "count_work_items".to_string()]),
            frequency: Some("weekly".to_string()),
            day_of_week: Some(3),
            ..Default::default()
        };
        let recipe = updated_recipe(
            "Weekly overdue report",
            &spec,
            &changes,
            "weekly",
            "10:30",
            Some(3),
            None,
            "Asia/Jakarta",
        )
        .unwrap();
        assert_eq!(recipe.proposal.frequency, "weekly");
        assert_eq!(recipe.proposal.day_of_week, Some(3));
        assert_eq!(recipe.proposal.timezone, "Asia/Jakarta");
        assert_eq!(recipe.spec.how_to.len(), 2);
        assert_eq!(recipe.spec.tools, vec!["count_work_items", "search_work_items"]);
        assert!(recipe.proposal.prompt.contains("Task: Weekly overdue report"));
        assert!(recipe.proposal.prompt.contains("2. Group by owner"));
    }

    #[test]
    fn updated_recipe_rejects_prompt_edits() {
        let changes = ScheduleChanges {
            prompt: Some("raw".to_string()),
            ..Default::default()
        };
        let error = updated_recipe(
            "Name",
            &base_spec(),
            &changes,
            "daily",
            "09:00",
            None,
            None,
            "UTC",
        )
        .unwrap_err();
        assert!(error.contains("legacy"));
    }

    #[test]
    fn updated_legacy_rejects_structured_fields() {
        let changes = ScheduleChanges {
            description: Some("recipe".to_string()),
            ..Default::default()
        };
        let error = updated_legacy(
            "Name",
            "old prompt",
            &changes,
            "daily",
            "09:00",
            None,
            None,
            "UTC",
        )
        .unwrap_err();
        assert!(error.contains("recreate"));
    }

    #[test]
    fn updated_legacy_merges_prompt_and_cadence() {
        let changes = ScheduleChanges {
            name: Some("New name".to_string()),
            prompt: Some("new prompt".to_string()),
            frequency: Some("weekly".to_string()),
            day_of_week: Some(2),
            ..Default::default()
        };
        let proposal = updated_legacy(
            "New name",
            "old prompt",
            &changes,
            "weekly",
            "08:15",
            Some(2),
            None,
            "UTC",
        )
        .unwrap();
        assert_eq!(proposal.name, "New name");
        assert_eq!(proposal.prompt, "new prompt");
        assert_eq!(proposal.day_of_week, Some(2));
        assert_eq!(proposal.time, "08:15");
    }
}
