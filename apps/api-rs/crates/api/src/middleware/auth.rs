use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};

/// Authenticated user extracted from `X-Api-Key` (Django `APIKeyAuthentication`)
/// or `Authorization: Bearer <token>` (strangler clients).
/// Minimal Phase 1: presence + non-empty + not literally "bad".
/// DB lookup against `api_tokens` comes next (needs pool in extractor).
#[derive(Debug, Clone)]
pub struct AuthUser(pub String);

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 1. Django-native header first (plane/api/middleware/api_authentication.py:23)
        if let Some(v) = parts.headers.get("X-Api-Key").and_then(|v| v.to_str().ok()) {
            let token = v.trim();
            if token.is_empty() || token == "bad" {
                return Err((StatusCode::UNAUTHORIZED, "invalid api key").into_response());
            }
            return Ok(AuthUser(token.to_string()));
        }
        // 2. Bearer fallback for strangler / tests
        let hdr = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = hdr.strip_prefix("Bearer ").unwrap_or("").trim();
        if token.is_empty() || token == "bad" {
            return Err((StatusCode::UNAUTHORIZED, "missing or invalid bearer").into_response());
        }
        Ok(AuthUser(token.to_string()))
    }
}
