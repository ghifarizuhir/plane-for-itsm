use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::routes::project::{deny, project_role, ws_role, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/views/view/` (IssueViewViewSet list/create +
/// WorkspaceViewViewSet list/create) for `plane/app/urls/views.py`:
/// project views (`.../views/`, project_id set) and global views
/// (`workspaces/:slug/views/`, project NULL). `name` required/255,
/// `access` 0 (Private) / 1 (Public). Owner/lock guards ("view is
/// locked", "Only admin or owner can delete the view") belong to detail.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateView {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub access: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewOut {
    pub id: uuid::Uuid,
    pub name: String,
}

/// Mirrors IssueViewFavoriteViewSet.create: body `{view: <uuid>}`
/// stored as UserFavorite(entity_type="view").
#[derive(Debug, Clone, Deserialize)]
pub struct CreateFavorite {
    #[serde(default)]
    pub view: Option<uuid::Uuid>,
}

pub fn validate_create(body: &CreateView) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(access) = body.access {
        if access != 0 && access != 1 {
            return Err("access must be 0 (Private) or 1 (Public)".to_string());
        }
    }
    Ok(())
}

/// Mirrors `plane/app/views/view/base.py:retrieve`: a project guest sees a
/// view only when guests may view all features or they own it.
pub fn guard_guest_access(is_guest: bool, guest_view_all: bool, is_owner: bool) -> Result<(), String> {
    if is_guest && !guest_view_all && !is_owner {
        return Err("You are not allowed to view this issue".to_string());
    }
    Ok(())
}

// Kept for `tests/view_test.rs` per user (E6): the favorite handler
// intentionally skips validation (NULL passthrough), so routes never call
// this — but the integration test still exercises it.
#[allow(dead_code)]
pub fn validate_favorite_create(body: &CreateFavorite) -> Result<(), String> {
    if body.view.is_none() {
        return Err("view is required".to_string());
    }
    Ok(())
}

/// `plane/app/views/base.py:92-97` (Django `IntegrityError` → 400; favorite dup).
pub const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
/// (`plane/app/permissions/base.py:40-59`) — GUEST (5) denied.
pub fn guard_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Gate for the view-favorite AM endpoint: allowed project role 20/15
/// outright, else the workspace-ADMIN fallback
/// (`plane/app/permissions/base.py:61-78` — any active project membership +
/// workspace ADMIN), mirroring `cycle.rs:gate_am`. Anything else denies.
async fn gate_am(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = project_role(pool, user, pid).await?;
    let ws_admin = ws_role(pool, user, slug)
        .await
        .map(|r| r == Some(20))
        .unwrap_or(false);
    Ok(guard_am(role).is_ok() || (role.is_some() && ws_admin))
}

/// Pure SQLSTATE check behind [`is_constraint_violation`]: class `23`
/// (integrity constraint violation — Django `IntegrityError` → 400).
/// Split out so unit tests can exercise the dup-arm mapping without
/// constructing a `sqlx::Error` (which has no public test constructor).
fn is_constraint_violation_code(code: &str) -> bool {
    code.starts_with("23")
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| is_constraint_violation_code(&c)))
}

/// Pure dup→400 mapping for the favorite insert (`views/base.py:92-97`):
/// `true` (constraint violation) → 400 `PAYLOAD_INVALID_MSG`,
/// `false` (insert ok) → 204. The handler delegates here so unit tests
/// fail if the dup-arm mapping breaks.
fn favorite_insert_outcome(is_dup: bool) -> (StatusCode, Json<Value>) {
    if is_dup {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )
    } else {
        (StatusCode::NO_CONTENT, Json(Value::Null))
    }
}

/// Pure extraction of the favorite target (`view/base.py:420-427`):
/// `request.data.get("view")` passes None straight through
/// (`entity_identifier` is nullable) — no validation, NULL still 204s.
fn favorite_entity_identifier(body: &CreateFavorite) -> Option<uuid::Uuid> {
    body.view
}

/// Pure port of the `issue_filters(params, "POST")` branch
/// (`plane/utils/issue_filters.py:428-457` + each filter's `else` arm,
/// prefix `""`): computes the stored `query` JSON from the `filters`
/// object on create/patch (`serializers/view.py:71-77`,
/// `IssueView.save`, `models/view.py:79-81`). Quirks preserved verbatim:
/// `type` always sets `state__group__in` (default all five); `sub_issue`
/// defaults `"false"` → `parent__isnull=True`; `intake_status` reads the
/// `inbox_status` value (copy-paste quirk, `issue_filters.py:345-350`);
/// labels/assignees/cycle/module/subscriber always add their
/// `...deleted_at__isnull: True` guards.
pub fn build_view_query(filters: &Value) -> Value {
    let get = |k: &str| filters.get(k);
    let present_nonempty = |k: &str| -> Option<&Value> {
        let v = get(k)?;
        if v.is_null() {
            return None;
        }
        if let Some(s) = v.as_str() {
            if s.is_empty() || s == "null" {
                return None;
            }
        }
        if let Some(a) = v.as_array() {
            if a.is_empty() {
                return None;
            }
        }
        Some(v)
    };
    let mut q = serde_json::Map::new();
    // Plain `__in` passthroughs (POST else-arms).
    for (key, mapped) in [
        ("state", "state__in"),
        ("estimate_point", "estimate_point__in"),
        ("priority", "priority__in"),
        ("parent", "parent__in"),
        ("mentions", "issue_mention__mention__id__in"),
        ("created_by", "created_by__in"),
        ("logged_by", "logged_by__in"),
        ("project", "project__in"),
        ("cycle", "issue_cycle__cycle_id__in"),
        ("module", "issue_module__module_id__in"),
        ("inbox_status", "issue_intake__status__in"),
        ("subscriber", "issue_subscribers__subscriber_id__in"),
    ] {
        if let Some(v) = present_nonempty(key) {
            q.insert(mapped.to_string(), v.clone());
        }
    }
    // Labels / assignees (+ the unconditional soft-delete guards).
    if present_nonempty("labels").is_some() {
        q.insert("labels__in".to_string(), get("labels").unwrap().clone());
    }
    q.insert("label_issue__deleted_at__isnull".to_string(), json!(true));
    if present_nonempty("assignees").is_some() {
        q.insert("assignees__in".to_string(), get("assignees").unwrap().clone());
    }
    q.insert("issue_assignee__deleted_at__isnull".to_string(), json!(true));
    q.insert("issue_cycle__deleted_at__isnull".to_string(), json!(true));
    q.insert("issue_module__deleted_at__isnull".to_string(), json!(true));
    q.insert("issue_subscribers__deleted_at__isnull".to_string(), json!(true));
    // Name (non-empty only).
    if let Some(Value::String(s)) = get("name") {
        if !s.is_empty() {
            q.insert("name__icontains".to_string(), json!(s));
        }
    }
    // Dates: raw passthrough (`*_date` / `*__date`).
    for (key, mapped) in [
        ("start_date", "start_date"),
        ("target_date", "target_date"),
        ("created_at", "created_at__date"),
        ("updated_at", "updated_at__date"),
        ("completed_at", "completed_at__date"),
    ] {
        if let Some(v) = present_nonempty(key) {
            q.insert(mapped.to_string(), v.clone());
        }
    }
    // `type` → state groups (always set).
    let group = match get("type").and_then(Value::as_str).unwrap_or("all") {
        "backlog" => vec!["backlog"],
        "active" => vec!["unstarted", "started"],
        _ => vec!["backlog", "unstarted", "started", "completed", "cancelled"],
    };
    q.insert("state__group__in".to_string(), json!(group));
    // `intake_status` reads the `inbox_status` value (quirk).
    if let Some(v) = present_nonempty("inbox_status") {
        q.insert("issue_intake__status__in".to_string(), v.clone());
    }
    // `sub_issue` toggle (POST: `params.get("sub_issue", "false") ==
    // "false"` — a JSON `false` does NOT equal `"false"` in Python, so
    // only a missing key or the literal string triggers the filter).
    let sub = get("sub_issue");
    if sub.is_none() || sub == Some(&json!("false")) {
        q.insert("parent__isnull".to_string(), json!(true));
    }
    // `start_target_date` toggle.
    if get("start_target_date") == Some(&json!(true))
        || get("start_target_date").and_then(Value::as_str) == Some("true")
    {
        q.insert("target_date__isnull".to_string(), json!(false));
        q.insert("start_date__isnull".to_string(), json!(false));
    }
    Value::Object(q)
}

/// Full `IssueViewSerializer` row (`serializers/view.py:56-69`:
/// `__all__` + read-only `is_favorite`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct ViewFullRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
    name: String,
    description: String,
    query: Value,
    filters: Value,
    display_filters: Value,
    display_properties: Value,
    rich_filters: Value,
    access: i16,
    sort_order: f64,
    logo_props: Value,
    owned_by_id: Option<uuid::Uuid>,
    is_locked: bool,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    is_favorite: bool,
}

const VIEW_FULL_COLS: &str = "v.id, v.created_at, v.updated_at, v.created_by_id, v.updated_by_id, \
    v.workspace_id, v.project_id, v.name, v.description, v.query, v.filters, \
    v.display_filters, v.display_properties, v.rich_filters, v.access, v.sort_order, \
    v.logo_props, v.owned_by_id, v.is_locked, v.archived_at, \
    EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'view' \
      AND uf.entity_identifier = v.id AND uf.user_id = $4 AND uf.deleted_at IS NULL) AS is_favorite";

fn view_full_json(r: &ViewFullRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
        "workspace": r.workspace_id,
        "project": r.project_id,
        "name": r.name,
        "description": r.description,
        "query": r.query,
        "filters": r.filters,
        "display_filters": r.display_filters,
        "display_properties": r.display_properties,
        "rich_filters": r.rich_filters,
        "access": r.access,
        "sort_order": r.sort_order,
        "logo_props": r.logo_props,
        "owned_by": r.owned_by_id,
        "is_locked": r.is_locked,
        "archived_at": r.archived_at,
        "is_favorite": r.is_favorite,
    })
}

async fn fetch_view_full(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    pk: uuid::Uuid,
    user: uuid::Uuid,
) -> Result<Option<ViewFullRow>, sqlx::Error> {
    let pid_filter = match project_id {
        Some(_) => "AND v.project_id = $2",
        None => "AND v.project_id IS NULL",
    };
    let sql = format!(
        "SELECT {VIEW_FULL_COLS} FROM issue_views v JOIN workspaces w ON w.id = v.workspace_id \
         WHERE v.id = $1 {pid_filter} AND w.slug = $3 AND v.deleted_at IS NULL"
    );
    let mut qb = sqlx::query_as::<_, ViewFullRow>(&sql).bind(pk);
    if let Some(pid) = project_id {
        qb = qb.bind(pid);
    }
    qb.bind(slug).bind(user).fetch_optional(pool).await
}

/// `?fields=` subset projection (`DynamicBaseSerializer`, `base.py:310-311,80`).
fn apply_view_fields(mut v: Value, fields: &[String]) -> Value {
    if !fields.is_empty() {
        if let Some(obj) = v.as_object_mut() {
            obj.retain(|k, _| fields.iter().any(|f| f == k));
        }
    }
    v
}

fn parse_view_fields(params: &HashMap<String, String>) -> Vec<String> {
    params
        .get("fields")
        .map(|s| s.split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect())
        .unwrap_or_default()
}

/// Shared list core: member/project scope + owned|access + guest scope +
/// favorite-first ordering, returning full rows.
async fn list_view_rows(
    st: &AppState,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    user: uuid::Uuid,
    order_by_raw: Option<&str>,
) -> Result<Vec<ViewFullRow>, common::errors::AppError> {
    // GUEST (+ !guest_view_all_features for project views) sees owned only.
    let guest_owned_only = if let Some(pid) = project_id {
        let role: Option<i16> = sqlx::query_scalar(
            "SELECT role FROM project_members WHERE project_id = $1 AND member_id = $2 AND is_active = true AND deleted_at IS NULL",
        )
        .bind(pid)
        .bind(user)
        .fetch_optional(&st.pool)
        .await?;
        if role == Some(5) {
            let gva: bool = sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
                .bind(pid)
                .fetch_optional(&st.pool)
                .await?
                .unwrap_or(false);
            !gva
        } else {
            false
        }
    } else {
        matches!(ws_role(&st.pool, user, slug).await?, Some(5))
    };
    let guest_filter_proj = if guest_owned_only { "AND v.owned_by_id = $4" } else { "" };
    let guest_filter_ws = if guest_owned_only { "AND v.owned_by_id = $3" } else { "" };
    // Project views: fixed `-is_favorite, name` (`base.py:269-293`); global
    // views: order_by allowlist {created_at,updated_at,name} (`base.py:60-75).
    let order_expr = match project_id {
        Some(_) => "-is_favorite, name".to_string(),
        None => {
            let raw = order_by_raw.unwrap_or("-created_at");
            let (bare, desc) = match raw.strip_prefix('-') {
                Some(b) => (b, true),
                None => (raw, false),
            };
            match (bare, desc) {
                ("created_at", false) => "v.created_at ASC",
                ("updated_at", false) => "v.updated_at ASC",
                ("updated_at", true) => "v.updated_at DESC",
                ("name", false) => "v.name ASC",
                ("name", true) => "v.name DESC",
                _ => "v.created_at DESC",
            }
            .to_string()
        }
    };
    // Scope mirrors the querysets: project views require an ACTIVE project
    // membership on a live project (`base.py:269-293`); global views are
    // ws-scoped with owned|access (`base.py:60-75`).
    let guest_filter = if guest_owned_only { "AND v.owned_by_id = $4" } else { "" };
    let rows: Vec<ViewFullRow> = if let Some(pid) = project_id {
        sqlx::query_as(&format!(
            "SELECT v.id, v.created_at, v.updated_at, v.created_by_id, v.updated_by_id, \
             v.workspace_id, v.project_id, v.name, v.description, v.query, v.filters, \
             v.display_filters, v.display_properties, v.rich_filters, v.access, v.sort_order, \
             v.logo_props, v.owned_by_id, v.is_locked, v.archived_at, \
             EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'view' \
               AND uf.entity_identifier = v.id AND uf.user_id = $4 AND uf.deleted_at IS NULL) AS is_favorite \
             FROM issue_views v JOIN workspaces w ON w.id = v.workspace_id \
             WHERE w.slug = $1 AND v.project_id = $2 AND v.deleted_at IS NULL \
             AND (v.owned_by_id = $4 OR v.access = 1) \
             AND EXISTS(SELECT 1 FROM project_members pm JOIN projects p ON p.id = pm.project_id \
               WHERE pm.project_id = v.project_id AND pm.member_id = $4 AND pm.is_active = true \
               AND pm.deleted_at IS NULL AND p.archived_at IS NULL) \
             {guest_filter_proj} ORDER BY is_favorite DESC, v.name ASC"
        ))
        .bind(slug)
        .bind(pid)
        .bind(user)
        .bind(user)
        .fetch_all(&st.pool)
        .await?
    } else {
        sqlx::query_as(&format!(
            "SELECT v.id, v.created_at, v.updated_at, v.created_by_id, v.updated_by_id, \
             v.workspace_id, v.project_id, v.name, v.description, v.query, v.filters, \
             v.display_filters, v.display_properties, v.rich_filters, v.access, v.sort_order, \
             v.logo_props, v.owned_by_id, v.is_locked, v.archived_at, \
             EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'view' \
               AND uf.entity_identifier = v.id AND uf.user_id = $3 AND uf.deleted_at IS NULL) AS is_favorite \
             FROM issue_views v JOIN workspaces w ON w.id = v.workspace_id \
             WHERE w.slug = $1 AND v.project_id IS NULL AND v.deleted_at IS NULL \
             AND (v.owned_by_id = $3 OR v.access = 1) \
             {guest_filter_ws} ORDER BY {order_expr}"
        ))
        .bind(slug)
        .bind(user)
        .bind(user)
        .fetch_all(&st.pool)
        .await?
    };
    Ok(rows)
}

/// GET `.../views/` — parity with `IssueViewViewSet.list`
/// (`base.py:295-311`): AMG project-level gate (decorator, level PROJECT —
/// any active project member incl. GUEST); member/live-project scope +
/// owned|access + `is_favorite` + `-is_favorite,name` order; guest-owned
/// scoping; `?fields=` projection. Deny = DRF permission-class 403.
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::member::deny_detail;
    if project_role(&st.pool, auth.0, project_id).await?.is_none() {
        return Ok(deny_detail());
    }
    let rows = list_view_rows(&st, &slug, Some(project_id), auth.0, None).await?;
    let fields = parse_view_fields(&params);
    Ok((StatusCode::OK, Json(json!(rows.iter().map(|r| apply_view_fields(view_full_json(r), &fields)).collect::<Vec<_>>()))))
}

/// GET `workspaces/:slug/views/` — parity with
/// `WorkspaceViewViewSet.list` (`base.py:77-84`): AMG ws-level gate; global
/// scope + order_by allowlist + guest-owned scoping + `?fields=`.
pub async fn list_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::member::deny_detail;
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let rows = list_view_rows(&st, &slug, None, auth.0, params.get("order_by").map(|s| s.as_str())).await?;
    let fields = parse_view_fields(&params);
    Ok((StatusCode::OK, Json(json!(rows.iter().map(|r| apply_view_fields(view_full_json(r), &fields)).collect::<Vec<_>>()))))
}

async fn create_view_row(
    st: &AppState,
    auth: AuthUser,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    body: &Value,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    if name.trim().is_empty() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"name": ["This field may not be blank."]}))));
    }
    if name.chars().count() > 255 {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"name": ["Ensure this field has no more than 255 characters."]}))));
    }
    let access = body.get("access").and_then(Value::as_i64).unwrap_or(1);
    if access != 0 && access != 1 {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"access": ["\"1\" is not a valid choice."]}))));
    }
    let filters = body.get("filters").cloned().unwrap_or(json!({}));
    let query = build_view_query(&filters);
    // `IssueView.save`: `sort_order = max+10000` within the (project|global)
    // scope (`models/view.py:83-93`).
    let scope_filter = match project_id {
        Some(_) => "AND v.project_id = $1",
        None => "AND v.project_id IS NULL",
    };
    let max_sort: Option<f64> = sqlx::query_scalar(&format!(
        "SELECT MAX(v.sort_order) FROM issue_views v JOIN workspaces w ON w.id = v.workspace_id \
         WHERE w.slug = $2 {scope_filter} AND v.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(slug)
    .fetch_optional(&st.pool)
    .await?
    .flatten();
    let sort_order = max_sort.map(|m| m + 10000.0).unwrap_or(65535.0);
    let row: Result<Option<uuid::Uuid>, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO issue_views (id, name, description, query, filters, display_filters, display_properties, rich_filters, logo_props, access, sort_order, is_locked, project_id, workspace_id, owned_by_id, created_by_id, updated_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, false, $11, w.id, $12, $12, $12, now(), now() FROM workspaces w WHERE w.slug = $13 RETURNING id",
    )
    .bind(name)
    .bind(body.get("description").and_then(Value::as_str).unwrap_or(""))
    .bind(&query)
    .bind(&filters)
    .bind(body.get("display_filters").cloned().unwrap_or(json!({})))
    .bind(body.get("display_properties").cloned().unwrap_or(json!({})))
    .bind(body.get("rich_filters").cloned().unwrap_or(json!({})))
    .bind(body.get("logo_props").cloned().unwrap_or(json!({})))
    .bind(access as i16)
    .bind(sort_order)
    .bind(project_id)
    .bind(auth.0)
    .bind(slug)
    .fetch_optional(&st.pool)
    .await;
    let row = match row {
        Ok(v) => v,
        // Unknown project FK → DRF IntegrityError body (`views/base.py:92-97`).
        Err(e) if is_constraint_violation(&e) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": PAYLOAD_INVALID_MSG}))));
        }
        Err(e) => return Err(e.into()),
    };
    match row {
        // 201 full shape (`perform_create` + DRF default create).
        Some(id) => match fetch_view_full(&st.pool, slug, project_id, id, auth.0).await? {
            Some(r) => Ok((StatusCode::CREATED, Json(view_full_json(&r)))),
            None => Ok((StatusCode::CREATED, Json(json!({"id": id, "name": name})))),
        },
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Workspace not found"})))),
    }
}

/// POST `.../views/` — `perform_create` stamps project+owner
/// (`base.py:266-267`); no explicit gate (any authenticated caller —
/// Django relies on `IsAuthenticated` + queryset scoping here).
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    create_view_row(&st, auth, &slug, Some(project_id), &body).await
}

/// POST `workspaces/:slug/views/` — stamps workspace+owner, project NULL
/// (`base.py:56-58`).
pub async fn create_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    create_view_row(&st, auth, &slug, None, &body).await
}

pub async fn list_favorites(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::view::UserFavorite>(
        "SELECT id, entity_identifier FROM user_favorites WHERE project_id = $1 AND entity_type = 'view' AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|f| json!({"id": f.id, "view": f.entity_identifier}))
            .collect(),
    ))
}

pub async fn create_favorite(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateFavorite>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
    // (`plane/app/views/view/base.py:419`): project ADMIN (20) / MEMBER (15)
    // only; GUEST (5) / non-member → 403 `deny()`
    // (`plane/app/permissions/base.py:81-84`).
    if !gate_am(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // `plane/app/views/view/base.py:420-427`: NO body validation —
    // `request.data.get("view")` passes None straight through
    // (`user_favorites.entity_identifier` is nullable) and still 204s.
    let view_id: Option<uuid::Uuid> = favorite_entity_identifier(&body);
    let owner = auth.0;

    let r = sqlx::query(
        "INSERT INTO user_favorites (id, entity_type, entity_identifier, user_id, is_folder, sequence, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), 'view', $1, $2, false, 65535, $3, w.id, now(), now() FROM workspaces w WHERE w.slug = $4",
    )
    .bind(view_id)
    .bind(owner)
    .bind(project_id)
    .bind(&slug)
    .execute(&st.pool)
    .await;
    // Dup → 400 `{"error": "The payload is not valid"}` (Django
    // `IntegrityError` → `plane/app/views/base.py:92-97`); unique index
    // `(entity_type, entity_identifier, user_id) WHERE deleted_at IS NULL`.
    match r {
        Ok(_) => Ok(favorite_insert_outcome(false)),
        Err(e) if is_constraint_violation(&e) => Ok(favorite_insert_outcome(true)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchView {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub access: Option<i16>,
}

/// Detail core: full row + the retrieve guest gate. Project views keep
/// the verified `guard_guest_access` 403 string; global views have NO
/// retrieve gate (`base.py:108-118` — queryset-scoped only). Miss stays
/// 404 `{"error": "View not found"}` (sane-mapping: Django serializes
/// None and 500s). `recent_visited_task` celery skipped batch-wide.
async fn view_detail_row(
    st: &AppState,
    auth: &AuthUser,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    pk: uuid::Uuid,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row = fetch_view_full(&st.pool, slug, project_id, pk, auth.0).await?;
    let Some(r) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "View not found"}))));
    };
    // Guest gate mirrors `IssueViewViewSet.retrieve` (`base.py:324-342`).
    // AuthUser identitas sudah tervalidasi di extractor.
    let uid = auth.0;
    if let Some(pid) = project_id {
        let role: Option<i16> = sqlx::query_scalar(
            "SELECT role FROM project_members WHERE project_id = $1 AND member_id = $2 AND is_active = true AND deleted_at IS NULL",
        )
        .bind(pid)
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?;
        let gva: Option<bool> =
            sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
                .bind(pid)
                .fetch_optional(&st.pool)
                .await?;
        if let Err(e) = guard_guest_access(
            role == Some(5),
            gva.unwrap_or(false),
            r.owned_by_id == Some(uid),
        ) {
            return Ok((StatusCode::FORBIDDEN, Json(json!({"error": e}))));
        }
    }
    Ok((StatusCode::OK, Json(view_full_json(&r))))
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    view_detail_row(&st, &auth, &_slug, Some(project_id), pk).await
}

pub async fn detail_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    view_detail_row(&st, &auth, &slug, None, pk).await
}

/// Patch core: lock + owner 400s, then the serializer-writable allowlist
/// (read-only: workspace/project/query/owned_by/access/is_locked —
/// `query` recomputed from `filters` when present), 200 full row.
/// Project twin gate: creator-only decorator (`base.py:348` —
/// non-owner/non-member 403 `deny()`); global twin: same creator gate
/// (`base.py:86`).
async fn patch_view_row(
    st: &AppState,
    auth: AuthUser,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    pk: uuid::Uuid,
    body: &Value,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(cur) = fetch_view_full(&st.pool, slug, project_id, pk, auth.0).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "View not found"}))));
    };
    // Creator-only decorator (`base.py:348` / `:86`): the in-view "Only the
    // owner..." 400 below it is unreachable in Django too (the decorator
    // 403s non-owners first) — kept as `deny()` only.
    if cur.owned_by_id != Some(auth.0) {
        return Ok(deny());
    }
    if cur.is_locked {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "view is locked"}))));
    }
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        if name.trim().is_empty() {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"name": ["This field may not be blank."]}))));
        }
        if name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"name": ["Ensure this field has no more than 255 characters."]}))));
        }
    }
    let mut tx = st.pool.begin().await?;
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        sqlx::query("UPDATE issue_views SET name = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3")
            .bind(name).bind(auth.0).bind(pk).execute(&mut *tx).await?;
    }
    if let Some(v) = body.get("description").and_then(Value::as_str) {
        sqlx::query("UPDATE issue_views SET description = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3")
            .bind(v).bind(auth.0).bind(pk).execute(&mut *tx).await?;
    }
    for (key, col) in [
        ("display_filters", "display_filters"),
        ("display_properties", "display_properties"),
        ("rich_filters", "rich_filters"),
        ("logo_props", "logo_props"),
    ] {
        if body.get(key).is_some() {
            sqlx::query(&format!("UPDATE issue_views SET {col} = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3"))
                .bind(body.get(key).cloned()).bind(auth.0).bind(pk).execute(&mut *tx).await?;
        }
    }
    if let Some(sort) = body.get("sort_order").and_then(Value::as_f64) {
        sqlx::query("UPDATE issue_views SET sort_order = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3")
            .bind(sort).bind(auth.0).bind(pk).execute(&mut *tx).await?;
    }
    if body.get("filters").is_some() {
        let filters = body.get("filters").cloned().unwrap_or(json!({}));
        let query = build_view_query(&filters);
        sqlx::query("UPDATE issue_views SET filters = $1, query = $2, updated_at = now(), updated_by_id = $3 WHERE id = $4")
            .bind(&filters).bind(&query).bind(auth.0).bind(pk).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    match fetch_view_full(&st.pool, slug, project_id, pk, auth.0).await? {
        Some(r) => Ok((StatusCode::OK, Json(view_full_json(&r)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "View not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    patch_view_row(&st, auth, &slug, Some(project_id), pk, &body).await
}

/// Destroy core: project-ADMIN or owner (`base.py:371-404`), else 400
/// `{"error": "Only admin or owner can delete the view"}`; global twin:
/// ws-ADMIN or owner (`base.py:120-141`). Cascades: `UserFavorite` rows
/// soft-deleted both scopes; project scope also hard-deletes
/// `UserRecentVisit` rows (`delete(soft=False)`).
async fn destroy_view_row(
    st: &AppState,
    auth: AuthUser,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    pk: uuid::Uuid,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(cur) = fetch_view_full(&st.pool, slug, project_id, pk, auth.0).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "View not found"}))));
    };
    let is_admin = match project_id {
        Some(pid) => matches!(project_role(&st.pool, auth.0, pid).await?, Some(20)),
        None => matches!(ws_role(&st.pool, auth.0, slug).await?, Some(20)),
    };
    // ADMIN-or-creator decorator (`base.py:371` / `:120`): the in-view
    // "Only admin or owner..." 400 is unreachable in Django too (the
    // decorator 403s the same callers first) — `deny()` only.
    if !is_admin && cur.owned_by_id != Some(auth.0) {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE issue_views SET deleted_at = now() WHERE id = $1")
        .bind(pk).execute(&mut *tx).await?;
    if let Some(pid) = project_id {
        sqlx::query(
            "UPDATE user_favorites SET deleted_at = now() WHERE project_id = $1 \
             AND entity_identifier = $2 AND entity_type = 'view' AND deleted_at IS NULL",
        )
        .bind(pid).bind(pk).execute(&mut *tx).await?;
        sqlx::query(
            "DELETE FROM user_recent_visits WHERE project_id = $1 \
             AND entity_identifier = $2 AND entity_name = 'view'",
        )
        .bind(pid).bind(pk).execute(&mut *tx).await?;
    } else {
        sqlx::query(
            "UPDATE user_favorites SET deleted_at = now() WHERE workspace_id = \
             (SELECT id FROM workspaces WHERE slug = $1) AND entity_identifier = $2 \
             AND entity_type = 'view' AND project_id IS NULL AND deleted_at IS NULL",
        )
        .bind(slug).bind(pk).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    destroy_view_row(&st, auth, &slug, Some(project_id), pk).await
}

/// Workspace-scoped PATCH twin of `patch` for `WorkspaceViewViewSet`
/// (`app/views/view/base.py:87-106`, `app/urls/views.py:41-47`): lock +
/// creator gate + full allowlist + full 200 via the shared core.
pub async fn patch_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    patch_view_row(&st, auth, &slug, None, pk, &body).await
}

/// Workspace-scoped DELETE twin of `destroy` for `WorkspaceViewViewSet.destroy`
/// (`app/views/view/base.py:121-143`): ws-ADMIN-or-owner + fav cascade.
pub async fn destroy_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    destroy_view_row(&st, auth, &slug, None, pk).await
}

#[cfg(test)]
mod view_e6_tests {
    use super::*;
    use crate::routes::project::deny;

    #[test]
    fn fav_gate_allows_admin_member_only() {
        // `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
        // (`permissions/base.py:40-59`): project ADMIN (20) / MEMBER (15)
        // pass; GUEST (5) / non-member → 403 `deny()`.
        assert!(guard_am(Some(20)).is_ok());
        assert!(guard_am(Some(15)).is_ok());
        assert!(guard_am(Some(5)).is_err());
        assert!(guard_am(None).is_err());
    }

    #[test]
    fn fav_dup_maps_to_400_payload_invalid() {
        // Django `IntegrityError` → `views/base.py:92-97`: a duplicate
        // favorite is 400 `{"error": "The payload is not valid"}` — never
        // the 409 `View already favorited` shape. Exercises the real
        // pure helpers the handler delegates to, so the test fails if
        // either the SQLSTATE check or the dup-arm mapping breaks.
        assert!(is_constraint_violation_code("23505"));
        assert!(!is_constraint_violation_code("22000"));

        let (status, body) = favorite_insert_outcome(true);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0, json!({"error": "The payload is not valid"}));
        assert!(!PAYLOAD_INVALID_MSG.contains("favorited"));

        let (status, body) = favorite_insert_outcome(false);
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(body.0, Value::Null);
    }

    #[test]
    fn fav_deny_is_403_permissions_error() {
        // `permissions/base.py:81-84`: the AM-gate deny body.
        let (status, body) = deny();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            body.0,
            json!({"error": "You don't have the required permissions."})
        );
    }

    #[test]
    fn fav_missing_view_deserializes_to_none() {
        // `view/base.py:421-424`: `request.data.get("view")` passes None
        // straight through (`entity_identifier` is nullable) — the handler
        // stores NULL and still returns 204, with no validation error.
        // Exercises the real extraction helper (not just serde), so the
        // NULL-passthrough contract breaks loudly if it changes. No DB
        // infra needed: the helper is pure.
        let body: CreateFavorite = serde_json::from_value(json!({})).unwrap();
        assert_eq!(body.view, None);
        assert_eq!(favorite_entity_identifier(&body), None);

        let id = uuid::Uuid::new_v4();
        let body: CreateFavorite =
            serde_json::from_value(json!({ "view": id })).unwrap();
        assert_eq!(body.view, Some(id));
        assert_eq!(favorite_entity_identifier(&body), Some(id));
    }

    #[test]
    fn view_query_post_branch_quirks() {
        // `type` always sets the groups (default all five).
        let q = build_view_query(&json!({}));
        assert_eq!(
            q["state__group__in"],
            json!(["backlog", "unstarted", "started", "completed", "cancelled"])
        );
        assert_eq!(build_view_query(&json!({"type": "active"}))["state__group__in"], json!(["unstarted", "started"]));
        // `sub_issue` missing or "false" filters parents out; JSON false
        // does NOT (Python `False != "false"`).
        assert_eq!(build_view_query(&json!({}))["parent__isnull"], json!(true));
        assert_eq!(build_view_query(&json!({"sub_issue": "false"}))["parent__isnull"], json!(true));
        assert!(build_view_query(&json!({"sub_issue": false})).get("parent__isnull").is_none());
        // `intake_status` reads the `inbox_status` value (quirk).
        let q = build_view_query(&json!({"inbox_status": [-2]}));
        assert_eq!(q["issue_intake__status__in"], json!([-2]));
        // Soft-delete guards are unconditional.
        let q = build_view_query(&json!({}));
        for k in [
            "label_issue__deleted_at__isnull",
            "issue_assignee__deleted_at__isnull",
            "issue_cycle__deleted_at__isnull",
            "issue_module__deleted_at__isnull",
            "issue_subscribers__deleted_at__isnull",
        ] {
            assert_eq!(q[k], json!(true));
        }
        // Empty filters → empty query (serializer.create `bool({})`).
        assert_eq!(build_view_query(&json!({})).get("state__in"), None);
    }
}
