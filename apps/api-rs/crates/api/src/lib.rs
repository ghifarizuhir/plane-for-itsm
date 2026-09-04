pub mod middleware;
pub mod routes;
pub mod state;

use axum::{routing::get, Router};

/// Minimal test app: health (public) + workspaces (auth-protected stub).
/// Workspace list/create real DB logic lands in Task 2.2.
pub async fn test_app() -> Router {
    Router::new()
        .route("/health", get(routes::health::health))
        .route(
            "/api/workspaces/",
            get(stub_workspaces_list).post(stub_workspaces_list),
        )
}

async fn stub_workspaces_list(
    _auth: middleware::auth::AuthUser,
) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        axum::http::StatusCode::OK,
        axum::Json(serde_json::json!([])),
    )
}
