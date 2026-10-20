# Galileo Tools Fase 3B — Artikel KB (Create/Update + Preview Card) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah dua proposal tool Fase 3 — `create_article` dan `update_article` — plus kartu bespoke `ArticleProposalCard` dengan preview HTML yang mengeksekusi create/update lewat endpoint page existing saat Confirm.

**Architecture:** Semua tool hidup di `tools/mutations.rs` (lanjutan Plan 2A/2B/3A) dan hanya memvalidasi bentuk + mencatat trace; konten proposal adalah teks polos. FE mengonversi teks → HTML (`textToDescriptionHtml`) dan, untuk update, HTML → payload dokumen lengkap (`convertHTMLDocumentToAllFormats` dari `@plane/editor`) agar binary/json/HTML konsisten. `create_article` cukup mengirim `description_html`; live server (Hocuspocus) mengonversi HTML → binary saat dokumen pertama dibuka (`apps/live/src/extensions/database.ts:33-57`).

**Tech Stack:** Rust (axum, sqlx, Rig 0.42, serde/schemars), React + MobX + vitest.

**Prasyarat:** Plan 2A/2B/3A sudah dieksekusi (`PROPOSAL_TOOL_NAMES` 13 entri, `ProposalCard` dispatcher, form engine).

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§7.1 `create_article`/`update_article`, §7.3 preview artikel, §4.4 result id, §8).

**Deviasi dari spec (diputuskan saat menyusun plan, karena endpoint existing):**

1. **`content` adalah teks polos di proposal**; FE mengubahnya ke HTML saat Confirm (`textToDescriptionHtml`). Endpoint `POST .../pages/` menerima `description_html`; `PATCH .../pages/:id/description/` menerima `description_binary`/`description_html`/`description_json` opsional (COALESCE, `routes/page.rs:2387-2407`).
2. **`update_article` mendapat arg opsional `project`.** Page bersifat project-scoped dan tidak ada endpoint workspace-level; kartu butuh project untuk memanggil `PATCH .../pages/:id/` (fallback: select project).
3. **`access` di tool bernilai `public`/`private`**, dipetakan ke `EPageAccess` 0/1 (`packages/types/src/enums.ts:22-25`).
4. **Create tidak menulis binary**: `POST .../pages/` menyimpan `description_binary = NULL` + `description_html`; live server mengonversi saat dokumen dibuka dan menyimpannya kembali.
5. **Update konten menulis ketiga format** (binary+html+json) hasil `convertHTMLDocumentToAllFormats`, supaya editor memuat konten baru (binary lama tidak boleh dibiarkan stale).
6. **Result id wajib saat applied**: `created_article_id` (pola `created_comment_id`).

---

## File Structure

| File                                                                                | Aksi | Tanggung jawab                                            |
| ----------------------------------------------------------------------------------- | ---- | --------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/tools/mutations.rs`                                      | Ubah | 2 tool + `mutation_kind_for_tool`                         |
| `apps/api-rs/crates/ai/src/tools/mod.rs`                                            | Ubah | `PROPOSAL_TOOL_NAMES` 15 entri, registrasi, test metadata |
| `apps/api-rs/crates/ai/src/agent.rs`                                                | Ubah | `PREAMBLE` artikel + test `pending_actions`               |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                             | Ubah | Allowlist `created_article_id` + wajib saat applied       |
| `apps/api-rs/crates/api/tests/ai_agent_test.rs`                                     | Ubah | Roundtrip 2 kind                                          |
| `apps/web/core/lib/ai-proposals.ts`                                                 | Ubah | 2 kind, tipe, validator                                   |
| `apps/web/core/lib/ai-proposals.test.ts`                                            | Ubah | Test validator                                            |
| `apps/web/core/store/ai-assistant.store.ts`                                         | Ubah | Dispatch artikel via `ProjectPageService`                 |
| `apps/web/core/store/ai-assistant.store.test.ts`                                    | Ubah | Test dispatch                                             |
| `apps/web/core/components/ai/assistant-sidebar/proposals/article-proposal-card.tsx` | Baru | Kartu artikel + preview                                   |
| `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`         | Ubah | 2 cabang baru                                             |

---

### Task 1: Tool `create_article`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn create_article_normalizes_and_validates() {
        let proposal = create_article_proposal_from_args(CreateArticleArgs {
            project: " LTS ".into(),
            name: " Runbook: restart API ".into(),
            content: " Step 1\nStep 2 ".into(),
            parent_article: Some(" 0f3f3f3f-0000-0000-0000-000000000001 ".into()),
            access: Some(" PRIVATE ".into()),
        })
        .unwrap();
        assert_eq!(proposal.project, "LTS");
        assert_eq!(proposal.name, "Runbook: restart API");
        assert_eq!(proposal.content, "Step 1\nStep 2");
        assert_eq!(
            proposal.parent_article.as_deref(),
            Some("0f3f3f3f-0000-0000-0000-000000000001")
        );
        assert_eq!(proposal.access.as_deref(), Some("private"));

        let blank_content = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "   ".into(),
            parent_article: None,
            access: None,
        })
        .unwrap_err();
        assert!(blank_content.to_string().contains("content"));

        let long_content = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "x".repeat(ARTICLE_CONTENT_MAX + 1),
            parent_article: None,
            access: None,
        })
        .unwrap_err();
        assert!(long_content.to_string().contains("at most"));

        let bad_parent = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "Step".into(),
            parent_article: Some("not-a-uuid".into()),
            access: None,
        })
        .unwrap_err();
        assert!(bad_parent.to_string().contains("parent_article"));

        let bad_access = create_article_proposal_from_args(CreateArticleArgs {
            project: "LTS".into(),
            name: "Runbook".into(),
            content: "Step".into(),
            parent_article: None,
            access: Some("secret".into()),
        })
        .unwrap_err();
        assert!(bad_access.to_string().contains("public"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/api-rs`): `cargo test -p ai create_article_normalizes`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `UpdateTrack` (sebelum `mutation_kind_for_tool`):

```rust
pub const CREATE_ARTICLE_NAME: &str = "create_article";
pub const ARTICLE_CONTENT_MAX: usize = 20_000;
pub const ARTICLE_ACCESSES: [&str; 2] = ["public", "private"];

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArticleArgs {
    /// Project identifier or name. Required.
    pub project: String,
    /// Article title (1-255 characters).
    pub name: String,
    /// Plain-text body (1-20000 characters); the UI converts it to rich text.
    pub content: String,
    /// Parent article (page) id, when the user wants a sub-page.
    pub parent_article: Option<String>,
    /// One of: public, private. Defaults to public.
    pub access: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateArticleProposal {
    pub project: String,
    pub name: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_article: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
}

/// Validate raw tool args into a normalized create-article proposal.
pub fn create_article_proposal_from_args(
    args: CreateArticleArgs,
) -> Result<CreateArticleProposal, ToolExecutionError> {
    let project = required_text(&args.project, "project", CONTAINER_PROJECT_MAX)?;
    let name = required_text(&args.name, "name", SERVICE_NAME_MAX)?;
    let content = required_text(&args.content, "content", ARTICLE_CONTENT_MAX)?;
    let parent_article = match args.parent_article.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => {
            let parsed = Uuid::parse_str(raw).map_err(|_| {
                ToolExecutionError::invalid_args("parent_article must be a page uuid")
            })?;
            Some(parsed.to_string())
        }
    };
    let access = optional_enum_arg(args.access.as_deref(), &ARTICLE_ACCESSES, "access")?;
    Ok(CreateArticleProposal {
        project,
        name,
        content,
        parent_article,
        access,
    })
}

pub struct CreateArticle {
    pub trace: ToolTrace,
}

impl Tool for CreateArticle {
    const NAME: &'static str = CREATE_ARTICLE_NAME;
    type Args = CreateArticleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose creating one knowledge base article (page) in a project. Only \
         call this when the user asks to write an article. The project, title, \
         and body text are required; never guess them. Access defaults to \
         public. The user must confirm and may edit the article in the UI \
         before anything is saved. Never claim the article was created until \
         they confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CreateArticleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = create_article_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("CreateArticleProposal serializes"))
    }
}
```

Tambahkan `use uuid::Uuid;` di bagian import `mutations.rs` (atas file, setelah `use serde_json::Value;`):

```rust
use uuid::Uuid;
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai create_article_normalizes`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add create_article proposal tool"
```

---

### Task 2: Tool `update_article`

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn update_article_requires_action_and_a_change() {
        let proposal = update_article_proposal_from_args(UpdateArticleArgs {
            article: " 0f3f3f3f-0000-0000-0000-000000000002 ".into(),
            project: Some(" LTS ".into()),
            action: " APPEND ".into(),
            name: None,
            content: Some(" Step 3 ".into()),
        })
        .unwrap();
        assert_eq!(proposal.article, "0f3f3f3f-0000-0000-0000-000000000002");
        assert_eq!(proposal.project.as_deref(), Some("LTS"));
        assert_eq!(proposal.action, "append");
        assert_eq!(proposal.content.as_deref(), Some("Step 3"));

        let empty = update_article_proposal_from_args(UpdateArticleArgs {
            article: "0f3f3f3f-0000-0000-0000-000000000002".into(),
            project: None,
            action: "replace".into(),
            name: None,
            content: None,
        })
        .unwrap_err();
        assert!(empty.to_string().contains("at least one"));

        let bad_action = update_article_proposal_from_args(UpdateArticleArgs {
            article: "0f3f3f3f-0000-0000-0000-000000000002".into(),
            project: None,
            action: "rewrite".into(),
            name: Some("x".into()),
            content: None,
        })
        .unwrap_err();
        assert!(bad_action.to_string().contains("append"));

        let bad_article = update_article_proposal_from_args(UpdateArticleArgs {
            article: "LTS-7".into(),
            project: None,
            action: "append".into(),
            name: Some("x".into()),
            content: None,
        })
        .unwrap_err();
        assert!(bad_article.to_string().contains("uuid"));

        let blank_name = update_article_proposal_from_args(UpdateArticleArgs {
            article: "0f3f3f3f-0000-0000-0000-000000000002".into(),
            project: None,
            action: "append".into(),
            name: Some("  ".into()),
            content: None,
        })
        .unwrap_err();
        assert!(blank_name.to_string().contains("name"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai update_article_requires`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Tambahkan di `mutations.rs` setelah `CreateArticle`:

```rust
pub const UPDATE_ARTICLE_NAME: &str = "update_article";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArticleArgs {
    /// Article (page) id, a uuid.
    pub article: String,
    /// Project identifier or name; use it when the article id is not unique.
    pub project: Option<String>,
    /// One of: append, replace. Required so a body is never replaced by accident.
    pub action: String,
    /// New title (1-255 characters).
    pub name: Option<String>,
    /// New plain-text body (1-20000 characters).
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateArticleProposal {
    pub article: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Validate raw tool args into a normalized update-article proposal.
pub fn update_article_proposal_from_args(
    args: UpdateArticleArgs,
) -> Result<UpdateArticleProposal, ToolExecutionError> {
    let article = Uuid::parse_str(args.article.trim())
        .map_err(|_| ToolExecutionError::invalid_args("article must be a page uuid"))?
        .to_string();
    let project = optional_container_project(args.project)?;
    let action = enum_arg(&args.action, &["append", "replace"], "action")?;
    let name = match args.name.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "name must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > SERVICE_NAME_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "name must be at most {SERVICE_NAME_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    let content = match args.content.as_deref().map(str::trim) {
        None => None,
        Some("") => {
            return Err(ToolExecutionError::invalid_args(
                "content must not be empty when provided",
            ))
        }
        Some(value) if value.chars().count() > ARTICLE_CONTENT_MAX => {
            return Err(ToolExecutionError::invalid_args(format!(
                "content must be at most {ARTICLE_CONTENT_MAX} characters"
            )))
        }
        Some(value) => Some(value.to_string()),
    };
    if name.is_none() && content.is_none() {
        return Err(ToolExecutionError::invalid_args(
            "at least one of name or content is required",
        ));
    }
    Ok(UpdateArticleProposal {
        article,
        project,
        action,
        name,
        content,
    })
}

pub struct UpdateArticle {
    pub trace: ToolTrace,
}

impl Tool for UpdateArticle {
    const NAME: &'static str = UPDATE_ARTICLE_NAME;
    type Args = UpdateArticleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Propose editing one knowledge base article (page). Only call this when \
         the user asks to change an article's title or body. The article id is \
         required; never guess it. The action is required: append adds the text \
         at the end, replace swaps the whole body. Pass the project when known. \
         The user must confirm and may edit the article in the UI before \
         anything is saved. Never claim the article was updated until they \
         confirm."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<UpdateArticleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let proposal = update_article_proposal_from_args(args)?;
        record(&self.trace, Self::NAME, &proposal);
        Ok(serde_json::to_string(&proposal).expect("UpdateArticleProposal serializes"))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai update_article_requires`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mutations.rs
git commit -m "feat(ai): add update_article proposal tool"
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
    async fn article_tools_expose_metadata() {
        let create = CreateArticle {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(CreateArticle::NAME, "create_article");
        assert!(create.parameters()["properties"]["content"].is_object());
        assert!(!create.description().is_empty());

        let update = UpdateArticle {
            trace: crate::agent::new_trace(),
        };
        assert_eq!(UpdateArticle::NAME, "update_article");
        assert!(update.parameters()["properties"]["action"].is_object());
    }
```

Tambahkan di `mod tests` `agent.rs`:

```rust
    #[test]
    fn pending_actions_covers_article_mutations() {
        let trace = new_trace();
        record(
            &trace,
            crate::tools::CREATE_ARTICLE_NAME,
            &json!({"project": "LTS", "name": "Runbook", "content": "Step 1"}),
        );
        record(
            &trace,
            crate::tools::UPDATE_ARTICLE_NAME,
            &json!({"article": "0f3f3f3f-0000-0000-0000-000000000002", "action": "append"}),
        );
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["kind"], json!("create_article"));
        assert_eq!(actions[1]["kind"], json!("update_article"));
    }
```

Tambahkan di `mod tests` `mutations.rs`:

```rust
    #[test]
    fn mutation_kind_mapping_covers_article_tools() {
        assert_eq!(
            mutation_kind_for_tool(CREATE_ARTICLE_NAME),
            Some("create_article")
        );
        assert_eq!(
            mutation_kind_for_tool(UPDATE_ARTICLE_NAME),
            Some("update_article")
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pending_actions_covers_article`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi**

Di `tools/mod.rs`, perbarui `PROPOSAL_TOOL_NAMES` menjadi 15 entri:

```rust
pub const PROPOSAL_TOOL_NAMES: [&str; 15] = [
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
];
```

Di `workspace_tools`, setelah `.tool(UpdateTrack { ... })` dan sebelum `.tool(CreateSchedule { ... })`:

```rust
        .tool(CreateArticle {
            trace: trace.clone(),
        })
        .tool(UpdateArticle {
            trace: trace.clone(),
        })
```

Di `mutations.rs`, perbarui `mutation_kind_for_tool`:

```rust
        CREATE_TRACK_NAME => Some("create_track"),
        UPDATE_TRACK_NAME => Some("update_track"),
        CREATE_ARTICLE_NAME => Some("create_article"),
        UPDATE_ARTICLE_NAME => Some("update_article"),
        _ => None,
```

Di `agent.rs`, tambahkan setelah paragraf create/update container di `PREAMBLE`:

```text
To write a knowledge base article, call create_article with the project, \
title, and body text. To change an article, call update_article with its id \
and action append or replace; use append unless the user explicitly wants \
the body replaced. Articles are proposals too: the user must confirm them \
in the UI first, so never claim they were created or updated before that \
confirmation.
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/tools/mutations.rs apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): register article proposal tools"
```

---

### Task 4: Allowlist decision + `created_article_id`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_conversations.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `ai_conversations.rs`:

```rust
    #[test]
    fn metadata_patch_accepts_article_decisions() {
        let key = Uuid::new_v4().to_string();
        let page = Uuid::new_v4();
        let clean = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {
                "kind": "create_article",
                "decision": "applied",
                "result": {"created_article_id": page},
            }}),
        )]))
        .expect("valid patch");
        assert_eq!(
            clean["proposal_decisions"][key.as_str()]["result"]["created_article_id"],
            json!(page)
        );

        let missing = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(key.clone()): {"kind": "create_article", "decision": "applied"}}),
        )]))
        .unwrap_err();
        assert!(missing.contains("created_article_id"));

        let update_key = Uuid::new_v4().to_string();
        let clean_update = clean_metadata_patch(&patch_map(vec![(
            "proposal_decisions",
            json!({(update_key.clone()): {"kind": "update_article", "decision": "applied"}}),
        )]))
        .expect("valid update patch");
        assert_eq!(
            clean_update["proposal_decisions"][update_key.as_str()]["decision"],
            json!("applied")
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p api --lib metadata_patch_accepts_article`
Expected: gagal (`kind not allowed: create_article`).

- [ ] **Step 3: Implementasi**

Perbarui `allowed_result_keys`:

```rust
        "add_comment" => Some(&["created_comment_id"]),
        "create_service" => Some(&["created_service_id"]),
        "create_sprint" => Some(&["created_sprint_id"]),
        "create_track" => Some(&["created_track_id"]),
        "create_article" => Some(&["created_article_id"]),
        _ => None,
```

Tambahkan `update_article` ke daftar kind tanpa result:

```rust
        "update_work_item"
        | "update_service"
        | "update_sprint"
        | "update_track"
        | "update_article"
        | "manage_service_links"
        | "manage_sprint_items"
        | "manage_track_items" => Some(&[]),
```

Perbarui `required_result_key`:

```rust
        "create_track" => Some("created_track_id"),
        "create_article" => Some("created_article_id"),
        _ => None,
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p api --lib metadata_patch`
Expected: seluruh test metadata lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_conversations.rs
git commit -m "feat(api): accept article proposal decisions"
```

---

### Task 5: Integration test roundtrip dua kind

**Files:**

- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Tambahkan dua test DB-backed**

Tambahkan di akhir file:

```rust
#[tokio::test]
async fn create_article_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let base_url = tool_roundtrip::tool_roundtrip_url(
        "create_article",
        r#"{"project":"LTS","name":"Runbook: restart API","content":"Step 1\nStep 2","access":"private"}"#,
    )
    .await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "write a runbook article in LTS",
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
    assert_eq!(proposals[0]["kind"], json!("create_article"));
    assert_eq!(proposals[0]["proposal"]["name"], json!("Runbook: restart API"));
    assert_eq!(proposals[0]["proposal"]["access"], json!("private"));
    let key = proposals[0]["key"].as_str().unwrap().to_string();
    let message_id = Uuid::parse_str(body["assistant_message"]["id"].as_str().unwrap()).unwrap();

    let created = Uuid::new_v4();
    let (status, Json(patched)) = api::routes::ai_conversations::patch_message(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), conversation_id, message_id)),
        Json(json!({"metadata": {"proposal_decisions": {
            (key.clone()): {"kind": "create_article", "decision": "applied", "result": {"created_article_id": created}}
        }}})),
    )
    .await
    .expect("patch applied");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        patched["metadata"]["proposal_decisions"][key.as_str()]["result"]["created_article_id"],
        json!(created)
    );

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn update_article_proposal_roundtrip() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    let article = Uuid::new_v4();
    let arguments = format!(
        r#"{{"article":"{article}","action":"append","content":"Step 3"}}"#
    );
    let leaked: &'static str = Box::leak(arguments.into_boxed_str());
    let base_url = tool_roundtrip::tool_roundtrip_url("update_article", leaked).await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "append a step to the runbook",
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
    assert_eq!(proposals[0]["kind"], json!("update_article"));
    assert_eq!(proposals[0]["proposal"]["action"], json!("append"));
    assert_eq!(proposals[0]["proposal"]["article"], json!(article));

    scratch.purge(&pool).await;
}
```

- [ ] **Step 2: Jalankan test**

Run: `cargo test -p api --test ai_agent_test -- --test-threads=1`
Expected: seluruh test lulus (butuh DB dev + Redis).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "test(api): roundtrip article proposal metadata"
```

---

### Task 6: FE `ai-proposals.ts` — 2 kind baru + validator

**Files:**

- Modify: `apps/web/core/lib/ai-proposals.ts`
- Modify: `apps/web/core/lib/ai-proposals.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Perbarui blok import di `ai-proposals.test.ts` dengan menambahkan:

```ts
  validateCreateArticleProposal,
  validateUpdateArticleProposal,
```

Tambahkan di akhir file:

```ts
describe("article proposal validators", () => {
  it("validates create-article proposals", () => {
    expect(
      validateCreateArticleProposal({
        project: "LTS",
        name: "Runbook",
        content: "Step 1",
        access: "private",
      })
    ).toBeNull();
    expect(validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "  " })).toBe(
      "Content is required."
    );
    expect(validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "x".repeat(20001) })).toBe(
      "Content must be at most 20000 characters."
    );
    expect(
      validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "Step", access: "secret" as never })
    ).toBe("Unknown access.");
    expect(
      validateCreateArticleProposal({ project: "LTS", name: "Runbook", content: "Step", parent_article: "nope" })
    ).toBe("Parent article must be a page uuid.");
  });

  it("validates update-article proposals", () => {
    expect(
      validateUpdateArticleProposal({
        article: "0f3f3f3f-0000-0000-0000-000000000002",
        action: "append",
        content: "Step 3",
      })
    ).toBeNull();
    expect(validateUpdateArticleProposal({ article: "0f3f3f3f-0000-0000-0000-000000000002", action: "append" })).toBe(
      "At least one of name or content is required."
    );
    expect(validateUpdateArticleProposal({ article: "nope", action: "append", content: "x" })).toBe(
      "Article must be a page uuid."
    );
    expect(
      validateUpdateArticleProposal({
        article: "0f3f3f3f-0000-0000-0000-000000000002",
        action: "rewrite" as never,
        content: "x",
      })
    ).toBe("Action must be append or replace.");
    expect(
      validateUpdateArticleProposal({
        article: "0f3f3f3f-0000-0000-0000-000000000002",
        action: "append",
        name: "  ",
      })
    ).toBe("Name must not be empty.");
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
] as const;
```

Tambahkan konstanta:

```ts
export const ARTICLE_ACCESS_VALUES = ["public", "private"] as const;
export const ARTICLE_ACTION_VALUES = ["append", "replace"] as const;
```

Tambahkan tipe (setelah `TAiUpdateTrackProposal`):

```ts
export type TAiCreateArticleProposal = {
  project: string;
  name: string;
  content: string;
  parent_article?: string | null;
  access?: string | null;
};

export type TAiUpdateArticleProposal = {
  article: string;
  project?: string | null;
  action: "append" | "replace";
  name?: string | null;
  content?: string | null;
};
```

Perbarui union `TAiProposal`:

```ts
  | { key: string; kind: "create_track"; proposal: TAiCreateTrackProposal }
  | { key: string; kind: "update_track"; proposal: TAiUpdateTrackProposal }
  | { key: string; kind: "create_article"; proposal: TAiCreateArticleProposal }
  | { key: string; kind: "update_article"; proposal: TAiUpdateArticleProposal };
```

Perbarui `TAiProposalDecisionResult`:

```ts
export type TAiProposalDecisionResult = {
  created_comment_id?: string;
  created_service_id?: string;
  created_sprint_id?: string;
  created_track_id?: string;
  created_article_id?: string;
};
```

Perbarui `TAiProposalConfirmPayload` dengan dua arm baru:

```ts
  | {
      kind: "create_article";
      projectId: string;
      data: { name: string; descriptionHtml: string; access: number; parent?: string };
    }
  | {
      kind: "update_article";
      projectId: string;
      pageId: string;
      action: "append" | "replace";
      name?: string;
      descriptionHtml?: string;
    };
```

Tambahkan validator di akhir file:

```ts
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Mirrors `create_article_proposal_from_args` on the backend. */
export const validateCreateArticleProposal = (proposal: Partial<TAiCreateArticleProposal>): string | null => {
  const name = proposal.name?.trim() ?? "";
  if (!name) return "Name is required.";
  if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  const content = proposal.content?.trim() ?? "";
  if (!content) return "Content is required.";
  if (content.length > PROPOSAL_LIMITS.articleContent)
    return `Content must be at most ${PROPOSAL_LIMITS.articleContent} characters.`;
  if (proposal.parent_article && !UUID_RE.test(proposal.parent_article.trim()))
    return "Parent article must be a page uuid.";
  if (proposal.access && !ARTICLE_ACCESS_VALUES.includes(proposal.access as (typeof ARTICLE_ACCESS_VALUES)[number]))
    return "Unknown access.";
  return null;
};

/** Mirrors `update_article_proposal_from_args` on the backend. */
export const validateUpdateArticleProposal = (proposal: Partial<TAiUpdateArticleProposal>): string | null => {
  if (!proposal.article || !UUID_RE.test(proposal.article.trim())) return "Article must be a page uuid.";
  if (!ARTICLE_ACTION_VALUES.includes(proposal.action as (typeof ARTICLE_ACTION_VALUES)[number]))
    return "Action must be append or replace.";
  if (proposal.name != null) {
    const name = proposal.name.trim();
    if (!name) return "Name must not be empty.";
    if (name.length > PROPOSAL_LIMITS.name) return `Name must be at most ${PROPOSAL_LIMITS.name} characters.`;
  }
  if (proposal.content != null) {
    const content = proposal.content.trim();
    if (!content) return "Content must not be empty.";
    if (content.length > PROPOSAL_LIMITS.articleContent)
      return `Content must be at most ${PROPOSAL_LIMITS.articleContent} characters.`;
  }
  if (proposal.name == null && proposal.content == null) return "At least one of name or content is required.";
  return null;
};
```

Perbarui `PROPOSAL_LIMITS`:

```ts
export const PROPOSAL_LIMITS = {
  comment: 5000,
  name: 255,
  description: 5000,
  refs: 10,
  services: 10,
  items: 25,
  url: 2048,
  articleContent: 20000,
} as const;
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/lib/ai-proposals.test.ts`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposals.test.ts
git commit -m "feat(web): add article proposal types and validators"
```

---

### Task 7: Store dispatch artikel

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Modify: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

Di `ai-assistant.store.test.ts`, tambahkan mock `pages` di `makeServices`:

```ts
  pages: {
    create: vi.fn(async (_slug: string, projectId: string) => ({ id: "pg1", project_id: projectId })),
    update: vi.fn(async () => ({ id: "pg1" })),
    fetchById: vi.fn(async () => ({ id: "pg1", description_html: "<p>Step 1</p>" })),
    updateDescription: vi.fn(async () => ({ message: "Updated successfully" })),
    ...overrides.pages,
  },
```

Perbarui `makeStore` agar meneruskan service kesembilan:

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
    services.pages as any
  );
```

Tambahkan di akhir file:

```ts
const createArticleMetadata = {
  proposals: [
    {
      key: "k12",
      kind: "create_article",
      proposal: { project: "LTS", name: "Runbook", content: "Step 1" },
    },
  ],
};

const updateArticleMetadata = {
  proposals: [
    {
      key: "k13",
      kind: "update_article",
      proposal: { article: "pg1", action: "append", content: "Step 3" },
    },
  ],
};

describe("article proposals", () => {
  const openAgent = async (services = makeServices(), metadata: Record<string, unknown>) => {
    services.conversations.listMessages = vi.fn(async () => [mutationMessage(metadata)]);
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    return { store, services };
  };

  it("creates an article and stores the created id", async () => {
    const { store, services } = await openAgent(makeServices(), createArticleMetadata);
    await store.confirmProposal("srv-assistant", "k12", {
      kind: "create_article",
      projectId: "p1",
      data: { name: "Runbook", descriptionHtml: "<p>Step 1</p>", access: 1 },
    });
    expect(services.pages.create).toHaveBeenCalledWith("acme", "p1", {
      name: "Runbook",
      description_html: "<p>Step 1</p>",
      access: 1,
      parent: undefined,
    });
    expect(store.messages[0].proposalDecisions?.k12).toEqual({
      kind: "create_article",
      decision: "applied",
      result: { created_article_id: "pg1" },
    });
  });

  it("appends to an article and rewrites the document payload", async () => {
    const { store, services } = await openAgent(makeServices(), updateArticleMetadata);
    await store.confirmProposal("srv-assistant", "k13", {
      kind: "update_article",
      projectId: "p1",
      pageId: "pg1",
      action: "append",
      descriptionHtml: "<p>Step 3</p>",
    });
    expect(services.pages.fetchById).toHaveBeenCalledWith("acme", "p1", "pg1", false);
    expect(services.pages.updateDescription).toHaveBeenCalledTimes(1);
    const [, , , payload] = services.pages.updateDescription.mock.calls[0];
    expect(typeof payload.description_binary).toBe("string");
    expect(payload.description_binary.length).toBeGreaterThan(0);
    expect(payload.description_html).toContain("Step 1");
    expect(payload.description_html).toContain("Step 3");
    expect(store.messages[0].proposalDecisions?.k13).toEqual({
      kind: "update_article",
      decision: "applied",
    });
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: gagal kompilasi (dispatch kind baru belum ada).

- [ ] **Step 3: Implementasi store**

Tambahkan import (catatan: `TPage` tidak punya field `parent`, sehingga body create memakai tipe perluasan `Partial<TPage> & { parent?: string }`):

```ts
import { convertHTMLDocumentToAllFormats } from "@plane/editor";
import { ProjectPageService } from "@/services/page/project-page.service";
```

dan perluas import tipe existing:

```ts
import type { TIssue, TPage } from "@plane/types";
```

Perbarui pick type dan constructor:

```ts
type TPagesService = Pick<ProjectPageService, "create" | "update" | "fetchById" | "updateDescription">;
```

```ts
    private cyclesService: TCyclesService = new CycleService(),
    private pagesService: TPagesService = new ProjectPageService()
  ) {
```

Di `confirmProposal`, sisipkan arm baru setelah `update_track` dan sebelum `manage_service_links`:

```ts
    } else if (payload.kind === "create_article") {
      const body: Partial<TPage> & { parent?: string } = {
        name: payload.data.name,
        description_html: payload.data.descriptionHtml,
        access: payload.data.access,
        ...(payload.data.parent ? { parent: payload.data.parent } : {}),
      };
      const created = await this.pagesService.create(slug, payload.projectId, body);
      if (!created?.id) throw new Error("Article creation returned no id");
      result = { created_article_id: created.id };
    } else if (payload.kind === "update_article") {
      if (payload.name) {
        await this.pagesService.update(slug, payload.projectId, payload.pageId, { name: payload.name });
      }
      if (payload.descriptionHtml) {
        const page = await this.pagesService.fetchById(slug, payload.projectId, payload.pageId, false);
        const currentHtml = page?.description_html ?? "";
        const mergedHtml = payload.action === "append" ? `${currentHtml}${payload.descriptionHtml}` : payload.descriptionHtml;
        const document = convertHTMLDocumentToAllFormats({ document_html: mergedHtml, variant: "document" });
        await this.pagesService.updateDescription(slug, payload.projectId, payload.pageId, document);
      }
    } else if (payload.kind === "manage_service_links") {
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `pnpm test core/store/ai-assistant.store.test.ts`
Expected: seluruh test store lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): dispatch article proposals"
```

---

### Task 8: `ArticleProposalCard` + dispatcher

**Files:**

- Create: `apps/web/core/components/ai/assistant-sidebar/proposals/article-proposal-card.tsx`
- Modify: `apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx`

- [ ] **Step 1: Buat `article-proposal-card.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { Button } from "@plane/propel/button";
import { EPageAccess } from "@plane/types";
// hooks
import { useProject } from "@/hooks/store/use-project";
// lib
import { matchProject, textToDescriptionHtml } from "@/lib/ai-work-items";
import {
  ARTICLE_ACCESS_VALUES,
  validateCreateArticleProposal,
  validateUpdateArticleProposal,
  type TAiCreateArticleProposal,
  type TAiProposalConfirmPayload,
  type TAiProposalDecision,
  type TAiUpdateArticleProposal,
} from "@/lib/ai-proposals";

type ArticleProposalEntry =
  | { kind: "create_article"; proposal: TAiCreateArticleProposal }
  | { kind: "update_article"; proposal: TAiUpdateArticleProposal };

type Props = ArticleProposalEntry & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

export const ArticleProposalCard = (props: Props) => {
  if (props.kind === "create_article") return <CreateArticleCard {...props} />;
  return <UpdateArticleCard {...props} />;
};

type CreateProps = Extract<ArticleProposalEntry, { kind: "create_article" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const CreateArticleCard = observer(function CreateArticleCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: CreateProps) {
  const { workspaceProjectIds, getProjectById } = useProject();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [name, setName] = useState(proposal.name);
  const [content, setContent] = useState(proposal.content);
  const [access, setAccess] = useState<string>(proposal.access ?? "public");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));
  const matchedProject = matchProject(projects, proposal.project);
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? null;

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article created.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article creation cancelled.
      </p>
    );
  }

  const draft = { project: proposal.project, name, content, parent_article: proposal.parent_article, access };
  const validationError = validateCreateArticleProposal(draft);
  const preview = textToDescriptionHtml(content);

  const confirm = async () => {
    if (!effectiveProjectId) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "create_article",
        projectId: effectiveProjectId,
        data: {
          name: name.trim(),
          descriptionHtml: preview ?? "",
          access: access === "private" ? EPageAccess.PRIVATE : EPageAccess.PUBLIC,
          ...(proposal.parent_article ? { parent: proposal.parent_article } : {}),
        },
      });
    } catch {
      setError("Could not create the article. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div role="group" aria-label="Article proposal" className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3">
      <p className="text-12 font-medium text-primary">Create article</p>
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
      <label className="mt-2 block text-12 text-secondary">
        Title
        <input
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <label className="mt-2 block text-12 text-secondary">
        Body
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={5}
          value={content}
          onChange={(event) => setContent(event.target.value)}
        />
      </label>
      <label className="mt-2 block text-12 text-secondary">
        Access
        <select
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          value={access}
          onChange={(event) => setAccess(event.target.value)}
        >
          {ARTICLE_ACCESS_VALUES.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </label>
      {proposal.parent_article && (
        <p className="mt-1 text-12 text-tertiary">Sub-page of article {proposal.parent_article}</p>
      )}
      {preview && (
        <div className="mt-2 rounded border border-subtle bg-layer-2 p-2">
          <p className="text-11 text-tertiary">Preview</p>
          <div className="ai-prose mt-1 text-12 text-primary" dangerouslySetInnerHTML={{ __html: preview }} />
        </div>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!effectiveProjectId || Boolean(validationError)}
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

type UpdateProps = Extract<ArticleProposalEntry, { kind: "update_article" }> & {
  decision?: TAiProposalDecision;
  onConfirm: (payload: TAiProposalConfirmPayload) => Promise<void>;
  onCancel: () => void;
};

const UpdateArticleCard = observer(function UpdateArticleCard({
  proposal,
  decision,
  onConfirm,
  onCancel,
}: UpdateProps) {
  const { workspaceProjectIds, getProjectById } = useProject();
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [action, setAction] = useState<"append" | "replace">(proposal.action);
  const [name, setName] = useState(proposal.name ?? "");
  const [content, setContent] = useState(proposal.content ?? "");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projects = (workspaceProjectIds ?? [])
    .map((id) => getProjectById(id))
    .filter((project): project is NonNullable<typeof project> => Boolean(project))
    .map((project) => ({ id: project.id, identifier: project.identifier, name: project.name }));
  const matchedProject = proposal.project ? matchProject(projects, proposal.project) : undefined;
  const effectiveProjectId = selectedProjectId ?? matchedProject?.id ?? null;

  if (decision?.decision === "applied") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article updated.
      </p>
    );
  }
  if (decision?.decision === "cancelled") {
    return (
      <p role="status" className="mt-2 text-12 text-tertiary">
        Article update cancelled.
      </p>
    );
  }

  const draft = {
    article: proposal.article,
    action,
    ...(proposal.name != null ? { name } : {}),
    ...(proposal.content != null ? { content } : {}),
  };
  const validationError = validateUpdateArticleProposal(draft);
  const preview = proposal.content != null ? textToDescriptionHtml(content) : null;

  const confirm = async () => {
    if (!effectiveProjectId) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm({
        kind: "update_article",
        projectId: effectiveProjectId,
        pageId: proposal.article,
        action,
        ...(proposal.name != null ? { name: name.trim() } : {}),
        ...(proposal.content != null ? { descriptionHtml: preview ?? "" } : {}),
      });
    } catch {
      setError("Could not update the article. Please retry.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="group"
      aria-label="Article update proposal"
      className="mt-2 rounded-lg border border-subtle bg-layer-1 p-3"
    >
      <p className="text-12 font-medium text-primary">Update article</p>
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
      {proposal.content != null && (
        <label className="mt-2 block text-12 text-secondary">
          Action
          <select
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={action}
            onChange={(event) => setAction(event.target.value === "replace" ? "replace" : "append")}
          >
            <option value="append">Append at the end</option>
            <option value="replace">Replace the whole body</option>
          </select>
        </label>
      )}
      {proposal.name != null && (
        <label className="mt-2 block text-12 text-secondary">
          Title
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
      )}
      {proposal.content != null && (
        <label className="mt-2 block text-12 text-secondary">
          Body
          <textarea
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            rows={5}
            value={content}
            onChange={(event) => setContent(event.target.value)}
          />
        </label>
      )}
      {preview && (
        <div className="mt-2 rounded border border-subtle bg-layer-2 p-2">
          <p className="text-11 text-tertiary">{action === "append" ? "Will be appended" : "Will replace the body"}</p>
          <div className="ai-prose mt-1 text-12 text-primary" dangerouslySetInnerHTML={{ __html: preview }} />
        </div>
      )}
      {validationError && <p className="mt-1 text-12 text-danger-primary">{validationError}</p>}
      {error && <p className="mt-1 text-12 text-danger-primary">{error}</p>}
      <div className="mt-2 flex gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={submitting}
          disabled={!effectiveProjectId || Boolean(validationError)}
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
import { ArticleProposalCard } from "./article-proposal-card";
```

Tambahkan cabang sebelum `return <ProposalFormCard kind="update_track" ...>`:

```tsx
if (entry.kind === "create_article")
  return (
    <ArticleProposalCard
      kind="create_article"
      proposal={entry.proposal}
      decision={decision}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
if (entry.kind === "update_article")
  return (
    <ArticleProposalCard
      kind="update_article"
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
git add apps/web/core/components/ai/assistant-sidebar/proposals/article-proposal-card.tsx apps/web/core/components/ai/assistant-sidebar/proposals/proposal-card.tsx
git commit -m "feat(web): render article proposal cards"
```

---

### Task 9: Verifikasi akhir Fase 3B

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
git commit -m "chore: format fase 3b"
```

- [ ] **Step 5: Smoke manual (opsional, butuh stack hidup)**

Ikuti AGENTS.md: rebuild `pnpm --filter=web build`, restart `plane-web-prod.service`, lalu di UI Agent mode:

1. "tulis artikel Runbook restart API di LTS" → kartu create article dengan preview; Confirm → halaman muncul di Pages dan editor memuat konten.
2. "tambahkan langkah 3 ke artikel itu" → kartu update dengan mode append; Confirm → konten bertambah.
3. Scheduled run tetap tidak bisa memutasi (proposal tool tidak ada di `SPEC_TOOLS`).

---

## Self-Review Checklist (sudah dijalankan)

- **Spec coverage:** §7.1 dua tool artikel → Task 1–2; §7.3 kartu preview bespoke → Task 8; §4.4 kind mapping + result id → Task 3–4; §8 testing → Task 1–9.
- **Deviasi terdokumentasi:** content teks polos (konversi FE), `project?` opsional di `update_article`, access public/private → 0/1, create tanpa binary (live server mengonversi), update menulis binary+html+json.
- **Placeholder scan:** tidak ada TODO/TBD; semua langkah berisi kode lengkap.
- **Type consistency:** `CreateArticleArgs`/`TAiCreateArticleProposal`, `UpdateArticleArgs`/`TAiUpdateArticleProposal`, `PROPOSAL_TOOL_NAMES` 15 entri, `allowed_result_keys`/`required_result_key` cocok dengan `TAiProposalDecisionResult.created_article_id`, payload arm `create_article`/`update_article` dipakai kartu dan store, dispatcher 13 cabang dengan `key` di `ProposalCard`.
