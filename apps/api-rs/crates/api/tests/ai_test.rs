//! Fake OpenAI-compatible upstream for `routes::ai::chat_completion`:
//! success, 429, 5xx, malformed JSON, empty content. No DB, no env.

use api::routes::ai::{chat_completion, LlmError};
use axum::{
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

fn fake_upstream(status: u16, body: Value) -> Router {
    let handler = move || {
        let body = body.clone();
        async move {
            let code = StatusCode::from_u16(status).unwrap();
            (code, Json(body))
        }
    };
    Router::new().route("/v1/chat/completions", post(handler))
}

async fn spawn(status: u16, body: Value) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, fake_upstream(status, body)).await.unwrap();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn success_returns_content() {
    let base = spawn(200, json!({"choices": [{"message": {"content": "hello"}}]})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Ok("hello".to_string()));
}

#[tokio::test]
async fn empty_content_is_ok_empty_string() {
    let base = spawn(200, json!({"choices": []})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Ok(String::new()));
}

#[tokio::test]
async fn upstream_429_maps_to_rate_limited() {
    let base = spawn(429, json!({"error": "slow down"})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Err(LlmError::RateLimited));
}

#[tokio::test]
async fn upstream_500_maps_to_upstream_error() {
    let base = spawn(500, json!({"error": "boom"})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Err(LlmError::Upstream));
}

#[tokio::test]
async fn malformed_json_maps_to_upstream_error() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(axum::body::Body::from("not-json"))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{addr}/v1");
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Err(LlmError::Upstream));
}
