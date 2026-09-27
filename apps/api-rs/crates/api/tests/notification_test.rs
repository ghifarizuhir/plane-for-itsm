use api::middleware::auth::AuthUser;
use api::routes::notification::{self, validate_preference_patch, PREFERENCE_KEYS};
use api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
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

async fn state(pool: &PgPool) -> AppState {
    AppState {
        pool: pool.clone(),
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

#[test]
fn rejects_unknown_preference_keys() {
    let mut patch = HashMap::new();
    patch.insert("telepathy".to_string(), serde_json::json!(true));
    let err = validate_preference_patch(&patch).unwrap_err();
    assert!(err.to_lowercase().contains("unknown"));
}

#[test]
fn accepts_known_preference_keys() {
    for key in PREFERENCE_KEYS {
        let mut patch = HashMap::new();
        patch.insert(key.to_string(), serde_json::json!(false));
        assert!(validate_preference_patch(&patch).is_ok(), "key={key}");
    }
}

#[test]
fn rejects_non_bool_preference_value() {
    let mut patch = HashMap::new();
    patch.insert("comment".to_string(), serde_json::json!("yes"));
    let err = validate_preference_patch(&patch).unwrap_err();
    assert!(err.to_lowercase().contains("boolean"));
}

#[tokio::test]
async fn list_includes_schedule_run_notifications() {
    let pool = pool().await;
    let slug = format!("ntf-{}", Uuid::new_v4().simple());
    let user_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
         background_color) VALUES ($1, 'Inbox', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(user_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();
    let run_id = Uuid::new_v4();
    for (entity_name, title, sender, data) in [
        (
            "issue",
            "Issue notification",
            "in_app:issue_activities:created",
            json!({}),
        ),
        (
            "ai_schedule_run",
            "Daily report",
            "in_app:ai_schedule:run",
            json!({"ai_schedule": {"schedule_id": schedule_id.to_string(), "run_id": run_id.to_string()}}),
        ),
        ("mystery", "Hidden", "in_app:other", json!({})),
    ] {
        sqlx::query(
            "INSERT INTO notifications (id, workspace_id, receiver_id, entity_name, \
             entity_identifier, title, sender, data, message_html, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, '<p></p>', now(), now())",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(entity_name)
        .bind(Uuid::new_v4())
        .bind(title)
        .bind(sender)
        .bind(data)
        .execute(&pool)
        .await
        .unwrap();
    }

    let state = state(&pool).await;
    let (status, Json(rows)) = notification::list(
        State(state.clone()),
        AuthUser(user_id),
        Path(slug.clone()),
        Query(HashMap::<String, String>::new()),
    )
    .await
    .expect("list ok");
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["entity_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"issue"), "issue notifications still listed");
    assert!(
        names.contains(&"ai_schedule_run"),
        "schedule run notifications must be listed"
    );
    assert!(!names.contains(&"mystery"), "unknown entity types stay hidden");

    let Json(counts) = notification::unread(State(state.clone()), AuthUser(user_id), Path(slug.clone()))
        .await
        .expect("unread ok");
    // The unread badge intentionally counts every entity_name (pre-existing
    // behaviour); only the list filter hides unknown types.
    assert_eq!(counts["total_unread_notifications_count"], json!(3));

    sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}
