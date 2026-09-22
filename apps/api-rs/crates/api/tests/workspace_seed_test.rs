//! Integration test seed workspace: panggil handler `POST /api/workspaces/`
//! (men-seed sinkron) lalu assert baris demo yang dihasilkan.
//! Butuh Postgres: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane`.

use api::middleware::auth::AuthUser;
use api::routes::workspace::create;
use api::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::json;
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

async fn purge(pool: &PgPool, slug: &str) {
    let ws: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .expect("purge lookup");
    let Some((ws_id,)) = ws else { return };
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT member_id FROM workspace_members WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_all(pool)
            .await
            .expect("purge members");
    for stmt in [
        "DELETE FROM module_issues WHERE workspace_id = $1",
        "DELETE FROM cycle_issues WHERE workspace_id = $1",
        "DELETE FROM issue_labels WHERE workspace_id = $1",
        "DELETE FROM issue_activities WHERE workspace_id = $1",
        "DELETE FROM issue_sequences WHERE workspace_id = $1",
        "DELETE FROM issues WHERE workspace_id = $1",
        "DELETE FROM issue_views WHERE workspace_id = $1",
        "DELETE FROM project_pages WHERE workspace_id = $1",
        "DELETE FROM pages WHERE workspace_id = $1",
        "DELETE FROM modules WHERE workspace_id = $1",
        "DELETE FROM cycles WHERE workspace_id = $1",
        "DELETE FROM labels WHERE workspace_id = $1",
        "DELETE FROM states WHERE workspace_id = $1",
        "DELETE FROM project_user_properties WHERE workspace_id = $1",
        "DELETE FROM project_members WHERE workspace_id = $1",
        "DELETE FROM projects WHERE workspace_id = $1",
        "DELETE FROM workspace_members WHERE workspace_id = $1",
        "DELETE FROM workspaces WHERE id = $1",
    ] {
        sqlx::query(stmt)
            .bind(ws_id)
            .execute(pool)
            .await
            .expect("purge");
    }
    for (member_id,) in members {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(member_id)
            .execute(pool)
            .await
            .expect("purge user");
    }
}

#[tokio::test]
async fn workspace_create_seeds_itsm_demo() {
    let st = state().await;
    let pool = st.pool.clone();
    let slug = format!("wsseed-{}", Uuid::new_v4().simple());
    let owner = Uuid::new_v4();
    insert_user(&pool, owner, &slug).await;

    let (status, _) = create(
        State(st.clone()),
        AuthUser(owner),
        Json(json!({"name": "Acme IT", "slug": slug})),
    )
    .await
    .expect("create must not error");
    assert_eq!(status, StatusCode::CREATED);

    let (ws_id,): (Uuid,) = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&pool)
        .await
        .expect("workspace row");

    let count = |table: &'static str| {
        let pool = pool.clone();
        async move {
            let (c,): (i64,) = sqlx::query_as(&format!(
                "SELECT COUNT(*) FROM {table} WHERE workspace_id = $1"
            ))
            .bind(ws_id)
            .fetch_one(&pool)
            .await
            .expect("count");
            c
        }
    };

    let (project_name, identifier): (String, String) =
        sqlx::query_as("SELECT name, identifier FROM projects WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_one(&pool)
            .await
            .expect("project row");
    assert_eq!(project_name, "Acme IT");
    assert_eq!(identifier, "AcmeI");

    assert_eq!(count("states").await, 5);
    assert_eq!(count("labels").await, 2);
    assert_eq!(count("cycles").await, 2);
    assert_eq!(count("modules").await, 3);
    assert_eq!(count("issues").await, 7);
    assert_eq!(count("issue_sequences").await, 7);
    assert_eq!(count("issue_activities").await, 7);
    assert_eq!(count("issue_labels").await, 3);
    assert_eq!(count("cycle_issues").await, 7);
    assert_eq!(count("module_issues").await, 11);
    assert_eq!(count("issue_views").await, 1);
    assert_eq!(count("pages").await, 2);
    assert_eq!(count("project_pages").await, 2);
    assert_eq!(count("project_members").await, 2);
    assert_eq!(count("project_user_properties").await, 2);

    let (bot_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE is_bot = true AND bot_type = 'WORKSPACE_SEED' AND email LIKE 'bot_user_%'")
            .fetch_one(&pool)
            .await
            .expect("bot count");
    assert_eq!(bot_count, 1);

    purge(&pool, &slug).await;
}
