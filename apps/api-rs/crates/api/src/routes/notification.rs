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
}
