# Workspace Aggregate Parity + Application Scope — Design

Date: 2026-10-21
Status: Approved design, ready for implementation planning

## 1. Context

Workspace-level Work Items (`/{workspaceSlug}/work-items`) was recently brought to 1:1
parity with project Work Items (same five layouts, filter row, display controls, create
flow). The remaining workspace-level surfaces—Cycles, Modules, Service Catalog, and
Pages—still use custom aggregate list views that only offer a project dropdown and search.

With the move to workspace-level pages, the **application (project) selector that used to
sit in the list toolbar disappeared from Work Items**. Users currently have to re-select an
application on every workspace page, and Cycles/Modules/Pages/Service Catalog do not match
their project-level counterparts.

The api-rs project issue endpoint only honors the rich `filters` JSON (legacy `?module=` /
`?cycle=` / `?project=` params are accepted-and-ignored). Any application scope for Work
Items must therefore be injected into the rich filter expression, not passed as a legacy
query param.

## 2. Goals

1. One global, persistent **Application scope** per workspace, selectable from the sidebar,
   shared by all workspace-level pages. No repeated selection when navigating.
2. Restore the application filter on Work Items (as the global scope, with a visible chip).
3. Bring workspace Cycles, Modules, Pages, and Service Catalog to 1:1 parity with their
   project-level views (same layouts, filters, create flow, empty states, peek overviews),
   aggregated across applications.
4. Keep project-level behavior unchanged.

## 3. Non-goals

- Multi-select applications.
- Creating a cycle/module/page/service without an owning application.
- Cross-project transfer or drag-and-drop.
- Backend changes, unless a concrete gap is found during implementation.

## 4. Decisions (locked)

- **Parity level:** reuse the project-level components, made scope-aware, rather than
  maintaining parallel workspace list implementations.
- **Application filter:** single-select `All applications` or one application.
- **Control placement:** sidebar only. Row sits below the sidebar title row (`Projects` +
  customize/collapse controls) and above `New work item`.
- **Indicator:** an `Application: <name> ×` chip on each workspace page toolbar; `×`
  resets to All.
- **Create behavior:** scoped application → create directly there. All applications →
  ask for the application first, then open the normal create modal.
- **URL:** `?project=<id>` remains supported as a deep-link seed and is replaced on the
  active URL when the scope changes.

## 5. Current state (relevant code)

- Work Items: `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/*`,
  `core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx`,
  `core/store/issue/workspace/*`, `core/lib/workspace-issue-grouping.ts`.
- Workspace aggregates today:
  `core/components/cycles/workspace/workspace-sprints-list-view.tsx`,
  `core/components/modules/workspace/workspace-tracks-list-view.tsx`,
  `core/components/pages/workspace/workspace-kb-list-view.tsx`,
  `core/components/services/workspace/workspace-services-list-view.tsx`.
- Project counterparts:
  `core/components/cycles/cycles-view.tsx` + `core/components/cycles/list/*`,
  `core/components/modules/modules-list-view.tsx` + `module-view-header.tsx`,
  `core/components/pages/pages-list-view.tsx` + `pages/list/*`,
  `core/components/services/services-list-view.tsx`.
- Sidebar: `apps/web/app/(all)/[workspaceSlug]/(projects)/sidebar.tsx`,
  `core/components/sidebar/sidebar-wrapper.tsx`,
  `core/components/workspace/sidebar/quick-actions.tsx`.
- Project service routes already redirect to the workspace catalog with `?project=`.

## 6. Architecture

### 6.1 Application scope store

New `apps/web/core/store/workspace-application-scope.store.ts`:

- Observable `projectIdByWorkspace: Record<string, string | null>`.
- `getProjectId(workspaceSlug): string | null` (`null` = All).
- `setProjectId(workspaceSlug, projectId | null)`.
- Persists to `localStorage` under a namespaced key (one map, or one key per workspace).
- Hydrates lazily on first access; guards `typeof window === "undefined"`.
- Registered on `CoreRootStore` (both constructor branches used for hydration).
- A per-workspace reset is exposed so switching workspaces cannot leak a stale scope.

### 6.2 Hook and URL sync

New `apps/web/core/hooks/use-workspace-application-scope.ts`:

- Validates the stored/URL project against `useProject().workspaceProjectIds`.
- On mount/workspace change: `?project=` (if valid) seeds the store; otherwise the stored
  value is used; invalid → `null`.
- `setApplication(projectId | null)` updates the store and `router.replace`s the current
  path, preserving other params and adding/removing `?project=`.
- Changing the scope triggers a refetch on the active page: Work Items calls
  `fetchIssuesWithExistingPagination`; Cycles/Modules/Pages/Services recompute from their
  workspace fetches (or refetch if the local data was scope-limited).
- All workspace pages consume this hook; no page owns project-filter state anymore.

### 6.3 Scope injection

- **Work Items:** `WorkspaceIssues.fetchIssues` merges
  `{ project_id: [scopedProjectId] }` into the rich `filters` expression using
  `appendRichFilterNode`, before pagination/grouping. `project_id` is removed from the
  `my_issues` filter allowlist so Add Filter cannot create a conflicting second project
  condition.
- **Cycles/Modules:** existing `fetchWorkspaceCycles` / `fetchWorkspaceModules` already
  load workspace-wide data; aggregate selectors filter by scope, feature flags
  (`cycle_view` / `module_view`), and the domain filters. Workspace-level filters use a
  reserved key (`__workspace__`) in the existing filter stores so project filter state is
  never touched.
- **Pages:** add `EPageStoreType.WORKSPACE` to `use-page-store`, backed by the existing
  workspace pages fetch, exposing the same interface `PagesListView` needs.
- **Services:** `fetchWorkspaceServices` plus scope filtering; the owner project remains a
  property of each service.

### 6.4 Scope-aware project components

Project components gain optional props (defaults preserve current behavior):

- `CyclesView` / `CyclesList`: `projectIds?: string[]`; per-row project comes from
  `cycle.project_id`.
- `ModulesListView` / `ModuleViewHeader` / module rows/cards: `projectIds?: string[]`;
  links and quick actions use `module.project_id`.
- `PagesListView` / `PagesListRoot`: workspace store type + `projectIds?: string[]`.
- `ServicesListView` / `WorkspaceServicesListView`: scope from the hook; no local project
  dropdown.

All scope filtering is memoized (`computedFn` / `useMemo`) to avoid re-sorting on every
render.

### 6.5 Shared shell pieces

- `SidebarWrapper` gains an optional `headerContent` slot rendered between the title row
  and `quickActions`. `AppSidebar` passes the application selector; project sidebars do
  not.
- New `WorkspaceApplicationScopeSelect` and `WorkspaceApplicationScopeChip` components
  under `core/components/workspace/application-scope/`. The chip is rendered in the page
  header (left, next to the page title/breadcrumb), so it stays visible even when the
  filter row is hidden.

## 7. UI/UX

### 7.1 Sidebar

- Order: title row (`Projects`, customize gear, collapse) → application select →
  `New work item` → quick actions/navigation.
- Compact full-width select showing an application icon and `All applications` or the
  project name; clear (`×`) when a project is selected.
- Hidden on project detail routes; the route project is the context there.
- Scope is persisted per workspace and survives reloads.

### 7.2 Scope chip

- Rendered on every workspace aggregate page when a project is scoped:
  `Application: <name> ×`.
- Placed in the page header’s left area, next to the breadcrumb/page title, so it is
  independent of filter-row visibility.
- Clicking `×` resets scope to All and refetches.
- The old per-page project dropdowns are removed; the header chip is the only in-page
  scope indicator, avoiding duplicate controls.

### 7.3 Work Items

Unchanged 1:1 layout stack. Only the source of the application condition changes (global
scope instead of a page dropdown). Counts, grouping, calendar, gantt, spreadsheet, create,
and peek keep working as today.

### 7.4 Cycles

- Reuse `CyclesView` + `CyclesList` + `CyclePeekOverview`.
- Toolbar: search, status filters, applied-filter chips (workspace filter key).
- Sections: when a single application is scoped, preserve the project layout including the
  active-cycle hero. When All, render current cycles as a normal `Current` group (there can
  be one active cycle per project, so a single hero is misleading).
- Each row shows a project badge when All; links/quick actions target `cycle.project_id`.
- Create: `CycleCreateUpdateModal` with the scoped project; when All, select application
  first.

### 7.5 Modules

- Reuse `ModulesListView` (list/board/gantt), `ModuleViewHeader` (search, order, status,
  favorites, layout), `ModuleAppliedFiltersList`, `ModulePeekOverview`.
- Project badge on rows/cards when All; actions use `module.project_id`.
- Gantt requires a single scoped application (same rationale as service graph); when All is
  selected the gantt option is disabled with a “Select an application to view gantt”
  tooltip.
- Create: `ModuleCreateUpdateModal` with the scoped project; select application first when
  All.

### 7.6 Pages

- Reuse `PagesListView` with Public/Private/Archived tabs, grid/list, favorites, search.
- Workspace store mode; project badge when All; creating selects the application first when
  All and then opens the new page.

### 7.7 Service Catalog

- Reuse the service board/graph, filters, and create modal.
- Scope from the sidebar, no local project dropdown.
- Graph stays available only when one application is scoped.
- Create preselects the scoped application as owner; when All, owner is chosen in the form.

### 7.8 Empty, disabled, and loading states

- Scoped application with no data → the project-level empty state for that domain.
- All applications with no data → existing workspace empty state.
- Application with the feature disabled → the project disabled-feature state, not an empty
  list.
- Keep the existing project loaders for each layout.

### 7.9 Mobile

- The application select lives in the mobile sidebar drawer.
- The scope chip remains in the mobile header/toolbar.

## 8. Create flows

Shared helper `resolveWorkspaceCreateProject(scopeProjectId)`:

- Non-null → return it immediately.
- Null (All) → open a compact application picker modal, then continue with the chosen
  project.

Flow per surface:

- Sidebar `New work item`: call
  `toggleCreateIssueModal(true, EIssuesStoreType.PROJECT, scoped ? [scoped] : undefined)`.
- Cycles/Modules: render the existing create modal with the resolved `projectId`.
- Pages: create through the page service with the resolved `projectId`, then navigate to
  the new page.
- Services: pass the resolved project as `defaultOwnerProjectId`; the existing form still
  allows changing the owner.

Permission checks are evaluated against the resolved project (membership/admin), not the
workspace role alone.

## 9. Permissions

- Scope options come from joined/workspace project ids that the user can access.
- Project feature flags (`cycle_view`, `module_view`, `page_view`, service availability)
  determine whether a scoped application can be selected or must show a disabled state.
- Guests keep the existing page-level guards; scope does not broaden access.

## 10. Phasing

1. **Foundation:** scope store + hook + sidebar select + chip + URL sync + Work Items
   integration.
2. **Cycles:** 1:1 workspace view, filters, create, peek.
3. **Modules:** 1:1 workspace view (list/board; scoped gantt), filters, create, peek.
4. **Pages:** 1:1 tabs/grid/list, create, favorites/search.
5. **Service Catalog:** align toolbar/filters/create with the shared shell.

Each phase is independently shippable and leaves project pages untouched. The first
implementation plan covers Phase 1 (foundation + Work Items); each later phase gets its own
plan that reuses the same scope/selector patterns.

## 11. Testing

Unit (vitest, `apps/web`):

- Scope store: persistence per workspace, hydration, isolation between workspaces, reset.
- URL sync: seed from `?project=`, invalid id ignored, replace preserves other params.
- Work Items: rich-filter merge produces `project_id: [id]` and does not mutate the cached
  filter params.
- Aggregate selectors: scope + feature-flag filtering for cycles/modules/pages/services.
- Create target resolution: scoped project vs All picker.

Manual per phase:

- All vs scoped; count/layout/filter behaviour.
- Scope changes refetch the current page without navigation.
- Scope persists across pages and reloads; deep link restores it.
- Project detail pages hide the selector and remain unchanged.
- Mobile sidebar/header.
- Guest/member/admin permissions.

Regression:

- Project Work Items, Cycles, Modules, Pages, and Services unchanged when no scope prop is
  passed.

## 12. Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Invasive store changes | Optional `scope`/`projectIds` props and separate workspace filter keys; project defaults untouched. |
| Hidden filtering confusion | Persistent sidebar control + per-page chip + clear button. |
| Performance across many applications | Reuse workspace fetches, memoized selectors, keep pages pagination. |
| Duplicate project controls | Remove per-page project dropdowns; `project_id` removed from Work Items Add Filter. |
| URL/store conflict | Store is the source of truth after hydration; URL only seeds and mirrors the current page. |
| Create into the wrong project | Resolve scope first, always show the target project in the modal header/owner field. |

## 13. Acceptance criteria

1. Sidebar shows the application scope between the title row and `New work item`, persists
   per workspace, and is hidden on project pages.
2. Work Items respects the scope; All shows every application; the chip and clear action
   work; no duplicate project dropdown/filter.
3. Cycles, Modules, Pages, and Service Catalog visually and behaviourally match their
   project counterparts (layouts, filters, create, empty/disabled states, peek) while
   aggregating across applications.
4. Create from any surface targets the scoped application, or asks for one when All.
5. Project-level pages are regression-free.
6. Type checks, lint, unit tests, and the production web build pass.

## 14. Out of scope

- Multi-select applications.
- Cross-project transfer/drag-and-drop.
- New backend endpoints.
- Reworking the command palette or global navigation beyond the scope selector.
