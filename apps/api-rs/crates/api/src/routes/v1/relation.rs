use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::v1::common::PageParams;
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub const DEPENDENCY_TYPES: [&str; 6] = [
    "blocking",
    "blocked_by",
    "start_before",
    "start_after",
    "finish_before",
    "finish_after",
];

#[derive(Debug, Clone, Deserialize)]
pub struct V1CreateDependency {
    #[serde(default)]
    pub relation_type: String,
    #[serde(default)]
    pub work_item_ids: Vec<uuid::Uuid>,
}

pub async fn list_dependencies(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    crate::routes::work_item::list_relations(State(st), auth, Path((slug, project_id, issue_id)))
        .await
}

pub async fn create_dependencies(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<V1CreateDependency>,
) -> R {
    if !DEPENDENCY_TYPES.contains(&body.relation_type.as_str()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid relation_type"})),
        ));
    }
    if body.work_item_ids.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "work_item_ids is required"})),
        ));
    }
    let inner = crate::routes::work_item::CreateRelation {
        issues: body.work_item_ids,
        relation_type: Some(body.relation_type),
    };
    crate::routes::work_item::create_relations(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
        Json(inner),
    )
    .await
}

pub async fn remove_dependency(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id, related_id)): Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> R {
    let inner = crate::routes::work_item::RemoveRelationBody {
        related_issue: Some(related_id),
    };
    crate::routes::work_item::remove_relation(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
        Json(inner),
    )
    .await
}

pub async fn list_custom(
    State(_st): State<AppState>,
    _auth: AuthUser,
    Path((_slug, _project_id, _issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(_q): Query<PageParams>,
) -> R {
    Ok((StatusCode::OK, Json(json!({}))))
}

pub async fn custom_create_not_supported(
    State(_st): State<AppState>,
    _auth: AuthUser,
    Path((_slug, _project_id, _issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    Ok((
        StatusCode::NOT_FOUND,
        Json(
            json!({"error": "Work item relation definitions are not available in this workspace"}),
        ),
    ))
}

pub async fn custom_delete_not_supported(
    State(_st): State<AppState>,
    _auth: AuthUser,
    Path((_slug, _project_id, _issue_id, _related_id)): Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> R {
    Ok((
        StatusCode::NOT_FOUND,
        Json(
            json!({"error": "Work item relation definitions are not available in this workspace"}),
        ),
    ))
}
