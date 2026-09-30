# Saran Triage Intake dengan Jev (TypeSafe) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah model keputusan kedua (`LLM_DECISION_MODEL`, Jev/System One via base URL LLM) dan memakainya untuk saran triage otomatis item intake: kategori (tipe work item project), severity (priority), needs-human — suggest-only, dijalankan worker (push + sweep), tampil di detail intake dengan Apply satu klik untuk priority.

**Architecture:** Klien Jev + builder pertanyaan + job DB ditempatkan di crate `ai` agar bisa diuji dari integration test `crates/api` (`ai::decision`, `ai::triage`, `ai::triage_job`). Worker hanya wrapper tipis + allowlist + beat tiap menit. API Rust menambah subresource `/triage-suggestion/` tanpa mengubah kontrak endpoint intake existing. Config admin lewat key instance baru yang di-seed data migration Django karena `configs_patch` Rust hanya meng-UPDATE row yang sudah ada.

**Tech Stack:** Rust (axum, sqlx, reqwest, serde_json), Redis Stream worker/beat, Django (data migration + seed config), React + MobX + vitest, TypeScript, i18n.

**Spec:** `docs/superpowers/specs/2026-09-29-jev-intake-triage-design.md`

**Execution notes (baca sebelum mulai):**

- Working tree punya banyak file unrelated yang termodifikasi — **jangan** `git add -A`; commit hanya file yang disebut task.
- Pre-commit menjalankan oxlint `--fix --deny-warnings` dan oxfmt pada file yang di-stage. Jangan pakai `key={index}`.
- `cargo test` DB butuh `DATABASE_URL` (default `postgres://plane:plane@localhost:5432/plane`). Suite scratch dijalankan seri: tambahkan `-- --test-threads=1`.
- Test yang memutasi env (`SKIP_ENV_VAR`, `LLM_*`) wajib `--test-threads=1`.
- Migration sqlx diterapkan saat boot api (`common::db::migrate`); untuk dev, terapkan manual dengan psql ke container `plane-for-itsm-plane-db-1`.
- Response `GET /triage-suggestion/` memakai `{ "data": ... }`; row `failed` → `data: null`.
- String FE: buka skill `translate` sebelum menyentuh `packages/i18n/src/locales`; plan ini menulis `en` dulu, locale lain mengikuti skill.
- Commit message mengikuti conventional commit: `feat(ai): ...`, `test(api-rs): ...`, `feat(web): ...`.

---

### Task 1: Klien Jev — config, questions, parsing (`crates/ai/src/decision.rs`)

**Files:**

- Create: `apps/api-rs/crates/ai/src/decision.rs`
- Modify: `apps/api-rs/crates/ai/src/lib.rs`
- Modify: `apps/api-rs/crates/ai/Cargo.toml`

- [ ] **Step 1: Tambah dependency `reqwest`**

Di `apps/api-rs/crates/ai/Cargo.toml`, tambahkan setelah baris `rig`:

```toml
reqwest = { workspace = true }
```

- [ ] **Step 2: Tulis test yang gagal**

Buat `apps/api-rs/crates/ai/src/decision.rs` dengan tipe + stub `todo!()` + test:

```rust
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

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionOutcome {
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub fn systemone_url(base_url: &str) -> String {
    todo!()
}

pub fn questions_json(questions: &[(String, Question)]) -> Value {
    todo!()
}

pub fn answers_json(answers: &BTreeMap<String, Answer>) -> Value {
    todo!()
}

pub fn decision_config_from_rows(
    rows: &[(String, Option<String>, bool)],
    env_api_key: String,
    env_decision_model: String,
    env_base_url: Option<String>,
    decrypt: impl Fn(&str) -> String,
) -> Option<DecisionConfig> {
    todo!()
}

pub async fn resolve_decision_config(pool: &PgPool) -> Option<DecisionConfig> {
    todo!()
}

pub fn parse_outcome(value: &Value) -> Result<DecisionOutcome, DecisionError> {
    todo!()
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
```

- [ ] **Step 3: Jalankan test — harus gagal**

Run: `cargo test -p ai decision -- --nocapture`
Expected: `panicked at 'not yet implemented'` (todo! di `decision_config_from_rows`/`questions_json`/`parse_outcome`).

- [ ] **Step 4: Implementasi penuh**

Ganti seluruh isi `apps/api-rs/crates/ai/src/decision.rs` dengan implementasi lengkap berikut, lalu tempelkan kembali `#[cfg(test)] mod tests { ... }` persis seperti Step 2 di akhir file:

```rust
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
```

- [ ] **Step 5: Daftarkan module di lib.rs**

Di `apps/api-rs/crates/ai/src/lib.rs`:

```rust
pub mod agent;
pub mod decision;
pub mod llm;
pub mod schedule;
pub mod tools;
```

- [ ] **Step 6: Jalankan test — harus lulus**

Run: `cargo test -p ai decision`
Expected: 6 test PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/ai/src/decision.rs apps/api-rs/crates/ai/src/lib.rs apps/api-rs/crates/ai/Cargo.toml apps/api-rs/Cargo.lock
git commit -m "feat(ai): add Jev systemone decision client"
```

---

### Task 2: Builder pertanyaan triage (`crates/ai/src/triage.rs`)

**Files:**

- Create: `apps/api-rs/crates/ai/src/triage.rs`
- Modify: `apps/api-rs/crates/ai/src/lib.rs`

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/api-rs/crates/ai/src/triage.rs` dengan stub + test:

```rust
//! Intake triage questions: state, typed questions over project types +
//! priorities, and mapping Jev answers onto suggestion fields.

use serde_json::{json, Value};
use uuid::Uuid;

use crate::decision::{Answer, DecisionOutcome, Question};

pub const MAX_TYPES: usize = 20;
pub const DESCRIPTION_LIMIT: usize = 4000;
pub const SEVERITY_LEVELS: [&str; 5] = ["none", "low", "medium", "high", "urgent"];

#[derive(Clone)]
pub struct TypeOption {
    pub type_id: Uuid,
    pub name: String,
    pub description: String,
}

pub struct TriageState<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub project_name: &'a str,
    pub source: &'a str,
}

pub fn truncate_chars(input: &str, limit: usize) -> String {
    todo!()
}

pub fn build_state(input: TriageState<'_>) -> Value {
    todo!()
}

pub fn build_questions(types: &[TypeOption]) -> Vec<(String, Question)> {
    todo!()
}

pub fn priority_from_score(score: f64) -> &'static str {
    todo!()
}

pub struct TriageOutcome {
    pub model: String,
    pub category_type_id: Option<Uuid>,
    pub category_label: Option<String>,
    pub category_confidence: Option<f64>,
    pub severity_priority: Option<String>,
    pub severity_score: Option<f64>,
    pub severity_confidence: Option<f64>,
    pub needs_human: Option<f64>,
    pub answers: Value,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub fn triage_outcome(outcome: DecisionOutcome, types: &[TypeOption]) -> TriageOutcome {
    todo!()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn type_option(name: &str) -> TypeOption {
        TypeOption {
            type_id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
        }
    }

    #[test]
    fn state_truncates_and_omits_empty_description() {
        let long = "x".repeat(DESCRIPTION_LIMIT + 50);
        let state = build_state(TriageState {
            name: "Login is broken",
            description: &long,
            project_name: "Support",
            source: "IN_APP",
        });
        assert_eq!(state["work_item"]["name"], "Login is broken");
        assert_eq!(
            state["work_item"]["description"].as_str().unwrap().chars().count(),
            DESCRIPTION_LIMIT
        );
        assert_eq!(state["project"]["name"], "Support");
        assert_eq!(state["source"], "IN_APP");

        let state = build_state(TriageState {
            name: "No description",
            description: "   ",
            project_name: "Support",
            source: "IN_APP",
        });
        assert!(state["work_item"].get("description").is_none());
    }

    #[test]
    fn questions_omit_category_without_types() {
        let questions = build_questions(&[]);
        let ids: Vec<&str> = questions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["severity", "needs_human"]);

        let types = vec![type_option("Incident"), type_option("Problem")];
        let questions = build_questions(&types);
        let ids: Vec<&str> = questions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["category", "severity", "needs_human"]);
        match &questions[0].1 {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria[0].0, "Incident");
                assert_eq!(criteria[0].1, "Incident");
                assert_eq!(criteria[1].0, "Problem");
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn questions_cap_types_and_order_severity_levels() {
        let types: Vec<TypeOption> = (0..30).map(|i| type_option(&format!("T{i}"))).collect();
        let questions = build_questions(&types);
        match &questions[0].1 {
            Question::Choice { criteria, .. } => assert_eq!(criteria.len(), MAX_TYPES),
            other => panic!("expected choice, got {other:?}"),
        }
        match &questions[1].1 {
            Question::Score { criteria, .. } => assert_eq!(criteria.len(), 5),
            other => panic!("expected score, got {other:?}"),
        }
    }

    #[test]
    fn priority_maps_rounding_boundaries() {
        assert_eq!(priority_from_score(0.0), "none");
        assert_eq!(priority_from_score(0.49), "none");
        assert_eq!(priority_from_score(0.5), "low");
        assert_eq!(priority_from_score(1.5), "medium");
        assert_eq!(priority_from_score(2.5), "high");
        assert_eq!(priority_from_score(3.6), "urgent");
        assert_eq!(priority_from_score(4.0), "urgent");
        assert_eq!(priority_from_score(9.0), "urgent");
    }

    #[test]
    fn outcome_maps_answers_to_fields() {
        let incident = type_option("Incident");
        let mut answers = BTreeMap::new();
        answers.insert(
            "category".to_string(),
            Answer::Choice {
                choice: "Incident".to_string(),
                confidence: 0.87,
                probabilities: BTreeMap::new(),
            },
        );
        answers.insert(
            "severity".to_string(),
            Answer::Score {
                score: 3.2,
                confidence: 0.91,
                probabilities: BTreeMap::new(),
            },
        );
        answers.insert(
            "needs_human".to_string(),
            Answer::Noul { probability: 0.78 },
        );
        let outcome = DecisionOutcome {
            model: "jev-1.13.0".to_string(),
            answers,
            input_tokens: 42,
            output_tokens: 7,
        };
        let mapped = triage_outcome(outcome, &[incident.clone()]);
        assert_eq!(mapped.category_type_id, Some(incident.type_id));
        assert_eq!(mapped.category_label.as_deref(), Some("Incident"));
        assert_eq!(mapped.severity_priority.as_deref(), Some("high"));
        assert_eq!(mapped.needs_human, Some(0.78));
        assert_eq!(mapped.answers["severity"]["score"], 3.2);
    }
}
```

- [ ] **Step 2: Jalankan test — harus gagal**

Run: `cargo test -p ai triage`
Expected: panic `not yet implemented`.

- [ ] **Step 3: Implementasi penuh**

Ganti stub fungsi di atas dengan:

```rust
pub fn truncate_chars(input: &str, limit: usize) -> String {
    input.chars().take(limit).collect()
}

pub fn build_state(input: TriageState<'_>) -> Value {
    let mut work_item = serde_json::Map::new();
    work_item.insert("name".into(), json!(input.name));
    let description = truncate_chars(input.description.trim(), DESCRIPTION_LIMIT);
    if !description.is_empty() {
        work_item.insert("description".into(), json!(description));
    }
    json!({
        "work_item": work_item,
        "project": {"name": input.project_name},
        "source": input.source,
    })
}

const SEVERITY_CRITERIA: [&str; 5] = [
    "No user impact; cosmetic or informational",
    "Minor issue with an available workaround",
    "Degraded functionality; workaround is difficult",
    "Major functionality blocked for users",
    "Critical outage, security incident, or data loss",
];

pub fn build_questions(types: &[TypeOption]) -> Vec<(String, Question)> {
    let mut questions = Vec::new();
    if !types.is_empty() {
        let criteria = types
            .iter()
            .take(MAX_TYPES)
            .map(|t| {
                let description = if t.description.trim().is_empty() {
                    t.name.clone()
                } else {
                    t.description.trim().to_string()
                };
                (t.name.clone(), description)
            })
            .collect();
        questions.push((
            "category".to_string(),
            Question::Choice {
                instructions: "Which work item type best fits this intake request?".to_string(),
                criteria,
            },
        ));
    }
    questions.push((
        "severity".to_string(),
        Question::Score {
            instructions: "How severe is this request?".to_string(),
            criteria: SEVERITY_CRITERIA.iter().map(|s| s.to_string()).collect(),
        },
    ));
    questions.push((
        "needs_human".to_string(),
        Question::Noul {
            instructions: "Does this request need a human to handle it?".to_string(),
            criteria: Some((
                "A person should review or handle this request".to_string(),
                "Routine request suitable for normal automated triage".to_string(),
            )),
        },
    ));
    questions
}

pub fn priority_from_score(score: f64) -> &'static str {
    let index = score.round().clamp(0.0, (SEVERITY_LEVELS.len() - 1) as f64) as usize;
    SEVERITY_LEVELS[index]
}

pub fn triage_outcome(outcome: DecisionOutcome, types: &[TypeOption]) -> TriageOutcome {
    let mut category: Option<(Option<Uuid>, String, f64)> = None;
    let mut severity: Option<(String, f64, f64)> = None;
    let mut needs_human = None;
    for (id, answer) in &outcome.answers {
        match (id.as_str(), answer) {
            ("category", Answer::Choice { choice, confidence, .. }) => {
                let type_id = types.iter().find(|t| &t.name == choice).map(|t| t.type_id);
                category = Some((type_id, choice.clone(), *confidence));
            }
            ("severity", Answer::Score { score, confidence, .. }) => {
                severity = Some((priority_from_score(*score).to_string(), *score, *confidence));
            }
            ("needs_human", Answer::Noul { probability }) => needs_human = Some(*probability),
            _ => {}
        }
    }
    let answers = crate::decision::answers_json(&outcome.answers);
    TriageOutcome {
        model: outcome.model,
        category_type_id: category.as_ref().and_then(|(id, _, _)| *id),
        category_label: category.as_ref().map(|(_, label, _)| label.clone()),
        category_confidence: category.as_ref().map(|(_, _, confidence)| *confidence),
        severity_priority: severity.as_ref().map(|(priority, _, _)| priority.clone()),
        severity_score: severity.as_ref().map(|(_, score, _)| *score),
        severity_confidence: severity.as_ref().map(|(_, _, confidence)| *confidence),
        needs_human,
        answers,
        input_tokens: outcome.input_tokens,
        output_tokens: outcome.output_tokens,
    }
}
```

- [ ] **Step 4: Daftarkan module**

Di `apps/api-rs/crates/ai/src/lib.rs`, tambahkan `pub mod triage;`.

- [ ] **Step 5: Jalankan test — harus lulus**

Run: `cargo test -p ai triage`
Expected: 5 test PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/triage.rs apps/api-rs/crates/ai/src/lib.rs
git commit -m "feat(ai): build intake triage state and questions"
```

---

### Task 3: Migration tabel saran (`0010_intake_triage_suggestions.sql`)

**Files:**

- Create: `apps/api-rs/migrations/0010_intake_triage_suggestions.sql`

- [ ] **Step 1: Tulis migration**

```sql
-- AI intake triage: Jev suggestions per intake_issue row.
-- Applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.intake_triage_suggestions (
    id uuid NOT NULL,
    intake_issue_id uuid NOT NULL REFERENCES public.intake_issues(id) ON DELETE CASCADE,
    project_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    status character varying(10) NOT NULL DEFAULT 'pending',
    model character varying(255),
    answers jsonb,
    category_type_id uuid,
    category_label character varying(255),
    category_confidence double precision,
    severity_priority character varying(10),
    severity_score double precision,
    severity_confidence double precision,
    needs_human double precision,
    applied_fields text[] NOT NULL DEFAULT '{}',
    dismissed_fields text[] NOT NULL DEFAULT '{}',
    attempts integer NOT NULL DEFAULT 0,
    last_error text,
    input_tokens integer,
    output_tokens integer,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT intake_triage_suggestions_status_check
        CHECK (status IN ('pending','ready','failed')),
    CONSTRAINT intake_triage_suggestions_severity_check
        CHECK (severity_priority IS NULL OR severity_priority IN ('none','low','medium','high','urgent'))
);

CREATE UNIQUE INDEX IF NOT EXISTS intake_triage_suggestions_issue_idx
    ON public.intake_triage_suggestions (intake_issue_id);

CREATE INDEX IF NOT EXISTS intake_triage_suggestions_retry_idx
    ON public.intake_triage_suggestions (updated_at) WHERE status = 'failed';
```

- [ ] **Step 2: Terapkan migration ke DB dev**

Run: `docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane < apps/api-rs/migrations/0010_intake_triage_suggestions.sql`
Expected: `CREATE TABLE`, `CREATE INDEX`, `CREATE INDEX`.

- [ ] **Step 3: Verifikasi tabel**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "\d intake_triage_suggestions"`
Expected: daftar kolom + 2 index + constraint status.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0010_intake_triage_suggestions.sql
git commit -m "feat(api-rs): add intake triage suggestions table"
```

---

### Task 4: Job klasifikasi (`crates/ai/src/triage_job.rs`)

**Files:**

- Create: `apps/api-rs/crates/ai/src/triage_job.rs`
- Modify: `apps/api-rs/crates/ai/src/lib.rs`

- [ ] **Step 1: Tulis job**

```rust
//! DB-backed triage jobs shared by the worker: claim a pending intake item,
//! ask Jev, persist the suggestion; list sweep candidates.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::decision::{self, DecisionError};
use crate::triage::{self, TypeOption};

pub const MAX_ATTEMPTS: i32 = 3;
pub const SWEEP_LIMIT: i64 = 10;
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(sqlx::FromRow)]
struct PendingItem {
    intake_issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    name: String,
    description: Option<String>,
    source: Option<String>,
    project_name: String,
}

async fn load_item(pool: &PgPool, intake_issue_id: Uuid) -> Result<Option<PendingItem>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ii.id AS intake_issue_id, ii.project_id, ii.workspace_id, \
                i.name, i.description_stripped AS description, ii.source, p.name AS project_name \
         FROM intake_issues ii \
         JOIN issues i ON i.id = ii.issue_id \
         JOIN projects p ON p.id = ii.project_id \
         WHERE ii.id = $1 AND ii.deleted_at IS NULL AND ii.status = -2 AND i.deleted_at IS NULL",
    )
    .bind(intake_issue_id)
    .fetch_optional(pool)
    .await
}

async fn load_types(pool: &PgPool, project_id: Uuid) -> Result<Vec<TypeOption>, sqlx::Error> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT t.id, t.name, t.description FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL \
           AND t.deleted_at IS NULL AND t.is_epic = false \
         ORDER BY t.name LIMIT $2",
    )
    .bind(project_id)
    .bind(triage::MAX_TYPES as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(type_id, name, description)| TypeOption { type_id, name, description })
        .collect())
}

/// Insert a pending row; on conflict take over a failed row below the attempt
/// cap. `true` means this invocation owns the item.
async fn claim(
    pool: &PgPool,
    intake_issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO intake_triage_suggestions \
            (id, intake_issue_id, project_id, workspace_id, status, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'pending', now(), now()) \
         ON CONFLICT (intake_issue_id) DO NOTHING RETURNING id",
    )
    .bind(intake_issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    if inserted.is_some() {
        return Ok(true);
    }
    let retried: Option<Uuid> = sqlx::query_scalar(
        "UPDATE intake_triage_suggestions SET status = 'pending', updated_at = now() \
         WHERE intake_issue_id = $1 AND status = 'failed' AND attempts < $2 RETURNING id",
    )
    .bind(intake_issue_id)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(pool)
    .await?;
    Ok(retried.is_some())
}

async fn save_ready(
    pool: &PgPool,
    intake_issue_id: Uuid,
    mapped: &triage::TriageOutcome,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE intake_triage_suggestions SET \
            status = 'ready', model = $2, answers = $3, \
            category_type_id = $4, category_label = $5, category_confidence = $6, \
            severity_priority = $7, severity_score = $8, severity_confidence = $9, \
            needs_human = $10, input_tokens = $11, output_tokens = $12, \
            last_error = NULL, updated_at = now() \
         WHERE intake_issue_id = $1",
    )
    .bind(intake_issue_id)
    .bind(&mapped.model)
    .bind(&mapped.answers)
    .bind(mapped.category_type_id)
    .bind(&mapped.category_label)
    .bind(mapped.category_confidence)
    .bind(&mapped.severity_priority)
    .bind(mapped.severity_score)
    .bind(mapped.severity_confidence)
    .bind(mapped.needs_human)
    .bind(mapped.input_tokens as i32)
    .bind(mapped.output_tokens as i32)
    .execute(pool)
    .await?;
    Ok(())
}

async fn save_failed(pool: &PgPool, intake_issue_id: Uuid, message: &str) -> Result<(), sqlx::Error> {
    let truncated: String = message.chars().take(500).collect();
    sqlx::query(
        "UPDATE intake_triage_suggestions SET status = 'failed', attempts = attempts + 1, \
         last_error = $2, updated_at = now() WHERE intake_issue_id = $1",
    )
    .bind(intake_issue_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}

fn decision_error_message(error: DecisionError) -> &'static str {
    match error {
        DecisionError::NotConfigured => "decision model is not configured",
        DecisionError::RateLimited => "decision model rate limited",
        DecisionError::InvalidRequest => "decision request was rejected (422)",
        DecisionError::Upstream => "decision upstream error",
        DecisionError::Timeout => "decision request timed out",
    }
}

/// Classify one intake item. No configuration → no row (sweep retries later);
/// already claimed/ready → no-op; decision failures are stored, not returned.
pub async fn classify(pool: &PgPool, intake_issue_id: Uuid) -> Result<(), sqlx::Error> {
    let Some(config) = decision::resolve_decision_config(pool).await else {
        tracing::debug!(intake_issue_id=%intake_issue_id, "ai.intake.triage: decision model not configured");
        return Ok(());
    };
    let Some(item) = load_item(pool, intake_issue_id).await? else {
        tracing::debug!(intake_issue_id=%intake_issue_id, "ai.intake.triage: item not pending");
        return Ok(());
    };
    if !claim(pool, item.intake_issue_id, item.project_id, item.workspace_id).await? {
        tracing::debug!(intake_issue_id=%intake_issue_id, "ai.intake.triage: already claimed");
        return Ok(());
    }
    let types = load_types(pool, item.project_id).await?;
    let state = triage::build_state(triage::TriageState {
        name: &item.name,
        description: item.description.as_deref().unwrap_or(""),
        project_name: &item.project_name,
        source: item.source.as_deref().unwrap_or("IN_APP"),
    });
    let questions = decision::questions_json(&triage::build_questions(&types));
    match tokio::time::timeout(REQUEST_TIMEOUT, decision::ask(&config, &state, questions)).await {
        Ok(Ok(outcome)) => {
            let mapped = triage::triage_outcome(outcome, &types);
            save_ready(pool, item.intake_issue_id, &mapped).await?;
        }
        Ok(Err(error)) => {
            tracing::warn!(intake_issue_id=%intake_issue_id, error=?error, "ai.intake.triage: decision failed");
            save_failed(pool, item.intake_issue_id, decision_error_message(error)).await?;
        }
        Err(_) => {
            save_failed(pool, item.intake_issue_id, "decision request timed out").await?;
        }
    }
    Ok(())
}

/// Pending intake items that still need a suggestion: no row at all, or a
/// failed row past the 5-minute cooldown with attempts left.
pub async fn sweep_candidates(pool: &PgPool, limit: i64) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT ii.id FROM intake_issues ii \
         LEFT JOIN intake_triage_suggestions s ON s.intake_issue_id = ii.id \
         WHERE ii.status = -2 AND ii.deleted_at IS NULL \
           AND (s.id IS NULL OR (s.status = 'failed' AND s.attempts < $2 \
                AND s.updated_at < now() - interval '5 minutes')) \
         ORDER BY ii.created_at ASC LIMIT $1",
    )
    .bind(limit)
    .bind(MAX_ATTEMPTS)
    .fetch_all(pool)
    .await
}
```

- [ ] **Step 2: Daftarkan module**

Di `apps/api-rs/crates/ai/src/lib.rs`, tambahkan `pub mod triage_job;`.

- [ ] **Step 3: Build**

Run: `cargo build -p ai`
Expected: sukses tanpa warning unused.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/ai/src/triage_job.rs apps/api-rs/crates/ai/src/lib.rs
git commit -m "feat(ai): add triage classify and sweep job logic"
```

---

### Task 5: Worker wiring + beat sweep

**Files:**

- Create: `apps/api-rs/crates/worker/src/handlers/intake_triage.rs`
- Modify: `apps/api-rs/crates/worker/src/handlers/mod.rs`
- Modify: `apps/api-rs/crates/beat/src/main.rs`

- [ ] **Step 1: Update allowlist test — harus gagal**

Di `apps/api-rs/crates/worker/src/handlers/mod.rs`, ganti test `only_ai_schedule_jobs_are_enabled` menjadi:

```rust
    #[test]
    fn only_validated_jobs_are_enabled() {
        assert!(is_enabled_job("ai.schedule.tick"));
        assert!(is_enabled_job("ai.schedule.run"));
        assert!(is_enabled_job("ai.intake.triage"));
        assert!(is_enabled_job("ai.intake.triage.sweep"));
        assert!(!is_enabled_job("email.notification"));
        assert!(!is_enabled_job("issue.archive"));
        assert!(!is_enabled_job(""));
    }
```

Run: `cargo test -p worker only_validated_jobs_are_enabled`
Expected: FAIL (assertion `ai.intake.triage`).

- [ ] **Step 2: Implementasi handler**

Buat `apps/api-rs/crates/worker/src/handlers/intake_triage.rs`:

```rust
//! Jev intake triage jobs: classify one item, or sweep pending items.

use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn classify(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let Some(intake_issue_id) = payload
        .get("intake_issue_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        tracing::warn!(payload=%payload, "ai.intake.triage: missing intake_issue_id");
        return Ok(());
    };
    ai::triage_job::classify(pool, intake_issue_id).await?;
    Ok(())
}

pub async fn sweep(pool: &PgPool, redis: &mut ConnectionManager) -> anyhow::Result<()> {
    if ai::decision::resolve_decision_config(pool).await.is_none() {
        tracing::debug!("ai.intake.triage.sweep: decision model not configured");
        return Ok(());
    }
    let candidates = ai::triage_job::sweep_candidates(pool, ai::triage_job::SWEEP_LIMIT).await?;
    for intake_issue_id in &candidates {
        if let Err(error) = common::stream::push_job(
            redis,
            "ai.intake.triage",
            json!({"intake_issue_id": intake_issue_id}),
        )
        .await
        {
            tracing::error!(intake_issue_id=%intake_issue_id, error=%error, "ai.intake.triage.sweep: push failed");
        }
    }
    tracing::info!(queued=%candidates.len(), "ai.intake.triage.sweep done");
    Ok(())
}
```

- [ ] **Step 3: Daftarkan module, allowlist, dan dispatch**

Di `apps/api-rs/crates/worker/src/handlers/mod.rs`:

1. Tambah module setelah `pub mod issue_automation;`:

```rust
pub mod intake_triage;
```

2. Ganti `is_enabled_job`:

```rust
pub fn is_enabled_job(name: &str) -> bool {
    matches!(
        name,
        "ai.schedule.tick" | "ai.schedule.run" | "ai.intake.triage" | "ai.intake.triage.sweep"
    )
}
```

3. Tambah arm di `handle_by_id` sebelum arm `_`:

```rust
        "ai.intake.triage" => intake_triage::classify(pool, payload).await,
        "ai.intake.triage.sweep" => intake_triage::sweep(pool, redis).await,
```

- [ ] **Step 4: Jadwalkan beat tiap menit**

Di `apps/api-rs/crates/beat/src/main.rs`, setelah blok `ai.schedule.tick` (baris ~115), tambahkan:

```rust
    // Every minute: AI intake triage sweep — backfill/retry unclassified items.
    {
        let r = redis.clone();
        sched
            .add(
                tokio_cron_scheduler::Job::new_async("0 * * * * *", move |_, _| {
                    let mut rr = r.clone();
                    Box::pin(async move {
                        let _ =
                            common::stream::push_job(&mut rr, "ai.intake.triage.sweep", json!({}))
                                .await;
                    })
                })
                .unwrap(),
            )
            .await
            .unwrap();
    }
```

- [ ] **Step 5: Jalankan test + build**

Run: `cargo test -p worker && cargo build -p beat`
Expected: PASS + build sukses.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/worker/src/handlers/intake_triage.rs apps/api-rs/crates/worker/src/handlers/mod.rs apps/api-rs/crates/beat/src/main.rs
git commit -m "feat(api-rs): enable intake triage worker jobs and sweep"
```

---

### Task 6: Push job saat intake dibuat

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs:693`

- [ ] **Step 1: Tambah push setelah commit**

Di `create_issue`, setelah `tx.commit().await?;` (baris 693) dan sebelum `let issue_id = issue.id;`, sisipkan:

```rust
    // Queue Jev triage suggestions. A Redis hiccup must never fail the create;
    // the worker sweep backfills missed jobs.
    if let Ok(mut redis) = st.redis_client().await {
        if let Err(error) = common::stream::push_job(
            &mut redis,
            "ai.intake.triage",
            json!({ "intake_issue_id": row.id }),
        )
        .await
        {
            tracing::warn!(intake_issue_id=%row.id, error=%error, "ai.intake.triage: push failed");
        }
    }
```

- [ ] **Step 2: Build**

Run: `cargo build -p api`
Expected: sukses.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs
git commit -m "feat(api-rs): queue intake triage on create"
```

---

### Task 7: Endpoint subresource `triage-suggestion`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` (append di akhir file)
- Modify: `apps/api-rs/crates/api/src/main.rs:858` (setelah route `intake-issues/:pk/`)

- [ ] **Step 1: Tambah query, serializer, dan handler**

Di akhir `apps/api-rs/crates/api/src/routes/intake.rs`, tambahkan:

```rust
// ============================================================================
// AI triage suggestions (Jev).
// ============================================================================

const TRIAGE_FIELDS: [&str; 3] = ["category", "severity", "needs_human"];
const TRIAGE_APPLY_FIELDS: [&str; 1] = ["severity"];

#[derive(Debug, Clone, Deserialize)]
pub struct TriageFieldsBody {
    #[serde(default)]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TriageRow {
    id: uuid::Uuid,
    status: String,
    model: Option<String>,
    answers: Option<Value>,
    category_type_id: Option<uuid::Uuid>,
    category_label: Option<String>,
    category_confidence: Option<f64>,
    severity_priority: Option<String>,
    severity_score: Option<f64>,
    severity_confidence: Option<f64>,
    needs_human: Option<f64>,
    applied_fields: Vec<String>,
    dismissed_fields: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn fetch_triage_row(
    pool: &sqlx::PgPool,
    row_id: uuid::Uuid,
) -> Result<Option<TriageRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, status, model, answers, category_type_id, category_label, \
                category_confidence, severity_priority, severity_score, severity_confidence, \
                needs_human, applied_fields, dismissed_fields, created_at \
         FROM intake_triage_suggestions WHERE intake_issue_id = $1",
    )
    .bind(row_id)
    .fetch_optional(pool)
    .await
}

/// `probabilities` dari jawaban mentah; level Score dipetakan dari indeks
/// (`"0".."4"`) ke nama priority lewat `level_names`.
fn probability_json(value: &Value, level_names: Option<&[&str]>) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(map) = value.get("probabilities").and_then(Value::as_object) {
        for (key, raw) in map {
            let label = level_names
                .and_then(|names| key.parse::<usize>().ok().and_then(|index| names.get(index)))
                .map(|name| name.to_string())
                .unwrap_or_else(|| key.clone());
            out.insert(label, json!(raw.as_f64().unwrap_or(0.0)));
        }
    }
    Value::Object(out)
}

fn triage_json(row: &TriageRow) -> Value {
    let ready = row.status == "ready";
    let empty = json!({});
    let answers = row.answers.as_ref().unwrap_or(&empty);
    let category = if ready {
        row.category_label.as_ref().map(|label| {
            json!({
                "type_id": row.category_type_id,
                "label": label,
                "confidence": row.category_confidence,
                "probabilities": answers
                    .get("category")
                    .map(|value| probability_json(value, None))
                    .unwrap_or_else(|| json!({})),
            })
        })
    } else {
        None
    };
    let severity = if ready {
        row.severity_priority.as_ref().map(|priority| {
            json!({
                "priority": priority,
                "score": row.severity_score,
                "confidence": row.severity_confidence,
                "probabilities": answers
                    .get("severity")
                    .map(|value| probability_json(value, Some(&ai::triage::SEVERITY_LEVELS[..])))
                    .unwrap_or_else(|| json!({})),
            })
        })
    } else {
        None
    };
    let needs_human = if ready {
        row.needs_human
            .map(|probability| json!({"probability": probability}))
    } else {
        None
    };
    json!({
        "id": row.id,
        "status": row.status,
        "model": row.model,
        "category": category,
        "severity": severity,
        "needs_human": needs_human,
        "applied_fields": row.applied_fields,
        "dismissed_fields": row.dismissed_fields,
        "created_at": row.created_at,
    })
}

fn triage_bad_request(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"error": message})))
}

/// GET `.../intake-issues/:pk/triage-suggestion/` — `data` = row pending/ready,
/// `null` untuk missing/failed.
pub async fn get_triage_suggestion(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug)
        .await?
        .is_none()
    {
        return Ok(crate::routes::member::deny_detail());
    }
    let Some(scope) = resolve_inbox_row(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    let row = fetch_triage_row(&st.pool, scope.row_id).await?;
    match row {
        Some(row) if row.status == "pending" || row.status == "ready" => {
            Ok((StatusCode::OK, Json(json!({"data": triage_json(&row)}))))
        }
        _ => Ok((StatusCode::OK, Json(json!({"data": null})))),
    }
}

/// POST `.../triage-suggestion/apply/` — v1 hanya `severity` (menulis
/// `issues.priority`), idempotent.
pub async fn apply_triage_suggestion(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<TriageFieldsBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug)
        .await?
        .is_none()
    {
        return Ok(crate::routes::member::deny_detail());
    }
    let Some(scope) = resolve_inbox_row(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    let Some(row) = fetch_triage_row(&st.pool, scope.row_id).await? else {
        return Ok(missing());
    };
    if row.status != "ready" {
        return Ok(triage_bad_request("Triage suggestion is not ready"));
    }
    if body.fields.is_empty() || body.fields.iter().any(|f| !TRIAGE_FIELDS.contains(&f.as_str())) {
        return Ok(triage_bad_request("Invalid triage fields"));
    }
    if body
        .fields
        .iter()
        .any(|field| row.dismissed_fields.contains(field))
    {
        return Ok(triage_bad_request("Triage field was dismissed"));
    }
    if body
        .fields
        .iter()
        .any(|field| !TRIAGE_APPLY_FIELDS.contains(&field.as_str()))
    {
        return Ok(triage_bad_request("Only severity can be applied"));
    }
    if let Some(priority) = &row.severity_priority {
        if !row.applied_fields.iter().any(|field| field == "severity") {
            let mut tx = st.pool.begin().await?;
            sqlx::query(
                "UPDATE issues SET priority = $1, updated_at = now(), updated_by_id = $2 \
                 WHERE id = $3 AND deleted_at IS NULL",
            )
            .bind(priority)
            .bind(auth.0)
            .bind(scope.issue_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE intake_triage_suggestions \
                 SET applied_fields = array_append(applied_fields, 'severity'), updated_at = now() \
                 WHERE id = $1",
            )
            .bind(row.id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
    }
    let refreshed = fetch_triage_row(&st.pool, scope.row_id).await?.unwrap_or(row);
    Ok((StatusCode::OK, Json(json!({"data": triage_json(&refreshed)}))))
}

/// POST `.../triage-suggestion/dismiss/` — tandai field diabaikan; idempotent.
pub async fn dismiss_triage_suggestion(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<TriageFieldsBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if crate::routes::project::ws_role(&st.pool, auth.0, &slug)
        .await?
        .is_none()
    {
        return Ok(crate::routes::member::deny_detail());
    }
    let Some(scope) = resolve_inbox_row(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    let Some(row) = fetch_triage_row(&st.pool, scope.row_id).await? else {
        return Ok(missing());
    };
    if row.status != "ready" {
        return Ok(triage_bad_request("Triage suggestion is not ready"));
    }
    if body.fields.is_empty() || body.fields.iter().any(|f| !TRIAGE_FIELDS.contains(&f.as_str())) {
        return Ok(triage_bad_request("Invalid triage fields"));
    }
    if body
        .fields
        .iter()
        .any(|field| row.applied_fields.contains(field))
    {
        return Ok(triage_bad_request("Triage field was already applied"));
    }
    let mut merged = row.dismissed_fields.clone();
    for field in &body.fields {
        if !merged.contains(field) {
            merged.push(field.clone());
        }
    }
    sqlx::query(
        "UPDATE intake_triage_suggestions SET dismissed_fields = $2, updated_at = now() WHERE id = $1",
    )
    .bind(row.id)
    .bind(&merged)
    .execute(&st.pool)
    .await?;
    let refreshed = fetch_triage_row(&st.pool, scope.row_id).await?.unwrap_or(row);
    Ok((StatusCode::OK, Json(json!({"data": triage_json(&refreshed)}))))
}
```

- [ ] **Step 2: Daftarkan route**

Di `apps/api-rs/crates/api/src/main.rs`, setelah route `/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/` (baris ~855-858), tambahkan:

```rust
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/triage-suggestion/",
            get(routes::intake::get_triage_suggestion),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/triage-suggestion/apply/",
            post(routes::intake::apply_triage_suggestion),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/triage-suggestion/dismiss/",
            post(routes::intake::dismiss_triage_suggestion),
        )
```

- [ ] **Step 3: Build + route-shape test**

Run: `cargo build -p api && cargo test -p api --test route_inventory_test`
Expected: build sukses; semua test PASS (tidak ada konflik route).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api-rs): expose intake triage suggestion endpoints"
```

---

### Task 8: Integration test end-to-end (DB)

**Files:**

- Modify: `apps/api-rs/crates/api/tests/support/mod.rs`
- Create: `apps/api-rs/crates/api/tests/intake_triage_test.rs`

- [ ] **Step 1: Tambah fake upstream System One**

Di akhir `apps/api-rs/crates/api/tests/support/mod.rs`, tambahkan:

```rust
/// Fake System One (`POST /v1/systemone`) upstream that records every request
/// body and answers with `response` for every call.
pub async fn spawn_systemone_upstream(
    response: Value,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::{routing::post, Json, Router};
    type Recorder = std::sync::Arc<std::sync::Mutex<Vec<Value>>>;
    let bodies: Recorder = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = bodies.clone();
    async fn handler(
        axum::extract::State((recorder, response)): axum::extract::State<(Recorder, Value)>,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        recorder.lock().unwrap().push(body);
        (axum::http::StatusCode::OK, Json(response))
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/systemone", post(handler))
                .with_state((recorder, response)),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}/v1"), bodies)
}
```

- [ ] **Step 2: Tulis test integrasi**

Buat `apps/api-rs/crates/api/tests/intake_triage_test.rs`:

```rust
//! DB-backed Jev intake triage tests. These tests mutate the process env, so
//! run with `--test-threads=1`.

mod support;

use api::middleware::auth::AuthUser;
use api::routes::intake::{
    apply_triage_suggestion, dismiss_triage_suggestion, get_triage_suggestion, CreateIntakeIssue,
    IntakeIssuePayload, TriageFieldsBody,
};
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::{json, Value};
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

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("trg-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Triage', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(user_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch workspace member");
        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'Triage', '', 2, $2, $3, false, false, false, false, \
             false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(format!("TRG{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase())
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");
        sqlx::query(
            "INSERT INTO project_members (id, created_at, updated_at, role, member_id, \
             project_id, workspace_id, is_active) \
             VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, $3, true)",
        )
        .bind(user_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project member");
        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
        }
    }

    async fn add_intake(&self, pool: &PgPool) {
        sqlx::query(
            "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, \
             project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Intake', '', true, '{}'::jsonb, '{}'::jsonb, $1, $2, \
             now(), now())",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch intake");
    }

    async fn add_type(&self, pool: &PgPool, name: &str) -> Uuid {
        let type_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
             is_active, level, workspace_id, created_at, updated_at) \
             VALUES ($1, $2, 'Something is broken', '{}'::jsonb, false, false, true, 0, $3, \
             now(), now())",
        )
        .bind(type_id)
        .bind(name)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch type");
        sqlx::query(
            "INSERT INTO project_issue_types (id, created_at, updated_at, project_id, \
             workspace_id, issue_type_id, level, is_default) \
             VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, 0, false)",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .bind(type_id)
        .execute(pool)
        .await
        .expect("scratch project type");
        type_id
    }

    async fn create_item(&self, state: &AppState, name: &str) -> (Uuid, Uuid) {
        let (status, Json(detail)) = api::routes::intake::create_issue(
            State(state.clone()),
            AuthUser(self.user_id),
            Path((self.slug.clone(), self.project_id)),
            Json(CreateIntakeIssue {
                issue: IntakeIssuePayload {
                    name: Some(name.to_string()),
                    priority: None,
                },
            }),
        )
        .await
        .expect("intake create");
        assert_eq!(status, StatusCode::OK);
        let row_id = Uuid::parse_str(detail["id"].as_str().unwrap()).unwrap();
        let issue_id = Uuid::parse_str(detail["issue"]["id"].as_str().unwrap()).unwrap();
        (row_id, issue_id)
    }

    async fn purge(&self, pool: &PgPool) {
        for statement in [
            "DELETE FROM intake_triage_suggestions WHERE workspace_id = $1",
            "DELETE FROM intake_issues WHERE workspace_id = $1",
            "DELETE FROM issues WHERE workspace_id = $1",
            "DELETE FROM states WHERE workspace_id = $1",
            "DELETE FROM intakes WHERE workspace_id = $1",
            "DELETE FROM project_issue_types WHERE workspace_id = $1",
            "DELETE FROM issue_types WHERE workspace_id = $1",
            "DELETE FROM project_members WHERE workspace_id = $1",
            "DELETE FROM workspace_members WHERE workspace_id = $1",
        ] {
            sqlx::query(statement).bind(self.workspace_id).execute(pool).await.ok();
        }
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE username LIKE $1")
            .bind(format!("{}%", self.slug))
            .execute(pool)
            .await
            .ok();
    }
}

fn set_decision_env(base_url: &str) {
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", base_url);
    std::env::set_var("LLM_DECISION_MODEL", "typesafe/jev-1.13");
}

fn clear_decision_env() {
    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_DECISION_MODEL");
}

#[tokio::test]
async fn classify_stores_suggestion_and_apply_dismiss_work() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    scratch.add_type(&pool, "Incident").await;
    let st = state(&pool).await;

    let (base_url, bodies) = support::spawn_systemone_upstream(json!({
        "model": "jev-1.13.0",
        "answers": {
            "category": {"type": "choice", "choice": "Incident", "confidence": 0.87,
                         "probabilities": {"Incident": 0.87, "Problem": 0.13}},
            "severity": {"type": "score", "score": 3.2, "confidence": 0.91,
                         "legend": {"0": "none", "1": "low", "2": "medium", "3": "high", "4": "urgent"},
                         "probabilities": {"3": 0.85, "4": 0.15}},
            "needs_human": {"type": "noul", "noul": 0.78}
        },
        "usage": {"input_tokens": 120, "output_tokens": 12}
    }))
    .await;
    set_decision_env(&base_url);

    let (row_id, issue_id) = scratch.create_item(&st, "Login is broken").await;
    ai::triage_job::classify(&pool, row_id).await.expect("classify");

    // Request terkirim ke `{base}/systemone` dengan pertanyaan bertipe.
    let sent = bodies.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["model"], "typesafe/jev-1.13");
    assert!(sent[0]["questions"]["category"]["criteria"].get("Incident").is_some());
    assert_eq!(
        sent[0]["questions"]["severity"]["criteria"][3],
        "Major functionality blocked for users"
    );

    let (status, Json(body)) = get_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .expect("get suggestion");
    assert_eq!(status, StatusCode::OK);
    let suggestion = &body["data"];
    assert_eq!(suggestion["status"], "ready");
    assert_eq!(suggestion["model"], "jev-1.13.0");
    assert_eq!(suggestion["category"]["label"], "Incident");
    assert_eq!(suggestion["severity"]["priority"], "high");
    assert_eq!(suggestion["severity"]["probabilities"]["high"], 0.85);
    assert_eq!(suggestion["needs_human"]["probability"], 0.78);

    let (status, Json(applied)) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["severity".to_string()],
        }),
    )
    .await
    .expect("apply");
    assert_eq!(status, StatusCode::OK);
    assert!(applied["data"]["applied_fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "severity"));
    let priority: String = sqlx::query_scalar("SELECT priority FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(priority, "high");

    let (status, Json(dismissed)) = dismiss_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["category".to_string(), "needs_human".to_string()],
        }),
    )
    .await
    .expect("dismiss");
    assert_eq!(status, StatusCode::OK);
    let dismissed_fields = dismissed["data"]["dismissed_fields"].as_array().unwrap();
    assert!(dismissed_fields.iter().any(|field| field == "category"));
    assert!(dismissed_fields.iter().any(|field| field == "needs_human"));

    // Field dismissed tidak bisa di-apply.
    let (status, _) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["category".to_string()],
        }),
    )
    .await
    .expect("apply dismissed");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    clear_decision_env();
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn sweep_candidates_skips_ready_and_recent_failures() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let st = state(&pool).await;

    let (ready_id, _) = scratch.create_item(&st, "already classified").await;
    let (recent_failed, _) = scratch.create_item(&st, "failed recently").await;
    let (old_failed, _) = scratch.create_item(&st, "failed long ago").await;
    let (unclassified, _) = scratch.create_item(&st, "never classified").await;

    sqlx::query(
        "INSERT INTO intake_triage_suggestions (id, intake_issue_id, project_id, workspace_id, \
         status, created_at, updated_at) VALUES \
         (gen_random_uuid(), $1, $2, $3, 'ready', now(), now()), \
         (gen_random_uuid(), $4, $2, $3, 'failed', now(), now()), \
         (gen_random_uuid(), $5, $2, $3, 'failed', now(), now() - interval '10 minutes')",
    )
    .bind(ready_id)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .bind(recent_failed)
    .bind(old_failed)
    .execute(&pool)
    .await
    .expect("seed suggestions");

    let candidates = ai::triage_job::sweep_candidates(&pool, 10).await.unwrap();
    assert!(candidates.contains(&unclassified));
    assert!(candidates.contains(&old_failed));
    assert!(!candidates.contains(&ready_id));
    assert!(!candidates.contains(&recent_failed));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 3: Jalankan test — harus lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1`
Expected: 2 test PASS.

Catatan race: `create_issue` push job ke Redis yang sama dengan worker container dev. Karena row `LLM_DECISION_MODEL` di DB dev di-seed kosong, worker mengabaikan job itu (tidak menyentuh row saran), jadi test tetap deterministik. Jika model sudah diisi di DB dev saat menjalankan test, hentikan sementara `plane-for-itsm-worker-1` agar tidak ada klasifikasi paralel.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/support/mod.rs apps/api-rs/crates/api/tests/intake_triage_test.rs
git commit -m "test(api-rs): cover intake triage suggestions end to end"
```

---

### Task 9: Seed key config (Django) + daftar seed fresh-install

**Files:**

- Create: `apps/api/plane/license/migrations/0007_seed_llm_decision_model.py`
- Modify: `apps/api/plane/utils/instance_config_variables/core.py:216-242`

- [ ] **Step 1: Tulis data migration**

```python
# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

# Django imports
from django.db import migrations


def seed_llm_decision_model(apps, schema_editor):
    InstanceConfiguration = apps.get_model("license", "InstanceConfiguration")
    InstanceConfiguration.objects.get_or_create(
        key="LLM_DECISION_MODEL",
        defaults={"value": "", "category": "AI", "is_encrypted": False},
    )


def unseed_llm_decision_model(apps, schema_editor):
    InstanceConfiguration = apps.get_model("license", "InstanceConfiguration")
    InstanceConfiguration.objects.filter(key="LLM_DECISION_MODEL").delete()


class Migration(migrations.Migration):
    dependencies = [("license", "0006_instance_is_current_version_deprecated")]

    operations = [
        migrations.RunPython(seed_llm_decision_model, unseed_llm_decision_model),
    ]
```

- [ ] **Step 2: Tambah entri di daftar seed fresh-install**

Di `apps/api/plane/utils/instance_config_variables/core.py`, di dalam `llm_config_variables` setelah entri `LLM_MODEL`:

```python
    {
        "key": "LLM_DECISION_MODEL",
        "value": os.environ.get("LLM_DECISION_MODEL", ""),
        "category": "AI",
        "is_encrypted": False,
    },
```

- [ ] **Step 3: Terapkan migration**

Run: `docker compose -f docker-compose-local.yml run --rm migrator`
Expected: log `Applying license.0007_seed_llm_decision_model... OK`.

- [ ] **Step 4: Verifikasi row ada**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT key, value, category FROM instance_configurations WHERE key = 'LLM_DECISION_MODEL'"`
Expected: 1 row dengan value kosong kategori `AI`.

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/license/migrations/0007_seed_llm_decision_model.py apps/api/plane/utils/instance_config_variables/core.py
git commit -m "feat(api): seed LLM decision model instance configuration"
```

---

### Task 10: Admin — field "Decision model"

**Files:**

- Modify: `packages/types/src/instance/ai.ts:7`
- Modify: `apps/admin/app/(all)/(dashboard)/ai/form.tsx`

- [ ] **Step 1: Tambah key ke tipe**

`packages/types/src/instance/ai.ts`:

```ts
export type TInstanceAIConfigurationKeys = "LLM_API_KEY" | "LLM_MODEL" | "LLM_DECISION_MODEL";
```

- [ ] **Step 2: Tambah default value + field di form**

Di `apps/admin/app/(all)/(dashboard)/ai/form.tsx`:

1. `defaultValues`:

```tsx
    defaultValues: {
      LLM_API_KEY: config["LLM_API_KEY"],
      LLM_MODEL: config["LLM_MODEL"],
      LLM_DECISION_MODEL: config["LLM_DECISION_MODEL"],
    },
```

2. Tambah field setelah objek `LLM_MODEL` (sebelum `LLM_API_KEY`):

```tsx
    {
      key: "LLM_DECISION_MODEL",
      type: "text",
      label: "Decision model",
      description: (
        <>
          System One model at your LLM base URL, used for intake triage suggestions. Leave empty to
          disable. On OpenRouter use <code>typesafe/jev-1.13</code>.
        </>
      ),
      placeholder: "typesafe/jev-1.13",
      error: Boolean(errors.LLM_DECISION_MODEL),
      required: false,
    },
```

- [ ] **Step 3: Typecheck**

Run: `pnpm check:types`
Expected: sukses (tidak ada error terkait `LLM_DECISION_MODEL`).

- [ ] **Step 4: Commit**

```bash
git add packages/types/src/instance/ai.ts "apps/admin/app/(all)/(dashboard)/ai/form.tsx"
git commit -m "feat(admin): expose decision model setting"
```

---

### Task 11: Web — tipe, service, store

**Files:**

- Modify: `packages/types/src/inbox.ts`
- Modify: `apps/web/core/services/inbox/inbox-issue.service.ts`
- Modify: `apps/web/core/store/inbox/inbox-issue.store.ts`

- [ ] **Step 1: Tambah tipe triage**

Di `packages/types/src/inbox.ts` setelah `TInboxDuplicateIssueDetails`:

```ts
export type TInboxIssueTriageField = "category" | "severity" | "needs_human";

export type TInboxIssueTriageStatus = "pending" | "ready" | "failed";

export type TInboxIssueTriageSuggestion = {
  id: string;
  status: TInboxIssueTriageStatus;
  model: string | null;
  category: {
    type_id: string | null;
    label: string;
    confidence: number;
    probabilities: Record<string, number>;
  } | null;
  severity: {
    priority: TIssuePriorities;
    score: number;
    confidence: number;
    probabilities: Partial<Record<TIssuePriorities, number>>;
  } | null;
  needs_human: { probability: number } | null;
  applied_fields: TInboxIssueTriageField[];
  dismissed_fields: TInboxIssueTriageField[];
  created_at: string;
};
```

- [ ] **Step 2: Tambah method service**

Di `apps/web/core/services/inbox/inbox-issue.service.ts`:

1. Tambah import tipe:

```ts
import type {
  TInboxIssue,
  TInboxIssueTriageField,
  TInboxIssueTriageSuggestion,
  TIssue,
  TInboxIssueWithPagination,
} from "@plane/types";
```

2. Tambah method sebelum `destroy`:

```ts
  async retrieveTriageSuggestion(
    workspaceSlug: string,
    projectId: string,
    inboxIssueId: string
  ): Promise<TInboxIssueTriageSuggestion | null> {
    return this.get(
      `/api/workspaces/${workspaceSlug}/projects/${projectId}/inbox-issues/${inboxIssueId}/triage-suggestion/`
    )
      .then((response) => response?.data?.data ?? null)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async applyTriageSuggestion(
    workspaceSlug: string,
    projectId: string,
    inboxIssueId: string,
    fields: TInboxIssueTriageField[]
  ): Promise<TInboxIssueTriageSuggestion> {
    return this.post(
      `/api/workspaces/${workspaceSlug}/projects/${projectId}/inbox-issues/${inboxIssueId}/triage-suggestion/apply/`,
      { fields }
    )
      .then((response) => response?.data?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async dismissTriageSuggestion(
    workspaceSlug: string,
    projectId: string,
    inboxIssueId: string,
    fields: TInboxIssueTriageField[]
  ): Promise<TInboxIssueTriageSuggestion> {
    return this.post(
      `/api/workspaces/${workspaceSlug}/projects/${projectId}/inbox-issues/${inboxIssueId}/triage-suggestion/dismiss/`,
      { fields }
    )
      .then((response) => response?.data?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
```

- [ ] **Step 3: Tambah state + action store**

Di `apps/web/core/store/inbox/inbox-issue.store.ts`:

1. Tambah import tipe:

```ts
import type {
  TInboxIssue,
  TInboxIssueStatus,
  TInboxIssueTriageField,
  TInboxIssueTriageSuggestion,
  EInboxIssueSource,
  TIssue,
  TInboxDuplicateIssueDetails,
} from "@plane/types";
```

2. Tambah di interface `IInboxIssueStore`:

```ts
triageSuggestion: TInboxIssueTriageSuggestion | null;
triageSuggestionFetched: boolean;
fetchTriageSuggestion: () => Promise<void>;
applyTriageSuggestion: (fields: TInboxIssueTriageField[]) => Promise<void>;
dismissTriageSuggestion: (fields: TInboxIssueTriageField[]) => Promise<void>;
```

3. Tambah observable:

```ts
  triageSuggestion: TInboxIssueTriageSuggestion | null = null;
  triageSuggestionFetched: boolean = false;
```

4. Tambah entry `makeObservable`:

```ts
      triageSuggestion: observable,
      triageSuggestionFetched: observable,
```

lalu di bagian actions:

```ts
      fetchTriageSuggestion: action,
      applyTriageSuggestion: action,
      dismissTriageSuggestion: action,
```

5. Tambah method setelah `updateInboxIssueSnoozeTill`:

```ts
fetchTriageSuggestion = async () => {
  if (!this.issue.id) return;
  try {
    const suggestion = await this.inboxIssueService.retrieveTriageSuggestion(
      this.workspaceSlug,
      this.projectId,
      this.issue.id
    );
    runInAction(() => {
      set(this, "triageSuggestion", suggestion);
      set(this, "triageSuggestionFetched", true);
    });
  } catch {
    runInAction(() => set(this, "triageSuggestionFetched", true));
  }
};

applyTriageSuggestion = async (fields: TInboxIssueTriageField[]) => {
  if (!this.issue.id) return;
  const suggestion = await this.inboxIssueService.applyTriageSuggestion(
    this.workspaceSlug,
    this.projectId,
    this.issue.id,
    fields
  );
  runInAction(() => {
    set(this, "triageSuggestion", suggestion);
    if (suggestion?.severity && suggestion.applied_fields.includes("severity")) {
      set(this.issue, "priority", suggestion.severity.priority);
    }
  });
};

dismissTriageSuggestion = async (fields: TInboxIssueTriageField[]) => {
  if (!this.issue.id) return;
  const suggestion = await this.inboxIssueService.dismissTriageSuggestion(
    this.workspaceSlug,
    this.projectId,
    this.issue.id,
    fields
  );
  runInAction(() => set(this, "triageSuggestion", suggestion));
};
```

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`
Expected: sukses.

- [ ] **Step 5: Commit**

```bash
git add packages/types/src/inbox.ts apps/web/core/services/inbox/inbox-issue.service.ts apps/web/core/store/inbox/inbox-issue.store.ts
git commit -m "feat(web): add intake triage suggestion types and store"
```

---

### Task 12: Web — panel triage + mount + i18n

**Files:**

- Create: `apps/web/core/components/inbox/content/triage-suggestion.tsx`
- Modify: `apps/web/core/components/inbox/content/issue-root.tsx`
- Modify: `packages/i18n/src/locales/en/inbox.json`

- [ ] **Step 1: Muat skill translate**

Baca `packages/i18n/src/locales/en/inbox.json` untuk konteks, lalu ikuti `.claude/skills/translate/SKILL.md` saat menambah key (fan-out locale lain mengikuti skill tersebut; step ini minimal menambahkan `en`).

- [ ] **Step 2: Buat komponen panel**

Buat `apps/web/core/components/inbox/content/triage-suggestion.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TInboxIssueTriageField } from "@plane/types";
// stores
import type { IInboxIssueStore } from "@/store/inbox/inbox-issue.store";

type Props = {
  inboxIssue: IInboxIssueStore;
};

const percent = (value: number) => `${Math.round(value * 100)}%`;

export const InboxIssueTriageSuggestion = observer(function InboxIssueTriageSuggestion(props: Props) {
  const { inboxIssue } = props;
  // store hooks
  const { t } = useTranslation();
  const suggestion = inboxIssue.triageSuggestion;

  useEffect(() => {
    if (!inboxIssue.triageSuggestionFetched) void inboxIssue.fetchTriageSuggestion();
  }, [inboxIssue]);

  const runAction = async (action: "apply" | "dismiss", fields: TInboxIssueTriageField[]) => {
    try {
      if (action === "apply") await inboxIssue.applyTriageSuggestion(fields);
      else await inboxIssue.dismissTriageSuggestion(fields);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("inbox_issue.triage.error_title"),
        message: t("inbox_issue.triage.error_message"),
      });
    }
  };

  if (!suggestion) return null;

  if (suggestion.status === "pending")
    return (
      <div className="mb-4 rounded-md border border-subtle bg-layer-1 px-3 py-2 text-13 text-tertiary">
        {t("inbox_issue.triage.pending")}
      </div>
    );

  const applied = suggestion.applied_fields;
  const dismissed = suggestion.dismissed_fields;

  return (
    <div className="mb-4 rounded-md border border-subtle bg-layer-1 px-3 py-2">
      <div className="mb-2 flex items-center justify-between gap-2">
        <h5 className="text-body-sm-medium">{t("inbox_issue.triage.title")}</h5>
        {suggestion.model && (
          <span className="text-11 text-tertiary">
            {t("inbox_issue.triage.suggested_by", { model: suggestion.model })}
          </span>
        )}
      </div>

      <div className="divide-y-2 divide-subtle-1">
        {suggestion.category && (
          <div className="flex items-center justify-between gap-2 py-2">
            <div className="flex flex-col">
              <span className="text-13 text-tertiary">{t("inbox_issue.triage.category")}</span>
              <span className="text-13 text-primary">
                {suggestion.category.label}{" "}
                <span className="text-tertiary">({percent(suggestion.category.confidence)})</span>
              </span>
            </div>
            {dismissed.includes("category") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
            ) : (
              <button
                type="button"
                className="text-11 text-tertiary hover:text-primary"
                onClick={() => void runAction("dismiss", ["category"])}
              >
                {t("inbox_issue.triage.dismiss")}
              </button>
            )}
          </div>
        )}

        {suggestion.severity && (
          <div className="flex items-center justify-between gap-2 py-2">
            <div className="flex flex-col">
              <span className="text-13 text-tertiary">{t("inbox_issue.triage.severity")}</span>
              <span className="text-13 text-primary">
                {suggestion.severity.priority}{" "}
                <span className="text-tertiary">
                  ({suggestion.severity.score.toFixed(1)} / 4, {percent(suggestion.severity.confidence)})
                </span>
              </span>
            </div>
            {applied.includes("severity") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.applied")}</span>
            ) : dismissed.includes("severity") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
            ) : (
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  className="text-11 text-accent-primary hover:underline"
                  onClick={() => void runAction("apply", ["severity"])}
                >
                  {t("inbox_issue.triage.apply")}
                </button>
                <button
                  type="button"
                  className="text-11 text-tertiary hover:text-primary"
                  onClick={() => void runAction("dismiss", ["severity"])}
                >
                  {t("inbox_issue.triage.dismiss")}
                </button>
              </div>
            )}
          </div>
        )}

        {suggestion.needs_human && (
          <div className="flex items-center justify-between gap-2 py-2">
            <div className="flex flex-col">
              <span className="text-13 text-tertiary">{t("inbox_issue.triage.needs_human")}</span>
              <span className="text-13 text-primary">
                {suggestion.needs_human.probability >= 0.5 ? t("inbox_issue.triage.yes") : t("inbox_issue.triage.no")}{" "}
                <span className="text-tertiary">({percent(suggestion.needs_human.probability)})</span>
              </span>
            </div>
            {dismissed.includes("needs_human") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
            ) : (
              <button
                type="button"
                className="text-11 text-tertiary hover:text-primary"
                onClick={() => void runAction("dismiss", ["needs_human"])}
              >
                {t("inbox_issue.triage.dismiss")}
              </button>
            )}
          </div>
        )}
      </div>

      <p className="mt-2 text-11 text-tertiary">{t("inbox_issue.triage.disclaimer")}</p>
    </div>
  );
});
```

- [ ] **Step 3: Mount di detail intake**

Di `apps/web/core/components/inbox/content/issue-root.tsx`:

1. Tambah import setelah import `InboxIssueContentProperties`:

```tsx
import { InboxIssueTriageSuggestion } from "./triage-suggestion";
```

2. Render setelah blok attachment (`<div className="py-4"><IssueAttachmentRoot ... /></div>`), sebelum blok properties:

```tsx
<InboxIssueTriageSuggestion inboxIssue={inboxIssue} />
```

- [ ] **Step 4: Tambah string en**

Di `packages/i18n/src/locales/en/inbox.json`, di dalam objek `inbox_issue` (setelah `"actions"`), tambahkan:

```json
    "triage": {
      "title": "Triage suggestion",
      "pending": "Classifying this work item…",
      "suggested_by": "Suggested by {model}",
      "category": "Category",
      "severity": "Severity",
      "needs_human": "Needs human",
      "yes": "Yes",
      "no": "No",
      "apply": "Apply",
      "dismiss": "Dismiss",
      "applied": "Applied",
      "dismissed": "Dismissed",
      "disclaimer": "Suggestions only. Review before applying.",
      "error_title": "Could not update suggestion",
      "error_message": "Please try again."
    },
```

- [ ] **Step 5: Lint + typecheck + test web**

Run: `pnpm check && pnpm --filter=web test`
Expected: sukses (test vitest web yang ada tetap PASS).

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/inbox/content/triage-suggestion.tsx apps/web/core/components/inbox/content/issue-root.tsx packages/i18n/src/locales/en/inbox.json
git commit -m "feat(web): show intake triage suggestions in detail"
```

---

### Task 13: Verifikasi akhir + dokumentasi

**Files:**

- Modify: `docs/features/intake.md`

- [ ] **Step 1: Jalankan seluruh test backend**

```bash
cd apps/api-rs
cargo test -p ai
cargo test -p worker
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_triage_test -- --test-threads=1
cd ../..
```

Expected: semua PASS.

- [ ] **Step 2: Jalankan check repo**

Run: `pnpm check`
Expected: format/lint/types sukses.

- [ ] **Step 3: Dokumentasikan endpoint baru**

Di `docs/features/intake.md`, tambahkan bagian singkat di akhir:

```markdown
### Saran triage AI (Rust-only)

- `GET /api/workspaces/:slug/projects/:project_id/intake-issues/:pk/triage-suggestion/`
- `POST .../triage-suggestion/apply/` (v1: `{"fields":["severity"]}` → `issues.priority`)
- `POST .../triage-suggestion/dismiss/` (field: `category`, `severity`, `needs_human`)

Diisi worker `ai.intake.triage` (push saat create + sweep beat tiap menit) memakai
`LLM_DECISION_MODEL` (Jev/System One di `{LLM_BASE_URL}/systemone`). Row `failed`
tidak dikembalikan (`data: null`). Tidak ada counterpart Django.
```

- [ ] **Step 4: Smoke test live (mengikuti AGENTS.md)**

1. Set `LLM_DECISION_MODEL=typesafe/jev-1.13` di halaman admin AI (key = OpenRouter key yang sudah ada; `LLM_BASE_URL` production `https://openrouter.ai/api/v1`).
2. Rebuild backend: `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &` lalu poll log (link LTO bisa 10+ menit).
3. Setelah `curl http://localhost:8000/health` → 200: `systemctl --user restart plane-live.service`.
4. Buat intake item baru di web; buka detailnya; pastikan panel "Triage suggestion" muncul (kategori + severity + needs human), tombol Apply severity mengubah priority, Dismiss menyembunyikan field.
5. Cek DB: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT status, model, severity_priority, needs_human, input_tokens FROM intake_triage_suggestions ORDER BY created_at DESC LIMIT 3"`.

- [ ] **Step 5: Commit**

```bash
git add docs/features/intake.md
git commit -m "docs(intake): document triage suggestion endpoints"
```
