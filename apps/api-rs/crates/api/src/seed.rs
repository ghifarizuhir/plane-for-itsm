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

fn slugify(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

async fn insert_states(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let mut map = std::collections::HashMap::new();
    for s in parse_seed::<StateSeed>(STATES_JSON) {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO states (id, name, description, color, slug, created_by_id, project_id, \
             workspace_id, sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', $2, $3, $4, $5, $6, $7, $8, $9, false, now(), now()) \
             RETURNING id",
        )
        .bind(&s.name)
        .bind(&s.color)
        .bind(slugify(&s.name))
        .bind(bot_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(s.sequence)
        .bind(&s.group)
        .bind(s.is_default)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(s.id, id);
    }
    Ok(map)
}

async fn insert_labels(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let mut map = std::collections::HashMap::new();
    for l in parse_seed::<LabelSeed>(LABELS_JSON) {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO labels (id, name, color, description, project_id, workspace_id, parent_id, \
             sort_order, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, '', $3, $4, NULL, $5, $6, $6, now(), now()) \
             RETURNING id",
        )
        .bind(&l.name)
        .bind(&l.color)
        .bind(project_id)
        .bind(workspace_id)
        .bind(l.sort_order)
        .bind(bot_id)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(l.id, id);
    }
    Ok(map)
}

async fn insert_cycles(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let now = Utc::now();
    let mut map = std::collections::HashMap::new();
    let mut last_end: Option<DateTime<Utc>> = None;
    for c in parse_seed::<CycleSeed>(CYCLES_JSON) {
        let (start, end) = cycle_dates(&c.cycle_type, last_end, now);
        last_end = Some(end);
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO cycles (id, name, description, project_id, workspace_id, owned_by_id, \
             created_by_id, timezone, version, view_props, logo_props, progress_snapshot, \
             sort_order, start_date, end_date, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', $2, $3, $4, $4, $5, 1, '{}', '{}', '{}', $6, $7, \
             $8, now(), now()) RETURNING id",
        )
        .bind(&c.name)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .bind(&c.timezone)
        .bind(c.sort_order)
        .bind(start)
        .bind(end)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(c.id, id);
    }
    Ok(map)
}

async fn insert_modules(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<std::collections::HashMap<i64, Uuid>, sqlx::Error> {
    let now = Utc::now();
    let mut map = std::collections::HashMap::new();
    for (index, m) in parse_seed::<ModuleSeed>(MODULES_JSON)
        .into_iter()
        .enumerate()
    {
        let (start, target) = module_dates(index as i64, now);
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO modules (id, name, description, status, lead_id, project_id, workspace_id, \
             created_by_id, updated_by_id, view_props, logo_props, sort_order, start_date, \
             target_date, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, NULL, $4, $5, $6, $6, '{}', '{}', $7, $8, $9, \
             now(), now()) RETURNING id",
        )
        .bind(&m.name)
        .bind(&m.description)
        .bind(&m.status)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .bind(m.sort_order)
        .bind(start)
        .bind(target)
        .fetch_one(&mut **tx)
        .await?;
        map.insert(m.id, id);
    }
    Ok(map)
}

async fn insert_issues(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
    states: &std::collections::HashMap<i64, Uuid>,
    labels: &std::collections::HashMap<i64, Uuid>,
    cycles: &std::collections::HashMap<i64, Uuid>,
    modules: &std::collections::HashMap<i64, Uuid>,
) -> Result<(), anyhow::Error> {
    for issue in parse_seed::<IssueSeed>(ISSUES_JSON) {
        let state_id = states
            .get(&issue.state_id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("seed: unknown state_id {}", issue.state_id))?;
        let (issue_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO issues (id, name, description_html, description_json, \
             description_stripped, priority, start_date, target_date, sequence_id, sort_order, \
             completed_at, is_draft, estimate_point_id, parent_id, type_id, state_id, project_id, \
             workspace_id, created_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, '{}', $3, COALESCE($4, 'none'), NULL, NULL, $5, \
             $6, NULL, false, NULL, NULL, NULL, $7, $8, $9, $10, now(), now()) RETURNING id",
        )
        .bind(&issue.name)
        .bind(&issue.description_html)
        .bind(issue.description_stripped.as_deref())
        .bind(issue.priority.as_deref())
        .bind(issue.sequence_id)
        .bind(issue.sort_order)
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, \
             created_by_id, deleted, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, now(), now())",
        )
        .bind(issue.sequence_id)
        .bind(issue_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .execute(&mut **tx)
        .await?;
        crate::routes::issue_activity_write::insert_created_activity(
            tx,
            issue_id,
            project_id,
            workspace_id,
            bot_id,
            Utc::now().timestamp_millis() as f64 / 1000.0,
        )
        .await?;
        for seed_label in &issue.labels {
            let label_id = labels
                .get(seed_label)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("seed: unknown label id {seed_label}"))?;
            sqlx::query(
                "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
            )
            .bind(issue_id)
            .bind(label_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
        if let Some(seed_cycle) = issue.cycle_id {
            let cycle_id = cycles
                .get(&seed_cycle)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("seed: unknown cycle id {seed_cycle}"))?;
            sqlx::query(
                "INSERT INTO cycle_issues (id, cycle_id, issue_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
            )
            .bind(cycle_id)
            .bind(issue_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
        for seed_module in &issue.module_ids {
            let module_id = modules
                .get(seed_module)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("seed: unknown module id {seed_module}"))?;
            sqlx::query(
                "INSERT INTO module_issues (id, module_id, issue_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
            )
            .bind(module_id)
            .bind(issue_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_views(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    for v in parse_seed::<ViewSeed>(VIEWS_JSON) {
        sqlx::query(
            "INSERT INTO issue_views (id, name, description, query, filters, display_filters, \
             display_properties, rich_filters, logo_props, access, sort_order, is_locked, \
             project_id, workspace_id, owned_by_id, created_by_id, updated_by_id, created_at, \
             updated_at) \
             VALUES (gen_random_uuid(), $1, $2, '{}', $3, $4, $5, $6, '{}', $7, $8, false, $9, \
             $10, $11, $11, $11, now(), now())",
        )
        .bind(&v.name)
        .bind(&v.description)
        .bind(&v.filters)
        .bind(&v.display_filters)
        .bind(&v.display_properties)
        .bind(&v.rich_filters)
        .bind(v.access)
        .bind(v.sort_order)
        .bind(project_id)
        .bind(workspace_id)
        .bind(bot_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_pages(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    project_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    for p in parse_seed::<PageSeed>(PAGES_JSON) {
        let (page_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
             description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
             access, parent_id, archived_at, is_locked, view_props, logo_props, is_global, \
             sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '{}', NULL, $2, $3, $4, $4, $4, $5, '', $6, NULL, \
             NULL, false, '{}', '{}', false, 65535, now(), now()) RETURNING id",
        )
        .bind(&p.name)
        .bind(&p.description_html)
        .bind(p.description_stripped.as_deref())
        .bind(bot_id)
        .bind(workspace_id)
        .bind(p.access)
        .fetch_one(&mut **tx)
        .await?;
        if p.project_id.is_some() && p.page_type == "PROJECT" {
            sqlx::query(
                "INSERT INTO project_pages (id, workspace_id, project_id, page_id, created_by_id, \
                 updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now())",
            )
            .bind(workspace_id)
            .bind(project_id)
            .bind(page_id)
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Seed demo untuk satu workspace baru. Semua insert dalam satu transaksi;
/// error menggagalkan seluruh seed (caller hanya mencatat log).
pub async fn seed_workspace(pool: &PgPool, workspace_id: Uuid) -> Result<(), anyhow::Error> {
    let mut tx = pool.begin().await?;
    let workspace_name: Option<(String,)> =
        sqlx::query_as("SELECT name FROM workspaces WHERE id = $1 AND deleted_at IS NULL")
            .bind(workspace_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((workspace_name,)) = workspace_name else {
        anyhow::bail!("seed: workspace {workspace_id} not found");
    };
    let bot_id = insert_bot_user(&mut tx, workspace_id).await?;
    insert_workspace_member(&mut tx, workspace_id, bot_id).await?;
    let project_id = insert_project(&mut tx, workspace_id, &workspace_name, bot_id).await?;
    insert_project_members_and_props(&mut tx, workspace_id, project_id, bot_id).await?;
    let states = insert_states(&mut tx, workspace_id, project_id, bot_id).await?;
    let labels = insert_labels(&mut tx, workspace_id, project_id, bot_id).await?;
    let cycles = insert_cycles(&mut tx, workspace_id, project_id, bot_id).await?;
    let modules = insert_modules(&mut tx, workspace_id, project_id, bot_id).await?;
    insert_issues(
        &mut tx,
        workspace_id,
        project_id,
        bot_id,
        &states,
        &labels,
        &cycles,
        &modules,
    )
    .await?;
    insert_views(&mut tx, workspace_id, project_id, bot_id).await?;
    insert_pages(&mut tx, workspace_id, project_id, bot_id).await?;
    tx.commit().await?;
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
