use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt as _;

#[tokio::test]
async fn rejects_invalid_bearer() {
    let app = api::test_app().await;
    let req = Request::builder()
        .uri("/api/workspaces/")
        .header("Authorization", "Bearer bad")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "bad Bearer token must be 401"
    );
}

#[tokio::test]
async fn rejects_missing_auth() {
    let app = api::test_app().await;
    let req = Request::builder()
        .uri("/api/workspaces/")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "missing auth must be 401"
    );
}
