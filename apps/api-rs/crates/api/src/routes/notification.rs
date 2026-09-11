use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::issue_common::{
    next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str,
    total_pages, DetailEnvelope, PageWindow,
};
use crate::routes::member::deny_detail;
use crate::routes::project::ws_role;

/// Mirrors `plane/app/views/notification/base.py` for
/// `plane/app/urls/notification.py`: list (receiver-scoped), unread counts,
/// read/unread + archive/unarchive toggles, mark-all-read (with
/// snoozed/archived/type variants), `:pk/` detail GET+PATCH+DELETE
/// (`NotificationViewSet` retrieve/partial_update/destroy,
/// `urls/notification.py:22-26`), and notification-preference GET/PATCH.
/// Sending notifications is a worker concern (out of scope).
pub const PREFERENCE_KEYS: [&str; 5] = [
    "property_change",
    "state_change",
    "comment",
    "mention",
    "issue_completed",
];

pub fn validate_preference_patch(patch: &HashMap<String, Value>) -> Result<(), String> {
    for (key, value) in patch {
        if !PREFERENCE_KEYS.contains(&key.as_str()) {
            return Err(format!("unknown preference key: {key}"));
        }
        if !value.is_boolean() {
            return Err(format!("preference value must be boolean: {key}"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MarkAllRead {
    #[serde(default)]
    pub snoozed: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub r#type: Option<String>,
}

/// Full `NotificationSerializer` row (`serializers/notification.py:14-22`:
/// `__all__` + nested `triggered_by_details` lite + the three annotated
/// bools). `is_inbox_issue`/`is_intake_issue` share one `Exists` subquery
/// (`base.py:59-65` — the duplication is Django-verbatim); the intake
/// statuses are `Issue.issue_intake__status__in=[0, 2, -2]`
/// (Snoozed/Accepted?/Duplicate… `models/intake.py:42-61`: 0 Snoozed,
/// 2 Duplicate, -2 Pending).
#[derive(Debug, Clone, sqlx::FromRow)]
struct NotifFullRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
    data: Option<Value>,
    entity_identifier: Option<uuid::Uuid>,
    entity_name: String,
    title: String,
    message: Option<Value>,
    message_html: String,
    message_stripped: Option<String>,
    sender: String,
    triggered_by_id: Option<uuid::Uuid>,
    tb_first_name: Option<String>,
    tb_last_name: Option<String>,
    tb_avatar: Option<String>,
    tb_avatar_url: Option<String>,
    tb_is_bot: Option<bool>,
    tb_display_name: Option<String>,
    receiver_id: uuid::Uuid,
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    snoozed_till: Option<chrono::DateTime<chrono::Utc>>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    is_inbox_issue: bool,
    is_intake_issue: bool,
    is_mentioned_notification: bool,
}

const NOTIF_FULL_COLS: &str = "n.id, n.created_at, n.updated_at, n.created_by_id, n.updated_by_id, \
    n.workspace_id, n.project_id, n.data, n.entity_identifier, n.entity_name, n.title, \
    n.message, n.message_html, n.message_stripped, n.sender, n.triggered_by_id, \
    tu.first_name AS tb_first_name, tu.last_name AS tb_last_name, tu.avatar AS tb_avatar, \
    CASE WHEN tu.avatar_asset_id IS NOT NULL \
      THEN '/api/assets/v2/static/' || tu.avatar_asset_id::text || '/' ELSE tu.avatar END AS tb_avatar_url, \
    tu.is_bot AS tb_is_bot, tu.display_name AS tb_display_name, \
    n.receiver_id, n.read_at, n.snoozed_till, n.archived_at, \
    EXISTS(SELECT 1 FROM issues ii JOIN intake_issues iti ON iti.issue_id = ii.id \
      WHERE ii.id = n.entity_identifier AND iti.status IN (0, 2, -2) \
      AND ii.workspace_id = n.workspace_id AND ii.deleted_at IS NULL \
      AND iti.deleted_at IS NULL) AS is_inbox_issue, \
    EXISTS(SELECT 1 FROM issues ii JOIN intake_issues iti ON iti.issue_id = ii.id \
      WHERE ii.id = n.entity_identifier AND iti.status IN (0, 2, -2) \
      AND ii.workspace_id = n.workspace_id AND ii.deleted_at IS NULL \
      AND iti.deleted_at IS NULL) AS is_intake_issue, \
    (n.sender ILIKE '%mentioned%') AS is_mentioned_notification";

fn notif_full_json(r: &NotifFullRow) -> Value {
    let triggered_by_details = match r.triggered_by_id {
        None => Value::Null,
        Some(id) => json!({
            "id": id,
            "first_name": r.tb_first_name,
            "last_name": r.tb_last_name,
            "avatar": r.tb_avatar,
            "avatar_url": r.tb_avatar_url,
            "is_bot": r.tb_is_bot,
            "display_name": r.tb_display_name,
        }),
    };
    let mut o = serde_json::Map::with_capacity(24);
    o.insert("id".to_string(), json!(r.id));
    o.insert("created_at".to_string(), json!(r.created_at));
    o.insert("updated_at".to_string(), json!(r.updated_at));
    o.insert("created_by".to_string(), json!(r.created_by_id));
    o.insert("updated_by".to_string(), json!(r.updated_by_id));
    o.insert("workspace".to_string(), json!(r.workspace_id));
    o.insert("project".to_string(), json!(r.project_id));
    o.insert("data".to_string(), json!(r.data));
    o.insert("entity_identifier".to_string(), json!(r.entity_identifier));
    o.insert("entity_name".to_string(), json!(r.entity_name));
    o.insert("title".to_string(), json!(r.title));
    o.insert("message".to_string(), json!(r.message));
    o.insert("message_html".to_string(), json!(r.message_html));
    o.insert("message_stripped".to_string(), json!(r.message_stripped));
    o.insert("sender".to_string(), json!(r.sender));
    o.insert("triggered_by".to_string(), json!(r.triggered_by_id));
    o.insert("triggered_by_details".to_string(), triggered_by_details);
    o.insert("receiver".to_string(), json!(r.receiver_id));
    o.insert("read_at".to_string(), json!(r.read_at));
    o.insert("snoozed_till".to_string(), json!(r.snoozed_till));
    o.insert("archived_at".to_string(), json!(r.archived_at));
    o.insert("is_inbox_issue".to_string(), json!(r.is_inbox_issue));
    o.insert("is_intake_issue".to_string(), json!(r.is_intake_issue));
    o.insert("is_mentioned_notification".to_string(), json!(r.is_mentioned_notification));
    Value::Object(o)
}

async fn fetch_notif_full(
    pool: &sqlx::PgPool,
    slug: &str,
    pk: uuid::Uuid,
    receiver: uuid::Uuid,
) -> Result<Option<NotifFullRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {NOTIF_FULL_COLS} FROM notifications n \
         JOIN workspaces w ON w.id = n.workspace_id \
         LEFT JOIN users tu ON tu.id = n.triggered_by_id \
         WHERE w.slug = $1 AND n.id = $2 AND n.receiver_id = $3 AND n.deleted_at IS NULL",
    ))
    .bind(slug)
    .bind(pk)
    .bind(receiver)
    .fetch_optional(pool)
    .await
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // AMG at WORKSPACE level (`base.py:48`).
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let receiver = auth.0;
    // `base.py:86-96` — exact-"true" match picks the true-branch.
    let snoozed = params.get("snoozed").map(|s| s.as_str()) == Some("true");
    let archived = params.get("archived").map(|s| s.as_str()) == Some("true");
    let read = params.get("read").map(|s| s.as_str());
    // `base.py:56` — `request.GET.get("mentioned", False)`: ANY present
    // non-empty value (even "false") is truthy → mentioned-only.
    let mentioned = params.get("mentioned").map(|s| !s.is_empty()).unwrap_or(false);
    let types: Vec<&str> = params
        .get("type")
        .map(|s| s.split(',').collect())
        .unwrap_or_else(|| vec!["all"]);
    // `base.py:113-135` type subqueries (OR-combined; empty Q = no-op).
    let mut type_clauses: Vec<String> = Vec::new();
    if types.contains(&"subscribed") {
        type_clauses.push(
            "n.entity_identifier IN (SELECT s.issue_id FROM issue_subscribers s \
             JOIN workspaces w2 ON w2.id = s.workspace_id WHERE w2.slug = $1 \
             AND s.subscriber_id = $2 \
             AND NOT EXISTS(SELECT 1 FROM issues ci WHERE ci.id = s.issue_id AND ci.created_by_id = $2) \
             AND NOT EXISTS(SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = s.issue_id AND ia.assignee_id = $2))".to_string(),
        );
    }
    if types.contains(&"assigned") {
        type_clauses.push(
            "n.entity_identifier IN (SELECT a.issue_id FROM issue_assignees a \
             JOIN workspaces w2 ON w2.id = a.workspace_id WHERE w2.slug = $1 \
             AND a.assignee_id = $2)".to_string(),
        );
    }
    if types.contains(&"created") {
        // Guests see NOTHING for created (`base.py:124-128` → `.none()`).
        let role = ws_role(&st.pool, receiver, &slug).await?.unwrap_or(0);
        if role < 15 {
            let empty: Vec<Value> = Vec::new();
            return Ok((StatusCode::OK, Json(json!(empty))));
        }
        type_clauses.push(
            "n.entity_identifier IN (SELECT i.id FROM issues i \
             JOIN workspaces w2 ON w2.id = i.workspace_id WHERE w2.slug = $1 \
             AND i.created_by_id = $2)".to_string(),
        );
    }
    let type_filter = if type_clauses.is_empty() {
        String::new()
    } else {
        format!("AND ({})", type_clauses.join(" OR "))
    };
    let snoozed_filter = if snoozed {
        "AND (n.snoozed_till < now() OR n.snoozed_till IS NOT NULL)"
    } else {
        "AND (n.snoozed_till >= now() OR n.snoozed_till IS NULL)"
    };
    let archived_filter = if archived {
        "AND n.archived_at IS NOT NULL"
    } else {
        "AND n.archived_at IS NULL"
    };
    let read_filter = match read {
        Some("true") => "AND n.read_at IS NOT NULL",
        Some("false") => "AND n.read_at IS NULL",
        _ => "",
    };
    let mentioned_filter = if mentioned {
        "AND n.sender ILIKE '%mentioned%'"
    } else {
        "AND n.sender NOT ILIKE '%mentioned%'"
    };
    let base_where = format!(
        "FROM notifications n JOIN workspaces w ON w.id = n.workspace_id \
         LEFT JOIN users tu ON tu.id = n.triggered_by_id \
         WHERE w.slug = $1 AND n.receiver_id = $2 AND n.entity_name = 'issue' \
         AND n.deleted_at IS NULL {snoozed_filter} {archived_filter} \
         {read_filter} {mentioned_filter} {type_filter}"
    );
    // `base.py:141-154` — paginate ONLY when per_page+cursor both present.
    let paginate = params.contains_key("per_page") && params.contains_key("cursor");
    if paginate {
        let per_page_raw = params.get("per_page").map(|s| s.as_str());
        let limit = match parse_per_page(per_page_raw) {
            Ok(v) => v,
            Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
        };
        let cursor = match parse_cursor(params.get("cursor").map(|s| s.as_str()).unwrap_or("1000:0:0")) {
            Ok(c) => c,
            Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
        };
        let window = match page_window(cursor.page, limit) {
            Ok(w) => w,
            Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
        };
        // `sanitize_order_by(..., NOTIFICATION_ORDER_BY_ALLOWLIST={created_at,updated_at})`.
        let order_raw = params.get("order_by").map(|s| s.as_str()).unwrap_or("-created_at");
        let (bare, desc) = match order_raw.strip_prefix('-') {
            Some(b) => (b, true),
            None => (order_raw, false),
        };
        let key_col = match bare {
            "created_at" | "updated_at" if !bare.starts_with("--") => bare,
            _ => "created_at",
        };
        let desc = if bare == "created_at" || bare == "updated_at" {
            desc
        } else {
            true
        };
        let order_expr = format!(
            "n.{key_col} {} NULLS LAST, n.created_at DESC",
            if desc { "DESC" } else { "ASC" }
        );
        let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {base_where}"))
            .bind(&slug)
            .bind(receiver)
            .fetch_one(&st.pool)
            .await?;
        let mut rows: Vec<NotifFullRow> = match window {
            PageWindow::Rows(offset) => {
                let sql = format!(
                    "SELECT {NOTIF_FULL_COLS} {base_where} ORDER BY {order_expr} LIMIT $3 OFFSET $4"
                );
                sqlx::query_as(&sql)
                    .bind(&slug)
                    .bind(receiver)
                    .bind(limit + 1)
                    .bind(offset)
                    .fetch_all(&st.pool)
                    .await?
            }
            PageWindow::BeyondEnd => Vec::new(),
        };
        let next_page_results = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
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
            results: rows.iter().map(notif_full_json).collect(),
        };
        return Ok((StatusCode::OK, Json(json!(envelope))));
    }
    let rows: Vec<NotifFullRow> = sqlx::query_as(&format!(
        "SELECT {NOTIF_FULL_COLS} {base_where} ORDER BY n.snoozed_till ASC NULLS LAST, n.created_at DESC"
    ))
    .bind(&slug)
    .bind(receiver)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!(rows.iter().map(notif_full_json).collect::<Vec<_>>()))))
}

pub async fn unread(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Value>, common::errors::AppError> {
    let receiver = auth.0;
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notifications n JOIN workspaces w ON w.id = n.workspace_id WHERE w.slug = $1 AND n.receiver_id = $2 AND n.read_at IS NULL AND n.archived_at IS NULL AND n.snoozed_till IS NULL AND n.sender NOT ILIKE '%mentioned%' AND n.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(receiver)
    .fetch_one(&st.pool)
    .await?;
    let mentions: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notifications n JOIN workspaces w ON w.id = n.workspace_id WHERE w.slug = $1 AND n.receiver_id = $2 AND n.read_at IS NULL AND n.archived_at IS NULL AND n.snoozed_till IS NULL AND n.sender ILIKE '%mentioned%' AND n.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(receiver)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({
        "total_unread_notifications_count": total.0,
        "mention_unread_notifications_count": mentions.0,
    })))
}

pub async fn mark_all_read(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<MarkAllRead>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    // Mirrors the viewset's snoozed/archived/type filter variants.
    let type_filter = match body.r#type.as_deref().unwrap_or("all") {
        "watching" => "AND n.entity_identifier IN (SELECT issue_id FROM issue_subscribers s JOIN workspaces w2 ON w2.id = s.workspace_id WHERE w2.slug = $1 AND s.subscriber_id = $2)",
        "assigned" => "AND n.entity_identifier IN (SELECT issue_id FROM issue_assignees a JOIN workspaces w2 ON w2.id = a.workspace_id WHERE w2.slug = $1 AND a.assignee_id = $2)",
        "created" => "AND n.entity_identifier IN (SELECT id FROM issues i JOIN workspaces w2 ON w2.id = i.workspace_id WHERE w2.slug = $1 AND i.created_by_id = $2)",
        _ => "",
    };
    let snoozed_filter = if body.snoozed {
        "AND (n.snoozed_till < now() OR n.snoozed_till IS NOT NULL)"
    } else {
        "AND (n.snoozed_till >= now() OR n.snoozed_till IS NULL)"
    };
    let archived_filter = if body.archived {
        "AND n.archived_at IS NOT NULL"
    } else {
        "AND n.archived_at IS NULL"
    };
    let sql = format!(
        "UPDATE notifications n SET read_at = now() FROM workspaces w WHERE w.id = n.workspace_id AND w.slug = $1 AND n.receiver_id = $2 AND n.read_at IS NULL AND n.deleted_at IS NULL {snoozed_filter} {archived_filter} {type_filter}"
    );
    sqlx::query(&sql).bind(&slug).bind(receiver).execute(&st.pool).await?;
    Ok((StatusCode::OK, Json(json!({"message": "Successful"}))))
}

async fn toggle(
    st: &AppState,
    slug: &str,
    receiver: uuid::Uuid,
    pk: uuid::Uuid,
    column: &str,
    set: bool,
) -> Result<bool, common::errors::AppError> {
    let sql = format!(
        "UPDATE notifications n SET {column} = CASE WHEN $3 THEN now() ELSE NULL END FROM workspaces w WHERE w.id = n.workspace_id AND w.slug = $1 AND n.id = $2 AND n.receiver_id = $4 AND n.deleted_at IS NULL"
    );
    let n = sqlx::query(&sql)
        .bind(slug)
        .bind(pk)
        .bind(set)
        .bind(receiver)
        .execute(&st.pool)
        .await?
        .rows_affected();
    Ok(n > 0)
}

pub async fn mark_read(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &slug, receiver, pk, "read_at", true).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    // Django `mark_read` (`base.py:168-174`) returns the 200 full serializer row.
    match fetch_notif_full(&st.pool, &slug, pk, receiver).await? {
        Some(row) => Ok((StatusCode::OK, Json(notif_full_json(&row)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"})))),
    }
}

pub async fn mark_unread(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &slug, receiver, pk, "read_at", false).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    // Django `mark_unread` (`base.py:176-182`) returns the 200 full serializer row.
    match fetch_notif_full(&st.pool, &slug, pk, receiver).await? {
        Some(row) => Ok((StatusCode::OK, Json(notif_full_json(&row)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"})))),
    }
}

pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &slug, receiver, pk, "archived_at", true).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    // Django `archive` (`base.py:184-190`) returns the 200 full serializer row.
    match fetch_notif_full(&st.pool, &slug, pk, receiver).await? {
        Some(row) => Ok((StatusCode::OK, Json(notif_full_json(&row)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"})))),
    }
}

pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &slug, receiver, pk, "archived_at", false).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    // Django `unarchive` (`base.py:192-198`) returns the 200 full serializer row.
    match fetch_notif_full(&st.pool, &slug, pk, receiver).await? {
        Some(row) => Ok((StatusCode::OK, Json(notif_full_json(&row)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"})))),
    }
}

/// Extracts ONLY `snoozed_till` as a string from a PATCH body, mirroring
/// Django's hardcoded `notification_data = {"snoozed_till":
/// request.data.get("snoozed_till", None)}` (`base.py:160`): every other key
/// (e.g. `read_at`) is ignored. Missing/non-string → None, which the caller
/// binds as SQL NULL (same as Django's `.get(..., None)` default clearing
/// the column).
pub fn snoozed_till_from_body(body: &Value) -> Option<String> {
    body.get("snoozed_till")?.as_str().map(|s| s.to_string())
}

/// Gate (detail handlers): mirrors the neighboring notification handlers —
/// receiver-scoped `(workspace__slug, pk, receiver=user)` queries
/// (`base.py:37-46` `get_queryset`), no explicit `ws_role` lookup. This
/// carries Django's `@allow_permission([ADMIN, MEMBER, GUEST],
/// level="WORKSPACE")`: a caller outside the workspace has no receiver rows
/// under that slug, so every path misses → 404 (Django would 403 the
/// non-member instead — normalized to the file's 404 precedent). Rows are
/// fetched via [`fetch_notif_full`] (full serializer shape).

/// Parity with `NotificationViewSet.retrieve` (default DRF retrieve over the
/// receiver-scoped queryset, `urls/notification.py:24`). Miss: Django's
/// `.get()` raises → 500; mapped to the file's sane 404
/// (`{"error": "Notification not found"}`, `mark_read` precedent).
pub async fn get_notification(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match fetch_notif_full(&st.pool, &slug, pk, auth.0).await? {
        Some(n) => Ok((StatusCode::OK, Json(notif_full_json(&n)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"})))),
    }
}

/// Parity with `NotificationViewSet.partial_update` (`base.py:156-166`):
/// writes ONLY `snoozed_till` (see `snoozed_till_from_body`), extra keys
/// ignored; **200** serializer row. Miss → 404. A present-but-invalid
/// datetime string errors at the `$4::timestamptz` cast (500 via `AppError`;
/// Django would 400 via serializer validation — unreachable from the FE,
/// which sends ISO strings or null).
pub async fn patch_notification(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let snoozed = snoozed_till_from_body(&body);
    let n = sqlx::query(
        "UPDATE notifications n SET snoozed_till = $4::timestamptz, updated_at = now() FROM workspaces w WHERE w.id = n.workspace_id AND w.slug = $1 AND n.id = $2 AND n.receiver_id = $3 AND n.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .bind(auth.0)
    .bind(snoozed)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    // 200 serializer row (`base.py:165`); the row exists (we just updated
    // it), re-read through the GET twin's scope (a concurrent delete
    // racing us here misses → 404, never a panic).
    match fetch_notif_full(&st.pool, &slug, pk, auth.0).await? {
        Some(row) => Ok((StatusCode::OK, Json(notif_full_json(&row)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"})))),
    }
}

/// Parity with `NotificationViewSet.destroy` (DRF `ModelViewSet` default
/// `destroy()` → `instance.delete()`, `urls/notification.py:24`). The
/// `Notification` model soft-deletes in this codebase (`deleted_at` column;
/// every read in this file filters `deleted_at IS NULL`), so the mapping is
/// a soft-delete UPDATE — never a hard DELETE. **204**; miss → 404.
pub async fn destroy_notification(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let n = sqlx::query(
        "UPDATE notifications n SET deleted_at = now() FROM workspaces w WHERE w.id = n.workspace_id AND w.slug = $1 AND n.id = $2 AND n.receiver_id = $3 AND n.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// Satu baris `user_notification_preferences` (kolom live: audit
/// `AuditModel` + `user/workspace/project` + 5 bool, `migrations/0056,0073`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct PreferenceRow {
    id: uuid::Uuid,
    user_id: uuid::Uuid,
    workspace_id: Option<uuid::Uuid>,
    project_id: Option<uuid::Uuid>,
    property_change: bool,
    state_change: bool,
    comment: bool,
    mention: bool,
    issue_completed: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Objek penuh `UserNotificationPreferenceSerializer` (`fields="__all__"`,
/// `serializers/notification.py:25-28`).
fn preference_json(r: &PreferenceRow) -> Value {
    json!({
        "id": r.id,
        "user": r.user_id,
        "workspace": r.workspace_id,
        "project": r.project_id,
        "property_change": r.property_change,
        "state_change": r.state_change,
        "comment": r.comment,
        "mention": r.mention,
        "issue_completed": r.issue_completed,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
        "deleted_at": r.deleted_at,
    })
}

async fn fetch_preferences(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
) -> Result<PreferenceRow, common::errors::AppError> {
    sqlx::query_as::<_, PreferenceRow>(
        "SELECT id, user_id, workspace_id, project_id, property_change, state_change, \
                comment, mention, issue_completed, created_at, updated_at, \
                created_by_id, updated_by_id, deleted_at \
         FROM user_notification_preferences WHERE user_id = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.into())
}

/// Get-or-create: INSERT default semua-true (sesuai default model
/// `notification.py:104-108`) bila baris belum ada, lalu baca kembali.
async fn get_or_create_preferences(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
) -> Result<PreferenceRow, common::errors::AppError> {
    sqlx::query(
        "INSERT INTO user_notification_preferences (id, user_id, property_change, state_change, comment, mention, issue_completed, created_at, updated_at) VALUES (gen_random_uuid(), $1, true, true, true, true, true, now(), now()) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    fetch_preferences(pool, user_id).await
}

/// Galat validasi per-field bentuk `serializer.errors` (`{field: [pesan]}`,
/// siap jadi body 400). Kunci tak dikenal ditolak (lihat
/// `validate_preference_patch`); non-bool memakai pesan DRF `BooleanField`.
pub fn preference_patch_errors(patch: &HashMap<String, Value>) -> Value {
    let mut keys: Vec<&String> = patch.keys().collect();
    keys.sort();
    let mut errors = serde_json::Map::new();
    for key in keys {
        let value = &patch[key];
        if !PREFERENCE_KEYS.contains(&key.as_str()) {
            errors.insert(key.clone(), json!([format!("unknown preference key: {key}")]));
        } else if !value.is_boolean() {
            errors.insert(key.clone(), json!(["Must be a valid boolean."]));
        }
    }
    Value::Object(errors)
}

pub async fn get_preferences(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, common::errors::AppError> {
    // Baris tak ada → INSERT default (semua-true sesuai model) lalu
    // kembalikan objek penuh (get-or-create; Django mengasumsikan backfill
    // `migrations/0057`, user baru belum punya baris).
    let row = get_or_create_preferences(&st.pool, auth.0).await?;
    Ok(Json(preference_json(&row)))
}

pub async fn patch_preferences(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<HashMap<String, Value>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Tak valid → 400 bentuk `serializer.errors` (`{field: [pesan]}`),
    // cermin `Response(serializer.errors, 400)` (`base.py:313`).
    if validate_preference_patch(&body).is_err() {
        return Ok((StatusCode::BAD_REQUEST, Json(preference_patch_errors(&body))));
    }
    let receiver = auth.0;
    // Pastikan baris ada dulu (sama seperti GET), lalu terapkan patch.
    get_or_create_preferences(&st.pool, receiver).await?;
    for key in PREFERENCE_KEYS {
        if let Some(value) = body.get(key) {
            let sql = format!("UPDATE user_notification_preferences SET {key} = $1, updated_at = now() WHERE user_id = $2 AND deleted_at IS NULL");
            sqlx::query(&sql).bind(value.as_bool().unwrap_or(true)).bind(receiver).execute(&st.pool).await?;
        }
    }
    // 200 objek penuh yang sudah ter-update (cermin
    // `Response(serializer.data, 200)` di `base.py:312`), bukan `{"message"}`.
    let row = fetch_preferences(&st.pool, receiver).await?;
    Ok((StatusCode::OK, Json(preference_json(&row))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_non_bool_is_400_errors_shape() {
        // PATCH tak valid (non-bool) → 400 bentuk `serializer.errors`
        // (`{field: [pesan]}`, cermin `Response(serializer.errors, 400)`
        // di `base.py:313`), bukan 500.
        let mut patch = HashMap::new();
        patch.insert("comment".to_string(), json!("yes"));
        let errors = preference_patch_errors(&patch);
        assert_eq!(errors, json!({"comment": ["Must be a valid boolean."]}));
        assert!(errors.as_object().expect("objek")["comment"].is_array());
    }

    #[test]
    fn patch_unknown_key_is_field_error() {
        let mut patch = HashMap::new();
        patch.insert("telepathy".to_string(), json!(true));
        let errors = preference_patch_errors(&patch);
        assert_eq!(errors.as_object().expect("objek").len(), 1);
        assert!(errors["telepathy"].is_array());
    }

    #[test]
    fn patch_valid_is_empty_no_400() {
        let mut patch = HashMap::new();
        patch.insert("comment".to_string(), json!(false));
        patch.insert("mention".to_string(), json!(true));
        assert_eq!(preference_patch_errors(&patch), json!({}));
    }

    #[test]
    fn preference_json_is_full_serializer_object() {
        // GET/PATCH mengembalikan objek PENUH
        // `UserNotificationPreferenceSerializer` (`fields="__all__"`,
        // `serializers/notification.py:25-28`): id + FK + 5 bool + audit.
        let r = PreferenceRow {
            id: uuid::Uuid::nil(),
            user_id: uuid::Uuid::nil(),
            workspace_id: None,
            project_id: None,
            property_change: true,
            state_change: false,
            comment: true,
            mention: true,
            issue_completed: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by_id: None,
            updated_by_id: None,
            deleted_at: None,
        };
        let v = preference_json(&r);
        for k in [
            "id", "user", "workspace", "project",
            "property_change", "state_change", "comment", "mention", "issue_completed",
            "created_at", "updated_at", "created_by", "updated_by", "deleted_at",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v.as_object().expect("objek").len(), 14);
        assert_eq!(v["state_change"], json!(false));
        assert_eq!(v["workspace"], Value::Null);
    }

    #[test]
    fn notification_patch_only_updates_snoozed_till() {
        // Django hardcodes notification_data = {"snoozed_till": ...} (base.py:160).
        let body = serde_json::json!({"snoozed_till": "2026-09-09T00:00:00Z", "read_at": "2026-09-08T00:00:00Z"});
        assert_eq!(snoozed_till_from_body(&body), Some("2026-09-09T00:00:00Z".to_string()));
    }
}
