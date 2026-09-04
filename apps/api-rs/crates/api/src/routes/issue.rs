use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/issue.py:IssueCreateSerializer`
/// with #9526 fix: unknown assignee/label ids must 400, not silently drop.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIssue {
    pub name: String,
    #[serde(default)]
    pub assignee_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub label_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub state_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreateIssue) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<IssueOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::issue::Issue>(
        "SELECT id, name FROM issues WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|i| IssueOut { id: i.id, name: i.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIssue>,
) -> Result<(StatusCode, Json<IssueOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    // #9526 fix: reject unknown assignees (must be active project members role>=15)
    if let Some(ids) = &body.assignee_ids {
        if !ids.is_empty() {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) AND is_active = true AND role >= 15",
            )
            .bind(project_id)
            .bind(ids)
            .fetch_one(&st.pool)
            .await?;
            if count.0 != ids.len() as i64 {
                return Err(anyhow::anyhow!("invalid assignee_id: not a project member").into());
            }
        }
    }
    // #9526 fix: reject unknown labels (must belong to project)
    if let Some(ids) = &body.label_ids {
        if !ids.is_empty() {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM labels WHERE project_id = $1 AND id = ANY($2)",
            )
            .bind(project_id)
            .bind(ids)
            .fetch_one(&st.pool)
            .await?;
            if count.0 != ids.len() as i64 {
                return Err(anyhow::anyhow!("invalid label_id: not in project").into());
            }
        }
    }
    // state must belong to project if provided
    if let Some(state_id) = &body.state_id {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2)",
        )
        .bind(state_id)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await?;
        if !exists.0 {
            return Err(anyhow::anyhow!("State is not valid please pass a valid state_id").into());
        }
    }

    let row = sqlx::query_as::<_, common::models::issue::Issue>(
        "INSERT INTO issues (id, name, project_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, now(), now()) RETURNING id, name",
    )
    .bind(&body.name)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(IssueOut { id: row.id, name: row.name }),
    ))
}
