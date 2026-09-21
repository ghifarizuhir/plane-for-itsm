//! v1 work-item handlers (`/api/v1/.../work-items/...`).

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};

use crate::routes::issue_archive_one::guard_archive_one_group;
use crate::routes::issue_common::{
    fetch_guest_scoped, fetch_project_member_role, is_workspace_admin, page_window, parse_date,
    project_gate_allows, replace_bridges, require_project_write, IssueDetailRow, IssueListRow,
    PageWindow,
};
use crate::routes::issue_query::{DETAIL_SELECT_SQL, LIST_SELECT_SQL, build_ungrouped_envelope};
use crate::routes::issue_write::resolve_effective_state;
use crate::routes::project::{deny, missing};
use crate::routes::v1::common::PageParams;
use crate::routes::v1::pql::{V1Pql, parse_v1_pql, push_pql_where};
use crate::routes::work_item::ws_active_member;

/// Serialize a list row and add the SDK-key aliases `assignees`/`labels`
/// (the fork's rows carry `assignee_ids`/`label_ids`). `WorkItemDetail`'s
/// `manage_assignee`/`manage_label` read `assignees`/`labels` back, so the
/// aliases are required for the MCP's merge-then-write path.
pub fn v1_work_item_json(row: &IssueListRow) -> Value {
    let mut v = serde_json::to_value(row).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        let assignees = o.get("assignee_ids").cloned().unwrap_or_else(|| json!([]));
        let labels = o.get("label_ids").cloned().unwrap_or_else(|| json!([]));
        o.insert("assignees".to_string(), assignees);
        o.insert("labels".to_string(), labels);
    }
    v
}

/// `WorkItemGroupedCountResponse`: flat `{grouped_by, sub_grouped_by,
/// total_count, grouped_counts}`. `counts` is `(key, count)` pairs; the
/// special key `"None"` represents a NULL dimension.
pub fn v1_count_json(
    grouped_by: Option<&str>,
    sub_grouped_by: Option<&str>,
    total: i64,
    counts: Vec<(String, i64)>,
) -> Value {
    let mut map = serde_json::Map::new();
    for (k, n) in counts {
        map.insert(k, json!({ "count": n }));
    }
    json!({
        "grouped_by": grouped_by,
        "sub_grouped_by": sub_grouped_by,
        "total_count": total,
        "grouped_counts": Value::Object(map),
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct V1SearchRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub project_identifier: String,
    pub workspace_slug: String,
}

pub fn v1_search_issue_json(row: &V1SearchRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "sequence_id": row.sequence_id,
        "project_id": row.project_id,
        "project__identifier": row.project_identifier,
        "workspace__slug": row.workspace_slug,
    })
}

use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// `?pql=&cursor=&per_page=&order_by=&expand=&fields=` (only the first three
/// have effect; the rest are accepted-and-ignored, matching the app API's
/// documented deviation).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1WorkItemQuery {
    #[serde(default)] pub cursor: Option<String>,
    #[serde(default)] pub per_page: Option<String>,
    #[serde(default)] pub order_by: Option<String>,
    #[serde(default)] pub pql: Option<String>,
    #[serde(default)] pub expand: Option<String>,
    #[serde(default)] pub fields: Option<String>,
    #[serde(default)] pub external_id: Option<String>,
    #[serde(default)] pub external_source: Option<String>,
}

fn pql_or_400(raw: Option<&str>) -> Result<V1Pql, (StatusCode, Json<Value>)> {
    parse_v1_pql(raw.unwrap_or("")).map_err(|msg| (StatusCode::BAD_REQUEST, Json(json!({"pql": msg}))))
}

pub async fn list_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Query(q): Query<V1WorkItemQuery>,
) -> R {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM projects WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL",
    )
    .bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    if exists.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))));
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await?;
    list_envelope(
        &st,
        &slug,
        auth.0,
        Some(project_id),
        false,
        q.cursor.as_deref(),
        q.per_page.as_deref(),
        &pql,
        guest_scoped,
    )
    .await
}

pub async fn list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<V1WorkItemQuery>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };
    list_envelope(
        &st,
        &slug,
        auth.0,
        None,
        false,
        q.cursor.as_deref(),
        q.per_page.as_deref(),
        &pql,
        false,
    )
    .await
}

/// Shared envelope path. `scope_project = Some(pid)` → project list (mirrors
/// `issue_query::list` visibility); `None` → workspace list (member projects
/// only). `archived = true` → `archived_at IS NOT NULL`.
#[allow(clippy::too_many_arguments)]
async fn list_envelope(
    st: &AppState,
    slug: &str,
    user_id: uuid::Uuid,
    scope_project: Option<uuid::Uuid>,
    archived: bool,
    cursor_raw: Option<&str>,
    per_page_raw: Option<&str>,
    pql: &V1Pql,
    guest_scoped: bool,
) -> R {
    let (per_page, cursor) = match (PageParams {
        cursor: cursor_raw.map(str::to_string),
        per_page: per_page_raw.map(str::to_string),
    })
    .resolve()
    {
        Ok(v) => v,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))),
    };
    let limit = per_page.min(1000);
    if limit <= 0 {
        return Ok((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": crate::routes::issue_query::GENERIC_500_MSG}))));
    }
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
        Ok(w) => w,
    };

    let where_qb = |qb: &mut QueryBuilder<Postgres>| {
        qb.push(" WHERE i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
          .push_bind(slug.to_string())
          .push(") AND i.deleted_at IS NULL AND i.is_draft = false AND s.\"group\" <> 'triage'");
        if archived {
            qb.push(" AND i.archived_at IS NOT NULL");
        } else {
            qb.push(" AND i.archived_at IS NULL");
        }
        if let Some(pid) = scope_project {
            qb.push(" AND i.project_id = ").push_bind(pid);
        } else {
            qb.push(" AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
                      AND pm.member_id = ").push_bind(user_id)
              .push(" AND pm.is_active = true AND pm.deleted_at IS NULL)");
        }
        if guest_scoped {
            qb.push(" AND i.created_by_id = ").push_bind(user_id);
        }
        qb.push(" AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id AND p.deleted_at IS NULL AND p.archived_at IS NULL)");
        push_pql_where(qb, pql, user_id);
    };

    let mut count_qb = QueryBuilder::new(
        "SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id",
    );
    where_qb(&mut count_qb);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    let offset_opt: Option<i64> = match window {
        PageWindow::Rows(o) => Some(o),
        PageWindow::BeyondEnd => None,
    };
    let rows: Vec<IssueListRow> = match offset_opt {
        Some(offset) => {
            let mut page_qb = QueryBuilder::new(LIST_SELECT_SQL);
            where_qb(&mut page_qb);
            page_qb.push(" ORDER BY i.created_at DESC LIMIT ").push_bind(limit);
            page_qb.push(" OFFSET ").push_bind(offset);
            page_qb.build_query_as().fetch_all(&st.pool).await?
        }
        None => Vec::new(),
    };
    let results: Vec<Value> = rows.iter().map(v1_work_item_json).collect();
    Ok((StatusCode::OK, Json(build_ungrouped_envelope(total, limit, cursor.page, results))))
}
pub async fn list_archived(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Query(q): Query<V1WorkItemQuery>,
) -> R {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await?;
    // Archived rows keep completed/cancelled groups; the archived clause is
    // flipped by `list_envelope(archived=true)`.
    list_envelope(
        &st,
        &slug,
        auth.0,
        Some(project_id),
        true,
        q.cursor.as_deref(),
        q.per_page.as_deref(),
        &pql,
        guest_scoped,
    )
    .await
}
/// Fetches the SDK `WorkItemDetail` shape: `DETAIL_SELECT_SQL` plus
/// `description_html`, and adds `assignees`/`labels` and the `project`/
/// `workspace` refs. Callers own the visibility gate — active ws member,
/// project member/creator, and the `get_issue` guest-view rule.
async fn fetch_detail(
    st: &AppState,
    slug: &str,
    project_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<Option<Value>, common::errors::AppError> {
    let row: Option<IssueDetailRow> = sqlx::query_as(&format!(
        "{DETAIL_SELECT_SQL} WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND i.deleted_at IS NULL"
    ))
    .bind(pk).bind(project_id).bind(slug)
    .fetch_optional(&st.pool).await?;
    let Some(row) = row else { return Ok(None); };
    let description_html: Option<String> =
        sqlx::query_scalar("SELECT description_html FROM issues WHERE id = $1")
            .bind(pk).fetch_optional(&st.pool).await?.flatten();
    let mut v = serde_json::to_value(&row).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        let assignees = o.get("assignee_ids").cloned().unwrap_or_else(|| json!([]));
        let labels = o.get("label_ids").cloned().unwrap_or_else(|| json!([]));
        o.insert("assignees".to_string(), assignees);
        o.insert("labels".to_string(), labels);
        o.insert("description_html".to_string(), json!(description_html.unwrap_or_else(|| "<p></p>".to_string())));
        o.insert("project".to_string(), json!(project_id));
        o.insert("workspace".to_string(), json!(slug));
    }
    Ok(Some(v))
}

pub async fn retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    ).bind(pk).bind(auth.0).fetch_one(&st.pool).await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15) | Some(5)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    // Guest-view rule (`base.py:596-609`): a guest whose project hides member
    // work and who did not create the issue → 403.
    if matches!(member_role, Some(5)) && !creator {
        let gva: bool =
            sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
                .bind(project_id)
                .fetch_optional(&st.pool)
                .await?
                .unwrap_or(false);
        if !gva {
            return Ok((
                StatusCode::FORBIDDEN,
                Json(json!({"error": crate::routes::versions::DESC_GUEST_MSG})),
            ));
        }
    }
    match fetch_detail(&st, &slug, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}

pub async fn retrieve_by_identifier(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, ident)): Path<(String, String)>,
) -> R {
    let Ok((proj_ident, seq_raw)) = crate::routes::work_item::resolve_identifier(&ident) else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": crate::routes::work_item::INVALID_IDENTIFIER_MSG}))));
    };
    let project_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE w.slug = $1 AND LOWER(p.identifier) = LOWER($2) AND p.deleted_at IS NULL",
    ).bind(&slug).bind(&proj_ident).fetch_optional(&st.pool).await?;
    let Some(project_id) = project_id else { return Ok(missing()); };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if role.is_none() {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": crate::routes::work_item::IDENTIFIER_FORBIDDEN_MSG}))));
    }
    let Ok(seq) = seq_raw.parse::<i32>() else { return Ok(missing()); };
    let pk: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i WHERE i.project_id = $1 AND i.sequence_id = $2 AND i.deleted_at IS NULL",
    ).bind(project_id).bind(seq).fetch_optional(&st.pool).await?;
    let Some(pk) = pk else { return Ok(missing()); };
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    ).bind(pk).bind(auth.0).fetch_one(&st.pool).await?;
    // Guest-view rule (`base.py:596-609`): a guest whose project hides member
    // work and who did not create the issue → 403.
    if matches!(role, Some(5)) && !creator {
        let gva: bool =
            sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
                .bind(project_id)
                .fetch_optional(&st.pool)
                .await?
                .unwrap_or(false);
        if !gva {
            return Ok((
                StatusCode::FORBIDDEN,
                Json(json!({"error": crate::routes::versions::DESC_GUEST_MSG})),
            ));
        }
    }
    match fetch_detail(&st, &slug, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1SearchQuery {
    #[serde(default)] pub search: Option<String>,
}

pub async fn search(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<V1SearchQuery>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let pattern = match q.search.as_deref() {
        Some(s) if !s.trim().is_empty() => format!("%{}%", s.replace(['%', '_', '\\'], "")),
        _ => "%".to_string(),
    };
    let rows: Vec<V1SearchRow> = sqlx::query_as(
        "SELECT i.id, i.name, i.sequence_id, i.project_id, p.identifier AS project_identifier, \
                w.slug AS workspace_slug \
         FROM issues i \
         LEFT JOIN states s ON s.id = i.state_id \
         JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN project_members pm ON pm.project_id = i.project_id \
         WHERE w.slug = $1 AND pm.member_id = $2 AND pm.is_active = true \
           AND pm.deleted_at IS NULL \
           AND i.name ILIKE $3 AND i.deleted_at IS NULL AND i.archived_at IS NULL \
           AND i.is_draft = false \
           AND (s.id IS NULL OR s.\"group\" <> 'triage') \
           AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id AND p.deleted_at IS NULL AND p.archived_at IS NULL) \
         ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(&slug).bind(auth.0).bind(&pattern)
    .fetch_all(&st.pool).await?;
    let issues: Vec<Value> = rows.iter().map(v1_search_issue_json).collect();
    Ok((StatusCode::OK, Json(json!({ "issues": issues }))))
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1CountQuery {
    #[serde(default)] pub pql: Option<String>,
    #[serde(default)] pub group_by: Option<String>,
    #[serde(default)] pub sub_group_by: Option<String>,
}

/// Maps an SDK `group_by` value to a SQL expression over the list scope.
/// Only dimensions backed by columns in this fork are supported; the MCP's
/// module/cycle/milestone/release keys return `None` → 400.
pub fn count_group_column(key: &str) -> Option<&'static str> {
    match key {
        "state_id" => Some("i.state_id::text"),
        "state__group" => Some("s.\"group\""),
        "priority" => Some("i.priority"),
        "project_id" => Some("i.project_id::text"),
        "type_id" => Some("i.type_id::text"),
        "created_by" => Some("i.created_by_id::text"),
        "target_date" => Some("i.target_date::text"),
        "start_date" => Some("i.start_date::text"),
        _ => None,
    }
}

pub async fn count(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<V1CountQuery>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    if matches!(q.sub_group_by.as_deref(), Some(s) if !s.trim().is_empty()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "sub_group_by is not supported"}))));
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };

    let base_where = |qb: &mut QueryBuilder<Postgres>| {
        qb.push(" WHERE i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
          .push_bind(slug.clone())
          .push(") AND i.deleted_at IS NULL AND i.is_draft = false AND s.\"group\" <> 'triage' AND i.archived_at IS NULL")
          .push(" AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
                  AND pm.member_id = ").push_bind(auth.0)
          .push(" AND pm.is_active = true AND pm.deleted_at IS NULL)")
          .push(" AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id AND p.deleted_at IS NULL AND p.archived_at IS NULL)");
        push_pql_where(qb, &pql, auth.0);
    };

    let group_key = q.group_by.as_deref().filter(|s| !s.trim().is_empty());
    let Some(key) = group_key else {
        let mut qb = QueryBuilder::new("SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id");
        base_where(&mut qb);
        let total: i64 = qb.build_query_scalar().fetch_one(&st.pool).await?;
        return Ok((StatusCode::OK, Json(v1_count_json(None, None, total, vec![]))));
    };
    let Some(expr) = count_group_column(key) else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": format!("Unsupported group_by: {key}")}))));
    };

    let count_sql = format!(
        "SELECT COALESCE({expr}, 'None') AS k, COUNT(*) AS n \
         FROM issues i LEFT JOIN states s ON s.id = i.state_id"
    );
    let mut qb = QueryBuilder::new(count_sql);
    base_where(&mut qb);
    qb.push(format!(" GROUP BY {expr}"));
    let rows: Vec<(String, i64)> = qb.build_query_as().fetch_all(&st.pool).await?;
    let total: i64 = rows.iter().map(|(_, n)| *n).sum();
    Ok((StatusCode::OK, Json(v1_count_json(Some(key), None, total, rows))))
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1WriteWorkItem {
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub assignees: Option<Vec<uuid::Uuid>>,
    #[serde(default)] pub labels: Option<Vec<uuid::Uuid>>,
    #[serde(default)] pub state: Option<uuid::Uuid>,
    #[serde(default)] pub type_id: Option<uuid::Uuid>,
    #[serde(default)] pub point: Option<i32>,
    #[serde(default)] pub estimate_point: Option<uuid::Uuid>,
    #[serde(default)] pub priority: Option<String>,
    #[serde(default)] pub start_date: Option<String>,
    #[serde(default)] pub target_date: Option<String>,
    #[serde(default)] pub sort_order: Option<f64>,
    #[serde(default)] pub parent: Option<uuid::Uuid>,
    #[serde(default)] pub is_draft: Option<bool>,
    #[serde(default)] pub description_html: Option<String>,
    #[serde(default)] pub description_stripped: Option<String>,
    #[serde(default)] pub external_source: Option<String>,
    #[serde(default)] pub external_id: Option<String>,
}

#[derive(Clone)]
enum BindValue {
    Text(String),
    Date(Option<chrono::NaiveDate>),
    Uuid(Option<uuid::Uuid>),
    Int(Option<i32>),
    Float(f64),
    Bool(bool),
}

pub const V1_PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

/// Wraps plain text into a single paragraph, escaping HTML metacharacters so
/// the documented plain-text field cannot inject markup.
fn wrap_stripped(plain: &str) -> String {
    format!(
        "<p>{}</p>",
        plain
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<br/>")
    )
}

fn description_of(body: &V1WriteWorkItem) -> String {
    if let Some(html) = body.description_html.as_deref().filter(|s| !s.is_empty()) {
        return html.to_string();
    }
    if let Some(plain) = body.description_stripped.as_deref().filter(|s| !s.is_empty()) {
        return wrap_stripped(plain);
    }
    "<p></p>".to_string()
}

fn internal(e: sqlx::Error) -> (StatusCode, Json<Value>) {
    let _ = e;
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Something went wrong please try again later"})))
}

async fn validate_write(
    st: &AppState,
    project_id: uuid::Uuid,
    body: &V1WriteWorkItem,
    require_name: bool,
) -> Result<(), (StatusCode, Json<Value>)> {
    let bad = |msg: String| (StatusCode::BAD_REQUEST, Json(json!({"error": msg})));
    match body.name.as_deref() {
        None if require_name => return Err(bad("name is required".into())),
        Some(n) if n.trim().is_empty() => return Err(bad("name is required".into())),
        Some(n) if n.chars().count() > 255 => return Err(bad("name max length 255".into())),
        _ => {}
    }
    if let Some(p) = &body.priority {
        if !V1_PRIORITIES.contains(&p.as_str()) { return Err(bad("Invalid priority".into())); }
    }
    if let Some(t) = body.type_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND deleted_at IS NULL)",
        ).bind(t).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("type_id is not valid".into())); }
    }
    if let Some(ids) = body.assignees.as_ref().filter(|v| !v.is_empty()) {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) AND is_active = true AND role >= 15 AND deleted_at IS NULL",
        ).bind(project_id).bind(ids).fetch_one(&st.pool).await.map_err(internal)?;
        if n != ids.len() as i64 { return Err(bad("invalid assignee: not a project member".into())); }
    }
    if let Some(ids) = body.labels.as_ref().filter(|v| !v.is_empty()) {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM labels WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        ).bind(project_id).bind(ids).fetch_one(&st.pool).await.map_err(internal)?;
        if n != ids.len() as i64 { return Err(bad("invalid label: not in project".into())); }
    }
    if let Some(state_id) = body.state {
        let (ok,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL AND \"group\" != 'triage' AND is_triage = false)")
            .bind(state_id).bind(project_id).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("State is not valid please pass a valid state_id".into())); }
    }
    if let Some(ep) = body.estimate_point {
        let (ok,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM estimate_points WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)")
            .bind(ep).bind(project_id).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("estimate_point is not valid".into())); }
    }
    if let Some(parent) = body.parent {
        let (ok,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)")
            .bind(parent).bind(project_id).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("parent is not valid".into())); }
    }
    parse_date(&body.start_date).map_err(bad)?;
    parse_date(&body.target_date).map_err(bad)?;
    Ok(())
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<V1WriteWorkItem>,
) -> R {
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let project_ok: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM projects WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL AND archived_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if project_ok.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))));
    }
    if let Err(e) = validate_write(&st, project_id, &body, true).await {
        return Ok(e);
    }
    let name = body.name.clone().unwrap_or_default();

    let (default_state, first_state): (Option<uuid::Uuid>, Option<uuid::Uuid>) = (
        sqlx::query_scalar("SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true ORDER BY created_at ASC LIMIT 1")
            .bind(project_id).fetch_optional(&st.pool).await?,
        sqlx::query_scalar("SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL AND \"group\" != 'triage' AND is_triage = false ORDER BY created_at ASC LIMIT 1")
            .bind(project_id).fetch_optional(&st.pool).await?,
    );
    let state_id = resolve_effective_state(body.state, default_state, first_state);
    let start_date = match parse_date(&body.start_date) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    let target_date = match parse_date(&body.target_date) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    let priority = body.priority.clone().unwrap_or_else(|| "none".to_string());
    let html = description_of(&body);

    let mut tx = st.pool.begin().await?;
    // Serialize per-project creates so two concurrent calls cannot pick the
    // same sequence (no unique index exists on issues.sequence_id).
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id = $1 FOR UPDATE")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(GREATEST(\
            (SELECT MAX(sequence) FROM issue_sequences WHERE project_id = $1), \
            (SELECT MAX(sequence_id) FROM issues WHERE project_id = $1 AND deleted_at IS NULL)\
         ), 0) + 1",
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "INSERT INTO issues (id, name, description_html, description_json, description_stripped, priority, start_date, target_date, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, created_by_id, updated_by_id, point, estimate_point_id, type_id, parent_id, external_source, external_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, '{}', $3, $4, $5, $6, $7, \
                COALESCE($8, COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $9 AND deleted_at IS NULL), 65535.0) + 10000), \
                $10, \
                $11, $9, w.id, $12, $12, $13, $14, $15, $16, $17, $18, now(), now() \
         FROM workspaces w WHERE w.slug = $19 RETURNING id",
    )
    .bind(&name).bind(&html).bind(body.description_stripped.as_deref())
    .bind(&priority).bind(start_date).bind(target_date)
    .bind(body.is_draft.unwrap_or(false)).bind(body.sort_order)
    .bind(project_id).bind(sequence as i32).bind(state_id).bind(auth.0)
    .bind(body.point).bind(body.estimate_point).bind(body.type_id).bind(body.parent)
    .bind(body.external_source.as_deref()).bind(body.external_id.as_deref())
    .bind(&slug)
    .fetch_optional(&mut *tx).await?;
    let Some((issue_id,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))));
    };
    sqlx::query(
        "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, created_by_id, deleted, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, (SELECT workspace_id FROM projects WHERE id = $3), $4, false, now(), now())",
    )
    .bind(sequence).bind(issue_id).bind(project_id).bind(auth.0)
    .execute(&mut *tx).await?;
    replace_bridges(&mut tx, issue_id, project_id, auth.0, body.assignees.as_deref(), body.labels.as_deref()).await?;
    tx.commit().await?;

    match fetch_detail(&st, &slug, project_id, issue_id).await? {
        Some(v) => Ok((StatusCode::CREATED, Json(v))),
        None => Ok(missing()),
    }
}
pub async fn update(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<V1WriteWorkItem>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    ).bind(pk).bind(auth.0).fetch_one(&st.pool).await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND (s.id IS NULL OR s.\"group\" != 'triage') \
         AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $2 AND p.deleted_at IS NULL AND p.archived_at IS NULL)",
    ).bind(pk).bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    if exists.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    if let Err(e) = validate_write(&st, project_id, &body, false).await {
        return Ok(e);
    }

    let start_date = match parse_date(&body.start_date) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    let target_date = match parse_date(&body.target_date) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    let html = body.description_html.clone().filter(|s| !s.is_empty()).or_else(|| {
        body.description_stripped.as_deref().filter(|s| !s.is_empty()).map(wrap_stripped)
    });

    // Only fields present in the JSON body are written (`COALESCE` cannot set
    // an explicit NULL back), so the SET list and its positional binds are
    // built together in one pass via `BindValue`.
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<BindValue> = Vec::new();
    fn add(sets: &mut Vec<String>, values: &mut Vec<BindValue>, col: &str, v: BindValue) {
        sets.push(format!("{col} = ${}", values.len() + 1));
        values.push(v);
    }
    if let Some(v) = body.name.clone() { add(&mut sets, &mut values, "name", BindValue::Text(v)); }
    if let Some(v) = html.clone() { add(&mut sets, &mut values, "description_html", BindValue::Text(v)); }
    if let Some(v) = body.description_stripped.clone() { add(&mut sets, &mut values, "description_stripped", BindValue::Text(v)); }
    if let Some(v) = body.priority.clone() { add(&mut sets, &mut values, "priority", BindValue::Text(v)); }
    if body.start_date.is_some() { add(&mut sets, &mut values, "start_date", BindValue::Date(start_date)); }
    if body.target_date.is_some() { add(&mut sets, &mut values, "target_date", BindValue::Date(target_date)); }
    if body.state.is_some() { add(&mut sets, &mut values, "state_id", BindValue::Uuid(body.state)); }
    if body.type_id.is_some() { add(&mut sets, &mut values, "type_id", BindValue::Uuid(body.type_id)); }
    if body.parent.is_some() { add(&mut sets, &mut values, "parent_id", BindValue::Uuid(body.parent)); }
    if body.point.is_some() { add(&mut sets, &mut values, "point", BindValue::Int(body.point)); }
    if body.estimate_point.is_some() { add(&mut sets, &mut values, "estimate_point_id", BindValue::Uuid(body.estimate_point)); }
    if let Some(v) = body.sort_order { add(&mut sets, &mut values, "sort_order", BindValue::Float(v)); }
    if let Some(v) = body.is_draft { add(&mut sets, &mut values, "is_draft", BindValue::Bool(v)); }
    if let Some(v) = body.external_source.clone() { add(&mut sets, &mut values, "external_source", BindValue::Text(v)); }
    if let Some(v) = body.external_id.clone() { add(&mut sets, &mut values, "external_id", BindValue::Text(v)); }

    if sets.is_empty() && body.assignees.is_none() && body.labels.is_none() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "No supported fields"}))));
    }

    let mut tx = st.pool.begin().await?;
    if !sets.is_empty() {
        let user_pos = values.len() + 1;
        let pk_pos = values.len() + 2;
        let project_pos = values.len() + 3;
        let sql = format!(
            "UPDATE issues SET {}, updated_at = now(), updated_by_id = ${user_pos} \
             WHERE id = ${pk_pos} AND project_id = ${project_pos} AND deleted_at IS NULL",
            sets.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for v in &values {
            q = match v {
                BindValue::Text(s) => q.bind(s.clone()),
                BindValue::Date(d) => q.bind(*d),
                BindValue::Uuid(u) => q.bind(*u),
                BindValue::Int(n) => q.bind(*n),
                BindValue::Float(f) => q.bind(*f),
                BindValue::Bool(b) => q.bind(*b),
            };
        }
        q = q.bind(auth.0).bind(pk).bind(project_id);
        q.execute(&mut *tx).await?;
    } else {
        sqlx::query("UPDATE issues SET updated_at = now(), updated_by_id = $1 WHERE id = $2 AND project_id = $3 AND deleted_at IS NULL")
            .bind(auth.0).bind(pk).bind(project_id).execute(&mut *tx).await?;
    }

    replace_bridges(&mut tx, pk, project_id, auth.0, body.assignees.as_deref(), body.labels.as_deref()).await?;
    tx.commit().await?;

    match fetch_detail(&st, &slug, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}
/// `POST .../work-items/{id}/archive/`. Mirrors the app API's single-issue
/// archive (`issue_archive_one::archive`): ADMIN/MEMBER gate, live+draft=false
/// scope, state group must be `completed`/`cancelled` else 400, then 204.
pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT s.\"group\" FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND (s.id IS NULL OR s.\"group\" != 'triage')",
    ).bind(pk).bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    let Some((group,)) = row else { return Ok(missing()); };
    if let Err(msg) = guard_archive_one_group(group.as_deref().unwrap_or("")) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": msg}))));
    }
    sqlx::query("UPDATE issues SET archived_at = now(), updated_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL")
        .bind(pk).bind(project_id).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i WHERE i.id = $1 AND i.project_id = $2 \
         AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NOT NULL",
    ).bind(pk).bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    if exists.is_none() {
        return Ok(missing());
    }
    sqlx::query("UPDATE issues SET archived_at = NULL, updated_at = now() WHERE id = $1 AND project_id = $2")
        .bind(pk).bind(project_id).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_absent_or_empty_is_none() {
        assert_eq!(parse_date(&None), Ok(None));
        assert_eq!(parse_date(&Some("".into())), Ok(None));
    }

    #[test]
    fn parse_date_parses_iso() {
        assert_eq!(
            parse_date(&Some("2026-01-02".into())),
            Ok(Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()))
        );
    }

    #[test]
    fn parse_date_rejects_garbage() {
        assert!(parse_date(&Some("nope".into())).is_err());
    }

    #[test]
    fn wrap_stripped_wraps_plain_text() {
        assert_eq!(wrap_stripped("text"), "<p>text</p>");
    }

    #[test]
    fn wrap_stripped_escapes_script() {
        assert_eq!(
            wrap_stripped("<script>x</script>"),
            "<p>&lt;script&gt;x&lt;/script&gt;</p>"
        );
    }

    #[test]
    fn wrap_stripped_turns_newline_into_break() {
        assert_eq!(wrap_stripped("a\nb"), "<p>a<br/>b</p>");
    }

    #[test]
    fn wrap_stripped_escapes_ampersand_without_double_escaping() {
        assert_eq!(wrap_stripped("a & b"), "<p>a &amp; b</p>");
    }

    #[test]
    fn description_prefers_html() {
        let body = V1WriteWorkItem {
            description_html: Some("<p>rich</p>".into()),
            description_stripped: Some("plain".into()),
            ..Default::default()
        };
        assert_eq!(description_of(&body), "<p>rich</p>");
    }

    #[test]
    fn description_wraps_stripped_when_no_html() {
        let body = V1WriteWorkItem {
            description_stripped: Some("a <b>\nline".into()),
            ..Default::default()
        };
        assert_eq!(description_of(&body), "<p>a &lt;b&gt;<br/>line</p>");
    }

    #[test]
    fn description_of_delegates_stripped_to_wrap_stripped() {
        let body = V1WriteWorkItem {
            description_html: Some("".into()),
            description_stripped: Some("a & b".into()),
            ..Default::default()
        };
        assert_eq!(description_of(&body), wrap_stripped("a & b"));
    }

    #[test]
    fn description_defaults_to_empty_paragraph() {
        let body = V1WriteWorkItem::default();
        assert_eq!(description_of(&body), "<p></p>");
    }
}
