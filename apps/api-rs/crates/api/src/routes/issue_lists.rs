use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};

use crate::routes::project::{deny, missing, ws_role, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{
    ArchiveRow, DetailEnvelope, PageWindow, build_cursor, detail_order_expr,
    fetch_project_member_role, is_workspace_admin, next_cursor_str, page_window, parse_cursor,
    parse_per_page, prev_cursor_str, project_gate_allows, sanitize_order_by, total_pages,
};
use super::issue_query::{
    DetailIssuesQuery, GENERIC_500_MSG, apply_complex_filter, archive_group_by_allowlist_error,
    archive_group_by_conflict, archive_grouping_unsupported, parse_complex_filter,
    ARCHIVE_GROUPING_UNSUPPORTED_MSG,
};
use super::versions::{VersionEnvelope, parse_version_cursor};

/// Workspace issues + v2 issues + user-issues — parity with Django
/// `WorkspaceViewIssuesViewSet.list` (`plane/app/views/view/base.py:222-259`,
/// `plane/app/urls/views.py:51-55`), `IssuePaginatedViewSet.list`
/// (`plane/app/views/issue/base.py:816-972`,
/// `plane/app/urls/issue.py:54-58`) and
/// `WorkspaceUserProfileIssuesEndpoint.get`
/// (`plane/app/views/workspace/user.py:98-203`,
/// `plane/app/urls/workspace.py:152-156`). Celery/activity side-effects
/// skipped (Batch C precedent — these endpoints are read-only GETs).
///
/// Locked semantics (plan D12):
/// - D12a `GET /api/workspaces/:slug/issues/`: 200 offset-paginated 26-key
///   rows (`ViewIssueListSerializer`, `serializers/view.py:24-52`), gate
///   WORKSPACE ADMIN/MEMBER/GUEST (`view/base.py:222`).
/// - D12b `GET .../projects/:pid/v2/issues/`: 200 cursor-`paginate()`
///   27-key rows (`base.py:871-898` + `description_html` iff
///   `?description=true`), `ORDER BY updated_at ASC` (not created_at!,
///   `base.py:906-907`), `?updated_at__gt` filter, guest
///   (role==5 AND NOT `project.guest_view_all_features`) scoped to
///   `created_by=user` (`base.py:910-920`); gate ADMIN/MEMBER/GUEST;
///   project miss → 404. NO `v2/work-items/` (Django defines none).
/// - D12c `GET /api/workspaces/:slug/user-issues/:uid/`: 200
///   `issue_on_results` (scope assignee∨creator∨subscriber `:uid`,
///   requester must be an ACTIVE project member, `user.py:139-147`;
///   annotations cycle_id subquery + link/attachment/sub_issues counts,
///   `user.py:104-133`); gate `WorkspaceViewerPermission`;
///   `group_by==sub_group_by` → 400 `{"error": "Group by and sub group by
///   cannot have same parameters"}` (`user.py:176-181`).
/// - Reuses `issue_common` paginator helpers (`parse_per_page`,
///   `parse_cursor`, `page_window`, `total_pages`, `next/prev_cursor_str`,
///   `sanitize_order_by`, `detail_order_expr`, `DetailEnvelope`,
///   `ArchiveRow`, gate helpers) and the `issue_query` complex-filter +
///   group-by helpers where shapes allow — no forks. The v2 cursor path
///   reuses `versions::parse_version_cursor` + `VersionEnvelope` (same
///   `global_paginator.paginate()` family, D6 precedent).
///
/// Deviations (documented, reviewer-adjudicable):
/// - Datetimes serialize RFC3339 UTC (chrono, batch convention) instead of
///   DRF's per-user-timezone conversion.
/// - D12a/D12c honor `?filters=` (ComplexFilterBackend JSON tree, reused
///   helpers incl. byte-exact 400s), `?order_by=` (default `-created_at`,
///   reused `order_issue_queryset` mapping) and `?per_page=`/`?cursor=`
///   (reused `BasePaginator` mapping). Legacy `issue_filters()` keys and
///   `IssueFilterSet`/`DjangoFilterBackend`/`SearchFilter` fields are
///   ACCEPTED-BUT-IGNORED (known gap — the reused `DetailIssuesQuery`
///   struct carries them, the WHERE builders never consult them).
/// - D12c grouped pagination (`GroupedOffsetPaginator` /
///   `SubGroupedOffsetPaginator`, `user.py:184-233`) is OUT — truthy
///   `group_by` 400s with the shared
///   `ARCHIVE_GROUPING_UNSUPPORTED_MSG` (Batch F defers grouped
///   pagination; same precedent as `archived_list`). A lone
///   `sub_group_by` is ignored (flat path), exactly like Django
///   (`if group_by:`, `user.py:175`).
/// - `label_ids`/`assignee_ids`/`module_ids` arrays carry
///   `ORDER BY bridge.created_at DESC` (Django `ArrayAgg(DISTINCT …)` /
///   prefetch order is unordered-or-`-created_at` — set-equal, order
///   deterministic; same note as `archived_list`).
/// - D12a `module_ids` INCLUDES archived modules (Django reads the
///   `issue_module` prefetch `.all()`, `view/base.py:208-213`, with no
///   archived exclusion); D12c/v2 EXCLUDE them (grouper subquery
///   `module__archived_at__isnull=True`, `grouper.py:59-68`, mirrored in
///   the v2 `list()` annotation, `base.py:950-961`).
/// - `?updated_at__gt` accepts RFC3339 (with offset/`Z`) and naive
///   `%Y-%m-%d[ T]%H:%M[:%S[.f]]` (naive assumed UTC — `TIME_ZONE = "UTC"`,
///   `settings/common.py:291`); anything else → 400
///   `{"error": "Please provide valid detail"}` (Django `DateTimeField`
///   `ValidationError` → `views/base.py:70-109`). Empty string is ignored
///   (`if updated_at:`, `base.py:925`), exactly like Django.
/// - JSON key ORDER inside row objects is alphabetical on the wire
///   (`serde_json::to_value` without `preserve_order`, repo-wide precedent
///   — see `versions.rs` key-order notes); the wire KEY SETS match Django
///   exactly and the const key lists below document the canonical Django
///   field order. Envelope key orders follow `paginator.py:728-743`
///   (D12a/D12c, struct serialization) and `global_paginator.py:75-85`
///   (D12b, reused `VersionEnvelope`) exactly.
/// - V2 ties on `updated_at` keep Postgres's unspecified order (Django
///   `.order_by("updated_at")` has no tiebreak either — mirrored
///   literally).
/// - D12a reads NO `group_by`/`sub_group_by`/`show_sub_issues` (Django
///   never reads them here), D12c reads NO `show_sub_issues` — the reused
///   `DetailIssuesQuery` fields stay unread (same accepted-and-ignored
///   precedent as I2's `group_by`).

/// Quoted from `plane/app/views/workspace/user.py:178` (D12c; byte-identical
/// in `plane/app/views/issue/archive.py:145`, reused via
/// `archive_group_by_conflict` — no fork).
pub(crate) const GROUPBY_SAME_MSG: &str = "Group by and sub group by cannot have same parameters";

/// The D12a 26-key row shape in exact `ViewIssueListSerializer` order
/// (`plane/app/serializers/view.py:24-52`). Identical key SET+ORDER to the
/// non-grouped `issue_on_results` (D12c, `grouper.py:106-141`) and to
/// `issue_common::ArchiveRow` (which is reused as the row struct).
#[allow(dead_code)]
pub(crate) const WS_ISSUE_KEYS: [&str; 26] = [
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
    "sub_issues_count",
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "attachment_count",
    "link_count",
    "is_draft",
    "archived_at",
    "state__group",
    "assignee_ids",
    "label_ids",
    "module_ids",
];

/// The D12b v2 key set in exact `required_fields` order
/// (`plane/app/views/issue/base.py:873-898`) PLUS the conditional
/// `description_html` appended last (`base.py:900-901`).
///
/// Count note (plan's "27 keys" reconciled with Django source): the base
/// list is 26 keys; `?description=true` appends `description_html` making
/// 27. The const holds the FULL emittable set (27); default rows carry the
/// first 26, `?description=true` rows carry all 27.
#[allow(dead_code)]
pub(crate) const V2_ISSUE_KEYS: [&str; 27] = [
    "id",
    "name",
    "state_id",
    "state__group",
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
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "is_draft",
    "archived_at",
    "module_ids",
    "label_ids",
    "assignee_ids",
    "link_count",
    "attachment_count",
    "sub_issues_count",
    "description_html",
];

/// Django `ValidationError` body for an unparseable `?updated_at__gt`
/// (`DateTimeField.to_python` failure → `views/base.py:70-109`,
/// `{"error": "Please provide valid detail"}`).
pub(crate) const V2_UPDATED_AT_INVALID_MSG: &str = "Please provide valid detail";

/// D12a gate — `@allow_permission([ADMIN, MEMBER, GUEST], level="WORKSPACE")`
/// (`view/base.py:222`, `permissions/base.py:44-51`): any ACTIVE ws member
/// with role 20/15/5 passes (no ws-admin fallback branch exists at
/// WORKSPACE level); non-member → 403 `deny()`. Same shape as D10
/// `guard_list_create`.
pub(crate) fn guard_ws_issues(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// D12b gate — `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
/// (`issue/base.py:863`, default `level="PROJECT"`): roles 20/15/5 pass;
/// anything else falls to the workspace-ADMIN fallback applied by the
/// caller via shared `project_gate_allows` (same shape as D2/D4/D5/D6).
pub(crate) fn guard_v2_issues(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// D12c gate — `permission_classes = [WorkspaceViewerPermission]`
/// (`user.py:100`, `permissions/workspace.py:93-100`): any ACTIVE ws
/// member passes (incl. GUEST, no role filter); non-member → 403 `deny()`.
/// Same shape as D11a `guard_ws_labels` (separate fn: different Django
/// permission use-site).
pub(crate) fn guard_user_issues(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(_) => Ok(()),
        None => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Pure encoding of the D12b guest scoping (`base.py:910-920`): scoped iff
/// the caller holds the GUEST project role AND the project hides all
/// features from guests. (A ws-admin fallback passer with a GUEST project
/// role is scoped too — Django checks the role unconditionally.)
pub(crate) fn v2_guest_scoped(role: Option<i16>, guest_view_all_features: bool) -> bool {
    role == Some(5) && !guest_view_all_features
}
/// Mirrors `is_description_required = request.GET.get("description", "false")`
/// + `str(...).lower() == "true"` (`base.py:867,900`): only (case-insensitive)
/// `"true"` appends `description_html`; `"1"`, `"false"`, `""`, absent → no.
pub(crate) fn is_description_required(raw: Option<&str>) -> bool {
    raw.unwrap_or("false").eq_ignore_ascii_case("true")
}

/// Parses a Django `DateTimeField` lookup input the way
/// `updated_at__gt=<raw>` needs it (`base.py:925-927`): `None`/empty →
/// `Ok(None)` (Django `if updated_at:` skips falsy); RFC3339 (offset/`Z`)
/// or naive `%Y-%m-%d[ T]%H:%M[:%S[.f]]` (naive assumed UTC —
/// `TIME_ZONE = "UTC"`) → `Ok(Some(_))`; anything else →
/// `Err(V2_UPDATED_AT_INVALID_MSG)` (Django `ValidationError` →
/// `views/base.py:70-109`). No trimming (Django doesn't trim either).
fn parse_django_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc));
        }
    }
    None
}

/// Mirrors the `?updated_at__gt` handling (`base.py:868,925-927`).
pub(crate) fn parse_updated_at_gt(
    raw: Option<&str>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, &'static str> {
    match raw {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => parse_django_datetime(s)
            .map(Some)
            .ok_or(V2_UPDATED_AT_INVALID_MSG),
    }
}

fn bad_request(body: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(body))
}

/// Generic-500 body, byte-exact from `BaseAPIView.handle_exception`
/// (`plane/app/views/base.py:200-209`), reusing the I2 constant (same
/// precedent as `versions.rs::cursor_500`).
fn server_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": GENERIC_500_MSG})),
    )
}

/// One v2 row: the 26 base `required_fields` (`base.py:873-898`) in Django
/// `.values()` order, plus `description_html` (always selected —
/// `issues.description_html NOT NULL` — but `skip_serializing`, re-inserted
/// into the JSON `Value` ONLY when `?description=true`, so the wire key is
/// present iff requested, exactly like `required_fields.append`,
/// `base.py:900-901`). `state_group` renders as `state__group` (serde
/// rename, same as `ArchiveRow`). sqlx maps `query_as` positionally, so
/// `V2_SELECT_SQL` lists columns in this exact order. Do NOT reuse I1/I2
/// structs: the key SET (extra `state__group`, no `deleted_at`) and ORDER
/// (group 4th, id-arrays last) differ from both.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct V2IssueRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) state_id: Option<uuid::Uuid>,
    #[serde(rename = "state__group")]
    pub(crate) state_group: Option<String>,
    pub(crate) sort_order: f64,
    pub(crate) completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) estimate_point: Option<uuid::Uuid>,
    pub(crate) priority: String,
    pub(crate) start_date: Option<chrono::NaiveDate>,
    pub(crate) target_date: Option<chrono::NaiveDate>,
    pub(crate) sequence_id: i32,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) parent_id: Option<uuid::Uuid>,
    pub(crate) cycle_id: Option<uuid::Uuid>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
    pub(crate) is_draft: bool,
    pub(crate) archived_at: Option<chrono::NaiveDate>,
    pub(crate) module_ids: Vec<uuid::Uuid>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) assignee_ids: Vec<uuid::Uuid>,
    pub(crate) link_count: i64,
    pub(crate) attachment_count: i64,
    pub(crate) sub_issues_count: i64,
    #[serde(skip_serializing)]
    pub(crate) description_html: String,
}

/// Query params for `v2_issues`: Django reads ONLY `cursor`, `description`
/// and `updated_at__gt` (`base.py:866-868`) — there is NO `per_page` (size
/// rides inside the cursor, `global_paginator`), NO `order_by` (fixed
/// `updated_at ASC`), NO `filters` (dedicated `DetailIssuesQuery` reuse
/// would wrongly accept them — hence this own struct).
#[derive(Debug, Clone, Deserialize, Default)]
#[allow(non_snake_case)]
pub struct V2IssuesQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    // Field name must stay `updated_at__gt` (Django `request.GET` key,
    // `base.py:868`) — not snake_case, hence the struct-level allow.
    #[serde(default)]
    pub updated_at__gt: Option<String>,
}

/// The page-query SELECT prefix for D12a: the 26 `ViewIssueListSerializer`
/// columns in serializer order (= `ArchiveRow` order) on top of the
/// `WorkspaceViewIssuesViewSet.apply_annotations` subqueries
/// (`view/base.py:172-213`): cycle_id (live bridge), link/attachment counts,
/// sub_issues_count over `issue_objects`-scoped children, id arrays from
/// live bridge rows. Delta vs `ARCHIVE_SELECT_SQL`: NO `issue_types` join
/// (the workspace view has no epic filter) and `module_ids` WITHOUT the
/// archived-module exclusion (Django reads the `issue_module` prefetch
/// `.all()` — archived modules INCLUDED).
pub(crate) const WS_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
     i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
     i.sequence_id, i.project_id, i.parent_id, \
     (SELECT ci.cycle_id FROM cycle_issues ci \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
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
     i.is_draft, i.archived_at, s.\"group\" AS state_group, \
     COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
     COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
     COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL), '{}'::uuid[]) AS module_ids \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";

/// The page-query SELECT prefix for D12c: the 26 non-grouped
/// `issue_on_results` columns in grouper order (= `ArchiveRow` order) on
/// top of `WorkspaceUserProfileIssuesEndpoint.apply_annotations`
/// (`user.py:104-133`) + the flat-path `issue_queryset_grouper`
/// annotations (`grouper.py:28-90`, all three id-arrays since no grouping):
/// identical to `WS_SELECT_SQL` except `module_ids` EXCLUDES archived
/// modules (`module__archived_at__isnull=True`, `grouper.py:59-68`).
pub(crate) const USER_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
     i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
     i.sequence_id, i.project_id, i.parent_id, \
     (SELECT ci.cycle_id FROM cycle_issues ci \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
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
     i.is_draft, i.archived_at, s.\"group\" AS state_group, \
     COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
     COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
     COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
       JOIN modules m ON m.id = mi.module_id \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
       AND m.archived_at IS NULL), '{}'::uuid[]) AS module_ids \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";

/// The page-query SELECT prefix for D12b: the 26 base `required_fields` in
/// Django order + `description_html` (struct position last) on top of
/// `IssuePaginatedViewSet.get_queryset` (`base.py:818-853`: `select_related`
/// state, cycle/link/attachment/sub counts) + the `list()` id-array
/// annotations (`base.py:936-963`): `label_ids` plain Coalesce,
/// `assignee_ids` restricted to assignees with an ACTIVE project membership
/// (`assignee__member_project__is_active=True`, `base.py:941-950` — any
/// project, default-manager soft-delete implied), `module_ids` excluding
/// archived modules (`module__archived_at__isnull=True`, `base.py:952-961`).
pub(crate) const V2_SELECT_SQL: &str = "SELECT i.id, i.name, i.state_id, s.\"group\" AS state_group, \
     i.sort_order, i.completed_at, \
     i.estimate_point_id AS estimate_point, i.priority, i.start_date, i.target_date, \
     i.sequence_id, i.project_id, i.parent_id, \
     (SELECT ci.cycle_id FROM cycle_issues ci \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL ORDER BY ci.created_at DESC LIMIT 1) AS cycle_id, \
     i.created_at, i.updated_at, \
     i.created_by_id AS created_by, i.updated_by_id AS updated_by, \
     i.is_draft, i.archived_at, \
     COALESCE((SELECT array_agg(mi.module_id ORDER BY mi.created_at DESC) FROM module_issues mi \
       JOIN modules m ON m.id = mi.module_id \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
       AND m.archived_at IS NULL), '{}'::uuid[]) AS module_ids, \
     COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
     COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.member_id = ia.assignee_id \
         AND pm.is_active = true AND pm.deleted_at IS NULL)), '{}'::uuid[]) AS assignee_ids, \
     (SELECT COUNT(*) FROM issue_links lin \
       WHERE lin.issue_id = i.id AND lin.deleted_at IS NULL) AS link_count, \
     (SELECT COUNT(*) FROM file_assets fa \
       WHERE fa.issue_id = i.id AND fa.entity_type = 'ISSUE_ATTACHMENT' \
       AND fa.deleted_at IS NULL) AS attachment_count, \
     (SELECT COUNT(*) FROM issues si \
       LEFT JOIN states ss ON ss.id = si.state_id \
       WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
       AND si.archived_at IS NULL AND si.is_draft = false \
       AND ss.\"group\" <> 'triage' \
       AND EXISTS(SELECT 1 FROM projects sp \
         WHERE sp.id = si.project_id AND sp.archived_at IS NULL)) AS sub_issues_count, \
     i.description_html AS description_html \
     FROM issues i LEFT JOIN states s ON s.id = i.state_id";

/// Shared `issue_objects` scope fragment for the D12a/D12c COUNT + page
/// queries: soft-delete + triage + archived-project/draft/archived excluded
/// (`IssueManager.get_queryset`, `db/models/issue.py:92-101`), slug-scoped.
/// Same reading as I2 `push_detail_where` (incl. the project
/// archived+deleted predicate and the join-form triage exclusion).
fn push_issue_objects_scope(qb: &mut QueryBuilder<Postgres>, slug: &str) {
    qb.push(" WHERE i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
        .push_bind(slug.to_string())
        .push(
            ") AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
             AND s.\"group\" <> 'triage' \
             AND EXISTS(SELECT 1 FROM projects p \
               WHERE p.id = i.project_id AND p.archived_at IS NULL AND p.deleted_at IS NULL)",
        );
}

/// D12a permission predicate, mirroring
/// `_get_project_permission_filters`
/// (`plane/app/views/view/base.py:155-171`): per-issue-row EXISTS over the
/// caller's ACTIVE project membership — role > 5 sees all, GUEST (5) on a
/// `guest_view_all_features` project sees all, otherwise GUEST sees only
/// own (`created_by=user`) rows.
fn push_ws_permission(qb: &mut QueryBuilder<Postgres>, user_id: uuid::Uuid) {
    qb.push(
        " AND EXISTS(SELECT 1 FROM project_members pm \
           JOIN projects gp ON gp.id = pm.project_id \
           WHERE pm.project_id = i.project_id AND pm.member_id = ",
    )
    .push_bind(user_id)
    .push(
        " AND pm.is_active = true AND pm.deleted_at IS NULL \
         AND (pm.role > 5 \
           OR (pm.role = 5 AND gp.guest_view_all_features = true) \
           OR (pm.role = 5 AND gp.guest_view_all_features = false AND i.created_by_id = ",
    )
    .push_bind(user_id)
    .push(")))");
}

/// Shared WHERE for D12a COUNT + page: scope + permission + `filters` tree.
/// Returns the complex-filter error for the caller's 400 mapping.
fn push_ws_where(
    qb: &mut QueryBuilder<Postgres>,
    slug: &str,
    user_id: uuid::Uuid,
    tree: Option<&Value>,
) -> Result<(), super::issue_query::ComplexFilterError> {
    push_issue_objects_scope(qb, slug);
    push_ws_permission(qb, user_id);
    if let Some(tree) = tree {
        apply_complex_filter(qb, tree)?;
    }
    Ok(())
}

/// Shared WHERE for D12c COUNT + page: scope + `:uid` involvement
/// (assignee ∨ creator ∨ live subscriber, `user.py:139-147`) + requester
/// must hold an ACTIVE membership on the row's project (no role filter,
/// `project__project_projectmember__member=request.user,
/// ...__is_active=True`, `user.py:143-146`) + `filters` tree.
fn push_user_where(
    qb: &mut QueryBuilder<Postgres>,
    slug: &str,
    uid: uuid::Uuid,
    requester: uuid::Uuid,
    tree: Option<&Value>,
) -> Result<(), super::issue_query::ComplexFilterError> {
    push_issue_objects_scope(qb, slug);
    qb.push(
        " AND (EXISTS(SELECT 1 FROM issue_assignees ia \
           WHERE ia.issue_id = i.id AND ia.assignee_id = ",
    )
    .push_bind(uid)
    .push(
        " AND ia.deleted_at IS NULL) \
         OR i.created_by_id = ",
    )
    .push_bind(uid)
    .push(
        " OR EXISTS(SELECT 1 FROM issue_subscribers sb \
           WHERE sb.issue_id = i.id AND sb.subscriber_id = ",
    )
    .push_bind(uid)
    .push(
        " AND sb.deleted_at IS NULL)) \
         AND EXISTS(SELECT 1 FROM project_members pm \
           WHERE pm.project_id = i.project_id AND pm.member_id = ",
    )
    .push_bind(requester)
    .push(" AND pm.is_active = true AND pm.deleted_at IS NULL)");
    if let Some(tree) = tree {
        apply_complex_filter(qb, tree)?;
    }
    Ok(())
}

/// Shared WHERE for D12b COUNT + page: `issue_objects` scope + slug/project
/// + guest scoping + `updated_at__gt` (`base.py:903-927`).
fn push_v2_where(
    qb: &mut QueryBuilder<Postgres>,
    slug: &str,
    project_id: uuid::Uuid,
    user_id: uuid::Uuid,
    guest_scoped: bool,
    gt: Option<chrono::DateTime<chrono::Utc>>,
) {
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
    if let Some(gt) = gt {
        qb.push(" AND i.updated_at > ").push_bind(gt);
    }
}

/// Project row for the D12b pre-check: Django runs
/// `Project.objects.get(pk=project_id, workspace__slug=slug)` BEFORE the
/// guest gate (`base.py:909`) — miss → 404 `missing()` — and reads
/// `project.guest_view_all_features` for the gate itself (`base.py:910-920`,
/// off the already-fetched row — mirrored literally, no second query).
/// (WITH slug scoping here, unlike the D6 twins' `get(pk)`.)
#[derive(Debug, Clone, sqlx::FromRow)]
struct V2GateProject {
    #[allow(dead_code)]
    id: uuid::Uuid,
    guest_view_all_features: bool,
}

/// GET `/api/workspaces/:slug/issues/` — parity with Django
/// `WorkspaceViewIssuesViewSet.list` (`view/base.py:222-259`,
/// `urls/views.py:51-55`).
///
/// - Gate WORKSPACE ADMIN/MEMBER/GUEST (any active ws member 20/15/5).
/// - `?order_by=` (default `-created_at`, reused `order_issue_queryset`
///   mapping + paginator `(key DIR NULLS LAST, -created_at)` tiebreak),
///   `?per_page=`/`?cursor=` (reused `BasePaginator` mapping, same 400s),
///   `?filters=` tree (reused ComplexFilterBackend mapping).
/// - 200 `DetailEnvelope` (reused, flat: grouped/sub-grouped null,
///   extra_stats null) of 26-key rows (reused `ArchiveRow` — same key
///   SET+ORDER as `ViewIssueListSerializer`).
/// - `group_by`/`sub_group_by`/`show_sub_issues`/`expand`/`fields`/legacy
///   keys accepted-and-ignored (Django never reads them on this view).
pub async fn workspace_issues(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<DetailIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_ws_issues(role).is_err() {
        return Ok(deny());
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok(bad_request(json!({"detail": msg}))),
    };
    let cursor_raw = q.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(msg) => return Ok(bad_request(json!({"detail": msg}))),
    };
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok(bad_request(json!({"detail": "Error in parsing"}))),
        Ok(w) => w,
    };
    if limit <= 0 {
        return Ok(server_error());
    }
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let (order_expr, desc) = detail_order_expr(&sanitized);
    let order_dir = if desc { "DESC" } else { "ASC" };
    let tree = match parse_complex_filter(q.filters.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(bad_request(json!({"message": e.message, "code": e.code})));
        }
    };

    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id");
    if let Err(e) = push_ws_where(&mut count_qb, &slug, auth.0, tree.as_ref()) {
        return Ok(bad_request(json!({"message": e.message, "code": e.code})));
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(WS_SELECT_SQL);
    if let Err(e) = push_ws_where(&mut page_qb, &slug, auth.0, tree.as_ref()) {
        return Ok(bad_request(json!({"message": e.message, "code": e.code})));
    }
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

    let results: Vec<Value> = rows
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!(e))?;
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

/// GET `/api/workspaces/:slug/user-issues/:user_id/` — parity with Django
/// `WorkspaceUserProfileIssuesEndpoint.get`
/// (`workspace/user.py:98-203`, `urls/workspace.py:152-156`).
///
/// - Gate `WorkspaceViewerPermission` (any ACTIVE ws member).
/// - Scope: `:uid` assignee ∨ creator ∨ live subscriber; requester ACTIVE
///   project member per row (no role filter). No 404 for unknown `:uid`
///   (Django filters by pk value — unknown yields `[]`).
/// - `group_by`+`sub_group_by` both truthy and equal → 400
///   `{"error": GROUPBY_SAME_MSG}` (`user.py:176-181`, via the shared
///   `archive_group_by_conflict` — byte-identical message); then the shared
///   allowlist 400 (`paginator.py:690-699`); then truthy `group_by` →
///   400 grouped-unsupported (Batch F gap, `archived_list` precedent).
/// - Otherwise the flat path: same order/pagination/filter/envelope
///   handling as D12a, rows in non-grouped `issue_on_results` shape
///   (reused `ArchiveRow`; `module_ids` excludes archived modules per the
///   grouper).
pub async fn user_issues(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, uid)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<DetailIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_user_issues(role).is_err() {
        return Ok(deny());
    }
    // View-level conflict check precedes `paginate()` (`user.py:175-181`).
    if archive_group_by_conflict(q.group_by.as_deref(), q.sub_group_by.as_deref()).is_some() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": GROUPBY_SAME_MSG}))));
    }
    let per_page = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok(bad_request(json!({"detail": msg}))),
    };
    let cursor_raw = q.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(msg) => return Ok(bad_request(json!({"detail": msg}))),
    };
    // Allowlist gate inside `paginate()` (`paginator.py:690-699`) precedes
    // the window check; grouped shapes are OUT (Batch F).
    if let Some(msg) = archive_group_by_allowlist_error(q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        return Ok(bad_request(json!({"detail": msg})));
    }
    if archive_grouping_unsupported(q.group_by.as_deref(), q.sub_group_by.as_deref()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ARCHIVE_GROUPING_UNSUPPORTED_MSG})),
        ));
    }
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok(bad_request(json!({"detail": "Error in parsing"}))),
        Ok(w) => w,
    };
    if limit <= 0 {
        return Ok(server_error());
    }
    let sanitized = sanitize_order_by(q.order_by.as_deref().unwrap_or("-created_at"));
    let (order_expr, desc) = detail_order_expr(&sanitized);
    let order_dir = if desc { "DESC" } else { "ASC" };
    let tree = match parse_complex_filter(q.filters.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(bad_request(json!({"message": e.message, "code": e.code})));
        }
    };

    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id");
    if let Err(e) = push_user_where(&mut count_qb, &slug, uid, auth.0, tree.as_ref()) {
        return Ok(bad_request(json!({"message": e.message, "code": e.code})));
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(USER_SELECT_SQL);
    if let Err(e) = push_user_where(&mut page_qb, &slug, uid, auth.0, tree.as_ref()) {
        return Ok(bad_request(json!({"message": e.message, "code": e.code})));
    }
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

    let results: Vec<Value> = rows
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!(e))?;
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

/// GET `/api/workspaces/:slug/projects/:project_id/v2/issues/` — parity with
/// Django `IssuePaginatedViewSet.list` (`issue/base.py:863-972`,
/// `urls/issue.py:54-58`).
///
/// - Gate PROJECT ADMIN/MEMBER/GUEST (+ ws-admin fallback).
/// - Project pre-check WITH slug scoping (`get(pk, workspace__slug)`,
///   `base.py:909`) — miss → 404 `missing()` — before cursor parsing.
/// - Guest scoping (role==5 AND NOT `guest_view_all_features` →
///   `created_by=user`, reused `fetch_guest_scoped`, `base.py:910-920`).
/// - `?updated_at__gt` filter (`base.py:925-927`; invalid → 400, empty →
///   ignored); fixed `ORDER BY updated_at ASC` (`base.py:906-907`); NO
///   `per_page`/`order_by`/`filters` params (dedicated `V2IssuesQuery`).
/// - 200 `VersionEnvelope` (reused — same `paginate()` family) of 26-key
///   rows, 27 with `?description=true` (`V2IssueRow` + conditional insert).
/// - Cursor failures / `size <= 0` → 500 `GENERIC_500_MSG` (Django
///   `ValueError`/`ZeroDivisionError` → `views/base.py:200-209`, same as
///   D6); `page <= 0` clamps to the first page (`paginate` only offsets
///   when `current_page > 0`, `global_paginator.py:48-50`).
pub async fn v2_issues(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<V2IssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        guard_v2_issues(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let project: Option<V2GateProject> = sqlx::query_as(
        "SELECT id, guest_view_all_features FROM projects \
         WHERE id = $1 AND workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(project) = project else {
        return Ok(missing());
    };
    let Ok((size, page)) = parse_version_cursor(q.cursor.as_deref()) else {
        return Ok(server_error());
    };
    let gt = match parse_updated_at_gt(q.updated_at__gt.as_deref()) {
        Ok(v) => v,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": msg})))),
    };
    let with_description = is_description_required(q.description.as_deref());
    // Guest scoping off the already-fetched row (`base.py:910-920`): the
    // caller's GUEST project membership (`member_role`, same row Django's
    // decorator + `project_member.exists()` read: active, non-deleted)
    // AND NOT `guest_view_all_features` → `created_by=user` on both
    // querysets. (A ws-admin fallback passer with a GUEST project role is
    // scoped too — Django checks the role unconditionally.)
    let guest_scoped = v2_guest_scoped(member_role, project.guest_view_all_features);

    // `total_results = base_queryset.count()` (`global_paginator.py:41`) —
    // the count evaluates the `updated_at__gt` filter, so it precedes the
    // `size <= 0` 500 (Django `ceil(total / 0)` → `ZeroDivisionError`).
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id");
    push_v2_where(&mut count_qb, &slug, project_id, auth.0, guest_scoped, gt);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;
    if size <= 0 {
        return Ok(server_error());
    }
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
    let rows: Vec<V2IssueRow> = match start {
        Some(offset) => {
            let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(V2_SELECT_SQL);
            push_v2_where(&mut page_qb, &slug, project_id, auth.0, guest_scoped, gt);
            // Fixed ORDER BY updated_at ASC (`base.py:906-907`) — no
            // tiebreak in Django (mirrored literally).
            page_qb.push(" ORDER BY i.updated_at ASC LIMIT ").push_bind(size);
            page_qb.push(" OFFSET ").push_bind(offset);
            page_qb.build_query_as().fetch_all(&st.pool).await?
        }
        None => Vec::new(),
    };
    let offset = start.unwrap_or(i64::MAX);
    let end = (i128::from(offset)).saturating_add(i128::from(size));
    let has_next = end < i128::from(total);
    let mut results = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut v = serde_json::to_value(row).map_err(|e| anyhow::anyhow!(e))?;
        if with_description {
            v["description_html"] = json!(row.description_html);
        }
        results.push(v);
    }
    let envelope = VersionEnvelope {
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

#[cfg(test)]
mod batch_d_d12_tests {
    use super::*;

    #[test]
    fn groupby_same_msg_verbatim() {
        // Quoted from `plane/app/views/workspace/user.py:178` (D12c);
        // byte-identical to `plane/app/views/issue/archive.py:145` — the
        // check reuses `archive_group_by_conflict` (no fork), so both
        // consts must stay equal.
        assert_eq!(
            GROUPBY_SAME_MSG,
            "Group by and sub group by cannot have same parameters"
        );
        assert_eq!(
            super::super::issue_query::ARCHIVE_GROUP_BY_CONFLICT_MSG,
            GROUPBY_SAME_MSG
        );
        // Truthy equal pair conflicts; anything else passes through
        // (`user.py:175-181`: `if group_by:` → `if sub_group_by:` →
        // `if group_by == sub_group_by:`).
        assert_eq!(
            archive_group_by_conflict(Some("priority"), Some("priority")),
            Some(GROUPBY_SAME_MSG)
        );
        assert_eq!(archive_group_by_conflict(Some("priority"), Some("state_id")), None);
        assert_eq!(archive_group_by_conflict(Some("priority"), None), None);
        assert_eq!(archive_group_by_conflict(None, Some("priority")), None);
        assert_eq!(archive_group_by_conflict(Some(""), Some("")), None);
    }

    #[test]
    fn v2_keys_full_set_is_27_in_django_order() {
        // `plane/app/views/issue/base.py:873-901`: 26 base
        // `required_fields` + `description_html` appended iff
        // `?description=true` → 27 emittable keys total (plan's "27 keys").
        assert_eq!(V2_ISSUE_KEYS.len(), 27);
        assert_eq!(
            &V2_ISSUE_KEYS[..26],
            [
                "id",
                "name",
                "state_id",
                "state__group",
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
                "created_at",
                "updated_at",
                "created_by",
                "updated_by",
                "is_draft",
                "archived_at",
                "module_ids",
                "label_ids",
                "assignee_ids",
                "link_count",
                "attachment_count",
                "sub_issues_count",
            ]
        );
        assert_eq!(V2_ISSUE_KEYS[3], "state__group");
        assert_eq!(V2_ISSUE_KEYS[26], "description_html");
        // Trailing id-array/count cluster (differs from the D12a order —
        // hence the dedicated struct, "do NOT reuse I1/I2 structs").
        assert_eq!(&V2_ISSUE_KEYS[20..26], [
            "module_ids",
            "label_ids",
            "assignee_ids",
            "link_count",
            "attachment_count",
            "sub_issues_count",
        ]);
    }

    fn sample_v2_row() -> V2IssueRow {
        V2IssueRow {
            id: uuid::Uuid::nil(),
            name: "Bug".to_string(),
            state_id: None,
            state_group: Some("started".to_string()),
            sort_order: 65535.0,
            completed_at: None,
            estimate_point: None,
            priority: "high".to_string(),
            start_date: None,
            target_date: None,
            sequence_id: 7,
            project_id: uuid::Uuid::nil(),
            parent_id: None,
            cycle_id: None,
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by: Some(uuid::Uuid::nil()),
            updated_by: None,
            is_draft: false,
            archived_at: None,
            module_ids: vec![],
            label_ids: vec![],
            assignee_ids: vec![uuid::Uuid::nil()],
            link_count: 1,
            attachment_count: 0,
            sub_issues_count: 2,
            description_html: "<p>hi</p>".to_string(),
        }
    }

    #[test]
    fn v2_row_serializes_26_by_default_27_with_description() {
        // Default rows carry the 26 base keys (`description_html` skipped);
        // `?description=true` inserts it → 27.
        let v = serde_json::to_value(&sample_v2_row()).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = V2_ISSUE_KEYS[..26].to_vec();
        want.sort_unstable();
        assert_eq!(keys, want);
        assert!(v.get("description_html").is_none());
        // The `state__group` rename renders (serde, same as `ArchiveRow`).
        assert_eq!(v.get("state__group"), Some(&json!("started")));
        assert!(v.get("state_group").is_none());
        // With-description branch (handler inserts the skipped field).
        let mut with = v.clone();
        with["description_html"] = json!("<p>hi</p>");
        let mut keys27: Vec<&str> =
            with.as_object().unwrap().keys().map(String::as_str).collect();
        keys27.sort_unstable();
        let mut want27 = V2_ISSUE_KEYS.to_vec();
        want27.sort_unstable();
        assert_eq!(keys27, want27);
    }

    #[test]
    fn ws_keys_are_26_view_serializer_shape() {
        // `plane/app/serializers/view.py:24-52` (D12a) — same key SET+ORDER
        // as non-grouped `issue_on_results` (D12c, `grouper.py:106-141`),
        // hence the shared `ArchiveRow` struct.
        assert_eq!(WS_ISSUE_KEYS.len(), 26);
        assert_eq!(WS_ISSUE_KEYS[3], "sort_order");
        assert_eq!(WS_ISSUE_KEYS[22], "state__group");
        assert_eq!(&WS_ISSUE_KEYS[23..26], ["assignee_ids", "label_ids", "module_ids"]);
        let row = ArchiveRow {
            id: uuid::Uuid::nil(),
            name: "Bug".to_string(),
            state_id: None,
            sort_order: 65535.0,
            completed_at: None,
            estimate_point: None,
            priority: "none".to_string(),
            start_date: None,
            target_date: None,
            sequence_id: 1,
            project_id: uuid::Uuid::nil(),
            parent_id: None,
            cycle_id: None,
            sub_issues_count: 0,
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by: None,
            updated_by: None,
            attachment_count: 0,
            link_count: 0,
            is_draft: false,
            archived_at: None,
            state_group: None,
            assignee_ids: vec![],
            label_ids: vec![],
            module_ids: vec![],
        };
        let v = serde_json::to_value(&row).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = WS_ISSUE_KEYS.to_vec();
        want.sort_unstable();
        assert_eq!(keys, want);
    }

    #[test]
    fn guards_match_django_gates() {
        // D12a WORKSPACE ADMIN/MEMBER/GUEST (`view/base.py:222`,
        // `permissions/base.py:44-51` — no ws-admin fallback at WORKSPACE
        // level).
        assert!(guard_ws_issues(Some(20)).is_ok());
        assert!(guard_ws_issues(Some(15)).is_ok());
        assert!(guard_ws_issues(Some(5)).is_ok());
        assert!(guard_ws_issues(Some(10)).is_err());
        assert!(guard_ws_issues(None).is_err());
        // D12b PROJECT ADMIN/MEMBER/GUEST (`issue/base.py:863`, default
        // `level="PROJECT"` — fallback via `project_gate_allows`).
        assert!(guard_v2_issues(Some(20)).is_ok());
        assert!(guard_v2_issues(Some(15)).is_ok());
        assert!(guard_v2_issues(Some(5)).is_ok());
        assert!(guard_v2_issues(Some(10)).is_err());
        assert!(guard_v2_issues(None).is_err());
        // D12c `WorkspaceViewerPermission` (`permissions/workspace.py:93-100`):
        // any ACTIVE ws member passes, incl. GUEST.
        assert!(guard_user_issues(Some(20)).is_ok());
        assert!(guard_user_issues(Some(15)).is_ok());
        assert!(guard_user_issues(Some(5)).is_ok());
        assert_eq!(
            guard_user_issues(None).unwrap_err(),
            crate::routes::project::FORBIDDEN_MSG
        );
    }

    #[test]
    fn v2_guest_scoping_matches_django() {
        // `base.py:910-920`: scoped iff (GUEST role AND NOT
        // `guest_view_all_features`) — the role check is unconditional, so
        // a ws-admin fallback passer with a GUEST project role is scoped
        // too.
        assert!(v2_guest_scoped(Some(5), false));
        assert!(!v2_guest_scoped(Some(5), true));
        assert!(!v2_guest_scoped(Some(15), false));
        assert!(!v2_guest_scoped(Some(20), false));
        assert!(!v2_guest_scoped(None, false));
    }

    #[test]
    fn description_flag_is_case_insensitive_true_only() {        // `str(request.GET.get("description", "false")).lower() == "true"`
        // (`base.py:867,900`).
        assert!(!is_description_required(None));
        assert!(is_description_required(Some("true")));
        assert!(is_description_required(Some("True")));
        assert!(is_description_required(Some("TRUE")));
        assert!(!is_description_required(Some("1")));
        assert!(!is_description_required(Some("false")));
        assert!(!is_description_required(Some("")));
    }

    #[test]
    fn updated_at_gt_parsing_matches_django() {
        // Absent/empty → ignored (`if updated_at:`, `base.py:925`).
        assert_eq!(parse_updated_at_gt(None).unwrap(), None);
        assert_eq!(parse_updated_at_gt(Some("")).unwrap(), None);
        // RFC3339 `Z` + naive variants (naive assumed UTC, `TIME_ZONE`).
        assert_eq!(
            parse_updated_at_gt(Some("2026-09-06T12:00:00Z")).unwrap().unwrap(),
            "2026-09-06T12:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
        );
        assert_eq!(
            parse_updated_at_gt(Some("2026-09-06 12:00:00")).unwrap().unwrap(),
            "2026-09-06T12:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
        );
        // Garbage → 400 `{"error": "Please provide valid detail"}`
        // (Django `ValidationError` → `views/base.py:70-109`).
        assert_eq!(
            parse_updated_at_gt(Some("junk")).unwrap_err(),
            V2_UPDATED_AT_INVALID_MSG
        );
        assert_eq!(V2_UPDATED_AT_INVALID_MSG, "Please provide valid detail");
    }
}
