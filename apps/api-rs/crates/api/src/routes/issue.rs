use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
/// exact `.values()` key order from `IssueListEndpoint.get`
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

/// GET `/api/workspaces/:slug/projects/:project_id/issues/list/` — parity
/// with Django `IssueListEndpoint.get` default branch
/// (`plane/app/views/issue/base.py:84-205`, `fields`/`expand` unset).
///
/// - Gate: workspace-level ADMIN/MEMBER/GUEST — `ws_role` `Some` (any of
///   20/15/5) passes, `None` → 403 `deny()`.
/// - Missing/empty `?issues` → 400 `{"error": "Issues are required"}`
///   (`base.py:88-89`); malformed UUID → 400
///   `{"error": "Please provide valid detail"}` (see `parse_issue_csv`).
/// - GUEST scoping mirrors `base.py:98-106`: an active GUEST (5) project
///   membership with `guest_view_all_features=false` restricts rows to
///   `created_by=user`.
/// - Manager scope mirrors `IssueManager`
///   (`plane/db/models/issue.py:92-101`) + `SoftDeletionManager` +
///   `base.py:114` (`state__deleted_at__isnull`): `deleted_at IS NULL`,
///   `archived_at IS NULL`, `is_draft=false`, state group `IS DISTINCT
///   FROM 'triage'` (live `states.group`, `state.py:14-20`), state's
///   `deleted_at IS NULL` (NULL-state rows pass both, matching the LEFT
///   JOIN semantics), project not archived/deleted.
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
/// side effect (`base.py:164-170`) is skipped (async worker concern);
/// annotation subqueries add explicit `deleted_at IS NULL` (Django's
/// soft-delete default managers do this implicitly); NULL-state rows pass
/// the triage exclusion via `IS DISTINCT FROM` (Django `exclude()` on an
/// empty FK is an edge case — issues always get a default state on save,
/// `issue.py:228-239`).
pub async fn list_by_ids(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(params): axum::extract::Query<ListIssuesQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if role.is_none() {
        return Ok(deny());
    }
    let ids = match parse_issue_csv(params.issues.as_deref()) {
        Ok(ids) => ids,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    // Mirrors `base.py:98-106`: active GUEST membership + project hides
    // guest features → restrict to own issues. (`deleted_at IS NULL` is
    // explicit here; Django's default managers imply it.)
    let guest_scoped: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_members pm \
          JOIN projects p ON p.id = pm.project_id \
          WHERE pm.project_id = $1 AND pm.member_id = $2 AND pm.role = 5 \
          AND pm.is_active = true AND pm.deleted_at IS NULL \
          AND p.guest_view_all_features = false AND p.deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let guest_filter = if guest_scoped {
        "AND i.created_by_id = $4"
    } else {
        "AND ($4 IS NULL OR TRUE)"
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
          AND m.archived_at IS NULL AND m.deleted_at IS NULL), '{{}}'::uuid[]) AS module_ids, \
        COALESCE((SELECT array_agg(DISTINCT il.label_id) FROM issue_labels il \
          WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{{}}'::uuid[]) AS label_ids, \
        COALESCE((SELECT array_agg(DISTINCT ia.assignee_id) FROM issue_assignees ia \
          WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '{{}}'::uuid[]) AS assignee_ids, \
        (SELECT COUNT(*) FROM issues si \
          LEFT JOIN states ss ON ss.id = si.state_id \
          WHERE si.parent_id = i.id AND si.deleted_at IS NULL \
          AND si.archived_at IS NULL AND si.is_draft = false \
          AND ss.deleted_at IS NULL AND ss.\"group\" IS DISTINCT FROM 'triage') AS sub_issues_count, \
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
        AND s.deleted_at IS NULL AND s.\"group\" IS DISTINCT FROM 'triage' \
        AND i.id = ANY($3) \
        AND EXISTS(SELECT 1 FROM projects p \
          WHERE p.id = i.project_id AND p.archived_at IS NULL AND p.deleted_at IS NULL) \
        {guest_filter} \
        ORDER BY i.created_at DESC"
    );
    let rows: Vec<IssueListRow> = sqlx::query_as(&sql)
        .bind(project_id)
        .bind(&slug)
        .bind(&ids)
        .bind(if guest_scoped { Some(auth.0) } else { None::<uuid::Uuid> })
        .fetch_all(&st.pool)
        .await?;
    Ok((StatusCode::OK, Json(json!(rows))))
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
}
