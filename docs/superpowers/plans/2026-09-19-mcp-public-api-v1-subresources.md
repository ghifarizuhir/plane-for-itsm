# MCP Public API v1 — Sub-Resources (Phase 4a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the MCP tools `workitem_comment`, `workitem_link`, `workitem_activity`, `workitem_attachment`, and the built-in-dependency half of `workitem_relation` work against `/api/v1` on this fork.

**Architecture:** v1 sub-resource routes live under
`/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/...`. Object endpoints
(comment/link detail + writes, all attachment endpoints) delegate directly to the existing
app-API handlers, which are already permission-gated and return shapes the SDK tolerates
(pydantic `extra="allow"`/ignore). List endpoints that the SDK expects as the 12-key envelope
wrap the app handler's already-filtered array with one shared in-memory paginator
(`v1::common::page_rows`). Activities need a new full-column query because the app handler
returns only `{id, verb}` while the SDK model requires `project`/`workspace`. Dependencies
reuse `list_relations`/`create_relations`/`remove_relation` with a field remap
(`work_item_ids` → `issues`); custom `work-item-relations` return `{}` because no relation
definitions table exists in this fork (deferred).

**Tech Stack:** Rust, axum, sqlx/Postgres, serde_json. Contract source: `plane-sdk==0.2.23`.

**Scope:** Phase 4a only. Work-item-types + workspace features are Phase 4b (separate plan).

**Deviations locked here (reconfirmed against app API + SDK):**

- Custom relation definitions do not exist → `GET work-item-relations/` returns `{}`,
  `POST`/`DELETE work-item-relations/` return 404. The SDK's `workitem_relation` custom
  branch is therefore unreachable; built-in dependencies work fully.
- `relations/` (8-grouped, `/relations/remove/`) is not used by any MCP tool → out of scope.
- Attachment end-to-end upload needs S3 configured; route wiring + list/presign are tested,
  live upload is best-effort.

**SDK list-shape reference (must hold):**

| Endpoint               | SDK expects                                                                                                       |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `comments/`            | 12-key envelope, `results: [WorkItemComment]`                                                                     |
| `links/`               | 12-key envelope, `results: [WorkItemLink]`                                                                        |
| `activities/`          | 12-key envelope, `results: [WorkItemActivity]`; item requires `project`, `workspace`                              |
| `attachments/`         | **bare array** (not envelope)                                                                                     |
| `dependencies/`        | grouped object, 6 keys (`blocking`, `blocked_by`, `start_before`, `start_after`, `finish_before`, `finish_after`) |
| `work-item-relations/` | **JSON object** keyed by label (return `{}`)                                                                      |

---

## File Structure

- `apps/api-rs/crates/api/src/routes/v1/common.rs` — add `page_rows`.
- `apps/api-rs/crates/api/src/routes/v1/subresource.rs` — NEW: `list_comments`, `list_links`.
- `apps/api-rs/crates/api/src/routes/v1/activity.rs` — NEW: `list`, `retrieve`.
- `apps/api-rs/crates/api/src/routes/v1/relation.rs` — NEW: `list_dependencies`,
  `create_dependencies`, `remove_dependency`, `list_custom`, `custom_not_supported`.
- `apps/api-rs/crates/api/src/routes/v1/mod.rs` — declare new modules.
- `apps/api-rs/crates/api/src/main.rs` — register v1 sub-resource routes after the v1 block
  (currently ends `main.rs:1376`).
- `apps/api-rs/crates/api/tests/v1_routes_test.rs` — route-presence test.
- `apps/api-rs/scripts/v1-smoke.py` — extend smoke.

---

### Task 1: Shared in-memory paginator `page_rows`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/common.rs`

- [ ] **Step 1: Write the failing test**

Append to `apps/api-rs/crates/api/src/routes/v1/common.rs` (inside the existing
`#[cfg(test)] mod tests`, after `page_params_default_matches_django`):

```rust
    #[test]
    fn page_rows_wraps_array_in_twelve_key_envelope() {
        let rows = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 3}),
        ];
        let out = page_rows(rows, None, None).unwrap();
        assert_eq!(out["total_count"], 3);
        assert_eq!(out["count"], 3);
        assert_eq!(out["total_pages"], 1);
        assert_eq!(out["results"].as_array().unwrap().len(), 3);
        assert_eq!(out["next_cursor"], "1000:0:0");
        assert_eq!(out["prev_cursor"], "1000:-1:1");
        assert!(out["grouped_by"].is_null());
        assert!(out["extra_stats"].is_null());
    }

    #[test]
    fn page_rows_slices_forward_pages() {
        let rows: Vec<serde_json::Value> = (0..5).map(|i| serde_json::json!({"id": i})).collect();
        // per_page=2, cursor "2:1:0" -> second page (offset 2), 2 rows.
        let out = page_rows(rows, Some("2"), Some("2:1:0")).unwrap();
        assert_eq!(out["count"], 2);
        assert_eq!(out["total_count"], 5);
        assert_eq!(out["total_pages"], 3);
        assert_eq!(out["next_page_results"], true);
        assert_eq!(out["results"][0]["id"], 2);
        assert_eq!(out["results"][1]["id"], 3);
    }

    #[test]
    fn page_rows_beyond_end_is_empty_not_panic() {
        let rows = vec![serde_json::json!({"id": 1})];
        let out = page_rows(rows, Some("2"), Some("2:9:0")).unwrap();
        assert_eq!(out["count"], 0);
        assert_eq!(out["next_page_results"], false);
    }

    #[test]
    fn page_rows_rejects_zero_per_page() {
        assert!(page_rows(vec![], Some("0"), None).is_err());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/api-rs && cargo test -p api page_rows -v`
Expected: FAIL — `cannot find function page_rows`.

- [ ] **Step 3: Implement `page_rows`**

Add these imports at the top of `apps/api-rs/crates/api/src/routes/v1/common.rs`:

```rust
use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::routes::issue_query::build_ungrouped_envelope;
```

(Keep the existing `use crate::routes::issue_common::{...};` line; add the
`build_ungrouped_envelope` import alongside.)

Then add, after `window_for`:

```rust
/// Wrap an already permission-filtered array (from an app-API list handler) in
/// the 12-key SDK envelope, applying `?per_page=`/`?cursor=` slicing in memory.
///
/// Sub-resource lists are small and the app handlers already return the full
/// filtered set, so this avoids duplicating their SQL and gates. `per_page`
/// defaults to 1000 and caps at 1000 like the app API; `per_page <= 0` is a
/// client error (never a divide-by-zero). Negative cursor pages clamp to the
/// first page — the SDK only walks forward (documented deviation).
pub(crate) fn page_rows(
    rows: Vec<Value>,
    per_page: Option<&str>,
    cursor: Option<&str>,
) -> Result<Value, String> {
    let params = PageParams {
        cursor: cursor.map(str::to_string),
        per_page: per_page.map(str::to_string),
    };
    let (limit, cur) = params.resolve()?;
    if limit <= 0 {
        return Err("Invalid per_page value. Cannot exceed 1000.".to_string());
    }
    let page = cur.page.max(0);
    let total = rows.len() as i64;
    let offset = page.saturating_mul(i128::from(limit));
    let slice: Vec<Value> = if offset >= i128::from(total) {
        Vec::new()
    } else {
        let start = offset as usize;
        let end = (start + limit as usize).min(rows.len());
        rows[start..end].to_vec()
    };
    Ok(build_ungrouped_envelope(total, limit, page, slice))
}

/// Convenience mapper so handlers can turn a `page_rows` error into the DRF
/// `400 {"detail": msg}` shape.
pub(crate) fn bad_request(msg: String) -> (StatusCode, serde_json::Value) {
    (StatusCode::BAD_REQUEST, json!({"detail": msg}))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/api-rs && cargo test -p api common:: -v`
Expected: PASS (the four `page_rows_*` tests plus `page_params_default_matches_django`).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/common.rs
git commit -m "feat(api-rs): v1 in-memory page_rows envelope helper"
```

---

### Task 2: Comments + links list envelopes

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/subresource.rs`

- [ ] **Step 1: Create the module with both list handlers**

Create `apps/api-rs/crates/api/src/routes/v1/subresource.rs`:

```rust
//! v1 wrappers for sub-resource list endpoints whose app-API handler returns a
//! bare array but whose SDK contract is the 12-key pagination envelope. The
//! app handler is called first so its membership gating and row shaping are
//! reused verbatim; only pagination is applied here.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::routes::v1::common::{self, PageParams};
use crate::state::AppState;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub async fn list_comments(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<PageParams>,
) -> R {
    let Json(rows) = crate::routes::work_item::list_comments(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
    )
    .await?;
    match common::page_rows(rows, q.per_page.as_deref(), q.cursor.as_deref()) {
        Ok(v) => Ok((StatusCode::OK, Json(v))),
        Err(msg) => Ok(common::bad_request(msg)),
    }
}

pub async fn list_links(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<PageParams>,
) -> R {
    let (status, Json(body)) = crate::routes::work_item::list_links(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
    )
    .await?;
    if status != StatusCode::OK {
        return Ok((status, Json(body)));
    }
    let rows = body.as_array().cloned().unwrap_or_default();
    match common::page_rows(rows, q.per_page.as_deref(), q.cursor.as_deref()) {
        Ok(v) => Ok((StatusCode::OK, Json(v))),
        Err(msg) => Ok(common::bad_request(msg)),
    }
}
```

Note: `common::errors` is the `common` crate's errors module, reachable here as
`common::errors::AppError` only if the existing `use` in sibling v1 files does the same. Copy
the exact `type R = ...` line from `apps/api-rs/crates/api/src/routes/v1/work_item.rs:78`
(it uses `common::errors::AppError`). If the crate is named ambiguously, mirror the imports
already present at the top of `v1/work_item.rs`.

- [ ] **Step 2: Register the module**

In `apps/api-rs/crates/api/src/routes/v1/mod.rs`, add:

```rust
pub mod subresource;
```

- [ ] **Step 3: Type-check (routes not yet wired)**

Run: `cd apps/api-rs && cargo build -p api`
Expected: builds. (Handlers are unused until Task 5; that is fine.)

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/subresource.rs apps/api-rs/crates/api/src/routes/v1/mod.rs
git commit -m "feat(api-rs): v1 comment and link list envelopes"
```

---

### Task 3: Activities list + retrieve (full columns)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/activity.rs`

- [ ] **Step 1: Create the module**

Create `apps/api-rs/crates/api/src/routes/v1/activity.rs`:

```rust
//! v1 work-item activities. The app-API handler returns only `{id, verb}`,
//! but the SDK's `WorkItemActivity` requires `project` and `workspace`, so v1
//! reads the full serialized column set directly (mirrors Django
//! `IssueActivitySerializer`, which excludes only `created_by`/`updated_by`).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::routes::issue_common::fetch_project_member_role;
use crate::routes::project::{deny, missing, is_workspace_admin};
use crate::routes::issue_common::project_gate_allows;
use crate::routes::v1::common::{self, PageParams};
use crate::state::AppState;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

#[derive(Debug, sqlx::FromRow)]
struct V1ActivityRow {
    id: uuid::Uuid,
    verb: String,
    field: Option<String>,
    old_value: Option<String>,
    new_value: Option<String>,
    comment: Option<String>,
    attachments: Option<Vec<String>>,
    created_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    deleted_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    old_identifier: Option<uuid::Uuid>,
    new_identifier: Option<uuid::Uuid>,
    epoch: Option<f64>,
    issue_id: Option<uuid::Uuid>,
    issue_comment_id: Option<uuid::Uuid>,
    actor_id: Option<uuid::Uuid>,
    project_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
}

const ACTIVITY_COLS: &str = "id, verb, field, old_value, new_value, comment, attachments, \
    created_at, updated_at, deleted_at, old_identifier, new_identifier, epoch, \
    issue_id, issue_comment_id, actor_id, project_id, workspace_id";

fn v1_activity_json(r: &V1ActivityRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "deleted_at": r.deleted_at,
        "verb": r.verb,
        "field": r.field,
        "old_value": r.old_value,
        "new_value": r.new_value,
        "comment": r.comment,
        "attachments": r.attachments,
        "old_identifier": r.old_identifier,
        "new_identifier": r.new_identifier,
        "epoch": r.epoch,
        "issue": r.issue_id,
        "issue_comment": r.issue_comment_id,
        "actor": r.actor_id,
        "project": r.project_id,
        "workspace": r.workspace_id,
    })
}

async fn read_gate(st: &AppState, user: uuid::Uuid, slug: &str, project_id: uuid::Uuid) -> Result<bool, common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ))
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<PageParams>,
) -> R {
    if !read_gate(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<V1ActivityRow> = sqlx::query_as(&format!(
        "SELECT {ACTIVITY_COLS} FROM issue_activities \
         WHERE project_id = $1 AND issue_id = $2 AND deleted_at IS NULL ORDER BY created_at ASC"
    ))
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await
    .map_err(common::errors::AppError::internal)?;
    let shaped: Vec<Value> = rows.iter().map(v1_activity_json).collect();
    match common::page_rows(shaped, q.per_page.as_deref(), q.cursor.as_deref()) {
        Ok(v) => Ok((StatusCode::OK, Json(v))),
        Err(msg) => Ok(common::bad_request(msg)),
    }
}

pub async fn retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !read_gate(&st, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<V1ActivityRow> = sqlx::query_as(&format!(
        "SELECT {ACTIVITY_COLS} FROM issue_activities \
         WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .fetch_optional(&st.pool)
    .await
    .map_err(common::errors::AppError::internal)?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(v1_activity_json(&r)))),
        None => Ok(missing()),
    }
}
```

Adjust the `use` lines to match the real paths: `fetch_project_member_role` and
`project_gate_allows` live in `crate::routes::issue_common`; `deny`, `missing`,
`is_workspace_admin` in `crate::routes::project`. `AppError::internal` exists
(`crates/common/src/errors.rs:12`). If `sqlx::types::chrono` is not re-exported, use
`chrono::{DateTime, Utc}` (the crate already serializes chrono timestamps elsewhere).

- [ ] **Step 2: Register the module**

In `apps/api-rs/crates/api/src/routes/v1/mod.rs`, add:

```rust
pub mod activity;
```

- [ ] **Step 3: Type-check**

Run: `cd apps/api-rs && cargo build -p api`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/activity.rs apps/api-rs/crates/api/src/routes/v1/mod.rs
git commit -m "feat(api-rs): v1 work item activities list and retrieve"
```

---

### Task 4: Dependencies + custom-relations stubs

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/relation.rs`

Context you need (verified):

- `crate::routes::work_item::{list_relations, create_relations, remove_relation}` are `pub`.
- `list_relations` returns the 8-key grouped object; each item is
  `relation_issue_json` with a `relation_type` key. The SDK's
  `WorkItemDependencyResponse` validates 6 of those keys and ignores the rest.
- `CreateRelation` (`work_item.rs:82-86`) has `#[serde(default)] issues: Vec<Uuid>` and
  `#[serde(default)] relation_type: Option<String>`.
- `RemoveRelationBody` (`work_item.rs:88-91`) has `related_issue: Option<Uuid>`.
- `Scope = (String, uuid::Uuid, uuid::Uuid)` (`work_item.rs:158`).

- [ ] **Step 1: Create the module**

Create `apps/api-rs/crates/api/src/routes/v1/relation.rs`:

```rust
//! v1 dependencies (built-in) and custom work-item-relations.
//!
//! Custom relations need a `work_item_relation_definitions` table, which this
//! fork does not have, so only the built-in dependency half is functional.
//! Built-in calls remap the SDK field name `work_item_ids` onto the app API's
//! `issues` and reuse the existing handlers unchanged.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::routes::v1::common::PageParams;
use crate::state::AppState;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

const DEPENDENCY_TYPES: &[&str] = &[
    "blocking",
    "blocked_by",
    "start_before",
    "start_after",
    "finish_before",
    "finish_after",
];

#[derive(Debug, Deserialize)]
pub struct V1CreateDependency {
    pub relation_type: String,
    pub work_item_ids: Vec<uuid::Uuid>,
}

pub async fn list_dependencies(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    crate::routes::work_item::list_relations(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
    )
    .await
}

pub async fn create_dependencies(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<V1CreateDependency>,
) -> R {
    if !DEPENDENCY_TYPES.contains(&body.relation_type.as_str()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid relation_type"})),
        ));
    }
    if body.work_item_ids.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "work_item_ids is required"})),
        ));
    }
    let mapped = json!({
        "relation_type": body.relation_type,
        "issues": body.work_item_ids,
    });
    crate::routes::work_item::create_relations(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
        Json(mapped),
    )
    .await
}

pub async fn remove_dependency(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id, related_id)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> R {
    crate::routes::work_item::remove_relation(
        State(st),
        auth,
        Path((slug, project_id, issue_id)),
        Json(json!({"related_issue": related_id})),
    )
    .await
}

pub async fn list_custom(
    State(_st): State<AppState>,
    _auth: AuthUser,
    Path((_slug, _project_id, _issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(_q): Query<PageParams>,
) -> R {
    // No relation-definition table on this fork → no labels to group under.
    // An empty object is valid for the SDK (it iterates `response.items()`).
    Ok((StatusCode::OK, Json(json!({}))))
}

pub async fn custom_not_supported(
    State(_st): State<AppState>,
    _auth: AuthUser,
    Path((_slug, _project_id, _issue_id, _related_id)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> R {
    Ok((
        StatusCode::NOT_FOUND,
        Json(json!({"error": "Work item relation definitions are not available in this workspace"})),
    ))
}
```

- [ ] **Step 2: Register the module**

In `apps/api-rs/crates/api/src/routes/v1/mod.rs`, add:

```rust
pub mod relation;
```

- [ ] **Step 3: Type-check**

Run: `cd apps/api-rs && cargo build -p api`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/relation.rs apps/api-rs/crates/api/src/routes/v1/mod.rs
git commit -m "feat(api-rs): v1 dependencies and custom relation stubs"
```

---

### Task 5: Register all v1 sub-resource routes

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs` (insert after the v1 block, currently `:1376`)
- Modify: `apps/api-rs/crates/api/tests/v1_routes_test.rs`

- [ ] **Step 1: Write the failing route-presence test**

In `apps/api-rs/crates/api/tests/v1_routes_test.rs`, add:

```rust
#[test]
fn v1_subresource_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/attachments/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/attachments/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/dependencies/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/dependencies/:related_id/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/work-item-relations/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/work-item-relations/:related_id/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/api-rs && cargo test -p api v1_subresource_routes_registered -v`
Expected: FAIL on the first missing path.

- [ ] **Step 3: Register the routes**

In `apps/api-rs/crates/api/src/main.rs`, insert immediately after the
`/api/v1/workspaces/:slug/work-items/:ident/` route (the last v1 work-item route, before
`/api/timezones/`):

```rust
        // ---- Public API v1: work-item sub-resources ----------------
        // Object endpoints delegate to the app-API handlers (superset shapes,
        // SDK tolerates extra keys). Lists that the SDK expects as the 12-key
        // envelope go through the v1 wrappers.
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/",
            get(routes::v1::subresource::list_comments).post(routes::work_item::create_comment),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/comments/:pk/",
            get(routes::work_item::get_comment)
                .patch(routes::work_item::patch_comment)
                .delete(routes::work_item::delete_comment),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/",
            get(routes::v1::subresource::list_links).post(routes::work_item::create_link),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/links/:pk/",
            get(routes::work_item::get_link)
                .patch(routes::work_item::patch_link)
                .delete(routes::work_item::delete_link),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/",
            get(routes::v1::activity::list),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/activities/:pk/",
            get(routes::v1::activity::retrieve),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/attachments/",
            get(routes::asset::issue_list).post(routes::asset::issue_presign),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/attachments/:pk/",
            get(routes::asset::issue_get)
                .patch(routes::asset::issue_complete)
                .delete(routes::asset::issue_delete),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/dependencies/",
            get(routes::v1::relation::list_dependencies).post(routes::v1::relation::create_dependencies),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/dependencies/:related_id/",
            delete(routes::v1::relation::remove_dependency),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/work-item-relations/",
            get(routes::v1::relation::list_custom).post(routes::v1::relation::custom_not_supported),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-items/:issue_id/work-item-relations/:related_id/",
            delete(routes::v1::relation::custom_not_supported),
        )
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/api-rs && cargo test -p api v1_routes -v`
Expected: PASS for all three route-presence tests.

- [ ] **Step 5: Build the whole crate**

Run: `cd apps/api-rs && cargo build -p api`
Expected: builds with no errors. Fix any route-handler signature mismatch surfaced by axum
(e.g. attachment handlers requiring `HeaderMap`) before committing.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/v1_routes_test.rs
git commit -m "feat(api-rs): register v1 sub-resource routes"
```

---

### Task 6: Smoke + live verification

**Files:**

- Modify: `apps/api-rs/scripts/v1-smoke.py`

- [ ] **Step 1: Extend the smoke script**

In `apps/api-rs/scripts/v1-smoke.py`, inside the existing success path (after the work-item
checks and before the final `print("v1 smoke passed")`), add calls that exercise each new
route through the real SDK. Use the already-created work item id from the earlier steps; if
the script does not keep one, create a dedicated item first. Add, adapted to the existing
client variable name and `try/except` style in that file:

```python
    # --- sub-resources (Phase 4a) ---
    comment = client.work_items.comments.create(
        workspace_slug=WORKSPACE,
        project_id=project_id,
        work_item_id=work_item_id,
        data={"comment_html": "<p>smoke</p>", "access": "INTERNAL"},
    )
    assert comment.comment_html is not None, "comment create returned no body"

    comments = client.work_items.comments.list(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_id=work_item_id
    )
    assert comments.total_count >= 1, "comment list envelope missing total_count"

    client.work_items.comments.delete(
        workspace_slug=WORKSPACE,
        project_id=project_id,
        work_item_id=work_item_id,
        comment_id=comment.id,
    )

    links = client.work_items.links.list(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_id=work_item_id
    )
    assert links.total_count == 0 or links.results is not None, "link list shape bad"

    activities = client.work_items.activities.list(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_id=work_item_id
    )
    for activity in activities.results:
        assert activity.project, "activity missing required project"
        assert activity.workspace, "activity missing required workspace"

    attachments = client.work_items.attachments.list(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_id=work_item_id
    )
    assert isinstance(attachments, list), "attachments must be a bare array"

    deps = client.work_items.dependencies.list(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_id=work_item_id
    )
    assert hasattr(deps, "blocking"), "dependency grouped object missing 'blocking'"

    custom = client.work_items.custom_relations.list(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_id=work_item_id
    )
    assert isinstance(custom, dict), "custom relations must be a dict"
```

Match the argument-passing style already used in the file (the SDK methods take keyword
args `workspace_slug=`, `project_id=`, `work_item_id=`; request bodies use `data=`).

- [ ] **Step 2: Rebuild the API image**

Run:

```bash
cd /home/ghifari/plane-for-itsm
docker compose -f docker-compose-local.yml build --build-arg BINS=api api
docker compose -f docker-compose-local.yml up -d api
```

Expected: build ~13-15 min; container recreated.

- [ ] **Step 3: Run the smoke script against the live container**

Run:

```bash
V1_TOKEN=plane_api_<redacted> V1_WS=itsm \
  /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
```

Expected: `v1 smoke passed`.

- [ ] **Step 4: Verify `per_page=0` still does not abort the process**

Run:

```bash
docker inspect plane-for-itsm-api-1 --format '{{.RestartCount}}'
curl -s -o /dev/null -w '%{http_code}\n' \
  -H "X-Api-Key: <token>" \
  "http://localhost:8000/api/v1/workspaces/itsm/projects/<pid>/work-items/<wid>/comments/?per_page=0"
docker inspect plane-for-itsm-api-1 --format '{{.RestartCount}}'
```

Expected: `RestartCount` unchanged; the request returns a 4xx/5xx (not a connection reset).

- [ ] **Step 5: Verify through the MCP server**

Run the MCP stdio check (as used in Plan 2 Task 12) calling `workitem_comment` `list` and
`workitem_activity` `list` against a real `<PROJECT_UUID>`/work-item, and confirm JSON
results without a 404 or pydantic validation error.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/scripts/v1-smoke.py
git commit -m "test(api-rs): v1 smoke covers sub-resources"
```

---

## Self-Review

**Spec coverage (Phase 4, sub-resource half):** comments ✓ (list envelope + delegated
CRUD), links ✓, activities ✓, attachments ✓ (delegated, incl. 302/presign/204),
dependencies ✓ (list/create/delete), custom relations ⚠ intentionally stubbed `{}` /
404 (definitions table absent — documented deviation), work-item-types + workspace
features → deferred to Plan 3b. `relations/` 8-group endpoint → out of scope (no MCP caller).

**Placeholder scan:** no TBDs; every step carries code or an exact command. The two
"adjust the `use` lines to match the real paths" notes are because sibling v1 files may
import `common::errors` differently — the implementer copies the proven import block from
`v1/work_item.rs`.

**Type consistency:** `page_rows(Vec<Value>, Option<&str>, Option<&str>) -> Result<Value,
String>` used identically in Tasks 2-4; `common::bad_request(String) -> (StatusCode,
Json<Value>)` used in Tasks 2-3; `V1CreateDependency { relation_type, work_item_ids }`
matches the SDK body; `remove_dependency` remaps to `RemoveRelationBody.related_issue`.

**Known risk:** axum route-param naming across the existing `:pk` work-item detail route and
the new `:issue_id` sub-resource routes — different segment counts, so matchit accepts both;
Task 5 Step 5 build is the gate. If axum reports a conflict, rename the sub-resource prefix
param to `:pk` consistently.
