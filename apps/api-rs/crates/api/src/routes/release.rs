//! Releases: workspace-level bundling of work items for RCB review.
//! Spec: docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md
//! Review engine (requests/sessions) lives in a later phase.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing, ws_role},
    state::AppState,
};

use super::service::{bad_request, deserialize_present, validate_enum};
use super::workflow::validate_name;

/// Allowed `releases.status` values (spec §Model data).
pub const RELEASE_STATUSES: &[&str] =
    &["draft", "planned", "in_review", "approved", "released", "cancelled"];

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// `YYYY-MM-DD`, or `Invalid target_date` for anything else.
pub fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| "Invalid target_date - expected YYYY-MM-DD.".to_string())
}

pub(crate) async fn gate_ws_member(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(ws_role(pool, user, slug).await?.is_some())
}

pub(crate) async fn gate_ws_admin(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(matches!(ws_role(pool, user, slug).await?, Some(20)))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReleaseRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub sequence_id: i64,
    pub name: String,
    pub version: Option<String>,
    pub description_html: String,
    pub status: String,
    pub target_date: Option<NaiveDate>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const RELEASE_SELECT: &str = "SELECT r.id, r.workspace_id, r.sequence_id, r.name, r.version, \
    r.description_html, r.status, r.target_date, r.created_at, r.updated_at, \
    r.created_by_id, r.updated_by_id FROM releases r";

pub(crate) fn release_json(row: &ReleaseRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "sequence_id": row.sequence_id,
        "name": row.name,
        "version": row.version,
        "description_html": row.description_html,
        "status": row.status,
        "target_date": row.target_date,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
        "updated_by": row.updated_by_id,
    })
}

pub(crate) async fn release_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    release_id: Uuid,
) -> Result<Option<ReleaseRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{RELEASE_SELECT} JOIN workspaces w ON w.id = r.workspace_id \
         WHERE r.id = $1 AND w.slug = $2 AND w.deleted_at IS NULL AND r.deleted_at IS NULL"
    ))
    .bind(release_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub target_date_from: Option<NaiveDate>,
    pub target_date_to: Option<NaiveDate>,
}

/// GET `/api/workspaces/:slug/releases/`
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<ListParams>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    if let Some(status) = params.status.as_deref() {
        if let Err(e) = validate_enum("status", status, RELEASE_STATUSES) {
            return Ok(bad_request(e));
        }
    }
    let rows: Vec<ReleaseRow> = sqlx::query_as(&format!(
        "{RELEASE_SELECT} JOIN workspaces w ON w.id = r.workspace_id \
         WHERE w.slug = $1 AND w.deleted_at IS NULL AND r.deleted_at IS NULL \
         AND ($2::text IS NULL OR r.status = $2) \
         AND ($3::date IS NULL OR r.target_date >= $3) \
         AND ($4::date IS NULL OR r.target_date <= $4) \
         ORDER BY r.created_at DESC"
    ))
    .bind(&slug)
    .bind(&params.status)
    .bind(params.target_date_from)
    .bind(params.target_date_to)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(release_json).collect())),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreateRelease {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    pub target_date: Option<NaiveDate>,
}

/// POST `/api/workspaces/:slug/releases/`
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<CreateRelease>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let name = match validate_name(body.name.as_deref().unwrap_or(""), "name") {
        Ok(name) => name,
        Err(e) => return Ok(bad_request(e)),
    };
    let version = match body.version.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(v) if v.chars().count() > 100 => {
            return Ok(bad_request("Ensure version has no more than 100 characters."))
        }
        Some(v) => Some(v.to_string()),
    };
    let status = body.status.clone().unwrap_or_else(|| "draft".to_string());
    if let Err(e) = validate_enum("status", &status, RELEASE_STATUSES) {
        return Ok(bad_request(e));
    }
    let id = Uuid::new_v4();
    let mut tx = st.pool.begin().await?;
    let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(&slug)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM releases WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO releases (id, workspace_id, sequence_id, name, version, description_html, \
         status, target_date, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now(), $9, $9)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(sequence)
    .bind(&name)
    .bind(&version)
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&status)
    .bind(body.target_date)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row: ReleaseRow = sqlx::query_as(&format!("{RELEASE_SELECT} WHERE r.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(release_json(&row))))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReleaseChangeRow {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub project_id: Uuid,
    pub issue_name: String,
    pub issue_identifier: String,
    pub project_identifier: String,
}

fn change_json(row: &ReleaseChangeRow) -> Value {
    json!({
        "id": row.id,
        "issue_id": row.issue_id,
        "project_id": row.project_id,
        "issue_name": row.issue_name,
        "issue_identifier": row.issue_identifier,
        "project_identifier": row.project_identifier,
    })
}

pub(crate) const CHANGE_SELECT: &str = "SELECT rc.id, rc.issue_id, rc.project_id, \
    i.name AS issue_name, (p.identifier || '-' || i.sequence_id::text) AS issue_identifier, \
    p.identifier AS project_identifier FROM release_changes rc \
    JOIN issues i ON i.id = rc.issue_id JOIN projects p ON p.id = rc.project_id";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewRequestRow {
    pub id: Uuid,
    pub board_type: String,
    pub status: String,
    pub submission_note: String,
    pub submitted_by_id: Option<Uuid>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

fn request_json(row: &ReviewRequestRow) -> Value {
    json!({
        "id": row.id,
        "board_type": row.board_type,
        "status": row.status,
        "submission_note": row.submission_note,
        "submitted_by": row.submitted_by_id,
        "submitted_at": row.submitted_at,
    })
}

/// GET `/api/workspaces/:slug/releases/:pk/`
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(row) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    let changes: Vec<ReleaseChangeRow> = sqlx::query_as(&format!(
        "{CHANGE_SELECT} WHERE rc.release_id = $1 AND rc.deleted_at IS NULL ORDER BY rc.created_at ASC"
    ))
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;
    let requests: Vec<ReviewRequestRow> = sqlx::query_as(
        "SELECT id, board_type, status, submission_note, submitted_by_id, submitted_at \
         FROM review_requests WHERE release_id = $1 AND deleted_at IS NULL \
         ORDER BY submitted_at DESC",
    )
    .bind(pk)
    .fetch_all(&st.pool)
    .await?;
    let mut value = release_json(&row);
    value["changes"] = json!(changes.iter().map(change_json).collect::<Vec<_>>());
    value["review_requests"] = json!(requests.iter().map(request_json).collect::<Vec<_>>());
    Ok((StatusCode::OK, Json(value)))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchRelease {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub version: Option<Option<String>>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub target_date: Option<Option<NaiveDate>>,
}

/// PATCH `/api/workspaces/:slug/releases/:pk/`
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<PatchRelease>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(current) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    let name = match body.name.as_deref() {
        Some(raw) => match validate_name(raw, "name") {
            Ok(name) => name,
            Err(e) => return Ok(bad_request(e)),
        },
        None => current.name.clone(),
    };
    let version = match body.version.clone() {
        Some(Some(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() > 100 {
                return Ok(bad_request(
                    "Ensure version has no more than 100 characters.",
                ));
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(None) => None,
        None => current.version.clone(),
    };
    let status = body.status.clone().unwrap_or_else(|| current.status.clone());
    if let Err(e) = validate_enum("status", &status, RELEASE_STATUSES) {
        return Ok(bad_request(e));
    }
    let target_date = match body.target_date {
        Some(value) => value,
        None => current.target_date,
    };
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    sqlx::query(
        "UPDATE releases SET name = $1, version = $2, description_html = $3, status = $4, \
         target_date = $5, updated_at = now(), updated_by_id = $6 \
         WHERE id = $7 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $8 AND deleted_at IS NULL) \
         AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&version)
    .bind(&description_html)
    .bind(&status)
    .bind(target_date)
    .bind(auth.0)
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    let Some(row) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(release_json(&row))))
}

/// DELETE `/api/workspaces/:slug/releases/:pk/`
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> R {
    if !gate_ws_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let Some(current) = release_in_workspace(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    if current.created_by_id != Some(auth.0) && !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM review_requests WHERE release_id = $1 \
         AND status IN ('pending', 'scheduled') AND deleted_at IS NULL)",
    )
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if active {
        return Ok(bad_request("Release has an active review request"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE release_changes SET deleted_at = now(), updated_at = now() \
         WHERE release_id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE releases SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_accepts_iso_and_trims() {
        assert_eq!(
            parse_date(" 2026-10-02 ").unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()
        );
        assert_eq!(
            parse_date("02-10-2026").unwrap_err(),
            "Invalid target_date - expected YYYY-MM-DD."
        );
    }

    #[test]
    fn status_enum_matches_spec() {
        assert!(validate_enum("status", "draft", RELEASE_STATUSES).is_ok());
        assert!(validate_enum("status", "in_review", RELEASE_STATUSES).is_ok());
        assert!(validate_enum("status", "bogus", RELEASE_STATUSES).is_err());
    }
}
