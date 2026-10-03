//! Review briefing (AI): pre-meeting briefing for TCB/RCB sessions.
//! Spec: docs/superpowers/specs/2026-10-03-review-briefing-design.md

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::ai::chat_completion;
use super::review::{
    can_manage_session, gate_session_read, items_for_session, session_in_workspace, SessionItemRow,
};
use super::service::bad_request;
use ai::llm::resolve_llm_config;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub const MAX_BRIEFING_ITEMS: usize = 20;
pub const MAX_ITEM_CONTEXT_CHARS: usize = 1500;
pub const MAX_RELEASE_CHANGES: usize = 30;
pub const MAX_TEXT_FALLBACK_CHARS: usize = 20_000;

/// Keep ASCII alphanumerics and `-`, cap at 16 chars; empty → `en`.
pub fn sanitize_language(raw: Option<&str>) -> String {
    let candidate: String = raw
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(16)
        .collect();
    if candidate.is_empty() {
        "en".to_string()
    } else {
        candidate
    }
}

/// Strip HTML tags, collapse whitespace, truncate on a char boundary.
pub fn truncate_text(raw: &str, max: usize) -> String {
    let mut out = String::with_capacity(raw.len().min(max));
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        collapsed
    } else {
        collapsed.chars().take(max).collect()
    }
}

/// Instruction block for the single-shot completion.
pub fn build_task(language: &str) -> String {
    format!(
        "You are a technical assistant preparing a pre-meeting briefing for a change/release \
review board. You receive a JSON context describing the session and its agenda items. For every \
item write: summary (2-3 sentences: what the change/release is and why it was submitted), \
discussion_points (0-3 points grounded strictly in the given facts, e.g. earlier decisions, \
deferred items, related war rooms, submission notes), and risks (0-3 risks visible in the \
facts). Never recommend approving or rejecting. Never invent facts that are not in the context. \
Write in language '{language}'. Reply with JSON only, exactly this shape: \
{{\"overall\": \"...\", \"items\": [{{\"session_item_id\": \"...\", \"summary\": \"...\", \
\"discussion_points\": [\"...\"], \"risks\": [\"...\"]}}]}}"
    )
}

pub fn build_prompt(context: &Value) -> String {
    format!("Context JSON:\n{context}")
}

fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest.trim().trim_end_matches("```").trim();
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest.trim().trim_end_matches("```").trim();
    }
    trimmed
}

/// Extract `(overall, items)` from a model reply. `None` when the reply is not
/// the expected shape. Items with unknown or duplicate ids are dropped.
pub fn parse_ai_json(raw: &str, valid_ids: &[Uuid]) -> Option<(String, Vec<Value>)> {
    let value: Value = serde_json::from_str(strip_code_fence(raw)).ok()?;
    let overall = value.get("overall")?.as_str()?.trim().to_string();
    if overall.is_empty() {
        return None;
    }
    let mut seen: Vec<Uuid> = Vec::new();
    let mut items: Vec<Value> = Vec::new();
    if let Some(list) = value.get("items").and_then(Value::as_array) {
        for entry in list {
            let Some(id_raw) = entry.get("session_item_id").and_then(Value::as_str) else {
                continue;
            };
            let Ok(id) = Uuid::parse_str(id_raw) else {
                continue;
            };
            if !valid_ids.contains(&id) || seen.contains(&id) {
                continue;
            }
            let strings = |key: &str| -> Vec<String> {
                entry
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            seen.push(id);
            items.push(json!({
                "session_item_id": id,
                "summary": entry
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim(),
                "discussion_points": strings("discussion_points"),
                "risks": strings("risks"),
            }));
        }
    }
    Some((overall, items))
}

/// Map an upstream LLM failure onto the route error idiom used by `routes/ai.rs`.
pub fn llm_error_response(error: ai::llm::LlmError, base_url: &str) -> (StatusCode, Json<Value>) {
    match error {
        ai::llm::LlmError::RateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": format!(
                "Rate limit exceeded for {}",
                ai::llm::host_of(base_url)
            )})),
        ),
        ai::llm::LlmError::Upstream => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "An internal error has occurred."})),
        ),
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ChangeContextRow {
    identifier: String,
    name: String,
    description_html: String,
    priority: String,
    state_name: Option<String>,
    target_date: Option<chrono::NaiveDate>,
}

async fn change_context(
    pool: &PgPool,
    issue_id: Uuid,
) -> Result<Option<ChangeContextRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT (p.identifier || '-' || i.sequence_id::text) AS identifier, i.name, \
         i.description_html, i.priority, s.name AS state_name, i.target_date \
         FROM issues i JOIN projects p ON p.id = i.project_id \
         LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await
}

async fn assignee_names(pool: &PgPool, issue_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(u.display_name, u.username) FROM issue_assignees ia \
         JOIN users u ON u.id = ia.assignee_id \
         WHERE ia.issue_id = $1 AND ia.deleted_at IS NULL ORDER BY ia.created_at ASC LIMIT 5",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
}

async fn war_room_context(pool: &PgPool, issue_id: Uuid) -> Result<Vec<Value>, sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT wr.name, wr.severity, wr.status FROM war_room_issues wri \
         JOIN war_rooms wr ON wr.id = wri.war_room_id \
         WHERE wri.issue_id = $1 AND wri.deleted_at IS NULL AND wr.deleted_at IS NULL \
         ORDER BY wr.created_at DESC LIMIT 3",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|(name, severity, status)| json!({"name": name, "severity": severity, "status": status}))
        .collect())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PriorDecisionRow {
    title: String,
    scheduled_at: DateTime<Utc>,
    outcome: Option<String>,
    outcome_note: String,
    decided_at: Option<DateTime<Utc>>,
}

async fn prior_decisions(pool: &PgPool, request_id: Uuid) -> Result<Vec<Value>, sqlx::Error> {
    let rows: Vec<PriorDecisionRow> = sqlx::query_as(
        "SELECT s.title, s.scheduled_at, i.outcome, i.outcome_note, i.decided_at \
         FROM review_session_items i JOIN review_sessions s ON s.id = i.session_id \
         WHERE i.review_request_id = $1 AND i.deleted_at IS NULL \
         ORDER BY s.scheduled_at ASC",
    )
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| {
            json!({
                "session_title": row.title,
                "scheduled_at": row.scheduled_at,
                "outcome": row.outcome,
                "outcome_note": row.outcome_note,
                "decided_at": row.decided_at,
            })
        })
        .collect())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ReleaseContextRow {
    sequence_id: i64,
    name: String,
    version: Option<String>,
    description_html: String,
    status: String,
    target_date: Option<chrono::NaiveDate>,
}

async fn release_context(
    pool: &PgPool,
    release_id: Uuid,
) -> Result<Option<ReleaseContextRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT sequence_id, name, version, description_html, status, target_date \
         FROM releases WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(release_id)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ReleaseChangeContextRow {
    identifier: String,
    name: String,
    project_identifier: String,
    latest_outcome: Option<String>,
}

async fn release_changes_context(
    pool: &PgPool,
    release_id: Uuid,
) -> Result<Vec<ReleaseChangeContextRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT (p.identifier || '-' || i.sequence_id::text) AS identifier, i.name, \
         p.identifier AS project_identifier, \
         (SELECT rsi.outcome FROM review_session_items rsi \
          JOIN review_requests rr ON rr.id = rsi.review_request_id \
          WHERE rr.change_issue_id = i.id AND rsi.outcome IS NOT NULL AND rsi.deleted_at IS NULL \
          ORDER BY rsi.decided_at DESC NULLS LAST LIMIT 1) AS latest_outcome \
         FROM release_changes rc JOIN issues i ON i.id = rc.issue_id \
         JOIN projects p ON p.id = i.project_id \
         WHERE rc.release_id = $1 AND rc.deleted_at IS NULL \
         ORDER BY rc.created_at ASC LIMIT $2",
    )
    .bind(release_id)
    .bind(MAX_RELEASE_CHANGES as i64)
    .fetch_all(pool)
    .await
}

async fn release_change_counts(pool: &PgPool, release_id: Uuid) -> Result<(i64, i64), sqlx::Error> {
    sqlx::query_as(
        "SELECT COUNT(*) AS total, \
         COUNT(*) FILTER (WHERE NOT EXISTS ( \
           SELECT 1 FROM review_session_items rsi \
           JOIN review_requests rr ON rr.id = rsi.review_request_id \
           WHERE rr.change_issue_id = i.id AND rsi.outcome IS NOT NULL AND rsi.deleted_at IS NULL \
         )) AS without_outcome \
         FROM release_changes rc JOIN issues i ON i.id = rc.issue_id \
         WHERE rc.release_id = $1 AND rc.deleted_at IS NULL",
    )
    .bind(release_id)
    .fetch_one(pool)
    .await
}

async fn display_name(pool: &PgPool, user_id: Uuid) -> Result<String, sqlx::Error> {
    let name: Option<String> =
        sqlx::query_scalar("SELECT COALESCE(display_name, username) FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(name.unwrap_or_default())
}

/// Deterministic agenda context + number of items skipped by the cap.
pub async fn assemble_context(
    pool: &PgPool,
    session: &crate::routes::review::ReviewSessionRow,
    items: &[SessionItemRow],
) -> Result<(Value, usize), sqlx::Error> {
    let skipped = items.len().saturating_sub(MAX_BRIEFING_ITEMS);
    let mut out_items: Vec<Value> = Vec::new();
    for item in items.iter().take(MAX_BRIEFING_ITEMS) {
        let mut entry = json!({
            "session_item_id": item.id,
            "kind": if item.board_type == "tcb" { "change" } else { "release" },
            "submission_note": truncate_text(&item.submission_note, MAX_ITEM_CONTEXT_CHARS),
            "prior_decisions": prior_decisions(pool, item.review_request_id).await?,
        });
        if item.board_type == "tcb" {
            if let Some(issue_id) = item.issue_id {
                if let Some(ctx) = change_context(pool, issue_id).await? {
                    entry["identifier"] = json!(ctx.identifier);
                    entry["name"] = json!(ctx.name);
                    entry["description"] =
                        json!(truncate_text(&ctx.description_html, MAX_ITEM_CONTEXT_CHARS));
                    entry["priority"] = json!(ctx.priority);
                    entry["state"] = json!(ctx.state_name);
                    entry["target_date"] = json!(ctx.target_date);
                    entry["assignees"] = json!(assignee_names(pool, issue_id).await?);
                    entry["war_rooms"] = json!(war_room_context(pool, issue_id).await?);
                }
            }
        } else if let Some(release_id) = item.release_id {
            if let Some(ctx) = release_context(pool, release_id).await? {
                let (total, without_outcome) = release_change_counts(pool, release_id).await?;
                let changes = release_changes_context(pool, release_id).await?;
                entry["identifier"] = json!(format!("REL-{}", ctx.sequence_id));
                entry["name"] = json!(ctx.name);
                entry["version"] = json!(ctx.version);
                entry["status"] = json!(ctx.status);
                entry["target_date"] = json!(ctx.target_date);
                entry["description"] =
                    json!(truncate_text(&ctx.description_html, MAX_ITEM_CONTEXT_CHARS));
                entry["changes_total"] = json!(total);
                entry["changes_without_outcome"] = json!(without_outcome);
                entry["changes"] = json!(changes
                    .iter()
                    .map(|change| json!({
                        "identifier": change.identifier,
                        "name": change.name,
                        "project": change.project_identifier,
                        "latest_outcome": change.latest_outcome,
                    }))
                    .collect::<Vec<_>>());
            }
        }
        out_items.push(entry);
    }
    Ok((
        json!({
            "board_type": session.board_type,
            "session_title": session.title,
            "scheduled_at": session.scheduled_at,
            "items": out_items,
        }),
        skipped,
    ))
}

#[derive(Debug, Deserialize, Default)]
pub struct BriefingRequest {
    pub language: Option<String>,
}

/// POST `/api/workspaces/:slug/review-sessions/:session_id/briefing/`
pub async fn generate_briefing(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, session_id)): Path<(String, Uuid)>,
    Json(body): Json<BriefingRequest>,
) -> R {
    let Some(session) = session_in_workspace(&st.pool, &slug, session_id).await? else {
        return Ok(missing());
    };
    if !gate_session_read(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if !can_manage_session(&st, auth.0, &slug, &session).await? {
        return Ok(deny());
    }
    if session.status != "scheduled" {
        return Ok(bad_request("Session is not scheduled"));
    }
    let items = items_for_session(&st.pool, session_id).await?;
    if items.is_empty() {
        return Ok(bad_request("The agenda is empty"));
    }
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let language = sanitize_language(body.language.as_deref());
    let (context, skipped) = assemble_context(&st.pool, &session, &items).await?;
    let valid_ids: Vec<Uuid> = items
        .iter()
        .take(MAX_BRIEFING_ITEMS)
        .map(|item| item.id)
        .collect();
    let task = build_task(&language);
    let prompt = build_prompt(&context);

    let first =
        match chat_completion(&cfg.base_url, &cfg.api_key, &cfg.model, &task, &prompt).await {
            Ok(text) => text,
            Err(error) => return Ok(llm_error_response(error, &cfg.base_url)),
        };
    let (overall, parsed_items, format) = match parse_ai_json(&first, &valid_ids) {
        Some((overall, parsed_items)) => (overall, parsed_items, "json"),
        None => {
            let retry_task =
                format!("{task}\nReturn ONLY the JSON object, no prose, no code fences.");
            let second = match chat_completion(
                &cfg.base_url,
                &cfg.api_key,
                &cfg.model,
                &retry_task,
                &prompt,
            )
            .await
            {
                Ok(text) => text,
                Err(error) => return Ok(llm_error_response(error, &cfg.base_url)),
            };
            match parse_ai_json(&second, &valid_ids) {
                Some((overall, parsed_items)) => (overall, parsed_items, "json"),
                None => (
                    truncate_text(&second, MAX_TEXT_FALLBACK_CHARS),
                    Vec::new(),
                    "text",
                ),
            }
        }
    };

    let briefing = json!({
        "version": 1,
        "generated_at": Utc::now(),
        "generated_by_name": display_name(&st.pool, auth.0).await?,
        "model": cfg.model,
        "language": language,
        "format": format,
        "overall": overall,
        "items": parsed_items,
        "included_items": valid_ids.len(),
        "skipped_items": skipped,
    });
    sqlx::query(
        "UPDATE review_sessions SET briefing = $1, briefing_generated_at = now(), \
         briefing_generated_by_id = $2, briefing_model = $3, updated_at = now(), updated_by_id = $2 \
         WHERE id = $4 AND deleted_at IS NULL",
    )
    .bind(&briefing)
    .bind(auth.0)
    .bind(&cfg.model)
    .bind(session_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(briefing)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_language_keeps_alnum_and_dash() {
        assert_eq!(sanitize_language(Some("id")), "id");
        assert_eq!(sanitize_language(Some("pt-BR")), "pt-BR");
        assert_eq!(sanitize_language(Some("id; drop table")), "iddroptable");
        assert_eq!(sanitize_language(Some("")), "en");
        assert_eq!(sanitize_language(None), "en");
    }

    #[test]
    fn truncate_text_strips_tags_and_collapses_space() {
        assert_eq!(truncate_text("<p>Halo <b>dunia</b></p>", 100), "Halo dunia");
        assert_eq!(truncate_text("abcdef", 3), "abc");
    }

    #[test]
    fn parse_ai_json_filters_unknown_and_duplicate_ids() {
        let id = Uuid::new_v4();
        let other = Uuid::new_v4();
        let raw = format!(
            r#"{{"overall":"ok","items":[
                {{"session_item_id":"{id}","summary":"s","discussion_points":["a"],"risks":[]}},
                {{"session_item_id":"{id}","summary":"dup","discussion_points":[],"risks":[]}},
                {{"session_item_id":"{other}","summary":"unknown","discussion_points":[],"risks":[]}}
            ]}}"#
        );
        let (overall, items) = parse_ai_json(&raw, &[id]).expect("valid");
        assert_eq!(overall, "ok");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["session_item_id"], id.to_string());
        assert_eq!(items[0]["summary"], "s");
        assert_eq!(items[0]["discussion_points"][0], "a");
    }

    #[test]
    fn parse_ai_json_rejects_non_json_and_missing_overall() {
        assert!(parse_ai_json("not json", &[]).is_none());
        assert!(parse_ai_json(r#"{"items":[]}"#, &[]).is_none());
    }

    #[test]
    fn parse_ai_json_accepts_fenced_json() {
        let raw = "```json\n{\"overall\":\"ok\",\"items\":[]}\n```";
        let (overall, items) = parse_ai_json(raw, &[]).expect("fenced");
        assert_eq!(overall, "ok");
        assert!(items.is_empty());
    }

    #[test]
    fn build_task_mentions_language_and_json() {
        let task = build_task("id");
        assert!(task.contains("'id'"));
        assert!(task.contains("session_item_id"));
        assert!(task.contains("Never recommend"));
    }
}
