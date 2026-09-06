use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::member::deny_detail;
use super::project::{deny, missing, ws_role};
use super::userprops::INVALID_DETAIL_MSG;
use crate::{middleware::auth::AuthUser, state::AppState};

// ============================================================================
// E7 — workspace prefs + misc.
//
// Covers (Django path → handler):
// - E7a `GET+PATCH .../user-properties/` → `user_props_get/patch`
//   (`plane/app/views/workspace/user.py:252-269`,
//   `serializers/workspace.py:174-178`).
// - Sidebar `GET+PATCH .../sidebar-preferences/` → `sidebar_get/patch`
//   (`plane/app/views/workspace/user_preference.py:18-103`,
//   `db/models/workspace.py:417-427`).
// - Home `GET .../home-preferences/` + `PATCH .../home-preferences/:key/`
//   → `home_list/patch` (`plane/app/views/workspace/home.py:24-79`,
//   `db/models/workspace.py:374-382`).
// - E7b quick-links → `quick_*`
//   (`plane/app/views/workspace/quick_link.py:24-61`,
//   `serializers/workspace.py:187-217`).
// - Recent-visits `GET .../recent-visits/` → `recent_list`
//   (`plane/app/views/workspace/recent_visit.py:25-33`,
//   `serializers/workspace.py:308-329`).
// - Workspace-views `POST .../workspace-views/` → `views_post`
//   (`plane/app/views/workspace/member.py:208-212`).
// - E7c ws estimates `GET .../estimates/` → `ws_estimates`
//   (`plane/app/views/workspace/estimate.py:22`,
//   `serializers/estimate.py:44-50`).
// - Global slug-check `GET /api/workspace-slug-check/` → `slug_check`
//   (`plane/app/views/workspace/base.py:215-224`).
// - Unsplash `GET /api/unsplash/` → `unsplash`
//   (`plane/app/views/external/base.py:215-243`).
// - Last-visited `GET /api/users/last-visited-workspace/` → `last_visited`
//   (`plane/app/views/workspace/user.py:68-95`).
//
// Locked conventions reused (not forked): `deny()` for
// `@allow_permission` denials (`permissions/base.py:81-84`,
// `{"error": "You don't have the required permissions."}`),
// `deny_detail()` for DRF permission-class denials (`member.rs`,
// `{"detail": "You do not have permission to perform this action."}`),
// `missing()` for generic 404s (`views/base.py:92-96`), `AuthUser` for
// the 401 (`middleware/auth.rs`), `INVALID_DETAIL_MSG` for 400
// detail-msgs (`views/base.py:100-104`). Celery/activity side-effects
// skipped (none of these endpoints emit any); recent-visit WRITES stay
// OUT (reads only).
//
// Live columns verified 2026-09-06 via
// `docker exec plane-db psql -U plane -d plane -c "\d <table>"`:
// workspace_user_properties (id, created_at, updated_at, created_by_id,
// updated_by_id, user_id, workspace_id, deleted_at, filters, display_
// filters, display_properties, rich_filters, navigation_control_
// preference, navigation_project_limit); workspace_user_preferences
// (+key, is_pinned, sort_order); workspace_home_preferences (+key,
// is_enabled, config, sort_order); workspace_user_links (+title, url,
// metadata, owner_id, project_id); user_recent_visits (+entity_
// identifier, entity_name, visited_at, project_id, user_id);
// estimates (+name, description, project_id, type, last_used);
// estimate_points (+key, description, value, estimate_id, project_id).
// ============================================================================

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/workspace/quick_link.py:52` (PATCH miss — `detail` key).
pub const QUICK_LINK_NOT_FOUND_DETAIL: &str = "Quick link not found.";
/// `plane/app/views/workspace/quick_link.py:60` (GET-detail miss — `error` key).
pub const QUICK_LINK_NOT_FOUND_ERROR: &str = "Quick link not found.";
/// `plane/app/views/workspace/home.py:79` (PATCH miss → **400**, NOT 404).
pub const HOME_PREF_NOT_FOUND_MSG: &str = "Preference not found";
/// `plane/app/views/workspace/base.py:218-221` (slug-check, no slug → 400).
pub const SLUG_REQUIRED_MSG: &str = "Workspace Slug is required";
/// `plane/app/views/workspace/user_preference.py:103` (sidebar PATCH, always).
pub const SIDEBAR_UPDATED_MSG: &str = "Successfully updated";
/// `plane/app/serializers/workspace.py:197-201` (`validate_url`).
pub const INVALID_URL_MSG: &str = "Invalid URL format.";
/// `plane/app/serializers/workspace.py:212-214,224-226` (create/update dup).
pub const DUP_URL_MSG: &str = "URL already exists for this workspace and owner";

/// `plane/utils/constants.py:5-64` (`RESTRICTED_WORKSPACE_SLUGS`) — full
/// list, verbatim (incl. the `"mobile"`/`"monitor"`/`"config"` dupes).
/// NOTE: `routes::workspace::RESTRICTED_SLUGS` is a SUBSET (missing chat,
/// calendar, drive, channels, upgrade, billing) kept for create-validation,
/// so the slug-check uses this complete copy instead of reusing it.
pub const RESTRICTED_WORKSPACE_SLUGS: &[&str] = &[
    "404", "accounts", "api", "create-workspace", "god-mode", "installations", "invitations",
    "onboarding", "profile", "spaces", "workspace-invitations", "password", "flags", "monitor",
    "monitoring", "ingest", "plane-pro", "plane-ultimate", "enterprise", "plane-enterprise",
    "disco", "silo", "chat", "calendar", "drive", "channels", "upgrade", "billing", "sign-in",
    "sign-up", "signin", "signup", "config", "live", "admin", "m", "import", "importers",
    "integrations", "integration", "configuration", "initiatives", "initiative", "workflow",
    "workflows", "epics", "epic", "story", "mobile", "dashboard", "desktop", "onload",
    "real-time", "one", "pages", "business", "pro", "settings", "license", "licenses",
    "instances", "instance",
];

/// Sidebar keys in enum declaration order
/// (`plane/db/models/workspace.py:419-425`,
/// `UserPreferenceKeys`: VIEWS, ACTIVE_CYCLES, ANALYTICS, DRAFTS,
/// YOUR_WORK, ARCHIVES, STICKIES).
pub const SIDEBAR_KEYS: &[&str] = &[
    "views",
    "active_cycles",
    "analytics",
    "drafts",
    "your_work",
    "archives",
    "stickies",
];

/// Home widget keys served by the endpoint: `HomeWidgetKeys`
/// (`plane/db/models/workspace.py:377-382`) MINUS `quick_tutorial` and
/// `new_at_plane` (`plane/app/views/workspace/home.py:32-36`).
pub const HOME_KEYS: &[&str] = &["quick_links", "recents", "my_stickies"];

/// Recent-visit entity allowlist
/// (`plane/app/views/workspace/recent_visit.py:31`,
/// `entity_name__in=["issue", "page", "project"]`).
pub const RECENT_ENTITIES: &[&str] = &["issue", "page", "project"];

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
/// (`plane/app/permissions/base.py:17-63`): roles 20/15/5 pass.
pub fn guard_amg(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(super::project::FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors the sidebar auto-create
/// (`plane/app/views/workspace/user_preference.py:43-63`): `sort_order =
/// 65535 + i*10000` where `i` is the index within the MISSING keys (the
/// loop appends then bulk-creates per iteration, so a pre-existing key
/// does NOT consume an index — Django bug preserved).
pub fn sidebar_sort_order(missing_index: usize) -> f64 {
    65535.0 + missing_index as f64 * 10000.0
}

/// Mirrors the sidebar pin rule (`user_preference.py:51-59`): `is_pinned`
/// ONLY for drafts/your_work/stickies.
/// Test-only: production applies this inline in the INSERT below.
#[cfg(test)]
pub fn sidebar_pinned(key: &str) -> bool {
    matches!(key, "drafts" | "your_work" | "stickies")
}

/// Full default rows for the all-missing case, in enum order:
/// `(key, is_pinned, sort_order)`.
/// Test-only (cycle.rs `cycle_status` precedent): production inserts via SQL.
#[cfg(test)]
pub fn sidebar_defaults() -> Vec<(String, bool, f64)> {
    SIDEBAR_KEYS
        .iter()
        .enumerate()
        .map(|(i, k)| {
            (
                k.to_string(),
                matches!(*k, "drafts" | "your_work" | "stickies"),
                sidebar_sort_order(i),
            )
        })
        .collect()
}

/// Mirrors the home auto-create sort
/// (`plane/app/views/workspace/home.py:44-58`): `sort_order = 1000 -
/// counter`, counter starting at 1 per missing key in `HOME_KEYS` order.
/// (The per-iteration bulk-create + `ignore_conflicts` means each missing
/// key keeps the FIRST sort computed for it — equivalent to this rule.)
pub fn home_sort_order(missing_position_1based: usize) -> f64 {
    1000.0 - missing_position_1based as f64
}

/// Mirrors `WorkspaceUserLinkSerializer.to_internal_value`
/// (`plane/app/serializers/workspace.py:192-197`): prepend `http://`
/// when the scheme is missing.
pub fn normalize_link_url(url: &str) -> String {
    let t = url.trim().to_string();
    if t.starts_with("http://") || t.starts_with("https://") {
        t
    } else {
        format!("http://{t}")
    }
}

/// Mirrors `validate_url` (`serializers/workspace.py:199-206`, Django
/// `URLValidator`): http/https scheme + non-empty host, no whitespace.
/// (Django additionally enforces TLD/IDNA rules — accepting e.g.
/// `http://localhost` here is a documented leniency.)
pub fn validate_link_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| INVALID_URL_MSG.to_string())?;
    let host = rest.split('/').next().unwrap_or("").split('@').next_back().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    if host.is_empty() || url.chars().any(|c| c.is_whitespace()) {
        return Err(INVALID_URL_MSG.to_string());
    }
    Ok(())
}

/// 400-vs-200 decision for the slug-check
/// (`plane/app/views/workspace/base.py:215-224`): Django reads
/// `request.GET.get("slug", False)` and guards
/// `if not slug or slug == ""` — an EXACT comparison with NO trimming —
/// so ONLY a missing param or exactly `""` → 400
/// `{"error": "Workspace Slug is required"}`. Anything else (including
/// whitespace-only) passes through to the 200 availability check.
pub fn guard_slug_present(slug: Option<&str>) -> Result<String, String> {
    match slug {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(SLUG_REQUIRED_MSG.to_string()),
    }
}

/// Availability rule (`base.py:224-225`): taken when the slug exists
/// (case-insensitive — locked contract) OR is restricted (Django-exact
/// case-sensitive `in`, preserved).
pub fn slug_available(exists_iexact: bool, slug: &str) -> bool {
    !(exists_iexact || RESTRICTED_WORKSPACE_SLUGS.contains(&slug))
}

/// Recent-visit row predicate mirroring the chained filters
/// (`recent_visit.py:27-31`): `?entity_name=` narrows first, then the
/// forced `entity_name__in` allowlist applies — a non-allowlisted query
/// value therefore matches NOTHING (empty 200, not 400).
pub fn recent_row_visible(query: Option<&str>, row_entity: &str) -> bool {
    if let Some(q) = query {
        if q != row_entity {
            return false;
        }
    }
    RECENT_ENTITIES.contains(&row_entity)
}

/// Unsplash URL builder (`plane/app/views/external/base.py:232-236`).
/// FIXES the Django `&page=${page}` literal-`$` bug (search branch
/// interpolates `page=${page}` → upstream receives e.g. `page=$2`;
/// documented deviation — we send the real page).
pub fn unsplash_url(query: Option<&str>, page: &str, per_page: &str, key: &str) -> String {
    match query.filter(|q| !q.trim().is_empty()) {
        Some(q) => format!(
            "https://api.unsplash.com/search/photos/?client_id={key}&query={q}&page={page}&per_page={per_page}"
        ),
        None => format!(
            "https://api.unsplash.com/photos/?client_id={key}&page={page}&per_page={per_page}"
        ),
    }
}

/// Null-shape for last-visited with no workspace
/// (`plane/app/views/workspace/user.py:76-80`).
pub fn last_visited_null() -> Value {
    json!({"project_details": [], "workspace_details": {}})
}

/// PATCH-miss body for quick-links (`quick_link.py:52`).
pub fn quick_patch_miss() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"detail": QUICK_LINK_NOT_FOUND_DETAIL})),
    )
}

/// GET-detail-miss body for quick-links (`quick_link.py:60` — `error` key).
pub fn quick_detail_miss() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": QUICK_LINK_NOT_FOUND_ERROR})),
    )
}

/// PATCH-miss body for home preferences (`home.py:79` — 400, NOT 404).
pub fn home_patch_miss() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"detail": HOME_PREF_NOT_FOUND_MSG})),
    )
}

/// Default `filters` for workspace user-properties creation
/// (`plane/db/models/workspace.py:62-73`).
fn default_ws_filters() -> Value {
    json!({
        "priority": null, "state": null, "state_group": null,
        "assignees": null, "created_by": null, "labels": null,
        "start_date": null, "target_date": null, "subscriber": null,
    })
}

/// Default `display_filters` (`workspace.py:76-87` — note the Django
/// nesting under a `display_filters` key, preserved verbatim).
fn default_ws_display_filters() -> Value {
    json!({"display_filters": {
        "group_by": null, "order_by": "-created_at", "type": null,
        "sub_issue": true, "show_empty_groups": true,
        "layout": "list", "calendar_date_range": "",
    }})
}

/// Default `display_properties` (`workspace.py:90-104` — same nesting).
fn default_ws_display_properties() -> Value {
    json!({"display_properties": {
        "assignee": true, "attachment_count": true, "created_on": true,
        "due_date": true, "estimate": true, "key": true, "labels": true,
        "link": true, "priority": true, "start_date": true, "state": true,
        "sub_issue_count": true, "updated_on": true,
    }})
}

fn opt_id(id: &Option<uuid::Uuid>) -> Value {
    id.map(|u| json!(u)).unwrap_or(Value::Null)
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

/// Workspace gate for `@allow_permission(..., level="WORKSPACE")` with an
/// AMG role list: pass on 20/15/5, else `deny()` (403 `{"error": ...}`).
async fn gate_ws_amg(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(guard_amg(ws_role(pool, user, slug).await?).is_ok())
}

/// Workspace gate for DRF permission-class endpoints
/// (`WorkspaceViewerPermission`, `workspace.py:93-100` /
/// `WorkspaceEntityPermission` safe branch, `workspace.py:74-82`): any
/// ACTIVE membership passes, else `deny_detail()` (403 `{"detail": ...}`).
async fn gate_ws_any(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(ws_role(pool, user, slug).await?.is_some())
}

// ============================================================================
// E7a — workspace user-properties (`user.py:252-269`).
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsPropRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    user_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    filters: Value,
    display_filters: Value,
    display_properties: Value,
    rich_filters: Value,
    navigation_control_preference: String,
    navigation_project_limit: i32,
}

/// Serializes like `WorkspaceUserPropertiesSerializer`
/// (`serializers/workspace.py:174-178`, `fields = "__all__"`): every
/// model column, FKs as id strings.
fn ws_prop_json(r: &WsPropRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": opt_id(&r.created_by_id),
        "updated_by": opt_id(&r.updated_by_id),
        "user": r.user_id,
        "workspace": r.workspace_id,
        "deleted_at": r.deleted_at,
        "filters": r.filters,
        "display_filters": r.display_filters,
        "display_properties": r.display_properties,
        "rich_filters": r.rich_filters,
        "navigation_control_preference": r.navigation_control_preference,
        "navigation_project_limit": r.navigation_project_limit,
    })
}

const WS_PROP_SELECT: &str = "p.id, p.created_at, p.updated_at, p.created_by_id, p.updated_by_id, \
    p.user_id, p.workspace_id, p.deleted_at, p.filters, p.display_filters, p.display_properties, \
    p.rich_filters, p.navigation_control_preference, p.navigation_project_limit";

/// Validates `navigation_control_preference` for the user-properties PATCH
/// (`plane/app/views/workspace/user.py:252-264` partial
/// `WorkspaceUserPropertiesSerializer`; choices
/// `NavigationControlPreference`: ACCORDION/TABBED,
/// `plane/db/models/workspace.py:311-313`). Returns the 400 message when
/// invalid, `None` when valid. DRF `ChoiceField` message
/// (`rest_framework/fields.py`,
/// `invalid_choice = '"{input}" is not a valid choice.'`) with the
/// submitted value interpolated; non-string JSON renders in its JSON form
/// (DRF stringifies `input` the same way).
fn nav_pref_error(value: &Value) -> Option<String> {
    if value
        .as_str()
        .is_some_and(|s| s == "ACCORDION" || s == "TABBED")
    {
        return None;
    }
    let shown = value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());
    Some(format!("\"{shown}\" is not a valid choice."))
}

/// `get_or_create(user, workspace)` (`user.py:257-259,266-268`): single
/// INSERT..SELECT with a predicate-matching `ON CONFLICT DO NOTHING`
/// (mirrors the partial unique index
/// `workspace_user_properties_unique_workspace_user_when_deleted_at_null`),
/// then SELECT the live row. Unknown slug → `missing()` (Django
/// `Workspace.objects.get` raises → sane 404, locked deviation).
async fn get_or_create_ws_prop(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<Option<WsPropRow>, sqlx::Error> {
    sqlx::query(
        "INSERT INTO workspace_user_properties (id, workspace_id, user_id, filters, \
         display_filters, display_properties, rich_filters, navigation_project_limit, \
         navigation_control_preference, created_at, updated_at) \
         SELECT gen_random_uuid(), w.id, $1, $2, $3, $4, '{}', 10, 'ACCORDION', now(), now() \
         FROM workspaces w WHERE w.slug = $5 AND w.deleted_at IS NULL \
         ON CONFLICT (workspace_id, user_id) WHERE deleted_at IS NULL DO NOTHING",
    )
    .bind(user)
    .bind(default_ws_filters())
    .bind(default_ws_display_filters())
    .bind(default_ws_display_properties())
    .bind(slug)
    .execute(pool)
    .await?;
    sqlx::query_as::<_, WsPropRow>(&format!(
        "SELECT {WS_PROP_SELECT} FROM workspace_user_properties p \
         JOIN workspaces w ON w.id = p.workspace_id \
         WHERE w.slug = $1 AND p.user_id = $2 AND p.deleted_at IS NULL"
    ))
    .bind(slug)
    .bind(user)
    .fetch_optional(pool)
    .await
}

pub async fn user_props_get(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_any(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    match get_or_create_ws_prop(&st.pool, auth.0, &slug).await? {
        Some(row) => Ok((StatusCode::OK, Json(ws_prop_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn user_props_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_any(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    let Some(mut row) = get_or_create_ws_prop(&st.pool, auth.0, &slug).await? else {
        return Ok(missing());
    };
    // Partial serializer (`serializers/workspace.py:174-178`,
    // read-only `workspace,user`): every other key is replaced when
    // present; `navigation_project_limit` (IntegerField) and
    // `navigation_control_preference` (ACCORDION/TABBED choices) are
    // validated like DRF, unknown keys ignored.
    if let Some(v) = body.get("filters") {
        row.filters = v.clone();
    }
    if let Some(v) = body.get("display_filters") {
        row.display_filters = v.clone();
    }
    if let Some(v) = body.get("display_properties") {
        row.display_properties = v.clone();
    }
    if let Some(v) = body.get("rich_filters") {
        row.rich_filters = v.clone();
    }
    if let Some(v) = body.get("navigation_project_limit") {
        let Some(n) = v.as_i64().and_then(|n| i32::try_from(n).ok()) else {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"navigation_project_limit": ["A valid integer is required."]})),
            ));
        };
        row.navigation_project_limit = n;
    }
    if let Some(v) = body.get("navigation_control_preference") {
        if let Some(msg) = nav_pref_error(v) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"navigation_control_preference": [msg]})),
            ));
        }
        row.navigation_control_preference = v.as_str().unwrap_or("ACCORDION").to_string();
    }
    let res = sqlx::query(
        "UPDATE workspace_user_properties SET filters = $1, display_filters = $2, \
         display_properties = $3, rich_filters = $4, navigation_project_limit = $5, \
         navigation_control_preference = $6, updated_at = now() WHERE id = $7",
    )
    .bind(&row.filters)
    .bind(&row.display_filters)
    .bind(&row.display_properties)
    .bind(&row.rich_filters)
    .bind(row.navigation_project_limit)
    .bind(&row.navigation_control_preference)
    .bind(row.id)
    .execute(&st.pool)
    .await;
    if let Err(e) = res {
        if is_constraint_violation(&e) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"detail": INVALID_DETAIL_MSG})),
            ));
        }
        return Err(e.into());
    }
    row.updated_at = chrono::Utc::now();
    Ok((StatusCode::OK, Json(ws_prop_json(&row))))
}

// ============================================================================
// Sidebar preferences (`user_preference.py:18-103`).
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct SidebarRow {
    key: String,
    is_pinned: bool,
    sort_order: f64,
}

pub async fn sidebar_get(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let ws_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(ws_id) = ws_id else {
        return Ok(missing());
    };
    // Auto-create missing keys (`user_preference.py:33-63`) in ONE tx:
    // each missing key gets `65535 + i*10000` (i = index within the
    // missing set) and `is_pinned` only for drafts/your_work/stickies.
    // `ON CONFLICT DO NOTHING` mirrors `ignore_conflicts=True`.
    let mut tx = st.pool.begin().await?;
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM workspace_user_preferences \
         WHERE workspace_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(ws_id)
    .bind(auth.0)
    .fetch_all(&mut *tx)
    .await?;
    let mut idx = 0usize;
    for key in SIDEBAR_KEYS {
        if existing.iter().any(|k| k == key) {
            continue;
        }
        let pinned = matches!(*key, "drafts" | "your_work" | "stickies");
        sqlx::query(
            "INSERT INTO workspace_user_preferences (id, workspace_id, user_id, key, \
             is_pinned, sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now()) \
             ON CONFLICT (workspace_id, user_id, key) WHERE deleted_at IS NULL DO NOTHING",
        )
        .bind(ws_id)
        .bind(auth.0)
        .bind(*key)
        .bind(pinned)
        .bind(sidebar_sort_order(idx))
        .execute(&mut *tx)
        .await?;
        idx += 1;
    }
    tx.commit().await?;
    let rows: Vec<SidebarRow> = sqlx::query_as(
        "SELECT key, is_pinned, sort_order FROM workspace_user_preferences \
         WHERE workspace_id = $1 AND user_id = $2 AND deleted_at IS NULL \
         ORDER BY sort_order",
    )
    .bind(ws_id)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    // 200 dict `{key: {is_pinned, sort_order}}` ordered
    // (`user_preference.py:65-85`).
    let mut map = serde_json::Map::new();
    for r in &rows {
        map.insert(r.key.clone(), json!({"is_pinned": r.is_pinned, "sort_order": r.sort_order}));
    }
    Ok((StatusCode::OK, Json(Value::Object(map))))
}

pub async fn sidebar_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // PATCH body `[{key, is_pinned?, sort_order?}]`
    // (`user_preference.py:87-103`): items without a key or with an
    // unknown key are SKIPPED; always 200 `{"message": ...}`.
    // A non-array body would crash Django (iterating a dict yields str
    // keys → `.pop` AttributeError → 500); sane 400 detail instead.
    let Some(items) = body.as_array() else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"detail": INVALID_DETAIL_MSG})),
        ));
    };
    let mut tx = st.pool.begin().await?;
    for item in items {
        let Some(key) = item.get("key").and_then(Value::as_str) else {
            continue;
        };
        if !SIDEBAR_KEYS.contains(&key) {
            continue;
        }
        // Type-mismatched fields are skipped (Django's unvalidated
        // `save(update_fields=...)` would 500 on a bad cast instead).
        let pinned: Option<bool> = item.get("is_pinned").and_then(Value::as_bool);
        let order: Option<f64> = item.get("sort_order").and_then(Value::as_f64);
        if pinned.is_none() && order.is_none() {
            continue;
        }
        sqlx::query(
            "UPDATE workspace_user_preferences SET \
             is_pinned = COALESCE($1, is_pinned), \
             sort_order = COALESCE($2, sort_order), updated_at = now() \
             WHERE workspace_id = (SELECT id FROM workspaces WHERE slug = $3 AND deleted_at IS NULL) \
             AND user_id = $4 AND key = $5 AND deleted_at IS NULL",
        )
        .bind(pinned)
        .bind(order)
        .bind(&slug)
        .bind(auth.0)
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::OK, Json(json!({"message": SIDEBAR_UPDATED_MSG}))))
}

// ============================================================================
// Home preferences (`home.py:24-79`). NO `:key/` GET/DELETE — Django wires
// only collection-GET + per-key PATCH (`urls/workspace.py:230-238`); the
// FE per-key PATCH stays OUT per contract.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct HomeRow {
    key: String,
    is_enabled: bool,
    config: Value,
    sort_order: f64,
}

pub async fn home_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let ws_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(ws_id) = ws_id else {
        return Ok(missing());
    };
    // Auto-create (`home.py:30-58`): `sort_order = 1000 - counter`,
    // counter from 1 over the missing keys in HOME_KEYS order.
    let mut tx = st.pool.begin().await?;
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM workspace_home_preferences \
         WHERE workspace_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(ws_id)
    .bind(auth.0)
    .fetch_all(&mut *tx)
    .await?;
    let mut pos = 0usize;
    for key in HOME_KEYS {
        if existing.iter().any(|k| k == key) {
            continue;
        }
        pos += 1;
        sqlx::query(
            "INSERT INTO workspace_home_preferences (id, workspace_id, user_id, key, \
             is_enabled, config, sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, true, '{}', $4, now(), now()) \
             ON CONFLICT (workspace_id, user_id, key) WHERE deleted_at IS NULL DO NOTHING",
        )
        .bind(ws_id)
        .bind(auth.0)
        .bind(*key)
        .bind(home_sort_order(pos))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let rows: Vec<HomeRow> = sqlx::query_as(
        "SELECT key, is_enabled, config, sort_order FROM workspace_home_preferences \
         WHERE workspace_id = $1 AND user_id = $2 AND deleted_at IS NULL \
         ORDER BY sort_order DESC",
    )
    .bind(ws_id)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    // 200 list `[{key, is_enabled, config, sort_order}]` (`home.py:60-64`).
    // (ORDER BY sort_order DESC keeps key order 999/998/997; Django's
    // default `-created_at` is tie-ambiguous for bulk rows — documented
    // ordering choice.)
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|r| json!({"key": r.key, "is_enabled": r.is_enabled,
                            "config": r.config, "sort_order": r.sort_order}))
            .collect::<Vec<_>>())),
    ))
}

/// Parses `is_enabled` for the home PATCH (`home.py:74-77` partial
/// `WorkspaceHomePreferenceSerializer`,
/// `serializers/workspace.py:332-336`). Absent keeps the current value;
/// present-but-wrong-type → DRF `BooleanField` serializer error
/// (`rest_framework/fields.py`,
/// `BooleanField.default_error_messages = {invalid: "Must be a valid boolean."}`,
/// from `models.BooleanField`). Source note: message quoted from DRF's
/// `BooleanField` (model field `is_enabled = models.BooleanField`,
/// `db/models/workspace.py:391`).
fn home_bool_opt(value: Option<&Value>, cur: bool) -> Result<bool, String> {
    match value {
        None => Ok(cur),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| "Must be a valid boolean.".to_string()),
    }
}

/// Parses `sort_order` for the home PATCH (same serializer). Absent keeps
/// the current value; present-but-wrong-type → DRF `FloatField`
/// serializer error (`rest_framework/fields.py`,
/// `FloatField.default_error_messages = {invalid: "A valid number is required."}`,
/// from `models.FloatField`). Source note: message quoted from DRF's
/// `FloatField` (model field `sort_order = models.FloatField`,
/// `db/models/workspace.py:393`).
fn home_order_opt(value: Option<&Value>, cur: f64) -> Result<f64, String> {
    match value {
        None => Ok(cur),
        Some(v) => v
            .as_f64()
            .ok_or_else(|| "A valid number is required.".to_string()),
    }
}

pub async fn home_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, key)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let row: Option<HomeRow> = sqlx::query_as(
        "SELECT key, is_enabled, config, sort_order FROM workspace_home_preferences p \
         JOIN workspaces w ON w.id = p.workspace_id \
         WHERE w.slug = $1 AND p.user_id = $2 AND p.key = $3 AND p.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(auth.0)
    .bind(&key)
    .fetch_optional(&st.pool)
    .await?;
    let Some(cur) = row else {
        return Ok(home_patch_miss());
    };
    // Partial through `WorkspaceHomePreferenceSerializer`
    // (`serializers/workspace.py:335-338`, fields ONLY
    // `key,is_enabled,sort_order`): `config` is READ-ONLY — present in
    // the body or not, it is never written.
    let new_key = body.get("key").and_then(Value::as_str).unwrap_or(&cur.key);
    let new_enabled = match home_bool_opt(body.get("is_enabled"), cur.is_enabled) {
        Ok(b) => b,
        Err(msg) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"is_enabled": [msg]})),
            ));
        }
    };
    let new_order = match home_order_opt(body.get("sort_order"), cur.sort_order) {
        Ok(n) => n,
        Err(msg) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"sort_order": [msg]})),
            ));
        }
    };
    let res = sqlx::query(
        "UPDATE workspace_home_preferences SET key = $1, is_enabled = $2, sort_order = $3, \
         updated_at = now() WHERE workspace_id = \
         (SELECT id FROM workspaces WHERE slug = $4 AND deleted_at IS NULL) \
         AND user_id = $5 AND key = $6 AND deleted_at IS NULL",
    )
    .bind(new_key)
    .bind(new_enabled)
    .bind(new_order)
    .bind(&slug)
    .bind(auth.0)
    .bind(&key)
    .execute(&st.pool)
    .await;
    if let Err(e) = res {
        if is_constraint_violation(&e) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"detail": INVALID_DETAIL_MSG})),
            ));
        }
        return Err(e.into());
    }
    Ok((
        StatusCode::OK,
        Json(json!({"key": new_key, "is_enabled": new_enabled, "sort_order": new_order})),
    ))
}

// ============================================================================
// E7b — quick-links (`quick_link.py:24-61`).
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct QuickRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    title: Option<String>,
    url: String,
    metadata: Value,
    created_by_id: Option<uuid::Uuid>,
    owner_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
}

/// Serializes like `WorkspaceUserLinkSerializer`
/// (`serializers/workspace.py:187-190`, `fields = "__all__"`).
fn quick_json(r: &QuickRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "deleted_at": r.deleted_at,
        "title": r.title.as_ref().map(|t| json!(t)).unwrap_or(Value::Null),
        "url": r.url,
        "metadata": r.metadata,
        "created_by": opt_id(&r.created_by_id),
        "owner": r.owner_id,
        "project": r.project_id.map(|u| json!(u)).unwrap_or(Value::Null),
        "updated_by": opt_id(&r.updated_by_id),
        "workspace": r.workspace_id,
    })
}

const QUICK_SELECT: &str = "l.id, l.created_at, l.updated_at, l.deleted_at, l.title, l.url, \
    l.metadata, l.created_by_id, l.owner_id, l.project_id, l.updated_by_id, l.workspace_id";

/// Unaliased twin of [`QUICK_SELECT`] for `INSERT/UPDATE ... RETURNING`
/// (no `FROM` alias `l` in scope there — `RETURNING l.id` 500s with
/// `missing FROM-clause entry for table "l"`).
const QUICK_RETURNING: &str = "id, created_at, updated_at, deleted_at, title, url, \
    metadata, created_by_id, owner_id, project_id, updated_by_id, workspace_id";

pub async fn quick_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let ws_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(ws_id) = ws_id else {
        return Ok(missing());
    };
    // `url` required (DRF `required=True`); `title?/metadata?` optional
    // (`quick_link.py:29-35`, serializer `__all__` minus read-only
    // `workspace,owner`).
    let Some(raw_url) = body.get("url").and_then(Value::as_str) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"url": ["This field is required."]})),
        ));
    };
    if let Some(t) = body.get("title").and_then(Value::as_str) {
        if t.chars().count() > 255 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"title": ["Ensure this field has no more than 255 characters."]})),
            ));
        }
    }
    let url = normalize_link_url(raw_url);
    if let Err(e) = validate_link_url(&url) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    // Unique per ws+owner (`serializers/workspace.py:205-214`): dup → 400.
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_user_links \
         WHERE url = $1 AND workspace_id = $2 AND owner_id = $3 AND deleted_at IS NULL)",
    )
    .bind(&url)
    .bind(ws_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": DUP_URL_MSG}))));
    }
    let title: Option<&str> = body.get("title").and_then(Value::as_str);
    let metadata: Value = body.get("metadata").cloned().unwrap_or_else(|| json!({}));
    let row: Option<QuickRow> = sqlx::query_as(&format!(
        "INSERT INTO workspace_user_links (id, workspace_id, owner_id, created_by_id, \
         title, url, metadata, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $2, $3, $4, $5, now(), now()) \
         RETURNING {QUICK_RETURNING}"
    ))
    .bind(ws_id)
    .bind(auth.0)
    .bind(title)
    .bind(&url)
    .bind(&metadata)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::CREATED, Json(quick_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn quick_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // Owner-scoped list, model ordering `-created_at`
    // (`quick_link.py:57-61`, `workspace.py:368`).
    let rows: Vec<QuickRow> = sqlx::query_as(&format!(
        "SELECT {QUICK_SELECT} FROM workspace_user_links l \
         JOIN workspaces w ON w.id = l.workspace_id \
         WHERE w.slug = $1 AND l.owner_id = $2 AND l.deleted_at IS NULL \
         ORDER BY l.created_at DESC"
    ))
    .bind(&slug)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(quick_json).collect::<Vec<_>>())),
    ))
}

pub async fn quick_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // Owner-scoped `.get()` (`quick_link.py:44-50`): miss → 404
    // `{"error": "Quick link not found."}` (the `error`-key twin).
    let row: Option<QuickRow> = sqlx::query_as(&format!(
        "SELECT {QUICK_SELECT} FROM workspace_user_links l \
         JOIN workspaces w ON w.id = l.workspace_id \
         WHERE l.id = $1 AND w.slug = $2 AND l.owner_id = $3 AND l.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(quick_json(&r)))),
        None => Ok(quick_detail_miss()),
    }
}

pub async fn quick_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // Owner-scoped `.filter().first()` (`quick_link.py:37-42`): miss →
    // 404 `{"detail": "Quick link not found."}` (the `detail`-key twin).
    let row: Option<QuickRow> = sqlx::query_as(&format!(
        "SELECT {QUICK_SELECT} FROM workspace_user_links l \
         JOIN workspaces w ON w.id = l.workspace_id \
         WHERE l.id = $1 AND w.slug = $2 AND l.owner_id = $3 AND l.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some(cur) = row else {
        return Ok(quick_patch_miss());
    };
    let mut title = cur.title.clone();
    let mut url = cur.url.clone();
    let mut metadata = cur.metadata.clone();
    if let Some(t) = body.get("title") {
        if t.is_null() {
            title = None;
        } else if let Some(s) = t.as_str() {
            if s.chars().count() > 255 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"title": ["Ensure this field has no more than 255 characters."]})),
                ));
            }
            title = Some(s.to_string());
        }
    }
    if let Some(u) = body.get("url").and_then(Value::as_str) {
        let norm = normalize_link_url(u);
        if let Err(e) = validate_link_url(&norm) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
        }
        // Dup check excluding self (`serializers/workspace.py:217-226`).
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_user_links \
             WHERE url = $1 AND workspace_id = $2 AND owner_id = $3 \
             AND id <> $4 AND deleted_at IS NULL)",
        )
        .bind(&norm)
        .bind(cur.workspace_id)
        .bind(auth.0)
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if dup {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": DUP_URL_MSG}))));
        }
        url = norm;
    }
    if let Some(m) = body.get("metadata") {
        metadata = m.clone();
    }
    let next: Option<QuickRow> = sqlx::query_as(&format!(
        "UPDATE workspace_user_links SET title = $1, url = $2, metadata = $3, \
         updated_at = now() WHERE id = $4 RETURNING {QUICK_RETURNING}"
    ))
    .bind(&title)
    .bind(&url)
    .bind(&metadata)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match next {
        Some(r) => Ok((StatusCode::OK, Json(quick_json(&r)))),
        None => Ok(quick_patch_miss()),
    }
}

pub async fn quick_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // Owner-scoped `.get()` (`quick_link.py:52-55`): Django raises
    // `DoesNotExist` → 500; sane generic 404 instead (locked). Delete is
    // a soft-delete UPDATE (locked soft-delete rule).
    let n: u64 = sqlx::query(
        "UPDATE workspace_user_links l SET deleted_at = now() FROM workspaces w \
         WHERE l.id = $1 AND w.slug = $2 AND w.id = l.workspace_id \
         AND l.owner_id = $3 AND l.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// Recent-visits (`recent_visit.py:25-33`). Reads only — POST/DELETE (the
// celery-written paths) stay OUT per contract.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecentQuery {
    #[serde(default)]
    pub entity_name: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RecentRow {
    id: uuid::Uuid,
    entity_name: String,
    entity_identifier: Option<uuid::Uuid>,
    visited_at: chrono::DateTime<chrono::Utc>,
}

/// `IssueRecentVisitSerializer` (`serializers/workspace.py:234-257`):
/// `{id,name,state,priority,assignees,type,sequence_id,project_id,
/// project_identifier}`. `state`/`type` are the FK ids; `assignees` are
/// live `issue_assignees` member ids; miss → null (mirrors the
/// `try/except DoesNotExist → None`, `workspace.py:322-326`).
async fn recent_issue_data(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
) -> Result<Value, sqlx::Error> {
    #[derive(Debug, Clone, sqlx::FromRow)]
    struct IssueHit {
        id: uuid::Uuid,
        name: String,
        state_id: Option<uuid::Uuid>,
        priority: String,
        type_id: Option<uuid::Uuid>,
        sequence_id: i32,
        project_id: uuid::Uuid,
        identifier: Option<String>,
    }
    let hit: Option<IssueHit> = sqlx::query_as(
        "SELECT i.id, i.name, i.state_id, i.priority, i.type_id, i.sequence_id, \
         i.project_id, p.identifier FROM issues i \
         LEFT JOIN projects p ON p.id = i.project_id \
         WHERE i.id = $1 AND i.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(h) = hit else {
        return Ok(Value::Null);
    };
    let assignees: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT assignee_id FROM issue_assignees WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(json!({
        "id": h.id, "name": h.name,
        "state": h.state_id.map(|u| json!(u)).unwrap_or(Value::Null),
        "priority": h.priority, "assignees": assignees,
        "type": h.type_id.map(|u| json!(u)).unwrap_or(Value::Null),
        "sequence_id": h.sequence_id, "project_id": h.project_id,
        "project_identifier": h.identifier.map(|s| json!(s)).unwrap_or(Value::Null),
    }))
}

/// `PageRecentVisitSerializer` (`serializers/workspace.py:275-295`):
/// `{id,name,logo_props,project_id,owned_by,project_identifier}` — the
/// project link resolves via `project_pages` (`page.projects.first()`).
async fn recent_page_data(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<Value, sqlx::Error> {
    #[derive(Debug, Clone, sqlx::FromRow)]
    struct PageHit {
        id: uuid::Uuid,
        name: String,
        logo_props: Value,
        owned_by_id: uuid::Uuid,
        project_id: Option<uuid::Uuid>,
        identifier: Option<String>,
    }
    let hit: Option<PageHit> = sqlx::query_as(
        "SELECT p.id, p.name, p.logo_props, p.owned_by_id, \
         (SELECT pp.project_id FROM project_pages pp WHERE pp.page_id = p.id \
          AND pp.deleted_at IS NULL LIMIT 1) AS project_id, \
         (SELECT pr.identifier FROM project_pages pp JOIN projects pr ON pr.id = pp.project_id \
          WHERE pp.page_id = p.id AND pp.deleted_at IS NULL LIMIT 1) AS identifier \
         FROM pages p WHERE p.id = $1 AND p.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match hit {
        Some(h) => Ok(json!({
            "id": h.id, "name": h.name, "logo_props": h.logo_props,
            "project_id": h.project_id.map(|u| json!(u)).unwrap_or(Value::Null),
            "owned_by": h.owned_by_id,
            "project_identifier": h.identifier.map(|s| json!(s)).unwrap_or(Value::Null),
        })),
        None => Ok(Value::Null),
    }
}

/// `ProjectRecentVisitSerializer` (`serializers/workspace.py:260-272`):
/// `{id,name,logo_props,project_members,identifier}` — members are live
/// non-bot member ids.
async fn recent_project_data(
    pool: &sqlx::PgPool,
    id: uuid::Uuid,
) -> Result<Value, sqlx::Error> {
    #[derive(Debug, Clone, sqlx::FromRow)]
    struct ProjHit {
        id: uuid::Uuid,
        name: String,
        logo_props: Value,
        identifier: String,
    }
    let hit: Option<ProjHit> = sqlx::query_as(
        "SELECT id, name, logo_props, identifier FROM projects \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(h) = hit else {
        return Ok(Value::Null);
    };
    let members: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT pm.member_id FROM project_members pm JOIN users u ON u.id = pm.member_id \
         WHERE pm.project_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL \
         AND u.is_bot = false",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(json!({
        "id": h.id, "name": h.name, "logo_props": h.logo_props,
        "project_members": members, "identifier": h.identifier,
    }))
}

pub async fn recent_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<RecentQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_amg(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // `?entity_name=` narrows, then the forced allowlist applies; cap 20,
    // no pagination; default ordering `-created_at`
    // (`recent_visit.py:25-33`, `recent_visit.py:36`).
    let entity_filter: Option<&str> = q.entity_name.as_deref();
    let rows: Vec<RecentRow> = sqlx::query_as(
        "SELECT v.id, v.entity_name, v.entity_identifier, v.visited_at FROM user_recent_visits v \
         JOIN workspaces w ON w.id = v.workspace_id \
         WHERE w.slug = $1 AND v.user_id = $2 AND v.deleted_at IS NULL \
         AND ($3::text IS NULL OR v.entity_name = $3) \
         AND v.entity_name IN ('issue', 'page', 'project') \
         ORDER BY v.created_at DESC LIMIT 20",
    )
    .bind(&slug)
    .bind(auth.0)
    .bind(entity_filter)
    .fetch_all(&st.pool)
    .await?;
    // Belt-and-braces: the same predicate in Rust (unit-tested) so the
    // SQL and the contract can never drift apart silently.
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        if !recent_row_visible(entity_filter, &r.entity_name) {
            continue;
        }
        let data = match (r.entity_name.as_str(), r.entity_identifier) {
            ("issue", Some(id)) => recent_issue_data(&st.pool, id).await?,
            ("page", Some(id)) => recent_page_data(&st.pool, id).await?,
            ("project", Some(id)) => recent_project_data(&st.pool, id).await?,
            _ => Value::Null,
        };
        out.push(json!({
            "id": r.id, "entity_name": r.entity_name,
            "entity_identifier": r.entity_identifier.map(|u| json!(u)).unwrap_or(Value::Null),
            "entity_data": data, "visited_at": r.visited_at,
        }));
    }
    Ok((StatusCode::OK, Json(json!(out))))
}

// ============================================================================
// Workspace-views (`member.py:208-212`). POST-only.
// ============================================================================

pub async fn views_post(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `.get(workspace__slug, member, is_active=True)` — miss (incl.
    // non-member) → generic 404 (Django raises `DoesNotExist`; locked).
    // NO gate beyond IsAuthenticated (the view defines no permission
    // classes beyond the base).
    let view_props: Value = body.get("view_props").cloned().unwrap_or_else(|| json!({}));
    let n: u64 = sqlx::query(
        "UPDATE workspace_members m SET view_props = $1, updated_at = now() \
         FROM workspaces w WHERE w.slug = $2 AND w.id = m.workspace_id \
         AND m.member_id = $3 AND m.is_active = true AND m.deleted_at IS NULL",
    )
    .bind(&view_props)
    .bind(&slug)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E7c — workspace estimates (`estimate.py:22`). GET-only; NO 2h cache
// (Django `@cache_response(60*60*2)` — caching stays OUT, documented
// deviation); NO POST/PATCH/DELETE on this path.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsEstimateRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    project_id: uuid::Uuid,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    name: String,
    description: String,
    estimate_type: String,
    last_used: bool,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsPointRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    estimate_id: uuid::Uuid,
    project_id: uuid::Uuid,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    point_key: i32,
    description: String,
    value: String,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Serializes like `WorkspaceEstimateSerializer`
/// (`serializers/estimate.py:44-50`, `__all__` + nested `points`).
fn ws_estimate_json(e: &WsEstimateRow, points: &[WsPointRow]) -> Value {
    json!({
        "id": e.id, "created_at": e.created_at, "updated_at": e.updated_at,
        "created_by": opt_id(&e.created_by_id), "project": e.project_id,
        "updated_by": opt_id(&e.updated_by_id), "workspace": e.workspace_id,
        "name": e.name, "description": e.description, "type": e.estimate_type,
        "last_used": e.last_used, "deleted_at": e.deleted_at,
        "points": points.iter().map(|p| json!({
            "id": p.id, "created_at": p.created_at, "updated_at": p.updated_at,
            "created_by": opt_id(&p.created_by_id), "estimate": p.estimate_id,
            "project": p.project_id, "updated_by": opt_id(&p.updated_by_id),
            "workspace": p.workspace_id, "key": p.point_key,
            "description": p.description, "value": p.value,
            "deleted_at": p.deleted_at,
        })).collect::<Vec<_>>(),
    })
}

pub async fn ws_estimates(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `WorkspaceEntityPermission` (`permissions/workspace.py:74-82`): GET
    // is safe → any ACTIVE ws member; deny is the DRF 403 `detail` body.
    if !gate_ws_any(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    // Project-estimate ids for the slug, then those estimates with
    // prefetched points (`estimate.py:25-32`).
    let rows: Vec<WsEstimateRow> = sqlx::query_as(
        "SELECT e.id, e.created_at, e.updated_at, e.created_by_id, e.project_id, \
         e.updated_by_id, e.workspace_id, e.name, e.description, e.type AS estimate_type, \
         e.last_used, e.deleted_at FROM estimates e \
         JOIN workspaces w ON w.id = e.workspace_id \
         WHERE w.slug = $1 AND e.deleted_at IS NULL \
         AND e.id IN (SELECT p.estimate_id FROM projects p \
                      JOIN workspaces w2 ON w2.id = p.workspace_id \
                      WHERE w2.slug = $1 AND p.estimate_id IS NOT NULL \
                      AND p.deleted_at IS NULL) \
         ORDER BY e.name",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for e in &rows {
        // `EstimatePoint` ordering `("value",)` (`db/models/estimate.py:57`).
        let points: Vec<WsPointRow> = sqlx::query_as(
            "SELECT id, created_at, updated_at, created_by_id, estimate_id, project_id, \
             updated_by_id, workspace_id, key AS point_key, description, value, deleted_at \
             FROM estimate_points WHERE estimate_id = $1 AND deleted_at IS NULL \
             ORDER BY value",
        )
        .bind(e.id)
        .fetch_all(&st.pool)
        .await?;
        out.push(ws_estimate_json(e, &points));
    }
    Ok((StatusCode::OK, Json(json!(out))))
}

// ============================================================================
// Global slug-check (`base.py:215-224`). IsAuthenticated only, no ws gate.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SlugCheckQuery {
    #[serde(default)]
    pub slug: Option<String>,
}

pub async fn slug_check(
    State(st): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<SlugCheckQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let slug = match guard_slug_present(q.slug.as_deref()) {
        Ok(s) => s,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    // `Workspace.objects.filter(slug=slug).exists()` is Django-exact;
    // the locked contract pins iexact (`lower() = lower()` — LIKE is
    // avoided because `_`/`%` are wildcards). Soft-deleted rows are
    // excluded via the default manager. Restricted check is Django-exact
    // (`slug in RESTRICTED_WORKSPACE_SLUGS`, `base.py:224`).
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspaces \
         WHERE lower(slug) = lower($1) AND deleted_at IS NULL)",
    )
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"status": slug_available(exists, &slug)}))))
}

// ============================================================================
// Unsplash (`external/base.py:215-243`). IsAuthenticated only.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UnsplashQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub page: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
}

pub async fn unsplash(
    State(_st): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<UnsplashQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Key via configuration value falling back to env
    // (`external/base.py:216-228` → `os.environ.get("UNSPLASH_ACCESS_KEY")`);
    // Rust reads env directly (same as `instance.rs:env_str_or`).
    let key = std::env::var("UNSPLASH_ACCESS_KEY").unwrap_or_default();
    // No key configured → 200 `[]` (`external/base.py:230-231`).
    if key.trim().is_empty() {
        return Ok((StatusCode::OK, Json(json!([]))));
    }
    let page = q.page.as_deref().unwrap_or("1");
    let per_page = q.per_page.as_deref().unwrap_or("20");
    let url = unsplash_url(q.query.as_deref(), page, per_page, key.trim());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()));
    // Upstream unreachable → sane 502 (Django would 500 on the raised
    // `requests` exception; locked Django-500 rule).
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            return Ok((
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Upstream request failed: {e}")})),
            ));
        }
    };
    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    // Passthrough `resp.json()` + upstream status (`external/base.py:242-243`);
    // non-JSON upstream → sane 502 (Django would 500 decoding it).
    match resp.json::<Value>().await {
        Ok(v) => Ok((status, Json(v))),
        Err(_) => Ok((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "Upstream response is not valid JSON"})),
        )),
    }
}

// ============================================================================
// Last-visited workspace (`user.py:68-95`). GET-only (the id is written
// elsewhere — NO PATCH here).
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsDetailRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    name: String,
    logo: Option<String>,
    slug: String,
    owner_id: uuid::Uuid,
    organization_size: Option<String>,
    logo_asset_id: Option<uuid::Uuid>,
    timezone: String,
    background_color: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PmDetailRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    member_id: Option<uuid::Uuid>,
    comment: Option<String>,
    role: i16,
    project_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    view_props: Value,
    default_props: Value,
    sort_order: f64,
    preferences: Value,
    is_active: bool,
    ws_name: String,
    ws_slug: String,
    ws_logo: Option<String>,
    proj_identifier: String,
    proj_name: String,
    proj_cover: Option<String>,
    proj_logo_props: Value,
    proj_description: String,
    u_first: Option<String>,
    u_last: Option<String>,
    u_avatar: Option<String>,
    u_is_bot: Option<bool>,
    u_display: Option<String>,
}

/// Serializes like `WorkSpaceSerializer` (`serializers/workspace.py:43-83`,
/// `fields = "__all__"`): annotation-only keys (`total_members`, `role`)
/// are OMITTED — Django's `WorkSpaceSerializer(workspace)` without queryset
/// annotations skips them (DRF `SkipField` on missing read-only attrs);
/// `logo_url` resolves to the raw `logo` column (the `FileAsset.asset_url`
/// property needs storage signing — documented fallback, null when unset).
fn ws_detail_json(w: &WsDetailRow) -> Value {
    json!({
        "id": w.id, "created_at": w.created_at, "updated_at": w.updated_at,
        "created_by": opt_id(&w.created_by_id), "updated_by": opt_id(&w.updated_by_id),
        "deleted_at": w.deleted_at, "name": w.name,
        "logo": w.logo.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
        "slug": w.slug, "owner": w.owner_id,
        "organization_size": w.organization_size.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
        "logo_asset": opt_id(&w.logo_asset_id), "timezone": w.timezone,
        "background_color": w.background_color,
        "logo_url": w.logo.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
    })
}

/// Serializes like `ProjectMemberSerializer`
/// (`serializers/project.py:156-163`, `__all__` + nested
/// `WorkspaceLiteSerializer`/`ProjectLiteSerializer`/`UserLiteSerializer`
/// lite shapes, `workspace.py:86-90`, `project.py:100-111`,
/// `user.py:141-153`; asset-resolved `*_url` keys fall back to null/raw
/// columns as above). The declared nested fields OVERRIDE the raw FK ids,
/// so the nested objects live under exactly `workspace`/`project`/`member`
/// (no `*_detail` keys, no separate id keys).
fn pm_detail_json(p: &PmDetailRow) -> Value {
    json!({
        "id": p.id, "created_at": p.created_at, "updated_at": p.updated_at,
        "created_by": opt_id(&p.created_by_id), "updated_by": opt_id(&p.updated_by_id),
        "deleted_at": p.deleted_at,
        "comment": p.comment.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
        "role": p.role,
        "view_props": p.view_props, "default_props": p.default_props,
        "sort_order": p.sort_order, "preferences": p.preferences,
        "is_active": p.is_active,
        "workspace": json!({
            "name": p.ws_name, "slug": p.ws_slug, "id": p.workspace_id,
            "logo_url": p.ws_logo.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
        }),
        "project": json!({
            "id": p.project_id, "identifier": p.proj_identifier, "name": p.proj_name,
            "cover_image": p.proj_cover.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
            "cover_image_url": Value::Null, "logo_props": p.proj_logo_props,
            "description": p.proj_description,
        }),
        "member": json!({
            "id": p.member_id.map(|u| json!(u)).unwrap_or(Value::Null),
            "first_name": p.u_first.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
            "last_name": p.u_last.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
            "avatar": p.u_avatar.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
            "avatar_url": Value::Null,
            "is_bot": p.u_is_bot.unwrap_or(false),
            "display_name": p.u_display.as_ref().map(|s| json!(s)).unwrap_or(Value::Null),
        }),
    })
}

pub async fn last_visited(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `user.last_workspace_id` (`user.py:72`); None → 200 null-shape
    // (`user.py:74-79`). The live dev DB predates the column — an
    // `UndefinedColumn` (42703) degrades to the same null-shape instead
    // of 500 (locked Django-500 rule, documented).
    let last_ws: Result<Option<Option<uuid::Uuid>>, sqlx::Error> = sqlx::query_scalar(
        "SELECT last_workspace_id FROM users WHERE id = $1",
    )
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await;
    let last_ws: Option<uuid::Uuid> = match last_ws {
        Ok(v) => v.flatten(),
        Err(e) => {
            let undefined_column =
                matches!(&e, sqlx::Error::Database(db) if db.code().as_deref() == Some("42703"));
            if undefined_column {
                None
            } else {
                return Err(e.into());
            }
        }
    };
    let Some(ws_id) = last_ws else {
        return Ok((StatusCode::OK, Json(last_visited_null())));
    };
    // `Workspace.objects.get(pk=...)` — miss (stale id) → sane 404
    // (Django raises → 500; locked).
    let ws: Option<WsDetailRow> = sqlx::query_as(
        "SELECT id, created_at, updated_at, created_by_id, updated_by_id, deleted_at, \
         name, logo, slug, owner_id, organization_size, logo_asset_id, timezone, \
         background_color FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(ws_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(ws) = ws else {
        return Ok(missing());
    };
    // `ProjectMember.objects.filter(workspace_id=..., member=user)` with
    // the lite nests (`user.py:85-88`).
    let pms: Vec<PmDetailRow> = sqlx::query_as(
        "SELECT pm.id, pm.created_at, pm.updated_at, pm.created_by_id, pm.updated_by_id, \
         pm.deleted_at, pm.member_id, pm.comment, pm.role, pm.project_id, pm.workspace_id, \
         pm.view_props, pm.default_props, pm.sort_order, pm.preferences, pm.is_active, \
          w.name AS ws_name, w.slug AS ws_slug, w.logo AS ws_logo, \
         p.identifier AS proj_identifier, p.name AS proj_name, p.cover_image AS proj_cover, \
         p.logo_props AS proj_logo_props, p.description AS proj_description, \
         u.first_name AS u_first, u.last_name AS u_last, u.avatar AS u_avatar, \
         u.is_bot AS u_is_bot, u.display_name AS u_display \
         FROM project_members pm JOIN workspaces w ON w.id = pm.workspace_id \
         JOIN projects p ON p.id = pm.project_id \
         LEFT JOIN users u ON u.id = pm.member_id \
         WHERE pm.workspace_id = $1 AND pm.member_id = $2 AND pm.deleted_at IS NULL",
    )
    .bind(ws_id)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "workspace_details": ws_detail_json(&ws),
            "project_details": pms.iter().map(pm_detail_json).collect::<Vec<_>>(),
        })),
    ))
}

// ============================================================================
// Unit tests (STEP 1 failing → green; pure helpers only, no DB).
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_defaults_cover_all_keys_with_pins_and_steps() {
        // `user_preference.py:37-63`, `workspace.py:417-427`.
        let d = sidebar_defaults();
        let keys: Vec<&str> = d.iter().map(|(k, _, _)| k.as_str()).collect();
        assert_eq!(keys, SIDEBAR_KEYS);
        for (k, pinned, _) in &d {
            assert_eq!(*pinned, sidebar_pinned(k));
        }
        let orders: Vec<f64> = d.iter().map(|(_, _, s)| *s).collect();
        assert_eq!(orders, vec![65535.0, 75535.0, 85535.0, 95535.0, 105535.0, 115535.0, 125535.0]);
    }

    #[test]
    fn sidebar_sort_index_counts_missing_only() {
        // Pre-existing keys must NOT consume an index (loop-append quirk).
        assert_eq!(sidebar_sort_order(0), 65535.0);
        assert_eq!(sidebar_sort_order(2), 85535.0);
    }

    #[test]
    fn home_keys_exclude_tutorial_and_new() {
        // `home.py:32-36`: minus quick_tutorial/new_at_plane.
        assert_eq!(HOME_KEYS, &["quick_links", "recents", "my_stickies"]);
        assert_eq!(home_sort_order(1), 999.0);
        assert_eq!(home_sort_order(3), 997.0);
    }

    #[test]
    fn home_miss_is_400_detail_not_404() {
        // `home.py:79` verbatim — NOT a 404.
        let (status, Json(v)) = home_patch_miss();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(v, json!({"detail": "Preference not found"}));
    }

    #[test]
    fn quick_link_misses_preserve_both_verbatims() {
        // PATCH `quick_link.py:52` (detail) vs GET `quick_link.py:60` (error).
        let (s1, Json(v1)) = quick_patch_miss();
        let (s2, Json(v2)) = quick_detail_miss();
        assert_eq!(s1, StatusCode::NOT_FOUND);
        assert_eq!(s2, StatusCode::NOT_FOUND);
        assert_eq!(v1, json!({"detail": "Quick link not found."}));
        assert_eq!(v2, json!({"error": "Quick link not found."}));
        assert_ne!(v1, v2);
    }

    #[test]
    fn quick_link_url_prepend_and_validate() {
        // `serializers/workspace.py:192-206`.
        assert_eq!(normalize_link_url("example.com/x"), "http://example.com/x");
        assert_eq!(normalize_link_url("https://a.b"), "https://a.b");
        assert!(validate_link_url("http://example.com").is_ok());
        assert!(validate_link_url("notaurl").is_err());
        assert_eq!(
            validate_link_url("notaurl").unwrap_err(),
            "Invalid URL format."
        );
    }

    #[test]
    fn slug_check_guards_and_status() {
        // `base.py:218`: `if not slug or slug == ""` — an EXACT comparison,
        // NO trimming: only a missing param or exactly `""` → 400.
        assert!(guard_slug_present(None).is_err());
        assert!(guard_slug_present(Some("")).is_err());
        assert_eq!(
            guard_slug_present(None).unwrap_err(),
            "Workspace Slug is required"
        );
        assert_eq!(
            guard_slug_present(Some("")).unwrap_err(),
            "Workspace Slug is required"
        );
        // Whitespace-only is truthy in Django → passes through to the 200
        // availability check (likely `{"status": true}`).
        assert_eq!(guard_slug_present(Some("  ")).unwrap(), "  ".to_string());
        assert!(slug_available(false, "fresh-slug"));
        assert!(!slug_available(true, "fresh-slug"));
        assert!(!slug_available(false, "admin"));
        assert!(!slug_available(false, "chat"));
    }

    #[test]
    fn unsplash_url_fixes_dollar_page_bug() {
        // `external/base.py:232-236`: Django emits `&page=${page}`.
        let u = unsplash_url(Some("cats"), "2", "20", "K");
        assert!(u.contains("&page=2&"), "{u}");
        assert!(!u.contains("${"), "{u}");
        let u2 = unsplash_url(None, "1", "20", "K");
        assert!(u2.contains("/photos/?"), "{u2}");
        let u3 = unsplash_url(Some(""), "1", "20", "K");
        assert!(u3.contains("/photos/?"), "{u3}");
    }

    #[test]
    fn recent_filter_forces_allowlist() {
        // `recent_visit.py:25-33`.
        assert!(recent_row_visible(None, "issue"));
        assert!(recent_row_visible(Some("page"), "page"));
        assert!(!recent_row_visible(Some("page"), "issue"));
        assert!(!recent_row_visible(Some("cycle"), "cycle"));
        assert!(!recent_row_visible(None, "cycle"));
    }

    #[test]
    fn last_visited_null_shape() {
        // `user.py:74-79`.
        assert_eq!(
            last_visited_null(),
            json!({"project_details": [], "workspace_details": {}})
        );
    }

    #[test]
    fn amg_gate_roles() {
        assert!(guard_amg(Some(20)).is_ok());
        assert!(guard_amg(Some(15)).is_ok());
        assert!(guard_amg(Some(5)).is_ok());
        assert!(guard_amg(Some(10)).is_err());
        assert!(guard_amg(None).is_err());
    }

    #[test]
    fn pm_detail_uses_django_serializer_keys() {
        // `serializers/project.py:156-163`: `ProjectMemberSerializer` nests
        // as `workspace`/`project`/`member` (`fields = "__all__"`, so every
        // model column plus exactly those three nested keys).
        let row = PmDetailRow {
            id: uuid::Uuid::nil(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by_id: None,
            updated_by_id: None,
            deleted_at: None,
            member_id: Some(uuid::Uuid::nil()),
            comment: None,
            role: 15,
            project_id: uuid::Uuid::nil(),
            workspace_id: uuid::Uuid::nil(),
            view_props: json!({}),
            default_props: json!({}),
            sort_order: 1.0,
            preferences: json!({}),
            is_active: true,
            ws_name: "WS".to_string(),
            ws_slug: "ws-slug".to_string(),
            ws_logo: None,
            proj_identifier: "P".to_string(),
            proj_name: "Proj".to_string(),
            proj_cover: None,
            proj_logo_props: json!({}),
            proj_description: "d".to_string(),
            u_first: Some("A".to_string()),
            u_last: None,
            u_avatar: None,
            u_is_bot: Some(false),
            u_display: Some("A".to_string()),
        };
        let v = pm_detail_json(&row);
        let keys: std::collections::BTreeSet<&str> = v
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let expected: std::collections::BTreeSet<&str> = [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "member",
            "comment",
            "role",
            "project",
            "workspace",
            "view_props",
            "default_props",
            "sort_order",
            "preferences",
            "is_active",
        ]
        .into_iter()
        .collect();
        assert_eq!(keys, expected);
        assert!(v.get("workspace_detail").is_none());
        assert!(v.get("project_detail").is_none());
        assert!(v.get("member_detail").is_none());
        // Inner lite shapes stay intact under the renamed keys.
        assert_eq!(v["workspace"]["slug"], json!("ws-slug"));
        assert_eq!(v["project"]["identifier"], json!("P"));
        assert_eq!(v["member"]["first_name"], json!("A"));
    }

    #[test]
    fn home_patch_rejects_mistyped_fields() {
        // `home.py:74-77` partial-serializer errors: DRF `BooleanField`
        // (`rest_framework/fields.py`, `invalid: "Must be a valid boolean."`)
        // and DRF `FloatField` (`invalid: "A valid number is required."`).
        assert_eq!(
            home_bool_opt(Some(&json!("yes")), true),
            Err("Must be a valid boolean.".to_string())
        );
        assert_eq!(
            home_order_opt(Some(&json!("abc")), 1.0),
            Err("A valid number is required.".to_string())
        );
        // Serializer-error body shape per field.
        assert_eq!(
            json!({"is_enabled": [home_bool_opt(Some(&json!("yes")), true).unwrap_err()]}),
            json!({"is_enabled": ["Must be a valid boolean."]})
        );
        assert_eq!(
            json!({"sort_order": [home_order_opt(Some(&json!("abc")), 1.0).unwrap_err()]}),
            json!({"sort_order": ["A valid number is required."]})
        );
        // Absent keeps current; valid values pass through unchanged.
        assert_eq!(home_bool_opt(None, true), Ok(true));
        assert_eq!(home_bool_opt(Some(&json!(false)), true), Ok(false));
        assert_eq!(home_order_opt(None, 1.0), Ok(1.0));
        assert_eq!(home_order_opt(Some(&json!(2.5)), 1.0), Ok(2.5));
    }

    #[test]
    fn nav_pref_bad_choice_uses_drf_message() {
        // DRF `ChoiceField` (`rest_framework/fields.py`,
        // `invalid_choice: '"{input}" is not a valid choice.'`) with the
        // submitted value interpolated.
        assert_eq!(
            nav_pref_error(&json!("FOO")),
            Some("\"FOO\" is not a valid choice.".to_string())
        );
        assert_eq!(nav_pref_error(&json!("ACCORDION")), None);
        assert_eq!(nav_pref_error(&json!("TABBED")), None);
    }
}
