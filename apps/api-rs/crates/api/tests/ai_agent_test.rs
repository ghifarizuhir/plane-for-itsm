//! Fake OpenAI-compatible upstream for `routes::ai_agent::run_agent`:
//! no-tool answer, tool round-trip, 429, 500, malformed JSON. No DB, no env.

use api::routes::ai_agent::run_agent;
use axum::{http::StatusCode, routing::post, Json, Router};
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
        axum::serve(
            listener,
            Router::new().route("/v1/chat/completions", post(handler)),
        )
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

mod tool_roundtrip {
    use std::sync::{Arc, Mutex};

    use api::routes::ai::LlmError;
    use api::routes::ai_agent::{new_trace, run_agent, ToolCallTrace, ToolTrace};
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use rig::tool::server::ToolServer;
    use rig::tool::{Tool, ToolContext, ToolExecutionError};
    use serde_json::{json, Value};

    #[derive(serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
    struct EchoArgs {
        text: String,
    }

    struct FakeEcho {
        trace: ToolTrace,
    }

    impl Tool for FakeEcho {
        const NAME: &'static str = "echo";
        type Args = EchoArgs;
        type Output = String;
        type Error = ToolExecutionError;

        fn description(&self) -> String {
            "Echo the text back".to_string()
        }

        fn parameters(&self) -> Value {
            serde_json::to_value(schemars::schema_for!(EchoArgs)).unwrap()
        }

        async fn call(
            &self,
            _context: &mut ToolContext,
            args: Self::Args,
        ) -> Result<Self::Output, Self::Error> {
            self.trace.lock().unwrap().push(ToolCallTrace {
                name: Self::NAME.to_string(),
                arguments: serde_json::to_value(&args).unwrap(),
            });
            Ok(format!("echo:{}", args.text))
        }
    }

    #[derive(Default)]
    struct Upstream {
        calls: Mutex<usize>,
        bodies: Mutex<Vec<Value>>,
    }

    type Shared = Arc<Upstream>;

    async fn handler(
        State(state): State<Shared>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let n = {
            let mut calls = state.calls.lock().unwrap();
            let n = *calls;
            *calls += 1;
            n
        };
        state.bodies.lock().unwrap().push(body);
        if n == 0 {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "1", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": {"name": "echo", "arguments": "{\"text\":\"hi\"}"}
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }]
                })),
            )
        } else {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "2", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "final answer"},
                        "finish_reason": "stop"
                    }]
                })),
            )
        }
    }

    async fn spawn_roundtrip() -> (String, Shared) {
        let state: Shared = Arc::new(Upstream::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/v1"), state)
    }

    #[tokio::test]
    async fn tool_call_roundtrip_records_trace_and_returns_final_text() {
        let (base, upstream) = spawn_roundtrip().await;
        let trace = new_trace();
        let tool = FakeEcho {
            trace: trace.clone(),
        };
        let out = run_agent(
            &base,
            "key",
            "model",
            ToolServer::new().tool(tool).run(),
            "say hi",
        )
        .await;
        assert_eq!(out, Ok("final answer".to_string()));

        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "echo");
        assert_eq!(recorded[0].arguments["text"], json!("hi"));

        let bodies = upstream.bodies.lock().unwrap();
        assert_eq!(
            bodies.len(),
            2,
            "tool loop must issue a second upstream call"
        );
        let messages = bodies[1]["messages"].as_array().unwrap();
        let tool_msg = messages
            .iter()
            .find(|m| m["role"] == json!("tool"))
            .expect("second call must carry the tool result message");
        assert_eq!(tool_msg["content"], json!("echo:hi"));
    }

    #[tokio::test]
    async fn upstream_429_maps_to_rate_limited() {
        let base = super::spawn_fixed(429, json!({"error": {"message": "slow down"}})).await;
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), "hi").await;
        assert_eq!(out, Err(LlmError::RateLimited));
    }

    #[tokio::test]
    async fn upstream_500_maps_to_upstream() {
        let base = super::spawn_fixed(500, json!({"error": "boom"})).await;
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), "hi").await;
        assert_eq!(out, Err(LlmError::Upstream));
    }

    #[tokio::test]
    async fn malformed_json_maps_to_upstream() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
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
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{addr}/v1");
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), "hi").await;
        assert_eq!(out, Err(LlmError::Upstream));
    }
}
