//! v1 workspace handlers (`/api/v1/workspaces/.../features/`). Synthetic
//! feature flags; this fork stores none of them so both verbs serve the
//! same all-false shape.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::routes::member::deny_detail;
use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub fn v1_workspace_features_json() -> Value {
    json!({
        "project_grouping": false,
        "initiatives": false,
        "teams": false,
        "customers": false,
        "wiki": false,
        "pi": false,
        "work_item_types": false,
        "releases": false,
        "states_owned_by_workspace": false,
    })
}

pub async fn get_features(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    Ok((StatusCode::OK, Json(v1_workspace_features_json())))
}

/// PATCH is a no-op: this fork stores none of the workspace feature flags,
/// so any body is accepted (admins only) and the same all-false shape is
/// returned without writing anything.
pub async fn patch_features(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(_body): Json<Value>,
) -> R {
    if !matches!(ws_role(&st.pool, auth.0, &slug).await?, Some(r) if r >= 20) {
        return Ok(deny());
    }
    Ok((StatusCode::OK, Json(v1_workspace_features_json())))
}
