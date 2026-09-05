use serde::Serialize;
use serde_json::Value;

use crate::routes::project::ws_role;


#[derive(Debug, Clone, Serialize)]
pub struct IssueOut {
    pub id: uuid::Uuid,
    pub name: String,
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
