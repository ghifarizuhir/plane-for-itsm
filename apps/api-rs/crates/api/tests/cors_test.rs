use api::middleware::cors::{build_cors_layer, parse_cors_origins};
use axum::{body::Body, http::{Method, Request, StatusCode}, routing::get, Router};
use tower::ServiceExt as _;

#[test]
fn parse_splits_trims_and_skips_empty_and_invalid() {
    let out = parse_cors_origins("http://a:3000, http://b:3000 ,,  ,http://c:3000");
    let strs: Vec<String> = out
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    assert_eq!(strs, vec!["http://a:3000", "http://b:3000", "http://c:3000"]);

    // invalid entries are skipped with a warn, empties ignored
    let out = parse_cors_origins("http://ok:3000, not a valid origin \n\t, ,");
    let strs: Vec<String> = out
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    assert_eq!(strs, vec!["http://ok:3000"]);

    assert!(parse_cors_origins("").is_empty());
    assert!(parse_cors_origins("  , , ").is_empty());
}

fn test_router() -> Router {
    let origins = parse_cors_origins("http://x:3000, http://y:3000");
    let cors = build_cors_layer(origins);
    Router::new()
        .route("/api/instances/", get(|| async { "ok" }))
        .layer(cors)
}

#[tokio::test]
async fn get_returns_allow_origin_mirror() {
    let app = test_router();
    let req = Request::builder()
        .uri("/api/instances/")
        .header("Origin", "http://x:3000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("http://x:3000"),
        "GET with Origin must echo allow-origin"
    );
    assert_eq!(
        resp.headers()
            .get("access-control-allow-credentials")
            .and_then(|v| v.to_str().ok()),
        Some("true"),
        "cookie auth cross-port requires allow-credentials"
    );
}

#[tokio::test]
async fn preflight_returns_200_with_credentials() {
    let app = test_router();
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/instances/")
        .header("Origin", "http://x:3000")
        .header("Access-Control-Request-Method", "POST")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "preflight must be 200");
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("http://x:3000")
    );
    assert_eq!(
        resp.headers()
            .get("access-control-allow-credentials")
            .and_then(|v| v.to_str().ok()),
        Some("true")
    );
}
