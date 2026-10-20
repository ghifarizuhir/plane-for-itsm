# Galileo Tools Fase 2B — Link Mutations (Services, Sprints, Tracks) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah tiga proposal tool Fase 2 — `manage_service_links`, `manage_sprint_items`, `manage_track_items` — plus kartu generik `LinkItemsProposalCard` yang me-resolve nama → id di FE dan mengeksekusi link/unlink lewat endpoint REST existing saat Confirm.

**Architecture:** Semua tool hidup di `tools/mutations.rs` (lanjutan Plan 2A) dan hanya memvalidasi bentuk + mencatat trace. Fondasi metadata generik (`proposals`/`proposal_decisions`, store `confirmProposal`) sudah ada dari Plan 2A; plan ini memperluas allowlist kind di tiga tempat backend dan menambah dispatch arm di FE. Kartu generik memakai data store yang sudah dimuat (services/cycles/modules) dan `TIssue.cycle_id`/`module_ids` untuk deteksi no-op.

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), React + MobX + vitest.

**Prasyarat:** Plan 2A sudah dieksekusi (metadata generik, store `confirmProposal`, `PROPOSAL_TOOL_NAMES` berisi 4 entri).

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4.4, §4.5, §4.7, §6.1 tiga tool link, §6.2, §6.3 `LinkItemsProposalCard`, §6.4).

---

## File Structure

| File                                                                                   | Aksi | Tanggung jawab                                                     |
| -------------------------------------------------------------------------------------- | ---- | ------------------------------------------------------------------ |
| `apps/api-rs/crates/ai/src/tools/mod.rs`                                               | Ubah | `enum_arg`, `PROPOSAL_TOOL_NAMES` 7 entri, registrasi 3 tool, test |
| `apps/api-rs/crates/ai/src/tools/mutations.rs`                                         | Ubah | 3 tool link + normalisasi + `mutation_kind_for_tool`               |
| `apps/api-rs/crates/ai/src/agent.rs`                                                   | Ubah | `PREAMBLE` link + test `pending_actions`                           |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                                | Ubah | Allowlist kind `proposal_decisions` + test                         |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`                                        | Ubah | Roundtrip 3 kind                                                   |
| `apps/web/core/lib/ai-proposals.ts`                                                    | Ubah | 3 kind baru, validator, `matchByNameOrId`                          |
| `apps/web/core/lib/ai-proposals.test.ts`                                               | Ubah | Test validator + matcher                                           |
| `apps/web/core/store/ai-assistant.store.ts`                                            | Ubah | Dispatch arm 3 kind + 2 service baru                               |
| `apps/web/core/store/ai-assistant.store.test.ts`                                       | Ubah | Test dispatch                                                      |
| `apps/web/core/components/ai/assistant-sidebar/proposals/link-items-proposal-card.tsx` | Baru | Kartu generik service/sprint/track                                 |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`                               | Ubah | Render `LinkItemsProposalCard`                                     |

---

### Task 1: Helper `enum_arg` + tool `manage_service_links`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
    #[test]
    fn enum_arg_normalizes_and_rejects() {
        assert_eq!(
            enum_arg(" Link ", &["link", "unlink"], "action").unwrap(),
            "link"
        );
        let err = enum_arg("attach", &["link", "unlink"], "action").unwrap_err();
        assert!(err.to_string().contains("link, unlink"));
    }
```

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn service_links_normalize_and_validate() {
        let proposal = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: " LTS-7 ".into(),
            services: vec![" Email ".into(), "email".into(), "VPN".into()],
            action: " LINK ".into(),
        })
        .unwrap();
        assert_eq!(proposal.work_item, "LTS-7");
        assert_eq!(
            proposal.services,
            vec!["Email".to_string(), "VPN".to_string()]
        );
        assert_eq!(proposal.action, "link");

        let no_services = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: "LTS-7".into(),
            services: vec![],
            action: "link".into(),
        })
        .unwrap_err();
        assert!(no_services.to_string().contains("service"));

        let too_many = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: "LTS-7".into(),
            services: (0..=MAX_SERVICE_REFS)
                .map(|index| format!("svc-{index}"))
                .collect(),
            action: "link".into(),
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("at most"));

        let bad_action = manage_service_links_proposal_from_args(ManageServiceLinksArgs {
            work_item: "LTS-7".into(),
            services: vec!["Email".into()],
            action: "attach".into(),
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("link, unlink"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p ai service_links_normalize`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi `enum_arg`**

Tambahkan di `tools/mod.rs` setelah `priority_arg`:

```rust
/// Normalize a small enum argument (e.g. `action`) against an allowlist.
pub fn enum_arg(
    value: &str,
    allowed: &[&str],
    label: &str,
) -> Result<String, ToolExecutionError> {
    let normalized = value.trim().to_ascii_lowercase();
    if allowed.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "{label} must be one of: {}",
            allowed.join(", ")
        )))
    }
}
```

- [ ] **Step 4: Implementasi tool**

Tambahkan di `mutations.rs` setelah `AddComment`:

```rust
pub const MANAGE_SERVICE_LINKS_NAME: &str = "manage_service_links";
pub const MAX_SERVICE_REFS: usize = 10;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageServiceLinksArgs {
    /// Work item identifier like "LTS-42". Required.
    pub work_item: String,
    /// Service names or ids (1-10).
    pub services: Vec<String>,
    /// One of: link, unlink.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageServiceLinksProposal {
    pub work_item: String,
    pub services: Vec<String>,
    pub action: String,
}

/// Validate raw tool args into a normalized service link proposal.
pub fn manage_service_links_proposal_from_args(
    args: ManageServiceLinksArgs,
) -> Result<ManageServiceLinksProposal, ToolExecutionError> {
    let work_item = args.work_item.trim();
    if work_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("work_item is required"));
    }
    parse_work_item_ref(work_item)?;
    let action = enum_arg(&args.action, &["link", "unlink"], "action")?;
    if args.services.is_empty() {
        return Err(ToolExecutionError::invalid_args(
            "at least one service is required",
        ));
    }
    if args.services.len() > MAX_SERVICE_REFS {
        return Err(ToolExecutionError::invalid_args(format!(
            "at most {MAX_SERVICE_REFS} services are allowed"
        )));
    }
    let services = bounded_ref_list(Some(args.services), "services")?;
    Ok(ManageServiceLinksProposal {
        work_item: work_item.to_string(),
        services,
        action,
    })
}

pub struct ManageServiceLinks {
    pub trace: ToolTrace,
}

impl Tool for ManageServiceLinks {
    const NAME: &'static str = MANAGE_SERVICE_LINKS_NAME;
    type Args = ManageServiceLinksArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose linking or unlinking services to one work item. Only call this \
         when the user asks to attach or detach services on a work item. The work \
         item identifier and at least one service name or id are required; never \
         guess them. The user must confirm the card in the UI before anything is \
         saved. Never claim the links were changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageServiceLinksArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_service_links_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageServiceLinksProposal serializes"))
    }
}
```

Perbarui import di `mutations.rs`:

```rust
use super::{
    bounded_ref_list, bounded_ref_list_opt, enum_arg, parse_iso_date,
    parse_work_item_ref, priority_arg, schema_of, WORK_ITEM_DESCRIPTION_MAX, WORK_ITEM_NAME_MAX,
    WORK_ITEM_STATE_MAX,
};
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai enum_arg service_links_normalize`
Expected: lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add manage_service_links proposal tool"
```

---

### Task 2: Tool `manage_sprint_items`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn sprint_items_normalize_and_validate() {
        let proposal = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: " Sprint 3 ".into(),
            project: Some(" LTS ".into()),
            work_items: vec![" lts-1 ".into(), "LTS-1".into(), "LTS-2".into()],
            action: " Add ".into(),
        })
        .unwrap();
        assert_eq!(proposal.sprint, "Sprint 3");
        assert_eq!(proposal.project.as_deref(), Some("LTS"));
        assert_eq!(
            proposal.work_items,
            vec!["lts-1".to_string(), "LTS-2".to_string()]
        );
        assert_eq!(proposal.action, "add");

        let blank_sprint = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "   ".into(),
            project: None,
            work_items: vec!["LTS-1".into()],
            action: "add".into(),
        })
        .unwrap_err();
        assert!(blank_sprint.to_string().contains("sprint"));

        let empty_items = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: vec![],
            action: "add".into(),
        })
        .unwrap_err();
        assert!(empty_items.to_string().contains("work item"));

        let bad_ref = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: vec!["nope".into()],
            action: "add".into(),
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));

        let too_many = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: (0..=MAX_BULK_ITEMS)
                .map(|index| format!("LTS-{index}"))
                .collect(),
            action: "add".into(),
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("at most"));

        let bad_action = manage_sprint_items_proposal_from_args(ManageSprintItemsArgs {
            sprint: "Sprint 3".into(),
            project: None,
            work_items: vec!["LTS-1".into()],
            action: "attach".into(),
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("add, remove"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai sprint_items_normalize`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `ManageServiceLinks`:

```rust
pub const MANAGE_SPRINT_ITEMS_NAME: &str = "manage_sprint_items";
pub const MAX_BULK_ITEMS: usize = 25;
pub const CONTAINER_REF_MAX: usize = 255;
pub const CONTAINER_PROJECT_MAX: usize = 100;

fn required_text(
    value: &str,
    label: &str,
    max: usize,
) -> Result<String, ToolExecutionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} is required"
        )));
    }
    if trimmed.chars().count() > max {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must be at most {max} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn optional_container_project(
    value: Option<String>,
) -> Result<Option<String>, ToolExecutionError> {
    match value.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(project) if project.chars().count() > CONTAINER_PROJECT_MAX => {
            Err(ToolExecutionError::invalid_args(format!(
                "project must be at most {CONTAINER_PROJECT_MAX} characters"
            )))
        }
        Some(project) => Ok(Some(project.to_string())),
    }
}

/// Validate and dedupe a bulk list of `PROJ-123` references.
pub fn work_item_refs_arg(
    values: Vec<String>,
) -> Result<Vec<String>, ToolExecutionError> {
    if values.is_empty() {
        return Err(ToolExecutionError::invalid_args(
            "at least one work item is required",
        ));
    }
    if values.len() > MAX_BULK_ITEMS {
        return Err(ToolExecutionError::invalid_args(format!(
            "at most {MAX_BULK_ITEMS} work items are allowed"
        )));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let trimmed = value.trim();
        parse_work_item_ref(trimmed)?;
        let key = trimmed.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(trimmed.to_string());
    }
    Ok(out)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageSprintItemsArgs {
    /// Sprint name or id. Required.
    pub sprint: String,
    /// Project identifier or name; use it when the sprint name is not unique.
    pub project: Option<String>,
    /// Work item identifiers like "LTS-42" (1-25).
    pub work_items: Vec<String>,
    /// One of: add, remove.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageSprintItemsProposal {
    pub sprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub work_items: Vec<String>,
    pub action: String,
}

/// Validate raw tool args into a normalized sprint items proposal.
pub fn manage_sprint_items_proposal_from_args(
    args: ManageSprintItemsArgs,
) -> Result<ManageSprintItemsProposal, ToolExecutionError> {
    let sprint = required_text(&args.sprint, "sprint", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["add", "remove"], "action")?;
    let work_items = work_item_refs_arg(args.work_items)?;
    Ok(ManageSprintItemsProposal {
        sprint,
        project,
        work_items,
        action,
    })
}

pub struct ManageSprintItems {
    pub trace: ToolTrace,
}

impl Tool for ManageSprintItems {
    const NAME: &'static str = MANAGE_SPRINT_ITEMS_NAME;
    type Args = ManageSprintItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose adding or removing work items in a sprint. Only call this when \
         the user asks to plan or change sprint scope. The sprint name (or id) \
         and at least one work item identifier are required; never guess them. \
         Pass the project when the sprint name is not unique. The user must \
         confirm the card in the UI before anything is saved. Never claim the \
         sprint was changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageSprintItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_sprint_items_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageSprintItemsProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai sprint_items_normalize`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add manage_sprint_items proposal tool"
```

---

### Task 3: Tool `manage_track_items`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn track_items_normalize_and_validate() {
        let proposal = manage_track_items_proposal_from_args(ManageTrackItemsArgs {
            track: " Onboarding ".into(),
            project: None,
            work_items: vec!["LTS-9".into()],
            action: " REMOVE ".into(),
        })
        .unwrap();
        assert_eq!(proposal.track, "Onboarding");
        assert_eq!(proposal.project, None);
        assert_eq!(proposal.work_items, vec!["LTS-9".to_string()]);
        assert_eq!(proposal.action, "remove");

        let blank_track = manage_track_items_proposal_from_args(ManageTrackItemsArgs {
            track: "  ".into(),
            project: None,
            work_items: vec!["LTS-9".into()],
            action: "remove".into(),
        })
        .unwrap_err();
        assert!(blank_track.to_string().contains("track"));

        let bad_action = manage_track_items_proposal_from_args(ManageTrackItemsArgs {
            track: "Onboarding".into(),
            project: None,
            work_items: vec!["LTS-9".into()],
            action: "detach".into(),
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("add, remove"));
    }

    #[test]
    fn mutation_kind_mapping_covers_link_tools() {
        assert_eq!(
            mutation_kind_for_tool(MANAGE_SERVICE_LINKS_NAME),
            Some("manage_service_links")
        );
        assert_eq!(
            mutation_kind_for_tool(MANAGE_SPRINT_ITEMS_NAME),
            Some("manage_sprint_items")
        );
        assert_eq!(
            mutation_kind_for_tool(MANAGE_TRACK_ITEMS_NAME),
            Some("manage_track_items")
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai track_items_normalize`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `ManageSprintItems`:

```rust
pub const MANAGE_TRACK_ITEMS_NAME: &str = "manage_track_items";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageTrackItemsArgs {
    /// Track (module) name or id. Required.
    pub track: String,
    /// Project identifier or name; use it when the track name is not unique.
    pub project: Option<String>,
    /// Work item identifiers like "LTS-42" (1-25).
    pub work_items: Vec<String>,
    /// One of: add, remove.
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManageTrackItemsProposal {
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub work_items: Vec<String>,
    pub action: String,
}

/// Validate raw tool args into a normalized track items proposal.
pub fn manage_track_items_proposal_from_args(
    args: ManageTrackItemsArgs,
) -> Result<ManageTrackItemsProposal, ToolExecutionError> {
    let track = required_text(&args.track, "track", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["add", "remove"], "action")?;
    let work_items = work_item_refs_arg(args.work_items)?;
    Ok(ManageTrackItemsProposal {
        track,
        project,
        work_items,
        action,
    })
}

pub struct ManageTrackItems {
    pub trace: ToolTrace,
}

impl Tool for ManageTrackItems {
    const NAME: &'static str = MANAGE_TRACK_ITEMS_NAME;
    type Args = ManageTrackItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose adding or removing work items in a track. Only call this when \
         the user asks to organize work into tracks. The track name (or id) and \
         at least one work item identifier are required; never guess them. Pass \
         the project when the track name is not unique. The user must confirm \
         the card in the UI before anything is saved. Never claim the track was \
         changed until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ManageTrackItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = manage_track_items_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ManageTrackItemsProposal serializes"))
    }
}
```

Perbarui `mutation_kind_for_tool`:

```rust
pub fn mutation_kind_for_tool(name: &str) -> Option<&'static str> {
    match name {
        UPDATE_WORK_ITEM_NAME => Some("update_work_item"),
        ADD_COMMENT_NAME => Some("add_comment"),
        MANAGE_SERVICE_LINKS_NAME => Some("manage_service_links"),
        MANAGE_SPRINT_ITEMS_NAME => Some("manage_sprint_items"),
        MANAGE_TRACK_ITEMS_NAME => Some("manage_track_items"),
        _ => None,
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai track_items_normalize mutation_kind_mapping`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add manage_track_items proposal tool"
```

---

### Task 4: Registry, `PROPOSAL_TOOL_NAMES`, PREAMBLE

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
    #[tokio::test]
    async fn link_tools_expose_metadata() {
        let links = ManageServiceLinks {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(ManageServiceLinks::NAME, "manage_service_links");
        assert!(links.parameters()["properties"]["services"].is_object());
        assert!(!links.description().is_empty());

        let sprint = ManageSprintItems {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(ManageSprintItems::NAME, "manage_sprint_items");
        assert!(sprint.parameters()["properties"]["work_items"].is_object());
        assert!(!sprint.description().is_empty());

        let track = ManageTrackItems {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(ManageTrackItems::NAME, "manage_track_items");
        assert!(track.parameters()["properties"]["track"].is_object());
        assert!(!track.description().is_empty());
    }
```

Tambahkan di `mod tests` `agent.rs`:

```rust
    #[test]
    fn pending_actions_covers_link_mutations() {
        let trace = new_trace();
        record(
            &trace,
            crate::tools::MANAGE_SERVICE_LINKS_NAME,
            &json!({"work_item": "LTS-1", "services": ["Email"], "action": "link"}),
        );
        record(
            &trace,
            crate::tools::MANAGE_SPRINT_ITEMS_NAME,
            &json!({"sprint": "Sprint 3", "work_items": ["LTS-1"], "action": "add"}),
        );
        record(
            &trace,
            crate::tools::MANAGE_TRACK_ITEMS_NAME,
            &json!({"track": "Onboarding", "work_items": ["LTS-1"], "action": "remove"}),
        );
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0]["kind"], json!("manage_service_links"));
        assert_eq!(actions[1]["kind"], json!("manage_sprint_items"));
        assert_eq!(actions[2]["kind"], json!("manage_track_items"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pending_actions_covers_link`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Di `tools/mod.rs`, perbarui `PROPOSAL_TOOL_NAMES` menjadi:

```rust
/// Proposal tool names: recorded in the trace but never executed against the
/// database and never schedule-eligible.
pub const PROPOSAL_TOOL_NAMES: [&str; 7] = [
    CREATE_SCHEDULE_NAME,
    CREATE_WORK_ITEM_NAME,
    UPDATE_WORK_ITEM_NAME,
    ADD_COMMENT_NAME,
    MANAGE_SERVICE_LINKS_NAME,
    MANAGE_SPRINT_ITEMS_NAME,
    MANAGE_TRACK_ITEMS_NAME,
];
```

Di `workspace_tools`, setelah `.tool(AddComment { ... })` dan sebelum `.tool(CreateSchedule { ... })`:

```rust
        .tool(ManageServiceLinks {
            trace: trace.clone(),
        })
        .tool(ManageSprintItems {
            trace: trace.clone(),
        })
        .tool(ManageTrackItems {
            trace: trace.clone(),
        })
```

Di `agent.rs`, tambahkan setelah paragraf update/comment di `PREAMBLE`:

```text
To link or unlink services on a work item, call manage_service_links with the \
work item identifier and service names or ids. To add or remove work items in \
a sprint or track, call manage_sprint_items or manage_track_items with the \
container name and the work item identifiers. These are proposals too: the \
user must confirm them in the UI first, so never claim they were applied \
before that confirmation.
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): register link proposal tools"
```

---

### Task 5: Allowlist kind link di `clean_proposal_decisions`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_conversations.rs`:

```rust
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
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --lib metadata_patch_accepts_link`
Expected: gagal (`kind not allowed: manage_service_links`).

- [ ] **Step 3: Implementasi**

Perbarui `allowed_result_keys`:

```rust
fn allowed_result_keys(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "update_work_item"
        | "manage_service_links"
        | "manage_sprint_items"
        | "manage_track_items" => Some(&[]),
        "add_comment" => Some(&["created_comment_id"]),
        _ => None,
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --lib metadata_patch`
Expected: seluruh test metadata lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): accept link proposal decisions"
```

---

### Task 6: Integration test roundtrip tiga kind

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tambahkan tiga test DB-backed**

Tambahkan di akhir file (helper `tool_roundtrip::tool_roundtrip_url` dari Plan 2A):

```rust
#[tokio::test]
async fn manage_service_links_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "manage_service_links",
        r#"{"work_item":"LTS-42","services":["Email"],"action":"link"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "link Email to LTS-42",
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
    assert_eq!(proposals[0]["kind"], json!("manage_service_links"));
    assert_eq!(proposals[0]["proposal"]["services"][0], json!("Email"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "manage_service_links", "decision": "applied"}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["decision"],
        json!("applied")
    );

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn manage_sprint_items_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "manage_sprint_items",
        r#"{"sprint":"Sprint 3","work_items":["LTS-42"],"action":"add"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "add LTS-42 to Sprint 3",
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
    assert_eq!(proposals[0]["kind"], json!("manage_sprint_items"));
    assert_eq!(proposals[0]["proposal"]["action"], json!("add"));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn manage_track_items_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "manage_track_items",
        r#"{"track":"Onboarding","work_items":["LTS-42"],"action":"remove"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "remove LTS-42 from Onboarding",
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
    assert_eq!(proposals[0]["kind"], json!("manage_track_items"));
    assert_eq!(proposals[0]["proposal"]["action"], json!("remove"));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan test**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus (butuh DB dev + Redis).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): roundtrip link proposal metadata"
```

---

### Task 7: FE `ai-proposals.ts` — 3 kind baru + `matchByNameOrId`

**Files:**

- Modify: `apps/web/core/lib/ai-proposals.ts`
- Modify: `apps/web/core/lib/ai-proposals.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `ai-proposals.test.ts`:

```ts
import {
  matchByNameOrId,
  validateManageServiceLinksProposal,
  validateManageSprintItemsProposal,
  validateManageTrackItemsProposal,
} from "./ai-proposals";

describe("link proposal validators", () => {
  it("validates service link proposals", () => {
    expect(validateManageServiceLinksProposal({ work_item: "LTS-1", services: ["Email"], action: "link" })).toBeNull();
    expect(validateManageServiceLinksProposal({ work_item: "LTS-1", services: [], action: "link" })).toBe(
      "At least one service is required."
    );
    expect(
      validateManageServiceLinksProposal({
        work_item: "LTS-1",
        services: ["Email"],
        action: "attach" as never,
      })
    ).toBe("Action must be link or unlink.");
    expect(validateManageServiceLinksProposal({ services: ["Email"], action: "link" })).toBe("Work item is required.");
  });

  it("validates sprint and track item proposals", () => {
    expect(validateManageSprintItemsProposal({ sprint: "Sprint 3", work_items: ["LTS-1"], action: "add" })).toBeNull();
    expect(validateManageSprintItemsProposal({ sprint: "Sprint 3", work_items: [], action: "add" })).toBe(
      "At least one work item is required."
    );
    expect(
      validateManageTrackItemsProposal({ track: "Onboarding", work_items: ["LTS-1"], action: "remove" })
    ).toBeNull();
    expect(validateManageTrackItemsProposal({ work_items: ["LTS-1"], action: "remove" })).toBe("Track is required.");
  });
});

describe("matchByNameOrId", () => {
  const items = [
    { id: "1", name: "Email" },
    { id: "2", name: "VPN" },
  ];

  it("matches by id first, then exact name case-insensitively", () => {
    expect(matchByNameOrId(items, "2")?.name).toBe("VPN");
    expect(matchByNameOrId(items, " email ")?.id).toBe("1");
    expect(matchByNameOrId(items, "nope")).toBeUndefined();
    expect(matchByNameOrId(items, "  ")).toBeUndefined();
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/web`): `pnpm test core/lib/ai-proposals.test.ts`
Expected: gagal kompilasi/import.

- [ ] **Step 3: Implementasi**

Di `ai-proposals.ts`, perbarui `PROPOSAL_KINDS`:

```ts
export const PROPOSAL_KINDS = [
  "update_work_item",
  "add_comment",
  "manage_service_links",
  "manage_sprint_items",
  "manage_track_items",
] as const;
```

Tambahkan limit:

```ts
export const PROPOSAL_LIMITS = {
  comment: 5000,
  name: 255,
  description: 5000,
  refs: 10,
  services: 10,
  items: 25,
} as const;
```

Tambahkan tipe payload:

```ts
export type TAiManageServiceLinksProposal = {
  work_item: string;
  services: string[];
  action: "link" | "unlink";
};

export type TAiManageSprintItemsProposal = {
  sprint: string;
  project?: string | null;
  work_items: string[];
  action: "add" | "remove";
};

export type TAiManageTrackItemsProposal = {
  track: string;
  project?: string | null;
  work_items: string[];
  action: "add" | "remove";
};
```

Perbarui union `TAiProposal`:

```ts
export type TAiProposal =
  | { key: string; kind: "update_work_item"; proposal: TAiUpdateWorkItemProposal }
  | { key: string; kind: "add_comment"; proposal: TAiAddCommentProposal }
  | { key: string; kind: "manage_service_links"; proposal: TAiManageServiceLinksProposal }
  | { key: string; kind: "manage_sprint_items"; proposal: TAiManageSprintItemsProposal }
  | { key: string; kind: "manage_track_items"; proposal: TAiManageTrackItemsProposal };
```

Perbarui `TAiProposalConfirmPayload`:

```ts
export type TAiProposalConfirmPayload =
  | { kind: "update_work_item"; projectId: string; issueId: string; changes: Partial<TIssue> }
  | { kind: "add_comment"; projectId: string; issueId: string; commentHtml: string }
  | {
      kind: "manage_service_links";
      projectId: string;
      issueId: string;
      action: "link" | "unlink";
      links: { serviceId: string; linkId?: string }[];
    }
  | {
      kind: "manage_sprint_items";
      projectId: string;
      cycleId: string;
      action: "add" | "remove";
      issueIds: string[];
    }
  | {
      kind: "manage_track_items";
      projectId: string;
      moduleId: string;
      action: "add" | "remove";
      issueIds: string[];
    };
```

Tambahkan validator + matcher di akhir file:

```ts
export type TNamedRef = { id: string; name: string };

/** Match a reference to an item by exact id or case-insensitive exact name. */
export const matchByNameOrId = (items: TNamedRef[], reference: string): TNamedRef | undefined => {
  const trimmed = reference.trim();
  if (!trimmed) return undefined;
  const byId = items.find((item) => item.id === trimmed);
  if (byId) return byId;
  const needle = trimmed.toLowerCase();
  return items.find((item) => item.name.toLowerCase() === needle);
};

/** Mirrors `manage_service_links_proposal_from_args` on the backend. */
export const validateManageServiceLinksProposal = (proposal: Partial<TAiManageServiceLinksProposal>): string | null => {
  if (!proposal.work_item?.trim()) return "Work item is required.";
  if (proposal.action !== "link" && proposal.action !== "unlink") return "Action must be link or unlink.";
  const services = (proposal.services ?? []).map((service) => service.trim()).filter(Boolean);
  if (services.length === 0) return "At least one service is required.";
  if (services.length > PROPOSAL_LIMITS.services) return `At most ${PROPOSAL_LIMITS.services} services are allowed.`;
  return null;
};

/** Mirrors `manage_sprint_items_proposal_from_args` on the backend. */
export const validateManageSprintItemsProposal = (proposal: Partial<TAiManageSprintItemsProposal>): string | null => {
  if (!proposal.sprint?.trim()) return "Sprint is required.";
  if (proposal.action !== "add" && proposal.action !== "remove") return "Action must be add or remove.";
  const items = proposal.work_items ?? [];
  if (items.length === 0) return "At least one work item is required.";
  if (items.length > PROPOSAL_LIMITS.items) return `At most ${PROPOSAL_LIMITS.items} work items are allowed.`;
  return null;
};

/** Mirrors `manage_track_items_proposal_from_args` on the backend. */
export const validateManageTrackItemsProposal = (proposal: Partial<TAiManageTrackItemsProposal>): string | null => {
  if (!proposal.track?.trim()) return "Track is required.";
  if (proposal.action !== "add" && proposal.action !== "remove") return "Action must be add or remove.";
  const items = proposal.work_items ?? [];
  if (items.length === 0) return "At least one work item is required.";
  if (items.length > PROPOSAL_LIMITS.items) return `At most ${PROPOSAL_LIMITS.items} work items are allowed.`;
  return null;
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-proposals.test.ts`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposals.test.ts
git commit -m "feat(web): add link proposal types and matchers"
```

---

### Task 8: Store dispatch tiga kind link

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-assistant.store.test.ts`, tambahkan mock service di `makeServices`:

```ts
  services: {
    linkWorkItem: vi.fn(async () => ({ id: "l1" })),
    unlinkWorkItem: vi.fn(async () => undefined),
    ...overrides.services,
  },
  modules: {
    addIssuesToModule: vi.fn(async () => undefined),
    removeIssuesFromModuleBulk: vi.fn(async () => undefined),
    ...overrides.modules,
  },
```

Perbarui `makeStore` agar meneruskan service keenam dan ketujuh:

```ts
const makeStore = (services = makeServices()) =>
  new AIAssistantStore(
    services.ai as any,
    services.schedules as any,
    services.conversations as any,
    services.issues as any,
    services.comments as any,
    services.services as any,
    services.modules as any
  );
```

Tambahkan di akhir file:

```ts
const serviceLinksMetadata = {
  proposals: [
    {
      key: "k3",
      kind: "manage_service_links",
      proposal: { work_item: "LTS-1", services: ["Email"], action: "link" },
    },
  ],
};

const sprintItemsMetadata = {
  proposals: [
    {
      key: "k4",
      kind: "manage_sprint_items",
      proposal: { sprint: "Sprint 3", work_items: ["LTS-1", "LTS-2"], action: "add" },
    },
  ],
};

const trackItemsMetadata = {
  proposals: [
    {
      key: "k5",
      kind: "manage_track_items",
      proposal: { track: "Onboarding", work_items: ["LTS-1"], action: "remove" },
    },
  ],
};

describe("link proposals", () => {
  const openAgent = async (services = makeServices(), metadata: Record<string, unknown>) => {
    services.conversations.listMessages = vi.fn(async () => [mutationMessage(metadata)]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    return { store, services };
  };

  it("links and unlinks services", async () => {
    const { store, services } = await openAgent(makeServices(), serviceLinksMetadata);
    await store.confirmProposal("srv-assistant", "k3", {
      kind: "manage_service_links",
      projectId: "p1",
      issueId: "i1",
      action: "link",
      links: [{ serviceId: "s1" }],
    });
    expect(services.services.linkWorkItem).toHaveBeenCalledWith("acme", "", "p1", "s1", { id: "i1" });
    expect(store.messages[0].proposalDecisions?.k3).toEqual({
      kind: "manage_service_links",
      decision: "applied",
    });
  });

  it("unlinks a service with its link id", async () => {
    const metadata = {
      proposals: [
        {
          key: "k3b",
          kind: "manage_service_links",
          proposal: { work_item: "LTS-1", services: ["Email"], action: "unlink" },
        },
      ],
    };
    const { store, services } = await openAgent(makeServices(), metadata);
    await store.confirmProposal("srv-assistant", "k3b", {
      kind: "manage_service_links",
      projectId: "p1",
      issueId: "i1",
      action: "unlink",
      links: [{ serviceId: "s1", linkId: "l1" }],
    });
    expect(services.services.unlinkWorkItem).toHaveBeenCalledWith("acme", "", "p1", "l1");
  });

  it("adds work items to a sprint", async () => {
    const { store, services } = await openAgent(makeServices(), sprintItemsMetadata);
    await store.confirmProposal("srv-assistant", "k4", {
      kind: "manage_sprint_items",
      projectId: "p1",
      cycleId: "c1",
      action: "add",
      issueIds: ["i1", "i2"],
    });
    expect(services.issues.addIssueToCycle).toHaveBeenCalledWith("acme", "p1", "c1", { issues: ["i1", "i2"] });
  });

  it("removes work items from a sprint one by one", async () => {
    const metadata = {
      proposals: [
        {
          key: "k4b",
          kind: "manage_sprint_items",
          proposal: { sprint: "Sprint 3", work_items: ["LTS-1", "LTS-2"], action: "remove" },
        },
      ],
    };
    const { store, services } = await openAgent(makeServices(), metadata);
    await store.confirmProposal("srv-assistant", "k4b", {
      kind: "manage_sprint_items",
      projectId: "p1",
      cycleId: "c1",
      action: "remove",
      issueIds: ["i1", "i2"],
    });
    expect(services.issues.removeIssueFromCycle).toHaveBeenCalledTimes(2);
    expect(services.issues.removeIssueFromCycle).toHaveBeenCalledWith("acme", "p1", "c1", "i1");
    expect(services.issues.removeIssueFromCycle).toHaveBeenCalledWith("acme", "p1", "c1", "i2");
  });

  it("adds work items to a track", async () => {
    const metadata = {
      proposals: [
        {
          key: "k5a",
          kind: "manage_track_items",
          proposal: { track: "Onboarding", work_items: ["LTS-1"], action: "add" },
        },
      ],
    };
    const { store, services } = await openAgent(makeServices(), metadata);
    await store.confirmProposal("srv-assistant", "k5a", {
      kind: "manage_track_items",
      projectId: "p1",
      moduleId: "m1",
      action: "add",
      issueIds: ["i1"],
    });
    expect(services.modules.addIssuesToModule).toHaveBeenCalledWith("acme", "p1", "m1", { issues: ["i1"] });
  });

  it("removes work items from a track in bulk", async () => {
    const { store, services } = await openAgent(makeServices(), trackItemsMetadata);
    await store.confirmProposal("srv-assistant", "k5", {
      kind: "manage_track_items",
      projectId: "p1",
      moduleId: "m1",
      action: "remove",
      issueIds: ["i1"],
    });
    expect(services.modules.removeIssuesFromModuleBulk).toHaveBeenCalledWith("acme", "p1", "m1", ["i1"]);
  });
});
```

Tambahkan `addIssueToCycle`/`removeIssueFromCycle` ke mock `issues` di `makeServices`:

```ts
    addIssueToCycle: vi.fn(async () => undefined),
    removeIssueFromCycle: vi.fn(async () => undefined),
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: gagal kompilasi (dispatch kind baru belum ada).

- [ ] **Step 3: Implementasi store**

Tambahkan import:

```ts
import { ModuleService } from "@/services/module.service";
import { ServiceService } from "@/services/service.service";
```

Perbarui pick type:

```ts
type TIssueService = Pick<IssueService, "createIssue" | "patchIssue" | "addIssueToCycle" | "removeIssueFromCycle">;
type TServicesService = Pick<ServiceService, "linkWorkItem" | "unlinkWorkItem">;
type TModulesService = Pick<ModuleService, "addIssuesToModule" | "removeIssuesFromModuleBulk">;
```

Perbarui constructor:

```ts
  constructor(
    private aiService: TAiService = new AIService(),
    private schedulesService: TAiSchedulesService = new AiSchedulesService(),
    private conversationsService: TAiConversationsService = new AiConversationsService(),
    private issuesService: TIssueService = new IssueService(),
    private commentsService: TCommentsService = new IssueCommentService(),
    private servicesService: TServicesService = new ServiceService(),
    private modulesService: TModulesService = new ModuleService()
  ) {
```

Di `confirmProposal`, ganti blok `if (payload.kind === "update_work_item") { ... } else { ... }` menjadi rantai lengkap:

```ts
let result: TAiProposalDecisionResult | undefined;
if (payload.kind === "update_work_item") {
  await this.issuesService.patchIssue(slug, payload.projectId, payload.issueId, payload.changes);
} else if (payload.kind === "add_comment") {
  const created = await this.commentsService.createIssueComment(slug, payload.projectId, payload.issueId, {
    comment_html: payload.commentHtml,
  });
  if (!created?.id) throw new Error("Comment creation returned no id");
  result = { created_comment_id: created.id };
} else if (payload.kind === "manage_service_links") {
  for (const link of payload.links) {
    if (payload.action === "link") {
      await this.servicesService.linkWorkItem(slug, "", payload.projectId, link.serviceId, {
        id: payload.issueId,
      });
    } else {
      if (!link.linkId) throw new Error("Unlink target is missing its link id");
      await this.servicesService.unlinkWorkItem(slug, "", payload.projectId, link.linkId);
    }
  }
} else if (payload.kind === "manage_sprint_items") {
  if (payload.action === "add") {
    await this.issuesService.addIssueToCycle(slug, payload.projectId, payload.cycleId, {
      issues: payload.issueIds,
    });
  } else {
    for (const issueId of payload.issueIds) {
      await this.issuesService.removeIssueFromCycle(slug, payload.projectId, payload.cycleId, issueId);
    }
  }
} else if (payload.action === "add") {
  await this.modulesService.addIssuesToModule(slug, payload.projectId, payload.moduleId, {
    issues: payload.issueIds,
  });
} else {
  await this.modulesService.removeIssuesFromModuleBulk(slug, payload.projectId, payload.moduleId, payload.issueIds);
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: seluruh test store lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): dispatch link proposals"
```

---

### Task 9: `LinkItemsProposalCard` + render root

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/link-items-proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Buat `link-items-proposal-card.tsx`**

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
import { useCycle } from "@/hooks/store/use-cycle";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { matchProject, parseWorkItemRef, workItemHref } from "@/lib/ai-work-items";
import {
  matchByNameOrId,
  validateManageServiceLinksProposal,
  validateManageSprintItemsProposal,
  validateManageTrackItemsProposal,
  type TAiManageServiceLinksProposal,
  type TAiManageSprintItemsProposal,
  type TAiManageTrackItemsProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TNamedRef,
} from "@/lib/ai-proposals";

type LinkProposalEntry =
  | { kind: "manage_service_links"; proposal: TAiManageServiceLinksProposal }
  | { kind: "manage_sprint_items"; proposal: TAiManageSprintItemsProposal }
  | { kind: "manage_track_items"; proposal: TAiManageTrackItemsProposal };

type Props = LinkProposalEntry & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

type ServiceLinksProps = Extract<Props, { kind: "manage_service_links" }>;
type ContainerProps = Extract<Props, { kind: "manage_sprint_items" | "manage_track_items" }>;

export const LinkItemsProposalCard = (props: Props) => {
  if (props.kind === "manage_service_links") return <ServiceLinksCard {...props} />;
  return <ContainerItemsCard {...props} />;
};

type ServiceLinkRow = {
  reference: string;
  serviceId: string | null;
  linkId: string | null;
  status: "actionable" | "skipped" | "missing";
};

const ServiceLinksCard = observer(function ServiceLinksCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: ServiceLinksProps) {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { currentWorkspace } = useWorkspace();
  const workspaceId = currentWorkspace?.id;
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { fetchServices, getProjectServiceIds, getServiceById, getWorkItemLinksByService } = useService();
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
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
    if (!workspaceSlug || !workspaceId || !projectId) return;
    void fetchServices(workspaceSlug, workspaceId, projectId);
  }, [workspaceSlug, workspaceId, projectId, fetchServices]);

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Service links updated.{" "}
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
        Service link update cancelled.
      </p>
    );
  }

  const serviceIds = projectId ? (getProjectServiceIds(projectId) ?? []) : [];
  const services: TNamedRef[] = serviceIds.flatMap((id) => {
    const service = getServiceById(id);
    return service ? [{ id: service.id, name: service.name }] : [];
  });
  const rows: ServiceLinkRow[] = proposal.services.map((reference) => {
    const service = matchByNameOrId(services, reference);
    if (!service || !issue) return { reference, serviceId: null, linkId: null, status: "missing" };
    const link = getWorkItemLinksByService(service.id).find((candidate) => candidate.issue_id === issue.id);
    const isActionable = proposal.action === "link" ? !link : Boolean(link);
    return {
      reference,
      serviceId: service.id,
      linkId: link?.id ?? null,
      status: isActionable ? "actionable" : "skipped",
    };
  });
  const actionable = rows.filter((row) => row.status === "actionable");
  const validationError = issue ? validateManageServiceLinksProposal(proposal) : null;

  const confirm = async () => {
    if (!issue || !projectId || actionable.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const links = actionable.map((row) => ({
        serviceId: row.serviceId as string,
        ...(row.linkId ? { linkId: row.linkId } : {}),
      }));
      await onConfirm({
        kind: "manage_service_links",
        projectId,
        issueId: issue.id,
        action: proposal.action,
        links,
      });
    } catch {
      setError("Could not update the service links. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Service link proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {proposal.action === "link" ? "Link services to" : "Unlink services from"} {proposal.work_item}
      </p>
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      <ul className="mt-2 flex flex-col gap-1">
        {rows.map((row) => (
          <li key={row.reference} className="text-12 text-secondary">
            {row.reference}
            {row.status === "missing" && <span className="text-tertiary"> — not found in this project, skipped</span>}
            {row.status === "skipped" && (
              <span className="text-tertiary">
                {" "}
                — {proposal.action === "link" ? "already linked" : "not linked"}, skipped
              </span>
            )}
          </li>
        ))}
      </ul>
      {actionable.length === 0 && !resolveError && <p className="mt-1 text-12 text-tertiary">Nothing to change.</p>}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || actionable.length === 0 || Boolean(validationError)}
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

type ContainerRow = {
  reference: string;
  issueId: string | null;
  status: "actionable" | "skipped" | "missing" | "other_project";
};

const ContainerItemsCard = observer(function ContainerItemsCard(props: ContainerProps) {
  const { decision, onConfirm, onCancel } = props;
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { fetchAllCycles, getProjectCycleDetails } = useCycle();
  const { fetchModules, getProjectModuleDetails } = useModule();
  const [issues, setIssues] = useState<Record<string, TIssue | null>>({});
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [containerId, setContainerId] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const kind = props.kind;
  const containerName = kind === "manage_sprint_items" ? props.proposal.sprint : props.proposal.track;
  const containerLabel = kind === "manage_sprint_items" ? "Sprint" : "Track";
  const workItemsKey = props.proposal.work_items.join("|");

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));

  useEffect(() => {
    if (!workspaceSlug) return;
    let cancelled = false;
    const references = workItemsKey.split("|").filter(Boolean);
    void Promise.all(
      references.map(async (reference) => {
        const parsed = parseWorkItemRef(reference);
        if (!parsed) return [reference, null] as const;
        try {
          const issue = await fetchIssueWithIdentifier(workspaceSlug, parsed.projectIdentifier, parsed.sequenceId);
          return [reference, issue] as const;
        } catch {
          return [reference, null] as const;
        }
      })
    ).then((entries) => {
      if (!cancelled) setIssues(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, workItemsKey, fetchIssueWithIdentifier]);

  const matchedProjectId = props.proposal.project ? (matchProject(projects, props.proposal.project)?.id ?? null) : null;
  const firstResolvedIssue = Object.values(issues).find((issue): issue is TIssue => Boolean(issue));
  const effectiveProjectId = selectedProjectId ?? matchedProjectId ?? firstResolvedIssue?.project_id ?? null;

  useEffect(() => {
    if (!workspaceSlug || !effectiveProjectId) return;
    if (kind === "manage_sprint_items") void fetchAllCycles(workspaceSlug, effectiveProjectId);
    else void fetchModules(workspaceSlug, effectiveProjectId);
  }, [workspaceSlug, effectiveProjectId, kind, fetchAllCycles, fetchModules]);

  const containers =
    effectiveProjectId == null
      ? []
      : ((kind === "manage_sprint_items"
          ? getProjectCycleDetails(effectiveProjectId)
          : getProjectModuleDetails(effectiveProjectId)) ?? []);
  const namedContainers = containers.map((container) => ({ id: container.id, name: container.name }));
  const matchedContainer = matchByNameOrId(namedContainers, containerName);
  const effectiveContainerId = containerId || matchedContainer?.id || "";

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} updated.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} update cancelled.
      </p>
    );
  }

  const rows: ContainerRow[] = props.proposal.work_items.map((reference) => {
    const issue = issues[reference];
    if (!issue) return { reference, issueId: null, status: "missing" };
    if (issue.project_id !== effectiveProjectId) return { reference, issueId: issue.id, status: "other_project" };
    const included =
      kind === "manage_sprint_items"
        ? issue.cycle_id === effectiveContainerId
        : (issue.module_ids ?? []).includes(effectiveContainerId);
    const isActionable = props.proposal.action === "add" ? !included : included;
    return { reference, issueId: issue.id, status: isActionable ? "actionable" : "skipped" };
  });
  const actionable = rows.filter((row) => row.status === "actionable");
  const validationError =
    kind === "manage_sprint_items"
      ? validateManageSprintItemsProposal(props.proposal)
      : validateManageTrackItemsProposal(props.proposal);

  const confirm = async () => {
    if (!effectiveProjectId || !effectiveContainerId || actionable.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const issueIds = actionable.map((row) => row.issueId as string);
      if (kind === "manage_sprint_items") {
        await onConfirm({
          kind: "manage_sprint_items",
          projectId: effectiveProjectId,
          cycleId: effectiveContainerId,
          action: props.proposal.action,
          issueIds,
        });
      } else {
        await onConfirm({
          kind: "manage_track_items",
          projectId: effectiveProjectId,
          moduleId: effectiveContainerId,
          action: props.proposal.action,
          issueIds,
        });
      }
    } catch {
      setError(`Could not update the ${containerLabel.toLowerCase()}. Please retry.`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Container items proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {props.proposal.action === "add"
          ? `Add work items to ${containerName}`
          : `Remove work items from ${containerName}`}
      </p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => setSelectedProjectId(event.target.value || null)}
          >
            <option value="">Select a project</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.identifier} — {project.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {effectiveProjectId && (
        <label className="mt-2 block text-12 text-secondary">
          {containerLabel}
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={effectiveContainerId}
            onChange={(event) => setContainerId(event.target.value)}
          >
            <option value="">Select a {containerLabel.toLowerCase()}</option>
            {namedContainers.map((container) => (
              <option key={container.id} value={container.id}>
                {container.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <ul className="mt-2 flex flex-col gap-1">
        {rows.map((row) => (
          <li key={row.reference} className="text-12 text-secondary">
            {row.reference}
            {row.status === "missing" && <span className="text-tertiary"> — not found, skipped</span>}
            {row.status === "other_project" && (
              <span className="text-tertiary"> — belongs to another project, skipped</span>
            )}
            {row.status === "skipped" && (
              <span className="text-tertiary">
                {" "}
                — {props.proposal.action === "add" ? "already included" : "not included"}, skipped
              </span>
            )}
          </li>
        ))}
      </ul>
      {actionable.length === 0 && !validationError && <p className="mt-1 text-12 text-tertiary">Nothing to change.</p>}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!effectiveProjectId || !effectiveContainerId || actionable.length === 0 || Boolean(validationError)}
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

- [ ] **Step 2: Wire ke `root.tsx`**

Tambahkan import:

```tsx
import { LinkItemsProposalCard } from "./proposals/link-items-proposal-card";
```

Ganti blok render `{message.proposals?.map(...)}` dari Plan 2A dengan versi lengkap:

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
    if (entry.kind === "add_comment") {
      return (
        <CommentProposalCard
          key={entry.key}
          proposal={entry.proposal}
          decision={message.proposalDecisions?.[entry.key]}
          onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
          onCancel={() => cancelProposal(message.id, entry.key)}
        />
      );
    }
    if (entry.kind === "manage_service_links") {
      return (
        <LinkItemsProposalCard
          key={entry.key}
          kind={entry.kind}
          proposal={entry.proposal}
          decision={message.proposalDecisions?.[entry.key]}
          onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
          onCancel={() => cancelProposal(message.id, entry.key)}
        />
      );
    }
    if (entry.kind === "manage_sprint_items") {
      return (
        <LinkItemsProposalCard
          key={entry.key}
          kind={entry.kind}
          proposal={entry.proposal}
          decision={message.proposalDecisions?.[entry.key]}
          onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
          onCancel={() => cancelProposal(message.id, entry.key)}
        />
      );
    }
    return (
      <LinkItemsProposalCard
        key={entry.key}
        kind="manage_track_items"
        proposal={entry.proposal}
        decision={message.proposalDecisions?.[entry.key]}
        onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
        onCancel={() => cancelProposal(message.id, entry.key)}
      />
    );
  });
}
```

- [ ] **Step 3: Verifikasi tipe**

Run (dari `apps/web`): `pnpm check:types`
Expected: lulus tanpa error baru.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/proposals/link-items-proposal-card.tsx apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): render generic link proposal card"
```

---

### Task 10: Verifikasi akhir Fase 2B

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
Commit bila formatter mengubah file:

```bash
git add -A
git commit -m "chore: format fase 2b"
```

- [ ] **Step 5: Smoke manual (opsional, butuh stack hidup)**

Ikuti AGENTS.md: rebuild `pnpm --filter=web build`, restart `plane-web-prod.service`, lalu di UI Agent mode:

1. "link service Email ke LTS-1" → kartu menampilkan daftar service; Confirm → link muncul di work item.
2. "masukkan LTS-1 ke sprint Sprint 3" → kartu container; Confirm → work item masuk sprint.
3. "keluarkan LTS-1 dari track Onboarding" → kartu container remove; Confirm.
4. Scheduled run tetap tidak bisa memutasi (proposal tool tidak ada di `SPEC_TOOLS`).

---

## Self-Review Checklist (sudah dijalankan)

- **Spec coverage:** §6.1 tiga tool link → Task 1–3; §4.4 kind mapping + decision → Task 4–6; §6.2 result kosong untuk link kinds → Task 5; §6.3 kartu generik + skip no-op → Task 9; §6.4 target tidak ketemu/ambigu → kartu (skipped/missing + select manual); §4.5 exact match unik nama container → `matchByNameOrId` + select fallback; §8 testing → unit Rust, integrasi, vitest.
- **Placeholder scan:** tidak ada TODO/TBD; semua langkah berisi kode lengkap.
- **Type consistency:** `ManageServiceLinksProposal`/`TAiManageServiceLinksProposal`, `manage_sprint_items`/`manage_track_items`, `PROPOSAL_TOOL_NAMES` 7 entri, `allowed_result_keys`, `TAiProposalConfirmPayload` link arms, dan dispatch store konsisten antar task. No-op sprint/track memakai `TIssue.cycle_id`/`module_ids`; unlink service memakai `linkId` dari `getWorkItemLinksByService`.
