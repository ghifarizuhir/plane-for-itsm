//! Shared test helpers for the API integration tests.

use serde_json::Value;

/// Fake OpenAI-compatible upstream that records every request body.
pub async fn spawn_recording_upstream(
    answer: &str,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::{routing::post, Json, Router};
    let bodies: std::sync::Arc<std::sync::Mutex<Vec<Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = bodies.clone();
    let answer = answer.to_string();
    async fn handler(
        axum::extract::State((recorder, answer)): axum::extract::State<(
            std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
            String,
        )>,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        recorder.lock().unwrap().push(body);
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": answer},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handler))
                .with_state((recorder, answer)),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}/v1"), bodies)
}
