use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/user.py:UserSerializer`
/// validate_first_name / validate_last_name (no URL).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateUser {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeOut {
    pub id: uuid::Uuid,
    pub email: Option<String>,
    pub first_name: String,
    pub last_name: String,
}

fn contains_url(value: &str) -> bool {
    if value.len() > 1000 {
        return false;
    }
    let lower = value.to_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

pub fn validate_update(body: &UpdateUser) -> Result<(), String> {
    if let Some(first) = &body.first_name {
        if contains_url(first) {
            return Err("First name cannot contain a URL.".to_string());
        }
    }
    if let Some(last) = &body.last_name {
        if contains_url(last) {
            return Err("Last name cannot contain a URL.".to_string());
        }
    }
    Ok(())
}

/// Mirrors `plane/app/views/user/base.py:_validate_new_email`.
pub fn validate_new_email(new_email: &str) -> Result<(), String> {
    if new_email.trim().is_empty() {
        return Err("Email is required".to_string());
    }
    let parts: Vec<&str> = new_email.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || !parts[1].contains('.') || parts[1].starts_with('.') {
        return Err("Invalid email format".to_string());
    }
    Ok(())
}

/// Deprecated: AuthUser kini membawa UUID user langsung (`auth.0`).
/// Dipertahankan hingga Task 6 selesai — jangan hapus dulu.
#[allow(dead_code)]
async fn user_id(st: &AppState, auth: &AuthUser) -> Option<uuid::Uuid> {
    sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE token = $1")
        .bind(&auth.0)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
}

async fn me_row(st: &AppState, uid: uuid::Uuid) -> Option<MeOut> {
    let row: Option<(uuid::Uuid, Option<String>, String, String)> =
        sqlx::query_as("SELECT id, email, first_name, last_name FROM users WHERE id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .ok()?;
    row.map(|(id, email, first_name, last_name)| MeOut { id, email, first_name, last_name })
}

/// GET /api/users/me/ — mirrors `UserEndpoint.retrieve` (UserMeSerializer).
pub async fn me(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    match me_row(&st, uid).await {
        Some(u) => Ok((StatusCode::OK, Json(json!({"id": u.id, "email": u.email, "first_name": u.first_name, "last_name": u.last_name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "User not found"})))),
    }
}

/// PATCH /api/users/me/ — validates names then updates.
pub async fn patch_me(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateUser>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_update(&body).map_err(|e| anyhow::anyhow!(e))?;
    let uid = auth.0;
    sqlx::query("UPDATE users SET first_name = COALESCE($1, first_name), last_name = COALESCE($2, last_name), updated_at = now() WHERE id = $3")
        .bind(&body.first_name)
        .bind(&body.last_name)
        .bind(uid)
        .execute(&st.pool)
        .await?;
    match me_row(&st, uid).await {
        Some(u) => Ok((StatusCode::OK, Json(json!({"id": u.id, "email": u.email, "first_name": u.first_name, "last_name": u.last_name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "User not found"})))),
    }
}

/// GET /api/users/me/settings/ — mirrors `retrieve_user_settings`.
pub async fn settings(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT theme FROM profiles WHERE user_id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await?;
    match row {
        Some((theme,)) => Ok((StatusCode::OK, Json(json!({"theme": theme})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Profile not found"})))),
    }
}

/// GET /api/users/me/instance-admin/ — mirrors `retrieve_instance_admin`.
pub async fn instance_admin(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let is_admin: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM instance_admins WHERE user_id = $1 LIMIT 1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await?;
    Ok((StatusCode::OK, Json(json!({"is_instance_admin": is_admin.is_some()}))))
}

/// GET /api/users/session/ — mirrors `UserSessionEndpoint.get`.
pub async fn session(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match me_row(&st, auth.0).await {
        Some(u) => Ok((
            StatusCode::OK,
            Json(json!({"is_authenticated": true, "user": {"id": u.id, "email": u.email}})),
        )),
        None => Ok((StatusCode::OK, Json(json!({"is_authenticated": false})))),
    }
}

/// PATCH /api/users/me/onboard/ — mirrors `UpdateUserOnBoardedEndpoint`.
pub async fn onboard(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let flag = body.get("is_onboarded").and_then(|v| v.as_bool()).unwrap_or(false);
    sqlx::query("UPDATE profiles SET is_onboarded = $1, updated_at = now() WHERE user_id = $2")
        .bind(flag)
        .bind(uid)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::OK, Json(json!({"message": "Updated successfully"}))))
}

/// PATCH /api/users/me/tour-completed/ — mirrors `UpdateUserTourCompletedEndpoint`.
pub async fn tour_completed(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let flag = body.get("is_tour_completed").and_then(|v| v.as_bool()).unwrap_or(false);
    sqlx::query("UPDATE profiles SET is_tour_completed = $1, updated_at = now() WHERE user_id = $2")
        .bind(flag)
        .bind(uid)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::OK, Json(json!({"message": "Updated successfully"}))))
}

/// GET /api/users/me/activities/ — latest actor activities (simple list).
pub async fn activities(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let rows: Vec<(uuid::Uuid, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as("SELECT id, created_at FROM issue_activities WHERE actor_id = $1 ORDER BY created_at DESC LIMIT 50")
            .bind(uid)
            .fetch_all(&st.pool)
            .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.into_iter().map(|(id, created_at)| json!({"id": id, "created_at": created_at})).collect::<Vec<_>>())),
    ))
}

/// GET /api/users/me/accounts/ (+/<pk>/) and DELETE — mirrors `AccountEndpoint`.
pub async fn list_accounts(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let rows: Vec<(uuid::Uuid, String)> =
        sqlx::query_as("SELECT id, provider FROM accounts WHERE user_id = $1")
            .bind(uid)
            .fetch_all(&st.pool)
            .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.into_iter().map(|(id, provider)| json!({"id": id, "provider": provider})).collect::<Vec<_>>())),
    ))
}

pub async fn get_account(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(pk): axum::extract::Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let row: Option<(uuid::Uuid, String)> =
        sqlx::query_as("SELECT id, provider FROM accounts WHERE id = $1 AND user_id = $2")
            .bind(pk)
            .bind(uid)
            .fetch_optional(&st.pool)
            .await?;
    match row {
        Some((id, provider)) => Ok((StatusCode::OK, Json(json!({"id": id, "provider": provider})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Account not found"})))),
    }
}

pub async fn delete_account(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(pk): axum::extract::Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    sqlx::query("DELETE FROM accounts WHERE id = $1 AND user_id = $2")
        .bind(pk)
        .bind(uid)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// GET/PATCH /api/users/me/profile/ — mirrors `ProfileEndpoint`.
pub async fn profile(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    let row: Option<(uuid::Uuid, serde_json::Value, bool, bool)> =
        sqlx::query_as("SELECT id, theme, is_onboarded, is_tour_completed FROM profiles WHERE user_id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await?;
    match row {
        Some((id, theme, onboarded, tour)) => Ok((
            StatusCode::OK,
            Json(json!({"id": id, "theme": theme, "is_onboarded": onboarded, "is_tour_completed": tour})),
        )),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Profile not found"})))),
    }
}

pub async fn patch_profile(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let uid = auth.0;
    if let Some(theme) = body.get("theme") {
        sqlx::query("UPDATE profiles SET theme = $1, updated_at = now() WHERE user_id = $2")
            .bind(theme)
            .bind(uid)
            .execute(&st.pool)
            .await?;
    }
    profile(State(st), auth).await
}
