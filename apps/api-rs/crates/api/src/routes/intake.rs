use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/intake.py:IntakeSerializer` served by
/// `plane/app/urls/intake.py` (IntakeViewSet list/create; `inboxes/` is
/// an alias of the same viewset). Unique (name, project) → 409 mirrors
/// `intake_unique_name_project_when_deleted_at_null`. The default-intake
/// delete guard ("You cannot delete the default intake",
/// `plane/app/views/intake/base.py:88`) belongs to the detail task.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIntake {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntakeOut {
    pub id: uuid::Uuid,
    pub name: String,
}

/// Mirrors `plane/app/views/intake/base.py:IntakeIssueViewSet.create`:
/// nested `issue.name` required ("Name is required"), `issue.priority`
/// must be low/medium/high/urgent/none ("Invalid priority"). The issue
/// is created in the project's triage state (created on demand, mirroring
/// the view) and linked with status -2 (Pending).
#[derive(Debug, Clone, Deserialize)]
pub struct IntakeIssuePayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateIntakeIssue {
    pub issue: IntakeIssuePayload,
}

const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

pub fn validate_create(body: &CreateIntake) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    Ok(())
}

pub fn validate_issue_create(body: &CreateIntakeIssue) -> Result<(), String> {
    match &body.issue.name {
        Some(n) if !n.trim().is_empty() => {}
        _ => return Err("Name is required".to_string()),
    }
    let priority = body.issue.priority.as_deref().unwrap_or("none");
    if !PRIORITIES.contains(&priority) {
        return Err("Invalid priority".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<IntakeOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::intake::Intake>(
        "SELECT id, name FROM intakes WHERE project_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|i| IntakeOut { id: i.id, name: i.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIntake>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let existing = sqlx::query_as::<_, common::models::intake::Intake>(
        "SELECT id, name FROM intakes WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&body.name)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(intake) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({"error": "Intake with the same name already exists in the project", "id": intake.id})),
        ));
    }

    let row = sqlx::query_as::<_, common::models::intake::Intake>(
        "INSERT INTO intakes (id, name, description, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, w.id, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

pub async fn list_issues(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::intake::IntakeIssue>(
        "SELECT id, status FROM intake_issues WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|ii| json!({"id": ii.id, "status": ii.status}))
            .collect(),
    ))
}

pub async fn create_issue(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIntakeIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_issue_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let name = body.issue.name.clone().unwrap_or_default();
    let priority = body.issue.priority.clone().unwrap_or_else(|| "none".to_string());

    let workspace_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Workspace not found"}))));
    };

    // Triage state lookup-or-create mirrors the viewset: the intake issue
    // lands in triage, creating the state row on demand.
    let triage_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND is_triage = true AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let triage_id = match triage_id {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO states (id, name, \"group\", color, sequence, is_triage, \"default\", project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), 'Triage', 'triage', '#4E5355', 65000, true, false, $1, $2, now(), now()) RETURNING id",
            )
            .bind(project_id)
            .bind(workspace_id)
            .fetch_one(&st.pool)
            .await?
        }
    };

    let issue_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO issues (id, name, priority, state_id, project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now()) RETURNING id",
    )
    .bind(&name)
    .bind(&priority)
    .bind(triage_id)
    .bind(project_id)
    .bind(workspace_id)
    .fetch_one(&st.pool)
    .await?;

    // The viewset attaches to the project's first intake (base.py:271).
    let intake_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM intakes WHERE project_id = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(intake_id) = intake_id else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
    };

    let row = sqlx::query_as::<_, common::models::intake::IntakeIssue>(
        "INSERT INTO intake_issues (id, intake_id, issue_id, status, project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, -2, $3, $4, now(), now()) RETURNING id, status",
    )
    .bind(intake_id)
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": row.id, "status": row.status, "issue_id": issue_id})),
    ))
}
