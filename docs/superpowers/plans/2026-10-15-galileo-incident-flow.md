# Galileo Incident Flow (`/incident`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `/incident REF` guided triage & enrich flow to Galileo: server-validated flow context that injects an Incident playbook into the agent preamble, with all six outcome changes still going through existing proposal cards.

**Architecture:** A new `flows` module in `crates/ai` (registry + Incident playbook + DB validation + preamble composition). `POST /ai-agent/` accepts optional `flow`/`flow_ref`, validates them (workspace, access, type = Incident), passes an `ActiveFlow` into `run_agent` for preamble injection, and stamps `flow`/`flow_ref` onto the assistant-message metadata. The web app parses the command, keeps `activeFlow` in the store, restores it from message metadata, and clears it via the allowlisted metadata PATCH on `/incident off` or a `flow_*` error.

**Tech Stack:** Rust (axum, sqlx, rig 0.42), MobX store + React, Vitest, i18next.

**Spec:** `docs/superpowers/specs/2026-10-15-galileo-incident-flow-design.md`

---

## File Structure

| File                                                              | Responsibility                                                                                                                  |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/flows/mod.rs` (create)                 | `Flow` registry, `resolve`, `is_valid_reference`, `ActiveFlow`, `FlowError` codes, `validate()`, `preamble_with_flow()`         |
| `apps/api-rs/crates/ai/src/flows/incident.rs` (create)            | `PLAYBOOK` const (triage & enrich steps, guardrails, completion checklist)                                                      |
| `apps/api-rs/crates/ai/src/lib.rs` (modify)                       | Register `pub mod flows;`                                                                                                       |
| `apps/api-rs/crates/ai/src/agent.rs` (modify)                     | `run_agent` gains `flow: Option<&ActiveFlow>`; preamble composition                                                             |
| `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` (modify)      | Body fields, pairing + validation ladder, coded 400s, `ActiveFlow` re-export, metadata stamp, tracing, pass flow to `run_agent` |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs` (modify)  | `clean_metadata_patch`: `flow` / `flow_ref` (null-clear pair)                                                                   |
| `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs` (modify)  | `run_agent(..., None)` call site                                                                                                |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs` (modify)          | Fixture + rejection/activation integration tests; call-site updates                                                             |
| `apps/web/core/lib/ai-flows.ts` (create)                          | `TAiFlow`, command parsing, `activeFlowFromMessages`, feedback keys                                                             |
| `apps/web/core/lib/ai-flows.test.ts` (create)                     | Parsing + restore-rule tests                                                                                                    |
| `apps/web/core/lib/ai-context.ts` (modify)                        | `TAiMessage` gains `flow` / `flowRef`                                                                                           |
| `apps/web/core/lib/ai-conversations.ts` (modify)                  | Metadata type, `toAiMessage` mapping, patch type                                                                                |
| `apps/web/core/services/ai.service.ts` (modify)                   | `TChatPayload` gains optional `flow` / `flow_ref`                                                                               |
| `apps/web/core/store/ai-assistant.store.ts` (modify)              | `activeFlow`, hint/notice, command handling, payload, restore, clear                                                            |
| `apps/web/core/store/ai-assistant.store.test.ts` (modify)         | Flow lifecycle tests                                                                                                            |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx` (modify) | Slash guard + suggestions + hint/notice rendering (i18n)                                                                        |
| `packages/i18n/src/constants/namespaces.ts` (modify)              | Add `assistant` namespace                                                                                                       |
| `packages/i18n/src/locales/*/assistant.json` (create ×20)         | `assistant.*` strings                                                                                                           |

---

### Task 1: Flow registry and Incident playbook (pure Rust)

**Files:**

- Create: `apps/api-rs/crates/ai/src/flows/incident.rs`
- Create: `apps/api-rs/crates/ai/src/flows/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/api-rs/crates/ai/src/flows/mod.rs` with only the registry skeleton and tests:

```rust
//! Guided flow modes for the agent: registry, validation, and preamble
//! composition. One flow (Incident) ships today; adding another flow is a new
//! module plus one registry entry.

pub mod incident;

pub struct Flow {
    pub id: &'static str,
    pub command: &'static str,
    pub expected_type: &'static str,
    pub playbook: &'static str,
}

pub static INCIDENT: Flow = Flow {
    id: "incident",
    command: "/incident",
    expected_type: "Incident",
    playbook: incident::PLAYBOOK,
};

static FLOWS: &[&Flow] = &[&INCIDENT];

/// Resolve a flow id to its definition. The only lookup authority.
pub fn resolve(id: &str) -> Option<&'static Flow> {
    FLOWS.iter().copied().find(|flow| flow.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_finds_the_incident_flow() {
        let flow = resolve("incident").expect("incident resolves");
        assert_eq!(flow.command, "/incident");
        assert_eq!(flow.expected_type, "Incident");
    }

    #[test]
    fn resolve_rejects_unknown_flows() {
        assert!(resolve("problem").is_none());
        assert!(resolve("").is_none());
        assert!(resolve("INCIDENT").is_none());
    }

    #[test]
    fn incident_playbook_covers_every_locked_outcome_and_guardrail() {
        let playbook = resolve("incident").unwrap().playbook;
        for marker in [
            "Orient",
            "Clarify",
            "Classification",
            "priority",
            "state",
            "labels",
            "Service link",
            "Assignee",
            "First response",
            "Review",
            "skip",
            "confirm",
        ] {
            assert!(
                playbook.to_lowercase().contains(&marker.to_lowercase()),
                "playbook must cover {marker}"
            );
        }
    }
}
```

Create `apps/api-rs/crates/ai/src/flows/incident.rs` as an empty placeholder so it compiles:

```rust
pub const PLAYBOOK: &str = "placeholder";
```

Add to `apps/api-rs/crates/ai/src/lib.rs` (after `pub mod decision;`):

```rust
pub mod flows;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ai flows::tests -- --nocapture` (from `apps/api-rs`)
Expected: the playbook coverage test FAILS (missing markers).

- [ ] **Step 3: Write the real playbook**

Replace `PLAYBOOK` in `apps/api-rs/crates/ai/src/flows/incident.rs`:

```rust
//! The Incident triage & enrich playbook, injected into the agent preamble on
//! every turn while the flow is active.

pub const PLAYBOOK: &str = "You are running the guided Incident triage and \
enrichment flow for the work item named above. Work the steps in order but stay \
flexible: the user can ask questions, jump ahead, or skip any step (\"service \
already linked, skip\", \"we don't use labels\"). Every change is a proposal \
card the user must confirm in the UI — never claim a change was applied before \
that confirmation, and never invent facts, people, services, labels, or times. \
Answer in the user's language; keep messages short; ask at most two questions \
at a time. If the user asks something unrelated, answer briefly, then return \
to the flow. If the work item is already fully triaged, say so and offer the \
review recap instead of re-proposing.\n\n\
Step 0 — Orient (read-only, do this before your first message): call \
get_work_item for the incident; list_work_item_comments and \
list_work_item_relations for existing context; get_service if a service is \
linked; search_work_items with keywords from the title and the linked service \
to find similar incidents; search_articles for known errors or workarounds and \
get_article for the best match. Open with a 3-5 line situational summary: what \
the incident is, what is already set (priority, state, service, assignee), what \
similar incidents or KB articles suggest, and what you will propose.\n\n\
Step 1 — Clarify (only when needed): if impact, urgency, or the existence of a \
workaround is unclear from the description and comments, ask at most two \
focused questions (what is affected and how widely; is there a workaround). \
State assumptions explicitly instead of guessing. Then decide priority using \
impact x urgency over urgent, high, medium, or low.\n\n\
Step 2 — Classification (one update_work_item proposal): propose priority \
(your recommendation plus one line of reasoning), a state transition to the \
project's triaged or working state (resolve valid names with list_states), and \
labels only from list_labels (for example a category or a known-error tag when \
a KB article matched). Propose only fields that should change; put priority, \
state, and labels in one update_work_item call. The card fields stay editable, \
so invite the user to adjust before confirming.\n\n\
Step 3 — Service link: if the incident already has a service, say so and treat \
this step as skipped. Otherwise pick the best service with list_services or \
get_service based on the description and similar incidents; when two services \
fit, ask the user which one. Propose manage_service_links.\n\n\
Step 4 — Assignee: suggest one assignee from list_members, preferring people \
who resolved similar incidents or own the linked service; ask when ambiguous. \
Propose update_work_item with the assignee. If the user already named someone \
in step 2, include them there instead of a second proposal.\n\n\
Step 5 — First response (one add_comment proposal): draft the acknowledgment \
comment — a one-line impact summary, the priority and state, any known error \
or workaround with its KB article reference, the owner, concrete next steps, \
and an ETA only if the user provided one (otherwise ask or omit).\n\n\
Step 6 — Review: close with a recap of every outcome — priority, state, \
labels, service link, assignee, first response — marked as proposed, already \
set, or explicitly skipped; remind the user that proposed cards still await \
their confirmation in the UI. Then offer to triage another incident with \
/incident REF or to keep chatting.\n\n\
Completion: the flow is complete when every outcome is proposed, already set, \
or explicitly skipped. Never fire two proposals for the same field; if the \
user changes an answer, make one superseding proposal and say it replaces the \
previous one.";
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ai flows::tests -- --nocapture`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/flows apps/api-rs/crates/ai/src/lib.rs
git commit -m "feat(api-rs): flow registry and incident triage playbook"
```

---

### Task 2: Flow validation and preamble composition (Rust)

**Files:**

- Modify: `apps/api-rs/crates/ai/src/flows/mod.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `apps/api-rs/crates/ai/src/flows/mod.rs`:

```rust
    #[test]
    fn is_valid_reference_accepts_proj_123_and_rejects_junk() {
        assert!(is_valid_reference("OPS-123"));
        assert!(is_valid_reference("lts-7"));
        assert!(!is_valid_reference("OPS"));
        assert!(!is_valid_reference("OPS-"));
        assert!(!is_valid_reference("-1"));
        assert!(!is_valid_reference(""));
        assert!(!is_valid_reference("OPS-1; DROP TABLE issues"));
    }

    #[tokio::test]
    async fn validate_rejects_unknown_flows_before_touching_the_db() {
        let pool = crate::tools::lazy_pool();
        let error = validate(&pool, uuid::Uuid::nil(), uuid::Uuid::nil(), "problem", "OPS-1")
            .await
            .expect_err("unknown flow");
        assert_eq!(error.code, CODE_UNKNOWN);
    }

    #[tokio::test]
    async fn validate_rejects_malformed_references_before_touching_the_db() {
        let pool = crate::tools::lazy_pool();
        let error = validate(&pool, uuid::Uuid::nil(), uuid::Uuid::nil(), "incident", "not-a-ref")
            .await
            .expect_err("malformed ref");
        assert_eq!(error.code, CODE_INVALID);
    }

    #[test]
    fn preamble_with_flow_names_the_work_item_and_embeds_the_playbook() {
        let active = ActiveFlow {
            id: "incident",
            reference: "OPS-123".to_string(),
        };
        let preamble = preamble_with_flow("BASE", &active);
        assert!(preamble.starts_with("BASE\n\n"));
        assert!(preamble.contains("Active work item: OPS-123"));
        assert!(preamble.contains("Incident triage"));
        assert!(preamble.contains("proposal"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ai flows::tests`
Expected: FAIL to compile (`validate`, `is_valid_reference`, `ActiveFlow`, `preamble_with_flow` missing).

- [ ] **Step 3: Implement validation and composition**

Add to `apps/api-rs/crates/ai/src/flows/mod.rs` (imports at the top, below the module docs):

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::tools::{parse_work_item_ref, resolve_work_item};
```

And the implementation, after `resolve`:

```rust
/// A validated active flow attached to one agent turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFlow {
    pub id: &'static str,
    pub reference: String,
}

/// A validation failure, carrying machine-readable `code` for the FE plus a
/// human-readable `message` for the chat bubble.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowError {
    pub code: &'static str,
    pub message: String,
}

pub const CODE_UNKNOWN: &str = "flow_unknown";
pub const CODE_INVALID: &str = "flow_invalid";
pub const CODE_REF_GONE: &str = "flow_ref_gone";
pub const CODE_TYPE_MISMATCH: &str = "flow_type_mismatch";

/// `PROJ-123` shape check, exposed for the metadata allowlist.
pub fn is_valid_reference(reference: &str) -> bool {
    parse_work_item_ref(reference).is_ok()
}

const WORK_ITEM_TYPE_SQL: &str = "SELECT t.name FROM issues i \
     LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
     WHERE i.id = $1";

/// Validate a requested flow turn. Order: registered flow -> reference shape ->
/// work item exists and is accessible -> work item type matches the flow.
/// Pure for the first two steps (unit-testable), DB-backed for the rest.
pub async fn validate(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    flow_id: &str,
    reference: &str,
) -> Result<ActiveFlow, FlowError> {
    let Some(flow) = resolve(flow_id) else {
        return Err(FlowError {
            code: CODE_UNKNOWN,
            message: format!("unknown flow '{flow_id}'"),
        });
    };
    if parse_work_item_ref(reference).is_err() {
        return Err(FlowError {
            code: CODE_INVALID,
            message: format!("'{reference}' is not a work item reference (e.g. OPS-123)"),
        });
    }
    let resolved = resolve_work_item(pool, workspace_id, user_id, reference)
        .await
        .map_err(|_| FlowError {
            code: CODE_REF_GONE,
            message: format!("work item '{reference}' was not found or is not accessible"),
        })?;
    let type_name: Option<String> = sqlx::query_scalar(WORK_ITEM_TYPE_SQL)
        .bind(resolved.id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "ai-flow: type lookup failed");
            FlowError {
                code: CODE_REF_GONE,
                message: format!("work item '{}' could not be loaded", resolved.identifier),
            }
        })?
        .flatten();
    match type_name {
        Some(name) if name.eq_ignore_ascii_case(flow.expected_type) => {}
        Some(name) => {
            return Err(FlowError {
                code: CODE_TYPE_MISMATCH,
                message: format!(
                    "{} is a {name}, not an {}",
                    resolved.identifier, flow.expected_type
                ),
            });
        }
        None => {
            return Err(FlowError {
                code: CODE_TYPE_MISMATCH,
                message: format!("{} has no work item type", resolved.identifier),
            });
        }
    }
    Ok(ActiveFlow {
        id: flow.id,
        reference: resolved.identifier,
    })
}

/// Append the flow block to the base preamble. Unknown ids fall back to the
/// base preamble; `validate` makes that unreachable in the route.
pub fn preamble_with_flow(base: &str, active: &ActiveFlow) -> String {
    match resolve(active.id) {
        Some(flow) => format!(
            "{base}\n\n# Guided flow: {label} {command}\nActive work item: {reference}\n\n{playbook}",
            label = flow.expected_type,
            command = flow.command,
            reference = active.reference,
            playbook = flow.playbook,
        ),
        None => base.to_string(),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ai flows::tests`
Expected: all flow unit tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/flows/mod.rs
git commit -m "feat(api-rs): flow validation and preamble composition"
```

---

### Task 3: `run_agent` flow parameter and call sites

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs:209-236`
- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs:31-41` (re-export)
- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs:210-217`
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs` (7 call sites + new test)

- [ ] **Step 1: Write the failing test**

Add to `apps/api-rs/crates/api/tests/ai_agent_test.rs` (after `prompt_without_tools_returns_content`):

```rust
#[tokio::test]
async fn flow_preamble_lands_in_the_system_message() {
    let (base, upstream) = spawn_capturing().await;
    let active = ActiveFlow {
        id: "incident",
        reference: "OPS-123".to_string(),
    };
    let out = run_agent(
        &base,
        "key",
        "model",
        ToolServer::new().run(),
        None,
        "hi",
        Some(&active),
    )
    .await;
    assert_eq!(out, Ok("final answer".to_string()));
    let bodies = upstream.bodies.lock().unwrap();
    let messages = bodies[0]["messages"].as_array().unwrap();
    let system = messages
        .iter()
        .find(|m| m["role"] == json!("system"))
        .expect("upstream call must carry a system message");
    let content = system["content"].as_str().unwrap();
    assert!(content.contains("Active work item: OPS-123"));
    assert!(content.contains("Incident triage"));
}
```

Update the file's imports to include the re-exported type:

```rust
use api::routes::ai_agent::{run_agent, ActiveFlow};
```

(Keep the existing `use api::routes::ai_agent::run_agent;` replaced by the line above.)

Update all existing `run_agent(...)` call sites in this test file (lines ~100, ~122, ~436, ~473, ~502, ~509, ~529) to pass `None` as the last argument, e.g.:

```rust
    let out = run_agent(
        &base,
        "key",
        "model",
        ToolServer::new().run(),
        Some("be terse"),
        "hi",
        None,
    )
    .await;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test flow_preamble_lands_in_the_system_message -- --test-threads=1`
Expected: FAIL to compile — `run_agent` takes 6 args and `ActiveFlow` is not exported.

- [ ] **Step 3: Change `run_agent` and export `ActiveFlow`**

In `apps/api-rs/crates/ai/src/agent.rs`, add the import at the top:

```rust
use crate::flows::{self, ActiveFlow};
```

Change the signature and preamble build in `run_agent`:

```rust
pub async fn run_agent(
    base_url: &str,
    api_key: &str,
    model: &str,
    tool_server: ToolServerHandle,
    task: Option<&str>,
    prompt: &str,
    flow: Option<&ActiveFlow>,
) -> Result<String, LlmError> {
    let client = openai::CompletionsClient::builder()
        .api_key(api_key)
        .base_url(base_url)
        .http_client(http_client().clone())
        .build()
        .map_err(|e| {
            tracing::warn!(error = %e, "ai-agent: llm client build failed");
            LlmError::Upstream
        })?;
    let preamble = match flow {
        Some(active) => flows::preamble_with_flow(PREAMBLE, active),
        None => PREAMBLE.to_string(),
    };
    let agent = client
        .agent(model)
        .preamble(preamble.as_str())
        .tool_server_handle(tool_server)
        .default_max_turns(MAX_TURNS)
        .build();
    agent
        .prompt(effective_prompt(task, prompt))
        .await
        .map_err(map_prompt_error)
}
```

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`, extend the re-export block:

```rust
// Used by `crates/api/tests/ai_agent_test.rs`; the bin target sees them as
// unused, so the lint is allowed there.
#[allow(unused_imports)]
pub use ai::agent::{ToolCallTrace, ToolTrace};
#[allow(unused_imports)]
pub use ai::flows::ActiveFlow;
```

In `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`, pass `None`:

```rust
        ai::agent::run_agent(
            &config.base_url,
            &config.api_key,
            &config.model,
            handle,
            None,
            &run.prompt,
            None,
        ),
```

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` (`workspace_ai_agent`), pass `None` for now (Task 5 flips it to the validated flow):

```rust
        run_agent(
            &cfg.base_url,
            &cfg.api_key,
            &cfg.model,
            tool_server,
            task,
            &model_prompt,
            None,
        ),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: all tests PASS (including the new one). Also run `cargo check -p worker` — expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/agent.rs apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): run_agent accepts an active flow for preamble injection"
```

---

### Task 4: Route rejects invalid flow turns (integration)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs:138-172`
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs` (fixtures + rejection tests)

- [ ] **Step 1: Write the failing tests**

First, add a fixture helper to `impl Scratch` in `apps/api-rs/crates/api/tests/ai_agent_test.rs` (after `add_actor`) and extend `purge`:

```rust
    /// One project (with the owner as project member), one work item type, and
    /// one work item `IDENT-1` of that type. Returns (project_id, "IDENT-1").
    async fn add_project_with_work_item(&self, pool: &PgPool, type_name: &str) -> (Uuid, String) {
        let project_id = Uuid::new_v4();
        let identifier = format!("T{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();
        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'Ops', '', 2, $2, $3, false, false, false, false, \
             false, 30, 30, '{}'::jsonb, false, true, false, 'UTC')",
        )
        .bind(project_id)
        .bind(&identifier)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");
        sqlx::query(
            "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, \
             is_active, view_props, default_props, sort_order, preferences, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, 20, $2, $3, true, '{}', '{}', 65535, '{}', now(), now())",
        )
        .bind(self.user_id)
        .bind(project_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch project member");
        let type_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
             is_active, level, requires_service, workspace_id, created_at, updated_at) \
             VALUES ($1, $2, 'Something is broken', '{}'::jsonb, false, false, true, 0, false, \
             $3, now(), now())",
        )
        .bind(type_id)
        .bind(type_name)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch type");
        sqlx::query(
            "INSERT INTO issues (id, name, description_html, description_json, priority, \
             sequence_id, is_draft, sort_order, project_id, workspace_id, created_by_id, type_id, \
             description_stripped, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Pump down', '<p>Pump down</p>', '{}', 'none', 1, false, \
             65535, $1, $2, $3, $4, 'Pump down', now(), now())",
        )
        .bind(project_id)
        .bind(self.workspace_id)
        .bind(self.user_id)
        .bind(type_id)
        .execute(pool)
        .await
        .expect("scratch issue");
        (project_id, format!("{identifier}-1"))
    }
```

In `purge`, before `DELETE FROM workspaces`:

```rust
        for statement in [
            "DELETE FROM issues WHERE workspace_id = $1",
            "DELETE FROM issue_types WHERE workspace_id = $1",
            "DELETE FROM project_members WHERE workspace_id = $1",
            "DELETE FROM projects WHERE workspace_id = $1",
        ] {
            sqlx::query(statement)
                .bind(self.workspace_id)
                .execute(pool)
                .await
                .ok();
        }
```

Add the rejection tests at the end of the file:

```rust
#[tokio::test]
async fn flow_turns_reject_unknown_flow_ids() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": "/problem OPS-1",
            "conversation_id": conversation_id,
            "flow": "problem",
            "flow_ref": "OPS-1",
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_unknown"));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn flow_turns_reject_malformed_and_missing_pairs() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // flow without flow_ref
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": "/incident",
            "conversation_id": conversation_id,
            "flow": "incident",
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_invalid"));

    // malformed reference
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": "/incident nothing",
            "conversation_id": conversation_id,
            "flow": "incident",
            "flow_ref": "nothing",
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_invalid"));

    // no partial state: neither turn inserted a message
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_messages WHERE conversation_id = $1")
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn flow_turns_reject_inaccessible_and_wrong_type_refs() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // nonexistent
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": "/incident T00000000-1",
            "conversation_id": conversation_id,
            "flow": "incident",
            "flow_ref": "T00000000-1",
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_ref_gone"));

    // exists but is a Change, not an Incident
    let (_, change_ref) = scratch.add_project_with_work_item(&pool, "Change").await;
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": format!("/incident {change_ref}"),
            "conversation_id": conversation_id,
            "flow": "incident",
            "flow_ref": change_ref,
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_type_mismatch"));
    assert!(body["error"].as_str().unwrap().contains("Change"));

    // accessible to the workspace but not a project member
    let (_, incident_ref) = scratch.add_project_with_work_item(&pool, "Incident").await;
    let outsider = scratch.add_actor(&pool, 15).await;
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(outsider),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": format!("/incident {incident_ref}"),
            "conversation_id": conversation_id,
            "flow": "incident",
            "flow_ref": incident_ref,
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_ref_gone"));

    // cross-workspace reference
    let other = Scratch::new(&pool).await;
    let (_, other_ref) = other.add_project_with_work_item(&pool, "Incident").await;
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": format!("/incident {other_ref}"),
            "conversation_id": conversation_id,
            "flow": "incident",
            "flow_ref": other_ref,
        })),
    )
    .await
    .expect("agent call");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], json!("flow_ref_gone"));

    other.purge(&pool).await;
    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test flow_turns_reject -- --test-threads=1`
Expected: FAIL — the route ignores `flow`/`flow_ref`, so these calls reach the LLM-less config check and return `400 AI is not configured`, not the flow codes.

- [ ] **Step 3: Implement the validation ladder in the route**

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`, add to imports:

```rust
use ai::flows;
```

In `workspace_ai_agent`, after the conversation mode check (the block returning `"conversation mode does not match this endpoint"`) and before `resolve_llm_config`, insert:

```rust
    // Guided-flow turns: `flow` and `flow_ref` must come together and pass the
    // full validation ladder before anything is inserted or the LLM is called.
    let flow_id = body
        .get("flow")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let flow_ref = body
        .get("flow_ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let active_flow = match (flow_id, flow_ref) {
        (None, None) => None,
        (Some(id), Some(reference)) => match flows::validate(&st.pool, workspace_id, auth.0, id, reference).await {
            Ok(active) => {
                tracing::info!(
                    flow_id = %active.id,
                    flow_ref = %active.reference,
                    "ai-agent: flow turn"
                );
                Some(active)
            }
            Err(error) => {
                tracing::warn!(code = %error.code, "ai-agent: flow rejected");
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": error.message, "code": error.code})),
                ));
            }
        },
        _ => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "flow and flow_ref must be provided together",
                    "code": flows::CODE_INVALID,
                })),
            ));
        }
    };
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test flow_turns_reject -- --test-threads=1`
Expected: 3 tests PASS. Then run the whole file to check nothing regressed:
`DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1`

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): validate flow turns on the ai-agent route"
```

---

### Task 5: Route activates valid flows (metadata + prompt)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` (metadata block ~241, `run_agent` call)
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `apps/api-rs/crates/api/tests/ai_agent_test.rs`:

```rust
#[tokio::test]
async fn flow_turn_stamps_metadata_and_injects_the_playbook() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;
    let (_, incident_ref) = scratch.add_project_with_work_item(&pool, "Incident").await;

    let (base_url, upstream) = spawn_capturing().await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "prompt": format!("/incident {incident_ref}"),
            "conversation_id": conversation_id,
            "flow": "incident",
            "flow_ref": incident_ref,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);

    // Assistant message metadata carries the canonical reference.
    assert_eq!(body["assistant_message"]["metadata"]["flow"], json!("incident"));
    assert_eq!(body["assistant_message"]["metadata"]["flow_ref"], json!(incident_ref));

    // The stored row carries it too.
    let stored: Value = sqlx::query_scalar(
        "SELECT metadata FROM ai_messages WHERE conversation_id = $1 AND role = 'assistant'",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["flow"], json!("incident"));
    assert_eq!(stored["flow_ref"], json!(incident_ref));

    // The upstream prompt carried the playbook with the canonical reference.
    let bodies = upstream.bodies.lock().unwrap();
    let messages = bodies[0]["messages"].as_array().unwrap();
    let system = messages
        .iter()
        .find(|m| m["role"] == json!("system"))
        .expect("system message");
    let content = system["content"].as_str().unwrap();
    assert!(content.contains(&format!("Active work item: {incident_ref}")));
    assert!(content.contains("Incident triage"));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test flow_turn_stamps_metadata -- --test-threads=1`
Expected: FAIL — metadata has no `flow` key and the system message lacks the playbook.

- [ ] **Step 3: Wire the flow into the turn**

In `workspace_ai_agent` (`apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`), pass the validated flow to the agent (replace the `None` added in Task 3):

```rust
        run_agent(
            &cfg.base_url,
            &cfg.api_key,
            &cfg.model,
            tool_server,
            task,
            &model_prompt,
            active_flow.as_ref(),
        ),
```

In the metadata build (where `metadata["proposals"]` is set), add:

```rust
            if let Some(active) = active_flow.as_ref() {
                metadata["flow"] = json!(active.id);
                metadata["flow_ref"] = json!(active.reference);
            }
```

Note: `active_flow` is moved out of by the `match` into `run_agent`'s borrow — keep `active_flow` alive for the metadata block by passing `active_flow.as_ref()` (borrow) as shown, then reusing it below.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): activate incident flows with playbook and metadata"
```

---

### Task 6: Metadata allowlist accepts flow clears

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs:401-430` (+ unit tests in the same file)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `apps/api-rs/crates/api/src/routes/ai_conversations.rs`:

```rust
    #[test]
    fn metadata_patch_accepts_flow_pair() {
        let clean = clean_metadata_patch(&patch_map(vec![
            ("flow", json!("incident")),
            ("flow_ref", json!("OPS-123")),
        ]))
        .expect("valid flow pair");
        assert_eq!(clean["flow"], json!("incident"));
        assert_eq!(clean["flow_ref"], json!("OPS-123"));
    }

    #[test]
    fn metadata_patch_accepts_flow_clear_pair() {
        let clean = clean_metadata_patch(&patch_map(vec![
            ("flow", json!(null)),
            ("flow_ref", json!(null)),
        ]))
        .expect("valid clear pair");
        assert!(clean["flow"].is_null());
        assert!(clean["flow_ref"].is_null());
    }

    #[test]
    fn metadata_patch_rejects_malformed_flows() {
        assert!(clean_metadata_patch(&patch_map(vec![("flow", json!("problem"))])).is_err());
        assert!(
            clean_metadata_patch(&patch_map(vec![
                ("flow", json!("incident")),
                ("flow_ref", json!("not a ref")),
            ]))
            .is_err()
        );
        assert!(
            clean_metadata_patch(&patch_map(vec![
                ("flow", json!("incident")),
                ("flow_ref", json!(null)),
            ]))
            .is_err()
        );
        assert!(clean_metadata_patch(&patch_map(vec![("flow", json!(null))])).is_err());
        assert!(clean_metadata_patch(&patch_map(vec![("flow_ref", json!("OPS-1"))])).is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p api metadata_patch_accepts_flow metadata_patch_rejects_malformed_flows -- --nocapture` (from `apps/api-rs`)
Expected: FAIL — `metadata key not allowed: flow`.

- [ ] **Step 3: Extend `clean_metadata_patch`**

In `clean_metadata_patch`, add the two arms to the `match key.as_str()`:

```rust
            "flow" => match value.as_str() {
                Some("incident") => {
                    clean.insert(key.clone(), value.clone());
                }
                None if value.is_null() => {
                    clean.insert(key.clone(), Value::Null);
                }
                _ => return Err("flow must be 'incident' or null".to_string()),
            },
            "flow_ref" => match value.as_str() {
                Some(raw) if ai::flows::is_valid_reference(raw) => {
                    clean.insert(key.clone(), json!(raw.trim()));
                }
                None if value.is_null() => {
                    clean.insert(key.clone(), Value::Null);
                }
                _ => return Err("flow_ref must be a work item reference or null".to_string()),
            },
```

Add the pairing check after the loop, before the empty check:

```rust
    let flow_present = clean.contains_key("flow");
    let ref_present = clean.contains_key("flow_ref");
    if flow_present != ref_present {
        return Err("flow and flow_ref must be set or cleared together".to_string());
    }
    if clean.get("flow").is_some_and(Value::is_null) != clean.get("flow_ref").is_some_and(Value::is_null) {
        return Err("flow and flow_ref must be set or cleared together".to_string());
    }
```

Confirm `use serde_json::{json, Map, Value};` is already imported at the top of the file; if `json` is only in the test module, add `use serde_json::json;` to the file's imports (check first with `grep -n "use serde_json" apps/api-rs/crates/api/src/routes/ai_conversations.rs`).

Add an end-reason log in `patch_message`, right after `clean` is computed and before the UPDATE runs:

```rust
    if clean.get("flow").is_some_and(Value::is_null) {
        tracing::info!("ai-flow: ended via metadata patch");
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p api conversations::tests -- --nocapture` (from `apps/api-rs`)
Expected: all conversation unit tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api-rs): allow clearing flow metadata via message patch"
```

---

### Task 7: Web command parsing lib (`ai-flows.ts`)

**Files:**

- Create: `apps/web/core/lib/ai-flows.ts`
- Test: `apps/web/core/lib/ai-flows.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/web/core/lib/ai-flows.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { activeFlowFromMessages, INCIDENT_FLOW_ID, isIncidentCommand, parseIncidentCommand } from "./ai-flows";

describe("isIncidentCommand", () => {
  it("matches only leading /incident commands", () => {
    expect(isIncidentCommand("/incident OPS-123")).toBe(true);
    expect(isIncidentCommand("  /incident OPS-123")).toBe(true);
    expect(isIncidentCommand("/INCIDENT off")).toBe(true);
    expect(isIncidentCommand("ask about /incident")).toBe(false);
    expect(isIncidentCommand("/incidents OPS-1")).toBe(false);
  });
});

describe("parseIncidentCommand", () => {
  it("parses a start command with a reference", () => {
    expect(parseIncidentCommand("/incident OPS-123")).toEqual({ kind: "start", ref: "OPS-123" });
    expect(parseIncidentCommand("/incident lts-7 please")).toEqual({ kind: "start", ref: "lts-7" });
  });

  it("parses the off command case-insensitively", () => {
    expect(parseIncidentCommand("/incident off")).toEqual({ kind: "off" });
    expect(parseIncidentCommand("/incident OFF")).toEqual({ kind: "off" });
  });

  it("reports missing and malformed references", () => {
    expect(parseIncidentCommand("/incident")).toEqual({ kind: "missing" });
    expect(parseIncidentCommand("/incident   ")).toEqual({ kind: "missing" });
    expect(parseIncidentCommand("/incident nothing")).toEqual({ kind: "invalid" });
    expect(parseIncidentCommand("/incident OPS-")).toEqual({ kind: "invalid" });
  });

  it("returns null for non-commands", () => {
    expect(parseIncidentCommand("hello")).toBeNull();
    expect(parseIncidentCommand("/schedule daily")).toBeNull();
  });
});

describe("activeFlowFromMessages", () => {
  it("returns the newest non-null flow across messages", () => {
    const messages = [
      { flow: "incident" as const, flowRef: "OPS-1" },
      { flow: "incident" as const, flowRef: "OPS-2" },
    ];
    expect(activeFlowFromMessages(messages)).toEqual({ id: INCIDENT_FLOW_ID, ref: "OPS-2" });
  });

  it("treats an explicit null as ended even with older active messages", () => {
    const messages = [
      { flow: "incident" as const, flowRef: "OPS-1" },
      { flow: null, flowRef: null },
    ];
    expect(activeFlowFromMessages(messages)).toBeUndefined();
  });

  it("returns undefined when no message carries a flow key", () => {
    expect(activeFlowFromMessages([{ content: "hi" }, { flow: undefined }])).toBeUndefined();
  });

  it("ignores an active flow without a reference", () => {
    expect(activeFlowFromMessages([{ flow: "incident", flowRef: null }])).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --filter=web exec vitest run core/lib/ai-flows.test.ts`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement `ai-flows.ts`**

Create `apps/web/core/lib/ai-flows.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const INCIDENT_FLOW_ID = "incident" as const;

export type TAiFlow = {
  id: typeof INCIDENT_FLOW_ID;
  ref: string;
};

/** Feedback keys the sidebar renders through i18n (`assistant.<key>`). */
export type TFlowFeedbackKey = "incident_ref_required" | "incident_invalid_ref" | "incident_flow_ended";

export type TIncidentCommand =
  | { kind: "start"; ref: string }
  | { kind: "off" }
  | { kind: "missing" }
  | { kind: "invalid" }
  | null;

const REFERENCE_PATTERN = /^[A-Za-z0-9]+-\d+$/;

/** True when the message starts with the `/incident` slash command (case-insensitive). */
export const isIncidentCommand = (text: string): boolean => /^\/incident(?:\s|$)/i.test(text.trimStart());

export const parseIncidentCommand = (text: string): TIncidentCommand => {
  if (!isIncidentCommand(text)) return null;
  const rest = text.trimStart().slice("/incident".length).trim();
  if (!rest) return { kind: "missing" };
  if (rest.toLowerCase() === "off") return { kind: "off" };
  const ref = rest.split(/\s+/)[0];
  return REFERENCE_PATTERN.test(ref) ? { kind: "start", ref } : { kind: "invalid" };
};

/**
 * Restore rule: the newest message carrying a `flow` key wins — non-null with a
 * reference means the flow is active, an explicit null means it was ended.
 */
export const activeFlowFromMessages = (
  messages: { flow?: string | null; flowRef?: string | null }[]
): TAiFlow | undefined => {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.flow !== undefined) {
      return message.flow === INCIDENT_FLOW_ID && message.flowRef
        ? { id: INCIDENT_FLOW_ID, ref: message.flowRef }
        : undefined;
    }
  }
  return undefined;
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm --filter=web exec vitest run core/lib/ai-flows.test.ts`
Expected: all `ai-flows` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-flows.ts apps/web/core/lib/ai-flows.test.ts
git commit -m "feat(web): incident flow command parsing and restore helpers"
```

---

### Task 8: Web types and metadata mapping

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts:19-33`
- Modify: `apps/web/core/lib/ai-conversations.ts:22-64`
- Modify: `apps/web/core/services/ai.service.ts:26-31`
- Test: `apps/web/core/lib/ai-flows.test.ts` (one more case)

- [ ] **Step 1: Write the failing test**

Add to `apps/web/core/lib/ai-flows.test.ts`:

```ts
import { toAiMessage } from "./ai-conversations";

describe("toAiMessage flow mapping", () => {
  it("maps flow metadata onto the message model", () => {
    const mapped = toAiMessage({
      id: "m1",
      role: "assistant",
      content: "ok",
      metadata: { flow: "incident", flow_ref: "OPS-9" },
      created_at: "2026-10-15T00:00:00Z",
    });
    expect(mapped.flow).toBe("incident");
    expect(mapped.flowRef).toBe("OPS-9");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm --filter=web exec vitest run core/lib/ai-flows.test.ts`
Expected: FAIL — `flow` not on the mapped object (TS error) / undefined at runtime.

- [ ] **Step 3: Add the fields**

In `apps/web/core/lib/ai-context.ts`, extend `TAiMessage`:

```ts
  proposals?: TAiProposal[];
  proposalDecisions?: Record<string, TAiProposalDecision>;
  flow?: string | null;
  flowRef?: string | null;
  createdAt?: string;
```

In `apps/web/core/lib/ai-conversations.ts`:

```ts
export type TAiMessageMetadata = {
  schedule_proposal?: TAiScheduleProposal;
  schedule_proposal_key?: string;
  schedule_decision?: "pending" | "created" | "cancelled";
  created_schedule_id?: string;
  work_item_proposals?: TAiWorkItemProposalEntry[];
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
  proposals?: TAiProposal[];
  proposal_decisions?: Record<string, TAiProposalDecision>;
  flow?: string | null;
  flow_ref?: string | null;
  is_error?: boolean;
};
```

```ts
/** Keys the `PATCH .../messages/:id/` endpoint accepts (server allowlist). */
export type TAiMessageMetadataPatch = Pick<TAiMessageMetadata, "schedule_decision" | "created_schedule_id"> & {
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
  proposal_decisions?: Record<string, TAiProposalDecision>;
  flow?: "incident" | null;
  flow_ref?: string | null;
};
```

And in `toAiMessage`, add:

```ts
  proposals: stored.metadata?.proposals,
  proposalDecisions: stored.metadata?.proposal_decisions,
  flow: stored.metadata?.flow,
  flowRef: stored.metadata?.flow_ref,
  createdAt: stored.created_at,
```

In `apps/web/core/services/ai.service.ts`, extend `TChatPayload`:

```ts
export type TChatPayload = {
  task: string;
  prompt: string;
  context: string;
  conversation_id: string;
  flow?: "incident";
  flow_ref?: string;
};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm --filter=web exec vitest run core/lib/ai-flows.test.ts`
Expected: all `ai-flows` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-conversations.ts apps/web/core/services/ai.service.ts apps/web/core/lib/ai-flows.test.ts
git commit -m "feat(web): map flow metadata through the message model"
```

---

### Task 9: Store flow lifecycle

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Write the failing tests**

Add a new describe block to `apps/web/core/store/ai-assistant.store.test.ts` (after the "AIAssistantStore" describe block's closing):

```ts
describe("incident flow", () => {
  it("starts the flow, switches to agent, and sends flow fields", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    await store.sendMessage("/incident OPS-123");

    expect(store.mode).toBe("agent");
    expect(store.activeFlow).toEqual({ id: "incident", ref: "OPS-123" });
    const call = (services.ai.createAgentTask as any).mock.calls[0];
    expect(call[1].flow).toBe("incident");
    expect(call[1].flow_ref).toBe("OPS-123");
    expect((services.ai.createGptTask as any).mock.calls).toHaveLength(0);
  });

  it("does not send a normal agent turn with flow fields when no flow is active", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.sendMessage("how many incidents?");

    const call = (services.ai.createAgentTask as any).mock.calls[0];
    expect(call[1].flow).toBeUndefined();
    expect(call[1].flow_ref).toBeUndefined();
  });

  it("shows a hint and skips the API when the reference is missing or malformed", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    await store.sendMessage("/incident");
    expect(store.flowHint).toBe("incident_ref_required");

    await store.sendMessage("/incident nothing");
    expect(store.flowHint).toBe("incident_invalid_ref");

    expect((services.ai.createAgentTask as any).mock.calls).toHaveLength(0);
    expect((services.ai.createGptTask as any).mock.calls).toHaveLength(0);
    expect(store.messages.some((message) => message.content.includes("/incident"))).toBe(false);
  });

  it("restores the active flow from message metadata and clears it on /incident off", async () => {
    const services = makeServices({
      conversations: {
        listMessages: vi.fn(async () => [
          { ...storedMessage("srv-1", "user", "/incident OPS-1"), metadata: {} },
          {
            ...storedMessage("srv-2", "assistant", "working on it"),
            metadata: { flow: "incident", flow_ref: "OPS-1" },
          },
        ]),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    expect(store.activeFlow).toEqual({ id: "incident", ref: "OPS-1" });

    await store.sendMessage("/incident off");

    expect(store.activeFlow).toBeUndefined();
    expect(store.flowNotice).toBe("incident_flow_ended");
    expect((services.conversations.updateMessageMetadata as any).mock.calls).toContainEqual([
      "acme",
      "c-agent",
      "srv-2",
      { flow: null, flow_ref: null },
    ]);
    expect((services.ai.createAgentTask as any).mock.calls).toHaveLength(0);
  });

  it("clears the flow when the server rejects it with a flow code", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async () => {
          throw { status: 400, data: { error: "work item OPS-9 was not found", code: "flow_ref_gone" } };
        }),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    await store.sendMessage("/incident OPS-9");

    expect(store.activeFlow).toBeUndefined();
    expect(store.messages.at(-1)?.isError).toBe(true);
    expect(
      (services.conversations.updateMessageMetadata as any).mock.calls.some(
        (args: any[]) => args[3] && args[3].flow === null
      )
    ).toBe(true);
  });

  it("keeps the flow when the turn fails for unrelated reasons", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async () => {
          throw { status: 500, data: { error: "An internal error has occurred." } };
        }),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    await store.sendMessage("/incident OPS-9");

    expect(store.activeFlow).toEqual({ id: "incident", ref: "OPS-9" });
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts`
Expected: FAIL — `activeFlow`, `flowHint`, `flowNotice` do not exist.

- [ ] **Step 3: Implement the store changes**

In `apps/web/core/store/ai-assistant.store.ts`:

Imports — add:

```ts
import { activeFlowFromMessages, parseIncidentCommand, type TFlowFeedbackKey, type TAiFlow } from "@/lib/ai-flows";
```

Interface `IAIAssistantStore` — add after `historyOpen: boolean;`:

```ts
activeFlow: TAiFlow | undefined;
flowHint: TFlowFeedbackKey | undefined;
flowNotice: TFlowFeedbackKey | undefined;
```

and after `setActiveIssueContext` line:

```ts
  clearFlowFeedback: () => void;
```

Class fields — add after `historyOpen = false;`:

```ts
  activeFlow: TAiFlow | undefined = undefined;
  flowHint: TFlowFeedbackKey | undefined = undefined;
  flowNotice: TFlowFeedbackKey | undefined = undefined;
```

`makeObservable` — add:

```ts
      activeFlow: observable.ref,
      flowHint: observable.ref,
      flowNotice: observable.ref,
      clearFlowFeedback: action,
```

New action (next to `setActiveIssueContext`):

```ts
clearFlowFeedback = () => {
  runInAction(() => {
    this.flowHint = undefined;
    this.flowNotice = undefined;
  });
};
```

`setWorkspace` — inside the reset branch, alongside `this.activeIssueContext = undefined;`:

```ts
this.activeFlow = undefined;
this.flowHint = undefined;
this.flowNotice = undefined;
```

`openConversation` — after the messages load, replace:

```ts
runInAction(() => {
  this.messages = messages.map(toAiMessage);
});
```

with:

```ts
runInAction(() => {
  this.messages = messages.map(toAiMessage);
  this.activeFlow = activeFlowFromMessages(this.messages);
});
```

Also add `this.activeFlow = undefined;` in the conversation's 404 branch and the generic catch (where `this.messages = []` is set).

`newChat` — add `this.activeFlow = undefined;` to the reset action.

`sendMessage` — after the entry guard, insert:

```ts
const incident = parseIncidentCommand(trimmed);
if (incident?.kind === "missing" || incident?.kind === "invalid") {
  runInAction(() => {
    this.flowHint = incident.kind === "missing" ? "incident_ref_required" : "incident_invalid_ref";
    this.flowNotice = undefined;
  });
  return;
}
if (incident?.kind === "off") {
  this.endIncidentFlow();
  return;
}
```

Inside the `try` block, replace the schedule auto-switch block:

```ts
if (isScheduleCommand(trimmed) && this.mode !== "agent") {
  this.setMode("agent");
  await this.openLastForMode();
}
```

with:

```ts
if ((incident?.kind === "start" || isScheduleCommand(trimmed)) && this.mode !== "agent") {
  this.setMode("agent");
  await this.openLastForMode();
}
if (incident?.kind === "start") {
  runInAction(() => {
    this.activeFlow = { id: "incident", ref: incident.ref };
    this.flowHint = undefined;
    this.flowNotice = undefined;
  });
}
```

New methods (next to `persistDecision`):

```ts
  endIncidentFlow = () => {
    if (!this.activeFlow) return;
    const conversationId = this.activeConversationId;
    const last = this.messages[this.messages.length - 1];
    runInAction(() => {
      this.activeFlow = undefined;
      this.flowHint = undefined;
      this.flowNotice = "incident_flow_ended";
    });
    if (conversationId && last?.id) void this.persistFlowCleared(conversationId, last.id);
  };

  private persistFlowCleared = async (conversationId: string, messageId: string) => {
    const slug = this.workspaceSlug;
    if (!slug) return;
    try {
      await this.conversationsService.updateMessageMetadata(slug, conversationId, messageId, {
        flow: null,
        flow_ref: null,
      });
    } catch {
      // best-effort: local state is already cleared
    }
  };
```

Declare `endIncidentFlow` in `makeObservable` as `action` too.

`request()` — extend the payload:

```ts
const payload = {
  task: AI_ASSISTANT_TASK,
  prompt: question,
  context: buildAiContext(this.activeIssueContext, userTimezone),
  conversation_id: conversationId,
  ...(mode === "agent" && this.activeFlow ? { flow: this.activeFlow.id, flow_ref: this.activeFlow.ref } : {}),
};
```

`request()` catch — inside `catch (err: any)`, before the generic errorContent is pushed (after the 404 branch):

```ts
if (typeof err?.data?.code === "string" && err.data.code.startsWith("flow_")) {
  const last = this.messages[this.messages.length - 1];
  runInAction(() => {
    this.activeFlow = undefined;
  });
  if (last?.id) void this.persistFlowCleared(conversationId, last.id);
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts`
Expected: the new "incident flow" block passes and every pre-existing store test still passes.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): incident flow lifecycle in the assistant store"
```

---

### Task 10: Sidebar slash suggestions, hints, and notices

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

(No component tests exist for the sidebar; the store/lib tests carry the logic. Verification is the manual smoke in Task 12.)

- [ ] **Step 1: Wire i18n and store fields**

In `apps/web/core/components/ai/assistant-sidebar/root.tsx`:

Add the import:

```ts
import { useTranslation } from "@plane/i18n";
```

In the destructure of `useAiAssistant()`, add `flowHint`, `flowNotice`, `clearFlowFeedback`.

Add inside the component:

```ts
const { t } = useTranslation();
```

- [ ] **Step 2: Extend the slash guard and suggestions**

Replace:

```ts
const trimmedQuestion = question.trimStart().toLowerCase();
const showSlashHint =
  trimmedQuestion.startsWith("/") && !trimmedQuestion.startsWith("/schedule") && !trimmedQuestion.startsWith("/task");
```

with:

```ts
const trimmedQuestion = question.trimStart().toLowerCase();
const showSlashHint =
  trimmedQuestion.startsWith("/") &&
  !trimmedQuestion.startsWith("/schedule") &&
  !trimmedQuestion.startsWith("/task") &&
  !trimmedQuestion.startsWith("/incident");
```

Inside the `showSlashHint` block, after the `/task` button, add two buttons:

```tsx
                      <button
                        type="button"
                        onClick={() => {
                          setQuestion("/incident ");
                          composerRef.current?.focus();
                        }}
                        className="w-full border-b border-subtle px-3.5 py-2 text-left text-12 text-secondary transition-colors hover:text-primary"
                      >
                        /incident <span className="text-tertiary">{t("assistant.slash_incident")}</span>
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setQuestion("/incident off");
                          composerRef.current?.focus();
                        }}
                        className="w-full border-b border-subtle px-3.5 py-2 text-left text-12 text-secondary transition-colors hover:text-primary"
                      >
                        /incident off <span className="text-tertiary">{t("assistant.slash_incident_off")}</span>
                      </button>
```

Migrate the two existing literal suggestion labels to i18n in the same block:

```tsx
                        /schedule — <span className="text-tertiary">{t("assistant.slash_schedule")}</span>
```

```tsx
                        /task — <span className="text-tertiary">{t("assistant.slash_task")}</span>
```

- [ ] **Step 3: Render hint and notice above the composer**

In the composer container (the `<div className="border-t border-subtle px-4 pt-2.5 pb-3">`), above the input wrapper `<div className="focus-within:border-accent-primary ...">`, add:

```tsx
{
  flowHint && (
    <p className="mb-2 text-12 text-danger-primary" role="alert">
      {t(`assistant.${flowHint}`)}
    </p>
  );
}
{
  flowNotice && (
    <p className="mb-2 text-12 text-tertiary" role="status">
      {t(`assistant.${flowNotice}`)}
    </p>
  );
}
```

Clear feedback when the textarea changes — in the textarea's `onChange` handler, call `clearFlowFeedback()` before/after `setQuestion` (match the existing handler shape):

```tsx
                        onChange={(event) => {
                          clearFlowFeedback();
                          setQuestion(event.target.value);
                        }}
```

- [ ] **Step 4: Type-check the sidebar**

Run: `pnpm --filter=web run check:types` (if that script exists; otherwise `pnpm check:types` from the repo root — check `apps/web/package.json`, the standard script is `check:types`).
Expected: PASS (i18n keys are typed only after Task 11 regenerates types; if `t()` typing rejects the new keys now, complete Task 11 first and re-run).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): incident flow suggestions and feedback in the sidebar"
```

---

### Task 11: i18n `assistant` namespace (20 locales)

**Files:**

- Modify: `packages/i18n/src/constants/namespaces.ts`
- Create: `packages/i18n/src/locales/en/assistant.json`
- Create: `packages/i18n/src/locales/{cs,de,es,fr,id,it,ja,ka-ge,ko,pl,pt-BR,ro,ru,sk,tr-TR,ua,vi-VN,zh-CN,zh-TW}/assistant.json`
- Regenerate: `packages/i18n/src/types/keys.generated.ts`

**Load the repo `translate` skill (`.claude/skills/translate/SKILL.md`) before editing locale files — it is required reading.**

- [ ] **Step 1: Register the namespace**

In `packages/i18n/src/constants/namespaces.ts`, add `"assistant",` to `NAMESPACES` (alphabetically, after `"auth"`):

```ts
  "accessibility",
  "assistant",
  "auth",
```

- [ ] **Step 2: Create the English source file**

Create `packages/i18n/src/locales/en/assistant.json`:

```json
{
  "assistant": {
    "slash_schedule": "Schedule a recurring AI report",
    "slash_task": "Create a work item from chat",
    "slash_incident": "Triage an existing incident",
    "slash_incident_off": "End the incident flow",
    "incident_ref_required": "Add an incident ID, e.g. /incident OPS-123",
    "incident_invalid_ref": "That doesn't look like a work item ID (e.g. OPS-123)",
    "incident_flow_ended": "Incident flow ended"
  }
}
```

- [ ] **Step 3: Translate into the other 19 locales**

Following the translate skill: load it, respect the do-not-translate glossary (Plane, `OPS-123`, slash commands `/incident` and `/incident off` stay Latin verbatim), keep placeholder-free strings, and use each locale's sentence-case/punctuation conventions (no trailing period). Create `assistant.json` in each of: cs, de, es, fr, id, it, ja, ka-ge, ko, pl, pt-BR, ro, ru, sk, tr-TR, ua, vi-VN, zh-CN, zh-TW — same key set, translated values, top-level `"assistant"` object.

- [ ] **Step 4: Regenerate types and check parity**

Run: `pnpm --filter @plane/i18n run generate:types`
Run: `pnpm --filter @plane/i18n run check:sync`
Expected: both PASS with no drift reports.

- [ ] **Step 5: Commit**

```bash
git add packages/i18n/src/constants/namespaces.ts packages/i18n/src/locales/*/assistant.json packages/i18n/src/types
git commit -m "feat(i18n): assistant namespace for the incident flow strings"
```

---

### Task 12: Full verification and manual smoke

**Files:** none (verification only)

- [ ] **Step 1: Rust formatting, lint, tests**

From `apps/api-rs`:

```bash
cargo fmt --all
cargo clippy -p ai -p api -p worker -- -D warnings
cargo test -p ai flows::tests
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_conversations_test -- --test-threads=1
```

Expected: all PASS.

- [ ] **Step 2: Web checks**

From the repo root:

```bash
pnpm check
```

Expected: format, lint, and types all PASS.

- [ ] **Step 3: Manual smoke (dev stack, agent mode)**

1. Ensure the stack runs per `AGENTS.md` (backend on 8000, web dev with `VITE_API_BASE_URL=http://192.168.1.11:8000`) and an LLM is configured in the admin panel.
2. Open a workspace, create/seed an Incident work item in a project you are a member of (service optional).
3. In Galileo, type `/incident` → suggestion rows show; send → inline "Add an incident ID" hint, no message sent.
4. Send `/incident NOT-A-REF` → "doesn't look like a work item ID" hint.
5. Send `/incident <REF>` → mode switches to Agent; the reply summarizes the incident and walks classification; a proposal card appears and confirms normally in the UI.
6. Send `/incident off` → "Incident flow ended" notice; reload the page, reopen the conversation → the flow is NOT restored.
7. Re-run `/incident <REF>`, reload mid-flow → flow is restored from metadata and the next turn still carries the playbook.
8. Try `/incident <a non-Incident ref>` (e.g. a Change) → error bubble "… is a Change, not an Incident" and the flow clears.
9. For the tunnel demo: `pnpm --filter=web build` then `systemctl --user restart plane-web-prod.service` (per `AGENTS.md`).

- [ ] **Step 4: Final commit (only if any fixes were needed)**

```bash
git add -A
git commit -m "fix(web,api-rs): incident flow smoke-test fixes"
```

---

## Self-review notes

- **Spec coverage:** §1 registry → Tasks 1-2; §2 request contract → Tasks 4-5; §3 prompt composition → Tasks 2-3; §4 persistence → Tasks 5-6, 8-9; §5 playbook → Task 1; §6 web → Tasks 7-10; i18n → Task 11; observability (`tracing` flow fields) → Tasks 4-5; testing → embedded per task.
- **Deliberate deviation from spec wording:** the sidebar has no component-test harness in this repo, so the spec's "sidebar test" is covered by lib/store tests plus the manual smoke (Step 3.3-3.5). The spec's two hint keys map 1:1 to `TIncidentCommand` kinds `missing`/`invalid`.
- **Type consistency check:** `ActiveFlow { id, reference }` (Rust) ↔ payload `flow`/`flow_ref` ↔ metadata `flow`/`flow_ref` ↔ `TAiFlow { id, ref }` (web) ↔ `TAiMessage.flow/flowRef`. Error codes `flow_unknown | flow_invalid | flow_ref_gone | flow_type_mismatch` are shared constants on both sides.
