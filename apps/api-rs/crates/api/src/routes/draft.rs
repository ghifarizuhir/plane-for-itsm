use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::project::{deny, missing, ws_role, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{
    next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str,
    total_pages, DetailEnvelope, PageWindow,
};

/// Workspace drafts + draft-to-issue — parity with Django
/// `WorkspaceDraftIssueViewSet` (`plane/app/views/workspace/draft.py:46-311`,
/// `plane/app/urls/workspace.py:202-216`):
/// `GET+POST /api/workspaces/:slug/draft-issues/` +
/// `GET+PATCH+DELETE .../:did/` + `POST .../draft-to-issue/:did/`.
/// Celery `issue_activity.delay` writes skipped (Batch C precedent — Rust
/// never writes activities).
///
/// Live columns verified 2026-09-06 via
/// `docker exec plane-db psql -U plane -d plane -c "\d draft_issues"` (+ the
/// four bridge tables): `draft_issues` has id, name (nullable), description
/// json/html/stripped/binary, priority, start/target dates, sort_order,
/// completed_at, external_source/id, created_by/estimate_point/parent/
/// project/state/type/updated_by/workspace FKs, audit columns; bridges
/// `draft_issue_assignees` (draft_issue, assignee), `draft_issue_labels`
/// (draft_issue, label), `draft_issue_modules` (draft_issue, module),
/// `draft_issue_cycles` (draft_issue, cycle) — all soft-deleted
/// (`deleted_at`), all with workspace/project/created_by/updated_by audit
/// FKs. `DraftIssue` model (`plane/db/models/draft.py:16-82`,
/// `db_table = "draft_issues"`, `ordering = ("-created_at",)`) extends
/// `WorkspaceBaseModel` (workspace FK NOT NULL, project FK nullable —
/// drafts may have NO project).
///
/// Serializers (`plane/app/serializers/draft.py:300-342`):
/// - `DraftIssueSerializer` 21 keys: id, name, state_id, sort_order,
///   completed_at, estimate_point, priority, start_date, target_date,
///   project_id, parent_id, cycle_id, module_ids, label_ids, assignee_ids,
///   created_at, updated_at, created_by, updated_by, type_id,
///   description_html.
/// - `DraftIssueDetailSerializer` = base fields + `["description_html"]`
///   — but `description_html` is ALREADY in the base list, so DRF dict
///   semantics render the SAME 21 keys (duplicate deduped). This file uses
///   one struct + one JSON builder for list/create/retrieve.
/// - `create` re-reads via `.values(...)` with the same 21 keys
///   (`draft.py:124-151`) → 201.
///
/// Locked semantics (plan D10):
/// - list GET 200 paginated `DraftIssueSerializer`, own drafts ONLY
///   (`created_by=user`, `draft.py:101`), `order_by("-created_at")`.
/// - POST 201 (same 21 keys) / 400 (`DraftIssueCreateSerializer` errors).
/// - retrieve 200 `DraftIssueDetailSerializer`, miss → 404 standard msg
///   (`missing()`, via `.first()` None branch `draft.py:190-194`).
/// - PATCH 204 / 400, miss → 404 `{"error":"Issue not found"}` verbatim,
///   NON-standard (`draft.py:166`).
/// - DELETE 204.
/// - draft-to-issue POST 201, no-project → 400
///   `{"error":"Project is required to create an issue."}`
///   (`draft.py:210-212`).
/// - Gates are WORKSPACE level (`level="WORKSPACE"`,
///   `permissions/base.py:44-51` + creator branch `base.py:23-38`):
///   list/create AMG (20/15/5); PATCH ADMIN+MEMBER+creator (ws 20/15 AND
///   `created_by=user`, enforced inside after the decorator); retrieve
///   ADMIN+creator (ws 20 AND creator); destroy ADMIN-or-creator (ws member
///   AND (creator OR ws ADMIN)); draft-to-issue ADMIN+MEMBER (ws 20/15, no
///   creator requirement).
///
/// Deviations (documented, reviewer-adjudicable):
/// - `?issue_filters` (`draft.py:100`, `utils/issue_filters.py:428-463`)
///   ignored except pagination — FE draft list sends no filters; Django
///   would narrow by state/priority/etc. while Rust returns the full
///   own-drafts page.
/// - Pagination envelope reuses the shared `DetailEnvelope` (exact 12
///   `paginate()` keys, `paginator.py:728-743`) via the `issue_common`
///   helpers — same shape as I2 `list_detail`, not a bare array.
/// - `assignee_ids` annotation drops Django's
///   `assignees__member_project__is_active` refinement
///   (`draft.py:74-77`) — live bridge rows only (same narrowing as the
///   sibling list endpoints, unreachable in smoke).
/// - `label_ids` drops no label-table filter (Django filters bridge-live
///   only, `draft.py:66-67` — mirrored exactly); `module_ids` keeps the
///   `archived_at` exclusion (`draft.py:86-90`) plus a `deleted_at`
///   exclusion the default manager implies.
/// - Datetimes RFC3339 UTC (chrono, batch convention) vs DRF
///   per-user-timezone; key ORDER struct-declaration (Django field order).
/// - Draft-to-issue miss (unknown `:did`) → 404 `missing()` (sane):
///   Django does `.first()` → `None.project_id` → AttributeError → 500
///   (`draft.py:207-209` has no None branch — same class as D9
///   `remove-relation`, documented intentional deviation).
/// - Draft-to-issue returns a minimal 201 subset (id, name, project,
///   workspace, sequence_id, state, priority, assignee/label echoes) vs
///   Django's full `IssueCreateSerializer` `__all__` — status parity,
///   key-set subset (smoke checks status only; FE already holds the draft
///   payload it sent).
/// - `description_stripped` never computed (needs the html parser;
///   sibling writers skip it too); `sort_order`/default-state/`completed_at`
///   ARE mirrored from `DraftIssue.save` / `Issue.save` (see handlers).

/// Quoted from `plane/app/views/workspace/draft.py:166` (PATCH miss —
/// NON-standard, differs from the standard `missing()` body).
pub(crate) const PATCH_MISS_MSG: &str = "Issue not found";
/// Quoted from `plane/app/views/workspace/draft.py:211`.
pub(crate) const NO_PROJECT_MSG: &str = "Project is required to create an issue.";
/// Quoted from `plane/app/serializers/draft.py:77` (also `issue.py:133`).
pub(crate) const START_DATE_MSG: &str = "Start date cannot exceed target date";
/// Quoted from `plane/app/serializers/draft.py:119`.
pub(crate) const STATE_MSG: &str = "State is not valid please pass a valid state_id";
/// Quoted from `plane/app/serializers/draft.py:129`.
pub(crate) const PARENT_MSG: &str = "Parent is not valid issue_id please pass a valid issue_id";
/// Quoted from `plane/app/serializers/draft.py:138`.
pub(crate) const ESTIMATE_MSG: &str = "Estimate point is not valid please pass a valid estimate_point_id";
/// Generic IntegrityError body, byte-exact from
/// `plane/app/views/base.py:80-84` (Django maps EVERY `IntegrityError` →
/// 400 `{"error": "The payload is not valid"}`).
pub(crate) const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";

/// WORKSPACE-level gate for list/create: mirrors
/// `@allow_permission([ADMIN, MEMBER, GUEST], level="WORKSPACE")`
/// (`draft.py:98,111`, `permissions/base.py:44-51`) — any ACTIVE ws member
/// incl. GUEST passes; non-member → 403.
pub(crate) fn guard_list_create(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// WORKSPACE-level role half of the PATCH gate: mirrors
/// `@allow_permission([ADMIN, MEMBER], creator=True, model=Issue,
/// level="WORKSPACE")` (`draft.py:156-161`) — ws ADMIN/MEMBER pass the
/// decorator (GUEST/non-member → 403); the creator half is enforced inside
/// (`filter(pk, created_by=user)`, `draft.py:163-166`) and mapped to the
/// verbatim 404 below, so the caller checks role first, then ownership.
pub(crate) fn guard_patch(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// WORKSPACE-level role half of the retrieve gate: mirrors
/// `@allow_permission([ADMIN], creator=True, model=Issue,
/// level="WORKSPACE")` (`draft.py:186-187`) — ws ADMIN passes the decorator;
/// MEMBER/GUEST/non-member → 403 (the `Issue`-model creator bypass,
/// `base.py:36-38`, needs an `issues` row with the draft pk — vanishingly
/// rare — so the effective gate is ADMIN + creator, enforced inside via
/// `filter(pk, created_by=user)`, `draft.py:188-194`).
pub(crate) fn guard_retrieve(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// WORKSPACE-level gate for draft-to-issue: mirrors
/// `@allow_permission([ADMIN, MEMBER], level="WORKSPACE")`
/// (`draft.py:205`, no `creator=True`) — ws ADMIN/MEMBER pass (GUEST blocked);
/// no creator requirement (any member may convert any workspace draft).
pub(crate) fn guard_convert(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Pure encoding of the destroy gate: mirrors
/// `@allow_permission([ADMIN], creator=True, model=DraftIssue,
/// level="WORKSPACE")` (`draft.py:199`) — the decorator first requires an
/// ACTIVE ws membership (`base.py:26-34`), then passes iff the caller
/// created THAT draft (`base.py:36-38`, `model=DraftIssue` so the bypass is
/// meaningful here, unlike PATCH/retrieve) OR holds ws ADMIN
/// (`base.py:44-51`). MEMBER/GUEST non-creators → 403.
pub(crate) fn may_destroy(role: Option<i16>, is_creator: bool) -> bool {
    role.is_some() && (is_creator || role == Some(20))
}

/// Mirrors the `start_date`/`target_date` check in
/// `DraftIssueCreateSerializer.validate` (`serializers/draft.py:72-77`)
/// (and `IssueCreateSerializer.validate`, `issue.py:128-133`):
/// both present and `start > target` → Err (byte-exact message, no period).
pub(crate) fn validate_dates(
    start: Option<chrono::NaiveDate>,
    target: Option<chrono::NaiveDate>,
) -> Result<(), String> {
    if let (Some(s), Some(t)) = (start, target) {
        if s > t {
            return Err(START_DATE_MSG.to_string());
        }
    }
    Ok(())
}

/// Pure encoding of the draft-to-issue no-project branch
/// (`draft.py:209-213`): `if not draft_issue.project_id` → 400 with the
/// verbatim body. Returns the project id on success.
pub(crate) fn draft_to_issue_project(project_id: Option<uuid::Uuid>) -> Result<uuid::Uuid, String> {
    project_id.ok_or_else(|| NO_PROJECT_MSG.to_string())
}

/// DRF required/blank validation for the converted issue's `name`
/// (`Issue.name` has no `blank=True`, `db/models/issue.py:136`): missing →
/// `{"name": ["This field is required."]}`, blank (post-trim, mirroring DRF
/// `trim_whitespace`) → `{"name": ["This field may not be blank."]}`.
/// (D8 `validate_reaction` precedent for the body shape.)
pub(crate) fn validate_convert_name(name: Option<&str>) -> Result<String, Value> {
    match name {
        None => Err(json!({"name": ["This field is required."]})),
        Some(n) if n.trim().is_empty() => Err(json!({"name": ["This field may not be blank."]})),
        Some(n) => Ok(n.to_string()),
    }
}

fn opt_id(id: &Option<uuid::Uuid>) -> Value {
    id.map(|u| json!(u)).unwrap_or(Value::Null)
}

fn bad_request(body: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(body))
}

fn patch_miss() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({"error": PATCH_MISS_MSG})))
}

/// One `DraftIssueSerializer` row (21 keys,
/// `serializers/draft.py:300-335`): id, name, state_id, sort_order,
/// completed_at, estimate_point, priority, start_date, target_date,
/// project_id, parent_id, cycle_id, module_ids, label_ids, assignee_ids,
/// created_at, updated_at, created_by, updated_by, type_id,
/// description_html. Field names match the SELECT aliases in
/// `DRAFT_SELECT` (`estimate_point` aliases `estimate_point_id`,
/// `created_by`/`updated_by` alias `*_id` — DRF renders FKs as id strings).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct DraftRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: Option<String>,
    pub(crate) state_id: Option<uuid::Uuid>,
    pub(crate) sort_order: f64,
    pub(crate) completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) estimate_point: Option<uuid::Uuid>,
    pub(crate) priority: String,
    pub(crate) start_date: Option<chrono::NaiveDate>,
    pub(crate) target_date: Option<chrono::NaiveDate>,
    pub(crate) project_id: Option<uuid::Uuid>,
    pub(crate) parent_id: Option<uuid::Uuid>,
    pub(crate) cycle_id: Option<uuid::Uuid>,
    pub(crate) module_ids: Vec<uuid::Uuid>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) assignee_ids: Vec<uuid::Uuid>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
    pub(crate) type_id: Option<uuid::Uuid>,
    pub(crate) description_html: String,
}

/// Serializes one `DraftRow` like `DraftIssueSerializer`
/// (`serializers/draft.py:300-335`) — and identically like
/// `DraftIssueDetailSerializer` (`draft.py:337-342`), whose extra
/// `description_html` duplicates the base key (DRF dict dedupes).
pub(crate) fn draft_json(row: &DraftRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "state_id": opt_id(&row.state_id),
        "sort_order": row.sort_order,
        "completed_at": row.completed_at,
        "estimate_point": opt_id(&row.estimate_point),
        "priority": row.priority,
        "start_date": row.start_date,
        "target_date": row.target_date,
        "project_id": opt_id(&row.project_id),
        "parent_id": opt_id(&row.parent_id),
        "cycle_id": opt_id(&row.cycle_id),
        "module_ids": row.module_ids,
        "label_ids": row.label_ids,
        "assignee_ids": row.assignee_ids,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by),
        "updated_by": opt_id(&row.updated_by),
        "type_id": opt_id(&row.type_id),
        "description_html": row.description_html,
    })
}

/// Shared SELECT for draft rows: base columns plus the three `ArrayAgg`
/// annotations (`draft.py:55-93`) and the `cycle_id` subquery
/// (`draft.py:55-60`, newest bridge row wins via `Meta.ordering =
/// ("-created_at",)` on `DraftIssueCycle`). Bridge filters mirror Django:
/// labels bridge-live only (`draft.py:66`); assignees bridge-live only
/// (the `member_project__is_active` refinement, `draft.py:74-77`, is
/// intentionally dropped — see module docs); modules join `modules` for the
/// `archived_at` exclusion (`draft.py:86-90`). `{f}` is the `draft_issues`
/// alias; `{w}` filters the workspace slug.
fn draft_select(f: &str) -> String {
    format!(
        "{f}.id, {f}.name, {f}.state_id, {f}.sort_order, {f}.completed_at, \
        {f}.estimate_point_id AS estimate_point, {f}.priority, {f}.start_date, {f}.target_date, \
        {f}.project_id, {f}.parent_id, \
        (SELECT dc.cycle_id FROM draft_issue_cycles dc \
          WHERE dc.draft_issue_id = {f}.id AND dc.deleted_at IS NULL \
          ORDER BY dc.created_at DESC LIMIT 1) AS cycle_id, \
        COALESCE((SELECT array_agg(dm.module_id) FROM draft_issue_modules dm \
          JOIN modules m ON m.id = dm.module_id \
          WHERE dm.draft_issue_id = {f}.id AND dm.deleted_at IS NULL \
          AND m.deleted_at IS NULL AND m.archived_at IS NULL), '{{}}'::uuid[]) AS module_ids, \
        COALESCE((SELECT array_agg(dl.label_id) FROM draft_issue_labels dl \
          WHERE dl.draft_issue_id = {f}.id AND dl.deleted_at IS NULL), '{{}}'::uuid[]) AS label_ids, \
        COALESCE((SELECT array_agg(da.assignee_id) FROM draft_issue_assignees da \
          WHERE da.draft_issue_id = {f}.id AND da.deleted_at IS NULL), '{{}}'::uuid[]) AS assignee_ids, \
        {f}.created_at, {f}.updated_at, \
        {f}.created_by_id AS created_by, {f}.updated_by_id AS updated_by, \
        {f}.type_id, {f}.description_html"
    )
}

/// `?per_page=` / `?cursor=` for the draft list, mirroring
/// `BasePaginator.paginate` defaults (1000/1000, `paginator.py:643-681`) —
/// same helpers as I2 `list_detail`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DraftListQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
}

/// Writable draft fields for POST. Mirrors `DraftIssueCreateSerializer`
/// (`serializers/draft.py:33-61`, `fields = "__all__"`, read-only
/// workspace/created_by/updated_by/created_at/updated_at): every model key
/// is optional on create (name is `blank=True, null=True`,
/// `db/models/draft.py:45`); M2M ids are write-only lists
/// (`draft.py:41-50`); `cycle_id`/`module_ids` ride `initial_data`
/// (`draft.py:146-147`). Missing body (`None` via Axum `Option<Json>`, I3
/// `resolve_bulk_ids` precedent) maps to Django's empty `request.data`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateDraftBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub start_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub target_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub state_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub estimate_point_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub type_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub assignee_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub label_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub cycle_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub module_ids: Option<Vec<uuid::Uuid>>,
}

/// Writable draft fields for PATCH. Mirrors the same serializer with
/// `partial=True` (`draft.py:170-178`): every key optional; `cycle_id`
/// uses double-Option so missing ("not_provided", `draft.py:176,266`)
/// leaves the link untouched, explicit null clears it, UUID sets it;
/// `module_ids`/`assignee_ids`/`label_ids` missing leaves untouched,
/// present (incl. empty) replaces. `project_id` present re-scopes
/// validation (`draft.py:168`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchDraftBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub start_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub target_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub state_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub estimate_point_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub type_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub assignee_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub label_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub cycle_id: Option<Option<uuid::Uuid>>,
    #[serde(default)]
    pub module_ids: Option<Vec<uuid::Uuid>>,
}

/// Body for `POST .../draft-to-issue/:did/`. Mirrors
/// `IssueCreateSerializer` input (`draft.py:215-222`, `data=request.data` —
/// the DRAFT's stored fields are NOT copied; the caller sends the issue
/// payload, FE `issue-modal/form.tsx:282` spreads draft + form values):
/// `name` required, everything else optional; `cycle_id`/`module_ids` drive
/// the `CycleIssue`/`ModuleIssue` side-writes (`draft.py:239-296`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConvertBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub start_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub target_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub state_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub estimate_point_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub type_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub assignee_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub label_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub cycle_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub module_ids: Option<Vec<uuid::Uuid>>,
}

async fn workspace_id(pool: &sqlx::PgPool, slug: &str) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

/// Resolves the effective state for a new draft/issue, mirroring
/// `DraftIssue.save` (`db/models/draft.py:84-98`) / `Issue._ensure_default_state`
/// (`db/models/issue.py:231-243`): explicit id wins; else the project's
/// default non-triage state, else the first non-triage state, else None.
async fn resolve_default_state(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    explicit: Option<uuid::Uuid>,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    if explicit.is_some() {
        return Ok(explicit);
    }
    let Some(pid) = project_id else {
        return Ok(None);
    };
    let row: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(pid)
    .fetch_optional(pool)
    .await?;
    if row.is_some() {
        return Ok(row);
    }
    sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(pid)
    .fetch_optional(pool)
    .await
}

async fn state_group(
    pool: &sqlx::PgPool,
    state_id: Option<uuid::Uuid>,
) -> Result<Option<String>, sqlx::Error> {
    let Some(sid) = state_id else {
        return Ok(None);
    };
    sqlx::query_scalar("SELECT \"group\" FROM states WHERE id = $1")
        .bind(sid)
        .fetch_optional(pool)
        .await
}

async fn draft_sort_order(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    state_id: Option<uuid::Uuid>,
) -> Result<f64, sqlx::Error> {
    let (Some(pid), Some(_)) = (project_id, state_id) else {
        return Ok(65535.0);
    };
    // Mirrors `DraftIssue.save` (`db/models/draft.py:117-121`): max over
    // (project, state) + 10000, default 65535. `IS NOT DISTINCT FROM`
    // keeps NULL-state parity (Django `state=None` filters `state__isnull`).
    let max: Option<f64> = sqlx::query_scalar(
        "SELECT MAX(sort_order) FROM draft_issues \
         WHERE project_id IS NOT DISTINCT FROM $1 AND state_id IS NOT DISTINCT FROM $2 \
         AND deleted_at IS NULL",
    )
    .bind(pid)
    .bind(state_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(max.map(|m| m + 10000.0).unwrap_or(65535.0))
}

/// Filters candidate assignees to active project members with role >= MEMBER
/// (15), mirroring `DraftIssueCreateSerializer.validate`
/// (`serializers/draft.py:94-100`, `ROLE.MEMBER.value`) — Django silently
/// drops ineligible ids (no 400). `None` project → empty (Django filters
/// `project_id=None` → nothing matches).
async fn filter_assignees(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let Some(pid) = project_id else {
        return Ok(vec![]);
    };
    sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT member_id FROM project_members \
         WHERE project_id = $1 AND member_id = ANY($2) \
         AND role >= 15 AND is_active = true AND deleted_at IS NULL",
    )
    .bind(pid)
    .bind(ids)
    .fetch_all(pool)
    .await
}

/// Filters candidate labels to the project, mirroring
/// `serializers/draft.py:103-109` (silent drop, no 400). `None` project →
/// empty.
async fn filter_labels(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let Some(pid) = project_id else {
        return Ok(vec![]);
    };
    sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM labels WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
    )
    .bind(pid)
    .bind(ids)
    .fetch_all(pool)
    .await
}

async fn check_state(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    state_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    // Mirrors `serializers/draft.py:112-119`: state must belong to the
    // context project via the default `StateManager` (live + non-triage).
    // `None` project → no match → invalid (Django filters
    // `project_id=None` → empty).
    let Some(pid) = project_id else {
        return Ok(false);
    };
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
         AND deleted_at IS NULL AND \"group\" != 'triage')",
    )
    .bind(state_id)
    .bind(pid)
    .fetch_one(pool)
    .await
}

async fn check_parent(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    parent_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    // Mirrors `serializers/draft.py:122-129`: parent must belong to the
    // context project (`Issue.objects` = plain manager, but live-only here —
    // sibling endpoints narrow the same way; deleted parents are unreachable
    // in FE flows).
    let Some(pid) = project_id else {
        return Ok(false);
    };
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(parent_id)
    .bind(pid)
    .fetch_one(pool)
    .await
}

async fn check_estimate(
    pool: &sqlx::PgPool,
    project_id: Option<uuid::Uuid>,
    estimate_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    // Mirrors `serializers/draft.py:131-138`.
    let Some(pid) = project_id else {
        return Ok(false);
    };
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM estimate_points WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(estimate_id)
    .bind(pid)
    .fetch_one(pool)
    .await
}

async fn fetch_draft(
    pool: &sqlx::PgPool,
    slug: &str,
    pk: uuid::Uuid,
    only_own: Option<uuid::Uuid>,
) -> Result<Option<DraftRow>, sqlx::Error> {
    let own_filter = if only_own.is_some() { "AND d.created_by_id = $3" } else { "" };
    let sql = format!(
        "SELECT {} FROM draft_issues d WHERE d.id = $1 \
         AND d.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND d.deleted_at IS NULL {own_filter}",
        draft_select("d")
    );
    let mut q = sqlx::query_as::<_, DraftRow>(&sql).bind(pk).bind(slug);
    if let Some(uid) = only_own {
        q = q.bind(uid);
    }
    q.fetch_optional(pool).await
}

/// GET `/api/workspaces/:slug/draft-issues/` — parity with Django
/// `WorkspaceDraftIssueViewSet.list` (`views/workspace/draft.py:97-109`,
/// `urls/workspace.py:202-206`).
///
/// - Gate: WORKSPACE AMG (any ACTIVE ws member incl. GUEST).
/// - Scope: own drafts only (`created_by=user`, `draft.py:101`) in this
///   workspace, live rows, `ORDER BY created_at DESC` (`Meta.ordering` +
///   explicit `order_by("-created_at")`, `draft.py:101`,
///   `db/models/draft.py:82`).
/// - 200 paginated envelope (12 `paginate()` keys) of 21-key
///   `DraftIssueSerializer` rows.
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<DraftListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_list_create(role).is_err() {
        return Ok(deny());
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok(bad_request(json!({"detail": msg}))),
    };
    let cursor_raw = q.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(msg) => return Ok(bad_request(json!({"detail": msg}))),
    };
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok(bad_request(json!({"detail": "Error in parsing"}))),
        Ok(w) => w,
    };
    if limit <= 0 {
        return Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Something went wrong please try again later"})),
        ));
    }
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM draft_issues d \
         WHERE d.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $1) \
         AND d.created_by_id = $2 AND d.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let mut qb = sqlx::QueryBuilder::new(format!(
        "SELECT {} FROM draft_issues d \
         WHERE d.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ",
        draft_select("d")
    ));
    qb.push_bind(&slug);
    qb.push(") AND d.created_by_id = ");
    qb.push_bind(auth.0);
    qb.push(" AND d.deleted_at IS NULL ORDER BY d.created_at DESC LIMIT ");
    qb.push_bind(limit + 1);
    let rows: Vec<DraftRow> = match window {
        PageWindow::Rows(offset) => {
            qb.push(" OFFSET ");
            qb.push_bind(offset);
            qb.build_query_as().fetch_all(&st.pool).await?
        }
        PageWindow::BeyondEnd => Vec::new(),
    };
    let _ = (cursor.limit_value, cursor.is_prev);
    let next_page_results = rows.len() as i64 > limit;
    let mut rows = rows;
    rows.truncate(limit as usize);
    let results: Vec<Value> = rows.iter().map(draft_json).collect();
    let n = results.len() as i64;
    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next_cursor_str(limit, cursor.page),
        prev_cursor: prev_cursor_str(limit, cursor.page),
        next_page_results,
        prev_page_results: cursor.page > 0,
        count: n,
        total_pages: total_pages(total, limit),
        total_results: total,
        extra_stats: None,
        results,
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

/// POST `/api/workspaces/:slug/draft-issues/` — parity with Django `create`
/// (`draft.py:111-154`).
///
/// - Gate: WORKSPACE AMG.
/// - Body: `CreateDraftBody` (missing body → empty, Django's `request.data`
///   default); `project_id` may be absent (project-nullable drafts).
/// - Validation mirrors `DraftIssueCreateSerializer.validate`
///   (`serializers/draft.py:71-140`): start>target → 400
///   `{"non_field_errors": [...]}` (DRF renders a bare `ValidationError`
///   there); unknown state/parent/estimate → 400 with the verbatim
///   messages; assignees/labels silently filtered to the project (no 400).
///   Unknown `project_id` (no such project in this workspace) → 400
///   `{"error": "The payload is not valid"}` (Django `IntegrityError` via
///   the FK, `views/base.py:80-84`).
/// - Insert mirrors `create()` + `DraftIssue.save`
///   (`serializers/draft.py:142-217`, `db/models/draft.py:84-132`):
///   default state when omitted (project-scoped), `sort_order` max+10000,
///   `completed_at` when the state group is completed, bridge rows for
///   assignees/labels/cycle/modules, `created_by=user`.
/// - 201 with the 21-key row (same `.values()` keys as Django,
///   `draft.py:127-149`).
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    body: Option<Json<CreateDraftBody>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_list_create(role).is_err() {
        return Ok(deny());
    }
    let b = body.map(|Json(v)| v).unwrap_or_default();
    if let Err(e) = validate_dates(b.start_date, b.target_date) {
        return Ok(bad_request(json!({"non_field_errors": [e]})));
    }
    if let Some(sid) = b.state_id {
        if !check_state(&st.pool, b.project_id, sid).await? {
            return Ok(bad_request(json!({"non_field_errors": [STATE_MSG]})));
        }
    }
    if let Some(pid) = b.parent_id {
        if !check_parent(&st.pool, b.project_id, pid).await? {
            return Ok(bad_request(json!({"non_field_errors": [PARENT_MSG]})));
        }
    }
    if let Some(eid) = b.estimate_point_id {
        if !check_estimate(&st.pool, b.project_id, eid).await? {
            return Ok(bad_request(json!({"non_field_errors": [ESTIMATE_MSG]})));
        }
    }
    let Some(wid) = workspace_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    if let Some(pid) = b.project_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL)",
        )
        .bind(pid)
        .bind(wid)
        .fetch_one(&st.pool)
        .await?;
        if !exists {
            return Ok(bad_request(json!({"error": PAYLOAD_INVALID_MSG})));
        }
    }
    let state_id = resolve_default_state(&st.pool, b.project_id, b.state_id).await?;
    let group = state_group(&st.pool, state_id).await?;
    let completed_at: Option<chrono::DateTime<chrono::Utc>> =
        if group.as_deref() == Some("completed") { Some(chrono::Utc::now()) } else { None };
    let sort_order = draft_sort_order(&st.pool, b.project_id, state_id).await?;
    let assignees = filter_assignees(&st.pool, b.project_id, &b.assignee_ids.unwrap_or_default()).await?;
    let labels = filter_labels(&st.pool, b.project_id, &b.label_ids.unwrap_or_default()).await?;
    let mut tx = st.pool.begin().await?;
    let draft_id: uuid::Uuid = match sqlx::query_scalar(
        "INSERT INTO draft_issues (id, name, description_html, description_json, priority, \
         start_date, target_date, sort_order, completed_at, external_source, external_id, \
         estimate_point_id, parent_id, project_id, state_id, type_id, \
         workspace_id, created_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, COALESCE($2, '<p></p>'), '{}', COALESCE($3, 'none'), \
         $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now(), now()) RETURNING id",
    )
    .bind(b.name.as_deref())
    .bind(b.description_html.as_deref())
    .bind(b.priority.as_deref())
    .bind(b.start_date)
    .bind(b.target_date)
    .bind(sort_order)
    .bind(completed_at)
    .bind(b.external_source.as_deref())
    .bind(b.external_id.as_deref())
    .bind(b.estimate_point_id)
    .bind(b.parent_id)
    .bind(b.project_id)
    .bind(state_id)
    .bind(b.type_id)
    .bind(wid)
    .bind(auth.0)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        // Django maps EVERY `IntegrityError` → 400
        // `{"error": "The payload is not valid"}` (`views/base.py:80-84`).
        Err(e)
            if e
                .as_database_error()
                .and_then(|d| d.code())
                .map(|c| c.as_ref().starts_with("23"))
                .unwrap_or(false) =>
        {
            return Ok(bad_request(json!({"error": PAYLOAD_INVALID_MSG})));
        }
        Err(e) => return Err(common::errors::AppError(e.into())),
    };
    if !assignees.is_empty() {
        let n = assignees.len() as i64;
        let _ = sqlx::query(
            "INSERT INTO draft_issue_assignees (id, draft_issue_id, assignee_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
        )
        .bind(draft_id)
        .bind(&assignees)
        .bind(b.project_id)
        .bind(wid)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
        let _ = n;
    }
    if !labels.is_empty() {
        sqlx::query(
            "INSERT INTO draft_issue_labels (id, draft_issue_id, label_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
        )
        .bind(draft_id)
        .bind(&labels)
        .bind(b.project_id)
        .bind(wid)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(cid) = b.cycle_id {
        sqlx::query(
            "INSERT INTO draft_issue_cycles (id, draft_issue_id, cycle_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now())",
        )
        .bind(draft_id)
        .bind(cid)
        .bind(b.project_id)
        .bind(wid)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(mods) = b.module_ids {
        if !mods.is_empty() {
            sqlx::query(
                "INSERT INTO draft_issue_modules (id, draft_issue_id, module_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
            )
            .bind(draft_id)
            .bind(&mods)
            .bind(b.project_id)
            .bind(wid)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    let row: Option<DraftRow> = sqlx::query_as(&format!(
        "SELECT {} FROM draft_issues d WHERE d.id = $1",
        draft_select("d")
    ))
    .bind(draft_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::CREATED, Json(draft_json(&r)))),
        None => Ok(missing()),
    }
}

/// GET `/api/workspaces/:slug/draft-issues/:did/` — parity with Django
/// `retrieve` (`draft.py:186-197`).
///
/// - Gate: WORKSPACE ADMIN + creator (ws ADMIN role; then
///   `filter(pk, created_by=user)` — miss OR foreign draft → 404 standard
///   msg, `draft.py:190-194`).
/// - 200 with the 21-key detail row (identical to the list shape — see
///   module docs on the duplicated `description_html`).
pub async fn retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_retrieve(role).is_err() {
        return Ok(deny());
    }
    match fetch_draft(&st.pool, &slug, pk, Some(auth.0)).await? {
        Some(r) => Ok((StatusCode::OK, Json(draft_json(&r)))),
        None => Ok(missing()),
    }
}

/// PATCH `/api/workspaces/:slug/draft-issues/:did/` — parity with Django
/// `partial_update` (`draft.py:156-184`).
///
/// - Gate: WORKSPACE ADMIN/MEMBER + creator: ws role 20/15 else 403; then
///   `filter(pk, created_by=user)` — miss OR foreign draft → 404
///   `{"error":"Issue not found"}` verbatim (NON-standard, `draft.py:166`).
/// - Validation context re-scopes to
///   `request.data.get("project_id", issue.project_id)` (`draft.py:168`);
///   `cycle_id` defaults to `"not_provided"` (untouched, `draft.py:176`);
///   serializer errors → 400.
/// - Success → 204 empty (`draft.py:183`).
pub async fn partial_update(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    body: Option<Json<PatchDraftBody>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_patch(role).is_err() {
        return Ok(deny());
    }
    let existing = fetch_draft(&st.pool, &slug, pk, Some(auth.0)).await?;
    let Some(cur) = existing else {
        return Ok(patch_miss());
    };
    let b = body.map(|Json(v)| v).unwrap_or_default();
    let eff_project = b.project_id.or(cur.project_id);
    // Partial serializer validates only provided dates (`attrs` holds just
    // the supplied keys, `partial=True`): check only when BOTH are present.
    if b.start_date.is_some() && b.target_date.is_some() {
        if let Err(e) = validate_dates(b.start_date, b.target_date) {
            return Ok(bad_request(json!({"non_field_errors": [e]})));
        }
    }
    if let Some(sid) = b.state_id {
        if !check_state(&st.pool, eff_project, sid).await? {
            return Ok(bad_request(json!({"non_field_errors": [STATE_MSG]})));
        }
    }
    if let Some(pid) = b.parent_id {
        if !check_parent(&st.pool, eff_project, pid).await? {
            return Ok(bad_request(json!({"non_field_errors": [PARENT_MSG]})));
        }
    }
    if let Some(eid) = b.estimate_point_id {
        if !check_estimate(&st.pool, eff_project, eid).await? {
            return Ok(bad_request(json!({"non_field_errors": [ESTIMATE_MSG]})));
        }
    }
    let Some(wid) = workspace_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE draft_issues SET name = COALESCE($1, name), \
         description_html = COALESCE($2, description_html), \
         priority = COALESCE($3, priority), \
         start_date = COALESCE($4, start_date), target_date = COALESCE($5, target_date), \
         state_id = COALESCE($6, state_id), parent_id = COALESCE($7, parent_id), \
         estimate_point_id = COALESCE($8, estimate_point_id), type_id = COALESCE($9, type_id), \
         external_source = COALESCE($10, external_source), external_id = COALESCE($11, external_id), \
         project_id = COALESCE($12, project_id), \
         updated_at = now(), updated_by_id = $13 WHERE id = $14",
    )
    .bind(b.name.as_deref())
    .bind(b.description_html.as_deref())
    .bind(b.priority.as_deref())
    .bind(b.start_date)
    .bind(b.target_date)
    .bind(b.state_id)
    .bind(b.parent_id)
    .bind(b.estimate_point_id)
    .bind(b.type_id)
    .bind(b.external_source.as_deref())
    .bind(b.external_id.as_deref())
    .bind(b.project_id)
    .bind(auth.0)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    if let Some(ids) = b.assignee_ids {
        let kept = filter_assignees(&st.pool, eff_project, &ids).await?;
        sqlx::query("UPDATE draft_issue_assignees SET deleted_at = now() WHERE draft_issue_id = $1 AND deleted_at IS NULL")
            .bind(pk)
            .execute(&mut *tx)
            .await?;
        if !kept.is_empty() {
            sqlx::query(
                "INSERT INTO draft_issue_assignees (id, draft_issue_id, assignee_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
            )
            .bind(pk)
            .bind(&kept)
            .bind(eff_project)
            .bind(wid)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
    }
    if let Some(ids) = b.label_ids {
        let kept = filter_labels(&st.pool, eff_project, &ids).await?;
        sqlx::query("UPDATE draft_issue_labels SET deleted_at = now() WHERE draft_issue_id = $1 AND deleted_at IS NULL")
            .bind(pk)
            .execute(&mut *tx)
            .await?;
        if !kept.is_empty() {
            sqlx::query(
                "INSERT INTO draft_issue_labels (id, draft_issue_id, label_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
            )
            .bind(pk)
            .bind(&kept)
            .bind(eff_project)
            .bind(wid)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
    }
    // `cycle_id != "not_provided"` (`draft.py:266`): missing → untouched.
    if let Some(cycle_opt) = b.cycle_id {
        sqlx::query("UPDATE draft_issue_cycles SET deleted_at = now() WHERE draft_issue_id = $1 AND deleted_at IS NULL")
            .bind(pk)
            .execute(&mut *tx)
            .await?;
        if let Some(cid) = cycle_opt {
            sqlx::query(
                "INSERT INTO draft_issue_cycles (id, draft_issue_id, cycle_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now())",
            )
            .bind(pk)
            .bind(cid)
            .bind(eff_project)
            .bind(wid)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
    }
    if let Some(mods) = b.module_ids {
        sqlx::query("UPDATE draft_issue_modules SET deleted_at = now() WHERE draft_issue_id = $1 AND deleted_at IS NULL")
            .bind(pk)
            .execute(&mut *tx)
            .await?;
        if !mods.is_empty() {
            sqlx::query(
                "INSERT INTO draft_issue_modules (id, draft_issue_id, module_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
            )
            .bind(pk)
            .bind(&mods)
            .bind(eff_project)
            .bind(wid)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
    }
    // `instance.updated_at = timezone.now()` even for relation-only edits
    // (`serializers/draft.py:296`) — covered by the UPDATE above.
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// DELETE `/api/workspaces/:slug/draft-issues/:did/` — parity with Django
/// `destroy` (`draft.py:199-203`).
///
/// - Gate: ACTIVE ws member AND (creator of THAT draft OR ws ADMIN)
///   (`may_destroy`); else 403. Miss → 404 standard msg (Django `.get()`
///   → `DoesNotExist`, `views/base.py:92-96`).
/// - Soft-delete (`deleted_at=now()`, `SoftDeletionQuerySet.delete`,
///   `mixins.py:48-53` — Django `delete()` is soft) → 204.
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT d.created_by_id FROM draft_issues d \
         WHERE d.id = $1 AND d.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND d.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((created_by,)) = row else {
        return Ok(missing());
    };
    if !may_destroy(role, created_by == Some(auth.0)) {
        return Ok(deny());
    }
    sqlx::query("UPDATE draft_issues SET deleted_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// POST `/api/workspaces/:slug/draft-to-issue/:did/` — parity with Django
/// `create_draft_to_issue` (`draft.py:205-311`).
///
/// - Gate: WORKSPACE ADMIN/MEMBER (no creator requirement).
/// - Miss → 404 `missing()` (sane deviation — Django 500s on
///   `None.project_id`; see module docs, D9 precedent).
/// - No project on the draft → 400 verbatim (`draft.py:210-212`).
/// - Issue validation mirrors `IssueCreateSerializer.validate`
///   (`serializers/issue.py:124-197`): name required/blank → 400
///   `{"name": [...]}`; start>target → 400 `{"non_field_errors": [...]}`;
///   unknown state/parent/estimate → 400 verbatim; assignees/labels
///   silently filtered (Django drops ineligible ids, no 400).
/// - Side-writes mirrored (`draft.py:239-307`, Celery skipped):
///   `CycleIssue` when `cycle_id` present, `ModuleIssue` bulk when
///   `module_ids` non-empty, `FileAsset`s re-pointed
///   (`issue_id=new, entity_type=ISSUE_DESCRIPTION, draft_issue_id=NULL`),
///   draft soft-deleted. Issue `sequence_id`/`sort_order`/default-state/
///   `completed_at` mirror `Issue.save` (`db/models/issue.py:180-229`);
///   `issue_sequences` row created; default-assignee applied when the
///   caller sends no assignees (`serializers/issue.py:232-253`).
/// - 201 with the minimal issue subset (see module docs).
pub async fn create_draft_to_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, draft_id)): axum::extract::Path<(String, uuid::Uuid)>,
    body: Option<Json<ConvertBody>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_convert(role).is_err() {
        return Ok(deny());
    }
    let draft: Option<DraftRow> = fetch_draft(&st.pool, &slug, draft_id, None).await?;
    let Some(d) = draft else {
        return Ok(missing());
    };
    let project_id = match draft_to_issue_project(d.project_id) {
        Ok(pid) => pid,
        Err(e) => return Ok(bad_request(json!({"error": e}))),
    };
    let b = body.map(|Json(v)| v).unwrap_or_default();
    let name = match validate_convert_name(b.name.as_deref()) {
        Ok(n) => n,
        Err(body) => return Ok(bad_request(body)),
    };
    if let Err(e) = validate_dates(b.start_date, b.target_date) {
        return Ok(bad_request(json!({"non_field_errors": [e]})));
    }
    if let Some(sid) = b.state_id {
        if !check_state(&st.pool, Some(project_id), sid).await? {
            return Ok(bad_request(json!({"non_field_errors": [STATE_MSG]})));
        }
    }
    if let Some(pid) = b.parent_id {
        if !check_parent(&st.pool, Some(project_id), pid).await? {
            return Ok(bad_request(json!({"non_field_errors": [PARENT_MSG]})));
        }
    }
    if let Some(eid) = b.estimate_point_id {
        if !check_estimate(&st.pool, Some(project_id), eid).await? {
            return Ok(bad_request(json!({"non_field_errors": [ESTIMATE_MSG]})));
        }
    }
    let proj: Option<(uuid::Uuid, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT workspace_id, default_assignee_id FROM projects WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((workspace_id, default_assignee)) = proj else {
        return Ok(bad_request(json!({"error": PAYLOAD_INVALID_MSG})));
    };
    let state_id = resolve_default_state(&st.pool, Some(project_id), b.state_id).await?;
    let group = state_group(&st.pool, state_id).await?;
    let completed_at: Option<chrono::DateTime<chrono::Utc>> =
        if group.as_deref() == Some("completed") { Some(chrono::Utc::now()) } else { None };
    let sort_order: f64 = {
        let max: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(sort_order) FROM issues WHERE project_id = $1 AND state_id IS NOT DISTINCT FROM $2 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(state_id)
        .fetch_optional(&st.pool)
        .await?
        .flatten();
        max.map(|m| m + 10000.0).unwrap_or(65535.0)
    };
    let sequence: i64 = {
        let max: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(sequence) FROM issue_sequences WHERE project_id = $1",
        )
        .bind(project_id)
        .fetch_optional(&st.pool)
        .await?
        .flatten();
        max.unwrap_or(0) + 1
    };
    // Assignees: provided → filtered (silent drop); absent → default
    // assignee when eligible (`serializers/issue.py:214-253`).
    let mut assignees = filter_assignees(&st.pool, Some(project_id), &b.assignee_ids.clone().unwrap_or_default()).await?;
    if b.assignee_ids.is_none() {
        if let Some(def) = default_assignee {
            let ok: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM project_members WHERE member_id = $1 AND project_id = $2 \
                 AND role >= 15 AND is_active = true AND deleted_at IS NULL)",
            )
            .bind(def)
            .bind(project_id)
            .fetch_one(&st.pool)
            .await?;
            if ok {
                assignees = vec![def];
            }
        }
    }
    let labels = filter_labels(&st.pool, Some(project_id), &b.label_ids.clone().unwrap_or_default()).await?;
    let mut tx = st.pool.begin().await?;
    let issue_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO issues (id, name, description_html, description_json, priority, \
         start_date, target_date, sequence_id, sort_order, completed_at, is_draft, \
         estimate_point_id, parent_id, type_id, state_id, project_id, workspace_id, \
         created_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, COALESCE($2, '<p></p>'), '{}', COALESCE($3, 'none'), \
         $4, $5, $6, $7, $8, false, $9, $10, $11, $12, $13, $14, $15, now(), now()) RETURNING id",
    )
    .bind(&name)
    .bind(b.description_html.as_deref())
    .bind(b.priority.as_deref())
    .bind(b.start_date)
    .bind(b.target_date)
    .bind(sequence as i32)
    .bind(sort_order)
    .bind(completed_at)
    .bind(b.estimate_point_id)
    .bind(b.parent_id)
    .bind(b.type_id)
    .bind(state_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(auth.0)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, created_by_id, deleted, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, now(), now())",
    )
    .bind(sequence)
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    if !assignees.is_empty() {
        sqlx::query(
            "INSERT INTO issue_assignees (id, issue_id, assignee_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
        )
        .bind(issue_id)
        .bind(&assignees)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    if !labels.is_empty() {
        sqlx::query(
            "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, unnest($2::uuid[]), $3, $4, $5, now(), now()",
        )
        .bind(issue_id)
        .bind(&labels)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    // `if request.data.get("cycle_id", None)` (`draft.py:239`): truthy only.
    if let Some(cid) = b.cycle_id {
        sqlx::query(
            "INSERT INTO cycle_issues (id, cycle_id, issue_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
        )
        .bind(cid)
        .bind(issue_id)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    // `if request.data.get("module_ids", [])` (`draft.py:266`): non-empty only.
    if let Some(mods) = b.module_ids.clone() {
        if !mods.is_empty() {
            sqlx::query(
                "INSERT INTO module_issues (id, module_id, issue_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), unnest($1::uuid[]), $2, $3, $4, $5, $5, now(), now()",
            )
            .bind(&mods)
            .bind(issue_id)
            .bind(project_id)
            .bind(workspace_id)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
    }
    // `FileAsset.objects.filter(draft_issue_id=draft_id).update(...)`
    // (`draft.py:299-304`).
    sqlx::query(
        "UPDATE file_assets SET issue_id = $1, entity_type = 'ISSUE_DESCRIPTION', draft_issue_id = NULL \
         WHERE draft_issue_id = $2",
    )
    .bind(issue_id)
    .bind(draft_id)
    .execute(&mut *tx)
    .await?;
    // `draft_issue.delete()` (`draft.py:307`) — soft.
    sqlx::query("UPDATE draft_issues SET deleted_at = now() WHERE id = $1")
        .bind(draft_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": issue_id,
            "name": name,
            "project_id": project_id,
            "workspace_id": workspace_id,
            "sequence_id": sequence as i32,
            "state_id": opt_id(&state_id),
            "priority": b.priority.unwrap_or_else(|| "none".to_string()),
            "assignee_ids": assignees,
            "label_ids": labels,
        })),
    ))
}

#[cfg(test)]
mod batch_d_d10_tests {
    use super::*;
    use crate::routes::project::{FORBIDDEN_MSG, NOT_FOUND_MSG};

    #[test]
    fn no_project_maps_to_django_400_verbatim() {
        // `create_draft_to_issue` (`draft.py:209-212`):
        // `if not draft_issue.project_id: return 400
        // {"error": "Project is required to create an issue."}`.
        assert_eq!(
            draft_to_issue_project(None).unwrap_err(),
            "Project is required to create an issue."
        );
        assert_eq!(NO_PROJECT_MSG, "Project is required to create an issue.");
        let pid = uuid::Uuid::nil();
        assert_eq!(draft_to_issue_project(Some(pid)).unwrap(), pid);
    }

    #[test]
    fn patch_miss_is_non_standard_verbatim() {
        // PATCH miss → 404 `{"error":"Issue not found"}` (NON-standard,
        // `draft.py:166`) — differs from the standard `missing()` body used
        // by retrieve/destroy.
        assert_eq!(PATCH_MISS_MSG, "Issue not found");
        let (status, body) = patch_miss();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body.0, json!({"error": "Issue not found"}));
        let (_, std_body) = missing();
        assert_eq!(std_body.0, json!({"error": NOT_FOUND_MSG}));
        assert_ne!(body.0, std_body.0);
    }

    #[test]
    fn start_date_validation_matches_django() {
        // `DraftIssueCreateSerializer.validate`
        // (`serializers/draft.py:72-77`): start>target → Err with the exact
        // message (no period).
        let s = chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        let t = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(
            validate_dates(Some(s), Some(t)).unwrap_err(),
            "Start date cannot exceed target date"
        );
        assert!(validate_dates(Some(t), Some(s)).is_ok());
        assert!(validate_dates(None, Some(s)).is_ok());
        assert!(validate_dates(Some(s), None).is_ok());
    }

    #[test]
    fn workspace_gates_match_django_decorators() {
        // list/create AMG (`draft.py:98,111`); PATCH ADMIN+MEMBER
        // (`draft.py:156`); retrieve ADMIN (`draft.py:186`); convert
        // ADMIN+MEMBER (`draft.py:205`) — all WORKSPACE level
        // (`permissions/base.py:44-51`).
        for r in [Some(20), Some(15), Some(5)] {
            assert!(guard_list_create(r).is_ok());
        }
        assert_eq!(guard_list_create(None).unwrap_err(), FORBIDDEN_MSG);
        assert!(guard_patch(Some(20)).is_ok());
        assert!(guard_patch(Some(15)).is_ok());
        assert!(guard_patch(Some(5)).is_err());
        assert!(guard_patch(None).is_err());
        assert!(guard_retrieve(Some(20)).is_ok());
        assert!(guard_retrieve(Some(15)).is_err());
        assert!(guard_retrieve(Some(5)).is_err());
        assert!(guard_retrieve(None).is_err());
        assert!(guard_convert(Some(20)).is_ok());
        assert!(guard_convert(Some(15)).is_ok());
        assert!(guard_convert(Some(5)).is_err());
        assert!(guard_convert(None).is_err());
    }

    #[test]
    fn destroy_gate_is_admin_or_creator() {
        // `@allow_permission([ADMIN], creator=True, model=DraftIssue)`
        // (`draft.py:199`): ACTIVE ws member AND (creator OR ws ADMIN).
        // GUEST creator passes (model=DraftIssue makes the bypass real —
        // unlike PATCH/retrieve which check model=Issue).
        assert!(may_destroy(Some(20), false));
        assert!(may_destroy(Some(20), true));
        assert!(may_destroy(Some(15), true));
        assert!(may_destroy(Some(5), true));
        assert!(!may_destroy(Some(15), false));
        assert!(!may_destroy(Some(5), false));
        assert!(!may_destroy(None, true));
        assert!(!may_destroy(None, false));
    }

    #[test]
    fn convert_name_validation_matches_drf() {
        // `Issue.name` has no `blank=True` (`db/models/issue.py:136`):
        // missing → required, blank → may-not-be-blank (D8 precedent).
        assert_eq!(
            validate_convert_name(None).unwrap_err(),
            json!({"name": ["This field is required."]})
        );
        assert_eq!(
            validate_convert_name(Some("  ")).unwrap_err(),
            json!({"name": ["This field may not be blank."]})
        );
        assert_eq!(validate_convert_name(Some("Bug")).unwrap(), "Bug");
    }

    #[test]
    fn draft_json_covers_all_serializer_keys() {
        // Mirrors `DraftIssueSerializer` (`serializers/draft.py:300-335`):
        // 21 keys (detail serializer dedupes the repeated
        // `description_html`, `draft.py:337-342`).
        let row = DraftRow {
            id: uuid::Uuid::nil(),
            name: Some("Draft".to_string()),
            state_id: None,
            sort_order: 65535.0,
            completed_at: None,
            estimate_point: None,
            priority: "none".to_string(),
            start_date: None,
            target_date: None,
            project_id: Some(uuid::Uuid::nil()),
            parent_id: None,
            cycle_id: None,
            module_ids: vec![],
            label_ids: vec![],
            assignee_ids: vec![],
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by: Some(uuid::Uuid::nil()),
            updated_by: None,
            type_id: None,
            description_html: "<p></p>".to_string(),
        };
        let v = draft_json(&row);
        for key in [
            "id",
            "name",
            "state_id",
            "sort_order",
            "completed_at",
            "estimate_point",
            "priority",
            "start_date",
            "target_date",
            "project_id",
            "parent_id",
            "cycle_id",
            "module_ids",
            "label_ids",
            "assignee_ids",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "type_id",
            "description_html",
        ] {
            assert!(v.get(key).is_some(), "DraftIssue missing key {key}");
        }
        assert!(v.get("state_id").unwrap().is_null());
        assert_eq!(v.get("description_html"), Some(&json!("<p></p>")));
    }
}
