# Workspace Service Catalog Grouping — Design

Date: 2026-10-21
Status: Approved (pending user review of this spec)
Scope: Frontend-only (`apps/web`). Backend, API, store, and data model unchanged.

## Goal

Make the workspace service catalog (`/:workspaceSlug/services`) easier to scan:

- Group services by owner application with a sticky section header.
- Make every service row compact and aligned so more services fit on screen.
- Apply the compact row treatment to the project-scoped services list too, while keeping that list flat.

## Current State

- `apps/web/core/components/services/workspace/workspace-services-list-view.tsx` renders a flat
  list of `ServicesBoardRow` with `px-3 py-2.5` rows, a health pill, incidents, deploy time, and an
  owner avatar. There is no column header and no grouping.
- `apps/web/core/components/services/board/services-board-row.tsx` is shared by the workspace list
  and the project-scoped `ServicesBoard`.
- `apps/web/core/components/services/board/services-board.tsx` renders a column header and the
  flat project list.
- Services already expose `owner_project_id`, and projects expose `name` and `identifier` through
  the existing stores.

## Decisions (brainstormed & approved)

1. **Grouping scope** — only the workspace catalog is grouped. The project-scoped list stays flat.
2. **Group key** — `owner_project_id`. Services whose owner project is missing or cannot be
   resolved go into an **Unassigned** group.
3. **Group order** — project name A–Z (case-insensitive); **Unassigned** always last.
4. **Within-group order** — service name A–Z (case-insensitive), with id as a tie-breaker.
5. **Group header** — sticky at the top of the scroll container, showing the project identifier
   badge (when present), project name, and a numeric count.
6. **Rows inside groups** — hide the project identifier badge and owner avatar because the group
   header already identifies the application. Health, incidents, and last deploy remain.
7. **Compact rows everywhere** — both workspace and project lists use the tighter row height and
   typography. Project list keeps its column header (also compacted).
8. **Criticality badge** — show it only for `critical` services; non-critical values are omitted to
   reduce row clutter.
9. **No new i18n keys** — the Unassigned label reuses `common.unassigned`.
10. **No change to peek** — clicking a row still opens the existing service peek.

## Grouping Algorithm

New pure helper: `apps/web/core/components/services/workspace/service-groups.ts`.

```ts
export type TServiceGroup = {
  key: string; // project id or "unassigned"
  projectId: string | null;
  identifier: string | null;
  name: string; // empty for the Unassigned group; the component supplies the label
  serviceIds: string[];
};

export function buildWorkspaceServiceGroups(
  serviceIds: string[],
  getServiceById: (id: string) => { name: string; owner_project_id: string | null } | undefined,
  getProjectById: (id: string) => { name: string; identifier: string } | undefined
): TServiceGroup[];
```

Rules:

1. Resolve each service's owner project. Missing service or missing/unresolvable project →
   `unassigned`.
2. Skip services that do not resolve to a service record.
3. Sort groups by `name` with `localeCompare(..., undefined, { sensitivity: "base" })`;
   `unassigned` last.
4. Sort each group's `serviceIds` by resolved service name with the same comparison; tie-break by id.
5. Omit empty groups.
6. The function returns new arrays and does not mutate its inputs.

## UI

### Workspace Catalog

`WorkspaceServicesListView` replaces the flat `filteredIds.map(...)` block with grouped rendering:

- Derive groups from the existing `filteredIds` so search and the project filter still apply.
- Render each group as:
  - A sticky header: `sticky top-0 z-[5] flex items-center justify-between border-b border-subtle bg-layer-2 px-3 py-1`.
  - Left: identifier badge (when present) + application name or `t("common.unassigned")`.
  - Right: numeric count in a small pill (`text-10 tabular-nums`).
- Render rows with `<ServicesBoardRow showProject={false} showOwner={false} />`.
- Keep the outer scroll container (`min-h-0 flex-1 overflow-y-auto`); sticky positioning is relative
  to that container.

### Compact Row

`ServicesBoardRow` changes:

- Row: `px-3 py-1.5` (was `py-2.5`).
- Name: `text-12 font-medium` (was `text-13`).
- Project identifier badge: `text-9` (was `text-10`).
- Criticality badge: rendered only when `critical`; `text-9`.
- Column widths: health `w-[90px] sm`, incidents `w-[100px] md`, deploy `w-[90px] lg`, owner
  `w-[40px] xl`.
- Add props:
  - `showProject?: boolean` (default `true`)
  - `showOwner?: boolean` (default `true`)
- The health rail stays.

### Project List Header

`ServicesBoard` keeps its structure but uses compact header classes:

- Header: `px-3 py-1 text-9 font-medium uppercase tracking-wide`.
- Column widths match `ServicesBoardRow`.

## Files

New:

- `apps/web/core/components/services/workspace/service-groups.ts`
- `apps/web/core/components/services/workspace/service-groups.test.ts`

Modified:

- `apps/web/core/components/services/workspace/workspace-services-list-view.tsx`
- `apps/web/core/components/services/board/services-board-row.tsx`
- `apps/web/core/components/services/board/services-board.tsx`

## Edge Cases

- Service with no owner project → Unassigned group, last.
- Owner id points to a project missing from the store → Unassigned group.
- Duplicate service ids in the input → rendered once (first occurrence wins).
- Search/filter removes all services in a group → the group is omitted.
- Project filter active → a single group (or Unassigned) is rendered; the header stays.
- Long application names truncate in the sticky header; the count remains visible.
- Row click still triggers peek on desktop and navigation on mobile through the existing hook.

## Non-Goals

- No backend/API/store changes.
- No new i18n keys.
- No collapse/expand control for groups.
- No changes to the graph layout.
- No changes to the service detail pane or peek content.

## Verification

1. Unit tests for `buildWorkspaceServiceGroups`:
   - groups by owner project;
   - sorts groups A–Z and places Unassigned last;
   - sorts services within groups A–Z;
   - merges unresolved owners into Unassigned;
   - omits empty groups and deduplicates ids.
2. `pnpm --filter=web exec vitest run core/components/services/workspace/service-groups.test.ts`
3. `pnpm --filter=web test`
4. `pnpm --filter=web check:types`
5. `pnpm --filter=web check:lint`
6. `pnpm --filter=web build`
7. Manual checks on `/services` and a project services board:
   - sticky group headers;
   - compact rows;
   - search and project filter;
   - peek still works;
   - project list remains flat with a compact header.
