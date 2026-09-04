#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod middleware;
mod routes;
mod state;

use axum::{routing::{get, post}, Router};
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
        .route(
            "/api/workspaces/",
            get(routes::workspace::list).post(routes::workspace::create),
        )
        .route(
            "/api/workspaces/:slug/projects/",
            get(routes::project::list).post(routes::project::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/",
            get(routes::issue::list).post(routes::issue::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/cycles/",
            get(routes::cycle::list).post(routes::cycle::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/",
            get(routes::module::list).post(routes::module::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/states/",
            get(routes::state::list).post(routes::state::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/labels/",
            get(routes::label::list).post(routes::label::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/",
            get(routes::estimate::list).post(routes::estimate::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/estimate-points/",
            post(routes::estimate::create_point),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intakes/",
            get(routes::intake::list).post(routes::intake::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inboxes/",
            get(routes::intake::list).post(routes::intake::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/members/",
            get(routes::member::list).post(routes::member::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members/",
            get(routes::member::list).post(routes::member::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members-lite/",
            get(routes::member::list_lite),
        )
        .route("/api/workspaces/:slug/members/", get(routes::member::list_workspace_members))
        .route("/api/workspaces/:slug/members-lite/", get(routes::member::list_workspace_members))
        .route(
            "/api/workspaces/:slug/invitations/",
            get(routes::member::list_invites).post(routes::member::create_invite),
        )
        .with_state(state::AppState { pool, redis })
        .layer(tower_http::limit::RequestBodyLimitLayer::new(5 * 1024 * 1024))
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
