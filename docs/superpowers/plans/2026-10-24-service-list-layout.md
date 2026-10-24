# Service Catalog Board → List Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the service catalog `board` layout with a grouped `list` layout modeled on work-items list conventions.

**Architecture:** Rename the layout key (`board` → `list`) with a persisted-value migration in `ServiceFilterStore`, add a pure `groupServices` helper tested by Vitest, build new `ServicesList`/`ServicesListRow` components reusing the existing health/incident/deploy cells, and add a group-by control next to the existing order-by dropdown.

**Tech Stack:** React Router, MobX, Vitest, i18next, Tailwind tokens.

---

### Task 1: Layout type, store migration, and layout toggle

**Files:**

- Modify: `packages/types/src/service/filters.ts:9-25`
- Modify: `apps/web/core/store/service_filter.store.ts:114-126`
- Modify: `apps/web/core/components/services/service-layout-icon.tsx`
- Test: none (store change covered by typecheck; no existing store test file)

- [ ] **Step 1: Update the layout and display-filter types**

Replace in `packages/types/src/service/filters.ts`:

```ts
export type TServiceLayoutOptions = "list" | "graph";

export type TServiceGroupByOptions = "status" | "criticality" | "health" | null;

export type TServiceDisplayFilters = {
  layout: TServiceLayoutOptions;
  order_by: TServiceOrderByOptions;
  group_by: TServiceGroupByOptions;
};
```

- [ ] **Step 2: Migrate persisted layouts in the store**

Replace in `apps/web/core/store/service_filter.store.ts`:

```ts
initProjectServiceFilters = (projectId: string) => {
  const displayFilters = this.getDisplayFiltersByProjectId(projectId);
  // Legacy persisted layouts ("board" | "grid") collapse into the list.
  const persistedLayout = displayFilters?.layout as string | undefined;
  const layout: TServiceLayoutOptions = persistedLayout === "graph" ? "graph" : "list";
  const persistedGroupBy = (displayFilters as { group_by?: unknown } | undefined)?.group_by;
  const groupBy: TServiceGroupByOptions =
    persistedGroupBy === "status" || persistedGroupBy === "criticality" || persistedGroupBy === "health"
      ? persistedGroupBy
      : "status";
  runInAction(() => {
    this.displayFilters[projectId] = {
      layout,
      order_by: displayFilters?.order_by || "health",
      group_by: groupBy,
    };
    this.filters[projectId] = this.filters[projectId] ?? {};
  });
  this.saveDisplayFiltersToLocalStorage();
  this.saveFiltersToLocalStorage();
};
```

Add the `TServiceGroupByOptions` import to the store's type imports from `@plane/types`.

- [ ] **Step 3: Update the layout toggle**

In `apps/web/core/components/services/service-layout-icon.tsx`, replace the layouts array and icon map:

```tsx
import { ListOutline, WorkgraphOutline } from "@makeplane/propel/icons";

export const SERVICE_VIEW_LAYOUTS: { key: TServiceLayoutOptions; i18n_label: string }[] = [
  { key: "list", i18n_label: "service.layout.list" },
  { key: "graph", i18n_label: "service.layout.graph" },
];
```

and the icon map:

```tsx
const icons = {
  list: ListOutline,
  graph: WorkgraphOutline,
};
const Icon = icons[layoutType ?? "list"];
```

`ListOutline` is confirmed exported from `@makeplane/propel/icons`.

- [ ] **Step 4: Typecheck**

Run:

```bash
pnpm turbo run check:types --filter=web --filter=@plane/types
```

Expected: FAIL — `services-list-view.tsx` and board files still reference `"board"`. This is intentional; Task 4 fixes the consumers.

- [ ] **Step 5: Commit**

```bash
git add packages/types/src/service/filters.ts \
  apps/web/core/store/service_filter.store.ts \
  apps/web/core/components/services/service-layout-icon.tsx
git commit -m "feat(services): rename board layout key to list with migration"
```

---

### Task 2: Grouping helper and tests

**Files:**

- Modify: `apps/web/core/services/service.helpers.ts`
- Test: `apps/web/core/services/service.helpers.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `apps/web/core/services/service.helpers.test.ts`:

```ts
describe("groupServices", () => {
  const makeService = (id: string, overrides: Record<string, unknown> = {}) =>
    ({
      id,
      name: id,
      status: "active",
      criticality: "medium",
      type: "internal",
      ...overrides,
    }) as never;

  it("groups by status in lifecycle order", () => {
    const services = [
      makeService("retired", { status: "retired" }),
      makeService("active", { status: "active" }),
      makeService("planned", { status: "planned" }),
    ];

    const groups = groupServices(services, "status", {});
    expect(groups.map((group) => group.key)).toEqual(["active", "planned", "retired"]);
    expect(groups[0]?.services.map((service) => service.id)).toEqual(["active"]);
  });

  it("groups by health with missing snapshots as unknown", () => {
    const services = [makeService("a"), makeService("b")];
    const healthMap = {
      a: { service_id: "a", health: "degraded", incidents: [], last_deployed_at: null },
    } as never;

    const groups = groupServices(services, "health", healthMap);
    expect(groups.map((group) => group.key)).toEqual(["degraded", "unknown"]);
  });

  it("returns a single flat group when group by is null", () => {
    const services = [makeService("a"), makeService("b")];

    const groups = groupServices(services, null, {});
    expect(groups).toHaveLength(1);
    expect(groups[0]?.services).toHaveLength(2);
  });
});
```

Add `groupServices` to the existing import from `./service.helpers` in that file.

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
pnpm --filter=web test -- core/services/service.helpers.test.ts
```

Expected: FAIL with `groupServices is not defined` (import error).

- [ ] **Step 3: Implement the grouping helper**

Append to `apps/web/core/services/service.helpers.ts` (after `orderServices`):

```ts
export type TServiceGroup = {
  key: string;
  services: IService[];
};

const STATUS_GROUP_ORDER = ["active", "maintenance", "planned", "deprecated", "retired"];

const CRITICALITY_GROUP_ORDER = ["critical", "high", "medium", "low"];

const HEALTH_GROUP_ORDER = ["down", "degraded", "healthy", "unknown"];

/**
 * Groups already-filtered services for the list layout. Returns groups in
 * dimension order; services keep their input order (callers pass ordered ids).
 */
export const groupServices = (
  services: IService[],
  groupBy: TServiceGroupByOptions,
  healthMap: Record<string, IServiceHealthSnapshot> = {}
): TServiceGroup[] => {
  if (!groupBy) return [{ key: "all", services }];
  const order =
    groupBy === "status"
      ? STATUS_GROUP_ORDER
      : groupBy === "criticality"
        ? CRITICALITY_GROUP_ORDER
        : HEALTH_GROUP_ORDER;
  const buckets = new Map<string, IService[]>();
  for (const service of services) {
    const key =
      groupBy === "status"
        ? service.status
        : groupBy === "criticality"
          ? service.criticality
          : (healthMap[service.id]?.health ?? "unknown");
    const bucket = buckets.get(key) ?? [];
    bucket.push(service);
    buckets.set(key, bucket);
  }
  return [...buckets.entries()]
    .sort(([a], [b]) => order.indexOf(a) - order.indexOf(b))
    .map(([key, bucket]) => ({ key, services: bucket }));
};
```

Add `TServiceGroupByOptions` to the type imports from `@plane/types` in that file. Note: `order.indexOf` returns `-1` for unexpected keys, sorting them first; service statuses/criticalities are closed unions so this only triggers on corrupt data — acceptable and documented here.

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
pnpm --filter=web test -- core/services/service.helpers.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/services/service.helpers.ts apps/web/core/services/service.helpers.test.ts
git commit -m "feat(services): add groupServices helper for list layout"
```

---

### Task 3: Status and Criticality pills

**Files:**

- Create: `apps/web/core/components/services/health/service-status-pill.tsx`
- Create: `apps/web/core/components/services/health/service-criticality-pill.tsx`
- Modify: `apps/web/core/components/services/index.ts`

- [ ] **Step 1: Create the status pill**

Create `apps/web/core/components/services/health/service-status-pill.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceStatus } from "@plane/types";
import { cn } from "@plane/utils";

const STATUS_PILL_STYLE: Record<TServiceStatus, string> = {
  active: "bg-success-subtle text-success-primary",
  maintenance: "bg-warning-subtle text-warning-primary",
  planned: "bg-layer-2 text-tertiary",
  deprecated: "bg-layer-2 text-secondary",
  retired: "bg-layer-2 text-tertiary",
};

type Props = {
  status: TServiceStatus;
  className?: string;
};

export function ServiceStatusPill({ status, className }: Props) {
  const { t } = useTranslation();
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-11 font-medium capitalize",
        STATUS_PILL_STYLE[status],
        className
      )}
    >
      {t(`service.status_values.${status}`)}
    </span>
  );
}
```

- [ ] **Step 2: Create the criticality pill**

Create `apps/web/core/components/services/health/service-criticality-pill.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceCriticality } from "@plane/types";
import { cn } from "@plane/utils";

const CRITICALITY_PILL_STYLE: Record<TServiceCriticality, string> = {
  critical: "bg-danger-subtle text-danger-primary",
  high: "bg-warning-subtle text-warning-primary",
  medium: "bg-layer-2 text-secondary",
  low: "bg-layer-2 text-tertiary",
};

type Props = {
  criticality: TServiceCriticality;
  className?: string;
};

export function ServiceCriticalityPill({ criticality, className }: Props) {
  const { t } = useTranslation();
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-11 font-medium capitalize",
        CRITICALITY_PILL_STYLE[criticality],
        className
      )}
    >
      {t(`service.criticality_values.${criticality}`)}
    </span>
  );
}
```

- [ ] **Step 3: Export from the barrel**

In `apps/web/core/components/services/index.ts`, add after the health exports:

```ts
export * from "./health/service-status-pill";
export * from "./health/service-criticality-pill";
```

- [ ] **Step 4: Typecheck and commit**

Run:

```bash
pnpm turbo run check:types check:lint --filter=web
```

Expected: PASS (Task 1's expected failures persist until Task 4; typecheck of these new files passes).

```bash
git add apps/web/core/components/services/health/service-status-pill.tsx \
  apps/web/core/components/services/health/service-criticality-pill.tsx \
  apps/web/core/components/services/index.ts
git commit -m "feat(services): add status and criticality pills"
```

---

### Task 4: List components, group-by control, and board removal

**Files:**

- Create: `apps/web/core/components/services/list/services-list.tsx`
- Create: `apps/web/core/components/services/list/services-list-row.tsx`
- Create: `apps/web/core/components/services/dropdowns/group-by.tsx`
- Modify: `apps/web/core/components/services/services-list-view.tsx`
- Modify: `apps/web/core/components/services/service-view-header.tsx`
- Modify: `apps/web/core/components/services/index.ts`
- Delete: `apps/web/core/components/services/board/services-board.tsx`
- Delete: `apps/web/core/components/services/board/services-board-row.tsx`

- [ ] **Step 1: Create the list row**

Create `apps/web/core/components/services/list/services-list-row.tsx`, mirroring `board/services-board-row.tsx` with two new columns. Full content:

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
import { ControlLink } from "@plane/ui";
import { cn } from "@plane/utils";
// components
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { usePlatformOS } from "@/hooks/use-platform-os";
import useServicePeekOverviewRedirection from "@/hooks/use-service-peek-overview-redirection";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceCriticalityPill } from "../health/service-criticality-pill";
import { ServiceDeployCell } from "../health/service-deploy-cell";
import { ServiceHealthDot } from "../health/service-health-dot";
import { ServiceHealthPill } from "../health/service-health-pill";
import { ServiceIncidentCell } from "../health/service-incident-cell";
import { ServiceStatusPill } from "../health/service-status-pill";

type Props = {
  serviceId: string;
};

export const ServicesListRow = observer(function ServicesListRow(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getServiceHealth } = useService();
  const { getProjectById } = useProject();
  // peek overview
  const { isMobile } = usePlatformOS();
  const { handleRedirection } = useServicePeekOverviewRedirection();
  // derived values
  const service = getServiceById(serviceId);
  const health = getServiceHealth(serviceId);

  if (!service) return null;

  const state = health?.health ?? DEFAULT_HEALTH;
  const serviceLink = `/${workspaceSlug?.toString()}/services/${service.id}`;
  const ownerProject = service.owner_project_id ? getProjectById(service.owner_project_id) : undefined;

  return (
    <ControlLink
      id={`service-${service.id}`}
      href={serviceLink}
      onClick={() => handleRedirection(workspaceSlug?.toString(), service, isMobile)}
      className="relative flex items-center gap-3 border-b border-subtle px-3 py-1.5 transition-colors hover:bg-layer-transparent-hover"
    >
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", HEALTH_CONFIG[state].rail)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <ServiceHealthDot health={state} />
        <span className="truncate text-12 font-medium text-primary">{service.name}</span>
        {ownerProject && (
          <span
            className="shrink-0 rounded-xs border border-subtle px-1 text-9 font-medium tracking-wide text-tertiary"
            title={ownerProject.name}
          >
            {ownerProject.identifier}
          </span>
        )}
      </div>
      <div className="hidden w-[100px] shrink-0 sm:block">
        <ServiceStatusPill status={service.status} />
      </div>
      <div className="hidden w-[90px] shrink-0 md:block">
        <ServiceCriticalityPill criticality={service.criticality} />
      </div>
      <div className="hidden w-[90px] shrink-0 md:block">
        <ServiceHealthPill health={state} />
      </div>
      <div className="hidden w-[100px] shrink-0 lg:block">
        <ServiceIncidentCell incidents={health?.incidents ?? []} />
      </div>
      <div className="hidden w-[90px] shrink-0 xl:block">
        <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
      </div>
      <div className="hidden w-[40px] shrink-0 items-center justify-end xl:flex">
        {service.owner_id && <ButtonAvatars showTooltip userIds={service.owner_id} />}
      </div>
    </ControlLink>
  );
});
```

Note: the `t` hook is kept for future row-level strings; if unused after implementation, remove the import before committing so `oxlint --deny-warnings` passes.

- [ ] **Step 2: Create the grouped list**

Create `apps/web/core/components/services/list/services-list.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { ChevronDownOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
// hooks
import { useServiceFilter } from "@/hooks/store/use-service-filter";
// helpers
import { groupServices } from "@/services/service.helpers";
// types
import type { IService, IServiceHealthSnapshot } from "@plane/types";
// local imports
import { ServicesListRow } from "./services-list-row";

type Props = {
  services: IService[];
  filterKey: string;
  healthMap: Record<string, IServiceHealthSnapshot>;
};

export const ServicesList = observer(function ServicesList({ services, filterKey, healthMap }: Props) {
  const { t } = useTranslation();
  const { getDisplayFiltersByProjectId } = useServiceFilter();
  const groupBy = getDisplayFiltersByProjectId(filterKey)?.group_by ?? "status";
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const groups = groupServices(services, groupBy, healthMap);

  const toggleGroup = (key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const groupLabel = (key: string): string => {
    if (groupBy === "status") return t(`service.status_values.${key}`);
    if (groupBy === "criticality") return t(`service.criticality_values.${key}`);
    if (groupBy === "health") return t(`service.health_values.${key}`);
    return key;
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center gap-3 border-b border-subtle px-3 py-1 text-9 font-medium tracking-wide text-tertiary uppercase">
        <span className="flex-1">{t("service.list.service")}</span>
        <span className="hidden w-[100px] shrink-0 sm:block">{t("service.list.status")}</span>
        <span className="hidden w-[90px] shrink-0 md:block">{t("service.list.criticality")}</span>
        <span className="hidden w-[90px] shrink-0 md:block">{t("service.list.health")}</span>
        <span className="hidden w-[100px] shrink-0 lg:block">{t("service.list.incidents")}</span>
        <span className="hidden w-[90px] shrink-0 xl:block">{t("service.list.deploy")}</span>
        <span className="hidden w-[40px] shrink-0 text-right xl:block">{t("service.list.owner")}</span>
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1">
        {groups.map((group) => {
          const isCollapsed = collapsed.has(group.key);
          return (
            <div key={group.key}>
              {groupBy && (
                <button
                  type="button"
                  onClick={() => toggleGroup(group.key)}
                  aria-expanded={!isCollapsed}
                  className="flex w-full items-center gap-1.5 border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 font-medium text-secondary hover:text-primary"
                >
                  <ChevronDownOutline
                    className={cn("h-3.5 w-3.5 text-tertiary transition-transform", isCollapsed && "-rotate-90")}
                  />
                  <span className="capitalize">{groupLabel(group.key)}</span>
                  <span className="font-code text-10 text-tertiary tabular-nums">{group.services.length}</span>
                </button>
              )}
              {!isCollapsed &&
                group.services.map((service) => <ServicesListRow key={service.id} serviceId={service.id} />)}
            </div>
          );
        })}
      </div>
    </div>
  );
});
```

`ChevronDownOutline` is confirmed exported from `@makeplane/propel/icons` (already used in `dropdowns/order-by.tsx`).

Group-label typing: the `t()` calls use template-literal keys cast to their union types. If `tsc` still rejects a call, replace that single call with an explicit `Record` lookup built from concrete `t("service.status_values.active")`-style calls:

```ts
import type { TServiceCriticality, TServiceHealth, TServiceStatus } from "@plane/types";

const groupLabel = (key: string): string => {
  if (groupBy === "status") return t(`service.status_values.${key as TServiceStatus}`);
  ...
};
```

If that still fails, fall back to a `Record<string, string>` built from explicit `t()` calls per known value. Do not use `as never` or `@ts-expect-error`.

Health snapshots for grouping: `ServicesList` receives the already-filtered-and-ordered `IService[]` plus the store's `healthMap` (exposed directly on the store object returned by `useService()`) as props.

- [ ] **Step 3: Create the group-by dropdown**

Create `apps/web/core/components/services/dropdowns/group-by.tsx`, mirroring `dropdowns/order-by.tsx` exactly (same `CustomMenu` + `MenuItem` contract):

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ChevronDownOutline, GroupOutline, TickOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { getButtonStyling } from "@plane/propel/button";
import type { TServiceGroupByOptions } from "@plane/types";
// ui
import { CustomMenu } from "@plane/ui";
// helpers
import { cn } from "@plane/utils";

const SERVICE_GROUP_BY_OPTIONS: { key: Exclude<TServiceGroupByOptions, null>; i18n_label: string }[] = [
  { key: "status", i18n_label: "service.list.group_by.status" },
  { key: "criticality", i18n_label: "service.list.group_by.criticality" },
  { key: "health", i18n_label: "service.list.group_by.health" },
];

type Props = {
  onChange: (value: TServiceGroupByOptions) => void;
  value: TServiceGroupByOptions | undefined;
};

export function ServiceGroupByDropdown(props: Props) {
  const { onChange, value } = props;
  // hooks
  const { t } = useTranslation();

  const groupByDetails = SERVICE_GROUP_BY_OPTIONS.find((option) => option.key === value);

  return (
    <CustomMenu
      customButton={
        <div className={cn(getButtonStyling("secondary", "lg"), "px-2 text-tertiary")}>
          <GroupOutline className="size-3" />
          {groupByDetails ? t(groupByDetails.i18n_label) : t("service.list.group_by.label")}
          <ChevronDownOutline className="size-3" />
        </div>
      }
      placement="bottom-end"
      maxHeight="lg"
      closeOnSelect
    >
      {SERVICE_GROUP_BY_OPTIONS.map((option) => (
        <CustomMenu.MenuItem
          key={option.key}
          className="flex items-center justify-between gap-2"
          onClick={() => onChange(option.key)}
        >
          {t(option.i18n_label)}
          {value === option.key && <TickOutline className="h-3 w-3" />}
        </CustomMenu.MenuItem>
      ))}
    </CustomMenu>
  );
}
```

`GroupOutline` is confirmed exported from `@makeplane/propel/icons`.

- [ ] **Step 4: Wire the view**

In `apps/web/core/components/services/services-list-view.tsx`:

1. Replace `import { ServicesBoard } from "./board/services-board";` with `import { ServicesList } from "./list/services-list";`.
2. Replace the layout fallback: `displayFilters?.layout ?? "board"` → `displayFilters?.layout ?? "list"`, and the workspace branch `"board"` → `"list"`.
3. Replace `<ServicesBoard serviceIds={serviceIds} />` with `<ServicesList services={orderedServices} filterKey={...} healthMap={healthMap} />` — resolve the actual ordered `IService[]` from the existing selectors (`getFilteredServiceIds` returns ids; map via `getServiceById`, filtering out nulls), and pass the store's `healthMap`. Use the same `resolvedFilterKey` already computed in that file for `filterKey`, falling back to `WORKSPACE_SERVICE_FILTER_KEY` (already imported in that file) when undefined.
4. Render the layout toggle change is automatic via `SERVICE_VIEW_LAYOUTS`.

In `apps/web/core/components/services/service-view-header.tsx`, add the group-by dropdown next to `ServiceOrderByDropdown`, visible only when `displayFilters?.layout === "list"`:

```tsx
{
  displayFilters?.layout === "list" && (
    <ServiceGroupByDropdown
      value={displayFilters?.group_by}
      onChange={(val) => {
        if (!resolvedFilterKey || val === displayFilters?.group_by) return;
        updateDisplayFilters(resolvedFilterKey, { group_by: val });
      }}
    />
  );
}
```

- [ ] **Step 5: Delete the board and update the barrel**

```bash
git rm apps/web/core/components/services/board/services-board.tsx \
  apps/web/core/components/services/board/services-board-row.tsx
```

In `apps/web/core/components/services/index.ts`, replace:

```ts
export * from "./board/services-board";
export * from "./board/services-board-row";
```

with:

```ts
export * from "./list/services-list";
export * from "./list/services-list-row";
```

- [ ] **Step 6: Typecheck, lint, and test**

Run:

```bash
pnpm turbo run check:types check:lint --filter=web
pnpm --filter=web test -- core/services/service.helpers.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/services/list \
  apps/web/core/components/services/dropdowns/group-by.tsx \
  apps/web/core/components/services/services-list-view.tsx \
  apps/web/core/components/services/service-view-header.tsx \
  apps/web/core/components/services/index.ts \
  apps/web/core/components/services/board
git commit -m "feat(services): replace board with grouped list layout"
```

---

### Task 5: i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/service.json` + all 19 other locale `service.json` files

- [ ] **Step 1: Add the English keys**

In `packages/i18n/src/locales/en/service.json`, add `"layout": { "list": "List", ... }` entry, a top-level `"list"` object, and remove the `"board"` object:

```json
    "layout": {
      "list": "List",
      "graph": "Graph"
    },
```

```json
    "list": {
      "service": "Service",
      "status": "Status",
      "criticality": "Criticality",
      "health": "Health",
      "incidents": "Incidents",
      "deploy": "Last deploy",
      "owner": "Owner",
      "group_by": {
        "label": "Group by",
        "status": "Status",
        "criticality": "Criticality",
        "health": "Health"
      },
      "expand_group": "Expand group",
      "collapse_group": "Collapse group"
    },
```

Delete the existing `"board": { "service": ..., "health": ..., "incidents": ..., "deploy": ..., "owner": ... }` object. Keep `"layout_label"` and `"order_by"` untouched. Verify no other `service.board.*` references remain with `rg "service\.board\." packages/i18n/src/locales/en`.

- [ ] **Step 2: Translate into every locale**

Mirror the English structure in each of the other 19 locale files. Rules per the `translate` skill: never copy English as a shortcut is forbidden — every value must be translated; preserve ICU/placeholder syntax (none here); keep `service.status_values.*` / `service.health_values.*` / `service.criticality_values.*` untouched.

Reference translations (verify against each locale file's existing vocabulary for Service/Status/Health/Incidents/Last deploy/Owner before writing):

| Locale  | List      | Service  | Status     | Criticality         | Health            | Incidents    | Last deploy             | Owner        | Group by            |
| ------- | --------- | -------- | ---------- | ------------------- | ----------------- | ------------ | ----------------------- | ------------ | ------------------- |
| `id`    | Daftar    | Layanan  | Status     | Kritikalitas        | Kesehatan         | Insiden      | Deployment terakhir     | Pemilik      | Kelompokkan         |
| `de`    | Liste     | Service  | Status     | Kritikalität        | Zustand           | Vorfälle     | Letztes Deployment      | Eigentümer   | Gruppieren nach     |
| `fr`    | Liste     | Service  | Statut     | Criticité           | Santé             | Incidents    | Dernier déploiement     | Propriétaire | Grouper par         |
| `es`    | Lista     | Servicio | Estado     | Criticidad          | Salud             | Incidentes   | Último despliegue       | Propietario  | Agrupar por         |
| `it`    | Elenco    | Servizio | Stato      | Criticità           | Stato di salute   | Incidenti    | Ultimo deployment       | Proprietario | Raggruppa per       |
| `pt-BR` | Lista     | Serviço  | Status     | Criticidade         | Saúde             | Incidentes   | Última implantação      | Proprietário | Agrupar por         |
| `pl`    | Lista     | Usługa   | Status     | Krytyczność         | Kondycja          | Incydenty    | Ostatnie wdrożenie      | Właściciel   | Grupuj według       |
| `cs`    | Seznam    | Služba   | Stav       | Kritičnost          | Zdraví            | Incidenty    | Poslední nasazení       | Vlastník     | Seskupit podle      |
| `sk`    | Zoznam    | Služba   | Stav       | Kritickosť          | Stav              | Incidenty    | Posledné nasadenie      | Vlastník     | Zoskupiť podľa      |
| `ro`    | Listă     | Serviciu | Stare      | Criticitate         | Stare de sănătate | Incidente    | Ultima implementare     | Proprietar   | Grupează după       |
| `ru`    | Список    | Сервис   | Статус     | Критичность         | Состояние         | Инциденты    | Последний деплой        | Владелец     | Группировать по     |
| `ua`    | Список    | Сервіс   | Статус     | Критичність         | Стан              | Інциденти    | Останній деплой         | Власник      | Групувати за        |
| `tr-TR` | Liste     | Hizmet   | Durum      | Kritiklik           | Sağlık            | Olaylar      | Son dağıtım             | Sahip        | Gruplandırma ölçütü |
| `vi-VN` | Danh sách | Dịch vụ  | Trạng thái | Mức độ nghiêm trọng | Tình trạng        | Sự cố        | Lần triển khai gần nhất | Chủ sở hữu   | Nhóm theo           |
| `ja`    | リスト    | サービス | ステータス | 重要度              | ヘルス            | インシデント | 最終デプロイ            | 所有者       | グループ化          |
| `ko`    | 목록      | 서비스   | 상태       | 중요도              | 상태              | 인시던트     | 최근 배포               | 소유자       | 그룹화 기준         |
| `zh-CN` | 列表      | 服务     | 状态       | 关键程度            | 健康状况          | 事故         | 上次部署                | 所有者       | 分组依据            |
| `zh-TW` | 清單      | 服務     | 狀態       | 關鍵程度            | 健康狀況          | 事故         | 上次部署                | 擁有者       | 分組依據            |
| `ka-ge` | სია       | სერვისი  | სტატუსი    | კრიტიკულობა         | ჯანმრთელობა       | ინციდენტები  | ბოლო განლაგება          | მფლობელი     | დაჯგუფება           |

Also translate `expand_group` / `collapse_group` per locale (e.g. en "Expand group" / "Collapse group", id "Bentangkan grup" / "Ciutkan grup", de "Gruppe erweitern" / "Gruppe einklappen", fr "Développer le groupe" / "Réduire le groupe", es "Expandir grupo" / "Contraer grupo"). For locales not listed here, translate both strings natively following the skill's tone/register rules.

- [ ] **Step 3: Regenerate types and verify service-namespace sync**

Run:

```bash
pnpm --filter=@plane/i18n run generate:types
```

Then verify every locale's `service.json` contains the new `list` object and no longer contains a `board` object:

```bash
for f in packages/i18n/src/locales/*/service.json; do
  python3 -c "import json,sys; d=json.load(open('$f')); svc=d['service']; assert 'list' in svc and 'board' not in svc and svc['layout']['list'], '$f'"
done && echo "service namespace synced"
```

Expected: `service namespace synced`. (Full-repo `check:sync` has known pre-existing drift in `auth`/`home`/`project_settings`; only the service namespace is in scope.)

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "feat(i18n): service list layout copy"
```

---

### Task 6: Verification and rollout

**Files:** none.

- [ ] **Step 1: Run the full relevant checks**

```bash
pnpm turbo run check:types check:lint --filter=web
pnpm --filter=web test -- core/services/service.helpers.test.ts core/services/service-health.helpers.test.ts core/services/service-health.service.test.ts
```

Expected: all pass.

- [ ] **Step 2: Build and restart the web app**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000
```

Expected: `200`.

- [ ] **Step 3: Smoke test**

In the UI, open the workspace Services catalog and verify:

- Default view is the grouped list (Status groups), not the old board; toggle switches between List and Graph.
- Group-by control switches Status / Criticality / Health groupings with correct group order and counts.
- Collapsing/expanding groups works; row click still opens the peek.
- Combining a filter with its group dimension yields a single group; incident/health filters keep working.
- A fresh browser profile (or cleared localStorage) gets `list`; an old profile with persisted `board` also lands on `list`.
- Mobile widths hide Status/Criticality/Health/Incidents/Deploy/Owner columns progressively without horizontal overflow.
- No api-rs rebuild or restart needed.

- [ ] **Step 4: Commit any final docs**

```bash
git add docs/superpowers/specs/2026-10-24-service-list-layout-design.md
git commit -m "docs: align service list spec with implementation"
```

(Only if the spec was adjusted during implementation; otherwise skip this step.)
