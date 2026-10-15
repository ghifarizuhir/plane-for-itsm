//! Refresh rotation must tolerate a concurrent second refresh with the
//! same token (two tabs refreshing at once). Pre-fix, the second call
//! gets 401 because the old hash is DELeted immediately.

use api::routes::auth::refresh;
use api::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use common::config::AppConfig;
use sqlx::PgPool;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
}

async fn state() -> AppState {
    AppState {
        pool: pool().await,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) {
    sqlx::query(
        "INSERT INTO users (id, password, username, email, first_name, last_name, avatar, \
         date_joined, created_at, updated_at, last_location, created_location, is_superuser, \
         is_managed, is_password_expired, is_active, is_staff, is_email_verified, \
         is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip, \
         last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid, \
         is_password_reset_required) \
         VALUES ($1, '', $2, $3, '', '', '', now(), now(), now(), '', '', false, false, \
         false, true, false, false, true, $4, 'UTC', '', '', '', '', false, $2, true, false)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
}

fn cookie_headers(raw_rt: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("cookie", format!("plane_rt={raw_rt}").parse().unwrap());
    h
}

#[tokio::test]
async fn concurrent_refresh_with_same_token_both_succeed() {
    let st = state().await;
    let uid = Uuid::new_v4();
    insert_user(&st.pool, uid, &format!("rtgrace-{}", uid.simple())).await;

    let family = Uuid::new_v4().simple().to_string();
    let raw = format!("rt_{}", Uuid::new_v4().simple());
    let hash = common::auth::sha256hex(&raw);
    let mut conn = st.redis_client().await.expect("redis must be reachable");
    redis::cmd("SET")
        .arg(common::auth::refresh_key(&hash))
        .arg(format!("{uid}:{family}"))
        .arg("EX")
        .arg(2_592_000)
        .query_async::<()>(&mut conn)
        .await
        .expect("seed refresh hash");
    redis::cmd("SADD")
        .arg(format!("auth:family:{family}"))
        .arg(&hash)
        .query_async::<()>(&mut conn)
        .await
        .expect("seed family set");

    // First refresh (tab A wins).
    let first = refresh(State(st.clone()), cookie_headers(&raw))
        .await
        .expect("handler runs");
    assert_eq!(first.0, StatusCode::OK, "first refresh must succeed");

    // Second refresh with the SAME old token (tab B raced) must also
    // succeed inside the grace window instead of 401.
    let second = refresh(State(st.clone()), cookie_headers(&raw))
        .await
        .expect("handler runs");
    assert_eq!(
        second.0,
        StatusCode::OK,
        "raced refresh must succeed inside grace window"
    );

    // Cleanup: scratch user + family keys.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(uid)
        .execute(&st.pool)
        .await
        .ok();
    redis::cmd("DEL")
        .arg(format!("auth:family:{family}"))
        .query_async::<()>(&mut conn)
        .await
        .ok();
}
