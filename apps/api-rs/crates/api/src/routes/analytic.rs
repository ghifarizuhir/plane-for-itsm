use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Read-only aggregates for `plane/app/urls/analytic.py`:
/// - `GET workspaces/:slug/default-analytics/` (DefaultAnalyticsEndpoint):
///   totals, state-group classification, month-wise completions (current
///   year), top creators/closers/pending assignees, estimate sums.
/// - `GET workspaces/:slug/project-stats/?fields=&project_ids=`
///   (ProjectStatsEndpoint): per-project total/completed issues, members
///   (non-bot, active), cycles, modules.
/// - `GET/POST workspaces/:slug/analytic-view/` (AnalyticViewViewset
///   list/create): saved views; `name` required/255, `query` required.
///
/// STAYS ON DJANGO: `analytics/`, `advance-analytics*` (custom
/// `build_graph_plot` histogram builder), `export-analytics/`,
/// `saved-analytic-view/` (runs the plot builder) — a dedicated task.
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
