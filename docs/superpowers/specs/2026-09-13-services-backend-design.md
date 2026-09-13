# Services Backend API (Rust api-rs) — Design

Date: 2026-09-13
Status: Approved (pending user review of this spec)
Scope: Backend (apps/api-rs) + frontend service-layer wiring (apps/web). Session auth only.
Predecessor: `2026-09-13-services-feature-design.md` (frontend-only v1, backend deferred).

## Goal

Make the existing Services feature functional against a real backend instead of the
localStorage mock. Build the Services API in the Rust service (`apps/api-rs`), which is the
primary and only API reachable through the default stack, and swap the frontend
`ServiceService` facade from the mock to HTTP calls without changing store or component
signatures.

## Context (why Rust, not Django)

- Default stack routes `/api/*` to the Rust API (`api:8000`). Django (`apps/api`) is opt-in
  under `--profile legacy` and is not reachable in the default stack.
- There is no `Service` model, table, or route anywhere in the backend today.
- Rust runs `sqlx::migrate!("../../migrations")` at boot; the Django `migrator` applies
  Django migrations separately and ignores unknown tables. New schema therefore goes in a
  Rust migration.
- Rust route handlers use runtime `sqlx::query` (not compile-time `query!`), so no offline
  sqlx data is required.

## Decisions (brainstormed & approved)

1. **Stack** — Rust `apps/api-rs` (not Django legacy, not both).
2. **Deliverable** — backend endpoints + migration, plus frontend wiring so the feature works
   end-to-end. Mock repository is kept in the repo as a dev fallback but is no longer imported
   by the runtime service.
3. **Auth surface** — session auth on `/api/` only. No public `api/v1` (API key) endpoints and
   no workspace-level list endpoint in this iteration.
4. **API shape** — project-level REST resources (`services/`, `service-dependencies/`,
   `service-issues/`) matching the store's three parallel list fetches and the existing
   `modules/` route style.
5. **Feature flag** — `service_view` project flag is out of scope. The frontend keeps
   `project?.service_view ?? true`, so Services remain visible.

## Data model & migration

New migration: `apps/api-rs/migrations/0003_services.sql` (plain, non-idempotent delta per
`apps/api-rs/migrations/README.md`). Column style follows Django-generated tables:
`created_at`/`updated_at NOT NULL`, `*_by_id` audit columns, `deleted_at` soft-delete. UUID
primary keys are generated in application code (uuid v4), matching the existing schema.

### `services`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | uuid PK | app-generated uuid v4 |
| `workspace_id` | uuid NOT NULL | FK `workspaces(id)` ON DELETE CASCADE |
| `project_id` | uuid NOT NULL | FK `projects(id)` ON DELETE CASCADE |
| `name` | varchar(255) NOT NULL | unique per project, case-insensitive + trimmed |
| `description` | text NOT NULL DEFAULT '' | plain text |
| `description_html` | text NOT NULL DEFAULT '' | rich text; plain string, matches mock contract |
| `status` | varchar(20) NOT NULL DEFAULT 'planned' | active/planned/maintenance/deprecated/retired |
| `criticality` | varchar(20) NOT NULL DEFAULT 'medium' | critical/high/medium/low |
| `type` | varchar(20) NOT NULL DEFAULT 'internal' | internal/external/infrastructure/third_party |
| `owner_id` | uuid NULL | FK `users(id)` ON DELETE SET NULL |
| `repository_url` | varchar(200) NULL | |
| `documentation_url` | varchar(200) NULL | |
| `position` | jsonb NULL | `{ "x": number, "y": number }` |
| `sort_order` | double precision NOT NULL DEFAULT 65535 | append = max(existing) + 65535 |
| `created_at` | timestamptz NOT NULL DEFAULT now() | |
| `updated_at` | timestamptz NOT NULL DEFAULT now() | |
| `created_by_id` | uuid NULL | |
| `updated_by_id` | uuid NULL | |
| `deleted_at` | timestamptz NULL | soft delete |

Indexes:

- Partial unique: `(project_id, lower(name)) WHERE deleted_at IS NULL` (case-insensitive
  uniqueness, matching mock behavior).
- `(project_id) WHERE deleted_at IS NULL`.

### `service_dependencies`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | uuid PK | |
| `workspace_id` | uuid NOT NULL | |
| `project_id` | uuid NOT NULL | |
| `from_service_id` | uuid NOT NULL | FK `services(id)` ON DELETE CASCADE; the dependent |
| `to_service_id` | uuid NOT NULL | FK `services(id)` ON DELETE CASCADE; the depended-upon |
| `created_at`, `updated_at` | timestamptz NOT NULL | |
| `created_by_id`, `updated_by_id` | uuid NULL | |
| `deleted_at` | timestamptz NULL | |

Edge direction: `from → to` means `from` **depends on** `to`.
Indexes: partial unique `(from_service_id, to_service_id) WHERE deleted_at IS NULL`;
`(project_id) WHERE deleted_at IS NULL`.

### `service_issues`

Bridge between services and work items (mirrors `module_issues`).

| Column | Type | Notes |
| --- | --- | --- |
| `id` | uuid PK | |
| `workspace_id` | uuid NOT NULL | |
| `project_id` | uuid NOT NULL | |
| `service_id` | uuid NOT NULL | FK `services(id)` ON DELETE CASCADE |
| `issue_id` | uuid NOT NULL | FK `issues(id)` ON DELETE CASCADE |
| `created_at`, `updated_at` | timestamptz NOT NULL | |
| `created_by_id`, `updated_by_id` | uuid NULL | |
| `deleted_at` | timestamptz NULL | |

Indexes: partial unique `(service_id, issue_id) WHERE deleted_at IS NULL`;
`(issue_id) WHERE deleted_at IS NULL`; `(project_id) WHERE deleted_at IS NULL`.

### Delete semantics

Deleting a service soft-deletes the service row and soft-deletes every dependency touching it
(either side) and every `service_issues` row for it, in one transaction. This mirrors the
mock's cascade and Django's `SoftDeletionQuerySet` behavior.

## API contract (session auth, `/api/`)

All paths are scoped to `workspaces/:slug/projects/:project_id`. `:pk` denotes a resource uuid.

```
GET    .../services/                 -> 200 IService[]
POST   .../services/                 -> 201 IService
GET    .../services/:pk/             -> 200 IService
PATCH  .../services/:pk/             -> 200 IService    (partial; used by update + position)
PUT    .../services/:pk/             -> 200 IService
DELETE .../services/:pk/             -> 204

GET    .../service-dependencies/     -> 200 IServiceDependency[]
POST   .../service-dependencies/     -> 201 IServiceDependency   body {from_service_id, to_service_id}
DELETE .../service-dependencies/:pk/ -> 204

GET    .../service-issues/           -> 200 TServiceWorkItemLink[]  (with computed issue_identifier/issue_name)
POST   .../service-issues/           -> 201 TServiceWorkItemLink    body {service_id, issue_id}
DELETE .../service-issues/:pk/       -> 204
```

### Response shapes (field-for-field `IService`)

Service: `id, workspace_id, project_id, name, description, description_html, status,
criticality, type, owner_id, repository_url, documentation_url, position, sort_order,
created_at, updated_at, created_by, updated_by`.

- `created_by`/`updated_by` are serialized from `created_by_id`/`updated_by_id`.
- `position` is `null` or `{x, y}`.
- `repository_url`/`documentation_url` are `null` when unset.

Dependency: `id, workspace_id, project_id, from_service_id, to_service_id, created_at`.

Work-item link: `id, service_id, issue_id, project_id, workspace_id, issue_identifier,
issue_name`. `issue_identifier` and `issue_name` are **computed** at read time by joining
`issues` and `projects` (identifier = project identifier + "-" + sequence id); they are not
stored.

### Create/update semantics (parity with the mock)

- **Defaults on create**: `status="planned"`, `criticality="medium"`, `type="internal"`,
  `position=null`, `sort_order=max(existing for project)+65535`; `name` defaults to
  `"Untitled service"` only if the request omits it.
- **Update strips identity**: `id`, `workspace_id`, `project_id`, `created_at` are never
  overwritten; `updated_at` is set to `now()`.
- **Name uniqueness**: case-insensitive and trimmed, per project, excluding self on update.
- **Link idempotency**: creating a link for an existing `(service_id, issue_id)` returns the
  existing row.

### Validation errors (server-side, messages verbatim from the mock)

| Condition | Status | Body |
| --- | --- | --- |
| Duplicate name | 400 | `{"error": "A service with this name already exists."}` |
| Dependency self-edge | 400 | `{"error": "A service cannot depend on itself."}` |
| Duplicate dependency | 400 | `{"error": "This dependency already exists."}` |
| Cycle-creating dependency | 400 | `{"error": "This dependency would create a cycle."}` |
| Source service missing | 400 | `{"error": "Source service not found."}` |
| Target service missing | 400 | `{"error": "Target service not found."}` |
| Service missing (update/delete) | 404 | `{"error": "Service not found"}` |
| Service missing (link create) | 404 | `{"error": "Service not found."}` |
| Unknown enum value | 400 | `{"error": "Invalid status"}` / `"Invalid criticality"` / `"Invalid type"` |
| Not permitted | 403 | `{"detail": "You do not have permission to perform this action."}` |

Cycle detection runs server-side with a recursive CTE: adding `from → to` is rejected when
`to` can already reach `from` (including `from == to`).

### Permissions

- Safe methods (GET): any active project member, including guests.
- Unsafe methods (POST/PUT/PATCH/DELETE): project ADMIN/MEMBER.
- Workspace admins are allowed as in existing routes; implemented via the reused
  `fetch_project_member_role` / `project_gate_allows` / `is_workspace_admin` helpers and the
  `deny` / `missing` response helpers.

## Rust implementation

New file `apps/api-rs/crates/api/src/routes/service.rs`, following the `module.rs` pattern
(documented constants for error strings, pure helpers with `#[cfg(test)]`, handlers using
runtime SQL).

- **Input structs**: `CreateService`, `PatchService`, `CreateDependency`, `CreateServiceIssue`
  (`serde::Deserialize`; PATCH fields optional).
- **Pure helpers (unit-tested)**:
  - `normalize_name(&str) -> String` (trim + lowercase).
  - `validate_choice(field, value, allowed) -> Result<(), String>`.
  - `would_create_cycle(edges: &[(Uuid, Uuid)], from, to) -> bool` (in-memory reachability).
  - `next_sort_order(max_existing: f64) -> f64` returns `max(0.0, max_existing) + 65535.0`.
- **Handlers**: `list`, `create`, `detail`, `patch`, `put`, `destroy`,
  `dependencies_list`, `dependencies_create`, `dependency_destroy`, `issues_list`,
  `issues_create`, `issue_destroy`.
- **SQL**: runtime `sqlx::query` / `query_as`; `Uuid::new_v4()` for ids; soft delete via
  `deleted_at = now()`; cycle check via recursive CTE; link list joins `issues` + `projects`
  to compute `issue_identifier` / `issue_name`.
- **Registration**: add `pub mod service;` to `routes/mod.rs`; add the `.route(...)` block to
  `main.rs` with contract comments (path + status codes + gate), consistent with existing
  blocks.
- **Parity inventory**: add a `service` domain to
  `apps/api-rs/crates/api/parity-inventory.json` (`schema_version: 2`), one entry per path,
  `rust_status: "implemented"`, `out_scope: false`, `django_source: "N/A (new ITSM feature,
  no Django parity)"`. This keeps the authoritative route list in sync and satisfies
  `route_inventory_test.rs`; no `adr` pointer is required for `implemented`.

## Frontend wiring

`apps/web/core/services/service.service.ts` becomes an HTTP client:

- `class ServiceService extends APIService`, `super(API_BASE_URL)` (pattern from
  `module.service.ts`).
- Signature-compatible: `workspaceId` arguments remain in the method signatures (the store
  still passes them) but are ignored; the backend derives workspace from the project.
- Method mapping:

| Method | HTTP |
| --- | --- |
| `getServices` | GET `/api/workspaces/{slug}/projects/{projectId}/services/` |
| `createService` | POST `/api/workspaces/{slug}/projects/{projectId}/services/` |
| `updateService` | PATCH `/api/workspaces/{slug}/projects/{projectId}/services/{id}/` |
| `updateNodePosition` | PATCH `/api/workspaces/{slug}/projects/{projectId}/services/{id}/` with `{position}` |
| `deleteService` | DELETE `/api/workspaces/{slug}/projects/{projectId}/services/{id}/` |
| `getDependencies` | GET `/api/workspaces/{slug}/projects/{projectId}/service-dependencies/` |
| `createDependency` | POST `.../service-dependencies/` with `{from_service_id, to_service_id}` |
| `deleteDependency` | DELETE `.../service-dependencies/{id}/` |
| `getWorkItemLinks` | GET `/api/workspaces/{slug}/projects/{projectId}/service-issues/` |
| `linkWorkItem` | POST `.../service-issues/` with `{service_id, issue_id}` |
| `unlinkWorkItem` | DELETE `.../service-issues/{id}/` |

### Error normalization

Add a small helper that converts an axios error into a JavaScript `Error`:

- `.message` = `response.data.detail` ?? `response.data.error` ?? first field error ??
  fallback string.
- Also attach `.detail` and `.error` properties from the response body.

This is required because `modal.tsx` reads `err.detail` / `err.error` while
`graph/service-graph.tsx` and `detail/dependencies.tsx` branch on `error instanceof Error`.
Without normalization, backend validation messages (duplicate name, DAG violations) would not
reach the toast.

### What does not change

- Store (`service.store.ts`), filter store, hooks, components, routes, types: unchanged.
- `service-mock.repository.ts` is kept in the repo as a dev fallback but is no longer imported
  by `service.service.ts`.
- `service.helpers.ts` continues to be used by the store for client-side filtering/sorting.

## Testing & verification

- **Rust unit tests**: pure helpers in `service.rs` `#[cfg(test)]` (name normalization, enum
  validation, in-memory cycle detection, sort order). Run with `cargo test -p api`.
- **Route/inventory gates**: `cargo test -p api --test route_inventory_test` after adding the
  `service` domain; ensures registered paths match the inventory.
- **Endpoint verification**: build and run the stack (`docker compose ... up --build`), then
  exercise CRUD, dependency self/duplicate/cycle rejection, position PATCH, and link/unlink
  (curl and/or UI). No DB-backed Rust integration harness exists, so this is manual/smoke.
- **Frontend checks**: `pnpm --filter=web check:types`, `pnpm --filter=web check:lint`,
  `pnpm --filter=web check:format`, then manual checklist: list/grid/graph render, create/edit/
  delete, drag position persists, DAG violations show the correct toast, link/unlink from the
  work-item detail and the service Work items tab.

## Out of scope (YAGNI)

- `service_view` project flag, project settings toggle, or any backend gating of the feature.
- Public `api/v1` (API key) endpoints and workspace-level service listing.
- Archive/restore, favorites, bulk operations, activity feed, webhooks, real-time sync.
- Migration of existing per-browser localStorage data into the database.
- A new frontend test runner or E2E automation.
- The UI does not call `GET services/:pk/`; the endpoint exists for REST completeness.
