//! v1 project handlers (`/api/v1/workspaces/.../projects/...`). Object
//! endpoints delegate to the app-API handlers; list/derived shapes live here.

use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::v1::common::PageParams;

pub async fn list_lite(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<PageParams>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
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
