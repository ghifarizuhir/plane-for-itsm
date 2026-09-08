use api::routes::s3proxy::{proxy_to_minio, proxy_to_minio_root};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{delete, get, head, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt as _;

async fn fallback_404() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "Page not found."})),
    )
}

fn app() -> Router {
    Router::new()
        .route(
            "/:bucket/*rest",
            get(proxy_to_minio)
                .post(proxy_to_minio)
                .put(proxy_to_minio)
                .delete(proxy_to_minio)
                .head(proxy_to_minio),
        )
        .route(
            "/:bucket",
            get(proxy_to_minio_root)
                .post(proxy_to_minio_root)
                .put(proxy_to_minio_root)
                .delete(proxy_to_minio_root)
                .head(proxy_to_minio_root),
        )
        .route(
            "/:bucket/",
            get(proxy_to_minio_root)
                .post(proxy_to_minio_root)
                .put(proxy_to_minio_root)
                .delete(proxy_to_minio_root)
                .head(proxy_to_minio_root),
        )
        .fallback(fallback_404)
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

#[tokio::test]
async fn bucket_root_post_reaches_handler_not_fallback() {
    // Presigned POSTs target `http://<host>/uploads/` (empty rest). In this
    // sandbox MinIO is unreachable, so reaching the handler yields 502 —
    // proving the request did NOT fall through to the app JSON 404.
    let req = Request::builder()
        .method("POST")
        .uri("/uploads/")
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"error": "Object storage unreachable."})
    );
}

#[tokio::test]
async fn bucket_root_wrong_bucket_still_404() {
    let req = Request::builder()
        .uri("/wrong/")
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
