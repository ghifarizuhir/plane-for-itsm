# Galileo Tools Fase 2A — Proposal Foundation + Update Work Item + Add Comment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun fondasi proposal generik (`proposals`/`proposal_decisions`) dan dua proposal tool pertama Fase 2 — `update_work_item` dan `add_comment` — end-to-end: validasi backend, metadata API, endpoint decision, tipe FE, store dispatch, dan dua kartu konfirmasi.

**Architecture:** Tool proposal hidup di `tools/mutations.rs` baru; keduanya hanya memvalidasi bentuk + mencatat trace (tidak menyentuh DB). Endpoint agent menulis metadata generik `proposals: [{key, kind, proposal}]`; FE menyimpan decision lewat `PATCH .../messages/:id/` dengan key `proposal_decisions`. Confirm memanggil endpoint REST existing dari FE. Bentuk lama (`schedule_*`, `work_item_*`) tidak berubah.

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), React + MobX + vitest. Perintah Rust dari `apps/api-rs`, perintah FE dari `apps/web`.

**Prasyarat:** Fase 1 (Plan 1A–1D) sudah dieksekusi.

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4.4, §4.7, §6.1 update/comment, §6.2, §6.3, §6.4, §6.5).

**Catatan pembagian:** Fase 2 dipecah dua plan yang masing-masing bisa diuji sendiri. Plan 2A (dokumen ini) menutup `update_work_item` + `add_comment` dan seluruh fondasi generik. Plan 2B menambah `manage_service_links`, `manage_sprint_items`, `manage_track_items` + `LinkItemsProposalCard` di atas fondasi yang sama.

---

## File Structure

| File                                                                                         | Aksi | Tanggung jawab                                                           |
| -------------------------------------------------------------------------------------------- | ---- | ------------------------------------------------------------------------ |
| `apps/api-rs/crates/ai/src/tools/mutations.rs`                                               | Baru | `update_work_item`, `add_comment`, normalisasi, `mutation_kind_for_tool` |
| `apps/api-rs/crates/ai/src/tools/proposals.rs`                                               | Ubah | Buka `bounded_ref_list`/`parse_iso_date` + tambah `bounded_ref_list_opt` |
| `apps/api-rs/crates/ai/src/tools/mod.rs`                                                     | Ubah | Module decl, `PROPOSAL_TOOL_NAMES`, registrasi `workspace_tools`, test   |
| `apps/api-rs/crates/ai/src/agent.rs`                                                         | Ubah | `pending_actions` pakai `PROPOSAL_TOOL_NAMES`, `PREAMBLE`, test          |
| `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`                                          | Ubah | `proposal_metadata` generik + metadata handler + test                    |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                                      | Ubah | `clean_proposal_decisions` + test                                        |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`                                              | Ubah | Helper `tool_roundtrip_url` + roundtrip 2 kind                           |
| `apps/web/core/lib/ai-work-items.ts`                                                         | Ubah | `parseWorkItemRef`                                                       |
| `apps/web/core/lib/ai-proposals.ts`                                                          | Baru | Tipe proposal/decision/payload + validator                               |
| `apps/web/core/lib/ai-context.ts`                                                            | Ubah | `TAiMessage.proposals`/`proposalDecisions`                               |
| `apps/web/core/lib/ai-conversations.ts`                                                      | Ubah | Mapping metadata + patch type                                            |
| `apps/web/core/store/ai-assistant.store.ts`                                                  | Ubah | `confirmProposal`/`cancelProposal` + dispatch                            |
| `apps/web/core/components/ai/assistant-sidebar/proposals/work-item-update-proposal-card.tsx` | Baru | Kartu diff update                                                        |
| `apps/web/core/components/ai/assistant-sidebar/proposals/comment-proposal-card.tsx`          | Baru | Kartu komentar                                                           |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`                                     | Ubah | Render `message.proposals`                                               |

---

### Task 1: Tool `update_work_item`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/proposals.rs`
- Create: `apps/api-rs/crates/ai/src/tools/mutations.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Buka helper `proposals.rs`**

Di `apps/api-rs/crates/ai/src/tools/proposals.rs` ubah `fn bounded_ref_list` menjadi `pub fn bounded_ref_list` dan `fn parse_iso_date` menjadi `pub fn parse_iso_date`, lalu tambahkan setelah `bounded_ref_list`:

```rust
/// Like `bounded_ref_list` but preserves "field absent" as `None` (an empty
/// list means "clear the field").
pub fn bounded_ref_list_opt(
    values: Option<Vec<String>>,
    label: &str,
) -> Result<Option<Vec<String>>, ToolExecutionError> {
    match values {
        None => Ok(None),
        Some(values) => bounded_ref_list(Some(values), label).map(Some),
    }
}
```

- [ ] **Step 2: Tulis test yang gagal**

Buat `apps/api-rs/crates/ai/src/tools/mutations.rs`:

```rust
//! Fase 2 mutation proposal tools. They never touch the database; the UI
//! renders a confirmation card from the recorded trace.

#[cfg(test)]
mod tests {
    use super::*;

    fn update_args(changes: UpdateWorkItemChanges) -> UpdateWorkItemArgs {
        UpdateWorkItemArgs {
            work_item: "LTS-42".to_string(),
            changes,
        }
    }

    #[test]
    fn update_work_item_requires_at_least_one_change() {
        let err =
            update_work_item_proposal_from_args(update_args(UpdateWorkItemChanges::default()))
                .unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn update_work_item_normalizes_text_refs_and_dates() {
        let proposal = update_work_item_proposal_from_args(update_args(UpdateWorkItemChanges {
            name: Some("  Pump failure  ".into()),
            priority: Some("HIGH".into()),
            assignees: Some(vec![" Budi ".into(), "budi".into(), "Sari".into()]),
            labels: Some(vec![]),
            start_date: Some("2026-10-01".into()),
            target_date: Some("2026-10-05".into()),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(proposal.work_item, "LTS-42");
        assert_eq!(proposal.changes.name.as_deref(), Some("Pump failure"));
        assert_eq!(proposal.changes.priority.as_deref(), Some("high"));
        assert_eq!(
            proposal.changes.assignees,
            Some(vec!["Budi".to_string(), "Sari".to_string()])
        );
        assert_eq!(proposal.changes.labels, Some(vec![]));
        assert_eq!(proposal.changes.start_date.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn update_work_item_rejects_invalid_values() {
        let blank_name = update_work_item_proposal_from_args(update_args(
            UpdateWorkItemChanges {
                name: Some("   ".into()),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));

        let bad_priority = update_work_item_proposal_from_args(update_args(
            UpdateWorkItemChanges {
                priority: Some("p0".into()),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(bad_priority.to_string().contains("priority"));

        let bad_dates = update_work_item_proposal_from_args(update_args(
            UpdateWorkItemChanges {
                start_date: Some("2026-10-05".into()),
                target_date: Some("2026-10-01".into()),
                ..Default::default()
            },
        ))
        .unwrap_err();
        assert!(bad_dates.to_string().contains("start_date"));

        let bad_ref = update_work_item_proposal_from_args(UpdateWorkItemArgs {
            work_item: "LTS".into(),
            changes: UpdateWorkItemChanges {
                name: Some("x".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));
    }

    #[test]
    fn mutation_kind_mapping_covers_update_work_item() {
        assert_eq!(
            mutation_kind_for_tool(UPDATE_WORK_ITEM_NAME),
            Some("update_work_item")
        );
        assert_eq!(mutation_kind_for_tool("search_work_items"), None);
    }
}
```

Tambahkan di `apps/api-rs/crates/ai/src/tools/mod.rs`, setelah `pub mod kb;`:

```rust
pub mod mutations;
```

dan setelah `pub use kb::*;`:

```rust
pub use mutations::*;
```

- [ ] **Step 3: Jalankan test, pastikan gagal**

Run: `cargo test -p ai update_work_item`
Expected: gagal kompilasi (`update_work_item_proposal_from_args` belum ada).

- [ ] **Step 4: Implementasi**

Tambahkan di `apps/api-rs/crates/ai/src/tools/mutations.rs` sebelum `mod tests`:

```rust
use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    bounded_ref_list_opt, parse_iso_date, parse_work_item_ref, priority_arg,
    schema_of, WORK_ITEM_DESCRIPTION_MAX, WORK_ITEM_NAME_MAX,
    WORK_ITEM_STATE_MAX,
};
use crate::agent::{record, ToolTrace};

pub const UPDATE_WORK_ITEM_NAME: &str = "update_work_item";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateWorkItemArgs {
    /// Work item identifier like "LTS-42". Required.
    pub work_item: String,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateWorkItemChanges,
}

/// One requested field change. Human-readable names (state, assignees, labels)
/// are kept as text; the UI resolves them against the project stores.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateWorkItemChanges {
    /// New title (1-255 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New plain-text description or markdown (max 5000 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New priority: urgent, high, medium, low, none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// New state name, e.g. "In Progress".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// New assignee display names or emails; an empty list clears them (max 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
    /// New label names; an empty list clears them (max 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// New start date "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// New target date "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
}

/// Normalized proposal recorded in the trace and rendered as a confirmation
/// card.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateWorkItemProposal {
    pub work_item: String,
    pub changes: UpdateWorkItemChanges,
}

fn normalize_changes(
    changes: UpdateWorkItemChanges,
) -> Result<UpdateWorkItemChanges, ToolExecutionError> {
    let name = match changes.name.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "name must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > WORK_ITEM_NAME_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "name must be at most {WORK_ITEM_NAME_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let description = match changes.description.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "description must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > WORK_ITEM_DESCRIPTION_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "description must be at most {WORK_ITEM_DESCRIPTION_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let priority = priority_arg(changes.priority.as_deref())?;
    let state = match changes.state.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "state must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > WORK_ITEM_STATE_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "state must be at most {WORK_ITEM_STATE_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let assignees = bounded_ref_list_opt(changes.assignees, "assignees")?;
    let labels = bounded_ref_list_opt(changes.labels, "labels")?;
    let start_date = changes
        .start_date
        .as_deref()
        .map(|value| parse_iso_date(value, "start_date"))
        .transpose()?;
    let target_date = changes
        .target_date
        .as_deref()
        .map(|value| parse_iso_date(value, "target_date"))
        .transpose()?;
    if let (Some(start_date), Some(target_date)) = (&start_date, &target_date) {
        if start_date > target_date {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    let normalized = UpdateWorkItemChanges {
        name,
        description,
        priority,
        state,
        assignees,
        labels,
        start_date: start_date.map(|date| date.to_string()),
        target_date: target_date.map(|date| date.to_string()),
    };
    if normalized == UpdateWorkItemChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    Ok(normalized)
}

/// Validate raw tool args into a normalized update proposal. Human-readable
/// names are kept as text; the UI resolves them.
pub fn update_work_item_proposal_from_args(
    args: UpdateWorkItemArgs,
) -> Result<UpdateWorkItemProposal, ToolExecutionError> {
    let work_item = args.work_item.trim();
    if work_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("work_item is required"));
    }
    parse_work_item_ref(work_item)?;
    let changes = normalize_changes(args.changes)?;
    Ok(UpdateWorkItemProposal {
        work_item: work_item.to_string(),
        changes,
    })
}

/// Map a mutation tool name to its proposal `kind`. Plan 2B extends the match.
pub fn mutation_kind_for_tool(name: &str) -> Option<&'static str> {
    match name {
        UPDATE_WORK_ITEM_NAME => Some("update_work_item"),
        _ => None,
    }
}

pub struct UpdateWorkItem {
    pub trace: ToolTrace,
}

impl Tool for UpdateWorkItem {
    const NAME: &'static str = UPDATE_WORK_ITEM_NAME;
    type Args = UpdateWorkItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing work item. Only call this when the \
         user asks to change a work item's fields (title, description, priority, \
         state, assignees, labels, or dates). The work item identifier is \
         required; never guess it. Send only the fields that change. State, \
         assignee, and label names may be human-readable; the UI resolves them. \
         The user must confirm and may edit every field in the UI before \
         anything is saved. Never claim the work item was updated until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateWorkItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_work_item_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateWorkItemProposal serializes"))
    }
}
```

Catatan: `bounded_ref_list_opt` (dari Step 1) sudah menangani batas 10 ref dan dedupe untuk `assignees`/`labels`.

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai update_work_item`
Expected: 3 test lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/tools/proposals.rs
git commit -m "feat(ai): add update_work_item proposal tool"
```

---

### Task 2: Tool `add_comment`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn add_comment_normalizes_and_bounds() {
        let proposal = add_comment_proposal_from_args(AddCommentArgs {
            work_item: " lts-7 ".into(),
            comment: "  replaced the filter  ".into(),
        })
        .unwrap();
        assert_eq!(proposal.work_item, "lts-7");
        assert_eq!(proposal.comment, "replaced the filter");

        let empty = add_comment_proposal_from_args(AddCommentArgs {
            work_item: "LTS-7".into(),
            comment: "   ".into(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("comment"));

        let too_long = add_comment_proposal_from_args(AddCommentArgs {
            work_item: "LTS-7".into(),
            comment: "a".repeat(MAX_COMMENT_CHARS + 1),
        })
        .unwrap_err();
        assert!(too_long.to_string().contains("at most"));

        let bad_ref = add_comment_proposal_from_args(AddCommentArgs {
            work_item: "nope".into(),
            comment: "hi".into(),
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));
    }

    #[test]
    fn mutation_kind_mapping_covers_add_comment() {
        assert_eq!(
            mutation_kind_for_tool(ADD_COMMENT_NAME),
            Some("add_comment")
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai add_comment`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `UpdateWorkItem`:

```rust
pub const ADD_COMMENT_NAME: &str = "add_comment";
pub const MAX_COMMENT_CHARS: usize = 5000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddCommentArgs {
    /// Work item identifier like "LTS-42". Required.
    pub work_item: String,
    /// Plain-text comment (1-5000 characters).
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddCommentProposal {
    pub work_item: String,
    pub comment: String,
}

/// Validate raw tool args into a normalized comment proposal.
pub fn add_comment_proposal_from_args(
    args: AddCommentArgs,
) -> Result<AddCommentProposal, ToolExecutionError> {
    let work_item = args.work_item.trim();
    if work_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("work_item is required"));
    }
    parse_work_item_ref(work_item)?;
    let comment = args.comment.trim();
    if comment.is_empty() {
        return Err(ToolExecutionError::invalid_args("comment is required"));
    }
    if comment.chars().count() > MAX_COMMENT_CHARS {
        return Err(ToolExecutionError::invalid_args(format!(
            "comment must be at most {MAX_COMMENT_CHARS} characters"
        )));
    }
    Ok(AddCommentProposal {
        work_item: work_item.to_string(),
        comment: comment.to_string(),
    })
}

pub struct AddComment {
    pub trace: ToolTrace,
}

impl Tool for AddComment {
    const NAME: &'static str = ADD_COMMENT_NAME;
    type Args = AddCommentArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose posting one comment on an existing work item. Only call this \
         when the user asks to comment on or reply to a work item. The work item \
         identifier is required; never guess it. Keep the comment in the user's \
         language. The user must confirm and may edit the text in the UI before \
         it is posted. Never claim the comment was posted until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<AddCommentArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = add_comment_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("AddCommentProposal serializes"))
    }
}
```

Perbarui `mutation_kind_for_tool`:

```rust
pub fn mutation_kind_for_tool(name: &str) -> Option<&'static str> {
    match name {
        UPDATE_WORK_ITEM_NAME => Some("update_work_item"),
        ADD_COMMENT_NAME => Some("add_comment"),
        _ => None,
    }
}
```

Tidak ada import baru yang diperlukan untuk Task 2 — pastikan daftar `use super::{...}` tidak memuat nama yang tidak terpakai.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai mutations`
Expected: 5 test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add add_comment proposal tool"
```

---

### Task 3: Registry, `PROPOSAL_TOOL_NAMES`, `pending_actions`, PREAMBLE

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs` (di akhir modul):

```rust
    #[tokio::test]
    async fn mutation_tools_expose_metadata() {
        let update = UpdateWorkItem {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(UpdateWorkItem::NAME, "update_work_item");
        assert!(update.parameters()["properties"]["changes"].is_object());
        assert!(!update.description().is_empty());

        let comment = AddComment {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(AddComment::NAME, "add_comment");
        assert!(comment.parameters()["properties"]["comment"].is_object());
        assert!(!comment.description().is_empty());
    }

    #[test]
    fn proposal_tools_are_not_schedule_eligible() {
        for name in PROPOSAL_TOOL_NAMES {
            assert!(
                !crate::schedule::SPEC_TOOLS.contains(&name),
                "{name} must never be schedule-eligible"
            );
        }
    }
```

Tambahkan di `mod tests` `agent.rs`:

```rust
    #[test]
    fn pending_actions_covers_fase2_mutations() {
        let trace = new_trace();
        record(
            &trace,
            crate::tools::UPDATE_WORK_ITEM_NAME,
            &json!({"work_item": "LTS-1", "changes": {"priority": "high"}}),
        );
        record(
            &trace,
            crate::tools::ADD_COMMENT_NAME,
            &json!({"work_item": "LTS-1", "comment": "hi"}),
        );
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["kind"], json!("update_work_item"));
        assert_eq!(actions[1]["kind"], json!("add_comment"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pending_actions_covers_fase2`
Expected: gagal kompilasi (`PROPOSAL_TOOL_NAMES` belum ada).

- [ ] **Step 3: Implementasi**

Di `tools/mod.rs`, setelah konstanta `MAX_LIMIT`:

```rust
/// Proposal tool names: recorded in the trace but never executed against the
/// database and never schedule-eligible. Plan 2B extends the list.
pub const PROPOSAL_TOOL_NAMES: [&str; 4] = [
    CREATE_SCHEDULE_NAME,
    CREATE_WORK_ITEM_NAME,
    UPDATE_WORK_ITEM_NAME,
    ADD_COMMENT_NAME,
];
```

Di `workspace_tools`, setelah `.tool(GetArticle { ... })` dan sebelum `.tool(CreateSchedule { ... })`:

```rust
        .tool(UpdateWorkItem {
            trace: trace.clone(),
        })
        .tool(AddComment {
            trace: trace.clone(),
        })
```

Di `agent.rs`, ganti isi filter `pending_actions`:

```rust
use crate::tools::PROPOSAL_TOOL_NAMES;
```

```rust
    recorded
        .iter()
        .filter(|call| PROPOSAL_TOOL_NAMES.contains(&call.name.as_str()))
        .map(|call| {
            json!({
                "kind": call.name,
                "proposal": call.arguments.clone(),
            })
        })
        .collect()
```

Perbarui `PREAMBLE`: setelah kalimat "A work item is only created after the user confirms the proposal card, so never say it is already created." tambahkan:

```text
When the user asks to change an existing work item's fields, call \
update_work_item with its identifier and only the fields that change. When \
the user asks to comment on a work item, call add_comment with its identifier \
and the comment text. Both are proposals: the user must confirm them in the \
UI first, so never say a work item was updated or a comment was posted before \
that confirmation.
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: seluruh test lulus (117 + 4 baru).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): register update and comment proposal tools"
```

---

### Task 4: Metadata `proposals` generik di endpoint agent

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_agent/mod.rs`:

```rust
    #[test]
    fn proposal_metadata_maps_mutation_tools_with_kind_and_key() {
        let trace = new_trace();
        record(
            &trace,
            ai::tools::UPDATE_WORK_ITEM_NAME,
            &json!({"work_item": "LTS-1", "changes": {"priority": "high"}}),
        );
        record(&trace, "search_work_items", &json!({}));
        record(
            &trace,
            ai::tools::ADD_COMMENT_NAME,
            &json!({"work_item": "LTS-1", "comment": "hi"}),
        );
        let proposals = proposal_metadata(&trace);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0]["kind"], json!("update_work_item"));
        assert_eq!(
            proposals[0]["proposal"]["changes"]["priority"],
            json!("high")
        );
        assert_ne!(proposals[0]["key"], proposals[1]["key"]);
        assert!(proposals[0]["key"].as_str().is_some());
        assert_eq!(proposals[1]["kind"], json!("add_comment"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --lib proposal_metadata_maps_mutation`
Expected: gagal kompilasi (`proposal_metadata` belum ada).

- [ ] **Step 3: Implementasi**

Tambahkan di `ai_agent/mod.rs` setelah `work_item_proposals_metadata`:

```rust
/// Metadata entry for every generic mutation proposal in this turn: one
/// server-generated key per proposal so the FE can persist a decision per card.
pub fn proposal_metadata(trace: &ToolTrace) -> Vec<Value> {
    trace
        .lock()
        .map(|recorded| {
            recorded
                .iter()
                .filter_map(|call| {
                    ai::tools::mutation_kind_for_tool(&call.name).map(|kind| {
                        json!({"key": Uuid::new_v4(), "kind": kind, "proposal": call.arguments.clone()})
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}
```

Di handler `workspace_ai_agent`, setelah `let work_items = work_item_proposals_metadata(&trace);`:

```rust
            let proposals = proposal_metadata(&trace);
```

dan setelah blok `if !work_items.is_empty() { ... }`:

```rust
            if !proposals.is_empty() {
                metadata["proposals"] = json!(proposals);
            }
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --lib proposal_metadata_maps_mutation`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs
git commit -m "feat(api): persist generic proposal metadata"
```

---

### Task 5: `proposal_decisions` di `clean_metadata_patch`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_conversations.rs`:

```rust
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
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --lib metadata_patch_accepts_proposal_decisions`
Expected: gagal (key `proposal_decisions` belum di-allowlist).

- [ ] **Step 3: Implementasi**

Di `clean_metadata_patch`, tambahkan match arm setelah `"work_item_decisions"`:

```rust
            "proposal_decisions" => {
                clean.insert(key.clone(), clean_proposal_decisions(value)?);
            }
```

Tambahkan setelah `clean_work_item_decisions`:

```rust
/// Result keys the FE may persist per proposal kind. Kinds without result
/// data get an empty allowlist.
fn allowed_result_keys(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "update_work_item" => Some(&[]),
        "add_comment" => Some(&["created_comment_id"]),
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
                if kind == "add_comment" && !result.contains_key("created_comment_id") {
                    return Err(
                        "add_comment applied decisions need result.created_comment_id".to_string()
                    );
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
```

Perbarui doc comment `patch_message` agar menyebut `proposal_decisions`.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --lib metadata_patch`
Expected: seluruh test metadata lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): accept generic proposal decisions"
```

---

### Task 6: Integration test roundtrip

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tambahkan helper upstream**

Di dalam `mod tool_roundtrip`, setelah `work_item_roundtrip_url`:

```rust
    /// Serve one tool call for `name`/`arguments`, then a final answer.
    pub(super) async fn tool_roundtrip_url(name: &'static str, arguments: &'static str) -> String {
        let state: Shared = Arc::new(Upstream::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handler = move |State(state): State<Shared>, Json(body): Json<Value>| async move {
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
                                    "function": {"name": name, "arguments": arguments}
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
        };
        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(state);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/v1")
    }
```

- [ ] **Step 2: Tambahkan dua test DB-backed**

Di akhir file (setelah `work_item_proposal_metadata_and_decisions_roundtrip`):

```rust
#[tokio::test]
async fn update_work_item_proposal_roundtrip_persists_generic_metadata() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "update_work_item",
        r#"{"work_item":"LTS-42","changes":{"priority":"high"}}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "set LTS-42 to high priority",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);

    let proposals = body["assistant_message"]["metadata"]["proposals"]
        .as_array()
        .expect("proposals array");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0]["kind"], json!("update_work_item"));
    assert_eq!(
        proposals[0]["proposal"]["changes"]["priority"],
        json!("high")
    );
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    // An applied decision (no result) roundtrips.
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "update_work_item", "decision": "applied"}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["decision"],
        json!("applied")
    );

    // A result on a result-less kind is rejected.
    let (status, _) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {
                "kind": "update_work_item",
                "decision": "applied",
                "result": {"created_comment_id": Uuid::new_v4()},
            }
        }}})),
    )
    .await
    .expect("patch bad result");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn add_comment_proposal_roundtrip_requires_comment_result() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "add_comment",
        r#"{"work_item":"LTS-42","comment":"Replaced the filter"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "comment on LTS-42",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);

    let proposals = body["assistant_message"]["metadata"]["proposals"]
        .as_array()
        .expect("proposals array");
    assert_eq!(proposals[0]["kind"], json!("add_comment"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    // Applied without the comment id is rejected.
    let (status, _) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "add_comment", "decision": "applied"}
        }}})),
    )
    .await
    .expect("patch missing id");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Applied with the comment id persists.
    let comment_id = Uuid::new_v4();
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {
                "kind": "add_comment",
                "decision": "applied",
                "result": {"created_comment_id": comment_id},
            }
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["result"]["created_comment_id"],
        json!(comment_id)
    );

    scratch.purge(&pool).await;
}
```

- [ ] **Step 3: Jalankan test**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus (butuh DB dev + Redis; lihat AGENTS.md).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): roundtrip generic proposal metadata and decisions"
```

---

### Task 7: FE `parseWorkItemRef` + `ai-proposals.ts`

**Files:**

- Modify: `apps/web/core/lib/ai-work-items.ts`
- Modify: `apps/web/core/lib/ai-work-items.test.ts`
- Create: `apps/web/core/lib/ai-proposals.ts`
- Create: `apps/web/core/lib/ai-proposals.test.ts`

- [ ] **Step 1: Tulis test `parseWorkItemRef` yang gagal**

Di `apps/web/core/lib/ai-work-items.test.ts`, tambahkan `parseWorkItemRef` ke daftar import dari `./ai-work-items`, lalu tambahkan di akhir file:

```ts
describe("parseWorkItemRef", () => {
  it("splits an identifier into project and sequence", () => {
    expect(parseWorkItemRef(" LTS-42 ")).toEqual({ projectIdentifier: "LTS", sequenceId: "42" });
    expect(parseWorkItemRef("lts-7")).toEqual({ projectIdentifier: "lts", sequenceId: "7" });
  });

  it("rejects malformed references", () => {
    expect(parseWorkItemRef("LTS")).toBeNull();
    expect(parseWorkItemRef("LTS-")).toBeNull();
    expect(parseWorkItemRef("-42")).toBeNull();
    expect(parseWorkItemRef("LTS-abc")).toBeNull();
    expect(parseWorkItemRef("LTS-0")).toBeNull();
    expect(parseWorkItemRef("L T S-4")).toBeNull();
  });
});
```

- [ ] **Step 2: Implementasi `parseWorkItemRef`**

Tambahkan di `apps/web/core/lib/ai-work-items.ts` setelah `isWorkItemCommand`:

```ts
export type TWorkItemRef = { projectIdentifier: string; sequenceId: string };

/** Split "LTS-42" into its project identifier and sequence id; null when malformed. */
export const parseWorkItemRef = (reference: string): TWorkItemRef | null => {
  const trimmed = reference.trim();
  const index = trimmed.lastIndexOf("-");
  if (index <= 0 || index === trimmed.length - 1) return null;
  const projectIdentifier = trimmed.slice(0, index);
  const sequenceId = trimmed.slice(index + 1);
  if (!/^[A-Za-z0-9]+$/.test(projectIdentifier) || !/^\d+$/.test(sequenceId)) return null;
  if (Number(sequenceId) <= 0) return null;
  return { projectIdentifier, sequenceId };
};
```

- [ ] **Step 3: Tulis test `ai-proposals` yang gagal**

Buat `apps/web/core/lib/ai-proposals.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { changedFieldCount, validateAddCommentProposal, validateUpdateWorkItemProposal } from "./ai-proposals";

describe("validateUpdateWorkItemProposal", () => {
  it("accepts a minimal change", () => {
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { priority: "high" } })).toBeNull();
  });

  it("requires a work item and at least one change", () => {
    expect(validateUpdateWorkItemProposal({ changes: { priority: "high" } })).toBe("Work item is required.");
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: {} })).toBe(
      "At least one change is required."
    );
  });

  it("rejects invalid fields", () => {
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { name: "  " } })).toBe(
      "Title must not be empty."
    );
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { priority: "p0" } })).toBe(
      "Unknown priority."
    );
    expect(validateUpdateWorkItemProposal({ work_item: "LTS-1", changes: { start_date: "05-10-2026" } })).toBe(
      "Start date must be YYYY-MM-DD."
    );
    expect(
      validateUpdateWorkItemProposal({
        work_item: "LTS-1",
        changes: { start_date: "2026-10-05", target_date: "2026-10-01" },
      })
    ).toBe("Start date must not be after target date.");
  });

  it("counts only set fields", () => {
    expect(changedFieldCount({ name: null, priority: "high", labels: [] })).toBe(2);
    expect(changedFieldCount({})).toBe(0);
  });
});

describe("validateAddCommentProposal", () => {
  it("accepts a comment and rejects empty or oversized ones", () => {
    expect(validateAddCommentProposal({ work_item: "LTS-1", comment: "hi" })).toBeNull();
    expect(validateAddCommentProposal({ work_item: "LTS-1", comment: "  " })).toBe("Comment is required.");
    expect(validateAddCommentProposal({ work_item: "LTS-1", comment: "a".repeat(5001) })).toBe(
      "Comment must be at most 5000 characters."
    );
    expect(validateAddCommentProposal({ comment: "hi" })).toBe("Work item is required.");
  });
});
```

- [ ] **Step 4: Jalankan test, pastikan gagal**

Run: `pnpm test core/lib/ai-proposals.test.ts core/lib/ai-work-items.test.ts`
Expected: gagal import (`ai-proposals` belum ada).

- [ ] **Step 5: Implementasi `ai-proposals.ts`**

Buat `apps/web/core/lib/ai-proposals.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TIssue } from "@plane/types";
import { WORK_ITEM_PRIORITIES, type TWorkItemPriority } from "@/lib/ai-work-items";

export const PROPOSAL_KINDS = ["update_work_item", "add_comment"] as const;
export type TAiProposalKind = (typeof PROPOSAL_KINDS)[number];

export const PROPOSAL_LIMITS = {
  comment: 5000,
  name: 255,
  description: 5000,
  refs: 10,
} as const;

export type TAiWorkItemChanges = {
  name?: string | null;
  description?: string | null;
  priority?: string | null;
  state?: string | null;
  assignees?: string[] | null;
  labels?: string[] | null;
  start_date?: string | null;
  target_date?: string | null;
};

export type TAiUpdateWorkItemProposal = {
  work_item: string;
  changes: TAiWorkItemChanges;
};

export type TAiAddCommentProposal = {
  work_item: string;
  comment: string;
};

export type TAiProposal =
  | { key: string; kind: "update_work_item"; proposal: TAiUpdateWorkItemProposal }
  | { key: string; kind: "add_comment"; proposal: TAiAddCommentProposal };

export type TAiProposalDecisionResult = {
  created_comment_id?: string;
};

export type TAiProposalDecision = {
  kind: TAiProposalKind;
  decision: "applied" | "cancelled";
  result?: TAiProposalDecisionResult;
};

export type TAiProposalConfirmPayload =
  | { kind: "update_work_item"; projectId: string; issueId: string; changes: Partial<TIssue> }
  | { kind: "add_comment"; projectId: string; issueId: string; commentHtml: string };

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

/** Count fields explicitly set (null/undefined means "not changed"). */
export const changedFieldCount = (changes: TAiWorkItemChanges): number =>
  Object.values(changes).filter((value) => value !== null && value !== undefined).length;

/** Mirrors `update_work_item_proposal_from_args` on the backend. */
export const validateUpdateWorkItemProposal = (proposal: Partial<TAiUpdateWorkItemProposal>): string | null => {
  if (!proposal.work_item?.trim()) return "Work item is required.";
  const changes = proposal.changes ?? {};
  if (changedFieldCount(changes) === 0) return "At least one change is required.";
  const name = changes.name?.trim() ?? "";
  if (changes.name != null && !name) return "Title must not be empty.";
  if (name.length > PROPOSAL_LIMITS.name) return `Title must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (changes.priority && !WORK_ITEM_PRIORITIES.includes(changes.priority as TWorkItemPriority))
    return "Unknown priority.";
  if ((changes.assignees?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} assignees are allowed.`;
  if ((changes.labels?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} labels are allowed.`;
  const start = changes.start_date?.trim() ?? "";
  const target = changes.target_date?.trim() ?? "";
  if (changes.start_date != null && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (changes.target_date != null && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  return null;
};

/** Mirrors `add_comment_proposal_from_args` on the backend. */
export const validateAddCommentProposal = (proposal: Partial<TAiAddCommentProposal>): string | null => {
  if (!proposal.work_item?.trim()) return "Work item is required.";
  const comment = proposal.comment?.trim() ?? "";
  if (!comment) return "Comment is required.";
  if (comment.length > PROPOSAL_LIMITS.comment) return `Comment must be at most ${PROPOSAL_LIMITS.comment} characters.`;
  return null;
};
```

- [ ] **Step 6: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-proposals.test.ts core/lib/ai-work-items.test.ts`
Expected: seluruh test lulus.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposals.test.ts apps/web/core/lib/ai-work-items.ts apps/web/core/lib/ai-work-items.test.ts
git commit -m "feat(web): add generic proposal types and validators"
```

---

### Task 8: FE metadata plumbing

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts`
- Modify: `apps/web/core/lib/ai-conversations.ts`
- Modify: `apps/web/core/lib/ai-conversations.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `ai-conversations.test.ts` di dalam `describe("toAiMessage")`:

```ts
it("maps generic proposals and decisions from metadata", () => {
  const message = toAiMessage(
    stored({
      metadata: {
        proposals: [
          { key: "k1", kind: "update_work_item", proposal: { work_item: "LTS-1", changes: { priority: "high" } } },
          { key: "k2", kind: "add_comment", proposal: { work_item: "LTS-1", comment: "hi" } },
        ],
        proposal_decisions: { k1: { kind: "update_work_item", decision: "applied" } },
      },
    })
  );
  expect(message.proposals).toHaveLength(2);
  expect(message.proposals?.[0].kind).toBe("update_work_item");
  expect(message.proposals?.[1].kind).toBe("add_comment");
  expect(message.proposalDecisions?.k1.decision).toBe("applied");
});

it("leaves generic proposal fields undefined when metadata is absent", () => {
  const message = toAiMessage(stored({}));
  expect(message.proposals).toBeUndefined();
  expect(message.proposalDecisions).toBeUndefined();
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm test core/lib/ai-conversations.test.ts`
Expected: gagal kompilasi/tipe (`proposals` belum ada di `TAiMessage`).

- [ ] **Step 3: Implementasi `ai-context.ts`**

Tambahkan import di `ai-context.ts`:

```ts
import type { TAiProposal, TAiProposalDecision } from "@/lib/ai-proposals";
```

Tambahkan field di `TAiMessage`:

```ts
  proposals?: TAiProposal[];
  proposalDecisions?: Record<string, TAiProposalDecision>;
```

- [ ] **Step 4: Implementasi `ai-conversations.ts`**

Tambahkan import:

```ts
import type { TAiProposal, TAiProposalDecision } from "@/lib/ai-proposals";
```

Tambahkan field di `TAiMessageMetadata`:

```ts
  proposals?: TAiProposal[];
  proposal_decisions?: Record<string, TAiProposalDecision>;
```

Perbarui `TAiMessageMetadataPatch`:

```ts
export type TAiMessageMetadataPatch = Pick<TAiMessageMetadata, "schedule_decision" | "created_schedule_id"> & {
  work_item_decisions?: Record<string, TAiWorkItemDecision>;
  proposal_decisions?: Record<string, TAiProposalDecision>;
};
```

Tambahkan mapping di `toAiMessage`:

```ts
  proposals: stored.metadata?.proposals,
  proposalDecisions: stored.metadata?.proposal_decisions,
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-conversations.test.ts`
Expected: lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-conversations.ts apps/web/core/lib/ai-conversations.test.ts
git commit -m "feat(web): map generic proposal metadata onto messages"
```

---

### Task 9: Store `confirmProposal` / `cancelProposal`

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-assistant.store.test.ts`, tambahkan `patchIssue` dan mock `comments` ke `makeServices`:

```ts
  issues: {
    createIssue: vi.fn(async (_slug: string, projectId: string) => ({ id: "i1", project_id: projectId })),
    patchIssue: vi.fn(async () => undefined),
    ...overrides.issues,
  },
  comments: {
    createIssueComment: vi.fn(async (_slug: string, projectId: string, issueId: string) => ({
      id: "cm1",
      project_id: projectId,
      issue: issueId,
    })),
    ...overrides.comments,
  },
```

Perbarui `makeStore` agar meneruskan service kelima:

```ts
const makeStore = (services = makeServices()) =>
  new AIAssistantStore(
    services.ai as any,
    services.schedules as any,
    services.conversations as any,
    services.issues as any,
    services.comments as any
  );
```

Tambahkan di akhir file:

```ts
const mutationMessage = (metadata: Record<string, unknown>) => ({
  id: "srv-assistant",
  role: "assistant" as const,
  content: "Here is a proposal",
  content_html: "<p>Here is a proposal</p>",
  metadata,
  created_at: "2026-10-05T09:00:00Z",
});

const updateMetadata = {
  proposals: [{ key: "k1", kind: "update_work_item", proposal: { work_item: "LTS-1", changes: { priority: "high" } } }],
};

const commentMetadata = {
  proposals: [{ key: "k2", kind: "add_comment", proposal: { work_item: "LTS-1", comment: "Replaced the filter" } }],
};

describe("generic proposals", () => {
  const openAgent = async (services = makeServices(), metadata: Record<string, unknown> = updateMetadata) => {
    services.conversations.listMessages = vi.fn(async () => [mutationMessage(metadata)]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    return { store, services };
  };

  it("applies an update proposal and persists the decision", async () => {
    const { store, services } = await openAgent();
    await store.confirmProposal("srv-assistant", "k1", {
      kind: "update_work_item",
      projectId: "p1",
      issueId: "i1",
      changes: { priority: "high" },
    });
    expect(services.issues.patchIssue).toHaveBeenCalledWith("acme", "p1", "i1", { priority: "high" });
    expect(store.messages[0].proposalDecisions?.k1).toEqual({
      kind: "update_work_item",
      decision: "applied",
    });
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith(
      "acme",
      expect.any(String),
      "srv-assistant",
      { proposal_decisions: { k1: { kind: "update_work_item", decision: "applied" } } }
    );
  });

  it("applies a comment proposal and stores the comment id", async () => {
    const { store, services } = await openAgent(makeServices(), commentMetadata);
    await store.confirmProposal("srv-assistant", "k2", {
      kind: "add_comment",
      projectId: "p1",
      issueId: "i1",
      commentHtml: "<p>Replaced the filter</p>",
    });
    expect(services.comments.createIssueComment).toHaveBeenCalledWith("acme", "p1", "i1", {
      comment_html: "<p>Replaced the filter</p>",
    });
    expect(store.messages[0].proposalDecisions?.k2).toEqual({
      kind: "add_comment",
      decision: "applied",
      result: { created_comment_id: "cm1" },
    });
  });

  it("does not apply the same proposal twice", async () => {
    const { store, services } = await openAgent();
    const payload = {
      kind: "update_work_item" as const,
      projectId: "p1",
      issueId: "i1",
      changes: { priority: "high" },
    };
    await store.confirmProposal("srv-assistant", "k1", payload);
    await store.confirmProposal("srv-assistant", "k1", payload);
    expect(services.issues.patchIssue).toHaveBeenCalledTimes(1);
  });

  it("keeps the decision pending when the mutation fails", async () => {
    const services = makeServices();
    services.issues.patchIssue = vi.fn(async () => {
      throw new Error("nope");
    });
    const { store } = await openAgent(services);
    await expect(
      store.confirmProposal("srv-assistant", "k1", {
        kind: "update_work_item",
        projectId: "p1",
        issueId: "i1",
        changes: { priority: "high" },
      })
    ).rejects.toThrow("nope");
    expect(store.messages[0].proposalDecisions?.k1).toBeUndefined();
    expect(services.conversations.updateMessageMetadata).not.toHaveBeenCalled();
  });

  it("cancels a proposal and persists the decision", async () => {
    const { store, services } = await openAgent();
    store.cancelProposal("srv-assistant", "k1");
    expect(store.messages[0].proposalDecisions?.k1).toEqual({
      kind: "update_work_item",
      decision: "cancelled",
    });
    await flush();
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith(
      "acme",
      expect.any(String),
      "srv-assistant",
      { proposal_decisions: { k1: { kind: "update_work_item", decision: "cancelled" } } }
    );
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: gagal kompilasi (`confirmProposal` belum ada).

- [ ] **Step 3: Implementasi store**

Di `ai-assistant.store.ts` tambahkan import:

```ts
import { IssueCommentService } from "@/services/issue/issue_comment.service";
import type { TAiProposalConfirmPayload, TAiProposalDecision, TAiProposalDecisionResult } from "@/lib/ai-proposals";
```

Perbarui pick type dan tambah pick komentar:

```ts
type TIssueService = Pick<IssueService, "createIssue" | "patchIssue">;
type TCommentsService = Pick<IssueCommentService, "createIssueComment">;
```

Tambahkan ke `IAIAssistantStore` setelah `resolveWorkItemProposal`:

```ts
  confirmProposal: (messageId: string, key: string, payload: TAiProposalConfirmPayload) => Promise<void>;
  cancelProposal: (messageId: string, key: string) => void;
```

Perbarui constructor:

```ts
  constructor(
    private aiService: TAiService = new AIService(),
    private schedulesService: TAiSchedulesService = new AiSchedulesService(),
    private conversationsService: TAiConversationsService = new AiConversationsService(),
    private issuesService: TIssueService = new IssueService(),
    private commentsService: TCommentsService = new IssueCommentService()
  ) {
```

Tambahkan `confirmProposal: action,` dan `cancelProposal: action,` di `makeObservable` setelah `resolveWorkItemProposal`.

Tambahkan method setelah `resolveWorkItemProposal`:

```ts
confirmProposal = async (messageId: string, key: string, payload: TAiProposalConfirmPayload) => {
  const slug = this.workspaceSlug;
  const conversationId = this.activeConversationId;
  const message = this.messages.find((candidate) => candidate.id === messageId);
  const entry = message?.proposals?.find((candidate) => candidate.key === key);
  if (!slug || !message || !entry) return;
  if (entry.kind !== payload.kind) return;
  if (message.proposalDecisions?.[key]) return;
  let result: TAiProposalDecisionResult | undefined;
  if (payload.kind === "update_work_item") {
    await this.issuesService.patchIssue(slug, payload.projectId, payload.issueId, payload.changes);
  } else {
    const created = await this.commentsService.createIssueComment(slug, payload.projectId, payload.issueId, {
      comment_html: payload.commentHtml,
    });
    if (!created?.id) throw new Error("Comment creation returned no id");
    result = { created_comment_id: created.id };
  }
  runInAction(() => {
    message.proposalDecisions = {
      ...message.proposalDecisions,
      [key]: { kind: payload.kind, decision: "applied", ...(result ? { result } : {}) },
    };
  });
  await this.persistProposalDecisions(conversationId, message);
};

cancelProposal = (messageId: string, key: string) => {
  const conversationId = this.activeConversationId;
  const message = this.messages.find((candidate) => candidate.id === messageId);
  const entry = message?.proposals?.find((candidate) => candidate.key === key);
  if (!message || !entry) return;
  if (message.proposalDecisions?.[key]) return;
  const decision: TAiProposalDecision = { kind: entry.kind, decision: "cancelled" };
  runInAction(() => {
    message.proposalDecisions = {
      ...message.proposalDecisions,
      [key]: decision,
    };
  });
  void this.persistProposalDecisions(conversationId, message);
};
```

Tambahkan private helper setelah `persistWorkItemDecisions`:

```ts
  private persistProposalDecisions = async (conversationId: string | undefined, message: TAiMessage) => {
    const slug = this.workspaceSlug;
    if (!slug || !conversationId || !message.proposalDecisions) return;
    try {
      await this.conversationsService.updateMessageMetadata(slug, conversationId, message.id, {
        proposal_decisions: message.proposalDecisions,
      });
    } catch {
      // best-effort: the mutation already succeeded; a reload re-fetches metadata
    }
  };
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: seluruh test store lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): confirm and cancel generic proposals"
```

---

### Task 10: Kartu `WorkItemUpdateProposalCard` + `CommentProposalCard` + render root

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/work-item-update-proposal-card.tsx`
- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/comment-proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Buat `work-item-update-proposal-card.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import type { TIssue } from "@plane/types";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useLabel } from "@/hooks/store/use-label";
import { useMember } from "@/hooks/store/use-member";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import { stripHtml } from "@/lib/ai-context";
import {
  matchAssignees,
  matchLabels,
  matchState,
  parseWorkItemRef,
  textToDescriptionHtml,
  workItemHref,
  WORK_ITEM_PRIORITIES,
} from "@/lib/ai-work-items";
import {
  validateUpdateWorkItemProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiUpdateWorkItemProposal,
  type TAiWorkItemChanges,
} from "@/lib/ai-proposals";

type Props = {
  proposal: TAiUpdateWorkItemProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const WorkItemUpdateProposalCard = observer(function WorkItemUpdateProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  // store hooks
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { getProjectStates, fetchProjectStates, getStateById } = useProjectState();
  const { project: projectMemberStore } = useMember();
  const { getProjectLabels, fetchProjectLabels } = useLabel();
  // component state
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
  const [draft, setDraft] = useState<TAiWorkItemChanges>(proposal.changes);
  const [stateId, setStateId] = useState("");
  const [assigneeIds, setAssigneeIds] = useState<string[]>([]);
  const [labelIds, setLabelIds] = useState<string[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceSlug) return;
    const ref = parseWorkItemRef(proposal.work_item);
    if (!ref) {
      setResolveError("Work item reference is invalid.");
      return;
    }
    let cancelled = false;
    fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId)
      .then((fetched) => {
        if (!cancelled) setIssue(fetched);
      })
      .catch(() => {
        if (!cancelled) setResolveError("Work item not found or not accessible.");
      });
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, proposal.work_item, fetchIssueWithIdentifier]);

  const projectId = issue?.project_id ?? null;

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

  // Resolve the agent's human-readable values against the loaded project data
  // (same adjust-state-during-render pattern as WorkItemProposalCard).
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

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Work item updated.{" "}
        {workspaceSlug && issue && (
          <Link
            href={workItemHref(workspaceSlug, issue.project_id, issue.id)}
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
        Update cancelled.
      </p>
    );
  }

  const has = (field: keyof TAiWorkItemChanges) =>
    proposal.changes[field] !== null && proposal.changes[field] !== undefined;
  const patch = (fields: Partial<TAiWorkItemChanges>) => setDraft((current) => ({ ...current, ...fields }));
  const toggleAssignee = (id: string, checked: boolean) =>
    setAssigneeIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));
  const toggleLabel = (id: string, checked: boolean) =>
    setLabelIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)));

  const validationError = issue
    ? validateUpdateWorkItemProposal({ work_item: proposal.work_item, changes: draft })
    : null;
  const oldStateName = issue ? getStateById(issue.state_id)?.name : undefined;
  const oldAssigneeNames = (issue?.assignee_ids ?? [])
    .map((id) => members.find((member) => member.id === id))
    .map((member) => member?.display_name || member?.email)
    .filter((name): name is string => Boolean(name));
  const oldLabelNames = (issue?.label_ids ?? [])
    .map((id) => labels?.find((label) => label.id === id)?.name)
    .filter((name): name is string => Boolean(name));

  const confirm = async () => {
    if (!issue) return;
    setSubmitting(true);
    setError(null);
    try {
      const changes: Partial<TIssue> = {};
      if (has("name") && draft.name) changes.name = draft.name.trim();
      if (has("description")) {
        const descriptionHtml = textToDescriptionHtml(draft.description);
        if (descriptionHtml) changes.description_html = descriptionHtml;
      }
      if (has("priority") && draft.priority) changes.priority = draft.priority as TIssue["priority"];
      if (has("state") && stateId) changes.state_id = stateId;
      if (has("assignees")) changes.assignee_ids = assigneeIds;
      if (has("labels")) changes.label_ids = labelIds;
      if (has("start_date") && draft.start_date) changes.start_date = draft.start_date;
      if (has("target_date") && draft.target_date) changes.target_date = draft.target_date;
      await onConfirm({
        kind: "update_work_item",
        projectId: issue.project_id,
        issueId: issue.id,
        changes,
      });
    } catch {
      setError("Could not update the work item. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Work item update proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Update {proposal.work_item}</p>
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}

      {has("name") && (
        <label className="mt-2 block text-12 text-secondary">
          Title <span className="text-tertiary">(was: {issue?.name ?? "—"})</span>
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.name ?? ""}
            onChange={(event) => patch({ name: event.target.value })}
          />
        </label>
      )}

      {has("description") && (
        <label className="mt-2 block text-12 text-secondary">
          Description{" "}
          <span className="text-tertiary">(was: {stripHtml(issue?.description_html ?? "").slice(0, 80) || "—"})</span>
          <textarea
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            rows={3}
            value={draft.description ?? ""}
            onChange={(event) => patch({ description: event.target.value })}
          />
        </label>
      )}

      {has("priority") && (
        <label className="mt-2 block text-12 text-secondary">
          Priority <span className="text-tertiary">(was: {issue?.priority ?? "none"})</span>
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
      )}

      {has("state") && (
        <label className="mt-2 block text-12 text-secondary">
          State <span className="text-tertiary">(was: {oldStateName ?? "—"})</span>
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
      )}

      {has("assignees") && (
        <div className="mt-2 text-12 text-secondary">
          Assignees <span className="text-tertiary">(was: {oldAssigneeNames.join(", ") || "none"})</span>
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

      {has("labels") && (labels ?? []).length > 0 && (
        <div className="mt-2 text-12 text-secondary">
          Labels <span className="text-tertiary">(was: {oldLabelNames.join(", ") || "none"})</span>
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

      {(has("start_date") || has("target_date")) && (
        <div className="mt-2 flex gap-2">
          {has("start_date") && (
            <label className="flex-1 text-12 text-secondary">
              Start date <span className="text-tertiary">(was: {issue?.start_date ?? "—"})</span>
              <input
                type="date"
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={draft.start_date ?? ""}
                onChange={(event) => patch({ start_date: event.target.value || null })}
              />
            </label>
          )}
          {has("target_date") && (
            <label className="flex-1 text-12 text-secondary">
              Target date <span className="text-tertiary">(was: {issue?.target_date ?? "—"})</span>
              <input
                type="date"
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={draft.target_date ?? ""}
                onChange={(event) => patch({ target_date: event.target.value || null })}
              />
            </label>
          )}
        </div>
      )}

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
          disabled={!issue || Boolean(validationError)}
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

- [ ] **Step 2: Buat `comment-proposal-card.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { useParams } from "next/navigation";
import Link from "next/link";
import type { TIssue } from "@plane/types";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
// lib
import { parseWorkItemRef, textToDescriptionHtml, workItemHref } from "@/lib/ai-work-items";
import {
  validateAddCommentProposal,
  type TAiAddCommentProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
} from "@/lib/ai-proposals";

type Props = {
  proposal: TAiAddCommentProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const CommentProposalCard = observer(function CommentProposalCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: Props) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
  const [comment, setComment] = useState(proposal.comment);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceSlug) return;
    const ref = parseWorkItemRef(proposal.work_item);
    if (!ref) {
      setResolveError("Work item reference is invalid.");
      return;
    }
    let cancelled = false;
    fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId)
      .then((fetched) => {
        if (!cancelled) setIssue(fetched);
      })
      .catch(() => {
        if (!cancelled) setResolveError("Work item not found or not accessible.");
      });
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, proposal.work_item, fetchIssueWithIdentifier]);

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Comment added.{" "}
        {workspaceSlug && issue && (
          <Link
            href={workItemHref(workspaceSlug, issue.project_id, issue.id)}
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
        Comment cancelled.
      </p>
    );
  }

  const previewHtml = textToDescriptionHtml(comment) ?? "";
  const validationError = issue ? validateAddCommentProposal({ work_item: proposal.work_item, comment }) : null;

  const confirm = async () => {
    if (!issue || !previewHtml) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "add_comment",
        projectId: issue.project_id,
        issueId: issue.id,
        commentHtml: previewHtml,
      });
    } catch {
      setError("Could not add the comment. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Comment proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-12 font-medium text-primary">Comment on {proposal.work_item}</p>
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      <textarea
        className="mt-2 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
        rows={3}
        value={comment}
        onChange={(event) => setComment(event.target.value)}
      />
      {previewHtml && (
        <div
          className="ai-prose mt-2 rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          dangerouslySetInnerHTML={{ __html: previewHtml }}
        />
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || Boolean(validationError)}
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

- [ ] **Step 3: Wire ke `root.tsx`**

Tambahkan import:

```tsx
import { CommentProposalCard } from "./proposals/comment-proposal-card";
import { WorkItemUpdateProposalCard } from "./proposals/work-item-update-proposal-card";
```

Tambahkan `confirmProposal, cancelProposal,` ke destructuring `useAiAssistant()` setelah `resolveWorkItemProposal`.

Tambahkan blok render setelah `{message.workItemProposals?.map(...)}`:

```tsx
{
  message.proposals?.map((entry) => {
    if (entry.kind === "update_work_item") {
      return (
        <WorkItemUpdateProposalCard
          key={entry.key}
          proposal={entry.proposal}
          decision={message.proposalDecisions?.[entry.key]}
          onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
          onCancel={() => cancelProposal(message.id, entry.key)}
        />
      );
    }
    return (
      <CommentProposalCard
        key={entry.key}
        proposal={entry.proposal}
        decision={message.proposalDecisions?.[entry.key]}
        onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
        onCancel={() => cancelProposal(message.id, entry.key)}
      />
    );
  });
}
```

- [ ] **Step 4: Verifikasi tipe**

Run: `pnpm --filter web check:types`
Expected: lulus tanpa error baru.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/proposals apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): render update and comment proposal cards"
```

---

### Task 11: Verifikasi akhir Fase 2A

- [ ] **Step 1: Seluruh test Rust**

Run (dari `apps/api-rs`): `cargo test -p ai`
Expected: seluruh test lulus.

Run (dari `apps/api-rs`, butuh DB dev): `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus.

- [ ] **Step 2: Build backend**

Run (dari `apps/api-rs`): `cargo build -p api -p worker`
Expected: `Finished`.

- [ ] **Step 3: Seluruh test FE + tipe + lint**

Run (dari `apps/web`): `pnpm test`
Expected: seluruh test vitest lulus.

Run (dari `apps/web`): `pnpm check:types`
Expected: lulus.

Run (dari `apps/web`): `pnpm check:lint`
Expected: lulus (jumlah warning tidak bertambah).

- [ ] **Step 4: Format**

Run (dari root): `pnpm fix`
Expected: tidak ada perubahan tak terduga; commit bila formatter mengubah file:

```bash
git add -A
git commit -m "chore: format fase 2a"
```

- [ ] **Step 5: Smoke manual (opsional, butuh stack hidup)**

Ikuti AGENTS.md: rebuild `pnpm --filter=web build`, restart `plane-web-prod.service`, lalu di UI Agent mode:

1. "ubah LTS-1 priority jadi high" → kartu Update muncul dengan diff priority; Confirm → work item berubah; reload halaman → kartu menampilkan "Work item updated".
2. "komentari LTS-1: filter sudah diganti" → kartu Comment muncul dengan preview; Confirm → komentar tampil di work item.
3. "update schedule laporan harian" (scheduled run) → pastikan proposal tool tidak tersedia untuk run terjadwal.

---

## Self-Review Checklist (sudah dijalankan)

- **Spec coverage:** §4.4 metadata generik + decision → Task 4, 5, 8, 9; §6.1 `update_work_item`/`add_comment` → Task 1, 2; §6.2 result `created_comment_id` → Task 2, 5, 9; §6.3 kartu → Task 10; §6.4 error handling → kartu + store (decision tetap pending saat gagal); §6.5 PREAMBLE → Task 3; §8 testing → unit + integrasi + FE.
- **Placeholder scan:** tidak ada TODO/TBD; semua langkah berisi kode lengkap.
- **Type consistency:** `UpdateWorkItemChanges`/`TAiWorkItemChanges`, `TAiProposalDecision`/`proposal_decisions`, `mutation_kind_for_tool`/`proposal_metadata`, `confirmProposal(messageId, key, payload)` konsisten antar task. Plan 2B akan menambah 3 kind ke `mutation_kind_for_tool`, `allowed_result_keys`, `PROPOSAL_TOOL_NAMES`, `PROPOSAL_KINDS`, dan `TAiProposalConfirmPayload`.
