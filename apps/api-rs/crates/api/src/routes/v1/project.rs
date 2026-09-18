//! v1 project handlers (`/api/v1/workspaces/.../projects/...`). Object
//! endpoints delegate to the app-API handlers; list/derived shapes live here.

use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::issue_query::build_ungrouped_envelope;
use crate::routes::member::deny_detail;
use crate::routes::project::{cover_image_url, ws_role};
use crate::routes::v1::common::PageParams;

/// Trimmed project row for the `projects-lite` shape (`ProjectLiteSerializer`
/// fields the SDK's `ProjectLite` model consumes).
#[derive(Debug, Clone, FromRow)]
pub struct ProjectLiteRow {
    pub id: uuid::Uuid,
    pub identifier: String,
    pub name: String,
    pub cover_image: Option<String>,
    pub icon_prop: Option<Value>,
    pub emoji: Option<String>,
    pub description: String,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub cover_image_asset_id: Option<uuid::Uuid>,
    pub cover_image_entity_type: Option<String>,
}

pub fn v1_project_lite_json(r: &ProjectLiteRow) -> Value {
    json!({
        "id": r.id,
        "identifier": r.identifier,
        "name": r.name,
        "cover_image": r.cover_image,
        "icon_prop": r.icon_prop.clone().unwrap_or(Value::Null),
        "emoji": r.emoji,
        "description": r.description,
        "cover_image_url": cover_image_url(r.cover_image_asset_id, r.cover_image_entity_type.as_deref(), r.cover_image.as_deref()),
        "archived_at": r.archived_at,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LiteListQuery {
    #[serde(default)] pub cursor: Option<String>,
    #[serde(default)] pub per_page: Option<String>,
    #[serde(default)] pub order_by: Option<String>,
    #[serde(default)] pub include_archived: Option<String>,
}

impl LiteListQuery {
    fn include_archived(&self) -> bool {
        match self.include_archived.as_deref() {
            Some(s) => {
                let t = s.trim().to_ascii_lowercase();
                t == "true" || t == "1"
            }
            None => false,
        }
    }
}

pub async fn list_lite(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<LiteListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(role) = ws_role(&st.pool, auth.0, &slug).await? else {
        return Ok(deny_detail());
    };
    let (per_page, cursor) = match (PageParams { cursor: q.cursor.clone(), per_page: q.per_page.clone() }).resolve() {
        Ok(v) => v,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))),
    };
    let limit = per_page.min(1000);
    let window = match crate::routes::v1::common::window_for(cursor.page, limit) {
        Ok(w) => w,
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
    };
    let scope = if role <= 5 {
        "AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
         AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL)"
    } else if role <= 15 {
        "AND (p.network = 2 OR EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
         AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL))"
    } else {
        ""
    };
    let archived_clause = if q.include_archived() { "" } else { "AND p.archived_at IS NULL" };
    let base = format!(
        "FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         LEFT JOIN file_assets fa ON fa.id = p.cover_image_asset_id \
         WHERE w.slug = $1 AND p.deleted_at IS NULL {archived_clause} {scope}"
    );
    let count_sql = format!("SELECT COUNT(*) {base}");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(&slug)
        .bind(auth.0)
        .fetch_one(&st.pool)
        .await?;
    let offset: Option<i64> = match window {
        crate::routes::issue_common::PageWindow::Rows(o) => Some(o),
        crate::routes::issue_common::PageWindow::BeyondEnd => None,
    };
    let rows: Vec<ProjectLiteRow> = match offset {
        Some(offset) => {
            let sql = format!(
                "SELECT p.id, p.identifier, p.name, p.cover_image, p.icon_prop, p.emoji, \
                 p.description, p.archived_at, p.cover_image_asset_id, fa.entity_type AS cover_image_entity_type {base} \
                 ORDER BY p.name ASC LIMIT $3 OFFSET $4"
            );
            sqlx::query_as(&sql)
                .bind(&slug)
                .bind(auth.0)
                .bind(limit)
                .bind(offset)
                .fetch_all(&st.pool)
                .await?
        }
        None => Vec::new(),
    };
    let results: Vec<Value> = rows.iter().map(v1_project_lite_json).collect();
    Ok((StatusCode::OK, Json(build_ungrouped_envelope(total, limit, cursor.page, results))))
}
pub async fn get_features(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn patch_features(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>, _: Json<Value>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn total_worklogs(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
