# Workspace Aggregate Parity — Phase 2 (Cycles) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make workspace Cycles a 1:1 match of project Cycles—same list, active-cycle hero for a single scoped application, filters, peek, project badges, create flow, and empty/disabled states—while aggregating across applications.

**Architecture:** Reuse `CyclesView` / `CyclesList` / `CyclePeekOverview` with optional `projectIds` and a reserved workspace filter key. A pure selector normalizes workspace cycles (the workspace endpoint may omit `status`), filters by scope/feature flag/search/date/status, and groups them into current/upcoming/draft/completed IDs. A reusable application-picker modal resolves the create target when the scope is All; the scoped application opens the existing modal directly.

**Tech Stack:** React, MobX, TypeScript, vitest, `@plane/ui`, `@plane/utils`, oxlint, Next.js navigation shims.

**Spec:** `docs/superpowers/specs/2026-10-21-workspace-parity-design.md` (sections 6.3, 6.4, 7.4, 8, 9, 11)

---

## File structure

Create:

- `apps/web/core/lib/workspace-cycles.ts` — workspace cycle normalization, scope filtering, grouping, filter key.
- `apps/web/core/lib/workspace-cycles.test.ts` — selector unit tests.
- `apps/web/core/lib/workspace-create-project.ts` — pure scoped/picked create-target resolver.
- `apps/web/core/lib/workspace-create-project.test.ts` — resolver tests.
- `apps/web/core/hooks/use-workspace-cycle-permissions.ts` — create permission resolution for scoped vs All.
- `apps/web/core/components/workspace/application-scope/project-picker-modal.tsx` — reusable application picker.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/header.tsx` — workspace cycles header (breadcrumb, chip, filters, add cycle).
- `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/create-modals.tsx` — command-palette-driven create flow (picker then modal).

Modify:

- `apps/web/core/store/cycle.store.ts` — normalize status when fetching workspace cycles.
- `apps/web/core/store/cycle_filter.store.ts` — guard `clearAllFilters` for lazy workspace keys.
- `apps/web/core/components/cycles/cycles-view-header.tsx` — accept a filter key instead of only the route project.
- `apps/web/core/components/cycles/cycles-view.tsx` — support `projectIds` / workspace filter key.
- `apps/web/core/components/cycles/list/root.tsx` — optional project, current group, active hero toggle, project badge.
- `apps/web/core/components/cycles/list/cycles-list-map.tsx` — optional project + badge.
- `apps/web/core/components/cycles/list/cycles-list-item.tsx` — derive project from the cycle, project badge.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/layout.tsx` — mount the new header and create modals.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/page.tsx` — replace the legacy aggregate list.

Delete:

- `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx` — replaced by the project `CyclesView`.

Do not change project Cycles pages, the project list header, or project create behavior.

---

### Task 1: Workspace cycle selector and status normalization

**Files:**

- Create: `apps/web/core/lib/workspace-cycles.ts`
- Create: `apps/web/core/lib/workspace-cycles.test.ts`
- Modify: `apps/web/core/store/cycle.store.ts:397-407`

- [ ] **Step 1: Write the failing selector test**

Create `apps/web/core/lib/workspace-cycles.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { ICycle } from "@plane/types";
import { normalizeWorkspaceCycle, selectWorkspaceCycles, WORKSPACE_CYCLE_FILTER_KEY } from "./workspace-cycles";

const NOW = new Date(2026, 9, 18); // Oct 18 2026, local time
const day = (value: number) => `2026-10-${String(value).padStart(2, "0")}`;

const makeCycle = (overrides: Partial<ICycle>): ICycle =>
  ({
    id: "cycle",
    name: "Cycle",
    project_id: "p1",
    start_date: null,
    end_date: null,
    archived_at: null,
    ...overrides,
  }) as ICycle;

describe("normalizeWorkspaceCycle", () => {
  it("derives a status when the workspace endpoint omits it", () => {
    const cycle = makeCycle({ id: "derived", start_date: day(16), end_date: day(20) });

    expect(normalizeWorkspaceCycle(cycle, NOW).status).toBe("current");
  });

  it("preserves an explicit status", () => {
    const cycle = makeCycle({
      id: "explicit",
      status: "completed",
      start_date: day(16),
      end_date: day(20),
    });

    expect(normalizeWorkspaceCycle(cycle, NOW).status).toBe("completed");
  });
});

describe("selectWorkspaceCycles", () => {
  it("groups and sorts current/upcoming/draft/completed cycles", () => {
    const selection = selectWorkspaceCycles({
      cycles: [
        makeCycle({ id: "current-b", name: "Beta", start_date: day(16), end_date: day(20) }),
        makeCycle({ id: "current-a", name: "Alpha", start_date: day(16), end_date: day(20) }),
        makeCycle({ id: "upcoming-later", name: "Later", start_date: day(26), end_date: day(30) }),
        makeCycle({ id: "upcoming-sooner", name: "Sooner", start_date: day(25), end_date: day(29) }),
        makeCycle({ id: "completed-new", name: "New", start_date: day(10), end_date: day(12) }),
        makeCycle({ id: "completed-old", name: "Old", start_date: day(1), end_date: day(5) }),
        makeCycle({ id: "draft", name: "Draft" }),
      ],
      projectIds: ["p1"],
      filters: {},
      searchQuery: "",
      now: NOW,
    });

    expect(selection.currentCycleIds).toEqual(["current-a", "current-b"]);
    expect(selection.upcomingCycleIds).toEqual(["upcoming-sooner", "upcoming-later"]);
    expect(selection.completedCycleIds).toEqual(["completed-new", "completed-old"]);
    expect(selection.draftCycleIds).toEqual(["draft"]);
    expect(selection.totalCount).toBe(7);
  });

  it("filters by project scope, archived state, and the project feature flag", () => {
    const selection = selectWorkspaceCycles({
      cycles: [
        makeCycle({ id: "p1", project_id: "p1", status: "current" }),
        makeCycle({ id: "p2", project_id: "p2", status: "current" }),
        makeCycle({ id: "p3", project_id: "p3", status: "current" }),
        makeCycle({ id: "archived", project_id: "p1", status: "current", archived_at: "2026-10-01" }),
      ],
      projectIds: ["p1", "p2"],
      filters: {},
      searchQuery: "",
      projectAllowed: (projectId) => projectId !== "p3",
      now: NOW,
    });

    expect(selection.currentCycleIds).toEqual(["p1", "p2"]);
    expect(selection.totalCount).toBe(2);
  });

  it("applies search and status filters", () => {
    const selection = selectWorkspaceCycles({
      cycles: [
        makeCycle({ id: "alpha-current", name: "Alpha", status: "current" }),
        makeCycle({ id: "alpha-completed", name: "Alpha", status: "completed" }),
        makeCycle({ id: "beta-current", name: "Beta", status: "current" }),
      ],
      projectIds: ["p1"],
      filters: { status: ["current"] },
      searchQuery: "alpha",
      now: NOW,
    });

    expect(selection.currentCycleIds).toEqual(["alpha-current"]);
    expect(selection.completedCycleIds).toEqual([]);
    expect(selection.totalCount).toBe(1);
  });

  it("exports the reserved workspace filter key", () => {
    expect(WORKSPACE_CYCLE_FILTER_KEY).toBe("__workspace__");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `apps/web`:

```bash
pnpm exec vitest run core/lib/workspace-cycles.test.ts
```

Expected: FAIL with “Failed to resolve import ./workspace-cycles”.

- [ ] **Step 3: Implement the selector**

Create `apps/web/core/lib/workspace-cycles.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ICycle, TCycleFilters, TCycleGroups } from "@plane/types";
import { getDate, shouldFilterCycle } from "@plane/utils";
// local imports
import { deriveCycleStatus } from "./workspace-aggregate";

export const WORKSPACE_CYCLE_FILTER_KEY = "__workspace__";

export type TWorkspaceCycleSelection = {
  currentCycleIds: string[];
  upcomingCycleIds: string[];
  draftCycleIds: string[];
  completedCycleIds: string[];
  totalCount: number;
};

type TSelectWorkspaceCyclesArgs = {
  cycles: ICycle[];
  projectIds: string[];
  filters: TCycleFilters;
  searchQuery: string;
  projectAllowed?: (projectId: string) => boolean;
  now?: Date;
};

const dateValue = (date: string | null | undefined) => {
  const parsed = getDate(date);
  const time = parsed?.getTime();
  return time !== undefined && !Number.isNaN(time) ? time : Number.POSITIVE_INFINITY;
};

/**
 * The workspace cycles endpoint does not always return `status`. Normalize it
 * from the date range so every row/action behaves like a project cycle.
 */
export const normalizeWorkspaceCycle = (cycle: ICycle, now: Date = new Date()): ICycle => {
  const status = (cycle.status?.toLowerCase() as TCycleGroups | undefined) ?? deriveCycleStatus(cycle, now);
  return cycle.status === status ? cycle : { ...cycle, status };
};

export const selectWorkspaceCycles = ({
  cycles,
  projectIds,
  filters,
  searchQuery,
  projectAllowed,
  now = new Date(),
}: TSelectWorkspaceCyclesArgs): TWorkspaceCycleSelection => {
  const projectIdSet = new Set(projectIds);
  const normalizedSearch = searchQuery.trim().toLowerCase();

  const filteredCycles = cycles
    .map((cycle) => normalizeWorkspaceCycle(cycle, now))
    .filter((cycle) => projectIdSet.has(cycle.project_id))
    .filter((cycle) => !cycle.archived_at)
    .filter((cycle) => (projectAllowed ? projectAllowed(cycle.project_id) : true))
    .filter((cycle) => cycle.name.toLowerCase().includes(normalizedSearch))
    .filter((cycle) => shouldFilterCycle(cycle, filters));

  const groups: Record<TCycleGroups, ICycle[]> = {
    current: [],
    upcoming: [],
    draft: [],
    completed: [],
  };

  filteredCycles.forEach((cycle) => {
    const status = (cycle.status ?? "draft") as TCycleGroups;
    groups[status].push(cycle);
  });

  const byName = (a: ICycle, b: ICycle) => a.name.localeCompare(b.name);
  const byStartDate = (a: ICycle, b: ICycle) => dateValue(a.start_date) - dateValue(b.start_date);
  const completedFirst = (a: ICycle, b: ICycle) => Number(!a.start_date) - Number(!b.start_date);

  groups.current.sort(byName);
  groups.upcoming.sort(byStartDate);
  groups.draft.sort(byName);
  groups.completed.sort(completedFirst);

  return {
    currentCycleIds: groups.current.map((cycle) => cycle.id),
    upcomingCycleIds: groups.upcoming.map((cycle) => cycle.id),
    draftCycleIds: groups.draft.map((cycle) => cycle.id),
    completedCycleIds: groups.completed.map((cycle) => cycle.id),
    totalCount: filteredCycles.length,
  };
};
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `apps/web`:

```bash
pnpm exec vitest run core/lib/workspace-cycles.test.ts
```

Expected: PASS (6 tests).

- [ ] **Step 5: Normalize workspace cycles in the store**

In `apps/web/core/store/cycle.store.ts`, add the import near the other `@/lib` / service imports:

```ts
// helpers
import { normalizeWorkspaceCycle } from "@/lib/workspace-cycles";
```

Replace the `fetchWorkspaceCycles` body:

```ts
/**
 * @description fetch all cycles
 * @param workspaceSlug
 * @returns ICycle[]
 */
fetchWorkspaceCycles = async (workspaceSlug: string) =>
  await this.cycleService.getWorkspaceCycles(workspaceSlug).then((response) => {
    runInAction(() => {
      response.forEach((cycle) => {
        const normalizedCycle = normalizeWorkspaceCycle(cycle);
        set(this.cycleMap, [normalizedCycle.id], {
          ...this.cycleMap[normalizedCycle.id],
          ...normalizedCycle,
        });
        set(this.fetchedMap, normalizedCycle.project_id, true);
      });
    });
    return response;
  });
```

- [ ] **Step 6: Type-check and lint the touched files**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "workspace-cycles|cycle.store" ; echo DONE
```

Expected: only `DONE` (no error lines above it).

```bash
pnpm exec oxlint core/lib/workspace-cycles.ts core/lib/workspace-cycles.test.ts core/store/cycle.store.ts
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/lib/workspace-cycles.ts \
  apps/web/core/lib/workspace-cycles.test.ts \
  apps/web/core/store/cycle.store.ts
git commit -m "feat(web): add workspace cycle selector and status normalization"
```

---

### Task 2: Workspace filter key support in the cycles header

**Files:**

- Modify: `apps/web/core/components/cycles/cycles-view-header.tsx`
- Modify: `apps/web/core/store/cycle_filter.store.ts:172-178`

- [ ] **Step 1: Make the header filter-key aware**

Replace the props and store usage in `apps/web/core/components/cycles/cycles-view-header.tsx`:

```tsx
type Props = {
  projectId?: string;
  filterKey?: string;
};

export const CyclesViewHeader = observer(function CyclesViewHeader(props: Props) {
  const { projectId, filterKey } = props;
  const resolvedFilterKey = filterKey ?? projectId;
  // refs
  const inputRef = useRef<HTMLInputElement>(null);
  // hooks
  const { getFiltersByProjectId, searchQuery, updateFilters, updateSearchQuery } = useCycleFilter();
  const { t } = useTranslation();
  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
  // states
  const [isSearchOpen, setIsSearchOpen] = useState(searchQuery !== "" ? true : false);
  // outside click detector hook
  useOutsideClickDetector(inputRef, () => {
    if (isSearchOpen && searchQuery.trim() === "") setIsSearchOpen(false);
  });

  const handleFilters = useCallback(
    (key: keyof TCycleFilters, value: string | string[]) => {
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

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      if (searchQuery && searchQuery.trim() !== "") updateSearchQuery("");
      else {
        setIsSearchOpen(false);
        inputRef.current?.blur();
      }
    }
  };

  const isFiltersApplied = calculateTotalFilters(filters ?? {}) !== 0;

  useEffect(() => {
    if (searchQuery.trim() !== "") setIsSearchOpen(true);
  }, [searchQuery]);

  return (
    <div className="flex items-center gap-2">
      {!isSearchOpen ? (
        <IconButton
          variant="ghost"
          size="lg"
          onClick={() => {
            setIsSearchOpen(true);
            inputRef.current?.focus();
          }}
          icon={SearchOutline}
        />
      ) : (
        <div
          className={cn(
            "ml-auto flex w-0 items-center justify-start gap-1 overflow-hidden rounded-md border border-transparent bg-surface-1 text-placeholder opacity-0 transition-[width] ease-linear",
            {
              "w-64 border-subtle px-2.5 py-1.5 opacity-100": isSearchOpen,
            }
          )}
        >
          <SearchOutline className="h-3.5 w-3.5" />
          <input
            ref={inputRef}
            className="w-full max-w-[234px] border-none bg-transparent text-13 text-primary placeholder:text-placeholder focus:outline-none"
            placeholder="Search"
            value={searchQuery}
            onChange={(e) => updateSearchQuery(e.target.value)}
            onKeyDown={handleInputKeyDown}
          />
          {isSearchOpen && (
            <button
              type="button"
              className="grid place-items-center"
              onClick={() => {
                updateSearchQuery("");
                setIsSearchOpen(false);
              }}
            >
              <CloseOutline className="h-3 w-3" />
            </button>
          )}
        </div>
      )}

      <FiltersDropdown
        icon={<FilterOutline className="h-3 w-3" />}
        title={t("common.filters")}
        placement="bottom-end"
        isFiltersApplied={isFiltersApplied}
      >
        <CycleFiltersSelection filters={filters ?? {}} handleFiltersUpdate={handleFilters} />
      </FiltersDropdown>
    </div>
  );
});
```

Keep the existing imports; only the props/store lines and filter references change.

- [ ] **Step 2: Guard lazy workspace filter keys**

In `apps/web/core/store/cycle_filter.store.ts`, replace `clearAllFilters`:

```ts
/**
 * @description clear all filters of a project
 * @param {string} projectId
 */
clearAllFilters = (projectId: string, state: keyof TCycleFiltersByState = "default") => {
  runInAction(() => {
    if (!this.filters[projectId]) return;
    this.filters[projectId][state] = {};
  });
};
```

- [ ] **Step 3: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "cycles-view-header|cycle_filter" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/components/cycles/cycles-view-header.tsx core/store/cycle_filter.store.ts
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/cycles/cycles-view-header.tsx \
  apps/web/core/store/cycle_filter.store.ts
git commit -m "feat(web): support workspace filter keys in cycles header"
```

---

### Task 3: Make the project cycle view/list scope-aware

**Files:**

- Modify: `apps/web/core/components/cycles/cycles-view.tsx`
- Modify: `apps/web/core/components/cycles/list/root.tsx`
- Modify: `apps/web/core/components/cycles/list/cycles-list-map.tsx`
- Modify: `apps/web/core/components/cycles/list/cycles-list-item.tsx`

- [ ] **Step 1: Extend `CyclesView`**

Replace `apps/web/core/components/cycles/cycles-view.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo } from "react";
import { observer } from "mobx-react";
// components
import { useTranslation } from "@plane/i18n";
// assets
import AllFiltersImage from "@/app/assets/empty-state/cycle/all-filters.svg?url";
import NameFilterImage from "@/app/assets/empty-state/cycle/name-filter.svg?url";
// components
import { CyclesList } from "@/components/cycles/list";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useCycleFilter } from "@/hooks/store/use-cycle-filter";
import { useProject } from "@/hooks/store/use-project";
// helpers
import { selectWorkspaceCycles, WORKSPACE_CYCLE_FILTER_KEY } from "@/lib/workspace-cycles";

export interface ICyclesView {
  workspaceSlug: string;
  projectId?: string;
  projectIds?: string[];
  filterKey?: string;
}

export const CyclesView = observer(function CyclesView(props: ICyclesView) {
  const { workspaceSlug, projectId, projectIds, filterKey: filterKeyProp } = props;
  const filterKey = filterKeyProp ?? projectId ?? WORKSPACE_CYCLE_FILTER_KEY;
  // store hooks
  const { getFilteredCycleIds, getFilteredCompletedCycleIds, loader, currentProjectActiveCycleId, cycleMap } =
    useCycle();
  const { getFiltersByProjectId, searchQuery } = useCycleFilter();
  const { getProjectById } = useProject();
  const { t } = useTranslation();
  // derived values
  const filters = getFiltersByProjectId(filterKey) ?? {};
  const projectAllowed = useMemo(() => {
    if (!projectIds) return undefined;
    return (cycleProjectId: string) => {
      const project = getProjectById(cycleProjectId);
      return project ? project.cycle_view !== false : true;
    };
  }, [getProjectById, projectIds]);

  const workspaceSelection = useMemo(() => {
    if (!projectIds) return null;
    return selectWorkspaceCycles({
      cycles: Object.values(cycleMap ?? {}),
      projectIds,
      filters,
      searchQuery,
      projectAllowed,
    });
  }, [
    cycleMap,
    filters,
    filters.start_date,
    filters.status,
    filters.end_date,
    projectAllowed,
    projectIds,
    searchQuery,
  ]);

  const filteredCycleIds = projectId ? getFilteredCycleIds(projectId, false) : null;
  const filteredCompletedCycleIds = projectId ? getFilteredCompletedCycleIds(projectId) : null;
  const filteredUpcomingCycleIds = (filteredCycleIds ?? []).filter(
    (cycleId) => cycleId !== currentProjectActiveCycleId
  );

  const workspaceCycleIds = workspaceSelection
    ? [
        ...workspaceSelection.currentCycleIds,
        ...workspaceSelection.upcomingCycleIds,
        ...workspaceSelection.draftCycleIds,
      ]
    : null;
  const cycleIds = workspaceCycleIds ?? filteredCycleIds;
  const upcomingCycleIds = workspaceSelection
    ? [...workspaceSelection.upcomingCycleIds, ...workspaceSelection.draftCycleIds]
    : filteredUpcomingCycleIds;
  const completedCycleIds = workspaceSelection?.completedCycleIds ?? filteredCompletedCycleIds ?? [];
  const currentCycleIds = workspaceSelection?.currentCycleIds;
  const activeCycleId = workspaceSelection && projectId ? (currentCycleIds?.[0] ?? null) : undefined;

  if (loader || (!workspaceSelection && !filteredCycleIds)) return <CycleModuleListLayoutLoader />;

  const hasNoResults = workspaceSelection
    ? workspaceSelection.totalCount === 0
    : (filteredCycleIds?.length ?? 0) === 0 && (filteredCompletedCycleIds?.length ?? 0) === 0;

  if (hasNoResults)
    return (
      <div className="grid h-full w-full place-items-center">
        <div className="text-center">
          <img
            src={searchQuery.trim() === "" ? AllFiltersImage : NameFilterImage}
            className="mx-auto h-36 w-36 object-contain sm:h-48 sm:w-48"
            alt="No matching cycles"
          />
          <h5 className="mt-7 mb-1 text-18 font-medium">{t("project_cycles.no_matching_cycles")}</h5>
          <p className="text-14 text-placeholder">
            {searchQuery.trim() === ""
              ? t("project_cycles.remove_filters_to_see_all_cycles")
              : t("project_cycles.remove_search_criteria_to_see_all_cycles")}
          </p>
        </div>
      </div>
    );

  return (
    <CyclesList
      completedCycleIds={completedCycleIds}
      upcomingCycleIds={upcomingCycleIds}
      currentCycleIds={workspaceSelection && !projectId ? currentCycleIds : undefined}
      cycleIds={cycleIds ?? []}
      activeCycleId={activeCycleId}
      showActiveCycle={!workspaceSelection || Boolean(projectId)}
      showProjectBadge={Boolean(workspaceSelection && !projectId)}
      projectId={projectId}
      workspaceSlug={workspaceSlug}
    />
  );
});
```

- [ ] **Step 2: Extend the list shell**

Replace `apps/web/core/components/cycles/list/root.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React from "react";
import { observer } from "mobx-react";
import { Disclosure } from "@headlessui/react";
// components
import { useTranslation } from "@plane/i18n";
import { ContentWrapper, ERowVariant } from "@plane/ui";
import { ListLayout } from "@/components/core/list";
// local imports
import { CyclePeekOverview } from "../cycle-peek-overview";
import { CycleListGroupHeader } from "./cycle-list-group-header";
import { CyclesListMap } from "./cycles-list-map";
import { ActiveCycleRoot } from "../active-cycle/root";

export interface ICyclesList {
  completedCycleIds: string[];
  upcomingCycleIds?: string[] | undefined;
  currentCycleIds?: string[];
  cycleIds: string[];
  workspaceSlug: string;
  projectId?: string;
  activeCycleId?: string | null;
  showActiveCycle?: boolean;
  showProjectBadge?: boolean;
  isArchived?: boolean;
}

export const CyclesList = observer(function CyclesList(props: ICyclesList) {
  const {
    completedCycleIds,
    upcomingCycleIds,
    currentCycleIds,
    cycleIds,
    workspaceSlug,
    projectId,
    activeCycleId,
    showActiveCycle = true,
    showProjectBadge = false,
    isArchived = false,
  } = props;
  const { t } = useTranslation();

  return (
    <ContentWrapper variant={ERowVariant.HUGGING} className="flex-row">
      <ListLayout>
        {isArchived ? (
          <>
            <CyclesListMap
              cycleIds={cycleIds}
              projectId={projectId}
              workspaceSlug={workspaceSlug}
              showProjectBadge={showProjectBadge}
            />
          </>
        ) : (
          <>
            {showActiveCycle && projectId && (
              <ActiveCycleRoot
                workspaceSlug={workspaceSlug}
                projectId={projectId}
                cycleId={activeCycleId ?? undefined}
              />
            )}

            {currentCycleIds && currentCycleIds.length > 0 && (
              <Disclosure as="div" className="flex flex-shrink-0 flex-col" defaultOpen>
                {({ open }) => (
                  <>
                    <Disclosure.Button className="sticky top-0 z-[2] w-full flex-shrink-0 cursor-pointer border-b border-subtle bg-layer-1">
                      <CycleListGroupHeader
                        title={t("project_cycles.active_cycle.label")}
                        type="current"
                        count={currentCycleIds.length}
                        showCount
                        isExpanded={open}
                      />
                    </Disclosure.Button>
                    <Disclosure.Panel>
                      <CyclesListMap
                        cycleIds={currentCycleIds}
                        projectId={projectId}
                        workspaceSlug={workspaceSlug}
                        showProjectBadge={showProjectBadge}
                      />
                    </Disclosure.Panel>
                  </>
                )}
              </Disclosure>
            )}

            {upcomingCycleIds && (
              <Disclosure as="div" className="flex flex-shrink-0 flex-col" defaultOpen>
                {({ open }) => (
                  <>
                    <Disclosure.Button className="sticky top-0 z-[2] w-full flex-shrink-0 cursor-pointer border-b border-subtle bg-layer-1">
                      <CycleListGroupHeader
                        title={t("project_cycles.upcoming_cycle.label")}
                        type="upcoming"
                        count={upcomingCycleIds.length}
                        showCount
                        isExpanded={open}
                      />
                    </Disclosure.Button>
                    <Disclosure.Panel>
                      <CyclesListMap
                        cycleIds={upcomingCycleIds}
                        projectId={projectId}
                        workspaceSlug={workspaceSlug}
                        showProjectBadge={showProjectBadge}
                      />
                    </Disclosure.Panel>
                  </>
                )}
              </Disclosure>
            )}
            <Disclosure as="div" className="flex flex-shrink-0 flex-col pb-7">
              {({ open }) => (
                <>
                  <Disclosure.Button className="sticky top-0 z-2 w-full flex-shrink-0 cursor-pointer border-b border-subtle bg-layer-1">
                    <CycleListGroupHeader
                      title={t("project_cycles.completed_cycle.label")}
                      type="completed"
                      count={completedCycleIds.length}
                      showCount
                      isExpanded={open}
                    />
                  </Disclosure.Button>
                  <Disclosure.Panel>
                    <CyclesListMap
                      cycleIds={completedCycleIds}
                      projectId={projectId}
                      workspaceSlug={workspaceSlug}
                      showProjectBadge={showProjectBadge}
                    />
                  </Disclosure.Panel>
                </>
              )}
            </Disclosure>
          </>
        )}
      </ListLayout>
      <CyclePeekOverview projectId={projectId} workspaceSlug={workspaceSlug} isArchived={isArchived} />
    </ContentWrapper>
  );
});
```

- [ ] **Step 3: Make the map project/id optional**

Replace `apps/web/core/components/cycles/list/cycles-list-map.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// components
import { CyclesListItem } from "./cycles-list-item";

type Props = {
  cycleIds: string[];
  projectId?: string;
  workspaceSlug: string;
  showProjectBadge?: boolean;
};

export function CyclesListMap(props: Props) {
  const { cycleIds, projectId, workspaceSlug, showProjectBadge = false } = props;

  return (
    <>
      {cycleIds.map((cycleId) => (
        <CyclesListItem
          key={cycleId}
          cycleId={cycleId}
          workspaceSlug={workspaceSlug}
          projectId={projectId}
          showProjectBadge={showProjectBadge}
        />
      ))}
    </>
  );
}
```

- [ ] **Step 4: Derive each row’s project and render the optional badge**

In `apps/web/core/components/cycles/list/cycles-list-item.tsx`:

Add the project hook import:

```tsx
import { useProject } from "@/hooks/store/use-project";
```

Change the props type:

```tsx
type TCyclesListItem = {
  cycleId: string;
  handleEditCycle?: () => void;
  handleDeleteCycle?: () => void;
  handleAddToFavorites?: () => void;
  handleRemoveFromFavorites?: () => void;
  workspaceSlug: string;
  projectId?: string;
  showProjectBadge?: boolean;
  className?: string;
};
```

Change the destructure and add the resolved project:

```tsx
export const CyclesListItem = observer(function CyclesListItem(props: TCyclesListItem) {
  const { cycleId, workspaceSlug, projectId, showProjectBadge = false, className = "" } = props;
  // refs
  const parentRef = useRef(null);
  // router
  const router = useAppRouter();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  // hooks
  const { isMobile } = usePlatformOS();
  // store hooks
  const { getCycleById } = useCycle();
  const { getProjectById } = useProject();

  // derived values
  const cycleDetails = getCycleById(cycleId);
  const resolvedProjectId = cycleDetails?.project_id || projectId;
  const project = showProjectBadge && resolvedProjectId ? getProjectById(resolvedProjectId) : undefined;

  if (!cycleDetails || !resolvedProjectId) return null;
```

Change `itemLink`:

```tsx
      itemLink={`/${workspaceSlug}/projects/${resolvedProjectId}/cycles/${cycleDetails.id}`}
```

Add `appendTitleElement` after `prependTitleElement`:

```tsx
      appendTitleElement={
        project ? (
          <span className="max-w-[140px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5 text-11 text-tertiary">
            {project.name}
          </span>
        ) : undefined
      }
```

Replace the `projectId` values passed to actions:

```tsx
<CycleListItemAction
  workspaceSlug={workspaceSlug}
  projectId={resolvedProjectId}
  cycleId={cycleId}
  cycleDetails={cycleDetails}
  parentRef={parentRef}
  isActive={isActive}
/>
```

```tsx
<CycleQuickActions
  parentRef={parentRef}
  cycleId={cycleId}
  projectId={resolvedProjectId}
  workspaceSlug={workspaceSlug}
/>
```

- [ ] **Step 5: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "cycles-view|cycles/list" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/components/cycles/cycles-view.tsx \
  core/components/cycles/list/root.tsx \
  core/components/cycles/list/cycles-list-map.tsx \
  core/components/cycles/list/cycles-list-item.tsx
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/cycles/cycles-view.tsx \
  apps/web/core/components/cycles/list/root.tsx \
  apps/web/core/components/cycles/list/cycles-list-map.tsx \
  apps/web/core/components/cycles/list/cycles-list-item.tsx
git commit -m "feat(web): make cycles view scope-aware"
```

---

### Task 4: Workspace cycles shell, create picker, and permissions

**Files:**

- Create: `apps/web/core/lib/workspace-create-project.ts`
- Create: `apps/web/core/lib/workspace-create-project.test.ts`
- Create: `apps/web/core/hooks/use-workspace-cycle-permissions.ts`
- Create: `apps/web/core/components/workspace/application-scope/project-picker-modal.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/header.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/create-modals.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/layout.tsx`

- [ ] **Step 1: Write the failing create-target test**

Create `apps/web/core/lib/workspace-create-project.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { resolveWorkspaceCreateProject } from "./workspace-create-project";

describe("resolveWorkspaceCreateProject", () => {
  it("uses the scoped application immediately", () => {
    expect(resolveWorkspaceCreateProject("project-1", null)).toBe("project-1");
  });

  it("uses the picked application when the scope is All", () => {
    expect(resolveWorkspaceCreateProject(null, "project-2")).toBe("project-2");
  });

  it("prefers the scope over a stale pick", () => {
    expect(resolveWorkspaceCreateProject("project-1", "project-2")).toBe("project-1");
  });

  it("returns null when neither is available", () => {
    expect(resolveWorkspaceCreateProject(null, null)).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `apps/web`:

```bash
pnpm exec vitest run core/lib/workspace-create-project.test.ts
```

Expected: FAIL with “Failed to resolve import ./workspace-create-project”.

- [ ] **Step 3: Implement the resolver**

Create `apps/web/core/lib/workspace-create-project.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const resolveWorkspaceCreateProject = (
  scopedProjectId: string | null,
  pickedProjectId: string | null
): string | null => scopedProjectId ?? pickedProjectId;
```

Run the test again:

```bash
pnpm exec vitest run core/lib/workspace-create-project.test.ts
```

Expected: PASS (4 tests).

- [ ] **Step 4: Implement the create-permission hook**

Create `apps/web/core/hooks/use-workspace-cycle-permissions.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const useWorkspaceCyclePermissions = () => {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // scope
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  // store hooks
  const { getProjectById, workspaceProjectIds } = useProject();
  const { allowPermissions } = useUserPermissions();
  const { projectsWithCreatePermissions } = useUser();

  const canCreateInProject = (projectId: string) => {
    const project = getProjectById(projectId);
    return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.cycle_view !== false;
  };

  const canCreateCycle = scopedProjectId
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

  return { canCreateCycle };
};
```

- [ ] **Step 5: Implement the reusable application picker**

Create `apps/web/core/components/workspace/application-scope/project-picker-modal.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ProjectDropdown } from "@/components/dropdowns/project/dropdown";
// hooks
import { useProject } from "@/hooks/store/use-project";

type Props = {
  isOpen: boolean;
  handleClose: () => void;
  onSelect: (projectId: string) => void;
  projectIds: string[];
};

export const WorkspaceApplicationPickerModal = observer(function WorkspaceApplicationPickerModal(props: Props) {
  const { isOpen, handleClose, onSelect, projectIds } = props;
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  // states
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) setSelectedProjectId(null);
  }, [isOpen]);

  const selectedProject = selectedProjectId ? getProjectById(selectedProjectId) : undefined;

  return (
    <ModalCore isOpen={isOpen} handleClose={handleClose} position={EModalPosition.CENTER} width={EModalWidth.SM}>
      <div className="flex flex-col gap-4 p-5">
        <h3 className="text-16 font-medium text-primary">{t("common.project")}</h3>
        <ProjectDropdown
          multiple={false}
          value={selectedProjectId}
          onChange={setSelectedProjectId}
          buttonVariant="border-with-text"
          buttonClassName="w-full"
          renderCondition={(projectId) => projectIds.includes(projectId)}
        />
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={handleClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            disabled={!selectedProject}
            onClick={() => selectedProjectId && onSelect(selectedProjectId)}
          >
            {t("common.continue")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
```

- [ ] **Step 6: Implement the create-modals controller**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/create-modals.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// components
import { CycleCreateUpdateModal } from "@/components/cycles/modal";
import { WorkspaceApplicationPickerModal } from "@/components/workspace/application-scope/project-picker-modal";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useProject } from "@/hooks/store/use-project";
import { useUser } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
// helpers
import { resolveWorkspaceCreateProject } from "@/lib/workspace-create-project";

export const WorkspaceCyclesCreateModals = observer(function WorkspaceCyclesCreateModals() {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // store hooks
  const { isCreateCycleModalOpen, toggleCreateCycleModal } = useCommandPalette();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { workspaceProjectIds, getProjectById } = useProject();
  const { projectsWithCreatePermissions } = useUser();
  // states
  const [pickedProjectId, setPickedProjectId] = useState<string | null>(null);

  const eligibleProjectIds = useMemo(
    () =>
      (workspaceProjectIds ?? []).filter((projectId) => {
        const project = getProjectById(projectId);
        return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.cycle_view !== false;
      }),
    [getProjectById, projectsWithCreatePermissions, workspaceProjectIds]
  );

  const createProjectId = resolveWorkspaceCreateProject(scopedProjectId, pickedProjectId);
  const showPicker = isCreateCycleModalOpen && !createProjectId;

  const handleClose = () => {
    setPickedProjectId(null);
    toggleCreateCycleModal(false);
  };

  useEffect(() => {
    if (!isCreateCycleModalOpen) setPickedProjectId(null);
  }, [isCreateCycleModalOpen]);

  useEffect(
    () => () => {
      toggleCreateCycleModal(false);
    },
    [toggleCreateCycleModal]
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
      <CycleCreateUpdateModal
        isOpen={isCreateCycleModalOpen && Boolean(createProjectId)}
        handleClose={handleClose}
        workspaceSlug={workspaceSlug}
        projectId={createProjectId ?? ""}
      />
    </>
  );
});
```

- [ ] **Step 7: Implement the workspace cycles header**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { CyclesOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CyclesViewHeader } from "@/components/cycles/cycles-view-header";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useWorkspaceCyclePermissions } from "@/hooks/use-workspace-cycle-permissions";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { WORKSPACE_CYCLE_FILTER_KEY } from "@/lib/workspace-cycles";

export const WorkspaceCyclesHeader = observer(function WorkspaceCyclesHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { toggleCreateCycleModal } = useCommandPalette();
  const { canCreateCycle } = useWorkspaceCyclePermissions();

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.cycles")}
                  href={`/${workspaceSlug}/cycles/`}
                  icon={<CyclesOutline className="h-4 w-4 text-tertiary" />}
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
        <CyclesViewHeader filterKey={WORKSPACE_CYCLE_FILTER_KEY} />
        {canCreateCycle && (
          <Button variant="primary" size="lg" onClick={() => toggleCreateCycleModal(true)}>
            <div className="block sm:hidden">{t("add")}</div>
            <div className="hidden sm:block">{t("project_cycles.add_cycle")}</div>
          </Button>
        )}
      </Header.RightItem>
    </Header>
  );
});
```

- [ ] **Step 8: Mount the new shell**

Replace `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/layout.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// local imports
import { WorkspaceCyclesCreateModals } from "./create-modals";
import { WorkspaceCyclesHeader } from "./header";

function WorkspaceSprintsLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceCyclesHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
      <WorkspaceCyclesCreateModals />
    </>
  );
}

export default observer(WorkspaceSprintsLayout);
```

- [ ] **Step 9: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "workspace-create-project|use-workspace-cycle-permissions|project-picker-modal|cycles/(header|create-modals|layout)" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/lib/workspace-create-project.ts \
  core/lib/workspace-create-project.test.ts \
  core/hooks/use-workspace-cycle-permissions.ts \
  core/components/workspace/application-scope/project-picker-modal.tsx \
  'app/(all)/[workspaceSlug]/(projects)/cycles/header.tsx' \
  'app/(all)/[workspaceSlug]/(projects)/cycles/create-modals.tsx' \
  'app/(all)/[workspaceSlug]/(projects)/cycles/layout.tsx'
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 10: Commit**

```bash
git add apps/web/core/lib/workspace-create-project.ts \
  apps/web/core/lib/workspace-create-project.test.ts \
  apps/web/core/hooks/use-workspace-cycle-permissions.ts \
  apps/web/core/components/workspace/application-scope/project-picker-modal.tsx \
  'apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/header.tsx' \
  'apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/create-modals.tsx' \
  'apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/layout.tsx'
git commit -m "feat(web): add workspace cycles shell and create picker"
```

---

### Task 5: Replace the legacy workspace cycles page

**Files:**

- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/page.tsx`
- Delete: `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx`

- [ ] **Step 1: Replace the page**

Replace `apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/page.tsx` with:

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
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import type { TCycleFilters } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { EHeaderVariant, Header } from "@plane/ui";
import { calculateTotalFilters } from "@plane/utils";
// assets
import darkEmptyState from "@/app/assets/empty-state/disabled-feature/cycles-dark.webp?url";
import lightEmptyState from "@/app/assets/empty-state/disabled-feature/cycles-light.webp?url";
// components
import { PageHead } from "@/components/core/page-title";
import { CycleAppliedFiltersList } from "@/components/cycles/applied-filters";
import { CyclesView } from "@/components/cycles/cycles-view";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useCycle } from "@/hooks/store/use-cycle";
import { useCycleFilter } from "@/hooks/store/use-cycle-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceCyclePermissions } from "@/hooks/use-workspace-cycle-permissions";
// helpers
import { WORKSPACE_CYCLE_FILTER_KEY } from "@/lib/workspace-cycles";

function WorkspaceSprintsPage() {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const router = useAppRouter();
  // theme
  const { resolvedTheme } = useTheme();
  // states
  const [isLoading, setIsLoading] = useState(true);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { cycleMap, fetchWorkspaceCycles } = useCycle();
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateCycle } = useWorkspaceCyclePermissions();
  const { toggleCreateCycleModal } = useCommandPalette();
  const { clearAllFilters, getFiltersByProjectId, initProjectCycleFilters, updateFilters, updateSearchQuery } =
    useCycleFilter();
  const { allowPermissions } = useUserPermissions();

  // derived values
  const filterKey = WORKSPACE_CYCLE_FILTER_KEY;
  const appliedFilters = getFiltersByProjectId(filterKey);
  const projectIds = useMemo(
    () => (scopedProjectId ? [scopedProjectId] : (workspaceProjectIds ?? [])),
    [scopedProjectId, workspaceProjectIds]
  );
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const resolvedEmptyState = resolvedTheme === "light" ? lightEmptyState : darkEmptyState;
  const hasAdminLevelPermission = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.cycle_view === false);
  const totalCycles = useMemo(
    () =>
      Object.values(cycleMap ?? {}).filter((cycle) => {
        if (cycle.archived_at || !projectIds.includes(cycle.project_id)) return false;
        const project = getProjectById(cycle.project_id);
        return project ? project.cycle_view !== false : true;
      }).length,
    [cycleMap, getProjectById, projectIds]
  );

  useEffect(() => {
    initProjectCycleFilters(filterKey);
    updateSearchQuery("");
  }, [filterKey, initProjectCycleFilters, updateSearchQuery]);

  useEffect(() => {
    if (!workspaceSlug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceCycles(workspaceSlug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [fetchWorkspaceCycles, workspaceSlug]);

  const handleRemoveFilter = (key: keyof TCycleFilters, value: string | null) => {
    const currentValues = appliedFilters?.[key] ?? [];
    const nextValues = value ? currentValues.filter((currentValue) => currentValue !== value) : [];
    updateFilters(filterKey, { [key]: nextValues });
  };

  if (!workspaceSlug) return null;
  if (isLoading || isInitializingProjects || workspaceProjectIds === undefined) return <CycleModuleListLayoutLoader />;

  if (isFeatureDisabled)
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title={t("disabled_project.empty_state.cycle.title")}
          description={t("disabled_project.empty_state.cycle.description")}
          assetPath={resolvedEmptyState}
          primaryButton={{
            text: t("disabled_project.empty_state.cycle.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !hasAdminLevelPermission,
          }}
        />
      </div>
    );

  return (
    <>
      <PageHead title={t("sidebar.cycles")} />
      <div className="h-full w-full">
        {totalCycles === 0 ? (
          <div className="h-full place-items-center">
            {scopedProjectId ? (
              <EmptyStateDetailed
                assetKey="cycle"
                title={t("project_empty_state.cycles.title")}
                description={t("project_empty_state.cycles.description")}
                actions={[
                  {
                    label: t("project_empty_state.cycles.cta_primary"),
                    onClick: () => toggleCreateCycleModal(true),
                    variant: "primary",
                    disabled: !canCreateCycle,
                  },
                ]}
              />
            ) : (
              <EmptyStateDetailed
                assetKey="cycle"
                title={t("workspace_empty_state.sprints.title")}
                description={t("workspace_empty_state.sprints.description")}
              />
            )}
          </div>
        ) : (
          <>
            {calculateTotalFilters(appliedFilters ?? {}) !== 0 && (
              <Header variant={EHeaderVariant.TERNARY}>
                <CycleAppliedFiltersList
                  appliedFilters={appliedFilters ?? {}}
                  handleClearAllFilters={() => clearAllFilters(filterKey)}
                  handleRemoveFilter={handleRemoveFilter}
                  alwaysAllowEditing
                />
              </Header>
            )}
            <CyclesView
              workspaceSlug={workspaceSlug}
              projectId={scopedProjectId ?? undefined}
              projectIds={projectIds}
              filterKey={filterKey}
            />
          </>
        )}
      </div>
    </>
  );
}

export default observer(WorkspaceSprintsPage);
```

- [ ] **Step 2: Delete the legacy view and verify no imports remain**

Run from the repo root:

```bash
rm apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx
rg -n "WorkspaceSprintsListView|workspace-sprints-list-view" apps/web || true
```

Expected: no output from `rg`.

- [ ] **Step 3: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "cycles/page" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint 'app/(all)/[workspaceSlug]/(projects)/cycles/page.tsx'
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 4: Commit**

```bash
git add 'apps/web/app/(all)/[workspaceSlug]/(projects)/cycles/page.tsx'
git rm apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx
git commit -m "feat(web): use project cycles view for workspace cycles"
```

---

### Task 6: Verification, build, and smoke test

**Files:** none (verification only; fix and commit only if a check fails).

- [ ] **Step 1: Run the focused unit tests**

Run from `apps/web`:

```bash
pnpm exec vitest run core/lib/workspace-cycles.test.ts core/lib/workspace-create-project.test.ts
```

Expected: PASS (10 tests total).

- [ ] **Step 2: Run the full web test suite**

Run from `apps/web`:

```bash
pnpm exec vitest run
```

Expected: all tests pass.

- [ ] **Step 3: Run type checking**

Run from the repo root:

```bash
pnpm check:types
```

Expected: exit 0.

- [ ] **Step 4: Lint the touched files**

Run from `apps/web`:

```bash
pnpm exec oxlint core/lib/workspace-cycles.ts \
  core/lib/workspace-cycles.test.ts \
  core/lib/workspace-create-project.ts \
  core/lib/workspace-create-project.test.ts \
  core/hooks/use-workspace-cycle-permissions.ts \
  core/store/cycle.store.ts \
  core/store/cycle_filter.store.ts \
  core/components/cycles/cycles-view.tsx \
  core/components/cycles/cycles-view-header.tsx \
  core/components/cycles/list/root.tsx \
  core/components/cycles/list/cycles-list-map.tsx \
  core/components/cycles/list/cycles-list-item.tsx \
  core/components/workspace/application-scope/project-picker-modal.tsx \
  'app/(all)/[workspaceSlug]/(projects)/cycles/header.tsx' \
  'app/(all)/[workspaceSlug]/(projects)/cycles/create-modals.tsx' \
  'app/(all)/[workspaceSlug]/(projects)/cycles/layout.tsx' \
  'app/(all)/[workspaceSlug]/(projects)/cycles/page.tsx'
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 5: Build constants and web**

Run from the repo root:

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: both builds exit 0.

- [ ] **Step 6: Deploy the production web build**

Run:

```bash
systemctl --user restart plane-web-prod.service
sleep 20
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/itsm/cycles
```

Expected: `200` (the first request can take ~15–20 seconds after restart).

- [ ] **Step 7: Manual smoke test**

Verify in the browser:

1. `/itsm/cycles` with All: Current (if any), Upcoming, Completed groups; project badge on each row; no local project dropdown.
2. `/itsm/cycles?project=<id>`: active-cycle hero, Upcoming/Completed, scope chip, select persists after reload.
3. Sidebar scope change: the cycles page re-renders with the new scope without a full navigation.
4. Create with All: picker → choose application → cycle modal → create; new cycle appears in the correct project.
5. Create with a scoped application: modal opens directly with that project.
6. Search/status/date filters: filtered rows and applied-filter chips; clear-all works.
7. Scoped project with `cycle_view=false`: disabled-feature state; add button hidden.
8. Project `/itsm/projects/<id>/cycles`: unchanged (hero, filters, create, peek).
9. Mobile viewport: sidebar drawer selector and header chip remain usable.

- [ ] **Step 8: Commit any verification fixes**

If any check required a code change, stage only the changed files and commit:

```bash
git add <changed-files>
git commit -m "fix(web): address workspace cycles verification"
```

If no changes were needed, do not create a commit.

---

## Self-review notes

- Spec 7.4: reused `CyclesView` / `CyclesList` / `CyclePeekOverview`; scoped hero; All current group; project badges; `cycle.project_id` links/actions; workspace filter key; create picker.
- Spec 6.3: selectors filter by scope + `cycle_view`; workspace data stays in the existing cycle store.
- Spec 7.8: scoped empty state, All empty state, disabled-feature state, existing loaders.
- Spec 7.9: selector stays in sidebar; chip stays in the header.
- Spec 11: selector tests, create-target tests, project defaults preserved.
- No new i18n keys; existing `common.project`, `common.continue`, `project_empty_state.cycles.*`, `workspace_empty_state.sprints.*` are reused.

---

## Execution notes (2026-10-22)

- Status normalization validates against known cycle groups (including unknown/empty/prototype-key values) and falls back to the derived status; bucket selection is guarded.
- `fetchWorkspaceCycles` stores and returns normalized cycles, so row actions see a status even when the workspace endpoint omits it.
- `initProjectCycleFilters` was added to `ICycleFilterStore` so the workspace page can initialize the reserved filter key.
- The scoped active-cycle hero derives its cycle from unfiltered scoped cycles, so status/search filters do not hide the active cycle (matching project Cycles).
- Verification: 504 web tests pass, `pnpm check:types` passes, OxLint reports 0 on touched files, `@plane/constants` + web builds pass, `plane-web-prod` restarted and `/itsm/cycles` returns 200.
- Manual browser smoke test was not run in this environment; the HTTP/build/test checks above are the available evidence.
