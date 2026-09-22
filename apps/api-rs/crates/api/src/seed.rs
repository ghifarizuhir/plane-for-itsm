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

async fn insert_bot_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let email = bot_email(workspace_id);
    let username = format!("bot_user_{workspace_id}");
    let password = common::auth::make_django_password(&Uuid::new_v4().simple().to_string());
    let token = Uuid::new_v4().simple().to_string();
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, \
         avatar, date_joined, token, user_timezone, last_location, created_location, last_login_ip, \
         last_logout_ip, last_login_medium, last_login_uagent, last_active, last_login_time, \
         token_updated_at, is_active, is_staff, is_superuser, is_managed, is_password_expired, \
         is_email_verified, is_password_autoset, is_bot, bot_type, is_email_valid, \
         is_password_reset_required, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'Terraline', '', 'Terraline', '', now(), $4, \
         'UTC', '', '', '', '', '', '', now(), now(), now(), true, false, false, false, false, \
         false, true, true, 'WORKSPACE_SEED', true, false, now(), now()) RETURNING id",
    )
    .bind(&email)
    .bind(&username)
    .bind(&password)
    .bind(&token)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

async fn insert_workspace_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO workspace_members (id, workspace_id, member_id, role, company_role, \
         view_props, default_props, issue_props, is_active, getting_started_checklist, tips, \
         explored_features, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 20, '', '{}', '{}', '{}', true, '{}', '{}', '{}', \
         now(), now())",
    )
    .bind(workspace_id)
    .bind(bot_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

const SEED_DISPLAY_FILTERS: &str = r#"{"layout":"list","calendar":{"layout":"month","show_weekends":false},"group_by":"state","order_by":"sort_order","sub_issue":true,"sub_group_by":null,"show_empty_groups":true}"#;
const SEED_DISPLAY_PROPERTIES: &str = r#"{"key":true,"link":true,"cycle":false,"state":true,"labels":false,"modules":false,"assignee":true,"due_date":false,"estimate":true,"priority":true,"created_on":true,"issue_type":true,"start_date":false,"updated_on":true,"customer_count":true,"sub_issue_count":false,"attachment_count":false,"customer_request_count":true}"#;
const SEED_FILTERS: &str = r#"{"priority":null,"state":null,"state_group":null,"assignees":null,"created_by":null,"labels":null,"start_date":null,"target_date":null,"subscriber":null}"#;
const SEED_PREFERENCES: &str = r#"{"pages":{"block_display":true},"navigation":{"default_tab":"work_items","hide_in_more_menu":[]}}"#;

async fn insert_project(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    workspace_name: &str,
    bot_id: Uuid,
) -> Result<Uuid, anyhow::Error> {
    let identifier = project_identifier(workspace_name);
    if identifier.is_empty() {
        anyhow::bail!("seed: workspace name has no alphanumeric characters for an identifier");
    }
    let seed = parse_seed::<ProjectSeed>(PROJECTS_JSON)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("seed: projects.json is empty"))?;
    let (project_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO projects (id, name, description, identifier, workspace_id, network, \
         module_view, cycle_view, issue_views_view, page_view, intake_view, \
         is_time_tracking_enabled, is_issue_type_enabled, guest_view_all_features, archive_in, \
         close_in, cover_image, logo_props, timezone, created_by_id, updated_by_id, created_at, \
         updated_at) \
         SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, true, true, true, true, false, false, \
         false, false, 0, 0, $5, $6, w.timezone, $7, $7, now(), now() \
         FROM workspaces w WHERE w.id = $8 RETURNING id",
    )
    .bind(workspace_name)
    .bind(&seed.description)
    .bind(&identifier)
    .bind(seed.network)
    .bind(seed.cover_image.as_deref())
    .bind(&seed.logo_props)
    .bind(bot_id)
    .bind(workspace_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(project_id)
}

async fn insert_project_members_and_props(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
         view_props, default_props, sort_order, preferences, created_at, updated_at) \
         SELECT gen_random_uuid(), wm.member_id, wm.role, $1, $2, true, '{}', '{}', 65535, '{}', \
         now(), now() \
         FROM workspace_members wm WHERE wm.workspace_id = $2 AND wm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(workspace_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO project_user_properties (id, user_id, project_id, workspace_id, filters, \
         display_filters, display_properties, rich_filters, preferences, sort_order, created_by_id, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), wm.member_id, $1, $2, $3::jsonb, $4::jsonb, $5::jsonb, \
         '{}'::jsonb, $6::jsonb, 65535, $7, now(), now() \
         FROM workspace_members wm WHERE wm.workspace_id = $2 AND wm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(SEED_FILTERS)
    .bind(SEED_DISPLAY_FILTERS)
    .bind(SEED_DISPLAY_PROPERTIES)
    .bind(SEED_PREFERENCES)
    .bind(bot_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
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
    fn upcoming_cycle_without_last_end_starts_two_weeks_out() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let (start, end) = cycle_dates("UPCOMING", None, now);
        assert_eq!(start, now + Duration::days(14));
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
    fn url_host_strips_scheme_userinfo_and_port() {
        assert_eq!(
            url_host("http://192.168.1.11:8000"),
            Some("192.168.1.11".into())
        );
        assert_eq!(
            url_host("https://terraline.space/path"),
            Some("terraline.space".into())
        );
        assert_eq!(
            url_host("https://user:pw@host.example:8443/x"),
            Some("host.example".into())
        );
        assert_eq!(url_host(""), None);
    }
}
