use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::routes::project::{deny, missing};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Project + cycle + module user-properties — parity with Django
/// `ProjectUserDisplayPropertyEndpoint`
/// (`plane/app/views/issue/base.py:743-770`,
/// `plane/app/urls/issue.py:217-221`), `CycleUserPropertiesEndpoint`
/// (`plane/app/views/cycle/base.py:625-655`,
/// `plane/app/urls/cycle.py:77-81`) and `ModuleUserPropertiesEndpoint`
/// (`plane/app/views/module/base.py:825-855`,
/// `plane/app/urls/module.py:86-90`). Celery/activity side-effects skipped
/// (Batch C precedent — these endpoints emit none anyway).
///
/// Live columns verified 2026-09-06 via
/// `docker exec plane-db psql -U plane -d plane -c "\d project_user_properties"`,
/// `"\d cycle_user_properties"`, `"\d module_user_properties"`:
/// - `project_user_properties`: id, created_at, updated_at, created_by_id,
///   updated_by_id, deleted_at, workspace_id, project_id, user_id, filters,
///   display_filters, display_properties, rich_filters, preferences,
///   sort_order (all jsonb `NOT NULL`, sort_order float `NOT NULL`).
/// - `cycle_user_properties`: same minus preferences/sort_order, plus
///   cycle_id (`NOT NULL`).
/// - `module_user_properties`: same minus preferences/sort_order, plus
///   module_id (`NOT NULL`).
/// Partial unique indexes enforce one live row per (user, scope):
/// `project_user_property_unique_user_project_when_deleted_at_null`,
/// `cycle_user_properties_unique_cycle_user_when_deleted_at_null`,
/// `module_user_properties_unique_module_user_when_deleted_at_null`.
///
/// Locked deltas (plan D4): project PATCH → 200 with the row AUTO-CREATED
/// if missing (`get` + `except DoesNotExist: create`, `base.py:747-755` —
/// never 404); cycle PATCH → **201**, missing row → `.get()` 404
/// `missing()`; module PATCH → **201**, missing row → 404 `missing()`;
/// cycle/module PATCH merge ONLY the 4 keys
/// (`filters,rich_filters,display_filters,display_properties`, absent key
/// keeps the old value, `cycle/base.py:635-642`); project PATCH goes
/// through the partial serializer (`base.py:757-766`) so every WRITABLE key
/// (`filters,display_filters,display_properties,rich_filters,preferences,
/// sort_order` — everything except read-only `user,workspace,project`,
/// `serializers/issue.py:354-358`) is replaced when present, unknown keys
/// ignored (DRF only reads declared writable fields). GETs →
/// 200 (`get_or_create`). All gates ADMIN/MEMBER/GUEST. NO POST anywhere
/// (the FE's literal `issue-display-properties/` POST is FE-dead — Django
/// defines no such path, only `user-properties/` GET+PATCH).

/// Quoted from `plane/app/views/base.py:100-104` (Django `ValidationError`
/// → 400; hit here when project PATCH carries a non-numeric `sort_order`,
/// mirroring the serializer `FloatField` rejection).
pub(crate) const INVALID_DETAIL_MSG: &str = "Please provide valid detail";
/// Quoted from `plane/app/views/base.py:92-97` (Django `IntegrityError` →
/// 400; hit here when an explicit JSON null reaches a `NOT NULL` column,
/// mirroring e.g. `filters=None` assignment + `save()`).
pub(crate) const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";

/// PROJECT-level role check shared by all six handlers: mirrors
/// `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])` (level
/// PROJECT is the decorator default, `plane/app/permissions/base.py:17`)
/// — roles 20/15/5 pass; anything else (incl. non-member) falls to the
/// workspace-ADMIN fallback applied by the caller via the shared
/// `project_gate_allows`, exactly like D2 `history`/`meta` and D3.
pub(crate) fn guard_userprops(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// Pure encoding of the plan's locked 404-vs-create distinction:
/// a PATCH that finds no live row auto-creates (then 200) ONLY for the
/// `"project"` scope (`issue/base.py:747-755`); `"cycle"`/`"module"`
/// scopes 404 via `.get()` (`cycle/base.py:628-633`,
/// `module/base.py:828-833`).
pub(crate) fn missing_prop_patch(scope: &str) -> StatusCode {
    match scope {
        "project" => StatusCode::OK,
        _ => StatusCode::NOT_FOUND,
    }
}

/// Mirrors `request.data.get(key, current)` (`cycle/base.py:635-642`,
/// `module/base.py:835-842`): a present key (even explicit null, exactly
/// like Django's passthrough assignment) wins; an absent key keeps the
/// old value.
pub(crate) fn pick_or(body: &Value, key: &str, current: &Value) -> Value {
    body.get(key).cloned().unwrap_or_else(|| current.clone())
}

/// Mirrors the project PATCH serializer validation
/// (`serializers/issue.py:354-358` over `ProjectUserPropertySerializer`):
/// every writable key is a free-form JSONField EXCEPT `sort_order`
/// (`FloatField`) — a present-but-non-numeric `sort_order` (incl. explicit
/// null, which the non-nullable field rejects) → Django
/// `ValidationError` → 400 `INVALID_DETAIL_MSG`.
pub(crate) fn validate_project_prop_patch(body: &Value) -> Result<(), String> {
    match body.get("sort_order") {
        Some(v) if v.as_f64().is_none() => Err(INVALID_DETAIL_MSG.to_string()),
        _ => Ok(()),
    }
}

/// Shared PROJECT-level gate for all six handlers (see `guard_userprops`).
async fn gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    Ok(project_gate_allows(
        guard_userprops(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ))
}

/// Maps Postgres constraint violations (class 23) to Django's
/// `IntegrityError` → 400 `PAYLOAD_INVALID_MSG` (`views/base.py:92-97`);
/// any other DB failure propagates as 500 via `AppError`.
fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

/// Default `filters` ala Django `get_default_filters()`
/// (`db/models/issue.py:47-58`) — also the live-verified shape of existing
/// rows. Same defaults feed project/cycle/module creation (all three
/// models share the getters).
fn default_filters() -> Value {
    json!({
        "priority": null, "state": null, "state_group": null,
        "assignees": null, "created_by": null, "labels": null,
        "start_date": null, "target_date": null, "subscriber": null,
    })
}

/// Default `display_filters` ala Django `get_default_display_filters()`
/// (`db/models/issue.py:61-70`).
fn default_display_filters() -> Value {
    json!({
        "group_by": null, "order_by": "-created_at", "type": null,
        "sub_issue": true, "show_empty_groups": true,
        "layout": "list", "calendar_date_range": "",
    })
}

/// Default `display_properties` ala Django
/// `get_default_display_properties()` (`db/models/issue.py:73-88`).
fn default_display_properties() -> Value {
    json!({
        "assignee": true, "attachment_count": true, "created_on": true,
        "due_date": true, "estimate": true, "key": true, "labels": true,
        "link": true, "priority": true, "start_date": true, "state": true,
        "sub_issue_count": true, "updated_on": true,
    })
}

/// Default `preferences` ala Django `get_default_preferences()`
/// (`db/models/project.py:64-65`) — project scope ONLY (cycle/module
/// have no such column).
fn default_preferences() -> Value {
    json!({"pages": {"block_display": true}, "navigation": {"default_tab": "work_items", "hide_in_more_menu": []}})
}

fn opt_id(id: &Option<uuid::Uuid>) -> Value {
    id.map(|u| json!(u)).unwrap_or(Value::Null)
}

/// One `project_user_properties` row. Field names match the SELECT aliases
/// below (`*_id` for FK columns, values for jsonb/float).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ProjectPropRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) user_id: uuid::Uuid,
    pub(crate) filters: Value,
    pub(crate) display_filters: Value,
    pub(crate) display_properties: Value,
    pub(crate) rich_filters: Value,
    pub(crate) preferences: Value,
    pub(crate) sort_order: f64,
}

/// Serializes one `ProjectPropRow` like `ProjectUserPropertySerializer`
/// (`serializers/issue.py:354-358`, `fields = "__all__"`): every model
/// column with FKs as id strings (DRF default PK representation;
/// `project`/`workspace`/`user` are non-nullable → bare ids,
/// `created_by`/`updated_by`/`deleted_at` nullable → null when unset).
/// Key ORDER follows repo batch convention while the KEY SET matches
/// Django exactly (documented non-divergence, same as D1/D2).
pub(crate) fn project_prop_json(row: &ProjectPropRow) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "project": row.project_id,
        "workspace": row.workspace_id,
        "user": row.user_id,
        "filters": row.filters,
        "display_filters": row.display_filters,
        "display_properties": row.display_properties,
        "rich_filters": row.rich_filters,
        "preferences": row.preferences,
        "sort_order": row.sort_order,
    })
}

/// One `cycle_user_properties` row (no preferences/sort_order columns —
/// verified live).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CyclePropRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) cycle_id: uuid::Uuid,
    pub(crate) user_id: uuid::Uuid,
    pub(crate) filters: Value,
    pub(crate) display_filters: Value,
    pub(crate) display_properties: Value,
    pub(crate) rich_filters: Value,
}

/// Serializes one `CyclePropRow` like `CycleUserPropertiesSerializer`
/// (`serializers/cycle.py:102-106`, `fields = "__all__"`).
pub(crate) fn cycle_prop_json(row: &CyclePropRow) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "project": row.project_id,
        "workspace": row.workspace_id,
        "cycle": row.cycle_id,
        "user": row.user_id,
        "filters": row.filters,
        "display_filters": row.display_filters,
        "display_properties": row.display_properties,
        "rich_filters": row.rich_filters,
    })
}

/// One `module_user_properties` row (no preferences/sort_order columns —
/// verified live).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ModulePropRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) module_id: uuid::Uuid,
    pub(crate) user_id: uuid::Uuid,
    pub(crate) filters: Value,
    pub(crate) display_filters: Value,
    pub(crate) display_properties: Value,
    pub(crate) rich_filters: Value,
}

/// Serializes one `ModulePropRow` like `ModuleUserPropertiesSerializer`
/// (`serializers/module.py:276-280`, `fields = "__all__"`).
pub(crate) fn module_prop_json(row: &ModulePropRow) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "project": row.project_id,
        "workspace": row.workspace_id,
        "module": row.module_id,
        "user": row.user_id,
        "filters": row.filters,
        "display_filters": row.display_filters,
        "display_properties": row.display_properties,
        "rich_filters": row.rich_filters,
    })
}

const PROJECT_COLS: &str = "id, created_at, updated_at, created_by_id, updated_by_id, \
    deleted_at, workspace_id, project_id, user_id, filters, display_filters, \
    display_properties, rich_filters, preferences, sort_order";

const CYCLE_COLS: &str = "id, created_at, updated_at, created_by_id, updated_by_id, \
    deleted_at, workspace_id, project_id, cycle_id, user_id, filters, \
    display_filters, display_properties, rich_filters";

const MODULE_COLS: &str = "id, created_at, updated_at, created_by_id, updated_by_id, \
    deleted_at, workspace_id, project_id, module_id, user_id, filters, \
    display_filters, display_properties, rich_filters";

/// Live project row for the caller, mirroring `.get(user, project_id)`
/// (`issue/base.py:747-750`) — default-manager semantics
/// (`deleted_at IS NULL`), NO slug scope (Django filters none here; the
/// gate already pins slug↔project).
async fn fetch_project_row(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    project_id: uuid::Uuid,
) -> Result<Option<ProjectPropRow>, sqlx::Error> {
    sqlx::query_as::<_, ProjectPropRow>(&format!(
        "SELECT {PROJECT_COLS} FROM project_user_properties \
        WHERE user_id = $1 AND project_id = $2 AND deleted_at IS NULL"
    ))
    .bind(user_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

/// Inserts the project row with Django's model defaults (see
/// `default_*`), mirroring `objects.create(user, project_id)`
/// (`issue/base.py:751-754`; `workspace` derived from the project exactly
/// like `ProjectBaseModel.save`, `db/models/project.py:180-189`). The
/// slug scope pins creation to the URL workspace (harmless post-gate —
/// the gate already proved membership in exactly this slug↔project pair).
/// `ON CONFLICT DO NOTHING` (bare, same as the `users_me::join_projects`
/// precedent) absorbs a lost get_or_create race; the caller re-reads.
async fn insert_project_row(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    project_id: uuid::Uuid,
    slug: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO project_user_properties (id, user_id, project_id, workspace_id, \
        filters, display_filters, display_properties, rich_filters, preferences, \
        sort_order, created_by_id, created_at, updated_at) \
        SELECT gen_random_uuid(), $1, $2, p.workspace_id, $3, $4, $5, '{}'::jsonb, $6, \
        65535, $1, now(), now() \
        FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
        WHERE p.id = $2 AND w.slug = $7 AND p.deleted_at IS NULL \
        ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(project_id)
    .bind(default_filters())
    .bind(default_display_filters())
    .bind(default_display_properties())
    .bind(default_preferences())
    .bind(slug)
    .execute(pool)
    .await?;
    Ok(())
}

/// GET `/api/workspaces/:slug/projects/:project_id/user-properties/` —
/// parity with `ProjectUserDisplayPropertyEndpoint.get`
/// (`issue/base.py:767-770`): `get_or_create` → always 200, never 404.
pub async fn project_props_get(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if fetch_project_row(&st.pool, auth.0, project_id).await?.is_none() {
        insert_project_row(&st.pool, auth.0, project_id, &slug).await?;
    }
    // Unreachable post-gate (the gate's slug-scoped membership lookup
    // already proved the project exists), but Django would 500 on a
    // missing project while Rust returns the standard 404 instead
    // (same sane-deviation precedent as `subscribe::subscribe`).
    match fetch_project_row(&st.pool, auth.0, project_id).await? {
        Some(row) => Ok((StatusCode::OK, Json(project_prop_json(&row)))),
        None => Ok(missing()),
    }
}

/// PATCH `/api/workspaces/:slug/projects/:project_id/user-properties/` —
/// parity with `ProjectUserDisplayPropertyEndpoint.patch`
/// (`issue/base.py:743-765`): get-or-create (never 404), partial
/// serializer update, **200**. Deviations: datetimes RFC3339 (batch
/// convention); key ORDER is repo convention while the KEY SET matches
/// Django exactly.
pub async fn project_props_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if let Err(msg) = validate_project_prop_patch(&body) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": msg}))));
    }
    if fetch_project_row(&st.pool, auth.0, project_id).await?.is_none() {
        insert_project_row(&st.pool, auth.0, project_id, &slug).await?;
    }
    let Some(cur) = fetch_project_row(&st.pool, auth.0, project_id).await? else {
        // `missing_prop_patch("project") == 200`: the project scope
        // auto-creates instead of 404ing, so this arm is unreachable
        // post-gate (kept for DB-race safety).
        debug_assert_eq!(missing_prop_patch("project"), StatusCode::OK);
        return Ok(missing());
    };
    let sort_order: f64 = body
        .get("sort_order")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(cur.sort_order);
    let updated: Result<Option<ProjectPropRow>, sqlx::Error> = sqlx::query_as::<_, ProjectPropRow>(
        &format!(
            "UPDATE project_user_properties SET filters = $1, display_filters = $2, \
            display_properties = $3, rich_filters = $4, preferences = $5, sort_order = $6, \
            updated_by_id = $7, updated_at = now() WHERE id = $8 RETURNING {PROJECT_COLS}"
        ),
    )
    .bind(pick_or(&body, "filters", &cur.filters))
    .bind(pick_or(&body, "display_filters", &cur.display_filters))
    .bind(pick_or(
        &body,
        "display_properties",
        &cur.display_properties,
    ))
    .bind(pick_or(&body, "rich_filters", &cur.rich_filters))
    .bind(pick_or(&body, "preferences", &cur.preferences))
    .bind(sort_order)
    .bind(auth.0)
    .bind(cur.id)
    .fetch_optional(&st.pool)
    .await;
    match updated {
        Ok(Some(row)) => Ok((StatusCode::OK, Json(project_prop_json(&row)))),
        Ok(None) => Ok(missing()),
        Err(e) if is_constraint_violation(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Live cycle row for the caller, mirroring `.get(user, cycle_id,
/// project_id, workspace__slug)` (`cycle/base.py:628-633`) — miss (incl.
/// soft-deleted) → 404 `missing()` (Django `DoesNotExist` →
/// `views/base.py:107-111`).
async fn fetch_cycle_row(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    cycle_id: uuid::Uuid,
    project_id: uuid::Uuid,
    slug: &str,
) -> Result<Option<CyclePropRow>, sqlx::Error> {
    sqlx::query_as::<_, CyclePropRow>(&format!(
        "SELECT {CYCLE_COLS} FROM cycle_user_properties c \
        JOIN workspaces w ON w.id = c.workspace_id \
        WHERE c.user_id = $1 AND c.cycle_id = $2 AND c.project_id = $3 \
        AND w.slug = $4 AND c.deleted_at IS NULL"
    ))
    .bind(user_id)
    .bind(cycle_id)
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// GET `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/user-properties/`
/// — parity with `CycleUserPropertiesEndpoint.get`
/// (`cycle/base.py:645-655`): `get_or_create` → 200. Creation requires
/// the cycle to exist in the URL workspace (Django would 500 on a missing
/// cycle's FK violation; Rust returns the standard 404 instead — same
/// sane-deviation precedent as `subscribe::subscribe`).
pub async fn cycle_props_get(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, cycle_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if fetch_cycle_row(&st.pool, auth.0, cycle_id, project_id, &slug)
        .await?
        .is_none()
    {
        sqlx::query(
            "INSERT INTO cycle_user_properties (id, user_id, cycle_id, project_id, workspace_id, \
            filters, display_filters, display_properties, rich_filters, created_by_id, \
            created_at, updated_at) \
            SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, $5, $6, '{}'::jsonb, $1, now(), now() \
            FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
            WHERE c.id = $2 AND w.slug = $7 AND c.deleted_at IS NULL \
            ON CONFLICT DO NOTHING",
        )
        .bind(auth.0)
        .bind(cycle_id)
        .bind(project_id)
        .bind(default_filters())
        .bind(default_display_filters())
        .bind(default_display_properties())
        .bind(&slug)
        .execute(&st.pool)
        .await?;
    }
    match fetch_cycle_row(&st.pool, auth.0, cycle_id, project_id, &slug).await? {
        Some(row) => Ok((StatusCode::OK, Json(cycle_prop_json(&row)))),
        None => Ok(missing()),
    }
}

/// PATCH `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/user-properties/`
/// — parity with `CycleUserPropertiesEndpoint.patch`
/// (`cycle/base.py:625-643`): `.get()` miss → 404 `missing()`; else merge
/// ONLY the 4 keys (absent keeps old) and return **201** (not 200 —
/// explicit in Django).
pub async fn cycle_props_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, cycle_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(cur) = fetch_cycle_row(&st.pool, auth.0, cycle_id, project_id, &slug).await? else {
        debug_assert_eq!(missing_prop_patch("cycle"), StatusCode::NOT_FOUND);
        return Ok(missing());
    };
    let updated: Result<Option<CyclePropRow>, sqlx::Error> =
        sqlx::query_as::<_, CyclePropRow>(&format!(
            "UPDATE cycle_user_properties SET filters = $1, rich_filters = $2, \
            display_filters = $3, display_properties = $4, updated_by_id = $5, \
            updated_at = now() WHERE id = $6 RETURNING {CYCLE_COLS}"
        ))
        .bind(pick_or(&body, "filters", &cur.filters))
        .bind(pick_or(&body, "rich_filters", &cur.rich_filters))
        .bind(pick_or(&body, "display_filters", &cur.display_filters))
        .bind(pick_or(
            &body,
            "display_properties",
            &cur.display_properties,
        ))
        .bind(auth.0)
        .bind(cur.id)
        .fetch_optional(&st.pool)
        .await;
    match updated {
        Ok(Some(row)) => Ok((StatusCode::CREATED, Json(cycle_prop_json(&row)))),
        Ok(None) => Ok(missing()),
        Err(e) if is_constraint_violation(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Live module row for the caller, mirroring `.get(user, module_id,
/// project_id, workspace__slug)` (`module/base.py:828-833`).
async fn fetch_module_row(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    module_id: uuid::Uuid,
    project_id: uuid::Uuid,
    slug: &str,
) -> Result<Option<ModulePropRow>, sqlx::Error> {
    sqlx::query_as::<_, ModulePropRow>(&format!(
        "SELECT {MODULE_COLS} FROM module_user_properties m \
        JOIN workspaces w ON w.id = m.workspace_id \
        WHERE m.user_id = $1 AND m.module_id = $2 AND m.project_id = $3 \
        AND w.slug = $4 AND m.deleted_at IS NULL"
    ))
    .bind(user_id)
    .bind(module_id)
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// GET `/api/workspaces/:slug/projects/:project_id/modules/:module_id/user-properties/`
/// — parity with `ModuleUserPropertiesEndpoint.get`
/// (`module/base.py:845-855`): `get_or_create` → 200 (same missing-cycle
/// sane-deviation note as `cycle_props_get`).
pub async fn module_props_get(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, module_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if fetch_module_row(&st.pool, auth.0, module_id, project_id, &slug)
        .await?
        .is_none()
    {
        sqlx::query(
            "INSERT INTO module_user_properties (id, user_id, module_id, project_id, workspace_id, \
            filters, display_filters, display_properties, rich_filters, created_by_id, \
            created_at, updated_at) \
            SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, $5, $6, '{}'::jsonb, $1, now(), now() \
            FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
            WHERE m.id = $2 AND w.slug = $7 AND m.deleted_at IS NULL \
            ON CONFLICT DO NOTHING",
        )
        .bind(auth.0)
        .bind(module_id)
        .bind(project_id)
        .bind(default_filters())
        .bind(default_display_filters())
        .bind(default_display_properties())
        .bind(&slug)
        .execute(&st.pool)
        .await?;
    }
    match fetch_module_row(&st.pool, auth.0, module_id, project_id, &slug).await? {
        Some(row) => Ok((StatusCode::OK, Json(module_prop_json(&row)))),
        None => Ok(missing()),
    }
}

/// PATCH `/api/workspaces/:slug/projects/:project_id/modules/:module_id/user-properties/`
/// — parity with `ModuleUserPropertiesEndpoint.patch`
/// (`module/base.py:825-843`): `.get()` miss → 404 `missing()`; else
/// merge ONLY the 4 keys and return **201**.
pub async fn module_props_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, module_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(cur) = fetch_module_row(&st.pool, auth.0, module_id, project_id, &slug).await?
    else {
        debug_assert_eq!(missing_prop_patch("module"), StatusCode::NOT_FOUND);
        return Ok(missing());
    };
    let updated: Result<Option<ModulePropRow>, sqlx::Error> =
        sqlx::query_as::<_, ModulePropRow>(&format!(
            "UPDATE module_user_properties SET filters = $1, rich_filters = $2, \
            display_filters = $3, display_properties = $4, updated_by_id = $5, \
            updated_at = now() WHERE id = $6 RETURNING {MODULE_COLS}"
        ))
        .bind(pick_or(&body, "filters", &cur.filters))
        .bind(pick_or(&body, "rich_filters", &cur.rich_filters))
        .bind(pick_or(&body, "display_filters", &cur.display_filters))
        .bind(pick_or(
            &body,
            "display_properties",
            &cur.display_properties,
        ))
        .bind(auth.0)
        .bind(cur.id)
        .fetch_optional(&st.pool)
        .await;
    match updated {
        Ok(Some(row)) => Ok((StatusCode::CREATED, Json(module_prop_json(&row)))),
        Ok(None) => Ok(missing()),
        Err(e) if is_constraint_violation(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod batch_d_d4_tests {
    use super::*;

    #[test]
    fn gate_passes_admin_member_guest_only() {
        // Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
        // (`issue/base.py:744,768`, `cycle/base.py:626,646`,
        // `module/base.py:826,846`): roles 20/15/5 pass outright (the
        // workspace-ADMIN fallback lives in the caller via
        // `project_gate_allows`, exactly like D2/D3).
        assert!(guard_userprops(Some(20)).is_ok());
        assert!(guard_userprops(Some(15)).is_ok());
        assert!(guard_userprops(Some(5)).is_ok());
        assert_eq!(
            guard_userprops(None).unwrap_err(),
            "You don't have the required permissions."
        );
        assert!(guard_userprops(Some(10)).is_err());
    }

    #[test]
    fn missing_patch_distinguishes_project_autocreate_from_cycle_module_404() {
        // Locked plan fact: project PATCH auto-creates the row
        // (`issue/base.py:747-755` — never 404, PATCH 200) while
        // cycle/module PATCH `.get()` a missing row → 404 `missing()`
        // (`cycle/base.py:628-633`, `module/base.py:828-833`).
        assert_eq!(missing_prop_patch("project"), StatusCode::OK);
        assert_eq!(missing_prop_patch("cycle"), StatusCode::NOT_FOUND);
        assert_eq!(missing_prop_patch("module"), StatusCode::NOT_FOUND);
    }

    #[test]
    fn pick_or_mirrors_request_data_get_with_default() {
        // Mirrors `request.data.get("filters", cycle_properties.filters)`
        // (`cycle/base.py:635-642`): present wins, absent keeps old.
        let cur = json!({"priority": null});
        let body = json!({"filters": {"priority": "high"}, "other": 1});
        assert_eq!(
            pick_or(&body, "filters", &cur),
            json!({"priority": "high"})
        );
        assert_eq!(
            pick_or(&body, "rich_filters", &json!({})),
            json!({})
        );
        // Explicit null passes through like Django's direct assignment
        // (DB `NOT NULL` then rejects it → 400 `PAYLOAD_INVALID_MSG`).
        let null_body = json!({"filters": null});
        assert!(pick_or(&null_body, "filters", &cur).is_null());
        // Unknown keys are ignored by both Django (undeclared fields) and
        // the Rust handlers (only the 4/6 known keys are read).
        assert_eq!(pick_or(&body, "unknown_key", &cur), cur);
    }

    #[test]
    fn project_patch_validation_rejects_non_numeric_sort_order() {
        // Mirrors the partial-serializer `FloatField` rejection
        // (`serializers/issue.py:354-358`): any non-number (incl. null on
        // the non-nullable column) → Django `ValidationError` → 400
        // `INVALID_DETAIL_MSG` (`views/base.py:100-104`).
        assert_eq!(
            INVALID_DETAIL_MSG,
            "Please provide valid detail"
        );
        assert_eq!(
            PAYLOAD_INVALID_MSG,
            "The payload is not valid"
        );
        assert!(validate_project_prop_patch(&json!({})).is_ok());
        assert!(
            validate_project_prop_patch(&json!({"sort_order": 12345.0})).is_ok()
        );
        assert!(
            validate_project_prop_patch(&json!({"filters": {"a": 1}})).is_ok()
        );
        assert_eq!(
            validate_project_prop_patch(&json!({"sort_order": "high"})).unwrap_err(),
            "Please provide valid detail"
        );
        assert_eq!(
            validate_project_prop_patch(&json!({"sort_order": null})).unwrap_err(),
            "Please provide valid detail"
        );
    }

    fn sample_project_row() -> ProjectPropRow {
        ProjectPropRow {
            id: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by_id: Some(uuid::Uuid::nil()),
            updated_by_id: None,
            deleted_at: None,
            workspace_id: uuid::Uuid::nil(),
            project_id: uuid::Uuid::nil(),
            user_id: uuid::Uuid::nil(),
            filters: default_filters(),
            display_filters: default_display_filters(),
            display_properties: default_display_properties(),
            rich_filters: json!({}),
            preferences: default_preferences(),
            sort_order: 65535.0,
        }
    }

    #[test]
    fn project_prop_json_covers_all_serializer_keys() {
        // Mirrors `ProjectUserPropertySerializer`
        // (`serializers/issue.py:354-358`, `fields = "__all__"`): all 15
        // model columns with DRF PK rendering (non-null FKs as ids,
        // nullable audit FKs null when unset).
        let row = sample_project_row();
        let v = project_prop_json(&row);
        for key in [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "project",
            "workspace",
            "user",
            "filters",
            "display_filters",
            "display_properties",
            "rich_filters",
            "preferences",
            "sort_order",
        ] {
            assert!(v.get(key).is_some(), "ProjectUserProperty missing key {key}");
        }
        assert!(v.get("updated_by").unwrap().is_null());
        assert!(v.get("deleted_at").unwrap().is_null());
        assert_eq!(v.get("sort_order"), Some(&json!(65535.0)));
        assert_eq!(v.get("rich_filters"), Some(&json!({})));
    }

    #[test]
    fn cycle_prop_json_covers_all_serializer_keys() {
        // Mirrors `CycleUserPropertiesSerializer`
        // (`serializers/cycle.py:102-106`, `fields = "__all__"`): all 14
        // model columns (no preferences/sort_order — verified live).
        let p = sample_project_row();
        let row = CyclePropRow {
            id: p.id,
            created_at: p.created_at,
            updated_at: p.updated_at,
            created_by_id: p.created_by_id,
            updated_by_id: p.updated_by_id,
            deleted_at: p.deleted_at,
            workspace_id: p.workspace_id,
            project_id: p.project_id,
            cycle_id: uuid::Uuid::nil(),
            user_id: p.user_id,
            filters: p.filters,
            display_filters: p.display_filters,
            display_properties: p.display_properties,
            rich_filters: p.rich_filters,
        };
        let v = cycle_prop_json(&row);
        for key in [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "project",
            "workspace",
            "cycle",
            "user",
            "filters",
            "display_filters",
            "display_properties",
            "rich_filters",
        ] {
            assert!(v.get(key).is_some(), "CycleUserProperties missing key {key}");
        }
        assert!(v.get("preferences").is_none());
        assert!(v.get("sort_order").is_none());
    }

    #[test]
    fn module_prop_json_covers_all_serializer_keys() {
        // Mirrors `ModuleUserPropertiesSerializer`
        // (`serializers/module.py:276-280`, `fields = "__all__"`): all 14
        // model columns (no preferences/sort_order — verified live).
        let p = sample_project_row();
        let row = ModulePropRow {
            id: p.id,
            created_at: p.created_at,
            updated_at: p.updated_at,
            created_by_id: p.created_by_id,
            updated_by_id: p.updated_by_id,
            deleted_at: p.deleted_at,
            workspace_id: p.workspace_id,
            project_id: p.project_id,
            module_id: uuid::Uuid::nil(),
            user_id: p.user_id,
            filters: p.filters,
            display_filters: p.display_filters,
            display_properties: p.display_properties,
            rich_filters: p.rich_filters,
        };
        let v = module_prop_json(&row);
        for key in [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "project",
            "workspace",
            "module",
            "user",
            "filters",
            "display_filters",
            "display_properties",
            "rich_filters",
        ] {
            assert!(v.get(key).is_some(), "ModuleUserProperties missing key {key}");
        }
        assert!(v.get("preferences").is_none());
        assert!(v.get("sort_order").is_none());
    }

    #[test]
    fn django_defaults_match_live_rows() {
        // The creation defaults mirror the Django model getters
        // (`db/models/issue.py:47-88`, `db/models/project.py:64-65`) and
        // match the live `project_user_properties` row observed 2026-09-06.
        assert_eq!(
            default_filters(),
            json!({
                "priority": null, "state": null, "state_group": null,
                "assignees": null, "created_by": null, "labels": null,
                "start_date": null, "target_date": null, "subscriber": null,
            })
        );
        let df = default_display_filters();
        assert_eq!(df.get("order_by"), Some(&json!("-created_at")));
        assert_eq!(df.get("layout"), Some(&json!("list")));
        let dp = default_display_properties();
        assert_eq!(dp.get("key"), Some(&json!(true)));
        assert_eq!(dp.get("state"), Some(&json!(true)));
        let prefs = default_preferences();
        assert_eq!(
            prefs.get("navigation"),
            Some(&json!({"default_tab": "work_items", "hide_in_more_menu": []}))
        );
    }
}
