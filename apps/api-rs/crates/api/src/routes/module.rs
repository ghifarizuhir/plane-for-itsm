use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::cycle::{burndown_chart, extract_date_part, format_archived_at, parse_point_value};
use crate::routes::project::{deny, missing, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{
    DetailEnvelope, PageWindow, detail_order_expr, fetch_project_member_role, is_workspace_admin,
    next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str,
    project_gate_allows, sanitize_order_by, total_pages,
};

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/serializers/module.py:72` (create) and `:100` (update,
/// excluding self): duplicate module name within the project.
pub const DUP_NAME_MSG: &str = "Module with this name already exists";
/// `plane/app/views/module/base.py:415` (retrieve) and `:658-661`
/// (partial_update): detail miss — verbatim (NOT `missing()`).
pub const MODULE_NOT_FOUND_MSG: &str = "Module not found";
/// `plane/app/views/module/base.py:663-667` (partial_update on archived).
pub const ARCHIVED_IMMUTABLE_MSG: &str = "Archived module cannot be updated";
/// `plane/app/serializers/module.py:61` (`ModuleWriteSerializer.validate`).
pub const START_EXCEEDS_TARGET_MSG: &str = "Start date cannot exceed target date";
/// `plane/app/views/module/issue.py:213-214` (create_module_issues, empty).
pub const ISSUES_REQUIRED_MSG: &str = "Issues are required";
/// `plane/app/views/module/issue.py:129-133` (group_by == sub_group_by).
pub const GROUP_DUP_MSG: &str = "Group by and sub group by cannot have same parameters";
/// `plane/app/views/module/archive.py:546-550` (archive wrong status).
pub const ARCHIVE_STATUS_MSG: &str = "Only completed or cancelled modules can be archived";
/// `plane/app/serializers/module.py:184` (`validate_url`).
pub const INVALID_URL_MSG: &str = "Invalid URL format.";
/// `plane/app/serializers/module.py:191` (link create dup).
pub const URL_EXISTS_MSG: &str = "URL already exists.";
/// `plane/app/serializers/module.py:201` (link update dup — sic "Issue",
/// verbatim; the module link serializer reuses the issue-link message).
pub const URL_EXISTS_ISSUE_MSG: &str = "URL already exists for this Issue";
/// `plane/app/views/base.py:92-97` (Django `IntegrityError` → 400; favorite dup).
pub const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";
/// Locked validation body.
pub const VALID_DETAIL_MSG: &str = "Please provide valid detail";
/// DRF permission-class deny body (links / favorites / archive / workspace).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Mirrors `Module.save` status default + `ModuleStatus`
/// (`plane/db/models/module.py:58-85`): absent → `planned`; unknown → Err.
pub fn normalize_status(raw: Option<&str>) -> Result<String, String> {
    const ALLOWED: &[&str] = &["backlog", "planned", "in-progress", "paused", "completed", "cancelled"];
    match raw {
        None => Ok("planned".to_string()),
        Some(s) if ALLOWED.contains(&s) => Ok(s.to_string()),
        Some(s) => Err(format!("\"{s}\" is not a valid choice.")),
    }
}

/// Mirrors `ModuleWriteSerializer.validate`
/// (`plane/app/serializers/module.py:55-62`).
pub fn guard_date_order(start: Option<NaiveDate>, target: Option<NaiveDate>) -> Result<(), String> {
    if let (Some(s), Some(t)) = (start, target) {
        if s > t {
            return Err(START_EXCEEDS_TARGET_MSG.to_string());
        }
    }
    Ok(())
}

/// Mirrors the name pre-check (`serializers/module.py:68-72,97-100`).
pub fn guard_dup_name(exists: bool) -> Result<(), String> {
    if exists {
        return Err(DUP_NAME_MSG.to_string());
    }
    Ok(())
}

/// Mirrors `ModuleViewSet.partial_update`
/// (`plane/app/views/module/base.py:663-667`).
pub fn guard_patch(archived: bool) -> Result<(), String> {
    if archived {
        return Err(ARCHIVED_IMMUTABLE_MSG.to_string());
    }
    Ok(())
}

/// Mirrors `ModuleArchiveUnarchiveEndpoint.post`
/// (`plane/app/views/module/archive.py:546-550`).
pub fn guard_archive_status(status: &str) -> Result<(), String> {
    if status == "completed" || status == "cancelled" {
        Ok(())
    } else {
        Err(ARCHIVE_STATUS_MSG.to_string())
    }
}

/// Mirrors the empty-issues gate (`plane/app/views/module/issue.py:213-214`).
pub fn guard_issues_present(n: usize) -> Result<(), String> {
    if n == 0 {
        return Err(ISSUES_REQUIRED_MSG.to_string());
    }
    Ok(())
}

/// PROJECT-level role guards mirroring `@allow_permission` role lists:
/// AMG (`module/base.py:353`, user-properties `:846`) vs AM
/// (`module/base.py:294,395,651`, `issue.py:95,209,256,325`).
pub fn guard_amg(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` — GUEST denied.
pub fn guard_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `ProjectEntityPermission` SAFE branch
/// (`plane/app/permissions/project.py:101-110`): any active project member,
/// roles unchecked. Strict — no workspace-ADMIN fallback (the permission
/// class has none, unlike `allow_permission`).
pub fn guard_safe_member(has_membership: bool) -> Result<(), String> {
    if has_membership {
        Ok(())
    } else {
        Err(PERMISSION_DETAIL_MSG.to_string())
    }
}

/// Mirrors `ProjectEntityPermission` unsafe branch
/// (`plane/app/permissions/project.py:112-119`): ADMIN/MEMBER only, strict.
pub fn guard_entity_write(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(PERMISSION_DETAIL_MSG.to_string()),
    }
}

/// Mirrors `ProjectLitePermission`
/// (`plane/app/permissions/project.py:136-148`): any active member, strict.
pub fn guard_lite(has_membership: bool) -> Result<(), String> {
    if has_membership {
        Ok(())
    } else {
        Err(PERMISSION_DETAIL_MSG.to_string())
    }
}

/// Mirrors `ModuleLinkSerializer.to_internal_value`
/// (`plane/app/serializers/module.py:170-176`): prepend `http://` when the
/// url has no scheme.
pub fn normalize_link_url(raw: &str) -> String {
    let t = raw.trim().to_string();
    if t.is_empty() || t.starts_with("http://") || t.starts_with("https://") {
        t
    } else {
        format!("http://{t}")
    }
}

/// Mirrors `validate_url` (`serializers/module.py:178-186`, Django
/// `URLValidator`): absolute http(s) URL with a non-empty host (dot or
/// `localhost`; no whitespace). Hand-rolled: the pinned `validator`
/// crate (0.18) no longer re-exports a `validate_url` helper.
pub fn valid_link_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#', ':'].as_ref())
        .next()
        .unwrap_or("");
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return false;
    }
    host.contains('.') || host.eq_ignore_ascii_case("localhost")
}

/// Parses a date-ish body value leniently (date-only, RFC3339, naive
/// datetime — time discarded, mirroring Django `DateField` which keeps the
/// date part). Unparseable → None (caller 400s `VALID_DETAIL_MSG`).
pub fn parse_body_date(raw: &str) -> Option<NaiveDate> {
    let t = raw.trim();
    if let Some(part) = extract_date_part(t) {
        if let Ok(d) = NaiveDate::parse_from_str(&part, "%Y-%m-%d") {
            return Some(d);
        }
    }
    None
}

/// Mirrors `DynamicBaseSerializer(fields=...)` (`?fields=` projection on
/// list, `module/base.py:356-357`): keep only requested keys that exist.
pub fn project_fields(mut v: Value, fields: &[String]) -> Value {
    if fields.is_empty() {
        return v;
    }
    let Some(obj) = v.as_object_mut() else {
        return v;
    };
    let keep: std::collections::HashSet<&str> = fields.iter().map(String::as_str).collect();
    let keys: Vec<String> = obj.keys().cloned().collect();
    for k in keys {
        if !keep.contains(k.as_str()) {
            obj.remove(&k);
        }
    }
    v
}

/// Parses a `?fields=a,b` query value.
pub fn parse_fields(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Pre-existing validate shape (required by
/// `crates/api/tests/module_state_test.rs`; CONSTRAINTS forbid touching that file).
/// `#[allow(dead_code)]`: the Axum handlers take `Json<Value>` bodies
/// (Django reads `request.data` dynamically), so these typed helpers are
/// construction points for tests only — the binary target would otherwise
/// lint them as unused (E2 `cycle.rs:CreateCycle` precedent).
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CreateModule {
    pub name: String,
    pub start_date: Option<NaiveDate>,
    pub target_date: Option<NaiveDate>,
}

#[allow(dead_code)]
pub fn validate_create(body: &CreateModule) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    guard_date_order(body.start_date, body.target_date)?;
    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchModule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start_date: Option<NaiveDate>,
    #[serde(default)]
    pub target_date: Option<NaiveDate>,
}

// ============================================================================
// Row structs + JSON builders.
// ============================================================================

/// Tolerant numeric guard for `estimate_points.value` (varchar): only
/// strings matching a plain decimal/scientific shape are cast — never a
/// bare `::double precision` on varchar (locked constraint; Django
/// `Cast(value, FloatField)` would 500 on garbage instead).
const PT_SUM: &str = "COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0)";

fn points_issue_join(group_filter: &str) -> String {
    format!(
        "FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL \
         AND i.deleted_at IS NULL \
         JOIN states s ON s.id = i.state_id \
         JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
         JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL \
         WHERE mi.module_id = m.id {group_filter}"
    )
}

fn count_sub(alias: &str, group_filter: &str) -> String {
    format!(
        "(SELECT COUNT(*) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          AND mi.deleted_at IS NULL AND i.deleted_at IS NULL \
          JOIN states s ON s.id = i.state_id \
          WHERE mi.module_id = m.id {group_filter}) AS {alias}"
    )
}

fn points_sub(alias: &str, group_filter: &str) -> String {
    format!("(SELECT {PT_SUM} {} ) AS {alias}", points_issue_join(group_filter))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ModuleRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    name: String,
    description: String,
    description_text: Option<Value>,
    description_html: Option<Value>,
    start_date: Option<NaiveDate>,
    target_date: Option<NaiveDate>,
    status: String,
    lead_id: Option<uuid::Uuid>,
    member_ids: Vec<uuid::Uuid>,
    view_props: Value,
    sort_order: f64,
    external_source: Option<String>,
    external_id: Option<String>,
    logo_props: Value,
    completed_estimate_points: f64,
    total_estimate_points: f64,
    total_issues: i64,
    is_favorite: bool,
    cancelled_issues: i64,
    completed_issues: i64,
    started_issues: i64,
    unstarted_issues: i64,
    backlog_issues: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn opt_uuid(u: &Option<uuid::Uuid>) -> Value {
    u.map(|v| json!(v)).unwrap_or(Value::Null)
}

fn opt_str(s: &Option<String>) -> Value {
    s.as_ref().map(|v| json!(v)).unwrap_or(Value::Null)
}

/// List-shape JSON in the exact `.values()` key order
/// (`plane/app/views/module/base.py:359-390`).
fn module_list_json(r: &ModuleRow) -> Value {
    json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "name": r.name,
        "description": r.description,
        "description_text": r.description_text.clone().unwrap_or(Value::Null),
        "description_html": r.description_html.clone().unwrap_or(Value::Null),
        "start_date": r.start_date,
        "target_date": r.target_date,
        "status": r.status,
        "lead_id": opt_uuid(&r.lead_id),
        "member_ids": r.member_ids,
        "view_props": r.view_props,
        "sort_order": r.sort_order,
        "external_source": opt_str(&r.external_source),
        "external_id": opt_str(&r.external_id),
        "logo_props": r.logo_props,
        "completed_estimate_points": r.completed_estimate_points,
        "total_estimate_points": r.total_estimate_points,
        "total_issues": r.total_issues,
        "is_favorite": r.is_favorite,
        "cancelled_issues": r.cancelled_issues,
        "completed_issues": r.completed_issues,
        "started_issues": r.started_issues,
        "unstarted_issues": r.unstarted_issues,
        "backlog_issues": r.backlog_issues,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
    })
}

/// Archived-list shape (`archive.py:261-290`): list keys MINUS
/// `logo_props` and both estimate-point keys, PLUS `archived_at`.
fn module_archived_json(r: &ModuleRow) -> Value {
    json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "name": r.name,
        "description": r.description,
        "description_text": r.description_text.clone().unwrap_or(Value::Null),
        "description_html": r.description_html.clone().unwrap_or(Value::Null),
        "start_date": r.start_date,
        "target_date": r.target_date,
        "status": r.status,
        "lead_id": opt_uuid(&r.lead_id),
        "member_ids": r.member_ids,
        "view_props": r.view_props,
        "sort_order": r.sort_order,
        "external_source": opt_str(&r.external_source),
        "external_id": opt_str(&r.external_id),
        "total_issues": r.total_issues,
        "is_favorite": r.is_favorite,
        "cancelled_issues": r.cancelled_issues,
        "completed_issues": r.completed_issues,
        "started_issues": r.started_issues,
        "unstarted_issues": r.unstarted_issues,
        "backlog_issues": r.backlog_issues,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "archived_at": r.archived_at,
    })
}

fn module_select(archived_only: bool) -> String {
    let arch = if archived_only {
        "AND m.archived_at IS NOT NULL"
    } else {
        "AND m.archived_at IS NULL"
    };
    format!(
        "SELECT m.id, m.workspace_id, m.project_id, m.name, m.description, \
         m.description_text, m.description_html, m.start_date, m.target_date, m.status, \
         m.lead_id, \
         COALESCE(ARRAY(SELECT DISTINCT mm.member_id FROM module_members mm \
            WHERE mm.module_id = m.id AND mm.deleted_at IS NULL), '{{}}') AS member_ids, \
         m.view_props, m.sort_order, m.external_source, m.external_id, m.logo_props, \
         {completed_pt} AS completed_estimate_points, \
         {total_pt} AS total_estimate_points, \
         {total} AS total_issues, \
         EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'module' \
            AND uf.entity_identifier = m.id AND uf.user_id = $3 AND uf.project_id = m.project_id \
            AND uf.deleted_at IS NULL) AS is_favorite, \
         {cancelled} AS cancelled_issues, \
         {completed} AS completed_issues, \
         {started} AS started_issues, \
         {unstarted} AS unstarted_issues, \
         {backlog} AS backlog_issues, \
         m.created_at, m.updated_at, m.archived_at \
         FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.project_id = $1 AND w.slug = $2 AND m.deleted_at IS NULL {arch}",
        completed_pt = points_sub("x", "AND s.\"group\" = 'completed'"),
        total_pt = points_sub("x", ""),
        total = count_sub("x", ""),
        cancelled = count_sub("x", "AND s.\"group\" = 'cancelled'"),
        completed = count_sub("x", "AND s.\"group\" = 'completed'"),
        started = count_sub("x", "AND s.\"group\" = 'started'"),
        unstarted = count_sub("x", "AND s.\"group\" = 'unstarted'"),
        backlog = count_sub("x", "AND s.\"group\" = 'backlog'"),
    )
}

// ============================================================================
// Shared gates + lookups.
// ============================================================================

async fn project_in_workspace(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(slug)
    .fetch_one(pool)
    .await
}

/// Gate for AMG endpoints (`allow_permission`, `permissions/base.py:53-78`):
/// allowed roles 20/15/5 outright, else the workspace-ADMIN fallback.
async fn gate_amg(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        guard_amg(role).is_ok(),
        role.is_some(),
        ws_admin,
    ))
}

/// Gate for AM endpoints: same fallback, narrower role list.
async fn gate_am(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        guard_am(role).is_ok(),
        role.is_some(),
        ws_admin,
    ))
}

/// Gate for `ProjectEntityPermission` SAFE endpoints (links/archive GETs):
/// any active project member, strict (no ws-admin fallback —
/// `permissions/project.py:101-110` has none).
async fn gate_safe(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    Ok(guard_safe_member(role.is_some()).is_ok())
}

/// Gate for `ProjectEntityPermission` unsafe endpoints (links write,
/// archive POST/DELETE): ADMIN/MEMBER, strict
/// (`permissions/project.py:112-119`).
async fn gate_entity_write(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    Ok(guard_entity_write(role).is_ok())
}

/// Gate for `ProjectLitePermission` (favorites,
/// `permissions/project.py:136-148`): any active member, strict.
async fn gate_lite(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    Ok(guard_lite(role.is_some()).is_ok())
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

/// DRF permission-class deny body (`WorkspaceViewerPermission`, E3g;
/// `ProjectEntityPermission`/`ProjectLitePermission`, E3c/E3d/E3f) —
/// same shape as the E2 `cycle::deny_detail`.
fn deny_detail() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": PERMISSION_DETAIL_MSG})),
    )
}

async fn project_has_points_estimate(pool: &sqlx::PgPool, pid: uuid::Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM estimates WHERE project_id = $1 \
         AND type = 'points' AND deleted_at IS NULL)",
    )
    .bind(pid)
    .fetch_one(pool)
    .await
}

async fn fetch_row(
    pool: &sqlx::PgPool,
    mid: uuid::Uuid,
    pid: uuid::Uuid,
    slug: &str,
    user: uuid::Uuid,
    archived_only: bool,
) -> Result<Option<ModuleRow>, sqlx::Error> {
    let select = module_select(archived_only);
    let sql = format!("{select} AND m.id = $4 ORDER BY m.created_at DESC");
    sqlx::query_as::<_, ModuleRow>(&sql)
        .bind(pid)
        .bind(slug)
        .bind(user)
        .bind(mid)
        .fetch_optional(pool)
        .await
}

/// Parses UUID list body keys (`member_ids`, Django
/// `PrimaryKeyRelatedField(many=True)` — invalid entries 400).
fn parse_uuid_list(body: &Value, key: &str) -> Result<Vec<uuid::Uuid>, ()> {
    let Some(arr) = body.get(key).and_then(Value::as_array) else {
        return Ok(vec![]);
    };
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let s = v.as_str().ok_or(())?;
        out.push(s.parse().map_err(|_| ())?);
    }
    Ok(out)
}

// ============================================================================
// E3a — list + create + detail + update + destroy.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModuleListQuery {
    #[serde(default)]
    pub fields: Option<String>,
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<ModuleListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `list` (`base.py:353`): AMG.
    if !gate_amg(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // Django `list` (`base.py:353-393`): GET 200 array ordered
    // `-is_favorite,-created_at`, unarchived only, with `?fields=`
    // projection (`base.py:356-357`).
    let select = module_select(false);
    let sql = format!("{select} ORDER BY is_favorite DESC, m.created_at DESC");
    let rows: Vec<ModuleRow> = sqlx::query_as(&sql)
        .bind(pid)
        .bind(&slug)
        .bind(auth.0)
        .fetch_all(&st.pool)
        .await?;
    let fields = parse_fields(q.fields.as_deref());
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|r| project_fields(module_list_json(r), &fields))
            .collect::<Vec<_>>())),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `create` (`base.py:294`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let name = body.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    if body.get("name").is_none() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"name": ["This field is required."]})),
        ));
    }
    if name.trim().is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"name": ["This field may not be blank."]})),
        ));
    }
    if name.chars().count() > 255 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"name": ["Ensure this field has no more than 255 characters."]})),
        ));
    }
    let start = match body.get("start_date").and_then(Value::as_str) {
        None | Some("") => None,
        Some(s) => match parse_body_date(s) {
            Some(d) => Some(d),
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        },
    };
    let target = match body.get("target_date").and_then(Value::as_str) {
        None | Some("") => None,
        Some(s) => match parse_body_date(s) {
            Some(d) => Some(d),
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        },
    };
    // Serializer order check (`serializers/module.py:55-62`) → DRF
    // `non_field_errors` shape.
    if guard_date_order(start, target).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"non_field_errors": [START_EXCEEDS_TARGET_MSG]})),
        ));
    }
    let status = match normalize_status(body.get("status").and_then(Value::as_str)) {
        Ok(s) => s,
        Err(e) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"status": [e]}))));
        }
    };
    // Dup-name pre-check (`serializers/module.py:68-72`).
    let dup: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM modules WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(&name)
    .fetch_one(&st.pool)
    .await?;
    if guard_dup_name(dup.0).is_err() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": DUP_NAME_MSG}))));
    }
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let lead_id: Option<uuid::Uuid> = body
        .get("lead_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    let members: Vec<uuid::Uuid> = match parse_uuid_list(&body, "member_ids") {
        Ok(v) => v,
        Err(()) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    // Django `Module.save` (`db/models/module.py:115-124`): sort_order =
    // min-10000 per project; `status` default `planned` (`:74-85`).
    // Insert + member bulk-create share one tx.
    let mut tx = st.pool.begin().await?;
    let mid: Option<(uuid::Uuid,)> = sqlx::query_as(
        "INSERT INTO modules (id, name, description, status, lead_id, project_id, workspace_id, \
         created_by_id, updated_by_id, view_props, logo_props, sort_order, start_date, target_date, \
         created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, $3, $4, p.id, p.workspace_id, $5, $5, \
         COALESCE($6::jsonb, '{}'::jsonb), '{}', \
         COALESCE((SELECT MIN(sort_order) FROM modules WHERE project_id = p.id), 65535 + 10000) - 10000, \
         $7, $8, now(), now() FROM projects p WHERE p.id = $9 RETURNING id",
    )
    .bind(&name)
    .bind(&description)
    .bind(&status)
    .bind(lead_id)
    .bind(auth.0)
    .bind(body.get("view_props").cloned().unwrap_or(json!({})))
    .bind(start)
    .bind(target)
    .bind(pid)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((mid,)) = mid else {
        return Ok(missing());
    };
    if !members.is_empty() {
        // `serializers/module.py:75-90`: bulk_create ignore_conflicts;
        // silently drop ids that are not live users (sane; Django would
        // 400 via `PrimaryKeyRelatedField` — documented below).
        sqlx::query(
            "INSERT INTO module_members (id, module_id, member_id, project_id, workspace_id, \
             created_by_id, updated_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, u.id, $2, p.workspace_id, $3, $3, now(), now() \
             FROM users u JOIN projects p ON p.id = $2 \
             WHERE u.id = ANY($4) AND u.is_active = true AND u.deleted_at IS NULL \
             ON CONFLICT DO NOTHING",
        )
        .bind(mid)
        .bind(pid)
        .bind(auth.0)
        .bind(&members)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    // Django `create` response (`base.py:302-337`): the list-shape row.
    match fetch_row(&st.pool, mid, pid, &slug, auth.0, false).await? {
        Some(row) => Ok((StatusCode::CREATED, Json(module_list_json(&row)))),
        None => Ok(missing()),
    }
}

// --- detail enrichment (E3a retrieve + E3f archived detail) ---

#[derive(Debug, Clone, sqlx::FromRow)]
struct LinkRow {
    id: uuid::Uuid,
    title: Option<String>,
    url: String,
    metadata: Value,
    module_id: uuid::Uuid,
    project_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn link_json(r: &LinkRow) -> Value {
    json!({
        "id": r.id,
        "title": opt_str(&r.title),
        "url": r.url,
        "metadata": r.metadata,
        "module": r.module_id,
        "module_id": r.module_id,
        "project_id": r.project_id,
        "workspace_id": r.workspace_id,
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
        "created_at": r.created_at,
        "updated_at": r.updated_at,
    })
}

async fn fetch_links(
    pool: &sqlx::PgPool,
    mid: uuid::Uuid,
) -> Result<Vec<LinkRow>, sqlx::Error> {
    sqlx::query_as::<_, LinkRow>(
        "SELECT id, title, url, metadata, module_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at \
         FROM module_links WHERE module_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(mid)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AssigneeDistRow {
    first_name: Option<String>,
    last_name: Option<String>,
    assignee_id: Option<uuid::Uuid>,
    avatar_url: Option<String>,
    display_name: Option<String>,
    total_issues: i64,
    completed_issues: i64,
    pending_issues: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LabelDistRow {
    label_name: Option<String>,
    color: Option<String>,
    label_id: Option<uuid::Uuid>,
    total_issues: i64,
    completed_issues: i64,
    pending_issues: i64,
}

async fn assignee_distribution(
    pool: &sqlx::PgPool,
    mid: uuid::Uuid,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<Value, sqlx::Error> {
    // `module/base.py:538-589` (retrieve) / `:431-491` (archive detail).
    let rows: Vec<AssigneeDistRow> = sqlx::query_as(
        "SELECT u.first_name AS first_name, u.last_name AS last_name, u.id AS assignee_id, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
            THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' \
            ELSE u.avatar END AS avatar_url, \
         u.display_name AS display_name, \
         COUNT(*) FILTER (WHERE i.archived_at IS NULL AND i.is_draft = false) AS total_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NOT NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS completed_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS pending_issues \
         FROM issues i JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
            AND mi.deleted_at IS NULL \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.deleted_at IS NULL \
         JOIN users u ON u.id = ia.assignee_id \
         WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL \
         GROUP BY u.first_name, u.last_name, u.id, u.avatar_asset_id, u.avatar, u.display_name \
         ORDER BY u.first_name, u.last_name",
    )
    .bind(mid)
    .bind(pid)
    .bind(slug)
    .fetch_all(pool)
    .await?;
    Ok(json!(rows
        .iter()
        .map(|r| json!({
            "first_name": r.first_name,
            "last_name": r.last_name,
            "assignee_id": r.assignee_id.map(|u| u.to_string()),
            "avatar_url": r.avatar_url,
            "display_name": r.display_name,
            "total_issues": r.total_issues,
            "completed_issues": r.completed_issues,
            "pending_issues": r.pending_issues,
        }))
        .collect::<Vec<_>>()))
}

async fn label_distribution(
    pool: &sqlx::PgPool,
    mid: uuid::Uuid,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<Value, sqlx::Error> {
    // `module/base.py:591-624` (retrieve) / `:493-526` (archive detail).
    let rows: Vec<LabelDistRow> = sqlx::query_as(
        "SELECT l.name AS label_name, l.color AS color, l.id AS label_id, \
         COUNT(*) FILTER (WHERE i.archived_at IS NULL AND i.is_draft = false) AS total_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NOT NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS completed_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS pending_issues \
         FROM issues i JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
            AND mi.deleted_at IS NULL \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_labels il ON il.issue_id = i.id AND il.deleted_at IS NULL \
         JOIN labels l ON l.id = il.label_id \
         WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL \
         GROUP BY l.name, l.color, l.id ORDER BY l.name",
    )
    .bind(mid)
    .bind(pid)
    .bind(slug)
    .fetch_all(pool)
    .await?;
    Ok(json!(rows
        .iter()
        .map(|r| json!({
            "label_name": r.label_name,
            "color": r.color,
            "label_id": r.label_id.map(|u| u.to_string()),
            "total_issues": r.total_issues,
            "completed_issues": r.completed_issues,
            "pending_issues": r.pending_issues,
        }))
        .collect::<Vec<_>>()))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EstRow {
    first_name: Option<String>,
    last_name: Option<String>,
    assignee_id: Option<uuid::Uuid>,
    avatar_url: Option<String>,
    display_name: Option<String>,
    label_name: Option<String>,
    color: Option<String>,
    label_id: Option<uuid::Uuid>,
    value: Option<String>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    is_draft: bool,
}

async fn estimate_distribution(
    pool: &sqlx::PgPool,
    mid: uuid::Uuid,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<Value, sqlx::Error> {
    // `module/base.py:429-527` (retrieve): per-assignee / per-label
    // estimate sums, tolerant of non-numeric `value`s (Django
    // `Cast(value, FloatField)` would 500 on garbage; skipped here via
    // `parse_point_value` — locked tolerant-numeric rule).
    let arows: Vec<EstRow> = sqlx::query_as(
        "SELECT u.first_name AS first_name, u.last_name AS last_name, u.id AS assignee_id, \
         CASE WHEN u.avatar_asset_id IS NOT NULL \
            THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' \
            ELSE u.avatar END AS avatar_url, \
         u.display_name AS display_name, \
         NULL::text AS label_name, NULL::text AS color, NULL::uuid AS label_id, \
         ep.value AS value, i.completed_at, i.archived_at, i.is_draft \
         FROM issues i JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
            AND mi.deleted_at IS NULL \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.deleted_at IS NULL \
         JOIN users u ON u.id = ia.assignee_id \
         JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
         JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL \
         WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL \
         ORDER BY u.first_name, u.last_name",
    )
    .bind(mid)
    .bind(pid)
    .bind(slug)
    .fetch_all(pool)
    .await?;
    let lrows: Vec<EstRow> = sqlx::query_as(
        "SELECT NULL::text AS first_name, NULL::text AS last_name, NULL::uuid AS assignee_id, \
         NULL::text AS avatar_url, NULL::text AS display_name, \
         l.name AS label_name, l.color AS color, l.id AS label_id, \
         ep.value AS value, i.completed_at, i.archived_at, i.is_draft \
         FROM issues i JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
            AND mi.deleted_at IS NULL \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN issue_labels il ON il.issue_id = i.id AND il.deleted_at IS NULL \
         JOIN labels l ON l.id = il.label_id \
         JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
         JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL \
         WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL \
         ORDER BY l.name",
    )
    .bind(mid)
    .bind(pid)
    .bind(slug)
    .fetch_all(pool)
    .await?;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Acc {
        total: f64,
        completed: f64,
        pending: f64,
    }
    let mut amap: BTreeMap<String, (EstRow, Acc)> = BTreeMap::new();
    for r in arows {
        let v = parse_point_value(r.value.as_deref()).unwrap_or(0.0);
        let key = r.assignee_id.map(|u| u.to_string()).unwrap_or_default();
        let e = amap.entry(key).or_insert_with(|| (r.clone(), Acc::default()));
        e.1.total += v;
        if r.completed_at.is_some() && r.archived_at.is_none() && !r.is_draft {
            e.1.completed += v;
        }
        if r.completed_at.is_none() && r.archived_at.is_none() && !r.is_draft {
            e.1.pending += v;
        }
    }
    let mut lmap: BTreeMap<String, (EstRow, Acc)> = BTreeMap::new();
    for r in lrows {
        let v = parse_point_value(r.value.as_deref()).unwrap_or(0.0);
        let key = r.label_id.map(|u| u.to_string()).unwrap_or_default();
        let e = lmap.entry(key).or_insert_with(|| (r.clone(), Acc::default()));
        e.1.total += v;
        if r.completed_at.is_some() && r.archived_at.is_none() && !r.is_draft {
            e.1.completed += v;
        }
        if r.completed_at.is_none() && r.archived_at.is_none() && !r.is_draft {
            e.1.pending += v;
        }
    }
    Ok(json!({
        "assignees": amap.values().map(|(r, a)| json!({
            "first_name": r.first_name, "last_name": r.last_name,
            "assignee_id": r.assignee_id.map(|u| u.to_string()),
            "avatar_url": r.avatar_url, "display_name": r.display_name,
            "total_estimates": a.total, "completed_estimates": a.completed,
            "pending_estimates": a.pending,
        })).collect::<Vec<_>>(),
        "labels": lmap.values().map(|(r, a)| json!({
            "label_name": r.label_name, "color": r.color,
            "label_id": r.label_id.map(|u| u.to_string()),
            "total_estimates": a.total, "completed_estimates": a.completed,
            "pending_estimates": a.pending,
        })).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DoneDay {
    day: Option<NaiveDate>,
    value: Option<String>,
}

/// Burndown inputs for a module: issues-branch (counts) and points-branch
/// (tolerant sums). Mirrors `plane/utils/analytics_plot.py:236-264`
/// (total − cumulative completed; future → null) via the shared
/// `cycle::burndown_chart` (732d cap included).
async fn module_burndown(
    pool: &sqlx::PgPool,
    mid: uuid::Uuid,
    pid: uuid::Uuid,
    slug: &str,
    start: NaiveDate,
    target: NaiveDate,
    points: bool,
) -> Result<Value, sqlx::Error> {
    let rows: Vec<DoneDay> = if points {
        sqlx::query_as(
            "SELECT (i.completed_at AT TIME ZONE 'UTC')::date AS day, ep.value AS value \
             FROM issues i JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
                AND mi.deleted_at IS NULL \
             JOIN workspaces w ON w.id = i.workspace_id \
             JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
             JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' \
                AND e.deleted_at IS NULL \
             WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL \
             AND i.completed_at IS NOT NULL",
        )
        .bind(mid)
        .bind(pid)
        .bind(slug)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT (i.completed_at AT TIME ZONE 'UTC')::date AS day, NULL::text AS value \
             FROM issues i JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
                AND mi.deleted_at IS NULL \
             JOIN workspaces w ON w.id = i.workspace_id \
             WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL \
             AND i.completed_at IS NOT NULL",
        )
        .bind(mid)
        .bind(pid)
        .bind(slug)
        .fetch_all(pool)
        .await?
    };
    let mut done: std::collections::BTreeMap<NaiveDate, f64> = std::collections::BTreeMap::new();
    let mut total = 0.0;
    if points {
        let all: Vec<(Option<String>,)> = sqlx::query_as(
            "SELECT ep.value FROM issues i \
             JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
                AND mi.deleted_at IS NULL \
             JOIN workspaces w ON w.id = i.workspace_id \
             JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
             JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' \
                AND e.deleted_at IS NULL \
             WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL",
        )
        .bind(mid)
        .bind(pid)
        .bind(slug)
        .fetch_all(pool)
        .await?;
        for (v,) in all {
            total += parse_point_value(v.as_deref()).unwrap_or(0.0);
        }
        for r in rows {
            if let Some(d) = r.day {
                *done.entry(d).or_insert(0.0) += parse_point_value(r.value.as_deref()).unwrap_or(0.0);
            }
        }
    } else {
        let t: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM issues i \
             JOIN module_issues mi ON mi.issue_id = i.id AND mi.module_id = $1 \
                AND mi.deleted_at IS NULL \
             JOIN workspaces w ON w.id = i.workspace_id \
             WHERE i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL",
        )
        .bind(mid)
        .bind(pid)
        .bind(slug)
        .fetch_one(pool)
        .await?;
        total = t.0 as f64;
        for r in rows {
            if let Some(d) = r.day {
                *done.entry(d).or_insert(0.0) += 1.0;
            }
        }
    }
    let today = chrono::Utc::now().date_naive();
    Ok(burndown_chart(start, target, today, total, &done, !points))
}

/// Builds the detail body shared by retrieve + archived-detail:
/// list-shape + `link_module[]`, `sub_issues`, the four extra estimate
/// keys, `distribution` (always) + `estimate_distribution` (points
/// estimate only), burndown only when start+target set and total>0
/// (`module/base.py:424-639`, `archive.py:318-540`).
async fn detail_body(
    pool: &sqlx::PgPool,
    row: &ModuleRow,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<Value, sqlx::Error> {
    let links = fetch_links(pool, row.id).await?;
    let sub: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
         WHERE mi.module_id = $1 AND mi.deleted_at IS NULL AND i.parent_id IS NOT NULL \
         AND i.deleted_at IS NULL",
    )
    .bind(row.id)
    .fetch_one(pool)
    .await?;
    let est: (f64, f64, f64, f64) = sqlx::query_as(
        "SELECT \
         (SELECT COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL AND i.deleted_at IS NULL JOIN states s ON s.id = i.state_id AND s.\"group\" = 'backlog' JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL WHERE mi.module_id = $1), \
         (SELECT COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL AND i.deleted_at IS NULL JOIN states s ON s.id = i.state_id AND s.\"group\" = 'unstarted' JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL WHERE mi.module_id = $1), \
         (SELECT COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL AND i.deleted_at IS NULL JOIN states s ON s.id = i.state_id AND s.\"group\" = 'started' JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL WHERE mi.module_id = $1), \
         (SELECT COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL AND i.deleted_at IS NULL JOIN states s ON s.id = i.state_id AND s.\"group\" = 'cancelled' JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL WHERE mi.module_id = $1)",
    )
    .bind(row.id)
    .fetch_one(pool)
    .await?;
    let mut v = module_list_json(row);
    let obj = v.as_object_mut().expect("module json is object");
    obj.insert(
        "link_module".to_string(),
        json!(links.iter().map(link_json).collect::<Vec<_>>()),
    );
    obj.insert("sub_issues".to_string(), json!(sub.0));
    obj.insert("backlog_estimate_points".to_string(), json!(est.0));
    obj.insert("unstarted_estimate_points".to_string(), json!(est.1));
    obj.insert("started_estimate_points".to_string(), json!(est.2));
    obj.insert("cancelled_estimate_points".to_string(), json!(est.3));
    let has_range = row.start_date.is_some() && row.target_date.is_some();
    let mut dist = json!({
        "assignees": assignee_distribution(pool, row.id, pid, slug).await?,
        "labels": label_distribution(pool, row.id, pid, slug).await?,
        "completion_chart": {},
    });
    if has_range && row.total_issues > 0 {
        if let (Some(s), Some(t)) = (row.start_date, row.target_date) {
            dist["completion_chart"] = module_burndown(pool, row.id, pid, slug, s, t, false).await?;
        }
    }
    obj.insert("distribution".to_string(), dist);
    let mut edist = json!({});
    if project_has_points_estimate(pool, pid).await? {
        edist = estimate_distribution(pool, row.id, pid, slug).await?;
        if has_range && row.total_issues > 0 {
            if let (Some(s), Some(t)) = (row.start_date, row.target_date) {
                edist["completion_chart"] = module_burndown(pool, row.id, pid, slug, s, t, true).await?;
            }
        }
    }
    obj.insert("estimate_distribution".to_string(), edist);
    Ok(v)
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `retrieve` (`base.py:395`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // Archived excluded; miss → 404 verbatim (`base.py:414-415`).
    let Some(row) = fetch_row(&st.pool, mid, pid, &slug, auth.0, false).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": MODULE_NOT_FOUND_MSG})),
        ));
    };
    Ok((StatusCode::OK, Json(detail_body(&st.pool, &row, pid, &slug).await?)))
}

async fn apply_update(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    mid: uuid::Uuid,
    body: &Value,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let cur: Option<(Option<chrono::DateTime<chrono::Utc>>, String, Option<NaiveDate>, Option<NaiveDate>)> =
        sqlx::query_as(
            "SELECT m.archived_at, m.status, m.start_date, m.target_date FROM modules m \
             JOIN workspaces w ON w.id = m.workspace_id \
             WHERE m.id = $1 AND m.project_id = $2 AND w.slug = $3 AND m.deleted_at IS NULL",
        )
        .bind(mid)
        .bind(pid)
        .bind(slug)
        .fetch_optional(pool)
        .await?;
    // Django `.first()` on empty → AttributeError → 500; sane 404 instead
    // (documented normalize-crash).
    let Some((archived_at, _status, cur_start, cur_target)) = cur else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": MODULE_NOT_FOUND_MSG})),
        ));
    };
    if guard_patch(archived_at.is_some()).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ARCHIVED_IMMUTABLE_MSG})),
        ));
    }
    if let Some(n) = body.get("name").and_then(Value::as_str) {
        if n.trim().is_empty() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"name": ["This field may not be blank."]})),
            ));
        }
        if n.chars().count() > 255 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"name": ["Ensure this field has no more than 255 characters."]})),
            ));
        }
        // Dup check excluding self (`serializers/module.py:97-100`).
        let dup: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM modules WHERE project_id = $1 AND name = $2 \
             AND id != $3 AND deleted_at IS NULL)",
        )
        .bind(pid)
        .bind(n)
        .bind(mid)
        .fetch_one(pool)
        .await?;
        if guard_dup_name(dup.0).is_err() {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": DUP_NAME_MSG}))));
        }
    }
    // Dates: provided values replace; absent keep current. Both-provided →
    // order check (`serializers/module.py:55-62`).
    let mut new_start = cur_start;
    let mut new_target = cur_target;
    if let Some(raw) = body.get("start_date").and_then(Value::as_str) {
        if raw.trim().is_empty() {
            new_start = None;
        } else if let Some(d) = parse_body_date(raw) {
            new_start = Some(d);
        } else {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    }
    if let Some(raw) = body.get("target_date").and_then(Value::as_str) {
        if raw.trim().is_empty() {
            new_target = None;
        } else if let Some(d) = parse_body_date(raw) {
            new_target = Some(d);
        } else {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    }
    if body.get("start_date").is_some() && body.get("target_date").is_some()
        && guard_date_order(new_start, new_target).is_err()
    {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"non_field_errors": [START_EXCEEDS_TARGET_MSG]})),
        ));
    }
    if let Some(s) = body.get("status").and_then(Value::as_str) {
        if normalize_status(Some(s)).is_err() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"status": [format!("\"{s}\" is not a valid choice.")]})),
            ));
        }
    }
    // Members replace = DELETE + bulk_create
    // (`serializers/module.py:102-118`). Accepts `member_ids` (Django
    // field name) with `members` as an alias.
    let members: Option<Vec<uuid::Uuid>> = if body.get("member_ids").is_some() {
        match parse_uuid_list(body, "member_ids") {
            Ok(v) => Some(v),
            Err(()) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        }
    } else if body.get("members").is_some() {
        match parse_uuid_list(body, "members") {
            Ok(v) => Some(v),
            Err(()) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        }
    } else {
        None
    };
    let lead: Option<Option<uuid::Uuid>> = if body.get("lead_id").is_some() || body.get("lead").is_some() {
        let raw = body.get("lead_id").or_else(|| body.get("lead"));
        match raw {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(s)) if s.trim().is_empty() => Some(None),
            Some(Value::String(s)) => match s.parse() {
                Ok(u) => Some(Some(u)),
                Err(_) => {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": VALID_DETAIL_MSG})),
                    ));
                }
            },
            _ => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        }
    } else {
        None
    };
    // All writes share one tx.
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE modules SET name = COALESCE($1, name), description = COALESCE($2, description), \
         description_text = COALESCE($3, description_text), \
         description_html = COALESCE($4, description_html), \
         start_date = $5, target_date = $6, status = COALESCE($7, status), \
         lead_id = COALESCE($8, lead_id, lead_id), \
         view_props = COALESCE($9, view_props), sort_order = COALESCE($10, sort_order), \
         external_source = COALESCE($11, external_source), \
         external_id = COALESCE($12, external_id), updated_by_id = $13, updated_at = now() \
         WHERE id = $14",
    )
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("description").and_then(Value::as_str))
    .bind(body.get("description_text").cloned())
    .bind(body.get("description_html").cloned())
    .bind(new_start)
    .bind(new_target)
    .bind(body.get("status").and_then(Value::as_str))
    .bind(lead.flatten())
    .bind(body.get("view_props").cloned())
    .bind(body.get("sort_order").and_then(Value::as_f64))
    .bind(body.get("external_source").and_then(Value::as_str))
    .bind(body.get("external_id").and_then(Value::as_str))
    .bind(user)
    .bind(mid)
    .execute(&mut *tx)
    .await?;
    // Explicit lead-clear (COALESCE cannot set NULL).
    if matches!(lead, Some(None)) {
        sqlx::query("UPDATE modules SET lead_id = NULL WHERE id = $1")
            .bind(mid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(ids) = members {
        sqlx::query("DELETE FROM module_members WHERE module_id = $1")
            .bind(mid)
            .execute(&mut *tx)
            .await?;
        if !ids.is_empty() {
            sqlx::query(
                "INSERT INTO module_members (id, module_id, member_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, u.id, $2, p.workspace_id, $3, $3, now(), now() \
                 FROM users u JOIN projects p ON p.id = $2 \
                 WHERE u.id = ANY($4) AND u.is_active = true AND u.deleted_at IS NULL \
                 ON CONFLICT DO NOTHING",
            )
            .bind(mid)
            .bind(pid)
            .bind(user)
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    // Django PATCH response (`base.py:673-705`): the list-shape row.
    match fetch_row(pool, mid, pid, slug, user, false).await? {
        Some(row) => Ok((StatusCode::OK, Json(module_list_json(&row)))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": MODULE_NOT_FOUND_MSG})),
        )),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `partial_update` (`base.py:651`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    apply_update(&st.pool, auth.0, &slug, pid, mid, &body).await
}

/// PUT (`urls/module.py:24-35` maps `put → update`; Django defines no
/// `update` on `ModuleViewSet`, so DRF falls back to the stock
/// `ModelViewSet.update` with NO `@allow_permission` decorator —
/// locked hardening deviation §12 implements it WITH `[ADMIN,MEMBER]`
/// enforcement, sharing the PATCH body logic).
pub async fn update(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    apply_update(&st.pool, auth.0, &slug, pid, mid, &body).await
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `@allow_permission([ROLE.ADMIN], creator=True, model=Module)`
    // (`base.py:723`): ADMIN passes; creator-bypass skips the role check.
    // `.get()` crash on miss → sane 404 (documented normalize-crash).
    let cur: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT m.created_by_id FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.id = $1 AND m.project_id = $2 AND w.slug = $3 AND m.deleted_at IS NULL",
    )
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((created_by,)) = cur else {
        return Ok(missing());
    };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, pid).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    let admin_ok = project_gate_allows(role == Some(20), role.is_some(), ws_admin);
    if !admin_ok && created_by != Some(auth.0) {
        return Ok(deny());
    }
    // Soft-delete module + module_issues + own favorites; HARD-delete
    // recent visits (`base.py:742-758`; Celery side-effects skipped).
    // All four writes share one tx.
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE modules SET deleted_at = now() WHERE id = $1")
        .bind(mid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE module_issues SET deleted_at = now() WHERE module_id = $1 AND deleted_at IS NULL")
        .bind(mid)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE user_favorites SET deleted_at = now() WHERE user_id = $1 \
         AND entity_type = 'module' AND entity_identifier = $2 AND project_id = $3 \
         AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(mid)
    .bind(pid)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM user_recent_visits WHERE project_id = $1 \
         AND entity_identifier = $2 AND entity_name = 'module'",
    )
    .bind(pid)
    .bind(mid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E3b — module issues.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModuleIssuesQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub sub_group_by: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ModuleIssueRow {
    id: uuid::Uuid,
    name: String,
    state_id: Option<uuid::Uuid>,
    sort_order: f64,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    estimate_point: Option<uuid::Uuid>,
    priority: String,
    sequence_id: i32,
    project_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    module_id: Option<uuid::Uuid>,
    link_count: i64,
    attachment_count: i64,
    sub_issues_count: i64,
}

fn module_issue_json(r: &ModuleIssueRow) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "state_id": opt_uuid(&r.state_id),
        "sort_order": r.sort_order,
        "completed_at": r.completed_at,
        "estimate_point": opt_uuid(&r.estimate_point),
        "priority": r.priority,
        "sequence_id": r.sequence_id,
        "project_id": r.project_id,
        "parent_id": opt_uuid(&r.parent_id),
        "module_id": opt_uuid(&r.module_id),
        "link_count": r.link_count,
        "attachment_count": r.attachment_count,
        "sub_issues_count": r.sub_issues_count,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
    })
}

pub async fn issues_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<ModuleIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `ModuleIssueViewSet.list` (`issue.py:94-96`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `issue.py:129-133`: group_by == sub_group_by → 400.
    if let (Some(g), Some(s)) = (q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        if !g.is_empty() && g == s {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": GROUP_DUP_MSG}))));
        }
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
    };
    let page: i128 = match q.cursor.as_deref() {
        None => 0,
        Some(c) => match parse_cursor(c) {
            Ok(cur) => cur.page,
            Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
        },
    };
    // Django default is `created_at` ASC (`issue.py:112`) — NOT
    // `-created_at` (differs from the cycle twin; mirrored literally).
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("created_at"));
    let (expr, desc) = detail_order_expr(&sanitized);
    let dir = if desc { "DESC NULLS LAST" } else { "ASC NULLS LAST" };
    let order = format!("{expr} {dir}, i.created_at DESC");
    let base = "FROM issues i JOIN module_issues mi ON mi.issue_id = i.id \
        AND mi.module_id = $1 AND mi.deleted_at IS NULL \
        LEFT JOIN states s ON s.id = i.state_id \
        WHERE i.project_id = $2 AND i.deleted_at IS NULL";
    let total: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) {base}"))
        .bind(mid)
        .bind(pid)
        .fetch_one(&st.pool)
        .await?;
    let total = total.0;
    let limit = per_page.max(0);
    // Truthy `group_by` grouped shapes are OUT (flat envelope; E2 cycle
    // precedent) — only the equality 400 above is Django-verbatim.
    let (rows, next, prev, next_has, prev_has, count, pages): (
        Vec<ModuleIssueRow>,
        String,
        String,
        bool,
        bool,
        i64,
        i64,
    ) = if limit <= 0 {
        (
            vec![],
            next_cursor_str(0, page),
            prev_cursor_str(0, page),
            false,
            page > 0,
            0,
            total_pages(total, 1),
        )
    } else {
        match page_window(page, limit) {
            Err(()) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"detail": "Invalid cursor parameter."})),
                ));
            }
            Ok(PageWindow::BeyondEnd) => (
                vec![],
                next_cursor_str(limit, page),
                prev_cursor_str(limit, page),
                false,
                true,
                0,
                total_pages(total, limit),
            ),
            Ok(PageWindow::Rows(offset)) => {
                let sql = format!(
                    "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
                     i.estimate_point_id AS estimate_point, i.priority, i.sequence_id, \
                     i.project_id, i.parent_id, i.created_at, i.updated_at, \
                     (SELECT mi2.module_id FROM module_issues mi2 WHERE mi2.issue_id = i.id \
                      AND mi2.deleted_at IS NULL LIMIT 1) AS module_id, \
                     (SELECT COUNT(*) FROM issue_links il WHERE il.issue_id = i.id \
                      AND il.deleted_at IS NULL) AS link_count, \
                     (SELECT COUNT(*) FROM file_assets fa WHERE fa.issue_id = i.id \
                      AND fa.entity_type = 'ISSUE_ATTACHMENT' AND fa.deleted_at IS NULL) AS attachment_count, \
                     (SELECT COUNT(*) FROM issues ch WHERE ch.parent_id = i.id \
                      AND ch.deleted_at IS NULL) AS sub_issues_count \
                     {base} ORDER BY {order} LIMIT $3 OFFSET $4"
                );
                let rows: Vec<ModuleIssueRow> = sqlx::query_as(&sql)
                    .bind(mid)
                    .bind(pid)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&st.pool)
                    .await?;
                let n = rows.len() as i64;
                let has_next = offset + n < total;
                (
                    rows,
                    next_cursor_str(limit, page),
                    prev_cursor_str(limit, page),
                    has_next,
                    page > 0,
                    n,
                    total_pages(total, limit),
                )
            }
        }
    };
    let env = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next,
        prev_cursor: prev,
        next_page_results: next_has,
        prev_page_results: prev_has,
        count,
        total_pages: pages,
        total_results: total,
        extra_stats: None,
        results: rows.iter().map(module_issue_json).collect(),
    };
    Ok((StatusCode::OK, Json(json!(env))))
}

pub async fn issues_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `create_module_issues` (`issue.py:209-211`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let issues: Vec<uuid::Uuid> = body
        .get("issues")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
                .collect()
        })
        .unwrap_or_default();
    // `issue.py:213-214`.
    if guard_issues_present(issues.len()).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ISSUES_REQUIRED_MSG})),
        ));
    }
    // Django `.get()` crash on missing module/project → sane 404
    // (documented normalize-crash).
    let ws: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT m.workspace_id FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.id = $1 AND m.project_id = $2 AND w.slug = $3 AND m.deleted_at IS NULL",
    )
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((ws_id,)) = ws else {
        return Ok(missing());
    };
    // M8 (`issue.py:215-238`): issues re-scoped to ws+project, unknown
    // silently dropped. Single tx for both writes.
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "INSERT INTO module_issues (id, project_id, workspace_id, created_by_id, \
         updated_by_id, module_id, issue_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, $3, $3, $4, i.id, now(), now() \
         FROM issues i WHERE i.id = ANY($5) AND i.workspace_id = $2 \
         AND i.project_id = $1 AND i.deleted_at IS NULL \
         ON CONFLICT DO NOTHING",
    )
    .bind(pid)
    .bind(ws_id)
    .bind(auth.0)
    .bind(mid)
    .bind(&issues)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"message": "success"}))))
}

pub async fn issue_modules_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, iid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `create_issue_modules` (`issue.py:256-258`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // Django `.get()` crash on missing issue → sane 404 (documented
    // normalize-crash; the added-modules path itself is unscoped below).
    let issue_ok: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
         WHERE i.id = $1 AND i.project_id = $2 AND w.slug = $3 AND i.deleted_at IS NULL)",
    )
    .bind(iid)
    .bind(pid)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if !issue_ok.0 {
        return Ok(missing());
    }
    let ws: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT p.workspace_id FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((ws_id,)) = ws else {
        return Ok(missing());
    };
    let modules: Vec<uuid::Uuid> = body
        .get("modules")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
                .collect()
        })
        .unwrap_or_default();
    let removed: Vec<uuid::Uuid> = body
        .get("removed_modules")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
                .collect()
        })
        .unwrap_or_default();
    // M9 (`issue.py:263-323`): always 201 even when both lists are empty;
    // added modules are NOT scoped to ws+project (replicated as-is —
    // Django bulk-creates whatever ids arrive; FK violations surface as
    // 500s there, normalized here by dropping unknown module ids, since
    // a hard 500 on user input is never the sane code).
    let mut tx = st.pool.begin().await?;
    if !modules.is_empty() {
        sqlx::query(
            "INSERT INTO module_issues (id, project_id, workspace_id, created_by_id, \
             updated_by_id, module_id, issue_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, $2, $3, $3, m.id, $4, now(), now() \
             FROM modules m WHERE m.id = ANY($5) AND m.deleted_at IS NULL \
             ON CONFLICT DO NOTHING",
        )
        .bind(pid)
        .bind(ws_id)
        .bind(auth.0)
        .bind(iid)
        .bind(&modules)
        .execute(&mut *tx)
        .await?;
    }
    if !removed.is_empty() {
        sqlx::query(
            "DELETE FROM module_issues WHERE issue_id = $1 AND module_id = ANY($2) \
             AND project_id = $3 AND workspace_id = $4",
        )
        .bind(iid)
        .bind(&removed)
        .bind(pid)
        .bind(ws_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"message": "success"}))))
}

pub async fn issue_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid, iid)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `destroy` (`issue.py:325-345`): AM.
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `issue.py:327-344`: soft-delete; **204** always, even with 0 rows.
    // Django `.first().module` crashes on a missing link (500); the 204
    // idempotent delete is the sane code (documented normalize-crash,
    // locked §3).
    sqlx::query(
        "UPDATE module_issues SET deleted_at = now() WHERE issue_id = $1 \
         AND project_id = $2 AND module_id = $3 AND workspace_id IN \
         (SELECT id FROM workspaces WHERE slug = $4) AND deleted_at IS NULL",
    )
    .bind(iid)
    .bind(pid)
    .bind(mid)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E3c — module links.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LinkListQuery {
    #[serde(default)]
    pub fields: Option<String>,
}

pub async fn links_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<LinkListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // `ModuleLinkViewSet` (`base.py:762-788`,
    // `permissions/project.py:101-110`): SAFE = any active member incl
    // GUEST; deny is the DRF permission-class 403 `{"detail": ...}`.
    if !gate_safe(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `base.py:786`: order `-created_at`.
    let rows = fetch_links(&st.pool, mid).await?;
    let fields = parse_fields(q.fields.as_deref());
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|r| project_fields(link_json(r), &fields))
            .collect::<Vec<_>>())),
    ))
}

pub async fn links_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Unsafe → ADMIN/MEMBER (`permissions/project.py:112-119`).
    if !gate_entity_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // Django `.get()`-style crash on missing module → sane 404
    // (documented normalize-crash).
    let ws: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT m.workspace_id FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.id = $1 AND m.project_id = $2 AND w.slug = $3 AND m.deleted_at IS NULL",
    )
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((ws_id,)) = ws else {
        return Ok(missing());
    };
    // `serializers/module.py:170-176`: prepend `http://` when missing.
    let raw = body.get("url").and_then(Value::as_str).unwrap_or("");
    let url = normalize_link_url(raw);
    // `serializers/module.py:178-186`: bad url → 400. DRF renders the
    // field-validator error under the `url` key; mirrored here.
    if url.is_empty() || !valid_link_url(&url) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"url": {"error": INVALID_URL_MSG}})),
        ));
    }
    // `serializers/module.py:190-191`: dup url+module → 400.
    let dup: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM module_links \
         WHERE url = $1 AND module_id = $2 AND deleted_at IS NULL)",
    )
    .bind(&url)
    .bind(mid)
    .fetch_one(&st.pool)
    .await?;
    if dup.0 {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": URL_EXISTS_MSG}))));
    }
    let title = body.get("title").and_then(Value::as_str);
    let row: Option<LinkRow> = sqlx::query_as(
        "INSERT INTO module_links (id, title, url, metadata, module_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, COALESCE($3::jsonb, '{}'::jsonb), $4, $5, $6, $7, $7, \
         now(), now()) \
         RETURNING id, title, url, metadata, module_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at",
    )
    .bind(title)
    .bind(&url)
    .bind(body.get("metadata").cloned())
    .bind(mid)
    .bind(pid)
    .bind(ws_id)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::CREATED, Json(link_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn link_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid, lid)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_safe(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    let row: Option<LinkRow> = sqlx::query_as(
        "SELECT ml.id, ml.title, ml.url, ml.metadata, ml.module_id, ml.project_id, ml.workspace_id, \
         ml.created_by_id, ml.updated_by_id, ml.created_at, ml.updated_at \
         FROM module_links ml JOIN workspaces w ON w.id = ml.workspace_id \
         WHERE ml.id = $1 AND ml.module_id = $2 AND ml.project_id = $3 AND w.slug = $4 \
         AND ml.deleted_at IS NULL",
    )
    .bind(lid)
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(link_json(&r)))),
        None => Ok(missing()),
    }
}

async fn apply_link_update(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    mid: uuid::Uuid,
    lid: uuid::Uuid,
    body: &Value,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let cur: Option<(String,)> = sqlx::query_as(
        "SELECT ml.url FROM module_links ml JOIN workspaces w ON w.id = ml.workspace_id \
         WHERE ml.id = $1 AND ml.module_id = $2 AND ml.project_id = $3 AND w.slug = $4 \
         AND ml.deleted_at IS NULL",
    )
    .bind(lid)
    .bind(mid)
    .bind(pid)
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    // Django `get_object()` miss → 404 (sane `missing()`).
    let Some((cur_url,)) = cur else {
        return Ok(missing());
    };
    let mut url = cur_url;
    if let Some(raw) = body.get("url").and_then(Value::as_str) {
        let norm = normalize_link_url(raw);
        if norm.is_empty() || !valid_link_url(&norm) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"url": {"error": INVALID_URL_MSG}})),
            ));
        }
        // `serializers/module.py:194-201`: update-dup (excluding self) →
        // 400 with the sic-"Issue" message, verbatim.
        let dup: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM module_links \
             WHERE url = $1 AND module_id = $2 AND id != $3 AND deleted_at IS NULL)",
        )
        .bind(&norm)
        .bind(mid)
        .bind(lid)
        .fetch_one(pool)
        .await?;
        if dup.0 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": URL_EXISTS_ISSUE_MSG})),
            ));
        }
        url = norm;
    }
    let title = body.get("title").and_then(Value::as_str);
    let row: Option<LinkRow> = sqlx::query_as(
        "UPDATE module_links SET url = $1, title = COALESCE($2, title), \
         metadata = COALESCE($3, metadata), updated_by_id = $4, updated_at = now() \
         WHERE id = $5 \
         RETURNING id, title, url, metadata, module_id, project_id, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at",
    )
    .bind(&url)
    .bind(title)
    .bind(body.get("metadata").cloned())
    .bind(user)
    .bind(lid)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(link_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn link_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid, lid)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_entity_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    apply_link_update(&st.pool, auth.0, &slug, pid, mid, lid, &body).await
}

/// PUT mirrors PATCH (`urls/module.py:63-74` maps both `put → update` and
/// `patch → partial_update` onto `ModuleLinkSerializer.update`,
/// `serializers/module.py:194-203`).
pub async fn link_put(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid, lid)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_entity_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    apply_link_update(&st.pool, auth.0, &slug, pid, mid, lid, &body).await
}

pub async fn link_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid, lid)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_entity_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    sqlx::query(
        "UPDATE module_links SET deleted_at = now() WHERE id = $1 AND module_id = $2 \
         AND project_id = $3 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $4) \
         AND deleted_at IS NULL",
    )
    .bind(lid)
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E3d — favorite modules.
// ============================================================================

pub async fn fav_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // `ModuleFavoriteViewSet` (`base.py:791-793`,
    // `permissions/project.py:136-148`): Lite = any active member.
    if !gate_lite(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `base.py:804-811`: NO module-existence check on create.
    let mid: Option<uuid::Uuid> = body
        .get("module")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    let Some(mid) = mid else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        ));
    };
    let r = sqlx::query(
        "INSERT INTO user_favorites (id, project_id, workspace_id, user_id, entity_type, \
         entity_identifier, name, is_folder, sequence, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, p.workspace_id, $2, 'module', $3, '', false, 0, now(), now() \
         FROM projects p WHERE p.id = $1",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(mid)
    .execute(&st.pool)
    .await;
    // Dup → 400 `{"error":"The payload is not valid"}` (Django
    // `IntegrityError` → `views/base.py:92-97`).
    match r {
        Ok(_) => Ok((StatusCode::NO_CONTENT, Json(Value::Null))),
        Err(e) if is_constraint_violation(&e) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )),
        Err(e) => Err(e.into()),
    }
}

pub async fn fav_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_lite(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `base.py:813-822`: `.get()` miss → 404; HARD delete
    // (`soft=False`).
    let n = sqlx::query(
        "DELETE FROM user_favorites WHERE project_id = $1 AND entity_type = 'module' \
         AND user_id = $2 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) \
         AND entity_identifier = $4",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(&slug)
    .bind(mid)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E3f — archive.
// ============================================================================

pub async fn archived_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<ModuleListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // `ModuleArchiveUnarchiveEndpoint` (`archive.py:42-43`): GET is SAFE →
    // any active member incl GUEST.
    if !gate_safe(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `archive.py:258-293`: archived-only, shape OMITS
    // `logo_props,estimate_points`.
    let select = module_select(true);
    let sql = format!("{select} ORDER BY is_favorite DESC, m.created_at DESC");
    let rows: Vec<ModuleRow> = sqlx::query_as(&sql)
        .bind(pid)
        .bind(&slug)
        .bind(auth.0)
        .fetch_all(&st.pool)
        .await?;
    let fields = parse_fields(q.fields.as_deref());
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|r| project_fields(module_archived_json(r), &fields))
            .collect::<Vec<_>>())),
    ))
}

pub async fn archived_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_safe(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `archive.py:294-309,318-319`: miss OR non-archived → 404. Django
    // serializes `None` and crashes (500); the 404 is the sane code
    // (documented normalize-crash).
    let Some(row) = fetch_row(&st.pool, pk, pid, &slug, auth.0, true).await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(detail_body(&st.pool, &row, pid, &slug).await?)))
}

pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // POST is unsafe → ADMIN/MEMBER (`permissions/project.py:112-119`).
    if !gate_entity_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // Django `.get()` crash → sane 404 (documented normalize-crash).
    let cur: Option<(String,)> = sqlx::query_as(
        "SELECT m.status FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.id = $1 AND m.project_id = $2 AND w.slug = $3 AND m.deleted_at IS NULL",
    )
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((status,)) = cur else {
        return Ok(missing());
    };
    // `archive.py:546-550`.
    if guard_archive_status(&status).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ARCHIVE_STATUS_MSG})),
        ));
    }
    // Single DB clock for the stored `archived_at` AND the response.
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&st.pool)
        .await?;
    // Both writes share one tx.
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE modules SET archived_at = $1, updated_at = now() WHERE id = $2")
        .bind(now)
        .bind(mid)
        .execute(&mut *tx)
        .await?;
    // `archive.py:553-558`: delete favorites on archive (all users).
    sqlx::query(
        "UPDATE user_favorites SET deleted_at = now() WHERE entity_type = 'module' \
         AND entity_identifier = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(mid)
    .bind(pid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    // `str(timezone.now())` format (`%Y-%m-%d %H:%M:%S%.6f+00:00`) — shared
    // with E2 (`cycle::format_archived_at`).
    Ok((StatusCode::OK, Json(json!({"archived_at": format_archived_at(now)}))))
}

pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, mid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_entity_write(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny_detail());
    }
    // `archive.py:561-565`: no status check on unarchive.
    let n = sqlx::query(
        "UPDATE modules SET archived_at = NULL, updated_at = now() WHERE id = $1 \
         AND project_id = $2 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) \
         AND deleted_at IS NULL",
    )
    .bind(mid)
    .bind(pid)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E3g — workspace modules.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsModuleRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    name: String,
    description: String,
    description_text: Option<Value>,
    description_html: Option<Value>,
    start_date: Option<NaiveDate>,
    target_date: Option<NaiveDate>,
    status: String,
    lead_id: Option<uuid::Uuid>,
    member_ids: Vec<uuid::Uuid>,
    view_props: Value,
    sort_order: f64,
    external_source: Option<String>,
    external_id: Option<String>,
    logo_props: Value,
    completed_estimate_points: f64,
    total_estimate_points: f64,
    total_issues: i64,
    cancelled_issues: i64,
    completed_issues: i64,
    started_issues: i64,
    unstarted_issues: i64,
    backlog_issues: i64,
}

pub async fn workspace_modules(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::project::ws_role;
    // `WorkspaceViewerPermission` = any ACTIVE ws member
    // (`workspace/module.py:25`); deny is the DRF permission-class 403
    // `{"detail": ...}` (NOT `deny()`).
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    // `workspace/module.py:26-129`: member-projects only, archived
    // excluded (module + project), WITH group counts + member_ids,
    // serialized via `ModuleSerializer` (model keys + estimate points).
    let rows: Vec<WsModuleRow> = sqlx::query_as(
        "SELECT m.id, m.workspace_id, m.project_id, m.name, m.description, \
         m.description_text, m.description_html, m.start_date, m.target_date, m.status, \
         m.lead_id, \
         COALESCE(ARRAY(SELECT DISTINCT mm.member_id FROM module_members mm \
            WHERE mm.module_id = m.id AND mm.deleted_at IS NULL), '{}') AS member_ids, \
         m.view_props, m.sort_order, m.external_source, m.external_id, m.logo_props, \
         (SELECT COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0) \
          FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL \
          AND i.deleted_at IS NULL JOIN states s ON s.id = i.state_id AND s.\"group\" = 'completed' \
          JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
          JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL \
          WHERE mi.module_id = m.id) AS completed_estimate_points, \
         (SELECT COALESCE(SUM(CASE WHEN ep.value ~ '^[+-]?(\\d+(\\.\\d*)?|\\.\\d+)([eE][+-]?\\d+)?$' THEN ep.value::double precision ELSE 0 END), 0) \
          FROM module_issues mi JOIN issues i ON i.id = mi.issue_id AND mi.deleted_at IS NULL \
          AND i.deleted_at IS NULL \
          JOIN estimate_points ep ON ep.id = i.estimate_point_id AND ep.deleted_at IS NULL \
          JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL \
          WHERE mi.module_id = m.id) AS total_estimate_points, \
         (SELECT COUNT(DISTINCT mi.issue_id) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          WHERE mi.module_id = m.id AND mi.deleted_at IS NULL \
          AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS total_issues, \
         (SELECT COUNT(DISTINCT mi.issue_id) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          JOIN states s ON s.id = i.state_id \
          WHERE mi.module_id = m.id AND mi.deleted_at IS NULL AND s.\"group\" = 'completed' \
          AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS completed_issues, \
         (SELECT COUNT(DISTINCT mi.issue_id) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          JOIN states s ON s.id = i.state_id \
          WHERE mi.module_id = m.id AND mi.deleted_at IS NULL AND s.\"group\" = 'cancelled' \
          AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS cancelled_issues, \
         (SELECT COUNT(DISTINCT mi.issue_id) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          JOIN states s ON s.id = i.state_id \
          WHERE mi.module_id = m.id AND mi.deleted_at IS NULL AND s.\"group\" = 'started' \
          AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS started_issues, \
         (SELECT COUNT(DISTINCT mi.issue_id) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          JOIN states s ON s.id = i.state_id \
          WHERE mi.module_id = m.id AND mi.deleted_at IS NULL AND s.\"group\" = 'unstarted' \
          AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS unstarted_issues, \
         (SELECT COUNT(DISTINCT mi.issue_id) FROM module_issues mi JOIN issues i ON i.id = mi.issue_id \
          JOIN states s ON s.id = i.state_id \
          WHERE mi.module_id = m.id AND mi.deleted_at IS NULL AND s.\"group\" = 'backlog' \
          AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS backlog_issues \
         FROM modules m JOIN workspaces w ON w.id = m.workspace_id \
         JOIN projects p ON p.id = m.project_id \
         WHERE w.slug = $1 AND m.deleted_at IS NULL AND m.archived_at IS NULL \
         AND p.archived_at IS NULL AND p.deleted_at IS NULL \
         AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
            AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         ORDER BY m.created_at DESC",
    )
    .bind(&slug)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|r| json!({
                "id": r.id,
                "workspace_id": r.workspace_id,
                "project_id": r.project_id,
                "name": r.name,
                "description": r.description,
                "description_text": r.description_text.clone().unwrap_or(Value::Null),
                "description_html": r.description_html.clone().unwrap_or(Value::Null),
                "start_date": r.start_date,
                "target_date": r.target_date,
                "status": r.status,
                "lead_id": opt_uuid(&r.lead_id),
                "member_ids": r.member_ids,
                "view_props": r.view_props,
                "sort_order": r.sort_order,
                "external_source": opt_str(&r.external_source),
                "external_id": opt_str(&r.external_id),
                "logo_props": r.logo_props,
                "completed_estimate_points": r.completed_estimate_points,
                "total_estimate_points": r.total_estimate_points,
                "total_issues": r.total_issues,
                "cancelled_issues": r.cancelled_issues,
                "completed_issues": r.completed_issues,
                "started_issues": r.started_issues,
                "unstarted_issues": r.unstarted_issues,
                "backlog_issues": r.backlog_issues,
            }))
            .collect::<Vec<_>>())),
    ))
}

// ============================================================================
// Tests (STEP 1 — pure fns; no DB).
// ============================================================================

#[cfg(test)]
mod module_e3_tests {
    use super::*;

    #[test]
    fn dup_name_const_verbatim() {
        // `serializers/module.py:68-72,97-100` (POST + PATCH-excluding-self).
        assert_eq!(DUP_NAME_MSG, "Module with this name already exists");
        assert!(guard_dup_name(true).is_err());
        assert_eq!(guard_dup_name(true).unwrap_err(), DUP_NAME_MSG);
        assert!(guard_dup_name(false).is_ok());
    }

    #[test]
    fn url_dup_consts_verbatim_incl_sic() {
        // `serializers/module.py:191` (create) vs `:201` (update — sic
        // "Issue", verbatim; the module serializer reuses the issue text).
        assert_eq!(URL_EXISTS_MSG, "URL already exists.");
        assert_eq!(URL_EXISTS_ISSUE_MSG, "URL already exists for this Issue");
        assert_ne!(URL_EXISTS_MSG, URL_EXISTS_ISSUE_MSG);
        assert_eq!(INVALID_URL_MSG, "Invalid URL format.");
    }

    #[test]
    fn archived_gate_const_verbatim() {
        // `base.py:663-667` (PATCH) + pre-existing `guard_patch` surface.
        assert_eq!(ARCHIVED_IMMUTABLE_MSG, "Archived module cannot be updated");
        assert!(guard_patch(true).is_err());
        assert!(guard_patch(false).is_ok());
    }

    #[test]
    fn status_and_estimate_shape() {
        // New module `status` defaults to `planned`
        // (`db/models/module.py:74-85`); unknown statuses rejected.
        assert_eq!(normalize_status(None).unwrap(), "planned");
        assert_eq!(normalize_status(Some("completed")).unwrap(), "completed");
        assert!(normalize_status(Some("archived")).is_err());
        // Archive gate (`archive.py:546-550`): only completed/cancelled.
        assert_eq!(
            ARCHIVE_STATUS_MSG,
            "Only completed or cancelled modules can be archived"
        );
        assert!(guard_archive_status("completed").is_ok());
        assert!(guard_archive_status("cancelled").is_ok());
        assert!(guard_archive_status("planned").is_err());
        assert!(guard_archive_status("in-progress").is_err());
        // Tolerant numeric parsing: garbage estimate values are skipped,
        // never crash (`parse_point_value`, shared with E2).
        assert_eq!(parse_point_value(Some("3.5")), Some(3.5));
        assert_eq!(parse_point_value(Some(" 2 ")), Some(2.0));
        assert!(parse_point_value(Some("abc")).is_none());
        assert!(parse_point_value(None).is_none());
        assert!(parse_point_value(Some("")).is_none());
    }

    #[test]
    fn link_url_normalize_and_validate() {
        // `serializers/module.py:170-186`: prepend + URLValidator shape.
        assert_eq!(normalize_link_url("example.com/x"), "http://example.com/x");
        assert_eq!(normalize_link_url("https://a.b"), "https://a.b");
        assert_eq!(normalize_link_url("http://a.b"), "http://a.b");
        assert_eq!(normalize_link_url(""), "");
        assert!(valid_link_url("http://example.com/x"));
        assert!(!valid_link_url("not a url"));
        assert!(!valid_link_url(""));
    }

    #[test]
    fn guards_roles_and_issues() {
        // AMG 20/15/5; AM 20/15 (`base.py:294,353,395,651` + userprops).
        assert!(guard_amg(Some(20)).is_ok());
        assert!(guard_amg(Some(15)).is_ok());
        assert!(guard_amg(Some(5)).is_ok());
        assert!(guard_amg(Some(10)).is_err());
        assert!(guard_amg(None).is_err());
        assert!(guard_am(Some(20)).is_ok());
        assert!(guard_am(Some(15)).is_ok());
        assert!(guard_am(Some(5)).is_err());
        // `issue.py:213-214`.
        assert_eq!(ISSUES_REQUIRED_MSG, "Issues are required");
        assert!(guard_issues_present(0).is_err());
        assert!(guard_issues_present(2).is_ok());
        // `issue.py:129-133`.
        assert_eq!(
            GROUP_DUP_MSG,
            "Group by and sub group by cannot have same parameters"
        );
        // Detail miss verbatim (`base.py:415`).
        assert_eq!(MODULE_NOT_FOUND_MSG, "Module not found");
        // Serializer order (`serializers/module.py:61`).
        assert_eq!(START_EXCEEDS_TARGET_MSG, "Start date cannot exceed target date");
    }

    #[test]
    fn fields_projection_keeps_requested() {
        // `?fields=` (`base.py:356-357`).
        let v = json!({"id": 1, "name": "M", "status": "planned"});
        let out = project_fields(v.clone(), &["name".to_string()]);
        assert_eq!(out, json!({"name": "M"}));
        assert_eq!(project_fields(v.clone(), &[]), v);
    }
}
