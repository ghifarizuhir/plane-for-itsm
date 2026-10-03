//! Review briefing (AI): pre-meeting briefing for TCB/RCB sessions.
//! Spec: docs/superpowers/specs/2026-10-03-review-briefing-design.md

use axum::{http::StatusCode, Json};
use serde_json::{json, Value};
use uuid::Uuid;

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
