//! v1 work-item handlers (`/api/v1/.../work-items/...`).

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};

use crate::routes::issue_common::{
    IssueDetailRow, IssueListRow, PageWindow, fetch_guest_scoped, fetch_project_member_role,
    is_workspace_admin, page_window, project_gate_allows,
};
use crate::routes::issue_query::{DETAIL_SELECT_SQL, LIST_SELECT_SQL, build_ungrouped_envelope};
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
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
        Ok(w) => w,
    };

    let mut where_qb = |qb: &mut QueryBuilder<Postgres>| {
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
    user_id: uuid::Uuid,
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
    let _ = user_id;
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
    match fetch_detail(&st, &slug, auth.0, project_id, pk).await? {
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
    match fetch_detail(&st, &slug, auth.0, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}
pub async fn search(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn count(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn create(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>, _: Json<Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn update(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>, _: Json<Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn archive(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn unarchive(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
