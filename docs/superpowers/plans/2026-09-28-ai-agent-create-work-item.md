# AI Agent: Buat Work Item dari Chat (Proposal + Confirm) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** User bisa meminta agent chat membuat work item (bahasa natural atau `/task`); agent mempropose card terstruktur yang bisa diedit, dan setelah Confirm work item dibuat lewat API create issue yang sudah ada.

**Architecture:** Tool baru `create_work_item` (proposal-only, crate `ai`) mencatat proposal ke trace; API Rust menaruh `work_item_proposals` (satu key per proposal) di metadata pesan + `pending_actions` di response; FE merender satu card per proposal, me-resolve nama→UUID lewat store yang ada, memanggil `IssueService.createIssue`, lalu menyimpan decision ke metadata lewat PATCH yang di-allowlist.

**Tech Stack:** Rust (axum, sqlx, rig, schemars) di `apps/api-rs`; React + MobX + vitest di `apps/web`; metadata jsonb (tanpa migration).

**Spec:** `docs/superpowers/specs/2026-09-28-ai-agent-create-work-item-design.md`

**Execution notes (baca sebelum mulai):**

- Semua perintah Rust dijalankan dari `apps/api-rs`; test DB-backed butuh `-- --test-threads=1`.
- DB test default `postgres://plane:plane@localhost:5432/plane`; Redis default `redis://127.0.0.1:6379`.
- Commit hanya file yang disebut di task; working tree punya file unrelated yang termodifikasi — **jangan** `git add -A`.
- Pre-commit menjalankan `oxlint --fix --deny-warnings` pada file web; jangan pakai `key={index}` (`no-array-index-key`).
- Repo auto-rewrite `.sort()` → `.toSorted()` yang ditolak TS lib target: jangan pakai `.sort()`; di plan ini tidak ada yang memakainya.
- Scheduled run tetap read-only: `SPEC_TOOLS` di `crates/ai/src/schedule.rs` tidak berubah, `read_tools` tidak berubah.
- Tidak ada migration; metadata pesan sudah jsonb.

---

### Task 1: Tool `create_work_item` (crate `ai`)

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools.rs`
- Test: `apps/api-rs/crates/ai/src/tools.rs` (mod `tests` di file yang sama)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di akhir mod `tests` (sebelum penutup `}` terakhir) di `apps/api-rs/crates/ai/src/tools.rs`:

```rust
    fn base_work_item_args() -> CreateWorkItemArgs {
        CreateWorkItemArgs {
            project: "LTS".to_string(),
            name: "Fix pump".to_string(),
            description: None,
            priority: None,
            state: None,
            assignees: None,
            labels: None,
            start_date: None,
            target_date: None,
        }
    }

    #[test]
    fn create_work_item_normalizes_and_dedupes() {
        let proposal = work_item_proposal_from_args(CreateWorkItemArgs {
            project: " LTS ".to_string(),
            name: " Fix pump ".to_string(),
            description: Some(" Pump is noisy ".to_string()),
            priority: Some("URGENT".to_string()),
            state: Some(" In Progress ".to_string()),
            assignees: Some(vec![
                "Budi".to_string(),
                " budi ".to_string(),
                "Sari".to_string(),
            ]),
            labels: Some(vec!["maintenance".to_string()]),
            start_date: Some("2026-10-01".to_string()),
            target_date: Some("2026-10-05".to_string()),
        })
        .expect("valid args");
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Fix pump");
        assert_eq!(proposal.description.as_deref(), Some("Pump is noisy"));
        assert_eq!(proposal.priority.as_deref(), Some("urgent"));
        assert_eq!(proposal.state.as_deref(), Some("In Progress"));
        assert_eq!(
            proposal.assignees,
            vec!["Budi".to_string(), "Sari".to_string()]
        );
        assert_eq!(proposal.labels, vec!["maintenance".to_string()]);
        assert_eq!(proposal.start_date.as_deref(), Some("2026-10-01"));
        assert_eq!(proposal.target_date.as_deref(), Some("2026-10-05"));
    }

    #[test]
    fn create_work_item_rejects_bad_args() {
        let empty_project = work_item_proposal_from_args(CreateWorkItemArgs {
            project: "  ".to_string(),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(empty_project.to_string().contains("project"));

        let empty_name = work_item_proposal_from_args(CreateWorkItemArgs {
            name: "  ".to_string(),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(empty_name.to_string().contains("name"));

        let long_name = work_item_proposal_from_args(CreateWorkItemArgs {
            name: "x".repeat(256),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(long_name.to_string().contains("255"));

        let bad_priority = work_item_proposal_from_args(CreateWorkItemArgs {
            priority: Some("p0".to_string()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(bad_priority.to_string().contains("priority"));

        let bad_date = work_item_proposal_from_args(CreateWorkItemArgs {
            start_date: Some("01-10-2026".to_string()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(bad_date.to_string().contains("start_date"));

        let reversed = work_item_proposal_from_args(CreateWorkItemArgs {
            start_date: Some("2026-10-05".to_string()),
            target_date: Some("2026-10-01".to_string()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));

        let too_many = work_item_proposal_from_args(CreateWorkItemArgs {
            assignees: Some((0..11).map(|index| format!("user-{index}")).collect()),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("10"));

        let empty_ref = work_item_proposal_from_args(CreateWorkItemArgs {
            labels: Some(vec!["  ".to_string()]),
            ..base_work_item_args()
        })
        .unwrap_err();
        assert!(empty_ref.to_string().contains("labels"));
    }

    #[tokio::test]
    async fn create_work_item_tool_records_proposal() {
        let trace = crate::agent::new_trace();
        let tool = CreateWorkItem {
            trace: trace.clone(),
        };
        let out = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateWorkItemArgs {
                    project: "LTS".to_string(),
                    name: "Fix pump".to_string(),
                    priority: Some("urgent".to_string()),
                    ..base_work_item_args()
                },
            )
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("proposal json");
        assert_eq!(parsed["project"], json!("LTS"));
        assert_eq!(parsed["name"], json!("Fix pump"));
        assert_eq!(parsed["priority"], json!("urgent"));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "create_work_item");
        assert_eq!(recorded[0].arguments["name"], json!("Fix pump"));
    }

    #[tokio::test]
    async fn rejected_create_work_item_is_not_traced() {
        let trace = crate::agent::new_trace();
        let tool = CreateWorkItem {
            trace: trace.clone(),
        };
        let error = tool
            .call(
                &mut rig::tool::ToolContext::new(),
                CreateWorkItemArgs {
                    name: "  ".to_string(),
                    ..base_work_item_args()
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("name"));
        assert!(trace.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn workspace_tools_builds_a_server_handle_with_create_work_item() {
        let _handle = workspace_tools(lazy_pool(), Uuid::nil(), crate::agent::new_trace());
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p ai create_work_item`
Expected: FAIL — `cannot find type CreateWorkItemArgs` / `cannot find function work_item_proposal_from_args`.

- [ ] **Step 3: Implementasi minimal**

Di `apps/api-rs/crates/ai/src/tools.rs`, tambahkan setelah blok `CreateSchedule` (sekitar baris 378, sebelum `workspace_tools`):

```rust
pub const CREATE_WORK_ITEM_NAME: &str = "create_work_item";

pub const WORK_ITEM_PROJECT_MAX: usize = 100;
pub const WORK_ITEM_NAME_MAX: usize = 255;
pub const WORK_ITEM_DESCRIPTION_MAX: usize = 5000;
pub const WORK_ITEM_STATE_MAX: usize = 100;
pub const WORK_ITEM_REFS_MAX: usize = 10;
pub const WORK_ITEM_REF_MAX: usize = 100;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateWorkItemArgs {
    /// Project identifier (e.g. "LTS") or project name the work item belongs to. Required.
    pub project: String,
    /// Short work item title (1-255 characters).
    pub name: String,
    /// Plain-text description or markdown (max 5000 characters).
    pub description: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// State name to start in, e.g. "In Progress". Omitted means the project default.
    pub state: Option<String>,
    /// Assignee display names or emails (max 10).
    pub assignees: Option<Vec<String>>,
    /// Label names (max 10).
    pub labels: Option<Vec<String>>,
    /// Start date "YYYY-MM-DD".
    pub start_date: Option<String>,
    /// Target date "YYYY-MM-DD"; must not be before start_date.
    pub target_date: Option<String>,
}

/// Normalized proposal recorded in the trace and rendered as a confirmation card.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkItemProposal {
    pub project: String,
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub state: Option<String>,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
}

fn bounded_ref_list(
    values: Option<Vec<String>>,
    label: &str,
) -> Result<Vec<String>, ToolExecutionError> {
    let values = values.unwrap_or_default();
    if values.len() > WORK_ITEM_REFS_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "at most {WORK_ITEM_REFS_MAX} {label} are allowed"
        )));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ToolExecutionError::invalid_args(format!(
                "{label} entries must not be empty"
            )));
        }
        if trimmed.chars().count() > WORK_ITEM_REF_MAX {
            return Err(ToolExecutionError::invalid_args(format!(
                "{label} entries must be at most {WORK_ITEM_REF_MAX} characters"
            )));
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(trimmed.to_string());
    }
    Ok(out)
}

fn parse_iso_date(value: &str, label: &str) -> Result<chrono::NaiveDate, ToolExecutionError> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| ToolExecutionError::invalid_args(format!("{label} must be YYYY-MM-DD")))
}

/// Validate raw tool args into a normalized work item proposal. Human-readable
/// names (state, assignees, labels) are kept as text; the UI resolves them.
pub fn work_item_proposal_from_args(
    args: CreateWorkItemArgs,
) -> Result<WorkItemProposal, ToolExecutionError> {
    let project = args.project.trim();
    if project.is_empty() {
        return Err(ToolExecutionError::invalid_args("project is required"));
    }
    if project.chars().count() > WORK_ITEM_PROJECT_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "project must be at most {WORK_ITEM_PROJECT_MAX} characters"
        )));
    }
    let name = args.name.trim();
    if name.is_empty() {
        return Err(ToolExecutionError::invalid_args("name is required"));
    }
    if name.chars().count() > WORK_ITEM_NAME_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "name must be at most {WORK_ITEM_NAME_MAX} characters"
        )));
    }
    let description = optional_text(args.description.as_deref());
    if let Some(description) = description.as_ref() {
        if description.chars().count() > WORK_ITEM_DESCRIPTION_MAX {
            return Err(ToolExecutionError::invalid_args(format!(
                "description must be at most {WORK_ITEM_DESCRIPTION_MAX} characters"
            )));
        }
    }
    let priority = priority_arg(args.priority.as_deref())?;
    let state = optional_text(args.state.as_deref());
    if let Some(state) = state.as_ref() {
        if state.chars().count() > WORK_ITEM_STATE_MAX {
            return Err(ToolExecutionError::invalid_args(format!(
                "state must be at most {WORK_ITEM_STATE_MAX} characters"
            )));
        }
    }
    let assignees = bounded_ref_list(args.assignees, "assignees")?;
    let labels = bounded_ref_list(args.labels, "labels")?;
    let start_date = args
        .start_date
        .as_deref()
        .map(|value| parse_iso_date(value, "start_date"))
        .transpose()?;
    let target_date = args
        .target_date
        .as_deref()
        .map(|value| parse_iso_date(value, "target_date"))
        .transpose()?;
    if let (Some(start_date), Some(target_date)) = (start_date, target_date) {
        if start_date > target_date {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    Ok(WorkItemProposal {
        project: project.to_string(),
        name: name.to_string(),
        description,
        priority,
        state,
        assignees,
        labels,
        start_date: start_date.map(|date| date.to_string()),
        target_date: target_date.map(|date| date.to_string()),
    })
}

pub struct CreateWorkItem {
    pub trace: ToolTrace,
}

impl Tool for CreateWorkItem {
    const NAME: &'static str = CREATE_WORK_ITEM_NAME;
    type Args = CreateWorkItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one work item in a named project for this workspace. \
         Only call this when the user clearly asks to create a work item or task \
         (natural language or a message starting with /task), and only after the \
         project is known: never guess the project, ask when it is missing or \
         ambiguous. Fill what you can from the conversation (title, description, \
         priority, state, assignee names or emails, label names, dates) and leave \
         the rest out. The user must confirm and may edit every field in the UI \
         before anything is saved. Never claim the work item exists until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateWorkItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = work_item_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("WorkItemProposal serializes"))
    }
}
```

Lalu ubah `workspace_tools` agar mendaftarkan tool baru (tepat sebelum `.run()`):

```rust
        .tool(CreateSchedule {
            trace: trace.clone(),
        })
        .tool(CreateWorkItem {
            trace: trace.clone(),
        })
        .run()
```

`read_tools` **tidak** diubah.

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/api-rs && cargo test -p ai create_work_item`
Expected: PASS (4 test baru + test lama yang relevan).

Run: `cd apps/api-rs && cargo test -p ai`
Expected: PASS semua (test `read_tools`/schedule tidak berubah).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools.rs
git commit -m "feat(ai): add create_work_item proposal tool"
```

---

### Task 2: `pending_actions` + PREAMBLE (crate `ai`)

**Files:**

- Modify: `apps/api-rs/crates/ai/src/agent.rs`
- Test: `apps/api-rs/crates/ai/src/agent.rs` (mod `tests` baru)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di akhir `apps/api-rs/crates/ai/src/agent.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pending_actions_returns_all_proposals_in_order() {
        let trace = new_trace();
        record(&trace, "list_projects", &json!({}));
        record(&trace, CREATE_SCHEDULE_NAME, &json!({"name": "Daily"}));
        record(&trace, CREATE_WORK_ITEM_NAME, &json!({"name": "Fix pump"}));
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["kind"], json!("create_schedule"));
        assert_eq!(actions[0]["proposal"]["name"], json!("Daily"));
        assert_eq!(actions[1]["kind"], json!("create_work_item"));
        assert_eq!(actions[1]["proposal"]["name"], json!("Fix pump"));
    }

    #[test]
    fn pending_action_keeps_returning_the_last_schedule() {
        let trace = new_trace();
        record(&trace, CREATE_SCHEDULE_NAME, &json!({"name": "First"}));
        record(&trace, CREATE_WORK_ITEM_NAME, &json!({"name": "Task"}));
        record(&trace, CREATE_SCHEDULE_NAME, &json!({"name": "Second"}));
        let action = pending_action(&trace).expect("schedule action");
        assert_eq!(action["kind"], json!("create_schedule"));
        assert_eq!(action["proposal"]["name"], json!("Second"));
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p ai pending_actions`
Expected: FAIL — `cannot find function pending_actions` / `CREATE_WORK_ITEM_NAME` belum di-import.

- [ ] **Step 3: Implementasi minimal**

Di `apps/api-rs/crates/ai/src/agent.rs`:

1. Ubah import tool name:

```rust
use crate::tools::{CREATE_SCHEDULE_NAME, CREATE_WORK_ITEM_NAME};
```

2. Tambahkan helper setelah `pending_action` (akhir file):

```rust
/// All proposal tool calls recorded during an agent run, in call order, shaped
/// for the FE confirmation cards. Covers `create_schedule` and
/// `create_work_item`; read-only calls are ignored.
pub fn pending_actions(trace: &ToolTrace) -> Vec<Value> {
    let Ok(recorded) = trace.lock() else {
        return Vec::new();
    };
    recorded
        .iter()
        .filter(|call| call.name == CREATE_SCHEDULE_NAME || call.name == CREATE_WORK_ITEM_NAME)
        .map(|call| {
            json!({
                "kind": call.name,
                "proposal": call.arguments.clone(),
            })
        })
        .collect()
}
```

`json!` sudah tersedia lewat `use serde_json::Value;`? Tidak — tambahkan `use serde_json::{json, Value};` (ganti baris `use serde_json::Value;`).

3. Ganti konstanta `PREAMBLE` menjadi (teks lengkap; tambahan ada di paragraf terakhir):

```rust
pub const PREAMBLE: &str = "You are the workspace AI assistant for Plane. \
Answer factual questions about projects and work items by calling the provided \
tools; never invent project identifiers, work item identifiers, counts, or \
states. All tools are scoped to the user's current workspace and read-only, \
except create_schedule and create_work_item, which only propose something and \
never save anything. If a tool returns no results, say so. Answer concisely in \
the user's language. When the user's message starts with /schedule they want a \
recurring scheduled task. A schedule is a recipe, not a one-line command: \
gather anything unclear first, then call create_schedule once with a complete \
recipe — description, ordered how_to steps, the tools it needs (at least one \
of list_projects, count_work_items, search_work_items), expected_output, and \
how often. Tell the user they can edit every field in the confirmation card. \
The schedule is only created after the user confirms the proposal card, so \
never say it is already created. When the user clearly asks to create a work \
item or task (natural language or a message starting with /task), propose \
exactly one work item per create_work_item call. The project must be named by \
the user: ask when it is missing or ambiguous, and never guess. State names, \
assignee names or emails, and label names may be human-readable; the UI \
resolves them. A work item is only created after the user confirms the \
proposal card, so never say it is already created.";
```

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/api-rs && cargo test -p ai`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): expose pending_actions and document work item proposals"
```

---

### Task 3: Metadata + `pending_actions` di response (crate `api`)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`

- [ ] **Step 1: Tulis test yang gagal**

Di mod `tests` `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`:

1. Ubah dua test lama agar memakai parameter baru `pending_actions`:

```rust
    #[test]
    fn success_body_maps_newlines_and_keeps_tool_calls() {
        let body = success_body(
            "line1\nline2",
            vec![json!({"name": "list_projects"})],
            None,
            vec![],
        );
        assert_eq!(body["response"], json!("line1\nline2"));
        assert_eq!(body["response_html"], json!("line1<br/>line2"));
        assert_eq!(body["tool_calls"][0]["name"], json!("list_projects"));
        assert_eq!(body["pending_action"], json!(null));
        assert_eq!(body["pending_actions"], json!([]));
    }

    #[test]
    fn success_body_carries_pending_action() {
        let action = json!({"kind": "create_schedule", "proposal": {"frequency": "daily"}});
        let body = success_body("done", vec![], Some(action.clone()), vec![action.clone()]);
        assert_eq!(body["pending_action"], action);
        assert_eq!(body["pending_actions"][0], action);
    }
```

2. Tambahkan dua test baru:

```rust
    #[test]
    fn success_body_carries_work_item_pending_actions() {
        let action = json!({"kind": "create_work_item", "proposal": {"name": "Fix pump"}});
        let body = success_body("done", vec![], None, vec![action.clone()]);
        assert_eq!(body["pending_actions"][0], action);
    }

    #[test]
    fn work_item_proposals_metadata_assigns_a_key_per_proposal() {
        let trace = new_trace();
        record(
            &trace,
            ai::tools::CREATE_WORK_ITEM_NAME,
            &json!({"name": "Fix pump"}),
        );
        record(
            &trace,
            ai::tools::CREATE_WORK_ITEM_NAME,
            &json!({"name": "Swap filter"}),
        );
        let proposals = work_item_proposals_metadata(&trace);
        assert_eq!(proposals.len(), 2);
        assert_ne!(proposals[0]["key"], proposals[1]["key"]);
        assert_eq!(proposals[0]["proposal"]["name"], json!("Fix pump"));
        assert_eq!(proposals[1]["proposal"]["name"], json!("Swap filter"));
        assert!(proposals[0]["key"].as_str().is_some());
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --lib routes::ai_agent`
Expected: FAIL — `this function takes 3 arguments but 4 were supplied` / `cannot find function work_item_proposals_metadata`.

- [ ] **Step 3: Implementasi minimal**

Di `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`:

1. Perluas re-export agent:

```rust
pub use ai::agent::{
    new_trace, pending_action, pending_actions, prompt_from_body, run_agent, AGENT_TIMEOUT,
};
```

2. Tambahkan helper sebelum `success_body`:

```rust
/// Metadata entry for every `create_work_item` proposal in this turn: one
/// server-generated key per proposal so the FE can persist a decision per card.
pub fn work_item_proposals_metadata(trace: &ToolTrace) -> Vec<Value> {
    trace
        .lock()
        .map(|recorded| {
            recorded
                .iter()
                .filter(|call| call.name == ai::tools::CREATE_WORK_ITEM_NAME)
                .map(|call| json!({"key": Uuid::new_v4(), "proposal": call.arguments.clone()}))
                .collect()
        })
        .unwrap_or_default()
}
```

3. Ubah `success_body` dan `chat_success_body`:

```rust
pub fn success_body(
    text: &str,
    tool_calls: Vec<Value>,
    action: Option<Value>,
    pending_actions: Vec<Value>,
) -> Value {
    json!({
        "response": text,
        "response_html": crate::routes::ai::response_html(text),
        "tool_calls": tool_calls,
        "pending_action": action,
        "pending_actions": pending_actions,
    })
}

pub(crate) fn chat_success_body(
    text: &str,
    tool_calls: Vec<Value>,
    action: Option<Value>,
    pending_actions: Vec<Value>,
    conversation: &ConversationRow,
    user_message: &MessageRow,
    assistant_message: &MessageRow,
) -> Value {
    let mut body = success_body(text, tool_calls, action, pending_actions);
    body["conversation"] = crate::routes::ai_conversations::conversation_json(conversation);
    body["user_message"] = message_json(user_message);
    body["assistant_message"] = message_json(assistant_message);
    body
}
```

4. Di handler `workspace_ai_agent`, cabang `Ok(text)` — ganti blok metadata + return:

```rust
            let action = pending_action(&trace);
            let actions = pending_actions(&trace);
            let work_items = work_item_proposals_metadata(&trace);
            let mut metadata = json!({ "is_error": false });
            if let Some(action) = action.as_ref() {
                metadata["schedule_proposal"] = action["proposal"].clone();
                metadata["schedule_proposal_key"] = json!(Uuid::new_v4());
                metadata["schedule_decision"] = json!("pending");
            }
            if !work_items.is_empty() {
                metadata["work_item_proposals"] = json!(work_items);
            }
```

dan pada `Ok((StatusCode::OK, Json(chat_success_body(...))))` tambahkan argumen `actions` setelah `action`:

```rust
                Json(chat_success_body(
                    &text,
                    tool_calls,
                    action,
                    actions,
                    &conversation,
                    &user_message,
                    &assistant_message,
                )),
```

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/api-rs && cargo test -p api --lib routes::ai_agent`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs
git commit -m "feat(api): persist and return work item proposals"
```

---

### Task 4: PATCH `work_item_decisions` (crate `api`)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di mod `tests` `apps/api-rs/crates/api/src/routes/ai_conversations.rs`:

```rust
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
    fn metadata_patch_rejects_unknown_and_empty() {
        let unknown = clean_metadata_patch(&patch_map(vec![("nope", json!(1))])).unwrap_err();
        assert!(unknown.contains("not allowed"));
        let empty = clean_metadata_patch(&patch_map(vec![])).unwrap_err();
        assert!(empty.contains("empty"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && cargo test -p api --lib routes::ai_conversations`
Expected: FAIL — `cannot find function clean_metadata_patch`.

- [ ] **Step 3: Implementasi minimal**

Di `apps/api-rs/crates/api/src/routes/ai_conversations.rs`:

1. Tambahkan dua fungsi sebelum `patch_message` (setelah `message_json`):

```rust
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
```

2. Ganti blok validasi di `patch_message` (dari `let mut clean = serde_json::Map::new();` sampai `if clean.is_empty() { ... }`) menjadi:

```rust
    let clean = match clean_metadata_patch(patch) {
        Ok(clean) => clean,
        Err(message) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    };
```

3. Pada UPDATE, ganti `.bind(Value::Object(clean))` menjadi `.bind(clean)`.

4. Perbarui doc comment `patch_message` (opsional, disarankan) menjadi:

```rust
/// `PATCH .../messages/:message_id/` — merge an allowlisted metadata patch
/// (`schedule_decision`, `created_schedule_id`, `work_item_decisions`) written
/// by the FE when the user resolves a proposal card.
```

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/api-rs && cargo test -p api --lib routes::ai_conversations`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): allow work_item_decisions metadata patches"
```

---

### Task 5: Integration test roundtrip (crate `api`)

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tambahkan fake upstream**

Di `apps/api-rs/crates/api/tests/ai_agent_test.rs`, di dalam `mod tool_roundtrip` (setelah `schedule_roundtrip_url`, sekitar baris 308), tambahkan:

```rust
    async fn work_item_handler(
        State(state): State<Shared>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let n = {
            let mut calls = state.calls.lock().unwrap();
            let n = *calls;
            *calls += 1;
            n
        };
        state.bodies.lock().unwrap().push(body);
        if n == 0 {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "1", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "create_work_item",
                                    "arguments": "{\"project\":\"LTS\",\"name\":\"Fix pump\",\"description\":\"Pump is noisy\",\"priority\":\"urgent\",\"state\":\"In Progress\",\"assignees\":[\"Budi\"],\"labels\":[\"maintenance\"],\"target_date\":\"2026-10-05\"}"
                                }
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }]
                })),
            )
        } else {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "2", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "final answer"},
                        "finish_reason": "stop"
                    }]
                })),
            )
        }
    }

    async fn spawn_work_item_roundtrip() -> (String, Shared) {
        let state: Shared = Arc::new(Upstream::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/chat/completions", post(work_item_handler))
            .with_state(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/v1"), state)
    }

    /// URL only — for DB-backed tests that don't need the upstream handle.
    pub(super) async fn work_item_roundtrip_url() -> String {
        spawn_work_item_roundtrip().await.0
    }
```

- [ ] **Step 2: Tulis test DB yang gagal**

Tambahkan di akhir file (setelah `agent_proposal_metadata_is_persisted_and_returned`, sebelum penutup):

```rust
#[tokio::test]
async fn work_item_proposal_metadata_and_decisions_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::work_item_roundtrip_url().await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "/task fix the pump in LTS",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);

    // pending_actions carries the proposal for the current turn.
    let actions = body["pending_actions"].as_array().expect("actions array");
    let action = actions
        .iter()
        .find(|action| action["kind"] == json!("create_work_item"))
        .expect("work item action");
    assert_eq!(action["proposal"]["project"], json!("LTS"));
    assert_eq!(action["proposal"]["name"], json!("Fix pump"));
    assert_eq!(action["proposal"]["priority"], json!("urgent"));
    assert_eq!(action["proposal"]["assignees"][0], json!("Budi"));
    assert_eq!(action["proposal"]["target_date"], json!("2026-10-05"));

    // Metadata on the assistant message carries one keyed proposal.
    let proposals = body["assistant_message"]["metadata"]["work_item_proposals"]
        .as_array()
        .expect("work_item_proposals array");
    assert_eq!(proposals.len(), 1);
    let key = proposals[0]["key"].as_str().expect("proposal key");
    assert_eq!(proposals[0]["proposal"]["name"], json!("Fix pump"));
    let message_id =
        Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    // A created decision roundtrips through the real PATCH endpoint.
    let issue_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"work_item_decisions": {
            (key): {
                "decision": "created",
                "created_work_item_id": issue_id,
                "created_project_id": project_id,
            }
        }}})),
    )
    .await
    .expect("patch created");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["work_item_decisions"][key]["decision"],
        json!("created")
    );
    assert_eq!(
        patched["metadata"]["work_item_decisions"][key]["created_work_item_id"],
        json!(issue_id)
    );

    // A cancelled decision replaces it (the FE sends the whole map).
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"work_item_decisions": {
            (key): {"decision": "cancelled"}
        }}})),
    )
    .await
    .expect("patch cancelled");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["work_item_decisions"][key]["decision"],
        json!("cancelled")
    );
    assert!(patched["metadata"]["work_item_decisions"][key]
        .get("created_work_item_id")
        .is_none());

    // Malformed decisions are rejected.
    let (status, _) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"work_item_decisions": {
            (key): {"decision": "created"}
        }}})),
    )
    .await
    .expect("patch invalid");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The stored row kept the proposals and the last decision.
    let stored: Value = sqlx::query_scalar(
        "SELECT metadata FROM ai_messages WHERE id = $1",
    )
    .bind(message_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["work_item_proposals"][0]["proposal"]["name"], json!("Fix pump"));
    assert_eq!(stored["work_item_decisions"][key]["decision"], json!("cancelled"));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 3: Jalankan test, pastikan gagal**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test work_item_proposal_metadata_and_decisions_roundtrip -- --test-threads=1`
Expected: FAIL pada assert pertama (`pending_actions` belum ada) — jika Task 3 belum dikerjakan; setelah Task 1-4 selesai test harus langsung PASS. Bila DB tidak jalan, test gagal koneksi: pastikan container `plane-for-itsm-plane-db-1` hidup.

- [ ] **Step 4: Jalankan seluruh test file**

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: PASS semua (termasuk test lama schedule roundtrip).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): cover work item proposal and decision roundtrip"
```

---

### Task 6: Lib FE `ai-work-items.ts`

**Files:**

- Create: `apps/web/core/lib/ai-work-items.ts`
- Test: `apps/web/core/lib/ai-work-items.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/web/core/lib/ai-work-items.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  isWorkItemCommand,
  matchAssignees,
  matchLabels,
  matchProject,
  matchState,
  textToDescriptionHtml,
  validateWorkItemProposal,
  workItemHref,
} from "./ai-work-items";

describe("isWorkItemCommand", () => {
  it("matches the /task prefix only", () => {
    expect(isWorkItemCommand("/task fix pump")).toBe(true);
    expect(isWorkItemCommand("  /TASK fix pump")).toBe(true);
    expect(isWorkItemCommand("/taskforce")).toBe(false);
    expect(isWorkItemCommand("buatkan task")).toBe(false);
  });
});

describe("validateWorkItemProposal", () => {
  it("requires a title", () => {
    expect(validateWorkItemProposal({ name: "  " })).toBe("Title is required.");
    expect(validateWorkItemProposal({ name: "Fix pump" })).toBeNull();
  });

  it("bounds lengths and refs", () => {
    expect(validateWorkItemProposal({ name: "x".repeat(256) })).toContain("255");
    expect(validateWorkItemProposal({ name: "ok", priority: "p0" })).toBe("Unknown priority.");
    expect(
      validateWorkItemProposal({ name: "ok", assignees: Array.from({ length: 11 }, (_, i) => `u${i}`) })
    ).toContain("10");
  });

  it("checks dates", () => {
    expect(validateWorkItemProposal({ name: "ok", start_date: "01-10-2026" })).toContain("YYYY-MM-DD");
    expect(validateWorkItemProposal({ name: "ok", start_date: "2026-10-05", target_date: "2026-10-01" })).toContain(
      "after"
    );
    expect(validateWorkItemProposal({ name: "ok", start_date: "2026-10-01", target_date: "2026-10-05" })).toBeNull();
  });
});

describe("textToDescriptionHtml", () => {
  it("escapes and keeps line breaks", () => {
    expect(textToDescriptionHtml("a < b & c\nsecond")).toBe("<p>a &lt; b &amp; c<br/>second</p>");
  });

  it("returns null for empty input", () => {
    expect(textToDescriptionHtml("   ")).toBeNull();
    expect(textToDescriptionHtml(undefined)).toBeNull();
  });
});

describe("matching helpers", () => {
  const projects = [
    { id: "p1", identifier: "LTS", name: "Logistics" },
    { id: "p2", identifier: "OPS", name: "Operations" },
  ];

  it("matches projects by identifier, exact name, then unique substring", () => {
    expect(matchProject(projects, "lts")?.id).toBe("p1");
    expect(matchProject(projects, "logistics")?.id).toBe("p1");
    expect(matchProject(projects, "oper")?.id).toBe("p2");
    expect(matchProject(projects, "o")).toBeUndefined();
    expect(matchProject(projects, "nope")).toBeUndefined();
  });

  it("matches members by display name or email and reports misses", () => {
    const members = [
      { id: "u1", display_name: "Budi", email: "budi@example.com" },
      { id: "u2", display_name: "Sari", email: "sari@example.com" },
    ];
    expect(matchAssignees(members, ["budi", "sari@example.com", "budi", "Ghost"])).toEqual({
      matched: ["u1", "u2"],
      missing: ["Ghost"],
    });
  });

  it("matches labels by name and reports misses", () => {
    const labels = [
      { id: "l1", name: "Maintenance" },
      { id: "l2", name: "Safety" },
    ];
    expect(matchLabels(labels, ["maintenance", "Nope"])).toEqual({ matched: ["l1"], missing: ["Nope"] });
  });

  it("matches states by name", () => {
    const states = [
      { id: "s1", name: "In Progress" },
      { id: "s2", name: "Done" },
    ];
    expect(matchState(states, "in progress")?.id).toBe("s1");
    expect(matchState(states, "")).toBeUndefined();
    expect(matchState(states, "Review")).toBeUndefined();
  });
});

describe("workItemHref", () => {
  it("builds the project issue route", () => {
    expect(workItemHref("acme", "p1", "i9")).toBe("/acme/projects/p1/issues/i9");
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/web && pnpm vitest run core/lib/ai-work-items.test.ts`
Expected: FAIL — module `./ai-work-items` tidak ditemukan.

- [ ] **Step 3: Implementasi minimal**

Buat `apps/web/core/lib/ai-work-items.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const WORK_ITEM_PRIORITIES = ["urgent", "high", "medium", "low", "none"] as const;
export type TWorkItemPriority = (typeof WORK_ITEM_PRIORITIES)[number];

export type TAiWorkItemProposal = {
  project: string;
  name: string;
  description?: string | null;
  priority?: string | null;
  state?: string | null;
  assignees?: string[] | null;
  labels?: string[] | null;
  start_date?: string | null;
  target_date?: string | null;
};

export type TAiWorkItemProposalEntry = {
  key: string;
  proposal: TAiWorkItemProposal;
};

export type TAiWorkItemDecision = {
  decision: "created" | "cancelled";
  created_work_item_id?: string;
  created_project_id?: string;
};

export const WORK_ITEM_LIMITS = {
  name: 255,
  description: 5000,
  refs: 10,
} as const;

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

/** True when the message starts with the `/task` slash command (case-insensitive). */
export const isWorkItemCommand = (text: string): boolean => /^\/task(?:\s|$)/i.test(text.trimStart());

/** Mirrors `work_item_proposal_from_args` on the backend; returns the first error. */
export const validateWorkItemProposal = (proposal: Partial<TAiWorkItemProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Title is required.";
  if (name.length > WORK_ITEM_LIMITS.name) return `Title must be at most ${WORK_ITEM_LIMITS.name} characters.`;
  const description = proposal.description?.trim() ?? "";
  if (description.length > WORK_ITEM_LIMITS.description)
    return `Description must be at most ${WORK_ITEM_LIMITS.description} characters.`;
  if (proposal.priority && !WORK_ITEM_PRIORITIES.includes(proposal.priority as TWorkItemPriority))
    return "Unknown priority.";
  if ((proposal.assignees?.length ?? 0) > WORK_ITEM_LIMITS.refs)
    return `At most ${WORK_ITEM_LIMITS.refs} assignees are allowed.`;
  if ((proposal.labels?.length ?? 0) > WORK_ITEM_LIMITS.refs)
    return `At most ${WORK_ITEM_LIMITS.refs} labels are allowed.`;
  const start = proposal.start_date?.trim() ?? "";
  const target = proposal.target_date?.trim() ?? "";
  if (start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (target && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  return null;
};

/** Plain text → safe HTML: escape, newlines to <br/>, wrapped in a paragraph. */
export const textToDescriptionHtml = (text: string | null | undefined): string | null => {
  const trimmed = text?.trim() ?? "";
  if (!trimmed) return null;
  const escaped = trimmed.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  return `<p>${escaped.split("\n").join("<br/>")}</p>`;
};

export const workItemHref = (workspaceSlug: string, projectId: string, issueId: string): string =>
  `/${workspaceSlug}/projects/${projectId}/issues/${issueId}`;

type TProjectRef = { id: string; identifier: string; name: string };

export const matchProject = (projects: TProjectRef[], reference: string): TProjectRef | undefined => {
  const needle = reference.trim().toLowerCase();
  if (!needle) return undefined;
  const byIdentifier = projects.find((project) => project.identifier.toLowerCase() === needle);
  if (byIdentifier) return byIdentifier;
  const byName = projects.find((project) => project.name.toLowerCase() === needle);
  if (byName) return byName;
  const partial = projects.filter((project) => project.name.toLowerCase().includes(needle));
  return partial.length === 1 ? partial[0] : undefined;
};

type TMemberRef = { id: string; display_name?: string | null; email?: string | null };

export const matchAssignees = (
  members: TMemberRef[],
  references: string[]
): { matched: string[]; missing: string[] } => {
  const matched: string[] = [];
  const missing: string[] = [];
  references.forEach((reference) => {
    const needle = reference.trim().toLowerCase();
    if (!needle) return;
    const member = members.find(
      (candidate) => candidate.display_name?.toLowerCase() === needle || candidate.email?.toLowerCase() === needle
    );
    if (member) {
      if (!matched.includes(member.id)) matched.push(member.id);
    } else {
      missing.push(reference);
    }
  });
  return { matched, missing };
};

type TLabelRef = { id: string; name: string };

export const matchLabels = (labels: TLabelRef[], references: string[]): { matched: string[]; missing: string[] } => {
  const matched: string[] = [];
  const missing: string[] = [];
  references.forEach((reference) => {
    const needle = reference.trim().toLowerCase();
    if (!needle) return;
    const label = labels.find((candidate) => candidate.name.toLowerCase() === needle);
    if (label) {
      if (!matched.includes(label.id)) matched.push(label.id);
    } else {
      missing.push(reference);
    }
  });
  return { matched, missing };
};

type TStateRef = { id: string; name: string };

export const matchState = (states: TStateRef[], reference: string | null | undefined): TStateRef | undefined => {
  const needle = reference?.trim().toLowerCase();
  if (!needle) return undefined;
  return states.find((state) => state.name.toLowerCase() === needle);
};
```

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/web && pnpm vitest run core/lib/ai-work-items.test.ts`
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-work-items.ts apps/web/core/lib/ai-work-items.test.ts
git commit -m "feat(web): add work item proposal helpers"
```

---

### Task 7: Mapping tipe chat (`ai-context.ts`, `ai-conversations.ts`)

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts`
- Modify: `apps/web/core/lib/ai-conversations.ts`
- Test: `apps/web/core/lib/ai-conversations.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan dua `it` di dalam `describe("toAiMessage")` yang sudah ada di `apps/web/core/lib/ai-conversations.test.ts` (file sudah punya helper `stored()` dan import `toAiMessage`):

```ts
it("maps work item proposals and decisions from metadata", () => {
  const message = toAiMessage(
    stored({
      metadata: {
        work_item_proposals: [{ key: "k1", proposal: { project: "LTS", name: "Fix pump" } }],
        work_item_decisions: {
          k1: { decision: "created", created_work_item_id: "i1", created_project_id: "p1" },
        },
      },
    })
  );
  expect(message.workItemProposals).toHaveLength(1);
  expect(message.workItemProposals?.[0].key).toBe("k1");
  expect(message.workItemProposals?.[0].proposal.name).toBe("Fix pump");
  expect(message.workItemDecisions?.k1.decision).toBe("created");
  expect(message.workItemDecisions?.k1.created_work_item_id).toBe("i1");
  expect(message.workItemDecisions?.k1.created_project_id).toBe("p1");
});

it("leaves work item fields undefined when metadata is absent", () => {
  const message = toAiMessage(stored({}));
  expect(message.workItemProposals).toBeUndefined();
  expect(message.workItemDecisions).toBeUndefined();
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/web && pnpm vitest run core/lib/ai-conversations.test.ts`
Expected: FAIL — properti `workItemProposals` undefined.

- [ ] **Step 3: Implementasi minimal**

Di `apps/web/core/lib/ai-context.ts`:

1. Tambahkan import:

```ts
import type { TAiWorkItemDecision, TAiWorkItemProposalEntry } from "@/lib/ai-work-items";
```

2. Tambahkan field di `TAiMessage`:

```ts
export type TAiMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  isError?: boolean;
  scheduleProposal?: TAiScheduleProposal;
  scheduleProposalKey?: string;
  scheduleDecision?: "pending" | "created" | "cancelled";
  createdScheduleId?: string;
  workItemProposals?: TAiWorkItemProposalEntry[];
  workItemDecisions?: Record<string, TAiWorkItemDecision>;
  createdAt?: string;
};
```

Di `apps/web/core/lib/ai-conversations.ts`:

1. Tambahkan import:

```ts
import type { TAiWorkItemDecision, TAiWorkItemProposalEntry } from "@/lib/ai-work-items";
```

2. Tambahkan field metadata:

```ts
export type TAiMessageMetadata = {
  schedule_proposal?: TAiScheduleProposal;
  schedule_proposal_key?: string;
  schedule_decision?: "pending" | "created" | "cancelled";
  created_schedule_id?: string;
  work_item_proposals?: TAiWorkItemProposalEntry[];
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
  is_error?: boolean;
};
```

3. Perluas patch type:

```ts
/** Keys the `PATCH .../messages/:id/` endpoint accepts (server allowlist). */
export type TAiMessageMetadataPatch = Pick<TAiMessageMetadata, "schedule_decision" | "created_schedule_id"> & {
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
};
```

4. Tambahkan mapping di `toAiMessage`:

```ts
export const toAiMessage = (stored: TAiStoredMessage): TAiMessage => ({
  id: stored.id,
  role: stored.role,
  content: stored.role === "assistant" ? (stored.content_html ?? stored.content) : stored.content,
  isError: stored.metadata?.is_error === true,
  scheduleProposal: stored.metadata?.schedule_proposal,
  scheduleProposalKey: stored.metadata?.schedule_proposal_key,
  scheduleDecision: stored.metadata?.schedule_decision,
  createdScheduleId: stored.metadata?.created_schedule_id,
  workItemProposals: stored.metadata?.work_item_proposals,
  workItemDecisions: stored.metadata?.work_item_decisions,
  createdAt: stored.created_at,
});
```

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/web && pnpm vitest run core/lib/ai-conversations.test.ts core/lib/ai-work-items.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-conversations.ts apps/web/core/lib/ai-conversations.test.ts
git commit -m "feat(web): map work item proposals on chat messages"
```

---

### Task 8: Store confirm/cancel (FE)

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `apps/web/core/store/ai-assistant.store.test.ts`:

1. Tambahkan service mock `issues` di `makeServices` (setelah `conversations`):

```ts
  issues: {
    createIssue: vi.fn(async (_slug: string, projectId: string) => ({ id: "i1", project_id: projectId })),
    ...overrides.issues,
  },
```

2. Ubah `makeStore`:

```ts
const makeStore = (services = makeServices()) =>
  new AIAssistantStore(
    services.ai as any,
    services.schedules as any,
    services.conversations as any,
    services.issues as any
  );
```

3. Tambahkan test baru di akhir file:

```ts
describe("work item proposals", () => {
  const messageWithProposal = () => ({
    id: "srv-assistant",
    role: "assistant" as const,
    content: "Here is a proposal",
    content_html: "<p>Here is a proposal</p>",
    metadata: {
      work_item_proposals: [{ key: "k1", proposal: { project: "LTS", name: "Fix pump" } }],
    },
    created_at: "2026-09-28T09:00:00Z",
  });

  it("confirms a proposal, creates the issue and persists the decision", async () => {
    const services = makeServices();
    services.conversations.listMessages = vi.fn(async () => [messageWithProposal()]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.confirmWorkItemProposal("srv-assistant", "k1", {
      projectId: "p1",
      issue: { name: "Fix pump" },
    });

    expect(services.issues.createIssue).toHaveBeenCalledWith("acme", "p1", { name: "Fix pump" });
    expect(store.messages[0].workItemDecisions?.k1).toEqual({
      decision: "created",
      created_work_item_id: "i1",
      created_project_id: "p1",
    });
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith(
      "acme",
      expect.any(String),
      "srv-assistant",
      {
        work_item_decisions: {
          k1: { decision: "created", created_work_item_id: "i1", created_project_id: "p1" },
        },
      }
    );
  });

  it("does not create twice for the same proposal", async () => {
    const services = makeServices();
    services.conversations.listMessages = vi.fn(async () => [messageWithProposal()]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.confirmWorkItemProposal("srv-assistant", "k1", { projectId: "p1", issue: { name: "Fix pump" } });
    await store.confirmWorkItemProposal("srv-assistant", "k1", { projectId: "p1", issue: { name: "Fix pump" } });
    expect(services.issues.createIssue).toHaveBeenCalledTimes(1);
  });

  it("keeps the decision pending when creation fails", async () => {
    const services = makeServices();
    services.conversations.listMessages = vi.fn(async () => [messageWithProposal()]);
    services.issues.createIssue = vi.fn(async () => {
      throw new Error("nope");
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await expect(
      store.confirmWorkItemProposal("srv-assistant", "k1", { projectId: "p1", issue: { name: "Fix pump" } })
    ).rejects.toThrow("nope");
    expect(store.messages[0].workItemDecisions?.k1).toBeUndefined();
    expect(services.conversations.updateMessageMetadata).not.toHaveBeenCalled();
  });

  it("cancels a proposal and persists the decision", async () => {
    const services = makeServices();
    services.conversations.listMessages = vi.fn(async () => [messageWithProposal()]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    store.resolveWorkItemProposal("srv-assistant", "k1");
    expect(store.messages[0].workItemDecisions?.k1).toEqual({ decision: "cancelled" });
    await flush();
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith(
      "acme",
      expect.any(String),
      "srv-assistant",
      {
        work_item_decisions: { k1: { decision: "cancelled" } },
      }
    );
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts`
Expected: FAIL — `store.confirmWorkItemProposal is not a function`.

- [ ] **Step 3: Implementasi minimal**

Di `apps/web/core/store/ai-assistant.store.ts`:

1. Tambahkan import:

```ts
import { IssueService } from "@/services/issue/issue.service";
import type { TIssue } from "@plane/types";
import type { TAiWorkItemDecision } from "@/lib/ai-work-items";
```

2. Tambahkan tipe service:

```ts
type TIssueService = Pick<IssueService, "createIssue">;
```

3. Tambahkan method ke `IAIAssistantStore`:

```ts
  confirmWorkItemProposal: (
    messageId: string,
    key: string,
    payload: { projectId: string; issue: Partial<TIssue> }
  ) => Promise<void>;
  resolveWorkItemProposal: (messageId: string, key: string) => void;
```

4. Tambahkan parameter constructor ke-4:

```ts
  constructor(
    private aiService: TAiService = new AIService(),
    private schedulesService: TAiSchedulesService = new AiSchedulesService(),
    private conversationsService: TAiConversationsService = new AiConversationsService(),
    private issuesService: TIssueService = new IssueService()
  ) {
```

5. Daftarkan action di `makeObservable` (setelah `resolveScheduleProposal`):

```ts
      confirmWorkItemProposal: action,
      resolveWorkItemProposal: action,
```

6. Tambahkan method setelah `resolveScheduleProposal`:

```ts
  confirmWorkItemProposal = async (
    messageId: string,
    key: string,
    payload: { projectId: string; issue: Partial<TIssue> }
  ) => {
    const slug = this.workspaceSlug;
    const conversationId = this.activeConversationId;
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!slug || !message?.workItemProposals?.some((entry) => entry.key === key)) return;
    if (message.workItemDecisions?.[key]) return;
    const created = await this.issuesService.createIssue(slug, payload.projectId, payload.issue);
    if (!created?.id) throw new Error("Work item creation returned no id");
    runInAction(() => {
      message.workItemDecisions = {
        ...message.workItemDecisions,
        [key]: {
          decision: "created",
          created_work_item_id: created.id,
          created_project_id: payload.projectId,
        },
      };
    });
    await this.persistWorkItemDecisions(conversationId, message);
  };

  resolveWorkItemProposal = (messageId: string, key: string) => {
    const conversationId = this.activeConversationId;
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!message?.workItemProposals?.some((entry) => entry.key === key)) return;
    if (message.workItemDecisions?.[key]) return;
    runInAction(() => {
      message.workItemDecisions = {
        ...message.workItemDecisions,
        [key]: { decision: "cancelled" } satisfies TAiWorkItemDecision,
      };
    });
    void this.persistWorkItemDecisions(conversationId, message);
  };

  private persistWorkItemDecisions = async (conversationId: string | undefined, message: TAiMessage) => {
    const slug = this.workspaceSlug;
    if (!slug || !conversationId || !message.workItemDecisions) return;
    try {
      await this.conversationsService.updateMessageMetadata(slug, conversationId, message.id, {
        work_item_decisions: message.workItemDecisions,
      });
    } catch {
      // best-effort: the created work item is already the source of truth
    }
  };
```

Catatan: `satisfies` didukung TS target repo; jika error, ganti dengan anotasi eksplisit `const cancelled: TAiWorkItemDecision = { decision: "cancelled" };`.

- [ ] **Step 4: Jalankan test, pastikan pass**

Run: `cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts`
Expected: PASS semua (test lama + 4 test baru).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): confirm work item proposals from the store"
```

---

### Task 9: Card `WorkItemProposalCard`

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/work-item-proposal-card.tsx`

- [ ] **Step 1: Buat komponen**

Buat `apps/web/core/components/ai/assistant-sidebar/work-item-proposal-card.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import type { TIssue } from "@plane/types";
// hooks
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import {
  matchAssignees,
  matchLabels,
  matchProject,
  matchState,
  textToDescriptionHtml,
  validateWorkItemProposal,
  workItemHref,
  WORK_ITEM_PRIORITIES,
  type TAiWorkItemDecision,
  type TAiWorkItemProposal,
} from "@/lib/ai-work-items";

type Props = {
  proposal: TAiWorkItemProposal;
  decision?: TAiWorkItemDecision;
  onConfirm: (payload: { projectId: string; issue: Partial<TIssue> }) => Promise<void>;
  onCancel: () => void;
};

export const WorkItemProposalCard = observer(function WorkItemProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  const { getProjectStates, fetchProjectStates } = useProjectState();
  const { project: projectMemberStore } = useMember();
  const { getProjectLabels, fetchProjectLabels } = useLabel();
  // component state
  const [draft, setDraft] = useState<TAiWorkItemProposal>(proposal);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [stateId, setStateId] = useState("");
  const [assigneeIds, setAssigneeIds] = useState<string[]>([]);
  const [labelIds, setLabelIds] = useState<string[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = useMemo(
    () =>
      (workspaceProjectIds ?? [])
        .map((id) => getProjectById(id))
        .filter((project): project is NonNullable<typeof project> => Boolean(project))
        .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name })),
    [workspaceProjectIds, getProjectById]
  );

  const resolvedProjectId = useMemo(() => matchProject(projects, draft.project)?.id ?? null, [projects, draft.project]);

  useEffect(() => {
    setProjectId(resolvedProjectId);
  }, [resolvedProjectId]);

  // Fetch project data that is not loaded yet. Fetch-only: no local state
  // writes, so it cannot loop.
  useEffect(() => {
    if (!projectId || !workspaceSlug) return;
    if (!getProjectStates(projectId)) void fetchProjectStates(workspaceSlug, projectId);
    if (!projectMemberStore.getProjectMemberFetchStatus(projectId))
      void projectMemberStore.fetchProjectMembers(workspaceSlug, projectId);
    if (!getProjectLabels(projectId)) void fetchProjectLabels(workspaceSlug, projectId);
  }, [
    projectId,
    workspaceSlug,
    getProjectStates,
    fetchProjectStates,
    projectMemberStore,
    getProjectLabels,
    fetchProjectLabels,
  ]);

  const states = projectId ? getProjectStates(projectId) : undefined;
  const labels = projectId ? getProjectLabels(projectId) : undefined;
  const memberIds = projectId ? (projectMemberStore.getProjectMemberIds(projectId, true) ?? []) : [];
  const members = memberIds
    .map((id) => ({ id, details: projectMemberStore.getProjectMemberDetails(id, projectId ?? "") }))
    .filter((entry) => Boolean(entry.details))
    .map((entry) => ({
      id: entry.id,
      display_name: entry.details?.member.display_name,
      email: entry.details?.member.email,
    }));

  // Resolve the agent's human-readable values against the loaded project data.
  // Runs during render (React's adjust-state-during-render pattern): re-runs
  // when the project or proposal changes, or when a fetch lands (lengths in
  // the key change). User edits to the resolved ids never touch the key, so
  // they are never clobbered.
  const resolutionKey = [
    projectId ?? "",
    states?.length ?? -1,
    labels?.length ?? -1,
    members.length,
    draft.state ?? "",
    (draft.assignees ?? []).join("|"),
    (draft.labels ?? []).join("|"),
  ].join("::");
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    if (!projectId) {
      setStateId("");
      setAssigneeIds([]);
      setLabelIds([]);
      setMissing([]);
    } else {
      setStateId(matchState(states ?? [], draft.state)?.id ?? "");
      const assigneeResult = matchAssignees(members, draft.assignees ?? []);
      setAssigneeIds(assigneeResult.matched);
      const labelResult = matchLabels(labels ?? [], draft.labels ?? []);
      setLabelIds(labelResult.matched);
      setMissing([
        ...assigneeResult.missing.map((name) => `Assignee not found: ${name}`),
        ...labelResult.missing.map((name) => `Label not found: ${name}`),
      ]);
    }
  }

  if (decision?.decision === "created" && decision.created_work_item_id && decision.created_project_id) {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Work item created.{" "}
        {workspaceSlug && (
          <Link
            href={workItemHref(workspaceSlug, decision.created_project_id, decision.created_work_item_id)}
            className="text-accent-primary hover:underline"
          >
            Open work item
          </Link>
        )}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Work item cancelled.
      </p>
    );
  }

  const patch = (fields: Partial<TAiWorkItemProposal>) => setDraft((current) => ({ ...current, ...fields }));
  const validationError = projectId ? validateWorkItemProposal(draft) : "Project is required.";

  const toggleAssignee = (id: string, checked: boolean) =>
    setAssigneeIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));
  const toggleLabel = (id: string, checked: boolean) =>
    setLabelIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));

  const confirm = async () => {
    if (!projectId) return;
    setSubmitting(true);
    setError(null);
    try {
      const issue: Partial<TIssue> = { name: draft.name.trim() };
      const descriptionHtml = textToDescriptionHtml(draft.description);
      if (descriptionHtml) issue.description_html = descriptionHtml;
      if (draft.priority) issue.priority = draft.priority as TIssue["priority"];
      if (stateId) issue.state_id = stateId;
      if (assigneeIds.length > 0) issue.assignee_ids = assigneeIds;
      if (labelIds.length > 0) issue.label_ids = labelIds;
      if (draft.start_date) issue.start_date = draft.start_date;
      if (draft.target_date) issue.target_date = draft.target_date;
      await onConfirm({ projectId, issue });
    } catch {
      setError("Could not create the work item. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Work item proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <label className="text-12 text-secondary">
        Project
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={projectId ?? ""}
          onChange={(event) => setProjectId(event.target.value)}
        >
          <option value="">Select a project</option>
          {projects.map((project) => (
            <option key={project.id} value={project.id}>
              {project.identifier} — {project.name}
            </option>
          ))}
        </select>
      </label>

      <label className="mt-2 block text-12 text-secondary">
        Title
        <input
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={draft.name}
          onChange={(event) => patch({ name: event.target.value })}
        />
      </label>

      <label className="mt-2 block text-12 text-secondary">
        Description
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={3}
          value={draft.description ?? ""}
          onChange={(event) => patch({ description: event.target.value })}
        />
      </label>

      <label className="mt-2 block text-12 text-secondary">
        Priority
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={draft.priority ?? ""}
          onChange={(event) => patch({ priority: event.target.value || null })}
        >
          <option value="">No priority</option>
          {WORK_ITEM_PRIORITIES.map((priority) => (
            <option key={priority} value={priority}>
              {priority}
            </option>
          ))}
        </select>
      </label>

      <label className="mt-2 block text-12 text-secondary">
        State
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={stateId}
          onChange={(event) => setStateId(event.target.value)}
        >
          <option value="">Project default</option>
          {(states ?? []).map((state) => (
            <option key={state.id} value={state.id}>
              {state.name}
            </option>
          ))}
        </select>
      </label>

      {members.length > 0 && (
        <div className="mt-2 text-12 text-secondary">
          Assignees
          <div className="mt-1 flex max-h-24 flex-col gap-1 overflow-y-auto">
            {members.map((member) => (
              <label key={member.id} className="flex items-center gap-1 text-12 text-primary">
                <input
                  type="checkbox"
                  checked={assigneeIds.includes(member.id)}
                  onChange={(event) => toggleAssignee(member.id, event.target.checked)}
                />
                {member.display_name || member.email || member.id}
              </label>
            ))}
          </div>
        </div>
      )}

      {(labels ?? []).length > 0 && (
        <div className="mt-2 text-12 text-secondary">
          Labels
          <div className="mt-1 flex flex-wrap gap-2">
            {(labels ?? []).map((label) => (
              <label key={label.id} className="flex items-center gap-1 text-12 text-primary">
                <input
                  type="checkbox"
                  checked={labelIds.includes(label.id)}
                  onChange={(event) => toggleLabel(label.id, event.target.checked)}
                />
                {label.name}
              </label>
            ))}
          </div>
        </div>
      )}

      <div className="mt-2 flex gap-2">
        <label className="flex-1 text-12 text-secondary">
          Start date
          <input
            type="date"
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.start_date ?? ""}
            onChange={(event) => patch({ start_date: event.target.value || null })}
          />
        </label>
        <label className="flex-1 text-12 text-secondary">
          Target date
          <input
            type="date"
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.target_date ?? ""}
            onChange={(event) => patch({ target_date: event.target.value || null })}
          />
        </label>
      </div>

      {missing.map((notice) => (
        <p key={notice} className="mt-1 text-12 text-tertiary">
          {notice} — pick one manually.
        </p>
      ))}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={Boolean(validationError)}
          onClick={() => void confirm()}
        >
          Confirm
        </Button>
        <Button size="sm" variant="secondary" disabled={submitting} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Typecheck + lint**

Run: `cd apps/web && pnpm check:types`
Expected: PASS. Jika muncul error tipe store (`getProjectMemberDetails`, `getProjectLabels`, `getProjectStates`), sesuaikan nama method dengan signature store yang ada di `apps/web/core/store/` (jangan ubah store).

Run: `cd apps/web && pnpm exec oxlint core/components/ai/assistant-sidebar/work-item-proposal-card.tsx`
Expected: `Found 0 warnings and 0 errors.` Perbaiki bila ada warning (mis. import tidak terpakai).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/work-item-proposal-card.tsx
git commit -m "feat(web): add work item proposal card"
```

---

### Task 10: Wiring di sidebar + hint `/task`

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Implementasi**

Di `apps/web/core/components/ai/assistant-sidebar/root.tsx`:

1. Tambahkan import komponen:

```ts
import { WorkItemProposalCard } from "./work-item-proposal-card";
```

2. Ambil dua method store baru dari `useAiAssistant()` (di destructuring yang sama dengan `confirmScheduleProposal`/`resolveScheduleProposal`):

```ts
    confirmWorkItemProposal,
    resolveWorkItemProposal,
```

3. Ganti hint slash agar menampilkan `/schedule` dan `/task`. Ganti baris:

```ts
const showScheduleHint = trimmedQuestion.startsWith("/") && !trimmedQuestion.startsWith("/schedule");
```

menjadi:

```ts
const showSlashHint =
  trimmedQuestion.startsWith("/") && !trimmedQuestion.startsWith("/schedule") && !trimmedQuestion.startsWith("/task");
```

lalu ganti blok JSX hint:

```tsx
{
  showScheduleHint && !isGenerating && (
    <button
      type="button"
      onClick={() => {
        setQuestion("/schedule ");
        composerRef.current?.focus();
      }}
      className="w-full border-b border-subtle px-3.5 py-2 text-left text-12 text-secondary transition-colors hover:text-primary"
    >
      /schedule — <span className="text-tertiary">Schedule a recurring AI report</span>
    </button>
  );
}
```

menjadi:

```tsx
{
  showSlashHint && !isGenerating && (
    <>
      <button
        type="button"
        onClick={() => {
          setQuestion("/schedule ");
          composerRef.current?.focus();
        }}
        className="w-full border-b border-subtle px-3.5 py-2 text-left text-12 text-secondary transition-colors hover:text-primary"
      >
        /schedule — <span className="text-tertiary">Schedule a recurring AI report</span>
      </button>
      <button
        type="button"
        onClick={() => {
          setQuestion("/task ");
          composerRef.current?.focus();
        }}
        className="w-full border-b border-subtle px-3.5 py-2 text-left text-12 text-secondary transition-colors hover:text-primary"
      >
        /task — <span className="text-tertiary">Create a work item from chat</span>
      </button>
    </>
  );
}
```

4. Render card work item tepat setelah blok `{message.scheduleProposal && (...)}`:

```tsx
{
  message.workItemProposals?.map((entry) => (
    <WorkItemProposalCard
      key={entry.key}
      proposal={entry.proposal}
      decision={message.workItemDecisions?.[entry.key]}
      onConfirm={(payload) => confirmWorkItemProposal(message.id, entry.key, payload)}
      onCancel={() => resolveWorkItemProposal(message.id, entry.key)}
    />
  ));
}
```

- [ ] **Step 2: Typecheck + lint + test**

Run: `cd apps/web && pnpm check:types`
Expected: PASS.

Run: `cd apps/web && pnpm exec oxlint core/components/ai/assistant-sidebar/root.tsx core/components/ai/assistant-sidebar/work-item-proposal-card.tsx`
Expected: `Found 0 warnings and 0 errors.`

Run: `cd apps/web && pnpm vitest run core/store/ai-assistant.store.test.ts core/lib/ai-work-items.test.ts core/lib/ai-conversations.test.ts`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): render work item proposal cards in chat"
```

---

### Task 11: Verifikasi penuh + build + smoke

**Files:** tidak ada perubahan kode (kecuali perbaikan bila verifikasi menemukan masalah).

- [ ] **Step 1: Test Rust lengkap**

Run: `cd apps/api-rs && cargo test -p ai`
Expected: PASS semua.

Run: `cd apps/api-rs && cargo test -p api --lib`
Expected: PASS semua unit test.

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: PASS semua.

Run: `cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p worker --test ai_schedule_test -- --test-threads=1`
Expected: PASS (regression: worker read-only allowlist tidak berubah).

- [ ] **Step 2: Test + check web lengkap**

Run: `cd apps/web && pnpm test`
Expected: 184 test lama + test baru PASS.

Run: `cd apps/web && pnpm check:types`
Expected: exit 0.

Run: `cd apps/web && pnpm check:lint`
Expected: 0 errors (warning pre-existing boleh).

- [ ] **Step 3: Build + restart API**

```bash
cd /home/ghifari/plane-for-itsm
setsid docker compose -f docker-compose-local.yml up -d --build api > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll (build Rust LTO bisa 10+ menit tanpa output — bukan hang):

```bash
tail -5 /tmp/plane-api-build.log
curl -s -o /dev/null -w "api: %{http_code}\n" http://localhost:8000/health
```

Expected: `api: 200`. Lalu restart live (memegang koneksi Redis yang ikut ter-recreate):

```bash
systemctl --user restart plane-live.service && sleep 8 && curl -s -o /dev/null -w "live: %{http_code}\n" http://localhost:3100/live/health/
```

Expected: `live: 200`.

- [ ] **Step 4: Build + restart web prod**

```bash
cd /home/ghifari/plane-for-itsm && pnpm --filter=web build && systemctl --user restart plane-web-prod.service
sleep 5 && curl -s -o /dev/null -w "web: %{http_code}\n" http://localhost:3000/
```

Expected: `web: 200`.

- [ ] **Step 5: Smoke manual**

1. Buka chat agent (Galileo), ketik `/` → hint `/schedule` dan `/task` muncul; klik `/task`.
2. Kirim: `/task buatkan task ganti filter pompa di project <IDENTIFIER> priority urgent`.
   - Expected: card proposal muncul dengan project terisi, title, priority urgent; tidak ada klaim "sudah dibuat".
3. Edit satu field (mis. description), klik Confirm.
   - Expected: "Work item created. Open work item" muncul; link membuka work item di project yang benar dengan field sesuai.
4. Reload halaman → card tetap menampilkan status created + link.
5. Chat natural: `buatkan task cek pompa bocor di project <IDENTIFIER>` → card muncul (tanpa `/task`).
6. Klik Cancel di satu proposal → status "Work item cancelled."; reload → tetap cancelled.
7. Klik Confirm dua kali cepat pada satu card → hanya satu work item terbuat.
8. Scheduled run sanity: buka Scheduler, Run now satu jadwal → run tetap sukses tanpa membuat work item apa pun.

- [ ] **Step 6: Commit perbaikan (bila ada) + push**

Jika verifikasi memerlukan perbaikan:

```bash
git add <file-yang-diubah>
git commit -m "fix(ai-agent): <deskripsi>"
```

Setelah semua hijau, push branch `preview` (hanya bila user meminta):

```bash
git push origin preview
```

---

## Self-review checklist (dijalankan saat menulis plan)

- **Spec coverage:** tool + validasi (Task 1), `pending_actions` + PREAMBLE (Task 2), metadata + response (Task 3), PATCH allowlist (Task 4), integrasi (Task 5), lib + validator + resolver (Task 6), mapping tipe (Task 7), store confirm/cancel (Task 8), card editable + resolusi + link (Task 9), render + hint `/task` (Task 10), verifikasi + smoke (Task 11). Out-of-scope spec tidak diimplementasikan (tidak ada task).
- **Placeholder scan:** tidak ada TBD/TODO; semua step berisi kode/perintah lengkap.
- **Type consistency:** `TAiWorkItemProposal`/`TAiWorkItemProposalEntry`/`TAiWorkItemDecision` dipakai konsisten di Task 6-10; `work_item_proposals`/`work_item_decisions` konsisten di Task 3-8; `confirmWorkItemProposal(messageId, key, payload)` konsisten di Task 8-10; `pending_actions` konsisten di Task 2-3-5.
