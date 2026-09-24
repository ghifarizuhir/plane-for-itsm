//! Rust-only Galileo chat history endpoints (no Django counterpart).
//!
//! `GET/POST /api/workspaces/:slug/ai-conversations/` — list + create;
//! `GET/PATCH/DELETE /:conversation_id/` — detail, rename, delete;
//! `GET /:conversation_id/messages/` — stored messages;
//! `PATCH /:conversation_id/messages/:message_id/` — schedule-decision
//! metadata. Reads and mutations are owner-only (404 otherwise).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::routes::ai_schedule::workspace_id_for_slug;
use crate::routes::module::guard_am;
use crate::routes::project::{deny, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

pub const MAX_CONVERSATIONS_PER_USER: i64 = 50;
pub const MAX_MESSAGES_PER_CONVERSATION: i64 = 200;
pub const TITLE_MAX_CHARS: usize = 60;
pub const RENAME_MAX_CHARS: usize = 120;

#[derive(serde::Deserialize)]
pub struct CreateConversationBody {
    pub mode: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ConversationRow {
    pub id: Uuid,
    pub mode: String,
    pub title: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct MessageRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub content: String,
    pub content_html: Option<String>,
    pub metadata: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub(crate) fn conversation_json(row: &ConversationRow) -> Value {
    json!({
        "id": row.id,
        "title": row.title,
        "mode": row.mode,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

pub(crate) fn message_json(row: &MessageRow) -> Value {
    json!({
        "id": row.id,
        "role": row.role,
        "content": row.content,
        "content_html": row.content_html,
        "metadata": row.metadata,
        "created_at": row.created_at,
    })
}

/// First line of the first user message, whitespace-collapsed, 60 chars max.
pub(crate) fn title_from(prompt: &str) -> String {
    let single_line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let collapsed = single_line.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(TITLE_MAX_CHARS).collect()
}

/// Load a conversation by workspace slug + id, scoped to its owner
/// (unknown slug, foreign user or missing row → `None`).
pub(crate) async fn load_owned_conversation(
    pool: &PgPool,
    slug: &str,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ConversationRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.id, c.mode, c.title, c.created_at, c.updated_at \
         FROM ai_conversations c \
         JOIN workspaces w ON w.id = c.workspace_id AND w.slug = $1 AND w.deleted_at IS NULL \
         WHERE c.id = $2 AND c.created_by_id = $3",
    )
    .bind(slug)
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// The newest `limit` messages, returned oldest-first for prompt building.
pub(crate) async fn recent_messages(
    pool: &PgPool,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    let mut rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT id, conversation_id, role, content, content_html, metadata, created_at \
         FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

/// Insert one message and return the stored row. Takes a connection so the
/// caller can run it inside a transaction (`&mut *tx`) or on a pooled
/// connection (`&mut conn`).
pub(crate) async fn insert_message(
    conn: &mut sqlx::PgConnection,
    conversation_id: Uuid,
    role: &str,
    content: &str,
    content_html: Option<&str>,
    metadata: &Value,
) -> Result<MessageRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO ai_messages (id, conversation_id, role, content, content_html, metadata, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         RETURNING id, conversation_id, role, content, content_html, metadata, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(content_html)
    .bind(metadata)
    .fetch_one(conn)
    .await
}

/// Close one chat turn: fill the auto-title when empty, bump `updated_at`,
/// and prune messages beyond the per-conversation cap. Runs on the caller's
/// connection/transaction — the success path wraps this together with the
/// assistant `insert_message` in one transaction.
pub(crate) async fn finish_turn(
    conn: &mut sqlx::PgConnection,
    conversation_id: Uuid,
    title: &str,
) -> Result<ConversationRow, sqlx::Error> {
    let row: ConversationRow = sqlx::query_as(
        "UPDATE ai_conversations SET \
         title = CASE WHEN title = '' THEN $2 ELSE title END, updated_at = now() \
         WHERE id = $1 \
         RETURNING id, mode, title, created_at, updated_at",
    )
    .bind(conversation_id)
    .bind(title)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM ai_messages WHERE id IN ( \
         SELECT id FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at DESC, id DESC OFFSET $2)",
    )
    .bind(conversation_id)
    .bind(MAX_MESSAGES_PER_CONVERSATION)
    .execute(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok((StatusCode::OK, Json(json!({"conversations": []}))));
    };
    let rows: Vec<ConversationRow> = sqlx::query_as(
        "SELECT id, mode, title, created_at, updated_at \
         FROM ai_conversations WHERE workspace_id = $1 AND created_by_id = $2 \
         ORDER BY updated_at DESC, id DESC LIMIT $3",
    )
    .bind(workspace_id)
    .bind(auth.0)
    .bind(MAX_CONVERSATIONS_PER_USER)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "conversations": rows.iter().map(conversation_json).collect::<Vec<_>>(),
        })),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let body: CreateConversationBody = match serde_json::from_value(payload) {
        Ok(body) => body,
        Err(err) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid conversation payload: {err}")})),
            ));
        }
    };
    if body.mode != "classic" && body.mode != "agent" {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "mode must be 'classic' or 'agent'"})),
        ));
    }
    let title = body
        .title
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .chars()
        .take(RENAME_MAX_CHARS)
        .collect::<String>();
    let Some(workspace_id) = workspace_id_for_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let mut tx = st.pool.begin().await?;
    let row: ConversationRow = sqlx::query_as(
        "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now(), now()) \
         RETURNING id, mode, title, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id)
    .bind(auth.0)
    .bind(&body.mode)
    .bind(&title)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM ai_conversations WHERE id IN ( \
         SELECT id FROM ai_conversations WHERE workspace_id = $1 AND created_by_id = $2 \
         ORDER BY updated_at DESC, id DESC OFFSET $3)",
    )
    .bind(workspace_id)
    .bind(auth.0)
    .bind(MAX_CONVERSATIONS_PER_USER)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(conversation_json(&row))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_collapses_whitespace_and_truncates() {
        assert_eq!(
            title_from("  buat laporan overdue  "),
            "buat laporan overdue"
        );
        assert_eq!(title_from("first line\nsecond line"), "first line");
        assert_eq!(title_from("   \n\n  "), "");
        let long = "a".repeat(80);
        assert_eq!(title_from(&long).chars().count(), TITLE_MAX_CHARS);
    }
}
