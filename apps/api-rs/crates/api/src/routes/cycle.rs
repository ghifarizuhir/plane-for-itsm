use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::project::{deny, missing, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{
    fetch_project_member_role, is_workspace_admin, next_cursor_str, parse_cursor, parse_per_page,
    prev_cursor_str, project_gate_allows, sanitize_order_by, total_pages, DetailEnvelope,
    PageWindow,
};

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/cycle/base.py:331`
/// ("Both start date and end date are either required or are to be null").
pub const BOTH_DATES_MSG: &str = "Both start date and end date are either required or are to be null";
/// `plane/app/serializers/cycle.py:22` (`CycleWriteSerializer.validate`).
pub const START_EXCEEDS_END_MSG: &str = "Start date cannot exceed end date";
/// `plane/app/views/cycle/base.py:459` (retrieve miss — verbatim).
pub const CYCLE_NOT_FOUND_MSG: &str = "Cycle not found";
/// `plane/app/views/cycle/base.py:341` (patch archived).
pub const ARCHIVED_IMMUTABLE_MSG: &str = "Archived cycle cannot be updated";
/// `plane/app/views/cycle/base.py:355` (patch completed without sort_order).
pub const COMPLETED_IMMUTABLE_MSG: &str = "The Cycle has already been completed so it cannot be edited";
/// `plane/app/views/cycle/issue.py:228` (cycle-issues POST empty).
pub const ISSUES_REQUIRED_MSG: &str = "Issues are required";
/// `plane/app/views/cycle/issue.py:234` (cycle-issues POST to completed).
pub const COMPLETED_NO_ADD_MSG: &str = "The Cycle has already been completed so no new issues can be added";
/// `plane/app/views/cycle/issue.py:148` (group_by == sub_group_by).
pub const GROUP_DUP_MSG: &str = "Group by and sub group by cannot have same parameters";
/// `plane/app/views/cycle/base.py:528` (date-check missing dates).
pub const DATECHECK_REQUIRED_MSG: &str = "Start date and end date both are required";
/// `plane/app/views/cycle/base.py:551` (date-check overlap; HTTP 200, NOT 4xx).
pub const DATECHECK_OVERLAP_MSG: &str = "You have a cycle already on the given dates, if you want to create a draft cycle you can do that by removing dates";
/// `plane/app/views/cycle/base.py:600` (transfer missing new_cycle_id).
pub const TRANSFER_TARGET_REQUIRED_MSG: &str = "New Cycle Id is required";
/// `plane/utils/cycle_transfer_issues.py:64` (transfer to completed target).
pub const TRANSFER_TARGET_COMPLETED_MSG: &str = "The cycle where the issues are transferred is already completed";
/// `plane/utils/cycle_transfer_issues.py:147` (transfer bad source).
pub const TRANSFER_SOURCE_MISSING_MSG: &str = "Source cycle not found";
/// `plane/app/views/cycle/archive.py:592` (archive non-completed).
pub const ARCHIVE_ONLY_COMPLETED_MSG: &str = "Only completed cycles can be archived";
/// `plane/app/views/cycle/base.py:809` (analytics without dates).
pub const NO_DATES_MSG: &str = "Cycle has no start or end date";
/// `plane/app/views/base.py:92-97` (Django `IntegrityError` → 400; favorite dup).
pub const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";
/// DRF permission-class deny body (`WorkspaceViewerPermission`, E2j).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Mirrors the `status` Case (`plane/app/views/cycle/base.py:153-167`):
/// CURRENT → UPCOMING → COMPLETED → DRAFT (both null) → default DRAFT.
/// Order matters: the first matching When wins.
pub fn cycle_status(
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> &'static str {
    if let (Some(s), Some(e)) = (start, end) {
        if s <= now && e >= now {
            return "CURRENT";
        }
        if s > now {
            return "UPCOMING";
        }
        if e < now {
            return "COMPLETED";
        }
    }
    if start.is_none() && end.is_none() {
        return "DRAFT";
    }
    "DRAFT"
}

/// Mirrors the overlap filter (`plane/app/views/cycle/base.py:542-546`):
/// `(start<=s & end>=s) | (start<=e & end>=e) | (start>=s & end<=e)`.
pub fn cycles_overlap(
    a_start: DateTime<Utc>,
    a_end: DateTime<Utc>,
    b_start: DateTime<Utc>,
    b_end: DateTime<Utc>,
) -> bool {
    (a_start <= b_start && a_end >= b_start)
        || (a_start <= b_end && a_end >= b_end)
        || (a_start >= b_start && a_end <= b_end)
}

/// Mirrors `CycleDateCheckEndpoint.post` (`plane/app/views/cycle/base.py:548-556`):
/// overlap → **200** `{error, status:false}` (200, NOT 4xx); else 200 `{status:true}`.
pub fn date_check_result(overlap: bool) -> (StatusCode, Value) {
    if overlap {
        (
            StatusCode::OK,
            json!({"error": DATECHECK_OVERLAP_MSG, "status": false}),
        )
    } else {
        (StatusCode::OK, json!({"status": true}))
    }
}

/// Pure edge of `convert_to_utc` (`plane/utils/timezone_converter.py:82-83`):
/// a start-date whose local date IS today-in-project-tz stores `now()`
/// instead of `00:00:01` local→UTC. The SQL helper `convert_to_utc` below
/// implements the same rule; this fn is the testable decision bit.
pub fn convert_start_is_today(date_part: &str, today_part: &str) -> bool {
    date_part == today_part
}

/// Extracts the `YYYY-MM-DD` date part Django's serializer converts
/// (`plane/app/serializers/cycle.py:29-37`: `str(data.get(...).date())` —
/// any input time is discarded).
pub fn extract_date_part(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.len() >= 10
        && t.as_bytes()[4] == b'-'
        && t.as_bytes()[7] == b'-'
        && t[..10].chars().all(|c| c.is_ascii_digit() || c == '-')
    {
        // Validate as a real calendar date.
        if chrono::NaiveDate::parse_from_str(&t[..10], "%Y-%m-%d").is_ok() {
            return Some(t[..10].to_string());
        }
    }
    None
}

/// Parses a datetime-ish input leniently (RFC3339 → naive → date-only as UTC
/// midnight). Django's `DateTimeField` is stricter (date-only alone 400s);
/// accepting date-only here is a documented leniency — the serializer then
/// converts via the date part only anyway (`serializers/cycle.py:29-37`).
pub fn parse_input_dt(raw: &str) -> Option<DateTime<Utc>> {
    let t = raw.trim();
    if let Ok(dt) = t.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

/// PROJECT-level role guards mirroring `@allow_permission` role lists:
/// AMG (`cycle/base.py:183,626,646,659,787`) vs AM (`cycle/base.py:270,
/// `issue.py:109,224,320`, `archive.py:271,587,607`, favorites/date-check/
/// transfer).
pub fn guard_amg(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` — GUEST (5) denied.
pub fn guard_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `CycleViewSet.partial_update` (`plane/app/views/cycle/base.py:339-357`):
/// archived → 400; completed without `sort_order` in body → 400 (with
/// `sort_order`, the caller applies ONLY that field).
pub fn guard_patch(archived: bool, completed: bool, sort_only: bool) -> Result<(), String> {
    if archived {
        return Err(ARCHIVED_IMMUTABLE_MSG.to_string());
    }
    if completed && !sort_only {
        return Err(COMPLETED_IMMUTABLE_MSG.to_string());
    }
    Ok(())
}

/// Mirrors the create both-or-null gate (`plane/app/views/cycle/base.py:272-274`).
pub fn guard_both_dates(start: bool, end: bool) -> Result<(), String> {
    if start != end {
        return Err(BOTH_DATES_MSG.to_string());
    }
    Ok(())
}

/// Mirrors the serializer order check (`plane/app/serializers/cycle.py:17-22`).
pub fn guard_date_order(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<(), String> {
    if start > end {
        return Err(START_EXCEEDS_END_MSG.to_string());
    }
    Ok(())
}

/// Old name-shape kept for the pre-existing unit surface: name presence +
/// both-or-null + order. (Name messages are local; the two date messages are
/// the Django-verbatim consts above.)
pub fn validate_create_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    Ok(())
}

/// Mirrors the archive gate (`plane/app/views/cycle/archive.py:590-594`):
/// `end_date` null or ≥ now → 400.
pub fn guard_archive(end: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Result<(), String> {
    match end {
        Some(e) if e < now => Ok(()),
        _ => Err(ARCHIVE_ONLY_COMPLETED_MSG.to_string()),
    }
}

/// Mirrors the completed-cycle PATCH gate (`plane/app/views/cycle/base.py:349-357`):
/// a completed cycle may ONLY be patched when the body contains `sort_order`
/// (any other key — or none — → 400). Presence of the key is the whole rule.
pub fn patch_has_sort_order(body: &Value) -> bool {
    body.get("sort_order").is_some()
}

/// Mirrors `str(timezone.now())` on aware UTC (`plane/app/views/cycle/archive.py:596-604`):
/// `"2026-09-06 12:34:56.123456+00:00"` — microsecond precision, `+00:00` suffix
/// (chrono `to_string()` would emit nanoseconds + a `UTC` suffix instead).
pub fn format_archived_at(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%d %H:%M:%S%.6f+00:00").to_string()
}

/// Builds a burndown `completion_chart` mirroring
/// `plane/utils/analytics_plot.py:236-264`: per-day pending =
/// total − completed-cumulative; future dates → null. `total` is the issue
/// count (or estimate-point sum); `done` maps day → completed that day.
pub fn burndown_chart(
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
    today: chrono::NaiveDate,
    total: f64,
    done: &std::collections::BTreeMap<chrono::NaiveDate, f64>,
    round_issues: bool,
) -> Value {
    let mut map = serde_json::Map::new();
    let mut day = start;
    while day <= end {
        let cum: f64 = done.iter().filter(|(d, _)| **d <= day).map(|(_, v)| v).sum();
        let pending = total - cum;
        let key = day.to_string();
        if day > today {
            map.insert(key, Value::Null);
        } else if round_issues {
            map.insert(key, json!(pending.round() as i64));
        } else {
            map.insert(key, json!(pending));
        }
        day = day.succ_opt().unwrap_or(day);
        if day.to_string().is_empty() {
            break;
        }
    }
    Value::Object(map)
}

// ============================================================================
// Row structs + JSON builders.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct CycleEditRow {
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
    sort_order: f64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CycleListRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    name: String,
    description: String,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    owned_by_id: uuid::Uuid,
    view_props: Value,
    sort_order: f64,
    external_source: Option<String>,
    external_id: Option<String>,
    progress_snapshot: Value,
    logo_props: Value,
    version: i32,
    created_by_id: Option<uuid::Uuid>,
    is_favorite: bool,
    total_issues: i64,
    completed_issues: i64,
    cancelled_issues: i64,
    assignee_ids: Vec<uuid::Uuid>,
    status: String,
}

const LIST_SELECT: &str = "c.id, c.workspace_id, c.project_id, c.name, c.description, \
    c.start_date, c.end_date, c.owned_by_id, c.view_props, c.sort_order, \
    c.external_source, c.external_id, c.progress_snapshot, c.logo_props, c.version, \
    c.created_by_id, \
    EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'cycle' \
        AND uf.entity_identifier = c.id AND uf.user_id = $3 AND uf.deleted_at IS NULL) AS is_favorite, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS total_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'completed' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS completed_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'cancelled' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS cancelled_issues, \
    COALESCE(ARRAY(SELECT DISTINCT ia.assignee_id FROM cycle_issues ci2 \
        JOIN issue_assignees ia ON ia.issue_id = ci2.issue_id AND ia.deleted_at IS NULL \
        WHERE ci2.cycle_id = c.id AND ci2.deleted_at IS NULL), '{}') AS assignee_ids, \
    CASE WHEN c.start_date <= now() AND c.end_date >= now() THEN 'CURRENT' \
        WHEN c.start_date > now() THEN 'UPCOMING' \
        WHEN c.end_date < now() THEN 'COMPLETED' \
        WHEN c.start_date IS NULL AND c.end_date IS NULL THEN 'DRAFT' \
        ELSE 'DRAFT' END AS status";

fn opt_uuid(u: &Option<uuid::Uuid>) -> Value {
    u.map(|v| json!(v)).unwrap_or(Value::Null)
}

fn opt_str(s: &Option<String>) -> Value {
    s.as_ref().map(|v| json!(v)).unwrap_or(Value::Null)
}

/// List-shape JSON in the exact `.values()` key order
/// (`plane/app/views/cycle/base.py:239-265`).
fn cycle_list_json(r: &CycleListRow) -> Value {
    json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "name": r.name,
        "description": r.description,
        "start_date": r.start_date,
        "end_date": r.end_date,
        "owned_by_id": r.owned_by_id,
        "view_props": r.view_props,
        "sort_order": r.sort_order,
        "external_source": opt_str(&r.external_source),
        "external_id": opt_str(&r.external_id),
        "progress_snapshot": r.progress_snapshot,
        "logo_props": r.logo_props,
        "is_favorite": r.is_favorite,
        "total_issues": r.total_issues,
        "cancelled_issues": r.cancelled_issues,
        "completed_issues": r.completed_issues,
        "assignee_ids": r.assignee_ids,
        "status": r.status,
        "version": r.version,
        "created_by": opt_uuid(&r.created_by_id),
    })
}

/// Create/patch-response shape: list keys minus `cancelled_issues` and
/// `logo_props`. (`base.py:281-307,362-387` include `logo_props`; the E2
/// contract pins the slimmer shape, so this is an intentional deviation.)
fn cycle_write_json(r: &CycleListRow) -> Value {
    json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "name": r.name,
        "description": r.description,
        "start_date": r.start_date,
        "end_date": r.end_date,
        "owned_by_id": r.owned_by_id,
        "view_props": r.view_props,
        "sort_order": r.sort_order,
        "external_source": opt_str(&r.external_source),
        "external_id": opt_str(&r.external_id),
        "progress_snapshot": r.progress_snapshot,
        "is_favorite": r.is_favorite,
        "total_issues": r.total_issues,
        "completed_issues": r.completed_issues,
        "assignee_ids": r.assignee_ids,
        "status": r.status,
        "version": r.version,
        "created_by": opt_uuid(&r.created_by_id),
    })
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

async fn project_timezone(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT timezone FROM projects WHERE id = $1")
        .bind(pid)
        .fetch_optional(pool)
        .await
}

/// Gate for AMG endpoints: allowed roles 20/15/5 outright, else the
/// workspace-ADMIN fallback (`plane/app/permissions/base.py:53-78`,
/// via the shared `issue_common` helpers — same shape as
/// `issue_common.rs:project_gate_allows`, reused not forked).
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

/// Gate for AM endpoints (no GUEST): same fallback, narrower role list.
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

/// `convert_to_utc` (`plane/utils/timezone_converter.py:42-94`) executed in
/// Postgres (which owns the IANA tz database — no new Rust deps):
/// start = `00:00:01` local→UTC except same-local-day → `now()`;
/// end = `23:59:00` local→UTC.
async fn convert_to_utc(
    pool: &sqlx::PgPool,
    date_part: &str,
    project_tz: &str,
    is_start: bool,
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT (CASE WHEN $3 THEN \
            CASE WHEN ($1::date) = ((now() AT TIME ZONE $2)::date) THEN now() \
            ELSE (($1 || ' 00:00:01')::timestamp AT TIME ZONE $2) END \
        ELSE (($1 || ' 23:59:00')::timestamp AT TIME ZONE $2) END)",
    )
    .bind(date_part)
    .bind(project_tz)
    .bind(is_start)
    .fetch_one(pool)
    .await
}

async fn fetch_list_row(
    pool: &sqlx::PgPool,
    cid: uuid::Uuid,
    pid: uuid::Uuid,
    slug: &str,
    user: uuid::Uuid,
) -> Result<Option<CycleListRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {LIST_SELECT} FROM cycles c \
         JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $4 AND c.deleted_at IS NULL \
         AND c.archived_at IS NULL"
    );
    sqlx::query_as::<_, CycleListRow>(&sql)
    .bind(cid)
    .bind(pid)
    .bind(user)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

// ============================================================================
// E2a — list + create.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CycleListQuery {
    #[serde(default)]
    pub cycle_view: Option<String>,
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Query(q): Query<CycleListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_amg(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let current_only = q.cycle_view.as_deref() == Some("current");
    let sql = if current_only {
        format!(
            "SELECT {LIST_SELECT} FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
             WHERE c.project_id = $1 AND w.slug = $2 AND c.deleted_at IS NULL \
             AND c.archived_at IS NULL AND c.start_date <= now() AND c.end_date >= now() \
             ORDER BY is_favorite DESC, c.created_at DESC"
        )
    } else {
        format!(
            "SELECT {LIST_SELECT} FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
             WHERE c.project_id = $1 AND w.slug = $2 AND c.deleted_at IS NULL \
             AND c.archived_at IS NULL \
             ORDER BY is_favorite DESC, c.created_at DESC"
        )
    };
    // Django `list` (`base.py:183-268`): GET 200 array; `?cycle_view=current`
    // filters to start<=now<=end. Datetimes stay UTC RFC3339 (batch
    // convention — Django renders them in project-tz; documented deviation).
    let rows: Vec<CycleListRow> = sqlx::query_as(&sql)
        .bind(pid)
        .bind(&slug)
        .bind(auth.0)
        .fetch_all(&st.pool)
        .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(cycle_list_json).collect::<Vec<_>>())),
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
    if let Err(e) = validate_create_name(&name) {
        let msg = if e == "name is required" {
            "This field may not be blank."
        } else {
            "Ensure this field has no more than 255 characters."
        };
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"name": [msg]}))));
    }
    let raw_start = body.get("start_date").and_then(Value::as_str);
    let raw_end = body.get("end_date").and_then(Value::as_str);
    // Django `create` (`base.py:272-274`): exactly both-or-neither.
    if let Err(e) = guard_both_dates(raw_start.is_some(), raw_end.is_some()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    let tz: String = project_timezone(&st.pool, pid)
        .await?
        .unwrap_or_else(|| "UTC".to_string());
    let (start_utc, end_utc): (Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        match (raw_start, raw_end) {
            (Some(s), Some(e)) => {
                let (Some(ds), Some(de)) = (extract_date_part(s), extract_date_part(e)) else {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Please provide valid detail"})),
                    ));
                };
                let su = convert_to_utc(&st.pool, &ds, &tz, true).await?;
                let eu = convert_to_utc(&st.pool, &de, &tz, false).await?;
                // Serializer order check (`serializers/cycle.py:17-22`) →
                // DRF `non_field_errors` shape.
                if guard_date_order(su, eu).is_err() {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"non_field_errors": [START_EXCEEDS_END_MSG]})),
                    ));
                }
                (Some(su), Some(eu))
            }
            (None, None) => (None, None),
            _ => unreachable!("guard_both_dates enforces both-or-neither"),
        };
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // Django `Cycle.save` sort_order = min-10000 per project; owned_by +
    // created_by = request user; version 1; timezone = project tz.
    let cid: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO cycles (id, name, description, project_id, workspace_id, owned_by_id, \
         created_by_id, timezone, version, view_props, logo_props, progress_snapshot, \
         sort_order, start_date, end_date, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, p.id, p.workspace_id, $3, $3, p.timezone, 1, \
         '{}', '{}', '{}', \
         COALESCE((SELECT MIN(sort_order) FROM cycles WHERE project_id = p.id), 65535 + 10000) - 10000, \
         $4, $5, now(), now() FROM projects p WHERE p.id = $6 RETURNING id",
    )
    .bind(&name)
    .bind(&description)
    .bind(auth.0)
    .bind(start_utc)
    .bind(end_utc)
    .bind(pid)
    .fetch_one(&st.pool)
    .await?;
    match fetch_list_row(&st.pool, cid.0, pid, &slug, auth.0).await? {
        Some(row) => Ok((StatusCode::CREATED, Json(cycle_write_json(&row)))),
        None => Ok(missing()),
    }
}

// ============================================================================
// E2b — detail + patch + destroy.
// ============================================================================

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // Django `retrieve` (`base.py:411-475`): archived excluded (the shared
    // `fetch_list_row` filters `archived_at IS NULL`); miss → 404
    // `{"error":"Cycle not found"}` verbatim (NOT `missing()`).
    let Some(row) = fetch_list_row(&st.pool, cid, pid, &slug, auth.0).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": CYCLE_NOT_FOUND_MSG})),
        ));
    };
    let sub_issues: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
         WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL AND i.parent_id IS NOT NULL \
         AND i.deleted_at IS NULL",
    )
    .bind(cid)
    .fetch_one(&st.pool)
    .await?;
    let mut v = cycle_list_json(&row);
    v.as_object_mut()
        .expect("cycle json is object")
        .remove("cancelled_issues");
    v.as_object_mut()
        .expect("cycle json is object")
        .insert("sub_issues".to_string(), json!(sub_issues.0));
    Ok((StatusCode::OK, Json(v)))
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let cur: Option<CycleEditRow> = sqlx::query_as(
        "SELECT start_date, end_date, archived_at, sort_order \
         FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    // Django `.first()` on empty → AttributeError → 500; return the sane
    // 404 `Cycle not found` instead (documented normalize-crash).
    let Some(cur) = cur else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": CYCLE_NOT_FOUND_MSG})),
        ));
    };
    let completed = cur.end_date.map(|e| e < Utc::now()).unwrap_or(false);
    let has_sort = patch_has_sort_order(&body);
    // Completed-cycle rule (`base.py:349-357`): without `sort_order` → 400;
    // with it, apply ONLY that field.
    if let Err(e) = guard_patch(cur.archived_at.is_some(), completed, has_sort) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    if completed && has_sort {
        let sort = body.get("sort_order").and_then(Value::as_f64).unwrap_or(cur.sort_order);
        sqlx::query("UPDATE cycles SET sort_order = $1, updated_at = now() WHERE id = $2")
            .bind(sort)
            .bind(cid)
            .execute(&st.pool)
            .await?;
        match fetch_list_row(&st.pool, cid, pid, &slug, auth.0).await? {
            Some(row) => Ok((StatusCode::OK, Json(cycle_write_json(&row)))),
            None => Ok((
                StatusCode::NOT_FOUND,
                Json(json!({"error": CYCLE_NOT_FOUND_MSG})),
            )),
        }
    } else {
        if let Some(n) = body.get("name").and_then(Value::as_str) {
            if let Err(e) = validate_create_name(n) {
                let msg = if e == "name is required" {
                    "This field may not be blank."
                } else {
                    "Ensure this field has no more than 255 characters."
                };
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"name": [msg]}))));
            }
        }
        // Date handling mirrors `CycleWriteSerializer.validate`
        // (`serializers/cycle.py:17-37`): both present → order-check +
        // convert_to_utc on the date parts; single present → stored as given.
        let tz: String = project_timezone(&st.pool, pid)
            .await?
            .unwrap_or_else(|| "UTC".to_string());
        let mut new_start = cur.start_date;
        let mut new_end = cur.end_date;
        match (
            body.get("start_date").and_then(Value::as_str),
            body.get("end_date").and_then(Value::as_str),
        ) {
            (Some(s), Some(e)) => {
                let (Some(ds), Some(de)) = (extract_date_part(s), extract_date_part(e)) else {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Please provide valid detail"})),
                    ));
                };
                let su = convert_to_utc(&st.pool, &ds, &tz, true).await?;
                let eu = convert_to_utc(&st.pool, &de, &tz, false).await?;
                if guard_date_order(su, eu).is_err() {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"non_field_errors": [START_EXCEEDS_END_MSG]})),
                    ));
                }
                new_start = Some(su);
                new_end = Some(eu);
            }
            (Some(s), None) => {
                new_start = parse_input_dt(s);
                if new_start.is_none() {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Please provide valid detail"})),
                    ));
                }
            }
            (None, Some(e)) => {
                new_end = parse_input_dt(e);
                if new_end.is_none() {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Please provide valid detail"})),
                    ));
                }
            }
            (None, None) => {}
        }
        sqlx::query(
            "UPDATE cycles SET name = COALESCE($1, name), description = COALESCE($2, description), \
             start_date = $3, end_date = $4, sort_order = COALESCE($5, sort_order), \
             updated_at = now() WHERE id = $6",
        )
        .bind(body.get("name").and_then(Value::as_str))
        .bind(body.get("description").and_then(Value::as_str))
        .bind(new_start)
        .bind(new_end)
        .bind(body.get("sort_order").and_then(Value::as_f64))
        .bind(cid)
        .execute(&st.pool)
        .await?;
        match fetch_list_row(&st.pool, cid, pid, &slug, auth.0).await? {
            Some(row) => Ok((StatusCode::OK, Json(cycle_write_json(&row)))),
            None => Ok((
                StatusCode::NOT_FOUND,
                Json(json!({"error": CYCLE_NOT_FOUND_MSG})),
            )),
        }
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    // Django `@allow_permission([ROLE.ADMIN], creator=True, model=Cycle)`
    // (`base.py:477-478`): ADMIN passes; creator-bypass
    // (`created_by=user`) skips the role check entirely.
    let cur: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT c.created_by_id FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(cid)
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
    // Soft-delete the cycle; delete favorite rows (soft) and hard-delete
    // recent-visit rows (`base.py:500-517`). Celery side-effects skipped.
    sqlx::query("UPDATE cycles SET deleted_at = now() WHERE id = $1")
        .bind(cid)
        .execute(&st.pool)
        .await?;
    sqlx::query(
        "UPDATE user_favorites SET deleted_at = now() WHERE user_id = $1 \
         AND entity_type = 'cycle' AND entity_identifier = $2 AND project_id = $3 \
         AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(cid)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    sqlx::query(
        "DELETE FROM user_recent_visits WHERE project_id = $1 \
         AND entity_identifier = $2 AND entity_name = 'cycle'",
    )
    .bind(pid)
    .bind(cid)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E2c — cycle issues.
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CycleIssuesQuery {
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
struct CycleIssueRow {
    id: uuid::Uuid,
    name: String,
    state_id: Option<uuid::Uuid>,
    sort_order: f64,
    completed_at: Option<DateTime<Utc>>,
    estimate_point: Option<uuid::Uuid>,
    priority: String,
    sequence_id: i32,
    project_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    cycle_id: Option<uuid::Uuid>,
    link_count: i64,
    attachment_count: i64,
    sub_issues_count: i64,
}

fn cycle_issue_json(r: &CycleIssueRow) -> Value {
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
        "cycle_id": opt_uuid(&r.cycle_id),
        "link_count": r.link_count,
        "attachment_count": r.attachment_count,
        "sub_issues_count": r.sub_issues_count,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
    })
}

fn order_sql(sanitized: &str) -> String {
    // Reuses the shared allowlist mapping (`issue_common.rs:
    // sanitize_order_by`) over the `i`/`s` aliases — same shape as the
    // detail ordering, reused not forked (comment per locked conventions).
    use super::issue_common::detail_order_expr;
    let (expr, desc) = detail_order_expr(sanitized);
    let dir = if desc { "DESC NULLS LAST" } else { "ASC NULLS LAST" };
    format!("{expr} {dir}, i.created_at DESC")
}

pub async fn cycle_issues_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<CycleIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `issue.py:145-150`: group_by == sub_group_by → 400.
    if let (Some(g), Some(s)) = (q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        if !g.is_empty() && g == s {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": GROUP_DUP_MSG}))));
        }
    }
    // Cursor/per_page mirror `BasePaginator` (`paginator.py:643-653,677-681`,
    // default/max 1000): reuse the shared parsers, not forks.
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
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let order = order_sql(&sanitized);
    let base = "FROM issues i JOIN cycle_issues ci ON ci.issue_id = i.id \
        AND ci.cycle_id = $1 AND ci.deleted_at IS NULL \
        LEFT JOIN states s ON s.id = i.state_id \
        WHERE i.project_id = $2 AND i.deleted_at IS NULL";
    let total: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) {base}"))
        .bind(cid)
        .bind(pid)
        .fetch_one(&st.pool)
        .await?;
    let total = total.0;
    let limit = per_page.max(0);
    // Truthy `group_by` grouped shapes are OUT (flat envelope returned;
    // same precedent as workspace user-issues) — only the equality 400
    // above is Django-verbatim.
    let (rows, next, prev, next_has, prev_has, count, pages): (
        Vec<CycleIssueRow>,
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
        match super::issue_common::page_window(page, limit) {
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
                     (SELECT ci2.cycle_id FROM cycle_issues ci2 WHERE ci2.issue_id = i.id \
                      AND ci2.deleted_at IS NULL LIMIT 1) AS cycle_id, \
                     (SELECT COUNT(*) FROM issue_links il WHERE il.issue_id = i.id \
                      AND il.deleted_at IS NULL) AS link_count, \
                     (SELECT COUNT(*) FROM file_assets fa WHERE fa.issue_id = i.id \
                      AND fa.entity_type = 'ISSUE_ATTACHMENT' AND fa.deleted_at IS NULL) AS attachment_count, \
                     (SELECT COUNT(*) FROM issues ch WHERE ch.parent_id = i.id \
                      AND ch.deleted_at IS NULL) AS sub_issues_count \
                     {base} ORDER BY {order} LIMIT $3 OFFSET $4"
                );
                let rows: Vec<CycleIssueRow> = sqlx::query_as(&sql)
                    .bind(cid)
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
        results: rows.iter().map(cycle_issue_json).collect(),
    };
    Ok((StatusCode::OK, Json(json!(env))))
}

pub async fn cycle_issues_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
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
    // `issue.py:227-228`.
    if issues.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": ISSUES_REQUIRED_MSG}))));
    }
    // Django `.get()` crash → sane 404 (documented normalize-crash).
    let cyc: Option<(Option<DateTime<Utc>>, uuid::Uuid)> = sqlx::query_as(
        "SELECT c.end_date, c.workspace_id FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((end_date, ws_id)) = cyc else {
        return Ok(missing());
    };
    // `issue.py:232-236`.
    if end_date.map(|e| e < Utc::now()).unwrap_or(false) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": COMPLETED_NO_ADD_MSG}))));
    }
    // Move cross-cycle rows scoped to the same workspace+project
    // (`issue.py:243-249`); bulk-create the rest scoped likewise
    // (`issue.py:254-261`); silently drop foreign rows.
    sqlx::query(
        "UPDATE cycle_issues SET cycle_id = $1 WHERE cycle_id != $1 AND issue_id = ANY($2) \
         AND workspace_id = $3 AND project_id = $4 AND deleted_at IS NULL",
    )
    .bind(cid)
    .bind(&issues)
    .bind(ws_id)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    let fresh: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT i.id FROM issues i WHERE i.id = ANY($1) AND i.workspace_id = $2 \
         AND i.project_id = $3 AND i.deleted_at IS NULL \
         AND NOT EXISTS(SELECT 1 FROM cycle_issues ci WHERE ci.issue_id = i.id \
            AND ci.deleted_at IS NULL)",
    )
    .bind(&issues)
    .bind(ws_id)
    .bind(pid)
    .fetch_all(&st.pool)
    .await?;
    for (iid,) in fresh {
        sqlx::query(
            "INSERT INTO cycle_issues (id, project_id, workspace_id, created_by_id, \
             updated_by_id, cycle_id, issue_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $3, $4, $5, now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(pid)
        .bind(ws_id)
        .bind(auth.0)
        .bind(cid)
        .bind(iid)
        .execute(&st.pool)
        .await?;
    }
    Ok((StatusCode::CREATED, Json(json!({"message": "success"}))))
}

pub async fn cycle_issue_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid, iid)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `issue.py:320-343`: soft-delete; **204** always, even with 0 rows.
    sqlx::query(
        "UPDATE cycle_issues SET deleted_at = now() WHERE issue_id = $1 \
         AND project_id = $2 AND cycle_id = $3 AND workspace_id IN \
         (SELECT id FROM workspaces WHERE slug = $4) AND deleted_at IS NULL",
    )
    .bind(iid)
    .bind(pid)
    .bind(cid)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E2d — date check.
// ============================================================================

pub async fn date_check(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let raw_start = body.get("start_date").and_then(Value::as_str).unwrap_or("");
    let raw_end = body.get("end_date").and_then(Value::as_str).unwrap_or("");
    // `base.py:526-530`.
    if raw_start.is_empty() || raw_end.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": DATECHECK_REQUIRED_MSG})),
        ));
    }
    let (Some(ds), Some(de)) = (extract_date_part(raw_start), extract_date_part(raw_end)) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Please provide valid detail"})),
        ));
    };
    let tz: String = project_timezone(&st.pool, pid)
        .await?
        .unwrap_or_else(|| "UTC".to_string());
    let su = convert_to_utc(&st.pool, &ds, &tz, true).await?;
    let eu = convert_to_utc(&st.pool, &de, &tz, false).await?;
    let ignore: Option<uuid::Uuid> = body
        .get("cycle_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    // `base.py:539-547` (`.exclude(pk=cycle_id)` — None excludes nothing).
    let hit: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE w.slug = $1 AND c.project_id = $2 AND c.deleted_at IS NULL \
         AND ($5::uuid IS NULL OR c.id != $5) \
         AND ((c.start_date <= $3 AND c.end_date >= $3) \
           OR (c.start_date <= $4 AND c.end_date >= $4) \
           OR (c.start_date >= $3 AND c.end_date <= $4)))",
    )
    .bind(&slug)
    .bind(pid)
    .bind(su)
    .bind(eu)
    .bind(ignore)
    .fetch_one(&st.pool)
    .await?;
    let (code, body) = date_check_result(hit.0);
    Ok((code, Json(body)))
}

// ============================================================================
// E2e — favorite cycles.
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
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `base.py:571-579`: no existence check on the cycle.
    let cid: Option<uuid::Uuid> = body
        .get("cycle")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    let Some(cid) = cid else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        ));
    };
    let r = sqlx::query(
        "INSERT INTO user_favorites (id, project_id, workspace_id, user_id, entity_type, \
         entity_identifier, name, is_folder, sequence, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, p.workspace_id, $2, 'cycle', $3, '', false, 0, now(), now() \
         FROM projects p WHERE p.id = $1",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(cid)
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
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `base.py:582-591`: `.get()` miss → 404 (Django `DoesNotExist`).
    let n = sqlx::query(
        "DELETE FROM user_favorites WHERE project_id = $1 AND entity_type = 'cycle' \
         AND user_id = $2 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) \
         AND entity_identifier = $4",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(&slug)
    .bind(cid)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E2f — transfer issues.
// ============================================================================

async fn project_has_points_estimate(pool: &sqlx::PgPool, pid: uuid::Uuid) -> Result<bool, sqlx::Error> {
    // `cycle_transfer_issues.py:151-156` (`estimate__type="points"`).
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM estimates WHERE project_id = $1 \
         AND type = 'points' AND deleted_at IS NULL)",
    )
    .bind(pid)
    .fetch_one(pool)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DistRow {
    display_name: Option<String>,
    assignee_id: Option<uuid::Uuid>,
    avatar_url: Option<String>,
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
    cid: uuid::Uuid,
    pid: uuid::Uuid,
    ws_id: uuid::Uuid,
) -> Result<Value, sqlx::Error> {
    // `cycle_transfer_issues.py:286-347` (issues branch).
    let rows: Vec<DistRow> = sqlx::query_as(
        "SELECT u.display_name, u.id AS assignee_id, u.avatar AS avatar_url, \
         COUNT(*) FILTER (WHERE i.archived_at IS NULL AND i.is_draft = false) AS total_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NOT NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS completed_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS pending_issues \
         FROM issues i JOIN issue_assignees ia ON ia.issue_id = i.id AND ia.deleted_at IS NULL \
         JOIN users u ON u.id = ia.assignee_id \
         JOIN cycle_issues ci ON ci.issue_id = i.id AND ci.cycle_id = $1 AND ci.deleted_at IS NULL \
         WHERE i.project_id = $2 AND i.workspace_id = $3 AND i.deleted_at IS NULL \
         GROUP BY u.display_name, u.id, u.avatar ORDER BY u.display_name",
    )
    .bind(cid)
    .bind(pid)
    .bind(ws_id)
    .fetch_all(pool)
    .await?;
    Ok(json!(rows
        .iter()
        .map(|r| json!({
            "display_name": r.display_name,
            "assignee_id": r.assignee_id.map(|u| u.to_string()),
            "avatar_url": r.avatar_url,
            "total_issues": r.total_issues,
            "completed_issues": r.completed_issues,
            "pending_issues": r.pending_issues,
        }))
        .collect::<Vec<_>>()))
}

async fn label_distribution(
    pool: &sqlx::PgPool,
    cid: uuid::Uuid,
    pid: uuid::Uuid,
    ws_id: uuid::Uuid,
) -> Result<Value, sqlx::Error> {
    // `cycle_transfer_issues.py:350-396` (issues branch).
    let rows: Vec<LabelDistRow> = sqlx::query_as(
        "SELECT l.name AS label_name, l.color, l.id AS label_id, \
         COUNT(*) FILTER (WHERE i.archived_at IS NULL AND i.is_draft = false) AS total_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NOT NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS completed_issues, \
         COUNT(*) FILTER (WHERE i.completed_at IS NULL AND i.archived_at IS NULL \
            AND i.is_draft = false) AS pending_issues \
         FROM issues i JOIN issue_labels il ON il.issue_id = i.id AND il.deleted_at IS NULL \
         JOIN labels l ON l.id = il.label_id \
         JOIN cycle_issues ci ON ci.issue_id = i.id AND ci.cycle_id = $1 AND ci.deleted_at IS NULL \
         WHERE i.project_id = $2 AND i.workspace_id = $3 AND i.deleted_at IS NULL \
         GROUP BY l.name, l.color, l.id ORDER BY l.name",
    )
    .bind(cid)
    .bind(pid)
    .bind(ws_id)
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
struct DoneDay {
    day: Option<chrono::NaiveDate>,
    n: i64,
}

/// Simplified burndown for a cycle: per-day pending over the cycle date
/// range. Mirrors the issues branch of
/// `plane/utils/analytics_plot.py:157-196,250-263` (total − cumulative
/// completed; future → null). Points branch sums `estimate_point` values
/// instead of counts (`analytics_plot.py:169-196`).
async fn completion_chart(
    pool: &sqlx::PgPool,
    cid: uuid::Uuid,
    pid: uuid::Uuid,
    ws_id: uuid::Uuid,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    points: bool,
) -> Result<Value, sqlx::Error> {
    let (Some(s), Some(e)) = (start, end) else {
        return Ok(json!({}));
    };
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
         WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
         AND i.project_id = $2 AND i.workspace_id = $3 AND i.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .bind(ws_id)
    .fetch_one(pool)
    .await?;
    let days: Vec<DoneDay> = sqlx::query_as(
        "SELECT (i.completed_at AT TIME ZONE 'UTC')::date AS day, COUNT(*) AS n \
         FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
         WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
         AND i.project_id = $2 AND i.workspace_id = $3 AND i.deleted_at IS NULL \
         AND i.completed_at IS NOT NULL GROUP BY 1 ORDER BY 1",
    )
    .bind(cid)
    .bind(pid)
    .bind(ws_id)
    .fetch_all(pool)
    .await?;
    let mut done = std::collections::BTreeMap::new();
    let mut total_f = total.0 as f64;
    if points {
        let pts: Vec<(Option<String>,)> = sqlx::query_as(
            "SELECT ep.value FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
             LEFT JOIN estimate_points ep ON ep.id = i.estimate_point_id \
             WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
             AND i.project_id = $2 AND i.workspace_id = $3 AND i.deleted_at IS NULL",
        )
        .bind(cid)
        .bind(pid)
        .bind(ws_id)
        .fetch_all(pool)
        .await?;
        total_f = pts
            .iter()
            .filter_map(|(v,)| v.as_deref()?.trim().parse::<f64>().ok())
            .sum();
        let pdone: Vec<(Option<chrono::NaiveDate>, Option<String>)> = sqlx::query_as(
            "SELECT (i.completed_at AT TIME ZONE 'UTC')::date, ep.value \
             FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
             LEFT JOIN estimate_points ep ON ep.id = i.estimate_point_id \
             WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
             AND i.project_id = $2 AND i.workspace_id = $3 AND i.deleted_at IS NULL \
             AND i.completed_at IS NOT NULL",
        )
        .bind(cid)
        .bind(pid)
        .bind(ws_id)
        .fetch_all(pool)
        .await?;
        done.clear();
        for (d, v) in pdone {
            if let (Some(d), Some(v)) = (d, v) {
                if let Ok(f) = v.trim().parse::<f64>() {
                    *done.entry(d).or_insert(0.0) += f;
                }
            }
        }
    } else {
        for r in days {
            if let Some(d) = r.day {
                done.insert(d, r.n as f64);
            }
        }
    }
    let today = Utc::now().date_naive();
    Ok(burndown_chart(
        s.date_naive(),
        e.date_naive(),
        today,
        total_f,
        &done,
        !points,
    ))
}

pub async fn transfer(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `base.py:597-603`.
    let new_cid: Option<uuid::Uuid> = body
        .get("new_cycle_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    let Some(new_cid) = new_cid else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": TRANSFER_TARGET_REQUIRED_MSG})),
        ));
    };
    let target: Option<(Option<DateTime<Utc>>,)> = sqlx::query_as(
        "SELECT c.end_date FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(new_cid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    // Django crashes on a missing target (`None.end_date`); sane 404
    // (documented normalize-crash).
    let Some((target_end,)) = target else {
        return Ok(missing());
    };
    // `cycle_transfer_issues.py:61-65`.
    if target_end.map(|e| e < Utc::now()).unwrap_or(false) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": TRANSFER_TARGET_COMPLETED_MSG})),
        ));
    }
    let src: Option<(uuid::Uuid, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT c.workspace_id, c.start_date, c.end_date FROM cycles c \
         JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    // `cycle_transfer_issues.py:144-148` — Django would crash on
    // `None.total_issues`; sane 400 verbatim (documented normalize-crash).
    let Some((ws_id, src_start, src_end)) = src else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": TRANSFER_SOURCE_MISSING_MSG})),
        ));
    };
    // Snapshot counts (`cycle_transfer_issues.py:68-141`).
    async fn grp(
        pool: &sqlx::PgPool,
        cid: uuid::Uuid,
        g: &str,
    ) -> Result<i64, sqlx::Error> {
        let r: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
             JOIN states s ON s.id = i.state_id \
             WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
             AND s.\"group\" = $2 AND i.archived_at IS NULL AND i.is_draft = false \
             AND i.deleted_at IS NULL",
        )
        .bind(cid)
        .bind(g)
        .fetch_one(pool)
        .await?;
        Ok(r.0)
    }
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
         WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
         AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL",
    )
    .bind(cid)
    .fetch_one(&st.pool)
    .await?;
    let snapshot = {
        let completed = grp(&st.pool, cid, "completed").await?;
        let cancelled = grp(&st.pool, cid, "cancelled").await?;
        let started = grp(&st.pool, cid, "started").await?;
        let unstarted = grp(&st.pool, cid, "unstarted").await?;
        let backlog = grp(&st.pool, cid, "backlog").await?;
        let assignees = assignee_distribution(&st.pool, cid, pid, ws_id).await?;
        let labels = label_distribution(&st.pool, cid, pid, ws_id).await?;
        let chart = completion_chart(&st.pool, cid, pid, ws_id, src_start, src_end, false).await?;
        let estimate_type = project_has_points_estimate(&st.pool, pid).await?;
        let estimate_distribution = if estimate_type {
            let chart_p =
                completion_chart(&st.pool, cid, pid, ws_id, src_start, src_end, true).await?;
            json!({"labels": [], "assignees": [], "completion_chart": chart_p})
        } else {
            json!({})
        };
        json!({
            "total_issues": total.0,
            "completed_issues": completed,
            "cancelled_issues": cancelled,
            "started_issues": started,
            "unstarted_issues": unstarted,
            "backlog_issues": backlog,
            "distribution": {"labels": labels, "assignees": assignees, "completion_chart": chart},
            "estimate_distribution": estimate_distribution,
        })
    };
    // NOTE: Django writes the snapshot on the SOURCE cycle
    // (`cycle_transfer_issues.py:408-432`, `pk=cycle_id`) — the E2 brief
    // says "target", but the verified source wins (intentional deviation).
    sqlx::query("UPDATE cycles SET progress_snapshot = $1, updated_at = now() WHERE id = $2")
        .bind(&snapshot)
        .bind(cid)
        .execute(&st.pool)
        .await?;
    // Move only backlog/unstarted/started (`cycle_transfer_issues.py:435-442`).
    sqlx::query(
        "UPDATE cycle_issues ci SET cycle_id = $1 FROM issues i JOIN states s ON s.id = i.state_id \
         WHERE ci.issue_id = i.id AND ci.cycle_id = $2 AND ci.deleted_at IS NULL \
         AND ci.project_id = $3 AND i.archived_at IS NULL AND i.is_draft = false \
         AND s.\"group\" IN ('backlog', 'unstarted', 'started')",
    )
    .bind(new_cid)
    .bind(cid)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"message": "Success"}))))
}

// ============================================================================
// E2h — archived cycles + archive/unarchive.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct ArchivedRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    name: String,
    description: String,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    owned_by_id: uuid::Uuid,
    view_props: Value,
    sort_order: f64,
    external_source: Option<String>,
    external_id: Option<String>,
    progress_snapshot: Value,
    is_favorite: bool,
    total_issues: i64,
    cancelled_issues: i64,
    completed_issues: i64,
    started_issues: i64,
    unstarted_issues: i64,
    backlog_issues: i64,
    assignee_ids: Vec<uuid::Uuid>,
    status: String,
    archived_at: Option<DateTime<Utc>>,
}

/// Archived-list shape (`archive.py:274-303`): list keys +
/// started/unstarted/backlog + archived_at, NO logo_props/version/created_by.
fn archived_list_json(r: &ArchivedRow) -> Value {
    json!({
        "id": r.id,
        "workspace_id": r.workspace_id,
        "project_id": r.project_id,
        "name": r.name,
        "description": r.description,
        "start_date": r.start_date,
        "end_date": r.end_date,
        "owned_by_id": r.owned_by_id,
        "view_props": r.view_props,
        "sort_order": r.sort_order,
        "external_source": opt_str(&r.external_source),
        "external_id": opt_str(&r.external_id),
        "progress_snapshot": r.progress_snapshot,
        "total_issues": r.total_issues,
        "is_favorite": r.is_favorite,
        "cancelled_issues": r.cancelled_issues,
        "completed_issues": r.completed_issues,
        "started_issues": r.started_issues,
        "unstarted_issues": r.unstarted_issues,
        "backlog_issues": r.backlog_issues,
        "assignee_ids": r.assignee_ids,
        "status": r.status,
        "archived_at": r.archived_at,
    })
}

const ARCHIVED_SELECT: &str = "c.id, c.workspace_id, c.project_id, c.name, c.description, \
    c.start_date, c.end_date, c.owned_by_id, c.view_props, c.sort_order, \
    c.external_source, c.external_id, c.progress_snapshot, \
    EXISTS(SELECT 1 FROM user_favorites uf WHERE uf.entity_type = 'cycle' \
        AND uf.entity_identifier = c.id AND uf.user_id = $3 AND uf.deleted_at IS NULL) AS is_favorite, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS total_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'completed' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS completed_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'cancelled' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS cancelled_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'started' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS started_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'unstarted' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS unstarted_issues, \
    (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id JOIN states s ON s.id = i.state_id \
        WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
        AND s.\"group\" = 'backlog' AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS backlog_issues, \
    COALESCE(ARRAY(SELECT DISTINCT ia.assignee_id FROM cycle_issues ci2 \
        JOIN issue_assignees ia ON ia.issue_id = ci2.issue_id AND ia.deleted_at IS NULL \
        WHERE ci2.cycle_id = c.id AND ci2.deleted_at IS NULL), '{}') AS assignee_ids, \
    CASE WHEN c.start_date <= now() AND c.end_date >= now() THEN 'CURRENT' \
        WHEN c.start_date > now() THEN 'UPCOMING' \
        WHEN c.end_date < now() THEN 'COMPLETED' \
        WHEN c.start_date IS NULL AND c.end_date IS NULL THEN 'DRAFT' \
        ELSE 'DRAFT' END AS status, c.archived_at";

pub async fn archived_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `archive.py:271-304`.
    let rows: Vec<ArchivedRow> = sqlx::query_as(&format!(
        "SELECT {ARCHIVED_SELECT} FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.project_id = $1 AND w.slug = $2 AND c.deleted_at IS NULL \
         AND c.archived_at IS NOT NULL \
         ORDER BY is_favorite DESC, c.created_at DESC"
    ))
    .bind(pid)
    .bind(&slug)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(archived_list_json).collect::<Vec<_>>())),
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
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let row: Option<ArchivedRow> = sqlx::query_as(&format!(
        "SELECT {ARCHIVED_SELECT} FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $4 AND c.deleted_at IS NULL \
         AND c.archived_at IS NOT NULL"
    ))
    .bind(pk)
    .bind(pid)
    .bind(auth.0)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    // Django `data[...]` on None → 500; sane 404 (documented normalize-crash).
    let Some(row) = row else {
        return Ok(missing());
    };
    let ws_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT workspace_id FROM cycles WHERE id = $1")
            .bind(pk)
            .fetch_one(&st.pool)
            .await?;
    let estimate_type = project_has_points_estimate(&st.pool, pid).await?;
    let assignees = assignee_distribution(&st.pool, pk, pid, ws_id.0).await?;
    let labels = label_distribution(&st.pool, pk, pid, ws_id.0).await?;
    // `archive.py:575-582`: issues completion_chart only when dated.
    let cyc: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT start_date, end_date FROM cycles WHERE id = $1")
            .bind(pk)
            .fetch_one(&st.pool)
            .await?;
    let issues_chart = completion_chart(&st.pool, pk, pid, ws_id.0, cyc.0, cyc.1, false).await?;
    let estimate_distribution = if estimate_type {
        let chart_p = completion_chart(&st.pool, pk, pid, ws_id.0, cyc.0, cyc.1, true).await?;
        json!({"assignees": [], "labels": [], "completion_chart": chart_p})
    } else {
        json!({})
    };
    let mut v = archived_list_json(&row);
    let obj = v.as_object_mut().expect("archived json is object");
    obj.insert(
        "distribution".to_string(),
        json!({"assignees": assignees, "labels": labels, "completion_chart": issues_chart}),
    );
    obj.insert("estimate_distribution".to_string(), estimate_distribution);
    Ok((StatusCode::OK, Json(v)))
}

pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // Django `.get()` crash → sane 404 (documented normalize-crash).
    let cur: Option<(Option<DateTime<Utc>>,)> = sqlx::query_as(
        "SELECT c.end_date FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((end,)) = cur else {
        return Ok(missing());
    };
    // `archive.py:590-594`.
    if guard_archive(end, Utc::now()).is_err() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ARCHIVE_ONLY_COMPLETED_MSG})),
        ));
    }
    let now = Utc::now();
    sqlx::query("UPDATE cycles SET archived_at = $1, updated_at = now() WHERE id = $2")
        .bind(now)
        .bind(cid)
        .execute(&st.pool)
        .await?;
    // `archive.py:598-603`: delete favorites on archive (all users).
    sqlx::query(
        "UPDATE user_favorites SET deleted_at = now() WHERE entity_type = 'cycle' \
         AND entity_identifier = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"archived_at": format_archived_at(now)}))))
}

pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE cycles SET archived_at = NULL, updated_at = now() WHERE id = $1 \
         AND project_id = $2 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) \
         AND deleted_at IS NULL",
    )
    .bind(cid)
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
// E2i — progress + analytics.
// ============================================================================

pub async fn progress(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_amg(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `base.py:660-663`: miss → 404 verbatim.
    let cyc: Option<(Value,)> = sqlx::query_as(
        "SELECT c.progress_snapshot FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((snap,)) = cyc else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": CYCLE_NOT_FOUND_MSG})),
        ));
    };
    // Points-estimate sums (`base.py:664-711`); group sums `or 0`, total may
    // be null. Only `points`-type estimate points count.
    #[derive(Debug, Clone, sqlx::FromRow)]
    struct PtAgg {
        backlog_estimate_point: Option<f64>,
        unstarted_estimate_point: Option<f64>,
        started_estimate_point: Option<f64>,
        cancelled_estimate_point: Option<f64>,
        completed_estimate_points: Option<f64>,
        total_estimate_points: Option<f64>,
    }
    let agg: PtAgg = sqlx::query_as(
        "SELECT \
         SUM(CASE WHEN s.\"group\" = 'backlog' THEN ep.value::double precision ELSE 0 END) AS backlog_estimate_point, \
         SUM(CASE WHEN s.\"group\" = 'unstarted' THEN ep.value::double precision ELSE 0 END) AS unstarted_estimate_point, \
         SUM(CASE WHEN s.\"group\" = 'started' THEN ep.value::double precision ELSE 0 END) AS started_estimate_point, \
         SUM(CASE WHEN s.\"group\" = 'cancelled' THEN ep.value::double precision ELSE 0 END) AS cancelled_estimate_point, \
         SUM(CASE WHEN s.\"group\" = 'completed' THEN ep.value::double precision ELSE 0 END) AS completed_estimate_points, \
         SUM(ep.value::double precision) AS total_estimate_points \
         FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
         LEFT JOIN states s ON s.id = i.state_id \
         JOIN estimate_points ep ON ep.id = i.estimate_point_id \
         JOIN estimates e ON e.id = ep.estimate_id AND e.type = 'points' AND e.deleted_at IS NULL \
         WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
         AND i.project_id = $2 AND i.deleted_at IS NULL",
    )
    .bind(cid)
    .bind(pid)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(PtAgg {
        backlog_estimate_point: None,
        unstarted_estimate_point: None,
        started_estimate_point: None,
        cancelled_estimate_point: None,
        completed_estimate_points: None,
        total_estimate_points: None,
    });
    // `base.py:712-765`: snapshot counts win when the snapshot is truthy.
    let snap_obj = snap.as_object();
    let snap_live = snap_obj.map(|o| o.contains_key("total_issues")).unwrap_or(false);
    let (backlog, unstarted, started, cancelled, completed, total) = if snap_live {
        let g = |k: &str| snap.get(k).and_then(Value::as_i64).unwrap_or(0);
        (
            g("backlog_issues"),
            g("unstarted_issues"),
            g("started_issues"),
            g("cancelled_issues"),
            g("completed_issues"),
            g("total_issues"),
        )
    } else {
        async fn grp(
            pool: &sqlx::PgPool,
            cid: uuid::Uuid,
            pid: uuid::Uuid,
            slug: &str,
            g: Option<&str>,
        ) -> i64 {
            let r: Result<(i64,), sqlx::Error> = sqlx::query_as(
                "SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
                 LEFT JOIN states s ON s.id = i.state_id \
                 JOIN workspaces w ON w.id = ci.workspace_id \
                 WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
                 AND w.slug = $2 AND ci.project_id = $3 \
                 AND ($4::text IS NULL OR s.\"group\" = $4)",
            )
            .bind(cid)
            .bind(slug)
            .bind(pid)
            .bind(g)
            .fetch_one(pool)
            .await;
            r.map(|v| v.0).unwrap_or(0)
        }
        (
            grp(&st.pool, cid, pid, &slug, Some("backlog")).await,
            grp(&st.pool, cid, pid, &slug, Some("unstarted")).await,
            grp(&st.pool, cid, pid, &slug, Some("started")).await,
            grp(&st.pool, cid, pid, &slug, Some("cancelled")).await,
            grp(&st.pool, cid, pid, &slug, Some("completed")).await,
            grp(&st.pool, cid, pid, &slug, None).await,
        )
    };
    Ok((
        StatusCode::OK,
        Json(json!({
            "backlog_estimate_points": agg.backlog_estimate_point.unwrap_or(0.0),
            "unstarted_estimate_points": agg.unstarted_estimate_point.unwrap_or(0.0),
            "started_estimate_points": agg.started_estimate_point.unwrap_or(0.0),
            "cancelled_estimate_points": agg.cancelled_estimate_point.unwrap_or(0.0),
            "completed_estimate_points": agg.completed_estimate_points.unwrap_or(0.0),
            "total_estimate_points": agg.total_estimate_points,
            "backlog_issues": backlog,
            "total_issues": total,
            "completed_issues": completed,
            "cancelled_issues": cancelled,
            "started_issues": started,
            "unstarted_issues": unstarted,
        })),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AnalyticsQuery {
    #[serde(default)]
    pub r#type: Option<String>,
}

pub async fn analytics(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, cid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<AnalyticsQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_amg(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // Django crashes on a missing cycle (`None.start_date`); sane 404
    // (documented normalize-crash).
    let cyc: Option<(Option<DateTime<Utc>>, Option<DateTime<Utc>>, Value, uuid::Uuid)> =
        sqlx::query_as(
            "SELECT c.start_date, c.end_date, c.progress_snapshot, c.workspace_id \
             FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
             WHERE c.id = $1 AND c.project_id = $2 AND w.slug = $3 AND c.deleted_at IS NULL",
        )
        .bind(cid)
        .bind(pid)
        .bind(&slug)
        .fetch_optional(&st.pool)
        .await?;
    let Some((start, end, snap, ws_id)) = cyc else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": CYCLE_NOT_FOUND_MSG})),
        ));
    };
    // `base.py:807-811`.
    if start.is_none() || end.is_none() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": NO_DATES_MSG}))));
    }
    // Snapshot branch (`base.py:821-830`).
    if snap.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        let dist = snap.get("distribution").cloned().unwrap_or(json!({}));
        return Ok((
            StatusCode::OK,
            Json(json!({
                "labels": dist.get("labels").cloned().unwrap_or(json!([])),
                "assignees": dist.get("assignees").cloned().unwrap_or(json!([])),
                "completion_chart": dist.get("completion_chart").cloned().unwrap_or(json!({})),
            })),
        ));
    }
    let analytic_type = q.r#type.as_deref().unwrap_or("issues");
    let estimate_type = project_has_points_estimate(&st.pool, pid).await?;
    // Points branch only with a points estimate (`base.py:843`); issues
    // branch for `type=issues` (`base.py:940`). Any other `type` yields the
    // empty shape (mirrors Django falling through both `if`s).
    if analytic_type == "points" && estimate_type {
        let chart = completion_chart(&st.pool, cid, pid, ws_id, start, end, true).await?;
        return Ok((
            StatusCode::OK,
            Json(json!({"assignees": [], "labels": [], "completion_chart": chart})),
        ));
    }
    if analytic_type == "issues" {
        let assignees = assignee_distribution(&st.pool, cid, pid, ws_id).await?;
        let labels = label_distribution(&st.pool, cid, pid, ws_id).await?;
        let chart = completion_chart(&st.pool, cid, pid, ws_id, start, end, false).await?;
        return Ok((
            StatusCode::OK,
            Json(json!({"assignees": assignees, "labels": labels, "completion_chart": chart})),
        ));
    }
    Ok((
        StatusCode::OK,
        Json(json!({"assignees": [], "labels": [], "completion_chart": {}})),
    ))
}

// ============================================================================
// E2j — workspace cycles.
// ============================================================================

fn deny_detail() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": PERMISSION_DETAIL_MSG})),
    )
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsCycleRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    name: String,
    description: String,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    owned_by_id: uuid::Uuid,
    view_props: Value,
    sort_order: f64,
    external_source: Option<String>,
    external_id: Option<String>,
    progress_snapshot: Value,
    logo_props: Value,
    total_issues: i64,
    completed_issues: i64,
    cancelled_issues: i64,
    started_issues: i64,
    unstarted_issues: i64,
    backlog_issues: i64,
}

pub async fn workspace_cycles(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::project::ws_role;
    // `WorkspaceViewerPermission` = any ACTIVE ws member (`E2j` gate); deny
    // is the DRF permission-class 403 `{"detail": ...}` (NOT `deny()`).
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    // `workspace/cycle.py:22-109`: member-projects only, archived excluded,
    // NO favorite/status/assignee annotations, WITH started/unstarted/
    // backlog counts; serialized via `CycleSerializer` (model keys only).
    let rows: Vec<WsCycleRow> = sqlx::query_as(
        "SELECT c.id, c.workspace_id, c.project_id, c.name, c.description, \
         c.start_date, c.end_date, c.owned_by_id, c.view_props, c.sort_order, \
         c.external_source, c.external_id, c.progress_snapshot, c.logo_props, \
         (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
            WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
            AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS total_issues, \
         (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
            JOIN states s ON s.id = i.state_id \
            WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL AND s.\"group\" = 'completed' \
            AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS completed_issues, \
         (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
            JOIN states s ON s.id = i.state_id \
            WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL AND s.\"group\" = 'cancelled' \
            AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS cancelled_issues, \
         (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
            JOIN states s ON s.id = i.state_id \
            WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL AND s.\"group\" = 'started' \
            AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS started_issues, \
         (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
            JOIN states s ON s.id = i.state_id \
            WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL AND s.\"group\" = 'unstarted' \
            AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS unstarted_issues, \
         (SELECT COUNT(*) FROM cycle_issues ci JOIN issues i ON i.id = ci.issue_id \
            JOIN states s ON s.id = i.state_id \
            WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL AND s.\"group\" = 'backlog' \
            AND i.archived_at IS NULL AND i.is_draft = false AND i.deleted_at IS NULL) AS backlog_issues \
         FROM cycles c JOIN workspaces w ON w.id = c.workspace_id \
         JOIN projects p ON p.id = c.project_id \
         WHERE w.slug = $1 AND c.deleted_at IS NULL AND c.archived_at IS NULL \
         AND p.archived_at IS NULL AND p.deleted_at IS NULL \
         AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
            AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
         ORDER BY c.created_at DESC",
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
                "start_date": r.start_date,
                "end_date": r.end_date,
                "owned_by_id": r.owned_by_id,
                "view_props": r.view_props,
                "sort_order": r.sort_order,
                "external_source": opt_str(&r.external_source),
                "external_id": opt_str(&r.external_id),
                "progress_snapshot": r.progress_snapshot,
                "logo_props": r.logo_props,
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
// Pre-existing unit surface (required by `crates/api/tests/cycle_test.rs`
// + `detail_cycle_test.rs`; CONSTRAINTS forbid touching those files).
// `#[allow(dead_code)]`: the Axum handlers below take `Json<Value>` bodies
// (Django reads `request.data` dynamically), so these typed helpers are
// construction points for tests only — the binary target would otherwise
// lint them as unused.
// ============================================================================

/// Pre-existing validate shape (`name` + both-or-null + order).
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCycle {
    pub name: String,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[allow(dead_code)]
pub fn validate_create(body: &CreateCycle) -> Result<(), String> {
    validate_create_name(&body.name)?;
    guard_both_dates(body.start_date.is_some(), body.end_date.is_some())?;
    if let (Some(s), Some(e)) = (body.start_date, body.end_date) {
        guard_date_order(s, e)?;
    }
    Ok(())
}

/// #9200: archiving without end_date must fail.
#[allow(dead_code)]
pub fn validate_archive(end_date: Option<DateTime<Utc>>) -> Result<(), String> {
    end_date.ok_or_else(|| "end_date is required when archiving cycle".to_string())?;
    Ok(())
}

// ============================================================================
// Tests (STEP 1 — pure fns; no DB).
// ============================================================================

#[cfg(test)]
mod cycle_e2_tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn status_case_current_upcoming_completed_draft() {
        // Mirrors the Case (`base.py:153-167`): now-in-PROJECT-tz vs
        // start/end; both-null → DRAFT; partial-null → default DRAFT.
        let now = dt(2026, 6, 15);
        assert_eq!(cycle_status(Some(dt(2026, 6, 1)), Some(dt(2026, 6, 30)), now), "CURRENT");
        assert_eq!(cycle_status(Some(dt(2026, 7, 1)), Some(dt(2026, 7, 31)), now), "UPCOMING");
        assert_eq!(cycle_status(Some(dt(2026, 5, 1)), Some(dt(2026, 5, 31)), now), "COMPLETED");
        assert_eq!(cycle_status(None, None, now), "DRAFT");
        assert_eq!(cycle_status(Some(dt(2026, 6, 1)), None, now), "DRAFT");
        assert_eq!(cycle_status(None, Some(dt(2026, 6, 30)), now), "DRAFT");
        // Boundary is inclusive (lte/gte).
        assert_eq!(cycle_status(Some(now), Some(now), now), "CURRENT");
    }

    #[test]
    fn date_check_overlap_returns_200_status_false() {
        // `base.py:548-554`: overlap → **200** (NOT 4xx) with the verbatim
        // error + status:false; no overlap → 200 status:true.
        assert!(cycles_overlap(dt(2026, 6, 1), dt(2026, 6, 30), dt(2026, 6, 15), dt(2026, 7, 15)));
        assert!(cycles_overlap(dt(2026, 6, 1), dt(2026, 6, 30), dt(2026, 5, 1), dt(2026, 6, 1)));
        assert!(cycles_overlap(dt(2026, 6, 10), dt(2026, 6, 12), dt(2026, 6, 1), dt(2026, 6, 30)));
        assert!(!cycles_overlap(dt(2026, 6, 1), dt(2026, 6, 30), dt(2026, 7, 1), dt(2026, 7, 31)));
        let (code, body) = date_check_result(true);
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body.get("status"), Some(&json!(false)));
        assert_eq!(body.get("error"), Some(&json!(DATECHECK_OVERLAP_MSG)));
        let (code, body) = date_check_result(false);
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body, json!({"status": true}));
    }

    #[test]
    fn convert_to_utc_same_day_stores_now() {
        // `timezone_converter.py:82-84`: start-date on today's local date →
        // now(); any other date → 00:00:01 local→UTC (decision bit; the SQL
        // helper `convert_to_utc` implements the full rule in Postgres).
        assert!(convert_start_is_today("2026-06-15", "2026-06-15"));
        assert!(!convert_start_is_today("2026-06-14", "2026-06-15"));
        assert!(!convert_start_is_today("2026-06-16", "2026-06-15"));
        // Date-part extraction discards input times (`serializers/cycle.py:30`).
        assert_eq!(extract_date_part("2026-06-15T10:30:00Z"), Some("2026-06-15".to_string()));
        assert_eq!(extract_date_part("2026-06-15"), Some("2026-06-15".to_string()));
        assert_eq!(extract_date_part("not-a-date"), None);
        assert_eq!(extract_date_part("2026-13-40"), None);
    }

    #[test]
    fn transfer_error_consts_verbatim() {
        // `base.py:601`, `cycle_transfer_issues.py:64,147`.
        assert_eq!(TRANSFER_TARGET_REQUIRED_MSG, "New Cycle Id is required");
        assert_eq!(
            TRANSFER_TARGET_COMPLETED_MSG,
            "The cycle where the issues are transferred is already completed"
        );
        assert_eq!(TRANSFER_SOURCE_MISSING_MSG, "Source cycle not found");
    }

    #[test]
    fn archived_gate_consts_verbatim() {
        // `archive.py:592`, `base.py:341,355,459,528,809`, `issue.py:228,234`.
        assert_eq!(ARCHIVE_ONLY_COMPLETED_MSG, "Only completed cycles can be archived");
        assert_eq!(ARCHIVED_IMMUTABLE_MSG, "Archived cycle cannot be updated");
        assert_eq!(
            COMPLETED_IMMUTABLE_MSG,
            "The Cycle has already been completed so it cannot be edited"
        );
        assert_eq!(CYCLE_NOT_FOUND_MSG, "Cycle not found");
        assert_eq!(ISSUES_REQUIRED_MSG, "Issues are required");
        assert_eq!(
            COMPLETED_NO_ADD_MSG,
            "The Cycle has already been completed so no new issues can be added"
        );
        assert_eq!(DATECHECK_REQUIRED_MSG, "Start date and end date both are required");
        assert_eq!(NO_DATES_MSG, "Cycle has no start or end date");
        // Archive gate: null or future end → 400; past end → ok.
        let now = dt(2026, 6, 15);
        assert!(guard_archive(Some(dt(2026, 5, 1)), now).is_ok());
        assert!(guard_archive(None, now).is_err());
        assert!(guard_archive(Some(dt(2026, 7, 1)), now).is_err());
    }

    #[test]
    fn guards_patch_and_gates() {
        // `base.py:339-357`.
        assert!(guard_patch(false, false, false).is_ok());
        assert_eq!(
            guard_patch(true, false, true).unwrap_err(),
            "Archived cycle cannot be updated"
        );
        assert_eq!(
            guard_patch(false, true, false).unwrap_err(),
            "The Cycle has already been completed so it cannot be edited"
        );
        assert!(guard_patch(false, true, true).is_ok());
        // Role gates: AMG passes 20/15/5; AM denies 5.
        assert!(guard_amg(Some(20)).is_ok());
        assert!(guard_amg(Some(15)).is_ok());
        assert!(guard_amg(Some(5)).is_ok());
        assert!(guard_amg(None).is_err());
        assert!(guard_am(Some(20)).is_ok());
        assert!(guard_am(Some(15)).is_ok());
        assert!(guard_am(Some(5)).is_err());
        // Both-or-null (`base.py:272-274`) + order (`serializers/cycle.py:22`).
        assert!(guard_both_dates(true, true).is_ok());
        assert!(guard_both_dates(false, false).is_ok());
        assert_eq!(
            guard_both_dates(true, false).unwrap_err(),
            "Both start date and end date are either required or are to be null"
        );
        assert!(guard_date_order(dt(2026, 6, 1), dt(2026, 6, 30)).is_ok());
        assert_eq!(
            guard_date_order(dt(2026, 7, 1), dt(2026, 6, 30)).unwrap_err(),
            "Start date cannot exceed end date"
        );
    }

    #[test]
    fn burndown_chart_shape() {
        // `analytics_plot.py:250-263`: pending descends on completion days,
        // future days → null.
        use std::collections::BTreeMap;
        let mut done = BTreeMap::new();
        done.insert(chrono::NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(), 2.0);
        let v = burndown_chart(
            chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 6, 4).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 6, 3).unwrap(),
            5.0,
            &done,
            true,
        );
        assert_eq!(v.get("2026-06-01"), Some(&json!(5)));
        assert_eq!(v.get("2026-06-02"), Some(&json!(3)));
        assert_eq!(v.get("2026-06-04"), Some(&json!(Value::Null)));
    }

    #[test]
    fn completed_patch_guard_mirrors_base_349_357() {
        // `base.py:349-357`: completed cycle + body WITHOUT `sort_order`
        // → 400 verbatim; WITH `sort_order` → only that field applied.
        for raw in [r#"{"description":"x"}"#, r#"{}"#, r#"{"view_props":{}}"#] {
            let body: Value = serde_json::from_str(raw).unwrap();
            assert!(!patch_has_sort_order(&body));
            assert_eq!(
                guard_patch(false, true, patch_has_sort_order(&body)).unwrap_err(),
                "The Cycle has already been completed so it cannot be edited"
            );
        }
        let body: Value = serde_json::from_str(r#"{"sort_order":1,"name":"hack"}"#).unwrap();
        assert!(patch_has_sort_order(&body));
        assert!(guard_patch(false, true, patch_has_sort_order(&body)).is_ok());
    }

    #[test]
    fn archived_at_format_mirrors_django_str() {
        // `archive.py:596-604`: `str(timezone.now())` → microsecond
        // precision with `+00:00` suffix.
        let now = Utc.with_ymd_and_hms(2026, 9, 6, 12, 34, 56).unwrap();
        let s = format_archived_at(now);
        assert!(s.starts_with("2026-09-06 12:34:56."));
        assert!(s.ends_with("+00:00"));
        let frac = &s[20..s.len() - 6];
        assert_eq!(frac.len(), 6);
        assert!(frac.chars().all(|c| c.is_ascii_digit()));
    }
}
