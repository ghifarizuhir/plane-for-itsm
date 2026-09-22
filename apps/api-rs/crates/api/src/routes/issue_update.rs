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
    is_workspace_admin, parse_date, project_gate_allows, resolve_issue_state, PRIORITIES,
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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CurrentIssue {
    pub name: String,
    pub description_html: String,
    pub description_json: Value,
    pub priority: String,
    pub state_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sort_order: f64,
    pub point: Option<i32>,
    pub estimate_point_id: Option<Uuid>,
    pub created_by_id: Option<Uuid>,
}

enum BindValue {
    Text(Option<String>),
    Json(Value),
    Date(Option<chrono::NaiveDate>),
    Uuid(Option<Uuid>),
    Int(Option<i32>),
    Float(f64),
}

fn add(sets: &mut Vec<String>, values: &mut Vec<BindValue>, col: &str, v: BindValue) {
    values.push(v);
    sets.push(format!("{col} = ${}", values.len()));
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
    // The row is also the snapshot `Issue.save` reads (`_state.adding ==
    // false`): `has_changed("state_id")` compares against it.
    let current: Option<CurrentIssue> = sqlx::query_as(
        "SELECT i.name, i.description_html, i.description_json, i.priority, i.state_id, i.parent_id, \
         i.start_date, i.target_date, i.sort_order, i.point, i.estimate_point_id, i.created_by_id \
         FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND (s.id IS NULL OR s.\"group\" != 'triage') \
         AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $2 AND p.archived_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": ISSUE_PATCH_MISS_MSG})),
        ));
    };
    if let Err(msg) = validate_patch(&body) {
        return Ok(bad(&msg));
    }
    let assignees = dedupe_ids(&body.assignee_ids.clone().flatten());
    let labels = dedupe_ids(&body.label_ids.clone().flatten());
    if let Err(e) = validate_patch_refs(&st, project_id, &body, &assignees, &labels).await {
        return Ok(e);
    }
    // Django sanitizes `description_html` in `IssueCreateSerializer.validate`
    // (`serializers/issue.py:135-143`); failures are 400
    // `{"error": "html content is not valid"}`.
    let sanitized_html: Option<String> = match &body.description_html {
        Some(Some(h)) => match super::page::clean_description_html(h) {
            Ok(v) => Some(v),
            Err(_) => return Ok(bad("html content is not valid")),
        },
        _ => None,
    };
    let start_date = match parse_tri_date(&body.start_date) {
        Ok(v) => v,
        Err(e) => return Ok(bad(&e)),
    };
    let target_date = match parse_tri_date(&body.target_date) {
        Ok(v) => v,
        Err(e) => return Ok(bad(&e)),
    };
    // `Issue._ensure_default_state` (`db/models/issue.py:180-236`).
    let new_state_id = match body.state_id {
        Some(Some(id)) => Some(id),
        Some(None) => resolve_issue_state(&st.pool, project_id, None).await?,
        None => match current.state_id {
            Some(id) => Some(id),
            None => resolve_issue_state(&st.pool, project_id, None).await?,
        },
    };
    let state_changed = new_state_id != current.state_id;
    let new_state_group: Option<String> = if state_changed {
        match new_state_id {
            Some(id) => {
                sqlx::query_scalar("SELECT \"group\" FROM states WHERE id = $1 AND project_id = $2")
                    .bind(id)
                    .bind(project_id)
                    .fetch_optional(&st.pool)
                    .await?
            }
            None => None,
        }
    } else {
        None
    };

    let mut tx = st.pool.begin().await?;
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<BindValue> = Vec::new();
    // `BaseModel.save` on update (`db/models/base.py:43-46`).
    add(
        &mut sets,
        &mut values,
        "updated_by_id",
        BindValue::Uuid(Some(auth.0)),
    );
    sets.push("updated_at = now()".to_string());
    // `Issue.save` recomputes `description_stripped` on every update
    // (`db/models/issue.py:212-217`).
    let effective_html = sanitized_html
        .as_deref()
        .unwrap_or(&current.description_html);
    let stripped = if effective_html.is_empty() {
        None
    } else {
        Some(super::page::strip_tags_text(effective_html))
    };
    add(
        &mut sets,
        &mut values,
        "description_stripped",
        BindValue::Text(stripped),
    );
    if let Some(Some(v)) = body.name.clone() {
        add(&mut sets, &mut values, "name", BindValue::Text(Some(v)));
    }
    if let Some(v) = sanitized_html.clone() {
        add(
            &mut sets,
            &mut values,
            "description_html",
            BindValue::Text(Some(v)),
        );
    }
    if let Some(Some(v)) = body.description.clone() {
        add(
            &mut sets,
            &mut values,
            "description_json",
            BindValue::Json(v),
        );
    }
    if let Some(Some(v)) = body.priority.clone() {
        add(&mut sets, &mut values, "priority", BindValue::Text(Some(v)));
    }
    if body.start_date.is_some() {
        add(
            &mut sets,
            &mut values,
            "start_date",
            BindValue::Date(start_date),
        );
    }
    if body.target_date.is_some() {
        add(
            &mut sets,
            &mut values,
            "target_date",
            BindValue::Date(target_date),
        );
    }
    if let Some(Some(v)) = body.sort_order {
        add(&mut sets, &mut values, "sort_order", BindValue::Float(v));
    }
    if body.point.is_some() {
        add(
            &mut sets,
            &mut values,
            "point",
            BindValue::Int(body.point.flatten()),
        );
    }
    if body.parent_id.is_some() {
        add(
            &mut sets,
            &mut values,
            "parent_id",
            BindValue::Uuid(body.parent_id.flatten()),
        );
    }
    if body.estimate_point.is_some() {
        add(
            &mut sets,
            &mut values,
            "estimate_point_id",
            BindValue::Uuid(body.estimate_point.flatten()),
        );
    }
    if body.type_id.is_some() {
        add(
            &mut sets,
            &mut values,
            "type_id",
            BindValue::Uuid(body.type_id.flatten()),
        );
    }
    if state_changed {
        add(
            &mut sets,
            &mut values,
            "state_id",
            BindValue::Uuid(new_state_id),
        );
        if new_state_id.is_some() {
            // `_sync_completed_at` (`db/models/issue.py:240-256`).
            if new_state_group.as_deref() == Some("completed") {
                sets.push("completed_at = now()".to_string());
            } else {
                sets.push("completed_at = NULL".to_string());
            }
        }
    }

    let pk_pos = values.len() + 1;
    let project_pos = values.len() + 2;
    let sql = format!(
        "UPDATE issues SET {} WHERE id = ${pk_pos} AND project_id = ${project_pos} AND deleted_at IS NULL",
        sets.join(", ")
    );
    let mut q = sqlx::query(&sql);
    for v in &values {
        q = match v {
            BindValue::Text(s) => q.bind(s.clone()),
            BindValue::Json(j) => q.bind(j.clone()),
            BindValue::Date(d) => q.bind(*d),
            BindValue::Uuid(u) => q.bind(*u),
            BindValue::Int(n) => q.bind(*n),
            BindValue::Float(f) => q.bind(*f),
        };
    }
    q.bind(pk).bind(project_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
