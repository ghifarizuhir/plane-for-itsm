# Galileo Tools Fase 3A — Create/Update Containers + Form Engine — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah enam proposal tool Fase 3 — `create_service`, `update_service`, `create_sprint`, `update_sprint`, `create_track`, `update_track` — plus form engine generik `proposal-form-card.tsx` yang me-resolve project/container/member di FE dan mengeksekusi create/update lewat endpoint REST existing saat Confirm.

**Architecture:** Semua tool hidup di `tools/mutations.rs` (lanjutan Plan 2A/2B) dan hanya memvalidasi bentuk + mencatat trace. Metadata generik (`proposals`/`proposal_decisions`, store `confirmProposal`) sudah ada; plan ini memperluas `mutation_kind_for_tool`, `PROPOSAL_TOOL_NAMES` (7→13), allowlist result dengan id wajib untuk kind create, lalu menambah tipe + schema form + dispatch + kartu di FE. Form engine berbasis data: definisi field per kind sebagai schema, builder murni untuk draft → payload REST.

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), React + MobX + vitest.

**Prasyarat:** Plan 2A + 2B sudah dieksekusi (metadata generik, store `confirmProposal`, `PROPOSAL_TOOL_NAMES` 7 entri, `LinkItemsProposalCard`, `matchByNameOrId`).

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4.4, §4.5, §4.7, §7.1 create/update container, §7.3 form engine, §8).

**Deviasi dari spec (diputuskan saat menyusun plan, karena endpoint existing):**

1. **Sprint owner dihapus.** `POST .../cycles/` selalu menulis `owned_by_id` = user yang Confirm, dan `PATCH .../cycles/:pk/` mengabaikan `owned_by_id` (`apps/api-rs/crates/api/src/routes/cycle.rs:864`, `:1054`). Karena itu `create_sprint`/`update_sprint` tidak punya arg `owner`; kartu menampilkan catatan "owner = Anda".
2. **Track create mengirim `description` polos.** `POST .../modules/` hanya menyimpan `description` (tanpa `description_html`) (`routes/module.rs:772-776`).
3. **Semantik clear di `changes`:** nilai blank pada field clearable dinormalisasi menjadi string kosong (`""` = clear): description/owner/urls untuk service, description untuk sprint, description/start_date/target_date/lead/members untuk track. `name` blank = error. Tanggal sprint blank = error (endpoint tidak bisa clear tanggal cycle).
4. **Result id wajib saat applied** untuk kind create: `created_service_id`, `created_sprint_id`, `created_track_id` (pola `created_comment_id`).
5. **`update_service` tidak mengubah `name`** (mengikuti spec §7.1: status, criticality, type, owner, description, urls).
6. **Resolusi target update di FE** memakai fetch workspace: service per project (`fetchServices` untuk tiap project), sprint/track lewat `fetchWorkspaceCycles`/`fetchWorkspaceModules`; bila ambigu → select project manual.

---

## File Structure

| File                                                                             | Aksi | Tanggung jawab                                                                        |
| -------------------------------------------------------------------------------- | ---- | ------------------------------------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/tools/mod.rs`                                         | Ubah | `optional_enum_arg`, `PROPOSAL_TOOL_NAMES` 13 entri, registrasi 6 tool, test metadata |
| `apps/api-rs/crates/ai/src/tools/mutations.rs`                                   | Ubah | 6 tool + helper normalisasi + `mutation_kind_for_tool`                                |
| `apps/api-rs/crates/ai/src/agent.rs`                                             | Ubah | `PREAMBLE` create/update + test `pending_actions`                                     |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                          | Ubah | Allowlist + result id wajib per kind                                                  |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`                                  | Ubah | Roundtrip 6 kind                                                                      |
| `apps/web/core/lib/ai-proposals.ts`                                              | Ubah | 6 kind, tipe, allowlist nilai, validator                                              |
| `apps/web/core/lib/ai-proposals.test.ts`                                         | Ubah | Test validator                                                                        |
| `apps/web/core/lib/ai-proposal-forms.ts`                                         | Baru | Schema field per kind + draft + builder murni                                         |
| `apps/web/core/lib/ai-proposal-forms.test.ts`                                    | Baru | Test schema/draft/builder                                                             |
| `apps/web/core/store/ai-assistant.store.ts`                                      | Ubah | Service create/update + dispatch 6 kind                                               |
| `apps/web/core/store/ai-assistant.store.test.ts`                                 | Ubah | Test dispatch                                                                         |
| `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-form-card.tsx` | Baru | Kartu form generik                                                                    |
| `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`      | Baru | Dispatcher `TAiProposal` → kartu                                                      |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`                         | Ubah | Render `ProposalCard`                                                                 |

---

### Task 1: Helper `optional_enum_arg` + tool `create_service`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
    #[test]
    fn optional_enum_arg_rules() {
        assert_eq!(optional_enum_arg(None, &["a"], "x").unwrap(), None);
        assert_eq!(optional_enum_arg(Some("  "), &["a"], "x").unwrap(), None);
        assert_eq!(
            optional_enum_arg(Some(" A "), &["a"], "x").unwrap(),
            Some("a".to_string())
        );
        let err = optional_enum_arg(Some("b"), &["a"], "x").unwrap_err();
        assert!(err.to_string().contains("x must be one of: a"));
    }
```

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn create_service_normalizes_and_validates() {
        let proposal = create_service_proposal_from_args(CreateServiceArgs {
            project: " LTS ".into(),
            name: " Email Gateway ".into(),
            description: Some("  Handles mail  ".into()),
            status: Some(" ACTIVE ".into()),
            criticality: Some("High".into()),
            service_type: Some("internal".into()),
            owner: Some(" Budi ".into()),
            repository_url: Some(" https://git.example.com/email ".into()),
            documentation_url: None,
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Email Gateway");
        assert_eq!(proposal.description.as_deref(), Some("Handles mail"));
        assert_eq!(proposal.status.as_deref(), Some("active"));
        assert_eq!(proposal.criticality.as_deref(), Some("high"));
        assert_eq!(proposal.service_type.as_deref(), Some("internal"));
        assert_eq!(proposal.owner.as_deref(), Some("Budi"));
        assert_eq!(
            proposal.repository_url.as_deref(),
            Some("https://git.example.com/email")
        );
        assert_eq!(proposal.documentation_url, None);

        let blank_name = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "  ".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: None,
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));

        let bad_status = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Email".into(),
            description: None,
            status: Some("broken".into()),
            criticality: None,
            service_type: None,
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap_err();
        assert!(bad_status.to_string().contains("active"));

        let bad_url = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Email".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: None,
            owner: None,
            repository_url: Some("git.example.com".into()),
            documentation_url: None,
        })
        .unwrap_err();
        assert!(bad_url.to_string().contains("http"));
    }

    #[test]
    fn create_service_proposal_serializes_type_key() {
        let proposal = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Email".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: Some("internal".into()),
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap();
        let value = serde_json::to_value(&proposal).unwrap();
        assert_eq!(value["type"], serde_json::json!("internal"));
        assert!(value.get("service_type").is_none());
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p ai create_service_normalizes`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi `optional_enum_arg`**

Tambahkan di `tools/mod.rs` setelah `enum_arg`:

```rust
/// Normalize an optional enum argument (e.g. a status) against an allowlist.
pub fn optional_enum_arg(
    value: Option<&str>,
    allowed: &[&str],
    label: &str,
) -> Result<Option<String>, ToolExecutionError> {
    match optional_text(value) {
        None => Ok(None),
        Some(normalized) => enum_arg(&normalized, allowed, label).map(Some),
    }
}
```

- [ ] **Step 4: Implementasi tool**

Tambahkan di akhir `mutations.rs` sebelum `#[cfg(test)]` (setelah blok `impl Tool for UpdateWorkItem`):

```rust
pub const SERVICE_NAME_MAX: usize = 255;
pub const SERVICE_DESCRIPTION_MAX: usize = 5000;
pub const SERVICE_URL_MAX: usize = 2048;
pub const MAX_MEMBER_REFS: usize = 10;
pub const SERVICE_STATUSES: [&str; 5] = ["active", "planned", "maintenance", "deprecated", "retired"];
pub const SERVICE_CRITICALITIES: [&str; 4] = ["critical", "high", "medium", "low"];
pub const SERVICE_TYPES: [&str; 4] = ["internal", "external", "infrastructure", "third_party"];
pub const MODULE_STATUSES: [&str; 6] = ["backlog", "planned", "in-progress", "paused", "completed", "cancelled"];

fn optional_bounded_text(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<Option<String>, ToolExecutionError> {
    let Some(text) = optional_text(value) else {
        return Ok(None);
    };
    if text.chars().count() > max {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must be at most {max} characters"
        )));
    }
    Ok(Some(text))
}

fn optional_url(value: Option<&str>, label: &str) -> Result<Option<String>, ToolExecutionError> {
    let Some(url) = optional_text(value) else {
        return Ok(None);
    };
    if url.chars().count() > SERVICE_URL_MAX {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must be at most {SERVICE_URL_MAX} characters"
        )));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(ToolExecutionError::invalid_args(format!(
            "{label} must start with http:// or https://"
        )));
    }
    Ok(Some(url))
}

pub const CREATE_SERVICE_NAME: &str = "create_service";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateServiceArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Service name (1-255 characters), unique per project.
    pub name: String,
    /// Plain-text description (max 5000 characters).
    pub description: Option<String>,
    /// One of: active, planned, maintenance, deprecated, retired.
    pub status: Option<String>,
    /// One of: critical, high, medium, low.
    pub criticality: Option<String>,
    /// One of: internal, external, infrastructure, third_party.
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    /// Owner display name or email; resolved by the UI.
    pub owner: Option<String>,
    /// Repository URL starting with http:// or https://.
    pub repository_url: Option<String>,
    /// Documentation URL starting with http:// or https://.
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateServiceProposal {
    pub project: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criticality: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

/// Validate raw tool args into a normalized create-service proposal.
pub fn create_service_proposal_from_args(
    args: CreateServiceArgs,
) -> Result<CreateServiceProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let description =
        optional_bounded_text(args.description.as_deref(), "description", SERVICE_DESCRIPTION_MAX)?;
    let status = optional_enum_arg(args.status.as_deref(), &SERVICE_STATUSES, "status")?;
    let criticality =
        optional_enum_arg(args.criticality.as_deref(), &SERVICE_CRITICALITIES, "criticality")?;
    let service_type = optional_enum_arg(args.service_type.as_deref(), &SERVICE_TYPES, "type")?;
    let owner = optional_bounded_text(args.owner.as_deref(), "owner", CONTAINER_REF_MAX)?;
    let repository_url = optional_url(args.repository_url.as_deref(), "repository_url")?;
    let documentation_url = optional_url(args.documentation_url.as_deref(), "documentation_url")?;
    Ok(CreateServiceProposal {
        project,
        name,
        description,
        status,
        criticality,
        service_type,
        owner,
        repository_url,
        documentation_url,
    })
}

pub struct CreateService {
    pub trace: ToolTrace,
}

impl Tool for CreateService {
    const NAME: &'static str = CREATE_SERVICE_NAME;
    type Args = CreateServiceArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one service in a project. Only call this when the user \
         asks to create a service. The project and service name are required; \
         never guess them. Status, criticality, type, owner, and URLs are \
         optional. The user must confirm and may edit every field in the UI \
         before anything is saved. Never claim the service was created until \
         they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateServiceArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_service_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateServiceProposal serializes"))
    }
}
```

Perbarui import `super::` di `mutations.rs` (tambahkan `optional_enum_arg`):

```rust
use super::{
    bounded_ref_list, bounded_ref_list_opt, enum_arg, optional_enum_arg, optional_text,
    parse_iso_date, parse_work_item_ref, priority_arg, schema_of, WORK_ITEM_DESCRIPTION_MAX,
    WORK_ITEM_NAME_MAX, WORK_ITEM_STATE_MAX,
};
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai create_service optional_enum_arg`
Expected: lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add create_service proposal tool"
```

---

### Task 2: Tool `update_service`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn update_service_normalizes_clears_and_requires_a_change() {
        let proposal = update_service_proposal_from_args(UpdateServiceArgs {
            service: " Email ".into(),
            project: Some(" LTS ".into()),
            changes: UpdateServiceChanges {
                status: Some(" DEPRECATED ".into()),
                owner: Some("".into()),
                repository_url: Some("".into()),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(proposal.service, "Email");
        assert_eq!(proposal.project.as_deref(), Some("LTS"));
        assert_eq!(proposal.changes.status.as_deref(), Some("deprecated"));
        assert_eq!(proposal.changes.owner.as_deref(), Some(""));
        assert_eq!(proposal.changes.repository_url.as_deref(), Some(""));

        let empty = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Email".into(),
            project: None,
            changes: UpdateServiceChanges::default(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let blank_service = update_service_proposal_from_args(UpdateServiceArgs {
            service: "  ".into(),
            project: None,
            changes: UpdateServiceChanges {
                status: Some("active".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_service.to_string().contains("service"));

        let bad_type = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Email".into(),
            project: None,
            changes: UpdateServiceChanges {
                service_type: Some("legacy".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_type.to_string().contains("internal"));

        let bad_url = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Email".into(),
            project: None,
            changes: UpdateServiceChanges {
                documentation_url: Some("docs.example.com".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_url.to_string().contains("http"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai update_service_normalizes`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `CreateService`:

```rust
fn required_when_present(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Err(ToolExecutionError::invalid_args(format!(
            "{label} must not be empty when provided"
        ))),
        Some(text) if text.chars().count() > max => Err(ToolExecutionError::invalid_args(
            format!("{label} must be at most {max} characters"),
        )),
        Some(text) => Ok(Some(text.to_string())),
    }
}

/// Optional text where a provided blank value means "clear the field".
fn clearable_text(
    value: Option<&str>,
    label: &str,
    max: usize,
) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Ok(Some(String::new())),
        Some(text) if text.chars().count() > max => Err(ToolExecutionError::invalid_args(
            format!("{label} must be at most {max} characters"),
        )),
        Some(text) => Ok(Some(text.to_string())),
    }
}

/// Optional URL where blank clears; non-blank must be http(s).
fn clearable_url(value: Option<&str>, label: &str) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Ok(Some(String::new())),
        Some(url) if url.chars().count() > SERVICE_URL_MAX => Err(ToolExecutionError::invalid_args(
            format!("{label} must be at most {SERVICE_URL_MAX} characters"),
        )),
        Some(url) if !(url.starts_with("http://") || url.starts_with("https://")) => Err(
            ToolExecutionError::invalid_args(format!("{label} must start with http:// or https://")),
        ),
        Some(url) => Ok(Some(url.to_string())),
    }
}

pub const UPDATE_SERVICE_NAME: &str = "update_service";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateServiceChanges {
    /// New status: active, planned, maintenance, deprecated, retired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// New criticality: critical, high, medium, low.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criticality: Option<String>,
    /// New type: internal, external, infrastructure, third_party.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    /// New owner display name or email; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// New plain-text description; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New repository URL; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    /// New documentation URL; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

fn normalize_service_changes(
    changes: UpdateServiceChanges,
) -> Result<UpdateServiceChanges, ToolExecutionError> {
    let normalized = UpdateServiceChanges {
        status: optional_enum_arg(changes.status.as_deref(), &SERVICE_STATUSES, "status")?,
        criticality: optional_enum_arg(
            changes.criticality.as_deref(),
            &SERVICE_CRITICALITIES,
            "criticality",
        )?,
        service_type: optional_enum_arg(changes.service_type.as_deref(), &SERVICE_TYPES, "type")?,
        owner: clearable_text(changes.owner.as_deref(), "owner", CONTAINER_REF_MAX)?,
        description: clearable_text(
            changes.description.as_deref(),
            "description",
            SERVICE_DESCRIPTION_MAX,
        )?,
        repository_url: clearable_url(changes.repository_url.as_deref(), "repository_url")?,
        documentation_url: clearable_url(changes.documentation_url.as_deref(), "documentation_url")?,
    };
    if normalized == UpdateServiceChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateServiceArgs {
    /// Service name or id. Required.
    pub service: String,
    /// Project identifier or name; use it when the service name is not unique.
    pub project: Option<String>,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateServiceChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateServiceProposal {
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub changes: UpdateServiceChanges,
}

/// Validate raw tool args into a normalized update-service proposal.
pub fn update_service_proposal_from_args(
    args: UpdateServiceArgs,
) -> Result<UpdateServiceProposal, ToolExecutionError> {
    let service = required_text(&args.service, "service", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let changes = normalize_service_changes(args.changes)?;
    Ok(UpdateServiceProposal {
        service,
        project,
        changes,
    })
}

pub struct UpdateService {
    pub trace: ToolTrace,
}

impl Tool for UpdateService {
    const NAME: &'static str = UPDATE_SERVICE_NAME;
    type Args = UpdateServiceArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing service. Only call this when the \
         user asks to change a service's status, criticality, type, owner, \
         description, or URLs. The service name or id is required; never guess \
         it. Send only the fields that change; an empty string clears a field. \
         The user must confirm and may edit every field in the UI before \
         anything is saved. Never claim the service was updated until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateServiceArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_service_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateServiceProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai update_service_normalizes`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add update_service proposal tool"
```

---

### Task 3: Tool `create_sprint` + `update_sprint`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn create_sprint_requires_paired_dates() {
        let proposal = create_sprint_proposal_from_args(CreateSprintArgs {
            project: " LTS ".into(),
            name: " Sprint 4 ".into(),
            description: Some(" Q4 hardening ".into()),
            start_date: Some("2026-11-01".into()),
            end_date: Some("2026-11-14".into()),
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Sprint 4");
        assert_eq!(proposal.description.as_deref(), Some("Q4 hardening"));
        assert_eq!(proposal.start_date.as_deref(), Some("2026-11-01"));
        assert_eq!(proposal.end_date.as_deref(), Some("2026-11-14"));

        let single = create_sprint_proposal_from_args(CreateSprintArgs {
            project: "LTS".into(),
            name: "Sprint 4".into(),
            description: None,
            start_date: Some("2026-11-01".into()),
            end_date: None,
        })
        .unwrap_err();
        assert!(single.to_string().contains("both"));

        let reversed = create_sprint_proposal_from_args(CreateSprintArgs {
            project: "LTS".into(),
            name: "Sprint 4".into(),
            description: None,
            start_date: Some("2026-11-14".into()),
            end_date: Some("2026-11-01".into()),
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));

        let bad_date = create_sprint_proposal_from_args(CreateSprintArgs {
            project: "LTS".into(),
            name: "Sprint 4".into(),
            description: None,
            start_date: Some("01-11-2026".into()),
            end_date: Some("2026-11-14".into()),
        })
        .unwrap_err();
        assert!(bad_date.to_string().contains("YYYY-MM-DD"));
    }

    #[test]
    fn update_sprint_allows_single_date_and_rejects_blank() {
        let proposal = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: " Sprint 4 ".into(),
            project: None,
            changes: UpdateSprintChanges {
                end_date: Some("2026-11-21".into()),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(proposal.sprint, "Sprint 4");
        assert_eq!(proposal.changes.end_date.as_deref(), Some("2026-11-21"));

        let empty = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges::default(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let blank_date = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges {
                start_date: Some("".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_date.to_string().contains("must not be empty"));

        let blank_name = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges {
                name: Some("  ".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));

        let reversed = update_sprint_proposal_from_args(UpdateSprintArgs {
            sprint: "Sprint 4".into(),
            project: None,
            changes: UpdateSprintChanges {
                start_date: Some("2026-11-21".into()),
                end_date: Some("2026-11-14".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai sprint`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `UpdateService`:

```rust
fn date_change(
    value: Option<&str>,
    label: &str,
    allow_clear: bool,
) -> Result<Option<String>, ToolExecutionError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") if allow_clear => Ok(Some(String::new())),
        Some("") => Err(ToolExecutionError::invalid_args(format!(
            "{label} must not be empty when provided"
        ))),
        Some(text) => parse_iso_date(text, label).map(|date| Some(date.to_string())),
    }
}

pub const CREATE_SPRINT_NAME: &str = "create_sprint";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSprintArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Sprint name (1-255 characters).
    pub name: String,
    /// Plain-text description (max 5000 characters).
    pub description: Option<String>,
    /// Start date "YYYY-MM-DD"; provide both dates or neither.
    pub start_date: Option<String>,
    /// End date "YYYY-MM-DD"; provide both dates or neither.
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateSprintProposal {
    pub project: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

/// Validate raw tool args into a normalized create-sprint proposal.
pub fn create_sprint_proposal_from_args(
    args: CreateSprintArgs,
) -> Result<CreateSprintProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let description =
        optional_bounded_text(args.description.as_deref(), "description", SERVICE_DESCRIPTION_MAX)?;
    let start_date = date_change(args.start_date.as_deref(), "start_date", false)?;
    let end_date = date_change(args.end_date.as_deref(), "end_date", false)?;
    if start_date.is_some() != end_date.is_some() {
        return Err(ToolExecutionError::invalid_args(
            "provide both start_date and end_date or neither",
        ));
    }
    if let (Some(start), Some(end)) = (&start_date, &end_date) {
        if start > end {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after end_date",
            ));
        }
    }
    Ok(CreateSprintProposal {
        project,
        name,
        description,
        start_date,
        end_date,
    })
}

pub struct CreateSprint {
    pub trace: ToolTrace,
}

impl Tool for CreateSprint {
    const NAME: &'static str = CREATE_SPRINT_NAME;
    type Args = CreateSprintArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one sprint (cycle) in a project. Only call this when \
         the user asks to create a sprint. The project and sprint name are \
         required; never guess them. Provide both start and end dates or \
         neither. The sprint owner is always the confirming user. The user must \
         confirm and may edit every field in the UI before anything is saved. \
         Never claim the sprint was created until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateSprintArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_sprint_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateSprintProposal serializes"))
    }
}

pub const UPDATE_SPRINT_NAME: &str = "update_sprint";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSprintChanges {
    /// New sprint name (1-255 characters); must not be blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New plain-text description; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New start date "YYYY-MM-DD"; cannot be cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// New end date "YYYY-MM-DD"; cannot be cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

fn normalize_sprint_changes(
    changes: UpdateSprintChanges,
) -> Result<UpdateSprintChanges, ToolExecutionError> {
    let normalized = UpdateSprintChanges {
        name: required_when_present(changes.name.as_deref(), "name", SERVICE_NAME_MAX)?,
        description: clearable_text(
            changes.description.as_deref(),
            "description",
            SERVICE_DESCRIPTION_MAX,
        )?,
        start_date: date_change(changes.start_date.as_deref(), "start_date", false)?,
        end_date: date_change(changes.end_date.as_deref(), "end_date", false)?,
    };
    if normalized == UpdateSprintChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    if let (Some(start), Some(end)) = (&normalized.start_date, &normalized.end_date) {
        if start > end {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after end_date",
            ));
        }
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSprintArgs {
    /// Sprint name or id. Required.
    pub sprint: String,
    /// Project identifier or name; use it when the sprint name is not unique.
    pub project: Option<String>,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateSprintChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateSprintProposal {
    pub sprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub changes: UpdateSprintChanges,
}

/// Validate raw tool args into a normalized update-sprint proposal.
pub fn update_sprint_proposal_from_args(
    args: UpdateSprintArgs,
) -> Result<UpdateSprintProposal, ToolExecutionError> {
    let sprint = required_text(&args.sprint, "sprint", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let changes = normalize_sprint_changes(args.changes)?;
    Ok(UpdateSprintProposal {
        sprint,
        project,
        changes,
    })
}

pub struct UpdateSprint {
    pub trace: ToolTrace,
}

impl Tool for UpdateSprint {
    const NAME: &'static str = UPDATE_SPRINT_NAME;
    type Args = UpdateSprintArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing sprint. Only call this when the \
         user asks to change a sprint's name, description, or dates. The sprint \
         name or id is required; never guess it. Send only the fields that \
         change; an empty description clears it and dates cannot be cleared. \
         Pass the project when the sprint name is not unique. The user must \
         confirm and may edit every field in the UI before anything is saved. \
         Never claim the sprint was updated until they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateSprintArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_sprint_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateSprintProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai sprint`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add sprint create and update proposal tools"
```

---

### Task 4: Tool `create_track` + `update_track`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn create_track_normalizes_and_bounds_members() {
        let proposal = create_track_proposal_from_args(CreateTrackArgs {
            project: " LTS ".into(),
            name: " Onboarding ".into(),
            description: Some(" New joiner path ".into()),
            start_date: Some("2026-11-01".into()),
            target_date: Some("2026-12-01".into()),
            status: Some(" PLANNED ".into()),
            lead: Some(" Budi ".into()),
            members: Some(vec![" Budi ".into(), "budi".into(), "Sari".into()]),
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Onboarding");
        assert_eq!(proposal.status.as_deref(), Some("planned"));
        assert_eq!(proposal.lead.as_deref(), Some("Budi"));
        assert_eq!(
            proposal.members,
            vec!["Budi".to_string(), "Sari".to_string()]
        );

        let bad_status = create_track_proposal_from_args(CreateTrackArgs {
            project: "LTS".into(),
            name: "Onboarding".into(),
            description: None,
            start_date: None,
            target_date: None,
            status: Some("live".into()),
            lead: None,
            members: None,
        })
        .unwrap_err();
        assert!(bad_status.to_string().contains("backlog"));

        let reversed = create_track_proposal_from_args(CreateTrackArgs {
            project: "LTS".into(),
            name: "Onboarding".into(),
            description: None,
            start_date: Some("2026-12-01".into()),
            target_date: Some("2026-11-01".into()),
            status: None,
            lead: None,
            members: None,
        })
        .unwrap_err();
        assert!(reversed.to_string().contains("start_date"));

        let too_many = create_track_proposal_from_args(CreateTrackArgs {
            project: "LTS".into(),
            name: "Onboarding".into(),
            description: None,
            start_date: None,
            target_date: None,
            status: None,
            lead: None,
            members: Some((0..=MAX_MEMBER_REFS).map(|index| format!("u{index}")).collect()),
        })
        .unwrap_err();
        assert!(too_many.to_string().contains("at most"));
    }

    #[test]
    fn update_track_supports_clears_and_member_replace() {
        let proposal = update_track_proposal_from_args(UpdateTrackArgs {
            track: " Onboarding ".into(),
            project: None,
            changes: UpdateTrackChanges {
                description: Some("".into()),
                start_date: Some("".into()),
                target_date: Some("".into()),
                lead: Some("".into()),
                members: Some(vec![]),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(proposal.track, "Onboarding");
        assert_eq!(proposal.changes.description.as_deref(), Some(""));
        assert_eq!(proposal.changes.start_date.as_deref(), Some(""));
        assert_eq!(proposal.changes.target_date.as_deref(), Some(""));
        assert_eq!(proposal.changes.lead.as_deref(), Some(""));
        assert_eq!(proposal.changes.members, Some(vec![]));

        let empty = update_track_proposal_from_args(UpdateTrackArgs {
            track: "Onboarding".into(),
            project: None,
            changes: UpdateTrackChanges::default(),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let blank_name = update_track_proposal_from_args(UpdateTrackArgs {
            track: "Onboarding".into(),
            project: None,
            changes: UpdateTrackChanges {
                name: Some("  ".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai track_`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `UpdateSprint`:

```rust
pub const CREATE_TRACK_NAME: &str = "create_track";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTrackArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Track (module) name (1-255 characters), unique per project.
    pub name: String,
    /// Plain-text description (max 5000 characters).
    pub description: Option<String>,
    /// Start date "YYYY-MM-DD".
    pub start_date: Option<String>,
    /// Target date "YYYY-MM-DD".
    pub target_date: Option<String>,
    /// One of: backlog, planned, in-progress, paused, completed, cancelled.
    pub status: Option<String>,
    /// Lead display name or email; resolved by the UI.
    pub lead: Option<String>,
    /// Member display names or emails (max 10); resolved by the UI.
    pub members: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateTrackProposal {
    pub project: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    pub members: Vec<String>,
}

/// Validate raw tool args into a normalized create-track proposal.
pub fn create_track_proposal_from_args(
    args: CreateTrackArgs,
) -> Result<CreateTrackProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let description =
        optional_bounded_text(args.description.as_deref(), "description", SERVICE_DESCRIPTION_MAX)?;
    let start_date = date_change(args.start_date.as_deref(), "start_date", false)?;
    let target_date = date_change(args.target_date.as_deref(), "target_date", false)?;
    if let (Some(start), Some(target)) = (&start_date, &target_date) {
        if start > target {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    let status = optional_enum_arg(args.status.as_deref(), &MODULE_STATUSES, "status")?;
    let lead = optional_bounded_text(args.lead.as_deref(), "lead", CONTAINER_REF_MAX)?;
    let members = bounded_ref_list(args.members, "members")?;
    Ok(CreateTrackProposal {
        project,
        name,
        description,
        start_date,
        target_date,
        status,
        lead,
        members,
    })
}

pub struct CreateTrack {
    pub trace: ToolTrace,
}

impl Tool for CreateTrack {
    const NAME: &'static str = CREATE_TRACK_NAME;
    type Args = CreateTrackArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one track (module) in a project. Only call this when \
         the user asks to create a track. The project and track name are \
         required; never guess them. Dates, status, lead, and members are \
         optional; lead and member names are human-readable and resolved by the \
         UI. The user must confirm and may edit every field in the UI before \
         anything is saved. Never claim the track was created until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateTrackArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_track_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateTrackProposal serializes"))
    }
}

pub const UPDATE_TRACK_NAME: &str = "update_track";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTrackChanges {
    /// New track name (1-255 characters); must not be blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New plain-text description; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New start date "YYYY-MM-DD"; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// New target date "YYYY-MM-DD"; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<String>,
    /// New status: backlog, planned, in-progress, paused, completed, cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// New lead display name or email; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    /// New member display names or emails; an empty list clears them (max 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<String>>,
}

fn normalize_track_changes(
    changes: UpdateTrackChanges,
) -> Result<UpdateTrackChanges, ToolExecutionError> {
    let normalized = UpdateTrackChanges {
        name: required_when_present(changes.name.as_deref(), "name", SERVICE_NAME_MAX)?,
        description: clearable_text(
            changes.description.as_deref(),
            "description",
            SERVICE_DESCRIPTION_MAX,
        )?,
        start_date: date_change(changes.start_date.as_deref(), "start_date", true)?,
        target_date: date_change(changes.target_date.as_deref(), "target_date", true)?,
        status: optional_enum_arg(changes.status.as_deref(), &MODULE_STATUSES, "status")?,
        lead: clearable_text(changes.lead.as_deref(), "lead", CONTAINER_REF_MAX)?,
        members: bounded_ref_list_opt(changes.members, "members")?,
    };
    if normalized == UpdateTrackChanges::default() {
        return Err(ToolExecutionError::invalid_args(
            "at least one change is required",
        ));
    }
    if let (Some(start), Some(target)) = (&normalized.start_date, &normalized.target_date) {
        if !start.is_empty() && !target.is_empty() && start > target {
            return Err(ToolExecutionError::invalid_args(
                "start_date must not be after target_date",
            ));
        }
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTrackArgs {
    /// Track (module) name or id. Required.
    pub track: String,
    /// Project identifier or name; use it when the track name is not unique.
    pub project: Option<String>,
    /// Fields to change. At least one field must be set.
    pub changes: UpdateTrackChanges,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateTrackProposal {
    pub track: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub changes: UpdateTrackChanges,
}

/// Validate raw tool args into a normalized update-track proposal.
pub fn update_track_proposal_from_args(
    args: UpdateTrackArgs,
) -> Result<UpdateTrackProposal, ToolExecutionError> {
    let track = required_text(&args.track, "track", CONTAINER_REF_MAX)?;
    let project = optional_container_project(args.project)?;
    let changes = normalize_track_changes(args.changes)?;
    Ok(UpdateTrackProposal {
        track,
        project,
        changes,
    })
}

pub struct UpdateTrack {
    pub trace: ToolTrace,
}

impl Tool for UpdateTrack {
    const NAME: &'static str = UPDATE_TRACK_NAME;
    type Args = UpdateTrackArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing exactly one existing track. Only call this when the \
         user asks to change a track's name, description, dates, status, lead, \
         or members. The track name or id is required; never guess it. Send \
         only the fields that change; empty strings and empty member lists \
         clear those fields. Pass the project when the track name is not \
         unique. The user must confirm and may edit every field in the UI \
         before anything is saved. Never claim the track was updated until \
         they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateTrackArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_track_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateTrackProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai track_`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add track create and update proposal tools"
```

---

### Task 5: Registry, `PROPOSAL_TOOL_NAMES`, PREAMBLE

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
    #[tokio::test]
    async fn create_and_update_tools_expose_metadata() {
        let create_service = CreateService {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(CreateService::NAME, "create_service");
        assert!(create_service.parameters()["properties"]["name"].is_object());
        assert!(create_service.parameters()["properties"]["type"].is_object());
        assert!(!create_service.description().is_empty());

        let update_service = UpdateService {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(UpdateService::NAME, "update_service");
        assert!(update_service.parameters()["properties"]["changes"].is_object());

        let create_sprint = CreateSprint {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(CreateSprint::NAME, "create_sprint");
        assert!(create_sprint.parameters()["properties"]["start_date"].is_object());

        let update_sprint = UpdateSprint {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(UpdateSprint::NAME, "update_sprint");
        assert!(update_sprint.parameters()["properties"]["sprint"].is_object());

        let create_track = CreateTrack {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(CreateTrack::NAME, "create_track");
        assert!(create_track.parameters()["properties"]["members"].is_object());

        let update_track = UpdateTrack {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(UpdateTrack::NAME, "update_track");
        assert!(update_track.parameters()["properties"]["track"].is_object());
    }
```

Tambahkan di `mod tests` `agent.rs`:

```rust
    #[test]
    fn pending_actions_covers_create_and_update_mutations() {
        let trace = new_trace();
        record(
            &trace,
            crate::tools::CREATE_SERVICE_NAME,
            &json!({"project": "LTS", "name": "Email"}),
        );
        record(
            &trace,
            crate::tools::UPDATE_SERVICE_NAME,
            &json!({"service": "Email", "changes": {"status": "deprecated"}}),
        );
        record(
            &trace,
            crate::tools::CREATE_SPRINT_NAME,
            &json!({"project": "LTS", "name": "Sprint 4"}),
        );
        record(
            &trace,
            crate::tools::UPDATE_SPRINT_NAME,
            &json!({"sprint": "Sprint 4", "changes": {"name": "Sprint 4b"}}),
        );
        record(
            &trace,
            crate::tools::CREATE_TRACK_NAME,
            &json!({"project": "LTS", "name": "Onboarding"}),
        );
        record(
            &trace,
            crate::tools::UPDATE_TRACK_NAME,
            &json!({"track": "Onboarding", "changes": {"status": "in-progress"}}),
        );
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 6);
        assert_eq!(actions[0]["kind"], json!("create_service"));
        assert_eq!(actions[1]["kind"], json!("update_service"));
        assert_eq!(actions[2]["kind"], json!("create_sprint"));
        assert_eq!(actions[3]["kind"], json!("update_sprint"));
        assert_eq!(actions[4]["kind"], json!("create_track"));
        assert_eq!(actions[5]["kind"], json!("update_track"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pending_actions_covers_create`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Di `tools/mod.rs`, perbarui `PROPOSAL_TOOL_NAMES` menjadi:

```rust
/// Proposal tool names: recorded in the trace but never executed against the
/// database and never schedule-eligible.
pub const PROPOSAL_TOOL_NAMES: [&str; 13] = [
    CREATE_SCHEDULE_NAME,
    CREATE_WORK_ITEM_NAME,
    UPDATE_WORK_ITEM_NAME,
    ADD_COMMENT_NAME,
    MANAGE_SERVICE_LINKS_NAME,
    MANAGE_SPRINT_ITEMS_NAME,
    MANAGE_TRACK_ITEMS_NAME,
    CREATE_SERVICE_NAME,
    UPDATE_SERVICE_NAME,
    CREATE_SPRINT_NAME,
    UPDATE_SPRINT_NAME,
    CREATE_TRACK_NAME,
    UPDATE_TRACK_NAME,
];
```

Di `workspace_tools`, setelah `.tool(ManageTrackItems { ... })` dan sebelum `.tool(CreateSchedule { ... })`:

```rust
        .tool(CreateService {
            trace: trace.clone(),
        })
        .tool(UpdateService {
            trace: trace.clone(),
        })
        .tool(CreateSprint {
            trace: trace.clone(),
        })
        .tool(UpdateSprint {
            trace: trace.clone(),
        })
        .tool(CreateTrack {
            trace: trace.clone(),
        })
        .tool(UpdateTrack {
            trace: trace.clone(),
        })
```

Di `mutations.rs`, perbarui `mutation_kind_for_tool`:

```rust
/// Map a mutation tool name to its proposal `kind`.
pub fn mutation_kind_for_tool(name: &str) -> Option<&'static str> {
    match name {
        UPDATE_WORK_ITEM_NAME => Some("update_work_item"),
        ADD_COMMENT_NAME => Some("add_comment"),
        MANAGE_SERVICE_LINKS_NAME => Some("manage_service_links"),
        MANAGE_SPRINT_ITEMS_NAME => Some("manage_sprint_items"),
        MANAGE_TRACK_ITEMS_NAME => Some("manage_track_items"),
        CREATE_SERVICE_NAME => Some("create_service"),
        UPDATE_SERVICE_NAME => Some("update_service"),
        CREATE_SPRINT_NAME => Some("create_sprint"),
        UPDATE_SPRINT_NAME => Some("update_sprint"),
        CREATE_TRACK_NAME => Some("create_track"),
        UPDATE_TRACK_NAME => Some("update_track"),
        _ => None,
    }
}
```

Di `agent.rs`, tambahkan setelah paragraf link di `PREAMBLE`:

```text
To create a service, sprint, or track, call create_service, create_sprint, \
or create_track with the project and name the user gave. To change an \
existing one, call update_service, update_sprint, or update_track with its \
name (or id) and only the fields that change. These are proposals: the user \
must confirm them in the UI first, so never claim they were created or \
updated before that confirmation. Sprint dates must be provided as a pair; \
service and track owners, leads, and members may be human-readable names.
```

Tambahkan juga di `mutations.rs` (setelah test `mutation_kind_mapping_covers_link_tools`):

```rust
    #[test]
    fn mutation_kind_mapping_covers_create_and_update_tools() {
        assert_eq!(
            mutation_kind_for_tool(CREATE_SERVICE_NAME),
            Some("create_service")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_SERVICE_NAME),
            Some("update_service")
        );
        assert_eq!(
            mutation_kind_for_tool(CREATE_SPRINT_NAME),
            Some("create_sprint")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_SPRINT_NAME),
            Some("update_sprint")
        );
        assert_eq!(
            mutation_kind_for_tool(CREATE_TRACK_NAME),
            Some("create_track")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_TRACK_NAME),
            Some("update_track")
        );
    }
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/tools/mutations.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): register container create and update proposal tools"
```

---

### Task 6: Allowlist decision + result id wajib untuk kind create

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_conversations.rs`:

```rust
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
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --lib metadata_patch_requires_create`
Expected: gagal (`kind not allowed: create_track`).

- [ ] **Step 3: Implementasi**

Perbarui `allowed_result_keys`:

```rust
/// Result keys the FE may persist per proposal kind. Kinds without result
/// data get an empty allowlist.
fn allowed_result_keys(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "update_work_item"
        | "update_service"
        | "update_sprint"
        | "update_track"
        | "manage_service_links"
        | "manage_sprint_items"
        | "manage_track_items" => Some(&[]),
        "add_comment" => Some(&["created_comment_id"]),
        "create_service" => Some(&["created_service_id"]),
        "create_sprint" => Some(&["created_sprint_id"]),
        "create_track" => Some(&["created_track_id"]),
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
        _ => None,
    }
}
```

Di `clean_proposal_decisions`, ganti blok khusus `add_comment`:

```rust
                if kind == "add_comment" && !result.contains_key("created_comment_id") {
                    return Err(
                        "add_comment applied decisions need result.created_comment_id".to_string()
                    );
                }
```

menjadi:

```rust
                if let Some(required) = required_result_key(kind) {
                    if !result.contains_key(required) {
                        return Err(format!(
                            "{kind} applied decisions need result.{required}"
                        ));
                    }
                }
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --lib metadata_patch`
Expected: seluruh test metadata lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): accept container proposal decisions"
```

---

### Task 7: Integration test roundtrip enam kind

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tambahkan enam test DB-backed**

Tambahkan di akhir file (helper `tool_roundtrip::tool_roundtrip_url` dari Plan 2A):

```rust
#[tokio::test]
async fn create_service_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "create_service",
        r#"{"project":"LTS","name":"Email","status":"active","criticality":"high","type":"internal","owner":"Budi"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "create the Email service in LTS",
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
    assert_eq!(proposals[0]["kind"], json!("create_service"));
    assert_eq!(proposals[0]["proposal"]["name"], json!("Email"));
    assert_eq!(proposals[0]["proposal"]["type"], json!("internal"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let created = Uuid::new_v4();
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "create_service", "decision": "applied", "result": {"created_service_id": created}}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["result"]["created_service_id"],
        json!(created)
    );

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn update_service_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "update_service",
        r#"{"service":"Email","changes":{"status":"deprecated","owner":"Sari"}}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "deprecate the Email service",
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
    assert_eq!(proposals[0]["kind"], json!("update_service"));
    assert_eq!(proposals[0]["proposal"]["changes"]["status"], json!("deprecated"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "update_service", "decision": "applied"}
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
async fn create_sprint_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "create_sprint",
        r#"{"project":"LTS","name":"Sprint 4","description":"Q4 hardening","start_date":"2026-11-01","end_date":"2026-11-14"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "create Sprint 4 in LTS",
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
    assert_eq!(proposals[0]["kind"], json!("create_sprint"));
    assert_eq!(proposals[0]["proposal"]["start_date"], json!("2026-11-01"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let created = Uuid::new_v4();
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "create_sprint", "decision": "applied", "result": {"created_sprint_id": created}}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["result"]["created_sprint_id"],
        json!(created)
    );

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn update_sprint_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "update_sprint",
        r#"{"sprint":"Sprint 4","changes":{"name":"Sprint 4 (rev)","end_date":"2026-11-21"}}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "rename Sprint 4",
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
    assert_eq!(proposals[0]["kind"], json!("update_sprint"));
    assert_eq!(proposals[0]["proposal"]["changes"]["end_date"], json!("2026-11-21"));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_track_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "create_track",
        r#"{"project":"LTS","name":"Onboarding","description":"New joiner path","status":"planned","lead":"Budi","members":["Budi","Sari"]}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "create an Onboarding track in LTS",
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
    assert_eq!(proposals[0]["kind"], json!("create_track"));
    assert_eq!(proposals[0]["proposal"]["members"][0], json!("Budi"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let created = Uuid::new_v4();
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "create_track", "decision": "applied", "result": {"created_track_id": created}}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["result"]["created_track_id"],
        json!(created)
    );

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn update_track_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "update_track",
        r#"{"track":"Onboarding","changes":{"status":"in-progress","members":["Budi","Sari"]}}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "start the Onboarding track",
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
    assert_eq!(proposals[0]["kind"], json!("update_track"));
    assert_eq!(proposals[0]["proposal"]["changes"]["status"], json!("in-progress"));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan test**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus (butuh DB dev + Redis).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): roundtrip container proposal metadata"
```

---

### Task 8: FE `ai-proposals.ts` — 6 kind baru + validator

**Files:**

- Modify: `apps/web/core/lib/ai-proposals.ts`
- Modify: `apps/web/core/lib/ai-proposals.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Ganti blok import di `ai-proposals.test.ts` menjadi:

```ts
import {
  changedFieldCount,
  matchByNameOrId,
  validateAddCommentProposal,
  validateCreateServiceProposal,
  validateCreateSprintProposal,
  validateCreateTrackProposal,
  validateManageServiceLinksProposal,
  validateManageSprintItemsProposal,
  validateManageTrackItemsProposal,
  validateUpdateServiceProposal,
  validateUpdateSprintProposal,
  validateUpdateTrackProposal,
  validateUpdateWorkItemProposal,
} from "./ai-proposals";
```

Tambahkan di akhir file:

```ts
describe("container proposal validators", () => {
  it("validates create-service proposals", () => {
    expect(validateCreateServiceProposal({ project: "LTS", name: "Email", status: "active" })).toBeNull();
    expect(validateCreateServiceProposal({ project: "LTS", name: "  " })).toBe("Name is required.");
    expect(validateCreateServiceProposal({ project: "LTS", name: "Email", status: "broken" })).toBe("Unknown status.");
    expect(validateCreateServiceProposal({ project: "LTS", name: "Email", repository_url: "git.example.com" })).toBe(
      "Repository URL must start with http:// or https://."
    );
  });

  it("validates update-service proposals", () => {
    expect(validateUpdateServiceProposal({ service: "Email", changes: { status: "deprecated" } })).toBeNull();
    expect(validateUpdateServiceProposal({ service: "Email", changes: {} })).toBe("At least one change is required.");
    expect(validateUpdateServiceProposal({ changes: { status: "active" } })).toBe("Service is required.");
    expect(validateUpdateServiceProposal({ service: "Email", changes: { owner: "" } })).toBeNull();
    expect(validateUpdateServiceProposal({ service: "Email", changes: { type: "legacy" } })).toBe("Unknown type.");
  });

  it("validates create-sprint proposals", () => {
    expect(
      validateCreateSprintProposal({
        project: "LTS",
        name: "Sprint 4",
        start_date: "2026-11-01",
        end_date: "2026-11-14",
      })
    ).toBeNull();
    expect(validateCreateSprintProposal({ project: "LTS", name: "Sprint 4", start_date: "2026-11-01" })).toBe(
      "Provide both start and end dates or neither."
    );
    expect(
      validateCreateSprintProposal({
        project: "LTS",
        name: "Sprint 4",
        start_date: "2026-11-14",
        end_date: "2026-11-01",
      })
    ).toBe("Start date must not be after end date.");
  });

  it("validates update-sprint proposals", () => {
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: { name: "Sprint 4b" } })).toBeNull();
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: {} })).toBe("At least one change is required.");
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: { name: "  " } })).toBe(
      "Name must not be empty."
    );
    expect(validateUpdateSprintProposal({ sprint: "Sprint 4", changes: { start_date: "" } })).toBe(
      "Start date must be YYYY-MM-DD."
    );
  });

  it("validates create-track proposals", () => {
    expect(validateCreateTrackProposal({ project: "LTS", name: "Onboarding", status: "planned" })).toBeNull();
    expect(validateCreateTrackProposal({ project: "LTS", name: "Onboarding", status: "live" })).toBe("Unknown status.");
    expect(
      validateCreateTrackProposal({
        project: "LTS",
        name: "Onboarding",
        start_date: "2026-12-01",
        target_date: "2026-11-01",
      })
    ).toBe("Start date must not be after target date.");
    expect(
      validateCreateTrackProposal({
        project: "LTS",
        name: "Onboarding",
        members: ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"],
      })
    ).toBe("At most 10 members are allowed.");
  });

  it("validates update-track proposals", () => {
    expect(validateUpdateTrackProposal({ track: "Onboarding", changes: { description: "" } })).toBeNull();
    expect(validateUpdateTrackProposal({ track: "Onboarding", changes: { members: [] } })).toBeNull();
    expect(validateUpdateTrackProposal({ track: "Onboarding", changes: {} })).toBe("At least one change is required.");
    expect(validateUpdateTrackProposal({ changes: { status: "paused" } })).toBe("Track is required.");
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/web`): `pnpm test core/lib/ai-proposals.test.ts`
Expected: gagal kompilasi/import.

- [ ] **Step 3: Implementasi**

Di `ai-proposals.ts`, perbarui import dan konstanta:

```ts
import type { ICycle, IModule, IService, TIssue } from "@plane/types";
import { WORK_ITEM_PRIORITIES, type TWorkItemPriority } from "@/lib/ai-work-items";

export const PROPOSAL_KINDS = [
  "update_work_item",
  "add_comment",
  "manage_service_links",
  "manage_sprint_items",
  "manage_track_items",
  "create_service",
  "update_service",
  "create_sprint",
  "update_sprint",
  "create_track",
  "update_track",
] as const;
export type TAiProposalKind = (typeof PROPOSAL_KINDS)[number];

export const PROPOSAL_LIMITS = {
  comment: 5000,
  name: 255,
  description: 5000,
  refs: 10,
  services: 10,
  items: 25,
  url: 2048,
} as const;

export const SERVICE_STATUS_VALUES = ["active", "planned", "maintenance", "deprecated", "retired"] as const;
export const SERVICE_CRITICALITY_VALUES = ["critical", "high", "medium", "low"] as const;
export const SERVICE_TYPE_VALUES = ["internal", "external", "infrastructure", "third_party"] as const;
export const MODULE_STATUS_VALUES = ["backlog", "planned", "in-progress", "paused", "completed", "cancelled"] as const;
```

Tambahkan tipe payload (setelah `TAiManageTrackItemsProposal`):

```ts
export type TAiCreateServiceProposal = {
  project: string;
  name: string;
  description?: string | null;
  status?: string | null;
  criticality?: string | null;
  type?: string | null;
  owner?: string | null;
  repository_url?: string | null;
  documentation_url?: string | null;
};

export type TAiUpdateServiceChanges = {
  status?: string | null;
  criticality?: string | null;
  type?: string | null;
  owner?: string | null;
  description?: string | null;
  repository_url?: string | null;
  documentation_url?: string | null;
};

export type TAiUpdateServiceProposal = {
  service: string;
  project?: string | null;
  changes: TAiUpdateServiceChanges;
};

export type TAiCreateSprintProposal = {
  project: string;
  name: string;
  description?: string | null;
  start_date?: string | null;
  end_date?: string | null;
};

export type TAiUpdateSprintChanges = {
  name?: string | null;
  description?: string | null;
  start_date?: string | null;
  end_date?: string | null;
};

export type TAiUpdateSprintProposal = {
  sprint: string;
  project?: string | null;
  changes: TAiUpdateSprintChanges;
};

export type TAiCreateTrackProposal = {
  project: string;
  name: string;
  description?: string | null;
  start_date?: string | null;
  target_date?: string | null;
  status?: string | null;
  lead?: string | null;
  members?: string[] | null;
};

export type TAiUpdateTrackChanges = {
  name?: string | null;
  description?: string | null;
  start_date?: string | null;
  target_date?: string | null;
  status?: string | null;
  lead?: string | null;
  members?: string[] | null;
};

export type TAiUpdateTrackProposal = {
  track: string;
  project?: string | null;
  changes: TAiUpdateTrackChanges;
};
```

Perbarui union `TAiProposal`:

```ts
export type TAiProposal =
  | { key: string; kind: "update_work_item"; proposal: TAiUpdateWorkItemProposal }
  | { key: string; kind: "add_comment"; proposal: TAiAddCommentProposal }
  | { key: string; kind: "manage_service_links"; proposal: TAiManageServiceLinksProposal }
  | { key: string; kind: "manage_sprint_items"; proposal: TAiManageSprintItemsProposal }
  | { key: string; kind: "manage_track_items"; proposal: TAiManageTrackItemsProposal }
  | { key: string; kind: "create_service"; proposal: TAiCreateServiceProposal }
  | { key: string; kind: "update_service"; proposal: TAiUpdateServiceProposal }
  | { key: string; kind: "create_sprint"; proposal: TAiCreateSprintProposal }
  | { key: string; kind: "update_sprint"; proposal: TAiUpdateSprintProposal }
  | { key: string; kind: "create_track"; proposal: TAiCreateTrackProposal }
  | { key: string; kind: "update_track"; proposal: TAiUpdateTrackProposal };
```

Perbarui `TAiProposalDecisionResult`:

```ts
export type TAiProposalDecisionResult = {
  created_comment_id?: string;
  created_service_id?: string;
  created_sprint_id?: string;
  created_track_id?: string;
};
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
    }
  | { kind: "create_service"; projectId: string; data: Partial<IService> }
  | { kind: "update_service"; projectId: string; serviceId: string; changes: Partial<IService> }
  | { kind: "create_sprint"; projectId: string; data: Partial<ICycle> }
  | { kind: "update_sprint"; projectId: string; cycleId: string; changes: Partial<ICycle> }
  | { kind: "create_track"; projectId: string; data: Partial<IModule> }
  | { kind: "update_track"; projectId: string; moduleId: string; changes: Partial<IModule> };
```

Tambahkan helper + validator di akhir file:

```ts
const isHttpUrl = (value: string): boolean => value.startsWith("http://") || value.startsWith("https://");

/** Count fields explicitly set (null/undefined means "not changed"). */
export const countChangedFields = (changes: object): number =>
  Object.values(changes).filter((value) => value !== null && value !== undefined).length;

/** Mirrors `create_service_proposal_from_args` on the backend. */
export const validateCreateServiceProposal = (proposal: Partial<TAiCreateServiceProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((proposal.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (proposal.status && !SERVICE_STATUS_VALUES.includes(proposal.status as (typeof SERVICE_STATUS_VALUES)[number]))
    return "Unknown status.";
  if (
    proposal.criticality &&
    !SERVICE_CRITICALITY_VALUES.includes(proposal.criticality as (typeof SERVICE_CRITICALITY_VALUES)[number])
  )
    return "Unknown criticality.";
  if (proposal.type && !SERVICE_TYPE_VALUES.includes(proposal.type as (typeof SERVICE_TYPE_VALUES)[number]))
    return "Unknown type.";
  if ((proposal.owner?.length ?? 0) > PROPOSAL_LIMITS.name) return "Owner is too long.";
  if (proposal.repository_url) {
    if (proposal.repository_url.length > PROPOSAL_LIMITS.url)
      return `Repository URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(proposal.repository_url)) return "Repository URL must start with http:// or https://.";
  }
  if (proposal.documentation_url) {
    if (proposal.documentation_url.length > PROPOSAL_LIMITS.url)
      return `Documentation URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(proposal.documentation_url)) return "Documentation URL must start with http:// or https://.";
  }
  return null;
};

/** Mirrors `update_service_proposal_from_args` on the backend. */
export const validateUpdateServiceProposal = (proposal: Partial<TAiUpdateServiceProposal>): string | null => {
  if (!proposal.service?.trim()) return "Service is required.";
  const changes = proposal.changes ?? {};
  if (countChangedFields(changes) === 0) return "At least one change is required.";
  if (changes.status && !SERVICE_STATUS_VALUES.includes(changes.status as (typeof SERVICE_STATUS_VALUES)[number]))
    return "Unknown status.";
  if (
    changes.criticality &&
    !SERVICE_CRITICALITY_VALUES.includes(changes.criticality as (typeof SERVICE_CRITICALITY_VALUES)[number])
  )
    return "Unknown criticality.";
  if (changes.type && !SERVICE_TYPE_VALUES.includes(changes.type as (typeof SERVICE_TYPE_VALUES)[number]))
    return "Unknown type.";
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if ((changes.owner?.length ?? 0) > PROPOSAL_LIMITS.name) return "Owner is too long.";
  if (changes.repository_url) {
    if (changes.repository_url.length > PROPOSAL_LIMITS.url)
      return `Repository URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(changes.repository_url)) return "Repository URL must start with http:// or https://.";
  }
  if (changes.documentation_url) {
    if (changes.documentation_url.length > PROPOSAL_LIMITS.url)
      return `Documentation URL must be at most ${PROPOSAL_LIMITS.url} characters.`;
    if (!isHttpUrl(changes.documentation_url)) return "Documentation URL must start with http:// or https://.";
  }
  return null;
};

/** Mirrors `create_sprint_proposal_from_args` on the backend. */
export const validateCreateSprintProposal = (proposal: Partial<TAiCreateSprintProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((proposal.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  const start = proposal.start_date?.trim() ?? "";
  const end = proposal.end_date?.trim() ?? "";
  if (start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (end && !ISO_DATE.test(end)) return "End date must be YYYY-MM-DD.";
  if (Boolean(start) !== Boolean(end)) return "Provide both start and end dates or neither.";
  if (start && end && start > end) return "Start date must not be after end date.";
  return null;
};

/** Mirrors `update_sprint_proposal_from_args` on the backend. */
export const validateUpdateSprintProposal = (proposal: Partial<TAiUpdateSprintProposal>): string | null => {
  if (!proposal.sprint?.trim()) return "Sprint is required.";
  const changes = proposal.changes ?? {};
  if (countChangedFields(changes) === 0) return "At least one change is required.";
  if (changes.name != null) {
    const name = changes.name.trim();
    if (!name) return "Name must not be empty.";
    if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  }
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  const start = changes.start_date?.trim() ?? "";
  const end = changes.end_date?.trim() ?? "";
  if (changes.start_date != null && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (changes.end_date != null && !ISO_DATE.test(end)) return "End date must be YYYY-MM-DD.";
  if (start && end && start > end) return "Start date must not be after end date.";
  return null;
};

/** Mirrors `create_track_proposal_from_args` on the backend. */
export const validateCreateTrackProposal = (proposal: Partial<TAiCreateTrackProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  if ((proposal.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (proposal.status && !MODULE_STATUS_VALUES.includes(proposal.status as (typeof MODULE_STATUS_VALUES)[number]))
    return "Unknown status.";
  const start = proposal.start_date?.trim() ?? "";
  const target = proposal.target_date?.trim() ?? "";
  if (start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (target && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  if ((proposal.members?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} members are allowed.`;
  return null;
};

/** Mirrors `update_track_proposal_from_args` on the backend. */
export const validateUpdateTrackProposal = (proposal: Partial<TAiUpdateTrackProposal>): string | null => {
  if (!proposal.track?.trim()) return "Track is required.";
  const changes = proposal.changes ?? {};
  if (countChangedFields(changes) === 0) return "At least one change is required.";
  if (changes.name != null) {
    const name = changes.name.trim();
    if (!name) return "Name must not be empty.";
    if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  }
  if ((changes.description?.length ?? 0) > PROPOSAL_LIMITS.description)
    return `Description must be at most ${PROPOSAL_LIMITS.description} characters.`;
  if (changes.status && !MODULE_STATUS_VALUES.includes(changes.status as (typeof MODULE_STATUS_VALUES)[number]))
    return "Unknown status.";
  const start = changes.start_date?.trim() ?? "";
  const target = changes.target_date?.trim() ?? "";
  if (changes.start_date != null && start && !ISO_DATE.test(start)) return "Start date must be YYYY-MM-DD.";
  if (changes.target_date != null && target && !ISO_DATE.test(target)) return "Target date must be YYYY-MM-DD.";
  if (start && target && start > target) return "Start date must not be after target date.";
  if ((changes.members?.length ?? 0) > PROPOSAL_LIMITS.refs)
    return `At most ${PROPOSAL_LIMITS.refs} members are allowed.`;
  return null;
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-proposals.test.ts`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposals.test.ts
git commit -m "feat(web): add container proposal types and validators"
```

---

### Task 9: Form engine `ai-proposal-forms.ts`

**Files:**

- Create: `apps/web/core/lib/ai-proposal-forms.ts`
- Create: `apps/web/core/lib/ai-proposal-forms.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/web/core/lib/ai-proposal-forms.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  buildServiceWrite,
  buildSprintWrite,
  buildTrackWrite,
  initialDraftForProposal,
  PROPOSAL_FORM_FIELDS,
  validateFormDraft,
  visibleFields,
} from "./ai-proposal-forms";

describe("PROPOSAL_FORM_FIELDS", () => {
  it("defines a required name for create kinds and update sprint/track", () => {
    for (const kind of ["create_service", "create_sprint", "create_track", "update_sprint", "update_track"] as const) {
      expect(PROPOSAL_FORM_FIELDS[kind].some((field) => field.key === "name" && field.required)).toBe(true);
    }
    // update_service has no name field: the service is a resolve target, not an editable field.
    expect(PROPOSAL_FORM_FIELDS.update_service.some((field) => field.key === "name")).toBe(false);
  });
});

describe("visibleFields", () => {
  it("shows every field for create and only changed fields for update", () => {
    expect(visibleFields("create_service", { project: "LTS", name: "Email" })).toHaveLength(
      PROPOSAL_FORM_FIELDS.create_service.length
    );
    const fields = visibleFields("update_service", {
      service: "Email",
      changes: { status: "deprecated" },
    });
    expect(fields.map((field) => field.key)).toEqual(["status"]);
  });
});

describe("initialDraftForProposal", () => {
  it("fills create drafts from the proposal", () => {
    const draft = initialDraftForProposal("create_track", {
      project: "LTS",
      name: "Onboarding",
      members: ["Budi"],
    });
    expect(draft.name).toBe("Onboarding");
    expect(draft.members).toEqual(["Budi"]);
    expect(draft.lead).toBe("");
  });

  it("fills update drafts from changes only", () => {
    const draft = initialDraftForProposal("update_sprint", {
      sprint: "Sprint 4",
      changes: { end_date: "2026-11-21" },
    });
    expect(draft).toEqual({ end_date: "2026-11-21" });
  });
});

describe("validateFormDraft", () => {
  it("validates create-service drafts", () => {
    expect(validateFormDraft("create_service", { name: "Email", status: "active" })).toBeNull();
    expect(validateFormDraft("create_service", { name: "  " })).toBe("Name is required.");
    expect(validateFormDraft("create_service", { name: "Email", repository_url: "nope" })).toBe(
      "Repository URL must start with http:// or https://."
    );
  });

  it("validates sprint drafts", () => {
    expect(validateFormDraft("create_sprint", { name: "Sprint 4", start_date: "2026-11-01" })).toBe(
      "Provide both dates or neither."
    );
    expect(validateFormDraft("update_sprint", { end_date: "" })).toBe(
      "Sprint dates cannot be cleared; pick a new date."
    );
    expect(validateFormDraft("update_service", { status: "" })).toBe("At least one change is required.");
  });

  it("allows clearing track fields", () => {
    expect(validateFormDraft("update_track", { description: "" })).toBeNull();
    expect(validateFormDraft("update_track", { members: [] })).toBeNull();
  });
});

describe("buildServiceWrite", () => {
  it("builds a create payload with html and owner", () => {
    const data = buildServiceWrite("create_service", { name: "Email", description: "Handles mail" }, "u1");
    expect(data.name).toBe("Email");
    expect(data.description).toBe("Handles mail");
    expect(data.description_html).toBe("<p>Handles mail</p>");
    expect(data.owner_id).toBe("u1");
  });

  it("builds an update payload with clears", () => {
    const changes = buildServiceWrite("update_service", { owner: "", repository_url: "" }, null);
    expect(changes.owner_id).toBeNull();
    expect(changes.repository_url).toBeNull();
    expect(changes.name).toBeUndefined();
  });
});

describe("buildSprintWrite", () => {
  it("keeps single update dates and omits blank ones", () => {
    expect(buildSprintWrite("update_sprint", { end_date: "2026-11-21" })).toEqual({ end_date: "2026-11-21" });
    expect(buildSprintWrite("update_sprint", { end_date: "" })).toEqual({});
  });
});

describe("buildTrackWrite", () => {
  it("builds create payloads and update clears", () => {
    expect(buildTrackWrite("create_track", { name: "Onboarding", members: [] }, null, ["u1", "u2"])).toEqual({
      name: "Onboarding",
      member_ids: ["u1", "u2"],
    });
    expect(buildTrackWrite("update_track", { lead: "", start_date: "" }, null, [])).toEqual({
      lead_id: null,
      start_date: null,
    });
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm test core/lib/ai-proposal-forms.test.ts`
Expected: gagal (file belum ada).

- [ ] **Step 3: Implementasi**

Buat `apps/web/core/lib/ai-proposal-forms.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ICycle, IModule, IService } from "@plane/types";
import {
  MODULE_STATUS_VALUES,
  SERVICE_CRITICALITY_VALUES,
  SERVICE_STATUS_VALUES,
  SERVICE_TYPE_VALUES,
  type TAiCreateServiceProposal,
  type TAiCreateSprintProposal,
  type TAiCreateTrackProposal,
  type TAiUpdateServiceProposal,
  type TAiUpdateSprintProposal,
  type TAiUpdateTrackProposal,
} from "@/lib/ai-proposals";
import { textToDescriptionHtml } from "@/lib/ai-work-items";

export const FORM_PROPOSAL_KINDS = [
  "create_service",
  "update_service",
  "create_sprint",
  "update_sprint",
  "create_track",
  "update_track",
] as const;
export type TFormProposalKind = (typeof FORM_PROPOSAL_KINDS)[number];

export type TFormProposal =
  | TAiCreateServiceProposal
  | TAiUpdateServiceProposal
  | TAiCreateSprintProposal
  | TAiUpdateSprintProposal
  | TAiCreateTrackProposal
  | TAiUpdateTrackProposal;

export type TFormDraftValue = string | string[] | null;
export type TFormDraft = Record<string, TFormDraftValue>;

export type TFormFieldKind = "text" | "textarea" | "select" | "date" | "member" | "members" | "url";

export type TFormField = {
  key: string;
  label: string;
  kind: TFormFieldKind;
  required?: boolean;
  maxLength?: number;
  options?: readonly string[];
};

export const PROPOSAL_FORM_FIELDS: Record<TFormProposalKind, readonly TFormField[]> = {
  create_service: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "status", label: "Status", kind: "select", options: SERVICE_STATUS_VALUES },
    { key: "criticality", label: "Criticality", kind: "select", options: SERVICE_CRITICALITY_VALUES },
    { key: "type", label: "Type", kind: "select", options: SERVICE_TYPE_VALUES },
    { key: "owner", label: "Owner", kind: "member" },
    { key: "repository_url", label: "Repository URL", kind: "url", maxLength: 2048 },
    { key: "documentation_url", label: "Documentation URL", kind: "url", maxLength: 2048 },
  ],
  update_service: [
    { key: "status", label: "Status", kind: "select", options: SERVICE_STATUS_VALUES },
    { key: "criticality", label: "Criticality", kind: "select", options: SERVICE_CRITICALITY_VALUES },
    { key: "type", label: "Type", kind: "select", options: SERVICE_TYPE_VALUES },
    { key: "owner", label: "Owner", kind: "member" },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "repository_url", label: "Repository URL", kind: "url", maxLength: 2048 },
    { key: "documentation_url", label: "Documentation URL", kind: "url", maxLength: 2048 },
  ],
  create_sprint: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "end_date", label: "End date", kind: "date" },
  ],
  update_sprint: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "end_date", label: "End date", kind: "date" },
  ],
  create_track: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "target_date", label: "Target date", kind: "date" },
    { key: "status", label: "Status", kind: "select", options: MODULE_STATUS_VALUES },
    { key: "lead", label: "Lead", kind: "member" },
    { key: "members", label: "Members", kind: "members" },
  ],
  update_track: [
    { key: "name", label: "Name", kind: "text", required: true, maxLength: 255 },
    { key: "description", label: "Description", kind: "textarea", maxLength: 5000 },
    { key: "start_date", label: "Start date", kind: "date" },
    { key: "target_date", label: "Target date", kind: "date" },
    { key: "status", label: "Status", kind: "select", options: MODULE_STATUS_VALUES },
    { key: "lead", label: "Lead", kind: "member" },
    { key: "members", label: "Members", kind: "members" },
  ],
};

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;
const isHttpUrl = (value: string): boolean => value.startsWith("http://") || value.startsWith("https://");

/** Fields to render: all of them for create, only changed fields for update. */
export const visibleFields = (kind: TFormProposalKind, proposal: TFormProposal): readonly TFormField[] => {
  const fields = PROPOSAL_FORM_FIELDS[kind];
  if (!kind.startsWith("update_")) return fields;
  const changes = (proposal as { changes?: Record<string, unknown> }).changes ?? {};
  return fields.filter((field) => changes[field.key] !== null && changes[field.key] !== undefined);
};

/** Draft seeded from the proposal; update kinds seed only their changed fields. */
export const initialDraftForProposal = (kind: TFormProposalKind, proposal: TFormProposal): TFormDraft => {
  const source = (
    kind.startsWith("update_") ? ((proposal as { changes?: Record<string, unknown> }).changes ?? {}) : proposal
  ) as Record<string, unknown>;
  const draft: TFormDraft = {};
  for (const field of visibleFields(kind, proposal)) {
    const value = source[field.key];
    if (field.kind === "members") draft[field.key] = Array.isArray(value) ? value.map(String) : [];
    else if (typeof value === "string") draft[field.key] = value;
    else if (typeof value === "number") draft[field.key] = String(value);
    else draft[field.key] = "";
  }
  return draft;
};

/** Draft-level validation mirroring the backend plus endpoint constraints. */
export const validateFormDraft = (kind: TFormProposalKind, draft: TFormDraft): string | null => {
  const fields = PROPOSAL_FORM_FIELDS[kind];
  for (const field of fields) {
    const value = draft[field.key];
    if (value === undefined) continue;
    if (field.kind === "members") {
      if ((Array.isArray(value) ? value.length : 0) > 10) return "At most 10 members are allowed.";
      continue;
    }
    const text = typeof value === "string" ? value.trim() : "";
    if (field.required && !text) return `${field.label} is required.`;
    if (field.maxLength && text.length > field.maxLength)
      return `${field.label} must be at most ${field.maxLength} characters.`;
    if (field.kind === "url" && text && !isHttpUrl(text)) return `${field.label} must start with http:// or https://.`;
    if (field.kind === "date" && text && !ISO_DATE.test(text)) return `${field.label} must be YYYY-MM-DD.`;
    if (field.kind === "select" && text && field.options && !field.options.includes(text))
      return `Unknown ${field.label.toLowerCase()}.`;
  }
  const start = typeof draft.start_date === "string" ? draft.start_date.trim() : "";
  const endKey = kind.includes("sprint") ? "end_date" : "target_date";
  const end = typeof draft[endKey] === "string" ? (draft[endKey] as string).trim() : "";
  if (start && end && start > end) {
    return kind.includes("sprint")
      ? "Start date must not be after end date."
      : "Start date must not be after target date.";
  }
  if (kind === "create_sprint" && Boolean(start) !== Boolean(end)) return "Provide both dates or neither.";
  if (kind === "update_sprint") {
    if (draft.start_date !== undefined && !start) return "Sprint dates cannot be cleared; pick a new date.";
    if (draft.end_date !== undefined && !end) return "Sprint dates cannot be cleared; pick a new date.";
  }
  if (kind.startsWith("update_")) {
    const hasEffectiveChange = fields.some((field) => {
      const value = draft[field.key];
      if (value === null || value === undefined) return false;
      if (field.kind === "members") return true;
      if (typeof value !== "string") return true;
      return value.trim() !== "" || field.kind !== "select";
    });
    if (!hasEffectiveChange) return "At least one change is required.";
  }
  return null;
};

const draftText = (draft: TFormDraft, key: string): string =>
  typeof draft[key] === "string" ? (draft[key] as string).trim() : "";

export const buildServiceWrite = (
  kind: "create_service" | "update_service",
  draft: TFormDraft,
  ownerId: string | null
): Partial<IService> => {
  const isUpdate = kind === "update_service";
  const data: Partial<IService> = {};
  if (!isUpdate || "name" in draft) data.name = draftText(draft, "name");
  if (!isUpdate || "description" in draft) {
    const description = draftText(draft, "description");
    if (description || isUpdate) {
      data.description = description;
      data.description_html = textToDescriptionHtml(description) ?? "";
    }
  }
  if (draftText(draft, "status")) data.status = draftText(draft, "status") as IService["status"];
  if (draftText(draft, "criticality")) data.criticality = draftText(draft, "criticality") as IService["criticality"];
  if (draftText(draft, "type")) data.type = draftText(draft, "type") as IService["type"];
  if (!isUpdate || "owner" in draft) {
    if (ownerId) data.owner_id = ownerId;
    else if (isUpdate) data.owner_id = null;
  }
  if (!isUpdate || "repository_url" in draft) {
    const url = draftText(draft, "repository_url");
    if (url || isUpdate) data.repository_url = url || null;
  }
  if (!isUpdate || "documentation_url" in draft) {
    const url = draftText(draft, "documentation_url");
    if (url || isUpdate) data.documentation_url = url || null;
  }
  return data;
};

export const buildSprintWrite = (kind: "create_sprint" | "update_sprint", draft: TFormDraft): Partial<ICycle> => {
  const isUpdate = kind === "update_sprint";
  const data: Partial<ICycle> = {};
  if (!isUpdate || "name" in draft) data.name = draftText(draft, "name");
  if (!isUpdate || "description" in draft) {
    const description = draftText(draft, "description");
    if (description || isUpdate) data.description = description;
  }
  if (draftText(draft, "start_date")) data.start_date = draftText(draft, "start_date");
  if (draftText(draft, "end_date")) data.end_date = draftText(draft, "end_date");
  return data;
};

export const buildTrackWrite = (
  kind: "create_track" | "update_track",
  draft: TFormDraft,
  leadId: string | null,
  memberIds: string[]
): Partial<IModule> => {
  const isUpdate = kind === "update_track";
  const data: Partial<IModule> = {};
  if (!isUpdate || "name" in draft) data.name = draftText(draft, "name");
  if (!isUpdate || "description" in draft) {
    const description = draftText(draft, "description");
    if (description || isUpdate) data.description = description;
  }
  if (!isUpdate || "start_date" in draft) {
    const start = draftText(draft, "start_date");
    if (start || isUpdate) data.start_date = start || null;
  }
  if (!isUpdate || "target_date" in draft) {
    const target = draftText(draft, "target_date");
    if (target || isUpdate) data.target_date = target || null;
  }
  if (draftText(draft, "status")) data.status = draftText(draft, "status") as IModule["status"];
  if (!isUpdate || "lead" in draft) {
    if (leadId) data.lead_id = leadId;
    else if (isUpdate) data.lead_id = null;
  }
  if (!isUpdate || "members" in draft) data.member_ids = memberIds;
  return data;
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-proposal-forms.test.ts`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-proposal-forms.ts apps/web/core/lib/ai-proposal-forms.test.ts
git commit -m "feat(web): add proposal form engine"
```

---

### Task 10: Store dispatch enam kind create/update

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-assistant.store.test.ts`, tambahkan mock service di `makeServices`:

```ts
  cycles: {
    createCycle: vi.fn(async (_slug: string, projectId: string) => ({ id: "cy1", project_id: projectId })),
    patchCycle: vi.fn(async () => ({ id: "cy1" })),
    ...overrides.cycles,
  },
```

Perluas mock `services` dan `modules`:

```ts
  services: {
    linkWorkItem: vi.fn(async () => ({ id: "l1" })),
    unlinkWorkItem: vi.fn(async () => undefined),
    createService: vi.fn(async (_slug: string, _workspaceId: string, projectId: string) => ({
      id: "srv1",
      project_id: projectId,
    })),
    updateService: vi.fn(async () => ({ id: "srv1" })),
    ...overrides.services,
  },
  modules: {
    addIssuesToModule: vi.fn(async () => undefined),
    removeIssuesFromModuleBulk: vi.fn(async () => undefined),
    createModule: vi.fn(async (_slug: string, projectId: string) => ({ id: "m1", project_id: projectId })),
    patchModule: vi.fn(async () => ({ id: "m1" })),
    ...overrides.modules,
  },
```

Perbarui `makeStore` agar meneruskan service kedelapan:

```ts
const makeStore = (services = makeServices()) =>
  new AIAssistantStore(
    services.ai as any,
    services.schedules as any,
    services.conversations as any,
    services.issues as any,
    services.comments as any,
    services.services as any,
    services.modules as any,
    services.cycles as any
  );
```

Tambahkan di akhir file:

```ts
const createServiceMetadata = {
  proposals: [
    {
      key: "k6",
      kind: "create_service",
      proposal: { project: "LTS", name: "Email", status: "active" },
    },
  ],
};

const updateServiceMetadata = {
  proposals: [
    {
      key: "k7",
      kind: "update_service",
      proposal: { service: "Email", changes: { status: "deprecated" } },
    },
  ],
};

const createSprintMetadata = {
  proposals: [
    {
      key: "k8",
      kind: "create_sprint",
      proposal: { project: "LTS", name: "Sprint 4" },
    },
  ],
};

const updateSprintMetadata = {
  proposals: [
    {
      key: "k9",
      kind: "update_sprint",
      proposal: { sprint: "Sprint 4", changes: { name: "Sprint 4b" } },
    },
  ],
};

const createTrackMetadata = {
  proposals: [
    {
      key: "k10",
      kind: "create_track",
      proposal: { project: "LTS", name: "Onboarding" },
    },
  ],
};

const updateTrackMetadata = {
  proposals: [
    {
      key: "k11",
      kind: "update_track",
      proposal: { track: "Onboarding", changes: { status: "in-progress" } },
    },
  ],
};

describe("container form proposals", () => {
  const openAgent = async (services = makeServices(), metadata: Record<string, unknown>) => {
    services.conversations.listMessages = vi.fn(async () => [mutationMessage(metadata)]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    return { store, services };
  };

  it("creates a service and stores the created id", async () => {
    const { store, services } = await openAgent(makeServices(), createServiceMetadata);
    await store.confirmProposal("srv-assistant", "k6", {
      kind: "create_service",
      projectId: "p1",
      data: { name: "Email", status: "active" },
    });
    expect(services.services.createService).toHaveBeenCalledWith("acme", "", "p1", {
      name: "Email",
      status: "active",
    });
    expect(store.messages[0].proposalDecisions?.k6).toEqual({
      kind: "create_service",
      decision: "applied",
      result: { created_service_id: "srv1" },
    });
  });

  it("updates a service without a result", async () => {
    const { store, services } = await openAgent(makeServices(), updateServiceMetadata);
    await store.confirmProposal("srv-assistant", "k7", {
      kind: "update_service",
      projectId: "p1",
      serviceId: "s1",
      changes: { status: "deprecated" },
    });
    expect(services.services.updateService).toHaveBeenCalledWith("acme", "", "p1", "s1", {
      status: "deprecated",
    });
    expect(store.messages[0].proposalDecisions?.k7).toEqual({
      kind: "update_service",
      decision: "applied",
    });
  });

  it("creates a sprint and stores the created id", async () => {
    const { store, services } = await openAgent(makeServices(), createSprintMetadata);
    await store.confirmProposal("srv-assistant", "k8", {
      kind: "create_sprint",
      projectId: "p1",
      data: { name: "Sprint 4" },
    });
    expect(services.cycles.createCycle).toHaveBeenCalledWith("acme", "p1", { name: "Sprint 4" });
    expect(store.messages[0].proposalDecisions?.k8).toEqual({
      kind: "create_sprint",
      decision: "applied",
      result: { created_sprint_id: "cy1" },
    });
  });

  it("updates a sprint without a result", async () => {
    const { store, services } = await openAgent(makeServices(), updateSprintMetadata);
    await store.confirmProposal("srv-assistant", "k9", {
      kind: "update_sprint",
      projectId: "p1",
      cycleId: "cy1",
      changes: { name: "Sprint 4b" },
    });
    expect(services.cycles.patchCycle).toHaveBeenCalledWith("acme", "p1", "cy1", { name: "Sprint 4b" });
    expect(store.messages[0].proposalDecisions?.k9).toEqual({
      kind: "update_sprint",
      decision: "applied",
    });
  });

  it("creates a track and stores the created id", async () => {
    const { store, services } = await openAgent(makeServices(), createTrackMetadata);
    await store.confirmProposal("srv-assistant", "k10", {
      kind: "create_track",
      projectId: "p1",
      data: { name: "Onboarding" },
    });
    expect(services.modules.createModule).toHaveBeenCalledWith("acme", "p1", { name: "Onboarding" });
    expect(store.messages[0].proposalDecisions?.k10).toEqual({
      kind: "create_track",
      decision: "applied",
      result: { created_track_id: "m1" },
    });
  });

  it("updates a track without a result", async () => {
    const { store, services } = await openAgent(makeServices(), updateTrackMetadata);
    await store.confirmProposal("srv-assistant", "k11", {
      kind: "update_track",
      projectId: "p1",
      moduleId: "m1",
      changes: { status: "in-progress" },
    });
    expect(services.modules.patchModule).toHaveBeenCalledWith("acme", "p1", "m1", {
      status: "in-progress",
    });
    expect(store.messages[0].proposalDecisions?.k11).toEqual({
      kind: "update_track",
      decision: "applied",
    });
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: gagal kompilasi (dispatch kind baru belum ada).

- [ ] **Step 3: Implementasi store**

Tambahkan import:

```ts
import { CycleService } from "@/services/cycle.service";
```

Perbarui pick type:

```ts
type TIssueService = Pick<IssueService, "createIssue" | "patchIssue" | "addIssueToCycle" | "removeIssueFromCycle">;
type TCommentsService = Pick<IssueCommentService, "createIssueComment">;
type TServicesService = Pick<ServiceService, "linkWorkItem" | "unlinkWorkItem" | "createService" | "updateService">;
type TModulesService = Pick<
  ModuleService,
  "addIssuesToModule" | "removeIssuesFromModuleBulk" | "createModule" | "patchModule"
>;
type TCyclesService = Pick<CycleService, "createCycle" | "patchCycle">;
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
    private modulesService: TModulesService = new ModuleService(),
    private cyclesService: TCyclesService = new CycleService()
  ) {
```

Di `confirmProposal`, sisipkan arm baru setelah blok `add_comment` dan sebelum `manage_service_links`:

```ts
    } else if (payload.kind === "create_service") {
      const created = await this.servicesService.createService(slug, "", payload.projectId, payload.data);
      if (!created?.id) throw new Error("Service creation returned no id");
      result = { created_service_id: created.id };
    } else if (payload.kind === "update_service") {
      await this.servicesService.updateService(slug, "", payload.projectId, payload.serviceId, payload.changes);
    } else if (payload.kind === "create_sprint") {
      const created = await this.cyclesService.createCycle(slug, payload.projectId, payload.data);
      if (!created?.id) throw new Error("Sprint creation returned no id");
      result = { created_sprint_id: created.id };
    } else if (payload.kind === "update_sprint") {
      await this.cyclesService.patchCycle(slug, payload.projectId, payload.cycleId, payload.changes);
    } else if (payload.kind === "create_track") {
      const created = await this.modulesService.createModule(slug, payload.projectId, payload.data);
      if (!created?.id) throw new Error("Track creation returned no id");
      result = { created_track_id: created.id };
    } else if (payload.kind === "update_track") {
      await this.modulesService.patchModule(slug, payload.projectId, payload.moduleId, payload.changes);
    } else if (payload.kind === "manage_service_links") {
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: seluruh test store lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): dispatch container create and update proposals"
```

---

### Task 11: `ProposalFormCard` + dispatcher + render root

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-form-card.tsx`
- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Buat `proposal-form-card.tsx`**

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
import type { ICycle, IModule } from "@plane/types";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useMember } from "@/hooks/store/use-member";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// lib
import { matchAssignees, matchProject } from "@/lib/ai-work-items";
import {
  validateCreateServiceProposal,
  validateCreateSprintProposal,
  validateCreateTrackProposal,
  validateUpdateServiceProposal,
  validateUpdateSprintProposal,
  validateUpdateTrackProposal,
  type TAiCreateServiceProposal,
  type TAiCreateSprintProposal,
  type TAiCreateTrackProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiUpdateServiceProposal,
  type TAiUpdateSprintProposal,
  type TAiUpdateTrackProposal,
} from "@/lib/ai-proposals";
import {
  buildServiceWrite,
  buildSprintWrite,
  buildTrackWrite,
  initialDraftForProposal,
  validateFormDraft,
  visibleFields,
  type TFormDraft,
  type TFormDraftValue,
  type TFormField,
} from "@/lib/ai-proposal-forms";

type FormProposalEntry =
  | { kind: "create_service"; proposal: TAiCreateServiceProposal }
  | { kind: "update_service"; proposal: TAiUpdateServiceProposal }
  | { kind: "create_sprint"; proposal: TAiCreateSprintProposal }
  | { kind: "update_sprint"; proposal: TAiUpdateSprintProposal }
  | { kind: "create_track"; proposal: TAiCreateTrackProposal }
  | { kind: "update_track"; proposal: TAiUpdateTrackProposal };

type CardHandlers = {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

type Props = FormProposalEntry & CardHandlers;
type ServiceProps = Extract<FormProposalEntry, { kind: "create_service" | "update_service" }> & CardHandlers;
type ContainerProps = Extract<
  FormProposalEntry,
  { kind: "create_sprint" | "update_sprint" | "create_track" | "update_track" }
> &
  CardHandlers;
type TMemberOption = { id: string; display_name?: string | null; email?: string | null };

export const ProposalFormCard = (props: Props) => {
  if (props.kind === "create_service" || props.kind === "update_service") return <ServiceFormCard {...props} />;
  return <ContainerFormCard {...props} />;
};

const FormFields = observer(function FormFields({
  fields,
  draft,
  onChange,
  members,
  ownerId,
  onOwnerChange,
  leadId,
  onLeadChange,
  memberIds,
  onToggleMember,
}: {
  fields: readonly TFormField[];
  draft: TFormDraft;
  onChange: (key: string, value: TFormDraftValue) => void;
  members: TMemberOption[];
  ownerId?: string;
  onOwnerChange?: (id: string) => void;
  leadId?: string;
  onLeadChange?: (id: string) => void;
  memberIds?: string[];
  onToggleMember?: (id: string, checked: boolean) => void;
}) {
  return (
    <>
      {fields.map((field) => {
        const value = draft[field.key];
        if (field.kind === "member") {
          const selected = field.key === "owner" ? ownerId : leadId;
          const onSelect = field.key === "owner" ? onOwnerChange : onLeadChange;
          return (
            <label key={field.key} className="mt-2 block text-12 text-secondary">
              {field.label}
              <select
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={selected ?? ""}
                onChange={(event) => onSelect?.(event.target.value)}
              >
                <option value="">No {field.label.toLowerCase()}</option>
                {members.map((member) => (
                  <option key={member.id} value={member.id}>
                    {member.display_name || member.email || member.id}
                  </option>
                ))}
              </select>
            </label>
          );
        }
        if (field.kind === "members") {
          return (
            <div key={field.key} className="mt-2 text-12 text-secondary">
              {field.label}
              <div className="mt-1 flex max-h-24 flex-col gap-1 overflow-y-auto">
                {members.map((member) => (
                  <label key={member.id} className="flex items-center gap-1 text-12 text-primary">
                    <input
                      type="checkbox"
                      checked={(memberIds ?? []).includes(member.id)}
                      onChange={(event) => onToggleMember?.(member.id, event.target.checked)}
                    />
                    {member.display_name || member.email || member.id}
                  </label>
                ))}
              </div>
            </div>
          );
        }
        if (field.kind === "textarea") {
          return (
            <label key={field.key} className="mt-2 block text-12 text-secondary">
              {field.label}
              <textarea
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                rows={3}
                value={typeof value === "string" ? value : ""}
                onChange={(event) => onChange(field.key, event.target.value)}
              />
            </label>
          );
        }
        if (field.kind === "select") {
          return (
            <label key={field.key} className="mt-2 block text-12 text-secondary">
              {field.label}
              <select
                className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
                value={typeof value === "string" ? value : ""}
                onChange={(event) => onChange(field.key, event.target.value)}
              >
                <option value="">Default</option>
                {(field.options ?? []).map((option) => (
                  <option key={option} value={option}>
                    {option.replace(/-/g, " ")}
                  </option>
                ))}
              </select>
            </label>
          );
        }
        return (
          <label key={field.key} className="mt-2 block text-12 text-secondary">
            {field.label}
            <input
              type={field.kind === "date" ? "date" : field.kind === "url" ? "url" : "text"}
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={typeof value === "string" ? value : ""}
              onChange={(event) => onChange(field.key, event.target.value)}
            />
          </label>
        );
      })}
    </>
  );
});

const ServiceFormCard = observer(function ServiceFormCard(props: ServiceProps) {
  const { decision, onConfirm, onCancel } = props;
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { currentWorkspace } = useWorkspace();
  const workspaceId = currentWorkspace?.id;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchServices, getProjectServiceIds, getServiceById } = useService();
  const { project: projectMemberStore } = useMember();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedServiceId, setSelectedServiceId] = useState("");
  const [draft, setDraft] = useState<TFormDraft>(() => initialDraftForProposal(props.kind, props.proposal));
  const [ownerId, setOwnerId] = useState("");
  const [missing, setMissing] = useState<string[]>([]);
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isUpdate = props.kind === "update_service";
  const serviceRef = props.kind === "update_service" ? props.proposal.service : "";
  const projectRef = props.kind === "update_service" ? (props.proposal.project ?? "") : props.proposal.project;
  const projectIdsKey = (workspaceProjectIds ?? []).join("|");
  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));

  useEffect(() => {
    if (!isUpdate || !workspaceSlug || !workspaceId) return;
    const ids = projectIdsKey.split("|").filter(Boolean);
    const pending = ids.filter((id) => getProjectServiceIds(id) === null);
    if (pending.length === 0) return;
    void Promise.all(pending.map((id) => fetchServices(workspaceSlug, workspaceId, id)));
  }, [isUpdate, workspaceSlug, workspaceId, projectIdsKey, getProjectServiceIds, fetchServices]);

  const serviceCandidates = isUpdate
    ? (workspaceProjectIds ?? []).flatMap((projectId) =>
        (getProjectServiceIds(projectId) ?? []).flatMap((serviceId) => {
          const service = getServiceById(serviceId);
          return service ? [{ id: service.id, name: service.name, projectId }] : [];
        })
      )
    : [];
  const needle = serviceRef.trim().toLowerCase();
  const matches = isUpdate
    ? serviceCandidates.filter(
        (candidate) => candidate.id === serviceRef.trim() || candidate.name.toLowerCase() === needle
      )
    : [];
  const uniqueMatch = matches.length === 1 ? matches[0] : undefined;
  const matchedProject = projectRef ? matchProject(projects, projectRef) : undefined;
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? uniqueMatch?.projectId ?? null;
  const serviceInProject = effectiveProjectId
    ? serviceCandidates.find(
        (candidate) =>
          candidate.projectId === effectiveProjectId &&
          (candidate.id === serviceRef.trim() || candidate.name.toLowerCase() === needle)
      )
    : undefined;
  const fallbackServiceId = !selectedProjectId && !matchedProject && uniqueMatch ? uniqueMatch.id : "";
  const effectiveServiceId = selectedServiceId || serviceInProject?.id || fallbackServiceId || "";
  const serviceOptions = effectiveProjectId
    ? serviceCandidates.filter((candidate) => candidate.projectId === effectiveProjectId)
    : [];

  useEffect(() => {
    if (!workspaceSlug || !effectiveProjectId) return;
    if (!projectMemberStore.getProjectMemberFetchStatus(effectiveProjectId))
      void projectMemberStore.fetchProjectMembers(workspaceSlug, effectiveProjectId);
  }, [workspaceSlug, effectiveProjectId, projectMemberStore]);

  const memberIds = effectiveProjectId ? (projectMemberStore.getProjectMemberIds(effectiveProjectId, true) ?? []) : [];
  const members: TMemberOption[] = memberIds
    .map((id) => ({ id, details: projectMemberStore.getProjectMemberDetails(id, effectiveProjectId ?? "") }))
    .filter((entry) => Boolean(entry.details))
    .map((entry) => ({
      id: entry.id,
      display_name: entry.details?.member.display_name,
      email: entry.details?.member.email,
    }));

  const ownerRef =
    props.kind === "update_service" ? (props.proposal.changes.owner ?? "") : (props.proposal.owner ?? "");
  const resolutionKey = [effectiveProjectId ?? "", members.length, ownerRef].join("::");
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    const result = matchAssignees(members, ownerRef ? [ownerRef] : []);
    setOwnerId(result.matched[0] ?? "");
    setMissing(result.missing.map((name) => `Owner not found: ${name}`));
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {isUpdate ? "Service updated." : "Service created."}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {isUpdate ? "Service update cancelled." : "Service creation cancelled."}
      </p>
    );
  }

  const fields = visibleFields(props.kind, props.proposal);
  const proposalError =
    props.kind === "create_service"
      ? validateCreateServiceProposal(props.proposal)
      : validateUpdateServiceProposal(props.proposal);
  const draftError = validateFormDraft(props.kind, draft);
  const targetReady = isUpdate ? Boolean(effectiveProjectId && effectiveServiceId) : Boolean(effectiveProjectId);

  const confirm = async () => {
    if (!effectiveProjectId || !targetReady) return;
    setSubmitting(true);
    setError(null);
    try {
      if (props.kind === "create_service") {
        await onConfirm({
          kind: "create_service",
          projectId: effectiveProjectId,
          data: buildServiceWrite("create_service", draft, ownerId || null),
        });
      } else {
        await onConfirm({
          kind: "update_service",
          projectId: effectiveProjectId,
          serviceId: effectiveServiceId,
          changes: buildServiceWrite("update_service", draft, ownerId || null),
        });
      }
    } catch {
      setError("Could not save the service. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Service proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-12 font-medium text-primary">
        {isUpdate ? `Update service ${serviceRef}` : `Create service ${props.proposal.name}`}
      </p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => {
              setSelectedProjectId(event.target.value || null);
              setSelectedServiceId("");
            }}
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
      {isUpdate && effectiveProjectId && (
        <label className="mt-2 block text-12 text-secondary">
          Service
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={effectiveServiceId}
            onChange={(event) => setSelectedServiceId(event.target.value)}
          >
            <option value="">Select a service</option>
            {serviceOptions.map((service) => (
              <option key={service.id} value={service.id}>
                {service.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <FormFields
        fields={fields}
        draft={draft}
        onChange={(key, value) => setDraft((current) => ({ ...current, [key]: value }))}
        members={members}
        ownerId={ownerId}
        onOwnerChange={setOwnerId}
      />
      {missing.map((notice) => (
        <p key={notice} className="mt-1 text-12 text-tertiary">
          {notice} — pick one manually.
        </p>
      ))}
      {proposalError && <p className="mt-1 text-12 text-danger-primary">{proposalError}</p>}
      {draftError && <p className="mt-1 text-12 text-danger-primary">{draftError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!targetReady || Boolean(proposalError) || Boolean(draftError)}
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

const ContainerFormCard = observer(function ContainerFormCard(props: ContainerProps) {
  const { decision, onConfirm, onCancel } = props;
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { workspaceProjectIds, getProjectById } = useProject();
  const { fetchWorkspaceCycles } = useCycle();
  const { fetchWorkspaceModules } = useModule();
  const { project: projectMemberStore } = useMember();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedContainerId, setSelectedContainerId] = useState("");
  const [containers, setContainers] = useState<{ id: string; name: string; projectId: string }[]>([]);
  const [draft, setDraft] = useState<TFormDraft>(() => initialDraftForProposal(props.kind, props.proposal));
  const [leadId, setLeadId] = useState("");
  const [memberIds, setMemberIds] = useState<string[]>([]);
  const [missing, setMissing] = useState<string[]>([]);
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isSprint = props.kind === "create_sprint" || props.kind === "update_sprint";
  const isTrack = props.kind === "create_track" || props.kind === "update_track";
  const isUpdate = props.kind === "update_sprint" || props.kind === "update_track";
  const containerLabel = isSprint ? "Sprint" : "Track";
  const containerRef =
    props.kind === "update_sprint" ? props.proposal.sprint : props.kind === "update_track" ? props.proposal.track : "";
  const projectRef =
    props.kind === "create_sprint" || props.kind === "create_track"
      ? props.proposal.project
      : (props.proposal.project ?? "");
  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));

  useEffect(() => {
    if (!isUpdate || !workspaceSlug) return;
    let cancelled = false;
    void (async () => {
      try {
        const fetched: (ICycle | IModule)[] = isSprint
          ? await fetchWorkspaceCycles(workspaceSlug)
          : await fetchWorkspaceModules(workspaceSlug);
        if (cancelled) return;
        setContainers(
          fetched.map((container) => ({ id: container.id, name: container.name, projectId: container.project_id }))
        );
      } catch {
        if (!cancelled) setContainers([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isUpdate, isSprint, workspaceSlug, fetchWorkspaceCycles, fetchWorkspaceModules]);

  const needle = containerRef.trim().toLowerCase();
  const matches = isUpdate
    ? containers.filter((container) => container.id === containerRef.trim() || container.name.toLowerCase() === needle)
    : [];
  const uniqueMatch = matches.length === 1 ? matches[0] : undefined;
  const matchedProject = projectRef ? matchProject(projects, projectRef) : undefined;
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? uniqueMatch?.projectId ?? null;
  const containerInProject = effectiveProjectId
    ? containers.find(
        (container) =>
          container.projectId === effectiveProjectId &&
          (container.id === containerRef.trim() || container.name.toLowerCase() === needle)
      )
    : undefined;
  const fallbackContainerId = !selectedProjectId && !matchedProject && uniqueMatch ? uniqueMatch.id : "";
  const effectiveContainerId = selectedContainerId || containerInProject?.id || fallbackContainerId || "";
  const containerOptions = effectiveProjectId
    ? containers.filter((container) => container.projectId === effectiveProjectId)
    : [];

  useEffect(() => {
    if (!isTrack || !workspaceSlug || !effectiveProjectId) return;
    if (!projectMemberStore.getProjectMemberFetchStatus(effectiveProjectId))
      void projectMemberStore.fetchProjectMembers(workspaceSlug, effectiveProjectId);
  }, [isTrack, workspaceSlug, effectiveProjectId, projectMemberStore]);

  const memberIdsForProject =
    isTrack && effectiveProjectId ? (projectMemberStore.getProjectMemberIds(effectiveProjectId, true) ?? []) : [];
  const members: TMemberOption[] = memberIdsForProject
    .map((id) => ({ id, details: projectMemberStore.getProjectMemberDetails(id, effectiveProjectId ?? "") }))
    .filter((entry) => Boolean(entry.details))
    .map((entry) => ({
      id: entry.id,
      display_name: entry.details?.member.display_name,
      email: entry.details?.member.email,
    }));

  const leadRef =
    props.kind === "create_track"
      ? (props.proposal.lead ?? "")
      : props.kind === "update_track"
        ? (props.proposal.changes.lead ?? "")
        : "";
  const memberRefs =
    props.kind === "create_track"
      ? (props.proposal.members ?? [])
      : props.kind === "update_track"
        ? (props.proposal.changes.members ?? [])
        : [];
  const resolutionKey = [effectiveProjectId ?? "", members.length, leadRef, memberRefs.join("|")].join("::");
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    const leadResult = matchAssignees(members, leadRef ? [leadRef] : []);
    setLeadId(leadResult.matched[0] ?? "");
    const memberResult = matchAssignees(members, memberRefs);
    setMemberIds(memberResult.matched);
    setMissing([
      ...leadResult.missing.map((name) => `Lead not found: ${name}`),
      ...memberResult.missing.map((name) => `Member not found: ${name}`),
    ]);
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} {isUpdate ? "updated." : "created."}
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        {containerLabel} {isUpdate ? "update cancelled." : "creation cancelled."}
      </p>
    );
  }

  const fields = visibleFields(props.kind, props.proposal);
  const proposalError =
    props.kind === "create_sprint"
      ? validateCreateSprintProposal(props.proposal)
      : props.kind === "update_sprint"
        ? validateUpdateSprintProposal(props.proposal)
        : props.kind === "create_track"
          ? validateCreateTrackProposal(props.proposal)
          : validateUpdateTrackProposal(props.proposal);
  const draftError = validateFormDraft(props.kind, draft);
  const targetReady = isUpdate ? Boolean(effectiveProjectId && effectiveContainerId) : Boolean(effectiveProjectId);

  const confirm = async () => {
    if (!effectiveProjectId || !targetReady) return;
    setSubmitting(true);
    setError(null);
    try {
      if (props.kind === "create_sprint") {
        await onConfirm({
          kind: "create_sprint",
          projectId: effectiveProjectId,
          data: buildSprintWrite("create_sprint", draft),
        });
      } else if (props.kind === "update_sprint") {
        await onConfirm({
          kind: "update_sprint",
          projectId: effectiveProjectId,
          cycleId: effectiveContainerId,
          changes: buildSprintWrite("update_sprint", draft),
        });
      } else if (props.kind === "create_track") {
        await onConfirm({
          kind: "create_track",
          projectId: effectiveProjectId,
          data: buildTrackWrite("create_track", draft, leadId || null, memberIds),
        });
      } else {
        await onConfirm({
          kind: "update_track",
          projectId: effectiveProjectId,
          moduleId: effectiveContainerId,
          changes: buildTrackWrite("update_track", draft, leadId || null, memberIds),
        });
      }
    } catch {
      setError(`Could not save the ${containerLabel.toLowerCase()}. Please retry.`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label={`${containerLabel} proposal`}
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">
        {isUpdate
          ? `Update ${containerLabel.toLowerCase()} ${containerRef}`
          : `Create ${containerLabel.toLowerCase()} ${props.proposal.name}`}
      </p>
      {!effectiveProjectId && projects.length > 0 && (
        <label className="mt-2 block text-12 text-secondary">
          Project
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={selectedProjectId ?? ""}
            onChange={(event) => {
              setSelectedProjectId(event.target.value || null);
              setSelectedContainerId("");
            }}
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
      {isUpdate && effectiveProjectId && (
        <label className="mt-2 block text-12 text-secondary">
          {containerLabel}
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={effectiveContainerId}
            onChange={(event) => setSelectedContainerId(event.target.value)}
          >
            <option value="">Select a {containerLabel.toLowerCase()}</option>
            {containerOptions.map((container) => (
              <option key={container.id} value={container.id}>
                {container.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {isSprint && !isUpdate && (
        <p className="mt-1 text-12 text-tertiary">The sprint owner is you (the confirming user).</p>
      )}
      <FormFields
        fields={fields}
        draft={draft}
        onChange={(key, value) => setDraft((current) => ({ ...current, [key]: value }))}
        members={members}
        leadId={leadId}
        onLeadChange={setLeadId}
        memberIds={memberIds}
        onToggleMember={(id, checked) =>
          setMemberIds((current) => (checked ? [...current, id] : current.filter((value) => value !== id)))
        }
      />
      {missing.map((notice) => (
        <p key={notice} className="mt-1 text-12 text-tertiary">
          {notice} — pick one manually.
        </p>
      ))}
      {proposalError && <p className="mt-1 text-12 text-danger-primary">{proposalError}</p>}
      {draftError && <p className="mt-1 text-12 text-danger-primary">{draftError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!targetReady || Boolean(proposalError) || Boolean(draftError)}
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

- [ ] **Step 2: Buat `proposal-card.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TAiProposal, TAiProposalConfirmPayload, TAiProposalDecision } from "@/lib/ai-proposals";
import { CommentProposalCard } from "./comment-proposal-card";
import { LinkItemsProposalCard } from "./link-items-proposal-card";
import { ProposalFormCard } from "./proposal-form-card";
import { WorkItemUpdateProposalCard } from "./work-item-update-proposal-card";

type Props = {
  entry: TAiProposal;
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const ProposalCard = ({ entry, decision, onConfirm, onCancel }: Props) => {
  if (entry.kind === "update_work_item")
    return (
      <WorkItemUpdateProposalCard
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "add_comment")
    return (
      <CommentProposalCard proposal={entry.proposal} decision={decision} onConfirm={onConfirm} onCancel={onCancel} />
    );
  if (entry.kind === "manage_service_links")
    return (
      <LinkItemsProposalCard
        kind="manage_service_links"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "manage_sprint_items")
    return (
      <LinkItemsProposalCard
        kind="manage_sprint_items"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "manage_track_items")
    return (
      <LinkItemsProposalCard
        kind="manage_track_items"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_service")
    return (
      <ProposalFormCard
        kind="create_service"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "update_service")
    return (
      <ProposalFormCard
        kind="update_service"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_sprint")
    return (
      <ProposalFormCard
        kind="create_sprint"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "update_sprint")
    return (
      <ProposalFormCard
        kind="update_sprint"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  if (entry.kind === "create_track")
    return (
      <ProposalFormCard
        kind="create_track"
        proposal={entry.proposal}
        decision={decision}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  return (
    <ProposalFormCard
      kind="update_track"
      proposal={entry.proposal}
      decision={decision}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
};
```

- [ ] **Step 3: Wire ke `root.tsx`**

Ganti tiga import kartu proposal lama:

```tsx
import { CommentProposalCard } from "./proposals/comment-proposal-card";
import { LinkItemsProposalCard } from "./proposals/link-items-proposal-card";
import { WorkItemUpdateProposalCard } from "./proposals/work-item-update-proposal-card";
```

menjadi satu import dispatcher:

```tsx
import { ProposalCard } from "./proposals/proposal-card";
```

Ganti seluruh blok `{message.proposals?.map((entry) => { ... })}` (IIFE dengan cabang per kind) menjadi:

```tsx
{
  message.proposals?.map((entry) => (
    <ProposalCard
      key={entry.key}
      entry={entry}
      decision={message.proposalDecisions?.[entry.key]}
      onConfirm={(payload) => confirmProposal(message.id, entry.key, payload)}
      onCancel={() => cancelProposal(message.id, entry.key)}
    />
  ));
}
```

- [ ] **Step 4: Verifikasi tipe**

Run (dari `apps/web`): `pnpm check:types`
Expected: lulus tanpa error baru.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/proposals/proposal-form-card.tsx apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): render container proposal form cards"
```

---

### Task 12: Verifikasi akhir Fase 3A

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
Revert file non-AI yang ikut berubah (pola Fase 2), lalu commit bila formatter mengubah file AI:

```bash
git status --short
git checkout -- <file-non-AI-yang-tidak-sengaja-berubah>
git add apps/api-rs/crates/ai apps/api-rs/crates/api apps/web/core/lib apps/web/core/store apps/web/core/components/ai
git commit -m "chore: format fase 3a"
```

- [ ] **Step 5: Smoke manual (opsional, butuh stack hidup)**

Ikuti AGENTS.md: rebuild `pnpm --filter=web build`, restart `plane-web-prod.service`, lalu di UI Agent mode:

1. "buat service Email di LTS" → kartu create service; Confirm → service muncul di halaman Services.
2. "ubah status service Email jadi deprecated" → kartu update service; Confirm.
3. "buat sprint Sprint 4 di LTS dari 1 sampai 14 November" → kartu create sprint (owner = Anda); Confirm.
4. "tambahkan Budi sebagai lead track Onboarding" → kartu update track; Confirm.
5. Scheduled run tetap tidak bisa memutasi (proposal tool tidak ada di `SPEC_TOOLS`).

---

## Self-Review Checklist (sudah dijalankan)

- **Spec coverage:** §7.1 enam tool create/update container → Task 1–4; §4.4 kind mapping + `PROPOSAL_TOOL_NAMES` + result id → Task 5–6; §7.3 form engine berbasis schema + test validasi → Task 8–9; §8 testing (unit Rust, integrasi roundtrip, vitest, worker tidak berubah) → Task 1–12; §4.5 resolusi nama unik + select manual saat ambigu → kartu (Task 11); §4.7 PREAMBLE → Task 5.
- **Deviasi terdokumentasi:** sprint owner dihapus (endpoint mengabaikan), track create tanpa HTML, semantik clear `changes`, result id wajib untuk kind create, update_service tanpa name.
- **Placeholder scan:** tidak ada TODO/TBD; semua langkah berisi kode lengkap.
- **Type consistency:** `CreateServiceArgs`/`TAiCreateServiceProposal`, `UpdateServiceChanges`/`TAiUpdateServiceChanges`, `CreateSprintArgs`/`TAiCreateSprintProposal`, `UpdateSprintChanges`/`TAiUpdateSprintChanges`, `CreateTrackArgs`/`TAiCreateTrackProposal`, `UpdateTrackChanges`/`TAiUpdateTrackChanges`; `PROPOSAL_TOOL_NAMES` 13 entri; `allowed_result_keys`/`required_result_key` cocok dengan `TAiProposalDecisionResult`; builder `buildServiceWrite`/`buildSprintWrite`/`buildTrackWrite` dipakai kartu dan store dengan payload arm yang sama; `proposal-card.tsx` menge-cover 11 kind tanpa `jsx-key`/TS2783 (key di `ProposalCard`, bukan spread).
