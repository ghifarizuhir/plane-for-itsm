# Service Typed Relationships — Design

Date: 2026-10-20
Status: Approved (pending user review of this spec)
Scope: `apps/api-rs` (schema, API, tests) + `apps/web` (types, store, UI) + `packages/types` + i18n.

## Goal

Make service dependencies meaningful instead of a single untyped edge. Each edge stores a
`dependency_type` (`runs_on`, `connects_to`, `replicates_to`, `depends_on`, `uses`, `other`) and the
detail page presents a grouped relationship list with forward and inverse labels:

- A `runs_on` B → A shows **Runs on: B**, B shows **Hosts: A**.
- A `uses` B → A shows **Uses: B**, B shows **Used by: A**.

The relationship list moves from the detail sidebar to a dedicated section in the main column and
is clickable. Existing dependencies are migrated to `depends_on` with no data loss.

## Decisions (brainstormed & approved)

1. **Semantics** — one directed, typed edge per relation. Inverse labels are derived, never stored.
2. **Type set** — six types matching `docs/features/cmdb.md`: `runs_on`, `connects_to`,
   `replicates_to`, `depends_on`, `uses`, `other`.
3. **Location** — dedicated `Relationships` section in the main detail column; sidebar keeps only
   properties.
4. **Graph** — drag-to-connect creates `depends_on`; edges stay unlabeled and uncolored.
5. **Uniqueness** — the same service pair may have multiple edges if the type differs. Unique key is
   `(from_service_id, to_service_id, dependency_type)`.
6. **Edit type** — inline picker per row backed by a new PATCH endpoint.
7. **Cycle rule** — unchanged and applies to every type: A→B and B→A cannot coexist.
8. **Storage** — a `dependency_type` column on the existing `service_dependencies` table. No lookup
   table.

## Data Model

`packages/types/src/service/core.ts`:

```ts
export type TServiceDependencyType = "runs_on" | "connects_to" | "replicates_to" | "depends_on" | "uses" | "other";

export interface IServiceDependency {
  // ...existing fields
  dependency_type: TServiceDependencyType;
}
```

Migration `apps/api-rs/migrations/0023_service_dependency_type.sql`:

```sql
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

- Existing rows become `depends_on`.
- Soft delete semantics and the legacy nullable `project_id` column are unchanged.
- The old pair index must be dropped inside the same migration so old duplicates cannot block the
  new index.

## API (api-rs)

`DependencyRow` and `DEPENDENCY_SELECT` gain `dependency_type`. `dependency_json` serializes it.

### Create

`CreateDependency`:

```rust
pub struct CreateDependency {
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
    pub dependency_type: Option<String>, // default "depends_on"
}
```

- Validate against the six-value whitelist; invalid → 400 `Invalid dependency type.`
- Duplicate check becomes `(from_service_id, to_service_id, dependency_type)` → 400
  `This dependency already exists.`
- Cycle check is unchanged (all edges, all types) → 400 `This dependency would create a cycle.`
- Legacy project endpoints keep working; clients that omit `dependency_type` get `depends_on`.

### Update

New handlers:

- `PATCH /api/workspaces/:slug/service-dependencies/:pk/`
- `PATCH /api/workspaces/:slug/projects/:project_id/service-dependencies/:pk/`

Body:

```json
{ "dependency_type": "uses" }
```

Rules:

- Caller must be a writer on the edge's `from_service_id` (`gate_writer`), matching create.
- Missing/blank/invalid type → 400 `Invalid dependency type.`
- Changing to a type that already exists for the same pair → 400
  `This dependency already exists.`
- Type changes cannot introduce cycles, so the cycle CTE is not re-run.
- Response is the serialized dependency (same shape as create/list).

### Tests

`apps/api-rs/crates/api/tests/service_workspace_test.rs`:

- Update existing `CreateDependency` literals for the new optional field.
- Default type is `depends_on` when omitted.
- Create with `uses` echoes `dependency_type`.
- Same pair + same type → 400; same pair + different type → 201.
- PATCH changes the type and is reflected in list responses.
- PATCH to an existing triple → 400; invalid type → 400.
- Cycle rejection still applies between different types.
- Run serially:
  `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test service_workspace_test -- --test-threads=1`

## Frontend

### Service layer & store

- `service.service.ts`
  - `createDependency(..., projectId: string | null, dependencyType?: TServiceDependencyType)`.
  - `updateDependency(workspaceSlug, workspaceId, projectId: string | null, dependencyId, dependencyType)`.
  - `deleteDependency(..., projectId: string | null, ...)`.
  - `projectId === null` targets the canonical workspace endpoints
    (`/api/workspaces/:slug/service-dependencies/`); a non-null value keeps the compat project
    endpoints. This is required for cross-project edges from the workspace catalog.
- `service.store.ts`
  - `addDependency(..., projectId: string | null, ..., dependencyType = "depends_on")`.
  - `updateDependency(...)` updates `dependencyMap` in place.
  - `getDependenciesByService(serviceId)` exposes edges touching a service.
- `service.helpers.ts`
  - `dependencyExists(dependencies, from, to, type)`.
  - `validateDependency(dependencies, from, to, type)`.
  - Export the ordered type list used by the add form.
- `service-mock.repository.ts` follows the new signatures.
- Graph `handleConnect` relies on the store default, so graph-created edges are `depends_on`.

### Relationships section

New `apps/web/core/components/services/detail/relationships.tsx`, rendered in
`detail/root.tsx` after `ServiceProjectLinks` and before `ServiceWorkItems`, and in
`peek-overview/view.tsx` after `ServiceDescription` so the peek keeps relationship visibility. The
old `detail/dependencies.tsx` is removed; `ServiceDetailSidebar` no longer renders dependencies.

Data source: `dependencyMap` entries where the current service is either endpoint. This also fixes
the current bug where the sidebar filters by the legacy `project_id` and hides cross-project edges.

Groups are rendered only when non-empty:

| Type            | Outgoing (current is `from`) | Incoming (current is `to`) |
| --------------- | ---------------------------- | -------------------------- |
| `depends_on`    | Depends on                   | Required by                |
| `uses`          | Uses                         | Used by                    |
| `runs_on`       | Runs on                      | Hosts                      |
| `connects_to`   | Connects to                  | Connected from             |
| `replicates_to` | Replicates to                | Replicated from            |
| `other`         | Related to                   | Related to                 |

Rows:

- Service name links to `/:workspaceSlug/services/:serviceId`.
- Status and criticality badges plus owner project identifier when available.
- `lite` endpoints show a lock and name only (no edit/remove).
- Full endpoints get an inline type picker (PATCH on change, toast + revert on failure) and a Remove
  action. The picker is available from either side of the edge; after a successful PATCH the row
  moves to the matching outgoing/incoming group.

Add form:

- Type picker (six types, default `depends_on`), target service picker, Add button.
- Candidates: workspace services on workspace routes, project services when a `projectId` param is
  present; exclude the current service.
- The same target may be reused with a different type.
- Add is disabled when the exact triple already exists or a candidate is not selected.
- Server errors (duplicate/cycle) surface through the existing toast pattern.

### i18n

New keys under `service.relationships.*` for group labels, type names, placeholders, and errors.
Use the `translate` skill; `en` and `id` are required, other locales follow that skill's rules.

## Out of Scope

- Custom/user-defined relation types.
- Type-specific edge colors, labels, or legend in the graph.
- Editing edge direction.
- Impact analysis or blast-radius changes beyond the existing graph behavior.

## Verification

- Rust: `service_workspace_test` with `--test-threads=1`.
- Web: `pnpm --filter=web exec vitest run core/store/service.store.test.ts` plus new helper tests.
- `pnpm check:types` and `pnpm check:lint`.
- `pnpm --filter=web build`, then `systemctl --user restart plane-web-prod.service` and
  `curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/` (expect 200).
- Rebuild api-rs per AGENTS.md, then `curl http://localhost:8000/health`.

## Changelog

| Date       | Change                                                              |
| ---------- | ------------------------------------------------------------------- |
| 2026-10-20 | Initial design — typed dependency edges and grouped relationship UI |
