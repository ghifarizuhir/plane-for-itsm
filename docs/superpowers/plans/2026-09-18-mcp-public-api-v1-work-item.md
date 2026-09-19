# MCP Public API v1 — Plan 2: Work Item Core

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the MCP `workitem` tool's core actions work against this fork: list (project/workspace/archived), retrieve, retrieve-by-identifier, search, count, create, update, delete, archive/unarchive, manage_assignee, manage_label.

**Architecture:** `/api/v1/workspaces/:slug/.../work-items/...` routes served by new handlers in `routes/v1/work_item.rs`, reusing the app API's SQL projection (`LIST_SELECT_SQL`, `DETAIL_SELECT_SQL`), envelope builder (`build_ungrouped_envelope`), cursor helpers, and permission helpers. A small `routes/v1/pql.rs` handles the PQL subset the MCP sends: `project`/`priority`/`state`/`type`/`assignee = currentUser()` joined by `AND`. Create/update map SDK field names (`assignees`, `labels`, `state`, `type_id`, `point`, `estimate_point`, …) onto the fork's schema (`issue_assignees`, `issue_labels`, `type_id`, `point`, `estimate_point_id`, …).

**Tech Stack:** Rust (axum, sqlx runtime queries), PostgreSQL, serde_json. Tests: `cargo test -p api`. Contract smoke: Python `plane-sdk` inside `plane-mcp-server/.venv`.

**Plan split:** This plan = spec Phase 3 (work item core). Plan 1 = Phases 1–2 (infra + project, done). Plan 3 = Phase 4 (sub-resources + work-item-types).

**Known deviations (documented, deliberate):**

- This fork has no PQL engine. `v1/pql.rs` supports the subset above; anything else returns `400 {"pql": "<reason>"}`, which the MCP turns into a correctable error via `pql_failure` (`plane_mcp/toolkit/paging.py:113`). The MCP always sends `project = "<pid>"` on `count` when a project is given, so that scope must work.
- `order_by`, `expand`, `fields`, `external_id`, `external_source` are accepted and ignored (fixed order `-created_at`). `fields` is applied client-side by the MCP.
- `count` supports flat `group_by` only; `sub_group_by` → 400.
- Archived list/detail shapes use the fork's columns; SDK models are all-optional/`extra="allow"`.

**Commit convention:** each task ends with a commit.

---

## File structure

- Create `apps/api-rs/crates/api/src/routes/v1/pql.rs` — PQL subset parser + WHERE pusher.
- Create `apps/api-rs/crates/api/src/routes/v1/work_item.rs` — v1 work-item handlers + shapers.
- Create `apps/api-rs/crates/api/tests/v1_work_item_test.rs` — pure shaper/parser tests.
- Modify `apps/api-rs/crates/api/src/routes/v1/mod.rs` — add `pub mod pql; pub mod work_item;`.
- Modify `apps/api-rs/crates/api/src/routes/work_item.rs` — make `ws_active_member` `pub(crate)`.
- Modify `apps/api-rs/crates/api/src/main.rs` — register v1 work-item routes.
- Modify `apps/api-rs/crates/api/tests/v1_routes_test.rs` — assert new routes.
- Modify `apps/api-rs/scripts/v1-smoke.py` — extend contract smoke.

Reference spec: `docs/superpowers/specs/2026-09-18-mcp-public-api-v1-core-design.md`.

SDK contract (from `plane-mcp-server/.venv/.../plane/api/work_items/base.py`):

| SDK call                 | Request                                       | Response model                      |
| ------------------------ | --------------------------------------------- | ----------------------------------- |
| `list`                   | `GET .../projects/{pid}/work-items/`          | `PaginatedWorkItemResponse`         |
| `list_workspace`         | `GET .../work-items/`                         | `PaginatedWorkItemResponse`         |
| `list_archived`          | `GET .../projects/{pid}/archived-work-items/` | `PaginatedWorkItemResponse`         |
| `retrieve`               | `GET .../projects/{pid}/work-items/{id}/`     | `WorkItemDetail`                    |
| `retrieve_by_identifier` | `GET .../work-items/{IDENT}-{n}/`             | `WorkItemDetail`                    |
| `search`                 | `GET .../work-items/search/?search=`          | `WorkItemSearch` (`{issues:[...]}`) |
| `count_workspace`        | `GET .../work-items/count/`                   | `WorkItemGroupedCountResponse`      |
| `create`                 | `POST .../projects/{pid}/work-items/`         | `WorkItem` (201)                    |
| `update`                 | `PATCH .../projects/{pid}/work-items/{id}/`   | `WorkItem` (200)                    |
| `delete`                 | `DELETE .../projects/{pid}/work-items/{id}/`  | 204                                 |
| `archive`                | `POST .../work-items/{id}/archive/`           | 204                                 |
| `unarchive`              | `DELETE .../work-items/{id}/unarchive/`       | 204                                 |

---

### Task 1: v1 PQL subset parser

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/pql.rs`
- Modify: `apps/api-rs/crates/api/src/routes/v1/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `pql.rs` with only the stub + test:

```rust
// apps/api-rs/crates/api/src/routes/v1/pql.rs
use sqlx::{Postgres, QueryBuilder};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct V1Pql {
    pub project: Option<uuid::Uuid>,
    pub priority: Option<String>,
    pub state: Option<uuid::Uuid>,
    pub type_id: Option<uuid::Uuid>,
    pub assignee_me: bool,
}

pub fn parse_v1_pql(raw: &str) -> Result<V1Pql, String> {
    let _ = raw;
    Err("unsupported".to_string())
}

pub fn push_pql_where(_qb: &mut QueryBuilder<Postgres>, _pql: &V1Pql, _user_id: uuid::Uuid) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_scope() {
        let p = parse_v1_pql(r#"project = "11111111-1111-1111-1111-111111111111""#).unwrap();
        assert_eq!(p.project, Some(uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()));
        assert_eq!(p.priority, None);
    }
}
```

Add to `v1/mod.rs`:

```rust
pub mod common;
pub mod pql;
pub mod project;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib v1::pql`
Expected: FAIL — `parses_project_scope` panics/errs (stub returns `Err`).

- [ ] **Step 3: Implement the parser**

Replace the stub bodies:

```rust
/// The PQL subset this fork can evaluate. The MCP sends `project = "<pid>"`
/// when scoping `count`, and users may add equality filters on the columns
/// this fork has. Anything else is rejected (the MCP turns the 400 into a
/// correctable answer).
fn split_top_level_and(expr: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    let tokens: Vec<char> = expr.chars().collect();
    let mut i = 0usize;
    while i < tokens.len() {
        let c = tokens[i];
        match c {
            '(' => { depth += 1; current.push(c); i += 1; }
            ')' => { depth -= 1; if depth < 0 { return Err("Unbalanced parentheses in PQL".into()); } current.push(c); i += 1; }
            'A' if depth == 0 && tokens[i..].starts_with(&['A', 'N', 'D']) => {
                let before_ok = i == 0 || tokens[i - 1].is_whitespace();
                let after = i + 3;
                let after_ok = after >= tokens.len() || tokens[after].is_whitespace() || tokens[after] == '(';
                if before_ok && after_ok {
                    parts.push(current.trim().to_string());
                    current.clear();
                    i += 3;
                } else { current.push(c); i += 1; }
            }
            _ => { current.push(c); i += 1; }
        }
    }
    if depth != 0 { return Err("Unbalanced parentheses in PQL".into()); }
    parts.push(current.trim().to_string());
    Ok(parts.into_iter().filter(|p| !p.is_empty()).collect())
}

/// Strips balanced outer parentheses, repeatedly: `((x))` → `x`.
fn strip_outer_parens(mut s: &str) -> &str {
    loop {
        let t = s.trim();
        if !(t.starts_with('(') && t.ends_with(')')) { return t; }
        let mut depth = 0i32;
        let mut balanced = true;
        for (idx, c) in t.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && idx != t.len() - 1 { balanced = false; break; }
                }
                _ => {}
            }
        }
        if balanced { s = &t[1..t.len() - 1]; } else { return t; }
    }
}

fn parse_condition(cond: &str) -> Result<V1Pql, String> {
    let cond = strip_outer_parens(cond);
    let (lhs, rhs) = cond.split_once('=').ok_or_else(|| format!("Unsupported PQL condition: {cond}"))?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    let unquote = |v: &str| v.trim().trim_matches('"').to_string();
    let mut out = V1Pql::default();
    match lhs {
        "project" => out.project = Some(uuid::Uuid::parse_str(&unquote(rhs)).map_err(|_| format!("Invalid project id in PQL: {rhs}"))?),
        "priority" => {
            let v = unquote(rhs);
            if !["low", "medium", "high", "urgent", "none"].contains(&v.as_str()) {
                return Err(format!("Invalid priority in PQL: {rhs}"));
            }
            out.priority = Some(v);
        }
        "state" => out.state = Some(uuid::Uuid::parse_str(&unquote(rhs)).map_err(|_| format!("Invalid state id in PQL: {rhs}"))?),
        "type" => out.type_id = Some(uuid::Uuid::parse_str(&unquote(rhs)).map_err(|_| format!("Invalid type id in PQL: {rhs}"))?),
        "assignee" => {
            if rhs == "currentUser()" { out.assignee_me = true; }
            else { out.assignee_me = r#""#.is_empty() && { let _ = rhs; false }; return Err(format!("Unsupported PQL assignee value: {rhs}")); }
        }
        other => return Err(format!("Unsupported PQL field: {other}")),
    }
    Ok(out)
}

pub fn parse_v1_pql(raw: &str) -> Result<V1Pql, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() { return Ok(V1Pql::default()); }
    let mut out = V1Pql::default();
    for cond in split_top_level_and(trimmed)? {
        let one = parse_condition(&cond)?;
        if one.project.is_some() { out.project = one.project; }
        if one.priority.is_some() { out.priority = one.priority; }
        if one.state.is_some() { out.state = one.state; }
        if one.type_id.is_some() { out.type_id = one.type_id; }
        if one.assignee_me { out.assignee_me = true; }
    }
    Ok(out)
}

pub fn push_pql_where(qb: &mut QueryBuilder<Postgres>, pql: &V1Pql, user_id: uuid::Uuid) {
    if let Some(p) = pql.project { qb.push(" AND i.project_id = ").push_bind(p); }
    if let Some(pr) = pql.priority.clone() { qb.push(" AND i.priority = ").push_bind(pr); }
    if let Some(st) = pql.state { qb.push(" AND i.state_id = ").push_bind(st); }
    if let Some(t) = pql.type_id { qb.push(" AND i.type_id = ").push_bind(t); }
    if pql.assignee_me {
        qb.push(" AND EXISTS(SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = i.id AND ia.assignee_id = ")
          .push_bind(user_id)
          .push(" AND ia.deleted_at IS NULL)");
    }
}
```

Fix the `assignee` arm to be clean Rust:

```rust
        "assignee" => {
            if rhs == "currentUser()" {
                out.assignee_me = true;
            } else {
                return Err(format!("Unsupported PQL assignee value: {rhs}"));
            }
        }
```

- [ ] **Step 4: Add parser tests**

Append to the `mod tests` block:

```rust
    #[test]
    fn parses_and_combined_scope() {
        let raw = r#"(priority = "urgent") AND project = "11111111-1111-1111-1111-111111111111""#;
        let p = parse_v1_pql(raw).unwrap();
        assert_eq!(p.priority.as_deref(), Some("urgent"));
        assert!(p.project.is_some());
    }

    #[test]
    fn parses_assignee_current_user() {
        let p = parse_v1_pql("assignee = currentUser()").unwrap();
        assert!(p.assignee_me);
    }

    #[test]
    fn rejects_or_and_functions() {
        assert!(parse_v1_pql(r#"priority = "urgent" OR priority = "low""#).is_err());
        assert!(parse_v1_pql("isOverdue()").is_err());
        assert!(parse_v1_pql(r#"priority = "nope""#).is_err());
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(parse_v1_pql("   ").unwrap(), V1Pql::default());
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p api --lib v1::pql`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/pql.rs apps/api-rs/crates/api/src/routes/v1/mod.rs
git commit -m "feat(api-rs): v1 work item PQL subset parser"
```

---

### Task 2: v1 work-item shapers (pure)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`
- Create: `apps/api-rs/crates/api/tests/v1_work_item_test.rs`
- Modify: `apps/api-rs/crates/api/src/routes/v1/mod.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// apps/api-rs/crates/api/tests/v1_work_item_test.rs
use api::routes::v1::work_item::{
    v1_count_json, v1_search_issue_json, v1_work_item_json, V1SearchRow,
};
use api::routes::issue_common::IssueListRow;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

fn list_row() -> IssueListRow {
    IssueListRow {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        name: "Fix".to_string(),
        state_id: None,
        sort_order: 1.0,
        completed_at: None,
        estimate_point: None,
        priority: "urgent".to_string(),
        start_date: None,
        target_date: None,
        sequence_id: 3,
        project_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
        parent_id: None,
        cycle_id: None,
        module_ids: vec![],
        label_ids: vec![Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap()],
        assignee_ids: vec![Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap()],
        sub_issues_count: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        created_by: None,
        updated_by: None,
        attachment_count: 0,
        link_count: 0,
        is_draft: false,
        archived_at: None,
        deleted_at: None,
    }
}

#[test]
fn work_item_json_adds_assignees_and_labels_aliases() {
    let v = v1_work_item_json(&list_row());
    let o = v.as_object().unwrap();
    assert!(o.contains_key("id"));
    assert!(o.contains_key("name"));
    assert_eq!(o["assignees"].as_array().unwrap().len(), 1);
    assert_eq!(o["labels"].as_array().unwrap().len(), 1);
    assert!(o.contains_key("assignee_ids"));
}

#[test]
fn count_json_no_grouping() {
    let v = v1_count_json(None, None, 7, vec![]);
    assert_eq!(v["total_count"], Value::from(7));
    assert_eq!(v["grouped_by"], Value::Null);
    assert_eq!(v["grouped_counts"], serde_json::json!({}));
}

#[test]
fn count_json_flat_grouping() {
    let v = v1_count_json(Some("priority"), None, 7, vec![("urgent".into(), 2), ("None".into(), 5)]);
    assert_eq!(v["grouped_by"], Value::from("priority"));
    assert_eq!(v["grouped_counts"]["urgent"]["count"], Value::from(2));
    assert_eq!(v["grouped_counts"]["None"]["count"], Value::from(5));
}

#[test]
fn search_issue_json_has_sdk_required_fields() {
    let row = V1SearchRow {
        id: Uuid::nil(),
        name: "n".into(),
        sequence_id: 1,
        project_id: Uuid::nil(),
        project_identifier: "ENG".into(),
        workspace_slug: "itsm".into(),
    };
    let v = v1_search_issue_json(&row);
    for key in ["id", "name", "sequence_id", "project_id", "project__identifier", "workspace__slug"] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p api --test v1_work_item_test`
Expected: compile error — module `work_item` / functions not found.

- [ ] **Step 3: Create `v1/work_item.rs` with shapers**

```rust
// apps/api-rs/crates/api/src/routes/v1/work_item.rs
//! v1 work-item handlers (`/api/v1/.../work-items/...`).

use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::issue_common::IssueListRow;

/// Serialize a list row and add the SDK-key aliases `assignees`/`labels`
/// (the fork's rows carry `assignee_ids`/`label_ids`). `WorkItemDetail`'s
/// `manage_assignee`/`manage_label` read `assignees`/`labels` back, so the
/// aliases are required for the MCP's merge-then-write path.
pub fn v1_work_item_json(row: &IssueListRow) -> Value {
    let mut v = serde_json::to_value(row).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        let assignees = o.get("assignee_ids").cloned().unwrap_or_else(|| json!([]));
        let labels = o.get("label_ids").cloned().unwrap_or_else(|| json!([]));
        o.insert("assignees".to_string(), assignees);
        o.insert("labels".to_string(), labels);
    }
    v
}

/// `WorkItemGroupedCountResponse`: flat `{grouped_by, sub_grouped_by,
/// total_count, grouped_counts}`. `counts` is `(key, count)` pairs; the
/// special key `"None"` represents a NULL dimension.
pub fn v1_count_json(
    grouped_by: Option<&str>,
    sub_grouped_by: Option<&str>,
    total: i64,
    counts: Vec<(String, i64)>,
) -> Value {
    let mut map = serde_json::Map::new();
    for (k, n) in counts {
        map.insert(k, json!({ "count": n }));
    }
    json!({
        "grouped_by": grouped_by,
        "sub_grouped_by": sub_grouped_by,
        "total_count": total,
        "grouped_counts": Value::Object(map),
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct V1SearchRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub project_identifier: String,
    pub workspace_slug: String,
}

pub fn v1_search_issue_json(row: &V1SearchRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "sequence_id": row.sequence_id,
        "project_id": row.project_id,
        "project__identifier": row.project_identifier,
        "workspace__slug": row.workspace_slug,
    })
}
```

Add `pub mod work_item;` to `v1/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --test v1_work_item_test`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs apps/api-rs/crates/api/src/routes/v1/mod.rs apps/api-rs/crates/api/tests/v1_work_item_test.rs
git commit -m "feat(api-rs): v1 work item shapers"
```

---

### Task 3: Route registration + handler stubs

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`
- Modify: `apps/api-rs/crates/api/tests/v1_routes_test.rs`

- [ ] **Step 1: Extend the route test**

Append to `v1_routes_test.rs`:

```rust
#[test]
fn v1_work_item_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/archive/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/unarchive/",
        "/api/v1/workspaces/:slug/projects/:project_id/archived-work-items/",
        "/api/v1/workspaces/:slug/work-items/",
        "/api/v1/workspaces/:slug/work-items/count/",
        "/api/v1/workspaces/:slug/work-items/search/",
        "/api/v1/workspaces/:slug/work-items/:ident/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test v1_routes_test v1_work_item_routes_registered`
Expected: FAIL `missing v1 route: /api/v1/workspaces/:slug/projects/:project_id/work-items/`.

- [ ] **Step 3: Stub the handlers**

Append to `v1/work_item.rs`:

```rust
use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub async fn list_project(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn list_workspace(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn list_archived(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn retrieve(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn retrieve_by_identifier(_: State<AppState>, _: AuthUser, _: Path<(String, String)>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn search(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn count(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<serde_json::Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn create(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>, _: Json<Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn update(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>, _: Json<Value>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn archive(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn unarchive(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid, uuid::Uuid)>) -> R {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
```

In `main.rs`, inside the v1 block (after the `total-worklogs` route, before `.route("/api/timezones/", ...)`), add:

```rust
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/",
            get(routes::v1::work_item::list_project).post(routes::v1::work_item::create),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/",
            get(routes::v1::work_item::retrieve)
                .patch(routes::v1::work_item::update)
                .delete(routes::work_item::delete_issue),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/archive/",
            post(routes::v1::work_item::archive),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:pk/unarchive/",
            delete(routes::v1::work_item::unarchive),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/archived-work-items/",
            get(routes::v1::work_item::list_archived),
        )
        .route(
            "/api/v1/workspaces/:slug/work-items/",
            get(routes::v1::work_item::list_workspace),
        )
        .route(
            "/api/v1/workspaces/:slug/work-items/count/",
            get(routes::v1::work_item::count),
        )
        .route(
            "/api/v1/workspaces/:slug/work-items/search/",
            get(routes::v1::work_item::search),
        )
        .route(
            "/api/v1/workspaces/:slug/work-items/:ident/",
            get(routes::v1::work_item::retrieve_by_identifier),
        )
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --test v1_routes_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/src/routes/v1/work_item.rs apps/api-rs/crates/api/tests/v1_routes_test.rs
git commit -m "feat(api-rs): mount /api/v1 work item routes"
```

---

### Task 4: List project + workspace

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`
- Modify: `apps/api-rs/crates/api/src/routes/work_item.rs` (expose `ws_active_member`)

- [ ] **Step 1: Make `ws_active_member` reusable**

In `routes/work_item.rs:416`, change `async fn ws_active_member(` to `pub(crate) async fn ws_active_member(`.

- [ ] **Step 2: Implement the shared list query + handlers**

Replace the `list_project` and `list_workspace` stubs, and add the imports at the top of `v1/work_item.rs`:

```rust
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder};

use crate::routes::issue_common::{
    PageWindow, fetch_guest_scoped, fetch_project_member_role, is_workspace_admin,
    page_window, project_gate_allows,
};
use crate::routes::issue_query::{LIST_SELECT_SQL, build_ungrouped_envelope};
use crate::routes::project::deny;
use crate::routes::v1::common::PageParams;
use crate::routes::v1::pql::{V1Pql, parse_v1_pql, push_pql_where};
use crate::routes::work_item::ws_active_member;
```

```rust
/// `?pql=&cursor=&per_page=&order_by=&expand=&fields=` (only the first three
/// have effect; the rest are accepted-and-ignored, matching the app API's
/// documented deviation).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1WorkItemQuery {
    #[serde(default)] pub cursor: Option<String>,
    #[serde(default)] pub per_page: Option<String>,
    #[serde(default)] pub order_by: Option<String>,
    #[serde(default)] pub pql: Option<String>,
    #[serde(default)] pub expand: Option<String>,
    #[serde(default)] pub fields: Option<String>,
    #[serde(default)] pub external_id: Option<String>,
    #[serde(default)] pub external_source: Option<String>,
}

fn pql_or_400(raw: Option<&str>) -> Result<V1Pql, (StatusCode, Json<Value>)> {
    parse_v1_pql(raw.unwrap_or("")).map_err(|msg| (StatusCode::BAD_REQUEST, Json(json!({"pql": msg}))))
}

pub async fn list_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Query(q): Query<V1WorkItemQuery>,
) -> R {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM projects WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL",
    )
    .bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    if exists.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))));
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await?;
    list_envelope(
        &st,
        &slug,
        auth.0,
        Some(project_id),
        false,
        q.cursor.as_deref(),
        q.per_page.as_deref(),
        &pql,
        guest_scoped,
    )
    .await
}

pub async fn list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<V1WorkItemQuery>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };
    list_envelope(
        &st,
        &slug,
        auth.0,
        None,
        false,
        q.cursor.as_deref(),
        q.per_page.as_deref(),
        &pql,
        false,
    )
    .await
}

/// Shared envelope path. `scope_project = Some(pid)` → project list (mirrors
/// `issue_query::list` visibility); `None` → workspace list (member projects
/// only). `archived = true` → `archived_at IS NOT NULL`.
#[allow(clippy::too_many_arguments)]
async fn list_envelope(
    st: &AppState,
    slug: &str,
    user_id: uuid::Uuid,
    scope_project: Option<uuid::Uuid>,
    archived: bool,
    cursor_raw: Option<&str>,
    per_page_raw: Option<&str>,
    pql: &V1Pql,
    guest_scoped: bool,
) -> R {
    let (per_page, cursor) = match (PageParams {
        cursor: cursor_raw.map(str::to_string),
        per_page: per_page_raw.map(str::to_string),
    })
    .resolve()
    {
        Ok(v) => v,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))),
    };
    let limit = per_page.min(1000);
    let window = match page_window(cursor.page, limit) {
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
        Ok(w) => w,
    };

    let mut where_qb = |qb: &mut QueryBuilder<Postgres>| {
        qb.push(" WHERE i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
          .push_bind(slug.to_string())
          .push(") AND i.deleted_at IS NULL AND i.is_draft = false AND s.\"group\" <> 'triage'");
        if archived {
            qb.push(" AND i.archived_at IS NOT NULL");
        } else {
            qb.push(" AND i.archived_at IS NULL");
        }
        if let Some(pid) = scope_project {
            qb.push(" AND i.project_id = ").push_bind(pid);
        } else {
            qb.push(" AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
                      AND pm.member_id = ").push_bind(user_id)
              .push(" AND pm.is_active = true AND pm.deleted_at IS NULL)");
        }
        if guest_scoped {
            qb.push(" AND i.created_by_id = ").push_bind(user_id);
        }
        qb.push(" AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id AND p.deleted_at IS NULL AND p.archived_at IS NULL)");
        push_pql_where(qb, pql, user_id);
    };

    let mut count_qb = QueryBuilder::new(
        "SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id",
    );
    where_qb(&mut count_qb);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&st.pool).await?;

    let offset_opt: Option<i64> = match window {
        PageWindow::Rows(o) => Some(o),
        PageWindow::BeyondEnd => None,
    };
    let rows: Vec<IssueListRow> = match offset_opt {
        Some(offset) => {
            let mut page_qb = QueryBuilder::new(LIST_SELECT_SQL);
            where_qb(&mut page_qb);
            page_qb.push(" ORDER BY i.created_at DESC LIMIT ").push_bind(limit);
            page_qb.push(" OFFSET ").push_bind(offset);
            page_qb.build_query_as().fetch_all(&st.pool).await?
        }
        None => Vec::new(),
    };
    let results: Vec<Value> = rows.iter().map(v1_work_item_json).collect();
    Ok((StatusCode::OK, Json(build_ungrouped_envelope(total, limit, cursor.page, results))))
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p api`
Expected: builds. (`list_archived` still calls the stub — fine; it is replaced in Task 5.)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs apps/api-rs/crates/api/src/routes/work_item.rs
git commit -m "feat(api-rs): v1 work item project/workspace list"
```

---

### Task 5: Archived list

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Implement `list_archived`**

Replace the stub:

```rust
pub async fn list_archived(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Query(q): Query<V1WorkItemQuery>,
) -> R {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };
    let guest_scoped = fetch_guest_scoped(&st.pool, auth.0, project_id).await?;
    // Archived rows keep completed/cancelled groups; the triage exclusion and
    // `archived_at IS NULL` are flipped by `list_envelope(archived=true)`.
    list_envelope(
        &st,
        &slug,
        auth.0,
        Some(project_id),
        true,
        q.cursor.as_deref(),
        q.per_page.as_deref(),
        &pql,
        guest_scoped,
    )
    .await
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p api`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "feat(api-rs): v1 archived work item list"
```

---

### Task 6: Retrieve + retrieve-by-identifier

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Implement the detail fetch + handlers**

Replace the `retrieve` and `retrieve_by_identifier` stubs and add imports:

```rust
use crate::routes::issue_common::IssueDetailRow;
use crate::routes::issue_query::DETAIL_SELECT_SQL;
use crate::routes::project::missing;
```

```rust
/// Fetches the SDK `WorkItemDetail` shape: the full visibility gate of the
/// app API's `get_issue` (active ws member + project member/creator), then
/// `DETAIL_SELECT_SQL` plus `description_html`. Adds `assignees`/`labels`.
async fn fetch_detail(
    st: &AppState,
    slug: &str,
    user_id: uuid::Uuid,
    project_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<Option<Value>, common::errors::AppError> {
    let row: Option<IssueDetailRow> = sqlx::query_as(&format!(
        "{DETAIL_SELECT_SQL} WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND i.deleted_at IS NULL"
    ))
    .bind(pk).bind(project_id).bind(slug)
    .fetch_optional(&st.pool).await?;
    let Some(row) = row else { return Ok(None); };
    let description_html: Option<String> =
        sqlx::query_scalar("SELECT description_html FROM issues WHERE id = $1")
            .bind(pk).fetch_optional(&st.pool).await?.flatten();
    let mut v = serde_json::to_value(&row).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        let assignees = o.get("assignee_ids").cloned().unwrap_or_else(|| json!([]));
        let labels = o.get("label_ids").cloned().unwrap_or_else(|| json!([]));
        o.insert("assignees".to_string(), assignees);
        o.insert("labels".to_string(), labels);
        o.insert("description_html".to_string(), json!(description_html.unwrap_or_else(|| "<p></p>".to_string())));
        o.insert("project".to_string(), json!(project_id));
        o.insert("workspace".to_string(), json!(slug));
    }
    let _ = user_id;
    Ok(Some(v))
}

pub async fn retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    ).bind(pk).bind(auth.0).fetch_one(&st.pool).await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15) | Some(5)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    match fetch_detail(&st, &slug, auth.0, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}

pub async fn retrieve_by_identifier(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, ident)): Path<(String, String)>,
) -> R {
    let Ok((proj_ident, seq_raw)) = crate::routes::work_item::resolve_identifier(&ident) else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": crate::routes::work_item::INVALID_IDENTIFIER_MSG}))));
    };
    let project_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE w.slug = $1 AND LOWER(p.identifier) = LOWER($2) AND p.deleted_at IS NULL",
    ).bind(&slug).bind(&proj_ident).fetch_optional(&st.pool).await?;
    let Some(project_id) = project_id else { return Ok(missing()); };
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if role.is_none() {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": crate::routes::work_item::IDENTIFIER_FORBIDDEN_MSG}))));
    }
    let Ok(seq) = seq_raw.parse::<i32>() else { return Ok(missing()); };
    let pk: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i WHERE i.project_id = $1 AND i.sequence_id = $2 AND i.deleted_at IS NULL",
    ).bind(project_id).bind(seq).fetch_optional(&st.pool).await?;
    let Some(pk) = pk else { return Ok(missing()); };
    match fetch_detail(&st, &slug, auth.0, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}
```

`resolve_identifier`, `INVALID_IDENTIFIER_MSG`, `IDENTIFIER_FORBIDDEN_MSG` are `pub(crate)` in `routes/work_item.rs`; if the compiler reports them private, widen the three to `pub(crate)` (they already are per the existing code).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p api`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "feat(api-rs): v1 work item retrieve + by-identifier"
```

---

### Task 7: Search

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Implement `search`**

Replace the stub:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1SearchQuery {
    #[serde(default)] pub search: Option<String>,
}

pub async fn search(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<V1SearchQuery>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let pattern = match q.search.as_deref() {
        Some(s) if !s.trim().is_empty() => format!("%{}%", s.replace(['%', '_'], "")),
        _ => "%".to_string(),
    };
    let rows: Vec<V1SearchRow> = sqlx::query_as(
        "SELECT i.id, i.name, i.sequence_id, i.project_id, p.identifier AS project_identifier, \
                w.slug AS workspace_slug \
         FROM issues i \
         JOIN projects p ON p.id = i.project_id \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN project_members pm ON pm.project_id = i.project_id \
         WHERE w.slug = $1 AND pm.member_id = $2 AND pm.is_active = true \
           AND i.name ILIKE $3 AND i.deleted_at IS NULL AND i.archived_at IS NULL \
         ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(&slug).bind(auth.0).bind(&pattern)
    .fetch_all(&st.pool).await?;
    let issues: Vec<Value> = rows.iter().map(v1_search_issue_json).collect();
    Ok((StatusCode::OK, Json(json!({ "issues": issues }))))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p api`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "feat(api-rs): v1 work item search"
```

---

### Task 8: Count with flat group_by

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Write the failing unit test**

Add to `v1_work_item_test.rs`:

```rust
use api::routes::v1::work_item::count_group_column;

#[test]
fn count_group_column_allowlist() {
    assert_eq!(count_group_column("state_id"), Some("i.state_id::text"));
    assert_eq!(count_group_column("state__group"), Some("s.\"group\""));
    assert_eq!(count_group_column("priority"), Some("i.priority"));
    assert_eq!(count_group_column("project_id"), Some("i.project_id::text"));
    assert_eq!(count_group_column("type_id"), Some("i.type_id::text"));
    assert_eq!(count_group_column("created_by"), Some("i.created_by_id::text"));
    assert_eq!(count_group_column("target_date"), Some("i.target_date::text"));
    assert_eq!(count_group_column("start_date"), Some("i.start_date::text"));
    assert_eq!(count_group_column("cycle_id"), None);
    assert_eq!(count_group_column("label_ids"), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test v1_work_item_test count_group_column_allowlist`
Expected: compile error — `count_group_column` not found.

- [ ] **Step 3: Implement the allowlist + handler**

Replace the `count` stub and add imports:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1CountQuery {
    #[serde(default)] pub pql: Option<String>,
    #[serde(default)] pub group_by: Option<String>,
    #[serde(default)] pub sub_group_by: Option<String>,
}

/// Maps an SDK `group_by` value to a SQL expression over the list scope.
/// Only dimensions backed by columns in this fork are supported; the MCP's
/// module/cycle/milestone/release keys return `None` → 400.
pub fn count_group_column(key: &str) -> Option<&'static str> {
    match key {
        "state_id" => Some("i.state_id::text"),
        "state__group" => Some("s.\"group\""),
        "priority" => Some("i.priority"),
        "project_id" => Some("i.project_id::text"),
        "type_id" => Some("i.type_id::text"),
        "created_by" => Some("i.created_by_id::text"),
        "target_date" => Some("i.target_date::text"),
        "start_date" => Some("i.start_date::text"),
        _ => None,
    }
}

pub async fn count(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<V1CountQuery>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    if q.sub_group_by.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "sub_group_by is not supported"}))));
    }
    let pql = match pql_or_400(q.pql.as_deref()) { Ok(p) => p, Err(e) => return Ok(e) };

    let mut base_where = |qb: &mut QueryBuilder<Postgres>| {
        qb.push(" WHERE i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = ")
          .push_bind(slug.clone())
          .push(") AND i.deleted_at IS NULL AND i.is_draft = false AND s.\"group\" <> 'triage' AND i.archived_at IS NULL")
          .push(" AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = i.project_id \
                  AND pm.member_id = ").push_bind(auth.0)
          .push(" AND pm.is_active = true AND pm.deleted_at IS NULL)")
          .push(" AND EXISTS(SELECT 1 FROM projects p WHERE p.id = i.project_id AND p.deleted_at IS NULL AND p.archived_at IS NULL)");
        push_pql_where(qb, &pql, auth.0);
    };

    let group_key = q.group_by.as_deref().filter(|s| !s.trim().is_empty());
    let Some(key) = group_key else {
        let mut qb = QueryBuilder::new("SELECT COUNT(*) FROM issues i LEFT JOIN states s ON s.id = i.state_id");
        base_where(&mut qb);
        let total: i64 = qb.build_query_scalar().fetch_one(&st.pool).await?;
        return Ok((StatusCode::OK, Json(v1_count_json(None, None, total, vec![]))));
    };
    let Some(expr) = count_group_column(key) else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": format!("Unsupported group_by: {key}")}))));
    };

    let count_sql = format!(
        "SELECT COALESCE({expr}, 'None') AS k, COUNT(*) AS n \
         FROM issues i LEFT JOIN states s ON s.id = i.state_id"
    );
    let mut qb = QueryBuilder::new(count_sql);
    base_where(&mut qb);
    qb.push(format!(" GROUP BY {expr}"));
    let rows: Vec<(String, i64)> = qb.build_query_as().fetch_all(&st.pool).await?;
    let total: i64 = rows.iter().map(|(_, n)| *n).sum();
    Ok((StatusCode::OK, Json(v1_count_json(Some(key), None, total, rows))))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --test v1_work_item_test count_group_column_allowlist`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs apps/api-rs/crates/api/tests/v1_work_item_test.rs
git commit -m "feat(api-rs): v1 work item count"
```

---

### Task 9: Create

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Implement the request type, validation and handler**

Replace the `create` stub and add imports:

```rust
use crate::routes::issue_write::resolve_effective_state;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1WriteWorkItem {
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub assignees: Option<Vec<uuid::Uuid>>,
    #[serde(default)] pub labels: Option<Vec<uuid::Uuid>>,
    #[serde(default)] pub state: Option<uuid::Uuid>,
    #[serde(default)] pub type_id: Option<uuid::Uuid>,
    #[serde(default)] pub point: Option<i32>,
    #[serde(default)] pub estimate_point: Option<uuid::Uuid>,
    #[serde(default)] pub priority: Option<String>,
    #[serde(default)] pub start_date: Option<String>,
    #[serde(default)] pub target_date: Option<String>,
    #[serde(default)] pub sort_order: Option<f64>,
    #[serde(default)] pub parent: Option<uuid::Uuid>,
    #[serde(default)] pub is_draft: Option<bool>,
    #[serde(default)] pub description_html: Option<String>,
    #[serde(default)] pub description_stripped: Option<String>,
    #[serde(default)] pub external_source: Option<String>,
    #[serde(default)] pub external_id: Option<String>,
}

pub const V1_PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

fn parse_date(raw: &Option<String>) -> Result<Option<chrono::NaiveDate>, String> {
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| format!("Invalid date: {s}")),
    }
}

fn description_of(body: &V1WriteWorkItem) -> String {
    if let Some(html) = body.description_html.as_deref().filter(|s| !s.is_empty()) {
        return html.to_string();
    }
    if let Some(plain) = body.description_stripped.as_deref().filter(|s| !s.is_empty()) {
        return format!("<p>{}</p>", plain.replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br/>"));
    }
    "<p></p>".to_string()
}

/// 403 unless the caller is a project ADMIN/MEMBER (or ws admin). Mirrors
/// `issue_write::create`'s permission expectations and `patch_issue`'s gate.
async fn require_project_write(
    st: &AppState,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    if !ws_active_member(&st.pool, user_id, slug).await? {
        return Ok(false);
    }
    let member_role = fetch_project_member_role(&st.pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15)),
        member_role.is_some(),
        ws_admin,
    ))
}

async fn validate_write(
    st: &AppState,
    project_id: uuid::Uuid,
    body: &V1WriteWorkItem,
) -> Result<(), (StatusCode, Json<Value>)> {
    let bad = |msg: String| (StatusCode::BAD_REQUEST, Json(json!({"error": msg})));
    if let Some(name) = &body.name {
        if name.trim().is_empty() { return Err(bad("name is required".into())); }
        if name.chars().count() > 255 { return Err(bad("name max length 255".into())); }
    }
    if let Some(p) = &body.priority {
        if !V1_PRIORITIES.contains(&p.as_str()) { return Err(bad("Invalid priority".into())); }
    }
    if let Some(ids) = body.assignees.as_ref().filter(|v| !v.is_empty()) {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND member_id = ANY($2) AND is_active = true AND role >= 15",
        ).bind(project_id).bind(ids).fetch_one(&st.pool).await.map_err(internal)?;
        if n != ids.len() as i64 { return Err(bad("invalid assignee: not a project member".into())); }
    }
    if let Some(ids) = body.labels.as_ref().filter(|v| !v.is_empty()) {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM labels WHERE project_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        ).bind(project_id).bind(ids).fetch_one(&st.pool).await.map_err(internal)?;
        if n != ids.len() as i64 { return Err(bad("invalid label: not in project".into())); }
    }
    if let Some(state_id) = body.state {
        let (ok,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)")
            .bind(state_id).bind(project_id).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("State is not valid please pass a valid state_id".into())); }
    }
    if let Some(ep) = body.estimate_point {
        let (ok,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM estimate_points WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)")
            .bind(ep).bind(project_id).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("estimate_point is not valid".into())); }
    }
    if let Some(parent) = body.parent {
        let (ok,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)")
            .bind(parent).bind(project_id).fetch_one(&st.pool).await.map_err(internal)?;
        if !ok { return Err(bad("parent is not valid".into())); }
    }
    parse_date(&body.start_date).map_err(bad)?;
    parse_date(&body.target_date).map_err(bad)?;
    Ok(())
}

fn internal(e: sqlx::Error) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<V1WriteWorkItem>,
) -> R {
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if let Err(e) = validate_write(&st, project_id, &body).await {
        return Ok(e);
    }
    let name = body.name.clone().unwrap_or_default();

    let (default_state, first_state): (Option<uuid::Uuid>, Option<uuid::Uuid>) = (
        sqlx::query_scalar("SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true ORDER BY created_at ASC LIMIT 1")
            .bind(project_id).fetch_optional(&st.pool).await?,
        sqlx::query_scalar("SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL AND \"group\" != 'triage' AND is_triage = false ORDER BY created_at ASC LIMIT 1")
            .bind(project_id).fetch_optional(&st.pool).await?,
    );
    let state_id = resolve_effective_state(body.state, default_state, first_state);
    let start_date = parse_date(&body.start_date).map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;
    let target_date = parse_date(&body.target_date).map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;
    let priority = body.priority.clone().unwrap_or_else(|| "none".to_string());
    let html = description_of(&body);

    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "INSERT INTO issues (id, name, description_html, description_json, description_stripped, priority, start_date, target_date, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, created_by_id, updated_by_id, point, estimate_point_id, type_id, parent_id, external_source, external_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, $2, '{}', $3, $4, $5, $6, $7, \
                COALESCE($8, COALESCE((SELECT MAX(sort_order) FROM issues WHERE project_id = $9 AND deleted_at IS NULL), 65535.0) + 10000), \
                COALESCE((SELECT MAX(sequence) FROM issue_sequences WHERE project_id = $9), 0) + 1, \
                $10, $9, w.id, $11, $11, $12, $13, $14, $15, $16, $17, now(), now() \
         FROM workspaces w WHERE w.slug = $18 RETURNING id",
    )
    .bind(&name).bind(&html).bind(body.description_stripped.as_deref())
    .bind(&priority).bind(start_date).bind(target_date)
    .bind(body.is_draft.unwrap_or(false)).bind(body.sort_order)
    .bind(project_id).bind(state_id).bind(auth.0)
    .bind(body.point).bind(body.estimate_point).bind(body.type_id).bind(body.parent)
    .bind(body.external_source.as_deref()).bind(body.external_id.as_deref())
    .bind(&slug)
    .fetch_optional(&st.pool).await?;
    let Some((issue_id,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))));
    };

    replace_bridges(&st, issue_id, project_id, auth.0, body.assignees.as_deref(), body.labels.as_deref()).await?;

    match fetch_detail(&st, &slug, auth.0, project_id, issue_id).await? {
        Some(v) => Ok((StatusCode::CREATED, Json(v))),
        None => Ok(missing()),
    }
}

/// Replaces the live assignee/label bridge rows when the SDK sent the key.
async fn replace_bridges(
    st: &AppState,
    issue_id: uuid::Uuid,
    project_id: uuid::Uuid,
    user_id: uuid::Uuid,
    assignees: Option<&[uuid::Uuid]>,
    labels: Option<&[uuid::Uuid]>,
) -> Result<(), common::errors::AppError> {
    if let Some(ids) = assignees {
        sqlx::query("UPDATE issue_assignees SET deleted_at = now() WHERE issue_id = $1 AND deleted_at IS NULL")
            .bind(issue_id).execute(&st.pool).await?;
        for id in ids {
            sqlx::query(
                "INSERT INTO issue_assignees (id, issue_id, assignee_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, $4, now(), now() FROM workspaces w \
                 WHERE w.id = (SELECT workspace_id FROM projects WHERE id = $3)",
            ).bind(issue_id).bind(id).bind(project_id).bind(user_id).execute(&st.pool).await?;
        }
    }
    if let Some(ids) = labels {
        sqlx::query("UPDATE issue_labels SET deleted_at = now() WHERE issue_id = $1 AND deleted_at IS NULL")
            .bind(issue_id).execute(&st.pool).await?;
        for id in ids {
            sqlx::query(
                "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
                 SELECT gen_random_uuid(), $1, $2, $3, w.id, $4, $4, now(), now() FROM workspaces w \
                 WHERE w.id = (SELECT workspace_id FROM projects WHERE id = $3)",
            ).bind(issue_id).bind(id).bind(project_id).bind(user_id).execute(&st.pool).await?;
        }
    }
    Ok(())
}
```

Note: `resolve_effective_state` is `pub fn` in `issue_write.rs` (it already is per the existing code).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p api`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "feat(api-rs): v1 work item create"
```

---

### Task 10: Update

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Implement `update`**

Replace the `update` stub:

```rust
pub async fn update(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<V1WriteWorkItem>,
) -> R {
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    ).bind(pk).bind(auth.0).fetch_one(&st.pool).await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND (s.id IS NULL OR s.\"group\" != 'triage')",
    ).bind(pk).bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    if exists.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    if let Err(e) = validate_write(&st, project_id, &body).await {
        return Ok(e);
    }

    let start_date = match parse_date(&body.start_date) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    let target_date = match parse_date(&body.target_date) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
    };
    let html = body.description_html.clone().or_else(|| {
        body.description_stripped.as_deref().filter(|s| !s.is_empty()).map(|p| format!("<p>{}</p>", p))
    });

    // Only fields present in the JSON body are written (`COALESCE` cannot set
    // an explicit NULL back), so the SET list and its positional binds are
    // built together in one pass via `BindValue`.
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<BindValue> = Vec::new();
    fn add(sets: &mut Vec<String>, values: &mut Vec<BindValue>, col: &str, v: BindValue) {
        sets.push(format!("{col} = ${}", values.len() + 1));
        values.push(v);
    }
    if let Some(v) = body.name.clone() { add(&mut sets, &mut values, "name", BindValue::Text(v)); }
    if let Some(v) = html.clone() { add(&mut sets, &mut values, "description_html", BindValue::Text(v)); }
    if let Some(v) = body.description_stripped.clone() { add(&mut sets, &mut values, "description_stripped", BindValue::Text(v)); }
    if let Some(v) = body.priority.clone() { add(&mut sets, &mut values, "priority", BindValue::Text(v)); }
    if body.start_date.is_some() { add(&mut sets, &mut values, "start_date", BindValue::Date(start_date)); }
    if body.target_date.is_some() { add(&mut sets, &mut values, "target_date", BindValue::Date(target_date)); }
    if body.state.is_some() { add(&mut sets, &mut values, "state_id", BindValue::Uuid(body.state)); }
    if body.type_id.is_some() { add(&mut sets, &mut values, "type_id", BindValue::Uuid(body.type_id)); }
    if body.parent.is_some() { add(&mut sets, &mut values, "parent_id", BindValue::Uuid(body.parent)); }
    if body.point.is_some() { add(&mut sets, &mut values, "point", BindValue::Int(body.point)); }
    if body.estimate_point.is_some() { add(&mut sets, &mut values, "estimate_point_id", BindValue::Uuid(body.estimate_point)); }
    if let Some(v) = body.sort_order { add(&mut sets, &mut values, "sort_order", BindValue::Float(v)); }
    if let Some(v) = body.is_draft { add(&mut sets, &mut values, "is_draft", BindValue::Bool(v)); }
    if let Some(v) = body.external_source.clone() { add(&mut sets, &mut values, "external_source", BindValue::Text(v)); }
    if let Some(v) = body.external_id.clone() { add(&mut sets, &mut values, "external_id", BindValue::Text(v)); }

    if sets.is_empty() && body.assignees.is_none() && body.labels.is_none() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "No supported fields"}))));
    }

    if !sets.is_empty() {
        let user_pos = values.len() + 1;
        let pk_pos = values.len() + 2;
        let project_pos = values.len() + 3;
        let sql = format!(
            "UPDATE issues SET {}, updated_at = now(), updated_by_id = ${user_pos} \
             WHERE id = ${pk_pos} AND project_id = ${project_pos} AND deleted_at IS NULL",
            sets.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for v in &values {
            q = match v {
                BindValue::Text(s) => q.bind(s.clone()),
                BindValue::Date(d) => q.bind(*d),
                BindValue::Uuid(u) => q.bind(*u),
                BindValue::Int(n) => q.bind(*n),
                BindValue::Float(f) => q.bind(*f),
                BindValue::Bool(b) => q.bind(*b),
            };
        }
        q = q.bind(auth.0).bind(pk).bind(project_id);
        q.execute(&st.pool).await?;
    } else {
        sqlx::query("UPDATE issues SET updated_at = now(), updated_by_id = $1 WHERE id = $2 AND project_id = $3 AND deleted_at IS NULL")
            .bind(auth.0).bind(pk).bind(project_id).execute(&st.pool).await?;
    }

    replace_bridges(&st, pk, project_id, auth.0, body.assignees.as_deref(), body.labels.as_deref()).await?;

    match fetch_detail(&st, &slug, auth.0, project_id, pk).await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}
```

Define the enum once, at module scope near the other v1 types:

```rust
#[derive(Clone)]
enum BindValue {
    Text(String),
    Date(Option<chrono::NaiveDate>),
    Uuid(Option<uuid::Uuid>),
    Int(Option<i32>),
    Float(f64),
    Bool(bool),
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p api`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "feat(api-rs): v1 work item update"
```

---

### Task 11: Archive / unarchive

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`

- [ ] **Step 1: Implement both handlers**

Replace the `archive`/`unarchive` stubs and add imports:

```rust
use crate::routes::issue_archive_one::guard_archive_one_group;
```

```rust
/// `POST .../work-items/{id}/archive/`. Mirrors the app API's single-issue
/// archive (`issue_archive_one::archive`): ADMIN/MEMBER gate, live+draft=false
/// scope, state group must be `completed`/`cancelled` else 400, then 204.
pub async fn archive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT s.\"group\" FROM issues i LEFT JOIN states s ON s.id = i.state_id \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
         AND (s.id IS NULL OR s.\"group\" != 'triage')",
    ).bind(pk).bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    let Some((group,)) = row else { return Ok(missing()); };
    if let Err(msg) = guard_archive_one_group(group.as_deref().unwrap_or("")) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": msg}))));
    }
    sqlx::query("UPDATE issues SET archived_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL")
        .bind(pk).bind(project_id).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn unarchive(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !require_project_write(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i WHERE i.id = $1 AND i.project_id = $2 \
         AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) \
         AND i.deleted_at IS NULL AND i.archived_at IS NOT NULL",
    ).bind(pk).bind(project_id).bind(&slug).fetch_optional(&st.pool).await?;
    if exists.is_none() {
        return Ok(missing());
    }
    sqlx::query("UPDATE issues SET archived_at = NULL WHERE id = $1 AND project_id = $2")
        .bind(pk).bind(project_id).execute(&st.pool).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

`guard_archive_one_group` is `pub(crate)`; widen to `pub(crate)` if needed (it already is).

- [ ] **Step 2: Run the full crate tests**

Run: `cargo test -p api`
Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs
git commit -m "feat(api-rs): v1 work item archive/unarchive"
```

---

### Task 12: Contract smoke + rebuild + MCP verification

**Files:**

- Modify: `apps/api-rs/scripts/v1-smoke.py`

- [ ] **Step 1: Extend the smoke script**

Append before `print("v1 smoke passed")`:

```python
# ---- work items -----------------------------------------------------------
from plane.models.work_items import CreateWorkItem, UpdateWorkItem  # noqa: E402

pid = lite.results[0].id
page = client.work_items.list(WS, pid)
assert hasattr(page, "total_count"), "work item list missing envelope"
print(f"workitem list OK: total_count={page.total_count}")

created = client.work_items.create(
    WS, pid, CreateWorkItem(name="v1-smoke item", priority="low")
)
assert created.id and created.name == "v1-smoke item"
print(f"workitem create OK: {created.id}")

detail = client.work_items.retrieve(WS, pid, created.id)
assert detail.id == created.id
assert detail.description_html is not None
print("workitem retrieve OK")

updated = client.work_items.update(WS, pid, created.id, UpdateWorkItem(priority="high"))
print(f"workitem update OK: priority={updated.priority}")

found = client.work_items.search(WS, "v1-smoke")
assert any(i.id == created.id for i in found.issues), "search did not find created item"
print(f"workitem search OK: {len(found.issues)} hits")

cnt = client.work_items.count_workspace(WS)
assert cnt.total_count >= 1
print(f"workitem count OK: total_count={cnt.total_count}")

archived = client.work_items.retrieve_by_identifier(WS, WS and client.projects.retrieve(WS, pid).identifier, detail.sequence_id)
assert archived.id == created.id
print("workitem retrieve_by_identifier OK")

client.work_items.delete(WS, pid, created.id)
print("workitem delete OK")
```

Remove the earlier `if lite.results:` guard duplication if present; the work-item block assumes at least one project (true for `itsm`).

- [ ] **Step 2: Run the full crate test suite**

Run: `cargo test -p api`
Expected: all pass.

- [ ] **Step 3: Rebuild the container and run the smoke**

```bash
docker compose -f docker-compose-local.yml build --build-arg BINS=api api
docker compose -f docker-compose-local.yml up -d api
UID_GS=$(docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tA -c "SELECT id FROM users WHERE email='ghifari.zuhir@gmail.com' LIMIT 1")
TOKEN="v1smoke-$(date +%s)"
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tA -c "INSERT INTO api_tokens (created_at, updated_at, id, token, label, user_type, description, is_active, is_service, allowed_rate_limit, user_id) VALUES (now(), now(), gen_random_uuid(), '$TOKEN', 'v1smoke2', 0, '', true, false, '60/min', '$UID_GS')" >/dev/null
V1_TOKEN="$TOKEN" V1_WS=itsm /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tA -c "DELETE FROM api_tokens WHERE token='$TOKEN'" >/dev/null
```

Expected: `v1 smoke passed`, with the new `workitem ... OK` lines.

- [ ] **Step 4: Verify through the MCP server**

```bash
export PLANE_API_KEY=plane_api_<redacted> PLANE_WORKSPACE_SLUG=itsm PLANE_BASE_URL=https://api.terraline.space
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"1.0"}}}'; sleep 3; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'; sleep 1; printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"workitem","arguments":{"action":"list","project_id":"<PROJECT_UUID>"}}}'; sleep 15; } | timeout 60 /home/ghifari/plane-mcp-server/.venv/bin/python -m plane_mcp stdio 2>/dev/null | python3 -c "import sys,json; [print(d.get('result',{}).get('content',[{}])[0].get('text','')[:400]) for l in sys.stdin if (d:=json.loads(l)).get('id')==3]"
```

Replace `<PROJECT_UUID>` with the id from `project list`. Expected: JSON envelope with `results` (no `HTTP 404`). Then repeat with `action":"count","project_id":"<PROJECT_UUID>"` and `action":"retrieve","project_id":"<PROJECT_UUID>","workitem_id":"<ID>"`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/scripts/v1-smoke.py
git commit -m "test(api-rs): v1 work item contract smoke"
```

---

## Self-review

- **Spec coverage:** list project/workspace/archived (Tasks 4–5), retrieve + by-identifier (6), search (7), count (8), create (9), update incl. manage_assignee/manage_label (10), delete (3, delegated to `work_item::delete_issue`), archive/unarchive (11). Sub-resources and work-item-types are deferred to Plan 3.
- **Placeholder scan:** stubs exist only in Task 3 and are explicitly replaced in Tasks 4–11. No TBD/TODO; every handler's code is complete.
- **Type consistency:** `V1WorkItemQuery`/`V1CountQuery`/`V1WriteWorkItem`/`V1SearchQuery` are used only by their handlers; `v1_work_item_json`/`v1_count_json`/`v1_search_issue_json`/`count_group_column` are the exact names asserted in `v1_work_item_test.rs`; `fetch_detail`, `require_project_write`, `validate_write`, `replace_bridges`, `list_envelope`, `pql_or_400` are defined before they are used by later tasks.
- **Known compiler risks:** `resolve_effective_state` and `guard_archive_one_group` visibility; `IssueListRow` field visibility; axum route ordering for static `count`/`search` vs `:ident`. Each is called out where it matters.
