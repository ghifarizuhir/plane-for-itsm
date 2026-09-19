# MCP Public API v1 — Plan 1: Infrastructure + Project Tool

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `/api/v1` surface the official Plane MCP server (via `plane-sdk`) needs, and make the `project` tool fully functional.

**Architecture:** Mount a new `/api/v1` router in `apps/api-rs`. Object endpoints (retrieve/create/update/delete/archive) delegate to the existing app-API handlers (same auth, response is a superset and the SDK allows extra keys). List/derived endpoints get new handlers that reuse the existing query patterns, cursor helpers, and the 12-key envelope. New code lives in `routes/v1/` with pure JSON shapers unit-tested per repo convention.

**Tech Stack:** Rust (axum, sqlx runtime queries), PostgreSQL, serde_json. Tests: `cargo test -p api`. Contract smoke: Python `plane-sdk` inside `plane-mcp-server/.venv`.

**Plan split:** This plan = spec Phases 1–2 (infra + project). Plans 2–3 (work item core; sub-resources + types) are written separately.

**Commit convention:** each task ends with a commit. Skip commits if you prefer; stages still apply.

---

## File structure

- Create `apps/api-rs/crates/api/src/routes/v1/mod.rs` — module wiring.
- Create `apps/api-rs/crates/api/src/routes/v1/common.rs` — shared query params + envelope helper.
- Create `apps/api-rs/crates/api/src/routes/v1/project.rs` — v1 project handlers + shapers.
- Create `apps/api-rs/crates/api/tests/v1_project_test.rs` — pure shaper tests.
- Create `apps/api-rs/scripts/v1-smoke.py` — contract smoke against `plane-sdk`.
- Modify `apps/api-rs/crates/api/src/routes/mod.rs` — add `pub mod v1;`.
- Modify `apps/api-rs/crates/api/src/main.rs` — register `/api/v1` routes.

Reference spec: `docs/superpowers/specs/2026-09-18-mcp-public-api-v1-core-design.md`.

---

### Task 1: v1 module + shared common helpers

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/v1/mod.rs`
- Create: `apps/api-rs/crates/api/src/routes/v1/common.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`

- [ ] **Step 1: Write the failing test**

Add to the bottom of the new `common.rs` (the file will not exist yet, so create it with the test and a stub):

```rust
// apps/api-rs/crates/api/src/routes/v1/common.rs
use serde::Deserialize;

/// `?cursor=&per_page=` params shared by v1 list endpoints.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PageParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_params_default_matches_django() {
        let (per_page, cursor) = PageParams::default().resolve().unwrap();
        assert_eq!(per_page, 1000);
        assert_eq!(cursor.page, 0);
        assert!(!cursor.is_prev);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib v1::common`
Expected: compile error `no method named 'resolve'` and `no field 'is_prev'` — feature missing.

- [ ] **Step 3: Implement `common.rs`**

Replace the imports/stub above the test with:

```rust
use serde::Deserialize;

use crate::routes::issue_common::{parse_cursor, parse_per_page, DetailCursor, PageWindow, page_window};

/// `?cursor=&per_page=` params shared by v1 list endpoints.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PageParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub per_page: Option<String>,
}

impl PageParams {
    /// Byte-exact DRF parse: per_page default/max 1000, cursor `value:page:is_prev`.
    pub(crate) fn resolve(&self) -> Result<(i64, DetailCursor), String> {
        let per_page = parse_per_page(self.per_page.as_deref())?;
        let raw = self.cursor.clone().unwrap_or_else(|| format!("{per_page}:0:0"));
        let cursor = parse_cursor(&raw)?;
        Ok((per_page, cursor))
    }
}

/// Offset window for a page (wrapper so handlers don't import `issue_common`
/// directly). `BeyondEnd` renders an empty page. Takes `page` by value because
/// `DetailCursor` is not `Clone`/`Copy` and the caller still needs it afterwards.
pub(crate) fn window_for(page: i128, limit: i64) -> Result<PageWindow, ()> {
    page_window(page, limit)
}
```

Create `v1/mod.rs`:

```rust
pub mod common;
pub mod project;
```

Add to `routes/mod.rs` (after `pub mod themes;`):

```rust
pub mod v1;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --lib v1::common`
Expected: PASS (`page_params_default_matches_django`).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1 apps/api-rs/crates/api/src/routes/mod.rs
git commit -m "feat(api-rs): add v1 module and shared list params"
```

---

### Task 2: Mount `/api/v1` and delegate project object endpoints

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs` (add a v1 route block)

Object endpoints reuse the existing handlers. `project::detail`/`create`/`patch`/`destroy`/`archive`/`unarchive` already authenticate via the same middleware and return a JSON object whose required SDK fields (`name`, `identifier`) are present; the SDK models allow extra keys.

- [ ] **Step 1: Write the failing test**

Add this integration test (it asserts the routes are registered by scanning `main.rs`, same approach as `route_inventory_test.rs`):

```rust
// apps/api-rs/crates/api/tests/v1_routes_test.rs
mod common;

#[test]
fn v1_project_object_routes_registered() {
    let src = std::fs::read_to_string(
        std::path::Path::new(common::MANIFEST_DIR).join("src/main.rs"),
    )
    .expect("main.rs");
    for path in [
        "/api/v1/workspaces/:slug/projects/",
        "/api/v1/workspaces/:slug/projects-lite/",
        "/api/v1/workspaces/:slug/projects/:pk/",
        "/api/v1/workspaces/:slug/projects/:project_id/archive/",
    ] {
        assert!(src.contains(path), "missing v1 route: {path}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test v1_routes_test`
Expected: FAIL `missing v1 route: /api/v1/workspaces/:slug/projects/`.

- [ ] **Step 3: Register the routes**

In `main.rs`, immediately before `.route("/api/timezones/", get(routes::misc::timezones))`, insert:

```rust
        // ---- Public API v1 (plane-sdk / MCP) ----------------------------
        // Object endpoints delegate to the app-API handlers: same auth,
        // response is a superset, and plane-sdk models allow extra keys.
        .route(
            "/api/v1/workspaces/:slug/projects/",
            post(routes::project::create),
        )
        .route(
            "/api/v1/workspaces/:slug/projects-lite/",
            get(routes::v1::project::list_lite),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:pk/",
            get(routes::project::detail)
                .patch(routes::project::patch)
                .delete(routes::project::destroy),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/archive/",
            post(routes::project::archive).delete(routes::project::unarchive),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/features/",
            get(routes::v1::project::get_features).patch(routes::v1::project::patch_features),
        )
        .route(
            "/api/v1/workspaces/:slug/projects/:project_id/total-worklogs/",
            get(routes::v1::project::total_worklogs),
        )
```

`list_lite`, `get_features`, `patch_features`, `total_worklogs` are created in Tasks 3–6. To compile Task 2 in isolation, create stubs in `v1/project.rs` now:

```rust
use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::v1::common::PageParams;

pub async fn list_lite(_: State<AppState>, _: AuthUser, _: Path<String>, _: Query<PageParams>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn get_features(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn patch_features(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>, _: Json<Value>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
pub async fn total_worklogs(_: State<AppState>, _: AuthUser, _: Path<(String, uuid::Uuid)>)
    -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    Ok((StatusCode::NOT_IMPLEMENTED, Json(json!({"detail": "stub"}))))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --test v1_routes_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/src/routes/v1/project.rs apps/api-rs/crates/api/tests/v1_routes_test.rs
git commit -m "feat(api-rs): mount /api/v1 project object routes"
```

---

### Task 3: `projects-lite` list (paginated envelope)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/project.rs`
- Create: `apps/api-rs/crates/api/tests/v1_project_test.rs`

- [ ] **Step 1: Write the failing shaper test**

```rust
// apps/api-rs/crates/api/tests/v1_project_test.rs
use api::routes::v1::project::{v1_project_lite_json, ProjectLiteRow};
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

fn row() -> ProjectLiteRow {
    ProjectLiteRow {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        identifier: "PREPAID".to_string(),
        name: "Ne".to_string(),
        cover_image: Some("https://cdn/cover.png".to_string()),
        icon_prop: None,
        emoji: None,
        description: "d".to_string(),
        archived_at: None,
        cover_image_asset_id: None,
        cover_image_entity_type: None,
    }
}

#[test]
fn project_lite_json_has_sdk_required_fields_and_keys() {
    let v = v1_project_lite_json(&row());
    let o = v.as_object().expect("object");
    for key in ["id", "identifier", "name", "cover_image", "icon_prop", "emoji", "description", "cover_image_url", "archived_at"] {
        assert!(o.contains_key(key), "missing key {key}");
    }
    assert_eq!(o["identifier"], Value::String("PREPAID".into()));
    assert_eq!(o["name"], Value::String("Ne".into()));
    assert_eq!(o["cover_image_url"], Value::String("https://cdn/cover.png".into()));
}

#[test]
fn project_lite_json_maps_sql_null_icon_prop_to_json_null() {
    let v = v1_project_lite_json(&row());
    assert_eq!(v["icon_prop"], Value::Null);
}

#[test]
fn project_lite_json_cover_image_url_matches_helper_semantics() {
    // asset-backed, legacy fallback, both-none, and legacy "" cases follow the
    // shared `cover_image_url` helper (see its unit tests in `project.rs`).
    let mut r = row();
    assert_eq!(v1_project_lite_json(&r)["cover_image_url"], Value::String("https://cdn/cover.png".into()));
    r.cover_image = None;
    assert!(v1_project_lite_json(&r)["cover_image_url"].is_null());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test v1_project_test`
Expected: compile error — `v1_project_lite_json` / `ProjectLiteRow` not found.

- [ ] **Step 3: Implement row + shaper + handler**

Replace the `list_lite` stub in `v1/project.rs` with:

```rust
use sqlx::FromRow;

use crate::routes::issue_query::build_ungrouped_envelope;
use crate::routes::member::deny_detail;
use crate::routes::project::{cover_image_url, missing, ws_role};

/// Trimmed project row for the `projects-lite` shape (`ProjectLiteSerializer`
/// fields the SDK's `ProjectLite` model consumes). `icon_prop` is
/// `Option<Value>` because the column is nullable (quality fix). `cover_image_url`
/// is computed with the shared `cover_image_url` helper from the asset id +
/// entity type (quality fix).
#[derive(Debug, Clone, FromRow)]
pub struct ProjectLiteRow {
    pub id: uuid::Uuid,
    pub identifier: String,
    pub name: String,
    pub cover_image: Option<String>,
    pub icon_prop: Option<Value>,
    pub emoji: Option<String>,
    pub description: String,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub cover_image_asset_id: Option<uuid::Uuid>,
    pub cover_image_entity_type: Option<String>,
}

pub fn v1_project_lite_json(r: &ProjectLiteRow) -> Value {
    json!({
        "id": r.id,
        "identifier": r.identifier,
        "name": r.name,
        "cover_image": r.cover_image,
        "icon_prop": r.icon_prop.clone().unwrap_or(Value::Null),
        "emoji": r.emoji,
        "description": r.description,
        "cover_image_url": cover_image_url(
            r.cover_image_asset_id,
            r.cover_image_entity_type.as_deref(),
            r.cover_image.as_deref(),
        ),
        "archived_at": r.archived_at,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LiteListQuery {
    #[serde(default)] pub cursor: Option<String>,
    #[serde(default)] pub per_page: Option<String>,
    #[serde(default)] pub order_by: Option<String>,
    #[serde(default)] pub include_archived: Option<String>,
}

impl LiteListQuery {
    fn include_archived(&self) -> bool {
        matches!(
            self.include_archived.as_deref().map(|s| s.trim().to_ascii_lowercase()).as_deref(),
            Some("true") | Some("1")
        )
    }
}

pub async fn list_lite(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<LiteListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(role) = ws_role(&st.pool, auth.0, &slug).await? else {
        return Ok(deny_detail());
    };
    let (per_page, cursor) = match (PageParams { cursor: q.cursor.clone(), per_page: q.per_page.clone() }).resolve() {
        Ok(v) => v,
        Err(msg) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": msg})))),
    };
    let limit = per_page.min(1000);
    let window = match crate::routes::v1::common::window_for(cursor.page, limit) {
        Ok(w) => w,
        Err(()) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"})))),
    };
    let scope = if role <= 5 {
        "AND EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
         AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL)"
    } else if role <= 15 {
        "AND (p.network = 2 OR EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
         AND pm.member_id = $2 AND pm.is_active = true AND pm.deleted_at IS NULL))"
    } else {
        ""
    };
    let archived_clause = if q.include_archived() { "" } else { "AND p.archived_at IS NULL" };
    let base = format!(
        "FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         LEFT JOIN file_assets fa ON fa.id = p.cover_image_asset_id \
         WHERE w.slug = $1 AND p.deleted_at IS NULL {archived_clause} {scope}"
    );
    let count_sql = format!("SELECT COUNT(*) {base}");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(&slug)
        .bind(auth.0)
        .fetch_one(&st.pool)
        .await?;
    let offset: Option<i64> = match window {
        crate::routes::issue_common::PageWindow::Rows(o) => Some(o),
        crate::routes::issue_common::PageWindow::BeyondEnd => None,
    };
    let rows: Vec<ProjectLiteRow> = match offset {
        Some(offset) => {
            let sql = format!(
                "SELECT p.id, p.identifier, p.name, p.cover_image, p.icon_prop, p.emoji, \
                 p.description, p.archived_at, p.cover_image_asset_id, \
                 fa.entity_type AS cover_image_entity_type {base} \
                 ORDER BY p.name ASC LIMIT $3 OFFSET $4"
            );
            sqlx::query_as(&sql)
                .bind(&slug)
                .bind(auth.0)
                .bind(limit)
                .bind(offset)
                .fetch_all(&st.pool)
                .await?
        }
        None => Vec::new(),
    };
    let results: Vec<Value> = rows.iter().map(v1_project_lite_json).collect();
    Ok((StatusCode::OK, Json(build_ungrouped_envelope(total, limit, cursor.page, results))))
}
```

Add `use axum::extract::Query;` and `use serde::Deserialize;` to the file imports. Remove the old `PageParams` import if unused.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --test v1_project_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/project.rs apps/api-rs/crates/api/tests/v1_project_test.rs
git commit -m "feat(api-rs): v1 projects-lite list"
```

---

### Task 4: Project features GET

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/project.rs`

- [ ] **Step 1: Write the failing shaper test**

Append to `v1_project_test.rs`:

```rust
use api::routes::v1::project::v1_project_features_json;

#[test]
fn project_features_json_maps_known_columns() {
    let v = v1_project_features_json(true, false, true, false, true, true);
    assert_eq!(v["modules"], Value::Bool(true));
    assert_eq!(v["cycles"], Value::Bool(false));
    assert_eq!(v["views"], Value::Bool(true));
    assert_eq!(v["pages"], Value::Bool(false));
    assert_eq!(v["intakes"], Value::Bool(true));
    assert_eq!(v["work_item_types"], Value::Bool(true));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test v1_project_test project_features_json_maps_known_columns`
Expected: compile error — `v1_project_features_json` not found.

- [ ] **Step 3: Implement shaper + handler**

Replace the `get_features` stub with:

```rust
pub fn v1_project_features_json(
    modules: bool,
    cycles: bool,
    views: bool,
    pages: bool,
    intakes: bool,
    work_item_types: bool,
) -> Value {
    // SDK `ProjectFeature` fields are all optional; these are the ones backed
    // by columns in this fork. `work_item_types` is what `workitem_type
    // resolve` reads.
    json!({
        "modules": modules,
        "cycles": cycles,
        "views": views,
        "pages": pages,
        "intakes": intakes,
        "work_item_types": work_item_types,
    })
}

pub async fn get_features(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let row: Option<(bool, bool, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT p.module_view, p.cycle_view, p.issue_views_view, p.page_view, \
         p.intake_view, p.is_issue_type_enabled \
         FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((m, c, v, p, i, t)) => Ok((
            StatusCode::OK,
            Json(v1_project_features_json(m, c, v, p, i, t)),
        )),
        None => Ok(missing()),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --test v1_project_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/project.rs apps/api-rs/crates/api/tests/v1_project_test.rs
git commit -m "feat(api-rs): v1 project features read"
```

---

### Task 5: Project features PATCH

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/project.rs`

- [ ] **Step 1: Write the failing unit test**

Add a `#[cfg(test)] mod tests` at the bottom of `v1/project.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_column_allowlist_maps_sdk_names() {
        assert_eq!(feature_column("modules"), Some("module_view"));
        assert_eq!(feature_column("cycles"), Some("cycle_view"));
        assert_eq!(feature_column("views"), Some("issue_views_view"));
        assert_eq!(feature_column("pages"), Some("page_view"));
        assert_eq!(feature_column("intakes"), Some("intake_view"));
        assert_eq!(feature_column("work_item_types"), Some("is_issue_type_enabled"));
        assert_eq!(feature_column("epics"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib v1::project`
Expected: compile error — `feature_column` not found.

- [ ] **Step 3: Implement allowlist + handler**

Add (and replace the `patch_features` stub):

```rust
/// Maps an SDK `ProjectFeature` key to the backing column. Unknown keys are
/// ignored (the SDK sends `extra` keys this fork has no column for).
pub fn feature_column(key: &str) -> Option<&'static str> {
    match key {
        "modules" => Some("module_view"),
        "cycles" => Some("cycle_view"),
        "views" => Some("issue_views_view"),
        "pages" => Some("page_view"),
        "intakes" => Some("intake_view"),
        "work_item_types" => Some("is_issue_type_enabled"),
        _ => None,
    }
}

pub async fn patch_features(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let Some(obj) = body.as_object() else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Invalid payload"}))));
    };
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<bool> = Vec::new();
    for (k, v) in obj {
        if let (Some(col), Some(b)) = (feature_column(k), v.as_bool()) {
            binds.push(b);
            sets.push(format!("{col} = ${}", binds.len() + 2));
        }
    }
    if sets.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "No supported feature keys"}))));
    }
    let sql = format!(
        "UPDATE projects SET {}, updated_at = now() \
         WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND deleted_at IS NULL \
         RETURNING module_view, cycle_view, issue_views_view, page_view, intake_view, is_issue_type_enabled",
        sets.join(", ")
    );
    let mut q = sqlx::query_as::<_, (bool, bool, bool, bool, bool, bool)>(&sql)
        .bind(project_id)
        .bind(&slug);
    for b in binds {
        q = q.bind(b);
    }
    match q.fetch_optional(&st.pool).await? {
        Some((m, c, v, p, i, t)) => Ok((StatusCode::OK, Json(v1_project_features_json(m, c, v, p, i, t)))),
        None => Ok(missing()),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p api --lib v1::project`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/project.rs
git commit -m "feat(api-rs): v1 project features update"
```

---

### Task 6: `total-worklogs` (documented empty)

This fork has no work-log table (`work_logs` absent). The SDK accepts an empty
`list[ProjectWorklogSummary]`, so the endpoint returns `[]` after a project-access check and is
documented as a known deviation.

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/project.rs`

- [ ] **Step 1: Implement the handler (no pure logic to unit-test)**

Replace the `total_worklogs` stub:

```rust
/// `GET projects/{id}/total-worklogs/`. This fork has no work-log table, so the
/// list is always empty; access is still gated on project existence + workspace
/// membership so the endpoint is not a silent 404.
pub async fn total_worklogs(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if exists.is_none() {
        return Ok(missing());
    }
    Ok((StatusCode::OK, Json(json!([]))))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p api`
Expected: builds (warnings OK; no errors).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/project.rs
git commit -m "feat(api-rs): v1 project total-worklogs (empty, documented)"
```

---

### Task 7: SDK contract smoke script

**Files:**

- Create: `apps/api-rs/scripts/v1-smoke.py`

- [ ] **Step 1: Write the script**

```python
#!/usr/bin/env python3
"""Contract smoke: drive plane-sdk against the local Rust /api/v1.

Usage:
  V1_TOKEN=plane_api_... V1_WS=itsm /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
Reads the SDK from plane-mcp-server/.venv (PLANE_SDK_PATH can override).
"""
import os
import sys

SDK = os.environ.get("PLANE_SDK_PATH", "/home/ghifari/plane-mcp-server/.venv/lib/python3.11/site-packages")
sys.path.insert(0, SDK)

from plane import PlaneClient  # noqa: E402

BASE = os.environ.get("V1_BASE", "http://localhost:8000")
TOKEN = os.environ["V1_TOKEN"]
WS = os.environ["V1_WS"]

client = PlaneClient(base_url=BASE, api_key=TOKEN)

lite = client.projects.list_lite(WS)
assert lite.total_count >= 0, "projects-lite missing envelope fields"
print(f"projects-lite OK: total_count={lite.total_count}")

if lite.results:
    pid = lite.results[0].id
    project = client.projects.retrieve(WS, pid)
    assert project.name and project.identifier, "retrieve missing required fields"
    print(f"project retrieve OK: {project.identifier} {project.name}")
    feats = client.projects.get_features(WS, pid)
    print(f"project features OK: work_item_types={feats.work_item_types}")
    wl = client.projects.get_worklog_summary(WS, pid)
    assert isinstance(wl, list), "worklog summary must be a list"
    print(f"worklog summary OK: {len(wl)} rows")

print("v1 smoke passed")
```

- [ ] **Step 2: Run it against the local API**

Create a throwaway token (same technique used for the earlier token fix):

```bash
UID_GS=$(docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tA -c "SELECT id FROM users WHERE email='ghifari.zuhir@gmail.com' LIMIT 1")
TOKEN="v1smoke-$(date +%s)"
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tA -c "INSERT INTO api_tokens (created_at, updated_at, id, token, label, user_type, description, is_active, is_service, allowed_rate_limit, user_id) VALUES (now(), now(), gen_random_uuid(), '$TOKEN', 'v1smoke', 0, '', true, false, '60/min', '$UID_GS')" >/dev/null
V1_TOKEN="$TOKEN" V1_WS=itsm /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -tA -c "DELETE FROM api_tokens WHERE token='$TOKEN'" >/dev/null
```

Note: run with the MCP venv Python (3.11), not system `python3` (3.12) — the SDK's
`pydantic_core` binary is built for 3.11 only.

Expected: `v1 smoke passed` (after Task 8's rebuild if the container has not been rebuilt yet).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/scripts/v1-smoke.py
git commit -m "test(api-rs): plane-sdk v1 project contract smoke"
```

---

### Task 8: Rebuild container and verify through the MCP server

**Files:** none (ops + verification).

- [ ] **Step 1: Run the full test suite**

Run: `cargo test -p api`
Expected: all pass (including `v1_project_test`, `v1_routes_test`, existing parity tests).

- [ ] **Step 2: Rebuild + restart the API container**

```bash
docker compose -f docker-compose-local.yml build --build-arg BINS=api api
docker compose -f docker-compose-local.yml up -d api
```

Expected: `Image plane-api-rs:local Built`, container recreated.

- [ ] **Step 3: Re-run the SDK smoke against the rebuilt container**

Repeat Task 7 Step 2. Expected: `v1 smoke passed`.

- [ ] **Step 4: Verify through the MCP server**

```bash
export PLANE_API_KEY=plane_api_<redacted> PLANE_WORKSPACE_SLUG=itsm PLANE_BASE_URL=https://api.terraline.space
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"1.0"}}}'; sleep 3; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'; sleep 1; printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project","arguments":{"action":"list"}}}'; sleep 12; } | timeout 40 /home/ghifari/plane-mcp-server/.venv/bin/python -m plane_mcp stdio 2>/dev/null | python3 -c "import sys,json; [print(d.get('result',{}).get('content',[{}])[0].get('text','')[:400]) for l in sys.stdin if (d:=json.loads(l)).get('id')==3]"
```

Expected: JSON list of projects (no `HTTP 404`).

- [ ] **Step 5: Commit** (none; verification only)

---

## Self-review

- **Spec coverage:** infra (Task 1), project list-lite (3), retrieve/create/update/delete/archive (2), features (4–5), total-worklogs (6). Work item and sub-resources are explicitly deferred to Plans 2–3. Spec's `workspaces/{slug}/features/` is deferred to Plan 3 because only `workitem_type resolve` needs it.
- **Placeholder scan:** no TBD/TODO; every code step shows complete code; stubs are explicitly replaced in later tasks.
- **Type consistency:** `ProjectLiteRow` (Task 3) is the type used by `v1_project_lite_json` and its test; `feature_column`/`v1_project_features_json` names match between Task 4 and 5; `PageParams.resolve`/`window_for` defined in Task 1 and used in Task 3.

## Task 9 (post-review hardening, executed after Task 8)

Final review found the v1 read/write handlers only gated on workspace membership. Fixed in
commit `0aab52605` (`fix(api-rs): v1 project gates mirror detail/patch visibility`):

- `ProjectFullRow` + `fetch_project_full` widened to `pub(crate)` (plus the 9 accessed fields).
- `get_features`/`total_worklogs` mirror `detail`: missing/archived → 404
  `{"error":"Project does not exist"}`; non-member SECRET → 403; non-member public → 409.
- `patch_features`: ws-ADMIN or project-ADMIN else `deny()` 403; missing → `missing()` 404;
  archived → `guard_patch` 400. Existing allowlist/UPDATE unchanged.
- Verified by spec + quality review; live adversarial checks re-run in the post-fix rebuild.
