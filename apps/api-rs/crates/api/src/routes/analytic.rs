use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

use super::cycle::{guard_am, guard_amg};
use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};
use crate::routes::project::{deny, missing, ws_role, FORBIDDEN_MSG};

/// Read-only aggregates for `plane/app/urls/analytic.py`:
/// - `GET workspaces/:slug/default-analytics/` (DefaultAnalyticsEndpoint):
///   totals, state-group classification, month-wise completions (current
///   year), top creators/closers/pending assignees, estimate sums.
/// - `GET workspaces/:slug/project-stats/?fields=&project_ids=`
///   (ProjectStatsEndpoint): per-project total/completed issues, members
///   (non-bot, active), cycles, modules.
/// - `GET/POST workspaces/:slug/analytic-view/` (AnalyticViewViewset
///   list/create): saved views; `name` required/255, `query` required.
/// - `GET workspaces/:slug/advance-analytics/` (AdvanceAnalyticsEndpoint,
///   `views/analytic/advance.py:104-119`): `?tab=overview|work-items`.
/// - `GET workspaces/:slug/advance-analytics-stats/` (`advance.py:158-169`):
///   per-project state-group work-item counts.
/// - `GET workspaces/:slug/advance-analytics-charts/` (`advance.py:285-318`):
///   projects chart, custom `build_analytics_chart`, monthly completions.
/// - `GET workspaces/:slug/projects/:pid/advance-analytics/`
///   (`views/analytic/project_analytics.py:84-94`): project work-item stats.
/// - `GET .../advance-analytics-stats/` (`project_analytics.py:165-179`):
///   per-assignee work-item counts.
/// - `GET .../advance-analytics-charts/` (`project_analytics.py:317-367`):
///   custom chart + daily/monthly completions.
/// - Deploy boards (`views/project/base.py:535-576`,
///   `urls/project.py:113-118`): list/get-or-create upsert/retrieve/patch/
///   soft-delete.
///
/// STAYS ON DJANGO: `analytics/`, `export-analytics/`,
/// `saved-analytic-view/` (custom `build_graph_plot` histogram builder).
pub const VALID_X_AXIS: [&str; 12] = [
    "state_id",
    "state__group",
    "labels__id",
    "assignees__id",
    "estimate_point__value",
    "issue_cycle__cycle_id",
    "issue_module__module_id",
    "priority",
    "start_date",
    "target_date",
    "created_at",
    "completed_at",
];

pub const VALID_Y_AXIS: [&str; 2] = ["issue_count", "estimate"];

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AxisParams {
    #[serde(default)]
    pub x_axis: Option<String>,
    #[serde(default)]
    pub y_axis: Option<String>,
    #[serde(default)]
    pub segment: Option<String>,
}

/// Mirrors the axis guards in AnalyticsEndpoint.get / SavedAnalyticEndpoint.
pub fn validate_axes(p: &AxisParams) -> Result<(), String> {
    match (&p.x_axis, &p.y_axis) {
        (Some(x), Some(y)) if VALID_X_AXIS.contains(&x.as_str()) && VALID_Y_AXIS.contains(&y.as_str()) => {}
        _ => return Err("x-axis and y-axis dimensions are required and the values should be valid".to_string()),
    }
    if let Some(segment) = &p.segment {
        if !VALID_X_AXIS.contains(&segment.as_str()) || Some(segment) == p.x_axis.as_ref() {
            return Err("Both segment and x axis cannot be same and segment should be valid".to_string());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAnalyticView {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub query: Value,
    #[serde(default)]
    pub query_dict: Option<Value>,
}

pub fn validate_view_create(body: &CreateAnalyticView) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if body.query.is_null() {
        return Err("query is required".to_string());
    }
    Ok(())
}

pub async fn default_analytics(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Value>, common::errors::AppError> {
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.deleted_at IS NULL",
    )
    .bind(&slug).fetch_one(&st.pool).await?;

    let classified: Vec<(String, i64)> = sqlx::query_as(
        "SELECT s.\"group\", COUNT(*) FROM issues i JOIN states s ON s.id = i.state_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.deleted_at IS NULL GROUP BY s.\"group\" ORDER BY s.\"group\"",
    )
    .bind(&slug).fetch_all(&st.pool).await?;

    let open: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues i JOIN states s ON s.id = i.state_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND s.\"group\" IN ('backlog','unstarted','started') AND i.deleted_at IS NULL",
    )
    .bind(&slug).fetch_one(&st.pool).await?;

    let open_classified: Vec<(String, i64)> = sqlx::query_as(
        "SELECT s.\"group\", COUNT(*) FROM issues i JOIN states s ON s.id = i.state_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND s.\"group\" IN ('backlog','unstarted','started') AND i.deleted_at IS NULL GROUP BY s.\"group\" ORDER BY s.\"group\"",
    )
    .bind(&slug).fetch_all(&st.pool).await?;

    let month_wise: Vec<(i32, i64)> = sqlx::query_as(
        "SELECT EXTRACT(MONTH FROM i.completed_at)::int, COUNT(*) FROM issues i JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND EXTRACT(YEAR FROM i.completed_at) = EXTRACT(YEAR FROM now()) AND i.deleted_at IS NULL GROUP BY 1 ORDER BY 1",
    )
    .bind(&slug).fetch_all(&st.pool).await?;

    let top_creators: Vec<(uuid::Uuid, String, i64)> = sqlx::query_as(
        "SELECT u.id, u.display_name, COUNT(*) FROM issues i JOIN users u ON u.id = i.created_by_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.created_by_id IS NOT NULL AND i.deleted_at IS NULL GROUP BY u.id, u.display_name ORDER BY COUNT(*) DESC LIMIT 5",
    )
    .bind(&slug).fetch_all(&st.pool).await?;

    let top_closers: Vec<(uuid::Uuid, String, i64)> = sqlx::query_as(
        "SELECT u.id, u.display_name, COUNT(*) FROM issues i JOIN issue_assignees a ON a.issue_id = i.id JOIN users u ON u.id = a.assignee_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.completed_at IS NOT NULL AND i.deleted_at IS NULL GROUP BY u.id, u.display_name ORDER BY COUNT(*) DESC LIMIT 5",
    )
    .bind(&slug).fetch_all(&st.pool).await?;

    let pending: Vec<(uuid::Uuid, String, i64)> = sqlx::query_as(
        "SELECT u.id, u.display_name, COUNT(*) FROM issues i JOIN issue_assignees a ON a.issue_id = i.id JOIN users u ON u.id = a.assignee_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.completed_at IS NULL AND i.deleted_at IS NULL GROUP BY u.id, u.display_name ORDER BY COUNT(*) DESC LIMIT 5",
    )
    .bind(&slug).fetch_all(&st.pool).await?;

    let open_estimate: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(i.point) FROM issues i JOIN states s ON s.id = i.state_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND s.\"group\" IN ('backlog','unstarted','started') AND i.deleted_at IS NULL",
    )
    .bind(&slug).fetch_one(&st.pool).await?;
    let total_estimate: (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(i.point) FROM issues i JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.deleted_at IS NULL",
    )
    .bind(&slug).fetch_one(&st.pool).await?;

    let user_row = |(id, display_name, count): (uuid::Uuid, String, i64)| json!({"id": id, "display_name": display_name, "count": count});
    Ok(Json(json!({
        "total_issues": total.0,
        "total_issues_classified": classified.into_iter().map(|(g, c)| json!({"state_group": g, "state_count": c})).collect::<Vec<_>>(),
        "open_issues": open.0,
        "open_issues_classified": open_classified.into_iter().map(|(g, c)| json!({"state_group": g, "state_count": c})).collect::<Vec<_>>(),
        "issue_completed_month_wise": month_wise.into_iter().map(|(m, c)| json!({"month": m, "count": c})).collect::<Vec<_>>(),
        "most_issue_created_user": top_creators.into_iter().map(user_row).collect::<Vec<_>>(),
        "most_issue_closed_user": top_closers.into_iter().map(user_row).collect::<Vec<_>>(),
        "pending_issue_user": pending.into_iter().map(user_row).collect::<Vec<_>>(),
        "open_estimate_sum": open_estimate.0,
        "total_estimate_sum": total_estimate.0,
    })))
}

#[derive(Debug, Deserialize, Default)]
pub struct ProjectStatsQuery {
    #[serde(default)]
    pub fields: Option<String>,
    #[serde(default)]
    pub project_ids: Option<String>,
}

const STAT_FIELDS: [&str; 5] = ["total_issues", "completed_issues", "total_members", "total_cycles", "total_modules"];

pub async fn project_stats(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<ProjectStatsQuery>,
) -> Result<Json<Value>, common::errors::AppError> {
    let requested: Vec<&str> = match &q.fields {
        Some(f) => f.split(',').map(str::trim).filter(|f| STAT_FIELDS.contains(f)).collect(),
        None => vec![],
    };
    let all = requested.is_empty();
    let want = |f: &str| all || requested.contains(&f);

    let ids: Option<Vec<uuid::Uuid>> = q.project_ids.as_deref().map(|s| {
        s.split(',').filter_map(|p| p.trim().parse::<uuid::Uuid>().ok()).collect()
    });

    let projects: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT p.id FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $1 AND ($2::uuid[] IS NULL OR p.id = ANY($2)) AND p.deleted_at IS NULL ORDER BY p.created_at",
    )
    .bind(&slug).bind(ids.clone()).fetch_all(&st.pool).await?;

    let mut out = vec![];
    for (pid,) in projects {
        let mut row = serde_json::Map::new();
        row.insert("id".to_string(), json!(pid));
        if want("total_issues") {
            let (c,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE project_id = $1 AND deleted_at IS NULL")
                .bind(pid).fetch_one(&st.pool).await?;
            row.insert("total_issues".to_string(), json!(c));
        }
        if want("completed_issues") {
            let (c,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues i JOIN states s ON s.id = i.state_id WHERE i.project_id = $1 AND s.\"group\" IN ('completed','cancelled') AND i.deleted_at IS NULL")
                .bind(pid).fetch_one(&st.pool).await?;
            row.insert("completed_issues".to_string(), json!(c));
        }
        if want("total_members") {
            let (c,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM project_members pm JOIN users u ON u.id = pm.member_id WHERE pm.project_id = $1 AND u.is_bot = false AND pm.is_active = true AND pm.deleted_at IS NULL")
                .bind(pid).fetch_one(&st.pool).await?;
            row.insert("total_members".to_string(), json!(c));
        }
        if want("total_cycles") {
            let (c,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cycles WHERE project_id = $1 AND deleted_at IS NULL")
                .bind(pid).fetch_one(&st.pool).await?;
            row.insert("total_cycles".to_string(), json!(c));
        }
        if want("total_modules") {
            let (c,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM modules WHERE project_id = $1 AND deleted_at IS NULL")
                .bind(pid).fetch_one(&st.pool).await?;
            row.insert("total_modules".to_string(), json!(c));
        }
        out.push(Value::Object(row));
    }
    Ok(Json(Value::Array(out)))
}

pub async fn list_views(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT a.id, a.name FROM analytic_views a JOIN workspaces w ON w.id = a.workspace_id WHERE w.slug = $1 AND a.deleted_at IS NULL ORDER BY a.created_at DESC",
    )
    .bind(&slug).fetch_all(&st.pool).await?;
    Ok(Json(rows.into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect()))
}

pub async fn create_view(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateAnalyticView>,
) -> Result<(axum::http::StatusCode, Json<Value>), common::errors::AppError> {
    validate_view_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let row: (uuid::Uuid, String) = sqlx::query_as(
        "INSERT INTO analytic_views (id, name, description, query, query_dict, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, w.id, now(), now() FROM workspaces w WHERE w.slug = $5 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(&body.query)
    .bind(body.query_dict.clone().unwrap_or(json!({})))
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(json!({"id": row.0, "name": row.1}))))
}

// ============================================================================
// E10 — advance-analytics + deploy-boards.
// ============================================================================
//
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/analytic/advance.py:119`
/// (`{"message": "Invalid tab"}`).
pub const INVALID_TAB_MSG: &str = "Invalid tab";
/// `plane/app/views/analytic/advance.py:169`
/// (stats), `:318` (charts), `project_analytics.py:179,367`
/// (`{"message": "Invalid type"}`).
pub const INVALID_TYPE_MSG: &str = "Invalid type";
/// DRF permission-class deny body for `ProjectMemberPermission`
/// (`plane/app/permissions/project.py:56-88`, deploy boards) — same shape
/// as the E2/E3 `deny_detail` twins (`cycle.rs`, `module.rs`).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";
/// `plane/utils/build_chart.py:161`
/// (`raise ValidationError(f"Invalid x_axis field: {x_axis}")`).
pub fn invalid_x_axis_msg(axis: &str) -> String {
    format!("Invalid x_axis field: {axis}")
}
/// `plane/utils/build_chart.py:165`
/// (`raise ValidationError(f"Invalid group_by field: {group_by}")`).
pub fn invalid_group_by_msg(group: &str) -> String {
    format!("Invalid group_by field: {group}")
}

/// Overview keys in Django order (`advance.py:78-91`).
pub const OVERVIEW_KEYS: [&str; 8] = [
    "total_users",
    "total_admins",
    "total_members",
    "total_guests",
    "total_projects",
    "total_work_items",
    "total_cycles",
    "total_intake",
];

/// Work-item keys in Django order (`advance.py:96-102`,
/// `project_analytics.py:76-82`).
pub const WORK_ITEM_KEYS: [&str; 5] = [
    "total_work_items",
    "started_work_items",
    "backlog_work_items",
    "un_started_work_items",
    "completed_work_items",
];

/// `plane/utils/build_chart.py:20-34` (`x_axis_mapper` keys).
pub const CHART_X_AXES: [&str; 13] = [
    "STATES",
    "STATE_GROUPS",
    "LABELS",
    "ASSIGNEES",
    "ESTIMATE_POINTS",
    "CYCLES",
    "MODULES",
    "PRIORITY",
    "START_DATE",
    "TARGET_DATE",
    "CREATED_AT",
    "COMPLETED_AT",
    "CREATED_BY",
];

/// Daily-series cap, mirroring the 732-day clamp on the cycle burndown
/// (`cycle.rs:burndown_chart`, from `utils/analytics_plot.py:236-264`).
pub const MAX_SERIES_DAYS: i64 = 732;
/// Monthly-series safety cap (workspace/project creation month → now).
pub const MAX_SERIES_MONTHS: i64 = 1200;

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// `?tab=` for A1 (`advance.py:107-119`): default `overview`; anything else
/// besides `work-items` → 400 `{"message": "Invalid tab"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvTab {
    Overview,
    WorkItems,
}

pub fn resolve_advance_tab(raw: Option<&str>) -> Result<AdvTab, String> {
    match raw.unwrap_or("overview") {
        "overview" => Ok(AdvTab::Overview),
        "work-items" => Ok(AdvTab::WorkItems),
        _ => Err(INVALID_TAB_MSG.to_string()),
    }
}

/// `?type=` for A2/A3/A6: Django `request.GET.get("type", <default>)`
/// (`advance.py:161,287`, `project_analytics.py:168,320`); any other value
/// → 400 `{"message": "Invalid type"}` (`advance.py:169,318`,
/// `project_analytics.py:179,367`). NOTE A6 defaults to `"projects"` which
/// is NOT in its allowed list, so a bare A6 call 400s — mirrored literally.
pub fn resolve_advance_type(raw: Option<&str>, default: &str, allowed: &[&str]) -> Result<String, String> {
    let t = raw.unwrap_or(default);
    if allowed.contains(&t) {
        Ok(t.to_string())
    } else {
        Err(INVALID_TYPE_MSG.to_string())
    }
}

/// Assembles the A1-overview object (`advance.py:78-91`): 8 `{count}` keys
/// (`get_filtered_counts` returns `{"count"}` only — the `filter_count`
/// line is commented out in Django).
pub fn overview_json(counts: [i64; 8]) -> Value {
    let mut m = serde_json::Map::new();
    for (k, c) in OVERVIEW_KEYS.iter().zip(counts.iter()) {
        m.insert((*k).to_string(), json!({"count": c}));
    }
    Value::Object(m)
}

/// Assembles the work-items object (`advance.py:93-102`): 5 `{count}` keys.
pub fn work_items_json(counts: [i64; 5]) -> Value {
    let mut m = serde_json::Map::new();
    for (k, c) in WORK_ITEM_KEYS.iter().zip(counts.iter()) {
        m.insert((*k).to_string(), json!({"count": c}));
    }
    Value::Object(m)
}

/// Mirrors `key.replace("_", " ").title()` (`advance.py:211`):
/// `"work_items"` → `"Work Items"`.
pub fn title_case(key: &str) -> String {
    key.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Mirrors `get_analytics_date_range` (`plane/utils/date_utils.py:12-87`):
/// `None`/unknown/`custom`-without-both-dates → `None` (no filtering).
/// Bounds are day edges in UTC (`datetime.min/max.time()`); the Django
/// `previous` comparison window is dropped — `get_filtered_counts`
/// (`advance.py:44-64`) only ever reads `["current"]` (`filter_count` is
/// commented out).
pub struct AnalyticsWindow {
    pub gte: DateTime<Utc>,
    pub lte: DateTime<Utc>,
}

fn day_edges(d: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let gte = d.and_hms_opt(0, 0, 0).expect("midnight valid").and_utc();
    let lte = d
        .and_hms_micro_opt(23, 59, 59, 999_999)
        .expect("day-end valid")
        .and_utc();
    (gte, lte)
}

pub fn analytics_window(
    date_filter: Option<&str>,
    start_date: Option<&str>,
    end_date: Option<&str>,
    today: NaiveDate,
) -> Option<AnalyticsWindow> {
    let (gte_day, lte_day) = match date_filter {
        None => return None,
        Some("yesterday") => {
            let y = today.pred_opt()?;
            (y, y)
        }
        Some("last_7_days") => (today.checked_sub_days(chrono::Days::new(7))?, today),
        Some("last_30_days") => (today.checked_sub_days(chrono::Days::new(30))?, today),
        Some("last_3_months") => (today.checked_sub_days(chrono::Days::new(90))?, today),
        Some("custom") => {
            let s = NaiveDate::parse_from_str(start_date?.trim(), "%Y-%m-%d").ok()?;
            let e = NaiveDate::parse_from_str(end_date?.trim(), "%Y-%m-%d").ok()?;
            (s, e)
        }
        Some(_) => return None,
    };
    let (gte, _) = day_edges(gte_day);
    let (_, lte) = day_edges(lte_day);
    Some(AnalyticsWindow { gte, lte })
}

/// Mirrors `get_chart_period_range` (`plane/utils/date_utils.py:90-122`):
/// `None`/unknown — and notably `custom` (no custom branch exists there) —
/// → `None` (no filtering). Days are `created_at__date` bounds.
pub fn chart_window(date_filter: Option<&str>, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    match date_filter {
        None => None,
        Some("yesterday") => today.pred_opt().map(|y| (y, y)),
        Some("last_7_days") => today.checked_sub_days(chrono::Days::new(7)).map(|s| (s, today)),
        Some("last_30_days") => today.checked_sub_days(chrono::Days::new(30)).map(|s| (s, today)),
        Some("last_3_months") => today.checked_sub_days(chrono::Days::new(90)).map(|s| (s, today)),
        Some(_) => None,
    }
}

/// Parses the `?project_ids=` csv (`date_utils.py:150-151`,
/// `advance.py:71-73`): split on comma; unparseable entries are dropped —
/// same leniency as the pre-existing `project_stats` handler above.
pub fn parse_project_ids(raw: Option<&str>) -> Option<Vec<uuid::Uuid>> {
    raw.map(|s| {
        s.split(',')
            .filter_map(|p| p.trim().parse::<uuid::Uuid>().ok())
            .collect()
    })
}

/// SQL fragments for one `build_chart` axis (`plane/utils/build_chart.py:
/// 44-75`, `get_x_axis_field`). `key_sql`/`name_sql` are SELECT expressions
/// (aliased by the caller); `join_sql` holds the extra joins, including the
/// `additional_filter` deleted-at guards (`:173-177` — LABELS/ASSIGNEES/
/// CYCLES/MODULES inner-join, so issues without that relation are dropped,
/// exactly like Django's `.filter(label_issue__deleted_at__isnull=True)`).
/// `numeric_key` marks the integer axis (ESTIMATE_POINTS `key`), whose
/// falsy `0` maps to `"none"` in the pivot, mirroring Python truthiness.
#[derive(Debug)]
pub struct ChartField {
    pub key_sql: &'static str,
    pub name_sql: &'static str,
    pub join_sql: &'static str,
    pub numeric_key: bool,
}

pub fn chart_field(axis: &str) -> Result<ChartField, String> {
    // Mapper-membership gate first (`x_axis not in x_axis_mapper`,
    // `build_chart.py:160-161`); the match below is the field table
    // (`get_x_axis_field`, `:44-75`).
    if !CHART_X_AXES.contains(&axis) {
        return Err(invalid_x_axis_msg(axis));
    }
    let f = match axis {
        "STATES" => ChartField { key_sql: "s.id", name_sql: "s.name", join_sql: "", numeric_key: false },
        "STATE_GROUPS" => ChartField {
            key_sql: "s.\"group\"",
            name_sql: "s.\"group\"",
            join_sql: "",
            numeric_key: false,
        },
        "LABELS" => ChartField {
            key_sql: "l.id",
            name_sql: "l.name",
            join_sql: "JOIN issue_labels il ON il.issue_id = i.id AND il.deleted_at IS NULL \
                       JOIN labels l ON l.id = il.label_id AND l.deleted_at IS NULL",
            numeric_key: false,
        },
        "ASSIGNEES" => ChartField {
            key_sql: "u.id",
            name_sql: "u.display_name",
            join_sql: "JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.deleted_at IS NULL \
                       JOIN users u ON u.id = ia.assignee_id",
            numeric_key: false,
        },
        "ESTIMATE_POINTS" => ChartField {
            key_sql: "ep.key",
            name_sql: "ep.value",
            join_sql: "LEFT JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL",
            numeric_key: true,
        },
        "CYCLES" => ChartField {
            key_sql: "c.id",
            name_sql: "c.name",
            join_sql: "JOIN cycle_issues ci_c ON ci_c.issue_id = i.id AND ci_c.deleted_at IS NULL \
                       JOIN cycles c ON c.id = ci_c.cycle_id AND c.deleted_at IS NULL",
            numeric_key: false,
        },
        "MODULES" => ChartField {
            key_sql: "m.id",
            name_sql: "m.name",
            join_sql: "JOIN module_issues mi_m ON mi_m.issue_id = i.id AND mi_m.deleted_at IS NULL \
                       JOIN modules m ON m.id = mi_m.module_id AND m.deleted_at IS NULL",
            numeric_key: false,
        },
        "PRIORITY" => ChartField { key_sql: "i.priority", name_sql: "i.priority", join_sql: "", numeric_key: false },
        "START_DATE" => ChartField { key_sql: "i.start_date", name_sql: "i.start_date", join_sql: "", numeric_key: false },
        "TARGET_DATE" => ChartField { key_sql: "i.target_date", name_sql: "i.target_date", join_sql: "", numeric_key: false },
        "CREATED_AT" => ChartField {
            key_sql: "i.created_at::date",
            name_sql: "i.created_at::date",
            join_sql: "",
            numeric_key: false,
        },
        "COMPLETED_AT" => ChartField {
            key_sql: "i.completed_at::date",
            name_sql: "i.completed_at::date",
            join_sql: "",
            numeric_key: false,
        },
        "CREATED_BY" => ChartField {
            key_sql: "cb.id",
            name_sql: "cb.display_name",
            join_sql: "LEFT JOIN users cb ON cb.id = i.created_by_id",
            numeric_key: false,
        },
        _ => return Err(invalid_x_axis_msg(axis)),
    };
    Ok(f)
}
/// Validates `group_by` against the same mapper (`build_chart.py:163-165`).
pub fn chart_group_field(group_by: &str) -> Result<ChartField, String> {
    chart_field(group_by).map_err(|_| invalid_group_by_msg(group_by))
}

/// Locked §2 validation shape: 400 `{"detail": msg}`. Used for the
/// `ValidationError`s raised by `build_analytics_chart` (`build_chart.py:
/// 161,165`) — DRF renders those as 400s.
pub fn detail_400(msg: String) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))
}

/// DRF permission-class deny body (deploy boards).
pub fn deny_detail() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"detail": PERMISSION_DETAIL_MSG})))
}

/// One row of the simple (ungrouped) chart (`build_simple_chart_response`,
/// `build_chart.py:133-150`): NULL key/name → `"None"` (capital N).
pub fn simple_chart_row(key: Option<&str>, name: Option<&str>, count: i64) -> Value {
    json!({
        "key": key.filter(|k| !k.is_empty()).unwrap_or("None"),
        "name": name.filter(|n| !n.is_empty()).unwrap_or("None"),
        "count": count,
    })
}

/// Input row for the grouped pivot.
pub struct GroupedInput {
    pub key: Option<String>,
    pub display: Option<String>,
    pub group_key: Option<String>,
    pub group_name: Option<String>,
    pub count: i64,
    pub numeric_key: bool,
    pub numeric_group: bool,
}

/// Ports `process_grouped_data` (`plane/utils/build_chart.py:78-98`)
/// literally: buckets keyed by raw key with `"key": key or "none"`
/// (lowercase!), `"name": (display_name or key) or "None"`, per-group
/// sub-counts plus a `count` total; `schema` maps each group key
/// (`str(group_key) or "none"`) to its name (`or "None"`). Integer `0`
/// keys are falsy in Python — the `numeric_*` flags reproduce that for the
/// ESTIMATE_POINTS axis.
pub fn process_grouped(rows: &[GroupedInput]) -> (Vec<Value>, Value) {
    let mut order: Vec<String> = vec![];
    let mut buckets: std::collections::HashMap<String, serde_json::Map<String, Value>> =
        std::collections::HashMap::new();
    let mut schema = serde_json::Map::new();
    for r in rows {
        let falsy_key = match &r.key {
            None => true,
            Some(k) => k.is_empty() || (r.numeric_key && k == "0"),
        };
        let raw_key = r.key.clone().unwrap_or_default();
        let disp = r.display.clone().filter(|s| !s.is_empty());
        let name = disp
            .or_else(|| if falsy_key { None } else { Some(raw_key.clone()) })
            .unwrap_or_else(|| "None".to_string());
        let out_key = if falsy_key { "none".to_string() } else { raw_key.clone() };
        let bucket = buckets.entry(raw_key.clone()).or_insert_with(|| {
            order.push(raw_key.clone());
            let mut m = serde_json::Map::new();
            m.insert("key".to_string(), json!(out_key));
            m.insert("name".to_string(), json!(name));
            m.insert("count".to_string(), json!(0));
            m
        });
        let falsy_group = match &r.group_key {
            None => true,
            Some(g) => g.is_empty() || (r.numeric_group && g == "0"),
        };
        let gk = if falsy_group {
            "none".to_string()
        } else {
            r.group_key.clone().unwrap_or_default()
        };
        let gn = r.group_name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "None".to_string());
        schema.insert(gk.clone(), json!(gn));
        let cur = bucket.get(&gk).and_then(Value::as_i64).unwrap_or(0);
        bucket.insert(gk, json!(cur + r.count));
        let total = bucket.get("count").and_then(Value::as_i64).unwrap_or(0);
        bucket.insert("count".to_string(), json!(total + r.count));
    }
    let data = order
        .into_iter()
        .filter_map(|k| buckets.remove(&k).map(Value::Object))
        .collect();
    (data, Value::Object(schema))
}

/// First-of-month key (`strftime("%Y-%m-%d")` on the month, `advance.py:
/// 246,293`).
pub fn month_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Next calendar month, overflow-safe (`advance.py:273-276,304-308`).
pub fn next_month(d: NaiveDate) -> NaiveDate {
    let (y, m) = (d.year(), d.month());
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).unwrap_or(d)
}

use chrono::Datelike;

/// Builds a `{key,name,count,completed_issues,created_issues}` series over
/// a month range (`advance.py:254-281,286-308`): months with no rows are
/// zero-filled; `count` is the created count (NOT the sum — `:267`).
/// Capped at `MAX_SERIES_MONTHS` steps so a degenerate range can never spin.
pub fn monthly_series(
    start_month: NaiveDate,
    last_month: NaiveDate,
    stats: &std::collections::BTreeMap<String, (i64, i64)>,
) -> Vec<Value> {
    let mut out = vec![];
    let mut cur = start_month;
    let mut steps = 0;
    while cur <= last_month && steps < MAX_SERIES_MONTHS {
        let k = month_key(cur);
        let (created, completed) = stats.get(&k).copied().unwrap_or((0, 0));
        out.push(json!({
            "key": k, "name": k,
            "count": created,
            "completed_issues": completed,
            "created_issues": created,
        }));
        let n = next_month(cur);
        if n <= cur {
            break;
        }
        cur = n;
        steps += 1;
    }
    out
}

/// Builds the daily series for a cycle/module range
/// (`project_analytics.py:243-258`): zero-filled; `count` is
/// created+completed (the literal `:253` sum — differs from monthly).
/// Clamped to `MAX_SERIES_DAYS` like the cycle burndown.
pub fn daily_series(
    start: NaiveDate,
    end: NaiveDate,
    stats: &std::collections::BTreeMap<String, (i64, i64)>,
) -> Vec<Value> {
    let mut end = end;
    if (end - start).num_days() > MAX_SERIES_DAYS {
        end = start.checked_add_days(chrono::Days::new(MAX_SERIES_DAYS as u64)).unwrap_or(end);
    }
    let mut out = vec![];
    let mut cur = start;
    while cur <= end {
        let k = cur.to_string();
        let (created, completed) = stats.get(&k).copied().unwrap_or((0, 0));
        out.push(json!({
            "key": k, "name": k,
            "count": created + completed,
            "completed_issues": completed,
            "created_issues": created,
        }));
        let Some(n) = cur.succ_opt() else { break };
        cur = n;
    }
    out
}

/// The completion-chart schema (`advance.py:278-281,310-313`).
pub fn completion_schema() -> Value {
    json!({"completed_issues": "completed_issues", "created_issues": "created_issues"})
}

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER], level="WORKSPACE")`
/// (`advance.py:104,158,285`): workspace ADMIN(20)/MEMBER(15); GUEST(5)
/// denied. Deny is the `allow_permission` 403 `deny()`.
pub fn guard_ws_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `ProjectMemberPermission` SAFE branch
/// (`plane/app/permissions/project.py:61-69`, deploy-board GETs): any
/// active project member, roles unchecked, strict (no workspace-ADMIN
/// fallback — the permission class has none). Deny is the DRF 403 detail.
pub fn guard_deploy_read(has_membership: bool) -> Result<(), String> {
    if has_membership {
        Ok(())
    } else {
        Err(PERMISSION_DETAIL_MSG.to_string())
    }
}

/// Mirrors `ProjectMemberPermission` unsafe branch
/// (`permissions/project.py:80-87`, deploy-board PATCH/DELETE): project
/// ADMIN(20)/MEMBER(15) only, strict. Deny is the DRF 403 detail.
pub fn guard_deploy_write(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(PERMISSION_DETAIL_MSG.to_string()),
    }
}

/// Default `views` for deploy-board create (`views/project/base.py:553-562`):
/// all five layouts enabled.
pub fn deploy_default_views() -> Value {
    json!({"list": true, "kanban": true, "calendar": true, "gantt": true, "spreadsheet": true})
}

/// Picks the stored `view_props` from a create body (`base.py:553-562`):
/// `views` when present (non-null), else the all-true default.
pub fn deploy_views_from(body: &Value) -> Value {
    match body.get("views") {
        Some(v) if !v.is_null() => v.clone(),
        _ => deploy_default_views(),
    }
}

/// Picks the stored `view_props` from a PATCH body: the `view_props`
/// serializer field wins, `views` is accepted as an alias.
pub fn deploy_views_patch(body: &Value) -> Option<Value> {
    match body.get("view_props") {
        Some(v) if !v.is_null() => Some(v.clone()),
        _ => match body.get("views") {
            Some(v) if !v.is_null() => Some(v.clone()),
            _ => None,
        },
    }
}

/// Flag triples for create (`base.py:549-552`: `request.data.get(..., False)`
/// — missing/null/non-bool → false).
pub fn deploy_flags(body: &Value) -> (bool, bool, bool) {
    let flag = |k: &str| body.get(k).and_then(Value::as_bool).unwrap_or(false);
    (flag("is_comments_enabled"), flag("is_reactions_enabled"), flag("is_votes_enabled"))
}

/// Parses an optional `intake` body value: missing/null → `Ok(None)`;
/// otherwise must be a UUID (`serializers/project.py` FK) — anything else
/// is a 400 (Django would crash assigning it; normalized per contract).
pub fn parse_intake_id(body: &Value) -> Result<Option<uuid::Uuid>, String> {
    match body.get("intake") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => s.trim().parse::<uuid::Uuid>().map(Some).map_err(|_| "Invalid intake.".to_string()),
        Some(_) => Err("Invalid intake.".to_string()),
    }
}

// ============================================================================
// Shared gates + lookups.
// ============================================================================

async fn project_in_workspace(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(slug)
    .fetch_one(pool)
    .await
}

/// Gate for A1–A3: workspace ADMIN/MEMBER (`advance.py:104,158,285`).
async fn gate_ws_am(pool: &sqlx::PgPool, user: uuid::Uuid, slug: &str) -> Result<bool, sqlx::Error> {
    Ok(guard_ws_am(ws_role(pool, user, slug).await?).is_ok())
}

/// Gate for A4/A5: `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
/// (`project_analytics.py:84,165`) — project AM + the workspace-ADMIN
/// fallback (`permissions/base.py:53-78`), via the shared helper.
async fn gate_project_am(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(guard_am(role).is_ok(), role.is_some(), ws_admin))
}

/// Gate for A6: `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
/// (`project_analytics.py:317`) — project AMG + fallback.
async fn gate_project_amg(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(guard_amg(role).is_ok(), role.is_some(), ws_admin))
}

/// Gate for deploy-board SAFE reads: any active project member, strict.
async fn gate_deploy_read(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    Ok(guard_deploy_read(role.is_some()).is_ok())
}

/// Gate for deploy-board POST: workspace ADMIN/MEMBER
/// (`permissions/project.py:70-76`).
async fn gate_deploy_post(pool: &sqlx::PgPool, user: uuid::Uuid, slug: &str) -> Result<bool, sqlx::Error> {
    Ok(guard_ws_am(ws_role(pool, user, slug).await?).is_ok())
}

/// Gate for deploy-board PATCH/DELETE: project ADMIN/MEMBER, strict
/// (`permissions/project.py:80-87`).
async fn gate_deploy_write(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    Ok(guard_deploy_write(role).is_ok())
}

// ============================================================================
// Shared SQL predicates.
// ============================================================================
//
// `get_analytics_filters` (`plane/utils/date_utils.py:125-191`) + the
// `Issue.issue_objects` manager (`plane/db/models/issue.py:92-101`):
// workspace slug, requester-active-project-member, project
// deleted/archived NULL (+ `project_id__in`), triage excluded,
// archived/draft issues excluded.

/// `Issue.issue_objects` aliveness (`db/models/issue.py:92-101`):
/// soft-delete manager plus `state.group != triage`, `archived_at IS NULL`,
/// project not archived, `is_draft = false`.
///
/// (The `deleted_at` half of the soft manager is spelled out at each call
/// site's table.)
const PRED_ISSUE_OBJECTS: &str = "i.archived_at IS NULL AND i.is_draft = false AND s.\"group\" != 'triage'";

/// `project__deleted_at__isnull + project__archived_at__isnull`
/// (`date_utils.py:158-159`).
const PRED_PROJECT_ALIVE: &str = "p.deleted_at IS NULL AND p.archived_at IS NULL";

/// `project__project_projectmember__member = user, is_active = True`
/// (`date_utils.py:156-157`); `proj` is a SQL expression for the project id.
fn pred_member(proj: &str, user_ph: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = {proj} \
         AND pm.member_id = {user_ph} AND pm.is_active = true AND pm.deleted_at IS NULL)"
    )
}

/// `project_id__in` (`date_utils.py:172-174`); NULL/empty = no constraint.
/// (Callers bind `Option<Vec<Uuid>>`; an empty vec matches nothing —
/// same as Django filtering `id__in=[]`.)
fn pred_ids(proj: &str, ph: &str) -> String {
    format!("({ph}::uuid[] IS NULL OR {proj} = ANY({ph}))")
}

/// `analytics_date_range` on `created_at` (`advance.py:47-51`): NULL
/// bounds = no constraint (single statement either way).
fn pred_analytics_range(col: &str, gte_ph: &str, lte_ph: &str) -> String {
    format!("({gte_ph}::timestamptz IS NULL OR {col} >= {gte_ph}) AND ({lte_ph}::timestamptz IS NULL OR {col} <= {lte_ph})")
}

/// `chart_period_range` on `created_at__date` (`advance.py:128-130`).
fn pred_chart_range(start_ph: &str, end_ph: &str) -> String {
    format!("({start_ph}::date IS NULL OR i.created_at::date >= {start_ph}) AND ({end_ph}::date IS NULL OR i.created_at::date <= {end_ph})")
}

// ============================================================================
// A1 — advance-analytics (overview | work-items).
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdvanceQuery {
    #[serde(default)]
    pub tab: Option<String>,
    #[serde(default)]
    pub date_filter: Option<String>,
    #[serde(default)]
    pub project_ids: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

async fn count_issues(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
    group: Option<&str>,
    gte: Option<DateTime<Utc>>,
    lte: Option<DateTime<Utc>>,
) -> Result<i64, sqlx::Error> {
    let member = pred_member("i.project_id", "$2");
    let idf = pred_ids("i.project_id", "$3");
    let range = pred_analytics_range("i.created_at", "$5", "$6");
    let grp = group.map(|g| format!(" AND s.\"group\" = '{g}'")).unwrap_or_default();
    let sql = format!(
        "SELECT COUNT(*) FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {idf} AND {range}{grp}"
    );
    sqlx::query_scalar(&sql)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .bind(gte)
        .bind(lte)
        .fetch_one(pool)
        .await
}

async fn count_cycles(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
    gte: Option<DateTime<Utc>>,
    lte: Option<DateTime<Utc>>,
) -> Result<i64, sqlx::Error> {
    let member = pred_member("c.project_id", "$2");
    let idf = pred_ids("c.project_id", "$3");
    let range = pred_analytics_range("c.created_at", "$4", "$5");
    let sql = format!(
        "SELECT COUNT(*) FROM cycles c JOIN projects p ON p.id = c.project_id \
         JOIN workspaces w ON w.id = c.workspace_id \
         WHERE w.slug = $1 AND c.deleted_at IS NULL AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {idf} AND {range}"
    );
    sqlx::query_scalar(&sql)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .bind(gte)
        .bind(lte)
        .fetch_one(pool)
        .await
}

/// Plain-`Issue.objects` intake count (`advance.py:86-91`): NO
/// `issue_objects` aliveness (triage/archived/drafts INCLUDED — the
/// manager is `Issue.objects`), but the workspace/member/project scoping
/// plus `issue_intake__status__in = ["-2","-1","0","1","2"]` still apply.
/// `intake_issues.status` is integer; the string literals Django passes
/// coerce to -2..2.
async fn count_intake(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
    gte: Option<DateTime<Utc>>,
    lte: Option<DateTime<Utc>>,
) -> Result<i64, sqlx::Error> {
    let member = pred_member("i.project_id", "$2");
    let idf = pred_ids("i.project_id", "$3");
    let range = pred_analytics_range("i.created_at", "$4", "$5");
    let sql = format!(
        "SELECT COUNT(*) FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {idf} AND {range} \
         AND EXISTS(SELECT 1 FROM intake_issues ii WHERE ii.issue_id = i.id \
                    AND ii.deleted_at IS NULL AND ii.status IN (-2,-1,0,1,2))"
    );
    sqlx::query_scalar(&sql)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .bind(gte)
        .bind(lte)
        .fetch_one(pool)
        .await
}

async fn count_projects(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
    gte: Option<DateTime<Utc>>,
    lte: Option<DateTime<Utc>>,
) -> Result<i64, sqlx::Error> {
    let member = pred_member("p.id", "$2");
    let idf = pred_ids("p.id", "$3");
    let range = pred_analytics_range("p.created_at", "$4", "$5");
    let sql = format!(
        "SELECT COUNT(*) FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE w.slug = $1 AND {PRED_PROJECT_ALIVE} AND {member} AND {idf} AND {range}"
    );
    sqlx::query_scalar(&sql)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .bind(gte)
        .bind(lte)
        .fetch_one(pool)
        .await
}

async fn count_members(
    pool: &sqlx::PgPool,
    slug: &str,
    ids: &Option<Vec<uuid::Uuid>>,
    role: Option<i16>,
    gte: Option<DateTime<Utc>>,
    lte: Option<DateTime<Utc>>,
) -> Result<i64, sqlx::Error> {
    // `advance.py:67-76`: without `project_ids` the workspace members are
    // counted (active, non-bot); WITH `project_ids` the scope switches to
    // project members of those projects (active, non-bot) — notably WITHOUT
    // a workspace-slug constraint. Mirrored literally.
    if let Some(id_list) = ids {
        let range = pred_analytics_range("pm.created_at", "$2", "$3");
        let role_f = role.map(|r| format!(" AND pm.role = {r}")).unwrap_or_default();
        let sql = format!(
            "SELECT COUNT(*) FROM project_members pm JOIN users u ON u.id = pm.member_id \
             WHERE pm.project_id = ANY($1) AND pm.is_active = true AND pm.deleted_at IS NULL \
             AND u.is_bot = false{role_f} AND {range}"
        );
        sqlx::query_scalar(&sql).bind(id_list.clone()).bind(gte).bind(lte).fetch_one(pool).await
    } else {
        let range = pred_analytics_range("wm.created_at", "$2", "$3");
        let role_f = role.map(|r| format!(" AND wm.role = {r}")).unwrap_or_default();
        let sql = format!(
            "SELECT COUNT(*) FROM workspace_members wm JOIN users u ON u.id = wm.member_id \
             JOIN workspaces w ON w.id = wm.workspace_id \
             WHERE w.slug = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
             AND u.is_bot = false{role_f} AND {range}"
        );
        sqlx::query_scalar(&sql).bind(slug).bind(gte).bind(lte).fetch_one(pool).await
    }
}

pub async fn advance_overview(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<AdvanceQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `get` (`advance.py:104-119`): WORKSPACE ADMIN/MEMBER.
    if !gate_ws_am(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let tab = match resolve_advance_tab(q.tab.as_deref()) {
        Ok(t) => t,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"message": e})))),
    };
    let ids = parse_project_ids(q.project_ids.as_deref());
    let today = Utc::now().date_naive();
    let window = analytics_window(q.date_filter.as_deref(), q.start_date.as_deref(), q.end_date.as_deref(), today);
    let (gte, lte) = window.map(|w| (Some(w.gte), Some(w.lte))).unwrap_or((None, None));
    match tab {
        AdvTab::Overview => {
            let counts = [
                count_members(&st.pool, &slug, &ids, None, gte, lte).await?,
                count_members(&st.pool, &slug, &ids, Some(20), gte, lte).await?,
                count_members(&st.pool, &slug, &ids, Some(15), gte, lte).await?,
                count_members(&st.pool, &slug, &ids, Some(5), gte, lte).await?,
                count_projects(&st.pool, &slug, auth.0, &ids, gte, lte).await?,
                count_issues(&st.pool, &slug, auth.0, &ids, None, gte, lte).await?,
                count_cycles(&st.pool, &slug, auth.0, &ids, gte, lte).await?,
                count_intake(&st.pool, &slug, auth.0, &ids, gte, lte).await?,
            ];
            Ok((StatusCode::OK, Json(overview_json(counts))))
        }
        AdvTab::WorkItems => {
            let counts = [
                count_issues(&st.pool, &slug, auth.0, &ids, None, gte, lte).await?,
                count_issues(&st.pool, &slug, auth.0, &ids, Some("started"), gte, lte).await?,
                count_issues(&st.pool, &slug, auth.0, &ids, Some("backlog"), gte, lte).await?,
                count_issues(&st.pool, &slug, auth.0, &ids, Some("unstarted"), gte, lte).await?,
                count_issues(&st.pool, &slug, auth.0, &ids, Some("completed"), gte, lte).await?,
            ];
            Ok((StatusCode::OK, Json(work_items_json(counts))))
        }
    }
}

// ============================================================================
// A2 — advance-analytics-stats (per-project state-group counts).
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdvanceStatsQuery {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub date_filter: Option<String>,
    #[serde(default)]
    pub project_ids: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProjectStatRow {
    project_id: uuid::Uuid,
    project_name: String,
    cancelled_work_items: i64,
    completed_work_items: i64,
    backlog_work_items: i64,
    un_started_work_items: i64,
    started_work_items: i64,
}

fn project_stat_json(r: &ProjectStatRow) -> Value {
    json!({
        "project_id": r.project_id,
        "project__name": r.project_name,
        "cancelled_work_items": r.cancelled_work_items,
        "completed_work_items": r.completed_work_items,
        "backlog_work_items": r.backlog_work_items,
        "un_started_work_items": r.un_started_work_items,
        "started_work_items": r.started_work_items,
    })
}

pub async fn advance_stats(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<AdvanceStatsQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `get` (`advance.py:158-169`): WORKSPACE ADMIN/MEMBER; only
    // `work-items` is valid.
    if !gate_ws_am(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    if resolve_advance_type(q.r#type.as_deref(), "work-items", &["work-items"]).is_err() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"message": INVALID_TYPE_MSG}))));
    }
    // `date_filter` is accepted (Django inits chart filters at `:160`) but
    // the effective helper ignores it — consumed for surface parity.
    let _ = q.date_filter.as_deref();
    let ids = parse_project_ids(q.project_ids.as_deref());
    let member = pred_member("i.project_id", "$2");
    let idf = pred_ids("i.project_id", "$3");
    // Single `GROUP BY` with `FILTER` counts — the `cycle.rs:group_counts`
    // pattern — replacing the 5 sequential annotated counts.
    let sql = format!(
        "SELECT i.project_id, p.name AS project_name, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'cancelled') AS cancelled_work_items, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'completed') AS completed_work_items, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'backlog') AS backlog_work_items, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'unstarted') AS un_started_work_items, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'started') AS started_work_items \
         FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {idf} \
         GROUP BY i.project_id, p.name ORDER BY i.project_id"
    );
    let rows: Vec<ProjectStatRow> =
        sqlx::query_as(&sql).bind(&slug).bind(auth.0).bind(ids.clone()).fetch_all(&st.pool).await?;
    Ok((StatusCode::OK, Json(json!(rows.iter().map(project_stat_json).collect::<Vec<_>>()))))
}

// ============================================================================
// A3 — advance-analytics-charts.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdvanceChartsQuery {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub x_axis: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub date_filter: Option<String>,
    #[serde(default)]
    pub project_ids: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

/// One of the 7 project-chart rows (`advance.py:208-215`).
fn chart_count_row(key: &str, count: i64) -> Value {
    json!({"key": key, "name": title_case(key), "count": count})
}

/// Row aliases keeping the chart SELECTs under the complexity lint.
type SimpleRow = (Option<String>, Option<String>, i64);
type GroupedRow = (Option<String>, Option<String>, Option<String>, Option<String>, i64);
type MonthRow = (NaiveDate, i64, i64);
type DateRangeRow = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);

#[allow(clippy::too_many_arguments)]
async fn count_scoped(
    pool: &sqlx::PgPool,
    table: &str,
    alias: &str,
    created_col: &str,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
    extra: &str,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> Result<i64, sqlx::Error> {
    let member = pred_member(&format!("{alias}.project_id"), "$2");
    let idf = pred_ids(&format!("{alias}.project_id"), "$3");
    let sql = format!(
        "SELECT COUNT(*) FROM {table} {alias} JOIN projects p ON p.id = {alias}.project_id \
         JOIN workspaces w ON w.id = {alias}.workspace_id \
         WHERE w.slug = $1 AND {alias}.deleted_at IS NULL AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {idf} \
         AND ($4::date IS NULL OR {alias}.{created_col}::date >= $4) \
         AND ($5::date IS NULL OR {alias}.{created_col}::date <= $5){extra}"
    );
    sqlx::query_scalar(&sql)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await
}

pub async fn advance_charts(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<AdvanceChartsQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `get` (`advance.py:285-318`): WORKSPACE ADMIN/MEMBER; default
    // type `projects`.
    if !gate_ws_am(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let t = match resolve_advance_type(q.r#type.as_deref(), "projects", &["projects", "custom-work-items", "work-items"]) {
        Ok(t) => t,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"message": e})))),
    };
    let ids = parse_project_ids(q.project_ids.as_deref());
    let today = Utc::now().date_naive();
    // Chart endpoints only read `date_filter` (`get_chart_period_range`
    // has no custom branch); `start_date`/`end_date` are accepted for
    // surface parity and ignored.
    let _ = (q.start_date.as_deref(), q.end_date.as_deref());
    let (start, end) = chart_window(q.date_filter.as_deref(), today).map(|(s, e)| (Some(s), Some(e))).unwrap_or((None, None));
    match t.as_str() {
        "projects" => {
            // `project_chart` (`advance.py:173-215`): 7 `{key,name,count}`
            // rows. Notes: `intake` uses plain `Issue.objects` (no triage
            // exclusion) with `issue_intake__isnull=False`; `members` has NO
            // bot filter; all seven take the `created_at__date` range.
            let work_items = {
                let member = pred_member("i.project_id", "$2");
                let idf = pred_ids("i.project_id", "$3");
                let sql = format!(
                    "SELECT COUNT(*) FROM issues i JOIN projects p ON p.id = i.project_id \
                     JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
                     WHERE w.slug = $1 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
                     AND {member} AND {idf} AND {rng}",
                    rng = pred_chart_range("$4", "$5")
                );
                sqlx::query_scalar::<_, i64>(&sql)
                    .bind(&slug)
                    .bind(auth.0)
                    .bind(ids.clone())
                    .bind(start)
                    .bind(end)
                    .fetch_one(&st.pool)
                    .await?
            };
            let cycles = count_scoped(&st.pool, "cycles", "c", "created_at", &slug, auth.0, &ids, "", start, end).await?;
            let modules = count_scoped(&st.pool, "modules", "m", "created_at", &slug, auth.0, &ids, "", start, end).await?;
            let intake = count_scoped(
                &st.pool,
                "issues",
                "i",
                "created_at",
                &slug,
                auth.0,
                &ids,
                " AND EXISTS(SELECT 1 FROM intake_issues ii WHERE ii.issue_id = i.id AND ii.deleted_at IS NULL)",
                start,
                end,
            )
            .await?;
            let members = {
                let sql =
                    "SELECT COUNT(*) FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id \
                     WHERE w.slug = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
                     AND ($2::date IS NULL OR wm.created_at::date >= $2) \
                     AND ($3::date IS NULL OR wm.created_at::date <= $3)";
                sqlx::query_scalar::<_, i64>(sql).bind(&slug).bind(start).bind(end).fetch_one(&st.pool).await?
            };
            let pages = count_scoped(&st.pool, "project_pages", "pp", "created_at", &slug, auth.0, &ids, "", start, end).await?;
            let views = count_scoped(&st.pool, "issue_views", "iv", "created_at", &slug, auth.0, &ids, "", start, end).await?;
            let rows = [
                ("work_items", work_items),
                ("cycles", cycles),
                ("modules", modules),
                ("intake", intake),
                ("members", members),
                ("pages", pages),
                ("views", views),
            ];
            Ok((StatusCode::OK, Json(json!(rows.iter().map(|(k, c)| chart_count_row(k, *c)).collect::<Vec<_>>()))))
        }
        "custom-work-items" => {
            let x_axis = q.x_axis.clone().unwrap_or_else(|| "PRIORITY".to_string());
            let field = match chart_field(&x_axis) {
                Ok(f) => f,
                Err(e) => return Ok(detail_400(e)),
            };
            let group = match &q.group_by {
                None => None,
                Some(g) => match chart_group_field(g) {
                    Ok(f) => Some(f),
                    Err(e) => return Ok(detail_400(e)),
                },
            };
            let out = custom_chart(&st.pool, &slug, auth.0, &ids, start, end, None, &field, group.as_ref()).await?;
            Ok((StatusCode::OK, Json(out)))
        }
        _ => {
            // `work_item_completion_chart` (`advance.py:217-283`): monthly
            // series from the workspace-creation month through the current
            // month; `count` == created (`:267`).
            let ws_created: Option<DateTime<Utc>> =
                sqlx::query_scalar("SELECT w.created_at FROM workspaces w WHERE w.slug = $1")
                    .bind(&slug)
                    .fetch_optional(&st.pool)
                    .await?;
            let Some(ws_created) = ws_created else {
                return Ok((StatusCode::OK, Json(json!({"data": [], "schema": completion_schema()}))));
            };
            let start_month = ws_created.date_naive().with_day(1).unwrap_or_else(|| ws_created.date_naive());
            let now = Utc::now().date_naive();
            let last_month = now.with_day(1).unwrap_or(now);
            let member = pred_member("i.project_id", "$2");
            let idf = pred_ids("i.project_id", "$3");
            let sql = format!(
                "SELECT date_trunc('month', i.created_at)::date AS m, COUNT(*) AS created_count, \
                 COUNT(*) FILTER (WHERE s.\"group\" = 'completed') AS completed_count \
                 FROM issues i JOIN projects p ON p.id = i.project_id \
                 JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
                 WHERE w.slug = $1 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
                 AND {member} AND {idf} AND {rng} \
                 GROUP BY 1 ORDER BY 1",
                rng = pred_chart_range("$4", "$5")
            );
            let month_rows: Vec<MonthRow> = sqlx::query_as(&sql)
                .bind(&slug)
                .bind(auth.0)
                .bind(ids.clone())
                .bind(start)
                .bind(end)
                .fetch_all(&st.pool)
                .await?;
            let stats: std::collections::BTreeMap<String, (i64, i64)> = month_rows
                .into_iter()
                .map(|(m, c, d)| (month_key(m), (c, d)))
                .collect();
            Ok((
                StatusCode::OK,
                Json(json!({"data": monthly_series(start_month, last_month, &stats), "schema": completion_schema()})),
            ))
        }
    }
}

/// Shared `build_analytics_chart` query (`plane/utils/build_chart.py:
/// 153-194`): `Count("id", distinct=True)`; grouped branch pivots via
/// `process_grouped`, simple branch emits `{key,name,count}` ordered by key.
/// `scope_project` pins a project (A6); `None` = workspace scope (A3).
#[allow(clippy::too_many_arguments)]
async fn custom_chart(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    scope_project: Option<uuid::Uuid>,
    field: &ChartField,
    group: Option<&ChartField>,
) -> Result<Value, sqlx::Error> {
    let member = pred_member("i.project_id", "$2");
    let idf = pred_ids("i.project_id", "$3");
    // `estimate_points.value` is varchar: group by the raw text (tolerant,
    // no `::double precision` cast — same rule as `cycle.rs:parse_point_value`).
    // `$6` is always bound (NULL = workspace scope, no project pin) so the
    // bind count matches the placeholders on every path.
    let scope_f = " AND ($6::uuid IS NULL OR i.project_id = $6)".to_string();
    let base = format!(
        "FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
         {xj} {gj} \
         WHERE w.slug = $1 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {idf} AND {rng}{scope_f}",
        xj = field.join_sql,
        gj = group.map(|g| g.join_sql).unwrap_or(""),
        rng = pred_chart_range("$4", "$5"),
    );
    if let Some(g) = group {
        let sql = format!(
            "SELECT ({xk})::text AS k, ({xn})::text AS n, ({gk})::text AS gk, ({gn})::text AS gn, \
             COUNT(DISTINCT i.id) AS c {base} GROUP BY 1, 2, 3, 4 ORDER BY c DESC",
            xk = field.key_sql,
            xn = field.name_sql,
            gk = g.key_sql,
            gn = g.name_sql,
        );
        let rows: Vec<GroupedRow> =
            sqlx::query_as(&sql)
                .bind(slug)
                .bind(user)
                .bind(ids.clone())
                .bind(start)
                .bind(end)
                .bind(scope_project)
                .fetch_all(pool)
                .await?;
        let inputs: Vec<GroupedInput> = rows
            .into_iter()
            .map(|(k, n, gk, gn, c)| GroupedInput {
                key: k,
                display: n,
                group_key: gk,
                group_name: gn,
                count: c,
                numeric_key: field.numeric_key,
                numeric_group: g.numeric_key,
            })
            .collect();
        let (data, schema) = process_grouped(&inputs);
        Ok(json!({"data": data, "schema": schema}))
    } else {
        // `build_simple_chart_response` orders by key (`build_chart.py:141`);
        // text ordering matches native ordering except for the integer
        // ESTIMATE_POINTS key, which keeps its native order (documented).
        let order = if field.numeric_key { format!("ORDER BY {}", field.key_sql) } else { "ORDER BY 1".to_string() };
        let sql = format!(
            "SELECT ({xk})::text AS k, ({xn})::text AS n, COUNT(DISTINCT i.id) AS c {base} GROUP BY 1, 2 {order}",
            xk = field.key_sql,
            xn = field.name_sql,
        );
        let rows: Vec<SimpleRow> = sqlx::query_as(&sql)
            .bind(slug)
            .bind(user)
            .bind(ids.clone())
            .bind(start)
            .bind(end)
            .bind(scope_project)
            .fetch_all(pool)
            .await?;
        Ok(json!({
            "data": rows.iter().map(|(k, n, c)| simple_chart_row(k.as_deref(), n.as_deref(), *c)).collect::<Vec<_>>(),
            "schema": {},
        }))
    }
}

// ============================================================================
// A4 — project advance-analytics (work-item stats, optional cycle/module).
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectAdvanceQuery {
    #[serde(default)]
    pub cycle_id: Option<String>,
    #[serde(default)]
    pub module_id: Option<String>,
    #[serde(default)]
    pub date_filter: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

fn parse_opt_uuid(raw: Option<&str>) -> Option<uuid::Uuid> {
    raw?.trim().parse::<uuid::Uuid>().ok()
}

/// Counts issues in an explicit id set with the `issue_objects` aliveness
/// predicate (the `id__in=cycle/module_issues` branches,
/// `project_analytics.py:62-72`).
async fn count_issue_ids(
    pool: &sqlx::PgPool,
    ids: &[uuid::Uuid],
    group: Option<&str>,
    gte: Option<DateTime<Utc>>,
    lte: Option<DateTime<Utc>>,
) -> Result<i64, sqlx::Error> {
    let range = pred_analytics_range("i.created_at", "$2", "$3");
    let grp = group.map(|g| format!(" AND s.\"group\" = '{g}'")).unwrap_or_default();
    let sql = format!(
        "SELECT COUNT(*) FROM issues i JOIN states s ON s.id = i.state_id \
         WHERE i.id = ANY($1) AND i.deleted_at IS NULL AND {PRED_ISSUE_OBJECTS} AND {range}{grp}"
    );
    sqlx::query_scalar(&sql).bind(ids).bind(gte).bind(lte).fetch_one(pool).await
}

/// Ids of a cycle's/module's link rows scoped by the analytics base
/// (`CycleIssue.objects.filter(**base_filters, cycle_id=...)`,
/// `project_analytics.py:64-71`).
async fn scoped_link_ids(
    pool: &sqlx::PgPool,
    table: &str,
    link_col: &str,
    link_id: uuid::Uuid,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    let member = pred_member("x.project_id", "$3");
    let idf = pred_ids("x.project_id", "$4");
    let sql = format!(
        "SELECT x.issue_id FROM {table} x JOIN projects p ON p.id = x.project_id \
         JOIN workspaces w ON w.id = x.workspace_id \
         WHERE x.{link_col} = $1 AND x.deleted_at IS NULL AND w.slug = $2 \
         AND {PRED_PROJECT_ALIVE} AND {member} AND {idf}"
    );
    sqlx::query_scalar(&sql)
        .bind(link_id)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .fetch_all(pool)
        .await
}

pub async fn project_advance(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<ProjectAdvanceQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `get` (`project_analytics.py:84-94`): project ADMIN/MEMBER.
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_project_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let today = Utc::now().date_naive();
    let window = analytics_window(q.date_filter.as_deref(), q.start_date.as_deref(), q.end_date.as_deref(), today);
    let (gte, lte) = window.map(|w| (Some(w.gte), Some(w.lte))).unwrap_or((None, None));
    // `get_work_items_stats` (`:58-82`): `cycle_id` wins over `module_id`
    // wins over the project scope. Unknown ids yield empty sets → zero
    // counts, never 404. (Unparseable ids are treated as unknown — sane
    // leniency; Django would 400 on the UUID cast.)
    let ids = parse_project_ids(None);
    let groups = [None, Some("started"), Some("backlog"), Some("unstarted"), Some("completed")];
    if let Some(cid) = parse_opt_uuid(q.cycle_id.as_deref()) {
        let link_ids = scoped_link_ids(&st.pool, "cycle_issues", "cycle_id", cid, &slug, auth.0, &ids).await?;
        let mut counts = [0; 5];
        for (i, g) in groups.iter().enumerate() {
            counts[i] = count_issue_ids(&st.pool, &link_ids, *g, gte, lte).await?;
        }
        return Ok((StatusCode::OK, Json(work_items_json(counts))));
    }
    if let Some(mid) = parse_opt_uuid(q.module_id.as_deref()) {
        let link_ids = scoped_link_ids(&st.pool, "module_issues", "module_id", mid, &slug, auth.0, &ids).await?;
        let mut counts = [0; 5];
        for (i, g) in groups.iter().enumerate() {
            counts[i] = count_issue_ids(&st.pool, &link_ids, *g, gte, lte).await?;
        }
        return Ok((StatusCode::OK, Json(work_items_json(counts))));
    }
    let only = Some(vec![pid]);
    let counts = [
        count_issues(&st.pool, &slug, auth.0, &only, None, gte, lte).await?,
        count_issues(&st.pool, &slug, auth.0, &only, Some("started"), gte, lte).await?,
        count_issues(&st.pool, &slug, auth.0, &only, Some("backlog"), gte, lte).await?,
        count_issues(&st.pool, &slug, auth.0, &only, Some("unstarted"), gte, lte).await?,
        count_issues(&st.pool, &slug, auth.0, &only, Some("completed"), gte, lte).await?,
    ];
    Ok((StatusCode::OK, Json(work_items_json(counts))))
}

// ============================================================================
// A5 — project advance-analytics-stats (per-assignee).
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectAdvanceStatsQuery {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub cycle_id: Option<String>,
    #[serde(default)]
    pub module_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AssigneeStatRow {
    display_name: Option<String>,
    assignee_id: Option<uuid::Uuid>,
    avatar_url: Option<String>,
    cancelled_work_items: i64,
    completed_work_items: i64,
    backlog_work_items: i64,
    un_started_work_items: i64,
    started_work_items: i64,
}

fn assignee_stat_json(r: &AssigneeStatRow) -> Value {
    json!({
        "display_name": r.display_name,
        "assignee_id": r.assignee_id,
        "avatar_url": r.avatar_url,
        "cancelled_work_items": r.cancelled_work_items,
        "completed_work_items": r.completed_work_items,
        "backlog_work_items": r.backlog_work_items,
        "un_started_work_items": r.un_started_work_items,
        "started_work_items": r.started_work_items,
    })
}

pub async fn project_advance_stats(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<ProjectAdvanceStatsQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `get` (`project_analytics.py:165-179`): project ADMIN/MEMBER;
    // only `work-items` is valid.
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_project_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    if resolve_advance_type(q.r#type.as_deref(), "work-items", &["work-items"]).is_err() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"message": INVALID_TYPE_MSG}))));
    }
    // `get_work_items_stats` (`:119-163`): cycle/module id__in scoping, else
    // the project scope. `avatar_url` CASE (`:137-153`): the linked avatar
    // asset wins (`/api/assets/v2/static/<id>/`), else the legacy `avatar`
    // text — same expression as the module assignee-stats twin
    // (`module.rs` avatar CASE). DISTINCT counts, ordered by display_name.
    // (Issues without assignees form the NULL row via the LEFT JOINs —
    // Django's `F("assignees__...")` outer join, mirrored literally.)
    let scope = if let Some(cid) = parse_opt_uuid(q.cycle_id.as_deref()) {
        let link_ids = scoped_link_ids(&st.pool, "cycle_issues", "cycle_id", cid, &slug, auth.0, &parse_project_ids(None)).await?;
        format!("AND i.id = ANY('{{{}}}'::uuid[])", link_ids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(","))
    } else if let Some(mid) = parse_opt_uuid(q.module_id.as_deref()) {
        let link_ids = scoped_link_ids(&st.pool, "module_issues", "module_id", mid, &slug, auth.0, &parse_project_ids(None)).await?;
        format!("AND i.id = ANY('{{{}}}'::uuid[])", link_ids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(","))
    } else {
        format!("AND i.project_id = '{pid}'")
    };
    let member = pred_member("i.project_id", "$2");
    let sql = format!(
        "SELECT u.display_name AS display_name, u.id AS assignee_id, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
              THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' \
              ELSE u.avatar END AS avatar_url, \
         COUNT(DISTINCT i.id) FILTER (WHERE s.\"group\" = 'cancelled') AS cancelled_work_items, \
         COUNT(DISTINCT i.id) FILTER (WHERE s.\"group\" = 'completed') AS completed_work_items, \
         COUNT(DISTINCT i.id) FILTER (WHERE s.\"group\" = 'backlog') AS backlog_work_items, \
         COUNT(DISTINCT i.id) FILTER (WHERE s.\"group\" = 'unstarted') AS un_started_work_items, \
         COUNT(DISTINCT i.id) FILTER (WHERE s.\"group\" = 'started') AS started_work_items \
         FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
         LEFT JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.deleted_at IS NULL \
         LEFT JOIN users u ON u.id = ia.assignee_id \
         WHERE w.slug = $1 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
         AND {member} {scope} \
         GROUP BY u.display_name, u.id, u.avatar_asset_id, u.avatar ORDER BY u.display_name"
    );
    let rows: Vec<AssigneeStatRow> = sqlx::query_as(&sql).bind(&slug).bind(auth.0).fetch_all(&st.pool).await?;
    Ok((StatusCode::OK, Json(json!(rows.iter().map(assignee_stat_json).collect::<Vec<_>>()))))
}

// ============================================================================
// A6 — project advance-analytics-charts.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectAdvanceChartsQuery {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub x_axis: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub cycle_id: Option<String>,
    #[serde(default)]
    pub module_id: Option<String>,
    #[serde(default)]
    pub date_filter: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

pub async fn project_advance_charts(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<ProjectAdvanceChartsQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `get` (`project_analytics.py:317-367`): project ADMIN/MEMBER/
    // GUEST. NOTE the default `type="projects"` is NOT valid here (`:320`)
    // → a bare call 400s, unlike A3. Mirrored literally.
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_project_amg(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let t = match resolve_advance_type(q.r#type.as_deref(), "projects", &["custom-work-items", "work-items"]) {
        Ok(t) => t,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"message": e})))),
    };
    let today = Utc::now().date_naive();
    // Same chart-type rule as A3: only `date_filter` applies.
    let _ = (q.start_date.as_deref(), q.end_date.as_deref());
    let (start, end) = chart_window(q.date_filter.as_deref(), today).map(|(s, e)| (Some(s), Some(e))).unwrap_or((None, None));
    if t == "custom-work-items" {
        // `:326-355`: project scope + optional cycle/module id__in + chart
        // range → `build_analytics_chart`.
        let x_axis = q.x_axis.clone().unwrap_or_else(|| "PRIORITY".to_string());
        let field = match chart_field(&x_axis) {
            Ok(f) => f,
            Err(e) => return Ok(detail_400(e)),
        };
        let group = match &q.group_by {
            None => None,
            Some(g) => match chart_group_field(g) {
                Ok(f) => Some(f),
                Err(e) => return Ok(detail_400(e)),
            },
        };
        let mut ids = parse_project_ids(None);
        if let Some(cid) = parse_opt_uuid(q.cycle_id.as_deref()) {
            ids = Some(scoped_link_ids(&st.pool, "cycle_issues", "cycle_id", cid, &slug, auth.0, &ids).await?);
        } else if let Some(mid) = parse_opt_uuid(q.module_id.as_deref()) {
            ids = Some(scoped_link_ids(&st.pool, "module_issues", "module_id", mid, &slug, auth.0, &ids).await?);
        }
        // Feed the id__in set through the shared custom-chart path: stash
        // the scoped ids as the ONLY allowed projects? No — scope by issue
        // ids instead: reuse `custom_chart` with a project pin plus an
        // issue-id prefilter. The prefilter needs its own predicate, so
        // build it here via a dedicated query below.
        let out = project_custom_chart(&st.pool, &slug, auth.0, pid, ids, start, end, &field, group.as_ref()).await?;
        return Ok((StatusCode::OK, Json(out)));
    }
    // `work-items` → `work_item_completion_chart` (`:183-315`).
    let cid = parse_opt_uuid(q.cycle_id.as_deref());
    let mid = parse_opt_uuid(q.module_id.as_deref());
    if let Some(cid) = cid {
        // Daily series over the CYCLE's link rows (`:192-202,223-258`):
        // the cycle lookup is unscoped (`.filter(id=...).first()`); a
        // missing cycle or a missing start date → empty data (`:201`).
        // (A missing end date would crash Django with `None <= date`;
        // normalized to empty + documented.)
        let cycle: Option<DateRangeRow> =
            sqlx::query_as("SELECT start_date, end_date FROM cycles WHERE id = $1 AND deleted_at IS NULL")
                .bind(cid)
                .fetch_optional(&st.pool)
                .await?;
        let (s, e) = cycle.unwrap_or((None, None));
        let (Some(s), Some(e)) = (s, e) else {
            return Ok((StatusCode::OK, Json(json!({"data": [], "schema": {}}))));
        };
        let stats = daily_link_stats(&st.pool, "cycle_issues", "cycle_id", cid, &slug, auth.0, &parse_project_ids(None)).await?;
        return Ok((
            StatusCode::OK,
            Json(json!({"data": daily_series(s.date_naive(), e.date_naive(), &stats), "schema": completion_schema()})),
        ));
    }
    if let Some(mid) = mid {
        // Daily series over the MODULE's link rows (`:204-214,223-258`).
        let module: Option<(Option<NaiveDate>, Option<NaiveDate>)> =
            sqlx::query_as("SELECT start_date, target_date FROM modules WHERE id = $1 AND deleted_at IS NULL")
                .bind(mid)
                .fetch_optional(&st.pool)
                .await?;
        let Some((Some(s), Some(e))) = module else {
            return Ok((StatusCode::OK, Json(json!({"data": [], "schema": {}}))));
        };
        let stats = daily_link_stats(&st.pool, "module_issues", "module_id", mid, &slug, auth.0, &parse_project_ids(None)).await?;
        return Ok((
            StatusCode::OK,
            Json(json!({"data": daily_series(s, e, &stats), "schema": completion_schema()})),
        ));
    }
    // Monthly series from the project-creation month (`:216-221,259-308`).
    let proj_created: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT p.created_at FROM projects p WHERE p.id = $1")
        .bind(pid)
        .fetch_optional(&st.pool)
        .await?;
    let Some(proj_created) = proj_created else {
        return Ok((StatusCode::OK, Json(json!({"data": [], "schema": completion_schema()}))));
    };
    let start_month = proj_created.date_naive().with_day(1).unwrap_or_else(|| proj_created.date_naive());
    let last_month = today.with_day(1).unwrap_or(today);
    let member = pred_member("i.project_id", "$2");
    let sql = format!(
        "SELECT date_trunc('month', i.created_at)::date AS m, COUNT(*) AS created_count, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'completed') AS completed_count \
         FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
         WHERE w.slug = $1 AND i.project_id = $3 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {rng} \
         GROUP BY 1 ORDER BY 1",
        rng = pred_chart_range("$4", "$5")
    );
    let month_rows: Vec<MonthRow> = sqlx::query_as(&sql)
        .bind(&slug)
        .bind(auth.0)
        .bind(pid)
        .bind(start)
        .bind(end)
        .fetch_all(&st.pool)
        .await?;
    let stats: std::collections::BTreeMap<String, (i64, i64)> =
        month_rows.into_iter().map(|(m, c, d)| (month_key(m), (c, d))).collect();
    Ok((
        StatusCode::OK,
        Json(json!({"data": monthly_series(start_month, last_month, &stats), "schema": completion_schema()})),
    ))
}

/// Project-scoped custom chart with an optional issue-id prefilter (the A6
/// cycle/module `id__in` branches, `project_analytics.py:335-345`).
#[allow(clippy::too_many_arguments)]
async fn project_custom_chart(
    pool: &sqlx::PgPool,
    slug: &str,
    user: uuid::Uuid,
    pid: uuid::Uuid,
    prefilter: Option<Vec<uuid::Uuid>>,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    field: &ChartField,
    group: Option<&ChartField>,
) -> Result<Value, sqlx::Error> {
    let member = pred_member("i.project_id", "$2");
    // `$6` always bound (NULL = no prefilter) so binds match placeholders.
    let pre = " AND ($6::uuid[] IS NULL OR i.id = ANY($6))".to_string();
    let base = format!(
        "FROM issues i JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id JOIN states s ON s.id = i.state_id \
         {xj} {gj} \
         WHERE w.slug = $1 AND i.project_id = $3 AND {PRED_ISSUE_OBJECTS} AND {PRED_PROJECT_ALIVE} \
         AND {member} AND {rng}{pre}",
        xj = field.join_sql,
        gj = group.map(|g| g.join_sql).unwrap_or(""),
        rng = pred_chart_range("$4", "$5"),
    );
    if let Some(g) = group {
        let sql = format!(
            "SELECT ({xk})::text AS k, ({xn})::text AS n, ({gk})::text AS gk, ({gn})::text AS gn, \
             COUNT(DISTINCT i.id) AS c {base} GROUP BY 1, 2, 3, 4 ORDER BY c DESC",
            xk = field.key_sql,
            xn = field.name_sql,
            gk = g.key_sql,
            gn = g.name_sql,
        );
        let rows: Vec<GroupedRow> =
            sqlx::query_as(&sql).bind(slug).bind(user).bind(pid).bind(start).bind(end).bind(prefilter).fetch_all(pool).await?;
        let inputs: Vec<GroupedInput> = rows
            .into_iter()
            .map(|(k, n, gk, gn, c)| GroupedInput {
                key: k,
                display: n,
                group_key: gk,
                group_name: gn,
                count: c,
                numeric_key: field.numeric_key,
                numeric_group: g.numeric_key,
            })
            .collect();
        let (data, schema) = process_grouped(&inputs);
        Ok(json!({"data": data, "schema": schema}))
    } else {
        let order = if field.numeric_key { format!("ORDER BY {}", field.key_sql) } else { "ORDER BY 1".to_string() };
        let sql = format!(
            "SELECT ({xk})::text AS k, ({xn})::text AS n, COUNT(DISTINCT i.id) AS c {base} GROUP BY 1, 2 {order}",
            xk = field.key_sql,
            xn = field.name_sql,
        );
        let rows: Vec<SimpleRow> = sqlx::query_as(&sql)
            .bind(slug)
            .bind(user)
            .bind(pid)
            .bind(start)
            .bind(end)
            .bind(prefilter)
            .fetch_all(pool)
            .await?;
        Ok(json!({
            "data": rows.iter().map(|(k, n, c)| simple_chart_row(k.as_deref(), n.as_deref(), *c)).collect::<Vec<_>>(),
            "schema": {},
        }))
    }
}

/// Daily link-row stats for the A6 cycle/module branches
/// (`project_analytics.py:224-232`): groups the LINK rows (cycle_issues/
/// module_issues) by their own `created_at::date`; completions join through
/// the issue to its state.
async fn daily_link_stats(
    pool: &sqlx::PgPool,
    table: &str,
    link_col: &str,
    link_id: uuid::Uuid,
    slug: &str,
    user: uuid::Uuid,
    ids: &Option<Vec<uuid::Uuid>>,
) -> Result<std::collections::BTreeMap<String, (i64, i64)>, sqlx::Error> {
    let member = pred_member("x.project_id", "$3");
    let idf = pred_ids("x.project_id", "$4");
    let sql = format!(
        "SELECT x.created_at::date AS d, COUNT(*) AS created_count, \
         COUNT(*) FILTER (WHERE s.\"group\" = 'completed') AS completed_count \
         FROM {table} x JOIN projects p ON p.id = x.project_id \
         JOIN workspaces w ON w.id = x.workspace_id \
         LEFT JOIN issues i ON i.id = x.issue_id AND i.deleted_at IS NULL \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE x.{link_col} = $1 AND x.deleted_at IS NULL AND w.slug = $2 \
         AND {PRED_PROJECT_ALIVE} AND {member} AND {idf} \
         GROUP BY 1 ORDER BY 1"
    );
    let rows: Vec<MonthRow> = sqlx::query_as(&sql)
        .bind(link_id)
        .bind(slug)
        .bind(user)
        .bind(ids.clone())
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(d, c, k)| (d.to_string(), (c, k))).collect())
}

// ============================================================================
// E10b — deploy boards.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct DeployRow {
    id: uuid::Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
    entity_identifier: Option<uuid::Uuid>,
    entity_name: Option<String>,
    anchor: String,
    is_comments_enabled: bool,
    is_reactions_enabled: bool,
    is_votes_enabled: bool,
    intake_id: Option<uuid::Uuid>,
    view_props: Value,
    is_activity_enabled: bool,
    is_disabled: bool,
}

/// `DeployBoardSerializer` (`serializers/project.py:247-254`):
/// `fields = "__all__"` with read-only `workspace/project/anchor`, plus the
/// nested `project_details`/`workspace_detail` (omitted here — two extra
/// queries for FE-optional data; documented deviation). FKs render as ids.
fn deploy_json(r: &DeployRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
        "workspace": r.workspace_id,
        "project": r.project_id,
        "entity_identifier": r.entity_identifier,
        "entity_name": r.entity_name,
        "anchor": r.anchor,
        "is_comments_enabled": r.is_comments_enabled,
        "is_reactions_enabled": r.is_reactions_enabled,
        "is_votes_enabled": r.is_votes_enabled,
        "intake": r.intake_id,
        "view_props": r.view_props,
        "is_activity_enabled": r.is_activity_enabled,
        "is_disabled": r.is_disabled,
    })
}

const DEPLOY_COLS: &str = "id, created_at, updated_at, created_by_id, updated_by_id, workspace_id, \
    project_id, entity_identifier, entity_name, anchor, is_comments_enabled, \
    is_reactions_enabled, is_votes_enabled, intake_id, view_props, is_activity_enabled, is_disabled";

/// Scoped lookup: the project's board in this workspace (`list` filters
/// `entity_name="project", entity_identifier=project_id, workspace__slug`,
/// `views/project/base.py:541-543`; retrieve/patch/delete tighten
/// Django's unscoped pk lookup to the same scope — documented).
async fn fetch_deploy(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    slug: &str,
    pk: Option<uuid::Uuid>,
) -> Result<Option<DeployRow>, sqlx::Error> {
    let pk_f = pk.map(|_| " AND d.id = $3".to_string()).unwrap_or_default();
    let sql = format!(
        "SELECT {DEPLOY_COLS} FROM deploy_boards d \
         JOIN workspaces w ON w.id = d.workspace_id \
         WHERE d.entity_name = 'project' AND d.entity_identifier = $1 AND w.slug = $2 \
         AND d.deleted_at IS NULL{pk_f} ORDER BY d.created_at DESC LIMIT 1"
    );
    // NOTE: `SELECT id, ...` against `deploy_boards d` needs the alias.
    let sql = sql.replacen("SELECT id,", "SELECT d.id,", 1);
    let mut q = sqlx::query_as::<_, DeployRow>(&sql).bind(pid).bind(slug);
    if let Some(pk) = pk {
        q = q.bind(pk);
    }
    q.fetch_optional(pool).await
}

pub async fn deploy_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_deploy_read(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `list` (`base.py:540-546`): `.first()` may be None → `serializer.data`
    // is `null` — Django returns 200 `null`, preserved here (NOT 404).
    match fetch_deploy(&st.pool, pid, &slug, None).await? {
        Some(row) => Ok((StatusCode::OK, Json(deploy_json(&row)))),
        None => Ok((StatusCode::OK, Json(Value::Null))),
    }
}

pub async fn deploy_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // `ProjectMemberPermission` POST branch (`permissions/project.py:70-76`):
    // workspace ADMIN/MEMBER.
    if !gate_deploy_post(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    // `create` (`base.py:548-576`): field defaults + `get_or_create(
    // entity_name="project", entity_identifier=project_id)` + **200**.
    let (comments, reactions, votes) = deploy_flags(&body);
    let views = deploy_views_from(&body);
    let intake_id = match parse_intake_id(&body) {
        Ok(v) => v,
        Err(e) => return Ok(detail_400(e)),
    };
    if let Some(iid) = intake_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM intakes WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(iid)
        .fetch_one(&st.pool)
        .await?;
        if !exists {
            return Ok(detail_400("Invalid intake.".to_string()));
        }
    }
    // Anchor mirrors `get_anchor` (`db/models/deploy_board.py:15-16`):
    // `uuid4().hex` (32 hex chars, no dashes).
    let mut tx = st.pool.begin().await?;
    let existing: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT d.id FROM deploy_boards d JOIN workspaces w ON w.id = d.workspace_id \
         WHERE d.entity_name = 'project' AND d.entity_identifier = $1 AND w.slug = $2 \
         AND d.deleted_at IS NULL",
    )
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((did,)) = existing {
        sqlx::query(
            "UPDATE deploy_boards SET is_comments_enabled = $1, is_reactions_enabled = $2, \
             is_votes_enabled = $3, intake_id = $4, view_props = $5, updated_by_id = $6, \
             updated_at = now() WHERE id = $7",
        )
        .bind(comments)
        .bind(reactions)
        .bind(votes)
        .bind(intake_id)
        .bind(&views)
        .bind(auth.0)
        .bind(did)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO deploy_boards (id, entity_name, entity_identifier, project_id, workspace_id, \
             anchor, is_comments_enabled, is_reactions_enabled, is_votes_enabled, intake_id, \
             view_props, is_activity_enabled, is_disabled, created_by_id, updated_by_id, \
             created_at, updated_at) \
             SELECT gen_random_uuid(), 'project', $1, $1, p.workspace_id, \
             replace(gen_random_uuid()::text, '-', ''), $2, $3, $4, $5, $6, true, false, $7, $7, \
             now(), now() FROM projects p WHERE p.id = $1",
        )
        .bind(pid)
        .bind(comments)
        .bind(reactions)
        .bind(votes)
        .bind(intake_id)
        .bind(&views)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    match fetch_deploy(&st.pool, pid, &slug, None).await? {
        Some(row) => Ok((StatusCode::OK, Json(deploy_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn deploy_retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_deploy_read(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    match fetch_deploy(&st.pool, pid, &slug, Some(pk)).await? {
        Some(row) => Ok((StatusCode::OK, Json(deploy_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn deploy_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_deploy_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    let Some(cur) = fetch_deploy(&st.pool, pid, &slug, Some(pk)).await? else {
        return Ok(missing());
    };
    // `DeployBoardSerializer` (`serializers/project.py:247-254`):
    // `__all__` minus read-only `workspace/project/anchor` (plus the
    // auto `id/created_at/updated_at`). `intake` must exist when set.
    let intake_id = if body.get("intake").is_some() {
        match parse_intake_id(&body) {
            Ok(v) => v,
            Err(e) => return Ok(detail_400(e)),
        }
    } else {
        cur.intake_id
    };
    if let Some(iid) = intake_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM intakes WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(iid)
        .fetch_one(&st.pool)
        .await?;
        if !exists {
            return Ok(detail_400("Invalid intake.".to_string()));
        }
    }
    let opt_uuid = |k: &str| -> Result<Option<Option<uuid::Uuid>>, String> {
        match body.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => s
                .trim()
                .parse::<uuid::Uuid>()
                .map(|u| Some(Some(u)))
                .map_err(|_| "Please provide valid detail".to_string()),
            Some(_) => Err("Please provide valid detail".to_string()),
        }
    };
    let entity_name = match body.get("entity_name").and_then(Value::as_str) {
        Some(s) => Some(s.to_string()),
        None => cur.entity_name.clone(),
    };
    let entity_identifier = match opt_uuid("entity_identifier") {
        Ok(None) => cur.entity_identifier,
        Ok(Some(v)) => v,
        Err(e) => return Ok(detail_400(e)),
    };
    let flag = |k: &str, cur_v: bool| body.get(k).and_then(Value::as_bool).unwrap_or(cur_v);
    let views = deploy_views_patch(&body).unwrap_or_else(|| cur.view_props.clone());
    sqlx::query(
        "UPDATE deploy_boards SET entity_name = $1, entity_identifier = $2, \
         is_comments_enabled = $3, is_reactions_enabled = $4, is_votes_enabled = $5, \
         is_activity_enabled = $6, is_disabled = $7, intake_id = $8, view_props = $9, \
         updated_by_id = $10, updated_at = now() WHERE id = $11",
    )
    .bind(entity_name)
    .bind(entity_identifier)
    .bind(flag("is_comments_enabled", cur.is_comments_enabled))
    .bind(flag("is_reactions_enabled", cur.is_reactions_enabled))
    .bind(flag("is_votes_enabled", cur.is_votes_enabled))
    .bind(flag("is_activity_enabled", cur.is_activity_enabled))
    .bind(flag("is_disabled", cur.is_disabled))
    .bind(intake_id)
    .bind(&views)
    .bind(auth.0)
    .bind(cur.id)
    .execute(&st.pool)
    .await?;
    match fetch_deploy(&st.pool, pid, &slug, Some(pk)).await? {
        Some(row) => Ok((StatusCode::OK, Json(deploy_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn deploy_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_deploy_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // Default `ModelViewSet.destroy` is a soft delete here (the model uses
    // the soft-deletion manager) → 204.
    let res = sqlx::query(
        "UPDATE deploy_boards d SET deleted_at = now() FROM workspaces w \
         WHERE d.id = $1 AND d.workspace_id = w.id AND w.slug = $2 \
         AND d.entity_name = 'project' AND d.entity_identifier = $3 AND d.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// Unit tests (pure surface — no DB).
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_defaults_to_overview() {
        assert_eq!(resolve_advance_tab(None), Ok(AdvTab::Overview));
        assert_eq!(resolve_advance_tab(Some("overview")), Ok(AdvTab::Overview));
        assert_eq!(resolve_advance_tab(Some("work-items")), Ok(AdvTab::WorkItems));
    }

    #[test]
    fn tab_invalid_400_const() {
        assert_eq!(resolve_advance_tab(Some("bogus")), Err(INVALID_TAB_MSG.to_string()));
        assert_eq!(INVALID_TAB_MSG, "Invalid tab");
    }

    #[test]
    fn type_invalid_400_const() {
        assert_eq!(
            resolve_advance_type(Some("bogus"), "work-items", &["work-items"]),
            Err(INVALID_TYPE_MSG.to_string())
        );
        assert_eq!(INVALID_TYPE_MSG, "Invalid type");
        // A2/A5 default.
        assert_eq!(
            resolve_advance_type(None, "work-items", &["work-items"]),
            Ok("work-items".to_string())
        );
        // A3 default accepts projects.
        assert_eq!(
            resolve_advance_type(None, "projects", &["projects", "custom-work-items", "work-items"]),
            Ok("projects".to_string())
        );
        // A6 default is "projects" which is NOT allowed → Invalid type.
        assert!(resolve_advance_type(None, "projects", &["custom-work-items", "work-items"]).is_err());
    }

    #[test]
    fn overview_key_set() {
        let v = overview_json([1, 2, 3, 4, 5, 6, 7, 8]);
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 8);
        for k in OVERVIEW_KEYS {
            assert_eq!(obj[k], json!({"count": match k {
                "total_users" => 1,
                "total_admins" => 2,
                "total_members" => 3,
                "total_guests" => 4,
                "total_projects" => 5,
                "total_work_items" => 6,
                "total_cycles" => 7,
                _ => 8,
            }}));
        }
    }

    #[test]
    fn work_items_key_set() {
        let v = work_items_json([10, 1, 2, 3, 4]);
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 5);
        assert_eq!(obj["total_work_items"], json!({"count": 10}));
        assert_eq!(obj["started_work_items"], json!({"count": 1}));
        assert_eq!(obj["backlog_work_items"], json!({"count": 2}));
        assert_eq!(obj["un_started_work_items"], json!({"count": 3}));
        assert_eq!(obj["completed_work_items"], json!({"count": 4}));
    }

    #[test]
    fn title_case_chart_names() {
        assert_eq!(title_case("work_items"), "Work Items");
        assert_eq!(title_case("cycles"), "Cycles");
        assert_eq!(title_case("views"), "Views");
    }

    #[test]
    fn analytics_windows() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 6).unwrap();
        assert!(analytics_window(None, None, None, today).is_none());
        assert!(analytics_window(Some("bogus"), None, None, today).is_none());
        let w = analytics_window(Some("yesterday"), None, None, today).unwrap();
        assert_eq!(w.gte.to_string(), "2026-09-05 00:00:00 UTC");
        assert!(w.lte.to_string().starts_with("2026-09-05 23:59:59"));
        let w = analytics_window(Some("last_7_days"), None, None, today).unwrap();
        assert_eq!(w.gte.date_naive(), NaiveDate::from_ymd_opt(2026, 8, 30).unwrap());
        assert_eq!(w.lte.date_naive(), today);
        let w = analytics_window(Some("last_30_days"), None, None, today).unwrap();
        assert_eq!(w.gte.date_naive(), NaiveDate::from_ymd_opt(2026, 8, 7).unwrap());
        let w = analytics_window(Some("last_3_months"), None, None, today).unwrap();
        assert_eq!(w.gte.date_naive(), NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        let w = analytics_window(Some("custom"), Some("2026-01-01"), Some("2026-01-31"), today).unwrap();
        assert_eq!(w.gte.date_naive(), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(w.lte.date_naive(), NaiveDate::from_ymd_opt(2026, 1, 31).unwrap());
        // custom without both dates → None (date_utils.py:75-87).
        assert!(analytics_window(Some("custom"), Some("2026-01-01"), None, today).is_none());
        assert!(analytics_window(Some("custom"), Some("nope"), Some("2026-01-31"), today).is_none());
    }

    #[test]
    fn chart_windows() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 6).unwrap();
        assert!(chart_window(None, today).is_none());
        assert!(chart_window(Some("bogus"), today).is_none());
        // No custom branch in get_chart_period_range (date_utils.py:90-122).
        assert!(chart_window(Some("custom"), today).is_none());
        assert_eq!(
            chart_window(Some("yesterday"), today),
            Some((NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(), NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()))
        );
        assert_eq!(
            chart_window(Some("last_7_days"), today),
            Some((NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(), today))
        );
    }

    #[test]
    fn chart_mapper_all_axes() {
        for axis in CHART_X_AXES {
            assert!(chart_field(axis).is_ok(), "{axis}");
            assert!(chart_group_field(axis).is_ok(), "{axis}");
        }
        assert_eq!(chart_field("PRIORITY").unwrap().key_sql, "i.priority");
        assert!(chart_field("ESTIMATE_POINTS").unwrap().numeric_key);
        assert!(!chart_field("PRIORITY").unwrap().numeric_key);
        assert!(chart_field("LABELS").unwrap().join_sql.contains("issue_labels"));
        assert!(chart_field("ASSIGNEES").unwrap().join_sql.contains("issue_assignees"));
    }

    #[test]
    fn chart_mapper_invalid_detail_400() {
        let err = chart_field("NOPE").unwrap_err();
        assert_eq!(err, "Invalid x_axis field: NOPE");
        let gerr = chart_group_field("NOPE").unwrap_err();
        assert_eq!(gerr, "Invalid group_by field: NOPE");
        let (status, body) = detail_400(err);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.get("detail").and_then(Value::as_str).unwrap().contains("Invalid x_axis"));
    }

    #[test]
    fn simple_row_none_mapping() {
        // build_simple_chart_response:143-150 — None → "None".
        let r = simple_chart_row(None, None, 3);
        assert_eq!(r, json!({"key": "None", "name": "None", "count": 3}));
        let r = simple_chart_row(Some("high"), Some("high"), 1);
        assert_eq!(r["key"], json!("high"));
    }

    #[test]
    fn grouped_pivot_literal() {
        // process_grouped_data (build_chart.py:78-98).
        let rows = vec![
            GroupedInput {
                key: Some("high".to_string()),
                display: Some("High".to_string()),
                group_key: Some("g1".to_string()),
                group_name: Some("G One".to_string()),
                count: 2,
                numeric_key: false,
                numeric_group: false,
            },
            GroupedInput {
                key: Some("high".to_string()),
                display: Some("High".to_string()),
                group_key: None,
                group_name: None,
                count: 1,
                numeric_key: false,
                numeric_group: false,
            },
            GroupedInput {
                key: None,
                display: None,
                group_key: Some("g1".to_string()),
                group_name: None,
                count: 4,
                numeric_key: false,
                numeric_group: false,
            },
        ];
        let (data, schema) = process_grouped(&rows);
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["key"], json!("high"));
        assert_eq!(data[0]["name"], json!("High"));
        assert_eq!(data[0]["count"], json!(3));
        assert_eq!(data[0]["g1"], json!(2));
        assert_eq!(data[0]["none"], json!(1));
        // Falsy key → "none" bucket, name falls back to "None".
        assert_eq!(data[1]["key"], json!("none"));
        assert_eq!(data[1]["name"], json!("None"));
        // The later NULL-name row OVERWRITES schema["g1"] (build_chart.py:
        // 93-94 assigns on every row) — mirrored literally.
        assert_eq!(schema["g1"], json!("None"));
        assert_eq!(schema["none"], json!("None"));
    }

    #[test]
    fn grouped_numeric_zero_is_falsy() {
        // Python `if key` on int 0 → "none" (ESTIMATE_POINTS axis).
        let rows = vec![GroupedInput {
            key: Some("0".to_string()),
            display: Some("0".to_string()),
            group_key: Some("0".to_string()),
            group_name: Some("0".to_string()),
            count: 5,
            numeric_key: true,
            numeric_group: true,
        }];
        let (data, schema) = process_grouped(&rows);
        assert_eq!(data[0]["key"], json!("none"));
        assert_eq!(data[0]["none"], json!(5));
        assert_eq!(schema["none"], json!("0"));
    }

    #[test]
    fn next_month_rolls_year() {
        let d = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(next_month(d), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        let d = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(next_month(d), NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
    }

    #[test]
    fn monthly_series_zero_fills() {
        let stats: std::collections::BTreeMap<String, (i64, i64)> =
            [("2026-08-01".to_string(), (2, 1))].into_iter().collect();
        let data = monthly_series(
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            &stats,
        );
        assert_eq!(data.len(), 2);
        // count == created (advance.py:267), not the sum.
        assert_eq!(data[0]["count"], json!(2));
        assert_eq!(data[0]["completed_issues"], json!(1));
        assert_eq!(data[0]["created_issues"], json!(2));
        assert_eq!(data[1]["count"], json!(0));
    }

    #[test]
    fn daily_series_sums_and_caps() {
        let stats: std::collections::BTreeMap<String, (i64, i64)> =
            [("2026-09-01".to_string(), (2, 1))].into_iter().collect();
        let data = daily_series(
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),
            &stats,
        );
        assert_eq!(data.len(), 2);
        // count == created + completed (project_analytics.py:253).
        assert_eq!(data[0]["count"], json!(3));
        assert_eq!(data[1]["count"], json!(0));
        // 732-day cap (cycle.rs burndown precedent).
        let data = daily_series(
            NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            &std::collections::BTreeMap::new(),
        );
        assert_eq!(data.len(), (MAX_SERIES_DAYS + 1) as usize);
    }

    #[test]
    fn deploy_upsert_defaults() {
        // base.py:549-562 — flags default false, views default all-true.
        let body = json!({});
        assert_eq!(deploy_flags(&body), (false, false, false));
        assert_eq!(deploy_views_from(&body), deploy_default_views());
        let body = json!({"is_comments_enabled": true, "views": {"list": false}});
        assert_eq!(deploy_flags(&body), (true, false, false));
        assert_eq!(deploy_views_from(&body), json!({"list": false}));
        // intake missing/null → None; garbage → 400 detail.
        assert_eq!(parse_intake_id(&json!({})), Ok(None));
        assert_eq!(parse_intake_id(&json!({"intake": null})), Ok(None));
        assert!(parse_intake_id(&json!({"intake": "nope"})).is_err());
        let id = uuid::Uuid::new_v4();
        assert_eq!(parse_intake_id(&json!({"intake": id})), Ok(Some(id)));
        // PATCH prefers view_props, accepts views alias.
        assert_eq!(
            deploy_views_patch(&json!({"view_props": {"list": true}})),
            Some(json!({"list": true}))
        );
        assert_eq!(deploy_views_patch(&json!({"views": {"list": true}})), Some(json!({"list": true})));
        assert_eq!(deploy_views_patch(&json!({})), None);
    }

    #[test]
    fn guards() {
        // WS AM: 20/15 pass, 5/None deny (advance.py:104,158,285).
        assert!(guard_ws_am(Some(20)).is_ok());
        assert!(guard_ws_am(Some(15)).is_ok());
        assert!(guard_ws_am(Some(5)).is_err());
        assert!(guard_ws_am(None).is_err());
        // Deploy SAFE read: any membership (permissions/project.py:61-69).
        assert!(guard_deploy_read(true).is_ok());
        assert_eq!(guard_deploy_read(false).unwrap_err(), PERMISSION_DETAIL_MSG);
        // Deploy write: project 20/15 only, strict.
        assert!(guard_deploy_write(Some(20)).is_ok());
        assert!(guard_deploy_write(Some(15)).is_ok());
        assert!(guard_deploy_write(Some(5)).is_err());
        assert!(guard_deploy_write(None).is_err());
    }
}
