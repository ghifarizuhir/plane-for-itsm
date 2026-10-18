# Workspace-level Sprints & Tracks (Pilot) — Design

**Date:** 2026-10-18
**Status:** Approved design; pending written-spec review

## Context

The fork already exposes workspace-level pages: Home, Scheduler, and the Service
catalog (`/[workspaceSlug]/services`, with a project filter via `?project=`). The
goal is to extend the same pattern to six project-scoped areas — Tickets,
Sprints, Tracks, Queues, KB, Intake — as workspace-level aggregate pages. These
are independent subsystems, so each area gets its own spec. This document is the
**pilot: Sprints and Tracks**, read-only aggregation with project filter and
click-through to existing project pages.

Sprints and Tracks are the pilot because the backend read endpoints already
exist and are correct:

- `GET /api/workspaces/:slug/cycles/` → `routes::cycle::workspace_cycles`
  (`apps/api-rs/crates/api/src/routes/cycle.rs:2968`). Filters to member
  projects, excludes archived cycles and archived projects, returns progress
  counts.
- `GET /api/workspaces/:slug/modules/` → `routes::module::workspace_modules`
  (`apps/api-rs/crates/api/src/routes/module.rs:2914`). Same scoping, plus
  `status`, `lead_id`, `member_ids`, and estimate points.

Both endpoints gate on any active workspace member and return only projects the
user is an active member of. The web data layer already exists:
`cycleService.getWorkspaceCycles` / `moduleService.getWorkspaceModules` and
store actions `fetchWorkspaceCycles` / `fetchWorkspaceModules`
(`apps/web/core/store/cycle.store.ts:397`, `apps/web/core/store/module.store.ts:271`).

**This is a web-only change. No backend, migration, or API changes.**

## Goals

- Workspace routes `/[workspaceSlug]/sprints` and `/[workspaceSlug]/tracks`
  listing all sprints/tracks across the user's member projects.
- Items grouped by state (sprints) / status (tracks).
- Project filter via `?project=` (catalog parity, URL-shareable) and search.
- Click-through into the existing project detail pages
  (`/[workspaceSlug]/projects/:projectId/cycles/:cycleId` and
  `.../modules/:moduleId`).
- A reusable "workspace aggregate list" shell so Tickets, KB, and Intake can
  follow the same pattern in their own specs.

## Non-goals

- Create/edit/delete from the workspace level.
- Workspace-level detail pages for sprints/tracks.
- Backend changes: no `?project=` API param, no `status` field added to the
  workspace cycles response, no Django/api-rs changes.
- Queues cleanup, Tickets, KB, Intake (separate specs).
- Pagination or virtualization (accepted for pilot; see Risks).

## Terminology

Sprints = cycles. Tracks = modules. UI labels reuse existing i18n keys:
`sidebar.cycles` ("Sprints"), `sidebar.modules` ("Tracks"), and the existing
project-level group/status labels.

## UX design

### Routes

Following the services pattern (`apps/web/app/routes/core.ts:123-130`):

- `:workspaceSlug/sprints` → `(projects)/sprints/page.tsx`, wrapped in a
  `layout.tsx` that renders `AppHeader` + breadcrumbs (mirrors
  `(projects)/services/layout.tsx`).
- `:workspaceSlug/tracks` → `(projects)/tracks/page.tsx`, same layout pattern.

Route block is placed next to the service catalog block in `core.ts`.

### Sidebar

In `packages/constants/src/workspace.ts`:

- Add `sprints` (`/sprints/`) and `tracks` (`/tracks/`) to
  `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS` and insert them into
  `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS` after Home:
  Home → **Sprints → Tracks** → Services → Scheduler → Docs.
- Labels reuse `sidebar.cycles` / `sidebar.modules`.
- Access: `ADMIN, MEMBER` (parity with the project sidebar, which excludes
  guests from cycles/modules).
- Register the item keys in `sidebar-item.tsx` `staticItems` and add icons in
  `helper.tsx` reusing `CyclesOutline` / `ModuleOutline` (same icons used in
  `project-navigation.tsx`).

Because web consumes built `@plane/constants` output, the package dist must be
rebuilt during verification (see Verification).

### Page layout

Both pages are thin `page.tsx` wrappers that read `?project=`, trigger the
workspace fetch, and render a list view:

- `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx`
- `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx`

Shared shell: `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`

- Header row: project filter (`CustomSelect` with "All projects" + member
  projects, exact copy of the catalog header) + search input + item count.
- Body: groups rendered via a render prop; only non-empty groups rendered.
- States: loading, global empty (no items at all), filtered-empty.

Row content (kept lean, read-only):

- **Sprint row:** status icon (`CycleGroupIcon`), name, project badge, date
  range, progress (completed/total counts). Click → project cycle detail.
- **Track row:** status, name, project badge, lead, target date, progress +
  estimate points. Click → project module detail.

### Filters

- Project filter: `?project=<projectId>` via `useSearchParams`, exactly like
  the catalog (`(projects)/services/page.tsx`). The param stays in the URL for
  shareability but filtering is client-side since the full member-project
  dataset is already fetched. Unknown/invalid project id falls back to "All
  projects".
- Search: local state, case-insensitive name match, not persisted to URL (same
  as catalog).
- Filters compose with AND.

### Grouping and ordering

**Sprints.** The workspace cycles endpoint intentionally omits `status`
(mirrors Django's workspace serializer), so status is derived client-side from
dates versus today:

- `draft`: `start_date` or `end_date` missing (also used for a reversed range
  `start > end`).
- `current`: `start_date <= today <= end_date`.
- `upcoming`: `start_date > today`.
- `completed`: `end_date < today`.

Group order: Draft, Current, Upcoming, Completed. Within groups: current →
`end_date` ascending; upcoming → `start_date` ascending; completed →
`end_date` descending; draft → keep API order (`created_at` desc).

**Tracks.** Group by the module `status` column. Group order and labels follow
the canonical `MODULE_STATUS` constant (`packages/constants/src/module.ts`):
backlog, planned, in-progress, paused, completed, cancelled. Within a group:
`target_date` ascending (nulls last), then name.

Helpers live in `apps/web/core/lib/workspace-aggregate.ts` with unit tests.

### Data flow

1. Page mounts → call `fetchWorkspaceCycles(slug)` / `fetchWorkspaceModules(slug)`.
   Loading state is local to the page (the stores' `fetchedMap` is per-project
   and a member project with zero items never gets flagged).
2. Select items: `project_id ∈ workspaceProjectIds` (the user's projects in the
   active workspace). Additionally hide items from projects where the feature
   is disabled (`project.cycle_view === false` / `project.module_view ===
false`), matching project sidebar behavior.
3. Apply `?project=` filter, then search, then group, then render.
4. Errors follow the existing catalog pattern (API layer notification, empty
   state shown).

### Permissions

No new gates. Backend already denies non-members (403) and returns only member
projects. Web mirrors this with sidebar access (`ADMIN, MEMBER`) and by using
`workspaceProjectIds` for the filter dropdown. A guest opening the URL directly
sees the empty state, not an error.

### i18n

- Reuse `sidebar.cycles` / `sidebar.modules` for nav and breadcrumb titles.
- Reuse existing sprint group labels and module status labels from the project
  UI.
- New keys only for workspace empty states (title + description per page) —
  added to `en` first; other locales via the translate skill when touched.

## Files

New:

- `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/page.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/page.tsx`
- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`
- `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx`
- `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx`
- `apps/web/core/lib/workspace-aggregate.ts` + `workspace-aggregate.test.ts`

Modified:

- `apps/web/app/routes/core.ts` (two routes)
- `packages/constants/src/workspace.ts` (nav items)
- `apps/web/core/components/workspace/sidebar/sidebar-item.tsx` (`staticItems`)
- `apps/web/core/components/workspace/sidebar/helper.tsx` (icons)
- i18n locale files (empty states)

## Testing

Vitest (`apps/web`, per repo convention: `core/lib/*.test.ts`):

- `deriveCycleStatus` boundaries: no dates → draft; today == start → current;
  today == end → current; start > today → upcoming; end < today → completed;
  reversed range → draft.
- Sprinter grouping: group order, empty groups skipped, in-group ordering.
- Track grouping: status group order, `target_date` ascending with nulls last.
- Filter helpers: project filter, search, feature-flag exclusion, composition.

No Rust tests: no backend changes.

## Verification

1. `pnpm --filter=web test` (targeted vitest files at minimum).
2. `pnpm check:types` and lint on touched files.
3. `pnpm --filter=@plane/constants build` (web consumes the dist) then
   `pnpm --filter=web build`.
4. `systemctl --user restart plane-web-prod.service`.
5. Smoke on the tunnel: sidebar shows Sprints/Tracks; both pages list items
   from member projects; `?project=` filters; click-through lands on the
   project detail page.

## Risks and open questions

- **Client-side status derivation:** if the workspace cycles endpoint later
  gains `status`, prefer the server value and drop the helper.
- **No pagination:** acceptable for pilot volume (member projects only);
  revisit with virtualization if workspaces grow large.
- **Guest visibility:** entries are `ADMIN/MEMBER` today; revisit if guests
  need read access.
- **Reuse debt:** the catalog's project-filter header remains a separate
  implementation; if a third area needs it, consider migrating the catalog
  onto the shell.
