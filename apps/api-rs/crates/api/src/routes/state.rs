use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Mirrors `plane/app/serializers/state.py:StateSerializer.validate`
/// + `plane/db/models/state.py:StateGroup`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateState {
    pub name: String,
    pub group: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub group: String,
}

pub const ALLOWED_GROUPS: &[&str] = &[
    "backlog",
    "unstarted",
    "started",
    "completed",
    "cancelled",
];

pub fn validate_create(body: &CreateState) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if body.group == "triage" {
        return Err("Cannot create triage state".to_string());
    }
    if !ALLOWED_GROUPS.contains(&body.group.as_str()) {
        return Err(format!("unknown group {}", body.group));
    }
    if body.color.trim().is_empty() {
        return Err("color is required".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StateListQuery {
    #[serde(default)]
    pub grouped: Option<String>,
}

/// AMG gate for state reads/writes that allow guests (`list`, `patch`):
/// roles 20/15/5 pass outright, plus any active member who is a workspace
/// ADMIN (shared fallback, `permissions/base.py:64-78`).
async fn gate_state_amg(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ))
}

/// ADMIN-only gate for state writes (`create`, `destroy`, `mark_default`):
/// role 20 passes outright, plus the shared ws-admin fallback.
async fn gate_state_admin(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(matches!(member_role, Some(20)), member_role.is_some(), ws_admin))
}

/// Full-row SELECT prefix for the 9 persisted `StateSerializer` keys
/// (`serializers/state.py:12-30`), aliased for [`StateFullRow`].
const STATE_FULL_SELECT_SQL: &str = "SELECT s.id, s.project_id, s.workspace_id, s.name, s.color, s.\"group\", s.\"default\" AS is_default, s.description, s.sequence FROM states s";

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<StateListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `list` (`state/base.py:77`): ADMIN/MEMBER/GUEST.
    if !gate_state_amg(&st.pool, auth.0, &slug, _project_id).await? {
        return Ok(deny());
    }
    // Triage excluded (`get_queryset` `is_triage=False`, `base.py:39`).
    let rows: Vec<StateFullRow> = sqlx::query_as(&format!(
        "{STATE_FULL_SELECT_SQL} WHERE s.project_id = $1 AND s.deleted_at IS NULL AND s.is_triage = false ORDER BY s.sequence ASC"
    ))
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    // `order = index / group_count`, 1-based within each group
    // (`base.py:82-88`) — present on the flat list too (the loop mutates
    // `states` before the `grouped` check).
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in &rows {
        *counts.entry(r.group.clone()).or_default() += 1;
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut ordered: Vec<(String, Value)> = Vec::with_capacity(rows.len());
    for r in &rows {
        let n = seen.entry(r.group.clone()).or_insert(0);
        *n += 1;
        let count = counts.get(&r.group).copied().unwrap_or(1).max(1) as f64;
        ordered.push((r.group.clone(), state_serializer_json(r, Some(*n as f64 / count))));
    }
    // `?grouped=true` dict mode (`base.py:91-100`).
    if q.grouped.as_deref() == Some("true") {
        let mut dict = serde_json::Map::new();
        for (group, val) in ordered {
            let slot = dict.entry(group).or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(items) = slot {
                items.push(val);
            }
        }
        return Ok((StatusCode::OK, Json(Value::Object(dict))));
    }
    Ok((StatusCode::OK, Json(Value::Array(ordered.into_iter().map(|(_, v)| v).collect()))))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateState>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `create` (`state/base.py:46`): ADMIN-only.
    if !gate_state_admin(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Django serializer 400s (never 500s) on invalid bodies; our messages
    // stay custom, the status now matches.
    if let Err(e) = validate_create(&body) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    // Django dup-name 400 (`state/base.py:54-59`): `{"name": [...]}` shape.
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM states WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&body.name)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"name": "The state name is already taken"})),
        ));
    }
    let created: StateFullRow = sqlx::query_as(
        "WITH ins AS (INSERT INTO states (id, name, description, \"group\", color, project_id, workspace_id, slug, sequence, \"default\", is_triage, created_at, updated_at) SELECT gen_random_uuid(), $1, '', $2, $3, p.id, p.workspace_id, lower(regexp_replace($1, '[^a-zA-Z0-9]+', '-', 'g')), COALESCE((SELECT MAX(sequence) FROM states WHERE project_id = p.id), 0) + 15000, false, false, now(), now() FROM projects p WHERE p.id = $4 RETURNING id) SELECT s.id, s.project_id, s.workspace_id, s.name, s.color, s.\"group\", s.\"default\" AS is_default, s.description, s.sequence FROM states s JOIN ins ON ins.id = s.id",
    )
    .bind(&body.name)
    .bind(&body.group)
    .bind(&body.color)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    // Django `create` (`state/base.py:146`) returns 200 (not 201).
    Ok((StatusCode::OK, Json(state_serializer_json(&created, None))))
}

/// Mirrors `plane/app/views/state/base.py:destroy`: default states and
/// non-empty states cannot be deleted.
pub fn guard_delete(is_default: bool, issue_count: i64) -> Result<(), String> {
    if is_default {
        return Err("Default state cannot be deleted".to_string());
    }
    if issue_count > 0 {
        return Err("The state is not empty, only empty states can be deleted".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchState {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    // Django `retrieve` is DRF-default with auth-only permission
    // (`views/base.py:50`): no gate. Miss → 404 `missing()` (DRF
    // `{"detail"}` normalized per repo rule; unifies "State not found").
    let row: Option<StateFullRow> = sqlx::query_as(&format!(
        "{STATE_FULL_SELECT_SQL} WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(s) => Ok((StatusCode::OK, Json(state_serializer_json(&s, None)))),
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchState>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    // Django `partial_update` (`state/base.py:61`): ADMIN/MEMBER/GUEST.
    if !gate_state_amg(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
        // Django dup-name 400 (`state/base.py:61-75`): `{"name": [...]}` shape.
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM states WHERE project_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL)",
        )
        .bind(project_id)
        .bind(name)
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if dup {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"name": "The state name is already taken"})),
            ));
        }
    }
    if let Some(group) = &body.group {
        if group == "triage" || !ALLOWED_GROUPS.contains(&group.as_str()) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid group"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE states SET name = COALESCE($1, name), \"group\" = COALESCE($2, \"group\"), color = COALESCE($3, color), updated_at = now() WHERE id = $4 AND project_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.group)
    .bind(&body.color)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    // Django returns 200 full `StateSerializer`.
    let row: Option<StateFullRow> = sqlx::query_as(&format!(
        "{STATE_FULL_SELECT_SQL} WHERE s.id = $1 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(s) => Ok((StatusCode::OK, Json(state_serializer_json(&s, None)))),
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    // Django `destroy` (`state/base.py:113`): ADMIN-only.
    if !gate_state_admin(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Triage states are outside the lookup scope (`base.py:115`
    // `is_triage=False`): a triage pk 404s.
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT \"default\" FROM states WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL AND is_triage = false",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((is_default,)) = row else {
        return Ok(missing());
    };
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE state_id = $1 AND deleted_at IS NULL")
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
    if let Err(e) = guard_delete(is_default, count.0) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    sqlx::query("UPDATE states SET deleted_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}

/// Mirrors `plane/app/views/state/base.py:104-106` (`mark_as_default`):
/// `@allow_permission([ROLE.ADMIN])` (level PROJECT) — project ADMIN (20)
/// passes outright, plus any active project member who is a workspace ADMIN
/// (shared fallback, `permissions/base.py:53-78`; FE-exercised via
/// `markDefault`, K4-class primer).
/// (`ROLE` values from `plane/app/permissions/base.py:13-16`.)
pub fn guard_mark_default(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// POST `/api/workspaces/:slug/projects/:project_id/states/:pk/mark-default/`
/// — parity with Django `StateViewSet.mark_as_default`
/// (`plane/app/views/state/base.py:104-110`).
///
/// - Gate: PROJECT ADMIN (20) outright + shared ws-admin fallback via
///   `fetch_project_member_role` + `guard_mark_default`;
///   MEMBER (15) / GUEST (5) / non-member → 403 `deny()`.
/// - Two blind updates, both scoped to (workspace slug + project_id), in a
///   single tx: clear `"default"` where true, then set `"default"=true`
///   where pk (`base.py:108-109`).
/// - **204 ALWAYS** — even when pk matches 0 rows (no existence check;
///   `base.py:108-109` are blind `.update()` calls whose row counts are
///   discarded).
///
/// Queryset filters mirror Django exactly: `State.objects.filter(...)`
/// uses the default `StateManager` (`plane/db/models/state.py:65-69`),
/// which is `SoftDeletionManager` (`plane/db/mixins.py:56-58`,
/// `deleted_at IS NULL`) + `.exclude(group="triage")`. So BOTH updates
/// carry `AND deleted_at IS NULL AND "group" != 'triage'` — triage states
/// are skipped on clear AND set (a triage pk yields 204 with no change).
/// `updated_at` is NOT bumped: Django `QuerySet.update()` bypasses
/// `save()`/`auto_now`, unlike the `patch`/`archive` paths.
///
/// Deviations: single explicit transaction (Django runs two autocommit
/// UPDATEs); no cache invalidation — Rust has no cache layer (Django
/// `@invalidate_cache(path="workspaces/:slug/states/")`, `base.py:104`).
pub async fn mark_default(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(guard_mark_default(role).is_ok(), role.is_some(), ws_admin) {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE states SET \"default\" = false WHERE project_id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND \"default\" = true AND deleted_at IS NULL AND \"group\" != 'triage'",
    )
    .bind(project_id)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE states SET \"default\" = true WHERE id = $1 AND project_id = $2 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL AND \"group\" != 'triage'",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}

/// Quoted verbatim from `plane/app/views/state/base.py`
/// (`IntakeStateEndpoint.get`): triage-state miss → 404 with this body.
pub(crate) const TRIAGE_MISS_MSG: &str = "Triage state not found";

/// PROJECT-level role check for `intake_state`: mirrors
/// `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])` (level
/// PROJECT is the decorator default, `plane/app/permissions/base.py:17`)
/// — roles 20/15/5 pass; non-member → 403. The workspace-ADMIN fallback
/// (`permissions/base.py:64-78`) is applied by the caller via the shared
/// `project_gate_allows`, exactly like D2 `history`/`meta`.
pub(crate) fn guard_intake_state(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// Full `StateSerializer` row (`plane/app/serializers/state.py:12-30`:
/// `id,project_id,workspace_id,name,color,group,default,description,
/// sequence` + view-computed `order`). The existing
/// `common::models::state::State` covers only `{id,name,group}` — a real
/// delta (plan D3 locked fact), so the 9 persisted keys are selected here;
/// `order` is never a column (`order = FloatField(required=False)` on the
/// serializer only) and is passed separately to `state_serializer_json`.
/// (`"default"` is aliased to `is_default`: `default` is a Rust keyword.)
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct StateFullRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) group: String,
    pub(crate) is_default: bool,
    pub(crate) description: String,
    pub(crate) sequence: f64,
}

/// Serializes one `StateFullRow` like `StateSerializer`
/// (`serializers/state.py:12-30`): FKs render as id strings (DRF default
/// PK representation), datetimes unneeded here (no datetime keys in the
/// shape). `order=None` (D3a single) omits the key — DRF raises
/// `SkipField` for the unset non-required `FloatField`; `Some` (D3b list)
/// emits it. Key ORDER on the wire follows repo batch convention
/// (serde_json map order), while the KEY SET matches Django exactly.
pub(crate) fn state_serializer_json(row: &StateFullRow, order: Option<f64>) -> Value {
    let mut obj = json!({
        "id": row.id,
        "project_id": row.project_id,
        "workspace_id": row.workspace_id,
        "name": row.name,
        "color": row.color,
        "group": row.group,
        "default": row.is_default,
        "description": row.description,
        "sequence": row.sequence,
    });
    if let Some(o) = order {
        obj["order"] = json!(o);
    }
    obj
}

/// Mirrors `plane/app/views/workspace/state.py:30-37`:
/// `state.order = index / count` with 1-based `index` within its `group`.
pub(crate) fn state_order(index_1based: usize, group_count: usize) -> f64 {
    index_1based as f64 / group_count as f64
}

/// GET `/api/workspaces/:slug/projects/:project_id/intake-state/` —
/// parity with Django `IntakeStateEndpoint.get`
/// (`plane/app/views/state/base.py:136-...`,
/// `plane/app/urls/state.py:22-26`).
///
/// - Gate: PROJECT ADMIN/MEMBER/GUEST via the shared helpers (same gate
///   as D2 `history`/`meta`: allowed roles 20/15/5 outright + active
///   project member who is a workspace ADMIN).
/// - Scope mirrors `State.triage_objects.filter(workspace__slug=slug,
///   project_id=project_id).first()`: `TriageStateManager` =
///   soft-deletion (`deleted_at IS NULL`) + `group='triage'`
///   (`plane/db/models/state.py:72-77`). `.first()` orders by pk when
///   unordered — meaningless for uuid pks, so `created_at ASC` is used
///   for determinism (exactly one triage row per project in practice).
/// - Miss → 404 `{"error":"Triage state not found"}` verbatim (NOT the
///   standard `missing()`); hit → 200 `StateSerializer` WITHOUT `order`.
pub async fn intake_state(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        guard_intake_state(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let row: Option<StateFullRow> = sqlx::query_as(
        "SELECT s.id, s.project_id, s.workspace_id, s.name, s.color, s.\"group\", s.\"default\" AS is_default, s.description, s.sequence \
         FROM states s JOIN workspaces w ON w.id = s.workspace_id \
         WHERE w.slug = $1 AND s.project_id = $2 AND s.deleted_at IS NULL AND s.\"group\" = 'triage' \
         ORDER BY s.created_at ASC LIMIT 1",
    )
    .bind(&slug)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(state_serializer_json(&r, None)))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": TRIAGE_MISS_MSG})),
        )),
    }
}

#[cfg(test)]
mod state_mark_default_tests {
    use super::*;

    #[test]
    fn mark_default_guard_allows_admin_only() {
        // Mirrors `plane/app/views/state/base.py:104-110` (`mark_as_default`):
        // `@allow_permission([ROLE.ADMIN])` (level PROJECT) — only project
        // ADMIN (20) passes; MEMBER (15) / GUEST (5) / non-member → 403.
        assert!(guard_mark_default(Some(20)).is_ok());
        assert!(guard_mark_default(Some(15)).is_err());
        assert!(guard_mark_default(Some(5)).is_err());
        assert!(guard_mark_default(None).is_err());
    }
}

#[cfg(test)]
mod batch_d_d3_tests {
    use super::*;

    fn sample_row() -> StateFullRow {
        StateFullRow {
            id: uuid::Uuid::nil(),
            project_id: uuid::Uuid::nil(),
            workspace_id: uuid::Uuid::nil(),
            name: "Triage".to_string(),
            color: "#4E5355".to_string(),
            group: "triage".to_string(),
            is_default: false,
            description: "".to_string(),
            sequence: 65000.0,
        }
    }

    #[test]
    fn triage_miss_msg_verbatim() {
        // `plane/app/views/state/base.py` (`IntakeStateEndpoint.get`):
        // miss → 404 `{"error": "Triage state not found"}` verbatim.
        assert_eq!(TRIAGE_MISS_MSG, "Triage state not found");
    }

    #[test]
    fn intake_gate_allows_admin_member_guest() {
        // `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
        // (level PROJECT default, `permissions/base.py:17`); non-member →
        // 403 `deny()`. (The ws-admin fallback branch lives in the async
        // handler via shared `project_gate_allows`, same as D2.)
        assert!(guard_intake_state(Some(20)).is_ok());
        assert!(guard_intake_state(Some(15)).is_ok());
        assert!(guard_intake_state(Some(5)).is_ok());
        assert!(guard_intake_state(None).is_err());
    }

    #[test]
    fn state_order_matches_django_group_math() {
        // `plane/app/views/workspace/state.py:30-37`:
        // `state.order = index / count` with 1-based `index`.
        assert_eq!(state_order(1, 3), 1.0 / 3.0);
        assert_eq!(state_order(3, 3), 1.0);
    }

    #[test]
    fn state_serializer_single_omits_order() {
        // `StateSerializer.order` is `FloatField(required=False)`
        // (`serializers/state.py:15`): unset on a single object → DRF
        // `SkipField` → key absent on the wire (D3a). The D3b loop sets it
        // per state → key present (D3b).
        let row = sample_row();
        let single = state_serializer_json(&row, None);
        assert!(single.get("order").is_none());
        assert_eq!(single["name"], serde_json::json!("Triage"));
        assert_eq!(single["group"], serde_json::json!("triage"));
        assert_eq!(single["sequence"], serde_json::json!(65000.0));
        let listed = state_serializer_json(&row, Some(0.5));
        assert_eq!(listed["order"], serde_json::json!(0.5));
    }
}
