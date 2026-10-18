# Workspace-level Sprints & Tracks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add read-only workspace-level Sprints and Tracks pages (aggregated across the user's member projects, grouped by state, with project filter and click-through into the project pages).

**Architecture:** Web-only. Reuse the existing workspace read endpoints (`GET /workspaces/:slug/cycles`, `/modules`) through the existing `fetchWorkspaceCycles` / `fetchWorkspaceModules` store actions. A shared "workspace aggregate list" shell (filter header + groups + states) is consumed by two thin list views (sprints, tracks). Pure grouping/filtering logic lives in a tested `core/lib` helper.

**Tech Stack:** React Router v7 (apps/web), MobX stores, `@plane/propel` icons, `@plane/ui` components, Vitest (`core/**/*.test.ts`).

**Spec:** `docs/superpowers/specs/2026-10-18-workspace-sprints-tracks-design.md`

---

## CRITICAL: Commit hygiene

The working tree already contains many unrelated pre-existing modifications
(repo-wide reformat, i18n locale diffs, etc.). **Never run `git add -A` or
`git add .`.** Stage only the exact paths listed in each task. Verify with
`git status --short <path>` before committing.

## File structure

New files:

- `apps/web/core/lib/workspace-aggregate.ts` — pure helpers: cycle status derivation, grouping (sprints/tracks), filters.
- `apps/web/core/lib/workspace-aggregate.test.ts` — Vitest unit tests for the helpers.
- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx` — shared shell (project filter + search + group rendering + loading/empty states).
- `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx` — sprints list view + row.
- `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx` — tracks list view + row.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/page.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/page.tsx`

Modified files:

- `apps/web/app/routes/core.ts` — two workspace routes.
- `packages/constants/src/workspace.ts` — sidebar nav items.
- `apps/web/core/components/workspace/sidebar/sidebar-item.tsx` — static item keys.
- `apps/web/core/components/workspace/sidebar/helper.tsx` — icons.
- `packages/i18n/src/locales/en/empty-state.json` — workspace empty states.

Out of scope (do NOT touch): api-rs/Django code, existing project pages,
service catalog, DB/docker.

---

### Task 1: Workspace aggregate helpers (TDD)

**Files:**

- Create: `apps/web/core/lib/workspace-aggregate.ts`
- Test: `apps/web/core/lib/workspace-aggregate.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/workspace-aggregate.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  applyWorkspaceFilters,
  deriveCycleStatus,
  groupWorkspaceCycles,
  groupWorkspaceModules,
} from "./workspace-aggregate";

// local-midnight ISO strings keep the tests timezone-independent
const isoDate = (year: number, monthIndex: number, day: number) => new Date(year, monthIndex, day).toISOString();
const NOW = new Date(2026, 9, 18); // Oct 18 2026, local time

describe("deriveCycleStatus", () => {
  it("returns draft when a date is missing or invalid", () => {
    expect(deriveCycleStatus({ start_date: null, end_date: null }, NOW)).toBe("draft");
    expect(deriveCycleStatus({ start_date: isoDate(2026, 9, 18), end_date: null }, NOW)).toBe("draft");
    expect(deriveCycleStatus({ start_date: "not-a-date", end_date: isoDate(2026, 9, 18) }, NOW)).toBe("draft");
  });

  it("returns draft for a reversed range", () => {
    expect(deriveCycleStatus({ start_date: isoDate(2026, 9, 20), end_date: isoDate(2026, 9, 18) }, NOW)).toBe("draft");
  });

  it("returns current when today is inside the range, inclusive", () => {
    expect(deriveCycleStatus({ start_date: isoDate(2026, 9, 18), end_date: isoDate(2026, 9, 20) }, NOW)).toBe(
      "current"
    );
    expect(deriveCycleStatus({ start_date: isoDate(2026, 9, 16), end_date: isoDate(2026, 9, 18) }, NOW)).toBe(
      "current"
    );
  });

  it("returns upcoming when the range starts after today", () => {
    expect(deriveCycleStatus({ start_date: isoDate(2026, 9, 19), end_date: isoDate(2026, 9, 20) }, NOW)).toBe(
      "upcoming"
    );
  });

  it("returns completed when the range ended before today", () => {
    expect(deriveCycleStatus({ start_date: isoDate(2026, 9, 16), end_date: isoDate(2026, 9, 17) }, NOW)).toBe(
      "completed"
    );
  });
});

describe("groupWorkspaceCycles", () => {
  const cycle = (id: string, start: string | null, end: string | null) => ({
    id,
    name: id,
    project_id: "p1",
    start_date: start,
    end_date: end,
  });

  it("returns non-empty groups in canonical order and sorts within groups", () => {
    const groups = groupWorkspaceCycles(
      [
        cycle("draft-2", null, null),
        cycle("draft-1", null, null),
        cycle("completed-old", isoDate(2026, 8, 1), isoDate(2026, 8, 10)),
        cycle("completed-new", isoDate(2026, 9, 1), isoDate(2026, 9, 10)),
        cycle("current-late", isoDate(2026, 9, 10), isoDate(2026, 9, 30)),
        cycle("current-early", isoDate(2026, 9, 10), isoDate(2026, 9, 20)),
        cycle("upcoming-later", isoDate(2026, 10, 1), isoDate(2026, 10, 10)),
        cycle("upcoming-sooner", isoDate(2026, 9, 25), isoDate(2026, 10, 1)),
      ],
      NOW
    );

    expect(groups.map((group) => group.key)).toEqual(["draft", "current", "upcoming", "completed"]);
    expect(groups[0].items.map((item) => item.id)).toEqual(["draft-2", "draft-1"]);
    expect(groups[1].items.map((item) => item.id)).toEqual(["current-early", "current-late"]);
    expect(groups[2].items.map((item) => item.id)).toEqual(["upcoming-sooner", "upcoming-later"]);
    expect(groups[3].items.map((item) => item.id)).toEqual(["completed-new", "completed-old"]);
  });

  it("skips empty groups", () => {
    const groups = groupWorkspaceCycles([cycle("draft-1", null, null)], NOW);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("draft");
  });
});

describe("groupWorkspaceModules", () => {
  const module = (id: string, status: "backlog" | "planned" | "completed" | undefined, target: string | null) => ({
    id,
    name: id,
    project_id: "p1",
    status,
    target_date: target,
  });

  it("groups in MODULE_STATUS order, defaults missing status to backlog, sorts by target date then name", () => {
    const groups = groupWorkspaceModules([
      module("planned-b", "planned", isoDate(2026, 9, 20)),
      module("planned-a", "planned", isoDate(2026, 9, 20)),
      module("planned-later", "planned", isoDate(2026, 10, 20)),
      module("planned-no-date", "planned", null),
      module("completed-1", "completed", null),
      module("backlog-1", undefined, null),
    ]);

    expect(groups.map((group) => group.key)).toEqual(["backlog", "planned", "completed"]);
    expect(groups[1].items.map((item) => item.id)).toEqual([
      "planned-a",
      "planned-b",
      "planned-later",
      "planned-no-date",
    ]);
  });
});

describe("applyWorkspaceFilters", () => {
  const items = [
    { id: "a", name: "Alpha Sprint", project_id: "p1" },
    { id: "b", name: "beta sprint", project_id: "p2" },
    { id: "c", name: "Gamma", project_id: "p3" },
  ];

  it("filters by project id", () => {
    expect(applyWorkspaceFilters(items, { projectId: "p2" }).map((item) => item.id)).toEqual(["b"]);
  });

  it("filters by case-insensitive name search", () => {
    expect(applyWorkspaceFilters(items, { search: "SPRINT" }).map((item) => item.id)).toEqual(["a", "b"]);
  });

  it("applies the projectAllowed predicate (membership + feature flag)", () => {
    expect(
      applyWorkspaceFilters(items, { projectAllowed: (projectId) => projectId !== "p3" }).map((item) => item.id)
    ).toEqual(["a", "b"]);
  });

  it("composes filters", () => {
    expect(
      applyWorkspaceFilters(items, { projectId: "p1", search: "alpha", projectAllowed: () => true }).map(
        (item) => item.id
      )
    ).toEqual(["a"]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/lib/workspace-aggregate`
Expected: FAIL — `Failed to resolve import "./workspace-aggregate"`.

- [ ] **Step 3: Write the implementation**

Create `apps/web/core/lib/workspace-aggregate.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { MODULE_STATUS } from "@plane/constants";
import type { TCycleGroups, TModuleStatus } from "@plane/types";
import { getDate } from "@plane/utils";

export type TWorkspaceCycle = {
  id: string;
  name: string;
  project_id: string;
  start_date: string | null;
  end_date: string | null;
};

export type TWorkspaceModule = {
  id: string;
  name: string;
  project_id: string;
  status?: TModuleStatus;
  target_date: string | null;
};

export type TWorkspaceAggregateGroup<T, K extends string> = {
  key: K;
  items: T[];
};

export type TWorkspaceFilterOptions = {
  projectId?: string;
  search?: string;
  projectAllowed?: (projectId: string) => boolean;
};

export const WORKSPACE_SPRINT_GROUP_ORDER: TCycleGroups[] = ["draft", "current", "upcoming", "completed"];

const toMidnight = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate());

const dateValue = (date: string | null | undefined) => {
  const parsed = getDate(date);
  const time = parsed?.getTime();
  return time !== undefined && !Number.isNaN(time) ? time : Number.POSITIVE_INFINITY;
};

/**
 * Derives the sprint (cycle) group from its dates. The workspace cycles
 * endpoint does not return `status`, so the client derives it the same way
 * the project UI's server-provided status behaves.
 */
export function deriveCycleStatus(
  cycle: Pick<TWorkspaceCycle, "start_date" | "end_date">,
  now: Date = new Date()
): TCycleGroups {
  const start = getDate(cycle.start_date);
  const end = getDate(cycle.end_date);
  if (!start || !end || Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) return "draft";
  const startDay = toMidnight(start);
  const endDay = toMidnight(end);
  if (startDay > endDay) return "draft";
  const today = toMidnight(now);
  if (endDay < today) return "completed";
  if (startDay > today) return "upcoming";
  return "current";
}

export function groupWorkspaceCycles<T extends TWorkspaceCycle>(
  cycles: T[],
  now: Date = new Date()
): TWorkspaceAggregateGroup<T, TCycleGroups>[] {
  const buckets: Record<TCycleGroups, T[]> = { draft: [], current: [], upcoming: [], completed: [] };
  cycles.forEach((cycle) => buckets[deriveCycleStatus(cycle, now)].push(cycle));
  const groups: TWorkspaceAggregateGroup<T, TCycleGroups>[] = [];
  for (const key of WORKSPACE_SPRINT_GROUP_ORDER) {
    const items = [...buckets[key]];
    if (key === "current") items.sort((a, b) => dateValue(a.end_date) - dateValue(b.end_date));
    if (key === "upcoming") items.sort((a, b) => dateValue(a.start_date) - dateValue(b.start_date));
    if (key === "completed") items.sort((a, b) => dateValue(b.end_date) - dateValue(a.end_date));
    if (items.length > 0) groups.push({ key, items });
  }
  return groups;
}

export function groupWorkspaceModules<T extends TWorkspaceModule>(
  modules: T[]
): TWorkspaceAggregateGroup<T, TModuleStatus>[] {
  const groups: TWorkspaceAggregateGroup<T, TModuleStatus>[] = [];
  for (const status of MODULE_STATUS) {
    const items = modules.filter((module) => (module.status ?? "backlog") === status.value);
    items.sort((a, b) => {
      const targetDiff = dateValue(a.target_date) - dateValue(b.target_date);
      if (targetDiff !== 0) return targetDiff;
      return a.name.localeCompare(b.name);
    });
    if (items.length > 0) groups.push({ key: status.value, items });
  }
  return groups;
}

export function applyWorkspaceFilters<T extends { name: string; project_id: string }>(
  items: T[],
  { projectId, search, projectAllowed }: TWorkspaceFilterOptions
): T[] {
  const normalizedSearch = search?.trim().toLowerCase() ?? "";
  return items.filter((item) => {
    if (projectAllowed && !projectAllowed(item.project_id)) return false;
    if (projectId && item.project_id !== projectId) return false;
    if (normalizedSearch && !item.name.toLowerCase().includes(normalizedSearch)) return false;
    return true;
  });
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/lib/workspace-aggregate`
Expected: PASS — 1 test file, all tests green.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/workspace-aggregate.ts apps/web/core/lib/workspace-aggregate.test.ts
git commit -m "feat(web): workspace aggregate filters and grouping helpers"
```

---

### Task 2: Shared workspace aggregate list shell

**Files:**

- Create: `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`

- [ ] **Step 1: Create the shell component**

Create `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CustomSelect } from "@plane/ui";
// hooks
import { useProject } from "@/hooks/store/use-project";

export type TWorkspaceAggregateGroupItem = {
  key: string;
  label: string;
  count: number;
  items: ReactNode;
};

type Props = {
  projectFilter?: string;
  onProjectFilterChange: (projectId: string) => void;
  search: string;
  onSearchChange: (value: string) => void;
  searchPlaceholder: string;
  groups: TWorkspaceAggregateGroupItem[];
  totalCount: number;
  isLoading?: boolean;
  emptyTitle: string;
  emptyDescription: string;
};

export const WorkspaceAggregateList = observer(function WorkspaceAggregateList({
  projectFilter,
  onProjectFilterChange,
  search,
  onSearchChange,
  searchPlaceholder,
  groups,
  totalCount,
  isLoading = false,
  emptyTitle,
  emptyDescription,
}: Props) {
  const { t } = useTranslation();
  const { workspaceProjectIds, getProjectById } = useProject();

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center justify-between gap-2 border-b border-subtle px-3 py-2">
        <div className="flex items-center gap-2">
          <CustomSelect
            value={projectFilter ?? ""}
            onChange={onProjectFilterChange}
            label={
              <span className="flex items-center gap-2 py-0.5 text-12">
                {projectFilter ? getProjectById(projectFilter)?.name : t("service.catalog_all_projects")}
              </span>
            }
            noChevron
          >
            <CustomSelect.Option value="">{t("service.catalog_all_projects")}</CustomSelect.Option>
            {(workspaceProjectIds ?? []).map((id) => {
              const project = getProjectById(id);
              return (
                <CustomSelect.Option key={id} value={id}>
                  <span className="truncate">{project?.name}</span>
                </CustomSelect.Option>
              );
            })}
          </CustomSelect>
          <input
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={searchPlaceholder}
            className="h-7 rounded-sm border border-subtle bg-layer-1 px-2 text-12 text-primary outline-none"
          />
          <span className="text-12 text-tertiary">{totalCount}</span>
        </div>
      </div>
      {isLoading ? (
        <div className="p-6 text-sm text-secondary">{t("common.loading")}</div>
      ) : totalCount === 0 ? (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{emptyTitle}</p>
          <p className="text-xs text-secondary">{emptyDescription}</p>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {groups.map((group) => (
            <div key={group.key}>
              <div className="flex items-center gap-2 border-b border-subtle bg-layer-1 px-4 py-2">
                <span className="text-12 font-medium text-secondary">{group.label}</span>
                <span className="text-11 text-tertiary">{group.count}</span>
              </div>
              {group.items}
            </div>
          ))}
        </div>
      )}
    </div>
  );
});
```

- [ ] **Step 2: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0 (takes 1-2 minutes; `react-router typegen` runs first).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx
git commit -m "feat(web): workspace aggregate list shell"
```

---

### Task 3: Sprints page (route, layout, page, list view)

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/page.tsx`
- Create: `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx`
- Modify: `apps/web/app/routes/core.ts` (insert after the service catalog block, currently lines 123-130)
- Modify: `packages/i18n/src/locales/en/empty-state.json` (add `workspace_empty_state.sprints`)

- [ ] **Step 1: Create the list view**

Create `apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CycleGroupIcon } from "@makeplane/propel/icons";
import type { ICycle, TCycleGroups } from "@plane/types";
import { calculateCycleProgress, renderFormattedDate } from "@plane/utils";
// components
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import { applyWorkspaceFilters, deriveCycleStatus, groupWorkspaceCycles } from "@/lib/workspace-aggregate";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";

type TSprintRowProps = {
  cycle: ICycle;
  workspaceSlug: string;
};

const SprintRow = observer(function SprintRow({ cycle, workspaceSlug }: TSprintRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const project = getProjectById(cycle.project_id);
  const status = (cycle.status?.toLowerCase() as TCycleGroups | undefined) ?? deriveCycleStatus(cycle);
  const progress = calculateCycleProgress(cycle);
  const dateRange =
    cycle.start_date && cycle.end_date
      ? `${renderFormattedDate(cycle.start_date)} - ${renderFormattedDate(cycle.end_date)}`
      : null;

  return (
    <ListItem
      title={cycle.name}
      itemLink={`/${workspaceSlug}/projects/${cycle.project_id}/cycles/${cycle.id}`}
      prependTitleElement={<CycleGroupIcon cycleGroup={status} className="h-4 w-4 flex-shrink-0" />}
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          {dateRange && <span className="hidden md:inline">{dateRange}</span>}
          <span>{`${progress}%`}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  projectFilter?: string;
};

export const WorkspaceSprintsListView = observer(function WorkspaceSprintsListView({ projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { cycleMap, fetchWorkspaceCycles } = useCycle();
  const { workspaceProjectIds, getProjectById } = useProject();
  const [search, setSearch] = useState("");
  const [isLoading, setIsLoading] = useState(true);

  const slug = workspaceSlug?.toString();

  useEffect(() => {
    if (!slug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceCycles(slug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slug, fetchWorkspaceCycles]);

  const filteredCycles = useMemo(() => {
    const projectIds = new Set(workspaceProjectIds ?? []);
    const cycles = Object.values(cycleMap ?? {}).filter((cycle) => !cycle.archived_at);
    const projectAllowed = (projectId: string) => {
      if (!projectIds.has(projectId)) return false;
      const project = getProjectById(projectId);
      return project ? project.cycle_view !== false : true;
    };
    return applyWorkspaceFilters(cycles, { projectId: projectFilter, search, projectAllowed });
  }, [cycleMap, workspaceProjectIds, getProjectById, projectFilter, search]);

  const groups = useMemo(() => groupWorkspaceCycles(filteredCycles), [filteredCycles]);

  const groupLabels: Record<TCycleGroups, string> = {
    current: t("project_cycles.active_cycle.label"),
    upcoming: t("project_cycles.upcoming_cycle.label"),
    completed: t("project_cycles.completed_cycle.label"),
    draft: t("project_cycles.status.draft"),
  };

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: groupLabels[group.key],
    count: group.items.length,
    items: (
      <>
        {group.items.map((cycle) => (
          <SprintRow key={cycle.id} cycle={cycle} workspaceSlug={slug ?? ""} />
        ))}
      </>
    ),
  }));

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    router.push(projectId ? `/${slug}/sprints?project=${projectId}` : `/${slug}/sprints`);
  };

  return (
    <WorkspaceAggregateList
      projectFilter={projectFilter}
      onProjectFilterChange={handleProjectFilterChange}
      search={search}
      onSearchChange={setSearch}
      searchPlaceholder={t("service.catalog_search_placeholder")}
      groups={renderedGroups}
      totalCount={filteredCycles.length}
      isLoading={isLoading}
      emptyTitle={t("workspace_empty_state.sprints.title")}
      emptyDescription={t("workspace_empty_state.sprints.description")}
    />
  );
});
```

- [ ] **Step 2: Create the layout**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { CyclesOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceSprintsBreadcrumbs = observer(function WorkspaceSprintsBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.cycles")}
                href={`/${workspaceSlug}/sprints/`}
                icon={<CyclesOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceSprintsLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceSprintsBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

- [ ] **Step 3: Create the page**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { WorkspaceSprintsListView } from "@/components/cycles/workspace/workspace-sprints-list-view";
import { PageHead } from "@/components/core/page-title";

function WorkspaceSprintsPage() {
  const searchParams = useSearchParams();
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.cycles")} />
      <WorkspaceSprintsListView projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceSprintsPage);
```

- [ ] **Step 4: Add the route**

In `apps/web/app/routes/core.ts`, right after the "Workspace Service Catalog" block (the `layout("./(all)/[workspaceSlug]/(projects)/services/layout.tsx", [...])` block), insert:

```ts
        // Workspace Sprints
        layout("./(all)/[workspaceSlug]/(projects)/sprints/layout.tsx", [
          route(":workspaceSlug/sprints", "./(all)/[workspaceSlug]/(projects)/sprints/page.tsx"),
        ]),

        // Workspace Tracks (route added in Task 4)
```

(The tracks route is added in Task 4; remove the trailing comment when adding it.)

- [ ] **Step 5: Add the empty-state keys**

In `packages/i18n/src/locales/en/empty-state.json`, add a new top-level namespace (place it after `common_empty_state`):

```json
  "workspace_empty_state": {
    "sprints": {
      "title": "No sprints yet",
      "description": "Sprints from projects you are a member of will show up here."
    }
  },
```

The `tracks` keys are added in Task 4. Note: `pnpm --filter=@plane/i18n check:sync` already fails on the repo baseline (pre-existing missing keys in other locales) — do not attempt to fix the baseline; en-only keys are intentional.

- [ ] **Step 6: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 7: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/layout.tsx" \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/sprints/page.tsx" \
  apps/web/core/components/cycles/workspace/workspace-sprints-list-view.tsx \
  apps/web/app/routes/core.ts packages/i18n/src/locales/en/empty-state.json
git commit -m "feat(web): workspace sprints page"
```

---

### Task 4: Tracks page (route, layout, page, list view)

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/page.tsx`
- Create: `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx`
- Modify: `apps/web/app/routes/core.ts` (add tracks route next to the sprints route from Task 3)
- Modify: `packages/i18n/src/locales/en/empty-state.json` (add `workspace_empty_state.tracks`)

- [ ] **Step 1: Create the list view**

Create `apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { MODULE_STATUS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { ModuleStatusIcon } from "@makeplane/propel/icons";
import type { IModule } from "@plane/types";
import { renderFormattedDate } from "@plane/utils";
// components
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import { applyWorkspaceFilters, groupWorkspaceModules } from "@/lib/workspace-aggregate";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useModule } from "@/hooks/store/use-module";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";

type TTrackRowProps = {
  module: IModule;
  workspaceSlug: string;
};

const TrackRow = observer(function TrackRow({ module, workspaceSlug }: TTrackRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getUserDetails } = useMember();
  const project = getProjectById(module.project_id);
  const lead = module.lead_id ? getUserDetails(module.lead_id) : undefined;
  const progress = module.total_issues > 0 ? Math.round((module.completed_issues / module.total_issues) * 100) : 0;
  const estimatePoints =
    (module.total_estimate_points ?? 0) > 0
      ? `${module.completed_estimate_points ?? 0}/${module.total_estimate_points ?? 0} pts`
      : null;

  return (
    <ListItem
      title={module.name}
      itemLink={`/${workspaceSlug}/projects/${module.project_id}/modules/${module.id}`}
      prependTitleElement={<ModuleStatusIcon status={module.status ?? "backlog"} />}
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          {lead?.display_name && <span className="hidden lg:inline">{lead.display_name}</span>}
          {module.target_date && <span className="hidden md:inline">{renderFormattedDate(module.target_date)}</span>}
          {estimatePoints && <span className="hidden lg:inline">{estimatePoints}</span>}
          <span>{`${progress}%`}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  projectFilter?: string;
};

export const WorkspaceTracksListView = observer(function WorkspaceTracksListView({ projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { moduleMap, fetchWorkspaceModules } = useModule();
  const { workspaceProjectIds, getProjectById } = useProject();
  const [search, setSearch] = useState("");
  const [isLoading, setIsLoading] = useState(true);

  const slug = workspaceSlug?.toString();

  useEffect(() => {
    if (!slug) return;
    let cancelled = false;
    setIsLoading(true);
    fetchWorkspaceModules(slug)
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slug, fetchWorkspaceModules]);

  const filteredModules = useMemo(() => {
    const projectIds = new Set(workspaceProjectIds ?? []);
    const modules = Object.values(moduleMap ?? {}).filter((module) => !module.archived_at);
    const projectAllowed = (projectId: string) => {
      if (!projectIds.has(projectId)) return false;
      const project = getProjectById(projectId);
      return project ? project.module_view !== false : true;
    };
    return applyWorkspaceFilters(modules, { projectId: projectFilter, search, projectAllowed });
  }, [moduleMap, workspaceProjectIds, getProjectById, projectFilter, search]);

  const groups = useMemo(() => groupWorkspaceModules(filteredModules), [filteredModules]);

  const statusLabels = new Map(MODULE_STATUS.map((status) => [status.value, t(status.i18n_label)]));

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: statusLabels.get(group.key) ?? group.key,
    count: group.items.length,
    items: (
      <>
        {group.items.map((module) => (
          <TrackRow key={module.id} module={module} workspaceSlug={slug ?? ""} />
        ))}
      </>
    ),
  }));

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    router.push(projectId ? `/${slug}/tracks?project=${projectId}` : `/${slug}/tracks`);
  };

  return (
    <WorkspaceAggregateList
      projectFilter={projectFilter}
      onProjectFilterChange={handleProjectFilterChange}
      search={search}
      onSearchChange={setSearch}
      searchPlaceholder={t("service.catalog_search_placeholder")}
      groups={renderedGroups}
      totalCount={filteredModules.length}
      isLoading={isLoading}
      emptyTitle={t("workspace_empty_state.tracks.title")}
      emptyDescription={t("workspace_empty_state.tracks.description")}
    />
  );
});
```

- [ ] **Step 2: Create the layout**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { ModuleOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceTracksBreadcrumbs = observer(function WorkspaceTracksBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.modules")}
                href={`/${workspaceSlug}/tracks/`}
                icon={<ModuleOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceTracksLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceTracksBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

- [ ] **Step 3: Create the page**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceTracksListView } from "@/components/modules/workspace/workspace-tracks-list-view";

function WorkspaceTracksPage() {
  const searchParams = useSearchParams();
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.modules")} />
      <WorkspaceTracksListView projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceTracksPage);
```

- [ ] **Step 4: Add the route**

In `apps/web/app/routes/core.ts`, replace the placeholder comment from Task 3 with the tracks route so the block reads:

```ts
        // Workspace Sprints
        layout("./(all)/[workspaceSlug]/(projects)/sprints/layout.tsx", [
          route(":workspaceSlug/sprints", "./(all)/[workspaceSlug]/(projects)/sprints/page.tsx"),
        ]),

        // Workspace Tracks
        layout("./(all)/[workspaceSlug]/(projects)/tracks/layout.tsx", [
          route(":workspaceSlug/tracks", "./(all)/[workspaceSlug]/(projects)/tracks/page.tsx"),
        ]),
```

- [ ] **Step 5: Add the empty-state keys**

In `packages/i18n/src/locales/en/empty-state.json`, extend `workspace_empty_state`:

```json
  "workspace_empty_state": {
    "sprints": {
      "title": "No sprints yet",
      "description": "Sprints from projects you are a member of will show up here."
    },
    "tracks": {
      "title": "No tracks yet",
      "description": "Tracks from projects you are a member of will show up here."
    }
  },
```

- [ ] **Step 6: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 7: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/layout.tsx" \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/tracks/page.tsx" \
  apps/web/core/components/modules/workspace/workspace-tracks-list-view.tsx \
  apps/web/app/routes/core.ts packages/i18n/src/locales/en/empty-state.json
git commit -m "feat(web): workspace tracks page"
```

---

### Task 5: Sidebar entries

**Files:**

- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`

- [ ] **Step 1: Add the nav items**

In `packages/constants/src/workspace.ts`, inside `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`, add after the `home` entry:

```ts
  sprints: {
    key: "sprints",
    labelTranslationKey: "sidebar.cycles",
    href: `/sprints/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
  tracks: {
    key: "tracks",
    labelTranslationKey: "sidebar.modules",
    href: `/tracks/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

And update the static links list to:

```ts
export const WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS: IWorkspaceSidebarNavigationItem[] = [
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["home"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["sprints"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["tracks"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["services"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["ai_scheduler"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["docs"],
];
```

- [ ] **Step 2: Register the item keys**

In `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`, update `staticItems`:

```ts
const staticItems = [
  "home",
  "pi_chat",
  "projects",
  "sprints",
  "tracks",
  "services",
  "your_work",
  "stickies",
  "drafts",
  "ai_scheduler",
  "docs",
  ...(additionalStaticItems || []),
];
```

- [ ] **Step 3: Add the icons**

In `apps/web/core/components/workspace/sidebar/helper.tsx`, add `CyclesOutline` and `ModuleOutline` to the `@makeplane/propel/icons` import, and add cases to the switch:

```tsx
    case "sprints":
      return <CyclesOutline className={cn("size-4 flex-shrink-0", className)} />;
    case "tracks":
      return <ModuleOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 4: Rebuild the constants package**

Web consumes the built `@plane/constants` dist (this bit the previous workspace-services work).

Run: `pnpm --filter=@plane/constants build`
Expected: build succeeds; `dist` updated.

- [ ] **Step 5: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 6: Commit**

```bash
git add packages/constants/src/workspace.ts \
  apps/web/core/components/workspace/sidebar/sidebar-item.tsx \
  apps/web/core/components/workspace/sidebar/helper.tsx
git commit -m "feat(web): workspace sprints and tracks sidebar entries"
```

---

### Task 6: Full verification and deploy

- [ ] **Step 1: Run the full web test suite**

Run: `pnpm --filter=web test`
Expected: all test files pass (370+ tests plus the new helper tests).

- [ ] **Step 2: Lint**

Run: `pnpm --filter=web check:lint`
Expected: exit 0. If it fails, fix issues only in the new files.

- [ ] **Step 3: Build constants and web**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: both succeed. Reminder from AGENTS.md: web prod is built with
`VITE_API_BASE_URL=https://api.terraline.space` — do not change env; just build.

- [ ] **Step 4: Restart the prod web service and smoke test**

```bash
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}" http://localhost:3000
```

Expected: `200`.

Manual smoke on the tunnel (browser):

1. Workspace sidebar shows **Sprints** and **Tracks** between Home and Services.
2. `/[workspaceSlug]/sprints` lists sprints from member projects, grouped Draft/Current/Upcoming/Completed; rows show project badge + date range + progress.
3. `/[workspaceSlug]/tracks` lists tracks grouped by status (backlog, planned, …); rows show project badge, lead, target date, progress, estimate points.
4. `?project=<id>` filters both pages and is shareable; selecting "All projects" clears it.
5. Clicking a row navigates to the project's sprint/track detail page.
6. A workspace with no sprints/tracks shows the empty state.

- [ ] **Step 5: Report**

Report the smoke results. No commit for this task.

---

## Self-review notes

- Spec coverage: routes/sidebar (Tasks 3-5), grouped lists + rows (Tasks 3-4), filters/search/feature-flag exclusion (Tasks 1, 3, 4), loading/empty states (Tasks 2-4), i18n en keys (Tasks 3-4), testing (Task 1), verification (Task 6). No backend tasks — spec is web-only.
- Type consistency: `groupWorkspaceCycles`/`groupWorkspaceModules` return `{ key, items }`; views map them to `TWorkspaceAggregateGroupItem` (`{ key, label, count, items }`). `applyWorkspaceFilters` takes `projectAllowed`, used the same way in both views.
- The `?project=` search-param pattern mirrors `(projects)/services/page.tsx`; `service.catalog_all_projects` and `service.catalog_search_placeholder` keys already exist from the services work.
- `t()` resolution: shell expects i18n keys resolved by callers (`PhaseHead`, empty states, labels) — no raw key strings inside the shell except `service.*` catalog keys, which exist.
