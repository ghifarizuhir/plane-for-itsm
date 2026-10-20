# Galileo Tools Fase 3C — Triage Intake (Apply + Actions + Triage Card) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah dua proposal tool Fase 3 — `apply_triage_suggestion` dan `triage_intake_item` — plus kartu bespoke `TriageProposalCard` yang menampilkan suggestion existing, memilih field, dan mengeksekusi apply/accept/reject/snooze/duplicate lewat endpoint inbox existing saat Confirm.

**Architecture:** Semua tool hidup di `tools/mutations.rs` (lanjutan Plan 2A/2B/3A/3B) dan hanya memvalidasi bentuk + mencatat trace. Pipeline AI triage existing (`ai.intake.triage` + `intake_triage_suggestions`) tetap satu-satunya generator suggestion; Galileo hanya membaca (Fase 1) dan meng-apply lewat Confirm. Kartu me-resolve work item ref → issue id + project id, memuat inbox issue + suggestion dari `projectInbox` store, lalu store dispatch memanggil `InboxIssueService`.

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), React + MobX + vitest.

**Prasyarat:** Plan 2A/2B/3A (dan idealnya 3B) sudah dieksekusi (`PROPOSAL_TOOL_NAMES` 15 entri, `ProposalCard` dispatcher). Bila 3B belum dieksekusi, angka `PROPOSAL_TOOL_NAMES` di plan ini (17) harus disesuaikan.

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§7.2 dua tool triage, §4.4 decision tanpa result, §4.5, §7.3 kartu triage bespoke, §8).

**Deviasi dari spec (diputuskan saat menyusun plan, karena endpoint existing):**

1. **`intake_item` adalah work item identifier (`PROJ-123`), bukan UUID.** Semua endpoint inbox (`detail`, `PATCH`, `triage-suggestion`, `destroy`) me-resolve pk lewat `resolve_inbox_row` yang mencocokkan `ii.issue_id` — yaitu id work item, bukan id baris intake (`routes/intake.rs:872-892`). Read tool `get_intake_item` mengembalikan `id` (row id) dan `identifier` (`PROJ-42`); hanya `identifier` yang bisa di-resolve FE ke issue id + project id lewat `fetchIssueWithIdentifier`.
2. **`duplicate_of` adalah work item identifier (`PROJ-123`)**, di-resolve kartu ke issue id untuk body `duplicate_to` (`routes/intake.rs:1256`).
3. **`snoozed_till` wajib RFC3339** saat `action: snooze` (`DateTime<Utc>`, `routes/intake.rs:1258`); dilarang untuk action lain.
4. **Tanpa result id**: decision `applied`/`cancelled` saja (kartu menampilkan status).
5. **Accept bisa gagal gate** (category/service tidak lagi tersedia, `routes/intake.rs:2154-2215`): error endpoint ditampilkan di kartu, decision tetap `pending` (retryable).

---

## File Structure

| File                                                                               | Aksi | Tanggung jawab                                            |
| ---------------------------------------------------------------------------------- | ---- | --------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/tools/mutations.rs`                                     | Ubah | 2 tool + `mutation_kind_for_tool`                         |
| `apps/api-rs/crates/ai/src/tools/mod.rs`                                           | Ubah | `PROPOSAL_TOOL_NAMES` 17 entri, registrasi, test metadata |
| `apps/api-rs/crates/ai/src/agent.rs`                                               | Ubah | `PREAMBLE` triage + test `pending_actions`                |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                            | Ubah | Allowlist tanpa result                                    |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`                                    | Ubah | Roundtrip 2 kind                                          |
| `apps/web/core/lib/ai-proposals.ts`                                                | Ubah | 2 kind, tipe, validator                                   |
| `apps/web/core/lib/ai-proposals.test.ts`                                           | Ubah | Test validator                                            |
| `apps/web/core/store/ai-assistant.store.ts`                                        | Ubah | Dispatch triage via `InboxIssueService`                   |
| `apps/web/core/store/ai-assistant.store.test.ts`                                   | Ubah | Test dispatch                                             |
| `apps/web/core/components/ai/assistant-sidebar/proposals/triage-proposal-card.tsx` | Baru | Kartu triage + suggestion                                 |
| `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`        | Ubah | 2 cabang baru                                             |

---

### Task 1: Tool `apply_triage_suggestion`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn apply_triage_suggestion_normalizes_fields() {
        let proposal = apply_triage_suggestion_proposal_from_args(ApplyTriageSuggestionArgs {
            intake_item: " lts-42 ".into(),
            fields: Some(vec![" Category ".into(), "category".into(), "severity".into()]),
        })
        .unwrap();
        assert_eq!(proposal.intake_item, "lts-42");
        assert_eq!(
            proposal.fields,
            vec!["category".to_string(), "severity".to_string()]
        );

        let defaulted = apply_triage_suggestion_proposal_from_args(ApplyTriageSuggestionArgs {
            intake_item: "LTS-42".into(),
            fields: None,
        })
        .unwrap();
        assert_eq!(
            defaulted.fields,
            vec!["category".to_string(), "service".to_string(), "severity".to_string()]
        );

        let bad_ref = apply_triage_suggestion_proposal_from_args(ApplyTriageSuggestionArgs {
            intake_item: "nope".into(),
            fields: None,
        })
        .unwrap_err();
        assert!(bad_ref.to_string().contains("PROJ-123"));

        let bad_field = apply_triage_suggestion_proposal_from_args(ApplyTriageSuggestionArgs {
            intake_item: "LTS-42".into(),
            fields: Some(vec!["needs_human".into()]),
        })
        .unwrap_err();
        assert!(bad_field.to_string().contains("category"));

        let empty = apply_triage_suggestion_proposal_from_args(ApplyTriageSuggestionArgs {
            intake_item: "LTS-42".into(),
            fields: Some(vec![]),
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p ai apply_triage_suggestion_normalizes`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `UpdateArticle`:

```rust
pub const APPLY_TRIAGE_SUGGESTION_NAME: &str = "apply_triage_suggestion";
pub const TRIAGE_INTAKE_ITEM_NAME: &str = "triage_intake_item";
pub const TRIAGE_APPLY_FIELDS: [&str; 3] = ["category", "service", "severity"];
pub const TRIAGE_ACTIONS: [&str; 4] = ["accept", "reject", "snooze", "duplicate"];

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyTriageSuggestionArgs {
    /// Work item identifier of the intake item, like "LTS-42".
    pub intake_item: String,
    /// Fields to apply, a subset of: category, service, severity. Defaults to all.
    pub fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApplyTriageSuggestionProposal {
    pub intake_item: String,
    pub fields: Vec<String>,
}

/// Validate raw tool args into a normalized apply-suggestion proposal.
pub fn apply_triage_suggestion_proposal_from_args(
    args: ApplyTriageSuggestionArgs,
) -> Result<ApplyTriageSuggestionProposal, ToolExecutionError> {
    let intake_item = args.intake_item.trim();
    if intake_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("intake_item is required"));
    }
    parse_work_item_ref(intake_item)?;
    let fields = match args.fields {
        None => TRIAGE_APPLY_FIELDS
            .iter()
            .map(|field| field.to_string())
            .collect(),
        Some(values) => {
            if values.is_empty() {
                return Err(ToolExecutionError::invalid_args(
                    "at least one field is required",
                ));
            }
            let mut seen: Vec<String> = Vec::new();
            let mut out: Vec<String> = Vec::new();
            for value in values {
                let normalized = enum_arg(&value, &TRIAGE_APPLY_FIELDS, "fields")?;
                if seen.contains(&normalized) {
                    continue;
                }
                seen.push(normalized.clone());
                out.push(normalized);
            }
            out
        }
    };
    Ok(ApplyTriageSuggestionProposal {
        intake_item: intake_item.to_string(),
        fields,
    })
}

pub struct ApplyTriageSuggestion {
    pub trace: ToolTrace,
}

impl Tool for ApplyTriageSuggestion {
    const NAME: &'static str = APPLY_TRIAGE_SUGGESTION_NAME;
    type Args = ApplyTriageSuggestionArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose applying the existing AI triage suggestion of one intake \
         item. Only call this when the user asks to apply the suggestion. The \
         intake item is identified by its work item identifier like LTS-42, \
         never guess it. Fields default to category, service, and severity. \
         The user must confirm the card in the UI first, so never claim the \
         suggestion was applied before that confirmation."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ApplyTriageSuggestionArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = apply_triage_suggestion_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("ApplyTriageSuggestionProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai apply_triage_suggestion_normalizes`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add apply_triage_suggestion proposal tool"
```

---

### Task 2: Tool `triage_intake_item`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn triage_intake_item_validates_action_specific_args() {
        let accept = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: " LTS-42 ".into(),
            action: " ACCEPT ".into(),
            snoozed_till: None,
            duplicate_of: None,
        })
        .unwrap();
        assert_eq!(accept.intake_item, "LTS-42");
        assert_eq!(accept.action, "accept");
        assert_eq!(accept.snoozed_till, None);

        let snooze = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "snooze".into(),
            snoozed_till: Some("2026-11-01T09:00:00Z".into()),
            duplicate_of: None,
        })
        .unwrap();
        assert_eq!(snooze.snoozed_till.as_deref(), Some("2026-11-01T09:00:00+00:00"));

        let duplicate = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "duplicate".into(),
            snoozed_till: None,
            duplicate_of: Some(" LTS-7 ".into()),
        })
        .unwrap();
        assert_eq!(duplicate.duplicate_of.as_deref(), Some("LTS-7"));

        let missing_snooze = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "snooze".into(),
            snoozed_till: None,
            duplicate_of: None,
        })
        .unwrap_err();
        assert!(missing_snooze.to_string().contains("snoozed_till"));

        let stray_snooze = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "accept".into(),
            snoozed_till: Some("2026-11-01T09:00:00Z".into()),
            duplicate_of: None,
        })
        .unwrap_err();
        assert!(stray_snooze.to_string().contains("only"));

        let bad_date = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "snooze".into(),
            snoozed_till: Some("2026-11-01".into()),
            duplicate_of: None,
        })
        .unwrap_err();
        assert!(bad_date.to_string().contains("RFC3339"));

        let missing_duplicate = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "duplicate".into(),
            snoozed_till: None,
            duplicate_of: None,
        })
        .unwrap_err();
        assert!(missing_duplicate.to_string().contains("duplicate_of"));

        let bad_action = triage_intake_item_proposal_from_args(TriageIntakeItemArgs {
            intake_item: "LTS-42".into(),
            action: "archive".into(),
            snoozed_till: None,
            duplicate_of: None,
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("accept"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai triage_intake_item_validates`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `ApplyTriageSuggestion`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TriageIntakeItemArgs {
    /// Work item identifier of the intake item, like "LTS-42".
    pub intake_item: String,
    /// One of: accept, reject, snooze, duplicate.
    pub action: String,
    /// RFC3339 datetime, required for snooze and not allowed otherwise.
    pub snoozed_till: Option<String>,
    /// Work item identifier like "LTS-42", required for duplicate.
    pub duplicate_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TriageIntakeItemProposal {
    pub intake_item: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snoozed_till: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
}

/// Validate raw tool args into a normalized triage-action proposal.
pub fn triage_intake_item_proposal_from_args(
    args: TriageIntakeItemArgs,
) -> Result<TriageIntakeItemProposal, ToolExecutionError> {
    let intake_item = args.intake_item.trim();
    if intake_item.is_empty() {
        return Err(ToolExecutionError::invalid_args("intake_item is required"));
    }
    parse_work_item_ref(intake_item)?;
    let action = enum_arg(&args.action, &TRIAGE_ACTIONS, "action")?;
    let snoozed_till = match args.snoozed_till.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(
            chrono::DateTime::parse_from_rfc3339(raw)
                .map_err(|_| {
                    ToolExecutionError::invalid_args("snoozed_till must be an RFC3339 datetime")
                })?
                .to_rfc3339(),
        ),
    };
    let duplicate_of = match args.duplicate_of.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => {
            parse_work_item_ref(raw)?;
            Some(raw.to_string())
        }
    };
    if action == "snooze" {
        if snoozed_till.is_none() {
            return Err(ToolExecutionError::invalid_args(
                "snoozed_till is required when action is snooze",
            ));
        }
    } else if snoozed_till.is_some() {
        return Err(ToolExecutionError::invalid_args(
            "snoozed_till is only allowed when action is snooze",
        ));
    }
    if action == "duplicate" {
        if duplicate_of.is_none() {
            return Err(ToolExecutionError::invalid_args(
                "duplicate_of is required when action is duplicate",
            ));
        }
    } else if duplicate_of.is_some() {
        return Err(ToolExecutionError::invalid_args(
            "duplicate_of is only allowed when action is duplicate",
        ));
    }
    Ok(TriageIntakeItemProposal {
        intake_item: intake_item.to_string(),
        action,
        snoozed_till,
        duplicate_of,
    })
}

pub struct TriageIntakeItem {
    pub trace: ToolTrace,
}

impl Tool for TriageIntakeItem {
    const NAME: &'static str = TRIAGE_INTAKE_ITEM_NAME;
    type Args = TriageIntakeItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose accepting, rejecting, snoozing, or marking one intake item as \
         duplicate. Only call this when the user asks to triage the item. The \
         intake item is identified by its work item identifier like LTS-42, \
         never guess it. Snooze requires an RFC3339 snoozed_till; duplicate \
         requires a duplicate_of work item identifier. The user must confirm \
         the card in the UI first, so never claim the item was triaged before \
         that confirmation."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<TriageIntakeItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = triage_intake_item_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("TriageIntakeItemProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai triage_intake_item_validates`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add triage_intake_item proposal tool"
```

---

### Task 3: Registry, `PROPOSAL_TOOL_NAMES`, PREAMBLE

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mod.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`
- Modify: `apps/api-rs/crates/ai/src/agent.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
    #[tokio::test]
    async fn triage_tools_expose_metadata() {
        let apply = ApplyTriageSuggestion {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(ApplyTriageSuggestion::NAME, "apply_triage_suggestion");
        assert!(apply.parameters()["properties"]["fields"].is_object());
        assert!(!apply.description().is_empty());

        let triage = TriageIntakeItem {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(TriageIntakeItem::NAME, "triage_intake_item");
        assert!(triage.parameters()["properties"]["action"].is_object());
    }
```

Tambahkan di `mod tests` `agent.rs`:

```rust
    #[test]
    fn pending_actions_covers_triage_mutations() {
        let trace = new_trace();
        record(
            &trace,
            crate::tools::APPLY_TRIAGE_SUGGESTION_NAME,
            &json!({"intake_item": "LTS-42", "fields": ["category"]}),
        );
        record(
            &trace,
            crate::tools::TRIAGE_INTAKE_ITEM_NAME,
            &json!({"intake_item": "LTS-42", "action": "accept"}),
        );
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["kind"], json!("apply_triage_suggestion"));
        assert_eq!(actions[1]["kind"], json!("triage_intake_item"));
    }
```

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn mutation_kind_mapping_covers_triage_tools() {
        assert_eq!(
            mutation_kind_for_tool(APPLY_TRIAGE_SUGGESTION_NAME),
            Some("apply_triage_suggestion")
        );
        assert_eq!(
            mutation_kind_for_tool(TRIAGE_INTAKE_ITEM_NAME),
            Some("triage_intake_item")
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pending_actions_covers_triage`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Di `tools/mod.rs`, perbarui `PROPOSAL_TOOL_NAMES` menjadi 17 entri:

```rust
pub const PROPOSAL_TOOL_NAMES: [&str; 17] = [
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
    CREATE_ARTICLE_NAME,
    UPDATE_ARTICLE_NAME,
    APPLY_TRIAGE_SUGGESTION_NAME,
    TRIAGE_INTAKE_ITEM_NAME,
];
```

Di `workspace_tools`, setelah `.tool(UpdateArticle { ... })` dan sebelum `.tool(CreateSchedule { ... })`:

```rust
        .tool(ApplyTriageSuggestion {
            trace: trace.clone(),
        })
        .tool(TriageIntakeItem {
            trace: trace.clone(),
        })
```

Di `mutations.rs`, perbarui `mutation_kind_for_tool`:

```rust
        CREATE_ARTICLE_NAME => Some("create_article"),
        UPDATE_ARTICLE_NAME => Some("update_article"),
        APPLY_TRIAGE_SUGGESTION_NAME => Some("apply_triage_suggestion"),
        TRIAGE_INTAKE_ITEM_NAME => Some("triage_intake_item"),
        _ => None,
```

Di `agent.rs`, tambahkan setelah paragraf artikel di `PREAMBLE`:

```text
To apply an intake item's existing AI triage suggestion, call \
apply_triage_suggestion with the item's work item identifier. To accept, \
reject, snooze, or mark an intake item as duplicate, call \
triage_intake_item with the identifier and the action. Never invent an \
intake item or a suggestion. These are proposals: the user must confirm \
them in the UI first, so never claim the item was triaged before that \
confirmation.
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/tools/mutations.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): register triage proposal tools"
```

---

### Task 4: Allowlist decision tanpa result

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_conversations.rs`:

```rust
    #[test]
    fn metadata_patch_accepts_triage_decisions() {
        let apply_key = Uuid::new_v4().to_string();
        let triage_key = Uuid::new_v4().to_string();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({
                (apply_key.clone()): {"kind": "apply_triage_suggestion", "decision": "applied"},
                (triage_key.clone()): {"kind": "triage_intake_item", "decision": "cancelled"},
            }),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][apply_key.as_str()]["decision"],
            json!("applied")
        );
        assert_eq!(
            clean["proposal_decisions"][triage_key.as_str()]["decision"],
            json!("cancelled")
        );

        let result_on_apply = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(apply_key.clone()): {
                "kind": "apply_triage_suggestion",
                "decision": "applied",
                "result": {"created_comment_id": Uuid::new_v4()},
            }}),
        )]))
        .unwrap_err();
        assert!(result_on_apply.contains("not allowed"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --lib metadata_patch_accepts_triage`
Expected: gagal (`kind not allowed: apply_triage_suggestion`).

- [ ] **Step 3: Implementasi**

Perbarui `allowed_result_keys` untuk menambahkan kedua kind ke daftar tanpa result:

```rust
        "update_work_item"
        | "update_service"
        | "update_sprint"
        | "update_track"
        | "update_article"
        | "manage_service_links"
        | "manage_sprint_items"
        | "manage_track_items"
        | "apply_triage_suggestion"
        | "triage_intake_item" => Some(&[]),
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --lib metadata_patch`
Expected: seluruh test metadata lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): accept triage proposal decisions"
```

---

### Task 5: Integration test roundtrip dua kind

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tambahkan dua test DB-backed**

Tambahkan di akhir file:

```rust
#[tokio::test]
async fn apply_triage_suggestion_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "apply_triage_suggestion",
        r#"{"intake_item":"LTS-42","fields":["category","severity"]}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "apply the triage suggestion for LTS-42",
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
    assert_eq!(proposals[0]["kind"], json!("apply_triage_suggestion"));
    assert_eq!(proposals[0]["proposal"]["intake_item"], json!("LTS-42"));
    assert_eq!(proposals[0]["proposal"]["fields"][1], json!("severity"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "apply_triage_suggestion", "decision": "applied"}
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
async fn triage_intake_item_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "triage_intake_item",
        r#"{"intake_item":"LTS-42","action":"snooze","snoozed_till":"2026-11-01T09:00:00Z"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "snooze LTS-42 until November",
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
    assert_eq!(proposals[0]["kind"], json!("triage_intake_item"));
    assert_eq!(proposals[0]["proposal"]["action"], json!("snooze"));
    assert_eq!(
        proposals[0]["proposal"]["snoozed_till"],
        json!("2026-11-01T09:00:00+00:00")
    );

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan test**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus (butuh DB dev + Redis).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): roundtrip triage proposal metadata"
```

---

### Task 6: FE `ai-proposals.ts` — 2 kind baru + validator

**Files:**

- Modify: `apps/web/core/lib/ai-proposals.ts`
- Modify: `apps/web/core/lib/ai-proposals.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Perbarui blok import di `ai-proposals.test.ts` dengan menambahkan:

```ts
  validateApplyTriageSuggestionProposal,
  validateTriageIntakeItemProposal,
```

Tambahkan di akhir file:

```ts
describe("triage proposal validators", () => {
  it("validates apply-suggestion proposals", () => {
    expect(
      validateApplyTriageSuggestionProposal({ intake_item: "LTS-42", fields: ["category", "severity"] })
    ).toBeNull();
    expect(validateApplyTriageSuggestionProposal({ intake_item: "LTS-42", fields: [] })).toBe(
      "At least one field is required."
    );
    expect(validateApplyTriageSuggestionProposal({ intake_item: "nope", fields: ["category"] })).toBe(
      "Intake item must look like PROJ-123."
    );
    expect(validateApplyTriageSuggestionProposal({ intake_item: "LTS-42", fields: ["needs_human" as never] })).toBe(
      "Unknown field: needs_human."
    );
  });

  it("validates triage-action proposals", () => {
    expect(validateTriageIntakeItemProposal({ intake_item: "LTS-42", action: "accept" })).toBeNull();
    expect(
      validateTriageIntakeItemProposal({
        intake_item: "LTS-42",
        action: "snooze",
        snoozed_till: "2026-11-01T09:00:00Z",
      })
    ).toBeNull();
    expect(
      validateTriageIntakeItemProposal({ intake_item: "LTS-42", action: "duplicate", duplicate_of: "LTS-7" })
    ).toBeNull();
    expect(validateTriageIntakeItemProposal({ intake_item: "LTS-42", action: "snooze" })).toBe(
      "Snoozed till is required for snooze."
    );
    expect(
      validateTriageIntakeItemProposal({
        intake_item: "LTS-42",
        action: "accept",
        snoozed_till: "2026-11-01T09:00:00Z",
      })
    ).toBe("Snoozed till is only allowed for snooze.");
    expect(validateTriageIntakeItemProposal({ intake_item: "LTS-42", action: "duplicate" })).toBe(
      "Duplicate of is required for duplicate."
    );
    expect(
      validateTriageIntakeItemProposal({ intake_item: "LTS-42", action: "snooze", snoozed_till: "2026-11-01" })
    ).toBe("Snoozed till must be an RFC3339 datetime.");
    expect(validateTriageIntakeItemProposal({ intake_item: "LTS-42", action: "archive" as never })).toBe(
      "Action must be accept, reject, snooze, or duplicate."
    );
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
  "create_service",
  "update_service",
  "create_sprint",
  "update_sprint",
  "create_track",
  "update_track",
  "create_article",
  "update_article",
  "apply_triage_suggestion",
  "triage_intake_item",
] as const;
```

Tambahkan konstanta:

```ts
export const TRIAGE_FIELD_VALUES = ["category", "service", "severity"] as const;
export const TRIAGE_ACTION_VALUES = ["accept", "reject", "snooze", "duplicate"] as const;
export type TTriageField = (typeof TRIAGE_FIELD_VALUES)[number];
export type TTriageAction = (typeof TRIAGE_ACTION_VALUES)[number];
```

Tambahkan tipe (setelah `TAiUpdateArticleProposal`):

```ts
export type TAiApplyTriageSuggestionProposal = {
  intake_item: string;
  fields: TTriageField[];
};

export type TAiTriageIntakeItemProposal = {
  intake_item: string;
  action: TTriageAction;
  snoozed_till?: string | null;
  duplicate_of?: string | null;
};
```

Perbarui union `TAiProposal`:

```ts
  | { key: string; kind: "create_article"; proposal: TAiCreateArticleProposal }
  | { key: string; kind: "update_article"; proposal: TAiUpdateArticleProposal }
  | { key: string; kind: "apply_triage_suggestion"; proposal: TAiApplyTriageSuggestionProposal }
  | { key: string; kind: "triage_intake_item"; proposal: TAiTriageIntakeItemProposal };
```

Perbarui `TAiProposalConfirmPayload` dengan dua arm baru:

```ts
  | {
      kind: "apply_triage_suggestion";
      projectId: string;
      issueId: string;
      fields: TTriageField[];
    }
  | {
      kind: "triage_intake_item";
      projectId: string;
      issueId: string;
      action: TTriageAction;
      snoozedTill?: string;
      duplicateToIssueId?: string;
    };
```

Tambahkan validator di akhir file:

```ts
const WORK_ITEM_REF_RE = /^[A-Za-z0-9]+-\d+$/;
const RFC3339_RE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?(\.\d+)?(Z|[+-]\d{2}:\d{2})$/;

/** Mirrors `apply_triage_suggestion_proposal_from_args` on the backend. */
export const validateApplyTriageSuggestionProposal = (
  proposal: Partial<TAiApplyTriageSuggestionProposal>
): string | null => {
  const intakeItem = proposal.intake_item?.trim() ?? "";
  if (!intakeItem) return "Intake item is required.";
  if (!WORK_ITEM_REF_RE.test(intakeItem)) return "Intake item must look like PROJ-123.";
  const fields = proposal.fields ?? [];
  if (fields.length === 0) return "At least one field is required.";
  const unknown = fields.find((field) => !TRIAGE_FIELD_VALUES.includes(field));
  if (unknown) return `Unknown field: ${unknown}.`;
  return null;
};

/** Mirrors `triage_intake_item_proposal_from_args` on the backend. */
export const validateTriageIntakeItemProposal = (proposal: Partial<TAiTriageIntakeItemProposal>): string | null => {
  const intakeItem = proposal.intake_item?.trim() ?? "";
  if (!intakeItem) return "Intake item is required.";
  if (!WORK_ITEM_REF_RE.test(intakeItem)) return "Intake item must look like PROJ-123.";
  if (!TRIAGE_ACTION_VALUES.includes(proposal.action as TTriageAction))
    return "Action must be accept, reject, snooze, or duplicate.";
  const snoozedTill = proposal.snoozed_till?.trim() ?? "";
  const duplicateOf = proposal.duplicate_of?.trim() ?? "";
  if (proposal.action === "snooze") {
    if (!snoozedTill) return "Snoozed till is required for snooze.";
    if (!RFC3339_RE.test(snoozedTill)) return "Snoozed till must be an RFC3339 datetime.";
  } else if (snoozedTill) {
    return "Snoozed till is only allowed for snooze.";
  }
  if (proposal.action === "duplicate") {
    if (!duplicateOf) return "Duplicate of is required for duplicate.";
    if (!WORK_ITEM_REF_RE.test(duplicateOf)) return "Duplicate of must look like PROJ-123.";
  } else if (duplicateOf) {
    return "Duplicate of is only allowed for duplicate.";
  }
  return null;
};
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-proposals.test.ts`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposals.test.ts
git commit -m "feat(web): add triage proposal types and validators"
```

---

### Task 7: Store dispatch triage

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-assistant.store.test.ts`, tambahkan mock `inbox` di `makeServices`:

```ts
  inbox: {
    applyTriageSuggestion: vi.fn(async () => ({ id: "ts1", status: "ready" })),
    update: vi.fn(async () => ({ id: "ii1" })),
    ...overrides.inbox,
  },
```

Perbarui `makeStore` agar meneruskan service kesepuluh:

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
    services.cycles as any,
    services.pages as any,
    services.inbox as any
  );
```

Tambahkan di akhir file:

```ts
const applyTriageMetadata = {
  proposals: [
    {
      key: "k14",
      kind: "apply_triage_suggestion",
      proposal: { intake_item: "LTS-42", fields: ["category", "severity"] },
    },
  ],
};

const triageAcceptMetadata = {
  proposals: [
    {
      key: "k15",
      kind: "triage_intake_item",
      proposal: { intake_item: "LTS-42", action: "accept" },
    },
  ],
};

const triageSnoozeMetadata = {
  proposals: [
    {
      key: "k16",
      kind: "triage_intake_item",
      proposal: { intake_item: "LTS-42", action: "snooze", snoozed_till: "2026-11-01T09:00:00Z" },
    },
  ],
};

const triageDuplicateMetadata = {
  proposals: [
    {
      key: "k17",
      kind: "triage_intake_item",
      proposal: { intake_item: "LTS-42", action: "duplicate", duplicate_of: "LTS-7" },
    },
  ],
};

describe("triage proposals", () => {
  const openAgent = async (services = makeServices(), metadata: Record<string, unknown>) => {
    services.conversations.listMessages = vi.fn(async () => [mutationMessage(metadata)]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    return { store, services };
  };

  it("applies triage suggestion fields", async () => {
    const { store, services } = await openAgent(makeServices(), applyTriageMetadata);
    await store.confirmProposal("srv-assistant", "k14", {
      kind: "apply_triage_suggestion",
      projectId: "p1",
      issueId: "i1",
      fields: ["category", "severity"],
    });
    expect(services.inbox.applyTriageSuggestion).toHaveBeenCalledWith("acme", "p1", "i1", ["category", "severity"]);
    expect(store.messages[0].proposalDecisions?.k14).toEqual({
      kind: "apply_triage_suggestion",
      decision: "applied",
    });
  });

  it("accepts an intake item", async () => {
    const { store, services } = await openAgent(makeServices(), triageAcceptMetadata);
    await store.confirmProposal("srv-assistant", "k15", {
      kind: "triage_intake_item",
      projectId: "p1",
      issueId: "i1",
      action: "accept",
    });
    expect(services.inbox.update).toHaveBeenCalledWith("acme", "p1", "i1", { status: 1 });
  });

  it("snoozes an intake item with an ISO date", async () => {
    const { store, services } = await openAgent(makeServices(), triageSnoozeMetadata);
    await store.confirmProposal("srv-assistant", "k16", {
      kind: "triage_intake_item",
      projectId: "p1",
      issueId: "i1",
      action: "snooze",
      snoozedTill: "2026-11-01T09:00:00Z",
    });
    expect(services.inbox.update).toHaveBeenCalledWith("acme", "p1", "i1", {
      status: 0,
      snoozed_till: new Date("2026-11-01T09:00:00Z"),
    });
  });

  it("marks an intake item as duplicate", async () => {
    const { store, services } = await openAgent(makeServices(), triageDuplicateMetadata);
    await store.confirmProposal("srv-assistant", "k17", {
      kind: "triage_intake_item",
      projectId: "p1",
      issueId: "i1",
      action: "duplicate",
      duplicateToIssueId: "i7",
    });
    expect(services.inbox.update).toHaveBeenCalledWith("acme", "p1", "i1", {
      status: 2,
      duplicate_to: "i7",
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
import { EInboxIssueStatus, type TInboxIssueStatus } from "@plane/types";
import { InboxIssueService } from "@/services/inbox/inbox-issue.service";
```

Perbarui pick type dan constructor:

```ts
type TInboxService = Pick<InboxIssueService, "applyTriageSuggestion" | "update">;
```

```ts
    private pagesService: TPagesService = new ProjectPageService(),
    private inboxService: TInboxService = new InboxIssueService()
  ) {
```

Tambahkan map status di atas class (setelah `const activeStorageKey`):

```ts
const TRIAGE_STATUS_BY_ACTION: Record<"accept" | "reject" | "snooze" | "duplicate", TInboxIssueStatus> = {
  accept: EInboxIssueStatus.ACCEPTED,
  reject: EInboxIssueStatus.DECLINED,
  snooze: EInboxIssueStatus.SNOOZED,
  duplicate: EInboxIssueStatus.DUPLICATE,
};
```

Di `confirmProposal`, sisipkan arm baru setelah `update_article` dan sebelum `manage_service_links`:

```ts
    } else if (payload.kind === "apply_triage_suggestion") {
      await this.inboxService.applyTriageSuggestion(slug, payload.projectId, payload.issueId, payload.fields);
    } else if (payload.kind === "triage_intake_item") {
      await this.inboxService.update(slug, payload.projectId, payload.issueId, {
        status: TRIAGE_STATUS_BY_ACTION[payload.action],
        ...(payload.snoozedTill ? { snoozed_till: new Date(payload.snoozedTill) } : {}),
        ...(payload.duplicateToIssueId ? { duplicate_to: payload.duplicateToIssueId } : {}),
      });
    } else if (payload.kind === "manage_service_links") {
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: seluruh test store lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): dispatch triage proposals"
```

---

### Task 8: `TriageProposalCard` + dispatcher

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/triage-proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`

- [ ] **Step 1: Buat `triage-proposal-card.tsx`**

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
import type { TIssue } from "@plane/types";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useProjectInbox } from "@/hooks/store/use-project-inbox";
// lib
import { parseWorkItemRef } from "@/lib/ai-work-items";
import {
  TRIAGE_FIELD_VALUES,
  validateApplyTriageSuggestionProposal,
  validateTriageIntakeItemProposal,
  type TAiApplyTriageSuggestionProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiTriageIntakeItemProposal,
  type TTriageAction,
  type TTriageField,
} from "@/lib/ai-proposals";

type TriageProposalEntry =
  | { kind: "apply_triage_suggestion"; proposal: TAiApplyTriageSuggestionProposal }
  | { kind: "triage_intake_item"; proposal: TAiTriageIntakeItemProposal };

type Props = TriageProposalEntry & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const TriageProposalCard = (props: Props) => {
  if (props.kind === "apply_triage_suggestion") return <ApplyTriageCard {...props} />;
  return <TriageActionCard {...props} />;
};

const ACTION_LABELS: Record<TTriageAction, string> = {
  accept: "Accept",
  reject: "Reject",
  snooze: "Snooze",
  duplicate: "Duplicate",
};

const useIntakeResolution = (intakeItem: string) => {
  const { workspaceSlug: rawSlug } = useParams<{ workspaceSlug: string }>();
  const workspaceSlug = Array.isArray(rawSlug) ? rawSlug[0] : rawSlug;
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const { fetchInboxIssueById, getIssueInboxByIssueId } = useProjectInbox();
  const [issue, setIssue] = useState<TIssue | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceSlug) return;
    const ref = parseWorkItemRef(intakeItem);
    if (!ref) {
      setResolveError("Intake item reference is invalid.");
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const fetched = await fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId);
        if (!cancelled) setIssue(fetched);
      } catch {
        if (!cancelled) setResolveError("Intake item not found or not accessible.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceSlug, intakeItem, fetchIssueWithIdentifier]);

  useEffect(() => {
    if (!workspaceSlug || !issue || !issue.project_id) return;
    void fetchInboxIssueById(workspaceSlug, issue.project_id, issue.id).catch(() => undefined);
  }, [workspaceSlug, issue, fetchInboxIssueById]);

  const inboxStore = issue ? getIssueInboxByIssueId(issue.id) : undefined;
  useEffect(() => {
    if (inboxStore && !inboxStore.triageSuggestionFetched) void inboxStore.fetchTriageSuggestion();
  }, [inboxStore]);

  return { workspaceSlug, issue, inboxStore, resolveError };
};

type ApplyProps = Extract<TriageProposalEntry, { kind: "apply_triage_suggestion" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const ApplyTriageCard = observer(function ApplyTriageCard({ proposal, decision, onConfirm, onCancel }: ApplyProps) {
  const { issue, inboxStore, resolveError } = useIntakeResolution(proposal.intake_item);
  const [selectedFields, setSelectedFields] = useState<TTriageField[]>([]);
  const [syncedKey, setSyncedKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const suggestion = inboxStore?.triageSuggestion ?? null;
  const availableFields = TRIAGE_FIELD_VALUES.filter((field) => {
    if (!suggestion) return false;
    if (field === "category") return Boolean(suggestion.category);
    if (field === "service") return Boolean(suggestion.service);
    return Boolean(suggestion.severity);
  });
  const resolutionKey = [issue?.id ?? "", suggestion?.id ?? "", availableFields.join("|")].join("::");
  if (syncedKey !== resolutionKey) {
    setSyncedKey(resolutionKey);
    setSelectedFields(
      availableFields.filter(
        (field) => !suggestion?.applied_fields.includes(field) && !suggestion?.dismissed_fields.includes(field)
      )
    );
  }

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Triage suggestion applied.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Triage suggestion cancelled.
      </p>
    );
  }

  const validationError = validateApplyTriageSuggestionProposal({
    intake_item: proposal.intake_item,
    fields: selectedFields,
  });
  const confirm = async () => {
    if (!issue || !issue.project_id || selectedFields.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "apply_triage_suggestion",
        projectId: issue.project_id,
        issueId: issue.id,
        fields: selectedFields,
      });
    } catch {
      setError("Could not apply the suggestion. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Triage suggestion proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Apply triage suggestion for {proposal.intake_item}</p>
      {issue && <p className="mt-0.5 text-12 text-tertiary">{issue.name}</p>}
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      {!suggestion && !resolveError && <p className="mt-1 text-12 text-tertiary">No ready suggestion found.</p>}
      {suggestion && (
        <div className="mt-2 flex flex-col gap-1">
          {TRIAGE_FIELD_VALUES.map((field) => {
            const available = availableFields.includes(field);
            const applied = suggestion.applied_fields.includes(field);
            const dismissed = suggestion.dismissed_fields.includes(field);
            const label =
              field === "category"
                ? suggestion.category?.label
                : field === "service"
                  ? suggestion.service?.label
                  : suggestion.severity?.priority;
            return (
              <label key={field} className="flex items-center gap-1 text-12 text-primary">
                <input
                  type="checkbox"
                  checked={selectedFields.includes(field)}
                  disabled={!available || applied || dismissed}
                  onChange={(event) =>
                    setSelectedFields((current) =>
                      event.target.checked ? [...current, field] : current.filter((value) => value !== field)
                    )
                  }
                />
                {field}: {label ?? "—"}
                {applied && <span className="text-tertiary"> (already applied)</span>}
                {dismissed && <span className="text-tertiary"> (dismissed)</span>}
                {!available && !applied && !dismissed && <span className="text-tertiary"> (unavailable)</span>}
              </label>
            );
          })}
        </div>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || !issue.project_id || selectedFields.length === 0 || Boolean(validationError)}
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

type ActionProps = Extract<TriageProposalEntry, { kind: "triage_intake_item" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const TriageActionCard = observer(function TriageActionCard({ proposal, decision, onConfirm, onCancel }: ActionProps) {
  const { workspaceSlug, issue, resolveError } = useIntakeResolution(proposal.intake_item);
  const { fetchIssueWithIdentifier } = useIssueDetail();
  const [action, setAction] = useState<TTriageAction>(proposal.action);
  const [snoozedTill, setSnoozedTill] = useState(proposal.snoozed_till ?? "");
  const [duplicateOf, setDuplicateOf] = useState(proposal.duplicate_of ?? "");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Intake item{" "}
        {proposal.action === "accept"
          ? "accepted"
          : proposal.action === "reject"
            ? "rejected"
            : proposal.action === "snooze"
              ? "snoozed"
              : "marked as duplicate"}
        .
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Triage action cancelled.
      </p>
    );
  }

  const draft = {
    intake_item: proposal.intake_item,
    action,
    ...(action === "snooze" ? { snoozed_till: snoozedTill } : {}),
    ...(action === "duplicate" ? { duplicate_of: duplicateOf } : {}),
  };
  const validationError = validateTriageIntakeItemProposal(draft);

  const confirm = async () => {
    if (!issue || !issue.project_id || !workspaceSlug) return;
    setSubmitting(true);
    setError(null);
    try {
      let duplicateToIssueId: string | undefined;
      if (action === "duplicate") {
        const ref = parseWorkItemRef(duplicateOf);
        if (!ref) {
          setError("Duplicate target must look like PROJ-123.");
          return;
        }
        const target = await fetchIssueWithIdentifier(workspaceSlug, ref.projectIdentifier, ref.sequenceId);
        duplicateToIssueId = target.id;
      }
      await onConfirm({
        kind: "triage_intake_item",
        projectId: issue.project_id,
        issueId: issue.id,
        action,
        ...(action === "snooze" ? { snoozedTill: new Date(snoozedTill).toISOString() } : {}),
        ...(duplicateToIssueId ? { duplicateToIssueId } : {}),
      });
    } catch {
      setError("Could not triage the intake item. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Intake triage proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Triage {proposal.intake_item}</p>
      {issue && <p className="mt-0.5 text-12 text-tertiary">{issue.name}</p>}
      {resolveError && <p className="mt-1 text-12 text-danger-primary">{resolveError}</p>}
      <label className="mt-2 block text-12 text-secondary">
        Action
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={action}
          onChange={(event) => setAction(event.target.value as TTriageAction)}
        >
          {(["accept", "reject", "snooze", "duplicate"] as const).map((value) => (
            <option key={value} value={value}>
              {ACTION_LABELS[value]}
            </option>
          ))}
        </select>
      </label>
      {action === "snooze" && (
        <label className="mt-2 block text-12 text-secondary">
          Snooze until
          <input
            type="datetime-local"
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={snoozedTill}
            onChange={(event) => setSnoozedTill(event.target.value)}
          />
        </label>
      )}
      {action === "duplicate" && (
        <label className="mt-2 block text-12 text-secondary">
          Duplicate of
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            placeholder="LTS-42"
            value={duplicateOf}
            onChange={(event) => setDuplicateOf(event.target.value)}
          />
        </label>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!issue || !issue.project_id || Boolean(validationError)}
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

- [ ] **Step 2: Wire ke `proposal-card.tsx`**

Tambahkan import:

```tsx
import { TriageProposalCard } from "./triage-proposal-card";
```

Tambahkan cabang sebelum `return <ProposalFormCard kind="update_track" ...>`:

```tsx
if (entry.kind === "apply_triage_suggestion")
  return (
    <TriageProposalCard
      kind="apply_triage_suggestion"
      proposal={entry.proposal}
      decision={decision}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
if (entry.kind === "triage_intake_item")
  return (
    <TriageProposalCard
      kind="triage_intake_item"
      proposal={entry.proposal}
      decision={decision}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
```

- [ ] **Step 3: Verifikasi tipe**

Run (dari `apps/web`): `pnpm check:types`
Expected: lulus tanpa error baru.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/proposals/triage-proposal-card.tsx apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx
git commit -m "feat(web): render triage proposal cards"
```

---

### Task 9: Verifikasi akhir Fase 3C

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
Revert file non-AI yang ikut berubah (pola Fase 3A), lalu commit bila formatter mengubah file AI:

```bash
git status --short
git checkout -- <file-non-AI-yang-tidak-sengaja-berubah>
git add apps/api-rs/crates/ai apps/api-rs/crates/api apps/web/core/lib apps/web/core/store apps/web/core/components/ai
git commit -m "chore: format fase 3c"
```

- [ ] **Step 5: Smoke manual (opsional, butuh stack hidup)**

Ikuti AGENTS.md: rebuild `pnpm --filter=web build`, restart `plane-web-prod.service`, lalu di UI Agent mode:

1. "apply saran triage untuk LTS-42" → kartu apply dengan checkbox field; Confirm → priority/type/service item berubah.
2. "snooze LTS-42 sampai 1 November" → kartu triage dengan tanggal; Confirm → item masuk snoozed.
3. "tandai LTS-42 duplikat dari LTS-7" → kartu triage duplicate; Confirm.
4. Scheduled run tetap tidak bisa memutasi (proposal tool tidak ada di `SPEC_TOOLS`).

---

## Self-Review Checklist (sudah dijalankan)

- **Spec coverage:** §7.2 dua tool triage → Task 1–2; §4.4 decision tanpa result → Task 4; §7.3 kartu triage bespoke (suggestion + subset fields + aksi) → Task 8; §4.5 resolusi target → kartu via `fetchIssueWithIdentifier`; §8 testing → Task 1–9.
- **Deviasi terdokumentasi:** `intake_item`/`duplicate_of` sebagai work item ref (`resolve_inbox_row` memakai issue id), `snoozed_till` RFC3339 wajib untuk snooze, tanpa result id, gate error accept retryable.
- **Placeholder scan:** tidak ada TODO/TBD; semua langkah berisi kode lengkap.
- **Type consistency:** `ApplyTriageSuggestionArgs`/`TAiApplyTriageSuggestionProposal`, `TriageIntakeItemArgs`/`TAiTriageIntakeItemProposal`, `PROPOSAL_TOOL_NAMES` 17 entri, `allowed_result_keys` menambahkan kedua kind, payload arm `apply_triage_suggestion`/`triage_intake_item` dipakai kartu dan store, dispatcher 15 cabang dengan `key` di `ProposalCard`.
