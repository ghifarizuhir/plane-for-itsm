use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use serde_json::{json, Value};

use crate::routes::project::{FORBIDDEN_MSG, deny, missing};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Single-issue archive — parity with Django `IssueArchiveViewSet`
/// (`plane/app/views/issue/archive.py:53,221-302`,
/// `plane/app/urls/issue.py:228-232`):
/// `GET+POST+DELETE .../issues/:pk/archive/` (issues path ONLY — Django
/// defines no work-items/epics variant). Celery `issue_activity.delay`
/// writes skipped (batch-wide precedent — Rust never writes activities).
///
/// Locked semantics (plan D7):
/// - GET 200 `IssueDetailSerializer` with scope ARCHIVED ONLY:
///   `get_queryset` filters `archived_at NOT NULL` + non-epic
///   (`archive.py:97-102`) — GET non-archived → 404. Miss → 404
///   `{"error":"The required object does not exist."}` verbatim
///   (`archive.py:240-243`, same body as `missing()`).
/// - POST 200 `{"archived_at":"YYYY-MM-DD"}` (date-only `str(date)`,
///   `archive.py:259-268`); state-group check `group ∉ {completed,cancelled}`
///   → 400 `{"error":"Can only archive completed or cancelled state group
///   issue"}` (key `error`, NOT `error_code` — differs from bulk I3b
///   `archive.py:319-327`, `archive.py:259-263`).
/// - DELETE 204 (empty, `archive.py:271-302`).
/// - Gate all three: `@allow_permission([ADMIN, MEMBER])`
///   (`archive.py:221,256,272`, default `level="PROJECT"`): allowed-role
///   branch needs 20/15, plus the standard workspace-ADMIN fallback (any
///   active membership + ws ADMIN, `permissions/base.py:53-78`) via the
///   shared `project_gate_allows` (same shape as D5 `issue_dates`).
///
/// Shape note (plan "reuse I2 shape if identical — verified, NOT identical,
/// so new struct"): I2 `IssueDetailRow` (`issue_common.rs`, 25 keys,
/// `IssueListDetailSerializer`) lacks `description_html`, `is_subscribed`,
/// `is_intake`. This endpoint returns `IssueDetailSerializer`
/// (`serializers/issue.py:934-945` = `IssueSerializer` 25 fields
/// `issue.py:786-812` + `description_html,is_subscribed,is_intake`), so a
/// dedicated 28-key `ArchivedIssueDetailRow` is defined below in the exact
/// Django `Meta.fields + [...]` order. `is_subscribed` is the
/// `Exists(IssueSubscriber...)` annotation (`archive.py:237-244`);
/// `is_intake` is `Exists(IntakeIssue status∈{-2,0})`
/// (`views/issue/base.py:1318-1325`, same annotation the sibling
/// detail endpoints use — the archive `retrieve` queryset omits it, but
/// the serializer field exists, so it is computed here rather than
/// skipped).
///
/// Deviations (documented, reviewer-adjudicable):
/// - Datetimes serialize RFC3339 UTC (chrono, batch convention) vs DRF's
///   per-user-timezone rendering; key ORDER on the wire follows struct
///   declaration (Django field order) since the handler returns
///   `Json(row)`.
/// - Counts (`sub_issues_count`, `attachment_count`, `link_count`) use
///   `COUNT(*)` (0 when empty, never null) like the `archived_list`
///   `ARCHIVE_SELECT_SQL` precedent; Django `Subquery(Count)` renders the
///   same 0/positive values.
/// - `assignee_ids`/`label_ids`/`module_ids` aggregate live bridge rows
///   (module join excludes `archived_at`, same as `ARCHIVE_SELECT_SQL`);
///   the `retrieve` `assignee__member_project__is_active` / distinct
///   refinements are not reproduced (unreachable in smoke; same narrowing
///   as the sibling list endpoints).
/// - `updated_at` is bumped alongside `archived_at` (Django `save()`
///   `auto_now`; bulk-archive I3b intentionally does NOT bump since it
///   uses `bulk_update(["archived_at"])`).
/// - POST scope is `Issue.issue_objects` (live + `exclude(triage)` +
///   `exclude(archived)` + `exclude(project archived)` + `exclude(draft)`,
///   `db/models/issue.py:92-101`): triage check is `s."group" <> 'triage'`
///   in WHERE position, dropping NULL-state rows exactly like Django's
///   `exclude` (I3 `bulk_delete_scope_sql` precedent) — so a NULL-state
///   POST misses → 404, never the `None.group` AttributeError 500 Django
///   would raise outside the manager scope.

/// Quoted from `plane/app/views/issue/archive.py:261`.
pub(crate) const ARCHIVE_ONE_GROUP_MSG: &str =
    "Can only archive completed or cancelled state group issue";

/// The 28-key `IssueDetailSerializer` shape
/// (`serializers/issue.py:786-812` + `934-945`): `IssueSerializer`
/// `Meta.fields` order, then `description_html,is_subscribed,is_intake`.
#[allow(dead_code)]
pub(crate) const ARCHIVED_DETAIL_KEYS: [&str; 28] = [
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
    "description_html",
    "is_subscribed",
    "is_intake",
];

/// Mirrors the POST state-group check (`archive.py:259-263`): only
/// `completed`/`cancelled` groups may archive — anything else (incl. empty)
/// → `Err(ARCHIVE_ONE_GROUP_MSG)`. Key is `error`, NOT `error_code`
/// (differs from bulk I3b `archive.py:319-327`).
pub(crate) fn guard_archive_one_group(group: &str) -> Result<(), String> {
    if group == "completed" || group == "cancelled" {
        Ok(())
    } else {
        Err(ARCHIVE_ONE_GROUP_MSG.to_string())
    }
}

/// PROJECT-level role check shared by all three handlers: mirrors
/// `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` (`archive.py:221,256,272`,
/// default `level="PROJECT"`, `permissions/base.py:17`): roles 20/15 pass;
/// anything else (incl. GUEST 5 and non-member) falls to the
/// workspace-ADMIN fallback applied by the caller via the shared
/// `project_gate_allows` (same shape as D5 `guard_issue_dates`).
pub(crate) fn guard_archive_one(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Shared PROJECT gate returning `(allowed, member_role)`.
async fn archive_one_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<(bool, Option<i16>), sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    let allowed = project_gate_allows(
        guard_archive_one(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    );
    Ok((allowed, member_role))
}

/// One archived-issue detail row: the 28-key `IssueDetailSerializer` shape
/// in Django field order (see `ARCHIVED_DETAIL_KEYS`). Field names match
/// the SELECT aliases in `retrieve`; `estimate_point` reads
/// `estimate_point_id`, `created_by`/`updated_by` the `*_id` columns.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub(crate) struct ArchivedIssueDetailRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) state_id: Option<uuid::Uuid>,
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
    pub(crate) module_ids: Vec<uuid::Uuid>,
    pub(crate) label_ids: Vec<uuid::Uuid>,
    pub(crate) assignee_ids: Vec<uuid::Uuid>,
    pub(crate) sub_issues_count: i64,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by: Option<uuid::Uuid>,
    pub(crate) updated_by: Option<uuid::Uuid>,
    pub(crate) attachment_count: i64,
    pub(crate) link_count: i64,
    pub(crate) is_draft: bool,
    pub(crate) archived_at: Option<chrono::NaiveDate>,
    pub(crate) description_html: String,
    pub(crate) is_subscribed: bool,
    pub(crate) is_intake: bool,
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:pk/archive/` —
/// parity with Django `IssueArchiveViewSet.retrieve`
/// (`plane/app/views/issue/archive.py:221-254`,
/// `plane/app/urls/issue.py:228-232`).
///
/// - Gate: ADMIN/MEMBER (+ ws-admin fallback) via `archive_one_gate`;
///   otherwise 403 `deny()`.
/// - Scope = ARCHIVED ONLY: plain `Issue.objects` (live only) +
///   `Q(type__isnull) | Q(type__is_epic=False)` + `archived_at IS NOT NULL`
///   + slug/project/pk (`archive.py:97-102` `get_queryset` + `.filter(pk)`,
///   `archive.py:224-236`). Non-archived/epic/missing → 404 `missing()`
///   (verbatim `{"error":"The required object does not exist."}`,
///   `archive.py:240-243`).
/// - 200 `IssueDetailSerializer` (28 keys, `ARCHIVED_DETAIL_KEYS`).
pub async fn retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, _) = archive_one_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    let user_id = auth.0;
    let row: Option<ArchivedIssueDetailRow> = sqlx::query_as(
        "SELECT i.id, i.name, i.state_id, i.sort_order, i.completed_at, \
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
            WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{}'::uuid[]) AS assignee_ids, \
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
          i.is_draft, i.archived_at, i.description_html, \
          EXISTS(SELECT 1 FROM issue_subscribers s \
            WHERE s.issue_id = i.id AND s.subscriber_id = $4 AND s.project_id = i.project_id \
            AND s.deleted_at IS NULL) AS is_subscribed, \
          EXISTS(SELECT 1 FROM intake_issues ii \
            WHERE ii.issue_id = i.id AND ii.status IN (-2, 0) AND ii.project_id = i.project_id \
            AND ii.deleted_at IS NULL) AS is_intake \
          FROM issues i LEFT JOIN issue_types t ON t.id = i.type_id \
          WHERE i.id = $1 AND i.project_id = $2 \
          AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
          AND i.deleted_at IS NULL AND i.archived_at IS NOT NULL \
          AND (i.type_id IS NULL OR t.is_epic = false)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .bind(user_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(serde_json::to_value(&r).unwrap()))),
        None => Ok(missing()),
    }
}

/// POST `/api/workspaces/:slug/projects/:project_id/issues/:pk/archive/` —
/// parity with Django `IssueArchiveViewSet.archive`
/// (`plane/app/views/issue/archive.py:256-268`).
///
/// - Gate: ADMIN/MEMBER (+ ws-admin fallback); otherwise 403 `deny()`.
/// - Lookup via `Issue.issue_objects` (live + not-triage + not-archived +
///   project-not-archived + not-draft, `db/models/issue.py:92-101`) scoped
///   ws/project/pk; miss → 404 `missing()` (Django `.get()` →
///   `ObjectDoesNotExist` → 404 via `views/base.py:92-96`).
/// - State-group check → 400 `{"error": ...}` (`archive.py:259-263`, key
///   `error` — differs from bulk I3b `error_code`/`error_message`).
/// - Else `archived_at=today` (+`updated_at=now()` via `save()`) → 200
///   `{"archived_at":"YYYY-MM-DD"}` (`archive.py:267-268`).
pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, _) = archive_one_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    // `Issue.issue_objects.get(workspace__slug, project_id, pk)`
    // (`archive.py:258`) with `select_related("state")` for the group.
    let group: Option<Option<String>> = sqlx::query_scalar(
        "SELECT s.\"group\" FROM issues i LEFT JOIN states s ON s.id = i.state_id \
          WHERE i.id = $1 AND i.project_id = $2 \
          AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
          AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
          AND s.\"group\" <> 'triage' \
          AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id \
            AND p.archived_at IS NULL AND p.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(group) = group else {
        return Ok(missing());
    };
    // The triage predicate above drops NULL-state rows like Django's
    // `exclude` (I3 precedent), so `group` is always `Some` here; a
    // missing group is treated as invalid (same 400, never a 500).
    match group.as_deref() {
        Some(g) => {
            if let Err(e) = guard_archive_one_group(g) {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
            }
        }
        None => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": ARCHIVE_ONE_GROUP_MSG})),
            ));
        }
    }
    let today = chrono::Utc::now().date_naive();
    sqlx::query(
        "UPDATE issues SET archived_at = $4, updated_at = now() \
          WHERE id = $1 AND project_id = $2 \
          AND workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
          AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .bind(today)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"archived_at": today.to_string()}))))
}

/// DELETE `/api/workspaces/:slug/projects/:project_id/issues/:pk/archive/`
/// — parity with Django `IssueArchiveViewSet.unarchive`
/// (`plane/app/views/issue/archive.py:271-302`).
///
/// - Gate: ADMIN/MEMBER (+ ws-admin fallback); otherwise 403 `deny()`.
/// - Scope: plain `Issue.objects` (live only) + `archived_at NOT NULL` +
///   ws/project/pk (`archive.py:273-278`); miss → 404 `missing()`.
/// - Else `archived_at=None` (+`updated_at=now()`) → 204 empty.
pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let (allowed, _) = archive_one_gate(&st.pool, auth.0, &slug, project_id).await?;
    if !allowed {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE issues SET archived_at = NULL, updated_at = now() \
          WHERE id = $1 AND project_id = $2 \
          AND workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
          AND deleted_at IS NULL AND archived_at IS NOT NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

#[cfg(test)]
mod archive_one_tests {
    use super::*;
    use crate::routes::issue_common::project_gate_allows;

    #[test]
    fn group_gate_vectors_match_django() {
        // Mirrors `IssueArchiveViewSet.archive` (`archive.py:259-263`):
        // only `completed`/`cancelled` state groups may archive; anything
        // else → 400 `{"error": "Can only archive completed or cancelled
        // state group issue"}` (key `error`, NOT `error_code` — differs
        // from bulk I3b `archive.py:319-327`).
        assert!(guard_archive_one_group("completed").is_ok());
        assert!(guard_archive_one_group("cancelled").is_ok());
        for bad in ["backlog", "unstarted", "started", "triage", ""] {
            assert_eq!(
                guard_archive_one_group(bad).unwrap_err(),
                "Can only archive completed or cancelled state group issue",
                "group {bad:?}"
            );
        }
    }

    #[test]
    fn archive_one_gate_is_admin_member_only() {
        // Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
        // (`archive.py:221,256,272`, default `level="PROJECT"`): 20/15 pass
        // outright; GUEST (5) / unknown / non-member fall to the
        // workspace-ADMIN fallback in the caller via
        // `project_gate_allows` (same shape as D5 `guard_issue_dates`).
        assert!(guard_archive_one(Some(20)).is_ok());
        assert!(guard_archive_one(Some(15)).is_ok());
        assert_eq!(
            guard_archive_one(Some(5)).unwrap_err(),
            "You don't have the required permissions."
        );
        assert!(guard_archive_one(Some(10)).is_err());
        assert_eq!(
            guard_archive_one(None).unwrap_err(),
            "You don't have the required permissions."
        );
        let allows = |role: Option<i16>, ws_admin: bool| {
            project_gate_allows(guard_archive_one(role).is_ok(), role.is_some(), ws_admin)
        };
        assert!(allows(Some(20), false));
        assert!(allows(Some(15), false));
        assert!(!allows(Some(5), false));
        assert!(!allows(None, false));
        assert!(!allows(None, true));
        // Django fallback parity: any active membership + ws ADMIN passes
        // (even GUEST — `permissions/base.py:64-78` is role-agnostic).
        assert!(allows(Some(5), true));
        assert!(allows(Some(15), true));
    }

    #[test]
    fn archived_detail_keys_are_issue_detail_serializer_order() {
        // `IssueDetailSerializer` (`serializers/issue.py:934-945`) =
        // `IssueSerializer.Meta.fields` (`issue.py:786-812`, 25 keys) +
        // `["description_html","is_subscribed","is_intake"]` in order.
        // Delta vs I2 `IssueDetailRow` (25 keys): PLUS those 3 trailing
        // keys — hence the dedicated struct (plan "verify, don't fork").
        assert_eq!(ARCHIVED_DETAIL_KEYS.len(), 28);
        assert_eq!(
            &ARCHIVED_DETAIL_KEYS[..25],
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
            ]
        );
        assert_eq!(
            &ARCHIVED_DETAIL_KEYS[25..],
            &["description_html", "is_subscribed", "is_intake"]
        );
    }

    #[test]
    fn archive_one_handlers_exist_for_archive_routes() {
        // Wiring guard: `main.rs` registers
        // `GET+POST+DELETE .../issues/:pk/archive/` → `retrieve`/`archive`/
        // `unarchive` (Django `IssueArchiveViewSet` retrieve/archive/
        // unarchive, `urls/issue.py:228-232`, issues path ONLY).
        let _ = super::retrieve;
        let _ = super::archive;
        let _ = super::unarchive;
    }
}
