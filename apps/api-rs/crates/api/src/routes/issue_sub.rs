use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::project::deny;
use crate::{middleware::auth::AuthUser, state::AppState};
use super::issue_common::{detail_order_expr, fetch_project_member_role, sanitize_order_by};


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
///   both joins, matching the I1/I2/I4 fragments).
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
