//! v1 wrappers for sub-resource list endpoints whose app-API handler returns a bare array but whose SDK contract is the 12-key pagination envelope.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::middleware::auth::AuthUser;
use crate::routes::v1::common::PageParams;
use crate::state::AppState;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub async fn list_comments(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<PageParams>,
) -> R {
    let Json(rows) =
        crate::routes::work_item::list_comments(State(st), auth, Path((slug, project_id, issue_id)))
            .await?;
    match crate::routes::v1::common::page_rows(rows, q.per_page.as_deref(), q.cursor.as_deref()) {
        Ok(v) => Ok((StatusCode::OK, Json(v))),
        Err(msg) => {
            let (s, v) = crate::routes::v1::common::bad_request(msg);
            Ok((s, Json(v)))
        }
    }
}

pub async fn list_links(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<PageParams>,
) -> R {
    let (status, Json(body)) =
        crate::routes::work_item::list_links(State(st), auth, Path((slug, project_id, issue_id)))
            .await?;
    if status != StatusCode::OK {
        return Ok((status, Json(body)));
    }
    let rows = body.as_array().cloned().unwrap_or_default();
    match crate::routes::v1::common::page_rows(rows, q.per_page.as_deref(), q.cursor.as_deref()) {
        Ok(v) => Ok((StatusCode::OK, Json(v))),
        Err(msg) => {
            let (s, v) = crate::routes::v1::common::bad_request(msg);
            Ok((s, Json(v)))
        }
    }
}
