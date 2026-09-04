#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod routes;
mod state;

use axum::{routing::get, Router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    let redis = common::redis::create_redis(&cfg.redis_url).await;

    let app = Router::new()
        .route("/health", get(routes::health::health))
        .with_state(state::AppState { pool, redis })
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", cfg.port))
        .await
        .unwrap();
    tracing::info!("rust-api listening on {}", cfg.port);
    axum::serve(listener, app).await.unwrap();
}

// helper for tests
pub async fn test_app() -> Router {
    Router::new().route("/health", get(routes::health::health))
}
