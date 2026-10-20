# Service Typed Relationships Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give service dependencies a type (`runs_on`, `connects_to`, `replicates_to`, `depends_on`, `uses`, `other`) and show a grouped, clickable relationship list on service detail and peek.

**Architecture:** Add a `dependency_type` column to `service_dependencies` (default `depends_on`), expose it through create/list/PATCH on the canonical workspace endpoints and the compat project endpoints, then replace the sidebar dependency block with a `Relationships` section in the detail main column and peek. Inverse labels are derived on the client.

**Tech Stack:** Rust/axum/sqlx, PostgreSQL, MobX, React, TypeScript, Vitest, i18n JSON.

---

## File Structure

**Backend**

- Create `apps/api-rs/migrations/0023_service_dependency_type.sql`
- Modify `apps/api-rs/crates/api/src/routes/service.rs`
- Modify `apps/api-rs/crates/api/src/main.rs`
- Test `apps/api-rs/crates/api/tests/service_workspace_test.rs`

**Frontend data layer**

- Modify `packages/types/src/service/core.ts`
- Modify `apps/web/core/services/service.service.ts`
- Modify `apps/web/core/store/service.store.ts`
- Modify `apps/web/core/services/service.helpers.ts`
- Modify `apps/web/core/services/service-mock.repository.ts`
- Test `apps/web/core/services/service.helpers.test.ts`
- Test `apps/web/core/store/service.store.test.ts`

**Frontend UI**

- Create `apps/web/core/components/services/detail/relationships.tsx`
- Modify `apps/web/core/components/services/detail/root.tsx`
- Modify `apps/web/core/components/services/detail/sidebar.tsx`
- Modify `apps/web/core/components/services/detail/index.ts`
- Modify `apps/web/core/components/services/peek-overview/view.tsx`
- Delete `apps/web/core/components/services/detail/dependencies.tsx`

**i18n**

- Modify `packages/i18n/src/locales/en/service.json`
- Modify all 19 other `packages/i18n/src/locales/*/service.json` files

---

## Task 1: Backend schema, typed create, default `depends_on`

**Files:**

- Create: `apps/api-rs/migrations/0023_service_dependency_type.sql`
- Modify: `apps/api-rs/crates/api/src/routes/service.rs:986-1045` (`DependencyRow`, `dependency_json`, `DEPENDENCY_SELECT`, `CreateDependency`, `create_dependency_core`)
- Modify: `apps/api-rs/crates/api/src/routes/service.rs:1141-1165` (`dependencies_create_workspace`)
- Modify: `apps/api-rs/crates/api/src/routes/service.rs:1258-1282` (`dependencies_create`)
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write the failing test and update existing struct literals**

Append this test after `canonical_dependency_destroy_by_from_writer` in `apps/api-rs/crates/api/tests/service_workspace_test.rs`:

```rust
#[tokio::test]
async fn canonical_dependency_type_default_and_validation() {
    use api::routes::service::{dependencies_create_workspace, CreateDependency};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // P1 must see S2 as full before it can depend on it.
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;

    let make = |dependency_type: Option<&str>| CreateDependency {
        from_service_id: sc.s1,
        to_service_id: sc.s2,
        dependency_type: dependency_type.map(str::to_string),
    };

    // Omitted type defaults to depends_on.
    let (status, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(None)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(edge["dependency_type"], "depends_on");

    // Same pair with a different type is allowed.
    let (status, Json(uses)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(Some("uses"))),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(uses["dependency_type"], "uses");

    // Same triple is a duplicate.
    let duplicate = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(Some("uses"))),
    )
    .await
    .unwrap();
    assert_eq!(duplicate.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(duplicate.1["error"], "This dependency already exists.");

    // Unknown type is rejected.
    let invalid = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(Some("eats"))),
    )
    .await
    .unwrap();
    assert_eq!(invalid.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(invalid.1["error"], "Invalid dependency type.");
}
```

Add `dependency_type: None,` to every existing `CreateDependency { ... }` literal in the same test file (6 occurrences). Example:

```rust
Json(CreateDependency {
    from_service_id: sc.s1,
    to_service_id: sc.s4,
    dependency_type: None,
}),
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test canonical_dependency_type_default_and_validation -- --test-threads=1
```

Expected: compile error mentioning `struct CreateDependency has no field named dependency_type`.

- [ ] **Step 3: Add the migration**

Create `apps/api-rs/migrations/0023_service_dependency_type.sql`:

```sql
-- Typed service dependency edges (design 2026-10-20).
-- Existing rows become depends_on. Constraint is recreated so this file stays
-- idempotent when applied manually before starting the API.

ALTER TABLE public.service_dependencies
    ADD COLUMN IF NOT EXISTS dependency_type varchar(20) NOT NULL DEFAULT 'depends_on';

ALTER TABLE public.service_dependencies
    DROP CONSTRAINT IF EXISTS service_dependencies_type_check;

ALTER TABLE public.service_dependencies
    ADD CONSTRAINT service_dependencies_type_check
    CHECK (dependency_type IN ('runs_on', 'connects_to', 'replicates_to', 'depends_on', 'uses', 'other'));

DROP INDEX IF EXISTS public.service_dependencies_pair_idx;

CREATE UNIQUE INDEX IF NOT EXISTS service_dependencies_typed_pair_idx
    ON public.service_dependencies (from_service_id, to_service_id, dependency_type)
    WHERE deleted_at IS NULL;
```

- [ ] **Step 4: Add the allowed-type constant and normalizer**

In `apps/api-rs/crates/api/src/routes/service.rs`, after `SERVICE_TYPES`:

```rust
/// Allowed `dependency_type` values (`packages/types/src/service/core.ts`).
pub const DEPENDENCY_TYPES: &[&str] = &[
    "runs_on",
    "connects_to",
    "replicates_to",
    "depends_on",
    "uses",
    "other",
];

/// Validate a dependency type, defaulting omitted values to `depends_on`.
pub fn normalize_dependency_type(value: Option<&str>) -> Result<&'static str, &'static str> {
    match value.unwrap_or("depends_on") {
        "runs_on" => Ok("runs_on"),
        "connects_to" => Ok("connects_to"),
        "replicates_to" => Ok("replicates_to"),
        "depends_on" => Ok("depends_on"),
        "uses" => Ok("uses"),
        "other" => Ok("other"),
        _ => Err("Invalid dependency type."),
    }
}
```

- [ ] **Step 5: Extend the row, serializer, select, and create payload**

In `DependencyRow` add:

```rust
pub dependency_type: String,
```

In `dependency_json` add to the `json!({ ... })` object:

```rust
"dependency_type": row.dependency_type,
```

Replace `DEPENDENCY_SELECT` with:

```rust
const DEPENDENCY_SELECT: &str = "SELECT id, workspace_id, project_id, from_service_id, \
    to_service_id, dependency_type, created_at FROM service_dependencies";
```

Replace `CreateDependency` with:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDependency {
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
    pub dependency_type: Option<String>,
}
```

- [ ] **Step 6: Thread the type through `create_dependency_core`**

Change the signature to add `dependency_type: &str` before `project_echo`:

```rust
pub(crate) async fn create_dependency_core(
    st: &AppState,
    user_id: Uuid,
    slug: &str,
    from_service_id: Uuid,
    to_service_id: Uuid,
    dependency_type: &str,
    project_echo: Option<Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
```

Replace the duplicate query bind block with:

```rust
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_dependencies WHERE workspace_id = $1 \
         AND from_service_id = $2 AND to_service_id = $3 AND dependency_type = $4 \
         AND deleted_at IS NULL)",
    )
    .bind(workspace_id)
    .bind(from_service_id)
    .bind(to_service_id)
    .bind(dependency_type)
    .fetch_one(&st.pool)
    .await?;
```

Replace the insert query with:

```rust
    sqlx::query(
        "INSERT INTO service_dependencies (id, workspace_id, project_id, from_service_id, \
         to_service_id, dependency_type, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, now(), now(), $7, $7)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(legacy_project)
    .bind(from_service_id)
    .bind(to_service_id)
    .bind(dependency_type)
    .bind(user_id)
    .execute(&st.pool)
    .await?;
```

- [ ] **Step 7: Validate and forward the type from both create handlers**

In `dependencies_create_workspace`, after the `full_visible` check:

```rust
    let dependency_type = match normalize_dependency_type(body.dependency_type.as_deref()) {
        Ok(value) => value,
        Err(message) => return Ok(bad_request(message)),
    };
```

Add `dependency_type,` to the `create_dependency_core(...)` call.

In `dependencies_create` (compat), after the project gates:

```rust
    let dependency_type = match normalize_dependency_type(body.dependency_type.as_deref()) {
        Ok(value) => value,
        Err(message) => return Ok(bad_request(message)),
    };
```

Add `dependency_type,` to its `create_dependency_core(...)` call.

- [ ] **Step 8: Apply the migration to the local database**

Run from the repo root:

```bash
docker compose -f docker-compose-local.yml exec -T plane-db psql -U plane -d plane < apps/api-rs/migrations/0023_service_dependency_type.sql
```

Expected: `ALTER TABLE`, `DROP CONSTRAINT` (or skip), `DROP INDEX`, `CREATE INDEX`.

- [ ] **Step 9: Run the test until it passes**

Run from `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test canonical_dependency_type_default_and_validation -- --test-threads=1
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 10: Run the full service workspace target**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 11: Commit**

```bash
git add apps/api-rs/migrations/0023_service_dependency_type.sql \
  apps/api-rs/crates/api/src/routes/service.rs \
  apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): add dependency_type to service dependencies"
```

---

## Task 2: Backend PATCH endpoint for dependency type

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs` (after `dependency_destroy_workspace`, and after `dependency_destroy`)
- Modify: `apps/api-rs/crates/api/src/main.rs:715-722` and `:818-826`
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write the failing test**

Append after the Task 1 test:

```rust
#[tokio::test]
async fn canonical_dependency_patch_type() {
    use api::routes::service::{
        dependencies_create_workspace, dependency_update_workspace, CreateDependency,
        UpdateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;

    let (_, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    let edge_id = Uuid::parse_str(edge["id"].as_str().unwrap()).unwrap();

    let (status, Json(patched)) = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("runs_on".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(patched["dependency_type"], "runs_on");

    // Create a second edge, then collide the PATCH with it.
    dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: Some("uses".into()),
        }),
    )
    .await
    .unwrap();

    let conflict = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("uses".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(conflict.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(conflict.1["error"], "This dependency already exists.");

    let invalid = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("eats".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(invalid.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(invalid.1["error"], "Invalid dependency type.");

    // Outsider cannot patch.
    let denied = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("depends_on".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test canonical_dependency_patch_type -- --test-threads=1
```

Expected: compile error mentioning unresolved imports `dependency_update_workspace` / `UpdateDependency`.

- [ ] **Step 3: Add the patch payload and shared core**

In `apps/api-rs/crates/api/src/routes/service.rs`, after `CreateDependency`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateDependency {
    pub dependency_type: Option<String>,
}

async fn update_dependency_core(
    st: &AppState,
    user_id: Uuid,
    slug: &str,
    pk: Uuid,
    requested: Option<&str>,
    project_echo: Option<Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    if !service_access::gate_writer(&st.pool, user_id, slug, row.from_service_id).await? {
        return Ok(deny());
    }
    let Some(requested) = requested else {
        return Ok(bad_request("Invalid dependency type."));
    };
    if !DEPENDENCY_TYPES.contains(&requested) {
        return Ok(bad_request("Invalid dependency type."));
    }
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_dependencies WHERE workspace_id = $1 \
         AND from_service_id = $2 AND to_service_id = $3 AND dependency_type = $4 \
         AND deleted_at IS NULL AND id <> $5)",
    )
    .bind(row.workspace_id)
    .bind(row.from_service_id)
    .bind(row.to_service_id)
    .bind(requested)
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if duplicate {
        return Ok(bad_request("This dependency already exists."));
    }
    sqlx::query(
        "UPDATE service_dependencies SET dependency_type = $1, updated_at = now() \
         WHERE id = $2 AND deleted_at IS NULL",
    )
    .bind(requested)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    let updated: DependencyRow = sqlx::query_as(&format!("{DEPENDENCY_SELECT} WHERE id = $1"))
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
    let ws_admin = is_workspace_admin(&st.pool, user_id, slug).await?;
    let visible = if ws_admin {
        vec![]
    } else {
        service_access::visible_service_ids(&st.pool, user_id, slug).await?
    };
    Ok((
        StatusCode::OK,
        Json(dependency_json(&st.pool, &updated, &visible, ws_admin, project_echo).await?),
    ))
}
```

- [ ] **Step 4: Add both handlers**

After `dependency_destroy_workspace`:

```rust
pub async fn dependency_update_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
    Json(body): Json<UpdateDependency>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    update_dependency_core(&st, auth.0, &slug, pk, body.dependency_type.as_deref(), None).await
}
```

After `dependency_destroy`:

```rust
pub async fn dependency_update(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<UpdateDependency>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    if !service_access::linked_to_project(&st.pool, row.from_service_id, project_id).await? {
        return Ok(missing());
    }
    update_dependency_core(
        &st,
        auth.0,
        &slug,
        pk,
        body.dependency_type.as_deref(),
        Some(project_id),
    )
    .await
}
```

- [ ] **Step 5: Register the PATCH routes**

In `apps/api-rs/crates/api/src/main.rs`, replace the project dependency detail route:

```rust
        .route(
            "/api/workspaces/:slug/projects/:project_id/service-dependencies/:pk/",
            delete(routes::service::dependency_destroy)
                .patch(routes::service::dependency_update),
        )
```

Replace the workspace dependency detail route:

```rust
        .route(
            "/api/workspaces/:slug/service-dependencies/:pk/",
            delete(routes::service::dependency_destroy_workspace)
                .patch(routes::service::dependency_update_workspace),
        )
```

- [ ] **Step 6: Run the test until it passes**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test canonical_dependency_patch_type -- --test-threads=1
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 7: Run the full target again**

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs \
  apps/api-rs/crates/api/src/main.rs \
  apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): patch service dependency type"
```

---

## Task 3: Frontend types, service, store, helpers

**Files:**

- Modify: `packages/types/src/service/core.ts`
- Modify: `apps/web/core/services/service.service.ts:47-53,107-135`
- Modify: `apps/web/core/store/service.store.ts:12-20,43-50,75-90,145-180,215-225,450-475`
- Modify: `apps/web/core/services/service.helpers.ts:24-69`
- Modify: `apps/web/core/services/service-mock.repository.ts:166-200,290-315`
- Test: `apps/web/core/services/service.helpers.test.ts`
- Test: `apps/web/core/store/service.store.test.ts`

- [ ] **Step 1: Write the failing helper tests**

In `apps/web/core/services/service.helpers.test.ts`, change the import to:

```ts
import {
  dependencyExists,
  isLiteService,
  isValidServiceUrl,
  normalizeServiceUrl,
  validateDependency,
} from "./service.helpers";
```

Add:

```ts
describe("typed dependency validation", () => {
  const dependencies = [
    {
      id: "dep-1",
      workspace_id: "ws-1",
      project_id: null,
      from_service_id: "service-1",
      to_service_id: "service-2",
      dependency_type: "uses",
      created_at: "2026-10-20T00:00:00Z",
    },
  ] as const;

  it("treats the same pair with another type as a new edge", () => {
    expect(dependencyExists([...dependencies], "service-1", "service-2", "uses")).toBe(true);
    expect(dependencyExists([...dependencies], "service-1", "service-2", "depends_on")).toBe(false);
  });

  it("validates duplicates per type", () => {
    expect(validateDependency([...dependencies], "service-1", "service-2", "uses")).toBe(
      "This dependency already exists."
    );
    expect(validateDependency([...dependencies], "service-1", "service-2", "runs_on")).toBeNull();
  });
});
```

- [ ] **Step 2: Run the helper test to verify it fails**

```bash
pnpm --filter=web exec vitest run core/services/service.helpers.test.ts
```

Expected: FAIL — `dependencyExists(...)` receives an extra argument but the implementation ignores type, so the `depends_on` assertion is `true` instead of `false` (or TypeScript complains about the extra argument).

- [ ] **Step 3: Add the shared type**

In `packages/types/src/service/core.ts`, before `IServiceDependency`:

```ts
export type TServiceDependencyType = "runs_on" | "connects_to" | "replicates_to" | "depends_on" | "uses" | "other";
```

Add to `IServiceDependency`:

```ts
dependency_type: TServiceDependencyType;
```

- [ ] **Step 4: Make the service client branch on project scope**

In `apps/web/core/services/service.service.ts`, change the import:

```ts
import type {
  IService,
  IServiceDependency,
  IServiceProjectLink,
  TServiceDependencyType,
  TServiceWorkItemLink,
} from "@plane/types";
```

Add a private helper inside `ServiceService`:

```ts
  private dependencyUrl(workspaceSlug: string, projectId: string | null) {
    return projectId
      ? `/api/workspaces/${workspaceSlug}/projects/${projectId}/service-dependencies/`
      : `/api/workspaces/${workspaceSlug}/service-dependencies/`;
  }
```

Replace `createDependency` with:

```ts
  async createDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string | null,
    fromServiceId: string,
    toServiceId: string,
    dependencyType?: TServiceDependencyType
  ): Promise<IServiceDependency> {
    return this.post(this.dependencyUrl(workspaceSlug, projectId), {
      from_service_id: fromServiceId,
      to_service_id: toServiceId,
      dependency_type: dependencyType,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
```

Replace `deleteDependency` with:

```ts
  async deleteDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string | null,
    dependencyId: string
  ): Promise<void> {
    return this.delete(`${this.dependencyUrl(workspaceSlug, projectId)}${dependencyId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
```

Add after `deleteDependency`:

```ts
  async updateDependency(
    workspaceSlug: string,
    _workspaceId: string,
    projectId: string | null,
    dependencyId: string,
    dependencyType: TServiceDependencyType
  ): Promise<IServiceDependency> {
    return this.patch(`${this.dependencyUrl(workspaceSlug, projectId)}${dependencyId}/`, {
      dependency_type: dependencyType,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
```

Also change `getDependencies`'s `projectId` parameter to `string | null` if TypeScript needs it for the graph call; its URL always uses the project endpoint, so leave the parameter as `string`. No change.

- [ ] **Step 5: Update helpers**

In `apps/web/core/services/service.helpers.ts`, add `TServiceDependencyType` to the type import and add:

```ts
export const SERVICE_DEPENDENCY_TYPES = [
  "depends_on",
  "uses",
  "runs_on",
  "connects_to",
  "replicates_to",
  "other",
] as const satisfies readonly TServiceDependencyType[];
```

Replace `dependencyExists` and `validateDependency` with:

```ts
export const dependencyExists = (
  dependencies: IServiceDependency[],
  from: string,
  to: string,
  type: TServiceDependencyType
): boolean =>
  dependencies.some((d) => d.from_service_id === from && d.to_service_id === to && d.dependency_type === type);

export const validateDependency = (
  dependencies: IServiceDependency[],
  from: string,
  to: string,
  type: TServiceDependencyType
): string | null => {
  if (isSelfDependency(from, to)) return "A service cannot depend on itself.";
  if (dependencyExists(dependencies, from, to, type)) return "This dependency already exists.";
  if (wouldCreateCycle(dependencies, from, to)) return "This dependency would create a cycle.";
  return null;
};
```

- [ ] **Step 6: Update the store**

In `apps/web/core/store/service.store.ts`, add `TServiceDependencyType` to the type import. Add to `IServiceStore`:

```ts
  getDependenciesByService: (serviceId: string) => IServiceDependency[];
```

Change the three dependency method signatures:

```ts
addDependency: (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  fromServiceId: string,
  toServiceId: string,
  dependencyType?: TServiceDependencyType
) => Promise<IServiceDependency>;
updateDependency: (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  dependencyId: string,
  dependencyType: TServiceDependencyType
) => Promise<IServiceDependency>;
removeDependency: (workspaceSlug: string, workspaceId: string, projectId: string | null, dependencyId: string) =>
  Promise<void>;
```

Add `updateDependency: action,` next to `addDependency: action,` in `makeObservable`.

Add this computed getter after `getDependenciesByProject`:

```ts
getDependenciesByService = computedFn((serviceId: string) =>
  Object.values(this.dependencyMap).filter((d) => d.from_service_id === serviceId || d.to_service_id === serviceId)
);
```

Replace `addDependency`, `removeDependency`, and add `updateDependency`:

```ts
addDependency = async (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  fromServiceId: string,
  toServiceId: string,
  dependencyType: TServiceDependencyType = "depends_on"
) => {
  const dependency = await this.serviceService.createDependency(
    workspaceSlug,
    workspaceId,
    projectId,
    fromServiceId,
    toServiceId,
    dependencyType
  );
  runInAction(() => {
    set(this.dependencyMap, [dependency.id], dependency);
  });
  return dependency;
};

updateDependency = async (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  dependencyId: string,
  dependencyType: TServiceDependencyType
) => {
  const dependency = await this.serviceService.updateDependency(
    workspaceSlug,
    workspaceId,
    projectId,
    dependencyId,
    dependencyType
  );
  runInAction(() => {
    set(this.dependencyMap, [dependency.id], dependency);
  });
  return dependency;
};

removeDependency = async (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  dependencyId: string
) => {
  await this.serviceService.deleteDependency(workspaceSlug, workspaceId, projectId, dependencyId);
  runInAction(() => {
    delete this.dependencyMap[dependencyId];
  });
};
```

- [ ] **Step 7: Update the mock repository**

In `apps/web/core/services/service-mock.repository.ts`, add `TServiceDependencyType` to the type import. Change `makeDep`:

```ts
const makeDep = (
  from: string,
  to: string,
  dependency_type: TServiceDependencyType = "depends_on"
): IServiceDependency => ({
  id: uid(),
  workspace_id: workspaceId,
  project_id: projectId,
  from_service_id: from,
  to_service_id: to,
  dependency_type,
  created_at: nowIso(),
});
```

Change `createDependency` to accept `dependencyType: TServiceDependencyType = "depends_on"`, pass it to `validateDependency(stored.dependencies, fromServiceId, toServiceId, dependencyType)`, and include `dependency_type: dependencyType` in the stored object.

Add after `createDependency`:

```ts
  updateDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    dependencyId: string,
    dependencyType: TServiceDependencyType
  ): IServiceDependency {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    const dependency = stored.dependencies.find((d) => d.id === dependencyId);
    if (!dependency) throw new Error("Dependency not found.");
    const duplicate = stored.dependencies.some(
      (d) =>
        d.id !== dependencyId &&
        d.from_service_id === dependency.from_service_id &&
        d.to_service_id === dependency.to_service_id &&
        d.dependency_type === dependencyType
    );
    if (duplicate) throw new Error("This dependency already exists.");
    const updated = { ...dependency, dependency_type: dependencyType };
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      dependencies: stored.dependencies.map((d) => (d.id === dependencyId ? updated : d)),
    });
    return updated;
  }
```

- [ ] **Step 8: Add the store test**

In `apps/web/core/store/service.store.test.ts`, add `updateDependency` to the mock:

```ts
const serviceService = {
  deleteService: vi.fn(async () => undefined),
  updateDependency: vi.fn(
    async (
      _slug: string,
      _workspaceId: string,
      _projectId: string | null,
      dependencyId: string,
      dependencyType: string
    ) => ({
      id: dependencyId,
      workspace_id: "workspace-1",
      project_id: null,
      from_service_id: "service-1",
      to_service_id: "service-2",
      dependency_type: dependencyType,
      created_at: "2026-10-20T00:00:00Z",
    })
  ),
};
```

Add this test:

```ts
describe("ServicesStore.updateDependency", () => {
  it("updates the dependency type in place", async () => {
    const { store, serviceService } = makeStore();
    store.dependencyMap = {
      "dep-1": {
        id: "dep-1",
        workspace_id: "workspace-1",
        project_id: null,
        from_service_id: "service-1",
        to_service_id: "service-2",
        dependency_type: "depends_on",
        created_at: "2026-10-20T00:00:00Z",
      },
    };

    await store.updateDependency("acme", "workspace-1", null, "dep-1", "runs_on");

    expect(serviceService.updateDependency).toHaveBeenCalledWith("acme", "workspace-1", null, "dep-1", "runs_on");
    expect(store.dependencyMap["dep-1"].dependency_type).toBe("runs_on");
  });
});
```

- [ ] **Step 9: Run the frontend tests**

```bash
pnpm --filter=web exec vitest run core/services/service.helpers.test.ts core/store/service.store.test.ts
```

Expected: all tests pass.

- [ ] **Step 10: Update graph call sites for the nullable project scope**

In `apps/web/core/components/services/graph/service-graph.tsx`, keep `pid` for layout and node drag, and add:

```ts
const projectScope = projectIdOverride ?? projectId?.toString() ?? null;
```

Replace the `handleConnect` guard and call:

```ts
      if (!slug || !workspaceId) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not create dependency. Please try again.",
        });
        return;
      }
      try {
        await addDependency(slug, workspaceId, projectScope, source, target);
```

Update its dependency array to:

```ts
[addDependency, projectScope, slug, workspaceId];
```

Replace the `handleEdgeDelete` guard and call:

```ts
      if (!slug || !workspaceId) return;
      void removeDependency(slug, workspaceId, projectScope, dependencyId).catch(() => {
```

Update its dependency array to:

```ts
[projectScope, removeDependency, slug, workspaceId];
```

- [ ] **Step 11: Typecheck**

```bash
pnpm --filter=web exec tsc --noEmit -p tsconfig.json
```

Expected: no errors.

- [ ] **Step 12: Commit**

```bash
git add packages/types/src/service/core.ts \
  apps/web/core/services/service.service.ts \
  apps/web/core/store/service.store.ts \
  apps/web/core/services/service.helpers.ts \
  apps/web/core/services/service-mock.repository.ts \
  apps/web/core/services/service.helpers.test.ts \
  apps/web/core/store/service.store.test.ts \
  apps/web/core/components/services/graph/service-graph.tsx
git commit -m "feat(web): typed service dependency data layer"
```

---

## Task 4: Relationships UI

**Files:**

- Create: `apps/web/core/components/services/detail/relationships.tsx`
- Modify: `apps/web/core/components/services/detail/root.tsx`
- Modify: `apps/web/core/components/services/detail/sidebar.tsx`
- Modify: `apps/web/core/components/services/detail/index.ts`
- Modify: `apps/web/core/components/services/peek-overview/view.tsx`
- Delete: `apps/web/core/components/services/detail/dependencies.tsx`

- [ ] **Step 1: Create the Relationships component**

Create `apps/web/core/components/services/detail/relationships.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { LockOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService, IServiceDependency, TServiceDependencyType } from "@plane/types";
import { ControlLink, CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { dependencyExists, isLiteService, SERVICE_DEPENDENCY_TYPES } from "@/services/service.helpers";

type Props = {
  serviceId: string;
};

type TDirection = "out" | "in";

type TGroup = {
  key: string;
  type: TServiceDependencyType;
  direction: TDirection;
  labelKey: string;
};

const GROUPS: TGroup[] = [
  { key: "depends_on_out", type: "depends_on", direction: "out", labelKey: "service.detail.depends_on" },
  { key: "depends_on_in", type: "depends_on", direction: "in", labelKey: "service.relationships.required_by" },
  { key: "uses_out", type: "uses", direction: "out", labelKey: "service.relationships.uses" },
  { key: "uses_in", type: "uses", direction: "in", labelKey: "service.relationships.used_by" },
  { key: "runs_on_out", type: "runs_on", direction: "out", labelKey: "service.relationships.runs_on" },
  { key: "runs_on_in", type: "runs_on", direction: "in", labelKey: "service.relationships.hosts" },
  { key: "connects_to_out", type: "connects_to", direction: "out", labelKey: "service.relationships.connects_to" },
  { key: "connects_to_in", type: "connects_to", direction: "in", labelKey: "service.relationships.connected_from" },
  {
    key: "replicates_to_out",
    type: "replicates_to",
    direction: "out",
    labelKey: "service.relationships.replicates_to",
  },
  {
    key: "replicates_to_in",
    type: "replicates_to",
    direction: "in",
    labelKey: "service.relationships.replicated_from",
  },
  { key: "other_out", type: "other", direction: "out", labelKey: "service.relationships.related_to" },
  { key: "other_in", type: "other", direction: "in", labelKey: "service.relationships.related_to" },
];

const TYPE_LABEL_KEYS: Record<TServiceDependencyType, string> = {
  depends_on: "service.detail.depends_on",
  uses: "service.relationships.uses",
  runs_on: "service.relationships.runs_on",
  connects_to: "service.relationships.connects_to",
  replicates_to: "service.relationships.replicates_to",
  other: "service.relationships.related_to",
};

export const ServiceRelationships = observer(function ServiceRelationships(props: Props) {
  const { serviceId } = props;
  // states
  const [selectedType, setSelectedType] = useState<TServiceDependencyType>("depends_on");
  const [selectedId, setSelectedId] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [updatingId, setUpdatingId] = useState<string | null>(null);
  // router
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getServiceById,
    getDependenciesByService,
    getProjectServiceIds,
    getWorkspaceServiceIds,
    addDependency,
    updateDependency,
    removeDependency,
  } = useService();
  const { getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const workspaceId = currentWorkspace?.id;
  const dependencyProjectId = projectId?.toString() ?? null;
  const dependencies = getDependenciesByService(serviceId);
  const selectedService = selectedId ? getServiceById(selectedId) : null;
  const candidateIds = projectId ? (getProjectServiceIds(projectId.toString()) ?? []) : getWorkspaceServiceIds();
  const candidates = candidateIds
    .map((id) => getServiceById(id))
    .filter((candidate): candidate is IService => Boolean(candidate) && candidate!.id !== serviceId)
    .filter((candidate) => !dependencyExists(dependencies, serviceId, candidate.id, selectedType));
  const canAdd = Boolean(selectedId) && !dependencyExists(dependencies, serviceId, selectedId, selectedType);

  const handleAdd = async () => {
    if (!slug || !workspaceId || !selectedId || !canAdd) return;
    setIsAdding(true);
    try {
      await addDependency(slug, workspaceId, dependencyProjectId, serviceId, selectedId, selectedType);
      setSelectedId("");
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: error instanceof Error ? error.message : t("service.detail.create_dependency_error"),
      });
    } finally {
      setIsAdding(false);
    }
  };

  const handleTypeChange = async (dependency: IServiceDependency, dependencyType: TServiceDependencyType) => {
    if (!slug || !workspaceId || (dependency.dependency_type ?? "depends_on") === dependencyType) return;
    setUpdatingId(dependency.id);
    try {
      await updateDependency(slug, workspaceId, dependencyProjectId, dependency.id, dependencyType);
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: error instanceof Error ? error.message : t("service.relationships.update_error"),
      });
    } finally {
      setUpdatingId(null);
    }
  };

  const handleRemove = async (dependencyId: string) => {
    if (!slug || !workspaceId) return;
    try {
      await removeDependency(slug, workspaceId, dependencyProjectId, dependencyId);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.delete_dependency_error"),
      });
    }
  };

  const renderRow = (dependency: IServiceDependency, direction: TDirection) => {
    const otherId = direction === "out" ? dependency.to_service_id : dependency.from_service_id;
    const embedded = dependency.from_service_id === otherId ? dependency.from_service : dependency.to_service;
    const other = getServiceById(otherId);
    const name = embedded?.name ?? other?.name ?? otherId;
    const lite = Boolean(embedded && isLiteService(embedded));
    const status = embedded?.status ?? other?.status;
    const criticality = embedded?.criticality ?? other?.criticality;
    const ownerProject = other?.owner_project_id ? getProjectById(other.owner_project_id) : undefined;
    const dependencyType = dependency.dependency_type ?? "depends_on";
    const href = slug ? `/${slug}/services/${otherId}` : "#";

    return (
      <div
        key={dependency.id}
        className="flex items-center justify-between gap-2 rounded-sm border border-subtle px-2 py-1.5"
      >
        <span className="flex min-w-0 items-center gap-1.5" title={name}>
          {lite && <LockOutline className="h-3 w-3 shrink-0 text-tertiary" />}
          <ControlLink
            href={href}
            onClick={() => router.push(href)}
            className={cn("truncate text-12", lite ? "text-tertiary" : "text-primary")}
          >
            {name}
          </ControlLink>
          {lite && <span className="shrink-0 text-10 text-tertiary">{t("service.lite.restricted")}</span>}
          {!lite && status && (
            <span className="shrink-0 rounded-xs border border-subtle px-1 text-10 uppercase text-tertiary">
              {t(`service.status_values.${status}`)}
            </span>
          )}
          {!lite && criticality && (
            <span className="shrink-0 rounded-xs border border-subtle px-1 text-10 uppercase text-tertiary">
              {t(`service.criticality_values.${criticality}`)}
            </span>
          )}
          {!lite && ownerProject && (
            <span
              className="shrink-0 rounded-xs border border-subtle px-1 text-10 font-medium tracking-wide text-tertiary"
              title={ownerProject.name}
            >
              {ownerProject.identifier}
            </span>
          )}
        </span>
        {!lite && (
          <span className="flex shrink-0 items-center gap-2">
            <CustomSelect
              value={dependencyType}
              onChange={(value: TServiceDependencyType) => void handleTypeChange(dependency, value)}
              disabled={updatingId === dependency.id}
              noChevron
              label={<span className="text-11 text-tertiary">{t(TYPE_LABEL_KEYS[dependencyType])}</span>}
            >
              {SERVICE_DEPENDENCY_TYPES.map((type) => (
                <CustomSelect.Option key={type} value={type}>
                  {t(TYPE_LABEL_KEYS[type])}
                </CustomSelect.Option>
              ))}
            </CustomSelect>
            <button
              type="button"
              onClick={() => void handleRemove(dependency.id)}
              className="text-11 text-tertiary hover:text-primary"
            >
              {t("remove")}
            </button>
          </span>
        )}
      </div>
    );
  };

  return (
    <div className="flex flex-col gap-3">
      <h4 className="text-13 font-medium text-secondary">{t("service.relationships.title")}</h4>
      <div className="flex items-center gap-2">
        <CustomSelect
          value={selectedType}
          onChange={(value: TServiceDependencyType) => setSelectedType(value)}
          className="h-7.5 w-[160px] shrink-0"
          label={<span className="truncate text-12">{t(TYPE_LABEL_KEYS[selectedType])}</span>}
        >
          {SERVICE_DEPENDENCY_TYPES.map((type) => (
            <CustomSelect.Option key={type} value={type}>
              {t(TYPE_LABEL_KEYS[type])}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
        <CustomSelect
          value={selectedId}
          onChange={(value: string) => setSelectedId(value)}
          className="h-7.5 min-w-0 grow basis-0"
          label={
            <span className="flex items-center gap-2 py-0.5 text-12">
              {selectedService ? (
                <span className="truncate">{selectedService.name}</span>
              ) : (
                <span className="text-secondary">{t("service.detail.select_service")}</span>
              )}
            </span>
          }
        >
          {candidates.map((candidate) => (
            <CustomSelect.Option key={candidate.id} value={candidate.id}>
              <span className="truncate">{candidate.name}</span>
            </CustomSelect.Option>
          ))}
        </CustomSelect>
        <Button variant="secondary" size="sm" onClick={handleAdd} disabled={!canAdd} loading={isAdding}>
          {t("add")}
        </Button>
      </div>
      {dependencies.length === 0 ? (
        <p className="text-12 text-tertiary">{t("service.relationships.empty")}</p>
      ) : (
        <div className="flex flex-col gap-4">
          {GROUPS.map((group) => {
            const rows = dependencies.filter((dependency) => {
              const type = dependency.dependency_type ?? "depends_on";
              return (
                type === group.type &&
                (group.direction === "out"
                  ? dependency.from_service_id === serviceId
                  : dependency.to_service_id === serviceId)
              );
            });
            if (rows.length === 0) return null;
            return (
              <div key={group.key} className="flex flex-col gap-1.5">
                <p className="text-12 font-medium text-tertiary">{t(group.labelKey)}</p>
                {rows.map((dependency) => renderRow(dependency, group.direction))}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
});
```

- [ ] **Step 2: Wire it into the detail root**

In `apps/web/core/components/services/detail/root.tsx`, add:

```tsx
import { ServiceRelationships } from "./relationships";
```

Render it after `<ServiceProjectLinks serviceId={serviceId} />`:

```tsx
<ServiceRelationships serviceId={serviceId} />
```

- [ ] **Step 3: Wire it into the peek**

In `apps/web/core/components/services/peek-overview/view.tsx`, add:

```tsx
import { ServiceRelationships } from "../detail/relationships";
```

Add after `<ServiceDescription ... />` inside `mainContent`:

```tsx
<ServiceRelationships serviceId={serviceId} />
```

- [ ] **Step 4: Remove the old sidebar dependency block**

In `apps/web/core/components/services/detail/sidebar.tsx`:

- Delete `import { ServiceDependencies } from "./dependencies";`
- Delete the block:

```tsx
<div className="mt-5 border-t border-subtle pt-4 pb-5">
  <h5 className="text-body-xs-medium">{t("common.dependencies")}</h5>
  <ServiceDependencies serviceId={serviceId} />
</div>
```

- [ ] **Step 5: Update the barrel and delete the old component**

In `apps/web/core/components/services/detail/index.ts`, replace:

```ts
export * from "./dependencies";
```

with:

```ts
export * from "./relationships";
```

Delete `apps/web/core/components/services/detail/dependencies.tsx`.

- [ ] **Step 6: Typecheck and lint**

```bash
pnpm --filter=web exec tsc --noEmit -p tsconfig.json
pnpm check:lint
```

Expected: no TypeScript errors; lint reports 0 errors (warnings are acceptable).

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/services/detail/relationships.tsx \
  apps/web/core/components/services/detail/root.tsx \
  apps/web/core/components/services/detail/sidebar.tsx \
  apps/web/core/components/services/detail/index.ts \
  apps/web/core/components/services/peek-overview/view.tsx
git rm apps/web/core/components/services/detail/dependencies.tsx
git commit -m "feat(web): show grouped typed service relationships"
```

---

## Task 5: i18n keys in every locale

**Files:**

- Modify: `packages/i18n/src/locales/*/service.json` (20 locales)

- [ ] **Step 1: Add the English block**

In `packages/i18n/src/locales/en/service.json`, add after the closing `"detail": { ... },` block:

```json
    "relationships": {
      "title": "Relationships",
      "empty": "No relationships yet",
      "update_error": "Could not update relationship. Please try again.",
      "required_by": "Required by",
      "uses": "Uses",
      "used_by": "Used by",
      "runs_on": "Runs on",
      "hosts": "Hosts",
      "connects_to": "Connects to",
      "connected_from": "Connected from",
      "replicates_to": "Replicates to",
      "replicated_from": "Replicated from",
      "related_to": "Related to"
    },
```

- [ ] **Step 2: Add the same block to the other 19 locales**

Insert the block in the same position (after `"detail"`) in each file. Use these values in the order `title | empty | update_error | required_by | uses | used_by | runs_on | hosts | connects_to | connected_from | replicates_to | replicated_from | related_to`:

| Locale  | Values                                                                                                                                                                                                                                                                                  |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cs`    | Vztahy \| Zatím žádné vztahy \| Vztah se nepodařilo aktualizovat. Zkuste to prosím znovu. \| Vyžadováno \| Používá \| Využíváno \| Běží na \| Hostuje \| Připojuje se k \| Připojeno z \| Replikuje na \| Replikováno z \| Související                                                  |
| `de`    | Beziehungen \| Noch keine Beziehungen \| Die Beziehung konnte nicht aktualisiert werden. Bitte versuchen Sie es erneut. \| Wird benötigt von \| Verwendet \| Verwendet von \| Läuft auf \| Hostet \| Verbindet mit \| Verbunden von \| Repliziert auf \| Repliziert von \| Verwandt mit |
| `es`    | Relaciones \| Aún no hay relaciones \| No se pudo actualizar la relación. Inténtelo de nuevo. \| Requerido por \| Usa \| Usado por \| Se ejecuta en \| Aloja \| Se conecta a \| Conectado desde \| Replica en \| Replicado desde \| Relacionado con                                     |
| `fr`    | Relations \| Aucune relation pour le moment \| La relation n'a pas pu être mise à jour. Veuillez réessayer. \| Requis par \| Utilise \| Utilisé par \| S'exécute sur \| Héberge \| Se connecte à \| Connecté depuis \| Réplique vers \| Répliqué depuis \| Lié à                        |
| `id`    | Hubungan \| Belum ada hubungan \| Hubungan tidak dapat diperbarui. Silakan coba lagi. \| Dibutuhkan oleh \| Menggunakan \| Digunakan oleh \| Berjalan di atas \| Menghosting \| Terhubung ke \| Terhubung dari \| Mereplikasi ke \| Direplikasi dari \| Terkait dengan                  |
| `it`    | Relazioni \| Ancora nessuna relazione \| Impossibile aggiornare la relazione. Riprovi. \| Richiesto da \| Utilizza \| Utilizzato da \| Eseguito su \| Ospita \| Si connette a \| Connesso da \| Replica su \| Replicato da \| Correlato a                                               |
| `ja`    | 関係 \| 関係はまだありません \| 関係を更新できませんでした。もう一度お試しください。 \| 必要とされています \| 使用 \| 使用されています \| 実行環境 \| ホスト \| 接続先 \| 接続元 \| 複製先 \| 複製元 \| 関連                                                                            |
| `ka-ge` | ურთიერთობები \| ჯერ არ არის ურთიერთობები \| ურთიერთობის განახლება ვერ მოხერხდა. გთხოვთ, სცადოთ ხელახლა. \| საჭიროებს \| იყენებს \| გამოიყენება \| მუშაობს \| მასპინძლობს \| უკავშირდება \| დაკავშირებულია \| ამრავლებს \| რეპლიცირებულია \| დაკავშირებული                               |
| `ko`    | 관계 \| 아직 관계가 없습니다 \| 관계를 업데이트하지 못했습니다. 다시 시도해 주세요. \| 필요로 함 \| 사용 \| 사용됨 \| 실행 위치 \| 호스트 \| 연결 대상 \| 연결 출처 \| 복제 대상 \| 복제 출처 \| 관련                                                                                   |
| `pl`    | Relacje \| Brak relacji \| Nie udało się zaktualizować relacji. Spróbuj ponownie. \| Wymagane przez \| Używa \| Używane przez \| Działa na \| Hostuje \| Łączy się z \| Połączone z \| Replikuje do \| Replikowane z \| Powiązane                                                       |
| `pt-BR` | Relações \| Ainda não há relações \| Não foi possível atualizar a relação. Tente novamente. \| Requerido por \| Usa \| Usado por \| Executa em \| Hospeda \| Conecta-se a \| Conectado de \| Replica para \| Replicado de \| Relacionado a                                              |
| `ro`    | Relații \| Încă nu există relații \| Relația nu a putut fi actualizată. Vă rugăm să încercați din nou. \| Necesar pentru \| Utilizează \| Utilizat de \| Rulează pe \| Găzduiește \| Se conectează la \| Conectat de la \| Replică către \| Replicat de la \| Legat de                  |
| `ru`    | Связи \| Связей пока нет \| Не удалось обновить связь. Попробуйте ещё раз. \| Требуется для \| Использует \| Используется \| Работает на \| Размещает \| Подключается к \| Подключено из \| Реплицирует в \| Реплицировано из \| Связано с                                              |
| `sk`    | Vzťahy \| Zatiaľ žiadne vzťahy \| Vzťah sa nepodarilo aktualizovať. Skúste to prosím znova. \| Vyžadované \| Používa \| Používané \| Beží na \| Hostuje \| Pripája sa k \| Pripojené z \| Replikuje na \| Replikované z \| Súvisiace                                                    |
| `tr-TR` | İlişkiler \| Henüz ilişki yok \| İlişki güncellenemedi. Lütfen tekrar deneyin. \| Gerektiren \| Kullanır \| Kullanılıyor \| Üzerinde çalışır \| Barındırır \| Bağlanır \| Bağlantı kaynağı \| Çoğaltır \| Çoğaltıldığı yer \| İlgili                                                    |
| `ua`    | Зв'язки \| Ще немає зв'язків \| Не вдалося оновити зв'язок. Спробуйте ще раз. \| Потрібен для \| Використовує \| Використовується \| Працює на \| Розміщує \| Підключається до \| Підключено з \| Реплікує до \| Репліковано з \| Пов'язано з                                           |
| `vi-VN` | Mối quan hệ \| Chưa có mối quan hệ nào \| Không thể cập nhật mối quan hệ. Vui lòng thử lại. \| Được yêu cầu bởi \| Sử dụng \| Được sử dụng bởi \| Chạy trên \| Lưu trữ \| Kết nối đến \| Kết nối từ \| Sao chép đến \| Sao chép từ \| Liên quan đến                                     |
| `zh-CN` | 关系 \| 暂无关系 \| 无法更新关系。请重试。 \| 被需要 \| 使用 \| 被使用 \| 运行于 \| 托管 \| 连接到 \| 连接自 \| 复制到 \| 复制自 \| 相关                                                                                                                                                |
| `zh-TW` | 關係 \| 尚無關係 \| 無法更新關係。請再試一次。 \| 被需要 \| 使用 \| 被使用 \| 執行於 \| 託管 \| 連線到 \| 連線自 \| 複製到 \| 複製自 \| 相關                                                                                                                                            |

- [ ] **Step 3: Regenerate types and check sync**

```bash
pnpm --filter @plane/i18n run generate:types
pnpm --filter @plane/i18n run sync:check
```

Expected: `0 missing, 0 stale, 0 collisions`.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "feat(i18n): add service relationship labels"
```

---

## Task 6: Full verification and deployment

**Files:** none.

- [ ] **Step 1: Run backend tests**

From `apps/api-rs`:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 2: Run frontend tests and checks**

From the repo root:

```bash
pnpm --filter=web exec vitest run core/services/service.helpers.test.ts core/store/service.store.test.ts
pnpm check:types
pnpm check:lint
```

Expected: tests pass, typecheck clean, lint 0 errors.

- [ ] **Step 3: Rebuild and restart the web prod server**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 8
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/
```

Expected: `200`.

- [ ] **Step 4: Rebuild api-rs**

Per `AGENTS.md`, run detached and poll (link stage can take 10+ minutes):

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll:

```bash
tail -f /tmp/plane-api-build.log
```

Wait for the containers to become healthy, then verify:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8000/health
systemctl --user restart plane-live.service
sleep 10
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `200` for both.

- [ ] **Step 5: Manual smoke test**

1. Open `http://localhost:3000/<workspace>/services` and open a service detail.
2. Confirm existing dependencies appear under **Depends on**.
3. Add a **Runs on** relationship to another service.
4. Open the target service and confirm it shows **Hosts** with the source service.
5. Change the source row type to **Uses**; confirm the row moves to the Uses group and the target now shows **Used by**.
6. Remove the relationship; confirm both sides update.
7. Drag between two nodes in the graph; confirm a **Depends on** edge is created.
8. Open the service from the board to confirm the peek shows the Relationships section.

- [ ] **Step 6: If verification required a fix**

Return to the task that owns the file, apply the fix there, and stage only that file in the same commit. Never use `git add -A` in this repo because the working tree contains unrelated modifications.

---

## Notes for the implementer

- Run Rust DB tests with `-- --test-threads=1`; the scratch purge helpers are prefix-based and parallel runs wipe each other.
- The migration is safe to apply manually before the API restart: column/index creation is `IF NOT EXISTS` and the check constraint is dropped then re-added.
- `projectId === null` in the web service layer means "canonical workspace endpoint". Do not fall back to `service.project_id` for API calls; that fallback was the cross-project bug.
- Lint warnings are pre-existing; only errors block the task.
