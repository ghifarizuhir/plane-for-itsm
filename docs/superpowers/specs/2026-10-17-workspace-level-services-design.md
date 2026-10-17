# Workspace-Level Services (Cross-Project Service Catalog) — Design

Date: 2026-10-17
Status: Approved in brainstorming (7 clarifying decisions, 5 design sections, 2026-10-17); awaiting written-spec review.
Scope: services move from project-owned rows to a workspace-level catalog with owner/consumer project links; cross-project dependency graph with scoped visibility (full vs lite); canonical workspace-level REST API + project-scoped compat layer; web UX moves to a workspace catalog page with a filtered project shortcut. Rust-only schema (`apps/api-rs/migrations/`, no Django model for services). No worker/Django changes.
Related: `docs/features/cmdb.md` (future workspace-level CI registry — same workspace-first direction, not implemented there), `2026-10-16-galileo-service-dependencies-design.md` (dependency tool semantics — this design lifts its same-project constraint), `2026-10-04-intake-itsm-type-service-gate-design.md` (intake service gate), `2026-09-13-services-feature-frontend.md`.

## Background

Services are currently **project-owned**:

- `services.project_id NOT NULL` + unique name per project (`services_unique_name_project_idx`) — `apps/api-rs/migrations/0003_services.sql:6-31`.
- `service_dependencies.project_id NOT NULL`; create validates that **both services belong to the queried project** (`service_exists(project_id, …)`, `apps/api-rs/crates/api/src/routes/service.rs:545-557,579-646`), cycle detection is per-project (`service.rs:610-624`).
- `service_issues.project_id` and `war_room_services.project_id` scope work-item and war-room links to a project (`0003_services.sql:53-74`, `0011_war_rooms.sql:35-50`).
- All routes are project-scoped (`/api/workspaces/:slug/projects/:projectId/services/…`, mount `main.rs:692-716`); gates are project membership (`gate_member`/`gate_writer`, `service.rs:129-157`).
- Web routes are project-scoped (`:workspaceSlug/projects/:projectId/services[/:serviceId]` — `apps/web/app/routes/core.ts:179-193`); `IService.project_id` drives URLs (`services-board-row.tsx:49`, `service-view-header.tsx:96`, `delete-service-modal.tsx:49`, war-room `service-map.tsx:93`); API client `apps/web/core/services/service.service.ts:40-177`.
- Consumers: intake service gate (`routes/intake.rs:1380-1412`), AI tools (`crates/ai/src/tools/services.rs` — `SERVICES_SQL:50-65` filters by `sv.project_id` membership), war rooms.

There is **no Django ORM model** for services — schema is Rust-only, so this design does not touch `apps/api/plane/db/models/`.

Motivation (user): the workspace — not the project — is the primary container. Projects are admin-managed **properties/links** of a service. Teams (and their per-project invites/membership) stay as they are; the service catalog and its dependency graph must span projects so incident responders can trace impact across team boundaries.

## Locked decisions (brainstorming 2026-10-17)

1. **Service is a workspace-level object.** Projects become links (`owner` / `consumer`), not the container. Membership stays per-project via the existing invite flows.
2. **Owner project is mandatory: exactly one.** Members of the owner project (role ≥ member) manage the service; workspace admins manage everything. Owner transfer is workspace-admin-only.
3. **Visibility = owner + consumer project membership.** A user sees a service (full) iff they are an active member of a linked project (owner or consumer) or a workspace admin. Guests with project membership see it read-only.
4. **Cross-project dependencies are allowed.** A dependency endpoint outside the viewer's visible set appears as **lite** (name, status, criticality only) so impact tracing stays complete without leaking details (`owner`, description, URLs).
5. **Work items / war rooms / intake may only pick services linked to their project** (owner or consumer). No auto-consumer-links on use.
6. **Consumer links are self-serve by the consumer project's admins**, revocable by owner project admins and workspace admins. No approval flow in v1.
7. **Workspace-first migration with compat layer**: workspace-level endpoints become canonical; project-scoped endpoints stay alive as filtered compat views until a later cleanup release. Phase 1 migration is additive (columns kept, dual-write), column drops happen in a separate cleanup release.
8. **No custom property schema in v1.** Project links are the "admin-defined properties" of a service; custom fields remain a possible later phase.
9. **Delete tightens to project-admin** (current code allows any member+ via `gate_writer`; the design changes delete to owner-project admin or workspace admin).

## Non-goals

- CMDB / configuration items, impact-analysis endpoint (see `docs/features/cmdb.md`).
- Custom admin-defined field schema on services.
- Consumer-link approval workflow or notifications.
- Backend service health.
- Removing the compat layer (separate cleanup spec/release).
- Changing project membership, invites, guests, or roles themselves.
- War rooms themselves becoming workspace-level (only their service attachment rule changes).

## Design

### 1. Data model & migration

**New table `service_project_links`** (`apps/api-rs/migrations/0020_workspace_services.sql`, additive phase 1):

```sql
CREATE TABLE public.service_project_links (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    project_id uuid NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    link_type character varying(20) NOT NULL CHECK (link_type IN ('owner', 'consumer')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    created_by_id uuid, updated_by_id uuid, deleted_at timestamptz,
    PRIMARY KEY (id)
);
CREATE UNIQUE INDEX service_project_links_service_project_idx
    ON public.service_project_links (service_id, project_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX service_project_links_one_owner_idx
    ON public.service_project_links (service_id) WHERE link_type = 'owner' AND deleted_at IS NULL;
CREATE INDEX service_project_links_project_idx
    ON public.service_project_links (project_id) WHERE deleted_at IS NULL;
CREATE INDEX service_project_links_service_idx
    ON public.service_project_links (service_id) WHERE deleted_at IS NULL;
```

Invariants: **at most one** owner per service (DB partial unique index); **at least one** owner enforced by the application (create service + owner link in one transaction; owner link removal only via transfer/deletion).

**Backfill** (same migration): for every existing non-deleted service insert an `owner` link with `project_id = services.project_id`; then dedupe + workspace-scope the name constraint:

- Detect case-insensitive duplicate `name` across the workspace; deterministically rename duplicates to `Name (PROJECT-IDENTIFIER)`, appending `(PROJECT-IDENTIFIER-<short id>)` on residual collision. Migration logs each rename.
- Create workspace unique index `services_unique_name_workspace_idx ON (workspace_id, lower(btrim(name))) WHERE deleted_at IS NULL`. Keep `services_unique_name_project_idx` (redundant) until phase 2.

**Phase-1 column handling (dual-write, rollback-safe):**

- `services.project_id`: keep, `ALTER … DROP NOT NULL`; new code writes it = owner project id on create, best-effort (kept for old-binary rollback and compat reads).
- `service_dependencies.project_id`: `ALTER … DROP NOT NULL`; new code writes owner project of `from_service` for same-project edges, `NULL` for cross-project edges. Old binary (rollback) sees same-project edges only.
- `service_issues.project_id`, `war_room_services.project_id`: unchanged (they mean the work-item/war-room project).

**Phase 2 (separate cleanup release, not this scope):** drop `services.project_id`, `service_dependencies.project_id`, `services_unique_name_project_idx`, legacy compat endpoints, deprecated JSON fields.

**Deletion semantics:** service soft-delete keeps links (queries filter `services.deleted_at IS NULL`). Deleting (soft) a project does not delete services; a service whose owner project is soft-deleted is visible/editable only by workspace admins until an owner transfer happens.

### 2. Access model

Full visibility: workspace admin ⇒ all services in the workspace; otherwise ⇒ services with a non-deleted link to a project where the user has an active `project_members` row (any role, including guest) under an active `workspace_members` row, and the project is not deleted.

Lite set: services that are endpoints of a dependency edge where the other endpoint is full-visible, minus the full-visible set.

| Action                                                                       | Allowed                                                                                                                                                |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Create service (body `owner_project_id`)                                     | ws admin, or member+ (15/20) of the owner project                                                                                                      |
| Read full (detail, list, dependency endpoint objects)                        | ws admin, or any active member of a linked project (owner or consumer; guests included, read-only)                                                     |
| Edit fields (name, description, status, criticality, type, owner user, URLs) | ws admin, or member+ of owner project (consumer read-only)                                                                                             |
| Delete/archive service                                                       | ws admin, or project admin (20) of owner project                                                                                                       |
| Transfer owner                                                               | ws admin only; old owner project keeps access via a demoted `consumer` link                                                                            |
| Add consumer link                                                            | ws admin, project admin of the consumer project (self-serve), project admin of owner project                                                           |
| Remove consumer link                                                         | ws admin, project admin of the consumer project (own link), project admin of owner project                                                             |
| Remove owner link                                                            | not directly — transfer owner or delete service                                                                                                        |
| Create dependency                                                            | ws admin, or member+ of `from_service`'s owner project; both endpoints must be full-visible **for the caller**                                         |
| Delete dependency                                                            | ws admin, or member+ of `from_service`'s owner project (no visibility requirement on `to`, so stale/hidden edges stay removable by the dependent side) |
| Link work item ↔ service                                                     | member+ of the work item's project, service full-visible, service linked to that project                                                               |
| Attach service to war room                                                   | member+ of the war room's project, service linked to that project                                                                                      |
| Accept intake triage suggestion (service)                                    | member+ of the intake's project, service linked to that project                                                                                        |
| AI tools                                                                     | run with caller's permissions; `list_services` returns full+visible set, dependencies include lite endpoints                                           |

Behavior changes to flag: service delete narrows from member+ to project-admin+; project-scoped reads widen from "project members only" to "any linked project member" (owner + consumer).

**Transfer-owner mechanics** (transactional): update the existing `owner` link's `project_id` to the new owner; if the new owner already had a `consumer` link, remove it; upsert a `consumer` link for the old owner project (keeps its access; removable afterwards by either party). Rejected when the target project is deleted.

**Cross-project dependency workflow:** because both endpoints must be full-visible to the creator, depending on another team's service requires a link to that service first. Consumer links are self-serve by the consumer project's admins, so this is a deliberate subscription step, not an approval/owner bottleneck. Services already visible as lite (via an existing edge) are still not selectable as new targets.

### 3. API surface

**Canonical (new, workspace-level):**

| Endpoint                                                                                        | Notes                                                                              |
| ----------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `GET /api/workspaces/:slug/services/`                                                           | visible services; filters `?project=&q=&status=&criticality=&link=`                |
| `POST /api/workspaces/:slug/services/`                                                          | body includes `owner_project_id`; transactional service + owner link               |
| `GET/PATCH/DELETE /api/workspaces/:slug/services/:id/`                                          | embed `project_links`; delete = soft delete + cascade deps/service-issues as today |
| `POST /api/workspaces/:slug/services/:id/transfer-owner/`                                       | body `{project_id}`; ws admin                                                      |
| `GET/POST /api/workspaces/:slug/services/:id/project-links/`, `DELETE …/project-links/:linkId/` | link management matrix §2                                                          |
| `GET/POST /api/workspaces/:slug/service-dependencies/`, `DELETE …/:depId/`                      | workspace-wide graph; cycle CTE without project filter                             |
| `GET/POST/DELETE /api/workspaces/:slug/service-issues/…`                                        | link/unlink work items                                                             |

**Compat (kept, project-scoped; delegates to canonical):** the project segment is only an additional scoping assertion — mutation gates stay canonical (e.g. `DELETE /projects/:pid/services/:id/` still requires project admin of the **owner** project or ws admin, regardless of `:pid`).

| Legacy endpoint                                  | New behavior                                                                                                                    |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| `GET /projects/:pid/services/`                   | services linked to `:pid` (owner + consumer), same JSON shape                                                                   |
| `POST /projects/:pid/services/`                  | create with `owner_project_id = :pid`                                                                                           |
| `GET/PATCH/DELETE /projects/:pid/services/:id/`  | must be linked to `:pid`, else 404                                                                                              |
| `GET/POST /projects/:pid/service-dependencies/`  | edges touching a service linked to `:pid`; non-visible endpoint projected lite; create requires `from_service` linked to `:pid` |
| `GET/POST/DELETE /projects/:pid/service-issues/` | unchanged shape; validation: service linked, issue in project                                                                   |

**JSON contract:**

- Full service: existing fields + `owner_project_id` + `project_links: [{id, project_id, link_type}]` + `access_level: "full"`. Deprecated `project_id` retained = owner project id.
- Lite service (dependency endpoints only): `{id, name, status, criticality, access_level: "lite"}` — no owner/description/URLs; no mutation affordances.
- Dependency rows: existing `from_service_id`/`to_service_id`/`created_at` + embedded `from_service` / `to_service` objects (full or lite per viewer).
- Fetching a service that is only lite for the caller ⇒ **404** (do not disclose more than the edge already shows).

**New Rust module** `crates/api/src/routes/service_access.rs`: `visible_service_ids(pool, user, ws)`, `lite_service_ids(...)`, `gate_service_writer(service_id)`, `gate_service_admin(service_id)`, `gate_service_create(owner_project_id)` — single source of truth used by services, war rooms, intake, and AI tooling.

### 4. Frontend & UX

- New canonical routes: `:workspaceSlug/services` (catalog, filters incl. project multi-select) and `:workspaceSlug/services/:serviceId`; old `:projectId/services/:serviceId` redirects to the new detail route (deep links keep working). Page files follow the existing `app/(all)/[workspaceSlug]/(projects)/…` conventions.
- Workspace sidebar gains a **Service catalog** entry (auto-scoped by visibility; visible to guests too, content-limited). Project sidebar "Services" entry stays and points to the catalog filtered `?project=<id>`.
- `service.service.ts` gains workspace-level methods; legacy methods remain against compat endpoints. `service.store.ts` gains workspace catalog state + keeps `fetchServicesForProject`.
- Board/list rows show owner-project badge; detail gains a **Linked projects** section (owner row + transfer action for ws admins; consumer rows with remove; "Add project" for project admins). Create form requires owner project (default = context project when started from the project shortcut); edit form shows owner read-only.
- Dependency section & graph: cross-project nodes carry project badge; lite nodes render muted + lock, click shows "Restricted — ask a member of this service's project" tooltip, no navigation. War-room `service-map.tsx` uses the same rendering; its "open service" URL switches to the workspace-level route.
- Issue/intake/peek service pickers need **no caller changes** — compat endpoint already returns exactly the linked set.

### 5. Impacted consumers

- **War rooms** (`routes/war_room.rs`): attach/detach validates service linked to the war room's project via `service_access`; service rows embed lite/full per viewer.
- **Intake** (`routes/intake.rs:1380-1412`): triage suggestion gate uses service linked to the intake's project.
- **AI tools** (`crates/ai/src/tools/services.rs`): `SERVICES_SQL` re-scoped from `sv.project_id` membership to link-based visibility; `get_service` dependencies include lite endpoints; the AI preamble/description notes cross-project dependencies replace the "same project only" constraint. Proposal tools (`manage_service_dependencies`) inherit the new server behavior.
- **Docs content** (`web/core/components/docs`, services topic): note cross-project catalog in a follow-up edit (not blocking).

## Files touched

Backend:

- `apps/api-rs/migrations/0020_workspace_services.sql` — new (links, backfill, dedupe, index, DROP NOT NULL).
- `apps/api-rs/crates/api/src/routes/service.rs` — rework routes/gates to the new model + compat delegation.
- `apps/api-rs/crates/api/src/routes/service_access.rs` — new gate/visibility helpers.
- `apps/api-rs/crates/api/src/main.rs` — mount workspace-level routes.
- `apps/api-rs/crates/api/src/routes/war_room.rs`, `routes/intake.rs` — service validation via `service_access`.
- `apps/api-rs/crates/ai/src/tools/services.rs` — visibility-scoped SQL + lite dependency projection.
- `apps/api-rs/crates/api/tests/service_workspace_test.rs` — new DB-backed integration suite.

Frontend:

- `apps/web/core/services/service.service.ts`, `core/store/service.store.ts`.
- `apps/web/app/routes/core.ts` + new page files for `:workspaceSlug/services[/:serviceId]` and the legacy redirect.
- `apps/web/core/components/services/*` — board row, list filters, detail (linked-projects section), dependency list/graph, delete modal, form (owner selector).
- `apps/web/core/components/workspace/sidebar/*` — catalog nav entry; `core/components/project/project-navigation.tsx` — shortcut URL.
- `apps/web/core/components/war-rooms/room/service-map.tsx` — detail URL + lite nodes.
- `packages/types/src/service/core.ts` — `owner_project_id`, `project_links`, `access_level`, lite type.

## Testing & verification

Rust integration (DB-backed; `DATABASE_URL=… cargo test -p api --test service_workspace_test -- --test-threads=1` because fixtures purge by slug prefix):

- Visibility matrix: ws admin all; owner member full; consumer member full; guest in linked project full read-only; outsider only lite via dependency; stranger nothing; lite fetch detail ⇒ 404.
- Create service + owner link atomicity; owner uniqueness (second owner link rejected); transfer owner (ws admin only; old owner demoted to consumer; target's consumer link absorbed).
- Consumer link: self-serve by consumer project admin; revoke by owner/ws admin; linked-set reflected in compat list.
- Dependencies: cross-project create allowed; cycle CTE workspace-wide; delete by `from` writer; lite projection in response for non-visible endpoint.
- Compat endpoints preserve legacy JSON shape (`project_id` present) and linked-set semantics; issue link/war-room attach/intake gate reject unlinked service with 404.
- AI `list_services` / `get_service` visibility + lite endpoints.
- Migration test data: duplicate service names across projects get renamed, backfilled owner links exist, workspace unique index holds.

Frontend (vitest via `pnpm --filter=web test`): lite node not navigable; legacy route redirect; catalog filter by project; linked-projects section permission rendering; store scoping.

Checks: `pnpm check` (format/lint/types) and cargo fmt/clippy per repo config.

Deploy order (must follow): 1) API release (migration + canonical + compat), verify `/health`; 2) web release (new routes + redirects), rebuild + restart per `AGENTS.md` (prod static build or dev server), verify `:3100/live/health/` after restart. DB backup before migration; migration logs duplicate-name renames.

Manual smoke: create service in P1 → consumer-link P2 → L1 member sees catalog filtered; P2 member links work item + attaches war room service; add dependency to a P3-owned service → P1 member sees lite endpoint (no detail), P3 member sees full graph both directions; P4 member sees nothing; ws admin transfers owner.
