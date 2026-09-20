# MCP Public API v1 — Work-Item Types + Workspace Features (Phase 4b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the MCP tool `workitem_type` (actions `list`, `retrieve`, `resolve`, `create`,
`update`, `delete`, `import_to_project`) work against `/api/v1`, and expose the workspace
`features` endpoint that `resolve` reads.

**Architecture:** New `routes/v1/work_item_type.rs` serves both scopes. A work-item type is a
workspace-owned row in `issue_types`; project membership is the `project_issue_types` link
table. Project-scope `list` deliberately returns a **bare JSON array** (the SDK iterates the
response and validates each element — an envelope would break it). Workspace features have no
backing storage in this fork, so `routes/v1/workspace.rs` returns a synthetic `WorkspaceFeature`
with all flags `false`; `workitem_type resolve` therefore takes the project-owned path.
`v1/project.rs` already implements project `features` (including the `work_item_types` flag).

**Tech Stack:** Rust, axum, sqlx/Postgres, serde_json. Contract: `plane-sdk==0.2.23`.

**Scope:** Phase 4b only. Relies on Plan 3a (`routes/v1/common.rs::page_rows` is not needed
here; project type list is a bare array).

**Deviations locked here:**

- Workspace features are **synthetic and read-mostly**: `GET` returns all-false flags backed by
  no table; `PATCH` requires ws-ADMIN and is a no-op returning the same all-false object
  (documented; this fork has no workspace feature columns).
- Project-scope `DELETE work-item-types/{id}/` removes only the `project_issue_types` link;
  workspace-scope `DELETE` soft-deletes the `issue_types` row.
- `is_default` is never accepted on create (SDK `CreateWorkItemType` has no such field).

**SDK contract reference:**

| Endpoint                                                 | Method           | Body / Response                        |
| -------------------------------------------------------- | ---------------- | -------------------------------------- |
| `/workspaces/{ws}/projects/{pid}/work-item-types`        | GET              | **bare `list[WorkItemType]`**          |
| `/workspaces/{ws}/projects/{pid}/work-item-types`        | POST             | `CreateWorkItemType` → `WorkItemType`  |
| `/workspaces/{ws}/projects/{pid}/work-item-types/{tid}`  | GET/PATCH/DELETE | `WorkItemType` / `WorkItemType` / 204  |
| `/workspaces/{ws}/projects/{pid}/import-work-item-types` | POST             | `{"work_item_types":[uuid,...]}` → 204 |
| `/workspaces/{ws}/work-item-types/`                      | GET              | **bare `list[WorkItemType]`**          |
| `/workspaces/{ws}/work-item-types/`                      | POST             | `CreateWorkItemType` → `WorkItemType`  |
| `/workspaces/{ws}/work-item-types/{tid}/`                | GET/PATCH/DELETE | `WorkItemType` / `WorkItemType` / 204  |
| `/workspaces/{ws}/features`                              | GET/PATCH        | `WorkspaceFeature`                     |

`WorkItemType` required field: `name`. Optional: `id`, `project_ids`, `created_at`,
`updated_at`, `deleted_at`, `description`, `logo_props`, `is_epic`, `is_default`, `is_active`,
`level`, `external_source`, `external_id`, `created_by`, `updated_by`, `workspace`.
`WorkspaceFeature` fields (all `bool|None`): `project_grouping`, `initiatives`, `teams`,
`customers`, `wiki`, `pi`, `work_item_types`, `releases`, `states_owned_by_workspace`.

**Schema (this fork):**

- `issue_types`: `created_at, updated_at, id, name varchar(255), description text,
logo_props jsonb, created_by_id, updated_by_id, workspace_id, is_active, deleted_at,
is_default, level double precision, external_id, external_source, is_epic`.
- `project_issue_types`: `created_at, updated_at, deleted_at, id, level int, is_default,
created_by_id, issue_type_id, project_id, updated_by_id, workspace_id`.
- `projects.is_issue_type_enabled boolean NOT NULL` (the project `work_item_types` flag).

---

## File Structure

- `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs` — NEW: row struct, shaper, body
  structs, all handlers.
- `apps/api-rs/crates/api/src/routes/v1/workspace.rs` — NEW: synthetic workspace features.
- `apps/api-rs/crates/api/src/routes/v1/mod.rs` — declare both.
- `apps/api-rs/crates/api/src/main.rs` — register routes (after the Plan 3a block).
- `apps/api-rs/crates/api/tests/v1_work_item_type_test.rs` — NEW: pure shaper tests.
- `apps/api-rs/crates/api/tests/v1_routes_test.rs` — route-presence test.
- `apps/api-rs/scripts/v1-smoke.py` — extend smoke.

---

### Task 1: `WorkItemType` row, shaper, and body structs

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`
- Create: `apps/api-rs/crates/api/tests/v1_work_item_type_test.rs`

- [ ] **Step 1: Write the failing test**

Create `apps/api-rs/crates/api/tests/v1_work_item_type_test.rs`:

```rust
use serde_json::json;

#[test]
fn work_item_type_json_has_sdk_required_and_optional_keys() {
    let row = v1_work_item_type_json(&V1WorkItemTypeRow {
        id: uuid::Uuid::nil(),
        name: "Bug".to_string(),
        description: Some("d".to_string()),
        logo_props: None,
        is_epic: false,
        is_default: false,
        is_active: true,
        level: Some(1),
        external_id: None,
        external_source: None,
        created_by: None,
        updated_by: None,
        workspace: uuid::Uuid::nil(),
        created_at: None,
        updated_at: None,
        deleted_at: None,
        project_ids: vec![],
    });
    assert_eq!(row["name"], json!("Bug"));
    for key in [
        "id", "name", "description", "logo_props", "is_epic", "is_default", "is_active",
        "level", "external_id", "external_source", "created_by", "updated_by", "workspace",
        "created_at", "updated_at", "deleted_at", "project_ids",
    ] {
        assert!(row.get(key).is_some(), "missing WorkItemType key: {key}");
    }
    assert_eq!(row["project_ids"], json!([]));
}

#[test]
fn create_body_accepts_missing_fields() {
    let body: V1CreateWorkItemType = serde_json::from_value(json!({})).unwrap();
    assert!(body.name.is_none());
    assert!(body.project_ids.is_empty());
}
```

The test file needs the crate items in scope. Add at the top, mirroring how other integration
tests import the crate (check `apps/api-rs/crates/api/tests/v1_work_item_test.rs` for the exact
`use` line, e.g. `use api::routes::v1::work_item_type::{v1_work_item_type_json, V1CreateWorkItemType, V1WorkItemTypeRow};`).
If the crate is not importable from tests, move these two tests into a `#[cfg(test)] mod tests`
at the bottom of `work_item_type.rs` instead (repo convention for some pure shapers is the
in-file module — pick whichever the sibling test file proves works).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/api-rs && cargo test -p api work_item_type -v`
Expected: FAIL — module/items missing.

- [ ] **Step 3: Implement the row, shaper, and bodies**

Create `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs` with the imports and the
data layer first (handlers arrive in Tasks 2-3):

```rust
//! v1 work-item types. `issue_types` is workspace-owned; membership of a type in
//! a project is the `project_issue_types` link row. Project-scope `list`
//! returns a bare array because that is what the SDK iterates.

use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::routes::member::deny_detail;
use crate::routes::project::{deny, missing, project_role, ws_role};
use crate::routes::v1::common::PageParams;
use crate::state::AppState;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

const TYPE_COLS: &str = "t.id, t.name, t.description, t.logo_props, t.is_epic, t.is_default, \
    t.is_active, t.level::int AS level, t.external_id, t.external_source, \
    t.created_by_id AS created_by, t.updated_by_id AS updated_by, t.workspace_id AS workspace, \
    t.created_at, t.updated_at, t.deleted_at, \
    COALESCE((SELECT array_agg(pit.project_id) FROM project_issue_types pit \
      WHERE pit.issue_type_id = t.id AND pit.deleted_at IS NULL), ARRAY[]::uuid[]) AS project_ids";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct V1WorkItemTypeRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub logo_props: Option<Value>,
    pub is_epic: bool,
    pub is_default: bool,
    pub is_active: bool,
    pub level: Option<i32>,
    pub external_id: Option<String>,
    pub external_source: Option<String>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub workspace: uuid::Uuid,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub project_ids: Vec<uuid::Uuid>,
}

pub fn v1_work_item_type_json(r: &V1WorkItemTypeRow) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "description": r.description,
        "logo_props": r.logo_props,
        "is_epic": r.is_epic,
        "is_default": r.is_default,
        "is_active": r.is_active,
        "level": r.level,
        "external_id": r.external_id,
        "external_source": r.external_source,
        "created_by": r.created_by,
        "updated_by": r.updated_by,
        "workspace": r.workspace,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "deleted_at": r.deleted_at,
        "project_ids": r.project_ids,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1CreateWorkItemType {
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub logo_props: Option<Value>,
    #[serde(default)] pub is_epic: Option<bool>,
    #[serde(default)] pub is_active: Option<bool>,
    #[serde(default)] pub level: Option<i32>,
    #[serde(default)] pub external_id: Option<String>,
    #[serde(default)] pub external_source: Option<String>,
    #[serde(default)] pub project_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1UpdateWorkItemType {
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub logo_props: Option<Value>,
    #[serde(default)] pub is_epic: Option<bool>,
    #[serde(default)] pub is_active: Option<bool>,
    #[serde(default)] pub level: Option<i32>,
    #[serde(default)] pub external_id: Option<String>,
    #[serde(default)] pub external_source: Option<String>,
    #[serde(default)] pub project_ids: Option<Vec<uuid::Uuid>>,
}

/// Resolve the workspace id for a slug, or `None`.
async fn ws_id(st: &AppState, slug: &str) -> Result<Option<uuid::Uuid>, common::errors::AppError> {
    Ok(sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(&st.pool)
        .await?)
}

/// `true` if the caller may write work-item types: ws-ADMIN, or project-ADMIN
/// for a project-scope call. Mirrors `patch_features` (`v1/project.rs:194-198`).
async fn can_write(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    project_id: Option<uuid::Uuid>,
) -> Result<bool, common::errors::AppError> {
    let ws_admin = matches!(ws_role(&st.pool, user, slug).await?, Some(r) if r >= 20);
    if ws_admin {
        return Ok(true);
    }
    match project_id {
        Some(pid) => Ok(matches!(project_role(&st.pool, user, pid).await?, Some(20))),
        None => Ok(false),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/api-rs && cargo test -p api work_item_type -v`
Expected: PASS (2 tests). If the integration-test import path is wrong, move the tests into the
in-file `#[cfg(test)] mod tests` and re-run.

- [ ] **Step 5: Register the module**

In `apps/api-rs/crates/api/src/routes/v1/mod.rs` add:

```rust
pub mod work_item_type;
```

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/src/routes/v1/mod.rs apps/api-rs/crates/api/tests/v1_work_item_type_test.rs
git commit -m "feat(api-rs): v1 work item type row and shaper"
```

---

### Task 2: Workspace-scope work-item-type CRUD

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`

- [ ] **Step 1: Add the workspace-scope handlers**

Append to `work_item_type.rs`:

```rust
pub async fn list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(_q): Query<PageParams>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t \
         WHERE t.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) \
         AND t.deleted_at IS NULL ORDER BY t.created_at ASC"
    );
    let rows: Vec<V1WorkItemTypeRow> = sqlx::query_as(&sql)
        .bind(&slug)
        .fetch_all(&st.pool)
        .await?;
    let out: Vec<Value> = rows.iter().map(v1_work_item_type_json).collect();
    // Bare array: the SDK iterates this response.
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

pub async fn create_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<V1CreateWorkItemType>,
) -> R {
    if !can_write(&st, auth.0, &slug, None).await? {
        return Ok(deny());
    }
    create_type(&st, auth.0, &slug, None, body).await
}

pub async fn retrieve_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t \
         WHERE t.id = $1 AND t.workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND t.deleted_at IS NULL"
    );
    let row: Option<V1WorkItemTypeRow> = sqlx::query_as(&sql).bind(pk).bind(&slug).fetch_optional(&st.pool).await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn update_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<V1UpdateWorkItemType>,
) -> R {
    if !can_write(&st, auth.0, &slug, None).await? {
        return Ok(deny());
    }
    update_type(&st, auth.0, &slug, None, pk, body).await
}

pub async fn delete_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> R {
    if !can_write(&st, auth.0, &slug, None).await? {
        return Ok(deny());
    }
    let affected = sqlx::query(
        "UPDATE issue_types SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Ok(missing());
    }
    sqlx::query("UPDATE project_issue_types SET deleted_at = now(), updated_at = now() WHERE issue_type_id = $1 AND deleted_at IS NULL")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Shared insert used by both scopes. `scope_project` is `Some` for a
/// project-scope create (always linked) and `None` for workspace scope.
async fn create_type(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    scope_project: Option<uuid::Uuid>,
    body: V1CreateWorkItemType,
) -> R {
    let name = body.name.as_deref().map(str::trim).unwrap_or("");
    if name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "name is required"}))));
    }
    if name.chars().count() > 255 {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "name max length 255"}))));
    }
    let Some(ws) = ws_id(st, slug).await? else {
        return Ok(missing());
    };
    let row: V1WorkItemTypeRow = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
         is_active, level, external_id, external_source, workspace_id, created_by_id, updated_by_id, \
         created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, false, $5, $6, $7, $8, $9, $10, $10, now(), now()) \
         RETURNING id, name, description, logo_props, is_epic, is_default, is_active, \
         level::int AS level, external_id, external_source, created_by_id AS created_by, \
         updated_by_id AS updated_by, workspace_id AS workspace, created_at, updated_at, deleted_at, \
         ARRAY[]::uuid[] AS project_ids",
    )
    .bind(name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.logo_props.clone().unwrap_or_else(|| json!({})))
    .bind(body.is_epic.unwrap_or(false))
    .bind(body.is_active.unwrap_or(true))
    .bind(body.level.unwrap_or(0) as f64)
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(ws)
    .bind(user)
    .fetch_one(&st.pool)
    .await?;

    let mut link_ids = body.project_ids.clone();
    if let Some(pid) = scope_project {
        link_ids.push(pid);
    }
    link_projects(st, &ws, row.id, user, &link_ids).await?;

    let row = reload(st, &ws, row.id).await?.ok_or_else(|| common::errors::AppError::internal(anyhow::anyhow!("type vanished after insert")))?;
    Ok((StatusCode::CREATED, Json(v1_work_item_type_json(&row))))
}

/// Insert `project_issue_types` links for types/projects both in `ws`,
/// skipping deleted projects and existing live links.
async fn link_projects(
    st: &AppState,
    ws: &uuid::Uuid,
    type_id: uuid::Uuid,
    user: uuid::Uuid,
    project_ids: &[uuid::Uuid],
) -> Result<(), common::errors::AppError> {
    for pid in project_ids {
        sqlx::query(
            "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, \
             level, is_default, created_by_id, updated_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, p.id, $2, 0, false, $3, $3, now(), now() \
             FROM projects p WHERE p.id = $4 AND p.workspace_id = $2 AND p.deleted_at IS NULL \
             AND NOT EXISTS(SELECT 1 FROM project_issue_types pit \
               WHERE pit.project_id = p.id AND pit.issue_type_id = $1 AND pit.deleted_at IS NULL)",
        )
        .bind(type_id)
        .bind(ws)
        .bind(user)
        .bind(pid)
        .execute(&st.pool)
        .await?;
    }
    Ok(())
}

async fn reload(st: &AppState, ws: &uuid::Uuid, id: uuid::Uuid) -> Result<Option<V1WorkItemTypeRow>, common::errors::AppError> {
    let sql = format!("SELECT {TYPE_COLS} FROM issue_types t WHERE t.id = $1 AND t.workspace_id = $2 AND t.deleted_at IS NULL");
    Ok(sqlx::query_as(&sql).bind(id).bind(ws).fetch_optional(&st.pool).await?)
}

/// Shared update used by both scopes. `scope_project` restricts the row to a
/// project (via a live `project_issue_types` link) when `Some`.
async fn update_type(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    scope_project: Option<uuid::Uuid>,
    pk: uuid::Uuid,
    body: V1UpdateWorkItemType,
) -> R {
    let Some(ws) = ws_id(st, slug).await? else {
        return Ok(missing());
    };
    let in_scope: bool = match scope_project {
        Some(pid) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_issue_types pit JOIN issue_types t ON t.id = pit.issue_type_id \
             WHERE pit.issue_type_id = $1 AND pit.project_id = $2 AND pit.deleted_at IS NULL \
             AND t.workspace_id = $3 AND t.deleted_at IS NULL)",
        )
        .bind(pk)
        .bind(pid)
        .bind(ws)
        .fetch_one(&st.pool)
        .await?,
        None => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL)",
        )
        .bind(pk)
        .bind(ws)
        .fetch_one(&st.pool)
        .await?,
    };
    if !in_scope {
        return Ok(missing());
    }

    if let Some(n) = body.name.as_deref().map(str::trim) {
        if n.is_empty() {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "name is required"}))));
        }
    }
    // Column-by-column COALESCE so omitted fields are untouched.
    let updated = sqlx::query(
        "UPDATE issue_types SET \
         name = COALESCE($2, name), \
         description = COALESCE($3, description), \
         logo_props = COALESCE($4, logo_props), \
         is_epic = COALESCE($5, is_epic), \
         is_active = COALESCE($6, is_active), \
         level = COALESCE($7, level), \
         external_id = COALESCE($8, external_id), \
         external_source = COALESCE($9, external_source), \
         updated_by_id = $10, updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.description.clone())
    .bind(body.logo_props.clone())
    .bind(body.is_epic)
    .bind(body.is_active)
    .bind(body.level.map(|l| l as f64))
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(user)
    .execute(&st.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(missing());
    }
    if let Some(ids) = body.project_ids.as_ref() {
        link_projects(st, &ws, pk, user, ids).await?;
    }
    match reload(st, &ws, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}
```

- [ ] **Step 2: Build and test**

Run: `cd apps/api-rs && cargo build -p api && cargo test -p api work_item_type`
Expected: builds; shaper tests pass. (Handlers are unused until Task 5 — `dead_code` warnings OK.)

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs
git commit -m "feat(api-rs): v1 workspace work item type CRUD"
```

---

### Task 3: Project-scope CRUD + import

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`

- [ ] **Step 1: Add the project-scope handlers**

Append to `work_item_type.rs`:

```rust
/// Project visibility mirrors `v1/project.rs::get_features` (member required;
/// SECRET project non-member → 403, public non-member → 409, missing/archived
/// → 404).
async fn project_readable(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<Option<(StatusCode, Json<Value>)>, common::errors::AppError> {
    if ws_role(&st.pool, user, slug).await?.is_none() {
        return Ok(Some(deny_detail()));
    }
    let Some(row) = crate::routes::project::fetch_project_full(&st.pool, slug, project_id, user).await? else {
        return Ok(Some((StatusCode::NOT_FOUND, Json(json!({"error": "Project does not exist"})))));
    };
    if row.archived_at.is_some() {
        return Ok(Some((StatusCode::NOT_FOUND, Json(json!({"error": "Project does not exist"})))));
    }
    if !row.member_ids.contains(&user) {
        if row.network == 0 {
            return Ok(Some((StatusCode::FORBIDDEN, Json(json!({"error": "You do not have permission"})))));
        }
        return Ok(Some((StatusCode::CONFLICT, Json(json!({"error": "You are not a member of this project"})))));
    }
    Ok(None)
}

pub async fn list_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Query(_q): Query<PageParams>,
) -> R {
    if let Some(resp) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(resp);
    }
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t \
         JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.deleted_at IS NULL \
         WHERE pit.project_id = $1 AND t.deleted_at IS NULL ORDER BY t.created_at ASC"
    );
    let rows: Vec<V1WorkItemTypeRow> = sqlx::query_as(&sql).bind(project_id).fetch_all(&st.pool).await?;
    let out: Vec<Value> = rows.iter().map(v1_work_item_type_json).collect();
    // Bare array: the SDK iterates this response.
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

pub async fn create_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<V1CreateWorkItemType>,
) -> R {
    if !can_write(&st, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(resp) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(resp);
    }
    create_type(&st, auth.0, &slug, Some(project_id), body).await
}

async fn project_scope_ok(
    st: &AppState,
    slug: &str,
    project_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    let ws = ws_id(st, slug).await?;
    let Some(ws) = ws else { return Ok(false) };
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_issue_types pit JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.issue_type_id = $1 AND pit.project_id = $2 AND pit.deleted_at IS NULL \
         AND t.workspace_id = $3 AND t.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(ws)
    .fetch_one(&st.pool)
    .await?)
}

pub async fn retrieve_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if let Some(resp) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(resp);
    }
    if !project_scope_ok(&st, &slug, project_id, pk).await? {
        return Ok(missing());
    }
    let ws = ws_id(&st, &slug).await?;
    match ws {
        Some(ws) => match reload(&st, &ws, pk).await? {
            Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
            None => Ok(missing()),
        },
        None => Ok(missing()),
    }
}

pub async fn update_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<V1UpdateWorkItemType>,
) -> R {
    if !can_write(&st, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(resp) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(resp);
    }
    update_type(&st, auth.0, &slug, Some(project_id), pk, body).await
}

pub async fn delete_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !can_write(&st, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(resp) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(resp);
    }
    // Project scope removes only the link; the workspace-owned type survives.
    let affected = sqlx::query(
        "UPDATE project_issue_types SET deleted_at = now(), updated_at = now() \
         WHERE issue_type_id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

pub async fn import_to_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> R {
    if !can_write(&st, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    let Some(ws) = ws_id(&st, &slug).await? else {
        return Ok(missing());
    };
    let ids: Vec<uuid::Uuid> = body
        .get("work_item_types")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str()).filter_map(|s| uuid::Uuid::parse_str(s).ok()).collect())
        .unwrap_or_default();
    for id in ids {
        sqlx::query(
            "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, \
             level, is_default, created_by_id, updated_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), t.id, p.id, $1, 0, false, $2, $2, now(), now() \
             FROM issue_types t JOIN projects p ON p.id = $3 AND p.workspace_id = $1 AND p.deleted_at IS NULL \
             WHERE t.id = $4 AND t.workspace_id = $1 AND t.deleted_at IS NULL \
             AND NOT EXISTS(SELECT 1 FROM project_issue_types pit \
               WHERE pit.project_id = p.id AND pit.issue_type_id = t.id AND pit.deleted_at IS NULL)",
        )
        .bind(ws)
        .bind(auth.0)
        .bind(project_id)
        .bind(id)
        .execute(&st.pool)
        .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Build and test**

Run: `cd apps/api-rs && cargo build -p api && cargo test -p api work_item_type`
Expected: builds; shaper tests pass.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs
git commit -m "feat(api-rs): v1 project work item types and import"
```

---

### Task 4: Synthetic workspace features

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/workspace.rs`

- [ ] **Step 1: Create the module**

Create `apps/api-rs/crates/api/src/routes/v1/workspace.rs`:

```rust
//! v1 workspace features. This fork has no workspace feature columns, so the
//! endpoint is synthetic: every flag is `false`. `workitem_type resolve` reads
//! `work_item_types`; `false` routes it to the project-owned behaviour, which
//! is what this fork supports.

use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::routes::member::deny_detail;
use crate::routes::project::{deny, ws_role};
use crate::state::AppState;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

pub fn v1_workspace_features_json() -> Value {
    json!({
        "project_grouping": false,
        "initiatives": false,
        "teams": false,
        "customers": false,
        "wiki": false,
        "pi": false,
        "work_item_types": false,
        "releases": false,
        "states_owned_by_workspace": false,
    })
}

pub async fn get_features(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    Ok((StatusCode::OK, Json(v1_workspace_features_json())))
}

pub async fn patch_features(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(_body): Json<Value>,
) -> R {
    if !matches!(ws_role(&st.pool, auth.0, &slug).await?, Some(r) if r >= 20) {
        return Ok(deny());
    }
    // No backing storage: the write is a documented no-op that returns the
    // current (all-false) capability set.
    Ok((StatusCode::OK, Json(v1_workspace_features_json())))
}
```

- [ ] **Step 2: Register the module**

In `apps/api-rs/crates/api/src/routes/v1/mod.rs` add:

```rust
pub mod workspace;
```

- [ ] **Step 3: Build**

Run: `cd apps/api-rs && cargo build -p api`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/workspace.rs apps/api-rs/crates/api/src/routes/v1/mod.rs
git commit -m "feat(api-rs): v1 synthetic workspace features"
```

---

### Task 5: Register routes + route-presence test

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/tests/v1_routes_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `apps/api-rs/crates/api/tests/v1_routes_test.rs`:

```rust
#[test]
fn v1_work_item_type_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/work-item-types/",
        "/api/v1/workspaces/:slug/work-item-types/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-item-types/",
        "/api/v1/workspaces/:slug/projects/:project_id/work-item-types/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/import-work-item-types/",
        "/api/v1/workspaces/:slug/features/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/api-rs && cargo test -p api v1_work_item_type_routes_registered -v`
Expected: FAIL on the first missing path.

- [ ] **Step 3: Register the routes**

In `apps/api-rs/crates/api/src/main.rs`, after the Plan 3a sub-resource block (immediately
before `/api/timezones/`), insert:

```rust
        // ---- Public API v1: work-item types + workspace features --------
        .route(
            "/api/v1/workspaces/:slug/features/",
            get(routes::v1::workspace::get_features).patch(routes::v1::workspace::patch_features),
        )
        .route(
            "/api/v1/workspaces/:slug/work-item-types/",
            get(routes::v1::work_item_type::list_workspace).post(routes::v1::work_item_type::create_workspace),
        )
        .route(
            "/api/v1/workspaces/:slug/work-item-types/:pk/",
            get(routes::v1::work_item_type::retrieve_workspace)
                .patch(routes::v1::work_item_type::update_workspace)
                .delete(routes::v1::work_item_type::delete_workspace),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-item-types/",
            get(routes::v1::work_item_type::list_project).post(routes::v1::work_item_type::create_project),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/work-item-types/:pk/",
            get(routes::v1::work_item_type::retrieve_project)
                .patch(routes::v1::work_item_type::update_project)
                .delete(routes::v1::work_item_type::delete_project),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/import-work-item-types/",
            post(routes::v1::work_item_type::import_to_project),
        )
```

Route-conflict note: `/api/v1/workspaces/:slug/work-items/:ident/` and
`/api/v1/workspaces/:slug/work-item-types/:pk/` are distinct static segments; if axum panics
with a route conflict at startup, rename the workspace type detail param to `:pk` (already `:pk`)
and confirm the work-items routes use `:ident` (they do). Build is the gate.

- [ ] **Step 4: Run tests + build**

Run: `cd apps/api-rs && cargo test -p api v1_routes -v && cargo build -p api`
Expected: 4 route-presence tests pass; build succeeds.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/v1_routes_test.rs
git commit -m "feat(api-rs): register v1 work item type and workspace feature routes"
```

---

### Task 6: Smoke + live verification (`workitem_type resolve`)

**Files:**

- Modify: `apps/api-rs/scripts/v1-smoke.py`

- [ ] **Step 1: Extend the smoke script**

In `apps/api-rs/scripts/v1-smoke.py`, after the Plan 3a sub-resource block and before
`print("v1 smoke passed")`, add a block adapted to the existing client/`WORKSPACE`/`project_id`
names:

```python
    # --- work-item types + workspace features (Phase 4b) ---
    features = client.workspaces.get_features(workspace_slug=WORKSPACE)
    assert features.work_item_types is False, "workspace should not own types"

    ws_types = client.workspace_work_item_types.list(workspace_slug=WORKSPACE)
    assert isinstance(ws_types, list), "workspace type list must be a bare array"

    created_type = client.workspace_work_item_types.create(
        workspace_slug=WORKSPACE, data={"name": "Smoke Type"}
    )
    assert created_type.name == "Smoke Type", "type create returned wrong name"
    assert created_type.id, "type create returned no id"

    got_type = client.workspace_work_item_types.retrieve(
        workspace_slug=WORKSPACE, type_id=created_type.id
    )
    assert got_type.id == created_type.id, "type retrieve mismatch"

    client.work_item_types.import_to_project(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_type_ids=[created_type.id]
    )
    proj_types = client.work_item_types.list(workspace_slug=WORKSPACE, project_id=project_id)
    assert isinstance(proj_types, list), "project type list must be a bare array"
    assert any(t.id == created_type.id for t in proj_types), "imported type missing from project"

    resolved = client.work_item_types.resolve(workspace_slug=WORKSPACE, project_id=project_id, name="Smoke Type") \
        if hasattr(client.work_item_types, "resolve") else None
    # `resolve` is an MCP-level helper, not an SDK method; verify via MCP in Step 6.

    client.work_item_types.delete(
        workspace_slug=WORKSPACE, project_id=project_id, work_item_type_id=created_type.id
    )
    client.workspace_work_item_types.delete(workspace_slug=WORKSPACE, type_id=created_type.id)
```

Only keep lines whose SDK methods actually exist on the pinned `plane-sdk==0.2.23`; verify
method names by reading `plane/api/workspace_work_item_types/` and `plane/api/work_item_types.py`
in the venv before running. Remove the `resolved = ...` placeholder (resolve is MCP-only).

- [ ] **Step 2: Run the full Rust suite**

Run: `cd apps/api-rs && cargo test -p api`
Expected: 0 failures.

- [ ] **Step 3: Rebuild + recreate**

```bash
cd /home/ghifari/plane-for-itsm
docker compose -f docker-compose-local.yml build --build-arg BINS=api api
docker compose -f docker-compose-local.yml up -d api
```

- [ ] **Step 4: Run smoke**

```bash
V1_TOKEN=<token> V1_WS=itsm \
  /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
```

Expected: `v1 smoke passed`.

- [ ] **Step 5: Verify `workitem_type resolve` through MCP**

Drive the MCP `workitem_type` tool with `action=resolve`, a real `project_id`, and a fresh
`name` (e.g. `"MCP Resolve Type"`). Expect a JSON `WorkItemType` whose `name` matches and which
has an `id`, and confirm via a follow-up `action=list` at project scope that it now appears.
Also call `workitem_type action=list` (workspace scope) and assert a bare JSON array.

- [ ] **Step 6: Regression guard**

Confirm `RestartCount` unchanged (`docker inspect plane-for-itsm-api-1 --format '{{.RestartCount}}'`)
and that `?per_page=0` on a type list returns a non-reset response.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/scripts/v1-smoke.py
git commit -m "test(api-rs): v1 smoke covers work item types and features"
```

---

## Self-Review

**Spec coverage (Phase 4, types half):** project CRUD ✓, workspace CRUD ✓,
`import-work-item-types` ✓, workspace features GET/PATCH ✓ (PATCH no-op, documented),
`workitem_type` actions list/retrieve/resolve/create/update/delete/import_to_project ✓.
Deferred (needs new tables, out of scope): `work_item_type_governance` (`/governance/`,
`/pins/`, `/workflow*/`), workspace work-item-type **properties** links.

**Placeholder scan:** no TBDs; every step carries real code or an exact command.

**Type consistency:** `V1WorkItemTypeRow` fields match `TYPE_COLS` order exactly (both used by
`v1_work_item_type_json` and every `query_as`); `create_type`/`update_type` receive
`V1CreateWorkItemType`/`V1UpdateWorkItemType` with distinct `project_ids` types (`Vec` vs
`Option<Vec>`) so "omitted means untouched" holds; `link_projects(&AppState, &Uuid, Uuid, Uuid,
&[Uuid])` is used identically in create/update/import.

**Known risks:**

- `COALESCE((SELECT array_agg(...)), ARRAY[]::uuid[])` must decode as `Vec<Uuid>`; if sqlx
  complains about the unknown array type, add `.bind`-free `AS project_ids` (already present)
  and fall back to a second query per row.
- `import_to_project` returns 204; the SDK treats 204 as `None` ✓.
- Project-scope `delete` removing only the link is a deliberate deviation; verify the SDK/MCP
  `workitem_type delete` (which passes `project_id`) still satisfies the tool's expectation
  (the tool returns `None` unconditionally, so yes).
