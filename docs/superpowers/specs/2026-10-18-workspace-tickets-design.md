# Workspace-level Tickets — Design

**Date:** 2026-10-18
**Status:** Approved design; pending written-spec review

## Context

This is the second sub-project of the workspace-level aggregation series
(Sprints & Tracks shipped first; see
`docs/superpowers/specs/2026-10-18-workspace-sprints-tracks-design.md`). Goal:
a simple, read-only workspace page listing Tickets (= issues) across the
projects the user belongs to, with a project filter and search, grouped by
state group, with click-through to the existing work item detail page.

**Important overlap note:** the fork already has a rich workspace issues
experience under **Queues** (`/workspace-views/all-issues`) — the full issue
layout (list/kanban/calendar) with saved global views, powered by
`GET /api/workspaces/:slug/issues/`. The Tickets page is deliberately NOT a
replacement: it is a simple aggregate list (catalog/sprints/tracks pattern),
while Queues remains the power-user surface for advanced filters and saved
views.

The backend already exists and needs no changes:

- `GET /api/workspaces/:slug/issues/` → `routes::issue_lists::workspace_issues`
  (`apps/api-rs/crates/api/src/routes/issue_lists.rs:620`). Cursor pagination,
  complex `filters` tree, `order_by`. Permission scoping is server-side: only
  projects where the user has an active membership; guests with
  `guest_view_all_features = false` are scoped to tickets they created
  (`push_ws_permission`, `issue_lists.rs:491`).
- Web client method already exists:
  `workspaceService.getViewIssues(workspaceSlug, params, config)`
  (`apps/web/core/services/workspace.service.ts:272`).
- Workspace states are already loaded on every workspace page
  (`workspace-wrapper.tsx:112` → `fetchWorkspaceStates`), so state lookups are
  available without extra fetching.

**This is a web-only change. No backend, migration, or API changes.**

## Goals

- Workspace route `/[workspaceSlug]/tickets` listing tickets across member
  projects, grouped by state group, 100 per page with "Load more".
- Project filter via `?project=` (catalog parity) and name search, both
  server-side.
- Click-through to the existing browse detail page
  (`/[workspaceSlug]/browse/{IDENTIFIER}-{sequence_id}`).
- Reuse the `WorkspaceAggregateList` shell from the Sprints & Tracks pilot
  (one small addition: optional `footer` prop for the Load more button).

## Non-goals (v1)

- Creating/editing tickets from the workspace level.
- Advanced filters (state, priority, assignee, labels, dates), saved views,
  custom grouping/ordering, board/kanban layouts — those stay in Queues.
- Peek sidebar on the Tickets page; no changes to global views/Queues.
- Backend changes of any kind.

## Terminology

Tickets = issues. Nav and labels reuse `sidebar.work_items` ("Tickets").
State groups use `STATE_GROUPS` (`packages/constants/src/state.ts`).

## UX design

### Routes

Following the sprints/tracks pattern (`apps/web/app/routes/core.ts`):

- `:workspaceSlug/tickets` → `(projects)/tickets/page.tsx`, wrapped in
  `layout.tsx` with `AppHeader` + breadcrumb ("Tickets",
  `WorkItemsOutline` icon), mirroring `(projects)/sprints/layout.tsx`.

### Sidebar

In `packages/constants/src/workspace.ts`:

- Add `tickets` (`/tickets/`) to `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`
  and insert into `..._LINKS` after Home:
  Home → **Tickets** → Sprints → Tracks → Services → Scheduler → Docs.
- Label `sidebar.work_items`, icon `WorkItemsOutline` (same as the project
  sidebar), access: `ADMIN, MEMBER, GUEST` (parity with the project-level
  Tickets entry; the endpoint allows guests with server-side scoping).
- Register `tickets` in `sidebar-item.tsx` `staticItems` and add the icon case
  in `helper.tsx`.

### Page layout

- `page.tsx` reads `?project=` and renders
  `core/components/issues/workspace/workspace-tickets-list-view.tsx`.
- The list view renders the shared `WorkspaceAggregateList` shell:
  project filter (`?project=`, same `CustomSelect` as the catalog), search
  input, count (`total_results`), grouped rows, loading/empty states.
- Shell addition: optional `footer?: ReactNode` prop, rendered at the end of
  the scroll area (used for the Load more button). No other shell changes.

### Rows

Each row (using the existing `ListItem` component like sprints/tracks):

- Prepend: `StateGroupIcon` (`stateGroup` from the row, color from
  `STATE_GROUPS`).
- Title: `{IDENTIFIER}-{sequence_id}` in muted text followed by the ticket
  name (e.g. `ENG-123 Fix login`).
- Append: project name badge, `PriorityIcon` (`priority` field), up to two
  assignee avatars (`assignee_ids` → `useMember().getUserDetails`), updated
  date (`renderFormattedDate`).
- Click: `ListItem itemLink` →
  `/[workspaceSlug]/browse/{IDENTIFIER}-{sequence_id}`.

### Grouping

- Group by the row's `state__group` JSON key (note the double underscore —
  the API serializes `ArchiveRow` with `#[serde(rename = "state__group")]`).
- Group order: `STATE_GROUPS` order — backlog, unstarted, started, completed,
  cancelled. Labels from `STATE_GROUPS[group].label`.
- Rows with a missing/unknown `state__group` fall into "backlog".
- Empty groups are not rendered. Group header shows label + count.

### Filters and pagination

**Query parameters** for `getViewIssues`:

- `per_page=100`, `order_by=-created_at`.
- `filters` — JSON string of the complex-filter tree (shape validated by
  `parse_complex_filter`, `issue_query.rs:1184`):
  `{"and":[{"project_id":"<uuid>"},{"name__icontains":"<search>"}]}` —
  conditions included only when active; empty → param omitted.
- `cursor` — only on Load more (value from the previous envelope's
  `next_cursor`).

**Behavior**:

- Search input debounced ~300ms. Changing project or search resets to page 1.
- In-flight requests aborted via `AbortController` (the service accepts an
  axios config).
- "Load more" button lives in the shell footer; hidden when the envelope says
  `next_page_results = false`. Header count uses `total_results`; the empty
  state shows only when `total_results === 0`.
- Invalid/unknown `?project=` (an id not in `workspaceProjectIds`) is ignored
  client-side and treated as "All projects", so a stale link does not produce
  a silent empty list. While the project list is still loading, the filter is
  applied only once the id is known to be a member project; the shell receives
  the normalized (possibly `undefined`) project filter so its label stays
  correct.

**Response envelope** (verified in the handler): `results[]`,
`next_cursor`, `prev_cursor`, `next_page_results`, `prev_page_results`,
`total_count`, `total_results`.

**Row fields available** (`ArchiveRow`, `issue_common.rs:489`): `id`, `name`,
`state_id`, `priority`, `sequence_id`, `project_id`, `parent_id`, `cycle_id`,
`sub_issues_count`, `created_at`, `updated_at`, `created_by`, `updated_by`,
`state__group`, `assignee_ids`, `label_ids`, `module_ids`, `is_draft`,
`archived_at`, plus counters. Sub-issues appear as their own rows (no special
handling in v1).

### Data flow

- Local React state in the list view (NOT the shared workspace-issue store, to
  avoid clobbering Queues state): `results`, `nextCursor`, `totalResults`,
  `isLoading`, `isLoadingMore`.
- Pagination is a small pure reducer (`appendTicketsPage`,
  `createTicketsPageState`) in `core/lib/workspace-tickets.ts` so it is
  unit-testable.
- Supporting lookups: `useProject().getProjectById` (project name +
  `identifier`), `useProjectState().getStateById` (state name/color for the
  icon), `useMember().getUserDetails` (avatar/display name), `PriorityIcon`,
  `renderFormattedDate`.
- Errors follow the existing API-notification pattern; a failed Load more
  keeps the already-loaded list visible.

### Permissions

No new gates. The endpoint scopes results server-side (member projects;
guest scoping when `guest_view_all_features = false`). The project filter
dropdown uses `workspaceProjectIds` from `useProject`.

### i18n

- Reuse `sidebar.work_items`, `service.catalog_all_projects`,
  `service.catalog_search_placeholder`, `common.loading`, `STATE_GROUPS`
  labels.
- New empty-state keys merged into the existing `workspace_empty_state`
  namespace in `packages/i18n/src/locales/en/empty-state.json` (do NOT create
  a second top-level namespace — the Sprints & Tracks work learned this the
  hard way):

```json
"tickets": {
  "title": "No tickets yet",
  "description": "Tickets from projects you are a member of will show up here."
}
```

## Files

New:

- `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/page.tsx`
- `apps/web/core/components/issues/workspace/workspace-tickets-list-view.tsx`
- `apps/web/core/lib/workspace-tickets.ts` + `workspace-tickets.test.ts`

Modified:

- `apps/web/app/routes/core.ts` (one route block)
- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`
  (optional `footer` prop)
- `packages/constants/src/workspace.ts` (nav item)
- `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- `apps/web/core/components/workspace/sidebar/helper.tsx`
- `packages/i18n/src/locales/en/empty-state.json` (keys merged)

## Testing

Vitest (`apps/web`, `core/lib/*.test.ts` convention):

- `buildWorkspaceTicketsFilter`: both absent → `undefined`; project only;
  search only; both; whitespace-only search ignored (trimmed).
- `formatIssueKey`: `("ENG", 123)` → `"ENG-123"`.
- Pagination reducer: first page state; append preserves order; `nextCursor`
  updated; `hasMore = false` when `next_page_results` is false; reset returns
  the initial state.

No Rust tests: no backend changes.

## Verification

1. `pnpm --filter=web test` (targeted new file first, then the full suite).
2. `pnpm check:types` and lint on touched files.
3. `pnpm --filter=@plane/constants build` (sidebar entry) then
   `pnpm --filter=web build`.
4. `systemctl --user restart plane-web-prod.service` and health check.
5. Smoke on the tunnel: sidebar shows Tickets between Home and Sprints; list
   loads grouped by state group; `?project=` and search filter server-side;
   Load more pages; clicking a row opens the browse detail page; a workspace
   with no visible tickets shows the empty state.

## Risks and open questions

- **Data volume:** 100-per-page + Load more is the agreed strategy; groups are
  computed over loaded rows only (group counts are "loaded" counts, not
  global). If that is confusing later, switch to server-side `group_by`.
- **Search semantics:** `name__icontains` searches the ticket name, not the
  identifier (`ENG-123`); identifier search would need client-side matching or
  a backend addition — out of scope for v1.
- **Guest visibility:** guests may see an empty page even though the entry is
  visible; acceptable (same as sprints/tracks).
