use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Pure check (unit-testable). GET/HEAD/OPTIONS selalu lolos; mutasi wajib
/// Origin cocok dengan SALAH SATU frontend yang diizinkan, fallback Referer
/// prefix-cocok terhadap salah satu. Bentuk list agar web (:3000) + admin
/// (:3001) + space lolos bersamaan — single-origin menolak admin dengan
/// 403 {"error":"bad origin"}.
pub fn origin_allowed_many(
    method: &Method,
    headers: &axum::http::HeaderMap,
    frontends: &[String],
) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    if let Some(o) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        return frontends.iter().any(|f| o == f);
    }
    if let Some(r) = headers.get("referer").and_then(|v| v.to_str().ok()) {
        return frontends.iter().any(|f| r.starts_with(f));
    }
    false
}

/// Single-origin wrapper (backward compat for existing callers/tests).
pub fn origin_allowed(method: &Method, headers: &axum::http::HeaderMap, frontend: &str) -> bool {
    origin_allowed_many(method, headers, std::slice::from_ref(&frontend.to_string()))
}

/// Build daftar origin yang diizinkan: `FRONTEND_URL` + `CORS_ALLOWED_ORIGINS`
/// (comma list, format Django) + `ADMIN_BASE_URL`/`APP_BASE_URL`/`WEB_URL`.
/// Normalisasi: trim, strip quotes, trim trailing `/`; kosong dilewati; duplikat
/// dibuang dengan menjaga urutan.
pub fn build_allowed_origins(
    frontend_url: &str,
    cors_allowed_origins: &str,
    extra: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let t = raw
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .trim()
            .trim_end_matches('/')
            .to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    push(frontend_url);
    for part in cors_allowed_origins.split(',') {
        push(part);
    }
    for e in extra {
        push(e);
    }
    out
}

/// Env-aware constructor: baca `FRONTEND_URL` (+ default), `CORS_ALLOWED_ORIGINS`,
/// `ADMIN_BASE_URL`, `APP_BASE_URL`, `WEB_URL`. Selalu non-empty (fallback
/// `FRONTEND_URL`/default) agar middleware tak memblokir semua mutasi.
pub fn allowed_origins_from_env(frontend_url: &str) -> Vec<String> {
    let cors = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
    let extra: Vec<String> = ["ADMIN_BASE_URL", "APP_BASE_URL", "WEB_URL"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .collect();
    let list = build_allowed_origins(frontend_url, &cors, &extra);
    if list.is_empty() {
        vec![frontend_url.trim_end_matches('/').to_string()]
    } else {
        list
    }
}

pub async fn origin_middleware(
    axum::extract::State(frontends): axum::extract::State<Vec<String>>,
    req: Request,
    next: Next,
) -> Response {
    if !origin_allowed_many(req.method(), req.headers(), &frontends) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({"error": "bad origin"}))).into_response();
    }
    next.run(req).await
}
