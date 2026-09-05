use axum::{extract::State, http::StatusCode, Json};
use chrono::NaiveDate;
use serde_json::{json, Value};

use crate::routes::project::{deny, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Bulk issue-dates update — parity with Django
/// `IssueBulkUpdateDateEndpoint`
/// (`plane/app/views/issue/base.py:1106-1183`,
/// `plane/app/urls/issue.py:251-255`):
/// `POST .../issue-dates/` with `{updates: [{id, start_date, target_date}]}`.
/// Celery `issue_activity.delay` writes skipped (Batch C precedent — Rust
/// never writes activities).
///
/// Locked semantics (plan D5):
/// - Validation merges new-over-current (`new_start or current_start`,
///   `base.py:1113-1114`); date strings are `%Y-%m-%d`
///   (`strptime`, `base.py:1117-1120`); `start > target` → 400
///   `{"message": "Start date cannot exceed target date"}`
///   (`base.py:1148-1152`, key `message`, NOT `error`).
/// - Unknown issue ids are SKIPPED silently (`continue`,
///   `base.py:1142-1143`); empty `updates` → 200 (loop no-op,
///   `bulk_update([])`).
/// - NO explicit transaction — atomicity comes from the single
///   `bulk_update` at the very end (`base.py:1181`); every 400 returns
///   before it so failed rows persist nothing. The Rust mirror is a
///   single `UPDATE ... FROM (unnest)` statement (one statement = atomic
///   in Postgres), with NO `BEGIN`; `updated_at` is NOT bumped
///   (`bulk_update(["start_date", "target_date"])` writes those columns
///   only).
/// - Missing `id` key → `KeyError` → 400 `{"error": "The required key
///   does not exist."}` via the base handler (`views/base.py:194-198`);
///   the `issue_ids` comprehension (`base.py:1130`) runs BEFORE the DB
///   fetch, so that 400 precedes any query.
/// - Scope is the PLAIN manager (`Issue.objects.filter(id__in=...,
///   workspace__slug=slug, project_id=...)`, `base.py:1134` =
///   `SoftDeletionManager`, live rows only) — NO triage/archived/draft
///   exclusion (unlike `IssueManager` / I3 `bulk_delete_scope_sql`).
/// - Gate `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
///   (`base.py:1128`, default `level="PROJECT"`): allowed-role branch
///   needs 20/15, plus the standard workspace-ADMIN fallback (any active
///   membership + ws ADMIN, `permissions/base.py:53-78`) via the shared
///   `project_gate_allows`.
///
/// Sane deviations (reviewer-adjudicable): a missing/unparseable body
/// reaches the handler as `None` via Axum's `Option<Json>` (I3
/// `resolve_bulk_ids` precedent) and maps to `[]` → 200 (Django's
/// `request.data.get("updates", [])` default); a malformed-JSON body is
/// likewise swallowed into `None` → 200 where Django would 400
/// `ParseError` (unreachable in FE flows). An unparseable `%Y-%m-%d`
/// string propagates as 500 `AppError` (Django `strptime` → `ValueError`
/// → generic 500, `views/base.py:200-206`; status parity, body differs).
/// A non-string truthy date (Django `TypeError` → 500) likewise 500s.
/// Falsy dates (`null`, `""`, missing) mean "no update for that field"
/// (`if start_date:`, `base.py:1154,1169`) and fall back to current in
/// validation (`or`, `base.py:1113-1114`) — mirrored by
/// `extract_new_date` mapping them to `None`.

/// Quoted from `plane/app/views/issue/base.py:1183`.
pub(crate) const DATES_SUCCESS_MSG: &str = "Issues updated successfully";
/// Quoted from `plane/app/views/issue/base.py:1150`.
pub(crate) const DATE_EXCEEDS_MSG: &str = "Start date cannot exceed target date";
/// Quoted from `plane/app/views/base.py:194-198` (Django `KeyError` → 400;
/// hit here when an entry misses the `id` key, `issue/base.py:1130,1140`).
pub(crate) const KEY_MISSING_MSG: &str = "The required key does not exist.";
/// Quoted from `plane/app/views/base.py:182-185` (Django `ValidationError`
/// → 400; hit here when an `id` is not a valid UUID, mirroring the ORM
/// `id__in` validation).
pub(crate) const INVALID_DETAIL_MSG: &str = "Please provide valid detail";

/// PROJECT-level role check: mirrors `@allow_permission([ROLE.ADMIN,
/// ROLE.MEMBER])` (`issue/base.py:1128`, default `level="PROJECT"` —
/// `permissions/base.py:17`): roles 20/15 pass; anything else (incl.
/// GUEST 5 and non-member) falls to the workspace-ADMIN fallback applied
/// by the caller via the shared `project_gate_allows`.
pub(crate) fn guard_issue_dates(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `datetime.strptime(s, "%Y-%m-%d").date()`
/// (`issue/base.py:1117-1120`): strict `%Y-%m-%d` only.
pub(crate) fn parse_ymd(s: &str) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
}

/// Mirrors `update.get("start_date")` (`issue/base.py:1145-1146`) COMBINED
/// with the truthiness tests (`if start_date:`, `base.py:1154,1169` and
/// `new_start or current_start`, `base.py:1113-1114`): missing / explicit
/// null / empty string are all falsy → `None` (no update for that field,
/// current kept); a non-empty string is parsed (`Err` → caller 500s like
/// Django's `ValueError`); any other JSON type is truthy-but-unparseable
/// in Django (`TypeError` → 500) → `Err` likewise.
pub(crate) fn extract_new_date(raw: Option<&Value>) -> Result<Option<NaiveDate>, String> {
    match raw {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => parse_ymd(s).map(Some).map_err(|e| e.to_string()),
        Some(_) => Err("invalid date".to_string()),
    }
}

/// Mirrors `start = new_start or current_start`
/// (`issue/base.py:1113-1114`): a truthy new date (already mapped through
/// `extract_new_date`, so falsy inputs are `None` here) wins, else the
/// current column value is kept.
pub(crate) fn resolve_date(new: Option<NaiveDate>, current: Option<NaiveDate>) -> Option<NaiveDate> {
    new.or(current)
}

/// Mirrors the tail of `validate_dates` (`issue/base.py:1122-1124`):
/// `if start and target and start > target: return False`. Callers pass
/// the MERGED pair (see `resolve_date`); `None` on either side passes.
pub(crate) fn validate_dates(
    start: Option<NaiveDate>,
    target: Option<NaiveDate>,
) -> Result<(), String> {
    if let (Some(s), Some(t)) = (start, target) {
        if s > t {
            return Err(DATE_EXCEEDS_MSG.to_string());
        }
    }
    Ok(())
}

/// Live issue row for the fetch, mirroring `Issue.objects.filter(...)`
/// (`issue/base.py:1134`): plain-manager scope (ws slug + project + ids,
/// `deleted_at IS NULL`), start/target columns for the merge.
#[derive(Debug, Clone, sqlx::FromRow)]
struct IssueDateRow {
    id: uuid::Uuid,
    start_date: Option<NaiveDate>,
    target_date: Option<NaiveDate>,
}

/// POST `/api/workspaces/:slug/projects/:project_id/issue-dates/` —
/// parity with `IssueBulkUpdateDateEndpoint.post`
/// (`issue/base.py:1128-1183`, `urls/issue.py:251-255`).
pub async fn bulk_update_dates(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    body: Option<Json<Value>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` at PROJECT level
    // (`permissions/base.py:53-78`): allowed-role branch needs 20/15; the
    // fallback (any active membership + workspace ADMIN) is shared.
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        guard_issue_dates(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    // `request.data.get("updates", [])` (`base.py:1129`): missing body
    // (`None` via `Option<Json>`), missing key, explicit null, or a
    // non-array all behave as `[]` → 200 below (Django would 500 on a
    // non-list; see the sane-deviation note above).
    let updates: Vec<Value> = body
        .as_ref()
        .and_then(|Json(v)| v.get("updates"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // `issue_ids = [update["id"] for update in updates]` (`base.py:1130`):
    // BEFORE any DB work. A missing key is Django's `KeyError` → 400
    // `KEY_MISSING_MSG`; an unparseable UUID is the ORM's
    // `ValidationError` → 400 `INVALID_DETAIL_MSG`.
    let mut ids: Vec<uuid::Uuid> = Vec::with_capacity(updates.len());
    for update in &updates {
        let Some(raw_id) = update.get("id") else {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": KEY_MISSING_MSG})),
            ));
        };
        let Ok(id) = raw_id
            .as_str()
            .ok_or(())
            .and_then(|s| uuid::Uuid::parse_str(s).map_err(|_| ()))
        else {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": INVALID_DETAIL_MSG})),
            ));
        };
        ids.push(id);
    }
    // Empty → 200 with no DB write (loop no-op, `bulk_update([])`).
    if ids.is_empty() {
        return Ok((StatusCode::OK, Json(json!({"message": DATES_SUCCESS_MSG}))));
    }
    // `Issue.objects.filter(id__in=issue_ids, workspace__slug=slug,
    // project_id=project_id)` (`base.py:1134`) — single query; the dict
    // (`base.py:1135`) becomes the `HashMap` below for the silent-skip
    // lookup (`issues_dict.get(issue_id)` + `continue`, `base.py:1140-1143`).
    // `str(issue.id)` keys == the JSON string ids parsed above.
    let rows: Vec<IssueDateRow> = sqlx::query_as(
        "SELECT id, start_date, target_date FROM issues \
         WHERE project_id = $1 \
         AND workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $2) \
         AND id = ANY($3) AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .bind(&ids)
    .fetch_all(&st.pool)
    .await?;
    let by_id: std::collections::HashMap<uuid::Uuid, &IssueDateRow> =
        rows.iter().map(|r| (r.id, r)).collect();
    // Per-row merge + validation in `updates` order; the FIRST failure
    // 400s before the single UPDATE below (so nothing persists —
    // `base.py:1148-1152` returns before `bulk_update`, `base.py:1181`).
    // Only rows with at least one truthy new date are collected
    // (`if start_date:` / `if target_date:`, `base.py:1154,1169`); the
    // stored pair is the MERGED one (`bulk_update` writes both columns,
    // unchanged values round-trip identically).
    let mut pending: Vec<(uuid::Uuid, Option<NaiveDate>, Option<NaiveDate>)> = Vec::new();
    for (update, id) in updates.iter().zip(ids.iter()) {
        let Some(cur) = by_id.get(id) else {
            continue;
        };
        let new_start =
            extract_new_date(update.get("start_date")).map_err(|e| anyhow::anyhow!(e))?;
        let new_target =
            extract_new_date(update.get("target_date")).map_err(|e| anyhow::anyhow!(e))?;
        if validate_dates(
            resolve_date(new_start, cur.start_date),
            resolve_date(new_target, cur.target_date),
        )
        .is_err()
        {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"message": DATE_EXCEEDS_MSG})),
            ));
        }
        if new_start.is_some() || new_target.is_some() {
            pending.push((
                *id,
                resolve_date(new_start, cur.start_date),
                resolve_date(new_target, cur.target_date),
            ));
        }
    }
    // The single `bulk_update` (`base.py:1181`): one statement, NO
    // explicit transaction. `bulk_update([])` is a no-op (Django skips
    // the query for an empty list), mirrored by skipping the UPDATE.
    if !pending.is_empty() {
        let p_ids: Vec<uuid::Uuid> = pending.iter().map(|(id, _, _)| *id).collect();
        let p_starts: Vec<Option<NaiveDate>> =
            pending.iter().map(|(_, s, _)| *s).collect();
        let p_targets: Vec<Option<NaiveDate>> =
            pending.iter().map(|(_, _, t)| *t).collect();
        sqlx::query(
            "UPDATE issues AS i SET start_date = d.start_date, target_date = d.target_date \
             FROM (SELECT unnest($1::uuid[]) AS id, unnest($2::date[]) AS start_date, \
             unnest($3::date[]) AS target_date) AS d \
             WHERE i.id = d.id",
        )
        .bind(&p_ids)
        .bind(&p_starts)
        .bind(&p_targets)
        .execute(&st.pool)
        .await?;
    }
    Ok((StatusCode::OK, Json(json!({"message": DATES_SUCCESS_MSG}))))
}

#[cfg(test)]
mod batch_d_d5_tests {
    use super::*;

    #[test]
    fn messages_are_byte_exact() {
        // Quoted from `plane/app/views/issue/base.py:1150,1183` (key
        // `message`) and `plane/app/views/base.py:194-198,182-185` (key
        // `error` mappings for `KeyError` / `ValidationError`).
        assert_eq!(DATES_SUCCESS_MSG, "Issues updated successfully");
        assert_eq!(DATE_EXCEEDS_MSG, "Start date cannot exceed target date");
        assert_eq!(KEY_MISSING_MSG, "The required key does not exist.");
        assert_eq!(INVALID_DETAIL_MSG, "Please provide valid detail");
    }

    #[test]
    fn validate_dates_rejects_start_after_target() {
        // Mirrors `validate_dates` (`issue/base.py:1107-1124`): the merged
        // start `2026-01-02` exceeds the merged target `2026-01-01` →
        // `False` → 400 `DATE_EXCEEDS_MSG`.
        let start = parse_ymd("2026-01-02").unwrap();
        let target = parse_ymd("2026-01-01").unwrap();
        assert_eq!(
            validate_dates(Some(start), Some(target)).unwrap_err(),
            "Start date cannot exceed target date"
        );
    }

    #[test]
    fn validate_dates_ok_cases_pass() {
        // start == target and start < target pass (`start > target` is the
        // only failure); `None` on either side passes (`if start and
        // target`, `base.py:1122`).
        let d1 = parse_ymd("2026-01-01").unwrap();
        let d2 = parse_ymd("2026-01-02").unwrap();
        assert!(validate_dates(Some(d1), Some(d2)).is_ok());
        assert!(validate_dates(Some(d1), Some(d1)).is_ok());
        assert!(validate_dates(None, Some(d2)).is_ok());
        assert!(validate_dates(Some(d1), None).is_ok());
        assert!(validate_dates(None, None).is_ok());
    }

    #[test]
    fn resolve_date_merges_new_over_current() {
        // Mirrors `start = new_start or current_start`
        // (`issue/base.py:1113-1114`): a truthy new date wins, else the
        // current column value is kept.
        let cur = parse_ymd("2026-01-01").unwrap();
        let new = parse_ymd("2026-02-01").unwrap();
        assert_eq!(resolve_date(Some(new), Some(cur)), Some(new));
        assert_eq!(resolve_date(None, Some(cur)), Some(cur));
        assert_eq!(resolve_date(None, None), None);
        assert_eq!(resolve_date(Some(new), None), Some(new));
    }

    #[test]
    fn extract_new_date_truthiness_mirrors_django() {
        // `update.get(...)` missing / explicit null are falsy → `None`
        // (`base.py:1145-1146` + `if start_date:`, `base.py:1154`); `""`
        // is falsy too (`"" or current → current`).
        assert_eq!(extract_new_date(None).unwrap(), None);
        assert_eq!(extract_new_date(Some(&Value::Null)).unwrap(), None);
        assert_eq!(
            extract_new_date(Some(&json!(""))).unwrap(),
            None
        );
        // Non-empty `%Y-%m-%d` strings parse (`strptime`,
        // `base.py:1117-1120`); other formats fail (Django `ValueError`
        // → 500, mapped by the handler to `AppError`).
        assert_eq!(
            extract_new_date(Some(&json!("2026-01-02"))).unwrap(),
            Some(parse_ymd("2026-01-02").unwrap())
        );
        assert!(extract_new_date(Some(&json!("02/01/2026"))).is_err());
        assert!(extract_new_date(Some(&json!("not-a-date"))).is_err());
        // Non-string truthy values fail (Django `TypeError` → 500).
        assert!(extract_new_date(Some(&json!(123))).is_err());
    }

    #[test]
    fn guard_issue_dates_is_admin_member_only() {
        // Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
        // (`issue/base.py:1128`, default `level="PROJECT"`): 20/15 pass
        // outright; GUEST (5) / unknown / non-member fall to the
        // workspace-ADMIN fallback in the caller via
        // `project_gate_allows` (same shape as the I3 ADMIN-only gate,
        // but with `has_allowed_role = (role ∈ {20, 15})`).
        assert!(guard_issue_dates(Some(20)).is_ok());
        assert!(guard_issue_dates(Some(15)).is_ok());
        assert_eq!(
            guard_issue_dates(Some(5)).unwrap_err(),
            "You don't have the required permissions."
        );
        assert!(guard_issue_dates(Some(10)).is_err());
        assert_eq!(
            guard_issue_dates(None).unwrap_err(),
            "You don't have the required permissions."
        );
        let allows = |role: Option<i16>, ws_admin: bool| {
            project_gate_allows(
                guard_issue_dates(role).is_ok(),
                role.is_some(),
                ws_admin,
            )
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
    fn parse_ymd_is_strptime_percent_y_m_d() {
        // `datetime.strptime(s, "%Y-%m-%d")` (`base.py:1118,1120`):
        // zero-padded ISO only; datetimes and slashes rejected.
        assert!(parse_ymd("2026-01-02").is_ok());
        assert!(parse_ymd("2026-01-02T00:00:00Z").is_err());
        assert!(parse_ymd("02/01/2026").is_err());
        assert!(parse_ymd("").is_err());
    }
}
