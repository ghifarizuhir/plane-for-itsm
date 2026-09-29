//! Jev (TypeSafe System One) decision client: `POST {base_url}/systemone`.
//!
//! Jev answers typed questions (noul/choice/score) about a `state` and returns
//! calibrated probabilities. Config resolution mirrors `llm.rs`.

use std::collections::BTreeMap;

use common::crypto::{decrypt_data, fernet_secret, skip_env_vars};
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::llm::{llm_config_from_env, llm_config_from_rows};

#[derive(Clone, PartialEq)]
pub struct DecisionConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl std::fmt::Debug for DecisionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionConfig")
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionError {
    NotConfigured,
    RateLimited,
    InvalidRequest,
    Upstream,
    Timeout,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    Noul {
        instructions: String,
        criteria: Option<(String, String)>,
    },
    Choice {
        instructions: String,
        criteria: Vec<(String, String)>,
    },
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Noul {
        probability: f64,
    },
    Choice {
        choice: String,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
    },
    Score {
        score: f64,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
    },
}

impl Answer {
    fn to_json(&self) -> Value {
        let probabilities = |values: &BTreeMap<String, f64>| {
            Value::Object(values.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
        };
        match self {
            Answer::Noul { probability } => json!({"type": "noul", "noul": probability}),
            Answer::Choice { choice, confidence, probabilities: p } => json!({
                "type": "choice",
                "choice": choice,
                "confidence": confidence,
                "probabilities": probabilities(p),
            }),
            Answer::Score { score, confidence, probabilities: p } => json!({
                "type": "score",
                "score": score,
                "confidence": confidence,
                "probabilities": probabilities(p),
            }),
        }
    }
}

pub fn answers_json(answers: &BTreeMap<String, Answer>) -> Value {
    Value::Object(
        answers
            .iter()
            .map(|(id, answer)| (id.clone(), answer.to_json()))
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionOutcome {
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

impl Question {
    fn to_json(&self) -> Value {
        match self {
            Question::Noul { instructions, criteria } => {
                let mut obj = Map::new();
                obj.insert("type".into(), json!("noul"));
                obj.insert("instructions".into(), json!(instructions));
                if let Some((yes, no)) = criteria {
                    obj.insert("criteria".into(), json!({"true": yes, "false": no}));
                }
                Value::Object(obj)
            }
            Question::Choice { instructions, criteria } => json!({
                "type": "choice",
                "instructions": instructions,
                "criteria": Value::Object(
                    criteria
                        .iter()
                        .map(|(key, value)| (key.clone(), json!(value)))
                        .collect(),
                ),
            }),
            Question::Score { instructions, criteria } => json!({
                "type": "score",
                "instructions": instructions,
                "criteria": criteria,
            }),
        }
    }
}

pub fn systemone_url(base_url: &str) -> String {
    format!("{}/systemone", base_url.trim_end_matches('/'))
}

pub fn questions_json(questions: &[(String, Question)]) -> Value {
    Value::Object(
        questions
            .iter()
            .map(|(id, question)| (id.clone(), question.to_json()))
            .collect(),
    )
}

/// Pure mapping from raw rows + env fallbacks. `None` when the decision model
/// is unset or no API key is available; `base_url`/key resolution is shared
/// with `llm_config_from_rows`.
pub fn decision_config_from_rows(
    rows: &[(String, Option<String>, bool)],
    env_api_key: String,
    env_decision_model: String,
    env_base_url: Option<String>,
    decrypt: impl Fn(&str) -> String,
) -> Option<DecisionConfig> {
    let base = llm_config_from_rows(rows, env_api_key, String::new(), env_base_url, decrypt);
    let model = match rows.iter().find(|(k, _, _)| k == "LLM_DECISION_MODEL") {
        Some((_, Some(value), _)) if !value.trim().is_empty() => value.trim().to_string(),
        _ if !env_decision_model.trim().is_empty() => env_decision_model.trim().to_string(),
        _ => return None,
    };
    if base.api_key.trim().is_empty() {
        return None;
    }
    Some(DecisionConfig {
        api_key: base.api_key.trim().to_string(),
        model,
        base_url: base.base_url,
    })
}

/// Resolve from DB rows or env per `SKIP_ENV_VAR` (mirrors `resolve_llm_config`).
pub async fn resolve_decision_config(pool: &PgPool) -> Option<DecisionConfig> {
    let (env_api_key, _, env_base_url) = llm_config_from_env();
    let env_model = std::env::var("LLM_DECISION_MODEL").unwrap_or_default();
    if !skip_env_vars() {
        return decision_config_from_rows(
            &[],
            env_api_key,
            env_model,
            env_base_url,
            |_| String::new(),
        );
    }
    let rows: Vec<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT key, value, is_encrypted FROM instance_configurations \
         WHERE key IN ('LLM_API_KEY','LLM_DECISION_MODEL') AND deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::warn!(error=%e, "decision: instance_configurations lookup failed");
        e
    })
    .unwrap_or_default();
    let secret = fernet_secret();
    decision_config_from_rows(&rows, env_api_key, env_model, env_base_url, |v| {
        decrypt_data(v, &secret)
    })
}

fn f64_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn probability_map(value: &Value) -> BTreeMap<String, f64> {
    value
        .get("probabilities")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_f64().map(|p| (k.clone(), p)))
                .collect()
        })
        .unwrap_or_default()
}

/// Read the System One response strictly: a missing required field is an
/// upstream error, not a default.
pub fn parse_outcome(value: &Value) -> Result<DecisionOutcome, DecisionError> {
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let answers_obj = value
        .get("answers")
        .and_then(Value::as_object)
        .ok_or(DecisionError::Upstream)?;
    let mut answers = BTreeMap::new();
    for (id, raw) in answers_obj {
        let answer = match raw.get("type").and_then(Value::as_str).unwrap_or_default() {
            "noul" => Answer::Noul {
                probability: f64_field(raw, "noul").ok_or(DecisionError::Upstream)?,
            },
            "choice" => Answer::Choice {
                choice: raw
                    .get("choice")
                    .and_then(Value::as_str)
                    .ok_or(DecisionError::Upstream)?
                    .to_string(),
                confidence: f64_field(raw, "confidence").unwrap_or(0.0),
                probabilities: probability_map(raw),
            },
            "score" => Answer::Score {
                score: f64_field(raw, "score").ok_or(DecisionError::Upstream)?,
                confidence: f64_field(raw, "confidence").unwrap_or(0.0),
                probabilities: probability_map(raw),
            },
            _ => return Err(DecisionError::Upstream),
        };
        answers.insert(id.clone(), answer);
    }
    let usage = value.get("usage").cloned().unwrap_or(Value::Null);
    Ok(DecisionOutcome {
        model,
        answers,
        input_tokens: usage.get("input_tokens").and_then(Value::as_i64).unwrap_or(0),
        output_tokens: usage.get("output_tokens").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn http() -> &'static reqwest::Client {
    static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("decision client")
    })
}

/// POST `{base_url}/systemone`; 429/422 mapped, other failures → `Upstream`,
/// timeout → `Timeout`. The API key is never logged.
pub async fn ask(
    config: &DecisionConfig,
    state: &Value,
    questions: Value,
) -> Result<DecisionOutcome, DecisionError> {
    let body = json!({"model": config.model, "state": state, "questions": questions});
    let resp = http()
        .post(systemone_url(&config.base_url))
        .header("Authorization", format!("Bearer {}", config.api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                DecisionError::Timeout
            } else {
                tracing::warn!(error=%e, "decision: upstream request failed");
                DecisionError::Upstream
            }
        })?;
    let status = resp.status();
    if status.as_u16() == 429 {
        return Err(DecisionError::RateLimited);
    }
    if status.as_u16() == 422 {
        return Err(DecisionError::InvalidRequest);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(500).collect();
        tracing::warn!(status = status.as_u16(), body = %snippet, "decision: upstream error");
        return Err(DecisionError::Upstream);
    }
    let value: Value = resp.json().await.map_err(|e| {
        tracing::warn!(error=%e, "decision: upstream returned invalid json");
        DecisionError::Upstream
    })?;
    parse_outcome(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(entries: &[(&str, Option<&str>, bool)]) -> Vec<(String, Option<String>, bool)> {
        entries
            .iter()
            .map(|(k, v, enc)| (k.to_string(), v.map(str::to_string), *enc))
            .collect()
    }

    #[test]
    fn config_requires_model_and_key() {
        let missing_model = rows(&[("LLM_DECISION_MODEL", None, false)]);
        assert_eq!(
            decision_config_from_rows(&missing_model, String::new(), String::new(), None, |_| String::new()),
            None
        );

        let missing_key = rows(&[("LLM_DECISION_MODEL", Some("typesafe/jev-1.13"), false)]);
        assert_eq!(
            decision_config_from_rows(&missing_key, String::new(), String::new(), None, |_| String::new()),
            None
        );

        let complete = rows(&[
            ("LLM_DECISION_MODEL", Some("typesafe/jev-1.13"), false),
            ("LLM_API_KEY", Some("secret"), false),
        ]);
        let cfg = decision_config_from_rows(
            &complete,
            String::new(),
            String::new(),
            Some("https://openrouter.ai/api/v1".into()),
            |_| String::new(),
        )
        .unwrap();
        assert_eq!(cfg.model, "typesafe/jev-1.13");
        assert_eq!(cfg.api_key, "secret");
        assert_eq!(cfg.base_url, "https://openrouter.ai/api/v1");
    }

    #[test]
    fn config_decrypts_key_and_falls_back_to_env_model() {
        let r = rows(&[("LLM_API_KEY", Some("enc"), true)]);
        let cfg = decision_config_from_rows(
            &r,
            "env-key".into(),
            "env-model".into(),
            None,
            |v| format!("plain-{v}"),
        )
        .unwrap();
        assert_eq!(cfg.api_key, "plain-enc");
        assert_eq!(cfg.model, "env-model");
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn systemone_url_joins_once() {
        assert_eq!(
            systemone_url("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1/systemone"
        );
    }

    #[test]
    fn questions_json_uses_typed_shapes() {
        let qs = vec![
            (
                "needs_human".to_string(),
                Question::Noul {
                    instructions: "Need human?".into(),
                    criteria: Some(("yes".into(), "no".into())),
                },
            ),
            (
                "severity".to_string(),
                Question::Score {
                    instructions: "How bad?".into(),
                    criteria: vec!["none".into(), "urgent".into()],
                },
            ),
        ];
        let v = questions_json(&qs);
        assert_eq!(v["needs_human"]["type"], "noul");
        assert_eq!(v["needs_human"]["criteria"]["true"], "yes");
        assert_eq!(v["severity"]["type"], "score");
        assert_eq!(v["severity"]["criteria"][1], "urgent");
    }

    #[test]
    fn parses_noul_choice_and_score_answers() {
        let value = json!({
            "model": "jev-1.13.0",
            "answers": {
                "needs_human": {"type": "noul", "noul": 0.78},
                "category": {"type": "choice", "choice": "Incident", "confidence": 0.87,
                             "probabilities": {"Incident": 0.87, "Problem": 0.13}},
                "severity": {"type": "score", "score": 3.2, "confidence": 0.9,
                             "legend": {"0": "none"}, "probabilities": {"3": 0.85, "4": 0.15}}
            },
            "usage": {"input_tokens": 42, "output_tokens": 7}
        });
        let outcome = parse_outcome(&value).unwrap();
        assert_eq!(outcome.model, "jev-1.13.0");
        assert_eq!(outcome.input_tokens, 42);
        assert_eq!(
            outcome.answers.get("needs_human"),
            Some(&Answer::Noul { probability: 0.78 })
        );
        match &outcome.answers["category"] {
            Answer::Choice { choice, confidence, .. } => {
                assert_eq!(choice, "Incident");
                assert!((confidence - 0.87).abs() < 1e-9);
            }
            other => panic!("expected choice, got {other:?}"),
        }
        match &outcome.answers["severity"] {
            Answer::Score { score, .. } => assert!((score - 3.2).abs() < 1e-9),
            other => panic!("expected score, got {other:?}"),
        }
    }

    #[test]
    fn rejects_answer_with_wrong_shape() {
        let value = json!({"answers": {"x": {"type": "choice"}}, "usage": {}});
        assert_eq!(parse_outcome(&value), Err(DecisionError::Upstream));
    }
}
