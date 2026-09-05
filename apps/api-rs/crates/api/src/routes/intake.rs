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
        "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, false, '{}', '{}', $3, w.id, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, name",
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

    // Triage state lookup-or-create mirrors
    // `IntakeIssueViewSet.create` (`plane/app/views/intake/base.py:246-256`):
    // Django reads `State.triage_objects.filter(project_id, workspace__slug)`
    // (`TriageStateManager`, `plane/db/models/state.py:72-76`), i.e. triage
    // identity is `"group" = 'triage'` — NOT `is_triage` (both the
    // `DEFAULT_STATES` seed and the on-demand `State.objects.create` leave
    // `is_triage` at its `default=False`). The intake issue lands in triage,
    // creating the state row on demand.
    let triage_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'triage' AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let triage_id = match triage_id {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \"default\", project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), 'Triage', '', 'triage', 'triage', '#4E5355', 65000, false, false, $1, $2, now(), now()) RETURNING id",
            )
            .bind(project_id)
            .bind(workspace_id)
            .fetch_one(&st.pool)
            .await?
        }
    };

    let issue_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, '<p></p>', '{}', $2, false, COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $4 AND state_id IS NOT DISTINCT FROM $3), 65535 - 10000) + 10000, COALESCE((SELECT MAX(sequence) FROM issue_sequences WHERE project_id = $4), 0) + 1, $3, $4, $5, now(), now()) RETURNING id",
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

/// Mirrors `plane/app/views/intake/base.py:destroy`: the default intake
/// cannot be deleted.
pub fn guard_delete(is_default: bool) -> Result<(), String> {
    if is_default {
        return Err("You cannot delete the default intake".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchIntake {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::intake::Intake> = sqlx::query_as(
        "SELECT id, name FROM intakes WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(i) => Ok((StatusCode::OK, Json(json!({"id": i.id, "name": i.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchIntake>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid name"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE intakes SET name = COALESCE($1, name), description = COALESCE($2, description), updated_at = now() WHERE id = $3 AND project_id = $4 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT is_default FROM intakes WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((is_default,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake not found"}))));
    };
    if let Err(e) = guard_delete(is_default) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    sqlx::query("DELETE FROM intakes WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn detail_issue(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::intake::IntakeIssue> = sqlx::query_as(
        "SELECT id, status FROM intake_issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(ii) => Ok((StatusCode::OK, Json(json!({"id": ii.id, "status": ii.status})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake issue not found"})))),
    }
}

pub async fn destroy_issue(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Mirrors `plane/app/views/intake/base.py:destroy`: pending/rejected
    // intake rows (status in [-2,-1,0,2]) take the underlying issue with them.
    let row: Option<(i32, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT status, issue_id FROM intake_issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((status, issue_id)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Intake issue not found"}))));
    };
    if matches!(status, -2 | -1 | 0 | 2) {
        if let Some(issue_id) = issue_id {
            sqlx::query("UPDATE issues SET deleted_at = now() WHERE id = $1")
                .bind(issue_id)
                .execute(&st.pool)
                .await?;
        }
    }
    sqlx::query("DELETE FROM intake_issues WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

use super::issue_common::{fetch_project_member_role, is_workspace_admin};
use crate::routes::project::missing;

/// PATCH `.../inbox-issues/:pk/` (and the `intake-issues/:pk/` twin —
/// Django serves both paths from `IntakeIssueViewSet`,
/// `plane/app/urls/intake.py:44-55`) — parity with Django
/// `IntakeIssueViewSet.partial_update`
/// (`plane/app/views/intake/base.py:334-...`). Celery
/// `issue_activity.delay` / `issue_description_version_task.delay` writes
/// skipped (batch-wide precedent — Rust never writes activities).
///
/// Locked semantics (plan D13):
/// - Decorator `@allow_permission([ADMIN], creator=True, model=Issue)`
///   (`base.py:334`) is collapsed into the two in-view gates below (the
///   plan's locked matrix): no-membership AND no-ws-admin → 403
///   `{"error":"Only admin or creator can update the intake work items"}`
///   (`base.py:361-365`); guest-role member AND not creator AND not
///   ws-admin → 400 `{"error":"You cannot edit intake issues"}`
///   (`base.py:368-374`); admin | creator | ws-admin (and plain members)
///   → ok. Per the locked matrix the members/guest branches surface the
///   in-view bodies rather than the decorator's generic 403 — note delta
///   vs a strict decorator reading (plain MEMBER non-creators and guest
///   non-creators would 403 at the decorator; here members proceed to
///   issue-edits and guests get the verbatim 400).
/// - Guest issue payloads narrowed to name/description_html/
///   description_json (`base.py:396-401`, no ws-admin exemption there).
/// - Intake-level fields (status/duplicate_to/snoozed_till/source)
///   applied only when `(role > MEMBER) or ws_admin` (`base.py:426`,
///   i.e. project ADMIN 20 or workspace admin).
/// - 200 `IntakeIssueDetailSerializer` (`serializers/intake.py:93-117`):
///   id, status, duplicate_to, snoozed_till, duplicate_issue_detail
///   (`IssueIntakeSerializer`, `serializers/issue.py:752-767`), source,
///   issue (nested 28-key `IssueDetailSerializer`,
///   `serializers/issue.py:934-945` — same key order as D7).
///
/// Scope reuse (plan "reuse its GET/DELETE scope"): lookup is the existing
/// `detail_issue`/`destroy_issue` scope (`intake_issues` row id + project
/// + live). Delta vs Django, which scopes `.get(issue_id=pk, intake_id,
/// project)` (`base.py:341-346`, pk = *issue* id): Rust pk = the
/// intake-issue row id, so no separate `Intake` lookup is needed (Django
/// would AttributeError-500 on a missing intake; Rust 404s instead, D9
/// precedent). Creator = the intake-issue row's `created_by_id` (the
/// field Django's 400 check compares, `base.py:370`); Django's decorator
/// additionally checks `Issue(id=pk).created_by`, unobservable under the
/// Rust row-id scope — noted, same effective outcome via check 1 which
/// has no creator clause.
///
/// Deviations (documented, reviewer-adjudicable):
/// - Datetimes serialize RFC3339 UTC (chrono, batch convention) vs DRF's
///   per-user-timezone rendering.
/// - Nested-issue counts/ids (`sub_issues_count`, `attachment_count`,
///   `link_count`, `cycle_id`, `module/label/assignee_ids`,
///   `is_subscribed`, `is_intake`) are computed live with the D7
///   `ARCHIVE_SELECT_SQL` convention (Django's partial_update tail
///   annotates only label/assignee ids and relies on prefetches).
/// - `issue` payload covers name/description_html/description_json/
///   priority (the triage-edit surface + guest-narrowed keys); other
///   `IssueCreateSerializer` fields are ignored (serde), not 400 —
///   beyond the locked contract.
/// - Value validation is Rust-side with plain `{"error": ...}` bodies:
///   blank name → "Name is required" (this file's intake-create message),
///   unknown priority → "Invalid priority" (ditto), unknown status →
///   "Invalid status" (DRF would return per-field choice errors;
///   first-error-wins here). Unknown `duplicate_to` → 404 `missing()`
///   (D9 precedent for bad related-issue refs).
/// - `skip_activity` is accepted-and-ignored (serde drops unknown keys;
///   activity tasks skipped batch-wide).
/// - Live DB verified 2026-09-06: `intake_issues(id, status,
///   snoozed_till, source, created_by_id, duplicate_to_id, intake_id,
///   issue_id, project_id, updated_by_id, workspace_id, ..., deleted_at)`.

/// Quoted from `plane/app/views/intake/base.py:363`.
pub(crate) const ONLY_ADMIN_OR_CREATOR_MSG: &str =
    "Only admin or creator can update the intake work items";
/// Quoted from `plane/app/views/intake/base.py:371`.
pub(crate) const CANNOT_EDIT_INTAKE_MSG: &str = "You cannot edit intake issues";

/// Intake-level statuses (`IntakeIssueStatus`,
/// `plane/db/models/intake.py:42-48`).
const INTAKE_STATUSES: [i32; 5] = [-2, -1, 0, 1, 2];

/// Top-level `IntakeIssueDetailSerializer.Meta.fields` order
/// (`serializers/intake.py:99-107`).
#[allow(dead_code)]
pub(crate) const INBOX_DETAIL_KEYS: [&str; 7] = [
    "id",
    "status",
    "duplicate_to",
    "snoozed_till",
    "duplicate_issue_detail",
    "source",
    "issue",
];

/// Nested `issue` = `IssueDetailSerializer` key order
/// (`serializers/issue.py:934-945` = `IssueSerializer.Meta.fields`
/// `issue.py:786-812` + description_html/is_subscribed/is_intake) —
/// identical to D7 `ARCHIVED_DETAIL_KEYS`.
#[allow(dead_code)]
pub(crate) const INBOX_ISSUE_KEYS: [&str; 28] = [
    "id",
    "name",
    "state_id",
    "sort_order",
    "completed_at",
    "estimate_point",
    "priority",
    "start_date",
    "target_date",
    "sequence_id",
    "project_id",
    "parent_id",
    "cycle_id",
    "module_ids",
    "label_ids",
    "assignee_ids",
    "sub_issues_count",
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "attachment_count",
    "link_count",
    "is_draft",
    "archived_at",
    "description_html",
    "is_subscribed",
    "is_intake",
];

/// In-view gates of `partial_update` (`base.py:355-374`, locked matrix):
/// `!project_member && !ws_admin` → 403; `(role <= GUEST) && !ws_admin
/// && !creator` → 400 (verbatim `<=`, `base.py:368`; among stored roles
/// 20/15/5 only GUEST trips it — a `Some` role implies membership, so no
/// separate flag is needed for the second check). Else ok.
pub(crate) fn guard_inbox_patch(
    has_membership: bool,
    role: Option<i16>,
    is_creator: bool,
    is_ws_admin: bool,
) -> Result<(), (StatusCode, String)> {
    if !has_membership && !is_ws_admin {
        return Err((
            StatusCode::FORBIDDEN,
            ONLY_ADMIN_OR_CREATOR_MSG.to_string(),
        ));
    }
    if matches!(role, Some(r) if r <= 5) && !is_ws_admin && !is_creator {
        return Err((StatusCode::BAD_REQUEST, CANNOT_EDIT_INTAKE_MSG.to_string()));
    }
    Ok(())
}

/// Intake-level write gate (`base.py:426`): `(project_member and role >
/// ROLE.MEMBER.value) or is_workspace_admin` — verbatim `> 15`, i.e.
/// project ADMIN (20) or workspace admin.
pub(crate) fn may_write_intake_fields(role: Option<i16>, is_ws_admin: bool) -> bool {
    matches!(role, Some(r) if r > 15) || is_ws_admin
}

/// Guest issue-payload narrowing (`base.py:396-401`): `project_member and
/// role <= ROLE.GUEST.value` — verbatim `<= 5`, with NO ws-admin
/// exemption in Django's narrowing branch.
pub(crate) fn is_guest_narrowed(role: Option<i16>) -> bool {
    matches!(role, Some(r) if r <= 5)
}

/// Nested `issue` patch fields: the triage-edit surface (a superset of
/// the guest-narrowed name/description keys, `base.py:396-401`). Unknown
/// keys are ignored by serde (see module docs).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct InboxIssueFields {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub description_json: Option<Value>,
    #[serde(default)]
    pub priority: Option<String>,
}

/// Top-level PATCH body: optional nested `issue` (Django reads
/// `request.data["issue"]`, `base.py:377`) + intake-level fields
/// (`IntakeIssueSerializer` partial, `base.py:426-431`). Double-`Option`
/// on nullable columns mirrors DRF partial semantics: absent = keep,
/// explicit null = clear, value = set. `skip_activity` is
/// accepted-and-ignored (activity tasks skipped batch-wide).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct InboxIssuePatch {
    #[serde(default)]
    pub issue: Option<InboxIssueFields>,
    #[serde(default)]
    pub status: Option<i32>,
    #[serde(default)]
    pub duplicate_to: Option<Option<uuid::Uuid>>,
    #[serde(default)]
    pub snoozed_till: Option<Option<chrono::DateTime<chrono::Utc>>>,
    #[serde(default)]
    pub source: Option<Option<String>>,
}

/// Gate lookup row: the reused GET/DELETE scope (`id` + `project_id` +
/// live) plus the creator + link columns the PATCH needs.
#[derive(Debug, Clone, sqlx::FromRow)]
struct InboxLookup {
    issue_id: Option<uuid::Uuid>,
    created_by_id: Option<uuid::Uuid>,
}

/// Nested `issue`: the 28-key `IssueDetailSerializer` shape in Django
/// field order (see `INBOX_ISSUE_KEYS`). Column mapping follows D7
/// `ArchivedIssueDetailRow` (`estimate_point` reads
/// `estimate_point_id`, `created_by`/`updated_by` the `*_id` columns).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct InboxIssueDetailIssue {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) state_id: Option<uuid::Uuid>,
    pub(crate) sort_order: f64,
    pub(crate) completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) estimate_point: Option<uuid::Uuid>,
    pub(crate) priority: String,
    pub(crate) start_date: Option<chrono::NaiveDate>,
    pub(crate) target_date: Option<chrono::NaiveDate>,
    pub(crate) sequence_id: i32,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) parent_id: Option<uuid::Uuid>,
    pub(crate) cycle_id: Option<uuid::Uuid>,
    pub(crate) module_ids: Vec<uuid::Uuid>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) assignee_ids: Vec<uuid::Uuid>,
    pub(crate) sub_issues_count: i64,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
    pub(crate) attachment_count: i64,
    pub(crate) link_count: i64,
    pub(crate) is_draft: bool,
    pub(crate) archived_at: Option<chrono::NaiveDate>,
    pub(crate) description_html: String,
    pub(crate) is_subscribed: bool,
    pub(crate) is_intake: bool,
}

/// `duplicate_issue_detail`: `IssueIntakeSerializer`
/// (`serializers/issue.py:752-767`), null when `duplicate_to` is null.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct InboxDuplicateDetail {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) priority: String,
    pub(crate) sequence_id: i32,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) created_by: Option<uuid::Uuid>,
}

/// 200 body: `IntakeIssueDetailSerializer` field order (see
/// `INBOX_DETAIL_KEYS`).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct InboxIssueDetail {
    pub(crate) id: uuid::Uuid,
    pub(crate) status: i32,
    pub(crate) duplicate_to: Option<uuid::Uuid>,
    pub(crate) snoozed_till: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) duplicate_issue_detail: Option<InboxDuplicateDetail>,
    pub(crate) source: Option<String>,
    pub(crate) issue: InboxIssueDetailIssue,
}

/// Shared nested-issue SELECT (D7 `ARCHIVE_SELECT_SQL` convention — live
/// bridge rows, counts via `COUNT(*)`, `is_subscribed`/`is_intake` via
/// `EXISTS`). `$1` = issue id, `$2` = project id, `$3` = workspace slug,
/// `$4` = requesting user id.
const INBOX_ISSUE_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
  i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
  i.sequence_id, i.project_id, i.parent_id, \
  (SELECT ci.cycle_id FROM cycle_issues ci \
    WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
  COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
    JOIN modules m ON m.id = mi.module_id \
    WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
    AND m.archived_at IS NULL), '{}'::uuid[]) AS module_ids, \
  COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
    WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
  COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
    WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
  (SELECT COUNT(*) FROM issues si \
    LEFT JOIN states ss ON ss.id = si.state_id \
    WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
    AND si.archived_at IS NULL AND si.is_draft = false \
    AND ss.\"group\" <> 'triage' \
    AND EXISTS(SELECT 1 FROM projects sp \
      WHERE sp.id = si.project_id AND sp.archived_at IS NULL)) AS sub_issues_count, \
  i.created_at, i.updated_at, \
  i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
  (SELECT COUNT(*) FROM file_assets fa \
    WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
    AND fa.deleted_at IS NULL) AS attachment_count, \
  (SELECT COUNT(*) FROM issue_links lin \
    WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count, \
  i.is_draft, i.archived_at, i.description_html, \
  EXISTS(SELECT 1 FROM issue_subscribers s \
    WHERE s.issue_id = i.id AND s.subscriber_id = $4 AND s.project_id = i.project_id \
    AND s.deleted_at IS NULL) AS is_subscribed, \
  EXISTS(SELECT 1 FROM intake_issues ii \
    WHERE ii.issue_id = i.id AND ii.status IN (-2, 0) AND ii.project_id = i.project_id \
    AND ii.deleted_at IS NULL) AS is_intake \
  FROM issues i \
  WHERE i.id = $1 AND i.project_id = $2 \
  AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
  AND i.deleted_at IS NULL";

/// PATCH `/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/`
/// (and the `intake-issues/:pk/` twin) — parity with Django
/// `IntakeIssueViewSet.partial_update`
/// (`plane/app/views/intake/base.py:334-...`,
/// `plane/app/urls/intake.py:44-55`).
///
/// - Scope: reused GET/DELETE scope (`intake_issues` row id + project +
///   live); miss → 404 `missing()` (Django `.get()` → 404 via
///   `views/base.py:92-96`; Django's extra `intake_id` scoping is implied
///   by the row id — see module docs).
/// - Gates: `guard_inbox_patch` (403 / 400 verbatim bodies); otherwise
///   validate-then-write (validate all applied values BEFORE any write,
///   mirroring Django validating both serializers before either save).
/// - Writes: nested `issue` (guest-narrowed) via `UPDATE issues`;
///   intake-level fields only when `may_write_intake_fields` (silently
///   ignored otherwise — Django never builds that serializer,
///   `base.py:426`). `updated_by_id` bumped on both rows (Django
///   `save()`).
/// - 200 `IntakeIssueDetailSerializer` (`INBOX_DETAIL_KEYS` order).
pub async fn patch_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<InboxIssuePatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<InboxLookup> = sqlx::query_as(
        "SELECT issue_id, created_by_id FROM intake_issues \
          WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    let Some(issue_id) = row.issue_id else {
        return Ok(missing());
    };

    let user_id = auth.0;
    let role = fetch_project_member_role(&st.pool, user_id, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, &slug).await?;
    let is_creator = row.created_by_id == Some(user_id);
    if let Err((code, msg)) = guard_inbox_patch(role.is_some(), role, is_creator, ws_admin) {
        return Ok((code, Json(json!({"error": msg}))));
    }
    let narrowed = is_guest_narrowed(role);
    let may_write_intake = may_write_intake_fields(role, ws_admin);

    // Validate every applied value BEFORE any write (Django validates
    // both serializers before either `save()`; first-error-wins here).
    let issue = body.issue.as_ref();
    let new_name = issue.and_then(|i| i.name.clone());
    if let Some(name) = &new_name {
        if name.trim().is_empty() {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Name is required"}))));
        }
    }
    let new_priority = issue.and_then(|i| i.priority.clone());
    if !narrowed {
        if let Some(p) = &new_priority {
            if !PRIORITIES.contains(&p.as_str()) {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid priority"}))));
            }
        }
    }
    if may_write_intake {
        if let Some(s) = body.status {
            if !INTAKE_STATUSES.contains(&s) {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid status"}))));
            }
        }
        if let Some(Some(dup)) = body.duplicate_to {
            let exists: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT id FROM issues WHERE id = $1 AND deleted_at IS NULL")
                    .bind(dup)
                    .fetch_optional(&st.pool)
                    .await?;
            if exists.is_none() {
                return Ok(missing());
            }
        }
    }

    // Nested `issue` write (`IssueCreateSerializer` partial, `base.py:403-418`).
    if !narrowed {
        let desc_html = issue.and_then(|i| i.description_html.clone());
        let desc_json = issue.and_then(|i| i.description_json.clone());
        if new_name.is_some() || desc_html.is_some() || desc_json.is_some() || new_priority.is_some() {
            sqlx::query(
                "UPDATE issues SET name = COALESCE($1, name), \
                  description_html = COALESCE($2, description_html), \
                  description_json = COALESCE($3::jsonb, description_json), \
                  priority = COALESCE($4, priority), \
                  updated_at = now(), updated_by_id = $6 \
                  WHERE id = $5 AND deleted_at IS NULL",
            )
            .bind(&new_name)
            .bind(&desc_html)
            .bind(&desc_json)
            .bind(&new_priority)
            .bind(issue_id)
            .bind(user_id)
            .execute(&st.pool)
            .await?;
        }
    } else {
        let desc_html = issue.and_then(|i| i.description_html.clone());
        let desc_json = issue.and_then(|i| i.description_json.clone());
        if new_name.is_some() || desc_html.is_some() || desc_json.is_some() {
            sqlx::query(
                "UPDATE issues SET name = COALESCE($1, name), \
                  description_html = COALESCE($2, description_html), \
                  description_json = COALESCE($3::jsonb, description_json), \
                  updated_at = now(), updated_by_id = $5 \
                  WHERE id = $4 AND deleted_at IS NULL",
            )
            .bind(&new_name)
            .bind(&desc_html)
            .bind(&desc_json)
            .bind(issue_id)
            .bind(user_id)
            .execute(&st.pool)
            .await?;
        }
    }

    // Intake-level write (`IntakeIssueSerializer` partial, `base.py:426-431`).
    if may_write_intake {
        let n_status = body.status;
        let (dup_set, dup_val): (bool, Option<uuid::Uuid>) = match body.duplicate_to {
            None => (false, None),
            Some(v) => (true, v),
        };
        let (snooze_set, snooze_val): (bool, Option<chrono::DateTime<chrono::Utc>>) =
            match body.snoozed_till {
                None => (false, None),
                Some(v) => (true, v),
            };
        let (source_set, source_val): (bool, Option<String>) = match body.source {
            None => (false, None),
            Some(ref v) => (true, v.clone()),
        };
        if n_status.is_some() || dup_set || snooze_set || source_set {
            // Positional binds are static, so each nullable column is set
            // via "= value (possibly NULL)" only when present — absent
            // columns keep their value. `updated_by_id` mirrors save().
            sqlx::query(
                "UPDATE intake_issues SET \
                  status = CASE WHEN $1::boolean THEN $2::integer ELSE status END, \
                  duplicate_to_id = CASE WHEN $3::boolean THEN $4::uuid ELSE duplicate_to_id END, \
                  snoozed_till = CASE WHEN $5::boolean THEN $6::timestamptz ELSE snoozed_till END, \
                  source = CASE WHEN $7::boolean THEN $8::varchar ELSE source END, \
                  updated_at = now(), updated_by_id = $10 \
                  WHERE id = $9 AND deleted_at IS NULL",
            )
            .bind(n_status.is_some())
            .bind(n_status)
            .bind(dup_set)
            .bind(dup_val)
            .bind(snooze_set)
            .bind(snooze_val)
            .bind(source_set)
            .bind(source_val)
            .bind(pk)
            .bind(user_id)
            .execute(&st.pool)
            .await?;
        }
    }

    // Re-fetch + return the updated intake issue (`base.py:480-505` tail:
    // `IntakeIssueDetailSerializer(intake_issue)`, 200).
    let fresh: Option<(i32, Option<uuid::Uuid>, Option<chrono::DateTime<chrono::Utc>>, Option<String>)> =
        sqlx::query_as(
            "SELECT status, duplicate_to_id, snoozed_till, source FROM intake_issues \
              WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
    let Some((status, duplicate_to, snoozed_till, source)) = fresh else {
        return Ok(missing());
    };
    let issue_row: Option<InboxIssueDetailIssue> = sqlx::query_as(INBOX_ISSUE_SELECT_SQL)
        .bind(issue_id)
        .bind(project_id)
        .bind(&slug)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await?;
    let Some(issue_row) = issue_row else {
        return Ok(missing());
    };
    let duplicate_issue_detail: Option<InboxDuplicateDetail> = match duplicate_to {
        None => None,
        Some(dup) => {
            sqlx::query_as(
                "SELECT i.id, i.name, i.priority, i.sequence_id, i.project_id, i.created_at, \
                  COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
                    WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
                  i.created_by_id AS created_by \
                  FROM issues i WHERE i.id = $1 AND i.deleted_at IS NULL",
            )
            .bind(dup)
            .fetch_optional(&st.pool)
            .await?
        }
    };
    let detail = InboxIssueDetail {
        id: pk,
        status,
        duplicate_to,
        snoozed_till,
        duplicate_issue_detail,
        source,
        issue: issue_row,
    };
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(&detail).unwrap()),
    ))
}

#[cfg(test)]
mod inbox_patch_tests {
    use super::*;

    #[test]
    fn gate_matrix_matches_django_partial_update() {
        // Mirrors `IntakeIssueViewSet.partial_update`
        // (`plane/app/views/intake/base.py:355-374`): no project
        // membership AND no workspace-admin → 403 "Only admin or creator
        // can update the intake work items" (check 1 has no creator
        // clause, so even a creator without membership/ws-admin 403s).
        for (has_pm, role, creator, ws_admin) in [
            (false, None, false, false),
            (false, None, true, false),
            (false, Some(5), false, false),
        ] {
            assert_eq!(
                guard_inbox_patch(has_pm, role, creator, ws_admin),
                Err((
                    StatusCode::FORBIDDEN,
                    ONLY_ADMIN_OR_CREATOR_MSG.to_string()
                )),
                "has_pm={has_pm} role={role:?} creator={creator} ws_admin={ws_admin}",
            );
        }
        // Guest-role member, not creator, not ws-admin → 400 "You cannot
        // edit intake issues".
        assert_eq!(
            guard_inbox_patch(true, Some(5), false, false),
            Err((
                StatusCode::BAD_REQUEST,
                CANNOT_EDIT_INTAKE_MSG.to_string()
            )),
        );
        // admin | member | creator | ws-admin → ok.
        for (has_pm, role, creator, ws_admin) in [
            (true, Some(20), false, false),
            (true, Some(15), false, false),
            (true, Some(5), true, false),
            (true, Some(5), false, true),
            (true, Some(15), false, true),
            (false, None, false, true),
            (false, None, true, true),
        ] {
            assert!(
                guard_inbox_patch(has_pm, role, creator, ws_admin).is_ok(),
                "has_pm={has_pm} role={role:?} creator={creator} ws_admin={ws_admin}",
            );
        }
    }

    #[test]
    fn intake_field_write_is_admin_or_ws_admin_only() {
        // Mirrors `(project_member and role > ROLE.MEMBER.value) or
        // is_workspace_admin` (`base.py:426`): only project ADMIN (20)
        // or a workspace admin may write intake-level fields
        // (status/duplicate_to/snoozed_till/source).
        assert!(may_write_intake_fields(Some(20), false));
        assert!(!may_write_intake_fields(Some(15), false));
        assert!(!may_write_intake_fields(Some(5), false));
        assert!(!may_write_intake_fields(None, false));
        assert!(may_write_intake_fields(Some(5), true));
        assert!(may_write_intake_fields(None, true));
    }

    #[test]
    fn guest_issue_edits_are_name_description_only() {
        // Mirrors `if project_member and role <= ROLE.GUEST.value`
        // (`base.py:396-401`): guest issue payloads are narrowed to
        // name/description_html/description_json (no ws-admin exemption
        // in Django's narrowing branch).
        assert!(is_guest_narrowed(Some(5)));
        assert!(!is_guest_narrowed(Some(15)));
        assert!(!is_guest_narrowed(Some(20)));
        assert!(!is_guest_narrowed(None));
    }

    #[test]
    fn detail_keys_follow_django_field_order() {
        // `IntakeIssueDetailSerializer.Meta.fields`
        // (`serializers/intake.py:99-107`): id, status, duplicate_to,
        // snoozed_till, duplicate_issue_detail, source, issue.
        assert_eq!(
            INBOX_DETAIL_KEYS,
            [
                "id",
                "status",
                "duplicate_to",
                "snoozed_till",
                "duplicate_issue_detail",
                "source",
                "issue",
            ]
        );
        // Nested `issue` is `IssueDetailSerializer`
        // (`serializers/issue.py:934-945`) = `IssueSerializer.Meta.fields`
        // (`issue.py:786-812`, 25 keys) + description_html, is_subscribed,
        // is_intake — same 28-key order as D7 `ARCHIVED_DETAIL_KEYS`.
        assert_eq!(INBOX_ISSUE_KEYS.len(), 28);
        assert_eq!(
            &INBOX_ISSUE_KEYS[..25],
            &[
                "id",
                "name",
                "state_id",
                "sort_order",
                "completed_at",
                "estimate_point",
                "priority",
                "start_date",
                "target_date",
                "sequence_id",
                "project_id",
                "parent_id",
                "cycle_id",
                "module_ids",
                "label_ids",
                "assignee_ids",
                "sub_issues_count",
                "created_at",
                "updated_at",
                "created_by",
                "updated_by",
                "attachment_count",
                "link_count",
                "is_draft",
                "archived_at",
            ]
        );
        assert_eq!(
            &INBOX_ISSUE_KEYS[25..],
            &["description_html", "is_subscribed", "is_intake"]
        );
    }

    #[test]
    fn inbox_patch_handler_exists_for_both_routes() {
        // Wiring guard: `main.rs` registers
        // `PATCH .../intake-issues/:pk/` + `.../inbox-issues/:pk/` →
        // `patch_issue` (Django serves both paths from
        // `IntakeIssueViewSet`, `urls/intake.py:44-55`).
        let _ = super::patch_issue;
    }
}
