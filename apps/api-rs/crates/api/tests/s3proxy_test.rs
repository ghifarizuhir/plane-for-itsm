use api::routes::s3proxy::proxy_to_minio;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
    Router,
};
use tower::ServiceExt as _;

fn app() -> Router {
    Router::new().route("/:bucket/*rest", get(proxy_to_minio).post(proxy_to_minio))
}

#[tokio::test]
async fn wrong_bucket_returns_django_identical_404() {
    let req = Request::builder()
        .uri("/not-a-bucket/x")
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"error": "Page not found."})
    );
}
