use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/views/notification/base.py` for
/// `plane/app/urls/notification.py`: list (receiver-scoped), unread counts,
/// read/unread + archive/unarchive toggles, mark-all-read (with
/// snoozed/archived/type variants), and notification-preference GET/PATCH.
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

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let receiver = auth.0;
    let rows = sqlx::query_as::<_, common::models::notification::Notification>(
        "SELECT n.id, n.title, n.read_at, n.archived_at FROM notifications n JOIN workspaces w ON w.id = n.workspace_id WHERE w.slug = $1 AND n.receiver_id = $2 AND n.deleted_at IS NULL ORDER BY n.created_at DESC",
    )
    .bind(&slug)
    .bind(receiver)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|n| json!({"id": n.id, "title": n.title, "read_at": n.read_at, "archived_at": n.archived_at}))
            .collect(),
    ))
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
    axum::extract::Path((_slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &_slug, receiver, pk, "read_at", true).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn mark_unread(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &_slug, receiver, pk, "read_at", false).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &_slug, receiver, pk, "archived_at", true).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let receiver = auth.0;
    if !toggle(&st, &_slug, receiver, pk, "archived_at", false).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Notification not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn get_preferences(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, common::errors::AppError> {
    let receiver = auth.0;
    let row: Option<(bool, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT property_change, state_change, comment, mention, issue_completed FROM user_notification_preferences WHERE user_id = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(receiver)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((property_change, state_change, comment, mention, issue_completed)) => Ok(Json(json!({
            "property_change": property_change, "state_change": state_change,
            "comment": comment, "mention": mention, "issue_completed": issue_completed,
        }))),
        None => Ok(Json(json!({
            "property_change": true, "state_change": true,
            "comment": true, "mention": true, "issue_completed": true,
        }))),
    }
}

pub async fn patch_preferences(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<HashMap<String, Value>>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_preference_patch(&body).map_err(|e| anyhow::anyhow!(e))?;
    let receiver = auth.0;
    // Upsert one row per user; only known keys can appear (validated above).
    sqlx::query(
        "INSERT INTO user_notification_preferences (id, user_id, property_change, state_change, comment, mention, issue_completed, created_at, updated_at) VALUES (gen_random_uuid(), $1, true, true, true, true, true, now(), now()) ON CONFLICT DO NOTHING",
    )
    .bind(receiver)
    .execute(&st.pool)
    .await?;
    for (key, value) in &body {
        let sql = format!("UPDATE user_notification_preferences SET {key} = $1 WHERE user_id = $2 AND deleted_at IS NULL");
        sqlx::query(&sql).bind(value.as_bool().unwrap_or(true)).bind(receiver).execute(&st.pool).await?;
    }
    Ok((StatusCode::OK, Json(json!({"message": "Successful"}))))
}
