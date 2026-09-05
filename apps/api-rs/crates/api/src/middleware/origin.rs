use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Pure check (unit-testable). GET/HEAD/OPTIONS selalu lolos; mutasi wajib
/// Origin cocok, fallback Referer prefix-cocok.
pub fn origin_allowed(method: &Method, headers: &axum::http::HeaderMap, frontend: &str) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    if let Some(o) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        return o == frontend;
    }
    if let Some(r) = headers.get("referer").and_then(|v| v.to_str().ok()) {
        return r.starts_with(frontend);
    }
    false
}

pub async fn origin_middleware(
    axum::extract::State(frontend): axum::extract::State<String>,
    req: Request,
    next: Next,
) -> Response {
    if !origin_allowed(req.method(), req.headers(), &frontend) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({"error": "bad origin"}))).into_response();
    }
    next.run(req).await
}
