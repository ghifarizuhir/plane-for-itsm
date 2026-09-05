pub mod middleware;
pub mod routes;
pub mod state;

use axum::{routing::get, Router};

/// Minimal test app: health (public) + workspaces (auth-protected stub).
/// State nyata (lazy pool + lazy redis client + config env) agar
/// `FromRequestParts<AppState>` terpenuhi; `connect_lazy` + `Client::open`
/// tidak membuka koneksi sehingga test 401 tetap murni unit.
pub async fn test_app() -> Router {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://plane:plane@127.0.0.1:5432/plane_test")
        .expect("lazy pool");
    let state = crate::state::AppState {
        pool,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: common::config::AppConfig::from_env(),
    };
    Router::new()
        .route("/health", get(routes::health::health))
        .route(
            "/api/workspaces/",
            get(stub_workspaces_list).post(stub_workspaces_list),
        )
        .with_state(state)
}

async fn stub_workspaces_list(
    _auth: middleware::auth::AuthUser,
) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        axum::http::StatusCode::OK,
        axum::Json(serde_json::json!([])),
    )
}
