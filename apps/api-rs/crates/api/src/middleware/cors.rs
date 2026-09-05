use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

/// Parse Django-style comma list `CORS_ALLOWED_ORIGINS` into HeaderValues.
/// Split ',', trim, skip empties; skip invalid entries with a warn.
/// Valid = parseable http/https URI with authority AND valid HeaderValue
/// (HeaderValue alone accepts spaces, so URI check rejects junk like
/// "not a valid origin").
pub fn parse_cors_origins(raw: &str) -> Vec<HeaderValue> {
    raw.split(',')
        .filter_map(|part| {
            let t = part.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            if t.is_empty() {
                return None;
            }
            let valid_uri = t
                .parse::<axum::http::Uri>()
                .ok()
                .filter(|uri| {
                    matches!(uri.scheme_str(), Some("http") | Some("https"))
                        && uri.authority().is_some()
                })
                .is_some();
            if !valid_uri {
                tracing::warn!(origin = %t, "skipping invalid CORS origin");
                return None;
            }
            match t.parse::<HeaderValue>() {
                Ok(v) => Some(v),
                Err(_) => {
                    tracing::warn!(origin = %t, "skipping invalid CORS origin");
                    None
                }
            }
        })
        .collect()
}

/// Shared builder so main + tests use the identical layer config.
/// NOTE: `AllowHeaders::any()` ("*") panics with `allow_credentials(true)`
/// on tower-http 0.5 (`ensure_usable_cors_rules`), so we use
/// `mirror_request()` — the credentialed equivalent of "allow any headers".
pub fn build_cors_layer(origins: Vec<HeaderValue>) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::mirror_request())
}

/// Env-aware constructor: `CORS_ALLOWED_ORIGINS`, else single `frontend_url`.
pub fn cors_layer_from_env(frontend_url: &str) -> CorsLayer {
    let raw = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
    let mut origins = parse_cors_origins(&raw);
    if origins.is_empty() {
        origins = parse_cors_origins(frontend_url);
    }
    build_cors_layer(origins)
}
