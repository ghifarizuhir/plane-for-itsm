//! Legacy issue PATCH (`PATCH /api/workspaces/:slug/projects/:project_id/issues/:pk/`)
//! — full parity with Django `IssueViewSet.partial_update`
//! (`plane/app/views/issue/base.py:627-713`).
//!
//! Wire contract: ADMIN/MEMBER (or creator) gate → miss 404
//! `{"error": "Issue not found"}` verbatim → serializer validation (400) →
//! 204 empty. This handler writes every scalar field with `Issue.save`'s
//! side effects (`description_stripped`, `completed_at`, `updated_by`) and
//! replaces the assignee/label bridges when their keys are present; it also
//! writes the per-field update activities and `issue_subscribers` rows
//! (description versions land in a later task of this slice).

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::issue_activity_write::{
    insert_activity_row, insert_assignee_activities, insert_subscribers, ActivityCtx,
};
use super::issue_common::{
    bad, de_double_opt_f64, de_double_opt_i32, de_double_opt_json, de_double_opt_string,
    de_double_opt_uuid_lax, de_double_opt_uuid_vec_lax, dedupe_ids, fetch_project_member_role,
    is_workspace_admin, parse_date, project_gate_allows, replace_bridges, resolve_issue_state,
    PRIORITIES,
};
use super::issue_version_write::record_description_version;
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
        // Django validates against `State.objects` = `StateManager`
        // (`db/models/state.py:65-69`, a `SoftDeletionManager` that also
        // excludes `group='triage'`): soft-deleted and triage states 400.
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
             AND deleted_at IS NULL AND \"group\" != 'triage')",
        )
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

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

async fn parent_label(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Option<Uuid>,
) -> Result<String, sqlx::Error> {
    let Some(id) = id else {
        return Ok(String::new());
    };
    let label: Option<String> = sqlx::query_scalar(
        "SELECT p.identifier || '-' || i.sequence_id FROM issues i \
         JOIN projects p ON p.id = i.project_id WHERE i.id = $1 AND i.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(label.unwrap_or_default())
}

/// `(id, name)` when the state exists under Django's `State.objects`
/// (`StateManager`: soft-deleted + triage excluded); identifiers are only
/// written when the row exists (`issue_activities_task.py:205-236`).
async fn state_info(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Option<Uuid>,
    project_id: Uuid,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let Some(id) = id else { return Ok(None) };
    sqlx::query_as(
        "SELECT id, name FROM states WHERE id = $1 AND project_id = $2 \
         AND deleted_at IS NULL AND \"group\" != 'triage'",
    )
    .bind(id)
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await
}

async fn live_label_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_all(&mut **tx)
    .await
}

/// Mirrors the `IssueDetailSerializer` assignee annotation
/// (`base.py:633-648`): live bridge rows whose member still has an active
/// project membership.
async fn live_assignee_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    // Exact `IssueDetailSerializer` annotation parity (`base.py:645-656`):
    // `assignees__member_project__is_active=True` is NOT project-scoped in
    // Django (a member active in ANY project qualifies), and joins use the
    // base manager (no `deleted_at` predicate) — `DISTINCT` mirrors
    // `ArrayAgg(distinct=True)`.
    sqlx::query_scalar(
        "SELECT DISTINCT ia.assignee_id FROM issue_assignees ia \
         JOIN project_members pm ON pm.member_id = ia.assignee_id AND pm.is_active = true \
         WHERE ia.issue_id = $1 AND ia.deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_all(&mut **tx)
    .await
}

async fn names_for(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    column: &str,
    ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let sql = format!("SELECT id, {column} FROM {table} WHERE id = ANY($1)");
    let rows: Vec<(Uuid, String)> = sqlx::query_as(&sql).bind(ids).fetch_all(&mut **tx).await?;
    Ok(rows.into_iter().collect())
}

/// `(value, estimates.type)` for an estimate point id.
async fn estimate_info(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ep.value, e.type FROM estimate_points ep \
         JOIN estimates e ON e.id = ep.estimate_id WHERE ep.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

/// Django's `track_description` merge rule: the issue's latest activity is a
/// `description` row by the same actor → bump its `created_at` instead of
/// inserting. Runs before this request's rows so the DB view matches
/// Django's pre-`bulk_create` lookup.
async fn merge_last_description_activity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    issue_id: Uuid,
    actor: Uuid,
) -> Result<bool, sqlx::Error> {
    let last: Option<(Uuid, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, field, actor_id FROM issue_activities WHERE issue_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((id, Some(field), Some(last_actor))) = last {
        if field == "description" && last_actor == actor {
            sqlx::query("UPDATE issue_activities SET created_at = clock_timestamp() WHERE id = $1")
                .bind(id)
                .execute(&mut **tx)
                .await?;
            return Ok(true);
        }
    }
    Ok(false)
}

async fn write_update_activities(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &ActivityCtx,
    current: &CurrentIssue,
    body: &PatchIssue,
    current_label_ids: &[Uuid],
    current_assignee_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    // description first: Django's merge lookup must see the pre-batch DB.
    if let Some(Some(html)) = &body.description_html {
        if &current.description_html != html {
            let merged = merge_last_description_activity(tx, ctx.issue_id, ctx.actor).await?;
            if !merged {
                insert_activity_row(
                    tx,
                    ctx,
                    "updated",
                    "description",
                    "updated the description to",
                    Some(current.description_html.as_str()),
                    Some(html),
                    None,
                    None,
                )
                .await?;
            }
        }
    }
    if let Some(Some(name)) = &body.name {
        if &current.name != name {
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "name",
                "updated the name to",
                Some(current.name.as_str()),
                Some(name),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = body.parent_id {
        if requested != current.parent_id {
            let old = parent_label(tx, current.parent_id).await?;
            let new = parent_label(tx, requested).await?;
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "parent",
                "updated the parent issue to",
                Some(old.as_str()),
                Some(new.as_str()),
                current.parent_id,
                requested,
            )
            .await?;
        }
    }
    if let Some(Some(priority)) = &body.priority {
        if &current.priority != priority {
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "priority",
                "updated the priority to",
                Some(current.priority.as_str()),
                Some(priority),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = body.state_id {
        if requested != current.state_id {
            let old = state_info(tx, current.state_id, ctx.project_id).await?;
            let new = state_info(tx, requested, ctx.project_id).await?;
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "state",
                "updated the state to",
                old.as_ref().map(|(_, name)| name.as_str()),
                new.as_ref().map(|(_, name)| name.as_str()),
                old.as_ref().map(|(id, _)| *id),
                new.as_ref().map(|(id, _)| *id),
            )
            .await?;
        }
    }
    if let Some(requested) = &body.target_date {
        let new = parse_tri_date(&body.target_date).unwrap_or(None);
        if new != current.target_date {
            let old_value = current
                .target_date
                .map(|d| d.to_string())
                .unwrap_or_default();
            let new_value = requested.clone().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "target_date",
                "updated the target date to",
                Some(old_value.as_str()),
                Some(new_value.as_str()),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = &body.start_date {
        let new = parse_tri_date(&body.start_date).unwrap_or(None);
        if new != current.start_date {
            let old_value = current
                .start_date
                .map(|d| d.to_string())
                .unwrap_or_default();
            let new_value = requested.clone().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "start_date",
                "updated the start date to ",
                Some(old_value.as_str()),
                Some(new_value.as_str()),
                None,
                None,
            )
            .await?;
        }
    }
    if let Some(Some(requested)) = &body.label_ids {
        let requested: std::collections::HashSet<Uuid> = requested.iter().copied().collect();
        let current_set: std::collections::HashSet<Uuid> =
            current_label_ids.iter().copied().collect();
        let added: Vec<Uuid> = requested.difference(&current_set).copied().collect();
        let dropped: Vec<Uuid> = current_set.difference(&requested).copied().collect();
        let names = names_for(
            tx,
            "labels",
            "name",
            &[added.clone(), dropped.clone()].concat(),
        )
        .await?;
        for id in added {
            let name = names.get(&id).cloned().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "labels",
                "added label ",
                Some(""),
                Some(name.as_str()),
                None,
                Some(id),
            )
            .await?;
        }
        for id in dropped {
            let name = names.get(&id).cloned().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "labels",
                "removed label ",
                Some(name.as_str()),
                Some(""),
                Some(id),
                None,
            )
            .await?;
        }
    }
    if let Some(Some(requested)) = &body.assignee_ids {
        let requested: std::collections::HashSet<Uuid> = requested.iter().copied().collect();
        let current_set: std::collections::HashSet<Uuid> =
            current_assignee_ids.iter().copied().collect();
        let added: Vec<Uuid> = requested.difference(&current_set).copied().collect();
        let dropped: Vec<Uuid> = current_set.difference(&requested).copied().collect();
        // Reuse the create-path "added assignee" writer (identical row shape:
        // `old_value=''`, `new_value=display_name`, `new_identifier=user`).
        insert_assignee_activities(
            tx,
            ctx.issue_id,
            ctx.project_id,
            ctx.workspace_id,
            ctx.actor,
            &added,
            ctx.epoch,
        )
        .await?;
        insert_subscribers(tx, ctx.issue_id, ctx.project_id, ctx.workspace_id, &added).await?;
        let names = names_for(tx, "users", "display_name", &dropped).await?;
        for id in dropped {
            let name = names.get(&id).cloned().unwrap_or_default();
            insert_activity_row(
                tx,
                ctx,
                "updated",
                "assignees",
                "removed assignee ",
                Some(name.as_str()),
                Some(""),
                Some(id),
                None,
            )
            .await?;
        }
    }
    if let Some(requested) = body.estimate_point {
        if requested != current.estimate_point_id {
            // `track_estimate_points` NPEs when the new estimate is None
            // (Django loses the whole batch); skip the row (deviation 6).
            if let Some(new_id) = requested {
                let old = match current.estimate_point_id {
                    Some(id) => estimate_info(tx, id).await?,
                    None => None,
                };
                let new = estimate_info(tx, new_id).await?;
                let (old_value, new_value, field) = match new {
                    Some((new_value, estimate_type)) => (
                        old.as_ref().map(|(v, _)| v.clone()),
                        Some(new_value),
                        format!("estimate_{estimate_type}"),
                    ),
                    None => (None, None, String::new()),
                };
                if !field.is_empty() {
                    insert_activity_row(
                        tx,
                        ctx,
                        "updated",
                        &field,
                        "updated the estimate point to ",
                        old_value.as_deref(),
                        new_value.as_deref(),
                        current.estimate_point_id,
                        Some(new_id),
                    )
                    .await?;
                }
            }
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
    // The row is also the snapshot `Issue.save` reads (`_state.adding ==
    // false`): `has_changed("state_id")` compares against it.
    let current: Option<CurrentIssue> = sqlx::query_as(
        "SELECT i.name, i.description_html, i.description_json, i.priority, i.state_id, i.parent_id, \
         i.start_date, i.target_date, i.estimate_point_id, i.created_by_id \
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

    // --- Update activities (`update_issue_activity`,
    // `issue_activities_task.py:594-638`): diffed against the pre-update
    // snapshot and the pre-request bridge sets. ---
    let skip_activity = body.skip_activity.as_ref().map(is_truthy).unwrap_or(false)
        && body.description_html.is_some();
    if !skip_activity {
        let workspace_id: Uuid =
            sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
                .bind(project_id)
                .fetch_one(&mut *tx)
                .await?;
        let ctx = ActivityCtx {
            issue_id: pk,
            project_id,
            workspace_id,
            actor: auth.0,
            epoch: chrono::Utc::now().timestamp() as f64,
        };
        let label_ids_current = live_label_ids(&mut tx, pk).await?;
        let assignee_ids_current = live_assignee_ids(&mut tx, pk).await?;
        write_update_activities(
            &mut tx,
            &ctx,
            &current,
            &body,
            &label_ids_current,
            &assignee_ids_current,
        )
        .await?;
        // `issue_description_version_task.delay` (`base.py:700-707`).
        let stored_html = sanitized_html
            .as_deref()
            .unwrap_or(&current.description_html);
        if sanitized_html.is_some() && stored_html != current.description_html {
            let description_json = body
                .description
                .clone()
                .flatten()
                .unwrap_or_else(|| current.description_json.clone());
            record_description_version(
                &mut tx,
                pk,
                project_id,
                workspace_id,
                auth.0,
                current.created_by_id,
                Some(auth.0),
                stored_html,
                &description_json,
            )
            .await?;
        }
    }
    // --- Bridge writes ---
    // Django `IssueCreateSerializer.update` (`serializers/issue.py:276-320`):
    // present keys replace the whole bridge set.
    if matches!(body.assignee_ids, Some(Some(_))) {
        replace_bridges(&mut tx, pk, project_id, auth.0, Some(&assignees), None).await?;
    }
    if matches!(body.label_ids, Some(Some(_))) {
        replace_bridges(&mut tx, pk, project_id, auth.0, None, Some(&labels)).await?;
    }
    // --- End bridge writes ---

    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
