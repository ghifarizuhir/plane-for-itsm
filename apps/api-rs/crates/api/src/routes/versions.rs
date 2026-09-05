use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::project::{deny, missing};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{
    build_cursor, fetch_project_member_role, is_workspace_admin, next_cursor_str,
    parse_python_int, project_gate_allows, total_pages,
};
use super::issue_query::GENERIC_500_MSG;

/// Issue snapshot + description versions — parity with Django
/// `IssueVersionEndpoint` (`plane/app/views/issue/version.py:27-74`,
/// `plane/app/urls/issue.py:256-265`), `WorkItemDescriptionVersionEndpoint`
/// (`version.py:77-144`, `urls/issue.py:266-275`) and
/// `IntakeWorkItemDescriptionVersionEndpoint`
/// (`plane/app/views/intake/base.py:572-640`,
/// `plane/app/urls/intake.py:56-65`). Celery/activity side-effects skipped
/// (Batch C precedent — these endpoints are read-only GETs).
///
/// Live columns verified 2026-09-06 via
/// `docker exec plane-db psql -U plane -d plane -c "\d issue_versions"` /
/// `"\d issue_description_versions"` (the intake twin reads the SAME
/// `issue_description_versions` table — Django defines no separate intake
/// version table, `intake/base.py:603-608`):
/// - `issue_versions`: id, parent, state, estimate_point, name, priority,
///   start_date, target_date, assignees (`uuid[] NOT NULL`),
///   sequence_id, labels (`uuid[] NOT NULL`), sort_order, completed_at,
///   archived_at, is_draft, external_source, external_id, `type`, cycle,
///   modules (`uuid[] NOT NULL`), properties/meta (`jsonb NOT NULL`),
///   last_saved_at, owned_by_id (`NOT NULL`), issue/project/workspace/
///   created_by/updated_by/activity FKs, audit columns.
/// - `issue_description_versions`: id, description_binary (`bytea`,
///   nullable), description_html (`text NOT NULL`), description_stripped,
///   description_json (`jsonb NOT NULL`), last_saved_at, owned_by_id
///   (`NOT NULL`), issue/project/workspace/created_by/updated_by FKs,
///   audit columns.
///
/// Locked semantics (plan D6):
/// - Lists are cursor-paginated 10-key rows (`id,workspace,project,issue,
///   last_saved_at,owned_by,created_at,updated_at,created_by,updated_by`,
///   `version.py:48-59,120-131`, `intake/base.py:615-626`) via Django's
///   `paginate()` (`plane/utils/global_paginator.py:33-86`) — envelope keys
///   `prev_cursor,cursor,next_cursor,prev_page_results,next_page_results,
///   page_count,total_results,total_pages,results` (FE
///   `TDescriptionVersionsListResponse`,
///   `packages/types/src/description_version.ts:25-35`, pins the same set).
/// - D6a single is the full snapshot (`serializers/issue.py:984-1016` —
///   Django lists `name` TWICE; DRF dict semantics keep the FIRST position,
///   so it is emitted ONCE after `estimate_point`); D6b/D6c singles are the
///   14-key description detail (`serializers/issue.py:1023-1038`).
/// - Guest-403 on BOTH description twins iff (role == GUEST AND NOT
///   `project.guest_view_all_features` AND NOT own issue) → 403
///   `{"error": "You are not allowed to view this issue"}`
///   (`version.py:91-105`, `intake/base.py:586-600`); D6a has NO such gate.
/// - Work-items path ONLY for description-versions — Django defines NO
///   `issues/:id/description-versions/` route, so none is added here.
/// - Gate everywhere: `@allow_permission([ADMIN, MEMBER, GUEST])`
///   (`version.py:36,86`, `intake/base.py:581`) at PROJECT level via the
///   shared helpers (roles 20/15/5 + workspace-ADMIN fallback).
///
/// Deviations (documented, reviewer-adjudicable):
/// - Datetimes serialize RFC3339 UTC (chrono, batch convention) instead of
///   DRF's per-user-timezone conversion (`user_timezone_converter`,
///   `version.py:28-34`).
/// - `description_binary` (`bytea`) renders as a base64 STANDARD string
///   (null stays null). Wire evidence: FE expects `string | null`
///   (`description_version.ts:18-23`); in-repo writers treat the field as
///   base64-on-the-wire (`test_copy_s3_objects.py:80`
///   `base64.b64encode(...).decode()`, `copy_s3_object.py:149`
///   `base64.b64decode`, `PageBinaryUpdateSerializer` "base64-encoded
///   binary data", `serializers/page.py:180-198`).
/// - Cursor errors (`PaginateCursor.from_string` `ValueError`,
///   `global_paginator.py:21-30`) → 500 `GENERIC_500_MSG`: DRF has no
///   `ParseError` branch here, so the `ValueError` falls through
///   `BaseAPIView.handle_exception` to the generic 500
///   (`views/base.py:200-209`) — unlike the `OffsetPaginator` endpoints
///   (I2/I4), which 400 `{"detail": ...}` via `BasePaginator.paginate`.
/// - `page_size <= 0` (`"0:0:0"`) → 500 (Django `ZeroDivisionError` in
///   `math.ceil`, same mapping); a `page <= 0` clamps to the first page
///   (`paginate` only offsets when `current_page > 0`,
///   `global_paginator.py:49-50`) instead of the `OffsetPaginator` 400.
/// - D6c list carries NO `ORDER BY`: Django applies neither an explicit
///   `order_by` (`intake/base.py:628-630`) nor a model `Meta.ordering`
///   (`IssueDescriptionVersion` defines none, `db/models/issue.py:782-799`)
///   — the same unordered query plan hits the same live table, so the
///   observable row order matches. D6a uses the model `Meta.ordering =
///   ("-created_at",)` (`db/models/issue.py:731`); D6b its explicit
///   `.order_by("-created_at")` (`version.py:135`).
/// - JSON key ORDER inside row objects is alphabetical on the wire
///   (`serde_json::to_value` without `preserve_order`, repo-wide precedent
///   — see `history.rs` key-order notes); the wire KEY SETS match Django
///   exactly and the const key lists below document the canonical Django
///   field order. The list envelope key order follows
///   `global_paginator.py:75-85` exactly (struct serialization).

/// Quoted from `plane/app/views/issue/version.py:103` (D6b) and
/// `plane/app/views/intake/base.py:598` (D6c).
pub(crate) const DESC_GUEST_MSG: &str = "You are not allowed to view this issue";

/// The 10-key cursor-page shape (`version.py:48-59,120-131`,
/// `intake/base.py:615-626`).
#[allow(dead_code)]
pub(crate) const VERSION_LIST_KEYS: [&str; 10] = [
    "id",
    "workspace",
    "project",
    "issue",
    "last_saved_at",
    "owned_by",
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
];

/// The D6a single-item shape (`serializers/issue.py:984-1016`): 31 Django
/// entries minus the duplicated `name` → 30 keys, `name` kept at its FIRST
/// position (after `estimate_point`).
#[allow(dead_code)]
pub(crate) const ISSUE_VERSION_DETAIL_KEYS: [&str; 30] = [
    "id",
    "workspace",
    "project",
    "issue",
    "parent",
    "state",
    "estimate_point",
    "name",
    "priority",
    "start_date",
    "target_date",
    "assignees",
    "sequence_id",
    "labels",
    "sort_order",
    "completed_at",
    "archived_at",
    "is_draft",
    "external_source",
    "external_id",
    "type",
    "cycle",
    "modules",
    "meta",
    "last_saved_at",
    "owned_by",
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
];

/// The D6b/D6c single-item shape
/// (`serializers/issue.py:1023-1038`, 14 keys).
#[allow(dead_code)]
pub(crate) const DESC_VERSION_DETAIL_KEYS: [&str; 14] = [
    "id",
    "workspace",
    "project",
    "issue",
    "description_binary",
    "description_html",
    "description_stripped",
    "description_json",
    "last_saved_at",
    "owned_by",
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
];

/// `?cursor=` query, mirroring `request.GET.get("cursor", None)`
/// (`version.py:46,118`, `intake/base.py:613`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub cursor: Option<String>,
}

/// PROJECT-level role check shared by all six handlers: mirrors
/// `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])` (level
/// PROJECT is the decorator default, `plane/app/permissions/base.py:17`)
/// — roles 20/15/5 pass; anything else (incl. non-member) falls to the
/// workspace-ADMIN fallback applied by the caller via the shared
/// `project_gate_allows`, exactly like D2 `history`/`meta` and D4/D5.
pub(crate) fn guard_versions(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// Pure encoding of the description-twins guest gate
/// (`version.py:91-105`, `intake/base.py:586-600`): blocked iff the caller
/// holds the GUEST project role AND the project hides all features from
/// guests AND the issue is not their own (`not issue.created_by ==
/// request.user` — a null `created_by` counts as "not own").
/// `role` is the outer-gate membership role (same row Django's
/// `ProjectMember...role=GUEST...exists()` reads: active, non-deleted).
pub(crate) fn desc_guest_gate(
    role: Option<i16>,
    guest_view_all: bool,
    is_owner: bool,
) -> Result<(), String> {
    if matches!(role, Some(5)) && !guest_view_all && !is_owner {
        return Err(DESC_GUEST_MSG.to_string());
    }
    Ok(())
}

/// Parses a `global_paginator` cursor (`PaginateCursor.from_string`,
/// `global_paginator.py:21-30`) into `(page_size, page)`.
///
/// Reuses `issue_common::parse_python_int` per slot (Python `int()`
/// semantics — whitespace/sign/underscore handling, unbounded magnitudes
/// saturate). `parse_cursor` is deliberately NOT reused: its float
/// value-slot branch mirrors `OffsetPaginator`
/// (`paginator.py:677-681`), while the global paginator runs `int()` on
/// ALL three slots (`"10.5:0:0"` → `ValueError` → 500 here). The third
/// (`offset`) slot is validated but unused — `paginate()` never reads it
/// (`global_paginator.py:33-85`).
///
/// `None` (no `?cursor=`) mirrors the `cursor is None` default
/// `PaginateCursor(PAGINATOR_MAX_LIMIT, 0, 0)`
/// (`global_paginator.py:35-36`). `Err(())` maps to the generic 500
/// (see module docs — Django `ValueError` → `views/base.py:200-209`).
pub(crate) fn parse_version_cursor(raw: Option<&str>) -> Result<(i64, i128), ()> {
    let s = raw.unwrap_or("1000:0:0");
    let bits: Vec<&str> = s.split(':').collect();
    if bits.len() != 3 {
        return Err(());
    }
    let size_raw = parse_python_int(bits[0]).ok_or(())?;
    let page = parse_python_int(bits[1]).ok_or(())?;
    parse_python_int(bits[2]).ok_or(())?;
    // `page_size = min(current_page_size, PAGINATOR_MAX_LIMIT)`
    // (`global_paginator.py:42`); unbounded magnitudes saturate into `i64`.
    let size = size_raw
        .min(1000)
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    Ok((size, page))
}

/// Generic-500 body for cursor failures, byte-exact from
/// `BaseAPIView.handle_exception` (`plane/app/views/base.py:200-209`),
/// reusing the I2 constant.
fn cursor_500() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": GENERIC_500_MSG})),
    )
}

/// Shared PROJECT gate returning `(allowed, member_role)`: the outer
/// `@allow_permission([ADMIN, MEMBER, GUEST])` check; `member_role` is
/// reused by the description twins' guest gate (same membership row
/// Django re-reads at `version.py:92-98`).
async fn versions_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<(bool, Option<i16>), sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    let allowed = project_gate_allows(
        guard_versions(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    );
    Ok((allowed, member_role))
}

/// One 10-key list row. Field names match the SELECT aliases; the const
/// `VERSION_LIST_KEYS` documents the exact `.values(*required_fields)`
/// order (`version.py:48-59`) — on the wire the KEY SET matches and the
/// order is alphabetical (serde `Value`, repo precedent).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct VersionListRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) workspace: uuid::Uuid,
    pub(crate) project: uuid::Uuid,
    pub(crate) issue: uuid::Uuid,
    pub(crate) last_saved_at: chrono::DateTime<chrono::Utc>,
    pub(crate) owned_by: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
}

/// One D6a full-snapshot row (`serializers/issue.py:984-1016`, `name`
/// once). `version_type` reads the `"type"` column (reserved word in both
/// Rust and SQL — aliased in the SELECT, renamed for serde).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct IssueVersionDetailRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) workspace: uuid::Uuid,
    pub(crate) project: uuid::Uuid,
    pub(crate) issue: uuid::Uuid,
    pub(crate) parent: Option<uuid::Uuid>,
    pub(crate) state: Option<uuid::Uuid>,
    pub(crate) estimate_point: Option<uuid::Uuid>,
    pub(crate) name: String,
    pub(crate) priority: String,
    pub(crate) start_date: Option<chrono::NaiveDate>,
    pub(crate) target_date: Option<chrono::NaiveDate>,
    pub(crate) assignees: Vec<uuid::Uuid>,
    pub(crate) sequence_id: i32,
    pub(crate) labels: Vec<uuid::Uuid>,
    pub(crate) sort_order: f64,
    pub(crate) completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) archived_at: Option<chrono::NaiveDate>,
    pub(crate) is_draft: bool,
    pub(crate) external_source: Option<String>,
    pub(crate) external_id: Option<String>,
    #[serde(rename = "type")]
    pub(crate) version_type: Option<uuid::Uuid>,
    pub(crate) cycle: Option<uuid::Uuid>,
    pub(crate) modules: Vec<uuid::Uuid>,
    pub(crate) meta: Value,
    pub(crate) last_saved_at: chrono::DateTime<chrono::Utc>,
    pub(crate) owned_by: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
}

/// Renders `bytea` as a base64 STANDARD string (null stays null) — see the
/// module-docs wire evidence. `Option<Vec<u8>>` is what sqlx decodes
/// `bytea` into.
fn base64_or_null<S: serde::Serializer>(
    v: &Option<Vec<u8>>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match v {
        Some(b) => s.serialize_str(&base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b,
        )),
        None => s.serialize_none(),
    }
}

/// One D6b/D6c description-detail row
/// (`serializers/issue.py:1023-1038`, 14 keys in listed order).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct DescVersionDetailRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) workspace: uuid::Uuid,
    pub(crate) project: uuid::Uuid,
    pub(crate) issue: uuid::Uuid,
    #[serde(serialize_with = "base64_or_null")]
    pub(crate) description_binary: Option<Vec<u8>>,
    pub(crate) description_html: String,
    pub(crate) description_stripped: Option<String>,
    pub(crate) description_json: Value,
    pub(crate) last_saved_at: chrono::DateTime<chrono::Utc>,
    pub(crate) owned_by: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
}

/// Paginated envelope. Field order is the exact `paginate()` dict order
/// (`global_paginator.py:75-85`) — NOT the I2 `DetailEnvelope` (different
/// paginator family, different keys).
#[derive(Debug, Clone, Serialize)]
pub struct VersionEnvelope {
    pub prev_cursor: String,
    pub cursor: String,
    pub next_cursor: Option<String>,
    pub prev_page_results: bool,
    pub next_page_results: bool,
    pub page_count: i64,
    pub total_results: i64,
    pub total_pages: i64,
    pub results: Vec<Value>,
}

const LIST_COLUMNS: &str = "v.id, v.workspace_id AS workspace, v.project_id AS project, \
    v.issue_id AS issue, v.last_saved_at, v.owned_by_id AS owned_by, \
    v.created_at, v.updated_at, v.created_by_id AS created_by, \
    v.updated_by_id AS updated_by";

const ISSUE_VERSION_COLUMNS: &str = "v.id, v.workspace_id AS workspace, \
    v.project_id AS project, v.issue_id AS issue, v.parent, v.state, \
    v.estimate_point, v.name, v.priority, v.start_date, v.target_date, \
    v.assignees, v.sequence_id, v.labels, v.sort_order, v.completed_at, \
    v.archived_at, v.is_draft, v.external_source, v.external_id, \
    v.\"type\" AS version_type, v.cycle, v.modules, v.meta, v.last_saved_at, \
    v.owned_by_id AS owned_by, v.created_at, v.updated_at, \
    v.created_by_id AS created_by, v.updated_by_id AS updated_by";

const DESC_VERSION_COLUMNS: &str = "v.id, v.workspace_id AS workspace, \
    v.project_id AS project, v.issue_id AS issue, v.description_binary, \
    v.description_html, v.description_stripped, v.description_json, \
    v.last_saved_at, v.owned_by_id AS owned_by, v.created_at, v.updated_at, \
    v.created_by_id AS created_by, v.updated_by_id AS updated_by";

/// Shared cursor-page fetch for the three list handlers, mirroring
/// `paginate()` (`global_paginator.py:33-86`).
///
/// `table` is one of two `&'static str` literals (`issue_versions` /
/// `issue_description_versions`) — never user input. `order` is the
/// per-endpoint ordering (` ORDER BY v.created_at DESC` for D6a/D6b, `""`
/// for D6c — see module docs).
async fn version_list_page(
    pool: &sqlx::PgPool,
    table: &'static str,
    order: &'static str,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    size: i64,
    page: i128,
) -> Result<(StatusCode, Json<Value>), sqlx::Error> {
    // `total_results = base_queryset.count()` (`global_paginator.py:41`).
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {table} v JOIN workspaces w ON w.id = v.workspace_id \
        WHERE v.project_id = $1 AND v.issue_id = $2 AND w.slug = $3 \
        AND v.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(issue_id)
    .bind(slug)
    .fetch_one(pool)
    .await?;
    // `start_index = current_page * page_size` when `current_page > 0`,
    // else 0 (`global_paginator.py:48-50`); past-`i64::MAX` windows slice
    // to `[]` in Django → empty 200 page, no extra query.
    let start: Option<i64> = if page <= 0 {
        Some(0)
    } else {
        let prod = page.saturating_mul(i128::from(size));
        if prod > i128::from(i64::MAX) {
            None
        } else {
            Some(prod as i64)
        }
    };
    // `queryset[start_index:end_index]`, `end_index = min(start + size,
    // total)` (`global_paginator.py:51-54`).
    let rows: Vec<VersionListRow> = match start {
        Some(offset) => {
            sqlx::query_as(&format!(
                "SELECT {LIST_COLUMNS} FROM {table} v \
                JOIN workspaces w ON w.id = v.workspace_id \
                WHERE v.project_id = $1 AND v.issue_id = $2 AND w.slug = $3 \
                AND v.deleted_at IS NULL{order} LIMIT $4 OFFSET $5"
            ))
            .bind(project_id)
            .bind(issue_id)
            .bind(slug)
            .bind(size)
            .bind(offset)
            .fetch_all(pool)
            .await?
        }
        None => Vec::new(),
    };
    let offset = start.unwrap_or(i64::MAX);
    // `next_cursor` is set only when `end_index < total_results`, else null
    // (`global_paginator.py:59-61`).
    let end = (i128::from(offset)).saturating_add(i128::from(size));
    let has_next = end < i128::from(total);
    let results: Vec<Value> = rows
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let envelope = VersionEnvelope {
        // `prev_cursor` always renders, even on the first page
        // (`global_paginator.py:57`); the `:0` suffix is literal — NOT
        // `issue_common::prev_cursor_str`'s `:1` flag (different paginator
        // family; the slot is unread downstream either way).
        prev_cursor: format!("{size}:{p}:0", p = page.saturating_sub(1)),
        cursor: build_cursor(size, page, false),
        next_cursor: has_next.then(|| next_cursor_str(size, page)),
        prev_page_results: page > 0,
        next_page_results: has_next,
        page_count: rows.len() as i64,
        total_results: total,
        total_pages: total_pages(total, size),
        results,
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

/// Project row for the description twins' pre-checks: Django runs
/// `Project.objects.get(pk=project_id)` BEFORE the guest gate
/// (`version.py:88`, `intake/base.py:583`) — miss → 404 `missing()` — and
/// reads `project.guest_view_all_features` for the gate itself. (No slug
/// scoping on this lookup — mirrored literally; the outer gate already
/// binds slug+project.)
#[derive(Debug, Clone, sqlx::FromRow)]
struct ProjectGateRow {
    #[allow(dead_code)]
    id: uuid::Uuid,
    guest_view_all_features: bool,
}

async fn fetch_gate_project(
    pool: &sqlx::PgPool,
    project_id: uuid::Uuid,
) -> Result<Option<ProjectGateRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, guest_view_all_features FROM projects \
        WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

/// Issue row for the description twins' pre-checks: Django runs
/// `Issue.objects.get(workspace__slug=slug, project_id=project_id,
/// pk=work_item_id)` (`version.py:89`, `intake/base.py:584`) — miss → 404
/// — and reads `issue.created_by` for the guest gate. Default manager
/// scope (live rows only); archived/draft/triage rows are NOT excluded
/// here (unlike the `meta` endpoint's `issue_objects` scope).
#[derive(Debug, Clone, sqlx::FromRow)]
struct IssueOwnerRow {
    #[allow(dead_code)]
    id: uuid::Uuid,
    created_by_id: Option<uuid::Uuid>,
}

async fn fetch_owner_issue(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
) -> Result<Option<IssueOwnerRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id, i.created_by_id FROM issues i \
        WHERE i.id = $1 AND i.project_id = $2 \
        AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
        AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Shared pre-check chain for the four description-twin handlers
/// (list + single × work-items/intake paths): outer gate → project 404 →
/// issue 404 → guest 403. Returns the short-circuit response when one
/// fires, `None` on pass.
/// Django's order is fixed (`version.py:87-105`); the 403 comes AFTER both
/// 404s, so an unknown id 404s even for a blocked guest.
async fn desc_gate_chain(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    member_role: Option<i16>,
) -> Result<Option<(StatusCode, Json<Value>)>, sqlx::Error> {
    let project = fetch_gate_project(pool, project_id).await?;
    let Some(project) = project else {
        return Ok(Some(missing()));
    };
    let issue = fetch_owner_issue(pool, slug, project_id, issue_id).await?;
    let Some(issue) = issue else {
        return Ok(Some(missing()));
    };
    let is_owner = issue.created_by_id == Some(user_id);
    if desc_guest_gate(member_role, project.guest_view_all_features, is_owner).is_err() {
        return Ok(Some((
            StatusCode::FORBIDDEN,
            Json(json!({"error": DESC_GUEST_MSG})),
        )));
    }
    Ok(None)
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/`
/// — parity with Django `IssueVersionEndpoint.get` (list branch,
/// `version.py:46-74`). No issue/project pre-check (Django filters
/// directly — unknown issue → 200 empty page); ordering is the model
/// `Meta.ordering = ("-created_at",)` (`db/models/issue.py:731`).
pub async fn issue_versions_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, _) = versions_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    let Ok((size, page)) = parse_version_cursor(q.cursor.as_deref()) else {
        return Ok(cursor_500());
    };
    if size <= 0 {
        // Django: `math.ceil(total / 0)` → `ZeroDivisionError` → generic
        // 500 (`views/base.py:200-209`).
        return Ok(cursor_500());
    }
    Ok(version_list_page(
        &st.pool,
        "issue_versions",
        " ORDER BY v.created_at DESC",
        &slug,
        project_id,
        issue_id,
        size,
        page,
    )
    .await?)
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/versions/:pk/`
/// — parity with Django `IssueVersionEndpoint.get` (single branch,
/// `version.py:38-44`): 200 full snapshot; miss → 404 `missing()` (Django
/// `.get()` → `ObjectDoesNotExist` → `views/base.py:92-96`).
pub async fn issue_version_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, _) = versions_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    let row: Option<IssueVersionDetailRow> = sqlx::query_as(&format!(
        "SELECT {ISSUE_VERSION_COLUMNS} FROM issue_versions v \
        JOIN workspaces w ON w.id = v.workspace_id \
        WHERE v.id = $4 AND v.issue_id = $2 AND v.project_id = $1 AND w.slug = $3 \
        AND v.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(issue_id)
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(serde_json::to_value(r)?))),
        None => Ok(missing()),
    }
}

/// GET `/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/`
/// — parity with Django `WorkItemDescriptionVersionEndpoint.get` (list
/// branch, `version.py:118-144`): explicit `.order_by("-created_at")`.
/// Work-items path ONLY — Django defines no
/// `issues/:id/description-versions/` route.
pub async fn desc_versions_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, work_item_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, member_role) = versions_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    if let Some(resp) =
        desc_gate_chain(&st.pool, auth.0, &slug, project_id, work_item_id, member_role).await?
    {
        return Ok(resp);
    }
    let Ok((size, page)) = parse_version_cursor(q.cursor.as_deref()) else {
        return Ok(cursor_500());
    };
    if size <= 0 {
        return Ok(cursor_500());
    }
    Ok(version_list_page(
        &st.pool,
        "issue_description_versions",
        " ORDER BY v.created_at DESC",
        &slug,
        project_id,
        work_item_id,
        size,
        page,
    )
    .await?)
}

/// GET `/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/:pk/`
/// — parity with Django `WorkItemDescriptionVersionEndpoint.get` (single
/// branch, `version.py:107-116`): 200 14-key detail; miss → 404.
pub async fn desc_version_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, work_item_id, pk)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, member_role) = versions_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    if let Some(resp) =
        desc_gate_chain(&st.pool, auth.0, &slug, project_id, work_item_id, member_role).await?
    {
        return Ok(resp);
    }
    let row: Option<DescVersionDetailRow> = sqlx::query_as(&format!(
        "SELECT {DESC_VERSION_COLUMNS} FROM issue_description_versions v \
        JOIN workspaces w ON w.id = v.workspace_id \
        WHERE v.id = $4 AND v.issue_id = $2 AND v.project_id = $1 AND w.slug = $3 \
        AND v.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(work_item_id)
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(serde_json::to_value(r)?))),
        None => Ok(missing()),
    }
}

/// GET `/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/`
/// — parity with Django `IntakeWorkItemDescriptionVersionEndpoint.get`
/// (list branch, `intake/base.py:613-640`): same 10 keys, same guest gate,
/// NO explicit ordering (see module docs).
pub async fn intake_desc_versions_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, work_item_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, member_role) = versions_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    if let Some(resp) =
        desc_gate_chain(&st.pool, auth.0, &slug, project_id, work_item_id, member_role).await?
    {
        return Ok(resp);
    }
    let Ok((size, page)) = parse_version_cursor(q.cursor.as_deref()) else {
        return Ok(cursor_500());
    };
    if size <= 0 {
        return Ok(cursor_500());
    }
    Ok(version_list_page(
        &st.pool,
        "issue_description_versions",
        "",
        &slug,
        project_id,
        work_item_id,
        size,
        page,
    )
    .await?)
}

/// GET `/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/:pk/`
/// — parity with Django `IntakeWorkItemDescriptionVersionEndpoint.get`
/// (single branch, `intake/base.py:602-611`): the FULL 14-key
/// `IssueDescriptionVersionDetailSerializer` (same as D6b single — the
/// plan's D6c row pins only the paginated shape, the Django single branch
/// is authoritative).
pub async fn intake_desc_version_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, work_item_id, pk)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, member_role) = versions_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    if let Some(resp) =
        desc_gate_chain(&st.pool, auth.0, &slug, project_id, work_item_id, member_role).await?
    {
        return Ok(resp);
    }
    let row: Option<DescVersionDetailRow> = sqlx::query_as(&format!(
        "SELECT {DESC_VERSION_COLUMNS} FROM issue_description_versions v \
        JOIN workspaces w ON w.id = v.workspace_id \
        WHERE v.id = $4 AND v.issue_id = $2 AND v.project_id = $1 AND w.slug = $3 \
        AND v.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(work_item_id)
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(serde_json::to_value(r)?))),
        None => Ok(missing()),
    }
}

#[cfg(test)]
mod batch_d_d6_tests {
    use super::*;
    use crate::routes::issue_common::{next_cursor_str, total_pages};

    #[test]
    fn desc_guest_gate_blocks_non_owner_guest_without_view_all() {
        // Mirrors `version.py:91-105` (D6b) and `intake/base.py:586-600`
        // (D6c): 403 `{"error": "You are not allowed to view this issue"}`
        // iff (role == GUEST AND NOT project.guest_view_all_features AND
        // NOT own issue).
        assert_eq!(
            desc_guest_gate(Some(5), false, false).unwrap_err(),
            "You are not allowed to view this issue"
        );
    }

    #[test]
    fn desc_guest_gate_passes_view_all_owner_and_non_guests() {
        // `(GUEST, view_all, _)` / `(MEMBER, _, _)` → ok (plan D6 step 1).
        assert!(desc_guest_gate(Some(5), true, false).is_ok());
        assert!(desc_guest_gate(Some(5), true, true).is_ok());
        assert!(desc_guest_gate(Some(5), false, true).is_ok());
        assert!(desc_guest_gate(Some(15), false, false).is_ok());
        assert!(desc_guest_gate(Some(20), false, false).is_ok());
        assert!(desc_guest_gate(None, false, false).is_ok());
    }

    #[test]
    fn version_cursor_reuses_issue_common_helpers() {
        // Cursor round-trip reuses `issue_common.rs` helpers — no new cursor
        // helper beyond the thin `parse_version_cursor` wrapper (plan D6
        // step 1: "cursor round-trip reuse (no new helper if existing
        // fits)"). `parse_cursor` itself does NOT fit: its float value-slot
        // branch mirrors `OffsetPaginator`, while `global_paginator.py:22-30`
        // runs `int()` on all three slots.
        assert_eq!(parse_version_cursor(None).unwrap(), (1000, 0));
        assert_eq!(parse_version_cursor(Some("1000:0:0")).unwrap(), (1000, 0));
        assert_eq!(parse_version_cursor(Some("50:3:0")).unwrap(), (50, 3));
        // Third slot is validated but unused (`paginate` never reads
        // `offset`, `global_paginator.py:33-85`).
        assert_eq!(parse_version_cursor(Some("50:3:7")).unwrap(), (50, 3));
        assert!(parse_version_cursor(Some("junk")).is_err());
        assert!(parse_version_cursor(Some("1:2")).is_err());
        assert!(parse_version_cursor(Some("10.5:0:0")).is_err());
        assert!(parse_version_cursor(Some("1000:0:x")).is_err());
        // Envelope strings reuse the shared builders byte-exact:
        // `PaginateCursor.__str__` (`global_paginator.py:18-19`) echoes
        // `"{size}:{page}:0"`, next is `+1`, prev is `-1` with the `:0`
        // suffix (NOT `issue_common::prev_cursor_str`'s `:1` flag).
        assert_eq!(build_cursor(1000, 0, false), "1000:0:0");
        assert_eq!(next_cursor_str(1000, 0), "1000:1:0");
        assert_eq!(format!("{}:{}:0", 1000, 0 - 1), "1000:-1:0");
        assert_eq!(total_pages(2501, 1000), 3);
        assert_eq!(total_pages(0, 1000), 0);
    }

    #[test]
    fn version_list_keys_are_the_10_key_shape() {
        // `version.py:48-59` / `version.py:120-131` /
        // `intake/base.py:615-626`: `id,workspace,project,issue,
        // last_saved_at,owned_by,created_at,updated_at,created_by,updated_by`.
        assert_eq!(
            VERSION_LIST_KEYS,
            [
                "id",
                "workspace",
                "project",
                "issue",
                "last_saved_at",
                "owned_by",
                "created_at",
                "updated_at",
                "created_by",
                "updated_by"
            ]
        );
        let row = VersionListRow {
            id: uuid::Uuid::nil(),
            workspace: uuid::Uuid::nil(),
            project: uuid::Uuid::nil(),
            issue: uuid::Uuid::nil(),
            last_saved_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            owned_by: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by: None,
            updated_by: Some(uuid::Uuid::nil()),
        };
        let v = serde_json::to_value(&row).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = VERSION_LIST_KEYS;
        want.sort_unstable();
        assert_eq!(keys, want);
    }

    fn sample_issue_version_row() -> IssueVersionDetailRow {
        IssueVersionDetailRow {
            id: uuid::Uuid::nil(),
            workspace: uuid::Uuid::nil(),
            project: uuid::Uuid::nil(),
            issue: uuid::Uuid::nil(),
            parent: None,
            state: Some(uuid::Uuid::nil()),
            estimate_point: None,
            name: "Bug".to_string(),
            priority: "high".to_string(),
            start_date: None,
            target_date: None,
            assignees: vec![uuid::Uuid::nil()],
            sequence_id: 7,
            labels: vec![],
            sort_order: 65535.0,
            completed_at: None,
            archived_at: None,
            is_draft: false,
            external_source: None,
            external_id: None,
            version_type: None,
            cycle: None,
            modules: vec![],
            meta: json!({}),
            last_saved_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            owned_by: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by: None,
            updated_by: None,
        }
    }

    #[test]
    fn issue_version_detail_keys_match_serializer_minus_dup_name() {
        // `serializers/issue.py:984-1016`: Django lists `name` TWICE —
        // emit once (first position, after `estimate_point`).
        assert_eq!(ISSUE_VERSION_DETAIL_KEYS.len(), 30);
        assert_eq!(
            ISSUE_VERSION_DETAIL_KEYS.iter().filter(|&&k| k == "name").count(),
            1
        );
        let v = serde_json::to_value(&sample_issue_version_row()).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = ISSUE_VERSION_DETAIL_KEYS;
        want.sort_unstable();
        assert_eq!(keys, want);
        // The `"type"` column renders under the `type` key (serde rename
        // of `version_type`).
        assert!(v.get("type").unwrap().is_null());
        assert!(v.get("version_type").is_none());
        assert_eq!(v.get("name"), Some(&json!("Bug")));
    }

    fn sample_desc_version_row() -> DescVersionDetailRow {
        DescVersionDetailRow {
            id: uuid::Uuid::nil(),
            workspace: uuid::Uuid::nil(),
            project: uuid::Uuid::nil(),
            issue: uuid::Uuid::nil(),
            description_binary: Some(b"test binary".to_vec()),
            description_html: "<p>hi</p>".to_string(),
            description_stripped: Some("hi".to_string()),
            description_json: json!({}),
            last_saved_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            owned_by: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by: None,
            updated_by: None,
        }
    }

    #[test]
    fn desc_version_detail_keys_match_serializer() {
        // `serializers/issue.py:1023-1038` (14 keys).
        assert_eq!(
            DESC_VERSION_DETAIL_KEYS,
            [
                "id",
                "workspace",
                "project",
                "issue",
                "description_binary",
                "description_html",
                "description_stripped",
                "description_json",
                "last_saved_at",
                "owned_by",
                "created_at",
                "updated_at",
                "created_by",
                "updated_by"
            ]
        );
        let v = serde_json::to_value(&sample_desc_version_row()).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = DESC_VERSION_DETAIL_KEYS;
        want.sort_unstable();
        assert_eq!(keys, want);
        // `bytea` renders base64-on-the-wire (see module docs):
        // `base64.b64encode(b"test binary").decode()`.
        assert_eq!(v.get("description_binary"), Some(&json!("dGVzdCBiaW5hcnk=")));
        let mut null_row = sample_desc_version_row();
        null_row.description_binary = None;
        let nv = serde_json::to_value(&null_row).unwrap();
        assert!(nv.get("description_binary").unwrap().is_null());
    }

    #[test]
    fn guest_msg_is_byte_exact() {
        // Quoted from `plane/app/views/issue/version.py:103` (D6b) and
        // `plane/app/views/intake/base.py:598` (D6c).
        assert_eq!(DESC_GUEST_MSG, "You are not allowed to view this issue");
    }

    #[test]
    fn guard_versions_is_admin_member_guest() {
        // Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
        // (`version.py:36,86`, `intake/base.py:581`, default
        // `level="PROJECT"`): 20/15/5 pass outright; anything else falls to
        // the workspace-ADMIN fallback via `project_gate_allows` (same
        // shape as D2/D4/D5).
        assert!(guard_versions(Some(20)).is_ok());
        assert!(guard_versions(Some(15)).is_ok());
        assert!(guard_versions(Some(5)).is_ok());
        assert!(guard_versions(Some(10)).is_err());
        assert!(guard_versions(None).is_err());
    }
}
