//! Fake OpenAI-compatible upstream for `routes::ai_agent::run_agent`:
//! no-tool answer, tool round-trip, 429, 500, malformed JSON. No DB, no env.

use axum::{http::StatusCode, routing::post, Json, Router};
use api::routes::ai::LlmError;
use api::routes::ai_agent::run_agent;
use rig::tool::server::ToolServer;
use serde_json::{json, Value};

fn chat_response(content: &str) -> Value {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 0,
        "model": "test",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }]
    })
}

async fn spawn_fixed(status: u16, body: Value) -> String {
    let handler = move || {
        let body = body.clone();
        async move { (StatusCode::from_u16(status).unwrap(), Json(body)) }
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/v1/chat/completions", post(handler)))
            .await
            .unwrap();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn prompt_without_tools_returns_content() {
    let base = spawn_fixed(200, chat_response("final answer")).await;
    let out = run_agent(&base, "key", "gpt-4o-mini", ToolServer::new().run(), "hi").await;
    assert_eq!(out, Ok("final answer".to_string()));
}
