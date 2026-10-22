# Workspace Parity Phase 3 (Modules) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The user selected inline execution for this repository; do not dispatch subagents.

**Goal:** Bring workspace Modules to 1:1 parity with project Modules, aggregated across applications and controlled by the global Application scope.

**Architecture:** Reuse `ModulesListView`, `ModuleViewHeader`, `ModuleListItem`, `ModuleCardItem`, `ModulePeekOverview`, and the existing Gantt view. Add an optional workspace mode driven by `projectIds`, a reserved `__workspace__` filter key, and per-module `project_id` resolution for links, permissions, mutations, and badges. Project Modules keep their existing prop-less behavior.

**Tech Stack:** React Router, MobX stores (`ModulesStore`, `ModuleFilterStore`), existing `@plane/utils` module filters/order helpers, Vitest.

---

## File Map

**Create**

- `apps/web/core/lib/workspace-modules.ts` — workspace module selector, reserved filter key, scoped/feature/search/filter/order logic.
- `apps/web/core/lib/workspace-modules.test.ts` — selector tests.
- `apps/web/core/hooks/use-workspace-module-permissions.ts` — scoped/All create permission.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/header.tsx` — workspace Modules header.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/create-modals.tsx` — create picker + module modal orchestration.
- `docs/superpowers/plans/2026-10-22-workspace-parity-phase-3.md` — this plan.

**Modify**

- `apps/web/core/store/module_filter.store.ts` — expose `initProjectModuleFilters`, guard `clearAllFilters`.
- `apps/web/core/components/modules/module-view-header.tsx` — `filterKey`, workspace filter lookup, gantt disable/tooltip, immutable filter arrays.
- `apps/web/core/components/modules/modules-list-view.tsx` — workspace selector mode, list/board/gantt rendering, badges, empty states, peek.
- `apps/web/core/components/modules/module-list-item.tsx` — project badge, resolved project for actions.
- `apps/web/core/components/modules/module-list-item-action.tsx` — mutations/permissions from `module.project_id`.
- `apps/web/core/components/modules/module-card-item.tsx` — project badge, resolved project for mutations/permissions/actions.
- `apps/web/core/components/modules/module-peek-overview.tsx` — optional project id, resolve from module.
- `apps/web/core/components/modules/gantt-chart/modules-list-layout.tsx` — scoped project/filter key, explicit module ids, project-aware date updates.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/layout.tsx` — new header + create modals.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/page.tsx` — replace legacy aggregate list with scope-aware page.

**Delete**

- `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx` — legacy aggregate view.

**Do not change**

- Project Modules routes/components must keep current behavior when the new optional props are absent.
- No new locale keys; reuse existing module/empty-state/common keys. The gantt tooltip may stay hardcoded English, consistent with the header's existing hardcoded `Filters`/`Search` strings.

---

## Task 1: Workspace Module Selector

**Files:**

- Create: `apps/web/core/lib/workspace-modules.ts`
- Test: `apps/web/core/lib/workspace-modules.test.ts`

- [ ] **Step 1: Write the failing selector tests**

Create `apps/web/core/lib/workspace-modules.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { IModule } from "@plane/types";
import { selectWorkspaceModules, WORKSPACE_MODULE_FILTER_KEY } from "./workspace-modules";

const day = (value: number) => `2026-10-${String(value).padStart(2, "0")}`;

const makeModule = (overrides: Partial<IModule>): IModule =>
  ({
    id: "module",
    name: "Module",
    project_id: "p1",
    archived_at: null,
    status: "planned",
    lead_id: null,
    member_ids: [],
    is_favorite: false,
    start_date: null,
    target_date: null,
    created_at: "2026-10-01",
    sort_order: 1,
    completed_issues: 0,
    total_issues: 0,
    cancelled_issues: 0,
    ...overrides,
  }) as IModule;

const select = (modules: IModule[], overrides: Partial<Parameters<typeof selectWorkspaceModules>[0]> = {}) =>
  selectWorkspaceModules({
    modules,
    projectIds: ["p1"],
    displayFilters: { layout: "list", order_by: "name", favorites: false },
    filters: {},
    searchQuery: "",
    ...overrides,
  });

describe("selectWorkspaceModules", () => {
  it("filters by project scope, archived state, and the project feature flag", () => {
    const selection = select(
      [
        makeModule({ id: "p1", project_id: "p1" }),
        makeModule({ id: "p2", project_id: "p2" }),
        makeModule({ id: "p3", project_id: "p3" }),
        makeModule({ id: "archived", project_id: "p1", archived_at: "2026-10-01" }),
      ],
      {
        projectIds: ["p1", "p2", "p3"],
        projectAllowed: (projectId) => projectId !== "p3",
      }
    );

    expect(selection.moduleIds).toEqual(["p1", "p2"]);
    expect(selection.totalCount).toBe(2);
  });

  it("applies search case-insensitively", () => {
    const selection = select(
      [
        makeModule({ id: "alpha", name: "Alpha" }),
        makeModule({ id: "beta", name: "Beta" }),
        makeModule({ id: "alphabet", name: "Alphabet" }),
      ],
      { searchQuery: "ALPHA" }
    );

    expect(selection.moduleIds).toEqual(["alpha", "alphabet"]);
    expect(selection.totalCount).toBe(2);
  });

  it("applies status, lead, and member filters through shouldFilterModule", () => {
    const modules = [
      makeModule({ id: "match", status: "planned", lead_id: "u1", member_ids: ["u2"] }),
      makeModule({ id: "wrong-status", status: "completed", lead_id: "u1", member_ids: ["u2"] }),
      makeModule({ id: "wrong-lead", status: "planned", lead_id: "u3", member_ids: ["u2"] }),
      makeModule({ id: "wrong-member", status: "planned", lead_id: "u1", member_ids: ["u4"] }),
    ];

    const selection = select(modules, {
      filters: { status: ["planned"], lead: ["u1"], members: ["u2"] },
    });

    expect(selection.moduleIds).toEqual(["match"]);
  });

  it("applies date filters through the shared module filter helper", () => {
    const selection = select(
      [
        makeModule({ id: "later", start_date: day(26), target_date: day(30) }),
        makeModule({ id: "sooner", start_date: day(25), target_date: day(29) }),
      ],
      {
        displayFilters: { order_by: "name" },
        filters: { start_date: [`${day(26)};after`] },
      }
    );

    expect(selection.moduleIds).toEqual(["later"]);
  });

  it("applies the favorites display filter", () => {
    const selection = select(
      [makeModule({ id: "favorite", is_favorite: true }), makeModule({ id: "other", is_favorite: false })],
      {
        displayFilters: { order_by: "name", favorites: true },
      }
    );

    expect(selection.moduleIds).toEqual(["favorite"]);
  });

  it("orders names case-insensitively by default", () => {
    const selection = select(
      [
        makeModule({ id: "upper", name: "Alpha" }),
        makeModule({ id: "lower", name: "alpha" }),
        makeModule({ id: "beta", name: "Beta" }),
      ],
      { displayFilters: {} }
    );

    expect(selection.moduleIds).toEqual(["upper", "lower", "beta"]);
  });

  it("orders by target date", () => {
    const selection = select(
      [
        makeModule({ id: "later", name: "Later", target_date: day(28) }),
        makeModule({ id: "sooner", name: "Sooner", target_date: day(20) }),
      ],
      { displayFilters: { order_by: "target_date" } }
    );

    expect(selection.moduleIds).toEqual(["sooner", "later"]);
  });

  it("returns the filtered count before ordering", () => {
    const selection = select([makeModule({ id: "one" }), makeModule({ id: "two" })]);

    expect(selection.totalCount).toBe(2);
    expect(selection.moduleIds).toHaveLength(2);
  });

  it("exports the reserved workspace filter key", () => {
    expect(WORKSPACE_MODULE_FILTER_KEY).toBe("__workspace__");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
pnpm exec vitest run core/lib/workspace-modules.test.ts
```

Expected: FAIL because `./workspace-modules` does not exist.

- [ ] **Step 3: Implement the selector**

Create `apps/web/core/lib/workspace-modules.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IModule, TModuleDisplayFilters, TModuleFilters } from "@plane/types";
import { orderModules, shouldFilterModule } from "@plane/utils";

export const WORKSPACE_MODULE_FILTER_KEY = "__workspace__";

export type TWorkspaceModuleSelection = {
  moduleIds: string[];
  totalCount: number;
};

type TSelectWorkspaceModulesArgs = {
  modules: IModule[];
  projectIds: string[];
  displayFilters?: TModuleDisplayFilters;
  filters?: TModuleFilters;
  searchQuery: string;
  projectAllowed?: (projectId: string) => boolean;
};

export const selectWorkspaceModules = ({
  modules,
  projectIds,
  displayFilters,
  filters,
  searchQuery,
  projectAllowed,
}: TSelectWorkspaceModulesArgs): TWorkspaceModuleSelection => {
  const projectIdSet = new Set(projectIds);
  const normalizedSearch = searchQuery.trim().toLowerCase();

  const filteredModules = modules
    .filter((module) => projectIdSet.has(module.project_id))
    .filter((module) => !module.archived_at)
    .filter((module) => (projectAllowed ? projectAllowed(module.project_id) : true))
    .filter((module) => module.name.toLowerCase().includes(normalizedSearch))
    .filter((module) => shouldFilterModule(module, displayFilters ?? {}, filters ?? {}));

  const orderedModules = orderModules(filteredModules, displayFilters?.order_by ?? "name");

  return {
    moduleIds: orderedModules.map((module) => module.id),
    totalCount: filteredModules.length,
  };
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
pnpm exec vitest run core/lib/workspace-modules.test.ts
```

Expected: PASS (9 tests).

- [ ] **Step 5: Lint and type-check the new files**

Run:

```bash
pnpm exec oxlint core/lib/workspace-modules.ts core/lib/workspace-modules.test.ts
pnpm check:types
```

Expected: 0 lint warnings/errors, typecheck exits 0.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/lib/workspace-modules.ts apps/web/core/lib/workspace-modules.test.ts
git commit -m "feat(web): add workspace module selector"
```

---

## Task 2: Workspace Module Filter Store + Header

**Files:**

- Modify: `apps/web/core/store/module_filter.store.ts`
- Modify: `apps/web/core/components/modules/module-view-header.tsx`

- [ ] **Step 1: Add the initializer to the store contract and guard clearing**

In `apps/web/core/store/module_filter.store.ts`, add to `IModuleFilterStore`:

```ts
  initProjectModuleFilters: (projectId: string) => void;
```

Replace `clearAllFilters` with:

```ts
clearAllFilters = (projectId: string, state: keyof TModuleFiltersByState = "default") => {
  runInAction(() => {
    this.filters[projectId] = this.filters[projectId] ?? { default: {}, archived: {} };
    this.displayFilters[projectId] = this.displayFilters[projectId] ?? {
      favorites: false,
      layout: "list",
      order_by: "name",
    };
    this.filters[projectId][state] = {};
    this.displayFilters[projectId].favorites = false;
  });
  this.saveFiltersToLocalStorage();
  this.saveDisplayFiltersToLocalStorage();
};
```

Also make the archived computed safe:

```ts
  get currentProjectArchivedFilters() {
    const projectId = this.rootStore.router.projectId;
    if (!projectId) return;
    return this.filters[projectId]?.archived;
  }
```

- [ ] **Step 2: Make the header filter-key aware**

In `apps/web/core/components/modules/module-view-header.tsx`, change the component signature to accept optional props:

```ts
type Props = {
  filterKey?: string;
  isGanttDisabled?: boolean;
};

export const ModuleViewHeader = observer(function ModuleViewHeader({
  filterKey,
  isGanttDisabled = false,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const { projectId } = useParams();
  const resolvedFilterKey = filterKey ?? projectId?.toString();
  const { isMobile } = usePlatformOS();
  const {
    workspace: { workspaceMemberIds },
  } = useMember();
  const {
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    searchQuery,
    updateDisplayFilters,
    updateFilters,
    updateSearchQuery,
  } = useModuleFilter();
  const { t } = useTranslation();

  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const [isSearchOpen, setIsSearchOpen] = useState(searchQuery !== "");
```

Replace `handleFilters` with:

```ts
const handleFilters = useCallback(
  (key: keyof TModuleFilters, value: string | string[]) => {
    if (!resolvedFilterKey) return;
    const newValues = [...(filters?.[key] ?? [])];

    if (Array.isArray(value))
      value.forEach((val) => {
        if (!newValues.includes(val)) newValues.push(val);
        else newValues.splice(newValues.indexOf(val), 1);
      });
    else {
      if (filters?.[key]?.includes(value)) newValues.splice(newValues.indexOf(value), 1);
      else newValues.push(value);
    }

    updateFilters(resolvedFilterKey, { [key]: newValues });
  },
  [filters, resolvedFilterKey, updateFilters]
);
```

Update `isFiltersApplied`:

```ts
const isFiltersApplied = calculateTotalFilters(filters ?? {}) !== 0 || displayFilters?.favorites;
```

Update the order dropdown:

```tsx
<ModuleOrderByDropdown
  value={displayFilters?.order_by}
  onChange={(val) => {
    if (!resolvedFilterKey || val === displayFilters?.order_by) return;
    updateDisplayFilters(resolvedFilterKey, {
      order_by: val,
    });
  }}
/>
```

Update the filter selection callbacks:

```tsx
<ModuleFiltersSelection
  displayFilters={displayFilters ?? {}}
  filters={filters ?? {}}
  handleDisplayFiltersUpdate={(val) => {
    if (!resolvedFilterKey) return;
    updateDisplayFilters(resolvedFilterKey, val);
  }}
  handleFiltersUpdate={handleFilters}
  memberIds={workspaceMemberIds ?? undefined}
/>
```

Replace the layout map with:

```tsx
{
  MODULE_VIEW_LAYOUTS.map((layout) => {
    const isLayoutDisabled = layout.key === "gantt" && isGanttDisabled;

    return (
      <Tooltip
        key={layout.key}
        label={isLayoutDisabled ? "Select an application to view timeline" : t(layout.i18n_title)}
        disabled={isMobile && !isLayoutDisabled}
      >
        <button
          type="button"
          aria-disabled={isLayoutDisabled}
          className={cn(
            "group grid h-5.5 w-7 place-items-center overflow-hidden rounded-sm transition-all hover:bg-layer-transparent-hover",
            {
              "bg-layer-transparent-active hover:bg-layer-transparent-active": displayFilters?.layout === layout.key,
              "cursor-not-allowed opacity-40 hover:bg-transparent": isLayoutDisabled,
            }
          )}
          onClick={() => {
            if (isLayoutDisabled || !resolvedFilterKey) return;
            updateDisplayFilters(resolvedFilterKey, { layout: layout.key });
          }}
        >
          <ModuleLayoutIcon layoutType={layout.key} />
        </button>
      </Tooltip>
    );
  });
}
```

- [ ] **Step 3: Verify project Modules header behavior**

Run:

```bash
pnpm exec oxlint core/store/module_filter.store.ts core/components/modules/module-view-header.tsx
pnpm check:types
```

Expected: both exit 0. Project routes still resolve `filterKey` from `useParams().projectId`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/store/module_filter.store.ts apps/web/core/components/modules/module-view-header.tsx
git commit -m "feat(web): support workspace module filters"
```

---

## Task 3: Scope-Aware Module Rows and Cards

**Files:**

- Modify: `apps/web/core/components/modules/module-list-item.tsx`
- Modify: `apps/web/core/components/modules/module-list-item-action.tsx`
- Modify: `apps/web/core/components/modules/module-card-item.tsx`

- [ ] **Step 1: Resolve project and show the badge in list rows**

In `module-list-item.tsx`:

```ts
type Props = {
  moduleId: string;
  showProjectBadge?: boolean;
};

export const ModuleListItem = observer(function ModuleListItem(props: Props) {
  const { moduleId, showProjectBadge = false } = props;
  const { workspaceSlug } = useParams();
  const { getModuleById } = useModule();
  const { getProjectById } = useProject();
  const moduleDetails = getModuleById(moduleId);
  const resolvedProjectId = moduleDetails?.project_id;
  const project = showProjectBadge && resolvedProjectId ? getProjectById(resolvedProjectId) : undefined;

  if (!moduleDetails || !resolvedProjectId) return null;
```

Keep the existing derived progress values. Replace `appendTitleElement` with:

```tsx
      appendTitleElement={
        <div className="flex items-center gap-2">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5 text-11 text-tertiary">
              {project.name}
            </span>
          )}
          <button
            onClick={openModuleOverview}
            className={`z-[5] flex-shrink-0 ${isMobile ? "flex" : "hidden group-hover:flex"}`}
          >
            <InfoOutline className="h-4 w-4 text-placeholder" />
          </button>
        </div>
      }
```

Replace quick actions project prop:

```tsx
<ModuleQuickActions
  parentRef={parentRef}
  moduleId={moduleId}
  projectId={resolvedProjectId}
  workspaceSlug={workspaceSlug.toString()}
/>
```

Add the `useProject` import:

```ts
import { useProject } from "@/hooks/store/use-project";
```

- [ ] **Step 2: Use the module project in list-row actions**

In `module-list-item-action.tsx`, remove `projectId` from the router destructure:

```ts
const { workspaceSlug } = useParams();
```

Add the resolved project after `moduleDetails`:

```ts
const resolvedProjectId = moduleDetails.project_id;
```

Use `resolvedProjectId` for permission, favorites, and updates:

```ts
const isEditingAllowed = allowPermissions(
  [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
  EUserPermissionsLevel.PROJECT,
  workspaceSlug?.toString(),
  resolvedProjectId
);
```

```ts
if (!workspaceSlug || !resolvedProjectId) return;
const addToFavoritePromise = addModuleToFavorites(workspaceSlug.toString(), resolvedProjectId, moduleId).then(() => {
  if (!storedValue) toggleFavoriteMenu(true);
});
```

```ts
if (!workspaceSlug || !resolvedProjectId) return;
const removeFromFavoritePromise = removeModuleFromFavorites(workspaceSlug.toString(), resolvedProjectId, moduleId);
```

```ts
if (!workspaceSlug || !resolvedProjectId) return;
await updateModuleDetails(workspaceSlug.toString(), resolvedProjectId, moduleId, payload);
```

Replace the final quick-actions guard/render:

```tsx
{
  workspaceSlug && (
    <div className="hidden md:block">
      <ModuleQuickActions
        parentRef={parentRef}
        moduleId={moduleId}
        projectId={resolvedProjectId}
        workspaceSlug={workspaceSlug.toString()}
      />
    </div>
  );
}
```

- [ ] **Step 3: Use the module project in cards and show the badge when All**

In `module-card-item.tsx`, add props and imports:

```ts
import { InfoOutline, ProjectsOutline, UserAltOutline, WorkItemsOutline } from "@makeplane/propel/icons";
import { useProject } from "@/hooks/store/use-project";
```

```ts
type Props = {
  moduleId: string;
  showProjectBadge?: boolean;
};

export const ModuleCardItem = observer(function ModuleCardItem(props: Props) {
  const { moduleId, showProjectBadge = false } = props;
```

Add `useProject` to the existing hook block at the top, next to `useModule`:

```ts
const { getProjectById } = useProject();
```

After `moduleDetails` is derived, add:

```ts
const resolvedProjectId = moduleDetails?.project_id ?? projectId?.toString();
const project = showProjectBadge && resolvedProjectId ? getProjectById(resolvedProjectId) : undefined;
```

Update permission:

```ts
const isEditingAllowed = allowPermissions(
  [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
  EUserPermissionsLevel.PROJECT,
  workspaceSlug?.toString(),
  resolvedProjectId
);
```

Update favorite handlers:

```ts
if (!workspaceSlug || !resolvedProjectId) return;
const addToFavoritePromise = addModuleToFavorites(workspaceSlug.toString(), resolvedProjectId, moduleId).then(() => {
  if (!storedValue) toggleFavoriteMenu(true);
});
```

```ts
if (!workspaceSlug || !resolvedProjectId) return;
const removeFromFavoritePromise = removeModuleFromFavorites(workspaceSlug.toString(), resolvedProjectId, moduleId);
```

Update the details handler:

```ts
if (!workspaceSlug || !resolvedProjectId) return;
await updateModuleDetails(workspaceSlug.toString(), resolvedProjectId, moduleId, payload);
```

Add the badge immediately after the title/status header block:

```tsx
{
  project && (
    <div className="flex items-center gap-1.5 text-11 text-tertiary">
      <ProjectsOutline className="h-3 w-3 flex-shrink-0" />
      <span className="truncate">{project.name}</span>
    </div>
  );
}
```

Replace the conditional quick actions:

```tsx
{
  workspaceSlug && resolvedProjectId && (
    <ModuleQuickActions
      parentRef={parentRef}
      moduleId={moduleId}
      projectId={resolvedProjectId}
      workspaceSlug={workspaceSlug.toString()}
    />
  );
}
```

- [ ] **Step 4: Run focused checks**

```bash
pnpm exec oxlint core/components/modules/module-list-item.tsx core/components/modules/module-list-item-action.tsx core/components/modules/module-card-item.tsx
pnpm check:types
```

Expected: both exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/modules/module-list-item.tsx core/components/modules/module-list-item-action.tsx core/components/modules/module-card-item.tsx
git commit -m "feat(web): resolve module projects in rows and cards"
```

---

## Task 4: Scope-Aware Peek and Gantt

**Files:**

- Modify: `apps/web/core/components/modules/module-peek-overview.tsx`
- Modify: `apps/web/core/components/modules/gantt-chart/modules-list-layout.tsx`

- [ ] **Step 1: Resolve the peek project from the module**

In `module-peek-overview.tsx`, change props:

```ts
type Props = {
  projectId?: string;
  workspaceSlug: string;
  isArchived?: boolean;
};
```

Update the store hook and derived project:

```ts
const { fetchModuleDetails, fetchArchivedModuleDetails, getModuleById } = useModule();
const peekModule = searchParams.get("peekModule");
const resolvedProjectId = projectId ?? (peekModule ? getModuleById(peekModule)?.project_id : undefined);
```

Update the effect:

```ts
useEffect(() => {
  if (!peekModule || !resolvedProjectId) return;
  if (isArchived) fetchArchivedModuleDetails(workspaceSlug, resolvedProjectId, peekModule.toString());
  else fetchModuleDetails(workspaceSlug, resolvedProjectId, peekModule.toString());
}, [fetchArchivedModuleDetails, fetchModuleDetails, isArchived, peekModule, resolvedProjectId, workspaceSlug]);
```

Project callers still pass the route project id, so their behavior is unchanged.

- [ ] **Step 2: Make the Gantt view accept an explicit scope**

In `modules-list-layout.tsx`, replace the component setup with:

```ts
type Props = {
  projectId?: string;
  moduleIds?: string[] | null;
  filterKey?: string;
};

export const ModulesListGanttChartView = observer(function ModulesListGanttChartView({
  projectId,
  moduleIds,
  filterKey,
}: Props) {
  const { workspaceSlug, projectId: routeProjectId } = useParams();
  const resolvedProjectId = projectId ?? routeProjectId?.toString();
  const resolvedFilterKey = filterKey ?? resolvedProjectId;
  const { getFilteredModuleIds, getModuleById, updateModuleDetails } = useModule();
  const { getDisplayFiltersByProjectId } = useModuleFilter();
  const { allowPermissions } = useUserPermissions();
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const filteredModuleIds =
    moduleIds ?? (resolvedProjectId ? getFilteredModuleIds(resolvedProjectId) : null);

  const isAllowed = resolvedProjectId
    ? allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.PROJECT,
        workspaceSlug?.toString(),
        resolvedProjectId
      )
    : false;

  const handleModuleUpdate = async (module: IModule, data: IBlockUpdateData) => {
    if (!workspaceSlug || !module) return;

    const payload: any = { ...data };
    if (data.sort_order) payload.sort_order = data.sort_order.newSortOrder;

    await updateModuleDetails(workspaceSlug.toString(), module.project_id, module.id, payload);
  };

  const updateBlockDates = async (blockUpdates: IBlockUpdateDependencyData[]) => {
    const blockUpdate = blockUpdates[0];
    if (!blockUpdate || !workspaceSlug) return;

    const module = getModuleById(blockUpdate.id);
    if (!module) return;

    const payload: Partial<IModule> = {};
    if (blockUpdate.start_date) payload.start_date = blockUpdate.start_date;
    if (blockUpdate.target_date) payload.target_date = blockUpdate.target_date;

    await updateModuleDetails(workspaceSlug.toString(), module.project_id, blockUpdate.id, payload);
  };

  if (!resolvedProjectId || !filteredModuleIds) return null;
```

Add imports:

```ts
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useUserPermissions } from "@/hooks/store/user";
```

Remove the `useProject` import and `currentProjectDetails` usage.

- [ ] **Step 3: Run focused checks**

```bash
pnpm exec oxlint core/components/modules/module-peek-overview.tsx core/components/modules/gantt-chart/modules-list-layout.tsx
pnpm check:types
```

Expected: both exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/modules/module-peek-overview.tsx core/components/modules/gantt-chart/modules-list-layout.tsx
git commit -m "feat(web): scope module peek and gantt by project"
```

---

## Task 5: Workspace Modules Shell, Header, and Create Flow

**Files:**

- Create: `apps/web/core/hooks/use-workspace-module-permissions.ts`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/header.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/create-modals.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/layout.tsx`

- [ ] **Step 1: Add the create-permission hook**

Create `apps/web/core/hooks/use-workspace-module-permissions.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useProject } from "@/hooks/store/use-project";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const useWorkspaceModulePermissions = () => {
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { getProjectById, workspaceProjectIds } = useProject();
  const { allowPermissions } = useUserPermissions();
  const { projectsWithCreatePermissions } = useUser();

  const canCreateInProject = (projectId: string) => {
    const project = getProjectById(projectId);
    return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.module_view !== false;
  };

  const canCreateModule = scopedProjectId
    ? canCreateInProject(scopedProjectId) &&
      allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.PROJECT,
        workspaceSlug,
        scopedProjectId
      )
    : Boolean(workspaceProjectIds?.some(canCreateInProject)) &&
      allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.WORKSPACE,
        workspaceSlug
      );

  return { canCreateModule };
};
```

- [ ] **Step 2: Add the workspace header**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { MODULE_VIEW_LAYOUTS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { ChevronDownOutline, ModuleOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, CustomMenu, Header, Row } from "@plane/ui";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { ModuleLayoutIcon, ModuleViewHeader } from "@/components/modules";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceModulePermissions } from "@/hooks/use-workspace-module-permissions";
import { useAppRouter } from "@/hooks/use-app-router";
import { WORKSPACE_MODULE_FILTER_KEY } from "@/lib/workspace-modules";

export const WorkspaceModulesHeader = observer(function WorkspaceModulesHeader() {
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const { t } = useTranslation();
  const { toggleCreateModuleModal } = useCommandPalette();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateModule } = useWorkspaceModulePermissions();

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.modules")}
                  href={`/${workspaceSlug}/modules/`}
                  icon={<ModuleOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          <WorkspaceApplicationScopeChip />
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        <ModuleViewHeader filterKey={WORKSPACE_MODULE_FILTER_KEY} isGanttDisabled={!scopedProjectId} />
        {canCreateModule && (
          <Button variant="primary" size="lg" onClick={() => toggleCreateModuleModal(true)}>
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("project_module.add_module")}</div>
          </Button>
        )}
      </Header.RightItem>
    </Header>
  );
});

export const WorkspaceModulesMobileHeader = observer(function WorkspaceModulesMobileHeader() {
  const { t } = useTranslation();
  const { getDisplayFiltersByProjectId, updateDisplayFilters } = useModuleFilter();
  const displayFilters = getDisplayFiltersByProjectId(WORKSPACE_MODULE_FILTER_KEY);

  return (
    <div className="flex justify-start md:hidden">
      <CustomMenu
        maxHeight="md"
        className="flex flex-grow justify-start border-b border-subtle bg-surface-1 py-2 text-13 text-secondary"
        customButton={
          <Row className="flex flex-grow justify-center gap-2 text-13 text-secondary">
            <span>Layout</span>
            <ChevronDownOutline className="my-auto h-4 w-4 text-secondary" />
          </Row>
        }
        customButtonClassName="flex flex-grow justify-center items-center text-secondary text-13"
        closeOnSelect
      >
        {MODULE_VIEW_LAYOUTS.map((layout) => {
          if (layout.key === "gantt") return null;

          return (
            <CustomMenu.MenuItem
              key={layout.key}
              onClick={() => updateDisplayFilters(WORKSPACE_MODULE_FILTER_KEY, { layout: layout.key })}
              className="flex items-center gap-2"
            >
              <ModuleLayoutIcon layoutType={layout.key} />
              <div className="text-tertiary">{t(layout.i18n_title)}</div>
            </CustomMenu.MenuItem>
          );
        })}
      </CustomMenu>
    </div>
  );
});
```

- [ ] **Step 3: Add the create orchestration**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/create-modals.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { CreateUpdateModuleModal } from "@/components/modules";
import { WorkspaceApplicationPickerModal } from "@/components/workspace/application-scope/project-picker-modal";
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useProject } from "@/hooks/store/use-project";
import { useUser } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { resolveWorkspaceCreateProject } from "@/lib/workspace-create-project";

export const WorkspaceModulesCreateModals = observer(function WorkspaceModulesCreateModals() {
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const { isCreateModuleModalOpen, toggleCreateModuleModal } = useCommandPalette();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { workspaceProjectIds, getProjectById } = useProject();
  const { projectsWithCreatePermissions } = useUser();
  const [pickedProjectId, setPickedProjectId] = useState<string | null>(null);

  const eligibleProjectIds = useMemo(
    () =>
      (workspaceProjectIds ?? []).filter((projectId) => {
        const project = getProjectById(projectId);
        return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.module_view !== false;
      }),
    [getProjectById, projectsWithCreatePermissions, workspaceProjectIds]
  );

  const createProjectId = resolveWorkspaceCreateProject(scopedProjectId, pickedProjectId);
  const showPicker = isCreateModuleModalOpen && !createProjectId;

  const handleClose = () => {
    setPickedProjectId(null);
    toggleCreateModuleModal(false);
  };

  useEffect(() => {
    if (!isCreateModuleModalOpen) setPickedProjectId(null);
  }, [isCreateModuleModalOpen]);

  useEffect(
    () => () => {
      toggleCreateModuleModal(false);
    },
    [toggleCreateModuleModal]
  );

  if (!workspaceSlug) return null;

  return (
    <>
      <WorkspaceApplicationPickerModal
        isOpen={showPicker}
        handleClose={handleClose}
        projectIds={eligibleProjectIds}
        onSelect={setPickedProjectId}
      />
      {isCreateModuleModalOpen && createProjectId && (
        <CreateUpdateModuleModal
          key={createProjectId}
          isOpen
          onClose={handleClose}
          workspaceSlug={workspaceSlug}
          projectId={createProjectId}
        />
      )}
    </>
  );
});
```

- [ ] **Step 4: Wire the layout**

Replace `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/layout.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { Outlet } from "react-router";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { WorkspaceModulesCreateModals } from "./create-modals";
import { WorkspaceModulesHeader, WorkspaceModulesMobileHeader } from "./header";

function WorkspaceModulesLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceModulesHeader />} mobileHeader={<WorkspaceModulesMobileHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
      <WorkspaceModulesCreateModals />
    </>
  );
}

export default observer(WorkspaceModulesLayout);
```

- [ ] **Step 5: Run focused checks**

```bash
pnpm exec oxlint core/hooks/use-workspace-module-permissions.ts 'app/(all)/[workspaceSlug]/(projects)/modules/header.tsx' 'app/(all)/[workspaceSlug]/(projects)/modules/create-modals.tsx' 'app/(all)/[workspaceSlug]/(projects)/modules/layout.tsx'
pnpm check:types
```

Expected: both exit 0.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/hooks/use-workspace-module-permissions.ts 'apps/web/app/(all)/[workspaceSlug]/(projects)/modules/header.tsx' 'apps/web/app/(all)/[workspaceSlug]/(projects)/modules/create-modals.tsx' 'apps/web/app/(all)/[workspaceSlug]/(projects)/modules/layout.tsx'
git commit -m "feat(web): add workspace modules shell and create picker"
```

---

## Task 6: Scope-Aware ModulesListView and Workspace Page

**Files:**

- Modify: `apps/web/core/components/modules/modules-list-view.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/page.tsx`
- Delete: `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx`

- [ ] **Step 1: Make `ModulesListView` accept workspace scope**

Replace the top of `modules-list-view.tsx` through the derived values with:

```tsx
import { useMemo } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import { EUserProjectRoles } from "@plane/types";
import { ContentWrapper, Row, ERowVariant } from "@plane/ui";
import { ListLayout } from "@/components/core/list";
import {
  ModuleCardItem,
  ModuleListItem,
  ModulePeekOverview,
  ModulesListGanttChartView,
} from "@/components/modules";
import { CycleModuleBoardLayoutLoader } from "@/components/ui/loader/cycle-module-board-loader";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
import { GanttLayoutLoader } from "@/components/ui/loader/layouts/gantt-layout-loader";
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useModule } from "@/hooks/store/use-module";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { selectWorkspaceModules } from "@/lib/workspace-modules";

type Props = {
  projectId?: string;
  projectIds?: string[];
  filterKey?: string;
  isGanttDisabled?: boolean;
  canCreate?: boolean;
};

export const ModulesListView = observer(function ModulesListView({
  projectId,
  projectIds,
  filterKey,
  isGanttDisabled = false,
  canCreate,
}: Props) {
  const { workspaceSlug, projectId: routeProjectId } = useParams();
  const searchParams = useSearchParams();
  const peekModule = searchParams.get("peekModule");
  const { t } = useTranslation();
  const { toggleCreateModuleModal } = useCommandPalette();
  const { getProjectModuleIds, getFilteredModuleIds, loader, moduleMap } = useModule();
  const { getDisplayFiltersByProjectId, getFiltersByProjectId, searchQuery } = useModuleFilter();
  const { getProjectById } = useProject();
  const { allowPermissions } = useUserPermissions();

  const resolvedProjectId = projectId ?? routeProjectId?.toString();
  const resolvedFilterKey = filterKey ?? resolvedProjectId;
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
  const layout =
    projectIds && !resolvedProjectId && displayFilters?.layout === "gantt"
      ? "list"
      : (displayFilters?.layout ?? "list");

  const projectAllowed = useMemo(() => {
    if (!projectIds) return undefined;
    return (moduleProjectId: string) => {
      if (!projectIds.includes(moduleProjectId)) return false;
      const project = getProjectById(moduleProjectId);
      return project ? project.module_view !== false : true;
    };
  }, [getProjectById, projectIds]);

  const workspaceSelection = useMemo(() => {
    if (!projectIds) return null;
    return selectWorkspaceModules({
      modules: Object.values(moduleMap ?? {}),
      projectIds,
      displayFilters,
      filters,
      searchQuery,
      projectAllowed,
    });
  }, [displayFilters, filters, moduleMap, projectAllowed, projectIds, searchQuery]);

  const projectModuleIds = resolvedProjectId ? getProjectModuleIds(resolvedProjectId) : null;
  const filteredModuleIds = workspaceSelection?.moduleIds ?? (resolvedProjectId ? getFilteredModuleIds(resolvedProjectId) : null);
  const totalModuleCount = workspaceSelection
    ? Object.values(moduleMap ?? {}).filter((module) => {
        if (module.archived_at || !projectIds?.includes(module.project_id)) return false;
        return projectAllowed ? projectAllowed(module.project_id) : true;
      }).length
    : (projectModuleIds?.length ?? 0);

  const canCreateWithPermissions = allowPermissions(
    [EUserProjectRoles.ADMIN, EUserProjectRoles.MEMBER],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug?.toString(),
    resolvedProjectId
  );
  const canPerformCreate = canCreate ?? canCreateWithPermissions;
  const showProjectBadge = Boolean(projectIds && !resolvedProjectId);
```

- [ ] **Step 2: Update the loading/empty/render branches**

Use the scoped layout for loaders:

```tsx
if (loader || (!workspaceSelection && (!projectModuleIds || !filteredModuleIds)))
  return (
    <>
      {layout === "list" && <CycleModuleListLayoutLoader />}
      {layout === "board" && <CycleModuleBoardLayoutLoader />}
      {layout === "gantt" && <GanttLayoutLoader />}
    </>
  );

if (totalModuleCount === 0)
  return (
    <EmptyStateDetailed
      assetKey="module"
      title={t("project_empty_state.modules.title")}
      description={t("project_empty_state.modules.description")}
      actions={[
        {
          label: t("project_empty_state.modules.cta_primary"),
          onClick: () => toggleCreateModuleModal(true),
          disabled: !canPerformCreate,
          variant: "primary",
        },
      ]}
    />
  );

if (!filteredModuleIds) return <CycleModuleListLayoutLoader />;

if (filteredModuleIds.length === 0)
  return (
    <EmptyStateDetailed
      assetKey="search"
      title={t("common_empty_state.search.title")}
      description={t("common_empty_state.search.description")}
    />
  );
```

Replace the layout rendering with:

```tsx
<div className="flex size-full justify-between">
  {layout === "list" && (
    <ListLayout>
      {filteredModuleIds.map((moduleId) => (
        <ModuleListItem key={moduleId} moduleId={moduleId} showProjectBadge={showProjectBadge} />
      ))}
    </ListLayout>
  )}
  {layout === "board" && (
    <Row
      className={`grid size-full grid-cols-1 gap-6 overflow-y-auto py-page-y ${
        peekModule ? "3xl:grid-cols-3 lg:grid-cols-1 xl:grid-cols-2" : "3xl:grid-cols-4 lg:grid-cols-2 xl:grid-cols-3"
      } vertical-scrollbar scrollbar-lg auto-rows-max transition-all`}
    >
      {filteredModuleIds.map((moduleId) => (
        <ModuleCardItem key={moduleId} moduleId={moduleId} showProjectBadge={showProjectBadge} />
      ))}
    </Row>
  )}
  {layout === "gantt" && resolvedProjectId && !isGanttDisabled && (
    <div className="size-full overflow-hidden">
      <ModulesListGanttChartView
        projectId={resolvedProjectId}
        moduleIds={filteredModuleIds}
        filterKey={resolvedFilterKey}
      />
    </div>
  )}
  <div className="flex-shrink-0">
    <ModulePeekOverview projectId={resolvedProjectId} workspaceSlug={workspaceSlug?.toString() ?? ""} />
  </div>
</div>
```

- [ ] **Step 3: Replace the workspace page**

Replace `apps/web/app/(all)/[workspaceSlug]/(projects)/modules/page.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useTheme } from "next-themes";
import { useParams } from "next/navigation";
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import type { TModuleFilters } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { EHeaderVariant, Header } from "@plane/ui";
import { calculateTotalFilters } from "@plane/utils";
import darkModulesAsset from "@/app/assets/empty-state/disabled-feature/modules-dark.webp?url";
import lightModulesAsset from "@/app/assets/empty-state/disabled-feature/modules-light.webp?url";
import { PageHead } from "@/components/core/page-title";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { ModuleAppliedFiltersList, ModulesListView } from "@/components/modules";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useModule } from "@/hooks/store/use-module";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceModulePermissions } from "@/hooks/use-workspace-module-permissions";
import { WORKSPACE_MODULE_FILTER_KEY } from "@/lib/workspace-modules";

function WorkspaceModulesPage() {
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const { resolvedTheme } = useTheme();
  const { t } = useTranslation();
  const [isLoading, setIsLoading] = useState(true);
  const { moduleMap, fetchWorkspaceModules } = useModule();
  const {
    clearAllFilters,
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    initProjectModuleFilters,
    updateDisplayFilters,
    updateFilters,
    updateSearchQuery,
  } = useModuleFilter();
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateModule } = useWorkspaceModulePermissions();
  const { toggleCreateModuleModal } = useCommandPalette();
  const { allowPermissions } = useUserPermissions();

  const filterKey = WORKSPACE_MODULE_FILTER_KEY;
  const appliedFilters = getFiltersByProjectId(filterKey) ?? {};
  const displayFilters = getDisplayFiltersByProjectId(filterKey);
  const projectIds = useMemo(
    () => (scopedProjectId ? [scopedProjectId] : (workspaceProjectIds ?? [])),
    [scopedProjectId, workspaceProjectIds]
  );
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const resolvedEmptyState = resolvedTheme === "light" ? lightModulesAsset : darkModulesAsset;
  const hasAdminLevelPermission = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.module_view === false);
  const totalModules = useMemo(
    () =>
      Object.values(moduleMap ?? {}).filter((module) => {
        if (module.archived_at || !projectIds.includes(module.project_id)) return false;
        const project = getProjectById(module.project_id);
        return project ? project.module_view !== false : true;
      }).length,
    [moduleMap, getProjectById, projectIds]
  );

  useEffect(() => {
    initProjectModuleFilters(filterKey);
    updateSearchQuery("");
  }, [filterKey, initProjectModuleFilters, updateSearchQuery]);

  useEffect(() => {
    if (scopedProjectId || !displayFilters || displayFilters.layout !== "gantt") return;
    updateDisplayFilters(filterKey, { layout: "list" });
  }, [displayFilters, filterKey, scopedProjectId, updateDisplayFilters]);

  useEffect(() => {
    if (!workspaceSlug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceModules(workspaceSlug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [fetchWorkspaceModules, workspaceSlug]);

  const handleRemoveFilter = (key: keyof TModuleFilters, value: string | null) => {
    const currentValues = appliedFilters[key] ?? [];
    const nextValues = value ? currentValues.filter((currentValue) => currentValue !== value) : [];
    updateFilters(filterKey, { [key]: nextValues });
  };

  if (!workspaceSlug) return null;
  if (isLoading || isInitializingProjects || workspaceProjectIds === undefined) return <CycleModuleListLayoutLoader />;

  if (isFeatureDisabled)
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title={t("disabled_project.empty_state.module.title")}
          description={t("disabled_project.empty_state.module.description")}
          assetPath={resolvedEmptyState}
          primaryButton={{
            text: t("disabled_project.empty_state.module.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !hasAdminLevelPermission,
          }}
        />
      </div>
    );

  return (
    <>
      <PageHead title={t("sidebar.modules")} />
      <div className="flex h-full w-full flex-col">
        {totalModules === 0 ? (
          <div className="h-full place-items-center">
            {scopedProjectId ? (
              <EmptyStateDetailed
                assetKey="module"
                title={t("project_empty_state.modules.title")}
                description={t("project_empty_state.modules.description")}
                actions={[
                  {
                    label: t("project_empty_state.modules.cta_primary"),
                    onClick: () => toggleCreateModuleModal(true),
                    variant: "primary",
                    disabled: !canCreateModule,
                  },
                ]}
              />
            ) : (
              <EmptyStateDetailed
                assetKey="module"
                title={t("workspace_empty_state.tracks.title")}
                description={t("workspace_empty_state.tracks.description")}
              />
            )}
          </div>
        ) : (
          <>
            {(calculateTotalFilters(appliedFilters) !== 0 || displayFilters?.favorites) && (
              <Header variant={EHeaderVariant.TERNARY}>
                <ModuleAppliedFiltersList
                  appliedFilters={appliedFilters}
                  isFavoriteFilterApplied={displayFilters?.favorites ?? false}
                  handleClearAllFilters={() => clearAllFilters(filterKey)}
                  handleRemoveFilter={handleRemoveFilter}
                  handleDisplayFiltersUpdate={(val) => updateDisplayFilters(filterKey, val)}
                  alwaysAllowEditing
                />
              </Header>
            )}
            <ModulesListView
              projectId={scopedProjectId ?? undefined}
              projectIds={projectIds}
              filterKey={filterKey}
              isGanttDisabled={!scopedProjectId}
              canCreate={canCreateModule}
            />
          </>
        )}
      </div>
    </>
  );
}

export default observer(WorkspaceModulesPage);
```

- [ ] **Step 4: Delete the legacy view**

```bash
rm apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx
```

- [ ] **Step 5: Run focused tests/checks**

```bash
pnpm exec vitest run core/lib/workspace-modules.test.ts
pnpm exec oxlint core/components/modules/modules-list-view.tsx 'app/(all)/[workspaceSlug]/(projects)/modules/page.tsx'
pnpm check:types
```

Expected: tests pass, 0 lint warnings/errors, typecheck exits 0.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/modules/modules-list-view.tsx 'apps/web/app/(all)/[workspaceSlug]/(projects)/modules/page.tsx'
git rm apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx
git commit -m "feat(web): use project modules view for workspace modules"
```

---

## Task 7: Full Verification and Deploy

**Files:**

- No new source files unless verification finds a defect.

- [ ] **Step 1: Run the full web test suite**

```bash
pnpm exec vitest run
```

Expected: all test files/tests pass.

- [ ] **Step 2: Run formatting, lint, and type checks**

```bash
pnpm fix:format
pnpm check:lint
pnpm check:types
```

Expected: all exit 0.

- [ ] **Step 3: Build**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: both exit 0, Vite prints `built in ...`.

- [ ] **Step 4: Restart the production web service**

```bash
systemctl --user restart plane-web-prod.service
sleep 20
curl -s -o /dev/null -w "modules=%{http_code}\n" http://localhost:3000/itsm/modules
```

Expected: `modules=200`.

- [ ] **Step 5: Manual and project-regression smoke**

Verify in the browser:

- `/itsm/modules` shows the current workspace Modules page, not the legacy grouped list.
- All applications: list and board show modules from multiple applications with project badges; gantt is disabled with the tooltip.
- Scoped application: header chip appears; list/board/gantt work; gantt edits use the module's own project; create opens the module modal directly.
- All applications: Add Module opens the application picker, then the module modal.
- Search, order, status/lead/member/date filters, favorites, applied chips, and clear-all use the workspace filter key and do not alter project filter state.
- Empty All, empty scoped, filtered-empty, and disabled-feature states match project Modules.
- Project Modules at `/itsm/projects/<projectId>/modules/` still behave exactly as before.

If a smoke check fails, fix it, rerun Steps 1-4, and commit the fix:

```bash
git add <changed-files>
git commit -m "fix(web): address workspace modules verification"
```

---

## Self-Review Notes

- Spec 7.5: reuses `ModulesListView`, `ModuleViewHeader`, `ModuleAppliedFiltersList`, `ModulePeekOverview`; list/board/gantt; scoped gantt; project badges; `module.project_id` for links/actions; create picker.
- Spec 6.3: selector filters by scope, archived state, `module_view`, search, shared module filters, and ordering. Workspace filters use `__workspace__`.
- Spec 7.8: scoped empty uses project keys; All empty uses `workspace_empty_state.tracks`; disabled feature uses the project disabled-module state; existing layout loaders are retained.
- Spec 7.9: scope chip is in the header; the mobile header keeps list/board layout selection like project Modules (gantt is not offered on mobile).
- Spec 8/9: create resolves scoped project or picker; permissions are evaluated against the resolved project.
- Project Modules: optional props default to route project id, so project behavior is unchanged.
- No new i18n keys; tooltip text is hardcoded English alongside existing hardcoded header strings.

---

## Execution notes (2026-10-22)

- Selector tests cover scope, feature flag, archived modules, search, status/lead/member/date filters, favorites, default and explicit ordering, and the reserved key.
- `ModuleViewHeader` now resolves both filters and display filters by `filterKey`; the gantt control uses `aria-disabled` (rather than the HTML `disabled` attribute) so its tooltip can still open.
- Workspace All keeps module rows/cards scoped per module project; the mobile header offers list/board only, matching project Modules.
- `ModuleCardItem`'s propagation handler was moved to module scope and given keyboard/role metadata to satisfy the repository lint rules. The existing `promise/always-return` warnings were fixed with explicit `return undefined`.
- Verification: 513 web tests pass, `pnpm check:types` passes, `pnpm check:lint` exits 0, `@plane/constants` + web builds pass, `plane-web-prod` restarted and `/itsm/modules`, `/itsm/cycles`, `/itsm/work-items` return 200.
- Manual browser smoke test was not run in this environment; the HTTP/build/test checks above are the available evidence.
