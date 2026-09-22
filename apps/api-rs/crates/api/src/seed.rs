//! Workspace demo seed — port dari `plane/bgtasks/workspace_seed_task.py`.

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const PROJECTS_JSON: &str = include_str!("../assets/seeds/data/projects.json");
const STATES_JSON: &str = include_str!("../assets/seeds/data/states.json");
const LABELS_JSON: &str = include_str!("../assets/seeds/data/labels.json");
const CYCLES_JSON: &str = include_str!("../assets/seeds/data/cycles.json");
const MODULES_JSON: &str = include_str!("../assets/seeds/data/modules.json");
const ISSUES_JSON: &str = include_str!("../assets/seeds/data/issues.json");
const VIEWS_JSON: &str = include_str!("../assets/seeds/data/views.json");
const PAGES_JSON: &str = include_str!("../assets/seeds/data/pages.json");

fn parse_seed<T: for<'de> Deserialize<'de>>(raw: &str) -> Vec<T> {
    serde_json::from_str(raw).expect("seed JSON embedded at compile time must be valid")
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectSeed {
    description: String,
    network: i16,
    cover_image: Option<String>,
    #[serde(default)]
    logo_props: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct StateSeed {
    id: i64,
    name: String,
    color: String,
    sequence: f64,
    group: String,
    #[serde(rename = "default")]
    is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct LabelSeed {
    id: i64,
    name: String,
    color: String,
    sort_order: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct CycleSeed {
    id: i64,
    name: String,
    sort_order: f64,
    timezone: String,
    #[serde(rename = "type")]
    cycle_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ModuleSeed {
    id: i64,
    name: String,
    sort_order: f64,
    status: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IssueSeed {
    id: i64,
    name: String,
    sequence_id: i32,
    description_html: String,
    #[serde(default)]
    description_stripped: Option<String>,
    sort_order: f64,
    state_id: i64,
    #[serde(default)]
    labels: Vec<i64>,
    priority: Option<String>,
    cycle_id: Option<i64>,
    #[serde(default)]
    module_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewSeed {
    name: String,
    description: String,
    access: i16,
    filters: Value,
    display_filters: Value,
    display_properties: Value,
    rich_filters: Value,
    sort_order: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct PageSeed {
    name: String,
    #[serde(default)]
    description_html: String,
    #[serde(default)]
    description_stripped: Option<String>,
    access: i16,
    #[serde(rename = "type")]
    page_type: String,
    #[serde(default)]
    logo_props: Value,
    project_id: Option<i64>,
}

/// `"".join(ch for ch in name if ch.isalnum())[:5]` (`workspace_seed_task.py:85`).
pub fn project_identifier(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .take(5)
        .collect()
}

fn cycle_dates(
    cycle_type: &str,
    last_end: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    if cycle_type == "UPCOMING" {
        let start = match last_end {
            Some(end) => end + Duration::days(1),
            None => now + Duration::days(14),
        };
        (start, start + Duration::days(14))
    } else {
        (now, now + Duration::days(14))
    }
}

fn module_dates(index: i64, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = now + Duration::days(index * 2);
    (start, start + Duration::days(14))
}

fn url_host(raw: &str) -> Option<String> {
    let after_scheme = raw.split("://").nth(1).unwrap_or(raw);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn bot_email(workspace_id: Uuid) -> String {
    let host = std::env::var("WEB_URL")
        .ok()
        .and_then(|u| url_host(&u))
        .unwrap_or_else(|| "terraline.space".into());
    format!("bot_user_{workspace_id}@{host}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn project_identifier_takes_five_alnum_chars() {
        assert_eq!(project_identifier("Acme IT"), "AcmeI");
        assert_eq!(project_identifier("My Workspace 42"), "MyWor");
    }

    #[test]
    fn project_identifier_can_be_empty() {
        assert_eq!(project_identifier("---"), "");
    }

    #[test]
    fn project_identifier_is_unicode_aware() {
        assert_eq!(project_identifier("Ünïcode 123"), "Ünïco");
    }

    #[test]
    fn current_cycle_runs_two_weeks_from_now() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let (start, end) = cycle_dates("CURRENT", None, now);
        assert_eq!(start, now);
        assert_eq!(end, now + Duration::days(14));
    }

    #[test]
    fn upcoming_cycle_starts_after_last_end() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let last_end = now + Duration::days(14);
        let (start, end) = cycle_dates("UPCOMING", Some(last_end), now);
        assert_eq!(start, last_end + Duration::days(1));
        assert_eq!(end, start + Duration::days(14));
    }

    #[test]
    fn module_dates_offset_by_index() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let (start, target) = module_dates(2, now);
        assert_eq!(start, now + Duration::days(4));
        assert_eq!(target, start + Duration::days(14));
    }

    #[test]
    fn bot_email_uses_web_url_host() {
        assert_eq!(
            url_host("http://192.168.1.11:8000"),
            Some("192.168.1.11".into())
        );
        assert_eq!(
            url_host("https://terraline.space/path"),
            Some("terraline.space".into())
        );
        assert_eq!(url_host(""), None);
    }
}
