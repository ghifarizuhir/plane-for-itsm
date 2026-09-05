use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};

use crate::state::AppState;

/// Authenticated user identity (UUID), resolved in order:
/// 1. `Authorization: Bearer <jwt>` (verified with `config.jwt_secret`)
/// 2. `plane_at` / `__Host-plane_at` access cookie
/// 3. `X-Api-Key` (DB lookup on `api_tokens`: active + not deleted + not expired)
#[derive(Debug, Clone)]
pub struct AuthUser(pub uuid::Uuid);

#[async_trait::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let unauthorized = || (StatusCode::UNAUTHORIZED, "missing or invalid auth").into_response();
        // 1. Bearer JWT
        if let Some(tok) = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
        {
            if let Ok(uid) = common::auth::decode_access(tok.trim(), &state.config.jwt_secret) {
                return Ok(AuthUser(uid));
            }
        }
        // 2. Cookie access
        if let Some(uid) = cookie_uid(parts, state) {
            return Ok(AuthUser(uid));
        }
        // 3. X-Api-Key (DB lookup + aktif + belum expired)
        if let Some(key) = parts.headers.get("X-Api-Key").and_then(|v| v.to_str().ok()) {
            let row: Option<(uuid::Uuid,)> = sqlx::query_as(
                "SELECT user_id FROM api_tokens WHERE token = $1 AND is_active = true AND deleted_at IS NULL AND (expired_at IS NULL OR expired_at > now())",
            )
            .bind(key.trim())
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| unauthorized())?;
            if let Some((uid,)) = row {
                return Ok(AuthUser(uid));
            }
        }
        Err(unauthorized())
    }
}

fn cookie_uid(parts: &Parts, state: &AppState) -> Option<uuid::Uuid> {
    let cookies = parts.headers.get("cookie")?.to_str().ok()?;
    for pair in cookies.split(';') {
        let (k, v) = pair.trim().split_once('=')?;
        if k == "plane_at" || k == "__Host-plane_at" {
            if let Ok(uid) = common::auth::decode_access(v.trim(), &state.config.jwt_secret) {
                return Some(uid);
            }
        }
    }
    None
}
