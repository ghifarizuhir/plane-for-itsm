# Services Feature (Project-level, Graph-based) — Design

Date: 2026-09-13
Status: Approved (pending user review of this spec)
Scope: Frontend-only v1 (apps/web + packages/types + i18n). Backend deferred.

## Goal

Add a first-class project feature **Services** alongside Work items, Cycles, Modules, and Pages. A Service is a node in a service catalog; services can depend on each other (`depends on`), forming a directed dependency graph. Work items can be linked to one or more services, and each service lists its linked work items.

This iteration delivers the frontend: types, a localStorage-backed mock service layer, MobX stores, routes/navigation, list/grid/graph views, service detail, and work-item linking. No Django model/API is built yet.

## Decisions (brainstormed & approved)

1. **Concept** — Service catalog + dependency graph. Each service is a node; edges are directed `A depends on B`. Work items link to services many-to-many (mirrors `ModuleIssue`).
2. **Work item ↔ Service** — many-to-many; visible from both the work-item detail and the service detail.
3. **Edge model** — a single relation type: directed `depends-on`. Graph must stay a DAG (reject self, duplicate, and cycle-creating edges).
4. **Service fields** — "service catalog dasar": name, description (rich text), status, owner/lead, criticality (tier), type, repository URL, documentation URL.
5. **UI structure** — List + Detail + Graph. Graph is a view mode of the list page; detail page has Overview / Work items / Dependencies tabs.
6. **Graph interaction** — auto layered layout (dagre) with draggable nodes (position persisted); create edges by dragging from a node handle; **Re-layout** button.
7. **Scope** — frontend first; backend/API deferred.
8. **Mock data** — localStorage-backed repository with auto-seed; async service methods so the implementation can later be swapped to HTTP without touching stores/components.
9. **Architecture** — mirror the Module architecture (types → service layer → MobX store → components/routes).
10. **Graph library** — `@xyflow/react` (React Flow) + `dagre` for layout.

## Data model & types

Types live in `packages/types/src/service/` (`core.ts`, `filters.ts`, `index.ts`), exported from `packages/types/src/index.ts`.

### `IService`

| Field                      | Type                               | Notes                                                       |
| -------------------------- | ---------------------------------- | ----------------------------------------------------------- |
| `id`                       | `string`                           | uuid                                                        |
| `workspace_id`             | `string`                           |                                                             |
| `project_id`               | `string`                           | project-scoped                                              |
| `name`                     | `string`                           | unique per project (enforced in mock)                       |
| `description`              | `string`                           | plain text                                                  |
| `description_html`         | `string`                           | rich text                                                   |
| `status`                   | `TServiceStatus`                   | `active \| planned \| maintenance \| deprecated \| retired` |
| `criticality`              | `TServiceCriticality`              | `critical \| high \| medium \| low`                         |
| `type`                     | `TServiceType`                     | `internal \| external \| infrastructure \| third_party`     |
| `owner_id`                 | `string \| null`                   | user/lead                                                   |
| `repository_url`           | `string \| null`                   |                                                             |
| `documentation_url`        | `string \| null`                   |                                                             |
| `position`                 | `{ x: number; y: number } \| null` | graph node position; `null` = use auto-layout               |
| `sort_order`               | `number`                           |                                                             |
| `created_at`, `updated_at` | `string`                           | ISO                                                         |
| `created_by`, `updated_by` | `string \| null`                   |                                                             |

### `IServiceDependency` (edge)

| Field                        | Type     | Notes                     |
| ---------------------------- | -------- | ------------------------- |
| `id`                         | `string` |                           |
| `workspace_id`, `project_id` | `string` |                           |
| `from_service_id`            | `string` | the dependent service     |
| `to_service_id`              | `string` | the service depended upon |
| `created_at`                 | `string` |                           |

- Direction: edge `from → to` means `from` **depends on** `to`.
- DAG rules enforced in the service layer: no self-edge, no duplicate pair `(from, to)`, no edge that introduces a cycle.

### `TServiceWorkItemLink` (mirrors `ModuleIssue`)

| Field                        | Type     | Notes                       |
| ---------------------------- | -------- | --------------------------- |
| `id`                         | `string` |                             |
| `service_id`, `issue_id`     | `string` |                             |
| `project_id`, `workspace_id` | `string` |                             |
| `issue_identifier?`          | `string` | snapshot for mock rendering |
| `issue_name?`                | `string` | snapshot for mock rendering |

### Filters / display

- `TServiceFilters`: `status[]`, `criticality[]`, `type[]`, `searchQuery`.
- `TServiceDisplayFilters`: `layout: "list" | "grid" | "graph"`, `order_by: "name" | "-created_at" | "-updated_at" | "criticality" | "status"`.

### Mock storage (localStorage)

- Key: `plane:services:<workspaceSlug>:<projectId>`.
- Value: `{ version: 1, services: IService[], dependencies: IServiceDependency[], links: TServiceWorkItemLink[] }`.
- Seed when key absent: ~6 sample services, several dependency edges, and 2 sample work-item links (with denormalized `issue_identifier`/`issue_name` snapshots).
- `position` per service is persisted, so drag survives refresh.

## Architecture

Mirrors the Module feature. Feature is frontend-only for v1; the service layer is the seam that will later be swapped to HTTP.

```
packages/types/src/service/{core.ts, filters.ts, index.ts}
apps/web/core/services/service.service.ts          # async API-shaped surface
apps/web/core/services/service-mock.repository.ts  # localStorage CRUD + seed + DAG rules
apps/web/core/store/service.store.ts               # ServicesStore (root: `service`)
apps/web/core/store/service_filter.store.ts        # ServiceFilterStore (root: `serviceFilter`)
apps/web/core/hooks/store/use-service.ts
apps/web/core/hooks/store/use-service-filter.ts
apps/web/core/components/services/...
apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/...
```

### Service layer

`ServiceService` exposes async methods whose signatures/return shapes match the eventual HTTP endpoints:

```
getServices(workspaceSlug, projectId)            -> Promise<IService[]>
getServiceDetails(workspaceSlug, projectId, id)  -> Promise<IService>
createService(...) / updateService(...) / deleteService(...)
getDependencies(workspaceSlug, projectId)        -> Promise<IServiceDependency[]>
createDependency(...) / deleteDependency(...)
getWorkItemLinks(serviceId) / linkWorkItems(...) / unlinkWorkItem(...)
updateNodePosition(serviceId, position)
```

Each method delegates to `service-mock.repository.ts`. DAG validation (self/duplicate/cycle, via reachability check) lives here so components stay thin and errors surface as toasts. This is the only layer that needs changing when the backend lands.

### Stores

- `ServicesStore` (root field `service`):
  - observable: `loader`, `serviceMap: Record<string, IService>`, `dependencyMap: Record<string, IServiceDependency>`, `workItemLinkMap: Record<string, TServiceWorkItemLink>`, `fetchedMap`
  - `computedFn`: `getServiceById`, `getProjectServiceIds`, `getFilteredServiceIds`, `getDependenciesByProject`, `getWorkItemLinksByService`, `getGraphData`
  - actions: `fetchServices`, `fetchServiceDetails`, `createService`, `updateService`, `deleteService`, `addDependency`, `removeDependency`, `linkWorkItems`, `unlinkWorkItem`, `updateNodePosition`
- `ServiceFilterStore` (root field `serviceFilter`): `displayFilters`, `filters`, `searchQuery`; persisted to localStorage; re-initialized via `reaction` on `router.projectId` change (mirrors `module_filter.store.ts`).
- Registered in `core/store/root.store.ts` (fields + instantiation + `resetOnSignOut`).
- Hooks `useService()` and `useServiceFilter()` follow `use-module.ts` / `use-module-filter.ts`.

## Routing & navigation

Route group: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/`

- `(list)/layout.tsx` — `AppHeader` + `ContentWrapper`
- `(list)/page.tsx` — list page; toolbar view toggle **List | Grid | Graph**
- `(list)/header.tsx`, `(list)/mobile-header.tsx` — title, **Add service**, filters, order-by
- `(detail)/layout.tsx`
- `(detail)/[serviceId]/page.tsx` — tabs **Overview | Work items | Dependencies**

Graph is a view mode of the list page (not a separate route) and renders the whole project's services, honoring active filters.

### Feature flag

- Add `service_view?: boolean` to `IPartialProject` (`packages/types/src/project/projects.ts`) so backend wiring is trivial later.
- v1: `shouldRender: project?.service_view ?? true` → always visible, since no backend returns the field yet.
- Project settings feature toggle is deferred.

### Navigation registration (4 places)

1. `core/components/workspace/sidebar/project-navigation.tsx` — entry `key: "services"`, `sortOrder: 4` (module=3, pages=5), icon from `@makeplane/propel/icons`, `shouldRender: project?.service_view ?? true`.
2. `core/components/navigation/use-navigation-items.ts` — same entry.
3. `core/components/navigation/tab-navigation-utils.ts` — `tabUrlMap.services`.
4. i18n keys (`sidebar.services`, headers/labels) under `packages/i18n/src/locales` — performed with the `translate` skill.

## Components & graph

`apps/web/core/components/services/`:

- `index.ts` barrel
- `services-list-view.tsx` — root; reads filtered ids from store, renders `list` / `grid` / `graph`
- `service-list-item.tsx`, `service-card-item.tsx` — name, status, criticality, type, owner
- `service-view-header.tsx` + mobile variant — title, Add service, view toggle, filters, order-by
- `service-form.tsx`, `modal.tsx`, `delete-service-modal.tsx`
- dropdowns: `status`, `criticality`, `type`, `owner`; `filters/` and `applied-filters/` mirroring module
- `detail/` — `root.tsx`, `header.tsx`, `tabs.tsx`, `overview.tsx` (info + dependency list), `work-items.tsx` (linked work items + unlink), `dependencies.tsx` (add/remove edges)
- `select/service-select.tsx` — selector mounted in work-item detail (mirrors `issues/issue-detail/module-select.tsx`)

Graph (`graph/`):

- `service-graph.tsx` — React Flow canvas (`@xyflow/react`) with `Controls` + `MiniMap`
- `service-node.tsx` — custom node: name, status dot, criticality badge, source/target handles
- `use-graph-layout.ts` — `dagre` layered layout; services with a saved `position` use it, others get dagre defaults
- Behavior: drag node → `updateNodePosition` on drag stop (persisted); drag handle → `addDependency` (DAG validation, toast on violation); select edge → delete; click node → open detail; **Re-layout** clears positions and re-runs dagre; filters hide nodes and their edges.

Work-item integration:

- Insert `ServiceSelect` into the work-item detail properties (following `ModuleSelect`).
- Service detail **Work items** tab lists linked issues and supports unlink/add via the existing issue store.

## Dependencies

Added via the pnpm catalog (`pnpm-workspace.yaml`) and `apps/web/package.json`:

- `@xyflow/react`
- `dagre`
- `@types/dagre` (dev)

## Testing & verification

`apps/web` has no configured test runner (only `apps/live` and `packages/codemods` use vitest). Verification for v1:

1. `pnpm --filter=web check:types` (after `react-router typegen`).
2. `pnpm --filter=web check:lint` and `check:format` (or `pnpm fix`).
3. Manual checklist against `pnpm --filter=web dev`:
   - Services entry appears in the project sidebar.
   - List / Grid / Graph render; filters and order-by work.
   - Create / edit / delete service.
   - Drag node persists after refresh; **Re-layout** works.
   - Connecting an edge creates a dependency; self/duplicate/cycle are rejected with a toast.
   - Link/unlink work items from the work-item detail and from the Work items tab.

Pure logic (DAG validation, mock repository seed/CRUD) is written as isolated functions to ease future unit tests. Adding a frontend test runner is a follow-up, not v1.

## Out of scope (YAGNI)

- No Django model, migration, or API; no public `api/v1` or `space` endpoints.
- No project settings feature toggle / `service_view` backend control.
- No peek overview, archive/restore, favorites, bulk operations, or activity feed.
- No real-time sync; localStorage data is per-browser and not shared.
- Graph not optimized for hundreds of nodes.
- No new frontend test runner or E2E automation in this iteration.
