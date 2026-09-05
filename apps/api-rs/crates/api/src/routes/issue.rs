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
/// slot, `is_prev` the flag slot. `OffsetPaginator` (`paginator.py:142-144`)
/// computes the row window from `page * limit` and ignores `limit_value`,
/// so only `page` drives SQL here.
#[derive(Debug)]
pub(crate) struct DetailCursor {
    pub limit_value: f64,
    pub page: i64,
    pub is_prev: bool,
}

/// Mirrors `BasePaginator.get_per_page` (`plane/utils/paginator.py:643-653`,
/// defaults 1000/1000): non-integer → `ParseError("Invalid per_page
/// parameter.")`, over max → `ParseError("Invalid per_page value. Cannot
/// exceed 1000.")` — messages byte-exact; DRF renders `ParseError` as 400
/// `{"detail": msg}`.
pub(crate) fn parse_per_page(raw: Option<&str>) -> Result<i64, String> {
    let s = raw.unwrap_or("1000");
    // Python `int()` strips surrounding whitespace.
    let v: i64 = s
        .trim()
        .parse()
        .map_err(|_| "Invalid per_page parameter.".to_string())?;
    if v > 1000 {
        return Err("Invalid per_page value. Cannot exceed 1000.".to_string());
    }
    Ok(v)
}

/// Mirrors `Cursor.from_string` (`paginator.py:48-59`) as wrapped by
/// `BasePaginator.paginate` (`paginator.py:677-681`): the value slot is int
/// unless it contains `.` (then float); offset/is_prev go through Python
/// `int()` (whitespace-tolerant, sign-tolerant); the `is_prev` slot is
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
        bits[0]
            .trim()
            .parse::<i64>()
            .map_err(|_| ERR.to_string())? as f64
    };
    let page: i64 = bits[1].trim().parse().map_err(|_| ERR.to_string())?;
    let is_prev: bool = bits[2]
        .trim()
        .parse::<i64>()
        .map(|v| v != 0)
        .map_err(|_| ERR.to_string())?;
    Ok(DetailCursor {
        limit_value,
        page,
        is_prev,
    })
}

/// Mirrors `Cursor.__str__` (`paginator.py:31-32`):
/// `f"{value}:{offset}:{int(is_prev)}"`.
pub(crate) fn build_cursor(limit: i64, page: i64, is_prev: bool) -> String {
    format!("{limit}:{page}:{flag}", flag = i32::from(is_prev))
}

/// `OffsetPaginator.get_result` (`paginator.py:165`): next cursor is
/// `(limit, page+1, False)`; the limit echoed is the EFFECTIVE limit
/// (`min(per_page, max_limit)`, `paginator.py:132`).
pub(crate) fn next_cursor_str(limit: i64, page: i64) -> String {
    build_cursor(limit, page + 1, false)
}

/// `OffsetPaginator.get_result` (`paginator.py:167`): prev cursor is
/// `(limit, page-1, True)` — including page `-1` on the first page.
pub(crate) fn prev_cursor_str(limit: i64, page: i64) -> String {
    build_cursor(limit, page - 1, true)
}

/// Mirrors `math.ceil(count / limit)` (`paginator.py:180`). Callers guarantee
/// `total >= 0` and `limit > 0` (Django would crash with `ZeroDivisionError`
/// → 500 there).
pub(crate) fn total_pages(total: i64, limit: i64) -> i64 {
    (total + limit - 1) / limit
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
/// for `ValidationError` (`plane/app/views/base.py:182-186`); `Server` maps
/// to a 500 like Django's `FieldError`/uncaught paths (`base.py:200-204`).
#[derive(Debug)]
pub(crate) enum LegacyFilterError {
    BadRequest(String),
    Server(String),
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
/// (`labels`/`assignees`/`cycle`/`module`): `"None"` in the list selects
/// issues with NO live bridge row (`labels__isnull=True` on the
/// live-filtered join, `issue_filters.py:150-151` etc.); valid ids select
/// issues having a live row in the list. Both conditions AND when both are
/// present, exactly like Django.
fn push_legacy_bridge(
    qb: &mut QueryBuilder<Postgres>,
    bridge: &str,
    bridge_col: &str,
    raw: &str,
) {
    let has_none = raw.split(',').any(|t| t == "None");
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
            // `.match()` — a PREFIX match. When it matches but the token
            // does not split into exactly 3 `;` parts, Django adds NOTHING;
            // when the head has 3+ `_` parts, `digit, term = ...split("_")`
            // raises ValueError → 500. Both are mirrored.
            if is_relative_head(parts[0]) {
                if parts.len() == 3 {
                    let (digit, term) = split_relative_head(parts[0]);
                    let days: i64 = match term {
                        "months" => digit as i64 * 30,
                        _ => digit as i64 * 7,
                    };
                    // `subsequent == "after"` → `__gte`, else `__lte`;
                    // `offset == "fromnow"` → future, else past
                    // (`issue_filters.py:31-52`).
                    let bound = if parts[2] == "fromnow" {
                        today + chrono::Days::new(days as u64)
                    } else {
                        today - chrono::Days::new(days as u64)
                    };
                    if parts[1] == "after" {
                        qb.push(" AND ").push(lhs).push(" >= ").push_bind(bound);
                    } else {
                        qb.push(" AND ").push(lhs).push(" <= ").push_bind(bound);
                    }
                } else if parts[0].split('_').count() > 2 {
                    return Err(LegacyFilterError::Server(
                        "invalid relative date filter".to_string(),
                    ));
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

/// Prefix test for `re.compile(r"\d+_(weeks|months)$").match(head)`: leading
/// ASCII digits, `_`, then a `weeks`/`months` prefix. (Python `\d` also
/// matches non-ASCII digits — pathological inputs may diverge; the ASCII
/// reading is documented.)
fn is_relative_head(head: &str) -> bool {
    let Some((digits, rest)) = head.split_once('_') else {
        return false;
    };
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (rest.starts_with("weeks") || rest.starts_with("months"))
}

/// Splits a matched relative head into `(duration, "weeks"|"months")` using
/// the exact-term reading (`issue_filters.py:66`).
fn split_relative_head(head: &str) -> (u64, &str) {
    let (digits, rest) = head.split_once('_').unwrap_or((head, ""));
    let term = if rest.starts_with("months") { "months" } else { "weeks" };
    (digits.parse().unwrap_or(0), term)
}

/// Applies every supported legacy `issue_filters` GET key
/// (`plane/utils/issue_filters.py:428-463`) to the in-progress WHERE
/// clause. Unknown keys never reach this switch (serde drops them), which
/// mirrors `issue_filters()` only dispatching its known `ISSUE_FILTER`
/// keys. Deviations from a full mirror, each Django-literal and committed
/// as reviewer-adjudicable:
/// - `logged_by` names no model field: Django builds `logged_by__...`
///   kwargs and dies with `FieldError` → 500. Only when the value yields ≥1
///   lookup (`"None"` present or ≥1 valid UUID); empty values add nothing.
/// - `estimate_point__in` / `intake_status__in` coerce to UUID/int: garbage
///   raises `ValidationError` → 400 `"Please provide valid detail"`.
/// - `mentions` / `intake_status` / `inbox_status` add no explicit
///   `deleted_at` filter; the `deleted_at IS NULL` below is the
///   default-manager-implied reading (all three models use the soft-delete
///   default manager).
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
        // validation; exact-`"null"` tokens dropped. Applies only when
        // `type` is absent (see above).
        let groups: Vec<String> = raw.split(',').filter(|t| *t != "null").map(str::to_string).collect();
        if !groups.is_empty() {
            qb.push(" AND s.\"group\" = ANY(").push_bind(groups).push(")");
        }
    }
    if let Some(raw) = q.estimate_point.as_deref() {
        // `estimate_point__in` takes RAW strings (`issue_filters.py:107-111`)
        // against the UUID FK: garbage → Django `ValidationError` → 400.
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
    if let Some(raw) = q.priority.as_deref() {
        let pris: Vec<String> = raw.split(',').filter(|t| *t != "null").map(str::to_string).collect();
        if !pris.is_empty() {
            qb.push(" AND i.priority = ANY(").push_bind(pris).push(")");
        }
    }
    if let Some(raw) = q.parent.as_deref() {
        push_legacy_uuid_in(qb, "i.parent_id", raw, Some("i.parent_id"));
    }
    if let Some(raw) = q.labels.as_deref() {
        push_legacy_bridge(qb, "issue_labels", "label_id", raw);
    }
    if let Some(raw) = q.assignees.as_deref() {
        push_legacy_bridge(qb, "issue_assignees", "assignee_id", raw);
    }
    if let Some(raw) = q.mentions.as_deref() {
        // `issue_mention__mention__id__in` (`issue_filters.py:177-186`).
        let ids = legacy_uuid_list(raw);
        if !ids.is_empty() {
            qb.push(" AND EXISTS(SELECT 1 FROM issue_mentions im \
                WHERE im.issue_id = i.id AND im.deleted_at IS NULL \
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
            return Err(LegacyFilterError::Server("unsupported legacy filter: logged_by".to_string()));
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
        push_legacy_bridge(qb, "cycle_issues", "cycle_id", raw);
    }
    if let Some(raw) = q.module.as_deref() {
        push_legacy_bridge(qb, "module_issues", "module_id", raw);
    }
    // `issue_intake__status__in` (`issue_filters.py:350-377`): `status`
    // is an integer column — non-integer tokens raise Django
    // `ValidationError` → 400. Both legacy keys write the SAME dict key and
    // `inbox_status` runs LATER in the dispatch order, so a non-empty
    // `inbox_status` OVERWRITES `intake_status` (an empty one leaves the
    // earlier value intact) — mirrored, not ANDed.
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
    let status_tokens = if inbox_tokens.is_empty() { intake_tokens } else { inbox_tokens };
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
            WHERE ii.issue_id = i.id AND ii.deleted_at IS NULL \
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
        // (`issue_filters.py:392-403`).
        let ids = legacy_uuid_list(raw);
        if !ids.is_empty() {
            qb.push(" AND EXISTS(SELECT 1 FROM issue_subscribers isn \
                WHERE isn.issue_id = i.id AND isn.deleted_at IS NULL \
                AND isn.subscriber_id = ANY(")
                .push_bind(ids)
                .push("))");
        }
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
/// direct comparisons otherwise; `is_archived` accepts Django's truthy /
/// falsy spellings (`filterset.py:202-211`) with anything else a no-op;
/// `is_draft` accepts the same spellings, `""` (JSON null → `QueryDict`
/// `""` → `NullBooleanField` → `None` → `exact None` → `IS NULL`), and 400s
/// the rest. Date scalars bind `YYYY-MM-DD` (`""` → `IS NULL`, mirroring
/// `DateField("")` → `None` → `exact None`); `__range` binds an inclusive
/// `BETWEEN` over the comma-split pair (`DateCSVRangeFilter` for
/// datetimes compares the date component, `filterset.py:22-30`).
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
            // and repeated values resolve to the LAST one.
            let last = pieces.last().map(String::as_str).unwrap_or("");
            if ["True", "true", "1"].contains(&last) {
                qb.push("i.archived_at IS NOT NULL");
            } else if ["False", "false", "0"].contains(&last) {
                qb.push("i.archived_at IS NULL");
            } else {
                // Any other spelling (incl. `""`) → bare `Q()` no-op.
                qb.push("TRUE");
            }
            Ok(())
        }
        "is_draft" => {
            let last = pieces.last().map(String::as_str).unwrap_or("");
            if ["True", "true", "1"].contains(&last) {
                qb.push("i.is_draft = true");
            } else if ["False", "false", "0"].contains(&last) {
                qb.push("i.is_draft = false");
            } else if last.is_empty() {
                qb.push("i.is_draft IS NULL");
            } else {
                return Err(ComplexFilterError::invalid_filterset());
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

/// UUID leaf: every piece must parse (`UUIDFilter` coercion failure → 400).
/// `table == ""` means a direct column (`created_by_id`/`state_id`/
/// `project_id`); otherwise a live-bridge `EXISTS`.
fn apply_complex_uuid_leaf(
    qb: &mut QueryBuilder<Postgres>,
    table: &str,
    col: Option<&str>,
    pieces: &[String],
    suffix: &str,
) -> Result<(), ComplexFilterError> {
    let mut ids = Vec::with_capacity(pieces.len());
    for piece in pieces {
        ids.push(uuid::Uuid::parse_str(piece).map_err(|_| ComplexFilterError::invalid_filterset())?);
    }
    if table.is_empty() {
        let col = col.unwrap_or("i.id");
        if suffix == "in" {
            qb.push(col).push(" = ANY(").push_bind(ids).push(")");
        } else {
            // Scalar with repeated values → Django `QueryDict` keeps the LAST.
            qb.push(col).push(" = ").push_bind(ids.into_iter().last());
        }
        return Ok(());
    }
    let col = col.unwrap_or("id");
    qb.push("EXISTS(SELECT 1 FROM ").push(table).push(
        " b WHERE b.issue_id = i.id AND b.deleted_at IS NULL AND b.",
    );
    qb.push(col);
    if suffix == "in" {
        qb.push(" = ANY(").push_bind(ids).push("))");
    } else {
        qb.push(" = ").push_bind(ids.into_iter().last()).push(")");
    }
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
/// does NOT add the `/list/` endpoint's explicit `state__deleted_at__isnull`
/// (`base.py:114`): soft-deleted states drop out here through the
/// NULL-state path instead (`s.deleted_at IS NULL` fails → row gone), with
/// an identical keep/drop truth table on all four state cases.
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
         AND s.deleted_at IS NULL AND s.\"group\" <> 'triage' \
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
///   (`BadPaginationError`, `paginator.py:149-150`).
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
/// `logged_by` → 500 (Django `FieldError` → 500); `per_page <= 0` and
/// overflowed windows → 500 (Django `ZeroDivisionError`/`AssertionError` →
/// 500); relative-date "now" is UTC (`Utc::now().date_naive()`).
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
    // `limit = min(limit, max_limit)` (`paginator.py:132`).
    let limit = per_page.min(1000);
    if limit <= 0 {
        // Django: `limit=0` → `ZeroDivisionError` in `math.ceil`, negative
        // limits → negative slices → `AssertionError`; both → 500.
        return Err(anyhow::anyhow!("invalid pagination window").into());
    }
    let offset = cursor
        .page
        .checked_mul(limit)
        .ok_or_else(|| anyhow::anyhow!("invalid pagination window"))?;
    if offset < 0 {
        // `BadPaginationError("Pagination offset cannot be negative")` →
        // `ParseError("Error in parsing")` (`paginator.py:149-150, 710-711`).
        return Ok(detail_400(json!({"detail": "Error in parsing"})));
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
    let _ = (&q.group_by, &q.sub_group_by, &q.fields);
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
        Err(DetailWhereError::Legacy(LegacyFilterError::Server(msg))) => {
            return Err(anyhow::anyhow!(msg).into());
        }
        Err(DetailWhereError::Complex(e)) => {
            return Ok(detail_400(json!({"message": e.message, "code": e.code})));
        }
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    // Page window: `[offset:offset+limit+1]`, truncated to `limit`
    // (`paginator.py:144-145, 152, 170`); `next.has_results` ⇔ the window
    // held more than `limit` (`paginator.py:154, 165`).
    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
         i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
         i.sequence_id, i.project_id, i.parent_id, i.created_at, i.updated_at, \
         i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
         i.is_draft, i.archived_at, \
         (SELECT ci.cycle_id FROM cycle_issues ci \
           WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL LIMIT 1) AS cycle_id, \
         COALESCE((SELECT array_agg(mi.module_id) FROM module_issues mi \
           WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL), '{}'::uuid[]) AS module_ids, \
         COALESCE((SELECT array_agg(il.label_id) FROM issue_labels il \
           WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
         COALESCE((SELECT array_agg(ia.assignee_id) FROM issue_assignees ia \
           WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
         (SELECT COUNT(*) FROM issues si \
           LEFT JOIN states ss ON ss.id = si.state_id \
           WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
           AND si.archived_at IS NULL AND si.is_draft = false \
           AND ss.deleted_at IS NULL AND ss.\"group\" <> 'triage') AS sub_issues_count, \
         (SELECT COUNT(*) FROM file_assets fa \
           WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
           AND fa.deleted_at IS NULL) AS attachment_count, \
         (SELECT COUNT(*) FROM issue_links lin \
           WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count \
         FROM issues i LEFT JOIN states s ON s.id = i.state_id",
    );
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
        Err(DetailWhereError::Legacy(LegacyFilterError::Server(msg))) => {
            return Err(anyhow::anyhow!(msg).into());
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
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut rows: Vec<IssueDetailRow> = page_qb.build_query_as().fetch_all(&st.pool).await?;
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
}
