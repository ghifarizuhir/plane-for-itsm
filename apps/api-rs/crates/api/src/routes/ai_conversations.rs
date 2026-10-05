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

/// True when a write failed because the conversation disappeared mid-turn
/// (row deleted between load and write, or an insert hit the FK): callers map
/// this to a 404 instead of a 500.
pub(crate) fn conversation_gone(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::RowNotFound => true,
        sqlx::Error::Database(db) => db.code().as_deref() == Some("23503"),
        _ => false,
    }
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

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await? else {
        return Ok(missing());
    };
    Ok((StatusCode::OK, Json(conversation_json(&row))))
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    let title = body
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if title.is_empty() || title.chars().count() > RENAME_MAX_CHARS {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("title must be 1-{RENAME_MAX_CHARS} characters")})),
        ));
    }
    // Owner-scoped UPDATE + fetch_optional: if the row is deleted between the
    // load above and this statement (other tab / 50-cap prune), answer 404
    // instead of a 500 from `fetch_one`.
    let row: Option<ConversationRow> = sqlx::query_as(
        "UPDATE ai_conversations SET title = $2, updated_at = now() \
         WHERE id = $1 AND created_by_id = $3 \
         RETURNING id, mode, title, created_at, updated_at",
    )
    .bind(conversation_id)
    .bind(title)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(row) => Ok((StatusCode::OK, Json(conversation_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    let deleted = sqlx::query("DELETE FROM ai_conversations WHERE id = $1 AND created_by_id = $2")
        .bind(conversation_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn messages(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT id, conversation_id, role, content, content_html, metadata, created_at \
         FROM ai_messages WHERE conversation_id = $1 ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "messages": rows.iter().map(message_json).collect::<Vec<_>>(),
        })),
    ))
}

/// Validate one allowlisted metadata patch. Pure so it is unit-testable
/// without a DB; returns the cleaned object or a 400 message.
fn clean_metadata_patch(patch: &serde_json::Map<String, Value>) -> Result<Value, String> {
    let mut clean = serde_json::Map::new();
    for (key, value) in patch {
        match key.as_str() {
            "schedule_decision" => match value.as_str() {
                Some("created") | Some("cancelled") => {
                    clean.insert(key.clone(), value.clone());
                }
                _ => return Err("schedule_decision must be 'created' or 'cancelled'".to_string()),
            },
            "created_schedule_id" => match value.as_str().and_then(|raw| Uuid::parse_str(raw).ok()) {
                Some(id) => {
                    clean.insert(key.clone(), json!(id));
                }
                None => return Err("created_schedule_id must be a uuid".to_string()),
            },
            "work_item_decisions" => {
                clean.insert(key.clone(), clean_work_item_decisions(value)?);
            }
            "proposal_decisions" => {
                clean.insert(key.clone(), clean_proposal_decisions(value)?);
            }
            _ => return Err(format!("metadata key not allowed: {key}")),
        }
    }
    if clean.is_empty() {
        return Err("metadata patch is empty".to_string());
    }
    Ok(Value::Object(clean))
}

/// Shape-check the `work_item_decisions` map: UUID keys, decision enum, and the
/// created ids required exactly when the decision is `created`.
fn clean_work_item_decisions(value: &Value) -> Result<Value, String> {
    let Some(decisions) = value.as_object() else {
        return Err("work_item_decisions must be an object".to_string());
    };
    let mut clean = serde_json::Map::new();
    for (key, decision) in decisions {
        let Some(decision_key) = Uuid::parse_str(key).ok() else {
            return Err("work_item_decisions keys must be uuids".to_string());
        };
        let Some(entry) = decision.as_object() else {
            return Err("work_item_decisions values must be objects".to_string());
        };
        match entry.get("decision").and_then(Value::as_str) {
            Some("created") => {
                let issue = entry
                    .get("created_work_item_id")
                    .and_then(Value::as_str)
                    .and_then(|raw| Uuid::parse_str(raw).ok());
                let project = entry
                    .get("created_project_id")
                    .and_then(Value::as_str)
                    .and_then(|raw| Uuid::parse_str(raw).ok());
                let (Some(issue), Some(project)) = (issue, project) else {
                    return Err(
                        "work_item_decisions created entries need created_work_item_id and \
                         created_project_id uuids"
                            .to_string(),
                    );
                };
                clean.insert(
                    decision_key.to_string(),
                    json!({
                        "decision": "created",
                        "created_work_item_id": issue,
                        "created_project_id": project,
                    }),
                );
            }
            Some("cancelled") => {
                if entry.get("created_work_item_id").is_some()
                    || entry.get("created_project_id").is_some()
                {
                    return Err(
                        "work_item_decisions cancelled entries must not carry created ids"
                            .to_string(),
                    );
                }
                clean.insert(
                    decision_key.to_string(),
                    json!({"decision": "cancelled"}),
                );
            }
            _ => {
                return Err(
                    "work_item_decisions decision must be 'created' or 'cancelled'".to_string(),
                );
            }
        }
    }
    Ok(Value::Object(clean))
}

/// Result keys the FE may persist per proposal kind. Kinds without result
/// data get an empty allowlist.
fn allowed_result_keys(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "update_work_item"
        | "update_service"
        | "update_sprint"
        | "update_track"
        | "update_article"
        | "manage_service_links"
        | "manage_sprint_items"
        | "manage_track_items" => Some(&[]),
        "add_comment" => Some(&["created_comment_id"]),
        "create_service" => Some(&["created_service_id"]),
        "create_sprint" => Some(&["created_sprint_id"]),
        "create_track" => Some(&["created_track_id"]),
        "create_article" => Some(&["created_article_id"]),
        _ => None,
    }
}

/// Result key that must be present when the decision is `applied`.
fn required_result_key(kind: &str) -> Option<&'static str> {
    match kind {
        "add_comment" => Some("created_comment_id"),
        "create_service" => Some("created_service_id"),
        "create_sprint" => Some("created_sprint_id"),
        "create_track" => Some("created_track_id"),
        "create_article" => Some("created_article_id"),
        _ => None,
    }
}

/// Shape-check the `proposal_decisions` map written by the generic proposal
/// cards: UUID keys, known kind, decision enum, and a result object limited to
/// the kind's allowlisted UUID keys.
fn clean_proposal_decisions(value: &Value) -> Result<Value, String> {
    let Some(decisions) = value.as_object() else {
        return Err("proposal_decisions must be an object".to_string());
    };
    let mut clean = serde_json::Map::new();
    for (key, decision) in decisions {
        let Some(decision_key) = Uuid::parse_str(key).ok() else {
            return Err("proposal_decisions keys must be uuids".to_string());
        };
        let Some(entry) = decision.as_object() else {
            return Err("proposal_decisions values must be objects".to_string());
        };
        let Some(kind) = entry.get("kind").and_then(Value::as_str) else {
            return Err("proposal_decisions entries need a kind".to_string());
        };
        let Some(result_keys) = allowed_result_keys(kind) else {
            return Err(format!("proposal_decisions kind not allowed: {kind}"));
        };
        match entry.get("decision").and_then(Value::as_str) {
            Some("applied") => {
                let mut result = serde_json::Map::new();
                if let Some(raw) = entry.get("result") {
                    let Some(result_object) = raw.as_object() else {
                        return Err("proposal_decisions result must be an object".to_string());
                    };
                    for (result_key, result_value) in result_object {
                        if !result_keys.contains(&result_key.as_str()) {
                            return Err(format!("{kind} result key not allowed: {result_key}"));
                        }
                        let Some(uuid) =
                            result_value.as_str().and_then(|raw| Uuid::parse_str(raw).ok())
                        else {
                            return Err(format!("{kind} result values must be uuids"));
                        };
                        result.insert(result_key.clone(), json!(uuid));
                    }
                }
                if let Some(required) = required_result_key(kind) {
                    if !result.contains_key(required) {
                        return Err(format!(
                            "{kind} applied decisions need result.{required}"
                        ));
                    }
                }
                let mut cleaned = serde_json::Map::new();
                cleaned.insert("kind".to_string(), json!(kind));
                cleaned.insert("decision".to_string(), json!("applied"));
                if !result.is_empty() {
                    cleaned.insert("result".to_string(), Value::Object(result));
                }
                clean.insert(decision_key.to_string(), Value::Object(cleaned));
            }
            Some("cancelled") => {
                if entry.get("result").is_some() {
                    return Err("cancelled decisions must not carry a result".to_string());
                }
                clean.insert(
                    decision_key.to_string(),
                    json!({"kind": kind, "decision": "cancelled"}),
                );
            }
            _ => {
                return Err(
                    "proposal_decisions decision must be 'applied' or 'cancelled'".to_string(),
                )
            }
        }
    }
    Ok(Value::Object(clean))
}

/// `PATCH .../messages/:message_id/` — merge an allowlisted metadata patch
/// (`schedule_decision`, `created_schedule_id`, `work_item_decisions`,
/// `proposal_decisions`) written by the FE when the user resolves a proposal
/// card.
pub async fn patch_message(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, conversation_id, message_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let Some(_row) = load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    let Some(patch) = body.get("metadata").and_then(Value::as_object) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "metadata must be an object"})),
        ));
    };
    let clean = match clean_metadata_patch(patch) {
        Ok(clean) => clean,
        Err(message) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    };
    let row: Option<MessageRow> = sqlx::query_as(
        "UPDATE ai_messages SET metadata = metadata || $3::jsonb \
         WHERE id = $1 AND conversation_id = $2 \
         RETURNING id, conversation_id, role, content, content_html, metadata, created_at",
    )
    .bind(message_id)
    .bind(conversation_id)
    .bind(clean)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(row) => Ok((StatusCode::OK, Json(message_json(&row)))),
        None => Ok(missing()),
    }
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

    fn patch_map(entries: Vec<(&str, Value)>) -> serde_json::Map<String, Value> {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect()
    }

    #[test]
    fn metadata_patch_accepts_schedule_keys() {
        let clean = clean_metadata_patch(&patch_map(vec![
            ("schedule_decision", json!("created")),
            ("created_schedule_id", json!(Uuid::new_v4())),
        ]))
        .expect("valid patch");
        assert_eq!(clean["schedule_decision"], json!("created"));
    }

    #[test]
    fn metadata_patch_accepts_work_item_decisions() {
        let key = Uuid::new_v4().to_string();
        let issue = Uuid::new_v4();
        let project = Uuid::new_v4();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!({ (key.clone()): {
                "decision": "created",
                "created_work_item_id": issue,
                "created_project_id": project,
            }}),
        )]))
        .expect("valid patch");
        assert_eq!(clean["work_item_decisions"][&key]["decision"], json!("created"));
        assert_eq!(
            clean["work_item_decisions"][&key]["created_work_item_id"],
            json!(issue)
        );
        assert_eq!(
            clean["work_item_decisions"][&key]["created_project_id"],
            json!(project)
        );
    }

    #[test]
    fn metadata_patch_accepts_cancelled_without_ids() {
        let key = Uuid::new_v4().to_string();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!({ (key.clone()): {"decision": "cancelled"} }),
        )]))
        .expect("valid patch");
        assert_eq!(clean["work_item_decisions"][&key]["decision"], json!("cancelled"));
    }

    #[test]
    fn metadata_patch_rejects_bad_work_item_decisions() {
        let key = Uuid::new_v4().to_string();
        let issue = Uuid::new_v4();
        let project = Uuid::new_v4();

        let missing_ids = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!({ (key.clone()): {"decision": "created"} }),
        )]))
        .unwrap_err();
        assert!(missing_ids.contains("created_work_item_id"));

        let ids_on_cancel = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!({ (key.clone()): {
                "decision": "cancelled",
                "created_work_item_id": issue,
                "created_project_id": project,
            }}),
        )]))
        .unwrap_err();
        assert!(ids_on_cancel.contains("cancelled"));

        let bad_decision = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!({ (key.clone()): {"decision": "maybe"} }),
        )]))
        .unwrap_err();
        assert!(bad_decision.contains("decision"));

        let bad_key = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!({"not-a-uuid": {"decision": "cancelled"}}),
        )]))
        .unwrap_err();
        assert!(bad_key.contains("uuid"));

        let not_an_object = clean_metadata_patch(&patch_map(vec![(
            "work_item_decisions",
            json!("nope"),
        )]))
        .unwrap_err();
        assert!(not_an_object.contains("object"));
    }

    #[test]
    fn metadata_patch_accepts_proposal_decisions() {
        let comment_key = Uuid::new_v4().to_string();
        let update_key = Uuid::new_v4().to_string();
        let comment = Uuid::new_v4();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({
                (comment_key.clone()): {
                    "kind": "add_comment",
                    "decision": "applied",
                    "result": {"created_comment_id": comment},
                },
                (update_key.clone()): {
                    "kind": "update_work_item",
                    "decision": "applied",
                },
            }),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][comment_key.as_str()]["decision"],
            json!("applied")
        );
        assert_eq!(
            clean["proposal_decisions"][comment_key.as_str()]["result"]["created_comment_id"],
            json!(comment)
        );
        assert_eq!(
            clean["proposal_decisions"][update_key.as_str()]["decision"],
            json!("applied")
        );
    }

    #[test]
    fn metadata_patch_rejects_bad_proposal_decisions() {
        let key = Uuid::new_v4().to_string();

        let unknown_kind = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {"kind": "delete_work_item", "decision": "applied"}}),
        )]))
        .unwrap_err();
        assert!(unknown_kind.contains("kind"));

        let missing_comment = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {"kind": "add_comment", "decision": "applied"}}),
        )]))
        .unwrap_err();
        assert!(missing_comment.contains("created_comment_id"));

        let result_on_update = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {
                "kind": "update_work_item",
                "decision": "applied",
                "result": {"created_comment_id": Uuid::new_v4()},
            }}),
        )]))
        .unwrap_err();
        assert!(result_on_update.contains("not allowed"));

        let result_on_cancel = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {
                "kind": "update_work_item",
                "decision": "cancelled",
                "result": {},
            }}),
        )]))
        .unwrap_err();
        assert!(result_on_cancel.contains("cancelled"));

        let bad_decision = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {"kind": "update_work_item", "decision": "done"}}),
        )]))
        .unwrap_err();
        assert!(bad_decision.contains("applied"));

        let bad_key = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({"nope": {"kind": "update_work_item", "decision": "applied"}}),
        )]))
        .unwrap_err();
        assert!(bad_key.contains("uuid"));

        let not_an_object = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!("nope"),
        )]))
        .unwrap_err();
        assert!(not_an_object.contains("object"));
    }

    #[test]
    fn metadata_patch_accepts_link_decisions_without_result() {
        let key = Uuid::new_v4().to_string();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({
                (key.clone()): {"kind": "manage_service_links", "decision": "applied"},
            }),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][key.as_str()]["decision"],
            json!("applied")
        );
    }

    #[test]
    fn metadata_patch_rejects_result_on_link_decisions() {
        let key = Uuid::new_v4().to_string();
        let err = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {
                "kind": "manage_sprint_items",
                "decision": "applied",
                "result": {"created_comment_id": Uuid::new_v4()},
            }}),
        )]))
        .unwrap_err();
        assert!(err.contains("not allowed"));
    }

    #[test]
    fn metadata_patch_accepts_container_decisions() {
        let create_key = Uuid::new_v4().to_string();
        let update_key = Uuid::new_v4().to_string();
        let service = Uuid::new_v4();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({
                (create_key.clone()): {
                    "kind": "create_service",
                    "decision": "applied",
                    "result": {"created_service_id": service},
                },
                (update_key.clone()): {
                    "kind": "update_sprint",
                    "decision": "applied",
                },
            }),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][create_key.as_str()]["result"]["created_service_id"],
            json!(service)
        );
        assert_eq!(
            clean["proposal_decisions"][update_key.as_str()]["decision"],
            json!("applied")
        );
    }

    #[test]
    fn metadata_patch_requires_create_result_ids() {
        let key = Uuid::new_v4().to_string();
        let missing = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {"kind": "create_track", "decision": "applied"}}),
        )]))
        .unwrap_err();
        assert!(missing.contains("created_track_id"));

        let wrong_key = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {
                "kind": "create_sprint",
                "decision": "applied",
                "result": {"created_comment_id": Uuid::new_v4()},
            }}),
        )]))
        .unwrap_err();
        assert!(wrong_key.contains("not allowed"));
    }

    #[test]
    fn metadata_patch_accepts_article_decisions() {
        let key = Uuid::new_v4().to_string();
        let page = Uuid::new_v4();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {
                "kind": "create_article",
                "decision": "applied",
                "result": {"created_article_id": page},
            }}),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][key.as_str()]["result"]["created_article_id"],
            json!(page)
        );

        let missing = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {"kind": "create_article", "decision": "applied"}}),
        )]))
        .unwrap_err();
        assert!(missing.contains("created_article_id"));

        let update_key = Uuid::new_v4().to_string();
        let clean_update = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(update_key.clone()): {"kind": "update_article", "decision": "applied"}}),
        )]))
        .expect("valid update patch");
        assert_eq!(
            clean_update["proposal_decisions"][update_key.as_str()]["decision"],
            json!("applied")
        );
    }

    #[test]
    fn metadata_patch_rejects_unknown_and_empty() {
        let unknown = clean_metadata_patch(&patch_map(vec![("nope", json!(1))])).unwrap_err();
        assert!(unknown.contains("not allowed"));
        let empty = clean_metadata_patch(&patch_map(vec![])).unwrap_err();
        assert!(empty.contains("empty"));
    }
}
