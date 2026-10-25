# Service Graph Layout Reset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the service graph re-layout deterministically whenever topology changes, fix Re-layout, and support persisted node positions in workspace scope.

**Architecture:** Backend clears `services.position` workspace-wide inside the same transaction as service/dependency create/delete. The web store mirrors that by clearing loaded positions and bumping `layoutEpoch`; the canvas applies dagre positions when `position` is null and fits the viewport when `layoutEpoch` changes.

**Tech Stack:** Rust (axum + sqlx), React 18 + React Flow (`@xyflow/react`) + dagre, MobX store, Vitest.

---

## File Structure

**Backend**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs` — layout reset helper + call sites.
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs` — position reset integration tests.

**Web**

- Create: `apps/web/core/components/services/graph/node-sync.ts` — pure node state merge helper.
- Create: `apps/web/core/components/services/graph/node-sync.test.ts` — helper tests.
- Modify: `apps/web/core/components/services/graph/service-graph-canvas.tsx` — use helper, accept `layoutEpoch`, fit on change.
- Modify: `apps/web/core/components/services/graph/service-graph.tsx` — workspace-aware drag/Re-layout, pass epoch.
- Modify: `apps/web/core/store/service.store.ts` — `layoutEpoch`, `clearLayoutPositions`, `markLayoutChanged`, workspace-aware `updateNodePosition`.
- Test: `apps/web/core/store/service.store.test.ts` — reset/epoch/endpoint tests.
- Modify: `apps/web/core/services/service.service.ts` — workspace/project branch for `updateNodePosition`.

No schema migration, no API shape change, no i18n change.

---

### Task 1: Service mutations reset workspace layout

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write the failing test**

Add helpers next to `insert_service` in `service_workspace_test.rs`:

```rust
async fn set_service_position(pool: &PgPool, service_id: Uuid, x: f64, y: f64) {
    sqlx::query(
        "UPDATE services SET position = jsonb_build_object('x', $2::float8, 'y', $3::float8) \
         WHERE id = $1",
    )
    .bind(service_id)
    .bind(x)
    .bind(y)
    .execute(pool)
    .await
    .expect("set service position");
}

async fn service_position(pool: &PgPool, service_id: Uuid) -> Option<serde_json::Value> {
    sqlx::query_scalar("SELECT position FROM services WHERE id = $1")
        .bind(service_id)
        .fetch_one(pool)
        .await
        .expect("service position")
}
```

Add this test near the other service mutation tests:

```rust
#[tokio::test]
async fn service_mutations_reset_workspace_layout() {
    use api::routes::service::{
        create_workspace, destroy_workspace, CreateService, CreateWorkspaceService,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let new_service = |name: &str| CreateWorkspaceService {
        owner_project_id: sc.p1,
        service: CreateService {
            name: Some(name.into()),
            description: None,
            description_html: None,
            status: None,
            criticality: None,
            service_type: None,
            owner_id: None,
            repository_url: None,
            documentation_url: None,
        },
    };

    for id in [sc.s1, sc.s2, sc.s4] {
        set_service_position(&st.pool, id, 10.0, 20.0).await;
    }
    let updated_at_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM services WHERE id = $1")
            .bind(sc.s1)
            .fetch_one(&st.pool)
            .await
            .expect("service updated_at");

    let (status, Json(created)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Payments API")),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let new_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    for id in [sc.s1, sc.s2, sc.s4, new_id] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "create service resets every workspace position"
        );
    }
    let updated_at_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM services WHERE id = $1")
            .bind(sc.s1)
            .fetch_one(&st.pool)
            .await
            .expect("service updated_at");
    assert_eq!(
        updated_at_before, updated_at_after,
        "layout reset must not touch updated_at"
    );

    set_service_position(&st.pool, sc.s1, 30.0, 40.0).await;
    set_service_position(&st.pool, new_id, 50.0, 60.0).await;
    let (status, _) = destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    for id in [sc.s1, sc.s2, sc.s4] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "delete service resets remaining workspace positions"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
(cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test service_mutations_reset_workspace_layout -- --test-threads=1)
```

Expected: FAIL because positions are still `Some(...)` after the mutations.

- [ ] **Step 3: Add the reset helper**

In `apps/api-rs/crates/api/src/routes/service.rs`, add after `next_sort_order`:

```rust
async fn reset_workspace_layout(
    tx: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE services SET position = NULL \
         WHERE workspace_id = $1 AND deleted_at IS NULL AND position IS NOT NULL",
    )
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 4: Call the helper from `create_service_core`**

Inside the existing transaction, after the `service_project_links` INSERT and before `tx.commit()`, add:

```rust
    let workspace_id: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
            .bind(owner_project_id)
            .fetch_one(&mut *tx)
            .await?;
    reset_workspace_layout(&mut *tx, workspace_id).await?;
```

- [ ] **Step 5: Call the helper from `destroy_service_core`**

At the start of the transaction (before the soft-deletes), capture the workspace:

```rust
    let workspace_id: Option<Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM services WHERE id = $1")
            .bind(pk)
            .fetch_optional(&mut *tx)
            .await?;
```

After the `services` soft-delete and before `tx.commit()`, add:

```rust
    if let Some(workspace_id) = workspace_id {
        reset_workspace_layout(&mut *tx, workspace_id).await?;
    }
```

- [ ] **Step 6: Run test to verify it passes**

```bash
(cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test service_mutations_reset_workspace_layout -- --test-threads=1)
```

Expected: PASS (`1 passed`).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): reset service layout on service mutations"
```

---

### Task 2: Dependency mutations reset workspace layout

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/service.rs`
- Test: `apps/api-rs/crates/api/tests/service_workspace_test.rs`

- [ ] **Step 1: Write the failing test**

Add after the test from Task 1:

```rust
#[tokio::test]
async fn dependency_mutations_reset_workspace_layout() {
    use api::routes::service::{
        dependencies_create as dependencies_create_compat,
        dependencies_create_workspace, dependency_destroy as dependency_destroy_compat,
        dependency_destroy_workspace, CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // S1 -> S2 requires S2 to be linked to P1.
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;

    for id in [sc.s1, sc.s2, sc.s4] {
        set_service_position(&st.pool, id, 10.0, 20.0).await;
    }
    let (status, Json(project_edge)) = dependencies_create_compat(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let project_edge_id = Uuid::parse_str(project_edge["id"].as_str().unwrap()).unwrap();
    for id in [sc.s1, sc.s2, sc.s4] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "create dependency resets every workspace position"
        );
    }

    // Duplicate is rejected and must not disturb positions.
    for id in [sc.s1, sc.s2, sc.s4] {
        set_service_position(&st.pool, id, 30.0, 40.0).await;
    }
    let duplicate = dependencies_create_compat(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(duplicate.0, axum::http::StatusCode::BAD_REQUEST);
    for id in [sc.s1, sc.s2, sc.s4] {
        assert!(
            service_position(&st.pool, id).await.is_some(),
            "rejected duplicate leaves positions intact"
        );
    }

    // Cycle is rejected and must not disturb positions.
    let cycle = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s2,
            to_service_id: sc.s1,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(cycle.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(cycle.1["error"], "This dependency would create a cycle.");
    for id in [sc.s1, sc.s2, sc.s4] {
        assert!(
            service_position(&st.pool, id).await.is_some(),
            "rejected cycle leaves positions intact"
        );
    }

    // Project delete resets.
    let (status, _) = dependency_destroy_compat(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1, project_edge_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    for id in [sc.s1, sc.s2, sc.s4] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "project delete resets every workspace position"
        );
    }

    // Workspace create resets.
    for id in [sc.s1, sc.s2, sc.s4] {
        set_service_position(&st.pool, id, 50.0, 60.0).await;
    }
    let (status, Json(workspace_edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s2,
            to_service_id: sc.s1,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let workspace_edge_id = Uuid::parse_str(workspace_edge["id"].as_str().unwrap()).unwrap();
    for id in [sc.s1, sc.s2, sc.s4] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "workspace create resets every workspace position"
        );
    }

    // Workspace delete resets.
    for id in [sc.s1, sc.s2, sc.s4] {
        set_service_position(&st.pool, id, 70.0, 80.0).await;
    }
    let (status, _) = dependency_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), workspace_edge_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    for id in [sc.s1, sc.s2, sc.s4] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "workspace delete resets every workspace position"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
(cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test dependency_mutations_reset_workspace_layout -- --test-threads=1)
```

Expected: FAIL on the first position assertion.

- [ ] **Step 3: Wrap dependency create in a transaction and reset**

In `create_dependency_core`, replace the existing INSERT execution with:

```rust
    let mut tx = st.pool.begin().await?;
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
    .execute(&mut *tx)
    .await?;
    reset_workspace_layout(&mut *tx, workspace_id).await?;
    tx.commit().await?;
```

- [ ] **Step 4: Wrap workspace dependency destroy in a transaction and reset**

Replace the soft-delete query in `dependency_destroy_workspace` with:

```rust
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    reset_workspace_layout(&mut *tx, row.workspace_id).await?;
    tx.commit().await?;
```

- [ ] **Step 5: Wrap project dependency destroy in a transaction and reset**

Replace the soft-delete query in `dependency_destroy` with the same block (it also has `row.workspace_id`):

```rust
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    reset_workspace_layout(&mut *tx, row.workspace_id).await?;
    tx.commit().await?;
```

- [ ] **Step 6: Run test to verify it passes**

```bash
(cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test dependency_mutations_reset_workspace_layout -- --test-threads=1)
```

Expected: PASS (`1 passed`).

- [ ] **Step 7: Run the whole service workspace suite**

```bash
(cd apps/api-rs && DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1)
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/service.rs apps/api-rs/crates/api/tests/service_workspace_test.rs
git commit -m "feat(api): reset service layout on dependency mutations"
```

---

### Task 3: Sync canvas nodes when positions or data change

**Files:**

- Create: `apps/web/core/components/services/graph/node-sync.ts`
- Create: `apps/web/core/components/services/graph/node-sync.test.ts`
- Modify: `apps/web/core/components/services/graph/service-graph-canvas.tsx`

- [ ] **Step 1: Write the failing tests**

Create `node-sync.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { IService } from "@plane/types";
import { describe, expect, it } from "vitest";
// local
import { syncGraphNodes } from "./node-sync";

const makeNode = (id: string, service = { id, name: id }): Node => ({
  id,
  type: "service",
  position: { x: 0, y: 0 },
  data: {
    service: service as unknown as IService,
    statusLabel: "Active",
    criticalityLabel: "Medium",
    typeLabel: "Internal",
    healthLabel: "Healthy",
    ownerProjectIdentifier: "P1",
    ownerProjectName: "Project One",
    health: "healthy",
    incidents: [],
    lastDeployedAt: null,
    noDeploymentLabel: "No deployment",
    dimmed: false,
  },
});

describe("syncGraphNodes", () => {
  it("returns the previous array when nothing changed", () => {
    const prev = [makeNode("a"), makeNode("b")];
    const next = prev.map((node) => ({ ...node, data: { ...node.data } }));

    expect(syncGraphNodes(prev, next)).toBe(prev);
  });

  it("applies a new position", () => {
    const prev = [makeNode("a")];
    const next = [{ ...prev[0], position: { x: 100, y: 200 } }];

    const result = syncGraphNodes(prev, next);

    expect(result[0].position).toEqual({ x: 100, y: 200 });
  });

  it("applies new labels", () => {
    const prev = [makeNode("a")];
    const next = [
      {
        ...prev[0],
        data: { ...prev[0].data, healthLabel: "Degraded" },
      },
    ];

    const result = syncGraphNodes(prev, next);

    expect((result[0].data as { healthLabel: string }).healthLabel).toBe("Degraded");
  });

  it("applies a new service reference", () => {
    const prev = [makeNode("a")];
    const next = [makeNode("a", { id: "a", name: "Renamed" })];

    const result = syncGraphNodes(prev, next);

    expect((result[0].data as { service: IService }).service.name).toBe("Renamed");
  });

  it("preserves selection", () => {
    const prev = [{ ...makeNode("a"), selected: true }];
    const next = [{ ...makeNode("a"), selected: false }];

    const result = syncGraphNodes(prev, next);

    expect(result[0].selected).toBe(true);
  });

  it("does not move a node while it is being dragged", () => {
    const dragging = { ...makeNode("a"), dragging: true };
    const prev = [dragging];
    const next = [{ ...makeNode("a"), position: { x: 500, y: 600 } }];

    const result = syncGraphNodes(prev, next);

    expect(result[0]).toBe(dragging);
    expect(result[0].position).toEqual({ x: 0, y: 0 });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/node-sync.test.ts
```

Expected: FAIL because `./node-sync` does not exist.

- [ ] **Step 3: Implement the helper**

Create `node-sync.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Node } from "@xyflow/react";
import type { ILiteService, IService } from "@plane/types";

type TServiceNodeData = {
  service?: IService | ILiteService;
  statusLabel?: string;
  criticalityLabel?: string;
  typeLabel?: string;
  healthLabel?: string;
  ownerProjectIdentifier?: string;
  ownerProjectName?: string;
  health?: string;
  incidents?: unknown[];
  lastDeployedAt?: string | null;
  noDeploymentLabel?: string;
  dimmed?: boolean;
};

const hasSamePosition = (left: Node["position"], right: Node["position"]) => left.x === right.x && left.y === right.y;

const hasSameData = (left?: TServiceNodeData, right?: TServiceNodeData) =>
  left?.service === right?.service &&
  left?.statusLabel === right?.statusLabel &&
  left?.criticalityLabel === right?.criticalityLabel &&
  left?.typeLabel === right?.typeLabel &&
  left?.healthLabel === right?.healthLabel &&
  left?.ownerProjectIdentifier === right?.ownerProjectIdentifier &&
  left?.ownerProjectName === right?.ownerProjectName &&
  left?.health === right?.health &&
  (left?.incidents?.length ?? 0) === (right?.incidents?.length ?? 0) &&
  left?.lastDeployedAt === right?.lastDeployedAt &&
  left?.noDeploymentLabel === right?.noDeploymentLabel &&
  left?.dimmed === right?.dimmed;

/**
 * Merges freshly computed layout nodes into React Flow state while preserving
 * selection and never fighting an in-progress drag.
 */
export function syncGraphNodes(previous: Node[], next: Node[]): Node[] {
  const previousById = new Map(previous.map((node) => [node.id, node]));
  let changed = previous.length !== next.length;

  const merged = next.map((node) => {
    const current = previousById.get(node.id);
    if (!current) {
      changed = true;
      return node;
    }
    if (current.dragging) return current;

    const same =
      hasSamePosition(current.position, node.position) &&
      hasSameData(current.data as TServiceNodeData, node.data as TServiceNodeData);

    if (same) return current;
    changed = true;
    return { ...node, selected: current.selected };
  });

  const sameOrder = previous.length === next.length && previous.every((node, index) => node.id === next[index]?.id);

  return changed || !sameOrder ? merged : previous;
}
```

- [ ] **Step 4: Run helper tests**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/node-sync.test.ts
```

Expected: PASS (`6 passed`).

- [ ] **Step 5: Use the helper in the canvas**

In `service-graph-canvas.tsx`:

1. Import the helper:

```ts
import { syncGraphNodes } from "./node-sync";
```

2. Replace the entire `setNodes(...)` body in the sync effect with:

```ts
useEffect(() => {
  setNodes((prev) => syncGraphNodes(prev, translatedNodes));
  setEdges((prev) => {
    if (
      prev.length === translatedEdges.length &&
      prev.every((edge, i) => {
        const next = translatedEdges[i];
        return (
          edge.id === next?.id &&
          edge.label === next?.label &&
          edge.animated === next?.animated &&
          edge.style?.stroke === next?.style?.stroke
        );
      })
    )
      return prev;
    const selectedById = new Map(prev.map((edge) => [edge.id, edge.selected]));
    return translatedEdges.map((edge) =>
      selectedById.has(edge.id) ? { ...edge, selected: selectedById.get(edge.id) } : edge
    );
  });
}, [translatedNodes, translatedEdges, setNodes, setEdges]);
```

3. Remove the now-unused inline node comparator/types (`prevData`, `nextData`, `selectedById` for nodes).

- [ ] **Step 6: Run web tests + typecheck**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/node-sync.test.ts
pnpm --filter=web check:types
```

Expected: tests PASS, typecheck exits 0.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/services/graph/node-sync.ts apps/web/core/components/services/graph/node-sync.test.ts apps/web/core/components/services/graph/service-graph-canvas.tsx
git commit -m "fix(web): apply graph layout position changes"
```

---

### Task 4: Store layout epoch and workspace-aware position updates

**Files:**

- Modify: `apps/web/core/store/service.store.ts`
- Modify: `apps/web/core/services/service.service.ts`
- Test: `apps/web/core/store/service.store.test.ts`

- [ ] **Step 1: Write the failing store tests**

Extend `apps/web/core/store/service.store.test.ts`.

Add this factory near the imports:

```ts
import type { IService } from "@plane/types";

const makeService = (id: string, position: IService["position"]): IService =>
  ({
    id,
    workspace_id: "workspace-1",
    project_id: "project-1",
    owner_project_id: "project-1",
    project_links: [],
    access_level: "full",
    name: id,
    description: "",
    description_html: "",
    status: "active",
    criticality: "medium",
    type: "internal",
    owner_id: null,
    repository_url: null,
    documentation_url: null,
    position,
    sort_order: 1,
    created_at: "2026-10-25T00:00:00Z",
    updated_at: "2026-10-25T00:00:00Z",
  }) as unknown as IService;
```

Extend the `serviceService` mock inside `makeStore`:

```ts
const serviceService = {
  deleteService: vi.fn(async () => undefined),
  deleteWorkspaceService: vi.fn(async () => undefined),
  createService: vi.fn(async (_slug: string, _workspaceId: string, _projectId: string, _data: Partial<IService>) =>
    makeService("service-3", null)
  ),
  createWorkspaceService: vi.fn(async (_slug: string, _workspaceId: string, _data: Partial<IService>) =>
    makeService("service-3", null)
  ),
  createDependency: vi.fn(async () => ({
    id: "dep-1",
    workspace_id: "workspace-1",
    project_id: null,
    from_service_id: "service-1",
    to_service_id: "service-2",
    dependency_type: "depends_on",
    created_at: "2026-10-25T00:00:00Z",
  })),
  deleteDependency: vi.fn(async () => undefined),
  updateNodePosition: vi.fn(async (_slug: string, _workspaceId: string, _projectId: string, serviceId: string) =>
    makeService(serviceId, { x: 5, y: 6 })
  ),
  updateWorkspaceService: vi.fn(async (_slug: string, serviceId: string) => makeService(serviceId, null)),
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
      created_at: "2026-10-25T00:00:00Z",
    })
  ),
};
```

Add these describe blocks:

```ts
describe("ServicesStore layout reset", () => {
  it("clears positions and bumps the epoch after a dependency is added", async () => {
    const { store } = makeStore();
    store.serviceMap = {
      "service-1": makeService("service-1", { x: 1, y: 2 }),
      "service-2": makeService("service-2", null),
    };

    await store.addDependency("acme", "workspace-1", null, "service-1", "service-2");

    expect(store.serviceMap["service-1"].position).toBeNull();
    expect(store.serviceMap["service-2"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });

  it("clears positions and bumps the epoch after a dependency is removed", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };
    store.dependencyMap = {
      "dep-1": {
        id: "dep-1",
        workspace_id: "workspace-1",
        project_id: null,
        from_service_id: "service-1",
        to_service_id: "service-2",
        dependency_type: "depends_on",
        created_at: "2026-10-25T00:00:00Z",
      },
    };

    await store.removeDependency("acme", "workspace-1", null, "dep-1");

    expect(store.serviceMap["service-1"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });

  it("clears positions after a service is created", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };

    await store.createService("acme", "workspace-1", "project-1", { name: "service-3" });

    expect(store.serviceMap["service-1"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });

  it("clears remaining positions after a service is deleted", async () => {
    const { store } = makeStore();
    store.serviceMap = {
      "service-1": makeService("service-1", { x: 1, y: 2 }),
      "service-2": makeService("service-2", { x: 3, y: 4 }),
    };

    await store.deleteService("acme", "workspace-1", "project-1", "service-1");

    expect(store.serviceMap["service-1"]).toBeUndefined();
    expect(store.serviceMap["service-2"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });
});

describe("ServicesStore.updateNodePosition", () => {
  it("uses the workspace endpoint when projectId is null", async () => {
    const { store, serviceService } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };

    await store.updateNodePosition("acme", "workspace-1", null, "service-1", null);

    expect(serviceService.updateWorkspaceService).toHaveBeenCalledWith("acme", "service-1", { position: null });
    expect(serviceService.updateNodePosition).not.toHaveBeenCalled();
    expect(store.serviceMap["service-1"].position).toBeNull();
  });

  it("uses the project endpoint when projectId is present", async () => {
    const { store, serviceService } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };

    await store.updateNodePosition("acme", "workspace-1", "project-1", "service-1", { x: 5, y: 6 });

    expect(serviceService.updateNodePosition).toHaveBeenCalledWith("acme", "workspace-1", "project-1", "service-1", {
      x: 5,
      y: 6,
    });
    expect(serviceService.updateWorkspaceService).not.toHaveBeenCalled();
  });

  it("restores the original position when the request fails", async () => {
    const { store, serviceService } = makeStore();
    const original = makeService("service-1", { x: 1, y: 2 });
    store.serviceMap = { "service-1": original };
    serviceService.updateWorkspaceService.mockRejectedValueOnce(new Error("nope"));

    await expect(store.updateNodePosition("acme", "workspace-1", null, "service-1", null)).rejects.toThrow("nope");

    expect(store.serviceMap["service-1"]).toStrictEqual(original);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
pnpm --filter=web exec vitest run core/store/service.store.test.ts
```

Expected: FAIL because `layoutEpoch`/`clearLayoutPositions` do not exist and `updateNodePosition` still calls the project endpoint.

- [ ] **Step 3: Add `layoutEpoch` to the store interface**

In `IServiceStore`, add:

```ts
  layoutEpoch: number;
  markLayoutChanged: () => void;
  clearLayoutPositions: () => void;
```

Change `updateNodePosition` in the interface to:

```ts
updateNodePosition: (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  serviceId: string,
  position: { x: number; y: number } | null
) => Promise<void>;
```

- [ ] **Step 4: Add the observable and actions**

In `ServicesStore`:

```ts
layoutEpoch: number = 0;
```

Register them in `makeObservable`:

```ts
      layoutEpoch: observable.ref,
      markLayoutChanged: action,
      clearLayoutPositions: action,
```

Add the methods near `setPeekService`:

```ts
markLayoutChanged = () => {
  this.layoutEpoch += 1;
};

clearLayoutPositions = () => {
  Object.entries(this.serviceMap).forEach(([serviceId, service]) => {
    if (service.position !== null) {
      set(this.serviceMap, [serviceId], { ...service, position: null });
    }
  });
  this.markLayoutChanged();
};
```

- [ ] **Step 5: Trigger the reset after topology mutations**

Change the success blocks to call the reset:

```ts
createService = async (workspaceSlug: string, workspaceId: string, projectId: string, data: Partial<IService>) => {
  const service = await this.serviceService.createService(workspaceSlug, workspaceId, projectId, data);
  runInAction(() => {
    set(this.serviceMap, [service.id], service);
    this.clearLayoutPositions();
  });
  return service;
};
```

```ts
createWorkspaceService = async (workspaceSlug: string, _workspaceId: string, data: Partial<IService>) => {
  const service = await this.serviceService.createWorkspaceService(workspaceSlug, data);
  runInAction(() => {
    set(this.serviceMap, [service.id], service);
    this.clearLayoutPositions();
  });
  return service;
};
```

```ts
deleteService = async (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => {
  await this.serviceService.deleteService(workspaceSlug, workspaceId, projectId, serviceId);
  runInAction(() => {
    this.removeLocalService(serviceId);
    this.clearLayoutPositions();
  });
};
```

```ts
deleteWorkspaceService = async (workspaceSlug: string, _workspaceId: string, serviceId: string) => {
  await this.serviceService.deleteWorkspaceService(workspaceSlug, serviceId);
  runInAction(() => {
    this.removeLocalService(serviceId);
    this.clearLayoutPositions();
  });
};
```

In `addDependency`:

```ts
runInAction(() => {
  set(this.dependencyMap, [dependency.id], dependency);
  this.clearLayoutPositions();
});
```

In `removeDependency`:

```ts
runInAction(() => {
  delete this.dependencyMap[dependencyId];
  this.clearLayoutPositions();
});
```

- [ ] **Step 6: Make `updateNodePosition` optimistic and scope-aware**

Replace the method with:

```ts
updateNodePosition = async (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string | null,
  serviceId: string,
  position: { x: number; y: number } | null
) => {
  const original = this.getServiceById(serviceId);
  if (!original) throw new Error("Service not found");
  try {
    runInAction(() => {
      set(this.serviceMap, [serviceId], { ...original, position });
    });
    const response = projectId
      ? await this.serviceService.updateNodePosition(workspaceSlug, workspaceId, projectId, serviceId, position)
      : await this.serviceService.updateWorkspaceService(workspaceSlug, serviceId, { position });
    runInAction(() => {
      set(this.serviceMap, [serviceId], response);
    });
  } catch (error) {
    console.error("Failed to update service position", error);
    runInAction(() => {
      set(this.serviceMap, [serviceId], original);
    });
    throw error;
  }
};
```

- [ ] **Step 7: Update the HTTP client**

In `apps/web/core/services/service.service.ts`, replace `updateNodePosition` with:

```ts
  async updateNodePosition(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string | null,
    serviceId: string,
    position: { x: number; y: number } | null
  ): Promise<IService> {
    return projectId
      ? this.updateService(workspaceSlug, workspaceId, projectId, serviceId, { position })
      : this.updateWorkspaceService(workspaceSlug, serviceId, { position });
  }
```

- [ ] **Step 8: Run tests to verify they pass**

```bash
pnpm --filter=web exec vitest run core/store/service.store.test.ts
```

Expected: PASS (existing tests + new tests).

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/store/service.store.ts apps/web/core/services/service.service.ts apps/web/core/store/service.store.test.ts
git commit -m "feat(web): reset service graph layout on topology changes"
```

---

### Task 5: Workspace-aware Re-layout and auto-fit

**Files:**

- Modify: `apps/web/core/components/services/graph/service-graph.tsx`
- Modify: `apps/web/core/components/services/graph/service-graph-canvas.tsx`

- [ ] **Step 1: Make drag and Re-layout workspace-aware**

In `service-graph.tsx`, read the new store values:

```ts
const {
  getGraphData,
  getWorkspaceGraphData,
  getServiceById,
  addDependency,
  removeDependency,
  updateNodePosition,
  layoutEpoch,
  markLayoutChanged,
} = useService();
```

Replace `handleNodeDragStop` with:

```ts
const handleNodeDragStop = useCallback(
  (serviceId: string, position: { x: number; y: number }) => {
    const targetProjectId: string | null = scope === "workspace" ? null : (pid ?? null);
    if (!slug || !workspaceId || (scope === "project" && !targetProjectId)) return;
    void updateNodePosition(slug, workspaceId, targetProjectId, serviceId, position).catch(() => {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: "Could not save node position. Please try again.",
      });
    });
  },
  [pid, scope, slug, updateNodePosition, workspaceId]
);
```

Replace `handleReLayout` with:

```ts
const handleReLayout = useCallback(async () => {
  const targetProjectId: string | null = scope === "workspace" ? null : (pid ?? null);
  if (!slug || !workspaceId || (scope === "project" && !targetProjectId)) return;
  const positioned = graphData.services.filter((service) => !isLiteService(service) && service.position !== null);
  setIsReLayouting(true);
  try {
    if (positioned.length > 0) {
      const results = await Promise.allSettled(
        positioned.map((service) => updateNodePosition(slug, workspaceId, targetProjectId, service.id, null))
      );
      if (results.some((result) => result.status === "rejected")) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message: "Could not reset layout. Please try again.",
        });
      }
    }
  } finally {
    markLayoutChanged();
    setIsReLayouting(false);
  }
}, [graphData.services, markLayoutChanged, pid, scope, slug, updateNodePosition, workspaceId]);
```

Pass the epoch to the canvas:

```tsx
<ServiceGraphCanvas
  graphData={graphData}
  projectLookup={projectLookup}
  layoutEpoch={layoutEpoch}
  onConnect={handleConnect}
  onEdgeDelete={handleEdgeDelete}
  onNodeDragStop={handleNodeDragStop}
  onNodeClick={handleNodeClick}
/>
```

- [ ] **Step 2: Accept `layoutEpoch` in the canvas**

In `service-graph-canvas.tsx`, add to `TServiceGraphCanvasProps`:

```ts
  /** Incremented by the store when topology/layout changes; triggers fitView. */
  layoutEpoch?: number;
```

Add to the destructured props with a default:

```ts
  layoutEpoch = 0,
```

- [ ] **Step 3: Fit the viewport when the epoch changes**

After the existing first-fit effect, add:

```ts
const lastLayoutEpoch = useRef(layoutEpoch);

useEffect(() => {
  if (layoutEpoch === lastLayoutEpoch.current) return;
  lastLayoutEpoch.current = layoutEpoch;
  if (!instance) return;

  let secondFrame = 0;
  const firstFrame = requestAnimationFrame(() => {
    secondFrame = requestAnimationFrame(() => {
      instance.fitView({ padding: 0.2, duration: 300 });
    });
  });

  return () => {
    cancelAnimationFrame(firstFrame);
    cancelAnimationFrame(secondFrame);
  };
}, [instance, layoutEpoch]);
```

Do not call `fitView` anywhere else; the initial-fit effect remains unchanged.

- [ ] **Step 4: Run web tests and typecheck**

```bash
pnpm --filter=web exec vitest run core/components/services/graph/node-sync.test.ts core/store/service.store.test.ts
pnpm --filter=web check:types
pnpm --filter=web check:lint
```

Expected: tests PASS, typecheck exits 0, lint exits 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/graph/service-graph.tsx apps/web/core/components/services/graph/service-graph-canvas.tsx
git commit -m "feat(web): re-layout graph on topology changes and fit viewport"
```

---

### Task 6: Verification and rollout

**Files:**

- No new files.

- [ ] **Step 1: Run the full web suite**

```bash
pnpm --filter=web test
```

Expected: all tests pass.

- [ ] **Step 2: Run the Rust checks**

```bash
cd apps/api-rs
cargo check -p api
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1
```

Expected: `cargo check` exits 0; all service workspace tests pass.

- [ ] **Step 3: Build web**

```bash
pnpm --filter=web build
```

Expected: build completes successfully.

- [ ] **Step 4: Rebuild and restart backend**

```bash
cd /home/ghifari/plane-for-itsm
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &
```

Poll until the build finishes, then verify:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200`.

- [ ] **Step 5: Restart live and web prod**

```bash
systemctl --user restart plane-live.service
sleep 10
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
systemctl --user restart plane-web-prod.service
sleep 5
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000
```

Expected: `200` and `200`.

- [ ] **Step 6: Manual smoke checklist**

1. Open a project Services page and switch to Graph.
2. Add a relationship from a node handle. Expected: graph re-layouts and fits without manual refresh.
3. Delete an edge. Expected: same.
4. Drag a node, refresh. Expected: position persists.
5. Click Re-layout. Expected: all visible nodes re-arrange and fit.
6. Open the workspace Services page with a project filter, repeat steps 2-5.
7. Add a dependency from the service detail Dependencies tab. Expected: graph is tidy when opened.
8. Leave the graph open through a health poll. Expected: no node movement.
9. Check a project-writer (non-workspace-admin) can drag and Re-layout services they can write.

If any step fails, fix and add a regression test before committing.

- [ ] **Step 7: Commit fixes only if step 6 required code changes**

```bash
git add -u
git commit -m "fix(web): address service graph layout smoke findings"
```

---

## Self-Review

- **Spec coverage:** Backend reset on service/dependency mutations, workspace-wide transaction behavior, rejected mutations untouched, `updated_at` untouched, node sync, store reset/epoch, workspace endpoint, Re-layout scope, auto-fit, tests, rollout — all mapped to tasks.
- **No placeholders:** All code, commands, and expected results are written out.
- **Type consistency:** `layoutEpoch`, `markLayoutChanged`, `clearLayoutPositions`, and `updateNodePosition(projectId: string | null, position: {x,y} | null)` are used consistently across store, client, graph, and canvas.
