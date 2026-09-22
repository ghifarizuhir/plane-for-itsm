//! Legacy issue PATCH (`PATCH /api/workspaces/:slug/projects/:project_id/issues/:pk/`)
//! — full parity with Django `IssueViewSet.partial_update`
//! (`plane/app/views/issue/base.py:627-713`).
//!
//! Wire contract: ADMIN/MEMBER (or creator) gate → miss 404
//! `{"error": "Issue not found"}` verbatim → serializer validation (400) →
//! 204 empty. Task 1 covers the request surface + validation; writes for
//! scalars, bridges, activities and description versions land in later tasks
//! of this slice.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::issue_common::{
    bad, de_double_opt_f64, de_double_opt_i32, de_double_opt_json, de_double_opt_string,
    de_double_opt_uuid_lax, de_double_opt_uuid_vec_lax, dedupe_ids, fetch_project_member_role,
    is_workspace_admin, parse_date, project_gate_allows, PRIORITIES,
};
use super::work_item::ws_active_member;
use crate::routes::project::deny;
use crate::{middleware::auth::AuthUser, state::AppState};

/// Quoted from `plane/app/views/issue/base.py:659-661`
/// (`IssueViewSet.partial_update`): miss → 404 with this body verbatim.
pub(crate) const ISSUE_PATCH_MISS_MSG: &str = "Issue not found";

/// `PATCH /issues/:pk/` body. Tri-state fields: absent → `None`,
/// explicit `null` → `Some(None)`, value → `Some(Some(_))` — matching
/// Django's `partial=True` + model null flags (see plan table).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PatchIssue {
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub description_html: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_json")]
    pub description: Option<Option<Value>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub priority: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub state_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub parent_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub start_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_string")]
    pub target_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_double_opt_f64")]
    pub sort_order: Option<Option<f64>>,
    #[serde(default, deserialize_with = "de_double_opt_i32")]
    pub point: Option<Option<i32>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub estimate_point: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_lax")]
    pub type_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_vec_lax")]
    pub assignee_ids: Option<Option<Vec<Uuid>>>,
    #[serde(default, deserialize_with = "de_double_opt_uuid_vec_lax")]
    pub label_ids: Option<Option<Vec<Uuid>>>,
    #[serde(default)]
    pub skip_activity: Option<Value>,
}

/// Tri-state date: absent/null → `None`, `""` → `None` (lax, create
/// precedent), valid `%Y-%m-%d` → `Some(date)`.
fn parse_tri_date(raw: &Option<Option<String>>) -> Result<Option<chrono::NaiveDate>, String> {
    match raw {
        None | Some(None) => Ok(None),
        Some(Some(s)) => parse_date(&Some(s.clone())),
    }
}

fn has_value(v: &Option<Option<String>>) -> bool {
    matches!(v, Some(Some(_)))
}

/// Pure validation, Django order (`serializers/issue.py:127-196`).
pub fn validate_patch(body: &PatchIssue) -> Result<(), String> {
    let start = parse_tri_date(&body.start_date)?;
    let target = parse_tri_date(&body.target_date)?;
    // Django compares only when BOTH keys are in `attrs` (`serializers/issue.py:129-134`).
    if has_value(&body.start_date) && has_value(&body.target_date) {
        if let (Some(s), Some(t)) = (start, target) {
            if s > t {
                return Err("Start date cannot exceed target date".to_string());
            }
        }
    }
    if let Some(v) = &body.name {
        let Some(name) = v else {
            return Err("name may not be null".to_string());
        };
        if name.trim().is_empty() {
            return Err("name must not be blank".to_string());
        }
        if name.chars().count() > 255 {
            return Err("name max length 255".to_string());
        }
    }
    if let Some(v) = &body.description_html {
        if v.is_none() {
            return Err("description_html may not be null".to_string());
        }
    }
    if let Some(v) = &body.description {
        if v.is_none() {
            return Err("description may not be null".to_string());
        }
    }
    if let Some(v) = &body.priority {
        let Some(p) = v else {
            return Err("priority may not be null".to_string());
        };
        if !PRIORITIES.contains(&p.as_str()) {
            return Err("Invalid priority".to_string());
        }
    }
    if matches!(body.sort_order, Some(None)) {
        return Err("sort_order may not be null".to_string());
    }
    if let Some(Some(p)) = body.point {
        if !(0..=12).contains(&p) {
            return Err("point must be between 0 and 12".to_string());
        }
    }
    if matches!(body.assignee_ids, Some(None)) {
        return Err("assignee_ids may not be null".to_string());
    }
    if matches!(body.label_ids, Some(None)) {
        return Err("label_ids may not be null".to_string());
    }
    Ok(())
}

fn internal(_e: sqlx::Error) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "Something went wrong please try again later"})),
    )
}

/// DB-backed reference validation; every failure is 400 (strict, #9526).
async fn validate_patch_refs(
    st: &AppState,
    project_id: Uuid,
    body: &PatchIssue,
    assignees: &[Uuid],
    labels: &[Uuid],
) -> Result<(), (StatusCode, Json<Value>)> {
    if !assignees.is_empty() {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) \
             AND is_active = true AND role >= 15 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(assignees)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if n != assignees.len() as i64 {
            return Err(bad("invalid assignee: not a project member"));
        }
    }
    if !labels.is_empty() {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM labels WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(labels)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if n != labels.len() as i64 {
            return Err(bad("invalid label: not in project"));
        }
    }
    if let Some(Some(state_id)) = body.state_id {
        let (ok,): (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2)")
                .bind(state_id)
                .bind(project_id)
                .fetch_one(&st.pool)
                .await
                .map_err(internal)?;
        if !ok {
            return Err(bad("State is not valid please pass a valid state_id"));
        }
    }
    if let Some(Some(parent)) = body.parent_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(parent)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("parent is not valid"));
        }
    }
    if let Some(Some(ep)) = body.estimate_point {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM estimate_points WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(ep)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("estimate_point is not valid"));
        }
    }
    if let Some(Some(t)) = body.type_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(t)
        .fetch_one(&st.pool)
        .await
        .map_err(internal)?;
        if !ok {
            return Err(bad("type_id is not valid"));
        }
    }
    Ok(())
}

pub async fn patch_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `partial_update` (`base.py:627`): `@allow_permission([ADMIN,
    // MEMBER], creator=True, model=Issue)` runs BEFORE the body — gate
    // first (a denied miss is 403, not 404), then the fetch.
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    // Existence uses `get_queryset()` = `issue_objects` (`base.py:628-629`):
    // drafts, archived issues, triage-state issues and archived-project
    // issues all miss with 404 `{"error": "Issue not found"}` verbatim.
    let row: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT i.created_by_id FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false AND (s.id IS NULL OR s.\"group\" != 'triage') AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $2 AND p.archived_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if row.is_none() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": ISSUE_PATCH_MISS_MSG})),
        ));
    }
    if let Err(msg) = validate_patch(&body) {
        return Ok(bad(&msg));
    }
    let assignees = dedupe_ids(&body.assignee_ids.clone().flatten());
    let labels = dedupe_ids(&body.label_ids.clone().flatten());
    if let Err(e) = validate_patch_refs(&st, project_id, &body, &assignees, &labels).await {
        return Ok(e);
    }
    // TEMP (Task 2 replaces this with the full dynamic UPDATE + side effects):
    sqlx::query(
        "UPDATE issues SET name = COALESCE($1, name), description_html = COALESCE($2, description_html), description_json = COALESCE($3::jsonb, description_json), priority = COALESCE($4, priority), updated_at = now() WHERE id = $5 AND project_id = $6 AND deleted_at IS NULL",
    )
    .bind(body.name.clone().flatten())
    .bind(body.description_html.clone().flatten())
    .bind(body.description.clone().flatten())
    .bind(body.priority.clone().flatten())
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
