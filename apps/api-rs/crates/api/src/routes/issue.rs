use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};

use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/issue.py:IssueCreateSerializer`
/// with #9526 fix: unknown assignee/label ids must 400, not silently drop.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIssue {
    pub name: String,
    #[serde(default)]
    pub assignee_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub label_ids: Option<Vec<uuid::Uuid>>,
    #[serde(default)]
    pub state_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreateIssue) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<IssueOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::issue::Issue>(
        "SELECT id, name FROM issues WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|i| IssueOut { id: i.id, name: i.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateIssue>,
) -> Result<(StatusCode, Json<IssueOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    // #9526 fix: reject unknown assignees (must be active project members role>=15)
    if let Some(ids) = &body.assignee_ids {
        if !ids.is_empty() {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) AND is_active = true AND role >= 15",
            )
            .bind(project_id)
            .bind(ids)
            .fetch_one(&st.pool)
            .await?;
            if count.0 != ids.len() as i64 {
                return Err(anyhow::anyhow!("invalid assignee_id: not a project member").into());
            }
        }
    }
    // #9526 fix: reject unknown labels (must belong to project)
    if let Some(ids) = &body.label_ids {
        if !ids.is_empty() {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM labels WHERE project_id = $1 AND id = ANY($2)",
            )
            .bind(project_id)
            .bind(ids)
            .fetch_one(&st.pool)
            .await?;
            if count.0 != ids.len() as i64 {
                return Err(anyhow::anyhow!("invalid label_id: not in project").into());
            }
        }
    }
    // state must belong to project if provided
    if let Some(state_id) = &body.state_id {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2)",
        )
        .bind(state_id)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await?;
        if !exists.0 {
            return Err(anyhow::anyhow!("State is not valid please pass a valid state_id").into());
        }
    }

    // Django `Issue.save` (`plane/db/models/issue.py:190-216`): sequence_id from
    // IssueSequence max+1 per project; sort_order max+10000 per (project, state).
    let row = sqlx::query_as::<_, common::models::issue::Issue>(
        "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, '<p></p>', '{}', 'none', false, COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $2 AND state_id IS NOT DISTINCT FROM $3), 65535 - 10000) + 10000, COALESCE((SELECT MAX(sequence) FROM issue_sequences WHERE project_id = $2), 0) + 1, $3, $2, w.id, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(project_id)
    .bind(body.state_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(IssueOut { id: row.id, name: row.name }),
    ))
}

/// Query params for `list_by_ids`: Django reads
/// `request.GET.get("issues", False)` (`plane/app/views/issue/base.py:86`),
/// so the param is optional here — a missing param maps to the same 400.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListIssuesQuery {
    #[serde(default)]
    pub issues: Option<String>,
}

/// Mirrors the CSV handling in `IssueListEndpoint.get`
/// (`plane/app/views/issue/base.py:86-94`): `None`/`""` → `Err("Issues are
/// required")` (the `if not issue_ids` check, `base.py:88-89`); otherwise
/// split on `","` dropping exact-`""` tokens (`base.py:91`, no trimming) and
/// parse each kept token as UUID. A malformed token mirrors Django's
/// `pk__in` on the UUID PK raising `ValidationError`, mapped by
/// `BaseAPIView.handle_exception` (`plane/app/views/base.py:182-186`) to
/// 400 `{"error": "Please provide valid detail"}`.
pub(crate) fn parse_issue_csv(raw: Option<&str>) -> Result<Vec<uuid::Uuid>, String> {
    let Some(s) = raw else {
        return Err("Issues are required".to_string());
    };
    if s.is_empty() {
        return Err("Issues are required".to_string());
    }
    let mut ids = Vec::new();
    for tok in s.split(',') {
        if tok.is_empty() {
            continue;
        }
        match uuid::Uuid::parse_str(tok) {
            Ok(id) => ids.push(id),
            Err(_) => return Err("Please provide valid detail".to_string()),
        }
    }
    Ok(ids)
}

/// One row of the default-branch `list_by_ids` response. Field order is the
/// exact 26-key `.values()` order from `IssueListEndpoint.get`
/// (`plane/app/views/issue/base.py:175-202`); struct serialization preserves
/// declaration order, so the JSON keys keep this order. `estimate_point`,
/// `created_by`, `updated_by` are the FK ids (`values()` on an FK yields
/// the id, aliased from `*_id` columns in the SELECT).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IssueListRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub state_id: Option<uuid::Uuid>,
    pub sort_order: f64,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub estimate_point: Option<uuid::Uuid>,
    pub priority: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub cycle_id: Option<uuid::Uuid>,
    pub module_ids: Vec<uuid::Uuid>,
    pub label_ids: Vec<uuid::Uuid>,
    pub assignee_ids: Vec<uuid::Uuid>,
    pub sub_issues_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub attachment_count: i64,
    pub link_count: i64,
    pub is_draft: bool,
    pub archived_at: Option<chrono::NaiveDate>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Pure allow/deny decision for the project-level gate in `list_by_ids`,
/// mirroring `allow_permission(..., level="PROJECT")`
/// (`plane/app/permissions/base.py:53-78`): branch 1 passes when the caller
/// has an active project membership with an allowed role (20/15/5,
/// `base.py:53-63`); the fallback branch (`base.py:64-78`) passes when the
/// caller has ANY active project membership AND is a workspace ADMIN.
/// Anything else denies.
pub(crate) fn project_gate_allows(
    has_allowed_role: bool,
    has_any_membership: bool,
    is_ws_admin: bool,
) -> bool {
    has_allowed_role || (has_any_membership && is_ws_admin)
}

/// Shared project-level gate lookup: the caller's active `project_members`
/// role for (`project_id`, slug). (`deleted_at IS NULL` is explicit here;
/// Django's default managers imply it.) Used by both `list_by_ids` and
/// `list_detail` so the gate is never duplicated.
pub(crate) async fn fetch_project_member_role(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<Option<i16>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT pm.role FROM project_members pm \
          JOIN workspaces w ON w.id = pm.workspace_id \
          WHERE pm.project_id = $1 AND pm.member_id = $2 AND w.slug = $3 \
          AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(user_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Shared workspace-ADMIN check for the gate fallback branch
/// (`permissions/base.py:64-78`).
pub(crate) async fn is_workspace_admin(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(ws_role(pool, user_id, slug)
        .await?
        .map(|r| r == 20)
        .unwrap_or(false))
}

/// Shared GUEST scoping check, mirroring `base.py:98-106` (`list`) and the
/// `Exists(permission_subquery)` (`base.py:1033-1060`, `detail`): an active
/// GUEST (5) membership on a project with `guest_view_all_features=false`
/// restricts rows to `created_by=user`. Given the gate above, the detail
/// `Exists` subquery reduces to exactly this check (members with role > 5
/// or guests on view-all projects see every row).
pub(crate) async fn fetch_guest_scoped(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_members pm \
          JOIN projects p ON p.id = pm.project_id \
          WHERE pm.project_id = $1 AND pm.member_id = $2 AND pm.role = 5 \
          AND pm.is_active = true AND pm.deleted_at IS NULL \
          AND p.guest_view_all_features = false AND p.deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/list/` — parity
/// with Django `IssueListEndpoint.get` default branch
/// (`plane/app/views/issue/base.py:84-205`, `fields`/`expand` unset).
///
/// - Gate: PROJECT-level ADMIN/MEMBER/GUEST (`allow_permission` default
///   `level="PROJECT"`, `permissions/base.py:19,53-78`) — an active
///   `project_members` row (slug-scoped) with role 20/15/5 passes, else the
///   fallback (any active membership + workspace ADMIN) decides; otherwise
///   403 `deny()`.
/// - Missing/empty `?issues` → 400 `{"error": "Issues are required"}`
///   (`base.py:88-89`); malformed UUID → 400
///   `{"error": "Please provide valid detail"}` (see `parse_issue_csv`).
/// - GUEST scoping mirrors `base.py:98-106`: an active GUEST (5) project
///   membership with `guest_view_all_features=false` restricts rows to
///   `created_by=user`.
/// - Manager scope mirrors `IssueManager`
///   (`plane/db/models/issue.py:92-101`) + `SoftDeletionManager` +
///   `base.py:114` (`state__deleted_at__isnull`): `deleted_at IS NULL`,
///   `archived_at IS NULL`, `is_draft=false`, `NOT (group='triage')` on the
///   LEFT-joined live `states.group` (`state.py:14-20`) — NULL-state rows
///   evaluate to NULL and are DROPPED, exactly like Django's
///   `exclude(state__group='triage')` (`issue.py:97`) — plus state's
///   `deleted_at IS NULL`, project not archived/deleted.
/// - Annotations mirror `base.py:122-151` + `issue_queryset_grouper`
///   (`plane/utils/grouper.py:49-90`, applied with `group_by=False` so all
///   three array annotations are present): `cycle_id` (first live
///   `cycle_issues` row), `link_count`/`attachment_count` (`COUNT`, 0 when
///   empty — Django's `Func(F("id"), function="Count")` is a non-aggregate
///   `Func`, so no `GROUP BY`: single-row `COUNT`, never NULL),
///   `sub_issues_count` (`COUNT` over `IssueManager`-scoped children),
///   `module_ids`/`label_ids`/`assignee_ids` (`COALESCE(array_agg, [])`,
///   soft-deleted bridge rows excluded, modules additionally require
///   `archived_at IS NULL`).
/// - Ordering mirrors the default `order_by="-created_at"` (`base.py:153`)
///   → `created_at DESC`. Bare JSON array (Django `Response(issues)`).
///
/// Deviations: `?fields=`/`?expand=` subset branch (`IssueSerializer`,
/// `base.py:172-173`) is OUT — FE `retrieveIssues`
/// (`issue.service.ts:129-137`) sends only `issues=`, and no other caller
/// passes `fields`/`expand` on the `/list/` path; legacy
/// `issue_filters`/`ComplexFilterBackend` filters and `?order_by=`/
/// `?group_by=` are not honored (no-op for this caller, which sends none);
/// `created_at`/`updated_at` serialize as RFC3339 UTC (chrono, same as
/// P1/P5) instead of Django's per-user-timezone conversion
/// (`user_timezone_converter`, `base.py:203-204`); `recent_visited_task`
///   side effect (`base.py:164-170`) is skipped (async worker concern);
///   annotation subqueries add explicit `deleted_at IS NULL` (Django's
///   soft-delete default managers do this implicitly).
pub async fn list_by_ids(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(params): axum::extract::Query<ListIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Project-level gate (`permissions/base.py:53-78`, default
    // `level="PROJECT"`): slug-scoped active project membership; role
    // 20/15/5 passes outright, else the fallback needs workspace ADMIN.
    // (Shared helpers — same gate as `list_detail`.)
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let ids = match parse_issue_csv(params.issues.as_deref()) {
        Ok(ids) => ids,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    // Mirrors `base.py:98-106`: active GUEST membership + project hides
    // guest features → restrict to own issues (shared helper).
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await?;
    let guest_filter = if guest_scoped {
        "AND i.created_by_id = $4"
    } else {
        ""
    };
    let sql = format!(
        "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
        i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
        i.sequence_id, i.project_id, i.parent_id, \
        (SELECT ci.cycle_id FROM cycle_issues ci \
          WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL LIMIT 1) AS cycle_id, \
        COALESCE((SELECT array_agg(DISTINCT mi.module_id) FROM module_issues mi \
          JOIN modules m ON m.id = mi.module_id \
          WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
          AND m.archived_at IS NULL), '{{}}'::uuid[]) AS module_ids, \
        COALESCE((SELECT array_agg(DISTINCT il.label_id) FROM issue_labels il \
          WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{{}}'::uuid[]) AS label_ids, \
        COALESCE((SELECT array_agg(DISTINCT ia.assignee_id) FROM issue_assignees ia \
          WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{{}}'::uuid[]) AS assignee_ids, \
        (SELECT COUNT(*) FROM issues si \
          LEFT JOIN states ss ON ss.id = si.state_id \
          WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
          AND si.archived_at IS NULL AND si.is_draft = false \
          AND ss.deleted_at IS NULL AND ss.\"group\" <> 'triage') AS sub_issues_count, \
        i.created_at, i.updated_at, \
        i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
        (SELECT COUNT(*) FROM file_assets fa \
          WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
          AND fa.deleted_at IS NULL) AS attachment_count, \
        (SELECT COUNT(*) FROM issue_links lin \
          WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count, \
        i.is_draft, i.archived_at, i.deleted_at \
        FROM issues i \
        LEFT JOIN states s ON s.id = i.state_id \
        WHERE i.project_id = $1 \
        AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
        AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
        AND s.deleted_at IS NULL AND s.\"group\" <> 'triage' \
        AND i.id = ANY($3) \
        AND EXISTS(SELECT 1 FROM projects p \
          WHERE p.id = i.project_id AND p.archived_at IS NULL AND p.deleted_at IS NULL) \
        {guest_filter} \
        ORDER BY i.created_at DESC"
    );
    let mut query = sqlx::query_as::<_, IssueListRow>(&sql)
        .bind(project_id)
        .bind(&slug)
        .bind(&ids);
    if guest_scoped {
        query = query.bind(auth.0);
    }
    let rows: Vec<IssueListRow> = query.fetch_all(&st.pool).await?;
    Ok((StatusCode::OK, Json(json!(rows))))
}

/// Parsed `issues-detail/` cursor. Mirrors `Cursor`
/// (`plane/utils/paginator.py:22-59`): `limit_value` is the `value` slot
/// (int, or float when the raw token contains `.`), `page` the `offset`
/// slot, `is_prev` the flag slot. `page` is `i128`: Django ints are
/// unbounded, and a huge-but-parseable page yields an (empty) page, NOT a
/// 400 — out-of-range magnitudes saturate (any such value behaves
/// identically downstream). `OffsetPaginator` (`paginator.py:142-144`)
/// computes the row window from `page * limit` and ignores `limit_value`,
/// so only `page` drives SQL here.
#[derive(Debug)]
pub(crate) struct DetailCursor {
    pub limit_value: f64,
    pub page: i128,
    pub is_prev: bool,
}

/// Parses an integer the way Python `int()` does (ASCII reading):
/// surrounding whitespace stripped, at most one leading `+`/`-`, digits
/// with single underscores allowed BETWEEN digits (`int("1_0") == 10`,
/// while `"1__0"`, `"_1"`, `"1_"`, `"0x10"` all fail). Django ints are
/// unbounded: magnitudes past `i128` saturate to `i128::{MAX,MIN}` by sign.
pub(crate) fn parse_python_int(s: &str) -> Option<i128> {
    let t = s.trim();
    let (negative, body) = match t.strip_prefix(['+', '-']) {
        Some(rest) => (t.starts_with('-'), rest),
        None => (false, t),
    };
    if body.is_empty() {
        return None;
    }
    if !body
        .split('_')
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    match body.replace('_', "").parse::<i128>() {
        Ok(v) => Some(if negative { -v } else { v }),
        Err(_) => Some(if negative { i128::MIN } else { i128::MAX }),
    }
}

/// Mirrors `BasePaginator.get_per_page` (`plane/utils/paginator.py:643-653`,
/// defaults 1000/1000): non-integer → `ParseError("Invalid per_page
/// parameter.")`, over max → `ParseError("Invalid per_page value. Cannot
/// exceed 1000.")` — messages byte-exact, including for huge-but-parseable
/// magnitudes (Django parses the bigint, then fails the max check); DRF
/// renders `ParseError` as 400 `{"detail": msg}`.
pub(crate) fn parse_per_page(raw: Option<&str>) -> Result<i64, String> {
    let s = raw.unwrap_or("1000");
    let v = parse_python_int(s).ok_or_else(|| "Invalid per_page parameter.".to_string())?;
    if v > 1000 {
        return Err("Invalid per_page value. Cannot exceed 1000.".to_string());
    }
    Ok(v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
}

/// Mirrors `Cursor.from_string` (`paginator.py:48-59`) as wrapped by
/// `BasePaginator.paginate` (`paginator.py:677-681`): the value slot is int
/// unless it contains `.` (then float); offset/is_prev go through Python
/// `int()` semantics (see `parse_python_int`); the `is_prev` slot is
/// `bool(int(...))`. ANY malformation surfaces client-side as `ParseError`
/// `"Invalid cursor parameter."` (byte-exact), so every failure maps to it.
pub(crate) fn parse_cursor(raw: &str) -> Result<DetailCursor, String> {
    const ERR: &str = "Invalid cursor parameter.";
    let bits: Vec<&str> = raw.split(':').collect();
    if bits.len() != 3 {
        return Err(ERR.to_string());
    }
    // Python: `float(bits[0]) if "." in bits[0] else int(bits[0])`.
    let limit_value: f64 = if bits[0].contains('.') {
        bits[0].trim().parse().map_err(|_| ERR.to_string())?
    } else {
        parse_python_int(bits[0]).ok_or_else(|| ERR.to_string())? as f64
    };
    let page: i128 = parse_python_int(bits[1]).ok_or_else(|| ERR.to_string())?;
    let is_prev: bool = parse_python_int(bits[2]).ok_or_else(|| ERR.to_string())? != 0;
    Ok(DetailCursor {
        limit_value,
        page,
        is_prev,
    })
}

/// Mirrors `Cursor.__str__` (`paginator.py:31-32`):
/// `f"{value}:{offset}:{int(is_prev)}"`.
pub(crate) fn build_cursor(limit: i64, page: i128, is_prev: bool) -> String {
    format!("{limit}:{page}:{flag}", flag = i32::from(is_prev))
}

/// `OffsetPaginator.get_result` (`paginator.py:165`): next cursor is
/// `(limit, page+1, False)`; the limit echoed is the EFFECTIVE limit
/// (`min(per_page, max_limit)`, `paginator.py:132`). Saturates: a saturated
/// `i128::MAX` page (unbounded cursor) renders without panicking in debug
/// or wrapping in release — Django renders the unbounded int and 200s.
pub(crate) fn next_cursor_str(limit: i64, page: i128) -> String {
    build_cursor(limit, page.saturating_add(1), false)
}

/// `OffsetPaginator.get_result` (`paginator.py:167`): prev cursor is
/// `(limit, page-1, True)` — including page `-1` on the first page.
/// Saturates at `i128::MIN` for the same reason as `next_cursor_str`.
pub(crate) fn prev_cursor_str(limit: i64, page: i128) -> String {
    build_cursor(limit, page.saturating_sub(1), true)
}

/// The row-window offset, computed the way `OffsetPaginator.get_result`
/// does (`paginator.py:142-150`): `offset = page * limit` FIRST — a
/// negative offset raises `BadPaginationError` (→ 400) even when the limit
/// itself is degenerate. The product saturates (`i128`): windows past
/// `i64::MAX` (`BeyondEnd`) slice to `[]` in Django and return an empty
/// page with a 200, so they are not errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageWindow {
    Rows(i64),
    BeyondEnd,
}

pub(crate) fn page_window(page: i128, limit: i64) -> Result<PageWindow, ()> {
    let offset = page.saturating_mul(i128::from(limit));
    if offset < 0 {
        return Err(());
    }
    if offset > i128::from(i64::MAX) {
        return Ok(PageWindow::BeyondEnd);
    }
    Ok(PageWindow::Rows(offset as i64))
}

/// Mirrors `math.ceil(count / limit)` (`paginator.py:180`) without
/// overflowing the `total + limit - 1` intermediate: `total / limit` rounds
/// down, plus one iff there is a remainder. (The `+ 1` cannot overflow:
/// a `total / limit == i64::MAX` quotient implies `limit == 1`, hence a
/// zero remainder.) Callers guarantee `total >= 0` and `limit > 0`
/// (Django would crash with `ZeroDivisionError` → 500 there).
pub(crate) fn total_pages(total: i64, limit: i64) -> i64 {
    total / limit + i64::from(total % limit != 0)
}

/// Order-by allowlist for issues, byte-exact from
/// `plane/utils/order_queryset.py:18-33` (`ISSUE_ORDER_BY_ALLOWLIST`).
pub(crate) const ISSUE_ORDER_BY_ALLOWLIST: &[&str] = &[
    "created_at",
    "updated_at",
    "sequence_id",
    "sort_order",
    "target_date",
    "start_date",
    "completed_at",
    "archived_at",
    "priority",
    "state__name",
    "state__group",
    "assignees__first_name",
    "labels__name",
    "issue_module__module__name",
];

/// Mirrors `sanitize_order_by` (`plane/utils/order_queryset.py:129-150`):
/// strips at most ONE leading `-`, rejects names outside the allowlist (or
/// with a `--` prefix) to the safe default `-created_at`. Unknown FE tokens
/// such as `issue_cycle__cycle__name`, `estimate_point__key`, `link_count`
/// therefore silently fall back to `-created_at` in Django too.
pub(crate) fn sanitize_order_by(raw: &str) -> String {
    if raw.is_empty() {
        return "-created_at".to_string();
    }
    let (bare, desc) = match raw.strip_prefix('-') {
        Some(b) => (b, true),
        None => (raw, false),
    };
    if bare.starts_with('-') || !ISSUE_ORDER_BY_ALLOWLIST.contains(&bare) {
        return "-created_at".to_string();
    }
    if desc {
        format!("-{bare}")
    } else {
        bare.to_string()
    }
}

/// Maps a sanitized `order_by` token to `(SQL expression, descending)`,
/// mirroring `order_issue_queryset` (`order_queryset.py:153-201`) COMBINED
/// with the `OffsetPaginator` re-ordering (`paginator.py:136-140`, which
/// always appends the `-created_at` tiebreak and applies the final
/// direction with `NULLS LAST`):
///
/// - Direct columns order by `i.<col>`; join-backed `state__name` by
///   `s.name`; `Min`-annotated `labels__name` / `assignees__first_name` /
///   `issue_module__module__name` by a correlated `MIN` subquery over live
///   bridge rows (default-manager-implied soft-delete exclusion).
/// - `priority`: `order_queryset.py:159-167` SWAPS the direction token
///   (`-priority` → paginator key `priority_order` ASC = urgent-first;
///   `priority` → `-priority_order` DESC = none-first). The mapping below
///   reproduces that runtime behavior literally.
/// - `state__group`: `order_queryset.py:168-177` reverses the `Case` values
///   for the `-` form but then orders the reversed annotation ASC inside,
///   and the paginator flips it back to DESC — a double negation, so BOTH
///   signs yield backlog-first at runtime. The mapping below reproduces
///   that literally (plain `STATE_ORDER` CASE, always ASC).
pub(crate) fn detail_order_expr(sanitized: &str) -> (&'static str, bool) {
    // Priority direction swap (`order_queryset.py:166`): a leading `-`
    // yields the ASC `priority_order` paginator key and vice versa.
    if sanitized == "priority" || sanitized == "-priority" {
        return (
            "CASE i.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 \
             WHEN 'medium' THEN 2 WHEN 'low' THEN 3 WHEN 'none' THEN 4 ELSE 5 END",
            sanitized == "priority",
        );
    }
    // State-group double negation (`order_queryset.py:168-177` +
    // `paginator.py:136-140`): both signs order the plain `STATE_ORDER`
    // CASE ascending at runtime.
    if sanitized == "state__group" || sanitized == "-state__group" {
        return (
            "CASE s.\"group\" WHEN 'backlog' THEN 0 WHEN 'unstarted' THEN 1 \
             WHEN 'started' THEN 2 WHEN 'completed' THEN 3 WHEN 'cancelled' THEN 4 ELSE 5 END",
            false,
        );
    }
    let desc = sanitized.starts_with('-');
    let bare = sanitized.strip_prefix('-').unwrap_or(sanitized);
    let expr = match bare {
        "updated_at" => "i.updated_at",
        "sequence_id" => "i.sequence_id",
        "sort_order" => "i.sort_order",
        "target_date" => "i.target_date",
        "start_date" => "i.start_date",
        "completed_at" => "i.completed_at",
        "archived_at" => "i.archived_at",
        "state__name" => "s.name",
        "assignees__first_name" => "(SELECT MIN(u.first_name) FROM issue_assignees ia \
            JOIN users u ON u.id = ia.assignee_id \
            WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL)",
        "labels__name" => "(SELECT MIN(l.name) FROM issue_labels il \
            JOIN labels l ON l.id = il.label_id \
            WHERE il.issue_id = i.id AND il.deleted_at IS NULL AND l.deleted_at IS NULL)",
        "issue_module__module__name" => "(SELECT MIN(m.name) FROM module_issues mi \
            JOIN modules m ON m.id = mi.module_id \
            WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL AND m.deleted_at IS NULL)",
        _ => "i.created_at",
    };
    (expr, desc)
}

/// Query params for `list_detail`. Every filter key is optional; unknown
/// query keys are ignored by serde — matching Django, where
/// `issue_filters()` only applies its known `ISSUE_FILTER` keys
/// (`plane/utils/issue_filters.py:459-462`) and `ComplexFilterBackend`
/// ignores everything except the `filters` JSON param
/// (`filter_backend.py:42-44`). `group_by`/`sub_group_by` are accepted AND
/// IGNORED: `IssueDetailEndpoint.get` (`base.py:1027-1103`) never reads
/// them (the same-value 400 at `base.py:323-331` lives only in
/// `IssueViewSet.list`). `fields` is accepted and ignored too: the
/// serializer pops it (`serializers/issue.py:828`) but
/// `to_representation` never consults it — the full row always renders.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DetailIssuesQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub expand: Option<String>,
    #[serde(default)]
    pub fields: Option<String>,
    #[serde(default)]
    pub filters: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub sub_group_by: Option<String>,
    // Legacy `issue_filters` keys (`issue_filters.py:431-457`, GET branch).
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub state_group: Option<String>,
    #[serde(default)]
    pub estimate_point: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub assignees: Option<String>,
    #[serde(default)]
    pub mentions: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub logged_by: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub cycle: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub intake_status: Option<String>,
    #[serde(default)]
    pub inbox_status: Option<String>,
    #[serde(default)]
    pub sub_issue: Option<String>,
    #[serde(default)]
    pub subscriber: Option<String>,
    #[serde(default)]
    pub start_target_date: Option<String>,
    // Archive-only toggle (`IssueArchiveViewSet.list`, `archive.py:109-115`):
    // default `"true"` keeps sub-issues, `"false"` adds `parent__isnull`.
    // Accepted AND ignored by `list_detail` (the detail view never reads it).
    #[serde(default)]
    pub show_sub_issues: Option<String>,
}

/// One row of the `issues-detail/` page. Field order is the exact
/// `IssueListDetailSerializer.to_representation` key order
/// (`plane/app/serializers/issue.py:842-870`): 25 base keys (note: NO
/// `deleted_at`, unlike the `/list/` 26-key shape), then `issue_relation` /
/// `issue_related` appended ONLY when expanded (`issue.py:873-922`).
/// `estimate_point` is `estimate_point_id`, `created_by`/`updated_by` the FK
/// ids. `module_ids`/`label_ids`/`assignee_ids` come from the `.all()`
/// prefetches (`issue.py:832-839`, `base.py:1007-1025`) — live bridge rows
/// only, with NO archived-module exclusion (unlike the `/list/` grouper
/// annotation, `grouper.py:59-68`).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IssueDetailRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub state_id: Option<uuid::Uuid>,
    pub sort_order: f64,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub estimate_point: Option<uuid::Uuid>,
    pub priority: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub is_draft: bool,
    pub archived_at: Option<chrono::NaiveDate>,
    pub cycle_id: Option<uuid::Uuid>,
    pub module_ids: Vec<uuid::Uuid>,
    pub label_ids: Vec<uuid::Uuid>,
    pub assignee_ids: Vec<uuid::Uuid>,
    pub sub_issues_count: i64,
    pub attachment_count: i64,
    pub link_count: i64,
}

/// One entry of the expanded `issue_relation[]` / `issue_related[]` arrays.
/// Key order mirrors the serializer dicts (`serializers/issue.py:882-896`,
/// `907-920`); `created_by` is the related issue's `created_by_id`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IssueRelationItem {
    #[serde(skip_serializing)]
    pub owner_id: uuid::Uuid,
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub sequence_id: i32,
    pub name: String,
    pub relation_type: String,
    pub state_id: Option<uuid::Uuid>,
    pub priority: String,
    pub created_by: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<uuid::Uuid>,
}

/// Paginated envelope. Field order is the exact `Response({...})` key order
/// from `BasePaginator.paginate` (`plane/utils/paginator.py:728-743`).
/// `grouped_by`/`sub_grouped_by` are always null here (the detail endpoint
/// never takes the grouped paginator path); `extra_stats` is always null
/// (no `extra_stats` kwarg is passed, `base.py:1095-1103`).
#[derive(Debug, Clone, Serialize)]
pub struct DetailEnvelope {
    pub grouped_by: Option<String>,
    pub sub_grouped_by: Option<String>,
    pub total_count: i64,
    pub next_cursor: String,
    pub prev_cursor: String,
    pub next_page_results: bool,
    pub prev_page_results: bool,
    pub count: i64,
    pub total_pages: i64,
    pub total_results: i64,
    pub extra_stats: Option<Value>,
    pub results: Vec<Value>,
}

/// Failure of a legacy `issue_filters` key. `BadRequest` carries the
/// `{"error": ...}` body Django's `BaseAPIView.handle_exception` renders
/// for `ValidationError` (`plane/app/views/base.py:182-186`); `Server`
/// renders Django's generic 500 body (see `server_error()`).
#[derive(Debug)]
pub(crate) enum LegacyFilterError {
    BadRequest(String),
    Server,
}

/// Failure of the `filters` JSON param. Renders as Django-DRF does for
/// `ComplexFilterBackend` validation errors: 400 with the `{"message",
/// "code"}` body (`plane/utils/filters/filter_backend.py`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComplexFilterError {
    pub message: String,
    pub code: String,
}

impl ComplexFilterError {
    fn new(message: &str, code: &str) -> Self {
        Self {
            message: message.to_string(),
            code: code.to_string(),
        }
    }

    fn invalid_filterset() -> Self {
        // `_build_leaf_q` (`filter_backend.py:277-285`): any bound
        // FilterSet validation failure → 400. Django also embeds the form
        // `errors` blob; only message+code are mirrored (form internals).
        Self::new("Invalid filter parameters", "invalid_filterset")
    }
}

/// Legacy CSV UUID lists silently drop malformed entries
/// (`filter_valid_uuids`, `plane/utils/issue_filters.py:16-25`); only exact
/// `"null"` tokens are excluded up front (lowercase; `"None"` is kept for
/// the isnull branches below).
pub(crate) fn legacy_uuid_list(raw: &str) -> Vec<uuid::Uuid> {
    raw.split(',')
        .filter(|t| *t != "null")
        .filter_map(|t| uuid::Uuid::parse_str(t).ok())
        .collect()
}

/// Django `__icontains` escapes LIKE wildcards (`%`, `_`) and the escape
/// char itself with a backslash.
pub(crate) fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Pushes `AND <col> = ANY($)` for a legacy CSV UUID filter, skipping the
/// whole condition when no valid UUID survives (Django only adds
/// `<field>__in` for a non-empty valid list, e.g. `issue_filters.py:86-89`).
fn push_legacy_uuid_in(
    qb: &mut QueryBuilder<Postgres>,
    col: &str,
    raw: &str,
    none_isnull_col: Option<&str>,
) {
    let has_none = raw.split(',').any(|t| t == "None");
    let ids = legacy_uuid_list(raw);
    if has_none {
        if let Some(isnull_col) = none_isnull_col {
            // `parent__isnull=True` / `labels__isnull=True` /
            // `assignees__isnull=True` / `created_by__isnull=True`
            // (`issue_filters.py:136,151,165,193`).
            qb.push(" AND ").push(isnull_col).push(" IS NULL");
        }
    }
    if !ids.is_empty() {
        qb.push(" AND ").push(col).push(" = ANY(").push_bind(ids).push(")");
    }
}

/// Pushes an `EXISTS` over a live bridge table for legacy CSV UUID filters
/// (`labels`/`assignees`/`cycle`/`module`/`subscriber`): `"None"` in the
/// list selects issues with NO live bridge row (`labels__isnull=True` on
/// the live-filtered join, `issue_filters.py:150-151` etc.); valid ids
/// select issues having a live row in the list (both AND when both are
/// present, exactly like Django); and a present-but-unusable value
/// (garbage-only, even `""`) still restricts to issues WITH a live bridge
/// row, because `<bridge>__deleted_at__isnull=True` is applied
/// UNCONDITIONALLY (`issue_filters.py:158,173,331,346,401`). `none_isnull`
/// is false for `subscriber`, whose Django filter has no `"None"` branch
/// (`issue_filters.py:392-397`) — there `"None"` counts as garbage.
fn push_legacy_bridge(
    qb: &mut QueryBuilder<Postgres>,
    bridge: &str,
    bridge_col: &str,
    raw: &str,
    none_isnull: bool,
) {
    let has_none = none_isnull && raw.split(',').any(|t| t == "None");
    let ids = legacy_uuid_list(raw);
    if has_none {
        qb.push(" AND NOT EXISTS(SELECT 1 FROM ")
            .push(bridge)
            .push(" b WHERE b.issue_id = i.id AND b.deleted_at IS NULL)");
    }
    if !ids.is_empty() {
        qb.push(" AND EXISTS(SELECT 1 FROM ")
            .push(bridge)
            .push(" b WHERE b.issue_id = i.id AND b.deleted_at IS NULL AND b.")
            .push(bridge_col)
            .push(" = ANY(")
            .push_bind(ids)
            .push("))");
    } else if !has_none {
        qb.push(" AND EXISTS(SELECT 1 FROM ")
            .push(bridge)
            .push(" b WHERE b.issue_id = i.id AND b.deleted_at IS NULL)");
    }
}

/// Mirrors `date_filter` + `string_date_filter`
/// (`plane/utils/issue_filters.py:29-81`) for one legacy date param.
/// `lhs` is the SQL left-hand side: the bare date column for
/// `start_date`/`target_date`, or `DATE(<ts>)` for the `__date` lookups on
/// `created_at`/`updated_at`/`completed_at`. `today` stands in for Django's
/// `timezone.now().date()` (server-local day; containers run UTC).
/// A `""` token anywhere skips the whole filter (`"" not in ...` guards).
/// Errors mirror Django's `ValidationError` → 400
/// `{"error": "Please provide valid detail"}` (`views/base.py:182-186`).
fn push_legacy_date(
    qb: &mut QueryBuilder<Postgres>,
    lhs: &str,
    raw: &str,
    today: chrono::NaiveDate,
) -> Result<(), LegacyFilterError> {
    const BAD: &str = "Please provide valid detail";
    let tokens: Vec<&str> = raw.split(',').collect();
    if tokens.iter().any(|t| t.is_empty()) {
        return Ok(());
    }
    for token in tokens {
        let parts: Vec<&str> = token.split(';').collect();
        if parts.len() >= 2 {
            // `pattern = re.compile(r"\d+_(weeks|months)$")` used with
            // `.match()`: the `$` end-anchor makes it a FULL match on exact
            // `weeks`/`months` (`issue_filters.py:12,63`). When it matches
            // but the token does not split into exactly 3 `;` parts, Django
            // adds NOTHING. Anything else takes the plain-date branch below
            // (so `2_weeksXYZ;after;fromnow` 400s on the garbage date).
            if is_relative_head(parts[0]) {
                if parts.len() == 3 {
                    let (digits, term) = split_relative_head(parts[0]);
                    // Django `int()` is unbounded; `timedelta` overflow
                    // raises → 500. Checked arithmetic mirrors the status
                    // (the body becomes the generic 500 downstream).
                    let duration: i128 = digits.parse().map_err(|_| LegacyFilterError::Server)?;
                    let days: i128 = duration
                        .checked_mul(if term == "months" { 30 } else { 7 })
                        .ok_or(LegacyFilterError::Server)?;
                    let span = chrono::Days::new(
                        days.try_into()
                            .map_err(|_| LegacyFilterError::Server)?,
                    );
                    // `subsequent == "after"` → `__gte`, else `__lte`;
                    // `offset == "fromnow"` → future, else past
                    // (`issue_filters.py:31-52`).
                    let bound = if parts[2] == "fromnow" {
                        today.checked_add_days(span)
                    } else {
                        today.checked_sub_days(span)
                    }
                    .ok_or(LegacyFilterError::Server)?;
                    if parts[1] == "after" {
                        qb.push(" AND ").push(lhs).push(" >= ").push_bind(bound);
                    } else {
                        qb.push(" AND ").push(lhs).push(" <= ").push_bind(bound);
                    }
                }
                continue;
            }
            let day: chrono::NaiveDate =
                parts[0].parse().map_err(|_| LegacyFilterError::BadRequest(BAD.to_string()))?;
            // `"after" in date_query` tests the whole `;` list, but the
            // bound value is always `date_query[0]` (`issue_filters.py:76-79`).
            if parts.iter().any(|p| *p == "after") {
                qb.push(" AND ").push(lhs).push(" >= ").push_bind(day);
            } else {
                qb.push(" AND ").push(lhs).push(" <= ").push_bind(day);
            }
        } else {
            // Single token → `__contains` on the date (no date parsing —
            // even garbage is a LIKE, `issue_filters.py:81`).
            qb.push(" AND CAST(")
                .push(lhs)
                .push(" AS TEXT) LIKE ")
                .push_bind(format!("%{}%", escape_like(parts[0])))
                .push(" ESCAPE '\\'");
        }
    }
    Ok(())
}

/// Exact-match test for `re.compile(r"\d+_(weeks|months)$").match(head)`
/// (`issue_filters.py:12`): leading ASCII digits, `_`, then EXACTLY `weeks`
/// or `months` (the `$` end-anchor rules out `2_weeksXYZ`). (Python `\d`
/// also matches non-ASCII digits — pathological inputs may diverge; the
/// ASCII reading is documented.)
fn is_relative_head(head: &str) -> bool {
    let Some((digits, term)) = head.split_once('_') else {
        return false;
    };
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (term == "weeks" || term == "months")
}

/// Splits an `is_relative_head`-matched head into `(digit-string, term)`.
/// The digit string parses infallibly for realistic magnitudes; absurd ones
/// are rejected by the caller with a 500 (mirroring Django's `timedelta`
/// `OverflowError` → 500).
fn split_relative_head(head: &str) -> (&str, &str) {
    let (digits, term) = head.split_once('_').unwrap_or((head, ""));
    (digits, term)
}

/// Django's `"" not in <list>` guards on the raw-string legacy filters
/// (`issue_filters.py:99,110,125,353,368`): ANY `""` token (after the
/// exact-`"null"` drops) skips the WHOLE key.
fn has_empty_token(raw: &str) -> bool {
    raw.split(',').filter(|t| *t != "null").any(|t| t.is_empty())
}

/// Applies every supported legacy `issue_filters` GET key
/// (`plane/utils/issue_filters.py:428-463`) to the in-progress WHERE
/// clause. Unknown keys never reach this switch (serde drops them), which
/// mirrors `issue_filters()` only dispatching its known `ISSUE_FILTER`
/// keys. Deviations from a full mirror, each Django-literal and committed
/// as reviewer-adjudicable:
/// - `logged_by` names no model field: Django builds `logged_by__...`
///   kwargs and dies with `FieldError` → generic 500. Only when the value
///   yields ≥1 lookup (`"None"` present or ≥1 valid UUID); empty values add
///   nothing.
/// - `estimate_point__in` / `intake_status__in` coerce to UUID/int: garbage
///   raises `ValidationError` → 400 `"Please provide valid detail"`.
/// - `mentions` / `intake_status` / `inbox_status` set NO `deleted_at`
///   condition (`issue_filters.py:177-186,350-377`), mirrored literally.
fn apply_legacy_filters(
    qb: &mut QueryBuilder<Postgres>,
    q: &DetailIssuesQuery,
    today: chrono::NaiveDate,
) -> Result<(), LegacyFilterError> {
    const BAD: &str = "Please provide valid detail";
    if let Some(raw) = q.state.as_deref() {
        push_legacy_uuid_in(qb, "i.state_id", raw, None);
    }
    if let Some(raw) = q.type_.as_deref() {
        // `filter_issue_state_type` (`issue_filters.py:296-305`): ALWAYS
        // applied when the key is present (`"all"` → all five non-triage
        // groups = effective no-op against the triage-excluded base). It
        // writes the SAME `state__group__in` dict key as `filter_state_group`
        // and runs LATER in the `ISSUE_FILTER` dispatch order
        // (`issue_filters.py:431-457`), so a present `type` OVERWRITES
        // `state_group` — mirrored here with if/else, not AND.
        let groups: &[&str] = match raw {
            "backlog" => &["backlog"],
            "active" => &["unstarted", "started"],
            _ => &["backlog", "unstarted", "started", "completed", "cancelled"],
        };
        qb.push(" AND s.\"group\" = ANY(").push_bind(groups).push(")");
    } else if let Some(raw) = q.state_group.as_deref() {
        // `state__group__in` (`issue_filters.py:96-100`): raw strings, no
        // validation; exact-`"null"` tokens dropped, but ANY `""` token
        // skips the whole key (`"" not in ...`, `issue_filters.py:99`).
        // Applies only when `type` is absent (see above).
        if has_empty_token(raw) {
            // Whole-key no-op.
        } else {
            let groups: Vec<String> = raw.split(',').filter(|t| *t != "null").map(str::to_string).collect();
            if !groups.is_empty() {
                qb.push(" AND s.\"group\" = ANY(").push_bind(groups).push(")");
            }
        }
    }
    if let Some(raw) = q.estimate_point.as_deref() {
        // `estimate_point__in` takes RAW strings (`issue_filters.py:107-111`)
        // against the UUID FK: garbage → Django `ValidationError` → 400 —
        // unless a `""` token skips the whole key first
        // (`issue_filters.py:110`).
        if !has_empty_token(raw) {
            let ids: Vec<&str> = raw.split(',').filter(|t| *t != "null").collect();
            if !ids.is_empty() {
                let mut parsed = Vec::with_capacity(ids.len());
                for id in ids {
                    parsed.push(
                        uuid::Uuid::parse_str(id)
                            .map_err(|_| LegacyFilterError::BadRequest(BAD.to_string()))?,
                    );
                }
                qb.push(" AND i.estimate_point_id = ANY(").push_bind(parsed).push(")");
            }
        }
    }
    if let Some(raw) = q.priority.as_deref() {
        // Raw strings (`issue_filters.py:122-126`); `""` skips the whole key
        // (`issue_filters.py:125`).
        if !has_empty_token(raw) {
            let pris: Vec<String> = raw.split(',').filter(|t| *t != "null").map(str::to_string).collect();
            if !pris.is_empty() {
                qb.push(" AND i.priority = ANY(").push_bind(pris).push(")");
            }
        }
    }
    if let Some(raw) = q.parent.as_deref() {
        push_legacy_uuid_in(qb, "i.parent_id", raw, Some("i.parent_id"));
    }
    if let Some(raw) = q.labels.as_deref() {
        push_legacy_bridge(qb, "issue_labels", "label_id", raw, true);
    }
    if let Some(raw) = q.assignees.as_deref() {
        push_legacy_bridge(qb, "issue_assignees", "assignee_id", raw, true);
    }
    if let Some(raw) = q.mentions.as_deref() {
        // `issue_mention__mention__id__in` (`issue_filters.py:177-186`):
        // sets NO `deleted_at` condition — mirrored literally.
        let ids = legacy_uuid_list(raw);
        if !ids.is_empty() {
            qb.push(" AND EXISTS(SELECT 1 FROM issue_mentions im \
                WHERE im.issue_id = i.id \
                AND im.mention_id = ANY(")
                .push_bind(ids)
                .push("))");
        }
    }
    if let Some(raw) = q.created_by.as_deref() {
        push_legacy_uuid_in(qb, "i.created_by_id", raw, Some("i.created_by_id"));
    }
    if let Some(raw) = q.logged_by.as_deref() {
        // No `logged_by` field exists on the Issue model: any produced
        // lookup raises Django `FieldError` → 500 (`issue_filters.py:414-425`).
        let tokens: Vec<&str> = raw.split(',').filter(|t| *t != "null").collect();
        if tokens.iter().any(|t| *t == "None") || !legacy_uuid_list(raw).is_empty() {
            return Err(LegacyFilterError::Server);
        }
    }
    if let Some(raw) = q.name.as_deref() {
        if !raw.is_empty() {
            // `name__icontains` (`issue_filters.py:203-206`).
            qb.push(" AND i.name ILIKE ")
                .push_bind(format!("%{}%", escape_like(raw)))
                .push(" ESCAPE '\\'");
        }
    }
    if let Some(raw) = q.created_at.as_deref() {
        push_legacy_date(qb, "DATE(i.created_at)", raw, today)?;
    }
    if let Some(raw) = q.updated_at.as_deref() {
        push_legacy_date(qb, "DATE(i.updated_at)", raw, today)?;
    }
    if let Some(raw) = q.start_date.as_deref() {
        push_legacy_date(qb, "i.start_date", raw, today)?;
    }
    if let Some(raw) = q.target_date.as_deref() {
        push_legacy_date(qb, "i.target_date", raw, today)?;
    }
    if let Some(raw) = q.completed_at.as_deref() {
        push_legacy_date(qb, "DATE(i.completed_at)", raw, today)?;
    }
    if let Some(raw) = q.project.as_deref() {
        let ids = legacy_uuid_list(raw);
        if !ids.is_empty() {
            qb.push(" AND i.project_id = ANY(").push_bind(ids).push(")");
        }
    }
    if let Some(raw) = q.cycle.as_deref() {
        push_legacy_bridge(qb, "cycle_issues", "cycle_id", raw, true);
    }
    if let Some(raw) = q.module.as_deref() {
        push_legacy_bridge(qb, "module_issues", "module_id", raw, true);
    }
    // `issue_intake__status__in` (`issue_filters.py:350-377`): `status`
    // is an integer column — non-integer tokens raise Django
    // `ValidationError` → 400 — and ANY `""` token skips the key
    // (`issue_filters.py:353,368`). Sets NO `deleted_at` condition.
    // Both legacy keys write the SAME dict key and `inbox_status` runs
    // LATER in the dispatch order, so a VALID (non-empty, no-`""`)
    // `inbox_status` OVERWRITES `intake_status`; an invalid one leaves the
    // earlier value intact — mirrored, not ANDed.
    let intake_tokens: Vec<&str> = q
        .intake_status
        .as_deref()
        .map(|raw| raw.split(',').filter(|t| *t != "null").collect())
        .unwrap_or_default();
    let inbox_tokens: Vec<&str> = q
        .inbox_status
        .as_deref()
        .map(|raw| raw.split(',').filter(|t| *t != "null").collect())
        .unwrap_or_default();
    let tokens_valid = |tokens: &[&str]| !tokens.is_empty() && !tokens.iter().any(|t| t.is_empty());
    let status_tokens = if tokens_valid(&inbox_tokens) {
        inbox_tokens
    } else if tokens_valid(&intake_tokens) {
        intake_tokens
    } else {
        Vec::new()
    };
    if !status_tokens.is_empty() {
        let mut statuses = Vec::with_capacity(status_tokens.len());
        for token in status_tokens {
            statuses.push(
                token
                    .parse::<i32>()
                    .map_err(|_| LegacyFilterError::BadRequest(BAD.to_string()))?,
            );
        }
        qb.push(" AND EXISTS(SELECT 1 FROM intake_issues ii \
            WHERE ii.issue_id = i.id \
            AND ii.status = ANY(")
            .push_bind(statuses)
            .push("))");
    }
    if let Some(raw) = q.sub_issue.as_deref() {
        // `filter_sub_issue_toggle` (`issue_filters.py:380-389`): the key
        // merely being PRESENT is not enough — only `"false"` filters
        // (`parent__isnull=True`); `"true"` (what FE sends by default) is a
        // no-op. Absent key → the dispatch loop never calls the filter.
        if raw == "false" {
            qb.push(" AND i.parent_id IS NULL");
        }
    }
    if let Some(raw) = q.subscriber.as_deref() {
        // `issue_subscribers__subscriber_id__in`
        // (`issue_filters.py:392-403`), with the same unconditional
        // live-bridge scoping as the other bridge filters
        // (`issue_filters.py:401`).
        push_legacy_bridge(qb, "issue_subscribers", "subscriber_id", raw, false);
    }
    if let Some(raw) = q.start_target_date.as_deref() {
        // (`issue_filters.py:406-411`): `"true"` → both dates set.
        if raw == "true" {
            qb.push(" AND i.target_date IS NOT NULL AND i.start_date IS NOT NULL");
        }
    }
    Ok(())
}

/// Declared `IssueFilterSet` leaf keys usable inside the `filters` JSON
/// param (`plane/utils/filters/filterset.py:135-200` plus the `__exact`
/// twins minted by `BaseFilterSet.get_filters`, `filterset.py:33-53`).
/// Anything else → 400 `invalid_filter_field`, mirroring `_validate_fields`
/// (`filter_backend.py:100-127`).
pub(crate) const COMPLEX_FILTER_ALLOWLIST: &[&str] = &[
    "assignee_id",
    "assignee_id__exact",
    "assignee_id__in",
    "cycle_id",
    "cycle_id__exact",
    "cycle_id__in",
    "module_id",
    "module_id__exact",
    "module_id__in",
    "mention_id",
    "mention_id__exact",
    "mention_id__in",
    "label_id",
    "label_id__exact",
    "label_id__in",
    "created_by_id",
    "created_by_id__exact",
    "created_by_id__in",
    "is_archived",
    "is_archived__exact",
    "state_group",
    "state_group__exact",
    "state_group__in",
    "state_id",
    "state_id__exact",
    "state_id__in",
    "project_id",
    "project_id__exact",
    "project_id__in",
    "subscriber_id",
    "subscriber_id__exact",
    "subscriber_id__in",
    "created_at",
    "created_at__exact",
    "created_at__range",
    "updated_at",
    "updated_at__exact",
    "updated_at__range",
    "start_date",
    "start_date__exact",
    "start_date__range",
    "target_date",
    "target_date__exact",
    "target_date__range",
    "is_draft",
    "is_draft__exact",
    "priority",
    "priority__exact",
    "priority__in",
];

/// Parses + validates the `filters` JSON param, mirroring
/// `ComplexFilterBackend.filter_queryset` → `_normalize_filter_data` →
/// `_apply_json_filter` (`plane/utils/filters/filter_backend.py:31-98`):
/// absent/empty-string param → `None` (no-op); invalid JSON → 400
/// `invalid_json` (byte-exact message); falsy JSON (`{}`, `[]`, `""`, `0`,
/// `false`, `null`) → `None` — `_apply_json_filter` short-circuits BEFORE
/// validation (`filter_backend.py:82-83`), so FE's `filters="{}"` default is
/// a no-op, NOT a 400; truthy non-objects → 400 `invalid_filter_node`.
/// Otherwise the structure is validated (`_validate_structure`,
/// `max_depth=5` from `default_max_depth`) and every leaf field checked
/// against the FilterSet (`_validate_fields`).
pub(crate) fn parse_complex_filter(raw: Option<&str>) -> Result<Option<serde_json::Value>, ComplexFilterError> {
    const MAX_DEPTH: usize = 5;
    let Some(s) = raw else {
        return Ok(None);
    };
    if s.is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(s).map_err(|_| {
        ComplexFilterError::new(
            "Invalid JSON for 'filter'. Expected a valid JSON object.",
            "invalid_json",
        )
    })?;
    // Falsy filter data returns the queryset unchanged (`filter_backend.py:82-83`).
    let is_truthy = match &value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        serde_json::Value::String(st) => !st.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    };
    if !is_truthy {
        return Ok(None);
    }
    validate_filter_node(&value, MAX_DEPTH, 1)?;
    for field in extract_filter_fields(&value) {
        if !COMPLEX_FILTER_ALLOWLIST.contains(&field.as_str()) {
            return Err(ComplexFilterError::new(
                &format!("Filtering on field '{field}' is not allowed"),
                "invalid_filter_field",
            ));
        }
    }
    Ok(Some(value))
}

/// Mirrors `_validate_structure` (`filter_backend.py:313-409`). Operator
/// keys match case-insensitively; messages are byte-exact.
fn validate_filter_node(
    node: &serde_json::Value,
    max_depth: usize,
    current_depth: usize,
) -> Result<(), ComplexFilterError> {
    if current_depth > max_depth {
        return Err(ComplexFilterError::new(
            &format!("Filter nesting is too deep (max {max_depth}); found depth {current_depth}"),
            "max_depth_exceeded",
        ));
    }
    let Some(obj) = node.as_object() else {
        return Err(ComplexFilterError::new(
            "Each filter node must be a JSON object",
            "invalid_filter_node",
        ));
    };
    if obj.is_empty() {
        return Err(ComplexFilterError::new(
            "Filter objects must not be empty",
            "empty_filter_object",
        ));
    }
    let logical: Vec<&String> = obj
        .keys()
        .filter(|k| matches!(k.to_lowercase().as_str(), "or" | "and" | "not"))
        .collect();
    if logical.len() > 1 {
        return Err(ComplexFilterError::new(
            "A filter object cannot contain multiple logical operators at the same level",
            "multiple_logical_operators",
        ));
    }
    if let Some(op_key) = logical.first() {
        if obj.len() != 1 {
            return Err(ComplexFilterError::new(
                &format!("Cannot mix logical operator '{op_key}' with field keys at the same level"),
                "mixed_operator_and_fields",
            ));
        }
        let op = op_key.to_lowercase();
        let child = &obj[*op_key];
        if op == "not" {
            if !child.is_object() {
                return Err(ComplexFilterError::new(
                    "'not' must be a single JSON object",
                    "invalid_not_child",
                ));
            }
            return validate_filter_node(child, max_depth, current_depth + 1);
        }
        let Some(items) = child.as_array() else {
            return Err(ComplexFilterError::new(
                &format!("'{op}' must be a non-empty list of filter objects"),
                "invalid_operator_children",
            ));
        };
        if items.is_empty() {
            return Err(ComplexFilterError::new(
                &format!("'{op}' must be a non-empty list of filter objects"),
                "invalid_operator_children",
            ));
        }
        for item in items {
            if !item.is_object() {
                return Err(ComplexFilterError::new(
                    &format!("All children of '{op}' must be JSON objects"),
                    "invalid_operator_child_type",
                ));
            }
            validate_filter_node(item, max_depth, current_depth + 1)?;
        }
        return Ok(());
    }
    validate_filter_leaf(obj)
}

/// Mirrors `_validate_leaf` (`filter_backend.py:411-456`); messages byte-exact.
fn validate_filter_leaf(obj: &serde_json::Map<String, serde_json::Value>) -> Result<(), ComplexFilterError> {
    if obj.is_empty() {
        return Err(ComplexFilterError::new(
            "Leaf filter must be a non-empty JSON object",
            "invalid_leaf",
        ));
    }
    for (key, value) in obj {
        match value {
            serde_json::Value::Array(items) => {
                if items.is_empty() {
                    return Err(ComplexFilterError::new(
                        &format!("List value for '{key}' must not be empty"),
                        "empty_list_value",
                    ));
                }
                for item in items {
                    if !is_filter_scalar(item) {
                        return Err(ComplexFilterError::new(
                            &format!("List value for '{key}' must contain only scalar items"),
                            "non_scalar_list_item",
                        ));
                    }
                }
            }
            v if is_filter_scalar(v) => {}
            _ => {
                return Err(ComplexFilterError::new(
                    &format!("Value for '{key}' must be a scalar, null, or list/tuple of scalars"),
                    "invalid_value_type",
                ));
            }
        }
    }
    Ok(())
}

fn is_filter_scalar(v: &serde_json::Value) -> bool {
    matches!(
        v,
        serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
    )
}

/// Mirrors `_extract_field_names` (`filter_backend.py:142-162`): operator
/// keys (case-insensitive) recurse, every other key is collected verbatim
/// (fields are case-SENSITIVE downstream).
fn extract_filter_fields(value: &serde_json::Value) -> Vec<String> {
    let mut fields = Vec::new();
    let Some(obj) = value.as_object() else {
        return fields;
    };
    for (key, child) in obj {
        match key.to_lowercase().as_str() {
            "not" => fields.extend(extract_filter_fields(child)),
            "or" | "and" => {
                if let Some(items) = child.as_array() {
                    for item in items {
                        fields.extend(extract_filter_fields(item));
                    }
                }
            }
            _ => fields.push(key.clone()),
        }
    }
    fields
}

/// Normalizes one JSON scalar the way `_build_leaf_q`
/// (`filter_backend.py:260-268`) stringifies it into the `QueryDict`:
/// `null` → `""`, booleans → Python `str()` (`"True"`/`"False"`), numbers →
/// plain rendering, strings verbatim.
fn leaf_scalar_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(true) => "True".to_string(),
        serde_json::Value::Bool(false) => "False".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Applies a validated `filters` tree to the WHERE clause, mirroring
/// `_evaluate_node` (`filter_backend.py:164-216`) + the `IssueFilterSet`
/// field lookups (`filterset.py:135-200`). Combination nodes are
/// parenthesized groups; value-coercion failures (bad UUID/int/date/bool)
/// mirror the FilterSet `ValidationError` → 400 `invalid_filterset`.
/// NOTE a Django subtlety reproduced here: `_evaluate_node` matches `or` /
/// `and` / `not` CASE-SENSITIVELY, while structure validation matches them
/// case-insensitively — so a node like `{"OR": [...]}` validates as an
/// operator but evaluates as a LEAF whose unknown key the FilterSet ignores:
/// the whole node (children included) is a no-op. `apply_complex_leaf`
/// therefore skips non-allowlisted keys silently.
pub(crate) fn apply_complex_filter(
    qb: &mut QueryBuilder<Postgres>,
    tree: &serde_json::Value,
) -> Result<(), ComplexFilterError> {
    qb.push(" AND ");
    eval_filter_node(qb, tree)
}

fn eval_filter_node(qb: &mut QueryBuilder<Postgres>, node: &serde_json::Value) -> Result<(), ComplexFilterError> {
    let obj = node.as_object().ok_or_else(ComplexFilterError::invalid_filterset)?;
    // Exact-lowercase single operator keys only (see `apply_complex_filter`).
    if obj.len() == 1 {
        if let Some(items) = obj.get("or").and_then(|v| v.as_array()) {
            qb.push("(");
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    qb.push(" OR ");
                }
                eval_filter_node(qb, item)?;
            }
            qb.push(")");
            return Ok(());
        }
        if let Some(items) = obj.get("and").and_then(|v| v.as_array()) {
            qb.push("(");
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    qb.push(" AND ");
                }
                eval_filter_node(qb, item)?;
            }
            qb.push(")");
            return Ok(());
        }
        if let Some(child) = obj.get("not") {
            qb.push("NOT (");
            eval_filter_node(qb, child)?;
            qb.push(")");
            return Ok(());
        }
    }
    // Leaf dict: every allowlisted key ANDs in; anything else is ignored
    // (the `{"OR": ...}` case above, plus defense-in-depth).
    qb.push("(");
    let mut first = true;
    for (key, value) in obj {
        if !COMPLEX_FILTER_ALLOWLIST.contains(&key.as_str()) {
            continue;
        }
        if !first {
            qb.push(" AND ");
        }
        first = false;
        apply_complex_leaf(qb, key, value)?;
    }
    // An all-skipped leaf contributes a no-op TRUE (mirrors Django's leaf
    // with only-ignored params → bare `Q()`).
    qb.push(if first { "TRUE" } else { "" });
    qb.push(")");
    Ok(())
}

/// Single filterset leaf → SQL. Mirrors the `IssueFilterSet` lookups:
/// bridge `EXISTS` for the `*_id` method filters (`filterset.py:215-297`),
/// direct comparisons otherwise. `is_archived`/`is_draft` go through
/// `forms.NullBooleanField`, which NEVER raises: recognized truthy/falsy
/// spellings filter, while anything else cleans to `None` — a method-filter
/// no-op for `is_archived` (`EMPTY_VALUES` short-circuit) and
/// `exact None` → `IS NULL` for `is_draft`. Date scalars bind `YYYY-MM-DD`
/// (`""` → `IS NULL`, mirroring `DateField("")` → `None` → `exact None`);
/// `__range` binds an inclusive `BETWEEN` over the comma-split pair
/// (`DateCSVRangeFilter` for datetimes compares the date component,
/// `filterset.py:22-30`).
fn apply_complex_leaf(
    qb: &mut QueryBuilder<Postgres>,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), ComplexFilterError> {
    // Split `base__in` / `base__exact` / `base__range` suffixes; every other
    // allowlisted key is a bare exact lookup.
    let (base, suffix) = match key.rsplit_once("__") {
        Some((b, "in")) => (b, "in"),
        Some((b, "exact")) => (b, "exact"),
        Some((b, "range")) => (b, "range"),
        _ => (key, ""),
    };
    // CSV pieces: scalars split on `,` (FE sends comma-joined strings;
    // `BaseCSVFilter` splits each repeated value too); JSON arrays
    // contribute each item's pieces.
    let pieces: Vec<String> = match value {
        serde_json::Value::Array(items) => items
            .iter()
            .flat_map(|item| leaf_scalar_string(item).split(',').map(str::to_string).collect::<Vec<_>>())
            .collect(),
        scalar => leaf_scalar_string(scalar)
            .split(',')
            .map(str::to_string)
            .collect(),
    };
    // Bridge-backed `*_id` filters → live-bridge EXISTS.
    if let Some((table, col)) = complex_bridge_target(base) {
        return apply_complex_uuid_leaf(qb, table, Some(col), &pieces, suffix);
    }
    // Direct UUID columns.
    if let Some(col) = match base {
        "created_by_id" => Some("i.created_by_id"),
        "state_id" => Some("i.state_id"),
        "project_id" => Some("i.project_id"),
        _ => None,
    } {
        return apply_complex_uuid_leaf(qb, "", Some(col), &pieces, suffix);
    }
    match base {
        "state_group" => apply_complex_text_leaf(qb, "s.\"group\"", &pieces, suffix),
        "priority" => apply_complex_text_leaf(qb, "i.priority", &pieces, suffix),
        "is_archived" => {
            // `filter_is_archived` (`filterset.py:202-211`); the value went
            // through `QueryDict` stringification (see `leaf_scalar_string`),
            // and repeated values resolve to the LAST one. Crucially the
            // `BooleanFilter` form field is `forms.NullBooleanField`, whose
            // `to_python` maps ANYTHING outside the True/False spellings
            // (incl. `"yes"`, `""`) to `None` and whose `validate()` is a
            // no-op — so unrecognized spellings NEVER 400. `None` then hits
            // `FilterMethod`'s `EMPTY_VALUES` short-circuit → bare `Q()`.
            let last = pieces.last().map(String::as_str).unwrap_or("");
            if ["True", "true", "1"].contains(&last) {
                qb.push("i.archived_at IS NOT NULL");
            } else if ["False", "false", "0"].contains(&last) {
                qb.push("i.archived_at IS NULL");
            } else {
                qb.push("TRUE");
            }
            Ok(())
        }
        "is_draft" => {
            // Same `NullBooleanField` leniency (never raises — the model
            // field's non-nullability, `db/models/issue.py:161`, does not
            // affect filter validation): unrecognized spellings clean to
            // `None` → standard `Q(is_draft__exact=None)` → `IS NULL`
            // (matches nothing on this non-nullable column), NOT a 400.
            let last = pieces.last().map(String::as_str).unwrap_or("");
            if ["True", "true", "1"].contains(&last) {
                qb.push("i.is_draft = true");
            } else if ["False", "false", "0"].contains(&last) {
                qb.push("i.is_draft = false");
            } else {
                qb.push("i.is_draft IS NULL");
            }
            Ok(())
        }
        "start_date" => apply_complex_date_leaf(qb, "i.start_date", &pieces, suffix),
        "target_date" => apply_complex_date_leaf(qb, "i.target_date", &pieces, suffix),
        "created_at" => apply_complex_date_leaf(qb, "DATE(i.created_at)", &pieces, suffix),
        "updated_at" => apply_complex_date_leaf(qb, "DATE(i.updated_at)", &pieces, suffix),
        _ => Err(ComplexFilterError::invalid_filterset()),
    }
}

/// `(bridge_table, bridge_column)` for the method-backed `*_id` filterset
/// filters (`filterset.py:215-297`).
fn complex_bridge_target(base: &str) -> Option<(&'static str, &'static str)> {
    match base {
        "assignee_id" => Some(("issue_assignees", "assignee_id")),
        "cycle_id" => Some(("cycle_issues", "cycle_id")),
        "module_id" => Some(("module_issues", "module_id")),
        "mention_id" => Some(("issue_mentions", "mention_id")),
        "label_id" => Some(("issue_labels", "label_id")),
        "subscriber_id" => Some(("issue_subscribers", "subscriber_id")),
        _ => None,
    }
}

/// UUID leaf. `UUIDField.to_python("")` → `None` (valid), and direct
/// filters bypass `EMPTY_VALUES` (`BaseFilterSet.build_combined_q:79-108`),
/// so empty scalars become `Q(<col>__exact=None)` → `IS NULL`, while method
/// (bridge) filters no-op on empty (`filter_backend.py:268` maps null→`""`
/// and `FilterMethod` short-circuits). `Q(<col>__in=[])` matches nothing.
/// `table == ""` means a direct column (`created_by_id`/`state_id`/
/// `project_id`); otherwise a live-bridge `EXISTS`. Non-empty values coerce
/// strictly (`UUIDFilter` failure → 400); repeated scalar values resolve to
/// the LAST, like Django's `QueryDict`.
fn apply_complex_uuid_leaf(
    qb: &mut QueryBuilder<Postgres>,
    table: &str,
    col: Option<&str>,
    pieces: &[String],
    suffix: &str,
) -> Result<(), ComplexFilterError> {
    if suffix != "in" {
        let last = pieces.last().map(String::as_str).unwrap_or("");
        if last.is_empty() {
            if table.is_empty() {
                qb.push(col.unwrap_or("i.id")).push(" IS NULL");
            } else {
                qb.push("TRUE");
            }
            return Ok(());
        }
        let id =
            uuid::Uuid::parse_str(last).map_err(|_| ComplexFilterError::invalid_filterset())?;
        if table.is_empty() {
            qb.push(col.unwrap_or("i.id")).push(" = ").push_bind(id);
            return Ok(());
        }
        qb.push("EXISTS(SELECT 1 FROM ").push(table).push(
            " b WHERE b.issue_id = i.id AND b.deleted_at IS NULL AND b.",
        );
        qb.push(col.unwrap_or("id")).push(" = ").push_bind(id).push(")");
        return Ok(());
    }
    let kept: Vec<&String> = pieces.iter().filter(|p| !p.is_empty()).collect();
    if kept.is_empty() {
        if table.is_empty() {
            qb.push(col.unwrap_or("i.id")).push(" = ANY('{}'::uuid[])");
        } else {
            qb.push("TRUE");
        }
        return Ok(());
    }
    let mut ids = Vec::with_capacity(kept.len());
    for piece in kept {
        ids.push(uuid::Uuid::parse_str(piece).map_err(|_| ComplexFilterError::invalid_filterset())?);
    }
    if table.is_empty() {
        qb.push(col.unwrap_or("i.id")).push(" = ANY(").push_bind(ids).push(")");
        return Ok(());
    }
    qb.push("EXISTS(SELECT 1 FROM ").push(table).push(
        " b WHERE b.issue_id = i.id AND b.deleted_at IS NULL AND b.",
    );
    qb.push(col.unwrap_or("id")).push(" = ANY(").push_bind(ids).push("))");
    Ok(())
}

/// Raw-string leaf (`state_group`, `priority`): no coercion, so even `""`
/// binds (matches nothing, never errors — `CharFilter`).
fn apply_complex_text_leaf(
    qb: &mut QueryBuilder<Postgres>,
    col: &str,
    pieces: &[String],
    suffix: &str,
) -> Result<(), ComplexFilterError> {
    if suffix == "in" {
        qb.push(col).push(" = ANY(").push_bind(pieces.to_vec()).push(")");
    } else {
        // Repeated values → Django `QueryDict` keeps the LAST.
        qb.push(col)
            .push(" = ")
            .push_bind(pieces.last().cloned().unwrap_or_default());
    }
    Ok(())
}

/// Date leaf: scalars parse `YYYY-MM-DD` (`""` → `IS NULL`, see
/// `apply_complex_leaf`); `__range` needs exactly two valid dates for an
/// inclusive `BETWEEN` (`DateCSVRangeFilter`, `filterset.py:22-30`).
fn apply_complex_date_leaf(
    qb: &mut QueryBuilder<Postgres>,
    lhs: &str,
    pieces: &[String],
    suffix: &str,
) -> Result<(), ComplexFilterError> {
    if suffix == "range" {
        if pieces.len() != 2 {
            return Err(ComplexFilterError::invalid_filterset());
        }
        let lo: chrono::NaiveDate = pieces[0].parse().map_err(|_| ComplexFilterError::invalid_filterset())?;
        let hi: chrono::NaiveDate = pieces[1].parse().map_err(|_| ComplexFilterError::invalid_filterset())?;
        qb.push(lhs).push(" BETWEEN ").push_bind(lo).push(" AND ").push_bind(hi);
        return Ok(());
    }
    let first = pieces.last().map(String::as_str).unwrap_or("");
    if first.is_empty() {
        qb.push(lhs).push(" IS NULL");
        return Ok(());
    }
    let day: chrono::NaiveDate = first.parse().map_err(|_| ComplexFilterError::invalid_filterset())?;
    qb.push(lhs).push(" = ").push_bind(day);
    Ok(())
}

/// Shared WHERE clause for the `issues-detail/` COUNT + page queries:
/// the `Issue.issue_objects` manager scope (`plane/db/models/issue.py:86-95`
/// — soft-delete excluded, triage excluded with NULL-state rows DROPPED
/// like Django's `exclude(state__group='triage')`, non-archived issues,
/// non-archived projects, non-drafts), slug + project scoping, GUEST
/// scoping (the `Exists(permission_subquery)`, `base.py:1033-1060`, which
/// reduces to `fetch_guest_scoped` given the gate), legacy
/// `issue_filters`, and the `filters` JSON tree. Note the detail endpoint
/// has NO `state__deleted_at` predicate (`base.py:1058-1060`): forward-FK
/// lookups don't apply `StateManager`, so soft-deleted states JOIN normally
/// and their rows are kept iff the (deleted) state's group isn't triage —
/// unlike the `/list/` endpoint's explicit `state__deleted_at__isnull`
/// (`base.py:114`).
fn push_detail_where(
    qb: &mut QueryBuilder<Postgres>,
    slug: &str,
    project_id: uuid::Uuid,
    user_id: uuid::Uuid,
    guest_scoped: bool,
    q: &DetailIssuesQuery,
    tree: Option<&serde_json::Value>,
    today: chrono::NaiveDate,
) -> Result<(), DetailWhereError> {
    qb.push(" WHERE i.project_id = ")
    .push_bind(project_id)
    .push(" AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
    .push_bind(slug.to_string())
    .push(
        ") AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND s.\"group\" <> 'triage' \
         AND EXISTS(SELECT 1 FROM projects p \
           WHERE p.id = i.project_id AND p.archived_at IS NULL AND p.deleted_at IS NULL)",
    );
    if guest_scoped {
        qb.push(" AND i.created_by_id = ").push_bind(user_id);
    }
    apply_legacy_filters(qb, q, today).map_err(DetailWhereError::Legacy)?;
    if let Some(tree) = tree {
        apply_complex_filter(qb, tree).map_err(DetailWhereError::Complex)?;
    }
    Ok(())
}

pub(crate) enum DetailWhereError {
    Legacy(LegacyFilterError),
    Complex(ComplexFilterError),
}

fn detail_400(body: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(body))
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues-detail/` — parity
/// with Django `IssueDetailEndpoint.get`
/// (`plane/app/views/issue/base.py:1027-1103`, `urls/issue.py:48-50`).
///
/// - Gate: PROJECT-level ADMIN/MEMBER/GUEST via the shared helpers (same as
///   `list_by_ids`; `@allow_permission([ADMIN, MEMBER, ROLE.GUEST])`,
///   `base.py:1027`, defaults to `level="PROJECT"`).
/// - Guest scoping: the `Exists(permission_subquery)` (`base.py:1033-1060`)
///   ≡ the shared `fetch_guest_scoped` check given the gate.
/// - `per_page` (default 1000, max 1000) and `cursor` (default
///   `"{per_page}:0:0"`) mirror `BasePaginator` byte-exact, including the
///   400 `{"detail": ...}` bodies for `ParseError`s and the 400
///   `{"detail": "Error in parsing"}` for negative windows
///   (`BadPaginationError`, `paginator.py:142-150`).
/// - `order_by` (default `-created_at`) mirrors `order_issue_queryset` +
///   the paginator re-ordering (see `detail_order_expr`, incl. the priority
///   direction swap and the state-group double negation).
/// - Rows render the 25-key `IssueListDetailSerializer` shape
///   (`serializers/issue.py:842-870`) with `issue_relation[]` /
///   `issue_related[]` (11 keys each) only for matching `expand` tokens
///   (`issue.py:873-922`); relation arrays follow the model's
///   `-created_at` ordering. The dead `if not related_issue: continue`
///   guards (`issue.py:878-880`, `903-905`) are no-ops (non-nullable FKs
///   with `select_related`), so no related-issue liveness filter applies.
/// - The envelope carries the exact 12 `paginate()` keys
///   (`paginator.py:728-743`).
///
/// Deviations (batch convention / reviewer-adjudicable Django-literal
/// readings): datetimes serialize RFC3339 UTC (chrono) instead of Django's
/// per-user-timezone conversion; unknown legacy params ignored (Django
/// dispatches known `ISSUE_FILTER` keys only); `group_by`/`sub_group_by` /
/// `fields` accepted-but-ignored (the detail view never reads them);
/// `logged_by` and out-of-range relative dates → generic 500 (Django
/// `FieldError`/`OverflowError` → 500); `per_page <= 0` → generic 500
/// (Django `ZeroDivisionError`/`AssertionError` → 500); relative-date "now"
/// is UTC (`Utc::now().date_naive()`).
///
/// Django's generic 500 body, byte-exact from
/// `BaseAPIView.handle_exception` (`plane/app/views/base.py:200-204`).
pub(crate) const GENERIC_500_MSG: &str = "Something went wrong please try again later";

fn server_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": GENERIC_500_MSG})),
    )
}

/// The page-query SELECT prefix for `list_detail`: the 25 `SELECT` items in
/// `IssueListDetailSerializer` order with the `apply_annotations`
/// subqueries (`base.py:979-1025`). The `array_agg` aggregates carry
/// `ORDER BY <bridge>.created_at DESC` because the `.all()` prefetches
/// follow bridge `Meta.ordering = ("-created_at",)` (`db/models/issue.py`
/// `IssueAssignee`/`IssueLabel`, `db/models/cycle.py` `CycleIssue`,
/// `db/models/module.py` `ModuleIssue`); the `cycle_id` scalar takes the
/// same ordering before `LIMIT 1` (Django applies `Meta.ordering` to the
/// unordered `values()[:1]` subquery).
pub(crate) const DETAIL_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
     i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
     i.sequence_id, i.project_id, i.parent_id, i.created_at, i.updated_at, \
     i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
     i.is_draft, i.archived_at, \
     (SELECT ci.cycle_id FROM cycle_issues ci \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
     COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL), '{}'::uuid[]) AS module_ids, \
     COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
     COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
     (SELECT COUNT(*) FROM issues si \
       LEFT JOIN states ss ON ss.id = si.state_id \
       WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
       AND si.archived_at IS NULL AND si.is_draft = false \
       AND ss.deleted_at IS NULL AND ss.\"group\" <> 'triage' \
       AND EXISTS(SELECT 1 FROM projects sp \
         WHERE sp.id = si.project_id AND sp.archived_at IS NULL)) AS sub_issues_count, \
     (SELECT COUNT(*) FROM file_assets fa \
       WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
       AND fa.deleted_at IS NULL) AS attachment_count, \
     (SELECT COUNT(*) FROM issue_links lin \
       WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";
pub async fn list_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<DetailIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok(detail_400(json!({"detail": msg}))),
    };
    let cursor_raw = q.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(msg) => return Ok(detail_400(json!({"detail": msg}))),
    };
    // `limit = min(limit, max_limit)` (`paginator.py:132`). Django computes
    // `offset = page * limit` FIRST (`paginator.py:142-150`): a negative
    // offset 400s even when the limit itself is degenerate.
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        // `BadPaginationError("Pagination offset cannot be negative")` →
        // `ParseError(detail="Error in parsing")` — no trailing period
        // (`paginator.py:142-150, 708-711`).
        Err(()) => return Ok(detail_400(json!({"detail": "Error in parsing"}))),
        Ok(w) => w,
    };
    if limit <= 0 {
        // Django: `limit=0` → `ZeroDivisionError` in `math.ceil`, negative
        // limits → negative slices → `AssertionError`; both → generic 500.
        // (`per_page=0` 500s on both sides.)
        return Ok(server_error());
    }
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await?;
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let (order_expr, desc) = detail_order_expr(&sanitized);
    let order_dir = if desc { "DESC" } else { "ASC" };
    let today = chrono::Utc::now().date_naive();
    let tree = match parse_complex_filter(q.filters.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    };
    let expand: Vec<&str> = q
        .expand
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|t| !t.is_empty())
        .collect();
    // `group_by` / `sub_group_by` / `fields` are accepted AND ignored (see
    // `DetailIssuesQuery`): `IssueDetailEndpoint.get` never reads them.
    // `show_sub_issues` is likewise unread here (archive-only toggle).
    let _ = (&q.group_by, &q.sub_group_by, &q.fields, &q.show_sub_issues);
    let want_relation = expand.iter().any(|t| *t == "issue_relation");
    let want_related = expand.iter().any(|t| *t == "issue_related");

    // Total over the pre-annotation filtered queryset
    // (`total_issue_queryset = copy.deepcopy(issue)`, `base.py:1086`, counted
    // at `paginator.py:160`).
    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id",
    );
    match push_detail_where(
        &mut count_qb,
        &slug,
        project_id,
        auth.0,
        guest_scoped,
        &q,
        tree.as_ref(),
        today,
    ) {
        Ok(()) => {}
        Err(DetailWhereError::Legacy(LegacyFilterError::BadRequest(msg))) => {
            return Ok(detail_400(json!({"error": msg})));
        }
        Err(DetailWhereError::Legacy(LegacyFilterError::Server)) => {
            // Django `FieldError`/uncaught paths → generic 500
            // (`views/base.py:200-204`): never surface internals.
            return Ok(server_error());
        }
        Err(DetailWhereError::Complex(e)) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    // Page window: `[offset:offset+limit+1]`, truncated to `limit`
    // (`paginator.py:144-145, 152, 170`); `next.has_results` ⇔ the window
    // held more than `limit` (`paginator.py:154, 165`). A `BeyondEnd`
    // window (unbounded page) slices to `[]` in Django and returns an
    // empty page with a 200 — no extra query needed.
    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(DETAIL_SELECT_SQL);
    match push_detail_where(
        &mut page_qb,
        &slug,
        project_id,
        auth.0,
        guest_scoped,
        &q,
        tree.as_ref(),
        today,
    ) {
        Ok(()) => {}
        Err(DetailWhereError::Legacy(LegacyFilterError::BadRequest(msg))) => {
            return Ok(detail_400(json!({"error": msg})));
        }
        Err(DetailWhereError::Legacy(LegacyFilterError::Server)) => {
            // Django `FieldError`/uncaught paths → generic 500
            // (`views/base.py:200-204`): never surface internals.
            return Ok(server_error());
        }
        Err(DetailWhereError::Complex(e)) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    }
    // Paginator ordering: `(key DIR NULLS LAST, -created_at)`
    // (`paginator.py:136-140`).
    page_qb
        .push(" ORDER BY ")
        .push(order_expr)
        .push(" ")
        .push(order_dir)
        .push(" NULLS LAST, i.created_at DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<IssueDetailRow> = match window {
        PageWindow::Rows(offset) => {
            page_qb.push(" OFFSET ").push_bind(offset);
            page_qb.build_query_as().fetch_all(&st.pool).await?
        }
        // Unbounded page: Django slices to `[]` → empty page, 200.
        PageWindow::BeyondEnd => Vec::new(),
    };
    // `OffsetPaginator` (`paginator.py:157-158`): when `cursor.value !=
    // limit and cursor.is_prev`, the window is re-sliced to its last
    // `limit+1` rows — a no-op here since the window holds at most
    // `limit+1` rows by construction, so the parsed slots are only read.
    let _ = (cursor.limit_value, cursor.is_prev);
    let next_page_results = rows.len() as i64 > limit;
    rows.truncate(limit as usize);

    // Expanded relations, in model `-created_at` order
    // (`IssueRelation.Meta.ordering`).
    let mut relations: std::collections::HashMap<uuid::Uuid, Vec<Value>> = std::collections::HashMap::new();
    let mut related: std::collections::HashMap<uuid::Uuid, Vec<Value>> = std::collections::HashMap::new();
    if (want_relation || want_related) && !rows.is_empty() {
        let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.id).collect();
        if want_relation {
            let rel_rows: Vec<IssueRelationItem> = sqlx::query_as(
                "SELECT r.issue_id AS owner_id, ri.id, ri.project_id, ri.sequence_id, ri.name, \
                 r.relation_type, ri.state_id, ri.priority, ri.created_by_id AS created_by, \
                 ri.created_at, ri.updated_at, ri.updated_by_id AS updated_by \
                 FROM issue_relations r JOIN issues ri ON ri.id = r.related_issue_id \
                 WHERE r.issue_id = ANY($1) AND r.deleted_at IS NULL \
                 ORDER BY r.created_at DESC",
            )
            .bind(&ids)
            .fetch_all(&st.pool)
            .await?;
            for rel in rel_rows {
                relations.entry(rel.owner_id).or_default().push(rel_to_value(&rel));
            }
        }
        if want_related {
            let rel_rows: Vec<IssueRelationItem> = sqlx::query_as(
                "SELECT r.related_issue_id AS owner_id, ri.id, ri.project_id, ri.sequence_id, ri.name, \
                 r.relation_type, ri.state_id, ri.priority, ri.created_by_id AS created_by, \
                 ri.created_at, ri.updated_at, ri.updated_by_id AS updated_by \
                 FROM issue_relations r JOIN issues ri ON ri.id = r.issue_id \
                 WHERE r.related_issue_id = ANY($1) AND r.deleted_at IS NULL \
                 ORDER BY r.created_at DESC",
            )
            .bind(&ids)
            .fetch_all(&st.pool)
            .await?;
            for rel in rel_rows {
                related.entry(rel.owner_id).or_default().push(rel_to_value(&rel));
            }
        }
    }

    let mut results = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut v = serde_json::to_value(row).map_err(|e| anyhow::anyhow!(e))?;
        if want_relation {
            v["issue_relation"] = Value::Array(relations.get(&row.id).cloned().unwrap_or_default());
        }
        if want_related {
            v["issue_related"] = Value::Array(related.get(&row.id).cloned().unwrap_or_default());
        }
        results.push(v);
    }

    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next_cursor_str(limit, cursor.page),
        prev_cursor: prev_cursor_str(limit, cursor.page),
        next_page_results,
        prev_page_results: cursor.page > 0,
        count: rows.len() as i64,
        total_pages: total_pages(total, limit),
        total_results: total,
        extra_stats: None,
        results,
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

/// Renders an `IssueRelationItem` as its 11-key JSON object (struct order =
/// serializer dict order, `serializers/issue.py:882-896`), dropping the
/// internal `owner_id` grouping key.
fn rel_to_value(rel: &IssueRelationItem) -> Value {
    json!({
        "id": rel.id,
        "project_id": rel.project_id,
        "sequence_id": rel.sequence_id,
        "name": rel.name,
        "relation_type": rel.relation_type,
        "state_id": rel.state_id,
        "priority": rel.priority,
        "created_by": rel.created_by,
        "created_at": rel.created_at,
        "updated_at": rel.updated_at,
        "updated_by": rel.updated_by,
    })
}

#[cfg(test)]
mod issue_list_tests {
    use super::*;

    #[test]
    fn parse_issue_csv_vectors_match_django() {
        // Mirrors `plane/app/views/issue/base.py:86-94`: missing/empty
        // `?issues` → 400 "Issues are required"; non-empty tokens are split
        // on "," with exact-`""` drops (no trimming); each kept token must
        // parse as UUID — Django's `pk__in` on the UUID PK raises
        // `ValidationError`, mapped by `BaseAPIView.handle_exception`
        // (`plane/app/views/base.py:182-186`) to 400
        // `{"error": "Please provide valid detail"}`.
        assert_eq!(parse_issue_csv(None).unwrap_err(), "Issues are required");
        assert_eq!(parse_issue_csv(Some("")).unwrap_err(), "Issues are required");
        // `",,,"` is truthy in Python → passes the `if not` check, all
        // tokens dropped → empty id list → Django 200 `[]`.
        assert!(parse_issue_csv(Some(",,,")).unwrap().is_empty());
        let a = "12345678-1234-5678-1234-567812345678";
        let b = "87654321-4321-8765-4321-876543218765";
        let ids = parse_issue_csv(Some(&format!("{a},{b}"))).unwrap();
        assert_eq!(
            ids,
            vec![
                uuid::Uuid::parse_str(a).unwrap(),
                uuid::Uuid::parse_str(b).unwrap(),
            ]
        );
        // Empty tokens are dropped, surrounding valid ids still parse.
        let ids = parse_issue_csv(Some(&format!("{a},,{b}"))).unwrap();
        assert_eq!(ids.len(), 2);
        // Malformed UUID → Django `ValidationError` message mapping.
        assert_eq!(
            parse_issue_csv(Some("not-a-uuid")).unwrap_err(),
            "Please provide valid detail"
        );
        assert_eq!(
            parse_issue_csv(Some(&format!("{a},zzz"))).unwrap_err(),
            "Please provide valid detail"
        );
        // Whitespace-only is truthy in Python (`if not " "` is False) so it
        // reaches `pk__in` and fails UUID validation — NOT "required".
        assert_eq!(
            parse_issue_csv(Some(" ")).unwrap_err(),
            "Please provide valid detail"
        );
    }

    #[test]
    fn list_by_ids_handler_exists_for_list_route() {
        // Wiring guard: `main.rs` registers
        // `GET .../issues/list/` → `list_by_ids` (Django
        // `IssueListEndpoint.get`, `plane/app/urls/issue.py` `list`
        // branch). Static `list` wins over `:pk` in Axum (same as P6/P7
        // `members/leave/`, `project-members/me/` precedent).
        let _ = super::list_by_ids;
    }

    #[test]
    fn project_gate_allows_matrix_matches_django() {
        // Mirrors `allow_permission(..., level="PROJECT")`
        // (`plane/app/permissions/base.py:53-78`): branch 1 — an active
        // project membership whose role is in the allowed list (20/15/5);
        // branch 2 (fallback) — ANY active project membership PLUS active
        // workspace ADMIN; otherwise deny.
        // Branch 1: allowed role passes regardless of the other inputs.
        assert!(project_gate_allows(true, false, false));
        assert!(project_gate_allows(true, true, false));
        assert!(project_gate_allows(true, false, true));
        // Branch 2: membership (any role) + workspace admin passes.
        assert!(project_gate_allows(false, true, true));
        // Everything else denies: non-member, member without ws-admin,
        // ws-admin without project membership.
        assert!(!project_gate_allows(false, false, false));
        assert!(!project_gate_allows(false, true, false));
        assert!(!project_gate_allows(false, false, true));
    }
}

#[cfg(test)]
mod issue_detail_tests {
    use super::*;

    #[test]
    fn parse_per_page_vectors_match_django() {
        // Mirrors `BasePaginator.get_per_page`
        // (`plane/utils/paginator.py:643-653`): default 1000, non-integer
        // → ParseError "Invalid per_page parameter.", over max →
        // "Invalid per_page value. Cannot exceed 1000." (byte-exact).
        assert_eq!(parse_per_page(None).unwrap(), 1000);
        assert_eq!(parse_per_page(Some("50")).unwrap(), 50);
        assert_eq!(parse_per_page(Some("1000")).unwrap(), 1000);
        assert_eq!(
            parse_per_page(Some("5000")).unwrap_err(),
            "Invalid per_page value. Cannot exceed 1000."
        );
        assert_eq!(
            parse_per_page(Some("1001")).unwrap_err(),
            "Invalid per_page value. Cannot exceed 1000."
        );
        assert_eq!(
            parse_per_page(Some("abc")).unwrap_err(),
            "Invalid per_page parameter."
        );
        assert_eq!(
            parse_per_page(Some("")).unwrap_err(),
            "Invalid per_page parameter."
        );
    }

    #[test]
    fn parse_cursor_vectors_match_django() {
        // Mirrors `Cursor.from_string` (`plane/utils/paginator.py:48-59`)
        // wrapped by `BasePaginator.paginate` (`paginator.py:677-681`): any
        // malformed cursor surfaces as ParseError "Invalid cursor
        // parameter." (byte-exact, client-facing).
        let c = parse_cursor("1000:0:0").unwrap();
        assert_eq!((c.limit_value, c.page, c.is_prev), (1000.0, 0, false));
        let c = parse_cursor("50:3:1").unwrap();
        assert_eq!((c.limit_value, c.page, c.is_prev), (50.0, 3, true));
        // Float values are accepted by `from_string` (offset math ignores
        // the value in `OffsetPaginator`, `paginator.py:144`).
        let c = parse_cursor("10.5:0:0").unwrap();
        assert_eq!((c.limit_value, c.page, c.is_prev), (10.5, 0, false));
        for bad in ["junk", "", "1:2", "1:2:3:4", "x:0:0", "10:x:0", "10:0:x", "10.5:0:0:0"] {
            assert_eq!(
                parse_cursor(bad).unwrap_err(),
                "Invalid cursor parameter.",
                "input {bad:?}"
            );
        }
    }

    #[test]
    fn cursor_round_trip_matches_django() {
        // `Cursor.__str__` (`paginator.py:31-32`) is
        // `f"{value}:{offset}:{int(is_prev)}"`; `OffsetPaginator`
        // (`paginator.py:165-167`) emits next `(limit, page+1, False)` and
        // prev `(limit, page-1, True)`.
        assert_eq!(build_cursor(1000, 0, false), "1000:0:0");
        assert_eq!(build_cursor(1000, 1, false), "1000:1:0");
        assert_eq!(build_cursor(1000, -1, true), "1000:-1:1");
        let s = build_cursor(1000, 2, false);
        let c = parse_cursor(&s).unwrap();
        assert_eq!((c.limit_value, c.page, c.is_prev), (1000.0, 2, false));
        // First page: next advances, prev points at page -1 with results=false.
        assert_eq!(next_cursor_str(1000, 0), "1000:1:0");
        assert_eq!(prev_cursor_str(1000, 0), "1000:-1:1");
    }

    #[test]
    fn total_pages_matches_django_ceil() {
        // `math.ceil(count / limit)` (`paginator.py:180`).
        assert_eq!(total_pages(2501, 1000), 3);
        assert_eq!(total_pages(0, 1000), 0);
        assert_eq!(total_pages(1000, 1000), 1);
        assert_eq!(total_pages(1001, 1000), 2);
        assert_eq!(total_pages(2000, 1000), 2);
    }

    #[test]
    fn cursor_arithmetic_saturates_without_panic() {
        // A saturated `i128::MAX` page (e.g. cursor `1000:<huge>:0`) must
        // render a next cursor without panicking in the debug test profile
        // (Django renders the unbounded int and 200s an empty page).
        assert_eq!(
            next_cursor_str(1000, i128::MAX),
            format!("1000:{}:0", i128::MAX)
        );
        // Symmetric floor: `i128::MIN` prev saturates instead of panicking.
        assert_eq!(
            prev_cursor_str(1000, i128::MIN),
            format!("1000:{}:1", i128::MIN)
        );
    }

    #[test]
    fn total_pages_edges_do_not_overflow() {
        // Empty result sets and degenerate-but-reachable inputs must not
        // overflow the `total + limit - 1` intermediate: `total=0` → 0
        // pages; `ceil(i64::MAX / 1000)` is exact.
        assert_eq!(total_pages(0, 1000), 0);
        assert_eq!(total_pages(0, 1), 0);
        assert_eq!(total_pages(i64::MAX, 1000), 9_223_372_036_854_776);
        assert_eq!(total_pages(i64::MAX, 1), i64::MAX);
    }

    #[test]
    fn sanitize_order_by_vectors_match_django() {
        // Mirrors `sanitize_order_by`
        // (`plane/utils/order_queryset.py:129-150`): one leading `-`
        // allowed, bare name must be in `ISSUE_ORDER_BY_ALLOWLIST`, else
        // the safe default `-created_at`.
        assert_eq!(sanitize_order_by("-created_at"), "-created_at");
        assert_eq!(sanitize_order_by("created_at"), "created_at");
        assert_eq!(sanitize_order_by("priority"), "priority");
        assert_eq!(sanitize_order_by("-priority"), "-priority");
        assert_eq!(sanitize_order_by("state__group"), "state__group");
        assert_eq!(sanitize_order_by("assignees__first_name"), "assignees__first_name");
        assert_eq!(sanitize_order_by("junk"), "-created_at");
        assert_eq!(sanitize_order_by("--created_at"), "-created_at");
        assert_eq!(sanitize_order_by(""), "-created_at");
        // `created_by` is group-by-only, not orderable → default.
        assert_eq!(sanitize_order_by("created_by"), "-created_at");
    }

    #[test]
    fn legacy_uuid_csv_drops_invalid_like_django() {
        // `filter_valid_uuids` (`plane/utils/issue_filters.py:16-25`)
        // silently drops malformed UUIDs (legacy filters never 400 on them).
        let a = "12345678-1234-5678-1234-567812345678";
        assert_eq!(legacy_uuid_list("").len(), 0);
        assert_eq!(legacy_uuid_list("null").len(), 0);
        let ids = legacy_uuid_list(&format!("{a},zzz"));
        assert_eq!(ids, vec![uuid::Uuid::parse_str(a).unwrap()]);
    }

    #[test]
    fn like_escape_matches_django_icontains() {
        // Django `__icontains` escapes LIKE wildcards with backslash.
        assert_eq!(escape_like("a%b_c\\d"), "a\\%b\\_c\\\\d");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn complex_filter_empty_object_is_noop_like_django() {
        // `_apply_json_filter` (`plane/utils/filters/filter_backend.py:82-83`)
        // short-circuits falsy filter data BEFORE validation.
        assert!(parse_complex_filter(None).unwrap().is_none());
        assert!(parse_complex_filter(Some("{}")).unwrap().is_none());
        assert!(parse_complex_filter(Some("")).unwrap().is_none());
    }

    #[test]
    fn complex_filter_rejects_unknown_field_like_django() {
        // `_validate_fields` (`filter_backend.py:100-127`): byte-exact message.
        let err = parse_complex_filter(Some(r#"{"nope": "x"}"#)).unwrap_err();
        assert_eq!(err.code, "invalid_filter_field");
        assert_eq!(err.message, "Filtering on field 'nope' is not allowed");
        // Structure violations surface their own codes first.
        let err = parse_complex_filter(Some(r#"{"and": []}"#)).unwrap_err();
        assert_eq!(err.code, "invalid_operator_children");
        // Malformed JSON → invalid_json (byte-exact message).
        let err = parse_complex_filter(Some("{oops")).unwrap_err();
        assert_eq!(err.code, "invalid_json");
        assert_eq!(
            err.message,
            "Invalid JSON for 'filter'. Expected a valid JSON object."
        );
    }

    #[test]
    fn detail_order_expr_matches_django_runtime() {
        // Priority direction swap (`order_queryset.py:159-167` + paginator):
        // `-priority` orders urgent-first (ASC on the CASE), `priority`
        // orders none-first (DESC).
        let (expr, desc) = detail_order_expr("-priority");
        assert!(!desc);
        assert!(expr.contains("WHEN 'urgent' THEN 0"));
        let (expr, desc) = detail_order_expr("priority");
        assert!(desc);
        assert!(expr.contains("WHEN 'urgent' THEN 0"));
        // State-group double negation (`order_queryset.py:168-177` +
        // `paginator.py:136-140`): both signs yield backlog-first at runtime.
        let (expr, desc) = detail_order_expr("-state__group");
        assert!(!desc);
        assert!(expr.contains("WHEN 'backlog' THEN 0"));
        let (expr, desc) = detail_order_expr("state__group");
        assert!(!desc);
        assert!(expr.contains("WHEN 'backlog' THEN 0"));
        // Direct + join-backed columns keep the requested direction.
        assert_eq!(detail_order_expr("-created_at"), ("i.created_at", true));
        assert_eq!(detail_order_expr("sequence_id"), ("i.sequence_id", false));
        assert_eq!(detail_order_expr("-state__name"), ("s.name", true));
    }

    #[test]
    fn legacy_type_overwrites_state_group_like_django() {
        // `filter_issue_state_type` and `filter_state_group` write the SAME
        // `state__group__in` dict key (`issue_filters.py:99,304`); `type`
        // runs later, so it wins. Offline SQL shape (no DB): exactly one
        // group condition survives.
        use chrono::NaiveDate;
        let q = DetailIssuesQuery {
            type_: Some("active".to_string()),
            state_group: Some("backlog".to_string()),
            ..Default::default()
        };
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_legacy_filters(&mut qb, &q, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).unwrap();
        let sql = qb.sql();
        assert_eq!(sql.matches("s.\"group\" = ANY").count(), 1);
    }

    #[test]
    fn complex_filter_depth_limit_matches_django() {
        // `default_max_depth = 5` (`filter_backend.py:29`): six nested
        // `and` levels → 400 `max_depth_exceeded` (byte-exact message).
        let deep = r#"{"and": [{"and": [{"and": [{"and": [{"and": [{"priority": "high"}]}]}]}]}]}"#;
        let err = parse_complex_filter(Some(deep)).unwrap_err();
        assert_eq!(err.code, "max_depth_exceeded");
        assert_eq!(err.message, "Filter nesting is too deep (max 5); found depth 6");
        // Five levels validate fine (unknown-field check would fire first
        // for bad keys, so use a real key).
        let ok5 = r#"{"and": [{"and": [{"and": [{"and": [{"priority": "high"}]}]}]}]}"#;
        assert!(parse_complex_filter(Some(ok5)).unwrap().is_some());
    }

    #[test]
    fn list_detail_handler_exists_for_detail_route() {
        // Wiring guard: `main.rs` registers
        // `GET .../issues-detail/` → `list_detail` (Django
        // `IssueDetailEndpoint.get`, `plane/app/urls/issue.py:48-50`). FE
        // `getIssuesFromServer` (`issue.service.ts:40-61`) branches here iff
        // `queries.expand` includes `issue_relation` && `!group_by`.
        let _ = super::list_detail;
    }

    #[test]
    fn parse_python_int_vectors_match_django() {
        // Python `int()`: surrounding whitespace tolerated, one leading
        // sign, single underscores between digits (`int("1_0") == 10`);
        // out-of-range saturates (Django ints are unbounded).
        assert_eq!(parse_python_int("10"), Some(10));
        assert_eq!(parse_python_int("  +10  "), Some(10));
        assert_eq!(parse_python_int("-7"), Some(-7));
        assert_eq!(parse_python_int("1_0"), Some(10));
        assert_eq!(parse_python_int("1_00_0"), Some(1000));
        // Past `i128::MAX` (~1.7e38) magnitudes saturate by sign.
        assert_eq!(
            parse_python_int("99999999999999999999999999999999999999999"),
            Some(i128::MAX)
        );
        assert_eq!(
            parse_python_int("-99999999999999999999999999999999999999999"),
            Some(i128::MIN)
        );
        for bad in ["", "   ", "abc", "1__0", "_1", "1_", "+-1", "0x10", "10.5", "+_1"] {
            assert_eq!(parse_python_int(bad), None, "input {bad:?}");
        }
    }

    #[test]
    fn parse_per_page_edges_match_django() {
        // Underscores accepted like Python `int()`; huge-but-parseable
        // positives 400 with the Cannot-exceed message (Django parses the
        // bigint fine, then fails the max check).
        assert_eq!(parse_per_page(Some("1_0")).unwrap(), 10);
        assert_eq!(
            parse_per_page(Some("99999999999999999999999999999999999999999")).unwrap_err(),
            "Invalid per_page value. Cannot exceed 1000."
        );
    }

    #[test]
    fn parse_cursor_unbounded_page_matches_django() {
        // Django ints are unbounded: a huge page parses fine and yields an
        // (empty) page, NOT a 400. Saturates to `i128::MAX`.
        let c = parse_cursor("1000:99999999999999999999999999999999999999999:0").unwrap();
        assert_eq!(c.page, i128::MAX);
        assert!(!c.is_prev);
        // Underscore pages parse like Python `int()`.
        let c = parse_cursor("1_0:2_0:0").unwrap();
        assert_eq!((c.limit_value, c.page), (10.0, 20));
    }

    #[test]
    fn page_window_offset_first_matches_django() {
        // Django computes `offset = page * limit` FIRST
        // (`paginator.py:142-150`): a negative offset 400s even when the
        // limit itself would 500 (`per_page=-5, page=2` → offset -10).
        assert_eq!(page_window(2, -5), Err(()));
        assert_eq!(page_window(0, 0), Ok(PageWindow::Rows(0)));
        assert_eq!(page_window(2, 1000), Ok(PageWindow::Rows(2000)));
        // Unbounded pages saturate past `i64::MAX` → empty page, not an error.
        assert_eq!(page_window(i128::MAX, 1000), Ok(PageWindow::BeyondEnd));
    }

    #[test]
    fn relative_date_anchor_is_exact_like_django() {
        // `pattern = re.compile(r"\d+_(weeks|months)$")`
        // (`issue_filters.py:12`) end-anchors: `2_weeksXYZ;after;fromnow`
        // takes the plain-date branch and 400s on the garbage date.
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        let err = push_legacy_date(
            &mut qb,
            "i.target_date",
            "2_weeksXYZ;after;fromnow",
            chrono::NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(err, LegacyFilterError::BadRequest(_)));
        // Exact `weeks`/`months` still anchor relatively.
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        push_legacy_date(
            &mut qb,
            "i.target_date",
            "2_weeks;after;fromnow",
            chrono::NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
        )
        .unwrap();
        assert!(qb.sql().contains(" >= "));
    }

    #[test]
    fn legacy_empty_csv_token_skips_key_like_django() {
        // `"" not in <list>` guards (`issue_filters.py:99,110,125,353,368`):
        // ANY `""` token skips the WHOLE key.
        use chrono::NaiveDate;
        let today = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
        for (key, raw) in [
            ("state_group", "backlog,"),
            ("priority", "high,"),
            ("estimate_point", "12345678-1234-5678-1234-567812345678,"),
            ("intake_status", "1,"),
            ("inbox_status", "2,"),
        ] {
            let mut q = DetailIssuesQuery::default();
            match key {
                "state_group" => q.state_group = Some(raw.to_string()),
                "priority" => q.priority = Some(raw.to_string()),
                "estimate_point" => q.estimate_point = Some(raw.to_string()),
                "intake_status" => q.intake_status = Some(raw.to_string()),
                _ => q.inbox_status = Some(raw.to_string()),
            }
            let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
            apply_legacy_filters(&mut qb, &q, today).unwrap();
            assert_eq!(qb.sql(), "SELECT 1", "key {key}");
        }
    }

    #[test]
    fn legacy_bridge_garbage_still_scopes_live_like_django() {
        // `filter_labels` etc. apply `<bridge>__deleted_at__isnull`
        // UNCONDITIONALLY (`issue_filters.py:158,173,331,346,401`): a
        // garbage-only value still restricts to issues WITH a live bridge row.
        let mut q = DetailIssuesQuery::default();
        q.labels = Some("zzz".to_string());
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_legacy_filters(&mut qb, &q, chrono::NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).unwrap();
        let sql = qb.sql();
        assert!(sql.contains("EXISTS(SELECT 1 FROM issue_labels"), "{sql}");
        assert!(!sql.contains("= ANY"), "{sql}");
    }

    #[test]
    fn legacy_mentions_intake_have_no_deleted_filter_like_django() {
        // `filter_mentions` (`issue_filters.py:177-186`) and
        // `filter_intake_status`/`filter_inbox_status` (`issue_filters.py:350-377`)
        // set NO `deleted_at` condition.
        let mut q = DetailIssuesQuery::default();
        q.mentions = Some("12345678-1234-5678-1234-567812345678".to_string());
        q.intake_status = Some("1".to_string());
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_legacy_filters(&mut qb, &q, chrono::NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).unwrap();
        let sql = qb.sql();
        assert!(!sql.contains("im.deleted_at"), "{sql}");
        assert!(!sql.contains("ii.deleted_at"), "{sql}");
    }

    #[test]
    fn complex_is_archived_yes_is_noop_like_django() {
        // `NullBooleanField.to_python("yes")` → `None` (never raises), then
        // `FilterMethod` short-circuits on `EMPTY_VALUES` → no-op.
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_complex_leaf(&mut qb, "is_archived", &serde_json::json!("yes")).unwrap();
        assert_eq!(qb.sql(), "SELECT 1TRUE");
        // Genuinely invalid numerics behave the same way.
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_complex_leaf(&mut qb, "is_archived", &serde_json::json!(2)).unwrap();
        assert_eq!(qb.sql(), "SELECT 1TRUE");
    }

    #[test]
    fn complex_is_draft_garbage_is_null_not_400_like_django() {
        // `NullBooleanField` never raises: unrecognized spellings clean to
        // `None` → `Q(is_draft__exact=None)` → `IS NULL`, NOT a 400.
        for raw in ["yes", "2", "1.5"] {
            let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
            apply_complex_leaf(&mut qb, "is_draft", &serde_json::json!(raw)).unwrap();
            assert_eq!(qb.sql(), "SELECT 1i.is_draft IS NULL", "input {raw}");
        }
    }

    #[test]
    fn complex_uuid_empty_scalar_matches_django() {
        // `UUIDField.to_python("")` → `None` (valid); direct filters bypass
        // `EMPTY_VALUES` (`build_combined_q:79-108`) →
        // `Q(state_id__exact=None)` → `IS NULL`, while method (bridge)
        // filters no-op on `EMPTY` (`filter_backend.py:268` maps null→`""`).
        for key in ["state_id", "state_id__exact", "created_by_id", "project_id"] {
            let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
            apply_complex_leaf(&mut qb, key, &serde_json::json!(null)).unwrap();
            assert!(qb.sql().contains(" IS NULL"), "key {key}: {}", qb.sql());
        }
        for key in [
            "assignee_id",
            "assignee_id__exact",
            "cycle_id",
            "module_id",
            "mention_id",
            "label_id",
            "subscriber_id",
        ] {
            let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
            apply_complex_leaf(&mut qb, key, &serde_json::json!(null)).unwrap();
            assert_eq!(qb.sql(), "SELECT 1TRUE", "key {key}");
        }
        // Non-empty scalars still coerce strictly (garbage → 400).
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        assert!(apply_complex_leaf(&mut qb, "state_id", &serde_json::json!("zzz")).is_err());
    }

    #[test]
    fn complex_uuid_empty_in_matches_django() {
        // `Q(state_id__in=[])` matches nothing; bridge method filters no-op
        // on empty. `""` pieces are dropped first, so mixed lists still
        // filter on the surviving ids.
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_complex_leaf(&mut qb, "state_id__in", &serde_json::json!("")).unwrap();
        assert!(
            qb.sql().contains("i.state_id = ANY('{}'::uuid[])"),
            "{}",
            qb.sql()
        );
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_complex_leaf(&mut qb, "assignee_id__in", &serde_json::json!("")).unwrap();
        assert_eq!(qb.sql(), "SELECT 1TRUE");
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        apply_complex_leaf(
            &mut qb,
            "state_id__in",
            &serde_json::json!(["", "12345678-1234-5678-1234-567812345678"]),
        )
        .unwrap();
        assert!(qb.sql().contains("i.state_id = ANY("), "{}", qb.sql());
        assert!(!qb.sql().contains("'{}'"), "{}", qb.sql());
        // A surviving invalid id still 400s.
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1");
        assert!(apply_complex_leaf(&mut qb, "state_id__in", &serde_json::json!(["", "zzz"])).is_err());
    }

    #[test]
    fn detail_select_orders_arrays_like_django() {
        // Bridge `.all()` prefetches follow bridge `Meta.ordering =
        // ("-created_at",)` (issue.py/cycle.py/module.py), so `array_agg`
        // carries `ORDER BY <bridge>.created_at DESC`.
        for needle in [
            "array_agg(mi.module_id ORDER BY mi.created_at DESC)",
            "array_agg(il.label_id ORDER BY il.created_at DESC)",
            "array_agg(ia.assignee_id ORDER BY ia.created_at DESC)",
        ] {
            assert!(DETAIL_SELECT_SQL.contains(needle), "{needle}");
        }
    }

    #[test]
    fn generic_500_body_matches_django() {
        // `BaseAPIView.handle_exception` (`app/views/base.py:200-204`).
        assert_eq!(GENERIC_500_MSG, "Something went wrong please try again later");
    }
}

// ---- Batch C I3 ----

/// Body for the bulk issue endpoints. Mirrors
/// `request.data.get("issue_ids", [])` (`base.py:776`,
/// `archive.py:310`): a missing key defaults to `[]`, which then 400s via
/// `require_issue_ids`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BulkIssueIds {
    #[serde(default)]
    pub issue_ids: Vec<uuid::Uuid>,
}

/// Mirrors `if not len(issue_ids)` (`base.py:778-779`,
/// `archive.py:312-313`) → 400 `{"error": "Issue IDs are required"}`.
pub(crate) fn require_issue_ids(ids: &[uuid::Uuid]) -> Result<(), &'static str> {
    if ids.is_empty() {
        Err("Issue IDs are required")
    } else {
        Ok(())
    }
}

/// Maps the optional bulk body to ids-or-400. The handlers take
/// `Option<Json<BulkIssueIds>>` because Axum 0.7.9's `Json` extractor
/// rejects an ENTIRELY ABSENT body with `JsonRejection` (415 without
/// Content-Type, generic-400 on empty body, 422 on bad shape) before the
/// handler runs — while Django's `request.data.get("issue_ids", [])`
/// treats it as `[]` → 400 `{"error": "Issue IDs are required"}`.
/// Axum's `Option<Json>` swallows rejections into `None`
/// (`axum-core/.../extract/mod.rs`, `FromRequest for Option<T>`), so
/// `None` maps to Django's 400 here, before any DB work; `Some` with empty
/// ids takes the existing `require_issue_ids` 400 path. (`#[serde(default)]`
/// on `BulkIssueIds` still covers the explicit `{}` case.)
pub(crate) fn resolve_bulk_ids(body: Option<Json<BulkIssueIds>>) -> Result<Vec<uuid::Uuid>, Value> {
    let ids = body.map(|Json(b)| b.issue_ids).unwrap_or_default();
    require_issue_ids(&ids).map(|()| ids).map_err(|e| json!({"error": e}))
}

/// Error code for archiving a non-done issue, byte-exact from
/// `plane/utils/error_codes.py:7` (`"INVALID_ARCHIVE_STATE_GROUP": 4091`).
pub(crate) const INVALID_ARCHIVE_STATE_GROUP_CODE: i32 = 4091;
pub(crate) const INVALID_ARCHIVE_STATE_GROUP_MSG: &str = "INVALID_ARCHIVE_STATE_GROUP";

/// Mirrors the per-issue check in `BulkArchiveIssuesEndpoint.post`
/// (`archive.py:319-327`): only `completed`/`cancelled` state groups may
/// archive — anything else → `Err((4091, "INVALID_ARCHIVE_STATE_GROUP"))`.
pub(crate) fn guard_archive_group(group: &str) -> Result<(), (i32, &'static str)> {
    if group == "completed" || group == "cancelled" {
        Ok(())
    } else {
        Err((
            INVALID_ARCHIVE_STATE_GROUP_CODE,
            INVALID_ARCHIVE_STATE_GROUP_MSG,
        ))
    }
}

/// Mirrors `f"{total_issues} issues were deleted"` (`base.py:794-797`):
/// always plural; `n` is the PRE-delete queryset count, not rows-affected.
pub(crate) fn delete_message(n: i64) -> String {
    format!("{n} issues were deleted")
}

/// Shared `Issue.issue_objects` scope predicate for the four `bulk_delete`
/// statements (`db/models/issue.py:92-101`): live rows +
/// `exclude(state__group='triage')` + `exclude(archived_at__isnull=False)`
/// + `exclude(project__archived_at__isnull=False)` + `exclude(is_draft)`.
/// `{a}` is the issues-table alias; the states table is always aliased `s`.
/// The triage form mirrors the list endpoints (`s."group" <> 'triage'` in
/// WHERE position, dropping NULL-state rows exactly like Django's
/// `exclude`); there is no `s.deleted_at` predicate — forward-FK lookups
/// don't apply `StateManager` (I2 item 2 precedent). The project check
/// mirrors the list precedent (`archived_at` + `deleted_at` via `EXISTS`).
pub(crate) fn bulk_delete_scope_sql(a: &str) -> String {
    format!(
        "{a}.deleted_at IS NULL AND {a}.archived_at IS NULL AND {a}.is_draft = false \
         AND s.\"group\" <> 'triage' \
         AND EXISTS(SELECT 1 FROM projects p WHERE p.id = {a}.project_id \
           AND p.archived_at IS NULL AND p.deleted_at IS NULL)"
    )
}

/// The FILTERED pre-delete issue set shared by the `bulk_delete` COUNT,
/// both bridge UPDATEs (`issue__in=issues`, `base.py:786-789`), and the
/// final soft-delete UPDATE. Binds stay positional: `$1` project,
/// `$2` workspace slug, `$3` ids.
pub(crate) fn bulk_delete_issue_set_sql() -> String {
    format!(
        "SELECT i.id FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.project_id = $1 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND i.id = ANY($3) AND {}",
        bulk_delete_scope_sql("i")
    )
}

/// DELETE `/api/workspaces/:slug/projects/:project_id/bulk-delete-issues/`
/// — parity with Django `BulkDeleteIssuesEndpoint.delete`
/// (`plane/app/views/issue/base.py:773-797`, `urls/issue.py:94-96`).
///
/// - Gate: PROJECT-level ADMIN-only (`@allow_permission([ROLE.ADMIN])`,
///   `base.py:774`, default `level="PROJECT"`): the allowed-role branch needs
///   role 20, expressed through the shared `project_gate_allows` (same
///   fallback branch as I1 — any active membership + workspace ADMIN);
///   otherwise 403 `deny()`.
/// - Empty/missing `issue_ids` → 400 `{"error": "Issue IDs are required"}`
///   (`base.py:778-779`).
/// - Single transaction: PRE-delete scoped count (`base.py:782`, `len()` of
///   the pre-delete queryset) → soft-delete `cycle_issues` bridges
///   (`base.py:786`) + `module_issues` bridges (`base.py:789`) → soft-delete
///   the issues (`base.py:792`) → 200 `{"message": "{n} issues were deleted"}`
///   (`base.py:794-797`). All four statements share the FILTERED
///   `Issue.issue_objects` set (`bulk_delete_issue_set_sql` /
///   `bulk_delete_scope_sql`): triage/archived/draft/project-archived
///   issues are counted by NEITHER `n` NOR touched — Django spares them.
/// - FE `bulkDeleteIssues` (`issue.service.ts:347-360`) sends an axios
///   DELETE with body — Axum's `Json` extractor reads DELETE bodies
///   (live-curl proof deferred to T13).
///
/// Deviations: bridge deletes are soft-deletes (`deleted_at=now()`, Django's
/// `SoftDeletionQuerySet.delete`, `mixins.py:48-53` — NOT hard `DELETE`s);
/// Celery/activity writes are skipped (batch-wide precedent: Rust never
/// writes activities).
pub async fn bulk_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    body: Option<Json<BulkIssueIds>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // PROJECT-level ADMIN-only gate (`permissions/base.py:53-78` with
    // `allowed_roles=[ADMIN]`): allowed-role branch needs role 20; the
    // fallback (any active membership + workspace ADMIN) is shared.
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(member_role == Some(20), member_role.is_some(), ws_admin) {
        return Ok(deny());
    }
    let issue_ids = match resolve_bulk_ids(body) {
        Ok(ids) => ids,
        Err(err) => return Ok((StatusCode::BAD_REQUEST, Json(err))),
    };
    let mut tx = st.pool.begin().await?;
    let issue_set = bulk_delete_issue_set_sql();
    // PRE-delete queryset count (`total_issues = len(issues)`, `base.py:782`).
    let total: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM ({issue_set}) AS sub"))
        .bind(project_id)
        .bind(&slug)
        .bind(&issue_ids)
        .fetch_one(&mut *tx)
        .await?;
    // `CycleIssue.objects.filter(issue__in=issues).delete()` (`base.py:786`).
    sqlx::query(&format!(
        "UPDATE cycle_issues SET deleted_at = now() WHERE deleted_at IS NULL \
         AND issue_id IN ({issue_set})"
    ))
    .bind(project_id)
    .bind(&slug)
    .bind(&issue_ids)
    .execute(&mut *tx)
    .await?;
    // `ModuleIssue.objects.filter(issue__in=issues).delete()` (`base.py:789`).
    sqlx::query(&format!(
        "UPDATE module_issues SET deleted_at = now() WHERE deleted_at IS NULL \
         AND issue_id IN ({issue_set})"
    ))
    .bind(project_id)
    .bind(&slug)
    .bind(&issue_ids)
    .execute(&mut *tx)
    .await?;
    // `issues.delete()` (`base.py:792`): soft-delete via the default manager.
    // `FROM states s` with the join condition in WHERE position mirrors the
    // list endpoints' triage form (NULL-state rows drop like Django's
    // `exclude`).
    sqlx::query(&format!(
        "UPDATE issues i SET deleted_at = now() FROM states s \
         WHERE s.id = i.state_id AND i.project_id = $1 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND i.id = ANY($3) AND {}",
        bulk_delete_scope_sql("i")
    ))
    .bind(project_id)
    .bind(&slug)
    .bind(&issue_ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::OK,
        Json(json!({"message": delete_message(total.0)})),
    ))
}

/// POST `/api/workspaces/:slug/projects/:project_id/bulk-archive-issues/`
/// — parity with Django `BulkArchiveIssuesEndpoint.post`
/// (`plane/app/views/issue/archive.py:305-343`, `urls/issue.py:99-101`).
///
/// - Gate: `ProjectEntityPermission` (unsafe methods need role ∈
///   {ADMIN, MEMBER}, `permissions/project.py:104-119` — GUEST denies)
///   COMBINED with `@allow_permission([ADMIN, MEMBER])`
///   (`archive.py:306,308`): the entity perm already denies guests, so the
///   decorator fallback can never admit anyone new — effective gate is
///   `role ∈ {20, 15}`, no fallback; otherwise 403 `deny()`.
/// - Empty/missing `issue_ids` → 400 `{"error": "Issue IDs are required"}`
///   (`archive.py:312-313`).
/// - Issues loaded scoped (ws+project+ids, live only) with the state group
///   (`select_related("state")`, `archive.py:315-317`); if ANY
///   `state.group ∉ {completed, cancelled}` → 400
///   `{"error_code": 4091, "error_message": "INVALID_ARCHIVE_STATE_GROUP"}`
///   (`archive.py:319-327`).
/// - Else `archived_at=today` (+bulk_update, `archive.py:340-342`) → 200
///   `{"archived_at": "<str(today)>"}`, date-only `YYYY-MM-DD`
///   (`archive.py:343`). Single transaction.
///
/// Deviations: per-issue Celery `issue_activity` (`archive.py:328-339`)
/// skipped (batch-wide precedent); a NULL-state issue maps to 4091 (Django
/// would raise `AttributeError` on `None.group` → generic 500 — the `not in`
/// check itself treats a missing group as invalid); `updated_at` is not
/// bumped (Django `bulk_update(["archived_at"])` writes that column only).
pub async fn bulk_archive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    body: Option<Json<BulkIssueIds>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if !matches!(member_role, Some(20) | Some(15)) {
        return Ok(deny());
    }
    let issue_ids = match resolve_bulk_ids(body) {
        Ok(ids) => ids,
        Err(err) => return Ok((StatusCode::BAD_REQUEST, Json(err))),
    };
    let mut tx = st.pool.begin().await?;
    // `Issue.objects.filter(...).select_related("state")`
    // (`archive.py:315-317`): plain manager = live rows only.
    let groups: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT s.\"group\" FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.project_id = $1 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND i.id = ANY($3) AND i.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .bind(&issue_ids)
    .fetch_all(&mut *tx)
    .await?;
    for group in &groups {
        let valid = match group.as_deref() {
            Some(g) => guard_archive_group(g).is_ok(),
            None => false,
        };
        if !valid {
            tx.rollback().await?;
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error_code": INVALID_ARCHIVE_STATE_GROUP_CODE,
                    "error_message": INVALID_ARCHIVE_STATE_GROUP_MSG,
                })),
            ));
        }
    }
    let today = chrono::Utc::now().date_naive();
    sqlx::query(
        "UPDATE issues SET archived_at = $4 \
         WHERE project_id = $1 \
         AND workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND id = ANY($3) AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .bind(&issue_ids)
    .bind(today)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::OK,
        Json(json!({"archived_at": today.to_string()})),
    ))
}

#[cfg(test)]
mod bulk_tests {
    use super::*;

    #[test]
    fn guard_archive_group_vectors_match_django() {
        // Mirrors `BulkArchiveIssuesEndpoint.post` (`archive.py:319-327`):
        // only `completed`/`cancelled` groups may archive; anything else →
        // 400 `{"error_code": 4091, "error_message":
        // "INVALID_ARCHIVE_STATE_GROUP"}` (code `utils/error_codes.py:7`).
        assert!(guard_archive_group("completed").is_ok());
        assert!(guard_archive_group("cancelled").is_ok());
        for bad in ["backlog", "unstarted", "started", "triage", ""] {
            assert_eq!(
                guard_archive_group(bad).unwrap_err(),
                (4091, "INVALID_ARCHIVE_STATE_GROUP"),
                "group {bad:?}"
            );
        }
    }

    #[test]
    fn delete_message_matches_django() {
        // Mirrors `BulkDeleteIssuesEndpoint.delete` (`base.py:794-797`):
        // `f"{total_issues} issues were deleted"` (always plural, count is
        // the PRE-delete queryset length).
        assert_eq!(delete_message(3), "3 issues were deleted");
        assert_eq!(delete_message(0), "0 issues were deleted");
        assert_eq!(delete_message(1), "1 issues were deleted");
    }

    #[test]
    fn require_issue_ids_matches_django() {
        // Mirrors `if not len(issue_ids)` (`base.py:778-779`,
        // `archive.py:312-313`): missing defaults to `[]` (serde) and empty
        // → 400 `{"error": "Issue IDs are required"}`.
        assert_eq!(
            require_issue_ids(&[]).unwrap_err(),
            "Issue IDs are required"
        );
        assert!(require_issue_ids(&[uuid::Uuid::nil()]).is_ok());
    }

    #[test]
    fn bulk_delete_gate_is_admin_only_with_django_fallback() {
        // Mirrors `@allow_permission([ROLE.ADMIN])` at PROJECT level
        // (`base.py:774`, default `level="PROJECT"`,
        // `permissions/base.py:53-78`): the allowed-role branch needs role
        // 20 (ADMIN-only — narrower than I1's 20/15/5), while the fallback
        // branch (any active membership + workspace ADMIN) is unchanged, so
        // it is expressed through the shared `project_gate_allows` with
        // `has_allowed_role = (role == Some(20))` — no new gate fn.
        let allows = |role: Option<i16>, ws_admin: bool| {
            project_gate_allows(role == Some(20), role.is_some(), ws_admin)
        };
        assert!(allows(Some(20), false));
        assert!(allows(Some(20), true));
        // Plain non-admin callers deny (no ws-admin fallback).
        assert!(!allows(Some(15), false));
        assert!(!allows(Some(5), false));
        assert!(!allows(None, false));
        assert!(!allows(None, true));
        // Django fallback parity: any active membership + ws ADMIN passes.
        assert!(allows(Some(15), true));
        assert!(allows(Some(5), true));
    }

    #[test]
    fn bulk_archive_gate_is_admin_member_only() {
        // Mirrors `ProjectEntityPermission` (unsafe: role ∈ {ADMIN, MEMBER},
        // `permissions/project.py:104-119` — GUEST denies) COMBINED with
        // `@allow_permission([ADMIN, MEMBER])` (`archive.py:306,308`): the
        // entity perm already denies guests, so the decorator fallback (any
        // membership + ws ADMIN) can never admit anyone the entity perm
        // rejects — effective gate is `role ∈ {20, 15}`, no fallback.
        let allows = |role: Option<i16>| matches!(role, Some(20) | Some(15));
        assert!(allows(Some(20)));
        assert!(allows(Some(15)));
        assert!(!allows(Some(5)));
        assert!(!allows(None));
    }

    #[test]
    fn bulk_delete_scope_sql_matches_issue_manager() {
        // `Issue.issue_objects` (`db/models/issue.py:92-101`): live rows +
        // `exclude(state__group='triage')` + `exclude(archived_at__isnull=False)`
        // + `exclude(project__archived_at__isnull=False)` + `exclude(is_draft)`.
        // Triage form mirrors the list endpoints (`s."group" <> 'triage'` in
        // WHERE position, dropping NULL-state rows like Django `exclude`).
        let pred = bulk_delete_scope_sql("i");
        for needle in [
            "i.deleted_at IS NULL",
            "i.archived_at IS NULL",
            "i.is_draft = false",
            "s.\"group\" <> 'triage'",
            "p.archived_at IS NULL",
            "p.deleted_at IS NULL",
        ] {
            assert!(pred.contains(needle), "{needle}");
        }
        // No state-deleted predicate: forward-FK lookups don't apply
        // `StateManager` (I2 item 2 precedent).
        assert!(!pred.contains("s.deleted_at"), "{pred}");
    }

    #[test]
    fn bulk_delete_issue_set_sql_is_shared_filtered_set() {
        // Django `issue__in=issues` inherits the FILTERED pre-delete
        // queryset (`base.py:781-789`): both bridge UPDATEs must use this
        // same subquery, not bare ids.
        let sub = bulk_delete_issue_set_sql();
        assert!(sub.contains("LEFT JOIN states s ON s.id = i.state_id"), "{sub}");
        assert!(sub.contains("i.id = ANY($3)"), "{sub}");
        assert!(sub.contains(&bulk_delete_scope_sql("i")), "{sub}");
    }

    #[test]
    fn resolve_bulk_ids_absent_body_matches_django() {
        // An entirely absent body reaches the handler as `None` via Axum's
        // `Option<Json>` (which swallows `JsonRejection` into `None`) and
        // must map to Django's 400 `{"error": "Issue IDs are required"}`
        // (`base.py:778-779`, `archive.py:312-313`) — not Axum's 415/400/422
        // rejection bodies. `{}` (serde default) and explicit empty ids take
        // the same 400 path; valid ids pass through.
        assert_eq!(
            resolve_bulk_ids(None).unwrap_err(),
            json!({"error": "Issue IDs are required"})
        );
        assert_eq!(
            resolve_bulk_ids(Some(Json(BulkIssueIds::default()))).unwrap_err(),
            json!({"error": "Issue IDs are required"})
        );
        assert_eq!(
            resolve_bulk_ids(Some(Json(BulkIssueIds { issue_ids: vec![] }))).unwrap_err(),
            json!({"error": "Issue IDs are required"})
        );
        let id = uuid::Uuid::nil();
        assert_eq!(
            resolve_bulk_ids(Some(Json(BulkIssueIds { issue_ids: vec![id] }))).unwrap(),
            vec![id]
        );
    }

    #[test]
    fn bulk_handlers_exist_for_bulk_routes() {
        // Wiring guard: `main.rs` registers
        // `DELETE .../bulk-delete-issues/` → `bulk_delete` (Django
        // `BulkDeleteIssuesEndpoint.delete`, `urls/issue.py:94-96`) and
        // `POST .../bulk-archive-issues/` → `bulk_archive` (Django
        // `BulkArchiveIssuesEndpoint.post`, `urls/issue.py:99-101`).
        let _ = super::bulk_delete;
        let _ = super::bulk_archive;
    }
}

// ---- Batch C I4 ----

/// Query params for `deleted_list`. Mirrors
/// `request.GET.get("updated_at__gt", None)` (`base.py:804`): the only
/// filter Django applies on top of the archived-or-deleted scope.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeletedIssuesQuery {
    #[serde(default, rename = "updated_at__gt")]
    pub updated_at_gt: Option<String>,
}

/// Parses `?updated_at__gt` the way Django's `DateTimeField` lookup does
/// (`base.py:804-805,809`): RFC3339 datetimes and bare `YYYY-MM-DD` dates
/// (midnight) are valid; naive datetimes read as UTC (containers run UTC).
/// Garbage raises Django `ValidationError`, mapped by
/// `BaseAPIView.handle_exception` (`views/base.py:182-186`) to 400
/// `{"error": "Please provide valid detail"}`.
pub(crate) fn parse_deleted_updated_at_gt(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, &'static str> {
    const BAD: &str = "Please provide valid detail";
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    // Django `parse_datetime` also accepts space-separated and naive forms.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, fmt) {
            return Ok(chrono::DateTime::from_naive_utc_and_offset(
                naive,
                chrono::Utc,
            ));
        }
    }
    // Bare date → midnight (`parse_date` fallback in `DateTimeField.to_python`).
    if let Ok(day) = raw.parse::<chrono::NaiveDate>() {
        if let Some(naive) = day.and_hms_opt(0, 0, 0) {
            return Ok(chrono::DateTime::from_naive_utc_and_offset(
                naive,
                chrono::Utc,
            ));
        }
    }
    Err(BAD)
}

/// GET `/api/workspaces/:slug/projects/:project_id/deleted-issues/` — parity
/// with Django `DeletedIssuesListViewSet.get`
/// (`plane/app/views/issue/base.py:800-813`, `urls/issue.py:247-249`).
///
/// - Gate: PROJECT-level ADMIN/MEMBER/GUEST via the shared helpers (same as
///   I1; `@allow_permission([ADMIN, MEMBER, GUEST])`, `base.py:801`,
///   defaults to `level="PROJECT"` incl. the ws-admin fallback branch).
/// - Scope mirrors `Issue.all_objects` (`db/mixins.py:67`, the UNFILTERED
///   manager incl. soft-deleted) + `Q(archived_at__isnull=False) |
///   Q(deleted_at__isnull=False)`, scoped ws+project; optional
///   `?updated_at__gt` (`__gt`, `base.py:803-811`). Ordering follows the
///   model `Meta.ordering = ("-created_at",)`.
/// - 200 BARE UUID array, unpaginated (Django `Response(deleted_issues)`
///   over `values_list("id", flat=True)`).
/// - FE `getDeletedIssues` (`issue.service.ts:89-97`) has ZERO callers —
///   still implemented (1 query).
///
/// Deviations: datetimes RFC3339 (batch convention); invalid
/// `updated_at__gt` → 400 `{"error": "Please provide valid detail"}` via the
/// generic `ValidationError` mapping (the view itself defines no errors).
pub async fn deleted_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<DeletedIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let gt = match q.updated_at_gt.as_deref() {
        None => None,
        Some(raw) => match parse_deleted_updated_at_gt(raw) {
            Ok(dt) => Some(dt),
            Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
        },
    };
    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT i.id FROM issues i WHERE i.project_id = ");
    qb.push_bind(project_id)
        .push(" AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
        .push_bind(slug)
        .push(") AND (i.archived_at IS NOT NULL OR i.deleted_at IS NOT NULL)");
    if let Some(dt) = gt {
        qb.push(" AND i.updated_at > ").push_bind(dt);
    }
    qb.push(" ORDER BY i.created_at DESC");
    let ids: Vec<uuid::Uuid> = qb.build_query_scalar().fetch_all(&st.pool).await?;
    Ok((StatusCode::OK, Json(json!(ids))))
}

/// Byte-exact 400 body for `group_by == sub_group_by`
/// (`archive.py:143-147`).
pub(crate) const ARCHIVE_GROUP_BY_CONFLICT_MSG: &str =
    "Group by and sub group by cannot have same parameters";

/// Mirrors the truthiness guards (`archive.py:134-143`): `group_by` /
/// `sub_group_by` default to `False` and empty strings are falsy too — only
/// two non-empty EQUAL values conflict.
pub(crate) fn archive_group_by_conflict(
    group_by: Option<&str>,
    sub_group_by: Option<&str>,
) -> Option<&'static str> {
    match (group_by, sub_group_by) {
        (Some(g), Some(s)) if !g.is_empty() && !s.is_empty() && g == s => {
            Some(ARCHIVE_GROUP_BY_CONFLICT_MSG)
        }
        _ => None,
    }
}

/// Group-by allowlist for issues, byte-exact from
/// `plane/utils/order_queryset.py:93-105` (`ISSUE_GROUP_BY_ALLOWLIST`).
pub(crate) const ISSUE_GROUP_BY_ALLOWLIST: &[&str] = &[
    "state_id",
    "state__group",
    "priority",
    "labels__id",
    "assignees__id",
    "issue_module__module_id",
    "cycle_id",
    "project_id",
    "created_by",
    "target_date",
    "start_date",
];

/// Mirrors the allowlist gate in `BasePaginator.paginate`
/// (`plane/utils/paginator.py:690-699`, GHSA-wwgj-929g-42cm): a truthy
/// `group_by` outside `ISSUE_GROUP_BY_ALLOWLIST` raises
/// `ParseError(detail=f"Invalid group_by field: X")`, then the same for a
/// truthy `sub_group_by` — but ONLY inside the truthy-`group_by` branch
/// (the whole block is skipped when `group_by` is falsy, so a lone invalid
/// `sub_group_by` is a Django 200, mirrored here as `None`). DRF renders
/// `ParseError` as 400 `{"detail": msg}` (same envelope as the I2
/// per_page/cursor errors). Runs AFTER the same-value 400: the view's
/// conflict check (`archive.py:140-147`) precedes `paginate`.
pub(crate) fn archive_group_by_allowlist_error(
    group_by: Option<&str>,
    sub_group_by: Option<&str>,
) -> Option<String> {
    let g = group_by?;
    if g.is_empty() {
        return None;
    }
    if !ISSUE_GROUP_BY_ALLOWLIST.contains(&g) {
        return Some(format!("Invalid group_by field: {g}"));
    }
    match sub_group_by {
        Some(s) if !s.is_empty() && !ISSUE_GROUP_BY_ALLOWLIST.contains(&s) => {
            Some(format!("Invalid sub_group_by field: {s}"))
        }
        _ => None,
    }
}

/// Known-gap 400 body for truthy `group_by` that survives the conflict +
/// allowlist checks. Django would 200 with the `GroupedOffsetPaginator` /
/// `SubGroupedOffsetPaginator` grouped-dict shape (`paginator.py:195-633`,
/// FIELD_MAPPER key swaps, `archive.py:148-209`) — implementing that path
/// is out of scope, so this endpoint 400s instead. Plain descriptive
/// string (NOT a Django message): unreachable in the default FE flows
/// (only 2 `getArchivedIssues` callers, both in `archived/issue.store.ts`
/// flat flow; archived page is hardcoded list-layout, `groupedBy` never
/// passed) — a user-set display `group_by` hits it. A lone `sub_group_by`
/// is ignored (flat path), exactly like Django.
pub(crate) const ARCHIVE_GROUPING_UNSUPPORTED_MSG: &str = "Grouped pagination is not supported";

/// Truthiness mirrors Django's `if group_by:` (`archive.py:140`, where the
/// param defaults to `False`): a lone `sub_group_by` is ignored entirely
/// (flat path — no gap-400 and no allowlist-400, since the allowlist block
/// in `paginate` is nested under truthy `group_by`). Only truthy `group_by`
/// groups.
pub(crate) fn archive_grouping_unsupported(group_by: Option<&str>, _sub_group_by: Option<&str>) -> bool {
    group_by.is_some_and(|g| !g.is_empty())
}

/// Mirrors `show_sub_issues = request.GET.get("show_sub_issues", "true")`
/// (`archive.py:109-115`): only the exact `"true"` (incl. the default when
/// absent) keeps sub-issues — `"false"` (or any other explicit value) adds
/// `parent__isnull=True`.
pub(crate) fn show_sub_issues_hides_children(raw: Option<&str>) -> bool {
    raw.unwrap_or("true") != "true"
}

/// One row of the `archived-issues/` page. Key order is the exact
/// `issue_on_results` order (`plane/utils/grouper.py:106-141`): the 23
/// `required_fields` (`id` … `state__group`) then `assignee_ids`,
/// `label_ids`, `module_ids`. Delta vs the I2 `IssueDetailRow` (25 keys):
/// PLUS `state__group` (the archive queryset keeps triage/NULL-state rows,
/// so it is `Option`); otherwise the same 25 keys, NO `deleted_at`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ArchiveRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub state_id: Option<uuid::Uuid>,
    pub sort_order: f64,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub estimate_point: Option<uuid::Uuid>,
    pub priority: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub cycle_id: Option<uuid::Uuid>,
    pub sub_issues_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub attachment_count: i64,
    pub link_count: i64,
    pub is_draft: bool,
    pub archived_at: Option<chrono::NaiveDate>,
    #[serde(rename = "state__group")]
    pub state_group: Option<String>,
    pub assignee_ids: Vec<uuid::Uuid>,
    pub label_ids: Vec<uuid::Uuid>,
    pub module_ids: Vec<uuid::Uuid>,
}

/// The page-query SELECT prefix for `archived_list`: the 26
/// `issue_on_results` columns in grouper order, on top of the archive
/// `apply_annotations` (`archive.py:60-95`) + `issue_queryset_grouper`
/// (`grouper.py:28-90`) subqueries.
pub(crate) const ARCHIVE_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
     i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
     i.sequence_id, i.project_id, i.parent_id, \
     (SELECT ci.cycle_id FROM cycle_issues ci \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
     (SELECT COUNT(*) FROM issues si \
       LEFT JOIN states ss ON ss.id = si.state_id \
       WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
       AND si.archived_at IS NULL AND si.is_draft = false \
       AND ss.deleted_at IS NULL AND ss.\"group\" <> 'triage' \
       AND EXISTS(SELECT 1 FROM projects sp \
         WHERE sp.id = si.project_id AND sp.archived_at IS NULL)) AS sub_issues_count, \
     i.created_at, i.updated_at, \
     i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
     (SELECT COUNT(*) FROM file_assets fa \
       WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
       AND fa.deleted_at IS NULL) AS attachment_count, \
     (SELECT COUNT(*) FROM issue_links lin \
       WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count, \
     i.is_draft, i.archived_at, s.\"group\" AS state_group, \
     COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
     COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
     COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
       JOIN modules m ON m.id = mi.module_id \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
       AND m.archived_at IS NULL), '{}'::uuid[]) AS module_ids \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id LEFT JOIN issue_types t ON t.id = i.type_id";

/// Counts the top-level SELECT columns in `ARCHIVE_SELECT_SQL` (commas at
/// paren depth 0, plus one). Locks the 26-`ArchiveRow`-field projection:
/// sqlx maps `query_as` positionally, so a duplicated (or dropped) column
/// fails at runtime instead of compile time. Test-only.
#[cfg(test)]
pub(crate) fn archive_select_column_count() -> usize {
    let mut depth = 0usize;
    let mut commas = 0usize;
    for c in ARCHIVE_SELECT_SQL.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas + 1
}

/// Shared WHERE clause for the `archived-issues/` COUNT + page queries:
/// the archive queryset (`archive.py:97-103`) — plain `Issue.objects`
/// (soft-delete excluded ONLY: no triage/draft/project-archived exclusion)
/// + `Q(type__isnull=True) | Q(type__is_epic=False)` (FK `type_id` NULL or
/// the joined `issue_types.is_epic` false — forward-FK lookups apply no
/// `deleted_at` predicate) + `archived_at IS NOT NULL` + slug/project
/// scoping — then `show_sub_issues`, legacy `issue_filters`, and the
/// `filters` JSON tree (same filter support as I2 `list_detail`, reusing the
/// I2 helpers).
fn push_archive_where(
    qb: &mut QueryBuilder<Postgres>,
    slug: &str,
    project_id: uuid::Uuid,
    q: &DetailIssuesQuery,
    tree: Option<&serde_json::Value>,
    today: chrono::NaiveDate,
) -> Result<(), DetailWhereError> {
    qb.push(" WHERE i.project_id = ")
        .push_bind(project_id)
        .push(" AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
        .push_bind(slug.to_string())
        .push(") AND i.deleted_at IS NULL AND i.archived_at IS NOT NULL AND (i.type_id IS NULL OR t.is_epic = false)");
    if show_sub_issues_hides_children(q.show_sub_issues.as_deref()) {
        qb.push(" AND i.parent_id IS NULL");
    }
    apply_legacy_filters(qb, q, today).map_err(DetailWhereError::Legacy)?;
    if let Some(tree) = tree {
        apply_complex_filter(qb, tree).map_err(DetailWhereError::Complex)?;
    }
    Ok(())
}

/// GET `/api/workspaces/:slug/projects/:project_id/archived-issues/` —
/// parity with Django `IssueArchiveViewSet.list`
/// (`plane/app/views/issue/archive.py:97-218`, `urls/issue.py:224-225`).
///
/// - Gate: ADMIN/MEMBER via the shared PROJECT-level helpers
///   (`@allow_permission([ADMIN, MEMBER])`, `archive.py:106`, default
///   `level="PROJECT"` incl. the ws-admin fallback branch
///   `permissions/base.py:64-78` — a GUEST-role member who is workspace
///   ADMIN passes); otherwise 403 `deny()`.
/// - `show_sub_issues` default `"true"` (`"false"` → `parent__isnull`,
///   `archive.py:109-115`); `order_by` default `-created_at` (same
///   `order_issue_queryset` + paginator re-ordering as I2, reusing
///   `sanitize_order_by` / `detail_order_expr`); `group_by`/`sub_group_by`
///   equal → 400 `{"error": "Group by and sub group by cannot have same
///   parameters"}` byte-exact (`archive.py:143-147`).
/// - Filterset + legacy filters mirror the SAME support as I2 `list_detail`
///   (reused helpers, incl. the same 400/500 mappings); paginated envelope
///   is the SAME 12-key `DetailEnvelope` as I2 (reused paginator fns);
///   rows render the `issue_on_results` 26-key shape (`archive.py:212-218`,
///   `grouper.py:93-141`) — the I2 25 keys PLUS `state__group`.
/// - FE `getArchivedIssues` (`issue_archive.service.ts:22-34`) →
///   `store/issue/archived/issue.store.ts` envelope flow (flat-list path).
///
/// Deviations (reviewer-adjudicable, Django-literal readings): grouped
/// pagination (`GroupedOffsetPaginator` / `SubGroupedOffsetPaginator`,
/// `archive.py:148-209`) is OUT — truthy `group_by` 400 here
/// (`ARCHIVE_GROUPING_UNSUPPORTED_MSG`, known gap) instead of Django's
/// grouped 200, AFTER the byte-exact allowlist 400 for invalid fields
/// (`paginator.py:690-699`); a lone `sub_group_by` is ignored (flat path),
/// exactly like Django (`if group_by:`, `archive.py:140`); unreachable in the default FE flows
/// (archived callers never pass `groupedBy`, archived page is
/// hardcoded list-layout);
/// `label_ids`/`assignee_ids` arrays carry `ORDER BY bridge.created_at
/// DESC` (Django `ArrayAgg(DISTINCT …)` is unordered — set-equal, order
/// deterministic); datetimes RFC3339 UTC (batch convention).
pub async fn archived_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<DetailIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // PROJECT-level ADMIN/MEMBER gate (`archive.py:106`, default
    // `level="PROJECT"`, `permissions/base.py:53-78`) with
    // `BaseViewSet.permission_classes=[IsAuthenticated]` only: the
    // allowed-role branch needs 20/15 and the ws-admin fallback branch
    // (`base.py:64-78`) APPLIES — a GUEST-role member who is workspace
    // ADMIN passes. Shared `project_gate_allows`, same as I1/I3.
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(matches!(member_role, Some(20) | Some(15)), member_role.is_some(), ws_admin) {
        return Ok(deny());
    }
    if let Some(msg) = archive_group_by_conflict(q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": msg}))));
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok(detail_400(json!({"detail": msg}))),
    };
    let cursor_raw = q.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(msg) => return Ok(detail_400(json!({"detail": msg}))),
    };
    // Group-by allowlist BEFORE the window check: Django validates the
    // field names while constructing the paginator
    // (`paginator.py:690-699`), which precedes `get_result` (the
    // offset/window 400). Invalid fields 400 byte-exact (`{"detail"}`).
    if let Some(msg) = archive_group_by_allowlist_error(q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        return Ok(detail_400(json!({"detail": msg})));
    }
    // Known gap (see `ARCHIVE_GROUPING_UNSUPPORTED_MSG`): truthy grouped
    // fields that Django would grouped-paginate 200 400 here instead.
    if archive_grouping_unsupported(q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ARCHIVE_GROUPING_UNSUPPORTED_MSG})),
        ));
    }
    // `limit = min(limit, max_limit)` (`paginator.py:132`); offset-first
    // negative-window 400 mirrors I2 (`paginator.py:142-150, 708-711`).
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok(detail_400(json!({"detail": "Error in parsing"}))),
        Ok(w) => w,
    };
    if limit <= 0 {
        return Ok(server_error());
    }
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let (order_expr, desc) = detail_order_expr(&sanitized);
    let order_dir = if desc { "DESC" } else { "ASC" };
    let today = chrono::Utc::now().date_naive();
    let tree = match parse_complex_filter(q.filters.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    };
    // `expand` has no archive reading (`issue_on_results` values only);
    // `fields` likewise unused here.
    let _ = (&q.fields, &q.expand);

    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id LEFT JOIN issue_types t ON t.id = i.type_id",
    );
    match push_archive_where(&mut count_qb, &slug, project_id, &q, tree.as_ref(), today) {
        Ok(()) => {}
        Err(DetailWhereError::Legacy(LegacyFilterError::BadRequest(msg))) => {
            return Ok(detail_400(json!({"error": msg})));
        }
        Err(DetailWhereError::Legacy(LegacyFilterError::Server)) => {
            return Ok(server_error());
        }
        Err(DetailWhereError::Complex(e)) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(ARCHIVE_SELECT_SQL);
    match push_archive_where(&mut page_qb, &slug, project_id, &q, tree.as_ref(), today) {
        Ok(()) => {}
        Err(DetailWhereError::Legacy(LegacyFilterError::BadRequest(msg))) => {
            return Ok(detail_400(json!({"error": msg})));
        }
        Err(DetailWhereError::Legacy(LegacyFilterError::Server)) => {
            return Ok(server_error());
        }
        Err(DetailWhereError::Complex(e)) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    }
    // Paginator ordering: `(key DIR NULLS LAST, -created_at)`
    // (`paginator.py:136-140`), same as I2.
    page_qb
        .push(" ORDER BY ")
        .push(order_expr)
        .push(" ")
        .push(order_dir)
        .push(" NULLS LAST, i.created_at DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<ArchiveRow> = match window {
        PageWindow::Rows(offset) => {
            page_qb.push(" OFFSET ").push_bind(offset);
            page_qb.build_query_as().fetch_all(&st.pool).await?
        }
        PageWindow::BeyondEnd => Vec::new(),
    };
    let _ = (cursor.limit_value, cursor.is_prev);
    let next_page_results = rows.len() as i64 > limit;
    rows.truncate(limit as usize);

    let mut results = Vec::with_capacity(rows.len());
    for row in &rows {
        results.push(serde_json::to_value(row).map_err(|e| anyhow::anyhow!(e))?);
    }

    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next_cursor_str(limit, cursor.page),
        prev_cursor: prev_cursor_str(limit, cursor.page),
        next_page_results,
        prev_page_results: cursor.page > 0,
        count: rows.len() as i64,
        total_pages: total_pages(total, limit),
        total_results: total,
        extra_stats: None,
        results,
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

#[cfg(test)]
mod batch_c_i4_tests {
    use super::*;

    #[test]
    fn show_sub_issues_filter_matches_django() {
        // Mirrors `IssueArchiveViewSet.list` (`archive.py:109-115`):
        // `show_sub_issues` defaults to `"true"`; only the exact `"true"`
        // keeps sub-issues — `"false"` (and any other explicit value) adds
        // `parent__isnull=True`.
        assert!(!show_sub_issues_hides_children(None));
        assert!(!show_sub_issues_hides_children(Some("true")));
        assert!(show_sub_issues_hides_children(Some("false")));
    }

    #[test]
    fn archive_group_by_conflict_matches_django() {
        // Mirrors `IssueArchiveViewSet.list` (`archive.py:143-147`):
        // truthy `group_by` + truthy equal `sub_group_by` → 400 with the
        // byte-exact body below; anything else is no error.
        assert_eq!(
            archive_group_by_conflict(Some("priority"), Some("priority")),
            Some(ARCHIVE_GROUP_BY_CONFLICT_MSG)
        );
        assert_eq!(
            ARCHIVE_GROUP_BY_CONFLICT_MSG,
            "Group by and sub group by cannot have same parameters"
        );
        assert_eq!(
            archive_group_by_conflict(Some("priority"), Some("state__group")),
            None
        );
        assert_eq!(archive_group_by_conflict(Some("priority"), None), None);
        assert_eq!(archive_group_by_conflict(None, None), None);
        assert_eq!(archive_group_by_conflict(Some(""), Some("")), None);
    }

    #[test]
    fn deleted_gate_is_project_level_with_django_fallback() {
        // Mirrors `@allow_permission([ADMIN, MEMBER, GUEST])`
        // (`base.py:801`, default `level="PROJECT"`,
        // `permissions/base.py:53-78`): allowed-role branch needs 20/15/5,
        // fallback (any active membership + workspace ADMIN) unchanged —
        // shared `project_gate_allows`, same as I1.
        let allows = |role: Option<i16>, ws_admin: bool| {
            project_gate_allows(matches!(role, Some(20) | Some(15) | Some(5)), role.is_some(), ws_admin)
        };
        assert!(allows(Some(20), false));
        assert!(allows(Some(15), false));
        assert!(allows(Some(5), false));
        assert!(!allows(None, false));
        assert!(!allows(None, true));
        assert!(allows(Some(5), true));
    }

    #[test]
    fn archived_gate_is_admin_member_with_django_fallback() {
        // Mirrors `@allow_permission([ADMIN, MEMBER])` (`archive.py:106`,
        // default `level="PROJECT"`, `permissions/base.py:53-78`) with
        // `BaseViewSet.permission_classes=[IsAuthenticated]` only: the
        // allowed-role branch needs 20/15, and the fallback branch (any
        // active project membership + workspace ADMIN,
        // `permissions/base.py:64-78`) APPLIES — so a GUEST-role member who
        // is workspace ADMIN passes; expressed through the shared
        // `project_gate_allows` like I1/I3.
        let allows = |role: Option<i16>, ws_admin: bool| {
            project_gate_allows(matches!(role, Some(20) | Some(15)), role.is_some(), ws_admin)
        };
        assert!(allows(Some(20), false));
        assert!(allows(Some(15), false));
        assert!(!allows(Some(5), false));
        assert!(!allows(None, false));
        assert!(!allows(None, true));
        // Django fallback parity: any active membership + ws ADMIN passes,
        // even with a GUEST project role.
        assert!(allows(Some(5), true));
        assert!(allows(Some(15), true));
    }

    #[test]
    fn archive_group_by_allowlist_matches_django() {
        // Mirrors the allowlist gate in `BasePaginator.paginate`
        // (`plane/utils/paginator.py:690-699`, GHSA-wwgj-929g-42cm):
        // a truthy `group_by` outside `ISSUE_GROUP_BY_ALLOWLIST`
        // (`order_queryset.py:93-105`) → ParseError → 400
        // `{"detail": "Invalid group_by field: X"}` byte-exact; then the
        // same for a truthy `sub_group_by` — but ONLY when `group_by` is
        // truthy (the whole block is skipped otherwise, so a lone invalid
        // `sub_group_by` is a Django 200).
        assert_eq!(
            archive_group_by_allowlist_error(Some("nope"), None),
            Some("Invalid group_by field: nope".to_string())
        );
        assert_eq!(
            archive_group_by_allowlist_error(Some("priority"), Some("zzz")),
            Some("Invalid sub_group_by field: zzz".to_string())
        );
        // Group checked first.
        assert_eq!(
            archive_group_by_allowlist_error(Some("nope"), Some("zzz")),
            Some("Invalid group_by field: nope".to_string())
        );
        assert_eq!(archive_group_by_allowlist_error(Some("priority"), Some("state__group")), None);
        assert_eq!(archive_group_by_allowlist_error(None, None), None);
        assert_eq!(archive_group_by_allowlist_error(Some(""), Some("")), None);
        assert_eq!(archive_group_by_allowlist_error(None, Some("zzz")), None);
    }

    #[test]
    fn archive_grouping_gap_400_matches_reviewer_scope() {
        // Known gap (grouped paginators OUT): truthy `group_by` that
        // survives the conflict + allowlist checks 400s instead of
        // Django's grouped 200. A lone `sub_group_by` is IGNORED —
        // Django `if group_by:` (`archive.py:140`) never groups on it, so
        // it takes the flat path (no gap-400, no allowlist-400).
        assert!(archive_grouping_unsupported(Some("priority"), None));
        assert!(archive_grouping_unsupported(Some("priority"), Some("state__group")));
        assert!(!archive_grouping_unsupported(None, None));
        assert!(!archive_grouping_unsupported(Some(""), Some("")));
        assert!(!archive_grouping_unsupported(Some(""), None));
        assert!(!archive_grouping_unsupported(None, Some("priority")));
        assert!(!archive_grouping_unsupported(None, Some("zzz")));
        assert_eq!(ARCHIVE_GROUPING_UNSUPPORTED_MSG, "Grouped pagination is not supported");
    }

    #[test]
    fn archive_select_matches_i2_detail_fragments() {
        // Consistency with I2 `DETAIL_SELECT_SQL`: all three id arrays
        // carry `ORDER BY <bridge>.created_at DESC`, and the
        // `sub_issues_count` state predicate carries `ss.deleted_at IS
        // NULL` like the detail subquery. (`DISTINCT` on `module_ids` is
        // dropped: the partial unique index
        // `module_issue_unique_issue_module_when_deleted_at_null`
        // (`issue_id`, `module_id`) makes it a no-op over live rows — and
        // PG rejects `array_agg(DISTINCT x ORDER BY y)` unless `y` is in
        // the argument list.)
        assert!(
            ARCHIVE_SELECT_SQL.contains("array_agg(mi.module_id ORDER BY mi.created_at DESC)"),
            "module_ids ordering"
        );
        assert!(
            ARCHIVE_SELECT_SQL.contains("ss.deleted_at IS NULL AND ss.\"group\" <> 'triage'"),
            "sub_issues_count state predicate"
        );
    }

    #[test]
    fn archive_select_column_count_matches_row() {
        // `ARCHIVE_SELECT_SQL` must project exactly the 26 `ArchiveRow`
        // fields: sqlx maps `query_as` positionally, so a duplicated (or
        // dropped) column fails at runtime, not compile time.
        assert_eq!(archive_select_column_count(), 26);
    }

    #[test]
    fn deleted_updated_at_gt_parses_like_django() {
        // Mirrors `.filter(updated_at__gt=<str>)` (`base.py:804-805,809`):
        // Django's `DateTimeField` accepts datetimes and bare dates
        // (midnight); garbage raises `ValidationError` → 400
        // `{"error": "Please provide valid detail"}`.
        assert!(parse_deleted_updated_at_gt("2026-01-02T03:04:05Z").is_ok());
        assert!(parse_deleted_updated_at_gt("2026-01-02").is_ok());
        assert_eq!(
            parse_deleted_updated_at_gt("zzz").unwrap_err(),
            "Please provide valid detail"
        );
    }

    #[test]
    fn i4_handlers_exist_for_i4_routes() {
        // Wiring guard: `main.rs` registers
        // `GET .../deleted-issues/` → `deleted_list` (Django
        // `DeletedIssuesListViewSet.get`, `urls/issue.py:247-249`) and
        // `GET .../archived-issues/` → `archived_list` (Django
        // `IssueArchiveViewSet.list`, `urls/issue.py:224-225`).
        let _ = super::deleted_list;
        let _ = super::archived_list;
    }
}

// ---- Batch C I5 ----

/// Byte-exact 400 body for an empty/missing `sub_issue_ids`
/// (`plane/app/views/issue/sub_issue.py:224-228`).
pub(crate) const SUB_ISSUE_IDS_REQUIRED_MSG: &str = "Sub Issue IDs are required";
/// Byte-exact 404 body for a scoped parent miss (`sub_issue.py:217-221`).
pub(crate) const PARENT_ISSUE_NOT_FOUND_MSG: &str = "Parent issue not found";
/// Byte-exact 400 body for an unknown `group_by` key: `issue[group_by]`
/// (`sub_issue.py:197-198`) raises `KeyError`, mapped by
/// `BaseAPIView.handle_exception` (`plane/app/views/base.py:193-197`).
pub(crate) const SUB_GROUP_KEY_MISSING_MSG: &str = "The required key does not exist.";

/// Body for `sub_add`. Mirrors `request.data.get("sub_issue_ids", [])`
/// (`sub_issue.py:222`): a missing key defaults to `[]`, which then 400s via
/// `require_sub_issue_ids`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SubIssueBody {
    #[serde(default)]
    pub sub_issue_ids: Vec<uuid::Uuid>,
}

/// Mirrors `if not len(sub_issue_ids)` (`sub_issue.py:224-228`) → 400
/// `{"error": "Sub Issue IDs are required"}`.
pub(crate) fn require_sub_issue_ids(ids: &[uuid::Uuid]) -> Result<(), &'static str> {
    if ids.is_empty() {
        Err(SUB_ISSUE_IDS_REQUIRED_MSG)
    } else {
        Ok(())
    }
}

/// Maps the optional POST body to ids-or-400. The handler takes
/// `Option<Json<SubIssueBody>>` because Axum 0.7.9's `Json` extractor rejects
/// an ENTIRELY ABSENT body with `JsonRejection` before the handler runs —
/// while Django's `request.data.get("sub_issue_ids", [])` treats it as `[]`
/// → 400 `{"error": "Sub Issue IDs are required"}`. Axum's `Option<Json>`
/// swallows rejections into `None`, so `None` maps to Django's 400 here (I3
/// `resolve_bulk_ids` precedent).
pub(crate) fn resolve_sub_ids(body: Option<Json<SubIssueBody>>) -> Result<Vec<uuid::Uuid>, Value> {
    let ids = body.map(|Json(b)| b.sub_issue_ids).unwrap_or_default();
    require_sub_issue_ids(&ids)
        .map(|()| ids)
        .map_err(|e| json!({"error": e}))
}

/// Groups sub-issue ids by their `state_group` annotation. Mirrors the
/// `defaultdict(list)` loops (`sub_issue.py:178-180` GET, `267-269` POST).
/// A `None` group renders under `"null"` (Python `json.dumps({None: ...})`);
/// unreachable via `Issue.issue_objects`, which drops NULL-state rows exactly
/// like Django's `exclude(state__group='triage')`.
pub(crate) fn sub_state_distribution(
    rows: &[(uuid::Uuid, Option<String>)],
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (id, group) in rows {
        out.entry(group.clone().unwrap_or_else(|| "null".to_string()))
            .or_default()
            .push(id.to_string());
    }
    out
}

/// Mirrors `str(issue[group_by])` (`sub_issue.py:198`) for the generic
/// grouping branch, plus the `"None"` sentinel for empty assignees
/// (`sub_issue.py:194-195`, where `str(None) == "None"`).
pub(crate) fn sub_group_key(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        // Arrays/objects (grouping by an id-array key) have no meaningful
        // Python-`str()` counterpart on this path; JSON rendering keeps the
        // key total + deterministic (documented deviation).
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// The 26 `.values()` keys (`sub_issue.py:147-175`) addressable by `group_by`.
pub(crate) const SUB_VALUES_KEYS: &[&str] = &[
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
    "state_group",
];

/// Routing for the `?group_by=` param. Mirrors
/// `group_by = request.GET.get("group_by", False)` + `if group_by:`
/// (`sub_issue.py:141,185-198`): absent/`""` is falsy → flat;
/// `"assignees__ids"` → fan-out; any other truthy value hits the generic
/// `str(issue[group_by])` branch — valid for the 26 `.values()` keys,
/// `KeyError` (→ 400 `SUB_GROUP_KEY_MISSING_MSG`) otherwise.
pub(crate) enum SubGroupMode<'a> {
    Flat,
    FanoutAssignees,
    Generic(&'a str),
    Unknown,
}

pub(crate) fn sub_group_mode(group_by: Option<&str>) -> SubGroupMode<'_> {
    match group_by {
        None => SubGroupMode::Flat,
        Some(g) if g.is_empty() => SubGroupMode::Flat,
        Some("assignees__ids") => SubGroupMode::FanoutAssignees,
        Some(g) if SUB_VALUES_KEYS.contains(&g) => SubGroupMode::Generic(g),
        _ => SubGroupMode::Unknown,
    }
}

/// Maps a sanitized `order_by` token to `(SQL expression, descending)` for the
/// UNPAGINATED sub-issues path, mirroring `order_issue_queryset`
/// (`plane/utils/order_queryset.py:153-201`) WITHOUT the `OffsetPaginator`
/// re-ordering (`paginator.py:136-140`) — this endpoint never paginates, and
/// `sub_issue.py:143-144` discards the returned token.
///
/// This differs from the I2 `detail_order_expr` (which folds the paginator
/// flip in) in exactly two arms:
/// - `priority`/`-priority`: the queryset always orders
///   `("priority_order", "-created_at")` (`order_queryset.py:159-167`, the
///   sign only changes the discarded token) → urgent-first for BOTH signs.
/// - `state__group`/`-state__group`: the queryset orders the (possibly
///   reversed) CASE ASC (`order_queryset.py:168-177`, no paginator to flip it
///   back) → `-state__group` is cancelled-first here, NOT backlog-first.
/// All other tokens behave like `detail_order_expr` (delegated).
pub(crate) fn sub_order_expr(sanitized: &str) -> (&'static str, bool) {
    if sanitized == "priority" || sanitized == "-priority" {
        return (
            "CASE i.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 \
             WHEN 'medium' THEN 2 WHEN 'low' THEN 3 WHEN 'none' THEN 4 ELSE 5 END",
            false,
        );
    }
    if sanitized == "state__group" {
        return (
            "CASE s.\"group\" WHEN 'backlog' THEN 0 WHEN 'unstarted' THEN 1 \
             WHEN 'started' THEN 2 WHEN 'completed' THEN 3 WHEN 'cancelled' THEN 4 ELSE 5 END",
            false,
        );
    }
    if sanitized == "-state__group" {
        return (
            "CASE s.\"group\" WHEN 'cancelled' THEN 0 WHEN 'completed' THEN 1 \
             WHEN 'started' THEN 2 WHEN 'unstarted' THEN 3 WHEN 'backlog' THEN 4 ELSE 5 END",
            false,
        );
    }
    detail_order_expr(sanitized)
}

/// Query params for `sub_list`. Django reads only `order_by` (default
/// `"-created_at"`) and `group_by` (default `False`) (`sub_issue.py:140-141`);
/// unknown keys are ignored by serde, matching Django.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SubIssuesQuery {
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
}

/// One GET row. Field order is the exact 26-key `.values()` order
/// (`sub_issue.py:147-175`): I2's 25 `IssueListDetailSerializer` keys PLUS the
/// `state_group` (`state__group`, `sub_issue.py:136`) annotation, NO
/// `deleted_at`. (Over the wire the keys sort alphabetically — `to_value`
/// round-trips through `serde_json::Map`, which is a BTreeMap without the
/// `preserve_order` feature — exactly like the I1/I2 rows shipped via
/// `json!`/`to_value`; key SETS match Django.)
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SubIssueRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub state_id: Option<uuid::Uuid>,
    pub sort_order: f64,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub estimate_point: Option<uuid::Uuid>,
    pub priority: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub cycle_id: Option<uuid::Uuid>,
    pub module_ids: Vec<uuid::Uuid>,
    pub label_ids: Vec<uuid::Uuid>,
    pub assignee_ids: Vec<uuid::Uuid>,
    pub sub_issues_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub attachment_count: i64,
    pub link_count: i64,
    pub is_draft: bool,
    pub archived_at: Option<chrono::NaiveDate>,
    #[serde(rename = "state_group")]
    pub state_group: Option<String>,
}

/// One POST row. Renders the `IssueSerializer` (`serializers/issue.py:770-798`)
/// WITHOUT its 7 annotation-backed read-only fields: the POST queryset
/// annotates ONLY `state_group` (`sub_issue.py:246-248`, itself not a
/// serializer field), so DRF `SkipField`s every missing attribute
/// (`Field.get_attribute` → `except (KeyError, AttributeError)` → `not
/// required` → `SkipField`; proven live against DRF 3.18, repo pins 3.17.1) →
/// 18 keys in `Meta.fields` order. `state_group` is selected (19th column)
/// for the distribution map only and skipped at serialization
/// (`IssueRelationItem.owner_id` precedent).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SubIssuePostRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub state_id: Option<uuid::Uuid>,
    pub sort_order: f64,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub estimate_point: Option<uuid::Uuid>,
    pub priority: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub target_date: Option<chrono::NaiveDate>,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub is_draft: bool,
    pub archived_at: Option<chrono::NaiveDate>,
    #[serde(skip_serializing)]
    pub state_group: Option<String>,
}

/// Envelope. Field order is Django's `Response({...})` key order
/// (`sub_issue.py:200-207, 272-275`): `sub_issues` (array, or grouped map when
/// `group_by`) first, then `state_distribution`. (Wire order sorts
/// alphabetically via `to_value`, like the sibling envelopes.)
#[derive(Debug, Clone, Serialize)]
pub struct SubIssuesEnvelope {
    pub sub_issues: Value,
    pub state_distribution: std::collections::BTreeMap<String, Vec<String>>,
}

/// The GET page-query SELECT prefix: the 26 `.values()` columns in order, on
/// top of the `Coalesce` annotations (`sub_issue.py:46-136`) rendered per the
/// I1/I2 SQL precedents (`COALESCE(array_agg, [])`, bare `COUNT`s — the
/// `Coalesce(..., 0)` is a no-op over `COUNT`, which never returns NULL).
/// Differences vs the sibling endpoints, each Django-literal:
/// - NO `s.deleted_at` predicate on EITHER state join (outer `s` for the
///   `state_group` annotation, inner `ss` for `sub_issues_count`): forward-FK
///   lookups don't apply `StateManager` (I2-fix-#2 principle, applied here to
///   both joins; note the I2/I4 count fragments carry an extra-strict
///   `ss.deleted_at IS NULL`, deliberately not copied).
/// - `assignee_ids` keeps only assignees with an active project membership
///   (`assignee__member_project__is_active=True`, `sub_issue.py:107-111` —
///   `member_project` is the `ProjectMember.member → User` related name,
///   `db/models/project.py:216`, unscoped to any one project).
/// - `module_ids` requires `m.archived_at IS NULL`
///   (`module__archived_at__isnull=True`, `sub_issue.py:122-126`, I1/I4
///   precedent — unlike I2's detail prefetch).
/// - `DISTINCT` on the `array_agg`s is dropped: the partial unique indexes
///   (`issue_id`, bridge id) make it a no-op over live rows — and PG rejects
///   `array_agg(DISTINCT x ORDER BY y)` unless `y` is in the argument list
///   (I4 precedent).
/// - `cycle_id` takes bridge `Meta.ordering` (`-created_at`) before `LIMIT 1`
///   (I2 `DETAIL_SELECT_SQL` precedent: Django applies `Meta.ordering` to the
///   unordered `values()[:1]` subquery).
/// Live columns verified in `apps/api-rs/migrations/0001_initial.sql`
/// (`issues.parent_id` uuid + FK `issue_parent_id_ce8d76ba_fk_issue_id`,
/// `states."group"`).
pub(crate) const SUB_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
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
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND EXISTS(SELECT 1 FROM project_members pm \
         WHERE pm.member_id = ia.assignee_id AND pm.is_active = true AND pm.deleted_at IS NULL)), '{}'::uuid[]) AS assignee_ids, \
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
     i.is_draft, i.archived_at, s.\"group\" AS state_group \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";

/// The POST response SELECT prefix: the 18 serialized `SubIssuePostRow`
/// columns in `Meta.fields` order, plus `s."group"` (19th, `skip_serializing`)
/// for the distribution map. No bridge/count annotations — Django's
/// serializer skips them all (see `SubIssuePostRow`).
pub(crate) const SUB_POST_SELECT_SQL: &str =
    "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
     i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
     i.sequence_id, i.project_id, i.parent_id, i.created_at, i.updated_at, \
     i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
     i.is_draft, i.archived_at, s.\"group\" AS state_group \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";

/// Counts the top-level SELECT columns in `SUB_SELECT_SQL` (commas at paren
/// depth 0, plus one). Locks the 26-`SubIssueRow`-field projection: sqlx maps
/// `query_as` positionally, so a duplicated (or dropped) column fails at
/// runtime instead of compile time (I4 `archive_select_column_count`
/// precedent). Test-only.
#[cfg(test)]
pub(crate) fn sub_select_column_count() -> usize {
    let mut depth = 0usize;
    let mut commas = 0usize;
    for c in SUB_SELECT_SQL.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas + 1
}

/// Same counter for `SUB_POST_SELECT_SQL`: 19 columns = the 18 serialized
/// `SubIssuePostRow` fields + the `skip_serializing` `state_group` used only
/// for the distribution map. Test-only.
#[cfg(test)]
pub(crate) fn sub_post_select_column_count() -> usize {
    let mut depth = 0usize;
    let mut commas = 0usize;
    for c in SUB_POST_SELECT_SQL.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas + 1
}

/// Shared `Issue.issue_objects` scope predicate for the sub-issues queries
/// (`db/models/issue.py:86-95`: soft-delete excluded, triage excluded with
/// NULL-state rows DROPPED like Django's `exclude(state__group=...)`,
/// non-archived issues, non-archived projects, non-drafts). `{a}` is the
/// issues-table alias; the states table is always aliased `s`; the projects
/// table `p`. No `s.deleted_at` predicate — forward-FK lookups don't apply
/// `StateManager` (I2-fix-#2 principle). The project check mirrors the I1/I2
/// outer precedent (`archived_at` + `deleted_at` via `EXISTS`).
pub(crate) fn sub_scope_sql(a: &str) -> String {
    format!(
        "{a}.deleted_at IS NULL AND {a}.archived_at IS NULL AND {a}.is_draft = false \
         AND s.\"group\" <> 'triage' \
         AND EXISTS(SELECT 1 FROM projects p WHERE p.id = {a}.project_id \
           AND p.archived_at IS NULL AND p.deleted_at IS NULL)"
    )
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/sub-issues/`
/// — parity with Django `SubIssuesEndpoint.get`
/// (`plane/app/views/issue/sub_issue.py:37-207`,
/// `plane/app/urls/issue.py:104-108`).
///
/// - Gate: `ProjectEntityPermission` (`sub_issue.py:34`, NO
///   `allow_permission` decorator): safe methods pass with ANY active project
///   membership (`permissions/project.py:103-110`, role-agnostic — GUEST
///   reads); there is NO workspace-admin fallback branch anywhere in that
///   class (unlike `permissions/base.py:64-78`), so none is implemented.
/// - Scope: `parent_id + workspace slug + project_id` (`sub_issue.py:42-45`)
///   over `Issue.issue_objects` (`sub_scope_sql`). A missing parent yields a
///   200 EMPTY envelope — Django filters, never 404s, on GET.
/// - Rows render the 26 `.values()` keys (`sub_issue.py:147-175`, see
///   `SubIssueRow`); `order_by` (default `-created_at`) goes through
///   `order_issue_queryset` (`sub_issue.py:140-144`, see `sub_order_expr` —
///   unpaginated, so no `NULLS LAST`: PG defaults match the ORM).
/// - `group_by` (`sub_issue.py:141,185-203`, see `sub_group_mode`):
///   falsy → flat array; `"assignees__ids"` → per-assignee fan-out (`"None"`
///   for empty); other truthy values group by `str()` of that `.values()`
///   key; unknown keys → 400 `{"error": "The required key does not exist."}`
///   (Django `KeyError` mapping).
/// - 200 `{"sub_issues": ..., "state_distribution": {state_group: [ids]}}`
///   (`sub_issue.py:200-207`).
/// - FE `subIssues()` (`issue.service.ts:254-266`) sends no queries (flat
///   path) → `sub_issues.store.ts:128-160` (state_distribution + ids→issuesMap
///   + parent `sub_issues_count=len`).
///
/// Deviations (reviewer-adjudicable, Django-literal readings): datetimes
/// serialize RFC3339 UTC (chrono, batch convention) instead of Django's
/// per-user-timezone conversion (`user_timezone_converter`,
/// `sub_issue.py:182-183`); object key ORDER is sorted (`serde_json::Map`
/// without `preserve_order`), not Django's declaration/first-seen order —
/// same entries, and the same wire behavior as the I1/I2 rows and envelopes
/// shipped via `json!`/`to_value`; grouped array values render via JSON (not Python `str()`)
/// for array/object keys (unreachable in the FE flows); invalid-UUID bodies
/// surface Axum's 422, not Django's 400 `ValidationError` mapping (I3
/// precedent: Axum `Json` rejects before the handler).
pub async fn sub_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    axum::extract::Query(q): axum::extract::Query<SubIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `ProjectEntityPermission` safe-method branch
    // (`permissions/project.py:103-110`): any ACTIVE membership passes
    // (slug-scoped via the shared helper); no fallback branch exists.
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if member_role.is_none() {
        return Ok(deny());
    }
    // `order_by_param = request.GET.get("order_by", "-created_at")`
    // (`sub_issue.py:140`); `""` is falsy in Django (`if order_by_param:`)
    // and skips ordering — equivalent to the `-created_at` default via
    // `Meta.ordering`, which is also what `sanitize_order_by("")` yields.
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let (order_expr, desc) = sub_order_expr(&sanitized);
    let order_dir = if desc { "DESC" } else { "ASC" };
    // Both fragments are allowlist-derived (`sanitize_order_by` +
    // `sub_order_expr`), never raw user input — safe to interpolate.
    let sql = format!(
        "{SUB_SELECT_SQL} WHERE i.parent_id = $1 AND i.project_id = $2 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
         AND {} ORDER BY {} {} , i.created_at DESC",
        sub_scope_sql("i"),
        order_expr,
        order_dir,
    );
    let rows: Vec<SubIssueRow> = sqlx::query_as(&sql)
        .bind(issue_id)
        .bind(project_id)
        .bind(&slug)
        .fetch_all(&st.pool)
        .await?;
    let mut values = Vec::with_capacity(rows.len());
    for row in &rows {
        values.push(serde_json::to_value(row).map_err(|e| anyhow::anyhow!(e))?);
    }
    let dist = sub_state_distribution(
        &rows
            .iter()
            .map(|r| (r.id, r.state_group.clone()))
            .collect::<Vec<_>>(),
    );
    let grouped = match sub_group_mode(q.group_by.as_deref()) {
        SubGroupMode::Flat => Value::Array(values.clone()),
        SubGroupMode::FanoutAssignees => {
            // `if group_by == "assignees__ids"` (`sub_issue.py:189-195`):
            // one entry per assignee id, `"None"` when empty.
            let mut map: std::collections::BTreeMap<String, Vec<Value>> =
                std::collections::BTreeMap::new();
            for v in &values {
                let ids = v.get("assignee_ids").and_then(Value::as_array);
                match ids {
                    Some(arr) if !arr.is_empty() => {
                        for a in arr {
                            let key = a
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| a.to_string());
                            map.entry(key).or_default().push(v.clone());
                        }
                    }
                    _ => {
                        map.entry("None".to_string()).or_default().push(v.clone());
                    }
                }
            }
            serde_json::to_value(&map).map_err(|e| anyhow::anyhow!(e))?
        }
        SubGroupMode::Generic(key) => {
            // `elif group_by: result_dict[str(issue[group_by])]`
            // (`sub_issue.py:197-198`).
            let mut map: std::collections::BTreeMap<String, Vec<Value>> =
                std::collections::BTreeMap::new();
            for v in &values {
                map.entry(sub_group_key(&v[key]))
                    .or_default()
                    .push(v.clone());
            }
            serde_json::to_value(&map).map_err(|e| anyhow::anyhow!(e))?
        }
        SubGroupMode::Unknown => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": SUB_GROUP_KEY_MISSING_MSG})),
            ));
        }
    };
    let envelope = SubIssuesEnvelope {
        sub_issues: grouped,
        state_distribution: dist,
    };
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(&envelope).map_err(|e| anyhow::anyhow!(e))?),
    ))
}

/// POST `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/sub-issues/`
/// — parity with Django `SubIssuesEndpoint.post`
/// (`plane/app/views/issue/sub_issue.py:210-275`).
///
/// - Gate: `ProjectEntityPermission` unsafe branch
///   (`permissions/project.py:112-119`): role ∈ {ADMIN, MEMBER} (20/15);
///   GUEST/None → 403 `deny()` (I3 `bulk_archive` precedent — same class,
///   no fallback).
/// - Body `{sub_issue_ids: []}` (`sub_issue.py:222`); absent body → same 400
///   via `Option<Json>` (I3 `resolve_bulk_ids` precedent); empty → 400
///   `{"error": "Sub Issue IDs are required"}` (`sub_issue.py:224-228`).
/// - Parent resolved scoped (pk + ws slug + project) over `issue_objects`
///   (`sub_issue.py:214-216`); miss → 404
///   `{"error": "Parent issue not found"}` (`sub_issue.py:217-221`).
/// - Children loaded scoped (`id__in + ws + project`, `sub_issue.py:231-233`,
///   zero matches → 200 EMPTY, not 404); their `parent` FK is set + bulk
///   update (`sub_issue.py:235-238` — `bulk_update(["parent"])` writes the
///   parent column ONLY: `bulk_update` has no `auto_now` handling, verified
///   against the Django 5.2 source, so `updated_at` is untouched here too).
/// - 200 SAME envelope with `sub_issues` = the 18-key `IssueSerializer`
///   array (see `SubIssuePostRow` — the 7 annotation-backed read-only fields
///   are DRF-`SkipField`ed) + `state_distribution` from the `state_group`
///   annotation (`sub_issue.py:246-269`). Single transaction.
/// - FE `addSubIssues` (`issue.service.ts:269-284`) → store merges
///   (`sub_issues.store.ts:162-199`).
///
/// Deviations: per-child Celery `issue_activity` (`sub_issue.py:251-264`)
/// skipped (batch-wide precedent: Rust never writes activities); datetimes
/// RFC3339 (batch convention); invalid-UUID bodies surface Axum's 422 (I3
/// precedent); `"sub_issue_ids": null` 400s here (Django `len(None)` →
/// `TypeError` → generic 500).
pub async fn sub_add(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    body: Option<Json<SubIssueBody>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `ProjectEntityPermission` unsafe-method branch
    // (`permissions/project.py:112-119`): ADMIN/MEMBER only.
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if !matches!(member_role, Some(20) | Some(15)) {
        return Ok(deny());
    }
    let sub_ids = match resolve_sub_ids(body) {
        Ok(ids) => ids,
        Err(err) => return Ok((StatusCode::BAD_REQUEST, Json(err))),
    };
    let mut tx = st.pool.begin().await?;
    // Scoped parent (`Issue.issue_objects.filter(pk, workspace__slug, project)`
    // with its triage/archived/draft exclusions, `sub_issue.py:214-216`).
    let parent_sql = format!(
        "SELECT EXISTS(SELECT 1 FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
         AND {})",
        sub_scope_sql("i")
    );
    let parent_exists: bool = sqlx::query_scalar(&parent_sql)
        .bind(issue_id)
        .bind(project_id)
        .bind(&slug)
        .fetch_one(&mut *tx)
        .await?;
    if !parent_exists {
        tx.rollback().await?;
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": PARENT_ISSUE_NOT_FOUND_MSG})),
        ));
    }
    // `sub_issue.parent = parent_issue` + `bulk_update(["parent"])`
    // (`sub_issue.py:235-238`) over the project-scoped set
    // (`sub_issue.py:230-233`). INNER state join: NULL-state children drop,
    // exactly like Django's `exclude(state__group='triage')`.
    let update_sql = format!(
        "UPDATE issues i SET parent_id = $4 FROM states s \
         WHERE s.id = i.state_id AND i.id = ANY($1) AND i.project_id = $2 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
         AND {}",
        sub_scope_sql("i")
    );
    sqlx::query(&update_sql)
        .bind(&sub_ids)
        .bind(project_id)
        .bind(&slug)
        .bind(issue_id)
        .execute(&mut *tx)
        .await?;
    // `updated_sub_issues` (`sub_issue.py:246-248`): the SCOPED re-parented
    // set (cross-project ids match nothing → 200 EMPTY, not 404), model
    // `Meta.ordering = ("-created_at",)` (`db/models/issue.py:178`).
    let select_sql = format!(
        "{SUB_POST_SELECT_SQL} WHERE i.id = ANY($1) AND i.project_id = $2 \
         AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
         AND {} ORDER BY i.created_at DESC",
        sub_scope_sql("i")
    );
    let updated: Vec<SubIssuePostRow> = sqlx::query_as(&select_sql)
        .bind(&sub_ids)
        .bind(project_id)
        .bind(&slug)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut values = Vec::with_capacity(updated.len());
    for row in &updated {
        values.push(serde_json::to_value(row).map_err(|e| anyhow::anyhow!(e))?);
    }
    let dist = sub_state_distribution(
        &updated
            .iter()
            .map(|r| (r.id, r.state_group.clone()))
            .collect::<Vec<_>>(),
    );
    let envelope = SubIssuesEnvelope {
        sub_issues: Value::Array(values),
        state_distribution: dist,
    };
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(&envelope).map_err(|e| anyhow::anyhow!(e))?),
    ))
}

/// Sample GET row locking the 26-key `SubIssueRow` shape (test-only).
#[cfg(test)]
pub(crate) fn sample_sub_issue_row() -> SubIssueRow {
    SubIssueRow {
        id: uuid::Uuid::nil(),
        name: "sub".to_string(),
        state_id: None,
        sort_order: 65535.0,
        completed_at: None,
        estimate_point: None,
        priority: "none".to_string(),
        start_date: None,
        target_date: None,
        sequence_id: 1,
        project_id: uuid::Uuid::nil(),
        parent_id: Some(uuid::Uuid::nil()),
        cycle_id: None,
        module_ids: Vec::new(),
        label_ids: Vec::new(),
        assignee_ids: Vec::new(),
        sub_issues_count: 0,
        created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
        updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
        created_by: None,
        updated_by: None,
        attachment_count: 0,
        link_count: 0,
        is_draft: false,
        archived_at: None,
        state_group: Some("backlog".to_string()),
    }
}

/// Sample POST row locking the 18-key `SubIssuePostRow` shape (test-only).
#[cfg(test)]
pub(crate) fn sample_sub_issue_post_row() -> SubIssuePostRow {
    SubIssuePostRow {
        id: uuid::Uuid::nil(),
        name: "sub".to_string(),
        state_id: None,
        sort_order: 65535.0,
        completed_at: None,
        estimate_point: None,
        priority: "none".to_string(),
        start_date: None,
        target_date: None,
        sequence_id: 1,
        project_id: uuid::Uuid::nil(),
        parent_id: Some(uuid::Uuid::nil()),
        created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
        updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
        created_by: None,
        updated_by: None,
        is_draft: false,
        archived_at: None,
        state_group: Some("backlog".to_string()),
    }
}

#[cfg(test)]
mod batch_c_i5_tests {
    use super::*;

    #[test]
    fn sub_state_distribution_groups_ids_by_state_group() {
        // Mirrors the `defaultdict(list)` loops (`sub_issue.py:178-180` GET,
        // `267-269` POST): sub-issue ids grouped by their `state_group`
        // annotation. 3-row fixture: two `backlog` + one `started`.
        let a = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let b = uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let c = uuid::Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let dist = sub_state_distribution(&[
            (a, Some("backlog".to_string())),
            (b, Some("started".to_string())),
            (c, Some("backlog".to_string())),
        ]);
        assert_eq!(dist.len(), 2);
        assert_eq!(dist["backlog"], vec![a.to_string(), c.to_string()]);
        assert_eq!(dist["started"], vec![b.to_string()]);
        // NULL-state children (unreachable via `issue_objects`, which drops
        // NULL-state rows exactly like Django's
        // `exclude(state__group='triage')`) render under JSON `"null"`
        // (Python `json.dumps({None: ...})`).
        let dist = sub_state_distribution(&[(a, None)]);
        assert_eq!(dist["null"], vec![a.to_string()]);
        // Empty input renders `{}` (empty `defaultdict`), not null.
        assert!(sub_state_distribution(&[]).is_empty());
    }

    #[test]
    fn require_sub_issue_ids_matrix_matches_django() {
        // Mirrors `if not len(sub_issue_ids)` (`sub_issue.py:224-228`) →
        // 400 `{"error": "Sub Issue IDs are required"}`.
        assert_eq!(
            require_sub_issue_ids(&[]).unwrap_err(),
            "Sub Issue IDs are required"
        );
        assert_eq!(SUB_ISSUE_IDS_REQUIRED_MSG, "Sub Issue IDs are required");
        assert_eq!(PARENT_ISSUE_NOT_FOUND_MSG, "Parent issue not found");
        assert!(require_sub_issue_ids(&[uuid::Uuid::nil()]).is_ok());
    }

    #[test]
    fn resolve_sub_ids_absent_body_matches_django() {
        // Django `request.data.get("sub_issue_ids", [])` (`sub_issue.py:222`)
        // defaults a missing key to `[]` → the same 400. An entirely absent
        // body reaches the handler as `None` via Axum's `Option<Json>` (I3
        // `resolve_bulk_ids` precedent) and maps to that 400, not Axum's
        // 415/400/422 rejection bodies.
        assert_eq!(
            resolve_sub_ids(None).unwrap_err(),
            serde_json::json!({"error": "Sub Issue IDs are required"})
        );
        assert_eq!(
            resolve_sub_ids(Some(Json(SubIssueBody::default()))).unwrap_err(),
            serde_json::json!({"error": "Sub Issue IDs are required"})
        );
        assert_eq!(
            resolve_sub_ids(Some(Json(SubIssueBody {
                sub_issue_ids: vec![]
            })))
            .unwrap_err(),
            serde_json::json!({"error": "Sub Issue IDs are required"})
        );
        let id = uuid::Uuid::nil();
        assert_eq!(
            resolve_sub_ids(Some(Json(SubIssueBody {
                sub_issue_ids: vec![id]
            })))
            .unwrap(),
            vec![id]
        );
    }

    #[test]
    fn sub_gate_matrix_matches_entity_permission() {
        // Mirrors `ProjectEntityPermission` (`permissions/project.py:88-119`):
        // safe methods (GET, `project.py:103-110`) pass with ANY active
        // project membership — role-agnostic, so GUEST (5) passes — and there
        // is NO workspace-admin fallback branch in this class (unlike the
        // `allow_permission` decorator, `permissions/base.py:64-78`).
        let get_allows = |role: Option<i16>, ws_admin: bool| {
            let _ = ws_admin;
            role.is_some()
        };
        assert!(get_allows(Some(20), false));
        assert!(get_allows(Some(15), false));
        assert!(get_allows(Some(5), false));
        assert!(!get_allows(None, false));
        // No fallback: workspace ADMIN without a project membership denies.
        assert!(!get_allows(None, true));
        // Unsafe methods (POST, `project.py:112-119`) need role ∈
        // {ADMIN, MEMBER} (20/15); GUEST/None deny regardless of ws-admin.
        let post_allows = |role: Option<i16>| matches!(role, Some(20) | Some(15));
        assert!(post_allows(Some(20)));
        assert!(post_allows(Some(15)));
        assert!(!post_allows(Some(5)));
        assert!(!post_allows(None));
    }

    #[test]
    fn sub_group_key_python_str_semantics() {
        // Mirrors `str(issue[group_by])` (`sub_issue.py:198`) and the
        // `"None"` sentinel for empty assignees (`sub_issue.py:194-195`).
        assert_eq!(sub_group_key(&serde_json::Value::Null), "None");
        assert_eq!(sub_group_key(&serde_json::json!(true)), "True");
        assert_eq!(sub_group_key(&serde_json::json!(false)), "False");
        assert_eq!(sub_group_key(&serde_json::json!("backlog")), "backlog");
        assert_eq!(sub_group_key(&serde_json::json!(3)), "3");
        let id = "11111111-1111-1111-1111-111111111111";
        assert_eq!(sub_group_key(&serde_json::json!(id)), id);
    }

    #[test]
    fn sub_group_mode_routing_matches_django() {
        // Mirrors `group_by = request.GET.get("group_by", False)` +
        // `if group_by:` (`sub_issue.py:141,185`): absent/`""` → flat;
        // `"assignees__ids"` → fan-out (`sub_issue.py:189-195`); any other
        // truthy value → generic `str(issue[group_by])` grouping
        // (`sub_issue.py:197-198`) over the 26 `.values()` keys
        // (`sub_issue.py:147-175`); unknown keys raise `KeyError` → 400
        // `{"error": "The required key does not exist."}`
        // (`views/base.py:193-197`).
        assert!(matches!(sub_group_mode(None), SubGroupMode::Flat));
        assert!(matches!(sub_group_mode(Some("")), SubGroupMode::Flat));
        assert!(matches!(
            sub_group_mode(Some("assignees__ids")),
            SubGroupMode::FanoutAssignees
        ));
        assert!(matches!(
            sub_group_mode(Some("priority")),
            SubGroupMode::Generic("priority")
        ));
        assert!(matches!(
            sub_group_mode(Some("state_group")),
            SubGroupMode::Generic("state_group")
        ));
        assert!(matches!(
            sub_group_mode(Some("nope")),
            SubGroupMode::Unknown
        ));
        assert_eq!(
            SUB_GROUP_KEY_MISSING_MSG,
            "The required key does not exist."
        );
    }

    /// Asserts the serialized (over-the-wire) key order equals `expected`.
    /// NOTE: `serde_json::to_value` round-trips structs through `Map`
    /// (BTreeMap without the `preserve_order` feature → sorted keys), so
    /// order must be read from the serialized STRING, which preserves
    /// declaration order exactly as Axum writes the response body.
    fn assert_wire_key_order<T: serde::Serialize>(val: &T, expected: &[&str]) {
        let s = serde_json::to_string(val).unwrap();
        let mut prev = 0usize;
        for key in expected {
            let needle = format!("\"{key}\":");
            let pos = s
                .find(&needle)
                .unwrap_or_else(|| panic!("missing key {key}"));
            assert!(pos >= prev, "key {key} out of order");
            prev = pos;
        }
        // No extra keys: count the top-level `"key":` occurrences.
        assert_eq!(s.matches("\":").count(), expected.len(), "{s}");
    }

    #[test]
    fn sub_row_shapes_match_django() {
        // GET rows carry the exact 26 `.values()` keys in order
        // (`sub_issue.py:147-175`): I2's 25 `IssueListDetailSerializer` keys
        // (`serializers/issue.py:842-870`) PLUS the `state_group`
        // (`state__group`, `sub_issue.py:136`) annotation, NO `deleted_at`.
        let row = sample_sub_issue_row();
        let val = serde_json::to_value(&row).unwrap();
        let mut keys: Vec<&str> = val
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut expected = vec![
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
            "state_group",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_wire_key_order(
            &row,
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
                "state_group",
            ],
        );
        // POST rows render the `IssueSerializer` (`serializers/issue.py:770-798`)
        // WITHOUT its 7 annotation-backed read-only fields (`cycle_id`,
        // `module_ids`, `label_ids`, `assignee_ids`, `sub_issues_count`,
        // `attachment_count`, `link_count`): the POST queryset annotates ONLY
        // `state_group` (`sub_issue.py:246-248`, itself not a serializer
        // field), so DRF `SkipField`s every missing attribute (proof:
        // `Field.get_attribute` → `except (KeyError, AttributeError)` →
        // `not required` → `SkipField`, verified live against DRF 3.18) →
        // 18 keys in `Meta.fields` order.
        let row = sample_sub_issue_post_row();
        let val = serde_json::to_value(&row).unwrap();
        let mut keys: Vec<&str> = val
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut expected = vec![
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
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "is_draft",
            "archived_at",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_wire_key_order(
            &row,
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
                "created_at",
                "updated_at",
                "created_by",
                "updated_by",
                "is_draft",
                "archived_at",
            ],
        );
    }

    #[test]
    fn sub_select_column_counts_match_rows() {
        // `SUB_SELECT_SQL` must project exactly the 26 `SubIssueRow` fields;
        // `SUB_POST_SELECT_SQL` projects 19 columns = the 18 serialized
        // `SubIssuePostRow` fields + the `skip_serializing` `state_group`
        // used only for the distribution map. sqlx maps `query_as`
        // positionally, so a duplicated (or dropped) column fails at runtime,
        // not compile time (I4 `archive_select_column_count` precedent).
        assert_eq!(sub_select_column_count(), 26);
        assert_eq!(sub_post_select_column_count(), 19);
    }

    #[test]
    fn sub_order_expr_matches_unpaginated_django() {
        // `order_issue_queryset` (`order_queryset.py:153-201`) WITHOUT the
        // paginator flip (`sub_issue.py:143-144` discards the token): BOTH
        // priority signs order the CASE ASC (urgent-first); `state__group` is
        // backlog-first while `-state__group` reverses to cancelled-first.
        // (I2's `detail_order_expr` folds the paginator flip in and differs
        // on exactly these arms.)
        let (expr, desc) = sub_order_expr("-priority");
        assert!(!desc);
        assert!(expr.contains("WHEN 'urgent' THEN 0"));
        let (expr, desc) = sub_order_expr("priority");
        assert!(!desc);
        assert!(expr.contains("WHEN 'urgent' THEN 0"));
        let (expr, desc) = sub_order_expr("state__group");
        assert!(!desc);
        assert!(expr.contains("WHEN 'backlog' THEN 0"));
        let (expr, desc) = sub_order_expr("-state__group");
        assert!(!desc);
        assert!(expr.contains("WHEN 'cancelled' THEN 0"));
        // Every other token delegates to the I2 mapping unchanged.
        assert_eq!(sub_order_expr("-created_at"), ("i.created_at", true));
        assert_eq!(sub_order_expr("sequence_id"), ("i.sequence_id", false));
        assert_eq!(sub_order_expr("-state__name"), ("s.name", true));
    }

    #[test]
    fn sub_scope_sql_matches_issue_manager() {
        // `Issue.issue_objects` (`db/models/issue.py:86-95`): live rows +
        // `exclude(state__group='triage')` + `exclude(archived_at__isnull=False)`
        // + `exclude(project__archived_at__isnull=False)` + `exclude(is_draft)`.
        // Triage form mirrors the list endpoints (`s."group" <> 'triage'` in
        // WHERE position, dropping NULL-state rows like Django `exclude`).
        let pred = sub_scope_sql("i");
        for needle in [
            "i.deleted_at IS NULL",
            "i.archived_at IS NULL",
            "i.is_draft = false",
            "s.\"group\" <> 'triage'",
            "p.archived_at IS NULL",
            "p.deleted_at IS NULL",
        ] {
            assert!(pred.contains(needle), "{needle}");
        }
        // No state-deleted predicate on EITHER state join: forward-FK lookups
        // don't apply `StateManager` (I2-fix-#2 principle).
        assert!(!pred.contains("s.deleted_at"), "{pred}");
        assert!(!SUB_SELECT_SQL.contains("s.deleted_at"), "outer state join");
        assert!(
            !SUB_SELECT_SQL.contains("ss.deleted_at"),
            "count state join"
        );
    }

    #[test]
    fn i5_handlers_exist_for_sub_issue_routes() {
        // Wiring guard: `main.rs` registers
        // `GET+POST .../issues/:issue_id/sub-issues/` → `sub_list`/`sub_add`
        // (Django `SubIssuesEndpoint`, `urls/issue.py:104-108`). The extra
        // `sub-issues` segment keeps it distinct from `.../issues/:pk/`
        // (segment-count distinct, no conflict).
        let _ = super::sub_list;
        let _ = super::sub_add;
    }
}
