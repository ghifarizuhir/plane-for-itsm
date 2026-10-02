//! Review control boards (TCB/RCB): generic review requests + rapat (sessions),
//! agenda outcomes, participants, in-app notifications.
//! Spec: docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::issue_common::{fetch_project_member_role, is_workspace_admin};
use super::release::{gate_ws_admin, gate_ws_member, release_in_workspace};
use super::service::{bad_request, validate_enum};
use super::workflow::validate_name;

pub const BOARD_TYPES: &[&str] = &["tcb", "rcb"];
pub const REQUEST_STATUSES: &[&str] = &["pending", "scheduled", "decided", "withdrawn"];
pub const SESSION_STATUSES: &[&str] = &["scheduled", "completed", "cancelled"];
pub const OUTCOMES: &[&str] = &["approved", "rejected", "approved_with_notes", "deferred"];
pub const FINAL_OUTCOMES: &[&str] = &["approved", "rejected", "approved_with_notes"];
pub const PARTICIPANT_ROLES: &[&str] = &["chair", "secretary", "member"];
pub const ATTENDANCE_VALUES: &[&str] = &["invited", "present", "absent"];

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub(crate) async fn gate_project_member(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    super::service::gate_member(pool, user, slug, project_id).await
}

pub(crate) async fn gate_project_admin(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(matches!(role, Some(20)) || ws_admin)
}

/// Description for notification titles, e.g. `PROJ-12 Fix login` / `REL-3 Rilis 2026.10`.
pub(crate) fn subject_label(row: &ReviewRequestRow) -> String {
    if row.board_type == "tcb" {
        match (row.issue_identifier.as_deref(), row.issue_name.as_deref()) {
            (Some(identifier), Some(name)) => format!("{identifier} {name}"),
            _ => "Change request".to_string(),
        }
    } else {
        match (row.release_sequence_id, row.release_name.as_deref()) {
            (Some(sequence), Some(name)) => format!("REL-{sequence} {name}"),
            _ => "Release".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Review requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewRequestRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub board_type: String,
    pub status: String,
    pub change_issue_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub submission_note: String,
    pub submitted_by_id: Option<Uuid>,
    pub submitted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub issue_identifier: Option<String>,
    pub issue_name: Option<String>,
    pub issue_project_id: Option<Uuid>,
    pub issue_project_identifier: Option<String>,
    pub release_sequence_id: Option<i64>,
    pub release_name: Option<String>,
    pub release_version: Option<String>,
    pub release_status: Option<String>,
    pub session_id: Option<Uuid>,
    pub session_title: Option<String>,
    pub session_scheduled_at: Option<DateTime<Utc>>,
}

pub(crate) const REQUEST_SELECT: &str = "SELECT rr.id, rr.workspace_id, rr.board_type, rr.status, \
    rr.change_issue_id, rr.release_id, rr.project_id, rr.submission_note, rr.submitted_by_id, \
    rr.submitted_at, rr.created_at, rr.updated_at, \
    (p.identifier || '-' || i.sequence_id::text) AS issue_identifier, i.name AS issue_name, \
    i.project_id AS issue_project_id, p.identifier AS issue_project_identifier, \
    rel.sequence_id AS release_sequence_id, rel.name AS release_name, rel.version AS release_version, \
    rel.status AS release_status, \
    sess.id AS session_id, sess.title AS session_title, sess.scheduled_at AS session_scheduled_at \
    FROM review_requests rr \
    LEFT JOIN issues i ON i.id = rr.change_issue_id \
    LEFT JOIN projects p ON p.id = i.project_id \
    LEFT JOIN releases rel ON rel.id = rr.release_id \
    LEFT JOIN LATERAL ( \
        SELECT s.id, s.title, s.scheduled_at FROM review_session_items rsi \
        JOIN review_sessions s ON s.id = rsi.session_id \
        WHERE rsi.review_request_id = rr.id AND rsi.deleted_at IS NULL \
          AND s.status = 'scheduled' AND s.deleted_at IS NULL \
        ORDER BY s.scheduled_at ASC LIMIT 1 \
    ) sess ON true";

pub(crate) fn request_json(row: &ReviewRequestRow) -> Value {
    let subject = if row.board_type == "tcb" {
        json!({
            "kind": "change",
            "id": row.change_issue_id,
            "issue": {
                "id": row.change_issue_id,
                "identifier": row.issue_identifier,
                "name": row.issue_name,
                "project_id": row.issue_project_id,
                "project_identifier": row.issue_project_identifier,
            },
            "release": Value::Null,
        })
    } else {
        json!({
            "kind": "release",
            "id": row.release_id,
            "issue": Value::Null,
            "release": {
                "id": row.release_id,
                "sequence_id": row.release_sequence_id,
                "name": row.release_name,
                "version": row.release_version,
                "status": row.release_status,
            },
        })
    };
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "board_type": row.board_type,
        "status": row.status,
        "change_issue_id": row.change_issue_id,
        "release_id": row.release_id,
        "project_id": row.project_id,
        "submission_note": row.submission_note,
        "submitted_by": row.submitted_by_id,
        "submitted_at": row.submitted_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "subject": subject,
        "session": row.session_id.map(|id| json!({
            "id": id,
            "title": row.session_title,
            "scheduled_at": row.session_scheduled_at,
        })),
    })
}

pub(crate) async fn request_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    request_id: Uuid,
) -> Result<Option<ReviewRequestRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{REQUEST_SELECT} WHERE rr.id = $1 \
         AND rr.workspace_id = (SELECT id FROM workspaces WHERE slug = $2 AND deleted_at IS NULL) \
         AND rr.deleted_at IS NULL"
    ))
    .bind(request_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub(crate) async fn gate_request_read(
    st: &AppState,
    user: Uuid,
    slug: &str,
    row: &ReviewRequestRow,
) -> Result<bool, sqlx::Error> {
    if row.board_type == "tcb" {
        match row.project_id {
            Some(project_id) => gate_project_member(&st.pool, user, slug, project_id).await,
            None => Ok(false),
        }
    } else {
        gate_ws_member(&st.pool, user, slug).await
    }
}

#[derive(Debug, Deserialize)]
pub struct RequestListParams {
    pub board_type: String,
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
    pub change_issue_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
}

/// GET `/api/workspaces/:slug/review-requests/`
pub async fn list_requests(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<RequestListParams>,
) -> R {
    if let Err(e) = validate_enum("board_type", &params.board_type, BOARD_TYPES) {
        return Ok(bad_request(e));
    }
    if let Some(status) = params.status.as_deref() {
        if let Err(e) = validate_enum("status", status, REQUEST_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    if params.board_type == "tcb" {
        let Some(project_id) = params.project_id else {
            return Ok(bad_request("project_id is required for TCB"));
        };
        if !gate_project_member(&st.pool, auth.0, &slug, project_id).await? {
            return Ok(deny());
        }
    } else if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let rows: Vec<ReviewRequestRow> = sqlx::query_as(&format!(
        "{REQUEST_SELECT} WHERE rr.workspace_id = (SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL) \
         AND rr.board_type = $2 AND rr.deleted_at IS NULL \
         AND ($3::uuid IS NULL OR rr.project_id = $3) \
         AND ($4::text IS NULL OR rr.status = $4) \
         AND ($5::uuid IS NULL OR rr.change_issue_id = $5) \
         AND ($6::uuid IS NULL OR rr.release_id = $6) \
         ORDER BY rr.submitted_at DESC"
    ))
    .bind(&slug)
    .bind(&params.board_type)
    .bind(params.project_id)
    .bind(&params.status)
    .bind(params.change_issue_id)
    .bind(params.release_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(request_json).collect())),
    ))
}

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub board_type: Option<String>,
    pub change_issue_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
    pub submission_note: Option<String>,
}

/// POST `/api/workspaces/:slug/review-requests/`
pub async fn submit_request(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<SubmitRequest>,
) -> R {
    let board = body.board_type.clone().unwrap_or_default();
    if let Err(e) = validate_enum("board_type", &board, BOARD_TYPES) {
        return Ok(bad_request(e));
    }
    let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let note = body.submission_note.clone().unwrap_or_default().trim().to_string();

    let (project_id, change_issue_id, release_id) = if board == "tcb" {
        let Some(issue_id) = body.change_issue_id else {
            return Ok(bad_request("change_issue_id is required"));
        };
        let issue: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT i.project_id, p.workspace_id FROM issues i JOIN projects p ON p.id = i.project_id \
             WHERE i.id = $1 AND i.deleted_at IS NULL AND p.deleted_at IS NULL",
        )
        .bind(issue_id)
        .fetch_optional(&st.pool)
        .await?;
        let Some((issue_project_id, issue_workspace_id)) = issue else {
            return Ok(bad_request("Invalid change_issue_id - object does not exist."));
        };
        if issue_workspace_id != workspace_id {
            return Ok(bad_request("Invalid change_issue_id - object does not exist."));
        }
        if !gate_project_member(&st.pool, auth.0, &slug, issue_project_id).await? {
            return Ok(deny());
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_requests WHERE board_type = 'tcb' \
             AND change_issue_id = $1 AND status IN ('pending', 'scheduled') AND deleted_at IS NULL)",
        )
        .bind(issue_id)
        .fetch_one(&st.pool)
        .await?;
        if active {
            return Ok(bad_request(
                "An active review request already exists for this subject",
            ));
        }
        (Some(issue_project_id), Some(issue_id), None)
    } else {
        let Some(release_id) = body.release_id else {
            return Ok(bad_request("release_id is required"));
        };
        if release_in_workspace(&st.pool, &slug, release_id).await?.is_none() {
            return Ok(bad_request("Invalid release_id - object does not exist."));
        }
        if !gate_ws_member(&st.pool, auth.0, &slug).await? {
            return Ok(deny());
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_requests WHERE board_type = 'rcb' \
             AND release_id = $1 AND status IN ('pending', 'scheduled') AND deleted_at IS NULL)",
        )
        .bind(release_id)
        .fetch_one(&st.pool)
        .await?;
        if active {
            return Ok(bad_request(
                "An active review request already exists for this subject",
            ));
        }
        (None, None, Some(release_id))
    };

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO review_requests (id, workspace_id, board_type, change_issue_id, release_id, \
         project_id, status, submission_note, submitted_by_id, submitted_at, created_at, updated_at, \
         created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, now(), now(), now(), $8, $8)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(&board)
    .bind(change_issue_id)
    .bind(release_id)
    .bind(project_id)
    .bind(&note)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    let row = request_in_workspace(&st.pool, &slug, id)
        .await?
        .expect("request just inserted");
    Ok((StatusCode::CREATED, Json(request_json(&row))))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RequestHistoryRow {
    pub session_id: Uuid,
    pub title: String,
    pub scheduled_at: DateTime<Utc>,
    pub session_status: String,
    pub outcome: Option<String>,
    pub outcome_note: String,
    pub decided_at: Option<DateTime<Utc>>,
}

/// GET `/api/workspaces/:slug/review-requests/:request_id/`
pub async fn request_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, request_id)): Path<(String, Uuid)>,
) -> R {
    let Some(row) = request_in_workspace(&st.pool, &slug, request_id).await? else {
        return Ok(missing());
    };
    if !gate_request_read(&st, auth.0, &slug, &row).await? {
        return Ok(deny());
    }
    let history: Vec<RequestHistoryRow> = sqlx::query_as(
        "SELECT s.id AS session_id, s.title, s.scheduled_at, s.status AS session_status, \
         i.outcome, i.outcome_note, i.decided_at \
         FROM review_session_items i JOIN review_sessions s ON s.id = i.session_id \
         WHERE i.review_request_id = $1 AND i.deleted_at IS NULL \
         ORDER BY s.scheduled_at ASC",
    )
    .bind(request_id)
    .fetch_all(&st.pool)
    .await?;
    let mut value = request_json(&row);
    value["history"] = json!(history
        .iter()
        .map(|h| json!({
            "session_id": h.session_id,
            "title": h.title,
            "scheduled_at": h.scheduled_at,
            "session_status": h.session_status,
            "outcome": h.outcome,
            "outcome_note": h.outcome_note,
            "decided_at": h.decided_at,
        }))
        .collect::<Vec<_>>());
    Ok((StatusCode::OK, Json(value)))
}

/// POST `/api/workspaces/:slug/review-requests/:request_id/withdraw/`
pub async fn withdraw_request(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, request_id)): Path<(String, Uuid)>,
) -> R {
    let Some(row) = request_in_workspace(&st.pool, &slug, request_id).await? else {
        return Ok(missing());
    };
    if !gate_request_read(&st, auth.0, &slug, &row).await? {
        return Ok(deny());
    }
    let is_submitter = row.submitted_by_id == Some(auth.0);
    let is_admin = if row.board_type == "tcb" {
        match row.project_id {
            Some(project_id) => gate_project_admin(&st.pool, auth.0, &slug, project_id).await?,
            None => false,
        }
    } else {
        gate_ws_admin(&st.pool, auth.0, &slug).await?
    };
    if !is_submitter && !is_admin {
        return Ok(deny());
    }
    if row.status != "pending" && row.status != "scheduled" {
        return Ok(bad_request(
            "Only pending or scheduled requests can be withdrawn",
        ));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE review_session_items i SET deleted_at = now(), updated_at = now() \
         FROM review_sessions s \
         WHERE i.session_id = s.id AND i.review_request_id = $1 \
         AND s.status = 'scheduled' AND i.deleted_at IS NULL",
    )
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE review_requests SET status = 'withdrawn', updated_at = now(), updated_by_id = $1 \
         WHERE id = $2 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = request_in_workspace(&st.pool, &slug, request_id)
        .await?
        .expect("request just withdrawn");
    Ok((StatusCode::OK, Json(request_json(&row))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_and_outcome_enums() {
        assert!(validate_enum("board_type", "tcb", BOARD_TYPES).is_ok());
        assert!(validate_enum("board_type", "cab", BOARD_TYPES).is_err());
        assert!(validate_enum("outcome", "approved_with_notes", OUTCOMES).is_ok());
        assert!(validate_enum("outcome", "passed", OUTCOMES).is_err());
        assert!(FINAL_OUTCOMES.contains(&"approved"));
        assert!(!FINAL_OUTCOMES.contains(&"deferred"));
    }
}
