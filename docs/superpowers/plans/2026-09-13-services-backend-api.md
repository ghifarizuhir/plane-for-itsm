# Services Backend API (Rust api-rs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Services feature work end-to-end by adding `services`, `service_dependencies`, and `service_issues` tables plus session-auth REST handlers to the Rust API (`apps/api-rs`), then wiring `apps/web` to call them.

**Architecture:** A single migration adds the three tables; a new `routes/service.rs` implements pure validation helpers (unit-tested) plus SQL-backed handlers following the existing `routes/state.rs` pattern; routes are registered in `main.rs` and documented in `parity-inventory.json`; the frontend `ServiceService` facade is swapped from the localStorage mock to `APIService` HTTP calls with normalized errors.

**Tech Stack:** Rust (axum 0.7, sqlx runtime queries, PostgreSQL 15, uuid, chrono, serde_json), TypeScript (axios-backed `APIService`), pnpm/oxlint.

**Spec:** `docs/superpowers/specs/2026-09-13-services-backend-design.md`

**Conventions to follow (verified):**
- `crate::routes::project::{deny, missing, FORBIDDEN_MSG}` — `deny()` returns `403 {"error":"You don't have the required permissions."}`; `missing()` returns `404 {"error":"The required object does not exist."}`.
- `crate::routes::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows}` — role `20` = ADMIN, `15` = MEMBER, `5` = GUEST.
- Handler signature pattern: `State(st): State<AppState>`, `auth: AuthUser` (use `auth.0`), `axum::extract::Path((slug, project_id, pk))`, returns `Result<(StatusCode, Json<Value>), common::errors::AppError>`.
- `AppState` has field `pool: sqlx::PgPool`. UUIDs are `uuid::Uuid`. Timestamps are `chrono::DateTime<chrono::Utc>`.
- Route handlers are referenced from `main.rs` as `routes::service::<handler>`; no `use` needed there.
- Rust route tests: `cargo test -p api` runs unit tests + `tests/route_inventory_test.rs`.

---

### Task 1: Migration `0003_services.sql`

**Files:**
- Create: `apps/api-rs/migrations/0003_services.sql`

- [ ] **Step 1: Write the migration**

Create `apps/api-rs/migrations/0003_services.sql`:

```sql
-- Services feature (ITSM): service catalog + dependency DAG + work-item links.
-- New schema delta applied at boot by `common::db::migrate` (sqlx migrate).

CREATE TABLE IF NOT EXISTS public.services (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    name character varying(255) NOT NULL,
    description text NOT NULL DEFAULT '',
    description_html text NOT NULL DEFAULT '',
    status character varying(20) NOT NULL DEFAULT 'planned',
    criticality character varying(20) NOT NULL DEFAULT 'medium',
    "type" character varying(20) NOT NULL DEFAULT 'internal',
    owner_id uuid REFERENCES public.users(id) ON DELETE SET NULL,
    repository_url character varying(200),
    documentation_url character varying(200),
    position jsonb,
    sort_order double precision NOT NULL DEFAULT 65535,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS services_unique_name_project_idx
    ON public.services (project_id, lower(btrim(name))) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS services_project_idx
    ON public.services (project_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.service_dependencies (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    from_service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    to_service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS service_dependencies_pair_idx
    ON public.service_dependencies (from_service_id, to_service_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS service_dependencies_project_idx
    ON public.service_dependencies (project_id) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS public.service_issues (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    issue_id uuid NOT NULL REFERENCES public.issues(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by_id uuid,
    updated_by_id uuid,
    deleted_at timestamp with time zone,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS service_issues_pair_idx
    ON public.service_issues (service_id, issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS service_issues_issue_idx
    ON public.service_issues (issue_id) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS service_issues_project_idx
    ON public.service_issues (project_id) WHERE deleted_at IS NULL;
```

- [ ] **Step 2: Apply the migration by rebuilding the Rust API**

Run:
```bash
docker compose -f docker-compose-local.yml up -d --build api
```
Expected: `api` container starts; `common::db::migrate` logs no error.

- [ ] **Step 3: Verify the tables exist**

Run:
```bash
docker compose -f docker-compose-local.yml exec -T plane-db psql -U plane -d plane -c "\d public.services" -c "\d public.service_dependencies" -c "\d public.service_issues"
```
Expected: three table descriptions, each including the indexes named above.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/migrations/0003_services.sql
git commit -m "feat(api-rs): services schema migration"
```

---

### Task 2: Pure helpers + unit tests in `routes/service.rs`

**Files:**
- Create: `apps/api-rs/crates/api/src/routes/service.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`

- [ ] **Step 1: Register the module**

In `apps/api-rs/crates/api/src/routes/mod.rs`, add this line after `pub mod search;`:

```rust
pub mod service;
```

- [ ] **Step 2: Write the failing tests**

Create `apps/api-rs/crates/api/src/routes/service.rs` with ONLY the following (implementation will be added in Step 4):

```rust
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_name_trims_and_lowercases() {
        assert_eq!(normalize_name("  Web API "), "web api");
    }

    #[test]
    fn validate_enum_accepts_known_and_rejects_unknown() {
        assert!(validate_enum("status", "active", SERVICE_STATUSES).is_ok());
        assert_eq!(
            validate_enum("status", "bogus", SERVICE_STATUSES).unwrap_err(),
            "Invalid status"
        );
        assert_eq!(
            validate_enum("criticality", "nope", SERVICE_CRITICALITIES).unwrap_err(),
            "Invalid criticality"
        );
        assert_eq!(
            validate_enum("type", "nope", SERVICE_TYPES).unwrap_err(),
            "Invalid type"
        );
    }

    #[test]
    fn cycle_detection_self_and_transitive() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        // existing: a -> b, b -> c (a depends on b, b depends on c)
        let edges = vec![(a, b), (b, c)];
        assert!(would_create_cycle(&edges, a, a));
        assert!(would_create_cycle(&edges, c, a));
        assert!(!would_create_cycle(&edges, a, c));
    }

    #[test]
    fn next_sort_order_appends() {
        assert_eq!(next_sort_order(0.0), 65535.0);
        assert_eq!(next_sort_order(65535.0), 131070.0);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p api --lib service::tests`
Expected: FAIL to compile — `normalize_name`, `validate_enum`, `SERVICE_STATUSES`, `would_create_cycle`, `next_sort_order` not found.

- [ ] **Step 4: Implement the helpers (prepend above the tests module)**

Replace the top of `apps/api-rs/crates/api/src/routes/service.rs` so it reads:

```rust
use uuid::Uuid;

/// Allowed `status` values (`packages/types/src/service/core.ts`).
pub const SERVICE_STATUSES: &[&str] = &["active", "planned", "maintenance", "deprecated", "retired"];
/// Allowed `criticality` values.
pub const SERVICE_CRITICALITIES: &[&str] = &["critical", "high", "medium", "low"];
/// Allowed `type` values.
pub const SERVICE_TYPES: &[&str] = &["internal", "external", "infrastructure", "third_party"];

/// trim + lowercase, matching the mock's case-insensitive uniqueness.
pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Enum validation; `field` appears in the error message (`"Invalid status"`).
pub fn validate_enum(field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("Invalid {field}"))
    }
}

/// True when adding edge `from -> to` creates a cycle, i.e. `from` is
/// reachable from `to` following existing `(from, to)` edges. `from == to`
/// is a self-edge and also reported as a cycle.
pub fn would_create_cycle(edges: &[(Uuid, Uuid)], from: Uuid, to: Uuid) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = std::collections::HashSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        for (a, b) in edges {
            if *a == node {
                if *b == from {
                    return true;
                }
                stack.push(*b);
            }
        }
    }
    false
}

/// Append order: `max(0, existing_max) + 65535`, matching the mock.
pub fn next_sort_order(max_existing: f64) -> f64 {
    max_existing.max(0.0) + 65535.0
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p api --lib service::tests`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api-rs): service helpers and unit tests"
```

---

### Task 3: Service CRUD handlers + serializer

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Append the CRUD code above the `#[cfg(test)]` module**

Insert the following code into `apps/api-rs/crates/api/src/routes/service.rs`, immediately before the `#[cfg(test)] mod tests` block (so the `use` lines sit with the other imports at the top; move them up if needed — see the final file layout note in Step 2):

```rust
// ---------------------------------------------------------------------------
// Row + serializer
// ---------------------------------------------------------------------------

/// Full `services` row. `type` is aliased to `service_type` (Rust keyword).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub description_html: String,
    pub status: String,
    pub criticality: String,
    pub service_type: String,
    pub owner_id: Option<Uuid>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
    pub position: Option<serde_json::Value>,
    pub sort_order: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const SERVICE_SELECT: &str = "SELECT s.id, s.workspace_id, s.project_id, s.name, s.description, \
    s.description_html, s.status, s.criticality, s.\"type\" AS service_type, s.owner_id, \
    s.repository_url, s.documentation_url, s.position, s.sort_order, s.created_at, s.updated_at, \
    s.created_by_id, s.updated_by_id FROM services s";

fn service_json(row: &ServiceRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "project_id": row.project_id,
        "name": row.name,
        "description": row.description,
        "description_html": row.description_html,
        "status": row.status,
        "criticality": row.criticality,
        "type": row.service_type,
        "owner_id": row.owner_id,
        "repository_url": row.repository_url,
        "documentation_url": row.documentation_url,
        "position": row.position,
        "sort_order": row.sort_order,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
        "updated_by": row.updated_by_id,
    })
}

// ---------------------------------------------------------------------------
// Gates + request bodies
// ---------------------------------------------------------------------------

async fn gate_member(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(role, Some(20) | Some(15) | Some(5)),
        role.is_some(),
        ws_admin,
    ))
}

async fn gate_writer(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(matches!(role, Some(20) | Some(15)), role.is_some(), ws_admin))
}

/// Deserialize a present-but-null field as `Some(None)` so PATCH can clear it.
fn deserialize_present<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateService {
    pub name: Option<String>,
    pub description: Option<String>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    pub criticality: Option<String>,
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    pub owner_id: Option<Uuid>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchService {
    pub name: Option<String>,
    pub description: Option<String>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    pub criticality: Option<String>,
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub owner_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub repository_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub documentation_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub position: Option<Option<Value>>,
}

fn bad_request(msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg.into() })))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.project_id = $1 AND s.deleted_at IS NULL \
         ORDER BY s.sort_order ASC, s.created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(Value::Array(rows.iter().map(service_json).collect()))))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateService>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Untitled service".to_string());
    let status = body.status.clone().unwrap_or_else(|| "planned".to_string());
    let criticality = body.criticality.clone().unwrap_or_else(|| "medium".to_string());
    let service_type = body.service_type.clone().unwrap_or_else(|| "internal".to_string());
    if let Err(e) = validate_enum("status", &status, SERVICE_STATUSES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("criticality", &criticality, SERVICE_CRITICALITIES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("type", &service_type, SERVICE_TYPES) {
        return Ok(bad_request(e));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services WHERE project_id = $1 \
         AND lower(btrim(name)) = lower(btrim($2)) AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&name)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("A service with this name already exists."));
    }
    let max_order: f64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), 0) FROM services WHERE project_id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, workspace_id, project_id, name, description, description_html, \
         status, criticality, \"type\", owner_id, repository_url, documentation_url, position, \
         sort_order, created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, $4, $5, $6, $7, $8, $9, $10, NULL, $11, \
         now(), now(), $12, $12 FROM projects p WHERE p.id = $13",
    )
    .bind(id)
    .bind(&name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(body.owner_id)
    .bind(body.repository_url.clone())
    .bind(body.documentation_url.clone())
    .bind(next_sort_order(max_order))
    .bind(auth.0)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    let row: ServiceRow = sqlx::query_as(&format!("{SERVICE_SELECT} WHERE s.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(service_json(&row))))
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(service_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchService>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let current: Option<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok(missing());
    };

    let name = body.name.clone().unwrap_or_else(|| current.name.clone());
    if name.trim().is_empty() {
        return Ok(bad_request("Invalid name"));
    }
    let status = body.status.clone().unwrap_or_else(|| current.status.clone());
    let criticality = body
        .criticality
        .clone()
        .unwrap_or_else(|| current.criticality.clone());
    let service_type = body
        .service_type
        .clone()
        .unwrap_or_else(|| current.service_type.clone());
    if let Err(e) = validate_enum("status", &status, SERVICE_STATUSES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("criticality", &criticality, SERVICE_CRITICALITIES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("type", &service_type, SERVICE_TYPES) {
        return Ok(bad_request(e));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services WHERE project_id = $1 \
         AND lower(btrim(name)) = lower(btrim($2)) AND id != $3 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&name)
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("A service with this name already exists."));
    }

    let description = body.description.clone().unwrap_or_else(|| current.description.clone());
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    let owner_id = match body.owner_id {
        Some(v) => v,
        None => current.owner_id,
    };
    let repository_url = match body.repository_url {
        Some(v) => v,
        None => current.repository_url.clone(),
    };
    let documentation_url = match body.documentation_url {
        Some(v) => v,
        None => current.documentation_url.clone(),
    };
    let position = match body.position {
        Some(v) => v,
        None => current.position.clone(),
    };

    sqlx::query(
        "UPDATE services SET name = $1, description = $2, description_html = $3, status = $4, \
         criticality = $5, \"type\" = $6, owner_id = $7, repository_url = $8, documentation_url = $9, \
         position = $10, updated_at = now(), updated_by_id = $11 \
         WHERE id = $12 AND project_id = $13 AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&description)
    .bind(&description_html)
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(owner_id)
    .bind(&repository_url)
    .bind(&documentation_url)
    .bind(&position)
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;

    let row: ServiceRow = sqlx::query_as(&format!("{SERVICE_SELECT} WHERE s.id = $1"))
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::OK, Json(service_json(&row))))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND (from_service_id = $2 OR to_service_id = $2) AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE service_issues SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND service_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE services SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    // Idempotent: a missing service still returns 204 (matches the mock).
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

Also update the top import block so the file starts with:

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};
```

- [ ] **Step 2: Verify it compiles and existing tests pass**

Run: `cargo test -p api --lib service::tests`
Expected: PASS (4 tests). If you see `unused import` for `Deserialize`, confirm the CRUD structs were appended.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api-rs): service CRUD handlers"
```

---

### Task 4: Dependency handlers

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Append dependency code before the `#[cfg(test)]` module**

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

fn dependency_json(row: &DependencyRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "project_id": row.project_id,
        "from_service_id": row.from_service_id,
        "to_service_id": row.to_service_id,
        "created_at": row.created_at,
    })
}

const DEPENDENCY_SELECT: &str = "SELECT id, workspace_id, project_id, from_service_id, \
    to_service_id, created_at FROM service_dependencies";

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDependency {
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
}

async fn service_exists(pool: &sqlx::PgPool, project_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(id)
    .bind(project_id)
    .fetch_one(pool)
    .await
}

pub async fn dependencies_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(dependency_json).collect())),
    ))
}

pub async fn dependencies_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateDependency>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if !service_exists(&st.pool, project_id, body.from_service_id).await? {
        return Ok(bad_request("Source service not found."));
    }
    if !service_exists(&st.pool, project_id, body.to_service_id).await? {
        return Ok(bad_request("Target service not found."));
    }
    if body.from_service_id == body.to_service_id {
        return Ok(bad_request("A service cannot depend on itself."));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_dependencies WHERE project_id = $1 \
         AND from_service_id = $2 AND to_service_id = $3 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(body.from_service_id)
    .bind(body.to_service_id)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("This dependency already exists."));
    }
    // Adding from -> to creates a cycle when `from` is reachable from `to`.
    let creates_cycle: bool = sqlx::query_scalar(
        "WITH RECURSIVE reach(node) AS ( \
           SELECT to_service_id FROM service_dependencies \
             WHERE project_id = $1 AND from_service_id = $2 AND deleted_at IS NULL \
           UNION \
           SELECT d.to_service_id FROM service_dependencies d \
             JOIN reach r ON d.from_service_id = r.node \
             WHERE d.project_id = $1 AND d.deleted_at IS NULL \
         ) SELECT EXISTS(SELECT 1 FROM reach WHERE node = $3)",
    )
    .bind(project_id)
    .bind(body.to_service_id)
    .bind(body.from_service_id)
    .fetch_one(&st.pool)
    .await?;
    if creates_cycle {
        return Ok(bad_request("This dependency would create a cycle."));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_dependencies (id, workspace_id, project_id, from_service_id, \
         to_service_id, created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, now(), now(), $4, $4 FROM projects p WHERE p.id = $5",
    )
    .bind(id)
    .bind(body.from_service_id)
    .bind(body.to_service_id)
    .bind(auth.0)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    let row: DependencyRow = sqlx::query_as(&format!("{DEPENDENCY_SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(dependency_json(&row))))
}

pub async fn dependency_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verify compile + tests**

Run: `cargo test -p api --lib service::tests`
Expected: PASS (4 tests). `DEPENDENCY_SELECT`/`dependency_json` are used by Task 5 handlers, so the `dead_code` lint does not fire once Task 5 lands. If `cargo test` fails now on `dead_code`, proceed to Task 5 before running the full suite.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api-rs): service dependency handlers"
```

---

### Task 5: Work-item link handlers

**Files:**
- Modify: `apps/api-rs/crates/api/src/routes/service.rs`

- [ ] **Step 1: Append link code before the `#[cfg(test)]` module**

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceIssueRow {
    pub id: Uuid,
    pub service_id: Uuid,
    pub issue_id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub issue_identifier: String,
    pub issue_name: String,
}

fn service_issue_json(row: &ServiceIssueRow) -> Value {
    json!({
        "id": row.id,
        "service_id": row.service_id,
        "issue_id": row.issue_id,
        "project_id": row.project_id,
        "workspace_id": row.workspace_id,
        "issue_identifier": row.issue_identifier,
        "issue_name": row.issue_name,
    })
}

const SERVICE_ISSUE_SELECT: &str = "SELECT si.id, si.service_id, si.issue_id, si.project_id, \
    si.workspace_id, (p.identifier || '-' || i.sequence_id) AS issue_identifier, \
    i.name AS issue_name FROM service_issues si \
    JOIN issues i ON i.id = si.issue_id JOIN projects p ON p.id = si.project_id";

#[derive(Debug, Clone, Deserialize)]
pub struct CreateServiceIssue {
    pub service_id: Uuid,
    pub issue_id: Uuid,
}

pub async fn issues_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} WHERE si.project_id = $1 AND si.deleted_at IS NULL \
         ORDER BY si.created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(service_issue_json).collect())),
    ))
}

pub async fn issues_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateServiceIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if !service_exists(&st.pool, project_id, body.service_id).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Service not found."}))));
    }
    let issue_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(body.issue_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    if !issue_exists {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    let existing: Option<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} WHERE si.project_id = $1 AND si.service_id = $2 \
         AND si.issue_id = $3 AND si.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(body.service_id)
    .bind(body.issue_id)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(row) = existing {
        return Ok((StatusCode::CREATED, Json(service_issue_json(&row))));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, now(), now(), $4, $4 FROM projects p WHERE p.id = $5",
    )
    .bind(id)
    .bind(body.service_id)
    .bind(body.issue_id)
    .bind(auth.0)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    let row: ServiceIssueRow = sqlx::query_as(&format!("{SERVICE_ISSUE_SELECT} WHERE si.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(service_issue_json(&row))))
}

pub async fn issue_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE service_issues SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verify compile + tests**

Run: `cargo test -p api --lib service::tests`
Expected: PASS (4 tests), no `dead_code` warnings.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs
git commit -m "feat(api-rs): service work-item link handlers"
```

---

### Task 6: Register routes + parity inventory

**Files:**
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/parity-inventory.json`

- [ ] **Step 1: Register the routes in `main.rs`**

In `apps/api-rs/crates/api/src/main.rs`, immediately after the states `.route(...)` block (the block that ends the `/states/:pk/` route around line 650), add:

```rust
        // Services (ITSM catalog + dependency DAG + work-item links).
        // Session auth; project-scoped. GET = any active member (incl.
        // guest), writes = project ADMIN/MEMBER. Bodies/error strings follow
        // the frontend mock contract (see services backend design spec).
        .route(
            "/api/workspaces/:slug/projects/:project_id/services/",
            get(routes::service::list).post(routes::service::create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/services/:pk/",
            get(routes::service::detail)
                .put(routes::service::patch)
                .patch(routes::service::patch)
                .delete(routes::service::destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/service-dependencies/",
            get(routes::service::dependencies_list).post(routes::service::dependencies_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/service-dependencies/:pk/",
            delete(routes::service::dependency_destroy),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/service-issues/",
            get(routes::service::issues_list).post(routes::service::issues_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/service-issues/:pk/",
            delete(routes::service::issue_destroy),
        )
```

- [ ] **Step 2: Add the `service` domain to the inventory**

In `apps/api-rs/crates/api/parity-inventory.json`, add a new `"service"` key inside the top-level `"domains"` object (for example after the `"module"` domain's closing brace, adding a comma after the previous entry). Use exactly this value:

```json
"service": {
  "rust_module": "routes/service.rs",
  "endpoints": [
    {
      "methods": ["GET", "POST"],
      "path": "/api/workspaces/:slug/projects/:project_id/services/",
      "django_source": "N/A (new ITSM feature, no Django parity)",
      "rust_status": "implemented",
      "rust_handler": "routes::service::list/create",
      "fe_evidence": [
        { "service": "apps/web/core/services/service.service.ts", "method": "getServices" },
        { "service": "apps/web/core/services/service.service.ts", "method": "createService" }
      ],
      "fe_pages": [],
      "batch_task": "Services backend",
      "out_scope": false,
      "notes": "New feature. Session-auth project-scoped service catalog; GET list, POST create (201)."
    },
    {
      "methods": ["GET", "PATCH", "PUT", "DELETE"],
      "path": "/api/workspaces/:slug/projects/:project_id/services/:pk/",
      "django_source": "N/A (new ITSM feature, no Django parity)",
      "rust_status": "implemented",
      "rust_handler": "routes::service::detail/patch/destroy",
      "fe_evidence": [
        { "service": "apps/web/core/services/service.service.ts", "method": "updateService" },
        { "service": "apps/web/core/services/service.service.ts", "method": "deleteService" }
      ],
      "fe_pages": [],
      "batch_task": "Services backend",
      "out_scope": false,
      "notes": "PUT aliases PATCH. PATCH accepts position:{x,y}|null. DELETE soft-cascades dependencies and links, 204 idempotent."
    },
    {
      "methods": ["GET", "POST"],
      "path": "/api/workspaces/:slug/projects/:project_id/service-dependencies/",
      "django_source": "N/A (new ITSM feature, no Django parity)",
      "rust_status": "implemented",
      "rust_handler": "routes::service::dependencies_list/dependencies_create",
      "fe_evidence": [
        { "service": "apps/web/core/services/service.service.ts", "method": "getDependencies" },
        { "service": "apps/web/core/services/service.service.ts", "method": "createDependency" }
      ],
      "fe_pages": [],
      "batch_task": "Services backend",
      "out_scope": false,
      "notes": "DAG edges (from depends on to); self/duplicate/cycle rejected 400 with mock-verbatim messages."
    },
    {
      "methods": ["DELETE"],
      "path": "/api/workspaces/:slug/projects/:project_id/service-dependencies/:pk/",
      "django_source": "N/A (new ITSM feature, no Django parity)",
      "rust_status": "implemented",
      "rust_handler": "routes::service::dependency_destroy",
      "fe_evidence": [
        { "service": "apps/web/core/services/service.service.ts", "method": "deleteDependency" }
      ],
      "fe_pages": [],
      "batch_task": "Services backend",
      "out_scope": false,
      "notes": "Soft delete, 204 idempotent."
    },
    {
      "methods": ["GET", "POST"],
      "path": "/api/workspaces/:slug/projects/:project_id/service-issues/",
      "django_source": "N/A (new ITSM feature, no Django parity)",
      "rust_status": "implemented",
      "rust_handler": "routes::service::issues_list/issues_create",
      "fe_evidence": [
        { "service": "apps/web/core/services/service.service.ts", "method": "getWorkItemLinks" },
        { "service": "apps/web/core/services/service.service.ts", "method": "linkWorkItem" }
      ],
      "fe_pages": [],
      "batch_task": "Services backend",
      "out_scope": false,
      "notes": "Bridge to work items; list computes issue_identifier (project.identifier-sequence_id) and issue_name via join."
    },
    {
      "methods": ["DELETE"],
      "path": "/api/workspaces/:slug/projects/:project_id/service-issues/:pk/",
      "django_source": "N/A (new ITSM feature, no Django parity)",
      "rust_status": "implemented",
      "rust_handler": "routes::service::issue_destroy",
      "fe_evidence": [
        { "service": "apps/web/core/services/service.service.ts", "method": "unlinkWorkItem" }
      ],
      "fe_pages": [],
      "batch_task": "Services backend",
      "out_scope": false,
      "notes": "Soft delete, 204 idempotent."
    }
  ]
}
```

- [ ] **Step 3: Validate JSON and the route gate**

Run:
```bash
python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null && echo "JSON OK"
cargo test -p api --test route_inventory_test
```
Expected: `JSON OK`; route inventory tests PASS (all implemented paths are registered in `main.rs`).

- [ ] **Step 4: Build and run the full Rust test suite**

Run: `cargo test -p api`
Expected: PASS (no compile errors, no new failures).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(api-rs): register services routes and inventory entries"
```

---

### Task 7: Wire the frontend `ServiceService` to HTTP

**Files:**
- Modify: `apps/web/core/services/service.service.ts`

- [ ] **Step 1: Replace the service implementation**

Replace the entire contents of `apps/web/core/services/service.service.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type { IService, IServiceDependency, TServiceWorkItemLink } from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TServiceErrorBody = { detail?: string; error?: string; [key: string]: unknown };

/**
 * Normalizes an axios error into a real Error whose `.message` carries the
 * backend message, while also exposing `.detail`/`.error`. Components use
 * both styles: the modal reads `err.detail/err.error`, the graph and
 * dependency views branch on `error instanceof Error`.
 */
const toServiceError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TServiceErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  return normalized;
};

const base = (workspaceSlug: string, projectId: string) =>
  `/api/workspaces/${workspaceSlug}/projects/${projectId}`;

export class ServiceService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getServices(workspaceSlug: string, _workspaceId: string, projectId: string): Promise<IService[]> {
    return this.get(`${base(workspaceSlug, projectId)}/services/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getDependencies(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string
  ): Promise<IServiceDependency[]> {
    return this.get(`${base(workspaceSlug, projectId)}/service-dependencies/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkItemLinks(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string
  ): Promise<TServiceWorkItemLink[]> {
    return this.get(`${base(workspaceSlug, projectId)}/service-issues/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async createService(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ): Promise<IService> {
    return this.post(`${base(workspaceSlug, projectId)}/services/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateService(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ): Promise<IService> {
    return this.patch(`${base(workspaceSlug, projectId)}/services/${serviceId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteService(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    serviceId: string
  ): Promise<void> {
    return this.delete(`${base(workspaceSlug, projectId)}/services/${serviceId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async createDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ): Promise<IServiceDependency> {
    return this.post(`${base(workspaceSlug, projectId)}/service-dependencies/`, {
      from_service_id: fromServiceId,
      to_service_id: toServiceId,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    dependencyId: string
  ): Promise<void> {
    return this.delete(`${base(workspaceSlug, projectId)}/service-dependencies/${dependencyId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateNodePosition(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ): Promise<IService> {
    return this.updateService(workspaceSlug, workspaceId, projectId, serviceId, { position });
  }

  async linkWorkItem(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ): Promise<TServiceWorkItemLink> {
    return this.post(`${base(workspaceSlug, projectId)}/service-issues/`, {
      service_id: serviceId,
      issue_id: issue.id,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async unlinkWorkItem(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string,
    linkId: string
  ): Promise<void> {
    return this.delete(`${base(workspaceSlug, projectId)}/service-issues/${linkId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
}
```

- [ ] **Step 2: Typecheck and lint the web app**

Run:
```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```
Expected: both PASS. If `check:types` complains about `response` being `any`, confirm `APIService.get/post/patch/delete` follow the same usage as `apps/web/core/services/module.service.ts`.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/service.service.ts
git commit -m "feat(web): wire services to backend API"
```

---

### Task 8: End-to-end verification

**Files:** none (verification only)

- [ ] **Step 1: Rebuild the stack**

Run:
```bash
docker compose -f docker-compose-local.yml up -d --build api proxy
```
Expected: `api` and `proxy` start cleanly.

- [ ] **Step 2: Verify API behavior with curl**

Log in through the web app (or reuse an existing session cookie) and run these against `http://localhost:8080` with `-b cookies.txt`. Replace `WS`, `PID`, and `SID` with real ids.

```bash
# list (expect 200 [])
curl -s -o /dev/null -w "%{http_code}\n" -b cookies.txt \
  "http://localhost:8080/api/workspaces/$WS/projects/$PID/services/"

# create (expect 201 + full row)
curl -s -b cookies.txt -H 'Content-Type: application/json' \
  -d '{"name":"Web API","status":"active","criticality":"high","type":"internal"}' \
  "http://localhost:8080/api/workspaces/$WS/projects/$PID/services/"

# duplicate name (expect 400 {"error":"A service with this name already exists."})
curl -s -b cookies.txt -H 'Content-Type: application/json' \
  -d '{"name":"Web API"}' \
  "http://localhost:8080/api/workspaces/$WS/projects/$PID/services/"

# position patch (expect 200, position echoed)
curl -s -b cookies.txt -X PATCH -H 'Content-Type: application/json' \
  -d '{"position":{"x":120,"y":40}}' \
  "http://localhost:8080/api/workspaces/$WS/projects/$PID/services/$SID/"

# clear position (expect 200, position null)
curl -s -b cookies.txt -X PATCH -H 'Content-Type: application/json' \
  -d '{"position":null}' \
  "http://localhost:8080/api/workspaces/$WS/projects/$PID/services/$SID/"

# self dependency (expect 400 "A service cannot depend on itself.")
curl -s -b cookies.txt -H 'Content-Type: application/json' \
  -d "{\"from_service_id\":\"$SID\",\"to_service_id\":\"$SID\"}" \
  "http://localhost:8080/api/workspaces/$WS/projects/$PID/service-dependencies/"
```
Expected: the status codes and bodies noted in each comment.

- [ ] **Step 3: Manual UI checklist**

With `pnpm dev` running, open the project's Services page and confirm:
- List / Grid / Graph render from the backend (no console errors).
- Create, edit, delete a service.
- Drag a graph node, refresh, position persists; **Re-layout** clears positions and re-runs dagre.
- Connecting a self/duplicate/cycle edge shows the correct error toast.
- Link and unlink a work item from the work-item detail and from the service **Work items** tab.

- [ ] **Step 4: Final format check**

Run: `pnpm --filter=web check:format`
Expected: PASS (run `pnpm fix:format` if it reports changes, then commit them).

- [ ] **Step 5: Commit any verification fixes**

```bash
git add -A
git commit -m "fix(services): verification follow-ups" || echo "nothing to commit"
```

---

## Notes for the implementer

- `service.rs` will import `Deserialize` (used by `CreateService`, `PatchService`, `CreateDependency`, `CreateServiceIssue`) and `Path`; confirm the top import block from Task 3 Step 1 is present before compiling.
- Do not remove `apps/web/core/services/service-mock.repository.ts` or `service.helpers.ts`; the mock stays as a dev reference and the helpers remain used by the store for client-side filter/sort.
- The backend returns 201 for create (the mock's `Error`-based contract does not care); the store reconciles from the response body.
- `PUT` is intentionally aliased to `patch`.
