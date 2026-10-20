# Galileo Tools Fase 1D — Sprints + Tracks + Knowledge Base Read — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambah delapan read tool Batch 1.3 — `list_sprints`, `get_sprint`, `list_sprint_work_items`, `list_tracks`, `get_track`, `list_track_work_items`, `search_articles`, `get_article` — dengan feature gate `cycle_view`, `module_view`, dan `page_view`.

**Architecture:** Tool sprints di `tools/sprints.rs` (status sprint dihitung dari tanggal, progress dihitung live dari `cycle_issues`), tracks di `tools/tracks.rs` (status enum tersimpan, member/link/links), KB di `tools/kb.rs` (halaman project-scoped lewat `project_pages`, akses privat dihormati). Bentuk ringkas item memakai `WorkItemBrief`/`GroupCountRow`/`SearchRow` bersama; semua memakai resolver + `ensure_feature` dari plan sebelumnya. Scheduled run eligible (read-only).

**Tech Stack:** Rust (axum, sqlx, Rig 0.42), serde/schemars; perintah Rust dari `apps/api-rs`.

**Prasyarat:** Plan 1A, 1B, 1C sudah dieksekusi.

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4.3, §4.5, §4.6, §5.3).

---

## File Structure

| File                                    | Tanggung jawab                                                                  |
| --------------------------------------- | ------------------------------------------------------------------------------- |
| `crates/ai/src/tools/work_items.rs`     | Shared `WorkItemBrief`, `GroupCountRow`, `assignee_arg`                         |
| `crates/ai/src/tools/sprints.rs` (baru) | `list_sprints`, `get_sprint`, `list_sprint_work_items`, `pick_sprint`, resolver |
| `crates/ai/src/tools/tracks.rs` (baru)  | `list_tracks`, `get_track`, `list_track_work_items`, `pick_track`, resolver     |
| `crates/ai/src/tools/kb.rs` (baru)      | `search_articles`, `get_article`                                                |
| `crates/ai/src/tools/mod.rs`            | Module decl + re-export + registrasi                                            |

---

### Task 1: Shared helper + `list_sprints`

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`
- Create: `crates/ai/src/tools/sprints.rs`
- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/work_items.rs`:

```rust
#[test]
fn assignee_arg_splits_me_from_names() {
    assert_eq!(assignee_arg(None), (None, false));
    assert_eq!(assignee_arg(Some("  ")), (None, false));
    assert_eq!(assignee_arg(Some(" ME ")), (None, true));
    assert_eq!(
        assignee_arg(Some(" Budi ")),
        (Some("Budi".to_string()), false)
    );
}
```

Buat `tools/sprints.rs` dengan header + tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sprint_status_allowlist() {
        assert_eq!(sprint_status_arg(None).unwrap(), None);
        assert_eq!(
            sprint_status_arg(Some("Current")).unwrap(),
            Some("current".to_string())
        );
        assert!(sprint_status_arg(Some("someday")).is_err());
    }

    #[test]
    fn sprints_json_shape() {
        let rows = vec![SprintRow {
            id: Uuid::nil(),
            name: "Sprint 3".to_string(),
            project: "LTS".to_string(),
            start_date: Some("2026-10-01 00:00:00+00".to_string()),
            end_date: Some("2026-10-14 00:00:00+00".to_string()),
            status: "current".to_string(),
            owner: "Budi".to_string(),
            total: 12,
            completed: 5,
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&sprints_json(&rows)).unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["name"], json!("Sprint 3"));
        assert_eq!(parsed["items"][0]["status"], json!("current"));
        assert_eq!(parsed["items"][0]["completed"], json!(5));
    }

    #[test]
    fn sprints_sql_is_scoped_guarded_and_gated() {
        assert!(SPRINTS_SQL.contains("workspace_id = $1"));
        assert!(SPRINTS_SQL.contains("project_members"));
        assert!(SPRINTS_SQL.contains("p.cycle_view = true"));
        assert!(SPRINTS_SQL.contains("c.deleted_at IS NULL"));
    }
}
```

Header `tools/sprints.rs`:

```rust
//! Sprint (cycle) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature,
};
use crate::agent::{record, ToolTrace};
```

Tambahkan di `tools/mod.rs`: `pub mod sprints;` dan `pub use sprints::*;`.

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai sprint_status_allowlist`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi helper bersama di `tools/work_items.rs`**

```rust
/// Split an assignee filter into `(name_or_email, is_me)`.
pub fn assignee_arg(value: Option<&str>) -> (Option<String>, bool) {
    match optional_text(value) {
        Some(value) if value.eq_ignore_ascii_case("me") => (None, true),
        other => (other, false),
    }
}

/// Compact work item row shared by container-scoped list tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct WorkItemBrief {
    pub identifier: String,
    pub name: String,
    pub state: String,
    pub state_group: String,
    pub priority: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct GroupCountRow {
    pub state_group: String,
    pub count: i64,
}
```

- [ ] **Step 4: Implementasi `tools/sprints.rs`**

```rust
pub const SPRINT_STATUSES: [&str; 4] = ["current", "upcoming", "completed", "draft"];

pub fn sprint_status_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if SPRINT_STATUSES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "status must be one of: {}",
            SPRINT_STATUSES.join(", ")
        )))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SprintRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub owner: String,
    pub total: i64,
    pub completed: i64,
}

pub const SPRINTS_SQL: &str = "SELECT * FROM (SELECT c.id, c.name, \
     p.identifier AS project, c.start_date::text AS start_date, \
     c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, \
     COALESCE((SELECT count(*) FROM cycle_issues ci \
       JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
       WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM cycle_issues ci \
       JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND c.archived_at IS NULL \
     AND p.cycle_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($3::uuid IS NULL OR p.id = $3) \
     AND ($4::text IS NULL OR c.name ILIKE '%' || $4 || '%')) t \
     WHERE ($5::text IS NULL OR t.status = $5) \
     ORDER BY t.start_date DESC NULLS LAST, t.name LIMIT $6";

pub fn sprints_json(rows: &[SprintRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "project": row.project,
                "start_date": row.start_date,
                "end_date": row.end_date,
                "status": row.status,
                "owner": row.owner,
                "total": row.total,
                "completed": row.completed,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListSprintsArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: current, upcoming, completed, draft.
    pub status: Option<String>,
    /// Case-insensitive substring to match against sprint names.
    pub query: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListSprints {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListSprints {
    const NAME: &'static str = "list_sprints";
    type Args = ListSprintsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List sprints (cycles) the caller can access, newest first, with start \
         and end dates, computed status (current, upcoming, completed, draft), \
         owner, and progress (total and completed work items). Only projects \
         with the sprints feature enabled are included."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListSprintsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Cycles).await?;
        }
        let status = sprint_status_arg(args.status.as_deref())?;
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<SprintRow> = sqlx::query_as(SPRINTS_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(project.as_ref().map(|row| row.id))
            .bind(&query)
            .bind(&status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(sprints_json(&rows))
    }
}
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai sprint`
Expected: 3 test sprints + `assignee_arg_splits_me_from_names` lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs apps/api-rs/crates/ai/src/tools/sprints.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add list_sprints read tool and shared brief helpers"
```

---

### Task 2: Tool `get_sprint`

**Files:**

- Modify: `crates/ai/src/tools/sprints.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/sprints.rs`:

```rust
    fn sprint(name: &str, project: &str) -> SprintRow {
        SprintRow {
            id: Uuid::new_v4(),
            name: name.to_string(),
            project: project.to_string(),
            start_date: None,
            end_date: None,
            status: "draft".to_string(),
            owner: String::new(),
            total: 0,
            completed: 0,
        }
    }

    #[test]
    fn pick_sprint_prefers_exact_then_unique_substring() {
        let rows = vec![sprint("Sprint 3", "LTS"), sprint("Sprint 30", "LTS")];
        assert_eq!(pick_sprint(rows.clone(), "sprint 3").unwrap().name, "Sprint 3");
        assert!(pick_sprint(rows, "sprint").is_err());
        assert!(pick_sprint(vec![], "sprint 3").is_err());
    }

    #[test]
    fn sprint_detail_json_shape() {
        let row = sprint("Sprint 3", "LTS");
        let counts = vec![crate::tools::work_items::GroupCountRow {
            state_group: "started".to_string(),
            count: 4,
        }];
        let items = vec![crate::tools::work_items::WorkItemBrief {
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "urgent".to_string(),
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&sprint_detail_json(&row, &counts, &items)).unwrap();
        assert_eq!(parsed["name"], json!("Sprint 3"));
        assert_eq!(parsed["state_counts"][0]["count"], json!(4));
        assert_eq!(parsed["open_work_items"][0]["identifier"], json!("LTS-42"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai pick_sprint`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/sprints.rs`**

Tambahkan `ProjectRow` ke baris `use super::{...}` di header file (`resolve_sprint` memakainya), lalu tambahkan:

```rust
pub const SPRINT_BY_ID_SQL: &str = "SELECT c.id, c.name, p.identifier AS project, \
     c.start_date::text AS start_date, c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, \
     COALESCE((SELECT count(*) FROM cycle_issues ci \
       JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
       WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM cycle_issues ci \
       JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE ci.cycle_id = c.id AND ci.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.id = $1 AND c.workspace_id = $2 AND c.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

pub const SPRINTS_BY_NAME_PROJECT_SQL: &str = "SELECT c.id, c.name, \
     p.identifier AS project, c.start_date::text AS start_date, \
     c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, 0::int8 AS total, \
     0::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.project_id = $1 AND c.workspace_id = $2 AND c.deleted_at IS NULL \
     AND c.archived_at IS NULL \
     AND (lower(c.name) = lower($3) OR c.name ILIKE '%' || $3 || '%') \
     ORDER BY c.start_date DESC NULLS LAST, c.name LIMIT 10";

pub const SPRINTS_BY_NAME_SQL: &str = "SELECT c.id, c.name, p.identifier AS project, \
     c.start_date::text AS start_date, c.end_date::text AS end_date, \
     CASE WHEN c.start_date IS NULL OR c.end_date IS NULL THEN 'draft' \
          WHEN now() < c.start_date THEN 'upcoming' \
          WHEN now() > c.end_date THEN 'completed' ELSE 'current' END AS status, \
     COALESCE(u.display_name, u.email, '') AS owner, 0::int8 AS total, \
     0::int8 AS completed \
     FROM cycles c \
     JOIN projects p ON p.id = c.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = c.owned_by_id \
     WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND c.archived_at IS NULL \
     AND p.cycle_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(c.name) = lower($3) OR c.name ILIKE '%' || $3 || '%') \
     ORDER BY c.start_date DESC NULLS LAST, c.name LIMIT 10";

pub fn pick_sprint(rows: Vec<SprintRow>, reference: &str) -> Result<SprintRow, ToolExecutionError> {
    let needle = reference.trim();
    if rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "sprint '{needle}' was not found or is not accessible"
        )));
    }
    let exact: Vec<&SprintRow> = rows
        .iter()
        .filter(|row| row.name.eq_ignore_ascii_case(needle))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }
    if exact.len() > 1 || rows.len() > 1 {
        let names = rows
            .iter()
            .map(|row| format!("{}/{}", row.project, row.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolExecutionError::invalid_args(format!(
            "sprint '{needle}' is ambiguous; use one of: {names}"
        )));
    }
    Ok(rows.into_iter().next().expect("one row"))
}

pub async fn resolve_sprint(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
    project: Option<&ProjectRow>,
) -> Result<SprintRow, ToolExecutionError> {
    if let Ok(id) = Uuid::parse_str(reference.trim()) {
        let row: Option<SprintRow> = sqlx::query_as(SPRINT_BY_ID_SQL)
            .bind(id)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
        return row.ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "sprint '{reference}' was not found or is not accessible"
            ))
        });
    }
    let rows: Vec<SprintRow> = match project {
        Some(project) => sqlx::query_as(SPRINTS_BY_NAME_PROJECT_SQL)
            .bind(project.id)
            .bind(workspace_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
        None => sqlx::query_as(SPRINTS_BY_NAME_SQL)
            .bind(workspace_id)
            .bind(user_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
    };
    pick_sprint(rows, reference)
}

pub const SPRINT_STATE_COUNTS_SQL: &str = "SELECT s.\"group\" AS state_group, \
     count(*)::int8 AS count \
     FROM cycle_issues ci \
     JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
     GROUP BY s.\"group\" ORDER BY s.\"group\"";

pub const SPRINT_OPEN_ITEMS_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, COALESCE(s.\"group\", '') AS state_group, i.priority \
     FROM cycle_issues ci \
     JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
     AND s.\"group\" NOT IN ('completed', 'cancelled') \
     ORDER BY i.updated_at DESC LIMIT 10";

pub fn sprint_detail_json(
    row: &SprintRow,
    counts: &[crate::tools::work_items::GroupCountRow],
    open_items: &[crate::tools::work_items::WorkItemBrief],
) -> String {
    json!({
        "name": row.name,
        "project": row.project,
        "status": row.status,
        "start_date": row.start_date,
        "end_date": row.end_date,
        "owner": row.owner,
        "total": row.total,
        "completed": row.completed,
        "state_counts": counts,
        "open_work_items": open_items,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSprintArgs {
    /// Sprint name, or a sprint id when known.
    pub sprint: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
}

pub struct GetSprint {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetSprint {
    const NAME: &'static str = "get_sprint";
    type Args = GetSprintArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one sprint by name or id: dates, computed status, owner, progress \
         (total and completed), the count of work items per state group \
         (computed live), and the 10 most recently updated unfinished items."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetSprintArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                ensure_feature(&self.pool, project.id, ProjectFeature::Cycles).await?;
                Some(project)
            }
            None => None,
        };
        let row = resolve_sprint(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.sprint,
            project.as_ref(),
        )
        .await?;
        let counts: Vec<crate::tools::work_items::GroupCountRow> =
            sqlx::query_as(SPRINT_STATE_COUNTS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        let open_items: Vec<crate::tools::work_items::WorkItemBrief> =
            sqlx::query_as(SPRINT_OPEN_ITEMS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        Ok(sprint_detail_json(&row, &counts, &open_items))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai pick_sprint`
Expected: lulus bersama `sprint_detail_json_shape`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/sprints.rs
git commit -m "feat(ai): add get_sprint read tool"
```

---

### Task 3: Tool `list_sprint_work_items`

**Files:**

- Modify: `crates/ai/src/tools/sprints.rs`

- [ ] **Step 1: Tulis test yang gagal**

```rust
    #[tokio::test]
    async fn list_sprint_items_rejects_a_bad_state_group_before_querying() {
        let tool = ListSprintWorkItems {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                ListSprintWorkItemsArgs {
                    sprint: Uuid::new_v4().to_string(),
                    project: None,
                    state_group: Some("nope".to_string()),
                    assignee: None,
                    limit: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("state_group"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai list_sprint_items_rejects`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/sprints.rs`**

Gunakan `SearchRow` + `search_json` dari `work_items` (bentuknya sama) dan `assignee_arg`:

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListSprintWorkItemsArgs {
    /// Sprint name, or a sprint id when known.
    pub sprint: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// Assignee display name, email, or "me" for the signed-in user.
    pub assignee: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub const SPRINT_ITEMS_SQL: &str = "SELECT p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, i.priority, \
     COALESCE((SELECT it.name FROM issue_types it \
       WHERE it.id = i.type_id AND it.deleted_at IS NULL), '') AS work_item_type, \
     COALESCE((SELECT string_agg(COALESCE(u.display_name, u.email, 'unknown'), ', ' \
         ORDER BY COALESCE(u.display_name, u.email, 'unknown')) \
       FROM issue_assignees ia JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '') AS assignees \
     FROM cycle_issues ci \
     JOIN issues i ON i.id = ci.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE ci.cycle_id = $1 AND ci.deleted_at IS NULL \
     AND ($2::text IS NULL OR s.\"group\" = $2) \
     AND ($3::text IS NULL OR EXISTS (SELECT 1 FROM issue_assignees ia \
       JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND (lower(COALESCE(u.display_name, '')) = lower($3) \
         OR lower(COALESCE(u.email, '')) = lower($3)))) \
     AND ($4::bool = false OR EXISTS (SELECT 1 FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL AND ia.assignee_id = $5)) \
     ORDER BY i.updated_at DESC LIMIT $6";

pub struct ListSprintWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListSprintWorkItems {
    const NAME: &'static str = "list_sprint_work_items";
    type Args = ListSprintWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List work items in one sprint, optionally filtered by state group and \
         assignee (a name, email, or \"me\"). Returns project, identifier, name, \
         state, priority, type, and assignees."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListSprintWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let state_group = super::state_group_arg(args.state_group.as_deref())?;
        let (assignee, assignee_me) = super::assignee_arg(args.assignee.as_deref());
        let limit = clamp_limit(args.limit);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Cycles).await?;
        }
        let sprint = resolve_sprint(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.sprint,
            project.as_ref(),
        )
        .await?;
        let rows: Vec<crate::tools::work_items::SearchRow> =
            sqlx::query_as(SPRINT_ITEMS_SQL)
                .bind(sprint.id)
                .bind(&state_group)
                .bind(&assignee)
                .bind(assignee_me)
                .bind(self.user_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        Ok(crate::tools::work_items::search_json(&rows))
    }
}
```

Catatan: `assignee_arg` diakses lewat `super::assignee_arg` karena dire-export dari `work_items::*`; bila resolver Rust mengeluh ambigu, gunakan `crate::tools::work_items::assignee_arg`.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai list_sprint_items_rejects`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/sprints.rs
git commit -m "feat(ai): add list_sprint_work_items read tool"
```

---

### Task 4: Tools `list_tracks` + `get_track`

**Files:**

- Create: `crates/ai/src/tools/tracks.rs`
- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Tulis test yang gagal**

Buat `tools/tracks.rs` dengan header + tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn track(name: &str, project: &str) -> TrackRow {
        TrackRow {
            id: Uuid::new_v4(),
            name: name.to_string(),
            project: project.to_string(),
            status: "planned".to_string(),
            lead: String::new(),
            members: 0,
            total: 0,
            completed: 0,
            start_date: None,
            target_date: None,
        }
    }

    #[test]
    fn track_status_allowlist() {
        assert_eq!(track_status_arg(None).unwrap(), None);
        assert_eq!(
            track_status_arg(Some("In-Progress")).unwrap(),
            Some("in-progress".to_string())
        );
        assert!(track_status_arg(Some("someday")).is_err());
    }

    #[test]
    fn pick_track_prefers_exact_then_unique_substring() {
        let rows = vec![track("Payments", "LTS"), track("Payments v2", "LTS")];
        assert_eq!(pick_track(rows.clone(), "payments").unwrap().name, "Payments");
        assert!(pick_track(rows, "pay").is_err());
        assert!(pick_track(vec![], "payments").is_err());
    }

    #[test]
    fn tracks_json_shape() {
        let parsed: serde_json::Value = serde_json::from_str(&tracks_json(&[track(
            "Payments",
            "LTS",
        )]))
        .unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["status"], json!("planned"));
    }

    #[test]
    fn track_detail_json_shape() {
        let row = track("Payments", "LTS");
        let links = vec![TrackLinkRow {
            title: Some("Runbook".to_string()),
            url: "https://example.com".to_string(),
        }];
        let counts = vec![crate::tools::work_items::GroupCountRow {
            state_group: "started".to_string(),
            count: 2,
        }];
        let items = vec![crate::tools::work_items::WorkItemBrief {
            identifier: "LTS-1".to_string(),
            name: "Fix".to_string(),
            state: "In Progress".to_string(),
            state_group: "started".to_string(),
            priority: "none".to_string(),
        }];
        let parsed: serde_json::Value = serde_json::from_str(&track_detail_json(
            &row,
            &["Budi".to_string()],
            &links,
            &counts,
            &items,
        ))
        .unwrap();
        assert_eq!(parsed["members"][0], json!("Budi"));
        assert_eq!(parsed["links"][0]["title"], json!("Runbook"));
        assert_eq!(parsed["open_work_items"][0]["identifier"], json!("LTS-1"));
    }

    #[test]
    fn tracks_sql_is_scoped_guarded_and_gated() {
        assert!(TRACKS_SQL.contains("workspace_id = $1"));
        assert!(TRACKS_SQL.contains("project_members"));
        assert!(TRACKS_SQL.contains("p.module_view = true"));
    }
}
```

Header:

```rust
//! Track (module) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature, ProjectRow,
};
use crate::agent::{record, ToolTrace};
```

Tambahkan di `tools/mod.rs`: `pub mod tracks;` dan `pub use tracks::*;`.

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai track_status_allowlist`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/tracks.rs`**

```rust
pub const TRACK_STATUSES: [&str; 6] = [
    "backlog",
    "planned",
    "in-progress",
    "paused",
    "completed",
    "cancelled",
];

pub fn track_status_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|v| v.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if TRACK_STATUSES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "status must be one of: {}",
            TRACK_STATUSES.join(", ")
        )))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct TrackRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub status: String,
    pub lead: String,
    pub members: i64,
    pub total: i64,
    pub completed: i64,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
}

pub const TRACKS_SQL: &str = "SELECT m.id, m.name, p.identifier AS project, m.status, \
     COALESCE(u.display_name, u.email, '') AS lead, \
     COALESCE((SELECT count(*) FROM module_members mm \
       WHERE mm.module_id = m.id AND mm.deleted_at IS NULL), 0)::int8 AS members, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.workspace_id = $1 AND m.deleted_at IS NULL AND m.archived_at IS NULL \
     AND p.module_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($3::uuid IS NULL OR p.id = $3) \
     AND ($4::text IS NULL OR m.name ILIKE '%' || $4 || '%') \
     AND ($5::text IS NULL OR m.status = $5) \
     ORDER BY m.created_at DESC LIMIT $6";

pub fn tracks_json(rows: &[TrackRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "project": row.project,
                "status": row.status,
                "lead": row.lead,
                "members": row.members,
                "total": row.total,
                "completed": row.completed,
                "start_date": row.start_date,
                "target_date": row.target_date,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

pub const TRACK_BY_ID_SQL: &str = "SELECT m.id, m.name, p.identifier AS project, m.status, \
     COALESCE(u.display_name, u.email, '') AS lead, \
     COALESCE((SELECT count(*) FROM module_members mm \
       WHERE mm.module_id = m.id AND mm.deleted_at IS NULL), 0)::int8 AS members, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL), 0)::int8 AS total, \
     COALESCE((SELECT count(*) FROM module_issues mi \
       JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
       JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
       WHERE mi.module_id = m.id AND mi.deleted_at IS NULL \
         AND s.\"group\" = 'completed'), 0)::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.id = $1 AND m.workspace_id = $2 AND m.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL)";

pub const TRACKS_BY_NAME_PROJECT_SQL: &str = "SELECT m.id, m.name, \
     p.identifier AS project, m.status, \
     COALESCE(u.display_name, u.email, '') AS lead, 0::int8 AS members, \
     0::int8 AS total, 0::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.project_id = $1 AND m.workspace_id = $2 AND m.deleted_at IS NULL \
     AND m.archived_at IS NULL \
     AND (lower(m.name) = lower($3) OR m.name ILIKE '%' || $3 || '%') \
     ORDER BY m.created_at DESC LIMIT 10";

pub const TRACKS_BY_NAME_SQL: &str = "SELECT m.id, m.name, p.identifier AS project, \
     m.status, COALESCE(u.display_name, u.email, '') AS lead, 0::int8 AS members, \
     0::int8 AS total, 0::int8 AS completed, \
     m.start_date::text AS start_date, m.target_date::text AS target_date \
     FROM modules m \
     JOIN projects p ON p.id = m.project_id AND p.deleted_at IS NULL \
     LEFT JOIN users u ON u.id = m.lead_id \
     WHERE m.workspace_id = $1 AND m.deleted_at IS NULL AND m.archived_at IS NULL \
     AND p.module_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (lower(m.name) = lower($3) OR m.name ILIKE '%' || $3 || '%') \
     ORDER BY m.created_at DESC LIMIT 10";

pub fn pick_track(rows: Vec<TrackRow>, reference: &str) -> Result<TrackRow, ToolExecutionError> {
    let needle = reference.trim();
    if rows.is_empty() {
        return Err(ToolExecutionError::invalid_args(format!(
            "track '{needle}' was not found or is not accessible"
        )));
    }
    let exact: Vec<&TrackRow> = rows
        .iter()
        .filter(|row| row.name.eq_ignore_ascii_case(needle))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0].clone());
    }
    if exact.len() > 1 || rows.len() > 1 {
        let names = rows
            .iter()
            .map(|row| format!("{}/{}", row.project, row.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ToolExecutionError::invalid_args(format!(
            "track '{needle}' is ambiguous; use one of: {names}"
        )));
    }
    Ok(rows.into_iter().next().expect("one row"))
}

pub async fn resolve_track(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    reference: &str,
    project: Option<&ProjectRow>,
) -> Result<TrackRow, ToolExecutionError> {
    if let Ok(id) = Uuid::parse_str(reference.trim()) {
        let row: Option<TrackRow> = sqlx::query_as(TRACK_BY_ID_SQL)
            .bind(id)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
        return row.ok_or_else(|| {
            ToolExecutionError::invalid_args(format!(
                "track '{reference}' was not found or is not accessible"
            ))
        });
    }
    let rows: Vec<TrackRow> = match project {
        Some(project) => sqlx::query_as(TRACKS_BY_NAME_PROJECT_SQL)
            .bind(project.id)
            .bind(workspace_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
        None => sqlx::query_as(TRACKS_BY_NAME_SQL)
            .bind(workspace_id)
            .bind(user_id)
            .bind(reference.trim())
            .fetch_all(pool)
            .await
            .map_err(db_error)?,
    };
    pick_track(rows, reference)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct TrackLinkRow {
    pub title: Option<String>,
    pub url: String,
}

pub const TRACK_MEMBERS_SQL: &str = "SELECT COALESCE(u.display_name, u.email, 'unknown') \
     FROM module_members mm JOIN users u ON u.id = mm.member_id \
     WHERE mm.module_id = $1 AND mm.deleted_at IS NULL ORDER BY 1";

pub const TRACK_LINKS_SQL: &str = "SELECT title, url FROM module_links \
     WHERE module_id = $1 AND deleted_at IS NULL ORDER BY title NULLS LAST, url";

pub const TRACK_STATE_COUNTS_SQL: &str = "SELECT s.\"group\" AS state_group, \
     count(*)::int8 AS count \
     FROM module_issues mi \
     JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE mi.module_id = $1 AND mi.deleted_at IS NULL \
     GROUP BY s.\"group\" ORDER BY s.\"group\"";

pub const TRACK_OPEN_ITEMS_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, COALESCE(s.\"group\", '') AS state_group, i.priority \
     FROM module_issues mi \
     JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE mi.module_id = $1 AND mi.deleted_at IS NULL \
     AND s.\"group\" NOT IN ('completed', 'cancelled') \
     ORDER BY i.updated_at DESC LIMIT 10";

pub fn track_detail_json(
    row: &TrackRow,
    members: &[String],
    links: &[TrackLinkRow],
    counts: &[crate::tools::work_items::GroupCountRow],
    open_items: &[crate::tools::work_items::WorkItemBrief],
) -> String {
    json!({
        "name": row.name,
        "project": row.project,
        "status": row.status,
        "lead": row.lead,
        "start_date": row.start_date,
        "target_date": row.target_date,
        "members": members,
        "links": links,
        "total": row.total,
        "completed": row.completed,
        "state_counts": counts,
        "open_work_items": open_items,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListTracksArgs {
    /// Project identifier (e.g. "LTS") or project name.
    pub project: Option<String>,
    /// One of: backlog, planned, in-progress, paused, completed, cancelled.
    pub status: Option<String>,
    /// Case-insensitive substring to match against track names.
    pub query: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListTracks {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListTracks {
    const NAME: &'static str = "list_tracks";
    type Args = ListTracksArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List tracks (modules) the caller can access, newest first, optionally \
         filtered by project, status, and name substring. Returns name, project, \
         status, lead, member count, and progress (total and completed work \
         items). Only projects with the tracks feature enabled are included."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListTracksArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Modules).await?;
        }
        let status = track_status_arg(args.status.as_deref())?;
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        let rows: Vec<TrackRow> = sqlx::query_as(TRACKS_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(project.as_ref().map(|row| row.id))
            .bind(&query)
            .bind(&status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(tracks_json(&rows))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetTrackArgs {
    /// Track name, or a track id when known.
    pub track: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
}

pub struct GetTrack {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetTrack {
    const NAME: &'static str = "get_track";
    type Args = GetTrackArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one track by name or id: status, lead, dates, members, resource \
         links, progress (total and completed), the count of work items per \
         state group, and the 10 most recently updated unfinished items."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetTrackArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                ensure_feature(&self.pool, project.id, ProjectFeature::Modules).await?;
                Some(project)
            }
            None => None,
        };
        let row = resolve_track(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.track,
            project.as_ref(),
        )
        .await?;
        let members: Vec<String> = sqlx::query_scalar(TRACK_MEMBERS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let links: Vec<TrackLinkRow> = sqlx::query_as(TRACK_LINKS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let counts: Vec<crate::tools::work_items::GroupCountRow> =
            sqlx::query_as(TRACK_STATE_COUNTS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        let open_items: Vec<crate::tools::work_items::WorkItemBrief> =
            sqlx::query_as(TRACK_OPEN_ITEMS_SQL)
                .bind(row.id)
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        Ok(track_detail_json(
            &row, &members, &links, &counts, &open_items,
        ))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai track`
Expected: seluruh test tracks lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/tracks.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add list_tracks and get_track read tools"
```

---

### Task 5: Tool `list_track_work_items`

**Files:**

- Modify: `crates/ai/src/tools/tracks.rs`

- [ ] **Step 1: Tulis test yang gagal**

```rust
    #[tokio::test]
    async fn list_track_items_rejects_a_bad_state_group_before_querying() {
        let tool = ListTrackWorkItems {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                ListTrackWorkItemsArgs {
                    track: Uuid::new_v4().to_string(),
                    project: None,
                    state_group: Some("nope".to_string()),
                    assignee: None,
                    limit: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("state_group"));
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai list_track_items_rejects`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/tracks.rs`**

```rust
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListTrackWorkItemsArgs {
    /// Track name, or a track id when known.
    pub track: String,
    /// Project identifier or name; narrows a name lookup across projects.
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// Assignee display name, email, or "me" for the signed-in user.
    pub assignee: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub const TRACK_ITEMS_SQL: &str = "SELECT p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, i.name, \
     COALESCE(s.name, '') AS state, i.priority, \
     COALESCE((SELECT it.name FROM issue_types it \
       WHERE it.id = i.type_id AND it.deleted_at IS NULL), '') AS work_item_type, \
     COALESCE((SELECT string_agg(COALESCE(u.display_name, u.email, 'unknown'), ', ' \
         ORDER BY COALESCE(u.display_name, u.email, 'unknown')) \
       FROM issue_assignees ia JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '') AS assignees \
     FROM module_issues mi \
     JOIN issues i ON i.id = mi.issue_id AND i.deleted_at IS NULL AND i.is_draft = false \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.deleted_at IS NULL \
     WHERE mi.module_id = $1 AND mi.deleted_at IS NULL \
     AND ($2::text IS NULL OR s.\"group\" = $2) \
     AND ($3::text IS NULL OR EXISTS (SELECT 1 FROM issue_assignees ia \
       JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND (lower(COALESCE(u.display_name, '')) = lower($3) \
         OR lower(COALESCE(u.email, '')) = lower($3)))) \
     AND ($4::bool = false OR EXISTS (SELECT 1 FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL AND ia.assignee_id = $5)) \
     ORDER BY i.updated_at DESC LIMIT $6";

pub struct ListTrackWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListTrackWorkItems {
    const NAME: &'static str = "list_track_work_items";
    type Args = ListTrackWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List work items in one track, optionally filtered by state group and \
         assignee (a name, email, or \"me\"). Returns project, identifier, name, \
         state, priority, type, and assignees."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListTrackWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let state_group = super::state_group_arg(args.state_group.as_deref())?;
        let (assignee, assignee_me) = super::assignee_arg(args.assignee.as_deref());
        let limit = clamp_limit(args.limit);
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => Some(
                resolve_project(&self.pool, self.workspace_id, self.user_id, &reference).await?,
            ),
            None => None,
        };
        if let Some(project) = project.as_ref() {
            ensure_feature(&self.pool, project.id, ProjectFeature::Modules).await?;
        }
        let track = resolve_track(
            &self.pool,
            self.workspace_id,
            self.user_id,
            &args.track,
            project.as_ref(),
        )
        .await?;
        let rows: Vec<crate::tools::work_items::SearchRow> = sqlx::query_as(TRACK_ITEMS_SQL)
            .bind(track.id)
            .bind(&state_group)
            .bind(&assignee)
            .bind(assignee_me)
            .bind(self.user_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(crate::tools::work_items::search_json(&rows))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai list_track_items_rejects`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/tracks.rs
git commit -m "feat(ai): add list_track_work_items read tool"
```

---

### Task 6: Tools `search_articles` + `get_article`

**Files:**

- Create: `crates/ai/src/tools/kb.rs`
- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Tulis test yang gagal**

Buat `tools/kb.rs` dengan header + tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn article_search_json_shape() {
        let rows = vec![ArticleSearchRow {
            id: Uuid::nil(),
            name: "Runbook: restart API".to_string(),
            project: "LTS".to_string(),
            updated_at: "2026-10-01 00:00:00+00".to_string(),
            snippet: "How to restart the API".to_string(),
        }];
        let parsed: serde_json::Value =
            serde_json::from_str(&articles_json(&rows)).unwrap();
        assert_eq!(parsed["returned"], json!(1));
        assert_eq!(parsed["items"][0]["name"], json!("Runbook: restart API"));
        assert_eq!(parsed["items"][0]["snippet"], json!("How to restart the API"));
    }

    #[test]
    fn article_detail_json_shape() {
        let row = ArticleDetailRow {
            id: Uuid::nil(),
            name: "Runbook: restart API".to_string(),
            project: "LTS".to_string(),
            access: 0,
            page_view: true,
            description: "x".repeat(9000),
        };
        let issues = vec![LinkedIssueRow {
            identifier: "LTS-42".to_string(),
            name: "Fix pump".to_string(),
        }];
        let parsed: serde_json::Value = serde_json::from_str(&article_detail_json(
            &row,
            &["Sub page".to_string()],
            &["runbook".to_string()],
            &issues,
        ))
        .unwrap();
        assert_eq!(parsed["name"], json!("Runbook: restart API"));
        assert_eq!(parsed["sub_pages"][0], json!("Sub page"));
        assert_eq!(parsed["labels"][0], json!("runbook"));
        assert_eq!(parsed["linked_issues"][0]["identifier"], json!("LTS-42"));
        assert_eq!(
            parsed["description"].as_str().unwrap().chars().count(),
            DESCRIPTION_MAX
        );
    }

    #[tokio::test]
    async fn search_articles_requires_a_query() {
        let tool = SearchArticles {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                SearchArticlesArgs {
                    query: "  ".to_string(),
                    project: None,
                    limit: None,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("query"));
    }

    #[tokio::test]
    async fn get_article_rejects_a_non_uuid_before_querying() {
        let tool = GetArticle {
            pool: super::super::lazy_pool(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            trace: crate::agent::new_trace(),
        };
        let error = tool
            .call(
                &mut ToolContext::new(),
                GetArticleArgs {
                    article: "Runbook".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("uuid"));
    }

    #[test]
    fn kb_sql_respects_private_access_and_membership() {
        for sql in [ARTICLE_SEARCH_SQL, ARTICLE_DETAIL_SQL] {
            assert!(sql.contains("workspace_id = $1") || sql.contains("workspace_id = $2"));
            assert!(sql.contains("project_members"));
            assert!(sql.contains("pa.access = 0 OR pa.owned_by_id"));
        }
        assert!(ARTICLE_SEARCH_SQL.contains("p.page_view = true"));
    }
}
```

Header:

```rust
//! Knowledge base (pages/articles) read tools.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    clamp_limit, db_error, ensure_feature, optional_text, resolve_project, schema_of,
    ProjectFeature,
};
use crate::agent::{record, ToolTrace};
use crate::tools::work_items::truncate_chars;
```

Tambahkan di `tools/mod.rs`: `pub mod kb;` dan `pub use kb::*;`.

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai article_search_json_shape`
Expected: gagal kompilasi.

- [ ] **Step 3: Implementasi di `tools/kb.rs`**

```rust
pub const DESCRIPTION_MAX: usize = 8000;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ArticleSearchRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub updated_at: String,
    pub snippet: String,
}

pub const ARTICLE_SEARCH_SQL: &str = "SELECT pa.id, pa.name, \
     p.identifier AS project, pa.updated_at::text AS updated_at, \
     left(COALESCE(pa.description_stripped, ''), 200) AS snippet \
     FROM pages pa \
     JOIN project_pages pp ON pp.page_id = pa.id AND pp.deleted_at IS NULL \
     JOIN projects p ON p.id = pp.project_id AND p.deleted_at IS NULL \
       AND p.archived_at IS NULL \
     WHERE pa.workspace_id = $1 AND pa.deleted_at IS NULL AND pa.archived_at IS NULL \
     AND p.page_view = true \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (pa.access = 0 OR pa.owned_by_id = $2) \
     AND ($3::text IS NULL OR p.identifier ILIKE $3 OR p.name ILIKE '%' || $3 || '%') \
     AND (pa.name ILIKE '%' || $4 || '%' \
       OR COALESCE(pa.description_stripped, '') ILIKE '%' || $4 || '%') \
     AND ($5::uuid IS NULL OR p.id = $5) \
     ORDER BY pa.updated_at DESC LIMIT $6";

pub fn articles_json(rows: &[ArticleSearchRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "name": row.name,
                "project": row.project,
                "updated_at": row.updated_at,
                "snippet": row.snippet,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchArticlesArgs {
    /// Case-insensitive substring matched against article names and bodies. Required.
    pub query: String,
    /// Project identifier or name to scope the search.
    pub project: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct SearchArticles {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for SearchArticles {
    const NAME: &'static str = "search_articles";
    type Args = SearchArticlesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Search knowledge base articles (pages) the caller can access by a \
         substring in the title or body, optionally scoped to a project. \
         Returns id, name, project, updated_at, and a 200-character snippet. \
         Private articles are only visible to their owner. Use the returned id \
         with get_article."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<SearchArticlesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(Some(args.query.as_str()))
            .ok_or_else(|| ToolExecutionError::invalid_args("query is required"))?;
        let project = match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                ensure_feature(&self.pool, project.id, ProjectFeature::Pages).await?;
                Some(project)
            }
            None => None,
        };
        let limit = clamp_limit(args.limit);
        let rows: Vec<ArticleSearchRow> = sqlx::query_as(ARTICLE_SEARCH_SQL)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .bind(None::<String>)
            .bind(&query)
            .bind(project.as_ref().map(|row| row.id))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(articles_json(&rows))
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ArticleDetailRow {
    pub id: Uuid,
    pub name: String,
    pub project: String,
    pub access: i16,
    pub page_view: bool,
    pub description: String,
}

pub const ARTICLE_DETAIL_SQL: &str = "SELECT pa.id, pa.name, \
     p.identifier AS project, pa.access, p.page_view AS page_view, \
     COALESCE(pa.description_stripped, '') AS description \
     FROM pages pa \
     JOIN project_pages pp ON pp.page_id = pa.id AND pp.deleted_at IS NULL \
     JOIN projects p ON p.id = pp.project_id AND p.deleted_at IS NULL \
     WHERE pa.id = $1 AND pa.workspace_id = $2 AND pa.deleted_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND (pa.access = 0 OR pa.owned_by_id = $3) \
     LIMIT 1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct LinkedIssueRow {
    pub identifier: String,
    pub name: String,
}

pub const ARTICLE_SUBPAGES_SQL: &str = "SELECT name FROM pages \
     WHERE parent_id = $1 AND deleted_at IS NULL ORDER BY name";

pub const ARTICLE_LABELS_SQL: &str = "SELECT l.name FROM page_labels pl \
     JOIN labels l ON l.id = pl.label_id AND l.deleted_at IS NULL \
     WHERE pl.page_id = $1 AND pl.deleted_at IS NULL ORDER BY l.name";

pub const ARTICLE_LINKED_ISSUES_SQL: &str = "SELECT \
     p.identifier || '-' || i.sequence_id AS identifier, i.name \
     FROM page_logs l \
     JOIN issues i ON i.id = l.entity_identifier AND i.deleted_at IS NULL \
     JOIN projects p ON p.id = i.project_id AND p.deleted_at IS NULL \
     WHERE l.page_id = $1 AND l.entity_name = 'issue' AND l.deleted_at IS NULL \
     ORDER BY identifier LIMIT 10";

pub fn article_detail_json(
    row: &ArticleDetailRow,
    sub_pages: &[String],
    labels: &[String],
    linked_issues: &[LinkedIssueRow],
) -> String {
    json!({
        "id": row.id,
        "name": row.name,
        "project": row.project,
        "description": truncate_chars(&row.description, DESCRIPTION_MAX),
        "sub_pages": sub_pages,
        "labels": labels,
        "linked_issues": linked_issues,
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetArticleArgs {
    /// Article id (uuid) as returned by search_articles.
    pub article: String,
}

pub struct GetArticle {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for GetArticle {
    const NAME: &'static str = "get_article";
    type Args = GetArticleArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Get one knowledge base article by id: body text (truncated), \
         sub-pages, labels, and the work items linked to it. Private articles \
         are only visible to their owner."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<GetArticleArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let article = Uuid::parse_str(args.article.trim()).map_err(|_| {
            ToolExecutionError::invalid_args("article must be a uuid from search_articles")
        })?;
        let row: Option<ArticleDetailRow> = sqlx::query_as(ARTICLE_DETAIL_SQL)
            .bind(article)
            .bind(self.workspace_id)
            .bind(self.user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?;
        let Some(row) = row else {
            return Err(ToolExecutionError::invalid_args(format!(
                "article '{}' was not found or is not accessible",
                args.article
            )));
        };
        if !row.page_view {
            return Err(ToolExecutionError::invalid_args(
                "the knowledge base feature is disabled in this project",
            ));
        }
        let sub_pages: Vec<String> = sqlx::query_scalar(ARTICLE_SUBPAGES_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let labels: Vec<String> = sqlx::query_scalar(ARTICLE_LABELS_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        let linked_issues: Vec<LinkedIssueRow> = sqlx::query_as(ARTICLE_LINKED_ISSUES_SQL)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(article_detail_json(&row, &sub_pages, &labels, &linked_issues))
    }
}
```

Catatan: `search_articles` tidak memakai filter `project` teks di SQL karena project di-resolve ke UUID; bind `$3` selalu `None::<String>` (dihapus dari args efektif), sedangkan `$5` adalah project id.

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai article`
Expected: seluruh test kb lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/kb.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add knowledge base read tools"
```

---

### Task 7: Registrasi + verifikasi akhir

**Files:**

- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Daftarkan 8 tool baru di kedua registry**

Tambahkan ke rantai `.tool(...)` `workspace_tools` (setelah `CountIntakeItems`, sebelum `CreateSchedule`):

```rust
        .tool(ListSprints {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetSprint {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListSprintWorkItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListTracks {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetTrack {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListTrackWorkItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(SearchArticles {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(GetArticle {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
```

Tambahkan blok kondisional yang sama di `read_tools` sebelum `server.run()`
(pola `if allowed.contains(&TYPE::NAME) { ... }`; tool terakhir memakai `pool` tanpa clone).

- [ ] **Step 2: Tambah test metadata**

```rust
#[tokio::test]
async fn sprint_track_and_kb_tools_expose_metadata() {
    let pool = lazy_pool();
    let trace = crate::agent::new_trace();
    let sprints = ListSprints {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListSprints::NAME, "list_sprints");
    assert!(sprints.parameters()["properties"]["status"].is_object());
    let get_sprint = GetSprint {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(GetSprint::NAME, "get_sprint");
    let sprint_items = ListSprintWorkItems {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListSprintWorkItems::NAME, "list_sprint_work_items");
    let tracks = ListTracks {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListTracks::NAME, "list_tracks");
    let get_track = GetTrack {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(GetTrack::NAME, "get_track");
    let track_items = ListTrackWorkItems {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListTrackWorkItems::NAME, "list_track_work_items");
    let search = SearchArticles {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(SearchArticles::NAME, "search_articles");
    assert!(search.parameters()["properties"]["query"].is_object());
    let article = GetArticle {
        pool,
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace,
    };
    assert_eq!(GetArticle::NAME, "get_article");
    assert!(!article.description().is_empty());
}
```

- [ ] **Step 3: Sinkronkan allowlist schedule (backend + FE + PREAMBLE)**

Tambah delapan nama tool baru ke `SPEC_TOOLS` di `crates/ai/src/schedule.rs`
(ubah panjang array menjadi 23) mengikuti urutan deklarasi di file:

```rust
pub const SPEC_TOOLS: [&str; 23] = [
    "list_projects",
    "count_work_items",
    "search_work_items",
    "get_work_item",
    "list_work_item_comments",
    "list_work_item_relations",
    "list_members",
    "list_states",
    "list_labels",
    "list_work_item_types",
    "list_services",
    "get_service",
    "list_intake_items",
    "get_intake_item",
    "count_intake_items",
    "list_sprints",
    "get_sprint",
    "list_sprint_work_items",
    "list_tracks",
    "get_track",
    "list_track_work_items",
    "search_articles",
    "get_article",
];
```

Perbarui `assert_eq!(SPEC_TOOLS.len(), 23)` di test `spec_accepts_every_spec_tool`,
test worker `allowed_tools_legacy_is_all_read_tools`, `AI_SCHEDULE_TOOLS` di
`apps/web/core/lib/ai-schedule.ts`, dan ekspektasi test
`apps/web/core/lib/ai-schedule.test.ts` dengan delapan nama baru.

Di `PREAMBLE` (`crates/ai/src/agent.rs`) ganti frasa:

```
projects, work items, services, the intake triage queue, people, and their metadata
```

menjadi:

```
projects, work items, services, the intake queue, sprints, tracks, knowledge base articles, people, and their metadata
```

- [ ] **Step 4: Verifikasi**

Run: `cargo test -p ai`
Expected: seluruh test lulus (read tool = 18, plus proposal dan helper).

Run: `cargo test -p worker allowed_tools`
Expected: lulus.

Run: `cargo build -p api -p worker`
Expected: sukses tanpa warning.

Run (dari `apps/web`): `pnpm test core/lib/ai-schedule.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs apps/api-rs/crates/ai/src/schedule.rs apps/api-rs/crates/ai/src/agent.rs apps/api-rs/crates/worker/src/handlers/ai_schedule.rs apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts
git commit -m "feat(ai): register sprint, track, and kb read tools"
```

---

### Task 8: Smoke manual (opsional)

- [ ] Rebuild backend (perintah `setsid docker compose ...` dari `AGENTS.md`), lalu uji chat:
  1. `sprint apa yang sedang berjalan di LTS` — `list_sprints` + status current.
  2. `progress sprint <nama>` — `get_sprint` (counts live).
  3. `tiket track <nama> yang belum selesai` — `list_track_work_items` + state_group.
  4. `cari artikel tentang restart API` — `search_articles`, lalu `detail artikel <id>`.
  5. Artikel private milik user lain tidak muncul; project dengan `page_view=false` → pesan "feature is disabled".
