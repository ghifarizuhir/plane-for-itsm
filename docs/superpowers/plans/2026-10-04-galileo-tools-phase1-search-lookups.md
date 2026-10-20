# Galileo Tools Fase 1B — Search Filters, Lookups, Schedule Sync — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Melengkapi Batch 1.1: filter baru `search_work_items`/`count_work_items` (service, type, assignee/`me`, sprint, track, label), empat tool lookup (`list_members`, `list_states`, `list_labels`, `list_work_item_types`), sinkronisasi allowlist schedule (backend + FE), dan update `PREAMBLE`.

**Architecture:** Semua filter dinormalisasi lewat satu struct `WorkItemFilters` di `tools/work_items.rs`; SQL tetap static dengan bind nullable dan guard `project_members` (memakai `user_id` dari Plan 1A). Tool lookup baru masuk `tools/lookups.rs` dan memakai `resolve_project` dari `tools/mod.rs`. `SPEC_TOOLS`/`AI_SCHEDULE_TOOLS` diperluas; tool baru didaftarkan di `workspace_tools` + `read_tools`.

**Tech Stack:** Rust (axum, sqlx, Rig 0.42), serde/schemars, vitest untuk FE; perintah Rust dari `apps/api-rs`.

**Prasyarat:** Plan 1A sudah dieksekusi (refactor `tools/`, `user_id`, `resolve_project`, `resolve_work_item`, tiga tool work item).

**Spec:** `docs/superpowers/specs/2026-10-04-galileo-tools-roadmap-design.md` (§4.3, §4.5, §4.6, §4.7, §5.1).

---

## File Structure

| File                                        | Tanggung jawab                                                             |
| ------------------------------------------- | -------------------------------------------------------------------------- |
| `crates/ai/src/tools/work_items.rs`         | `WorkItemFilters`, `SearchRow`, SQL search/count yang diperluas, bind baru |
| `crates/ai/src/tools/lookups.rs` (baru)     | `ListMembers`, `ListStates`, `ListLabels`, `ListWorkItemTypes`             |
| `crates/ai/src/tools/mod.rs`                | Module decl + re-export `lookups`, registrasi 4 tool baru                  |
| `crates/ai/src/schedule.rs`                 | `SPEC_TOOLS` diperluas                                                     |
| `crates/ai/src/tools/proposals.rs`          | Deskripsi `create_schedule` tidak lagi menyebut tiga tool spesifik         |
| `crates/ai/src/agent.rs`                    | `PREAMBLE` diperbarui                                                      |
| `crates/worker/src/handlers/ai_schedule.rs` | Test ekspektasi legacy tools diperbarui                                    |
| `apps/web/core/lib/ai-schedule.ts`          | `AI_SCHEDULE_TOOLS` diperluas                                              |

---

### Task 1: `WorkItemFilters` + normalisasi

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`
- Test: `crates/ai/src/tools/work_items.rs` (`mod tests`)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/work_items.rs`:

```rust
#[test]
fn work_item_filters_normalize_me_and_trim() {
    let filters = WorkItemFilters::normalize(
        None,
        Some(" lts "),
        Some("Started"),
        Some("URGENT"),
        Some(" Incident "),
        Some(" API "),
        Some("me"),
        Some(" Sprint 3 "),
        Some(" Track A "),
        Some(" bug "),
    )
    .unwrap();
    assert_eq!(filters.project.as_deref(), Some("lts"));
    assert_eq!(filters.state_group.as_deref(), Some("started"));
    assert_eq!(filters.priority.as_deref(), Some("urgent"));
    assert_eq!(filters.work_item_type.as_deref(), Some("Incident"));
    assert_eq!(filters.service.as_deref(), Some("API"));
    assert!(filters.assignee_me);
    assert_eq!(filters.assignee, None);
    assert_eq!(filters.sprint.as_deref(), Some("Sprint 3"));
    assert_eq!(filters.track.as_deref(), Some("Track A"));
    assert_eq!(filters.label.as_deref(), Some("bug"));
}

#[test]
fn work_item_filters_keep_a_named_assignee() {
    let filters = WorkItemFilters::normalize(
        None, None, None, None, None, None, Some("Budi"), None, None, None,
    )
    .unwrap();
    assert_eq!(filters.assignee.as_deref(), Some("Budi"));
    assert!(!filters.assignee_me);
}

#[test]
fn work_item_filters_validate_allowlists() {
    assert!(
        WorkItemFilters::normalize(
            None, None, Some("nope"), None, None, None, None, None, None, None
        )
        .is_err()
    );
    assert!(
        WorkItemFilters::normalize(
            None, None, None, Some("p0"), None, None, None, None, None, None
        )
        .is_err()
    );
}

#[test]
fn work_item_filters_assignee_label_prefers_me() {
    let me = WorkItemFilters {
        assignee_me: true,
        ..Default::default()
    };
    assert_eq!(me.assignee_label().as_deref(), Some("me"));
    let named = WorkItemFilters {
        assignee: Some("Budi".to_string()),
        ..Default::default()
    };
    assert_eq!(named.assignee_label().as_deref(), Some("Budi"));
    assert_eq!(WorkItemFilters::default().assignee_label(), None);
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai work_item_filters`
Expected: gagal kompilasi — `WorkItemFilters` belum ada.

- [ ] **Step 3: Implementasi di `tools/work_items.rs`**

Tambahkan sebelum definisi tool:

```rust
/// Normalized filter set shared by `search_work_items` and `count_work_items`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkItemFilters {
    pub query: Option<String>,
    pub project: Option<String>,
    pub state_group: Option<String>,
    pub priority: Option<String>,
    pub work_item_type: Option<String>,
    pub service: Option<String>,
    pub assignee: Option<String>,
    pub assignee_me: bool,
    pub sprint: Option<String>,
    pub track: Option<String>,
    pub label: Option<String>,
}

impl WorkItemFilters {
    #[allow(clippy::too_many_arguments)]
    pub fn normalize(
        query: Option<&str>,
        project: Option<&str>,
        state_group: Option<&str>,
        priority: Option<&str>,
        work_item_type: Option<&str>,
        service: Option<&str>,
        assignee: Option<&str>,
        sprint: Option<&str>,
        track: Option<&str>,
        label: Option<&str>,
    ) -> Result<Self, ToolExecutionError> {
        let assignee = optional_text(assignee);
        let (assignee, assignee_me) = match assignee {
            Some(value) if value.eq_ignore_ascii_case("me") => (None, true),
            other => (other, false),
        };
        Ok(Self {
            query: optional_text(query),
            project: optional_text(project),
            state_group: state_group_arg(state_group)?,
            priority: priority_arg(priority)?,
            work_item_type: optional_text(work_item_type),
            service: optional_text(service),
            assignee,
            assignee_me,
            sprint: optional_text(sprint),
            track: optional_text(track),
            label: optional_text(label),
        })
    }

    /// Echo of the assignee filter for tool output: "me", a name, or nothing.
    pub fn assignee_label(&self) -> Option<String> {
        if self.assignee_me {
            Some("me".to_string())
        } else {
            self.assignee.clone()
        }
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai work_item_filters`
Expected: 4 test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs
git commit -m "feat(ai): add normalized work item filters"
```

---

### Task 2: Perluas `search_work_items`

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`

- [ ] **Step 1: Tulis test yang gagal**

Ganti test `output_json_shapes` bagian search (baris dengan `search_json(&[(...tuple...)])`) menjadi:

```rust
        let search = search_json(&[SearchRow {
            project: "LTS".to_string(),
            identifier: "LTS-12".to_string(),
            name: "Fix pump".to_string(),
            state: "In Progress".to_string(),
            priority: "urgent".to_string(),
            work_item_type: "Incident".to_string(),
            assignees: "Budi, Sari".to_string(),
        }]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&search).unwrap(),
            json!({"returned": 1, "items": [{
                "project": "LTS",
                "identifier": "LTS-12",
                "name": "Fix pump",
                "state": "In Progress",
                "priority": "urgent",
                "type": "Incident",
                "assignees": "Budi, Sari"
            }]})
        );
```

Tambahkan test baru:

```rust
#[test]
fn search_sql_carries_the_new_filters_and_membership_guard() {
    for fragment in [
        "service_issues",
        "cycle_issues",
        "module_issues",
        "issue_labels",
        "issue_types",
        "project_members",
        "$9::bool = false",
    ] {
        assert!(SEARCH_SQL.contains(fragment), "missing {fragment}");
    }
    assert!(SEARCH_SQL.contains("LIMIT $14"));
}

#[tokio::test]
async fn search_work_items_normalizes_filters_before_query() {
    let tool = SearchWorkItems {
        pool: lazy_pool(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: crate::agent::new_trace(),
    };
    let error = tool
        .call(
            &mut ToolContext::new(),
            SearchWorkItemsArgs {
                query: None,
                project: None,
                state_group: Some("nope".to_string()),
                priority: None,
                work_item_type: None,
                service: None,
                assignee: None,
                sprint: None,
                track: None,
                label: None,
                limit: None,
            },
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("state_group"));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai search_sql_carries`
Expected: gagal kompilasi — `SearchRow`/field baru belum ada.

- [ ] **Step 3: Ganti `SEARCH_SQL`, `SearchRow`, `search_json`, `SearchWorkItemsArgs`, dan `SearchWorkItems::call`**

```rust
pub const SEARCH_SQL: &str = "SELECT p.identifier AS project, \
     p.identifier || '-' || i.sequence_id AS identifier, \
     i.name, COALESCE(s.name, '') AS state, i.priority, \
     COALESCE((SELECT it.name FROM issue_types it \
       WHERE it.id = i.type_id AND it.deleted_at IS NULL), '') AS work_item_type, \
     COALESCE((SELECT string_agg(COALESCE(u.display_name, u.email, 'unknown'), ', ' \
         ORDER BY COALESCE(u.display_name, u.email, 'unknown')) \
       FROM issue_assignees ia JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL), '') AS assignees \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 \
       AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.workspace_id = $1 \
       AND s.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND s.\"group\" <> 'triage' \
     AND i.archived_at IS NULL \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $10 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($2::text IS NULL OR i.name ILIKE '%' || $2 || '%') \
     AND ($3::text IS NULL OR p.identifier ILIKE $3 OR p.name ILIKE '%' || $3 || '%') \
     AND ($4::text IS NULL OR s.\"group\" = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     AND ($6::text IS NULL OR EXISTS (SELECT 1 FROM issue_types it \
       WHERE it.id = i.type_id AND it.deleted_at IS NULL \
       AND (lower(it.name) = lower($6) OR it.name ILIKE '%' || $6 || '%'))) \
     AND ($7::text IS NULL OR EXISTS (SELECT 1 FROM service_issues si \
       JOIN services sv ON sv.id = si.service_id AND sv.deleted_at IS NULL \
       WHERE si.issue_id = i.id AND si.deleted_at IS NULL \
       AND (lower(sv.name) = lower($7) OR sv.name ILIKE '%' || $7 || '%'))) \
     AND ($8::text IS NULL OR EXISTS (SELECT 1 FROM issue_assignees ia \
       JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND (lower(COALESCE(u.display_name, '')) = lower($8) \
         OR lower(COALESCE(u.email, '')) = lower($8)))) \
     AND ($9::bool = false OR EXISTS (SELECT 1 FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL AND ia.assignee_id = $10)) \
     AND ($11::text IS NULL OR EXISTS (SELECT 1 FROM cycle_issues ci \
       JOIN cycles c ON c.id = ci.cycle_id AND c.deleted_at IS NULL \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL \
       AND (lower(c.name) = lower($11) OR c.name ILIKE '%' || $11 || '%'))) \
     AND ($12::text IS NULL OR EXISTS (SELECT 1 FROM module_issues mi \
       JOIN modules m ON m.id = mi.module_id AND m.deleted_at IS NULL \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
       AND (lower(m.name) = lower($12) OR m.name ILIKE '%' || $12 || '%'))) \
     AND ($13::text IS NULL OR EXISTS (SELECT 1 FROM issue_labels il \
       JOIN labels l ON l.id = il.label_id AND l.deleted_at IS NULL \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL \
       AND (lower(l.name) = lower($13) OR l.name ILIKE '%' || $13 || '%'))) \
     ORDER BY i.updated_at DESC LIMIT $14";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SearchRow {
    pub project: String,
    pub identifier: String,
    pub name: String,
    pub state: String,
    pub priority: String,
    pub work_item_type: String,
    pub assignees: String,
}

pub fn search_json(rows: &[SearchRow]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "project": row.project,
                "identifier": row.identifier,
                "name": row.name,
                "state": row.state,
                "priority": row.priority,
                "type": row.work_item_type,
                "assignees": row.assignees,
            })
        })
        .collect();
    json!({"returned": items.len(), "items": items}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchWorkItemsArgs {
    /// Case-insensitive substring to match against work item names.
    pub query: Option<String>,
    /// Project identifier (case-insensitive exact, e.g. "LTS") or project name (case-insensitive substring).
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Work item type name, e.g. "Incident".
    #[serde(rename = "type")]
    pub work_item_type: Option<String>,
    /// Service name (case-insensitive exact or substring).
    pub service: Option<String>,
    /// Assignee display name, email, or "me" for the signed-in user.
    pub assignee: Option<String>,
    /// Sprint (cycle) name (case-insensitive exact or substring).
    pub sprint: Option<String>,
    /// Track (module) name (case-insensitive exact or substring).
    pub track: Option<String>,
    /// Label name (case-insensitive exact or substring).
    pub label: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}
```

Ganti isi `SearchWorkItems`:

```rust
pub struct SearchWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for SearchWorkItems {
    const NAME: &'static str = "search_work_items";
    type Args = SearchWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Search non-archived, non-draft work items the caller can access by \
         name substring, optionally filtered by project, state group, priority, \
         type, service, assignee (a name, email, or \"me\"), sprint, track, and \
         label. `returned` is the number of rows returned (at most limit), not \
         the total match count. Returns project, identifier, name, state, \
         priority, type, and assignees."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<SearchWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let filters = WorkItemFilters::normalize(
            args.query.as_deref(),
            args.project.as_deref(),
            args.state_group.as_deref(),
            args.priority.as_deref(),
            args.work_item_type.as_deref(),
            args.service.as_deref(),
            args.assignee.as_deref(),
            args.sprint.as_deref(),
            args.track.as_deref(),
            args.label.as_deref(),
        )?;
        let limit = clamp_limit(args.limit);
        let rows: Vec<SearchRow> = sqlx::query_as(SEARCH_SQL)
            .bind(self.workspace_id)
            .bind(&filters.query)
            .bind(&filters.project)
            .bind(&filters.state_group)
            .bind(&filters.priority)
            .bind(&filters.work_item_type)
            .bind(&filters.service)
            .bind(&filters.assignee)
            .bind(filters.assignee_me)
            .bind(self.user_id)
            .bind(&filters.sprint)
            .bind(&filters.track)
            .bind(&filters.label)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(search_json(&rows))
    }
}
```

Perhatikan: `search_json` tidak lagi memakai `search_json(&rows)` tuple; test lama sudah diganti di Step 1.

- [ ] **Step 4: Perbarui konstruksi `SearchWorkItems` di `tools/mod.rs`**

`SearchWorkItems` kini menyimpan `user_id`; tambahkan field itu di **dua** tempat — rantai `.tool(...)` di `workspace_tools` dan blok `if allowed.contains(&SearchWorkItems::NAME)` di `read_tools`:

```rust
        .tool(SearchWorkItems {
            pool,
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
```

```rust
    if allowed.contains(&SearchWorkItems::NAME) {
        server = server.tool(SearchWorkItems {
            pool,
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai search`
Expected: seluruh test search lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): extend search_work_items filters"
```

---

### Task 3: Perluas `count_work_items`

**Files:**

- Modify: `crates/ai/src/tools/work_items.rs`

- [ ] **Step 1: Tulis test yang gagal**

Ganti test `output_json_shapes` bagian count menjadi:

```rust
        let count = count_json(
            7,
            &WorkItemFilters {
                project: Some("LTS".to_string()),
                state_group: Some("started".to_string()),
                priority: Some("urgent".to_string()),
                ..Default::default()
            },
            false,
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&count).unwrap(),
            json!({
                "count": 7,
                "filters": {
                    "project": "LTS",
                    "state_group": "started",
                    "priority": "urgent",
                    "include_archived": false,
                    "type": null,
                    "service": null,
                    "assignee": null,
                    "sprint": null,
                    "track": null,
                    "label": null
                }
            })
        );
```

Tambahkan test baru:

```rust
#[test]
fn count_sql_carries_the_new_filters_and_membership_guard() {
    for fragment in [
        "service_issues",
        "cycle_issues",
        "module_issues",
        "issue_labels",
        "issue_types",
        "project_members",
        "$9::bool = false",
    ] {
        assert!(COUNT_SQL.contains(fragment), "missing {fragment}");
    }
}
```

Update literal `CountWorkItemsArgs` di test `invalid_allowlist_values_surface_before_any_query` menjadi:

```rust
                CountWorkItemsArgs {
                    project: None,
                    state_group: Some("nope".to_string()),
                    priority: None,
                    include_archived: None,
                    work_item_type: None,
                    service: None,
                    assignee: None,
                    sprint: None,
                    track: None,
                    label: None,
                },
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai count_sql_carries`
Expected: gagal kompilasi — field/`COUNT_SQL` baru belum ada.

- [ ] **Step 3: Ganti `COUNT_SQL`, `count_json`, `CountWorkItemsArgs`, `CountWorkItems`**

```rust
pub const COUNT_SQL: &str = "SELECT count(*)::int8 FROM issues i \
     JOIN projects p ON p.id = i.project_id AND p.workspace_id = $1 \
       AND p.deleted_at IS NULL AND p.archived_at IS NULL \
     JOIN states s ON s.id = i.state_id AND s.workspace_id = $1 \
       AND s.deleted_at IS NULL \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.is_draft = false \
     AND s.\"group\" <> 'triage' \
     AND EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
       AND pm.member_id = $10 AND pm.is_active = true AND pm.deleted_at IS NULL) \
     AND ($2::text IS NULL OR p.identifier ILIKE $2 OR p.name ILIKE '%' || $2 || '%') \
     AND ($3::text IS NULL OR s.\"group\" = $3) \
     AND ($4::text IS NULL OR i.priority = $4) \
     AND ($5::bool OR i.archived_at IS NULL) \
     AND ($6::text IS NULL OR EXISTS (SELECT 1 FROM issue_types it \
       WHERE it.id = i.type_id AND it.deleted_at IS NULL \
       AND (lower(it.name) = lower($6) OR it.name ILIKE '%' || $6 || '%'))) \
     AND ($7::text IS NULL OR EXISTS (SELECT 1 FROM service_issues si \
       JOIN services sv ON sv.id = si.service_id AND sv.deleted_at IS NULL \
       WHERE si.issue_id = i.id AND si.deleted_at IS NULL \
       AND (lower(sv.name) = lower($7) OR sv.name ILIKE '%' || $7 || '%'))) \
     AND ($8::text IS NULL OR EXISTS (SELECT 1 FROM issue_assignees ia \
       JOIN users u ON u.id = ia.assignee_id \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
       AND (lower(COALESCE(u.display_name, '')) = lower($8) \
         OR lower(COALESCE(u.email, '')) = lower($8)))) \
     AND ($9::bool = false OR EXISTS (SELECT 1 FROM issue_assignees ia \
       WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL AND ia.assignee_id = $10)) \
     AND ($11::text IS NULL OR EXISTS (SELECT 1 FROM cycle_issues ci \
       JOIN cycles c ON c.id = ci.cycle_id AND c.deleted_at IS NULL \
       WHERE ci.issue_id = i.id AND ci.deleted_at IS NULL \
       AND (lower(c.name) = lower($11) OR c.name ILIKE '%' || $11 || '%'))) \
     AND ($12::text IS NULL OR EXISTS (SELECT 1 FROM module_issues mi \
       JOIN modules m ON m.id = mi.module_id AND m.deleted_at IS NULL \
       WHERE mi.issue_id = i.id AND mi.deleted_at IS NULL \
       AND (lower(m.name) = lower($12) OR m.name ILIKE '%' || $12 || '%'))) \
     AND ($13::text IS NULL OR EXISTS (SELECT 1 FROM issue_labels il \
       JOIN labels l ON l.id = il.label_id AND l.deleted_at IS NULL \
       WHERE il.issue_id = i.id AND il.deleted_at IS NULL \
       AND (lower(l.name) = lower($13) OR l.name ILIKE '%' || $13 || '%')))";

pub fn count_json(count: i64, filters: &WorkItemFilters, include_archived: bool) -> String {
    json!({
        "count": count,
        "filters": {
            "project": filters.project,
            "state_group": filters.state_group,
            "priority": filters.priority,
            "include_archived": include_archived,
            "type": filters.work_item_type,
            "service": filters.service,
            "assignee": filters.assignee_label(),
            "sprint": filters.sprint,
            "track": filters.track,
            "label": filters.label,
        }
    })
    .to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CountWorkItemsArgs {
    /// Project identifier (case-insensitive exact, e.g. "LTS") or project name (case-insensitive substring).
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Include archived work items. Defaults to false.
    pub include_archived: Option<bool>,
    /// Work item type name, e.g. "Incident".
    #[serde(rename = "type")]
    pub work_item_type: Option<String>,
    /// Service name (case-insensitive exact or substring).
    pub service: Option<String>,
    /// Assignee display name, email, or "me" for the signed-in user.
    pub assignee: Option<String>,
    /// Sprint (cycle) name (case-insensitive exact or substring).
    pub sprint: Option<String>,
    /// Track (module) name (case-insensitive exact or substring).
    pub track: Option<String>,
    /// Label name (case-insensitive exact or substring).
    pub label: Option<String>,
}
```

Ganti isi `CountWorkItems`:

```rust
pub struct CountWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for CountWorkItems {
    const NAME: &'static str = "count_work_items";
    type Args = CountWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Count non-deleted, non-draft work items the caller can access, \
         excluding triage items and items in archived projects. Archived items \
         are excluded unless include_archived is true. Optionally filtered by \
         project, state group, priority, type, service, assignee (a name, \
         email, or \"me\"), sprint, track, and label."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CountWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let filters = WorkItemFilters::normalize(
            None,
            args.project.as_deref(),
            args.state_group.as_deref(),
            args.priority.as_deref(),
            args.work_item_type.as_deref(),
            args.service.as_deref(),
            args.assignee.as_deref(),
            args.sprint.as_deref(),
            args.track.as_deref(),
            args.label.as_deref(),
        )?;
        let include_archived = args.include_archived.unwrap_or(false);
        let count: i64 = sqlx::query_scalar(COUNT_SQL)
            .bind(self.workspace_id)
            .bind(&filters.project)
            .bind(&filters.state_group)
            .bind(&filters.priority)
            .bind(include_archived)
            .bind(&filters.work_item_type)
            .bind(&filters.service)
            .bind(&filters.assignee)
            .bind(filters.assignee_me)
            .bind(self.user_id)
            .bind(&filters.sprint)
            .bind(&filters.track)
            .bind(&filters.label)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(count_json(count, &filters, include_archived))
    }
}
```

- [ ] **Step 4: Perbarui konstruksi `CountWorkItems` di `tools/mod.rs`**

`CountWorkItems` kini menyimpan `user_id`; tambahkan field itu di rantai `.tool(...)` `workspace_tools` dan blok `if allowed.contains(&CountWorkItems::NAME)` di `read_tools`:

```rust
        .tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai count`
Expected: seluruh test count lulus.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/work_items.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): extend count_work_items filters"
```

---

### Task 4: Tool `list_members` + file `tools/lookups.rs`

**Files:**

- Create: `crates/ai/src/tools/lookups.rs`
- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Tulis test yang gagal**

Buat `tools/lookups.rs` dengan header + `#[cfg(test)] mod tests` berisi:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn role_label_maps_plane_roles() {
        assert_eq!(role_label(20), "admin");
        assert_eq!(role_label(15), "member");
        assert_eq!(role_label(5), "guest");
        assert_eq!(role_label(99), "unknown");
    }

    #[test]
    fn members_json_shape() {
        let rows = vec![
            MemberRow {
                name: "Budi".to_string(),
                email: "budi@example.com".to_string(),
                role: 20,
            },
            MemberRow {
                name: "Sari".to_string(),
                email: String::new(),
                role: 15,
            },
        ];
        let parsed: serde_json::Value =
            serde_json::from_str(&members_json("project LTS", &rows)).unwrap();
        assert_eq!(parsed["scope"], json!("project LTS"));
        assert_eq!(parsed["returned"], json!(2));
        assert_eq!(parsed["members"][0]["role"], json!("admin"));
        assert_eq!(parsed["members"][1]["email"], json!(""));
    }
}
```

dan header file:

```rust
//! Lookup tools: members, states, labels, and work item types.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{clamp_limit, db_error, optional_text, resolve_project, schema_of};
use crate::agent::{record, ToolTrace};
```

Tambahkan di `tools/mod.rs`:

```rust
pub mod lookups;
```

dan `pub use lookups::*;` di antara re-export yang ada.

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai role_label_maps`
Expected: gagal kompilasi — `role_label`, `MemberRow`, `members_json` belum ada.

- [ ] **Step 3: Implementasi di `tools/lookups.rs`**

```rust
/// Plane project/workspace role codes to display labels.
pub fn role_label(role: i16) -> &'static str {
    match role {
        20 => "admin",
        15 => "member",
        5 => "guest",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MemberRow {
    pub name: String,
    pub email: String,
    pub role: i16,
}

pub fn members_json(scope: &str, rows: &[MemberRow]) -> String {
    let members: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "email": row.email,
                "role": role_label(row.role),
            })
        })
        .collect();
    json!({"scope": scope, "returned": members.len(), "members": members}).to_string()
}

pub const PROJECT_MEMBERS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS name, \
     COALESCE(u.email, '') AS email, pm.role AS role \
     FROM project_members pm JOIN users u ON u.id = pm.member_id \
     WHERE pm.project_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL \
     AND ($2::text IS NULL OR u.display_name ILIKE '%' || $2 || '%' \
       OR COALESCE(u.email, '') ILIKE '%' || $2 || '%') \
     ORDER BY u.display_name, u.id LIMIT $3";

pub const WORKSPACE_MEMBERS_SQL: &str = "SELECT \
     COALESCE(u.display_name, u.email, 'unknown') AS name, \
     COALESCE(u.email, '') AS email, wm.role AS role \
     FROM workspace_members wm JOIN users u ON u.id = wm.member_id \
     WHERE wm.workspace_id = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
     AND ($2::text IS NULL OR u.display_name ILIKE '%' || $2 || '%' \
       OR COALESCE(u.email, '') ILIKE '%' || $2 || '%') \
     ORDER BY u.display_name, u.id LIMIT $3";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListMembersArgs {
    /// Project identifier or name to scope the list; omitted means the whole workspace.
    pub project: Option<String>,
    /// Case-insensitive substring matched against display name or email.
    pub query: Option<String>,
    /// Maximum members to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

pub struct ListMembers {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListMembers {
    const NAME: &'static str = "list_members";
    type Args = ListMembersArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List people with their role: project members when a project is given, \
         otherwise workspace members. Optionally filtered by a name or email \
         substring. Use this to resolve a person before proposing assignments."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListMembersArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let limit = clamp_limit(args.limit);
        match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                let rows: Vec<MemberRow> = sqlx::query_as(PROJECT_MEMBERS_SQL)
                    .bind(project.id)
                    .bind(&query)
                    .bind(limit)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)?;
                Ok(members_json(
                    &format!("project {}", project.identifier),
                    &rows,
                ))
            }
            None => {
                let rows: Vec<MemberRow> = sqlx::query_as(WORKSPACE_MEMBERS_SQL)
                    .bind(self.workspace_id)
                    .bind(&query)
                    .bind(limit)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)?;
                Ok(members_json("workspace", &rows))
            }
        }
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai role_label_maps`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/lookups.rs apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): add list_members lookup tool"
```

---

### Task 5: Tool `list_states` + `list_labels`

**Files:**

- Modify: `crates/ai/src/tools/lookups.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `tools/lookups.rs`:

```rust
#[test]
fn states_json_shape() {
    let rows = vec![
        StateRow {
            name: "Backlog".to_string(),
            group: "backlog".to_string(),
            is_default: false,
            is_triage: false,
        },
        StateRow {
            name: "In Progress".to_string(),
            group: "started".to_string(),
            is_default: true,
            is_triage: false,
        },
    ];
    let parsed: serde_json::Value =
        serde_json::from_str(&states_json("LTS", &rows)).unwrap();
    assert_eq!(parsed["project"], json!("LTS"));
    assert_eq!(parsed["returned"], json!(2));
    assert_eq!(parsed["states"][0]["name"], json!("Backlog"));
    assert_eq!(parsed["states"][1]["group"], json!("started"));
    assert_eq!(parsed["states"][1]["default"], json!(true));
}

#[test]
fn labels_json_shape() {
    let parsed: serde_json::Value = serde_json::from_str(&labels_json(
        "LTS",
        &["bug".to_string(), "infra".to_string()],
    ))
    .unwrap();
    assert_eq!(parsed["project"], json!("LTS"));
    assert_eq!(parsed["returned"], json!(2));
    assert_eq!(parsed["labels"][0], json!("bug"));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai states_json_shape`
Expected: gagal kompilasi — `StateRow`/`states_json` belum ada.

- [ ] **Step 3: Implementasi di `tools/lookups.rs`**

```rust
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StateRow {
    pub name: String,
    pub group: String,
    pub is_default: bool,
    pub is_triage: bool,
}

pub const STATES_SQL: &str = "SELECT s.name, s.\"group\" AS \"group\", \
     s.\"default\" AS is_default, s.is_triage \
     FROM states s \
     WHERE s.project_id = $1 AND s.workspace_id = $2 AND s.deleted_at IS NULL \
     ORDER BY s.sequence";

pub fn states_json(project: &str, rows: &[StateRow]) -> String {
    let states: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "group": row.group,
                "default": row.is_default,
                "is_triage": row.is_triage,
            })
        })
        .collect();
    json!({"project": project, "returned": states.len(), "states": states}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListStatesArgs {
    /// Project identifier (e.g. "LTS") or project name. Required.
    pub project: String,
}

pub struct ListStates {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListStates {
    const NAME: &'static str = "list_states";
    type Args = ListStatesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List the workflow states of one project with their state group \
         (backlog, unstarted, started, completed, cancelled, triage) and which \
         state is the default. Use this to resolve a state name before \
         proposing a state change."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListStatesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project =
            resolve_project(&self.pool, self.workspace_id, self.user_id, &args.project).await?;
        let rows: Vec<StateRow> = sqlx::query_as(STATES_SQL)
            .bind(project.id)
            .bind(self.workspace_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(states_json(&project.identifier, &rows))
    }
}

pub const LABELS_SQL: &str = "SELECT l.name FROM labels l \
     WHERE l.project_id = $1 AND l.workspace_id = $2 AND l.deleted_at IS NULL \
     ORDER BY l.name";

pub fn labels_json(project: &str, names: &[String]) -> String {
    json!({"project": project, "returned": names.len(), "labels": names}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListLabelsArgs {
    /// Project identifier (e.g. "LTS") or project name. Required.
    pub project: String,
}

pub struct ListLabels {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListLabels {
    const NAME: &'static str = "list_labels";
    type Args = ListLabelsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List the label names available in one project. Use this to resolve a \
         label before proposing to add it to a work item."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListLabelsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project =
            resolve_project(&self.pool, self.workspace_id, self.user_id, &args.project).await?;
        let names: Vec<String> = sqlx::query_scalar(LABELS_SQL)
            .bind(project.id)
            .bind(self.workspace_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(labels_json(&project.identifier, &names))
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai states_json labels_json`
Expected: kedua test lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/lookups.rs
git commit -m "feat(ai): add list_states and list_labels lookup tools"
```

---

### Task 6: Tool `list_work_item_types`

**Files:**

- Modify: `crates/ai/src/tools/lookups.rs`

- [ ] **Step 1: Tulis test yang gagal**

```rust
#[test]
fn types_json_shape() {
    let rows = vec![
        TypeRow {
            name: "Incident".to_string(),
            is_epic: false,
            requires_service: true,
        },
        TypeRow {
            name: "Change".to_string(),
            is_epic: false,
            requires_service: false,
        },
    ];
    let parsed: serde_json::Value =
        serde_json::from_str(&types_json("project LTS", &rows)).unwrap();
    assert_eq!(parsed["scope"], json!("project LTS"));
    assert_eq!(parsed["types"][0]["name"], json!("Incident"));
    assert_eq!(parsed["types"][0]["requires_service"], json!(true));
}
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai types_json_shape`
Expected: gagal kompilasi — `TypeRow`/`types_json` belum ada.

- [ ] **Step 3: Implementasi di `tools/lookups.rs`**

```rust
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TypeRow {
    pub name: String,
    pub is_epic: bool,
    pub requires_service: bool,
}

pub const WORKSPACE_TYPES_SQL: &str = "SELECT it.name, it.is_epic, it.requires_service \
     FROM issue_types it \
     WHERE it.workspace_id = $1 AND it.deleted_at IS NULL AND it.is_active = true \
     ORDER BY it.level, it.name";

pub const PROJECT_TYPES_SQL: &str = "SELECT it.name, it.is_epic, it.requires_service \
     FROM issue_types it \
     JOIN project_issue_types pit ON pit.issue_type_id = it.id \
       AND pit.project_id = $1 AND pit.deleted_at IS NULL \
     WHERE it.workspace_id = $2 AND it.deleted_at IS NULL AND it.is_active = true \
     ORDER BY it.level, it.name";

pub fn types_json(scope: &str, rows: &[TypeRow]) -> String {
    let types: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "is_epic": row.is_epic,
                "requires_service": row.requires_service,
            })
        })
        .collect();
    json!({"scope": scope, "returned": types.len(), "types": types}).to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListWorkItemTypesArgs {
    /// Project identifier or name; omitted lists every workspace-level type.
    pub project: Option<String>,
}

pub struct ListWorkItemTypes {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListWorkItemTypes {
    const NAME: &'static str = "list_work_item_types";
    type Args = ListWorkItemTypesArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List work item types (e.g. Incident, Problem, Change, Improvement) with \
         is_epic and requires_service flags: the types enabled for one project, \
         or every workspace-level type when no project is given. An Incident \
         that requires a service cannot be accepted into a project until a \
         service is linked."
            .to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListWorkItemTypesArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        match optional_text(args.project.as_deref()) {
            Some(reference) => {
                let project =
                    resolve_project(&self.pool, self.workspace_id, self.user_id, &reference)
                        .await?;
                let rows: Vec<TypeRow> = sqlx::query_as(PROJECT_TYPES_SQL)
                    .bind(project.id)
                    .bind(self.workspace_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)?;
                Ok(types_json(&format!("project {}", project.identifier), &rows))
            }
            None => {
                let rows: Vec<TypeRow> = sqlx::query_as(WORKSPACE_TYPES_SQL)
                    .bind(self.workspace_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)?;
                Ok(types_json("workspace", &rows))
            }
        }
    }
}
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run: `cargo test -p ai types_json_shape`
Expected: lulus.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/lookups.rs
git commit -m "feat(ai): add list_work_item_types lookup tool"
```

---

### Task 7: Sinkronisasi allowlist schedule (backend)

**Files:**

- Modify: `crates/ai/src/schedule.rs:18`
- Modify: `crates/ai/src/tools/proposals.rs` (deskripsi `create_schedule`)
- Modify: `crates/worker/src/handlers/ai_schedule.rs` (test legacy)

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan di `mod tests` `schedule.rs`:

```rust
#[test]
fn spec_accepts_every_spec_tool() {
    let all: Vec<String> = SPEC_TOOLS.iter().map(|tool| tool.to_string()).collect();
    let spec = ScheduleSpec::new("d", &["s".to_string()], &all, "o").unwrap();
    assert_eq!(spec.tools.len(), SPEC_TOOLS.len());
    assert_eq!(SPEC_TOOLS.len(), 10);
}
```

dan ganti test worker `allowed_tools_legacy_is_all_read_tools` di
`crates/worker/src/handlers/ai_schedule.rs` menjadi:

```rust
    #[test]
    fn allowed_tools_legacy_is_all_read_tools() {
        assert_eq!(
            allowed_tools(None).unwrap(),
            vec![
                "list_projects".to_string(),
                "count_work_items".to_string(),
                "search_work_items".to_string(),
                "get_work_item".to_string(),
                "list_work_item_comments".to_string(),
                "list_work_item_relations".to_string(),
                "list_members".to_string(),
                "list_states".to_string(),
                "list_labels".to_string(),
                "list_work_item_types".to_string(),
            ]
        );
    }
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run: `cargo test -p ai spec_accepts_every_spec_tool`
Expected: FAIL — `SPEC_TOOLS.len()` masih 3.

- [ ] **Step 3: Perluas `SPEC_TOOLS`**

Ganti baris 18 `crates/ai/src/schedule.rs` menjadi:

```rust
pub const SPEC_TOOLS: [&str; 10] = [
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
];
```

- [ ] **Step 4: Perbarui deskripsi `create_schedule`**

Di `tools/proposals.rs`, ganti doc `CreateScheduleArgs.tools`:

```rust
    /// Tools the run may use: at least one read tool name from the schema's list.
    pub tools: Vec<String>,
```

dan ganti kalimat deskripsi tool `CreateSchedule` dari
`"the tools it needs (at least one of list_projects, count_work_items, search_work_items)"`
menjadi:

```rust
        "Propose a recurring scheduled task for this workspace. Only call this \
         when the user explicitly asks for a recurring or scheduled task (for \
         example a message starting with /schedule), and only after the recipe \
         is complete: description, ordered how_to steps, the read tools it \
         needs (at least one), expected_output, and the frequency. The user \
         must confirm and may edit every field in the UI before anything is \
         saved. Never claim the schedule exists until they confirm."
            .to_string()
```

- [ ] **Step 5: Jalankan test, pastikan lulus**

Run: `cargo test -p ai spec_accepts`
Expected: lulus.

Run: `cargo test -p worker allowed_tools`
Expected: lulus (membutuhkan `DATABASE_URL`? Test ini murni, tidak menyentuh DB).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/schedule.rs apps/api-rs/crates/ai/src/tools/proposals.rs apps/api-rs/crates/worker/src/handlers/ai_schedule.rs
git commit -m "feat(ai): expand schedule tool allowlist"
```

---

### Task 8: Sinkronisasi FE + `PREAMBLE`

**Files:**

- Modify: `apps/web/core/lib/ai-schedule.ts:12`
- Modify: `apps/web/core/lib/ai-schedule.test.ts`
- Modify: `crates/ai/src/agent.rs:45-66`

- [ ] **Step 1: Tulis test FE yang gagal**

Tambahkan di `apps/web/core/lib/ai-schedule.test.ts`:

```ts
import { AI_SCHEDULE_TOOLS } from "./ai-schedule";
// (tambahkan AI_SCHEDULE_TOOLS ke import yang sudah ada di baris atas)

describe("AI_SCHEDULE_TOOLS", () => {
  it("lists every read tool in backend canonical order", () => {
    expect([...AI_SCHEDULE_TOOLS]).toEqual([
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
    ]);
  });
});
```

- [ ] **Step 2: Jalankan test, pastikan gagal**

Run (dari `apps/web`): `pnpm test core/lib/ai-schedule.test.ts`
Expected: FAIL — list masih 3 tool.

- [ ] **Step 3: Perluas `AI_SCHEDULE_TOOLS`**

Ganti baris 12 `apps/web/core/lib/ai-schedule.ts`:

```ts
export const AI_SCHEDULE_TOOLS = [
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
] as const;
```

- [ ] **Step 4: Jalankan test, pastikan lulus**

Run (dari `apps/web`): `pnpm test core/lib/ai-schedule.test.ts`
Expected: PASS.

- [ ] **Step 5: Ganti `PREAMBLE`**

Ganti konstanta `PREAMBLE` di `crates/ai/src/agent.rs` (baris 45-66) menjadi:

```rust
pub const PREAMBLE: &str = "You are the workspace AI assistant for Plane. \
Answer factual questions about projects, work items, people, and their \
metadata by calling the provided tools; never invent identifiers, names, \
counts, or states. All tools are scoped to the user's current workspace and \
read-only, except create_schedule and create_work_item, which only propose \
something and never save anything. If a tool returns no results, say so. \
Answer concisely in the user's language. Use get_work_item for one work \
item's details, list_work_item_comments for its discussion, and \
list_work_item_relations for blockers. search_work_items and \
count_work_items can filter by service, type, assignee (a name, email, or \
\"me\"), sprint, track, and label; use assignee \"me\" for the user's own \
items. Use list_members, list_states, list_labels, and list_work_item_types \
to resolve names before answering or proposing changes. When the user's \
message starts with /schedule they want a recurring scheduled task. A \
schedule is a recipe, not a one-line command: gather anything unclear first, \
then call create_schedule once with a complete recipe — description, ordered \
how_to steps, at least one read tool, expected_output, and how often. Tell \
the user they can edit every field in the confirmation card. The schedule is \
only created after the user confirms the proposal card, so never say it is \
already created. When the user clearly asks to create a work item or task \
(natural language or a message starting with /task), propose exactly one work \
item per create_work_item call. The project must be named by the user: ask \
when it is missing or ambiguous, and never guess. State names, assignee names \
or emails, and label names may be human-readable; the UI resolves them. The \
context may name the signed-in user as Current user; when the user refers to \
themselves (\"me\", \"saya\"), that is the person to use as the assignee. A \
work item is only created after the user confirms the proposal card, so never \
say it is already created.";
```

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/lib/ai-schedule.ts apps/web/core/lib/ai-schedule.test.ts apps/api-rs/crates/ai/src/agent.rs
git commit -m "feat(ai): sync schedule tool list and refresh agent preamble"
```

---

### Task 9: Registrasi lookup tools + verifikasi akhir

**Files:**

- Modify: `crates/ai/src/tools/mod.rs`

- [ ] **Step 1: Daftarkan 4 tool baru di kedua registry**

Tambahkan ke rantai `.tool(...)` di `workspace_tools` (setelah `ListWorkItemRelations`, sebelum `CreateSchedule`):

```rust
        .tool(ListMembers {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListStates {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListLabels {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
        .tool(ListWorkItemTypes {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        })
```

Tambahkan blok kondisional yang sama di `read_tools` sebelum `server.run()`:

```rust
    if allowed.contains(&ListMembers::NAME) {
        server = server.tool(ListMembers {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListStates::NAME) {
        server = server.tool(ListStates {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListLabels::NAME) {
        server = server.tool(ListLabels {
            pool: pool.clone(),
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
    if allowed.contains(&ListWorkItemTypes::NAME) {
        server = server.tool(ListWorkItemTypes {
            pool,
            workspace_id,
            user_id,
            trace: trace.clone(),
        });
    }
```

- [ ] **Step 2: Tambah test metadata**

Tambahkan di `mod tests` `tools/mod.rs`:

```rust
#[tokio::test]
async fn lookup_tools_expose_metadata() {
    let pool = lazy_pool();
    let trace = crate::agent::new_trace();
    let members = ListMembers {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListMembers::NAME, "list_members");
    assert!(members.parameters()["properties"]["project"].is_object());
    let states = ListStates {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListStates::NAME, "list_states");
    assert!(states.parameters()["properties"]["project"].is_object());
    let labels = ListLabels {
        pool: pool.clone(),
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace: trace.clone(),
    };
    assert_eq!(ListLabels::NAME, "list_labels");
    let types = ListWorkItemTypes {
        pool,
        workspace_id: Uuid::nil(),
        user_id: Uuid::nil(),
        trace,
    };
    assert_eq!(ListWorkItemTypes::NAME, "list_work_item_types");
    assert!(!types.description().is_empty());
}
```

- [ ] **Step 3: Verifikasi seluruhnya**

Run: `cargo test -p ai`
Expected: seluruh test lulus.

Run: `cargo build -p api -p worker`
Expected: sukses tanpa warning.

Run (dari `apps/web`): `pnpm test core/lib/ai-schedule.test.ts`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/mod.rs
git commit -m "feat(ai): register lookup tools in both tool servers"
```

---

### Task 10: Smoke manual (opsional)

- [ ] **Step 1: Rebuild backend** (dari root repo; sesuai `AGENTS.md`)

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200` setelah build selesai.

- [ ] **Step 2: Smoke chat (mode Agent)**

1. `tiket saya yang urgent` — filter `assignee: "me"` + priority bekerja.
2. `berapa tiket Incident di project LTS` — filter type.
3. `tiket di sprint <nama>` / `tiket di track <nama>` — filter sprint/track.
4. `siapa saja member project LTS` — `list_members`.
5. `state apa saja di LTS` — `list_states`.
6. Buka editor schedule di panel Galileo → 10 tool muncul dan bisa dicentang; buat schedule memakai `get_work_item` lalu jalankan manual.

---

## Catatan Plan berikutnya

- Plan 1C (Batch 1.2): `list_services`, `get_service`, `list_intake_items`, `get_intake_item`, `count_intake_items` + helper feature gate project.
- Plan 1D (Batch 1.3): sprints, tracks, KB.
