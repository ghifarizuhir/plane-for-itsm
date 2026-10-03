# Review Briefing (AI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a stored, manually generated AI pre-meeting briefing to TCB/RCB review sessions, rendered per agenda item on the session detail page.

**Architecture:** One new `jsonb` column set on `review_sessions`, one new Rust route module (`review_briefing.rs`) that assembles deterministic agenda context, calls the existing single-shot `chat_completion`, validates the JSON reply (retry once, then text fallback), and stores it. The web reads `briefing` from the existing session-detail payload and renders it with a new `SessionBriefing` component.

**Tech Stack:** Rust (axum, sqlx, serde_json), sqlx migrations, vitest, MobX store, React Router, `@plane/types`, `@plane/i18n`.

**Spec:** `docs/superpowers/specs/2026-10-03-review-briefing-design.md`

---

## File Structure

### Backend (`apps/api-rs`)

| Action | File                                       | Responsibility                                                         |
| ------ | ------------------------------------------ | ---------------------------------------------------------------------- |
| Create | `migrations/0013_review_briefing.sql`      | 4 briefing columns on `review_sessions`.                               |
| Create | `crates/api/src/routes/review_briefing.rs` | Pure core (sanitize/truncate/parse/prompt), context assembly, handler. |
| Modify | `crates/api/src/routes/mod.rs`             | Register `pub mod review_briefing;`.                                   |
| Modify | `crates/api/src/routes/review.rs`          | Item facts in `ITEM_SELECT`/`item_json`; `briefing` in session detail. |
| Modify | `crates/api/src/main.rs`                   | Route `POST /review-sessions/:session_id/briefing/`.                   |
| Create | `crates/api/tests/review_briefing_test.rs` | Integration tests with a fake OpenAI-compatible upstream.              |
| Modify | `crates/api/tests/release_review_test.rs`  | Session detail briefing/facts assertions.                              |

### Types, i18n, web

| Action | File                                                                      | Responsibility                            |
| ------ | ------------------------------------------------------------------------- | ----------------------------------------- |
| Modify | `packages/types/src/review/core.ts`                                       | Briefing types, item facts, detail field. |
| Modify | `apps/web/core/services/review.helpers.ts` + `.test.ts`                   | Briefing presentation helpers.            |
| Modify | `apps/web/core/services/review.service.ts`                                | `generateReviewBriefing`.                 |
| Modify | `apps/web/core/store/review.store.ts`                                     | `generateBriefing` action.                |
| Create | `apps/web/core/store/review.store.test.ts`                                | Store action tests.                       |
| Modify | `packages/i18n/src/locales/en/review.json` (+19 locales)                  | `review.briefing.*` keys.                 |
| Create | `apps/web/core/components/reviews/session/session-briefing.tsx`           | Briefing section UI.                      |
| Modify | `apps/web/core/components/reviews/session/review-session-detail-root.tsx` | Render `SessionBriefing`.                 |

**Conventions that bite:**

- Rust integration tests that mutate `LLM_*`/`SKIP_ENV_VAR` env vars MUST run with `--test-threads=1` and as a single test binary.
- Migrations are embedded by `sqlx::migrate!` at compile time; a new `.sql` file is picked up on the next `cargo build`. Apply it to the dev DB manually before running DB-backed tests (the migrator is idempotent because the DDL uses `IF NOT EXISTS`).
- Web uses `@plane/types` and `@plane/i18n` from their built `dist/`; rebuild them before `pnpm --filter=web build` (stale dist has burned us before).
- Never use `toSorted` (ES2022 target). Sort a copied list with `[...list].sort()` and the existing `oxlint-disable-next-line unicorn/no-array-sort` comment.

---

## Task 1: Migration 0013

**Files:**

- Create: `apps/api-rs/migrations/0013_review_briefing.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Review briefing (AI): pre-meeting briefing stored on a review session.
-- Spec: docs/superpowers/specs/2026-10-03-review-briefing-design.md
-- Delta applied at boot by `common::db::migrate`; the IF NOT EXISTS guards
-- keep a manual apply + boot apply idempotent.

ALTER TABLE public.review_sessions
    ADD COLUMN IF NOT EXISTS briefing jsonb,
    ADD COLUMN IF NOT EXISTS briefing_generated_at timestamp with time zone,
    ADD COLUMN IF NOT EXISTS briefing_generated_by_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS briefing_model character varying(100);
```

- [ ] **Step 2: Apply it to the dev DB (tests connect to `localhost:5432`)**

Run:

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane < apps/api-rs/migrations/0013_review_briefing.sql
```

Expected: `ALTER TABLE`

- [ ] **Step 3: Verify the columns exist**

Run:

```bash
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tAc \
  "SELECT column_name FROM information_schema.columns WHERE table_name='review_sessions' AND column_name LIKE 'briefing%' ORDER BY column_name"
```

Expected output (4 lines): `briefing`, `briefing_generated_at`, `briefing_generated_by_id`, `briefing_model`

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0013_review_briefing.sql
git commit -m "feat(api-rs): add review briefing migration"
```

---

## Task 2: Pure core of `review_briefing.rs`

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/review_briefing.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs:41`

- [ ] **Step 1: Register the module**

In `apps/api-rs/crates/api/src/routes/mod.rs`, right after `pub mod review;` add:

```rust
pub mod review_briefing;
```

- [ ] **Step 2: Write the pure core with failing unit tests**

Create `apps/api-rs/crates/api/src/routes/review_briefing.rs`:

````rust
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
````

- [ ] **Step 3: Run the unit tests**

Run:

```bash
cd apps/api-rs && cargo test -p api --lib review_briefing
```

Expected: 6 tests pass (`sanitize_language_keeps_alnum_and_dash`, `truncate_text_strips_tags_and_collapses_space`, `parse_ai_json_filters_unknown_and_duplicate_ids`, `parse_ai_json_rejects_non_json_and_missing_overall`, `parse_ai_json_accepts_fenced_json`, `build_task_mentions_language_and_json`).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/routes/review_briefing.rs
git commit -m "feat(api-rs): add review briefing pure core and parser"
```

---

## Task 3: Item facts and `briefing` in session detail

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/review.rs` (`ITEM_SELECT` at ~1185, `SessionItemRow` at ~1166, `item_json` at ~1196, `session_detail` at ~751)
- Modify: `apps/api-rs/crates/api/tests/release_review_test.rs`

- [ ] **Step 1: Write the failing test**

Append to `apps/api-rs/crates/api/tests/release_review_test.rs` (after `session_detail_reads_scope`, before `participants_add_list_patch_remove_and_notify`):

```rust
#[tokio::test]
async fn session_detail_includes_briefing_and_item_facts() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Facts")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();
    items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add items");

    let (status, Json(body)) = session_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("session detail");
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("briefing").is_some());
    assert!(body["briefing"].is_null());
    let item = &body["items"][0];
    assert_eq!(item["facts"]["issue_id"], issue_id.to_string());
    assert_eq!(item["facts"]["project_id"], scratch.project_id.to_string());
    assert_eq!(item["facts"]["state_name"], "New");
    assert_eq!(item["facts"]["priority"], "none");
    assert!(item["facts"]["war_room"].is_null());
    assert!(item["facts"]["assignees"].as_array().unwrap().is_empty());

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run it to verify it fails**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test session_detail_includes_briefing_and_item_facts -- --test-threads=1
```

Expected: FAIL — `assert!(body.get("briefing").is_some())` panics because the key is absent.

- [ ] **Step 3: Extend `SessionItemRow` and `ITEM_SELECT`**

In `apps/api-rs/crates/api/src/routes/review.rs`, add fields to `SessionItemRow` after `release_version`:

```rust
    pub issue_id: Option<Uuid>,
    pub issue_project_id: Option<Uuid>,
    pub issue_priority: Option<String>,
    pub issue_state_name: Option<String>,
    pub issue_target_date: Option<chrono::NaiveDate>,
    pub issue_assignees: Option<String>,
    pub war_room_id: Option<Uuid>,
    pub war_room_name: Option<String>,
    pub release_id: Option<Uuid>,
    pub release_status: Option<String>,
    pub release_target_date: Option<chrono::NaiveDate>,
```

Replace `ITEM_SELECT` with:

```rust
const ITEM_SELECT: &str = "SELECT i.id, i.session_id, i.review_request_id, i.position, i.outcome, \
    i.outcome_note, i.decided_by_id, i.decided_at, i.created_at, rr.status AS request_status, \
    rr.board_type, rr.submission_note, \
    (p.identifier || '-' || iss.sequence_id::text) AS issue_identifier, iss.name AS issue_name, \
    iss.id AS issue_id, iss.project_id AS issue_project_id, iss.priority AS issue_priority, \
    iss.target_date AS issue_target_date, st.name AS issue_state_name, \
    (SELECT string_agg(COALESCE(u.display_name, u.username), ', ' ORDER BY ia.created_at) \
     FROM issue_assignees ia JOIN users u ON u.id = ia.assignee_id \
     WHERE ia.issue_id = iss.id AND ia.deleted_at IS NULL) AS issue_assignees, \
    (SELECT wr.id FROM war_room_issues wri JOIN war_rooms wr ON wr.id = wri.war_room_id \
     WHERE wri.issue_id = iss.id AND wri.deleted_at IS NULL AND wr.deleted_at IS NULL \
     ORDER BY wr.created_at DESC LIMIT 1) AS war_room_id, \
    (SELECT wr.name FROM war_room_issues wri JOIN war_rooms wr ON wr.id = wri.war_room_id \
     WHERE wri.issue_id = iss.id AND wri.deleted_at IS NULL AND wr.deleted_at IS NULL \
     ORDER BY wr.created_at DESC LIMIT 1) AS war_room_name, \
    rel.id AS release_id, rel.status AS release_status, rel.target_date AS release_target_date, \
    rel.name AS release_name, rel.version AS release_version \
    FROM review_session_items i \
    JOIN review_requests rr ON rr.id = i.review_request_id \
    LEFT JOIN issues iss ON iss.id = rr.change_issue_id \
    LEFT JOIN projects p ON p.id = iss.project_id \
    LEFT JOIN states st ON st.id = iss.state_id \
    LEFT JOIN releases rel ON rel.id = rr.release_id";
```

- [ ] **Step 4: Add `facts` to `item_json`**

In `item_json`, before the final `json!({...})`, add:

```rust
    let facts = if row.board_type == "tcb" {
        json!({
            "issue_id": row.issue_id,
            "project_id": row.issue_project_id,
            "priority": row.issue_priority,
            "state_name": row.issue_state_name,
            "target_date": row.issue_target_date,
            "assignees": row
                .issue_assignees
                .as_deref()
                .map(|raw| raw.split(", ").map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default(),
            "war_room": match (row.war_room_id, row.war_room_name.as_deref()) {
                (Some(id), Some(name)) => json!({"id": id, "name": name}),
                _ => Value::Null,
            },
        })
    } else {
        json!({
            "release_id": row.release_id,
            "status": row.release_status,
            "target_date": row.release_target_date,
        })
    };
```

and add `"facts": facts,` to the returned object (after `"subject": subject,`).

- [ ] **Step 5: Add `briefing` to the session detail payload**

In `session_detail` (`review.rs:751`), after the participants block and before `Ok(...)`:

```rust
    let briefing: Option<Value> = sqlx::query_scalar(
        "SELECT briefing FROM review_sessions WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(session_id)
    .fetch_optional(&st.pool)
    .await?
    .flatten();
    value["briefing"] = briefing.unwrap_or(Value::Null);
```

- [ ] **Step 6: Run the test to verify it passes**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test session_detail_includes_briefing_and_item_facts -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 7: Run the whole review suite to catch regressions**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/review.rs apps/api-rs/crates/api/tests/release_review_test.rs
git commit -m "feat(api-rs): expose session item facts and briefing in session detail"
```

---

## Task 4: Briefing handler, context assembly, route, happy path test

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/review_briefing.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs:839`
- Create: `apps/api-rs/crates/api/tests/review_briefing_test.rs`

- [ ] **Step 1: Write the failing integration test**

Create `apps/api-rs/crates/api/tests/review_briefing_test.rs`:

```rust
//! Review briefing (AI) integration tests: fake OpenAI-compatible upstream,
//! degradation paths, permissions. DB-backed and mutates process-level LLM env
//! → run serially as its own binary:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test review_briefing_test -- --test-threads=1`

#[path = "support/mod.rs"]
mod support;

use api::middleware::auth::AuthUser;
use api::routes::review::{
    complete_session, create_session, items_create, session_detail, submit_request, AddItems,
    CreateSession, SubmitRequest,
};
use api::routes::review_briefing::{generate_briefing, BriefingRequest};
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

async fn insert_workspace_member(pool: &PgPool, user_id: Uuid, workspace_id: Uuid, role: i16) {
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(role)
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project_member(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    role: i16,
) {
    sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
         view_props, default_props, sort_order, preferences, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, true, '{}', '{}', 65535, '{}', now(), now())",
    )
    .bind(user_id)
    .bind(role)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project member");
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    state_id: Uuid,
    type_id: Uuid,
    extra_users: Vec<Uuid>,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("rb-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let type_id = Uuid::new_v4();
        let identifier = format!("RB{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Briefing Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        insert_workspace_member(pool, user_id, workspace_id, 20).await;
        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'Briefing Scratch', '', 2, $2, $3, false, false, false, \
             false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(&identifier)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");
        insert_project_member(pool, user_id, project_id, workspace_id, 20).await;
        sqlx::query(
            "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
             sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
             VALUES ($1, 'New', '', '#F59E0B', 'new', $2, $3, 65535, \
             'backlog', true, false, now(), now())",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch state");
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
             is_default, level, is_epic, created_at, updated_at) \
             VALUES ($1, 'Change', '', '{}'::jsonb, $2, true, true, 0, false, now(), now())",
        )
        .bind(type_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch issue type");

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            state_id,
            type_id,
            extra_users: Vec::new(),
        }
    }

    async fn add_actor(&mut self, pool: &PgPool, ws_role: i16) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        insert_workspace_member(pool, user_id, self.workspace_id, ws_role).await;
        self.extra_users.push(user_id);
        user_id
    }

    async fn insert_issue(&self, pool: &PgPool) -> Uuid {
        let mut tx = pool.begin().await.expect("scratch tx");
        let (issue_id, sequence_id): (Uuid, i32) = sqlx::query_as(
            "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
             sort_order, sequence_id, state_id, type_id, project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Change request', '<p></p>', '{}', 'none', false, \
             65535, (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $3), \
             $1, $2, $3, $4, now(), now()) RETURNING id, sequence_id",
        )
        .bind(self.state_id)
        .bind(self.type_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .fetch_one(&mut *tx)
        .await
        .expect("scratch issue");
        sqlx::query(
            "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, \
             created_by_id, deleted, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, now(), now())",
        )
        .bind(sequence_id)
        .bind(issue_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .bind(self.user_id)
        .execute(&mut *tx)
        .await
        .expect("scratch issue sequence");
        tx.commit().await.expect("scratch commit");
        issue_id
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_sequences WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        let mut user_ids = vec![self.user_id];
        user_ids.extend(self.extra_users.iter().copied());
        sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(&user_ids)
            .execute(pool)
            .await
            .ok();
    }
}

fn set_llm_env(base_url: &str) {
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", base_url);
    std::env::set_var("LLM_MODEL", "test-model");
}

fn clear_llm_env() {
    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");
}

async fn spawn_status(status: u16) -> String {
    use axum::routing::post;
    let handler = move || async move {
        (
            StatusCode::from_u16(status).unwrap(),
            Json(json!({"error": "boom"})),
        )
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route("/v1/chat/completions", post(handler)),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}/v1")
}

fn submit_body(board: &str, change: Option<Uuid>, release: Option<Uuid>) -> SubmitRequest {
    SubmitRequest {
        board_type: Some(board.to_string()),
        change_issue_id: change,
        release_id: release,
        submission_note: Some("Bukti uji terlampir".to_string()),
    }
}

fn session_body(board: &str, project_id: Option<Uuid>, title: &str) -> CreateSession {
    CreateSession {
        board_type: Some(board.to_string()),
        project_id,
        title: Some(title.to_string()),
        scheduled_at: Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-10T09:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
        location: Some("Ruang Rapat 3".to_string()),
        minutes: None,
    }
}

fn add_items(request_id: Uuid) -> AddItems {
    AddItems {
        request_ids: vec![request_id],
    }
}

fn request_id_of(body: &Value) -> Uuid {
    body["id"].as_str().unwrap().parse().unwrap()
}

/// TCB session with one submitted change on the agenda.
/// Returns `(request_id, session_id, item_id)`.
async fn setup_session(st: &AppState, scratch: &Scratch) -> (Uuid, Uuid, Uuid) {
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Briefing")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();
    let (_, Json(items)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add items");
    let item_id: Uuid = items[0]["id"].as_str().unwrap().parse().unwrap();
    (request_id, session_id, item_id)
}

#[tokio::test]
async fn generate_briefing_stores_json_and_regenerates() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, item_id) = setup_session(&st, &scratch).await;

    let answer = json!({
        "overall": "Satu item menunggu keputusan.",
        "items": [{
            "session_item_id": item_id,
            "summary": "Perubahan retry gateway.",
            "discussion_points": ["Pantau setelah deploy"],
            "risks": ["Menyentuh gateway pembayaran"]
        }]
    })
    .to_string();
    let (base, bodies) = support::spawn_recording_upstream(&answer).await;
    set_llm_env(&base);

    let (status, Json(briefing)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest {
            language: Some("id".into()),
        }),
    )
    .await
    .expect("generate");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(briefing["format"], "json");
    assert_eq!(briefing["language"], "id");
    assert_eq!(briefing["included_items"], 1);
    assert_eq!(briefing["skipped_items"], 0);
    assert_eq!(briefing["items"][0]["session_item_id"], item_id.to_string());
    assert_eq!(briefing["items"][0]["risks"][0], "Menyentuh gateway pembayaran");

    let request_body = bodies.lock().unwrap()[0].clone();
    let prompt = request_body["messages"][0]["content"].as_str().unwrap();
    assert!(prompt.contains("TCB Briefing"));
    assert!(prompt.contains(&item_id.to_string()));

    let (_, Json(detail)) = session_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("detail");
    assert_eq!(detail["briefing"]["format"], "json");
    assert_eq!(detail["briefing"]["items"][0]["summary"], "Perubahan retry gateway.");

    let second = json!({"overall": "Versi kedua.", "items": []}).to_string();
    let (base2, _) = support::spawn_recording_upstream(&second).await;
    set_llm_env(&base2);
    let (_, Json(regenerated)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest { language: None }),
    )
    .await
    .expect("regenerate");
    assert_eq!(regenerated["overall"], "Versi kedua.");
    assert_eq!(regenerated["language"], "en");
    assert_eq!(regenerated["items"].as_array().unwrap().len(), 0);

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run it to verify it fails**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test review_briefing_test -- --test-threads=1
```

Expected: compile error — `api::routes::review_briefing::generate_briefing` and `BriefingRequest` do not exist yet.

- [ ] **Step 3: Add the handler, assembly and request type**

In `apps/api-rs/crates/api/src/routes/review_briefing.rs`, add the new `use` lines to the import block at the top of the file (merge with the existing imports; do not duplicate `axum`, `serde_json`, or `uuid`), then insert the rest of the code below — everything from `type R = ...` down to the end of `generate_briefing` — before the `#[cfg(test)]` block:

```rust
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::ai::chat_completion;
use super::review::{
    can_manage_session, gate_session_read, items_for_session, session_in_workspace,
    SessionItemRow,
};
use super::service::bad_request;
use ai::llm::resolve_llm_config;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ChangeContextRow {
    identifier: String,
    name: String,
    description_html: String,
    priority: String,
    state_name: Option<String>,
    target_date: Option<chrono::NaiveDate>,
}

async fn change_context(pool: &PgPool, issue_id: Uuid) -> Result<Option<ChangeContextRow>, sqlx::Error> {
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
        .map(|(name, severity, status)| {
            json!({"name": name, "severity": severity, "status": status})
        })
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

async fn release_context(pool: &PgPool, release_id: Uuid) -> Result<Option<ReleaseContextRow>, sqlx::Error> {
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
            if let Some(issue_id) = item.change_issue_id {
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

    let first = match chat_completion(&cfg.base_url, &cfg.api_key, &cfg.model, &task, &prompt).await {
        Ok(text) => text,
        Err(error) => return Ok(llm_error_response(error, &cfg.base_url)),
    };
    let (overall, parsed_items, format) = match parse_ai_json(&first, &valid_ids) {
        Some((overall, parsed_items)) => (overall, parsed_items, "json"),
        None => {
            let retry_task =
                format!("{task}\nReturn ONLY the JSON object, no prose, no code fences.");
            let second =
                match chat_completion(&cfg.base_url, &cfg.api_key, &cfg.model, &retry_task, &prompt)
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
```

- [ ] **Step 4: Register the route**

In `apps/api-rs/crates/api/src/main.rs`, after the items route block (line ~839, after the `/items/:item_id/` route) add:

```rust
        .route(
            "/api/workspaces/:slug/review-sessions/:session_id/briefing/",
            post(routes::review_briefing::generate_briefing),
        )
```

- [ ] **Step 5: Run the test to verify it passes**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test review_briefing_test -- --test-threads=1
```

Expected: `generate_briefing_stores_json_and_regenerates` PASS.

- [ ] **Step 6: Run route-inventory guards**

Run:

```bash
cd apps/api-rs && cargo test -p api --test route_inventory_test
```

Expected: 5 tests pass (the new path builds without shape conflicts).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/review_briefing.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/review_briefing_test.rs
git commit -m "feat(api-rs): add review briefing generation endpoint"
```

---

## Task 5: Failure-mode and permission tests

**Files:**

- Modify: `apps/api-rs/crates/api/tests/review_briefing_test.rs`

- [ ] **Step 1: Append the tests**

Append to `apps/api-rs/crates/api/tests/review_briefing_test.rs`:

```rust
#[tokio::test]
async fn generate_briefing_requires_ai_config() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::remove_var("LLM_API_KEY");
    std::env::set_var("LLM_MODEL", "test-model");
    let (status, Json(body)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("not configured");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "AI is not configured for this workspace.");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_gates_manage_and_scheduled() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let outsider = scratch.add_actor(&st.pool, 15).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    let (base, _) = support::spawn_recording_upstream(r#"{"overall":"ok","items":[]}"#).await;
    set_llm_env(&base);

    let (status, _) = generate_briefing(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("deny");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, Json(_)) = complete_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("complete");
    let (status, Json(body)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("completed");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Session is not scheduled");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_rejects_empty_agenda() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "Empty")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    let (base, _) = support::spawn_recording_upstream(r#"{"overall":"ok","items":[]}"#).await;
    set_llm_env(&base);
    let (status, Json(body)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("empty agenda");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "The agenda is empty");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_falls_back_to_text_on_bad_json() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    let (base, bodies) = support::spawn_recording_upstream("ini bukan json").await;
    set_llm_env(&base);
    let (status, Json(briefing)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("fallback");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(briefing["format"], "text");
    assert_eq!(briefing["overall"], "ini bukan json");
    assert!(briefing["items"].as_array().unwrap().is_empty());
    assert_eq!(bodies.lock().unwrap().len(), 2, "must retry exactly once");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_upstream_error_keeps_previous() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    let (base, _) =
        support::spawn_recording_upstream(r#"{"overall":"Versi lama.","items":[]}"#).await;
    set_llm_env(&base);
    generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("first generate");

    let base500 = spawn_status(500).await;
    set_llm_env(&base500);
    let (status, _) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("upstream error");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let (_, Json(detail)) = session_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("detail");
    assert_eq!(detail["briefing"]["overall"], "Versi lama.");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 2: Run the full briefing suite**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test review_briefing_test -- --test-threads=1
```

Expected: 6 tests pass. If `generate_briefing_gates_manage_and_scheduled` fails on the completed-session call, verify `complete_session` imported from `api::routes::review` (it is) and that the session has an agenda item (it does).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/review_briefing_test.rs
git commit -m "test(api-rs): cover review briefing failure modes"
```

---

## Task 6: Backend verification and rebuild

**Files:** none new.

- [ ] **Step 1: Format**

Run:

```bash
cd apps/api-rs && cargo fmt --all
```

- [ ] **Step 2: Run all touched suites (each as its own binary, serially)**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test review_briefing_test -- --test-threads=1
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test release_review_test -- --test-threads=1
cd apps/api-rs && cargo test -p api --test route_inventory_test
cd apps/api-rs && cargo test -p api --lib review_briefing
```

Expected: all pass.

- [ ] **Step 3: Rebuild the backend detached (Rust LTO link can take 10+ minutes with no output)**

Run:

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll the log until the containers are up (`docker ps` shows `plane-for-itsm-api-1` healthy / recreated).

- [ ] **Step 4: Verify health and restart live**

Run:

```bash
curl -s -o /dev/null -w "api=%{http_code}\n" http://localhost:8000/health
systemctl --user restart plane-live.service
sleep 8
curl -s -o /dev/null -w "live=%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `api=200`, `live=200`.

- [ ] **Step 5: Commit any fmt changes**

```bash
git status --short
# if cargo fmt touched files:
git add -u && git commit -m "style(api-rs): format review briefing module"
```

---

## Task 7: Types

**Files:**

- Modify: `packages/types/src/review/core.ts`

- [ ] **Step 1: Add the briefing and facts types**

In `packages/types/src/review/core.ts`, after `IReviewItemSubject`, add:

```ts
export type TReviewBriefingFormat = "json" | "text";

export interface IReviewBriefingItem {
  session_item_id: string;
  summary: string;
  discussion_points: string[];
  risks: string[];
}

export interface IReviewSessionBriefing {
  version: number;
  generated_at: string;
  generated_by_name: string;
  model: string;
  language: string;
  format: TReviewBriefingFormat;
  overall: string;
  items: IReviewBriefingItem[];
  included_items: number;
  skipped_items: number;
}

export interface IReviewItemFacts {
  issue_id?: string | null;
  project_id?: string | null;
  priority?: string | null;
  state_name?: string | null;
  target_date?: string | null;
  assignees?: string[];
  war_room?: { id: string; name: string } | null;
  release_id?: string | null;
  status?: string | null;
}
```

Add `facts: IReviewItemFacts;` to `IReviewSessionItem` (after `subject`) and `briefing: IReviewSessionBriefing | null;` to `IReviewSessionDetail` (after `participants`).

- [ ] **Step 2: Build and typecheck the package**

Run:

```bash
pnpm --filter=@plane/types build && pnpm --filter=@plane/types check:types
```

Expected: build complete, 0 type errors.

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/review/core.ts
git commit -m "feat(types): add review briefing types"
```

---

## Task 8: Web helpers

**Files:**

- Modify: `apps/web/core/services/review.helpers.ts`
- Modify: `apps/web/core/services/review.helpers.test.ts`

- [ ] **Step 1: Write the failing tests**

In `apps/web/core/services/review.helpers.test.ts`, change the type import line to:

```ts
import type { IReviewSession, IReviewSessionBriefing } from "@plane/types";
```

add the new helpers to the import list:

```ts
import {
  activeRequestForSubject,
  briefingItemById,
  fromDateTimeLocal,
  isBriefingStale,
  isBriefingTextMode,
  latestRequestForSubject,
  reviewRequestsKey,
  reviewSessionsKey,
  sortSessionsByScheduledAt,
  toDateTimeLocal,
} from "./review.helpers";
```

and append:

```ts
const makeBriefing = (overrides: Partial<IReviewSessionBriefing> = {}): IReviewSessionBriefing =>
  ({
    version: 1,
    generated_at: "2026-10-03T00:00:00Z",
    generated_by_name: "Budi",
    model: "gpt-4o-mini",
    language: "id",
    format: "json",
    overall: "Ringkasan",
    items: [],
    included_items: 0,
    skipped_items: 0,
    ...overrides,
  }) as IReviewSessionBriefing;

describe("briefingItemById", () => {
  it("maps items by session_item_id and tolerates null", () => {
    const briefing = makeBriefing({
      items: [
        { session_item_id: "item-1", summary: "s", discussion_points: [], risks: [] },
        { session_item_id: "item-2", summary: "t", discussion_points: [], risks: [] },
      ],
    });
    expect(briefingItemById(briefing).get("item-2")?.summary).toBe("t");
    expect(briefingItemById(null).size).toBe(0);
  });
});

describe("isBriefingTextMode", () => {
  it("is true only for text format", () => {
    expect(isBriefingTextMode(makeBriefing({ format: "text" }))).toBe(true);
    expect(isBriefingTextMode(makeBriefing())).toBe(false);
    expect(isBriefingTextMode(null)).toBe(false);
  });
});

describe("isBriefingStale", () => {
  it("compares included plus skipped against the current agenda size", () => {
    expect(isBriefingStale(makeBriefing({ included_items: 2, skipped_items: 0 }), 3)).toBe(true);
    expect(isBriefingStale(makeBriefing({ included_items: 2, skipped_items: 1 }), 3)).toBe(false);
    expect(isBriefingStale(null, 3)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
pnpm --filter=web test review.helpers.test.ts
```

Expected: FAIL — `briefingItemById is not a function` (or a TS/import resolution error).

- [ ] **Step 3: Implement the helpers**

In `apps/web/core/services/review.helpers.ts`, change the type import to:

```ts
import type {
  IReviewBriefingItem,
  IReviewSession,
  IReviewSessionBriefing,
  TReviewRequestListParams,
  TReviewSessionListParams,
} from "@plane/types";
```

and append:

```ts
export const briefingItemById = (
  briefing: IReviewSessionBriefing | null | undefined
): Map<string, IReviewBriefingItem> => {
  const map = new Map<string, IReviewBriefingItem>();
  briefing?.items.forEach((item) => map.set(item.session_item_id, item));
  return map;
};

export const isBriefingTextMode = (briefing: IReviewSessionBriefing | null | undefined): boolean =>
  briefing?.format === "text";

export const isBriefingStale = (
  briefing: IReviewSessionBriefing | null | undefined,
  agendaItemCount: number
): boolean => (briefing ? briefing.included_items + briefing.skipped_items !== agendaItemCount : false);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
pnpm --filter=web test review.helpers.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/review.helpers.ts apps/web/core/services/review.helpers.test.ts
git commit -m "feat(web): add review briefing helpers"
```

---

## Task 9: Service and store action

**Files:**

- Modify: `apps/web/core/services/review.service.ts`
- Modify: `apps/web/core/store/review.store.ts`
- Create: `apps/web/core/store/review.store.test.ts`

- [ ] **Step 1: Write the failing store test**

Create `apps/web/core/store/review.store.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import type { IReviewSessionBriefing, IReviewSessionDetail } from "@plane/types";
// store
import { ReviewStore } from "./review.store";

const makeBriefing = (overrides: Partial<IReviewSessionBriefing> = {}): IReviewSessionBriefing =>
  ({
    version: 1,
    generated_at: "2026-10-03T00:00:00.000Z",
    generated_by_name: "Budi",
    model: "gpt-4o-mini",
    language: "id",
    format: "json",
    overall: "Ringkasan",
    items: [],
    included_items: 0,
    skipped_items: 0,
    ...overrides,
  }) as IReviewSessionBriefing;

const makeSessionDetail = (): IReviewSessionDetail =>
  ({
    id: "session-1",
    workspace_id: "ws-1",
    board_type: "tcb",
    project_id: "project-1",
    title: "Review",
    scheduled_at: "2026-10-10T09:00:00.000Z",
    status: "scheduled",
    minutes: "",
    location: null,
    completed_at: null,
    cancelled_at: null,
    created_at: "2026-10-01T00:00:00.000Z",
    updated_at: "2026-10-01T00:00:00.000Z",
    created_by: "user-1",
    counts: { items: 0, participants: 0, pending_outcome: 0 },
    items: [],
    participants: [],
    briefing: null,
  }) as IReviewSessionDetail;

const makeStore = () => {
  const store = new ReviewStore({} as never);
  const reviewService = {
    generateReviewBriefing: vi.fn(async () => makeBriefing()),
  };
  (store as unknown as { reviewService: typeof reviewService }).reviewService = reviewService;
  return { store, reviewService };
};

describe("ReviewStore.generateBriefing", () => {
  it("stores the generated briefing on the cached session detail", async () => {
    const { store, reviewService } = makeStore();
    store.sessionDetailMap["session-1"] = makeSessionDetail();

    await store.generateBriefing("acme", "session-1", "id");

    expect(reviewService.generateReviewBriefing).toHaveBeenCalledWith("acme", "session-1", "id");
    expect(store.getSessionDetailById("session-1")?.briefing?.overall).toBe("Ringkasan");
  });

  it("propagates service errors", async () => {
    const { store, reviewService } = makeStore();
    reviewService.generateReviewBriefing.mockRejectedValueOnce(new Error("boom"));

    await expect(store.generateBriefing("acme", "session-1", "en")).rejects.toThrow("boom");
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run:

```bash
pnpm --filter=web test review.store.test.ts
```

Expected: FAIL — `store.generateBriefing is not a function`.

- [ ] **Step 3: Add the service method**

In `apps/web/core/services/review.service.ts`, add `IReviewSessionBriefing` to the type import list and append this method to `ReviewService`:

```ts
  async generateReviewBriefing(
    workspaceSlug: string,
    sessionId: string,
    language: string
  ): Promise<IReviewSessionBriefing> {
    return this.post(`${this.workspacePath(workspaceSlug)}/review-sessions/${sessionId}/briefing/`, { language })
      .then((response) => response?.data)
      .catch((error) => {
        throw toReviewError(error);
      });
  }
```

- [ ] **Step 4: Add the store action**

In `apps/web/core/store/review.store.ts`:

1. Add `IReviewSessionBriefing` to the `@plane/types` import list.
2. Add to the `IReviewStore` interface, after `fetchSessionDetail`:

```ts
generateBriefing: (workspaceSlug: string, sessionId: string, language: string) => Promise<IReviewSessionBriefing>;
```

3. Add `generateBriefing: action,` to the `makeObservable` block (after `fetchSessionDetail: action,`).
4. Add the action implementation after `fetchSessionDetail`:

```ts
generateBriefing = async (workspaceSlug: string, sessionId: string, language: string) => {
  const briefing = await this.reviewService.generateReviewBriefing(workspaceSlug, sessionId, language);
  runInAction(() => {
    const detail = this.sessionDetailMap[sessionId];
    if (detail) set(this.sessionDetailMap, [sessionId], { ...detail, briefing });
  });
  return briefing;
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run:

```bash
pnpm --filter=web test review.store.test.ts
```

Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/services/review.service.ts apps/web/core/store/review.store.ts apps/web/core/store/review.store.test.ts
git commit -m "feat(web): add review briefing service and store action"
```

---

## Task 10: i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/review.json`
- Modify: `packages/i18n/src/locales/*/review.json` (19 other locales)

- [ ] **Step 1: Add the English keys**

In `packages/i18n/src/locales/en/review.json`, inside `"review"`, add this block after `"participants"` and before `"notification"`:

```json
    "briefing": {
      "title": "Briefing",
      "ai_badge": "AI",
      "disclaimer": "AI-generated — verify the facts before deciding.",
      "generate": "Generate briefing",
      "regenerate": "Regenerate",
      "generated": "Briefing generated.",
      "generating": "The AI is drafting the briefing… this can take up to a minute.",
      "empty": "No briefing yet. Generate one after the agenda is set.",
      "empty_readonly": "No briefing for this session.",
      "generated_at": "Generated {date} by {name} · {model}",
      "overall": "Summary",
      "discussion_points": "Discussion points",
      "risks": "Risks",
      "item_no_ai": "No AI notes for this item.",
      "skipped_notice": "{count} agenda item(s) were not included (limit of 20 per generate).",
      "text_mode_notice": "The AI output was not structured; showing it as plain text.",
      "stale_notice": "The agenda changed since this briefing was generated.",
      "generate_failed": "Could not generate the briefing. Please try again."
    },
```

- [ ] **Step 2: Translate into the 19 other locales**

Load the repo `translate` skill (`.claude/skills/translate/SKILL.md`) and follow it exactly. Apply the DNT glossary: keep `TCB`, `RCB`, `AI`, `JSON` Latin; translate "Briefing" as a common noun. Preserve the ICU variables `{date}`, `{name}`, `{model}`, `{count}` character-for-character. One locale file at a time; never copy English values into non-English files.

- [ ] **Step 3: Regenerate types and rebuild the package**

Run:

```bash
pnpm --filter=@plane/i18n run generate:types
pnpm --filter=@plane/i18n build
```

Expected: build complete; `dist/index.js` contains `"review"` namespace.

- [ ] **Step 4: Check drift**

Run:

```bash
pnpm --filter=@plane/i18n run sync:check
```

Expected: the pre-existing `auth.*` drift (18 missing / 32 stale per locale) is unchanged — no new `review.briefing.*` missing/stale keys. Do not chase the pre-existing auth drift.

- [ ] **Step 5: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "feat(i18n): add review briefing keys"
```

---

## Task 11: Session briefing UI

**Files:**

- Create: `apps/web/core/components/reviews/session/session-briefing.tsx`
- Modify: `apps/web/core/components/reviews/session/review-session-detail-root.tsx`

- [ ] **Step 1: Create the component**

Create `apps/web/core/components/reviews/session/session-briefing.tsx`:

```tsx
import { useState } from "react";
import { observer } from "mobx-react";
import { Link } from "react-router";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail } from "@plane/types";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// helpers
import { briefingItemById, isBriefingStale, isBriefingTextMode } from "@/services/review.helpers";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionBriefing = observer(function SessionBriefing({ workspaceSlug, session, canManage }: Props) {
  const { t, currentLocale } = useTranslation();
  const { generateBriefing } = useReview();
  const [isGenerating, setIsGenerating] = useState(false);

  const briefing = session.briefing;
  const canGenerate = canManage && session.status === "scheduled";
  // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; sorting a fresh copied list
  const items = [...session.items].sort((a, b) => a.position - b.position);
  const briefByItem = briefingItemById(briefing);
  const isTextMode = isBriefingTextMode(briefing);
  const isStale = isBriefingStale(briefing, items.length);

  const handleGenerate = async () => {
    setIsGenerating(true);
    try {
      await generateBriefing(workspaceSlug, session.id, currentLocale);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.briefing.generated") });
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.briefing.generate_failed"),
      });
    } finally {
      setIsGenerating(false);
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h3 className="text-13 font-semibold text-primary">{t("review.briefing.title")}</h3>
          <span className="rounded-full bg-layer-2 px-2 py-0.5 text-10 font-medium text-tertiary">
            {t("review.briefing.ai_badge")}
          </span>
        </div>
        {canGenerate && (
          <Button variant="secondary" size="sm" loading={isGenerating} onClick={() => void handleGenerate()}>
            {briefing ? t("review.briefing.regenerate") : t("review.briefing.generate")}
          </Button>
        )}
      </div>
      {isGenerating && <p className="text-12 text-secondary">{t("review.briefing.generating")}</p>}
      {!briefing && !isGenerating && (
        <p className="text-12 text-secondary">
          {canManage ? t("review.briefing.empty") : t("review.briefing.empty_readonly")}
        </p>
      )}
      {briefing && (
        <div className="space-y-3">
          <p className="text-11 text-tertiary">
            {t("review.briefing.generated_at", {
              date: `${renderFormattedDate(briefing.generated_at)} ${renderFormattedTime(briefing.generated_at)}`,
              name: briefing.generated_by_name,
              model: briefing.model,
            })}
          </p>
          <p className="text-11 text-tertiary">{t("review.briefing.disclaimer")}</p>
          {isTextMode && <p className="text-11 text-warning-primary">{t("review.briefing.text_mode_notice")}</p>}
          {briefing.skipped_items > 0 && (
            <p className="text-11 text-warning-primary">
              {t("review.briefing.skipped_notice", { count: briefing.skipped_items })}
            </p>
          )}
          {isStale && canGenerate && (
            <p className="text-11 text-warning-primary">{t("review.briefing.stale_notice")}</p>
          )}
          {briefing.overall && (
            <div className="space-y-1">
              <h4 className="text-12 font-medium text-primary">{t("review.briefing.overall")}</h4>
              <p className="text-12 whitespace-pre-wrap text-secondary">{briefing.overall}</p>
            </div>
          )}
          {!isTextMode && items.length > 0 && (
            <ul className="space-y-2">
              {items.map((item) => {
                const brief = briefByItem.get(item.id);
                const label = [item.subject.identifier, item.subject.name].filter(Boolean).join(" ");
                const facts = item.facts;
                const subjectHref =
                  item.subject.kind === "change" && facts.issue_id && facts.project_id
                    ? `/${workspaceSlug}/projects/${facts.project_id}/issues/${facts.issue_id}`
                    : item.subject.kind === "release" && facts.release_id
                      ? `/${workspaceSlug}/releases/${facts.release_id}`
                      : null;
                const warRoomHref =
                  facts.war_room && facts.project_id
                    ? `/${workspaceSlug}/projects/${facts.project_id}/war-rooms/${facts.war_room.id}`
                    : null;
                return (
                  <li key={item.id} className="space-y-2 rounded-md border border-subtle px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-11 text-tertiary">
                      {subjectHref ? (
                        <Link to={subjectHref} className="text-13 text-accent-primary hover:underline">
                          {label}
                        </Link>
                      ) : (
                        <span className="text-13 text-primary">{label}</span>
                      )}
                      {item.subject.kind === "change" && (
                        <>
                          {facts.priority && <span>· {facts.priority}</span>}
                          {facts.state_name && <span>· {facts.state_name}</span>}
                          {(facts.assignees?.length ?? 0) > 0 && <span>· {facts.assignees?.join(", ")}</span>}
                          {facts.target_date && <span>· {facts.target_date}</span>}
                          {facts.war_room && warRoomHref && (
                            <Link to={warRoomHref} className="text-accent-primary hover:underline">
                              · {facts.war_room.name}
                            </Link>
                          )}
                        </>
                      )}
                      {item.subject.kind === "release" && (
                        <>
                          {facts.status && <span>· {facts.status}</span>}
                          {facts.target_date && <span>· {facts.target_date}</span>}
                        </>
                      )}
                    </div>
                    {brief ? (
                      <div className="space-y-1">
                        {brief.summary && <p className="text-12 text-secondary">{brief.summary}</p>}
                        {brief.discussion_points.length > 0 && (
                          <div>
                            <p className="text-11 font-medium text-primary">{t("review.briefing.discussion_points")}</p>
                            <ul className="list-inside list-disc text-11 text-secondary">
                              {brief.discussion_points.map((point) => (
                                <li key={point}>{point}</li>
                              ))}
                            </ul>
                          </div>
                        )}
                        {brief.risks.length > 0 && (
                          <div>
                            <p className="text-11 font-medium text-primary">{t("review.briefing.risks")}</p>
                            <ul className="list-inside list-disc text-11 text-secondary">
                              {brief.risks.map((risk) => (
                                <li key={risk}>{risk}</li>
                              ))}
                            </ul>
                          </div>
                        )}
                      </div>
                    ) : (
                      <p className="text-11 text-tertiary">{t("review.briefing.item_no_ai")}</p>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </section>
  );
});
```

- [ ] **Step 2: Render it in the session root**

In `apps/web/core/components/reviews/session/review-session-detail-root.tsx`:

1. Add the import after `SessionAgenda`:

```ts
import { SessionBriefing } from "./session-briefing";
```

2. Render it between the header block and `SessionAgenda`:

```tsx
        <SessionBriefing workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
        <SessionAgenda workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
```

- [ ] **Step 3: Typecheck**

Run:

```bash
pnpm --filter=web check:types
```

Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/reviews/session/session-briefing.tsx \
  apps/web/core/components/reviews/session/review-session-detail-root.tsx
git commit -m "feat(web): add session briefing UI"
```

---

## Task 12: Web verification, build, and manual E2E

**Files:** none new.

- [ ] **Step 1: Run web checks**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web test
```

Expected: 0 type errors, 0 lint errors, all vitest suites pass.

- [ ] **Step 2: Rebuild dependencies and web, restart prod**

Run:

```bash
pnpm --filter=@plane/types build
pnpm --filter=@plane/i18n build
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 8
curl -s -o /dev/null -w "prod=%{http_code}\n" http://localhost:3000/
```

Expected: `prod=200`.

- [ ] **Step 3: Manual E2E on the TCB board**

1. Open a project → Testing Control → a scheduled session with 2–3 agenda items.
2. Click **Generate briefing**; wait for the spinner to finish (up to 60s).
3. Verify: overall summary paragraph, one card per agenda item in agenda order, each with identifier link, priority/state/assignee/target-date facts, AI summary, discussion points, risks, and the AI disclaimer.
4. Click **Regenerate**; verify `Generated …` timestamp changes.
5. Open a change from a card; verify the link resolves to the issue detail.
6. Complete the session; reopen it; verify the briefing is still visible and the Regenerate button is gone.
7. With the LLM config unset (or unreachable), verify the toast shows the server error and any previous briefing stays.

- [ ] **Step 4: Commit any drift**

```bash
git status --short
# commit only if the build/format produced changes
```

---

## Self-review notes (already applied)

- **Spec coverage:** storage + migration (Task 1), context assembly/prompt/parse/fallback (Tasks 2, 4), API + permissions + status + limits (Tasks 4, 5), fact line + links (Tasks 3, 11), states UI (Task 11), types/service/store/i18n (Tasks 7–10), testing + rebuild + E2E (Tasks 5, 6, 12).
- **Deliberate deviations from the spec's testing section:** the repo has no React component-test harness, so state logic is covered by pure helpers (`briefingItemById`, `isBriefingTextMode`, `isBriefingStale`) plus the store action test instead of render tests. Everything else matches the spec.
- **Type consistency:** `generateBriefing(workspaceSlug, sessionId, language)` is identical in the store interface, implementation, and test; `IReviewSessionBriefing` fields match the Rust JSON contract (`format`, `included_items`, `skipped_items`, `generated_by_name`); `facts` field names match `item_json` (`issue_id`, `project_id`, `priority`, `state_name`, `target_date`, `assignees`, `war_room`, `release_id`, `status`).
