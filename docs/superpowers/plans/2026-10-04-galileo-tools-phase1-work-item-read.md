# Galileo Tools Fase 1A — Work Item Read Kit — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah tiga read tool Galileo — `get_work_item`, `list_work_item_comments`, `list_work_item_relations` — plus resolver project/work item ber-permission, di atas refactor struktur `tools/` per domain dan plumbing `user_id`.

**Architecture:** Pisahkan `crates/ai/src/tools.rs` menjadi direktori `tools/` (`mod.rs` helper bersama + registry, `projects.rs`, `work_items.rs`, `proposals.rs`). Tambah `user_id` ke registry tool dan teruskan dari handler API (`auth.0`) dan worker (`ai_schedules.created_by_id`). Resolver project/work item di `tools/mod.rs` menegakkan akses `project_members` aktif; tiga tool baru memakainya lalu menjalankan query SQL static yang workspace-scoped, dengan output JSON ringkas + truncation.

**Tech Stack:** Rust (axum, sqlx 0.7 runtime query, Rig 0.42 tool server), serde/schemars, tokio; test `cargo test` dari `apps/api-rs`.

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4, §5.1 Batch 1.1 — work items + resolvers).

**Batas plan ini (sengaja):** Sisa Batch 1.1 — perluasan filter `search_work_items`/`count_work_items`, `list_members`/`list_states`/`list_labels`/`list_work_item_types`, sinkronisasi `SPEC_TOOLS`/`AI_SCHEDULE_TOOLS`, dan update `PREAMBLE` — ditulis di plan 1B. Semua perintah Rust dijalankan dari `apps/api-rs`.

---

## File Structure

| File                                        | Tanggung jawab                                                                                                                                                                                                                                                                              |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/ai/src/tools/mod.rs`                | Konstanta & helper bersama (`optional_text`, `state_group_arg`, `priority_arg`, `clamp_limit`, `db_error`, `schema_of`), resolver (`parse_work_item_ref`, `pick_project`, `resolve_project`, `resolve_work_item`), registry `workspace_tools`/`read_tools`, seluruh test lama yang dipindah |
| `crates/ai/src/tools/projects.rs`           | `ListProjects` + `PROJECTS_SQL` + `projects_json`                                                                                                                                                                                                                                           |
| `crates/ai/src/tools/work_items.rs`         | `CountWorkItems`, `SearchWorkItems`, dan tiga tool baru (`GetWorkItem`, `ListWorkItemComments`, `ListWorkItemRelations`) + SQL/JSON helper-nya                                                                                                                                              |
| `crates/ai/src/tools/proposals.rs`          | `CreateSchedule` + `CreateWorkItem` (proposal, tidak menyentuh DB)                                                                                                                                                                                                                          |
| `crates/api/src/routes/ai_agent/mod.rs`     | Teruskan `auth.0` ke `workspace_tools`                                                                                                                                                                                                                                                      |
| `crates/worker/src/handlers/ai_schedule.rs` | Baca `created_by_id` jadwal, teruskan ke `read_tools`                                                                                                                                                                                                                                       |
| `crates/ai/src/agent.rs`                    | Tidak berubah di plan ini (hanya menerima re-export nama proposal)                                                                                                                                                                                                                          |

---

### Task 1: Pecah `tools.rs` menjadi modul per domain

**Files:**

- Move: `crates/ai/src/tools.rs` → `crates/ai/src/tools/mod.rs`
- Create: `crates/ai/src/tools/projects.rs`
- Create: `crates/ai/src/tools/work_items.rs`
- Create: `crates/ai/src/tools/proposals.rs`

- [ ] **Step 1: Pastikan baseline hijau**

Run: `cargo test -p ai`
Expected: seluruh test lulus (baseline sebelum refactor).

- [ ] **Step 2: Pindahkan file dan buat direktori**

```bash
mkdir -p crates/ai/src/tools
git mv crates/ai/src/tools.rs crates/ai/src/tools/mod.rs
```

- [ ] **Step 3: Isi `tools/mod.rs` dengan header, module decl, dan helper bersama**

Ganti isi `tools/mod.rs` menjadi (semua item helper di bawah dipindah **apa adanya** dari file lama; jangan ubah isinya):

```rust
//! Shared tool infrastructure: constants, validation helpers, project/work
//! item resolvers, and the tool registry. Domain tools live in sibling
//! modules and are re-exported here so `crate::tools::<Item>` keeps working.

pub mod projects;
pub mod proposals;
pub mod work_items;

pub use projects::*;
pub use proposals::*;
pub use work_items::*;

use rig::tool::ToolExecutionError;
use schemars::JsonSchema;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent::ToolTrace;

pub const STATE_GROUPS: [&str; 5] = ["backlog", "unstarted", "started", "completed", "cancelled"];
pub const PRIORITIES: [&str; 5] = ["urgent", "high", "medium", "low", "none"];
pub const DEFAULT_LIMIT: i64 = 10;
pub const MAX_LIMIT: i64 = 25;

pub fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn state_group_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|s| s.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if STATE_GROUPS.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "state_group must be one of: {}",
            STATE_GROUPS.join(", ")
        )))
    }
}

pub fn priority_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|s| s.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if PRIORITIES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "priority must be one of: {}",
            PRIORITIES.join(", ")
        )))
    }
}

pub fn clamp_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub fn db_error(error: sqlx::Error) -> ToolExecutionError {
    tracing::warn!(error = %error, "ai-agent: tool query failed");
    ToolExecutionError::from_error(error)
}

pub fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| json!({"type": "object", "properties": {}}))
}
```

- [ ] **Step 4: Isi `tools/projects.rs` dengan tool project yang ada**

Pindah apa adanya dari file lama: `PROJECTS_SQL` (baris 19-21), `projects_json` (96-102), `ListProjectsArgs` (139-140), `ListProjects` (178-212). Header file:

```rust
//! `list_projects` read tool.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{db_error, schema_of};
use crate::agent::{record, ToolTrace};
```

Tidak ada test di file ini; seluruh test lama tetap di `mod.rs`.

- [ ] **Step 5: Isi `tools/work_items.rs` untuk tool lama**

Pindah apa adanya: `COUNT_SQL` (23-33), `SEARCH_SQL` (35-50), `count_json` (104-121), `search_json` (123-137), `CountWorkItemsArgs` (142-152), `SearchWorkItemsArgs` (154-166), `CountWorkItems` (214-261), `SearchWorkItems` (263-306). Header file:

```rust
//! Work item read tools: count, search, detail, comments, relations.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{clamp_limit, db_error, optional_text, priority_arg, schema_of, state_group_arg};
use crate::agent::{record, ToolTrace};
```

- [ ] **Step 6: Isi `tools/proposals.rs` untuk tool proposal**

Pindah apa adanya: `CREATE_SCHEDULE_NAME` (308), `CreateScheduleArgs` (310-332), `recipe_from_args` (335-349), `CreateSchedule` (351-378), `CREATE_WORK_ITEM_NAME` (380), konstanta `WORK_ITEM_*` (382-387), `CreateWorkItemArgs` (389-409), `WorkItemProposal` (411-423), `bounded_ref_list` (425-457), `parse_iso_date` (459-462), `work_item_proposal_from_args` (466-534), `CreateWorkItem` (536-572). Header file:

```rust
//! Proposal tools: `create_schedule` and `create_work_item`. They never touch
//! the database; the UI renders a confirmation card from the recorded trace.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{optional_text, priority_arg, schema_of};
use crate::agent::{record, ToolTrace};
use crate::schedule::ScheduleRecipe;
```

- [ ] **Step 7: Kembalikan registry + tests ke `tools/mod.rs`**

Salin `workspace_tools` (574-605) dan `read_tools` (607-639) apa adanya, lalu tambahkan `#[cfg(test)] mod tests { ... }` berisi seluruh test lama (641-1140) apa adanya. Karena `mod.rs` melakukan glob re-export submodule, `use super::*;` di dalam `mod tests` memberi akses ke `ListProjects`, `CountWorkItems`, `CreateScheduleArgs`, dst. tanpa perubahan test. Tambahkan tepat di atas `mod tests`:

```rust
#[cfg(test)]
pub(crate) fn lazy_pool() -> PgPool {
    sqlx::PgPool::connect_lazy("postgres://user:pass@127.0.0.1:1/plane").expect("lazy pool")
}
```

dan hapus definisi `fn lazy_pool()` yang lama di dalam `mod tests` (biar hanya ada satu).

- [ ] **Step 8: Verifikasi refactor**

Run: `cargo test -p ai`
Expected: seluruh test lulus (jumlah test sama seperti baseline).

Run: `cargo build -p api -p worker`
Expected: kompilasi sukses; `crate::tools::{CREATE_SCHEDULE_NAME, CREATE_WORK_ITEM_NAME}`, `tools::workspace_tools`, dan `tools::read_tools` masih resolve via re-export.

Run: `rg -n "pub struct ListProjects" crates/ai/src/tools/`
Expected: satu hasil di `crates/ai/src/tools/projects.rs`.

- [ ] **Step 9: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools
git commit -m "refactor(ai): split agent tools into domain modules"
```

---

### Task 2: Plumbing `user_id` ke registry tool

**Files:**

- Modify: `crates/ai/src/tools/mod.rs` (signature `workspace_tools`/`read_tools` + call site test)
- Modify: `crates/api/src/routes/ai_agent/mod.rs:191`
- Modify: `crates/worker/src/handlers/ai_schedule.rs:176-202`

- [ ] **Step 1: Ubah signature registry di `tools/mod.rs`**

```rust
pub fn workspace_tools(
    pool: PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    trace: ToolTrace,
) -> rig::tool::server::ToolServerHandle {
    // user_id belum dibaca tool mana pun di Task 2; tool baru memakainya mulai Task 4.
    let _ = user_id;
    rig::tool::server::ToolServer::new()
        .tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(SearchWorkItems {
            pool,
            workspace_id,
            trace: trace.clone(),
        })
        .tool(CreateSchedule {
            trace: trace.clone(),
        })
        .tool(CreateWorkItem {
            trace: trace.clone(),
        })
        .run()
}

pub fn read_tools(
    pool: PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    trace: ToolTrace,
    allowed: &[&str],
) -> rig::tool::server::ToolServerHandle {
    let _ = user_id;
    let mut server = rig::tool::server::ToolServer::new();
    if allowed.contains(&ListProjects::NAME) {
        server = server.tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&CountWorkItems::NAME) {
        server = server.tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&SearchWorkItems::NAME) {
        server = server.tool(SearchWorkItems {
            pool,
            workspace_id,
            trace: trace.clone(),
        });
    }
    server.run()
}
```

- [ ] **Step 2: Perbarui test call site di `tools/mod.rs`**

```rust
#[tokio::test]
async fn workspace_tools_builds_a_server_handle() {
    let _handle = workspace_tools(lazy_pool(), Uuid::nil(), Uuid::nil(), crate::agent::new_trace());
}
```

```rust
#[tokio::test]
async fn read_tools_accepts_a_subset() {
    let _handle = read_tools(
        lazy_pool(),
        Uuid::nil(),
        Uuid::nil(),
        crate::agent::new_trace(),
        &["list_projects"],
    );
}
```

dan call site terakhir di `workspace_tools_builds_a_server_handle_with_create_work_item`:

```rust
let _handle = workspace_tools(lazy_pool(), Uuid::nil(), Uuid::nil(), crate::agent::new_trace());
```

- [ ] **Step 3: Teruskan `auth.0` dari handler API**

Di `crates/api/src/routes/ai_agent/mod.rs` ganti baris 191 menjadi:

```rust
let tool_server = tools::workspace_tools(st.pool.clone(), workspace_id, auth.0, trace.clone());
```

- [ ] **Step 4: Baca `created_by_id` di worker**

Di `crates/worker/src/handlers/ai_schedule.rs` ganti blok baris 176-181:

```rust
    let spec_value: Option<Value> =
        sqlx::query_scalar("SELECT spec FROM ai_schedules WHERE id = $1")
            .bind(run.schedule_id)
            .fetch_optional(pool)
            .await?
            .flatten();
```

menjadi:

```rust
    let (spec_value, created_by_id): (Option<Value>, Uuid) =
        sqlx::query_as("SELECT spec, created_by_id FROM ai_schedules WHERE id = $1")
            .bind(run.schedule_id)
            .fetch_one(pool)
            .await?;
```

lalu ganti baris 201-202:

```rust
    let handle =
        ai::tools::read_tools(pool.clone(), run.workspace_id, trace.clone(), &allowed_refs);
```

menjadi:

```rust
    let handle = ai::tools::read_tools(
        pool.clone(),
        run.workspace_id,
        created_by_id,
        trace.clone(),
        &allowed_refs,
    );
```

- [ ] **Step 5: Verifikasi**

Run: `cargo test -p ai`
Expected: lulus.

Run: `cargo build -p api -p worker`
Expected: sukses tanpa warning unused variable.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/worker/src/handlers/ai_schedule.rs
git commit -m "refactor(ai): pass user_id through the tool registry"
```

---

### Task 3: Resolver project & work item ber-permission

**Files:**

- Modify: `crates/ai/src/tools/mod.rs`
- Test: `crates/ai/src/tools/mod.rs` (`mod tests`)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` pada `tools/mod.rs`:

```rust
#[test]
fn parse_work_item_ref_rules() {
    assert_eq!(
        parse_work_item_ref("LTS-42").unwrap(),
        WorkItemRef {
            project: "LTS".to_string(),
            sequence_id: 42
        }
    );
    assert_eq!(parse_work_item_ref(" lts-7 ").unwrap().project, "lts");
    for bad in ["LTS", "LTS-0", "LTS--1", "LTS-abc", "-5"] {
        assert!(parse_work_item_ref(bad).is_err(), "{bad} should be rejected");
    }
}

#[test]
fn pick_project_prefers_identifier_then_exact_name_then_unique_substring() {
    let rows = vec![
        ProjectRow { id: Uuid::from_u128(1), identifier: "LTS".into(), name: "Logistics".into() },
        ProjectRow { id: Uuid::from_u128(2), identifier: "OPS".into(), name: "Operations".into() },
    ];
    assert_eq!(pick_project(&rows, "lts").unwrap().id, Uuid::from_u128(1));
    assert_eq!(pick_project(&rows, "logistics").unwrap().id, Uuid::from_u128(1));
    assert_eq!(pick_project(&rows, "oper").unwrap().id, Uuid::from_u128(2));
    assert!(pick_project(&rows, "o").is_err());
    assert!(pick_project(&[], "lts").is_err());
}

#[test]
fn resolver_sql_is_workspace_scoped_and_requires_membership() {
    for sql in [RESOLVE_PROJECTS_SQL, RESOLVE_WORK_ITEM_SQL] {
        assert!(sql.contains("workspace_id = $1"));
        assert!(sql.contains("project_members"));
        assert!(sql.contains("is_active = true"));
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai parse_work_item_ref_rules`
Expected: gagal kompilasi — `parse_work_item_ref`, `WorkItemRef`, `ProjectRow`, `pick_project`, `RESOLVE_PROJECTS_SQL` belum ada.

- [ ] **Step 3: Implementasi resolver di `tools/mod.rs`**

Tambahkan (di luar `mod tests`):

```rust
/// Project reference parsed from a work item identifier like "LTS-42".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItemRef {
    pub project: String,
    pub sequence_id: i64,
}

pub fn parse_work_item_ref(reference: &str) -> Result<WorkItemRef, ToolExecutionError> {
    let trimmed = reference.trim();
    let Some((project, sequence)) = trimmed.rsplit_once('-') else {
        return Err(ToolExecutionError::invalid_args(
            "work_item must look like PROJ-123",
        ));
    };
    let project = project.trim();
    if project.is_empty() || !project.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ToolExecutionError::invalid_args(
            "work_item must look like PROJ-123",
        ));
    }
    let sequence_id: i64 = sequence
        .trim()
        .parse()
        .map_err(|_| ToolExecutionError::invalid_args("work_item must look like PROJ-123"))?;
    if sequence_id <= 0 {
        return Err(ToolExecutionError::invalid_args(
            "work_item must look like PROJ-123",
        ));
    }
    Ok(WorkItemRef {
        project: project.to_string(),
        sequence_id,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: Uuid,
    pub identifier: String,
    pub name: String,
}

pub const RESOLVE_PROJECTS_SQL: &str = "SELECT p.id, p.identifier, p.name FROM projects p \
     WHERE p.workspace_id = $1 AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.member_id = $2 \
       AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(p.identifier) = lower($3) OR lower(p.name) = lower($3) \
       OR p.name ILIKE '%' || $3 || '%') \
     ORDER BY p.name LIMIT 10";

/// Pick one project: identifier exact, then name exact, then a unique name
/// substring. Anything else is a model-visible error; never guess.
pub fn pick_project(
    rows: &[ProjectRow],
    reference: &str,
) -> Result<ProjectRow, ToolExecutionError> {
    let needle = reference.trim().to_ascii_lowercase();
    if rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "project '{reference}' was not found or is not accessible"
        )));
    }
    if let Some(exact) = rows
        .iter()
        .find(|row| row.identifier.to_ascii_lowercase() == needle)
    {
        return Ok(exact.clone());
    }
    if let Some(exact) = rows
        .iter()
        .find(|row| row.name.to_ascii_lowercase() == needle)
    {
        return Ok(exact.clone());
    }
    if rows.len() == 1 {
        return Ok(rows[0].clone());
    }
    let identifiers = rows
        .iter()
        .map(|row| row.identifier.clone())
        .collect::<Vec<_>>()
        .join(", ");
    Err(ToolExecutionError::invalid_args(format!(
        "project '{reference}' is ambiguous; use one identifier: {identifiers}"
    )))
}

pub async fn resolve_project(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
) -> Result<ProjectRow, ToolExecutionError> {
    let rows: Vec<ProjectRow> = sqlx::query_as(RESOLVE_PROJECTS_SQL)
        .bind(workspace_id)
        .bind(user_id)
        .bind(reference.trim())
        .fetch_all(pool)
        .await
        .map_err(db_error)?;
    pick_project(&rows, reference)
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ResolvedWorkItem {
    pub id: Uuid,
    pub project_id: Uuid,
    pub identifier: String,
}

pub const RESOLVE_WORK_ITEM_SQL: &str = "SELECT i.id, i.project_id, \
     p.identifier || '-' || i.sequence_id AS identifier \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 AND p.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND p.identifier ILIKE $2 AND i.sequence_id = $3 \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id AND pm.member_id = $4 \
       AND pm.is_active = true AND pm.deleted_at IS NULL) \
     LIMIT 1";

pub async fn resolve_work_item(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
) -> Result<ResolvedWorkItem, ToolExecutionError> {
    let parsed = parse_work_item_ref(reference)?;
    let row: Option<ResolvedWorkItem> = sqlx::query_as(RESOLVE_WORK_ITEM_SQL)
        .bind(workspace_id)
        .bind(&parsed.project)
        .bind(parsed.sequence_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?;
    row.ok_or_else(|| {
        ToolExecutionError::invalid_args(format!(
            "work item '{reference}' was not found or is not accessible"
        ))
    })
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: lulus, termasuk tiga test baru.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add project and work item resolvers with membership guard"
```

---

### Task 4: Tool `get_work_item`

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`
- Test: `crates/ai/src/tools/work_items.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan `#[cfg(test)] mod tests` di akhir `tools/work_items.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{lazy_pool, ResolvedWorkItem};
    use rig::tool::ToolContext;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_row() -> WorkItemDetailRow {
        WorkItemDetailRow {
            project: "LTS".to_string(),
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
            description: "x".repeat(4100),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "urgent".to_string(),
            work_item_type: "Incident".to_string(),
            start_date: Some("2026-10-01".to_string()),
            target_date: None,
            completed_at: None,
            created_at: "2026-09-30 08:00:00+00".to_string(),
            updated_at: "2026-10-02 09:00:00+00".to_string(),
            sub_issues: 2,
            comment_count: 4,
            relation_count: 1,
        }
    }

    #[test]
    fn truncate_chars_respects_char_boundaries() {
        assert_eq!(truncate_chars("short", 10), "short");
        assert_eq!(truncate_chars("abcdef", 5), "ab...");
        assert_eq!(truncate_chars("ééééé", 4), "é...");
    }

    #[test]
    fn detail_json_is_compact_and_truncated() {
        let comments = vec![CommentView {
            author: "Budi".to_string(),
            at: "2026-10-01 10:00:00+00".to_string(),
            text: "looking into it".to_string(),
        }];
        let out = work_item_detail_json(
            &sample_row(),
            &["Budi".to_string()],
            &["maintenance".to_string()],
            Some("Sprint 3"),
            &["Track A".to_string()],
            &["API".to_string()],
            &comments,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["identifier"], json!("LTS-42"));
        assert_eq!(parsed["state"], json!("In Progress"));
        assert_eq!(parsed["type"], json!("Incident"));
        assert_eq!(parsed["assignees"][0], json!("Budi"));
        assert_eq!(parsed["sprint"], json!("Sprint 3"));
        assert_eq!(parsed["recent_comments"][0]["author"], json!("Budi"));
        let description = parsed["description"].as_str().unwrap();
        assert_eq!(description.chars().count(), DESCRIPTION_MAX);
        assert!(description.ends_with("..."));
    }

    #[tokio::test]
    async fn get_work_item_rejects_a_malformed_ref_before_querying() {
        let trace = crate::agent::new_trace();
        let tool = GetWorkItem {
            pool: lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: trace.clone(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                GetWorkItemArgs {
                    work_item: "nope".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("PROJ-123"));
        assert_eq!(trace.lock().unwrap().len(), 1, "read tools trace before validation");
    }

    #[test]
    fn resolved_work_item_struct_shape() {
        let item = ResolvedWorkItem {
            id: Uuid::nil(),
            project_id: Uuid::nil(),
            identifier: "LTS-1".to_string(),
        };
        assert_eq!(item.identifier, "LTS-1");
    }
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai truncate_chars_respects_char_boundaries`
Expected: gagal kompilasi — `WorkItemDetailRow`, `truncate_chars`, `CommentView`, `work_item_detail_json`, `GetWorkItem` belum ada.

- [ ] **Step 3: Implementasi di `tools/work_items.rs`**

Ubah baris `use super::{...}` di header file menjadi (menambah `resolve_work_item`):

```rust
use super::{
    clamp_limit, db_error, optional_text, priority_arg, resolve_work_item, schema_of,
    state_group_arg,
};
```

Tambahkan setelah tool lama:

```rust
pub const DESCRIPTION_MAX: usize = 4000;
pub const RECENT_COMMENTS_LIMIT: i64 = 5;

/// Char-bounded truncation with an ASCII ellipsis so tool output stays small.
pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WorkItemDetailRow {
    pub project: String,
    pub identifier: String,
    pub name: String,
    pub description: String,
    pub state: String,
    pub state_group: String,
    pub priority: String,
    pub work_item_type: String,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub sub_issues: i64,
    pub comment_count: i64,
    pub relation_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct CommentView {
    pub author: String,
    pub at: String,
    pub text: String,
}

pub const WORK_ITEM_DETAIL_SQL: &str = "SELECT \
     p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, \
     i.name, \
     COALESCE(i.description_stripped, '') AS description, \
     COALESCE(s.name, '') AS state, \
     COALESCE(s.\"group\", '') AS state_group, \
     i.priority, \
     COALESCE(t.name, '') AS work_item_type, \
     i.start_date::text AS start_date, \
     i.target_date::text AS target_date, \
     i.completed_at::text AS completed_at, \
     i.created_at::text AS created_at, \
     i.updated_at::text AS updated_at, \
     (SELECT count(*) FROM issues c WHERE c.parent_id = i.id AND c.deleted_at IS NULL)::int8 \
       AS sub_issues, \
     (SELECT count(*) FROM issue_comments cm WHERE cm.issue_id = i.id AND cm.deleted_at IS NULL)::int8 \
       AS comment_count, \
     (SELECT count(*) FROM issue_relations r \
       WHERE (r.issue_id = i.id OR r.related_issue_id = i.id) AND r.deleted_at IS NULL)::int8 \
       AS relation_count \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     LEFT JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     LEFT JOIN issue_types t ON t.id = i.type_id AND t.deleted_at IS NULL \
     WHERE i.id = $1 AND i.workspace_id = $2 AND i.deleted_at IS NULL";

pub const WORK_ITEM_ASSIGNEES_SQL: &str =
    "SELECT COALESCE(u.display_name, u.email, 'unknown') FROM issue_assignees ia \
     JOIN users u ON u.id = ia.assignee_id \
     WHERE ia.issue_id = $1 AND ia.deleted_at IS NULL ORDER BY 1";

pub const WORK_ITEM_LABELS_SQL: &str =
    "SELECT l.name FROM issue_labels il \
     JOIN labels l ON l.id = il.label_id AND l.deleted_at IS NULL \
     WHERE il.issue_id = $1 AND il.deleted_at IS NULL ORDER BY l.name";

pub const WORK_ITEM_SPRINT_SQL: &str =
    "SELECT c.name FROM cycle_issues ci \
     JOIN cycles c ON c.id = ci.cycle_id AND c.deleted_at IS NULL \
     WHERE ci.issue_id = $1 AND ci.deleted_at IS NULL \
     ORDER BY ci.created_at DESC, ci.id DESC LIMIT 1";

pub const WORK_ITEM_TRACKS_SQL: &str =
    "SELECT m.name FROM module_issues mi \
     JOIN modules m ON m.id = mi.module_id AND m.deleted_at IS NULL \
     WHERE mi.issue_id = $1 AND mi.deleted_at IS NULL ORDER BY m.name";

pub const WORK_ITEM_SERVICES_SQL: &str =
    "SELECT sv.name FROM service_issues si \
     JOIN services sv ON sv.id = si.service_id AND sv.deleted_at IS NULL \
     WHERE si.issue_id = $1 AND si.deleted_at IS NULL ORDER BY sv.name";

pub const WORK_ITEM_RECENT_COMMENTS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS author, \
     cm.created_at::text AS at, \
     left(cm.comment_stripped, 500) AS text \
     FROM issue_comments cm \
     LEFT JOIN users u ON u.id = COALESCE(cm.actor_id, cm.created_by_id) \
     WHERE cm.issue_id = $1 AND cm.deleted_at IS NULL \
     ORDER BY cm.created_at DESC, cm.id DESC LIMIT $2";

#[allow(clippy::too_many_arguments)]
pub fn work_item_detail_json(
    row: &WorkItemDetailRow,
    assignees: &[String],
    labels: &[String],
    sprint: Option<&str>,
    tracks: &[String],
    services: &[String],
    comments: &[CommentView],
) -> String {
    json!({
        "project": row.project,
        "identifier": row.identifier,
        "name": row.name,
        "description": truncate_chars(&row.description, DESCRIPTION_MAX),
        "state": row.state,
        "state_group": row.state_group,
        "priority": row.priority,
        "type": row.work_item_type,
        "assignees": assignees,
        "labels": labels,
        "sprint": sprint,
        "tracks": tracks,
        "services": services,
        "start_date": row.start_date,
        "target_date": row.target_date,
        "completed_at": row.completed_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "sub_issues": row.sub_issues,
        "comment_count": row.comment_count,
        "relation_count": row.relation_count,
        "recent_comments": comments,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetWorkItemArgs {
    /// Work item identifier, e.g. "LTS-42".
    pub work_item: String,
}

pub struct GetWorkItem {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetWorkItem {
    const NAME: &'static str = "get_work_item";
    type Args = GetWorkItemArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one work item by identifier (e.g. LTS-42): name, description \
         (truncated), state and state group, priority, type, assignees, labels, \
         sprint, tracks, services, dates, counts, and the 5 most recent comments. \
         Use this before answering questions about a specific work item."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetWorkItemArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let resolved =
            resolve_work_item(&self.pool, self.workspace_id, self.user_id, &args.work_item).await?;
        let row: WorkItemDetailRow = sqlx::query_as(WORK_ITEM_DETAIL_SQL)
            .bind(resolved.id)
            .bind(self.workspace_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        let assignees: Vec<String> = sqlx::query_scalar(WORK_ITEM_ASSIGNEES_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let labels: Vec<String> = sqlx::query_scalar(WORK_ITEM_LABELS_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let sprint: Option<String> = sqlx::query_scalar(WORK_ITEM_SPRINT_SQL)
            .bind(resolved.id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let tracks: Vec<String> = sqlx::query_scalar(WORK_ITEM_TRACKS_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let services: Vec<String> = sqlx::query_scalar(WORK_ITEM_SERVICES_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let comments: Vec<CommentView> = sqlx::query_as(WORK_ITEM_RECENT_COMMENTS_SQL)
            .bind(resolved.id)
            .bind(RECENT_COMMENTS_LIMIT)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(work_item_detail_json(
            &row,
            &assignees,
            &labels,
            sprint.as_deref(),
            &tracks,
            &services,
            &comments,
        ))
    }
}
```

Catatan: `CommentView` dipindah ke sini (bukan di test) supaya bisa dipakai handler; `use rig::tool::...` sudah ada di header file dari Task 1 Step 5.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai detail_json_is_compact_and_truncated`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs
git commit -m "feat(ai): add get_work_item read tool"
```

---

### Task 5: Tool `list_work_item_comments`

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`
- Test: `crates/ai/src/tools/work_items.rs` (`mod tests`)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/work_items.rs`:

```rust
#[test]
fn comments_json_shape() {
    let rows = vec![
        CommentView {
            author: "Budi".to_string(),
            at: "2026-10-01 10:00:00+00".to_string(),
            text: "first".to_string(),
        },
        CommentView {
            author: "Sari".to_string(),
            at: "2026-10-02 10:00:00+00".to_string(),
            text: "second".to_string(),
        },
    ];
    let parsed: serde_json::Value =
        serde_json::from_str(&comments_json("LTS-42", &rows)).unwrap();
    assert_eq!(parsed["work_item"], json!("LTS-42"));
    assert_eq!(parsed["returned"], json!(2));
    assert_eq!(parsed["comments"][0]["author"], json!("Budi"));
    assert_eq!(parsed["comments"][1]["text"], json!("second"));
}

#[tokio::test]
async fn list_comments_rejects_a_malformed_ref_before_querying() {
    let tool = ListWorkItemComments {
        pool: lazy_pool(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: crate::agent::new_trace(),
    };
    let error = tool
        .call(
            &mut ToolContext::new(),
            ListWorkItemCommentsArgs {
                work_item: "no-dash".to_string(),
                limit: Some(5),
            },
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("PROJ-123"));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai comments_json_shape`
Expected: gagal kompilasi — `comments_json`, `ListWorkItemComments` belum ada.

- [ ] **Step 3: Implementasi di `tools/work_items.rs`**

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListWorkItemCommentsArgs {
    /// Work item identifier, e.g. "LTS-42".
    pub work_item: String,
    /// Maximum comments to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub const WORK_ITEM_COMMENTS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS author, \
     cm.created_at::text AS at, \
     left(cm.comment_stripped, 500) AS text \
     FROM issue_comments cm \
     LEFT JOIN users u ON u.id = COALESCE(cm.actor_id, cm.created_by_id) \
     WHERE cm.issue_id = $1 AND cm.deleted_at IS NULL \
     ORDER BY cm.created_at DESC, cm.id DESC LIMIT $2";

pub fn comments_json(work_item: &str, rows: &[CommentView]) -> String {
    json!({
        "work_item": work_item,
        "returned": rows.len(),
        "comments": rows,
    })
    .to_string()
}

pub struct ListWorkItemComments {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListWorkItemComments {
    const NAME: &'static str = "list_work_item_comments";
    type Args = ListWorkItemCommentsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List the most recent comments on one work item (newest first), each \
         with author, timestamp, and text truncated to 500 characters."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListWorkItemCommentsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let resolved =
            resolve_work_item(&self.pool, self.workspace_id, self.user_id, &args.work_item).await?;
        let limit = clamp_limit(args.limit);
        let rows: Vec<CommentView> = sqlx::query_as(WORK_ITEM_COMMENTS_SQL)
            .bind(resolved.id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(comments_json(&resolved.identifier, &rows))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai comments_json_shape`
Expected: lulus bersama test `get_work_item` yang sudah ada.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs
git commit -m "feat(ai): add list_work_item_comments read tool"
```

---

### Task 6: Tool `list_work_item_relations`

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`
- Test: `crates/ai/src/tools/work_items.rs` (`mod tests`)

- [ ] **Step 1: Tulis test yang gagal**

```rust
#[test]
fn relations_json_shape() {
    let rows = vec![RelationRow {
        relation_type: "blocked_by".to_string(),
        identifier: "OPS-3".to_string(),
        name: "Fix network".to_string(),
    }];
    let parsed: serde_json::Value =
        serde_json::from_str(&relations_json("LTS-42", &rows)).unwrap();
    assert_eq!(parsed["work_item"], json!("LTS-42"));
    assert_eq!(parsed["relations"][0]["type"], json!("blocked_by"));
    assert_eq!(parsed["relations"][0]["identifier"], json!("OPS-3"));
}

#[tokio::test]
async fn list_relations_rejects_a_malformed_ref_before_querying() {
    let tool = ListWorkItemRelations {
        pool: lazy_pool(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: crate::agent::new_trace(),
    };
    let error = tool
        .call(
            &mut ToolContext::new(),
            ListWorkItemRelationsArgs {
                work_item: "nope".to_string(),
            },
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("PROJ-123"));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai relations_json_shape`
Expected: gagal kompilasi — `relations_json`, `RelationRow`, `ListWorkItemRelations` belum ada.

- [ ] **Step 3: Implementasi di `tools/work_items.rs`**

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListWorkItemRelationsArgs {
    /// Work item identifier, e.g. "LTS-42".
    pub work_item: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RelationRow {
    pub relation_type: String,
    pub identifier: String,
    pub name: String,
}

pub const WORK_ITEM_RELATIONS_SQL: &str = "SELECT \
     r.relation_type AS relation_type, \
     p.identifier || '-' || other.sequence_id AS identifier, \
     other.name AS name \
     FROM issue_relations r \
     JOIN issues other ON other.id = \
       CASE WHEN r.issue_id = $1 THEN r.related_issue_id ELSE r.issue_id END \
       AND other.deleted_at IS NULL \
     JOIN projects p ON p.id = other.project_id AND p.deleted_at IS NULL \
     WHERE (r.issue_id = $1 OR r.related_issue_id = $1) AND r.deleted_at IS NULL \
     ORDER BY r.relation_type, identifier";

pub fn relations_json(work_item: &str, rows: &[RelationRow]) -> String {
    json!({
        "work_item": work_item,
        "returned": rows.len(),
        "relations": rows,
    })
    .to_string()
}

pub struct ListWorkItemRelations {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListWorkItemRelations {
    const NAME: &'static str = "list_work_item_relations";
    type Args = ListWorkItemRelationsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List the relations of one work item (e.g. blocked_by, blocking, \
         relates_to, duplicate) with the other item's identifier and name."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListWorkItemRelationsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let resolved =
            resolve_work_item(&self.pool, self.workspace_id, self.user_id, &args.work_item).await?;
        let rows: Vec<RelationRow> = sqlx::query_as(WORK_ITEM_RELATIONS_SQL)
            .bind(resolved.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(relations_json(&resolved.identifier, &rows))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs
git commit -m "feat(ai): add list_work_item_relations read tool"
```

---

### Task 7: Registrasi tool baru + verifikasi akhir

**Files:**

- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Daftarkan ketiga tool di kedua registry**

Ganti seluruh isi `workspace_tools` (hapus `let _ = user_id;`) menjadi:

```rust
pub fn workspace_tools(
    pool: PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    trace: ToolTrace,
) -> rig::tool::server::ToolServerHandle {
    rig::tool::server::ToolServer::new()
        .tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(SearchWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(GetWorkItem {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListWorkItemComments {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListWorkItemRelations {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(CreateSchedule {
            trace: trace.clone(),
        })
        .tool(CreateWorkItem {
            trace: trace.clone(),
        })
        .run()
}
```

Ganti seluruh isi `read_tools` (hapus `let _ = user_id;`) menjadi:

```rust
pub fn read_tools(
    pool: PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    trace: ToolTrace,
    allowed: &[&str],
) -> rig::tool::server::ToolServerHandle {
    let mut server = rig::tool::server::ToolServer::new();
    if allowed.contains(&ListProjects::NAME) {
        server = server.tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&CountWorkItems::NAME) {
        server = server.tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&SearchWorkItems::NAME) {
        server = server.tool(SearchWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&GetWorkItem::NAME) {
        server = server.tool(GetWorkItem {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListWorkItemComments::NAME) {
        server = server.tool(ListWorkItemComments {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListWorkItemRelations::NAME) {
        server = server.tool(ListWorkItemRelations {
            pool,
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    server.run()
}
```

- [ ] **Step 2: Tambah test metadata tool baru**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
#[tokio::test]
async fn new_work_item_tools_expose_metadata() {
    let pool = lazy_pool();
    let trace = crate::agent::new_trace();

    let get = GetWorkItem {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(GetWorkItem::NAME, "get_work_item");
    assert_eq!(get.parameters()["type"], json!("object"));
    assert!(get.parameters()["properties"]["work_item"].is_object());
    assert!(!get.description().is_empty());

    let comments = ListWorkItemComments {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListWorkItemComments::NAME, "list_work_item_comments");
    assert!(comments.parameters()["properties"]["limit"].is_object());

    let relations = ListWorkItemRelations {
        pool,
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace,
    };
    assert_eq!(ListWorkItemRelations::NAME, "list_work_item_relations");
    assert!(!relations.description().is_empty());
}
```

- [ ] **Step 3: Verifikasi seluruh crate**

Run: `cargo test -p ai`
Expected: seluruh test lulus (test lama + test baru Task 3-7).

Run: `cargo build -p api -p worker`
Expected: sukses tanpa warning.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): register work item read tools in both tool servers"
```

---

### Task 8: Smoke manual (opsional, butuh stack dev)

**Files:** tidak ada perubahan kode.

- [ ] **Step 1: Rebuild + restart backend** (dari root repo; sesuai `AGENTS.md`)

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Tunggu (link Rust bisa 10+ menit tanpa output), lalu:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
systemctl --user restart plane-live.service && curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
```

Expected: `200` dua-duanya.

- [ ] **Step 2: Smoke lewat chat Galileo (mode Agent)**

Prompt yang diuji:

1. `detail <identifier>` — jawaban memuat state, priority, assignee, sprint/track/service (sesuai data), dan komentar terbaru.
2. `komentar terakhir di <identifier>` — daftar komentar, terbaru dulu.
3. `apa yang memblokir <identifier>` — relasi `blocked_by`/`blocking` benar.
4. `<identifier>` project yang bukan member — memastikan jawaban "not found or is not accessible", bukan data bocor.

Expected: tool terpanggil, jawaban sesuai data; tidak ada klaim mengarang saat kosong.

---

## Catatan untuk Plan 1B (sisa Batch 1.1)

- Perluas filter `search_work_items`/`count_work_items`: `service`, `type`, `assignee` (nama/email/`me`), `sprint`, `track`, `label` + `user_id` yang sudah tersedia dari plan ini.
- Tool `list_members`, `list_states`, `list_labels`, `list_work_item_types` (file baru `tools/lookups.rs`).
- Sinkronkan `SPEC_TOOLS` (`crates/ai/src/schedule.rs`) + `AI_SCHEDULE_TOOLS` (`apps/web/core/lib/ai-schedule.ts`) + test worker `allowed_tools`.
- Update `PREAMBLE` (`crates/ai/src/agent.rs`).
