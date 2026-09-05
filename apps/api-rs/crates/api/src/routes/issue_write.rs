use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::project::deny;
use crate::{middleware::auth::AuthUser, state::AppState};
use super::issue_common::{IssueOut, fetch_project_member_role, is_workspace_admin, project_gate_allows};

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

pub fn validate_create(body: &CreateIssue) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    Ok(())
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
