#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod middleware;
mod routes;
mod state;

use axum::{middleware as axum_middleware, routing::{get, patch, post}, Router};

use crate::middleware::rate_limit::{ip_rate_limit_middleware, rate_limit_middleware, IpRateLimiter, RateLimiter};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = common::config::AppConfig::from_env();
    let pool = common::db::create_pool(&cfg).await;
    if let Err(e) = common::db::migrate(&pool).await {
        tracing::warn!(error=%e, "migrate failed");
    }
    let redis = redis::Client::open(cfg.redis_url.as_str()).expect("redis client open failed");

    let app = Router::new()
        .route("/health", get(routes::health::health))
        .route(
            "/api/workspaces/",
            get(routes::workspace::list).post(routes::workspace::create),
        )
        .route(
            "/api/workspaces/:slug/",
            get(routes::workspace::detail)
                .patch(routes::workspace::patch)
                .delete(routes::workspace::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/",
            get(routes::project::list).post(routes::project::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:pk/",
            get(routes::project::detail)
                .patch(routes::project::patch)
                .delete(routes::project::destroy),
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
            "/api/workspaces/:slug/projects/:project_id/cycles/:pk/",
            get(routes::cycle::detail)
                .patch(routes::cycle::patch)
                .delete(routes::cycle::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/",
            get(routes::module::list).post(routes::module::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/modules/:pk/",
            get(routes::module::detail)
                .patch(routes::module::patch)
                .delete(routes::module::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/states/",
            get(routes::state::list).post(routes::state::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/states/:pk/",
            get(routes::state::detail)
                .patch(routes::state::patch)
                .delete(routes::state::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/labels/",
            get(routes::label::list).post(routes::label::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/labels/:pk/",
            get(routes::label::detail)
                .patch(routes::label::patch)
                .delete(routes::label::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issue-labels/:pk/",
            get(routes::label::detail)
                .patch(routes::label::patch)
                .delete(routes::label::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/",
            get(routes::estimate::list).post(routes::estimate::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/",
            get(routes::estimate::detail)
                .patch(routes::estimate::patch)
                .delete(routes::estimate::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/estimate-points/",
            post(routes::estimate::create_point),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/estimates/:estimate_id/estimate-points/:point_id/",
            get(routes::estimate::detail)
                .patch(routes::estimate::patch_point)
                .delete(routes::estimate::destroy_point),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intakes/",
            get(routes::intake::list).post(routes::intake::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intakes/:pk/",
            get(routes::intake::detail)
                .patch(routes::intake::patch)
                .delete(routes::intake::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inboxes/",
            get(routes::intake::list).post(routes::intake::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inboxes/:pk/",
            get(routes::intake::detail)
                .patch(routes::intake::patch)
                .delete(routes::intake::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/",
            get(routes::intake::detail_issue).delete(routes::intake::destroy_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/",
            get(routes::intake::list_issues).post(routes::intake::create_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/",
            get(routes::intake::detail_issue).delete(routes::intake::destroy_issue),
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
        .route(
            "/api/workspaces/:slug/projects/:project_id/views/",
            get(routes::view::list).post(routes::view::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/views/:pk/",
            get(routes::view::detail)
                .patch(routes::view::patch)
                .delete(routes::view::destroy),
        )
        .route(
            "/api/workspaces/:slug/views/",
            get(routes::view::list_global).post(routes::view::create_global),
        )
        .route(
            "/api/workspaces/:slug/views/:pk/",
            get(routes::view::detail_global),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/members/:pk/",
            get(routes::member::detail)
                .patch(routes::member::patch)
                .delete(routes::member::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/project-members/:pk/",
            get(routes::member::detail)
                .patch(routes::member::patch)
                .delete(routes::member::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/user-favorite-views/",
            get(routes::view::list_favorites).post(routes::view::create_favorite),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/",
            get(routes::page::list).post(routes::page::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages/:page_id/",
            get(routes::page::detail)
                .patch(routes::page::patch)
                .delete(routes::page::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/pages-summary/",
            get(routes::page::summary),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/favorite-pages/:page_id/",
            post(routes::page::create_favorite),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/check/:asset_id/",
            get(routes::asset::check),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/restore/:asset_id/",
            post(routes::asset::restore),
        )
        .route(
            "/api/assets/v2/workspaces/:slug/:asset_id/",
            axum::routing::patch(routes::asset::mark_uploaded).delete(routes::asset::soft_delete),
        )
        .route(
            "/api/workspaces/:slug/webhooks/",
            get(routes::webhook::list).post(routes::webhook::create),
        )
        .route(
            "/api/workspaces/:slug/webhooks/:pk/",
            get(routes::webhook::detail)
                .patch(routes::webhook::patch)
                .delete(routes::webhook::destroy),
        )
        .route(
            "/api/workspaces/:slug/webhooks/:pk/regenerate/",
            post(routes::webhook::regenerate),
        )
        .route("/api/workspaces/:slug/webhook-logs/:webhook_id/", get(routes::webhook::list_logs))
        .route("/api/workspaces/:slug/users/notifications/", get(routes::notification::list))
        .route("/api/workspaces/:slug/users/notifications/unread/", get(routes::notification::unread))
        .route(
            "/api/workspaces/:slug/users/notifications/mark-all-read/",
            post(routes::notification::mark_all_read),
        )
        .route(
            "/api/workspaces/:slug/users/notifications/:pk/read/",
            post(routes::notification::mark_read).delete(routes::notification::mark_unread),
        )
        .route(
            "/api/workspaces/:slug/users/notifications/:pk/archive/",
            post(routes::notification::archive).delete(routes::notification::unarchive),
        )
        .route(
            "/api/users/me/notification-preferences/",
            get(routes::notification::get_preferences).patch(routes::notification::patch_preferences),
        )
        .route("/api/workspaces/:slug/search/", get(routes::search::global_search))
        .route(
            "/api/workspaces/:slug/projects/:project_id/search-issues/",
            get(routes::search::issue_search),
        )
        .route("/api/workspaces/:slug/entity-search/", get(routes::search::entity_search))
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:pk/",
            get(routes::work_item::get_issue)
                .patch(routes::work_item::patch_issue)
                .delete(routes::work_item::delete_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/",
            get(routes::work_item::list_comments).post(routes::work_item::create_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/comments/:pk/",
            get(routes::work_item::get_comment)
                .patch(routes::work_item::patch_comment)
                .delete(routes::work_item::delete_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/links/",
            get(routes::work_item::list_links).post(routes::work_item::create_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/links/:pk/",
            get(routes::work_item::get_link)
                .patch(routes::work_item::patch_link)
                .delete(routes::work_item::delete_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/relations/",
            get(routes::work_item::list_relations).post(routes::work_item::create_relations),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/activities/",
            get(routes::work_item::list_activities),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/issues/:issue_id/activities/:pk/",
            get(routes::work_item::get_activity),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/",
            get(routes::issue::list).post(routes::issue::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:pk/",
            get(routes::work_item::get_issue)
                .patch(routes::work_item::patch_issue)
                .delete(routes::work_item::delete_issue),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/",
            get(routes::work_item::list_comments).post(routes::work_item::create_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/:pk/",
            get(routes::work_item::get_comment)
                .patch(routes::work_item::patch_comment)
                .delete(routes::work_item::delete_comment),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/",
            get(routes::work_item::list_links).post(routes::work_item::create_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/:pk/",
            get(routes::work_item::get_link)
                .patch(routes::work_item::patch_link)
                .delete(routes::work_item::delete_link),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/relations/",
            get(routes::work_item::list_relations).post(routes::work_item::create_relations),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/",
            get(routes::work_item::list_activities),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/:pk/",
            get(routes::work_item::get_activity),
        )
        .route("/api/workspaces/:slug/work-items/search/", get(routes::work_item::workspace_issue_search))
        .route("/api/workspaces/:slug/work-items/:ident/", get(routes::work_item::get_by_identifier))
        .route("/api/timezones/", get(routes::misc::timezones))
        .route("/api/instances/", get(routes::instance::get))
        .route("/api/auth/refresh/", post(routes::auth::refresh))
        .route("/api/auth/logout/", post(routes::auth::logout))
        // Tanpa throttle selaras Django (`ChangePasswordEndpoint` tanpa throttle_classes).
        .route("/auth/change-password/", post(routes::auth_compat::change_password))
        .route("/auth/set-password/", post(routes::auth_compat::set_password))
        .route("/auth/get-csrf-token/", get(routes::auth_compat::csrf_token))
        .route("/api/auth/oauth/:provider/start/", get(routes::auth::oauth_start))
        .route(
            "/api/workspaces/:slug/export-issues/",
            post(routes::misc::create_export).get(routes::misc::export_history),
        )
        .route(
            "/api/users/me/",
            get(routes::user::me).patch(routes::user::patch_me),
        )
        .route("/api/users/session/", get(routes::user::session))
        .route("/api/users/me/settings/", get(routes::user::settings))
        .route("/api/users/me/instance-admin/", get(routes::user::instance_admin))
        .route("/api/users/me/onboard/", patch(routes::user::onboard))
        .route("/api/users/me/tour-completed/", patch(routes::user::tour_completed))
        .route("/api/users/me/activities/", get(routes::user::activities))
        .route(
            "/api/users/me/email/generate-code/",
            post(routes::users_me::generate_email_code),
        )
        .route(
            "/api/users/me/profile/",
            get(routes::user::profile).patch(routes::user::patch_profile),
        )
        .route(
            "/api/users/me/accounts/",
            get(routes::user::list_accounts),
        )
        .route(
            "/api/users/me/accounts/:pk/",
            get(routes::user::get_account).delete(routes::user::delete_account),
        )
        .route(
            "/api/users/api-tokens/",
            get(routes::misc::list_tokens).post(routes::misc::create_token),
        )
        .route(
            "/api/users/api-tokens/:pk/",
            get(routes::misc::get_token).delete(routes::misc::delete_token),
        )
        .route(
            "/api/workspaces/:slug/stickies/",
            get(routes::misc::list_stickies).post(routes::misc::create_sticky),
        )
        .route(
            "/api/workspaces/:slug/stickies/:pk/",
            get(routes::misc::get_sticky)
                .patch(routes::misc::patch_sticky)
                .delete(routes::misc::delete_sticky),
        )
        .route("/api/workspaces/:slug/default-analytics/", get(routes::analytic::default_analytics))
        .route("/api/workspaces/:slug/project-stats/", get(routes::analytic::project_stats))
        .route(
            "/api/workspaces/:slug/analytic-view/",
            get(routes::analytic::list_views).post(routes::analytic::create_view),
        );

    // Login + OAuth callback + email-check di-limit per-IP (5/mnt); refresh/logout/start bebas.
    let auth_router = Router::new()
        .route("/api/auth/login/", post(routes::auth::login))
        .route("/api/auth/oauth/:provider/callback/", get(routes::auth::oauth_callback))
        .route("/auth/email-check/", post(routes::auth::email_check))
        .route("/auth/forgot-password/", post(routes::auth_compat::forgot_password))
        .route("/auth/magic-generate/", post(routes::auth_compat::magic_generate))
        .route_layer(axum_middleware::from_fn_with_state(
            IpRateLimiter::new(5, std::time::Duration::from_secs(60)),
            ip_rate_limit_middleware,
        ));

    let app = Router::new()
        .merge(auth_router)
        .merge(app)
        .with_state(state::AppState { pool, redis, config: cfg.clone() })
        .layer(tower_http::limit::RequestBodyLimitLayer::new(5 * 1024 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // Process-level burst backstop. Mirrors DRF throttle intent
    // (`plane/settings/common.py`: anon 30/min, API-key 60/min) with
    // immediate 429 (unlike tower's delay-based limiter) — true per-key
    // accounting stays a follow-up (Redis-backed); this bucket is shared
    // process-wide.
    let app = app.route_layer(axum_middleware::from_fn_with_state(
        RateLimiter::new(600, std::time::Duration::from_secs(60)),
        rate_limit_middleware,
    ));

    let app = app.route_layer(axum_middleware::from_fn_with_state(
        cfg.frontend_url.clone(),
        crate::middleware::origin::origin_middleware,
    ));

    // CORS paling luar: preflight dijawab sebelum origin/rate logic.
    // allow-credentials + origin eksplisit agar cookie auth lintas-port menempel.
    let cors = crate::middleware::cors::cors_layer_from_env(&cfg.frontend_url);
    let app = app.layer(cors);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", cfg.port))
        .await
        .unwrap();
    tracing::info!("rust-api listening on {}", cfg.port);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    ).await.unwrap();
}
