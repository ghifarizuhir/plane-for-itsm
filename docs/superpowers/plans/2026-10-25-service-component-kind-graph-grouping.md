# Service Component Kind & Graph Grouping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `kind` dimension to services (service/database/server/…) and group the service graph by Project × Kind with live cluster frames.

**Architecture:** `kind` is a new additive enum column and API field, separate from the existing `type`. The backend validates it, returns it in service JSON, and clears a service's stored position when its kind changes. The web graph computes dagre per `(owner_project_id, kind)` group, packs groups into a grid, then draws cluster frames through React Flow's `ViewportPortal` from the final node positions.

**Tech Stack:** Rust/axum/sqlx (api-rs), TypeScript/React/MobX/React Flow/dagre/Vitest (web), Tailwind, i18n JSON locales.

**Spec:** `docs/superpowers/specs/2026-10-25-service-component-kind-graph-grouping-design.md`

---

### Task 0: Create the feature branch

**Files:** none

- [ ] **Step 1: Create branch from main**

```bash
git checkout main
git pull --ff-only
git checkout -b feat/service-kind-graph-grouping
```

Expected: `Switched to a new branch 'feat/service-kind-graph-grouping'`.

- [ ] **Step 2: Confirm clean tree**

```bash
git status --short
```

Expected: no output.

---

### Task 1: Add the `kind` column and shared Rust constant

**Files:**

- Create: `apps/api-rs/migrations/0025_service_kind.sql`
- Create: `apps/api-rs/crates/common/src/models/service.rs`
- Modify: `apps/api-rs/crates/common/src/models/mod.rs`

- [ ] **Step 1: Write the migration**

Create `apps/api-rs/migrations/0025_service_kind.sql`:

```sql
-- Component kind for services (design 2026-10-25). `kind` describes the
-- component class (database/server/...) while the existing "type" column keeps
-- describing the internal/external boundary. Existing rows default to service.
-- Idempotent so it can be applied manually before the API restarts.

ALTER TABLE public.services
    ADD COLUMN IF NOT EXISTS kind varchar(20) NOT NULL DEFAULT 'service';

ALTER TABLE public.services
    DROP CONSTRAINT IF EXISTS services_kind_check;

ALTER TABLE public.services
    ADD CONSTRAINT services_kind_check
    CHECK (kind IN ('service', 'database', 'server', 'cache', 'queue',
                    'storage', 'network', 'external_api', 'other'));
```

- [ ] **Step 2: Add the shared constant**

Create `apps/api-rs/crates/common/src/models/service.rs`:

```rust
//! Shared service catalog vocabulary.

pub const SERVICE_KINDS: [&str; 9] = [
    "service",
    "database",
    "server",
    "cache",
    "queue",
    "storage",
    "network",
    "external_api",
    "other",
];
```

Modify `apps/api-rs/crates/common/src/models/mod.rs`:

```rust
pub mod service;
```

Place it alphabetically with the other modules (after `project`, before `state`).

- [ ] **Step 3: Compile the shared crate**

Run:

```bash
cd apps/api-rs && cargo check -p common
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Apply the migration to the local database**

Run:

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane < apps/api-rs/migrations/0025_service_kind.sql
```

Expected output contains `ALTER TABLE`. If the DB container has a different name, use `docker ps --format '{{.Names}}' | rg plane-db` and substitute it.

Verify:

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -c "\d services" | rg kind
```

Expected: a `kind | character varying(20) | not null default 'service'::character varying` line.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/migrations/0025_service_kind.sql apps/api-rs/crates/common/src/models/service.rs apps/api-rs/crates/common/src/models/mod.rs
git commit -m "feat(api): add service kind column and shared vocabulary"
```

---

### Task 2: Expose `kind` through the service API

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write the failing integration test**

Add this test at the end of `apps/api-rs/crates/api/tests/service_workspace_test.rs`:

```rust
#[tokio::test]
async fn service_kind_create_patch_and_layout() {
    use api::routes::service::{
        create_workspace, patch_workspace, CreateService, CreateWorkspaceService, PatchService,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let new_service = |name: &str, kind: Option<&str>| CreateWorkspaceService {
        owner_project_id: sc.p1,
        service: CreateService {
            name: Some(name.into()),
            description: None,
            description_html: None,
            status: None,
            criticality: None,
            service_type: None,
            kind: kind.map(str::to_string),
            owner_id: None,
            repository_url: None,
            documentation_url: None,
        },
    };

    let backfilled: String = sqlx::query_scalar("SELECT kind FROM services WHERE id = $1")
        .bind(sc.s1)
        .fetch_one(&st.pool)
        .await
        .expect("kind backfill");
    assert_eq!(backfilled, "service", "existing rows backfill to service");

    let (status, axum::Json(defaulted)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Default Kind", None)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(defaulted["kind"], "service");

    let (status, axum::Json(database)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Database Kind", Some("database"))),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(database["kind"], "database");
    let database_id = Uuid::parse_str(database["id"].as_str().unwrap()).unwrap();

    let (status, axum::Json(invalid)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Invalid Kind", Some("legacy"))),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(invalid["error"], "Invalid kind");

    // Create resets workspace positions, so seed them after creating.
    set_service_position(&st.pool, sc.s1, 10.0, 20.0).await;
    set_service_position(&st.pool, sc.s2, 30.0, 40.0).await;
    let s2_position_before = service_position(&st.pool, sc.s2).await;

    let (status, axum::Json(patched)) = patch_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), database_id)),
        Json(PatchService {
            kind: Some("server".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(patched["kind"], "server");
    assert_eq!(
        service_position(&st.pool, database_id).await,
        None,
        "kind change clears that service's position"
    );
    assert_eq!(
        service_position(&st.pool, sc.s2).await,
        s2_position_before,
        "kind change must not clear other services' positions"
    );

    // A non-kind patch keeps the stored position.
    set_service_position(&st.pool, database_id, 12.0, 34.0).await;
    let (status, axum::Json(type_patched)) = patch_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), database_id)),
        Json(PatchService {
            service_type: Some("external".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(type_patched["type"], "external");
    assert_eq!(
        service_position(&st.pool, database_id).await,
        Some(serde_json::json!({"x": 12.0, "y": 34.0}))
    );

    let (status, axum::Json(invalid_patch)) = patch_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), database_id)),
        Json(PatchService {
            kind: Some("legacy".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(invalid_patch["error"], "Invalid kind");
    assert_eq!(service_position(&st.pool, database_id).await, Some(serde_json::json!({"x": 12.0, "y": 34.0})));
}
```

Then add `kind: None,` to the two existing `CreateService` literals in this file:

- `canonical_create_patch_destroy` (around line 484, inside `new_service`)
- `service_mutations_reset_workspace_layout` (around line 612, inside `new_service`)

The struct literal should look like:

```rust
        service: CreateService {
            name: Some(name.into()),
            description: None,
            description_html: None,
            status: None,
            criticality: None,
            service_type: None,
            kind: None,
            owner_id: None,
            repository_url: None,
            documentation_url: None,
        },
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_workspace_test service_kind_create_patch_and_layout -- --test-threads=1
```

Expected: compile error `no field kind on struct CreateService` or assertion failure.

- [ ] **Step 3: Implement the API changes**

In `apps/api-rs/crates/api/src/routes/service.rs`:

Add the import:

```rust
use common::models::service::SERVICE_KINDS;
```

Add `kind` to `ServiceRow` after `service_type`:

```rust
    pub service_type: String,
    pub kind: String,
```

Add `s.kind` to `SERVICE_SELECT`:

```rust
const SERVICE_SELECT: &str = "SELECT s.id, s.workspace_id, s.name, s.description, \
    s.description_html, s.status, s.criticality, s.\"type\" AS service_type, s.kind, s.owner_id, \
    s.repository_url, s.documentation_url, s.position, s.sort_order, s.created_at, s.updated_at, \
    s.created_by_id, s.updated_by_id FROM services s";
```

Add kind to `service_json` after `"type"`:

```rust
        "type": row.service_type,
        "kind": row.kind,
```

Add the field to both request structs after `service_type`:

```rust
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    pub kind: Option<String>,
```

In `create_service_core`, after `service_type` is resolved, add:

```rust
    let kind = body.kind.clone().unwrap_or_else(|| "service".to_string());
```

After the `type` validation, add:

```rust
    if let Err(e) = validate_enum("kind", &kind, &SERVICE_KINDS) {
        return Ok(bad_request(e));
    }
```

Replace the create INSERT and bindings with:

```rust
    sqlx::query(
        "INSERT INTO services (id, workspace_id, project_id, name, description, description_html, \
         status, criticality, \"type\", kind, owner_id, repository_url, documentation_url, position, \
         sort_order, created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NULL, $12, \
         now(), now(), $13, $13 FROM projects p WHERE p.id = $14",
    )
    .bind(id)
    .bind(&name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(&kind)
    .bind(body.owner_id)
    .bind(body.repository_url.clone())
    .bind(body.documentation_url.clone())
    .bind(next_sort_order(max_order))
    .bind(user_id)
    .bind(owner_project_id)
    .execute(&mut *tx)
    .await?;
```

In the patch handler, after `service_type` is resolved, add:

```rust
    let kind = body.kind.clone().unwrap_or_else(|| current.kind.clone());
```

After the `type` validation, add:

```rust
    if let Err(e) = validate_enum("kind", &kind, &SERVICE_KINDS) {
        return Ok(bad_request(e));
    }
    let kind_changed = kind != current.kind;
```

Replace the position resolution with:

```rust
    let position = if kind_changed {
        None
    } else {
        match body.position.clone() {
            Some(v) => v,
            None => current.position.clone(),
        }
    };
```

Replace the UPDATE and bindings with:

```rust
    sqlx::query(
        "UPDATE services SET name = $1, description = $2, description_html = $3, status = $4, \
         criticality = $5, \"type\" = $6, kind = $7, owner_id = $8, repository_url = $9, \
         documentation_url = $10, position = $11, updated_at = now(), updated_by_id = $12 \
         WHERE id = $13 AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&description)
    .bind(&description_html)
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(&kind)
    .bind(owner_id)
    .bind(&repository_url)
    .bind(&documentation_url)
    .bind(&position)
    .bind(user_id)
    .bind(pk)
    .execute(&st.pool)
    .await?;
```

- [ ] **Step 4: Run the test and the whole service suite**

Run the new test:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_workspace_test service_kind_create_patch_and_layout -- --test-threads=1
```

Expected: `1 passed`.

Then run the full suite:

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_workspace_test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): support service kind in CRUD"
```

---

### Task 3: Add `kind` to AI service tools

**Files:**

- Modify: `apps/api-rs/crates/ai/src/tools/services.rs`
- Modify: `apps/api-rs/crates/ai/src/tools/mutations.rs`
- Modify: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write failing AI tests**

In `apps/api-rs/crates/ai/src/tools/services.rs`, extend `service_allowlist_rejects_unknown_values` with:

```rust
        assert_eq!(
            service_allowlist(Some(" DATABASE "), &SERVICE_KINDS, "kind").unwrap(),
            Some("database".to_string())
        );
        assert!(service_allowlist(Some("legacy"), &SERVICE_KINDS, "kind").is_err());
```

Add the import at the top of the file:

```rust
use common::models::service::SERVICE_KINDS;
```

In `apps/api-rs/crates/ai/src/tools/mutations.rs`, add this test inside the existing test module:

```rust
    #[test]
    fn create_service_normalizes_kind() {
        let proposal = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Primary DB".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: None,
            kind: Some(" DATABASE ".into()),
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap();
        assert_eq!(proposal.kind.as_deref(), Some("database"));
        assert_eq!(
            serde_json::to_value(&proposal).unwrap()["kind"],
            serde_json::json!("database")
        );

        let invalid = create_service_proposal_from_args(CreateServiceArgs {
            project: "LTS".into(),
            name: "Primary DB".into(),
            description: None,
            status: None,
            criticality: None,
            service_type: None,
            kind: Some("legacy".into()),
            owner: None,
            repository_url: None,
            documentation_url: None,
        })
        .unwrap_err();
        assert!(invalid.to_string().contains("database"));
    }
```

Add kind assertions to `update_service_normalizes_clears_and_requires_a_change`:

```rust
        let kind_proposal = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Primary DB".into(),
            project: None,
            changes: UpdateServiceChanges {
                kind: Some(" SERVER ".into()),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(kind_proposal.changes.kind.as_deref(), Some("server"));

        let bad_kind = update_service_proposal_from_args(UpdateServiceArgs {
            service: "Primary DB".into(),
            project: None,
            changes: UpdateServiceChanges {
                kind: Some("legacy".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(bad_kind.to_string().contains("database"));
```

In `apps/api-rs/crates/api/tests/service_workspace_test.rs`, update the `args` helper inside `ai_list_services_respects_visibility`:

```rust
    let args = |query: Option<&str>| ListServicesArgs {
        query: query.map(str::to_string),
        status: None,
        criticality: None,
        service_type: None,
        kind: None,
        project: None,
        limit: None,
    };
```

Add this filter assertion after the first `let out = ...` visibility assertion:

```rust
    sqlx::query("UPDATE services SET kind = 'database' WHERE id = $1")
        .bind(sc.s1)
        .execute(&st.pool)
        .await
        .expect("set kind");
    let db_only = tool
        .call(
            &mut ToolContext::new(),
            ListServicesArgs {
                kind: Some("database".into()),
                ..args(None)
            },
        )
        .await
        .unwrap();
    assert!(db_only.contains("\"kind\":\"database\""));
    assert!(!db_only.contains("\"kind\":\"service\""));
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
cd apps/api-rs && cargo test -p ai --lib
```

Expected: compile errors for missing `kind` fields.

- [ ] **Step 3: Implement the AI changes**

In `apps/api-rs/crates/ai/src/tools/services.rs`:

Add the import:

```rust
use common::models::service::SERVICE_KINDS;
```

Add `kind` to `ServiceRow` after `service_type`:

```rust
    pub service_type: String,
    pub kind: String,
```

Add `sv.kind` after `sv."type" AS service_type` in all four SQL constants (`SERVICES_SQL`, `SERVICE_BY_ID_SQL`, `SERVICES_BY_NAME_PROJECT_SQL`, `SERVICES_BY_NAME_SQL`):

```sql
     sv.criticality, sv."type" AS service_type, sv.kind, \
```

In `SERVICES_SQL`, add the kind filter and shift the later parameters:

```sql
     AND ($7::text IS NULL OR sv.kind = $7) \
     AND ($8::text IS NULL OR p.identifier ILIKE $8 OR p.name ILIKE '%' || $8 || '%') \
     ORDER BY sv.name LIMIT $9
```

Update `services_json` to include kind:

```rust
                "type": row.service_type,
                "kind": row.kind,
```

Add to `ListServicesArgs`:

```rust
    /// One of: service, database, server, cache, queue, storage, network, external_api, other.
    pub kind: Option<String>,
```

Add the filter in `call`:

```rust
        let kind = service_allowlist(args.kind.as_deref(), &SERVICE_KINDS, "kind")?;
```

Bind it between `service_type` and `project`:

```rust
            .bind(&service_type)
            .bind(&kind)
            .bind(&project)
            .bind(limit)
```

Update the description to mention kind:

```rust
         filtered by name substring, lifecycle status, criticality, type, kind, and \
         project. Returns name, project, status, criticality, type, kind, and owner. \
```

In `apps/api-rs/crates/ai/src/tools/mutations.rs`:

Add the import:

```rust
use common::models::service::SERVICE_KINDS;
```

Add `kind` to `CreateServiceArgs` after `service_type`:

```rust
    /// One of: service, database, server, cache, queue, storage, network, external_api, other.
    pub kind: Option<String>,
```

Add `kind` to `CreateServiceProposal` after `service_type`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
```

In `create_service_proposal_from_args`, add:

```rust
    let kind = optional_enum_arg(args.kind.as_deref(), &SERVICE_KINDS, "kind")?;
```

and include `kind` in the returned struct.

Add `kind` to `UpdateServiceChanges` after `service_type`:

```rust
    /// New component kind: service, database, server, cache, queue, storage, network, external_api, other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
```

In `normalize_service_changes`, add:

```rust
        kind: optional_enum_arg(changes.kind.as_deref(), &SERVICE_KINDS, "kind")?,
```

Update the create tool description: replace `Status, criticality, type, owner, and URLs are optional.` with `Status, criticality, type, kind, owner, and URLs are optional.`

Update the update tool description so its list reads `status, criticality, type, kind, owner, description, or URLs`.

Add `kind: None,` to every remaining `CreateServiceArgs` test literal in this file (four literals around lines 2304–2385).

- [ ] **Step 4: Run the AI tests**

```bash
cd apps/api-rs && cargo test -p ai --lib
```

Expected: all pass.

- [ ] **Step 5: Run the API service suite**

```bash
cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_workspace_test -- --test-threads=1
```

Expected: all pass, including `ai_list_services_respects_visibility`.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/ai/src/tools/services.rs apps/api-rs/crates/ai/src/tools/mutations.rs apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(ai): support service kind in read and mutation tools"
```

---

### Task 4: Add web types, filter logic, and fixtures

**Files:**

- Modify: `packages/types/src/service/core.ts`
- Modify: `packages/types/src/service/filters.ts`
- Modify: `apps/web/core/services/service.helpers.ts`
- Modify: `apps/web/core/lib/workspace-services.test.ts`
- Modify: `apps/web/core/store/service.store.test.ts`
- Modify: `apps/web/core/services/service-mock.repository.ts`

- [ ] **Step 1: Write the failing filter test**

In `apps/web/core/lib/workspace-services.test.ts`, add `kind: "service"` to `makeService` defaults:

```ts
    criticality: "medium",
    type: "internal",
    kind: "service",
```

Add this test inside `describe("selectWorkspaceServiceIds", ...)`:

```ts
it("filters by component kind", () => {
  const selection = select(
    [
      makeService({ id: "db", name: "Primary DB", kind: "database" }),
      makeService({ id: "svc", name: "Payments", kind: "service" }),
    ],
    { filters: { kind: ["database"] } }
  );

  expect(selection).toEqual(["db"]);
});
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
pnpm --filter=web exec vitest run core/lib/workspace-services.test.ts
```

Expected: TypeScript/runtime failure because `kind` is not part of `IService`/`TServiceFilters`.

- [ ] **Step 3: Implement the types and filter**

In `packages/types/src/service/core.ts`, add after `TServiceType`:

```ts
export type TServiceKind =
  | "service"
  | "database"
  | "server"
  | "cache"
  | "queue"
  | "storage"
  | "network"
  | "external_api"
  | "other";
```

Add to `IService` after `type`:

```ts
kind: TServiceKind;
```

In `packages/types/src/service/filters.ts`, add after `type`:

```ts
  kind?: TServiceKind[];
```

and import `TServiceKind`:

```ts
import type { TServiceCriticality, TServiceHealth, TServiceKind, TServiceStatus, TServiceType } from "./core";
```

In `apps/web/core/services/service.helpers.ts`, add after the type filter:

```ts
if (filters.kind && filters.kind.length > 0 && !filters.kind.includes(service.kind)) return false;
```

- [ ] **Step 4: Update the remaining fixtures**

In `apps/web/core/store/service.store.test.ts`, add `kind: "service",` after `type: "internal",` in `makeService`.

In `apps/web/core/services/service-mock.repository.ts`:

- Add `kind` to the seed input type and every seed entry:

```ts
      kind: "service" as const,
```

Use `database` for `Postgres Primary`, `external_api` for `Email Provider`, and `service` for the rest.

- Add `kind: s.kind` to the mapped `IService` object.
- Add `kind: data.kind ?? "service",` to `createService` after `type`.

Example seed entries:

```ts
    {
      name: "Payment Gateway",
      status: "active",
      criticality: "critical",
      type: "internal",
      kind: "service",
      description: "Handles all card payments.",
      repo: "https://example.com/payment",
    },
    {
      name: "Postgres Primary",
      status: "active",
      criticality: "critical",
      type: "infrastructure",
      kind: "database",
      description: "Primary relational database.",
      repo: null,
    },
    {
      name: "Email Provider",
      status: "active",
      criticality: "high",
      type: "third_party",
      kind: "external_api",
      description: "External SMTP provider.",
      repo: null,
    },
```

- [ ] **Step 5: Run the test and typecheck**

```bash
pnpm --filter=web exec vitest run core/lib/workspace-services.test.ts core/store/service.store.test.ts
pnpm --filter=web check:types
```

Expected: tests pass, typecheck exits 0.

- [ ] **Step 6: Commit**

```bash
git add packages/types/src/service/core.ts packages/types/src/service/filters.ts \
  apps/web/core/services/service.helpers.ts apps/web/core/lib/workspace-services.test.ts \
  apps/web/core/store/service.store.test.ts apps/web/core/services/service-mock.repository.ts
git commit -m "feat(web): add service kind types and filtering"
```

---

### Task 5: Add kind to forms, sidebar, filters, AI proposal UI, and i18n

**Files:**

- Create: `apps/web/core/components/services/kind.ts`
- Modify: `apps/web/core/components/services/service-form.tsx`
- Modify: `apps/web/core/components/services/detail/sidebar.tsx`
- Create: `apps/web/core/components/services/filters/kind.tsx`
- Modify: `apps/web/core/components/services/filters/index.ts`
- Modify: `apps/web/core/components/services/filters/root.tsx`
- Modify: `apps/web/core/components/services/applied-filters/root.tsx`
- Modify: `apps/web/core/lib/ai-proposals.ts`
- Modify: `apps/web/core/lib/ai-proposal-forms.ts`
- Modify: `packages/i18n/src/locales/*/service.json`

- [ ] **Step 1: Add the pure kind order helper**

Create `apps/web/core/components/services/kind.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceKind } from "@plane/types";

export const SERVICE_KIND_ORDER: TServiceKind[] = [
  "service",
  "database",
  "server",
  "cache",
  "queue",
  "storage",
  "network",
  "external_api",
  "other",
];

export const serviceKindLabelKey = (kind: TServiceKind) => `service.kind_values.${kind}`;
```

- [ ] **Step 2: Add the form dropdown**

In `apps/web/core/components/services/service-form.tsx`:

Add the import:

```ts
import { SERVICE_KIND_ORDER } from "./kind";
```

Replace the option/type union block with:

```ts
const SERVICE_TYPE_OPTIONS: IService["type"][] = ["internal", "external", "infrastructure", "third_party"];

type TServiceOptionSelectProps = {
  control: Control<IService, any>;
  name: "status" | "criticality" | "type" | "kind";
  label: string;
};

function ServiceOptionSelect(props: TServiceOptionSelectProps) {
  const { control, name, label } = props;
  const { t } = useTranslation();
  const options =
    name === "status"
      ? SERVICE_STATUS_OPTIONS
      : name === "criticality"
        ? SERVICE_CRITICALITY_OPTIONS
        : name === "type"
          ? SERVICE_TYPE_OPTIONS
          : SERVICE_KIND_ORDER;
  const prefix =
    name === "status"
      ? "service.status_values"
      : name === "criticality"
        ? "service.criticality_values"
        : name === "type"
          ? "service.type_values"
          : "service.kind_values";
```

Add `kind: "service",` to `defaultValues` and to the `useForm` defaults:

```ts
const defaultValues: Partial<IService> = {
  name: "",
  description_html: "",
  status: "planned",
  criticality: "medium",
  type: "internal",
  kind: "service",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
};
```

```ts
      type: data?.type || "internal",
      kind: data?.kind || "service",
```

Add the dropdown after the type dropdown:

```tsx
<div className="h-7">
  <ServiceOptionSelect control={control} name="kind" label={t("service.fields.kind")} />
</div>
```

- [ ] **Step 3: Add the sidebar property**

In `apps/web/core/components/services/detail/sidebar.tsx`:

Add the import:

```ts
import { SERVICE_KIND_ORDER } from "../kind";
```

Remove the local `SERVICE_TYPE_OPTIONS` declaration and add:

```ts
const SERVICE_TYPE_OPTIONS: IService["type"][] = ["internal", "external", "infrastructure", "third_party"];
const SERVICE_KIND_OPTIONS: IService["kind"][] = SERVICE_KIND_ORDER;
```

Add the property after the type property:

```tsx
<SidebarPropertyListItem icon={ModuleOutline} label={t("service.fields.kind")}>
  <ServicePropertySelect
    value={service.kind}
    options={SERVICE_KIND_OPTIONS}
    i18nPrefix="service.kind_values"
    onChange={(val) => update({ kind: val })}
  />
</SidebarPropertyListItem>
```

- [ ] **Step 4: Add the filter component**

Create `apps/web/core/components/services/filters/kind.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useTranslation } from "@plane/i18n";
import type { TServiceKind } from "@plane/types";
// components
import { FilterHeader, FilterOption } from "@/components/issues/issue-layouts/filters";
// local imports
import { SERVICE_KIND_ORDER } from "../kind";

export const SERVICE_KIND_OPTIONS: { value: TServiceKind; i18n_label: string }[] = SERVICE_KIND_ORDER.map((value) => ({
  value,
  i18n_label: `service.kind_values.${value}`,
}));

type Props = {
  appliedFilters: TServiceKind[] | null;
  handleUpdate: (val: string) => void;
  searchQuery: string;
};

export const FilterServiceKind = observer(function FilterServiceKind(props: Props) {
  const { appliedFilters, handleUpdate, searchQuery } = props;
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const { t } = useTranslation();

  const filteredOptions = SERVICE_KIND_OPTIONS.filter((option) =>
    t(option.i18n_label).toLowerCase().includes(searchQuery.toLowerCase())
  );
  const appliedFiltersCount = appliedFilters?.length ?? 0;

  return (
    <>
      <FilterHeader
        title={`${t("service.fields.kind")}${appliedFiltersCount > 0 ? ` (${appliedFiltersCount})` : ""}`}
        isPreviewEnabled={previewEnabled}
        handleIsPreviewEnabled={() => setPreviewEnabled(!previewEnabled)}
      />
      {previewEnabled && (
        <div>
          {filteredOptions.length > 0 ? (
            filteredOptions.map((option) => (
              <FilterOption
                key={option.value}
                isChecked={appliedFilters?.includes(option.value) ?? false}
                onClick={() => handleUpdate(option.value)}
                title={t(option.i18n_label)}
              />
            ))
          ) : (
            <p className="text-11 text-placeholder italic">{t("common.search.no_matches_found")}</p>
          )}
        </div>
      )}
    </>
  );
});
```

Add to `apps/web/core/components/services/filters/index.ts`:

```ts
export * from "./kind";
```

In `apps/web/core/components/services/filters/root.tsx`:

Add `FilterServiceKind` to the import list and add this section after the type section:

```tsx
{
  /* component kind */
}
<div className="py-2">
  <FilterServiceKind
    appliedFilters={filters.kind ?? null}
    handleUpdate={(val) => handleFiltersUpdate("kind", val)}
    searchQuery={filtersSearchQuery}
  />
</div>;
```

- [ ] **Step 5: Add the applied-filter label**

In `apps/web/core/components/services/applied-filters/root.tsx`, add to `SERVICE_FILTER_VALUE_I18N`:

```ts
  kind: (value) => `service.kind_values.${value}`,
```

- [ ] **Step 6: Add kind to AI proposals**

In `apps/web/core/lib/ai-proposals.ts`:

Add after `SERVICE_TYPE_VALUES`:

```ts
export const SERVICE_KIND_VALUES = [
  "service",
  "database",
  "server",
  "cache",
  "queue",
  "storage",
  "network",
  "external_api",
  "other",
] as const;
```

Add `kind?: string | null;` to `TAiCreateServiceProposal` and `TAiUpdateServiceChanges`.

Add validation in `validateCreateServiceProposal` after the type check:

```ts
if (proposal.kind && !SERVICE_KIND_VALUES.includes(proposal.kind as (typeof SERVICE_KIND_VALUES)[number]))
  return "Unknown component kind.";
```

Add the analogous check in `validateUpdateServiceProposal`:

```ts
if (changes.kind && !SERVICE_KIND_VALUES.includes(changes.kind as (typeof SERVICE_KIND_VALUES)[number]))
  return "Unknown component kind.";
```

In `apps/web/core/lib/ai-proposal-forms.ts`:

Import `SERVICE_KIND_VALUES` and add the fields:

```ts
    { key: "kind", label: "Component kind", kind: "select", options: SERVICE_KIND_VALUES },
```

after the `type` field in both `create_service` and `update_service`.

In `buildServiceWrite`, add after the type assignment:

```ts
if (draftText(draft, "kind")) data.kind = draftText(draft, "kind") as IService["kind"];
```

- [ ] **Step 7: Add i18n keys to every locale**

Invoke the `translate` skill before editing locale files. Add these keys to all 20 files under `packages/i18n/src/locales/*/service.json`, **inside the top-level `service` object** (not at the file root):

English (`en`):

```json
{
  "fields": {
    "kind": "Component kind"
  },
  "kind_values": {
    "service": "Service",
    "database": "Database",
    "server": "Server",
    "cache": "Cache",
    "queue": "Queue",
    "storage": "Storage",
    "network": "Network",
    "external_api": "External API",
    "other": "Other"
  },
  "graph": {
    "unassigned": "Unassigned",
    "restricted": "Restricted"
  }
}
```

Indonesian (`id`):

```json
{
  "fields": {
    "kind": "Jenis komponen"
  },
  "kind_values": {
    "service": "Layanan",
    "database": "Basis data",
    "server": "Server",
    "cache": "Cache",
    "queue": "Antrean",
    "storage": "Penyimpanan",
    "network": "Jaringan",
    "external_api": "API eksternal",
    "other": "Lainnya"
  },
  "graph": {
    "unassigned": "Tanpa project",
    "restricted": "Terbatas"
  }
}
```

Use the translate skill's workflow for the remaining 18 locales. Keep `API` untranslated in `external_api`; adapt technical nouns to each locale's established service.json terminology. Do not add placeholder English values to non-English locales.

Verify all files parse:

```bash
for f in packages/i18n/src/locales/*/service.json; do jq -e '.service.kind_values.database and .service.fields.kind and .service.graph.unassigned' "$f" >/dev/null || echo "missing: $f"; done
```

Expected: no `missing:` lines.

- [ ] **Step 8: Typecheck and lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: both exit 0.

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/components/services/kind.ts \
  apps/web/core/components/services/service-form.tsx \
  apps/web/core/components/services/detail/sidebar.tsx \
  apps/web/core/components/services/filters/kind.tsx \
  apps/web/core/components/services/filters/index.ts \
  apps/web/core/components/services/filters/root.tsx \
  apps/web/core/components/services/applied-filters/root.tsx \
  apps/web/core/lib/ai-proposals.ts apps/web/core/lib/ai-proposal-forms.ts \
  packages/i18n/src/locales
git commit -m "feat(web): add component kind to forms, filters, and AI proposals"
```

---

### Task 6: Group graph layout by Project × Kind

**Files:**

- Create: `apps/web/core/components/services/graph/graph-groups.ts`
- Create: `apps/web/core/components/services/graph/graph-groups.test.ts`
- Modify: `apps/web/core/components/services/graph/use-graph-layout.ts`

- [ ] **Step 1: Write the failing layout tests**

Create `apps/web/core/components/services/graph/graph-groups.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ILiteService, IService } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { getServiceGroups } from "./graph-groups";
import {
  CLUSTER_HEADER_HEIGHT,
  CLUSTER_PADDING,
  getLayoutedElements,
  SERVICE_NODE_HEIGHT,
  SERVICE_NODE_WIDTH,
} from "./use-graph-layout";

const makeService = (id: string, overrides: Partial<IService> = {}): IService => ({
  id,
  workspace_id: "workspace-1",
  project_id: "p1",
  owner_project_id: "p1",
  project_links: [],
  access_level: "full",
  name: id,
  description: "",
  description_html: "",
  status: "active",
  criticality: "medium",
  type: "internal",
  kind: "service",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
  position: null,
  sort_order: 1,
  created_at: "2026-10-25T00:00:00Z",
  updated_at: "2026-10-25T00:00:00Z",
  created_by: null,
  updated_by: null,
  ...overrides,
});

const makeLite = (id: string): ILiteService => ({
  id,
  name: id,
  status: "active",
  criticality: "medium",
  access_level: "lite",
});

const getNodes = (services: (IService | ILiteService)[]) => getLayoutedElements(services, []).nodes;

const boundsFor = (
  nodes: ReturnType<typeof getNodes>,
  ids: string[]
): { x: number; y: number; width: number; height: number } => {
  const positions = nodes.filter((node) => ids.includes(node.id)).map((node) => node.position);
  const minX = Math.min(...positions.map((position) => position.x));
  const minY = Math.min(...positions.map((position) => position.y));
  const maxX = Math.max(...positions.map((position) => position.x + SERVICE_NODE_WIDTH));
  const maxY = Math.max(...positions.map((position) => position.y + SERVICE_NODE_HEIGHT));
  return {
    x: minX - CLUSTER_PADDING,
    y: minY - CLUSTER_HEADER_HEIGHT - CLUSTER_PADDING,
    width: maxX - minX + CLUSTER_PADDING * 2,
    height: maxY - minY + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2,
  };
};

const overlaps = (
  a: { x: number; y: number; width: number; height: number },
  b: { x: number; y: number; width: number; height: number }
) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

describe("getServiceGroups", () => {
  it("groups by project and kind, with unassigned and restricted fallbacks", () => {
    const groups = getServiceGroups([
      makeService("svc", { kind: "service" }),
      makeService("db", { kind: "database" }),
      makeService("db2", { owner_project_id: "p2", project_id: "p2", kind: "database" }),
      makeService("unassigned", { owner_project_id: null, project_id: null, kind: "server" }),
      makeLite("lite"),
    ]);

    expect(groups.map((group) => group.key)).toEqual([
      "p1::service",
      "p1::database",
      "p2::database",
      "__unassigned__::server",
      "__restricted__",
    ]);
    expect(groups.at(-1)?.restricted).toBe(true);
    expect(groups.find((group) => group.key === "__unassigned__::server")?.projectId).toBeNull();
  });
});

describe("grouped layout", () => {
  it("is deterministic", () => {
    const services = [makeService("a"), makeService("db", { kind: "database" })];

    expect(getNodes(services)).toEqual(getNodes(services));
  });

  it("packages different groups without overlapping", () => {
    const services = [
      makeService("p1-svc", { owner_project_id: "p1", project_id: "p1" }),
      makeService("p1-db", { owner_project_id: "p1", project_id: "p1", kind: "database" }),
      makeService("p2-svc", { owner_project_id: "p2", project_id: "p2" }),
      makeService("p2-db", { owner_project_id: "p2", project_id: "p2", kind: "database" }),
    ];
    const nodes = getNodes(services);

    const p1Service = boundsFor(nodes, ["p1-svc"]);
    const p1Db = boundsFor(nodes, ["p1-db"]);
    const p2Service = boundsFor(nodes, ["p2-svc"]);
    const p2Db = boundsFor(nodes, ["p2-db"]);

    expect(overlaps(p1Service, p1Db)).toBe(false);
    expect(overlaps(p1Service, p2Service)).toBe(false);
    expect(overlaps(p1Service, p2Db)).toBe(false);
    expect(overlaps(p1Db, p2Db)).toBe(false);
  });

  it("keeps a stored position", () => {
    const nodes = getNodes([makeService("db", { kind: "database", position: { x: 900, y: 700 } })]);

    expect(nodes.find((node) => node.id === "db")?.position).toEqual({ x: 900, y: 700 });
  });

  it("shifts unpositioned nodes toward the stored centroid of their group", () => {
    const nodes = getNodes([
      makeService("stored", { kind: "database", position: { x: 1000, y: 1000 } }),
      makeService("fresh", { kind: "database" }),
    ]);
    const fresh = nodes.find((node) => node.id === "fresh");

    expect(fresh).toBeDefined();
    expect(Math.abs((fresh?.position.x ?? 0) - 1000)).toBeLessThan(SERVICE_NODE_WIDTH * 2);
    expect(Math.abs((fresh?.position.y ?? 0) - 1000)).toBeLessThan(SERVICE_NODE_HEIGHT * 3);
  });

  it("keeps cross-group edges", () => {
    const from = makeService("from", { kind: "service" });
    const to = makeService("to", { kind: "database" });
    const { edges } = getLayoutedElements(
      [from, to],
      [
        {
          id: "dep-1",
          workspace_id: "workspace-1",
          project_id: "p1",
          from_service_id: from.id,
          to_service_id: to.id,
          dependency_type: "depends_on",
          created_at: "2026-10-25T00:00:00Z",
        },
      ]
    );

    expect(edges.map((edge) => edge.id)).toEqual(["dep-1"]);
  });
});
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/graph-groups.test.ts
```

Expected: module `./graph-groups` not found.

- [ ] **Step 3: Implement the grouping helper**

Create `apps/web/core/components/services/graph/graph-groups.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ILiteService, IService, TServiceKind } from "@plane/types";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { SERVICE_KIND_ORDER } from "../kind";

export const RESTRICTED_GROUP_KEY = "__restricted__";
export const UNASSIGNED_GROUP_PREFIX = "__unassigned__";

export type TServiceGroup = {
  key: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  services: (IService | ILiteService)[];
};

const kindRank = (kind: TServiceKind | null) => (kind ? SERVICE_KIND_ORDER.indexOf(kind) : SERVICE_KIND_ORDER.length);

const groupRank = (group: TServiceGroup) => (group.restricted ? 2 : group.projectId === null ? 1 : 0);

export const getServiceGroupKey = (service: IService | ILiteService): string => {
  if (isLiteService(service)) return RESTRICTED_GROUP_KEY;
  const projectKey = service.owner_project_id ?? UNASSIGNED_GROUP_PREFIX;
  return `${projectKey}::${service.kind}`;
};

export const getServiceGroups = (services: (IService | ILiteService)[]): TServiceGroup[] => {
  const groups = new Map<string, TServiceGroup>();

  services.forEach((service) => {
    const key = getServiceGroupKey(service);
    const existing = groups.get(key);
    if (existing) {
      existing.services.push(service);
      return;
    }
    groups.set(key, {
      key,
      projectId: isLiteService(service) ? null : service.owner_project_id,
      kind: isLiteService(service) ? null : service.kind,
      restricted: isLiteService(service),
      services: [service],
    });
  });

  return [...groups.values()]
    .map((group) => ({
      ...group,
      services: [...group.services].sort((a, b) => a.name.localeCompare(b.name)),
    }))
    .sort(
      (a, b) =>
        groupRank(a) - groupRank(b) ||
        (a.projectId ?? "").localeCompare(b.projectId ?? "") ||
        kindRank(a.kind) - kindRank(b.kind)
    );
};
```

- [ ] **Step 4: Replace the layout implementation**

Replace `apps/web/core/components/services/graph/use-graph-layout.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Edge, Node } from "@xyflow/react";
import { MarkerType, Position } from "@xyflow/react";
import dagre from "dagre";
import type { ILiteService, IService, IServiceDependency } from "@plane/types";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { getServiceGroups } from "./graph-groups";

export const SERVICE_NODE_WIDTH = 256;
export const SERVICE_NODE_HEIGHT = 104;
export const GROUP_GAP = 64;
export const GROUP_MAX_ROW_WIDTH = 1800;
export const CLUSTER_PADDING = 24;
export const CLUSTER_HEADER_HEIGHT = 32;

type TPosition = { x: number; y: number };

type TGroupLayout = {
  positions: Map<string, TPosition>;
  minX: number;
  minY: number;
  width: number;
  height: number;
};

const centroid = (positions: TPosition[]): TPosition => ({
  x: positions.reduce((sum, position) => sum + position.x, 0) / positions.length,
  y: positions.reduce((sum, position) => sum + position.y, 0) / positions.length,
});

const layoutGroup = (services: (IService | ILiteService)[], dependencies: IServiceDependency[]): TGroupLayout => {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", nodesep: 48, ranksep: 112 });

  const ids = new Set(services.map((service) => service.id));
  services.forEach((service) => graph.setNode(service.id, { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT }));
  dependencies.forEach((dependency) => {
    if (ids.has(dependency.from_service_id) && ids.has(dependency.to_service_id)) {
      graph.setEdge(dependency.from_service_id, dependency.to_service_id);
    }
  });

  dagre.layout(graph);

  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  const positions = new Map<string, TPosition>();

  services.forEach((service) => {
    const position = graph.node(service.id);
    const x = (position?.x ?? 0) - SERVICE_NODE_WIDTH / 2;
    const y = (position?.y ?? 0) - SERVICE_NODE_HEIGHT / 2;
    positions.set(service.id, { x, y });
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x + SERVICE_NODE_WIDTH);
    maxY = Math.max(maxY, y + SERVICE_NODE_HEIGHT);
  });

  return {
    positions,
    minX: Number.isFinite(minX) ? minX : 0,
    minY: Number.isFinite(minY) ? minY : 0,
    width: Number.isFinite(maxX) ? maxX - minX : SERVICE_NODE_WIDTH,
    height: Number.isFinite(maxY) ? maxY - minY : SERVICE_NODE_HEIGHT,
  };
};

export const getLayoutedElements = (
  services: (IService | ILiteService)[],
  dependencies: IServiceDependency[]
): { nodes: Node[]; edges: Edge[] } => {
  const groups = getServiceGroups(services);
  const layouts = groups.map((group) => ({ group, layout: layoutGroup(group.services, dependencies) }));

  const slots = new Map<string, TPosition>();
  let cursorX = 0;
  let cursorY = 0;
  let rowHeight = 0;

  layouts.forEach(({ group, layout }) => {
    const outerWidth = layout.width + CLUSTER_PADDING * 2;
    const outerHeight = layout.height + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2;
    if (cursorX > 0 && cursorX + outerWidth > GROUP_MAX_ROW_WIDTH) {
      cursorX = 0;
      cursorY += rowHeight + GROUP_GAP;
      rowHeight = 0;
    }
    slots.set(group.key, { x: cursorX, y: cursorY });
    cursorX += outerWidth + GROUP_GAP;
    rowHeight = Math.max(rowHeight, outerHeight);
  });

  const finalPositions = new Map<string, TPosition>();

  layouts.forEach(({ group, layout }) => {
    const slot = slots.get(group.key) ?? { x: 0, y: 0 };
    const defaults = new Map<string, TPosition>();
    group.services.forEach((service) => {
      const position = layout.positions.get(service.id) ?? { x: 0, y: 0 };
      defaults.set(service.id, {
        x: slot.x + CLUSTER_PADDING + (position.x - layout.minX),
        y: slot.y + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING + (position.y - layout.minY),
      });
    });

    const stored = group.services.filter(
      (service): service is IService => !isLiteService(service) && service.position !== null
    );

    if (stored.length === 0) {
      group.services.forEach((service) => {
        finalPositions.set(service.id, defaults.get(service.id) ?? { x: 0, y: 0 });
      });
      return;
    }

    const defaultCentroid = centroid(group.services.map((service) => defaults.get(service.id) ?? { x: 0, y: 0 }));
    const storedCentroid = centroid(stored.map((service) => service.position ?? { x: 0, y: 0 }));
    const offset = { x: storedCentroid.x - defaultCentroid.x, y: storedCentroid.y - defaultCentroid.y };

    group.services.forEach((service) => {
      const storedPosition = !isLiteService(service) ? service.position : null;
      if (storedPosition) {
        finalPositions.set(service.id, storedPosition);
        return;
      }
      const fallback = defaults.get(service.id) ?? { x: 0, y: 0 };
      finalPositions.set(service.id, { x: fallback.x + offset.x, y: fallback.y + offset.y });
    });
  });

  const ids = new Set(services.map((service) => service.id));
  const nodes: Node[] = services.map((service) => ({
    id: service.id,
    type: "service",
    data: { service, accessLevel: isLiteService(service) ? "lite" : "full" },
    position: finalPositions.get(service.id) ?? { x: 0, y: 0 },
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
    zIndex: 10,
  }));

  const edges: Edge[] = dependencies
    .filter((dependency) => ids.has(dependency.from_service_id) && ids.has(dependency.to_service_id))
    .map((dependency) => ({
      id: dependency.id,
      source: dependency.from_service_id,
      target: dependency.to_service_id,
      type: "smoothstep",
      data: { dependencyType: dependency.dependency_type ?? "depends_on" },
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
    }));

  return { nodes, edges };
};
```

- [ ] **Step 5: Run the tests**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/graph-groups.test.ts
```

Expected: all pass. If the overlap assertion fails, print the bounds and adjust `GROUP_GAP`/packing—do not weaken the test.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/graph/graph-groups.ts \
  apps/web/core/components/services/graph/graph-groups.test.ts \
  apps/web/core/components/services/graph/use-graph-layout.ts
git commit -m "feat(web): lay out service graph by project and component kind"
```

---

### Task 7: Draw live cluster frames

**Files:**

- Create: `apps/web/core/components/services/kind-config.ts`
- Create: `apps/web/core/components/services/graph/graph-clusters.ts`
- Create: `apps/web/core/components/services/graph/graph-clusters.test.ts`
- Modify: `apps/web/core/components/services/graph/service-graph-canvas.tsx`
- Modify: `apps/web/core/components/services/graph/service-node.tsx`
- Modify: `apps/web/core/components/services/graph/node-sync.ts`
- Modify: `apps/web/core/components/services/graph/node-sync.test.ts`

- [ ] **Step 1: Write the failing cluster tests**

Create `apps/web/core/components/services/graph/graph-clusters.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { ILiteService, IService } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { getServiceClusters } from "./graph-clusters";
import { SERVICE_NODE_HEIGHT, SERVICE_NODE_WIDTH } from "./use-graph-layout";

const makeService = (id: string, overrides: Partial<IService> = {}): IService => ({
  id,
  workspace_id: "workspace-1",
  project_id: "p1",
  owner_project_id: "p1",
  project_links: [],
  access_level: "full",
  name: id,
  description: "",
  description_html: "",
  status: "active",
  criticality: "medium",
  type: "internal",
  kind: "service",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
  position: { x: 100, y: 100 },
  sort_order: 1,
  created_at: "2026-10-25T00:00:00Z",
  updated_at: "2026-10-25T00:00:00Z",
  created_by: null,
  updated_by: null,
  ...overrides,
});

const makeLite = (id: string): ILiteService => ({
  id,
  name: id,
  status: "active",
  criticality: "medium",
  access_level: "lite",
});

const makeNode = (id: string, x: number, y: number): Node => ({
  id,
  position: { x, y },
  measured: { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT },
  data: {},
});

const labels: Record<string, string> = {
  "service.kind_values.database": "Database",
  "service.kind_values.service": "Service",
  "service.kind_values.server": "Server",
  "service.graph.unassigned": "Unassigned",
  "service.graph.restricted": "Restricted",
};
const t = (key: string) => labels[key] ?? key;

describe("getServiceClusters", () => {
  it("computes frame bounds with padding and header", () => {
    const service = makeService("db", { kind: "database" });
    const clusters = getServiceClusters([makeNode("db", 100, 100)], [service], {}, t);

    expect(clusters).toHaveLength(1);
    expect(clusters[0]).toMatchObject({
      x: 76,
      y: 44,
      width: 304,
      height: 184,
      label: "Database",
    });
  });

  it("prefixes the project identifier when multiple projects are visible", () => {
    const services = [
      makeService("db", { kind: "database" }),
      makeService("svc", { owner_project_id: "p2", project_id: "p2" }),
    ];
    const clusters = getServiceClusters(
      [makeNode("db", 0, 0), makeNode("svc", 400, 0)],
      services,
      { p1: { identifier: "PAY", name: "Payments" }, p2: { identifier: "AUTH", name: "Auth" } },
      t
    );

    expect(clusters.map((cluster) => cluster.label).sort()).toEqual(["AUTH · Service", "PAY · Database"]);
  });

  it("labels unassigned and restricted groups", () => {
    const services = [
      makeService("named"),
      makeService("unassigned", { owner_project_id: null, project_id: null, kind: "server" }),
      makeLite("lite"),
    ];
    const clusters = getServiceClusters(
      [makeNode("named", 0, 0), makeNode("unassigned", 400, 0), makeNode("lite", 800, 0)],
      services,
      {},
      t
    );

    expect(clusters.map((cluster) => cluster.label).sort()).toEqual(["Restricted", "Service", "Unassigned · Server"]);
  });

  it("moves the frame when a member node moves", () => {
    const service = makeService("db", { kind: "database" });
    const before = getServiceClusters([makeNode("db", 100, 100)], [service], {}, t)[0];
    const after = getServiceClusters([makeNode("db", 300, 100)], [service], {}, t)[0];

    expect(after.x - before.x).toBe(200);
  });
});
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/graph-clusters.test.ts
```

Expected: module `./graph-clusters` not found.

- [ ] **Step 3: Add kind icons and styles**

Create `apps/web/core/components/services/kind-config.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ComponentType } from "react";
import {
  ArchiveOutline,
  BoxesOutline,
  CacheOutline,
  ConnectionsOutline,
  CubeOutline,
  GlobeOutline,
  LibraryOutline,
  ListOutline,
  ServerOutline,
} from "@makeplane/propel/icons";
import type { TServiceKind } from "@plane/types";

export const SERVICE_KIND_ICON: Record<TServiceKind, ComponentType<{ className?: string }>> = {
  service: CubeOutline,
  database: LibraryOutline,
  server: ServerOutline,
  cache: CacheOutline,
  queue: ListOutline,
  storage: ArchiveOutline,
  network: ConnectionsOutline,
  external_api: GlobeOutline,
  other: BoxesOutline,
};

export const SERVICE_KIND_CLUSTER_STYLE: Record<TServiceKind, string> = {
  service: "border-sky-500/40 bg-sky-500/5 text-sky-600 dark:text-sky-400",
  database: "border-violet-500/40 bg-violet-500/5 text-violet-600 dark:text-violet-400",
  server: "border-orange-500/40 bg-orange-500/5 text-orange-600 dark:text-orange-400",
  cache: "border-pink-500/40 bg-pink-500/5 text-pink-600 dark:text-pink-400",
  queue: "border-amber-500/40 bg-amber-500/5 text-amber-600 dark:text-amber-400",
  storage: "border-emerald-500/40 bg-emerald-500/5 text-emerald-600 dark:text-emerald-400",
  network: "border-cyan-500/40 bg-cyan-500/5 text-cyan-600 dark:text-cyan-400",
  external_api: "border-indigo-500/40 bg-indigo-500/5 text-indigo-600 dark:text-indigo-400",
  other: "border-subtle bg-layer-2/40 text-tertiary",
};

export const RESTRICTED_CLUSTER_STYLE = "border-dashed border-subtle bg-layer-2/30 text-tertiary";
```

- [ ] **Step 4: Implement cluster computation**

Create `apps/web/core/components/services/graph/graph-clusters.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { ILiteService, IService, TServiceKind } from "@plane/types";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { SERVICE_KIND_ORDER } from "../kind";
import { getServiceGroupKey } from "./graph-groups";
import { CLUSTER_HEADER_HEIGHT, CLUSTER_PADDING, SERVICE_NODE_HEIGHT, SERVICE_NODE_WIDTH } from "./use-graph-layout";

export type TServiceCluster = {
  id: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
};

type TClusterBucket = {
  key: string;
  projectId: string | null;
  kind: TServiceKind | null;
  restricted: boolean;
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
};

const kindRank = (kind: TServiceKind | null) => (kind ? SERVICE_KIND_ORDER.indexOf(kind) : SERVICE_KIND_ORDER.length);

const bucketRank = (bucket: TClusterBucket) => (bucket.restricted ? 2 : bucket.projectId === null ? 1 : 0);

export const getServiceClusters = (
  nodes: Node[],
  services: (IService | ILiteService)[],
  projectLookup: Record<string, { identifier: string; name: string }> | undefined,
  t: (key: string) => string
): TServiceCluster[] => {
  const serviceById = new Map(services.map((service) => [service.id, service]));
  const buckets = new Map<string, TClusterBucket>();

  nodes.forEach((node) => {
    const service = serviceById.get(node.id);
    if (!service) return;

    const key = getServiceGroupKey(service);
    const width = node.measured?.width ?? SERVICE_NODE_WIDTH;
    const height = node.measured?.height ?? SERVICE_NODE_HEIGHT;
    const bucket = buckets.get(key) ?? {
      key,
      projectId: isLiteService(service) ? null : service.owner_project_id,
      kind: isLiteService(service) ? null : service.kind,
      restricted: isLiteService(service),
      minX: Number.POSITIVE_INFINITY,
      minY: Number.POSITIVE_INFINITY,
      maxX: Number.NEGATIVE_INFINITY,
      maxY: Number.NEGATIVE_INFINITY,
    };

    bucket.minX = Math.min(bucket.minX, node.position.x);
    bucket.minY = Math.min(bucket.minY, node.position.y);
    bucket.maxX = Math.max(bucket.maxX, node.position.x + width);
    bucket.maxY = Math.max(bucket.maxY, node.position.y + height);
    buckets.set(key, bucket);
  });

  const ordered = [...buckets.values()].sort(
    (a, b) =>
      bucketRank(a) - bucketRank(b) ||
      (a.projectId ?? "").localeCompare(b.projectId ?? "") ||
      kindRank(a.kind) - kindRank(b.kind)
  );

  const namedProjectIds = new Set(
    ordered.filter((bucket) => bucket.projectId !== null).map((bucket) => bucket.projectId as string)
  );
  const hasUnassigned = ordered.some((bucket) => !bucket.restricted && bucket.projectId === null);
  const showProjectPrefix = namedProjectIds.size + (hasUnassigned ? 1 : 0) > 1;

  return ordered.map((bucket) => {
    const kindLabel = bucket.kind ? t(`service.kind_values.${bucket.kind}`) : null;
    const projectLabel = bucket.restricted
      ? t("service.graph.restricted")
      : bucket.projectId
        ? (projectLookup?.[bucket.projectId]?.identifier ?? null)
        : t("service.graph.unassigned");
    const parts = [showProjectPrefix ? projectLabel : null, kindLabel].filter(Boolean) as string[];
    const label = parts.length > 0 ? parts.join(" · ") : (projectLabel ?? "");

    return {
      id: bucket.key,
      projectId: bucket.projectId,
      kind: bucket.kind,
      restricted: bucket.restricted,
      label,
      x: bucket.minX - CLUSTER_PADDING,
      y: bucket.minY - CLUSTER_HEADER_HEIGHT - CLUSTER_PADDING,
      width: bucket.maxX - bucket.minX + CLUSTER_PADDING * 2,
      height: bucket.maxY - bucket.minY + CLUSTER_HEADER_HEIGHT + CLUSTER_PADDING * 2,
    };
  });
};
```

- [ ] **Step 5: Run the cluster tests**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/graph-clusters.test.ts
```

Expected: all pass.

- [ ] **Step 6: Render frames in the canvas**

In `apps/web/core/components/services/graph/service-graph-canvas.tsx`:

Add `ViewportPortal` to the `@xyflow/react` import and add `cn`:

```tsx
import { cn } from "@plane/utils";
import {
  Background,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  ViewportPortal,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
  type ReactFlowInstance,
} from "@xyflow/react";
```

Add the imports:

```tsx
import { RESTRICTED_CLUSTER_STYLE, SERVICE_KIND_CLUSTER_STYLE } from "../kind-config";
import { getServiceClusters } from "./graph-clusters";
```

Add `kindLabel` to the translated node data after `typeLabel`:

```tsx
            typeLabel: !isLite ? t(`service.type_values.${service.type}`) : t("service.lite.restricted"),
            kindLabel: !isLite ? t(`service.kind_values.${service.kind}`) : undefined,
```

After the `nodes`/`edges` state declarations, add:

```tsx
const clusters = useMemo(
  () => getServiceClusters(nodes, graphData.services, projectLookup, t),
  // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change
  [nodes, graphData.services, projectLookup, currentLocale]
);
```

Inside `<ReactFlow>`, before `<Background />`, add:

```tsx
<ViewportPortal>
  {clusters.map((cluster) => (
    <div
      key={cluster.id}
      className={cn(
        "pointer-events-none absolute rounded-xl border",
        cluster.restricted ? RESTRICTED_CLUSTER_STYLE : SERVICE_KIND_CLUSTER_STYLE[cluster.kind ?? "other"]
      )}
      style={{
        transform: `translate(${cluster.x}px, ${cluster.y}px)`,
        width: cluster.width,
        height: cluster.height,
        zIndex: 0,
      }}
    >
      <span className="absolute left-3 top-2 text-11 font-medium">{cluster.label}</span>
    </div>
  ))}
</ViewportPortal>
```

Change both fit calls to use padding:

```tsx
instance.fitView({ padding: 0.25, duration: 300 });
```

and in the initial fit:

```tsx
instance.fitView({ padding: 0.25 });
```

- [ ] **Step 7: Use kind icons on nodes**

In `apps/web/core/components/services/graph/service-node.tsx`:

Remove the unused icon imports (`BoxesOutline`, `GlobeOutline`, `ServerOutline`) and the `SERVICE_TYPE_ICON` map, keep `CubeOutline` as the fallback, and import the shared map:

```tsx
// Existing import becomes:
import { ClockOutline, CubeOutline, LockOutline, WarningTriangleOutline } from "@makeplane/propel/icons";
// New import:
import { SERVICE_KIND_ICON } from "../kind-config";
```

Remove `TServiceType` from the `@plane/types` import and remove `ComponentType` from the React import.

Add `kindLabel?: string;` to `TServiceNodeData` after `typeLabel`.

Destructure `kindLabel` with the other labels and replace the icon selection:

```tsx
const kind = !isLite && "kind" in service ? service.kind : "service";
const KindIcon = SERVICE_KIND_ICON[kind] ?? CubeOutline;
```

Replace `TypeIcon` in the icon title/JSX:

```tsx
              title={kindLabel ?? typeLabel}
            >
              {isLite ? <LockOutline className="h-3 w-3 text-tertiary" /> : <KindIcon className="h-3 w-3" />}
```

- [ ] **Step 8: Keep kind labels in sync**

In `apps/web/core/components/services/graph/node-sync.ts`:

Add `kindLabel?: string;` to `TServiceNodeData` after `typeLabel`, and add the comparison:

```ts
  left?.typeLabel === right?.typeLabel &&
  left?.kindLabel === right?.kindLabel &&
```

In `apps/web/core/components/services/graph/node-sync.test.ts`, add `kindLabel: "Database",` to `makeNode` data so the fixtures remain representative. Add one test:

```ts
it("applies a new kind label", () => {
  const prev = [makeNode("a")];
  const next = [{ ...prev[0], data: { ...prev[0].data, kindLabel: "Server" } }];

  const result = syncGraphNodes(prev, next);

  expect((result[0].data as { kindLabel: string }).kindLabel).toBe("Server");
});
```

- [ ] **Step 9: Run graph tests, typecheck, and lint**

```bash
pnpm --filter=web exec vitest run core/components/services/graph
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: all pass, exit 0.

- [ ] **Step 10: Commit**

```bash
git add apps/web/core/components/services/kind-config.ts \
  apps/web/core/components/services/graph/graph-clusters.ts \
  apps/web/core/components/services/graph/graph-clusters.test.ts \
  apps/web/core/components/services/graph/service-graph-canvas.tsx \
  apps/web/core/components/services/graph/service-node.tsx \
  apps/web/core/components/services/graph/node-sync.ts \
  apps/web/core/components/services/graph/node-sync.test.ts
git commit -m "feat(web): render live service clusters on the graph"
```

---

### Task 8: Re-fit the graph when a service kind changes

**Files:**

- Modify: `apps/web/core/store/service.store.ts`
- Test: `apps/web/core/store/service.store.test.ts`

- [ ] **Step 1: Write the failing store test**

In `apps/web/core/store/service.store.test.ts`, add `updateService` to the mocked `serviceService` in `makeStore`:

```ts
    updateService: vi.fn(
      async (
        _slug: string,
        _workspaceId: string,
        _projectId: string,
        serviceId: string,
        data: Partial<IService>
      ) => ({
        ...makeService(serviceId, null),
        ...data,
      })
    ),
```

Add this test at the end of the file:

```ts
describe("ServicesStore.updateService layout", () => {
  it("bumps the layout epoch when the kind changes", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", null) };

    await store.updateService("acme", "workspace-1", "project-1", "service-1", { kind: "database" });

    expect(store.serviceMap["service-1"].kind).toBe("database");
    expect(store.layoutEpoch).toBe(1);
  });

  it("does not bump the layout epoch for other fields", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", null) };

    await store.updateService("acme", "workspace-1", "project-1", "service-1", { status: "maintenance" });

    expect(store.serviceMap["service-1"].status).toBe("maintenance");
    expect(store.layoutEpoch).toBe(0);
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
pnpm --filter=web exec vitest run core/store/service.store.test.ts
```

Expected: layout epoch stays `0` for the kind test.

- [ ] **Step 3: Implement the epoch bump**

In `apps/web/core/store/service.store.ts`, inside `updateService`, add before the try block:

```ts
const kindChanged = data.kind !== undefined && data.kind !== original.kind;
```

Replace the success `runInAction` with:

```ts
runInAction(() => {
  set(this.serviceMap, [serviceId], response);
  if (kindChanged) this.markLayoutChanged();
});
```

- [ ] **Step 4: Run the store tests**

```bash
pnpm --filter=web exec vitest run core/store/service.store.test.ts
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/service.store.ts apps/web/core/store/service.store.test.ts
git commit -m "feat(web): refit graph when service kind changes"
```

---

### Task 9: Verify and roll out

**Files:** none (fixes only if verification fails)

- [ ] **Step 1: Run the full web test suite**

```bash
pnpm --filter=web test
```

Expected: all test files pass.

- [ ] **Step 2: Run typecheck, lint, and build**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web build
```

Expected: all exit 0; build prints `built in ...`.

- [ ] **Step 3: Run Rust checks**

```bash
(cd apps/api-rs && cargo check -p api -p ai)
(cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_workspace_test -- --test-threads=1)
```

Expected: check succeeds; suite passes.

- [ ] **Step 4: Rebuild and restart the backend**

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker \
  > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll until done:

```bash
sleep 300; tail -5 /tmp/plane-api-build.log
```

Expected: containers started. Rust LTO link can take 10+ minutes with no output; do not abort. Then:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200`.

- [ ] **Step 5: Restart live and web prod**

```bash
systemctl --user restart plane-live.service
sleep 15
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
systemctl --user restart plane-web-prod.service
sleep 10
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000
```

Expected: `200` for both.

- [ ] **Step 6: Manual smoke checklist**

With a browser:

- Create a service with kind `Database` from the form; verify the API stores `database` (sidebar shows it).
- Change kind in the sidebar; verify the node moves to the new cluster and other nodes do not move.
- Filter by Component kind in project and workspace views.
- Open the project graph, workspace graph, and war room map; verify Project × Kind frames and labels.
- Drag a node; verify the frame follows; refresh and verify the position persists.
- Press Re-layout; verify clustered layout and fit.
- Add/remove a dependency; verify clustered auto-layout and fit.
- Verify health polling and language switching do not move nodes.
- Toggle light/dark mode; verify frame colors remain readable.
- Verify a lite endpoint and an ownerless service fall into Restricted/Unassigned frames.

- [ ] **Step 7: Commit any verification fixes**

If a fix was needed:

```bash
git add <changed-files>
git commit -m "fix(web): <short description>"
```

If no fixes were needed, do not create an empty commit.
