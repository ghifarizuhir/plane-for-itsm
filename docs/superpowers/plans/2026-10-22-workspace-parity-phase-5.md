# Workspace Parity Phase 5 (Service Catalog) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The user selected inline execution for this repository; do not dispatch subagents.

**Goal:** Make the workspace Service Catalog use the same board/graph, filters, ordering, create modal, and empty/loading states as project Services, driven by the global Application scope.

**Architecture:** Keep services workspace-owned but scope them by owner project or project links. Reuse `ServicesListView`, `ServicesBoard`, `ServiceGraph`, `ServiceViewHeader`, `ServiceAppliedFiltersList`, and `WorkspaceCreateServiceModal`. Add a reserved `__workspace__` service-filter key and workspace-scoped service selectors. Graph is available only with a single scoped application; All falls back to the board.

**Tech Stack:** React Router, MobX (`ServicesStore`, `ServiceFilterStore`), existing service filters/order helpers, Vitest.

---

## File Map

**Create**

- `apps/web/core/lib/workspace-services.ts` — scope/select helpers + reserved filter key.
- `apps/web/core/lib/workspace-services.test.ts` — selector tests.
- `apps/web/core/hooks/use-workspace-service-permissions.ts` — scoped/All service create permission.

**Modify**

- `apps/web/core/store/service_filter.store.ts` — expose initializer, guard clearing.
- `apps/web/core/store/service.store.ts` — workspace error + scoped/filtered/health selectors, graph scope by owner/link.
- `apps/web/core/components/services/board/services-board.tsx` — explicit service ids.
- `apps/web/core/components/services/services-list-view.tsx` — workspace mode, graph/empty/create behavior.
- `apps/web/core/components/services/service-view-header.tsx` — workspace breadcrumbs/chip, filter key, graph disable, workspace create modal.
- `apps/web/core/components/services/service-mobile-header.tsx` — workspace filter key/graph disable.
- `apps/web/core/components/services/workspace/create-service-modal.tsx` — respect `service_view`.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx` — list header/mobile header.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/services/page.tsx` — scope-aware catalog.

**Delete**

- `apps/web/core/components/services/workspace/workspace-services-list-view.tsx`
- `apps/web/core/components/services/workspace/service-groups.ts`
- `apps/web/core/components/services/workspace/service-groups.test.ts`

**Do not change**

- Service detail route `/services/:serviceId` behavior.
- Project service redirects.
- No new i18n keys; reuse `service.*` keys and the existing disabled-feature button label.

---

## Task 1: Workspace Service Selector

**Files:**

- Create: `apps/web/core/lib/workspace-services.ts`
- Test: `apps/web/core/lib/workspace-services.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/web/core/lib/workspace-services.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { IService, IServiceHealthSnapshot } from "@plane/types";
import { selectWorkspaceServiceIds, WORKSPACE_SERVICE_FILTER_KEY } from "./workspace-services";

const makeService = (overrides: Partial<IService>): IService =>
  ({
    id: "service",
    name: "Service",
    owner_project_id: "p1",
    project_links: [],
    status: "active",
    criticality: "medium",
    type: "internal",
    created_at: "2026-10-01T00:00:00.000Z",
    updated_at: "2026-10-01T00:00:00.000Z",
    ...overrides,
  }) as IService;

const select = (services: IService[], overrides: Partial<Parameters<typeof selectWorkspaceServiceIds>[0]> = {}) =>
  selectWorkspaceServiceIds({
    services,
    projectIds: ["p1"],
    filters: {},
    searchQuery: "",
    orderBy: "name",
    healthMap: {},
    ...overrides,
  });

describe("selectWorkspaceServiceIds", () => {
  it("matches services by owner project or linked project", () => {
    const selection = select([
      makeService({ id: "owner", owner_project_id: "p1" }),
      makeService({
        id: "linked",
        owner_project_id: "p2",
        project_links: [{ id: "l1", project_id: "p1", link_type: "consumer" }],
      }),
      makeService({ id: "other", owner_project_id: "p2" }),
    ]);

    expect(selection).toEqual(["linked", "owner"]);
  });

  it("returns all services for All applications", () => {
    const selection = select(
      [
        makeService({ id: "assigned", owner_project_id: "p1" }),
        makeService({ id: "unassigned", owner_project_id: null }),
      ],
      { projectIds: undefined }
    );

    expect(selection).toEqual(["assigned", "unassigned"]);
  });

  it("applies the search query", () => {
    const selection = select(
      [makeService({ id: "billing", name: "Billing API" }), makeService({ id: "auth", name: "Auth" })],
      { searchQuery: "bill" }
    );

    expect(selection).toEqual(["billing"]);
  });

  it("applies status, criticality, and type filters", () => {
    const selection = select(
      [
        makeService({ id: "match", status: "active", criticality: "high", type: "internal" }),
        makeService({ id: "wrong-status", status: "retired", criticality: "high", type: "internal" }),
        makeService({ id: "wrong-criticality", status: "active", criticality: "low", type: "internal" }),
        makeService({ id: "wrong-type", status: "active", criticality: "high", type: "external" }),
      ],
      { filters: { status: ["active"], criticality: ["high"], type: ["internal"] } }
    );

    expect(selection).toEqual(["match"]);
  });

  it("applies health and active-incident filters", () => {
    const healthMap: Record<string, IServiceHealthSnapshot> = {
      sick: {
        service_id: "sick",
        health: "down",
        incidents: [{ id: "i1", service_id: "sick", severity: "sev1", opened_at: "2026-10-01" }],
        last_deployed_at: null,
      },
      healthy: {
        service_id: "healthy",
        health: "healthy",
        incidents: [],
        last_deployed_at: null,
      },
    };

    const selection = select([makeService({ id: "sick" }), makeService({ id: "healthy" })], {
      filters: { health: ["down"], incidents: ["active"] },
      healthMap,
    });

    expect(selection).toEqual(["sick"]);
  });

  it("orders by health and then criticality", () => {
    const healthMap: Record<string, IServiceHealthSnapshot> = {
      down: { service_id: "down", health: "down", incidents: [], last_deployed_at: null },
      healthy: { service_id: "healthy", health: "healthy", incidents: [], last_deployed_at: null },
    };

    const selection = select(
      [
        makeService({ id: "healthy", name: "A", criticality: "low" }),
        makeService({ id: "down", name: "Z", criticality: "critical" }),
      ],
      { orderBy: "health", healthMap }
    );

    expect(selection).toEqual(["down", "healthy"]);
  });

  it("exports the reserved workspace filter key", () => {
    expect(WORKSPACE_SERVICE_FILTER_KEY).toBe("__workspace__");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
pnpm exec vitest run core/lib/workspace-services.test.ts
```

Expected: FAIL because `./workspace-services` does not exist.

- [ ] **Step 3: Implement the selector**

Create `apps/web/core/lib/workspace-services.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceHealthSnapshot, TServiceFilters, TServiceOrderByOptions } from "@plane/types";
import { filterServices, orderServices } from "@/services/service.helpers";

export const WORKSPACE_SERVICE_FILTER_KEY = "__workspace__";

type TSelectWorkspaceServiceIdsArgs = {
  services: IService[];
  projectIds?: string[];
  filters: TServiceFilters;
  searchQuery: string;
  orderBy: TServiceOrderByOptions;
  healthMap: Record<string, IServiceHealthSnapshot>;
};

export const isServiceInWorkspaceScope = (service: IService, projectIds?: string[]) => {
  if (!projectIds || projectIds.length === 0) return true;
  if (service.owner_project_id && projectIds.includes(service.owner_project_id)) return true;
  return Boolean(service.project_links?.some((link) => projectIds.includes(link.project_id)));
};

export const selectWorkspaceServiceIds = ({
  services,
  projectIds,
  filters,
  searchQuery,
  orderBy,
  healthMap,
}: TSelectWorkspaceServiceIdsArgs): string[] =>
  orderServices(
    filterServices(
      services.filter((service) => isServiceInWorkspaceScope(service, projectIds)),
      filters,
      searchQuery,
      healthMap
    ),
    orderBy,
    healthMap
  ).map((service) => service.id);
```

- [ ] **Step 4: Run tests, lint, and commit**

```bash
pnpm exec vitest run core/lib/workspace-services.test.ts
pnpm exec oxlint core/lib/workspace-services.ts core/lib/workspace-services.test.ts
pnpm check:types
git add apps/web/core/lib/workspace-services.ts apps/web/core/lib/workspace-services.test.ts
git commit -m "feat(web): add workspace service selector"
```

---

## Task 2: Workspace Service Store + Filter Store

**Files:**

- Modify: `apps/web/core/store/service_filter.store.ts`
- Modify: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Expose the filter initializer and guard clearing**

In `service_filter.store.ts`, add to `IServiceFilterStore`:

```ts
  initProjectServiceFilters: (projectId: string) => void;
```

Replace `clearAllFilters` with:

```ts
clearAllFilters = (projectId: string) => {
  runInAction(() => {
    this.filters[projectId] = {};
  });
  this.saveFiltersToLocalStorage();
};
```

This is already safe for an unknown key; keep it.

- [ ] **Step 2: Add workspace selectors/error to the service store**

In `service.store.ts`, import:

```ts
import { selectWorkspaceServiceIds, WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";
```

Add to `IServiceStore`:

```ts
  workspaceError: boolean;
  getWorkspaceScopedServiceIds: (projectIds?: string[]) => string[];
  getWorkspaceFilteredServiceIds: (projectIds?: string[]) => string[];
  getWorkspaceHealthSummary: (projectIds?: string[]) => TServiceHealthSummary;
```

Add the observable:

```ts
workspaceError: boolean = false;
```

Add `workspaceError: observable.ref` in `makeObservable`.

Replace `getWorkspaceServiceIds`/graph with these methods (keep `getWorkspaceServiceIds`):

```ts
getWorkspaceServiceIds = computedFn(() => {
  if (!this.workspaceFetched) return [];
  return sortBy(Object.values(this.serviceMap), [(s) => s.sort_order]).map((s) => s.id);
});

getWorkspaceScopedServiceIds = computedFn((projectIds?: string[]) => {
  if (!this.workspaceFetched) return [];
  return selectWorkspaceServiceIds({
    services: Object.values(this.serviceMap),
    projectIds,
    filters: {},
    searchQuery: "",
    orderBy: "name",
    healthMap: {},
  });
});

getWorkspaceFilteredServiceIds = computedFn((projectIds?: string[]) => {
  if (!this.workspaceFetched) return [];
  const displayFilters = this.rootStore.serviceFilter.getDisplayFiltersByProjectId(WORKSPACE_SERVICE_FILTER_KEY);
  const filters = this.rootStore.serviceFilter.getFiltersByProjectId(WORKSPACE_SERVICE_FILTER_KEY);
  const searchQuery = this.rootStore.serviceFilter.searchQuery;
  return selectWorkspaceServiceIds({
    services: Object.values(this.serviceMap),
    projectIds,
    filters,
    searchQuery,
    orderBy: displayFilters?.order_by ?? "health",
    healthMap: this.healthMap,
  });
});

getWorkspaceHealthSummary = computedFn((projectIds?: string[]): TServiceHealthSummary => {
  const summary: TServiceHealthSummary = { down: 0, degraded: 0, healthy: 0, unknown: 0, criticalImpacted: 0 };
  this.getWorkspaceScopedServiceIds(projectIds).forEach((serviceId) => {
    const service = this.serviceMap[serviceId];
    if (!service) return;
    const state = this.healthMap[service.id]?.health ?? "unknown";
    summary[state] += 1;
    if (state !== "healthy" && state !== "unknown" && service.criticality === "critical") {
      summary.criticalImpacted += 1;
    }
  });
  return summary;
});
```

Update `getWorkspaceGraphData` scope filter:

```ts
      .filter(
        (service) =>
          !projectId ||
          service.owner_project_id === projectId ||
          service.project_links?.some((l) => l.project_id === projectId)
      );
```

In `fetchWorkspaceServices`:

```ts
this.workspaceError = false;
this.loader = true;
```

and on catch:

```ts
runInAction(() => {
  this.loader = false;
  this.workspaceError = true;
});
```

- [ ] **Step 3: Type-check and commit**

```bash
pnpm exec oxlint core/store/service_filter.store.ts core/store/service.store.ts
pnpm check:types
git add apps/web/core/store/service_filter.store.ts apps/web/core/store/service.store.ts
git commit -m "feat(web): add workspace service filters"
```

---

## Task 3: Workspace Service Board/List/Header

**Files:**

- Modify: `apps/web/core/components/services/board/services-board.tsx`
- Modify: `apps/web/core/components/services/services-list-view.tsx`
- Modify: `apps/web/core/components/services/service-view-header.tsx`
- Modify: `apps/web/core/components/services/service-mobile-header.tsx`
- Create: `apps/web/core/hooks/use-workspace-service-permissions.ts`

- [ ] **Step 1: Make `ServicesBoard` accept explicit ids**

In `services-board.tsx`:

```ts
type Props = {
  serviceIds?: string[];
};

export const ServicesBoard = observer(function ServicesBoard({ serviceIds: serviceIdsProp }: Props) {
  const { projectId } = useParams();
  const { getFilteredServiceIds } = useService();
  const serviceIds = serviceIdsProp ?? (projectId ? (getFilteredServiceIds(projectId.toString()) ?? []) : []);
```

Keep the rest unchanged.

- [ ] **Step 2: Add the workspace permission hook**

Create `use-workspace-service-permissions.ts`:

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

export const useWorkspaceServicePermissions = () => {
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { getProjectById, workspaceProjectIds } = useProject();
  const { allowPermissions } = useUserPermissions();
  const { projectsWithCreatePermissions } = useUser();

  const canCreateInProject = (projectId: string) => {
    const project = getProjectById(projectId);
    return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.service_view !== false;
  };

  const canCreateService = scopedProjectId
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

  return { canCreateService };
};
```

- [ ] **Step 3: Make `ServicesListView` workspace-aware**

In `services-list-view.tsx` add props:

```ts
type Props = {
  workspaceMode?: boolean;
  projectIds?: string[];
  filterKey?: string;
  scopeProjectId?: string;
};
```

Component setup:

```ts
export const ServicesListView = observer(function ServicesListView({
  workspaceMode = false,
  projectIds,
  filterKey,
  scopeProjectId,
}: Props) {
  const { workspaceSlug, projectId } = useParams();
  const { t } = useTranslation();
  const {
    errorMap,
    fetchServices,
    fetchWorkspaceServices,
    getFilteredServiceIds,
    getProjectHealthSummary,
    getProjectServiceIds,
    getWorkspaceFilteredServiceIds,
    getWorkspaceHealthSummary,
    getWorkspaceScopedServiceIds,
    loader,
    workspaceError,
    workspaceFetched,
  } = useService();
  const { getDisplayFiltersByProjectId, clearAllFilters } = useServiceFilter();
  const { currentWorkspace } = useWorkspace();
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const openCreateModal = () => setIsCreateModalOpen(true);
  const closeCreateModal = () => setIsCreateModalOpen(false);

  const resolvedFilterKey = filterKey ?? (workspaceMode ? WORKSPACE_SERVICE_FILTER_KEY : projectId?.toString());
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const layout =
    workspaceMode && !scopeProjectId && displayFilters?.layout === "graph"
      ? "board"
      : (displayFilters?.layout ?? "board");

  const totalServiceIds = workspaceMode
    ? getWorkspaceScopedServiceIds(projectIds)
    : projectId
      ? getProjectServiceIds(projectId.toString())
      : null;
  const serviceIds = workspaceMode
    ? getWorkspaceFilteredServiceIds(projectIds)
    : projectId
      ? getFilteredServiceIds(projectId.toString())
      : null;
  const summary = workspaceMode
    ? getWorkspaceHealthSummary(projectIds)
    : projectId
      ? getProjectHealthSummary(projectId.toString())
      : null;

  const workspaceId = currentWorkspace?.id;
  const hasError = workspaceMode ? workspaceError : projectId ? errorMap[projectId.toString()] : false;
  const isLoading = loader && (workspaceMode ? !workspaceFetched : totalServiceIds === null);
```

Retry:

```ts
const handleRetry = () => {
  if (!workspaceSlug || !workspaceId) return;
  if (workspaceMode) {
    void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId);
    return;
  }
  if (projectId) void fetchServices(workspaceSlug.toString(), workspaceId, projectId.toString());
};
```

Render checks:

```ts
    if (isLoading || totalServiceIds === null || serviceIds === null) {
      return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    }
    if (totalServiceIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
          <Button variant="primary" size="sm" onClick={openCreateModal}>
            {t("service.add")}
          </Button>
        </div>
      );
    }
    if (serviceIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.no_matches.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.no_matches.description")}</p>
          {resolvedFilterKey && (
            <Button variant="secondary" size="sm" onClick={() => clearAllFilters(resolvedFilterKey)}>
              {t("common.clear_all")}
            </Button>
          )}
        </div>
      );
    }
```

Graph/board:

```tsx
if (layout === "graph" && (workspaceMode ? scopeProjectId : projectId)) {
  return (
    <div className="min-h-0 w-full flex-1">
      <ServiceGraph
        scope={workspaceMode ? "workspace" : "project"}
        projectIdOverride={workspaceMode ? scopeProjectId : undefined}
      />
    </div>
  );
}
return (
  <>
    {summary && <ServiceHealthSummary summary={summary} />}
    <ServicesBoard serviceIds={serviceIds} />
  </>
);
```

Create modal:

```tsx
{
  workspaceMode ? (
    <WorkspaceCreateServiceModal
      isOpen={isCreateModalOpen}
      onClose={closeCreateModal}
      workspaceSlug={workspaceSlug?.toString() ?? ""}
      defaultOwnerProjectId={scopeProjectId}
    />
  ) : (
    workspaceSlug &&
    projectId && (
      <CreateUpdateServiceModal
        isOpen={isCreateModalOpen}
        onClose={closeCreateModal}
        workspaceSlug={workspaceSlug.toString()}
        projectId={projectId.toString()}
      />
    )
  );
}
```

Imports to add/remove:

```ts
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";
import { WorkspaceCreateServiceModal } from "@/components/services/workspace/create-service-modal";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
```

- [ ] **Step 4: Make `ServiceViewHeader` workspace-aware**

Add props:

```ts
type Props = {
  workspaceMode?: boolean;
  filterKey?: string;
};
```

Component setup:

```ts
export const ServiceViewHeader = observer(function ServiceViewHeader({
  workspaceMode = false,
  filterKey,
}: Props) {
  const router = useAppRouter();
  const { workspaceSlug, projectId } = useParams();
  const { isMobile } = usePlatformOS();
  const { allowPermissions } = useUserPermissions();
  const { loader } = useProject();
  const {
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    updateDisplayFilters,
    updateFilters,
  } = useServiceFilter();
  const { t } = useTranslation();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreateService } = useWorkspaceServicePermissions();
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const resolvedFilterKey =
    filterKey ?? (workspaceMode ? WORKSPACE_SERVICE_FILTER_KEY : projectId?.toString());
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
```

Filters handler guard `resolvedFilterKey`; `updateFilters(resolvedFilterKey,...)`.

Auth:

```ts
const canUserCreateService = workspaceMode
  ? canCreateService
  : allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT);
```

Breadcrumbs conditional:

```tsx
{
  workspaceMode ? (
    <div className="flex min-w-0 items-center gap-2">
      <Breadcrumbs onBack={router.back}>
        <Breadcrumbs.Item
          component={
            <BreadcrumbLink
              label={t("service.catalog_title")}
              href={`/${workspaceSlug}/services/`}
              icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
              isLast
            />
          }
          isLast
        />
      </Breadcrumbs>
      <WorkspaceApplicationScopeChip />
    </div>
  ) : (
    <Breadcrumbs onBack={router.back} isLoading={loader === "init-loader"}>
      <CommonProjectBreadcrumbs workspaceSlug={workspaceSlug?.toString()} projectId={projectId?.toString()} />
      <Breadcrumbs.Item
        component={
          <BreadcrumbLink
            label={t("service.title")}
            href={`/${workspaceSlug}/projects/${projectId}/services/`}
            icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
            isLast
          />
        }
        isLast
      />
    </Breadcrumbs>
  );
}
```

Layout controls use `resolvedFilterKey`; graph disabled when workspace mode and no scoped application:

```tsx
<div className="hidden items-center gap-1 rounded-sm bg-layer-3 p-1 md:flex">
  {SERVICE_VIEW_LAYOUTS.map((layout) => {
    const isLayoutDisabled = workspaceMode && !scopedProjectId && layout.key === "graph";

    return (
      <Tooltip
        key={layout.key}
        label={isLayoutDisabled ? "Select an application to view graph" : t(layout.i18n_label)}
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
          <ServiceLayoutIcon layoutType={layout.key} />
        </button>
      </Tooltip>
    );
  })}
</div>
```

Create modal conditional:

```tsx
{
  workspaceMode ? (
    <WorkspaceCreateServiceModal
      isOpen={isCreateModalOpen}
      onClose={() => setIsCreateModalOpen(false)}
      workspaceSlug={workspaceSlug?.toString() ?? ""}
      defaultOwnerProjectId={scopedProjectId ?? undefined}
    />
  ) : (
    workspaceSlug &&
    projectId && (
      <CreateUpdateServiceModal
        isOpen={isCreateModalOpen}
        onClose={() => setIsCreateModalOpen(false)}
        workspaceSlug={workspaceSlug.toString()}
        projectId={projectId.toString()}
      />
    )
  );
}
```

Add imports:

```ts
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceServicePermissions } from "@/hooks/use-workspace-service-permissions";
import { WorkspaceCreateServiceModal } from "./workspace/create-service-modal";
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";
```

- [ ] **Step 5: Make the mobile header scope-aware**

Replace `service-mobile-header.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { CustomMenu, Row } from "@plane/ui";
import { SERVICE_VIEW_LAYOUTS, ServiceLayoutIcon } from "./service-layout-icon";
import { useServiceFilter } from "@/hooks/store/use-service-filter";

type Props = {
  filterKey?: string;
  isGraphDisabled?: boolean;
};

export const ServiceMobileHeader = observer(function ServiceMobileHeader({
  filterKey,
  isGraphDisabled = false,
}: Props) {
  const { projectId } = useParams();
  const { t } = useTranslation();
  const { updateDisplayFilters } = useServiceFilter();
  const resolvedFilterKey = filterKey ?? projectId?.toString();

  return (
    <div className="flex justify-start md:hidden">
      <CustomMenu
        maxHeight="md"
        className="flex flex-grow justify-start border-b border-subtle bg-surface-1 py-2 text-13 text-secondary"
        customButton={
          <Row className="flex flex-grow justify-center gap-2 text-13 text-secondary">
            <span>{t("service.layout_label")}</span>
            <ChevronDownOutline className="my-auto h-4 w-4 text-secondary" />
          </Row>
        }
        customButtonClassName="flex flex-grow justify-center items-center text-secondary text-13"
        closeOnSelect
      >
        {SERVICE_VIEW_LAYOUTS.map((layout) => {
          const isLayoutDisabled = layout.key === "graph" && isGraphDisabled;

          return (
            <CustomMenu.MenuItem
              key={layout.key}
              onClick={() => {
                if (isLayoutDisabled || !resolvedFilterKey) return;
                updateDisplayFilters(resolvedFilterKey, { layout: layout.key });
              }}
              className="flex items-center gap-2"
            >
              <ServiceLayoutIcon layoutType={layout.key} />
              <div className="text-tertiary">{t(layout.i18n_label)}</div>
            </CustomMenu.MenuItem>
          );
        })}
      </CustomMenu>
    </div>
  );
});
```

- [ ] **Step 6: Respect `service_view` in the workspace create picker**

In `workspace/create-service-modal.tsx`, add `&& p.service_view !== false` to the projects filter.

- [ ] **Step 7: Lint/type-check and commit**

```bash
pnpm exec oxlint core/components/services/board/services-board.tsx core/components/services/services-list-view.tsx core/components/services/service-view-header.tsx core/components/services/service-mobile-header.tsx core/components/services/workspace/create-service-modal.tsx core/hooks/use-workspace-service-permissions.ts
pnpm check:types
git add apps/web/core/components/services/board/services-board.tsx apps/web/core/components/services/services-list-view.tsx apps/web/core/components/services/service-view-header.tsx apps/web/core/components/services/service-mobile-header.tsx apps/web/core/components/services/workspace/create-service-modal.tsx apps/web/core/hooks/use-workspace-service-permissions.ts
git commit -m "feat(web): make service catalog scope-aware"
```

---

## Task 4: Workspace Service Page/Layout + Legacy Cleanup

**Files:**

- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/services/page.tsx`
- Delete: legacy workspace service list/group files.

- [ ] **Step 1: Update the layout**

Keep `WorkspaceServicesBreadcrumbs` for the detail route. Replace the default export body with:

```tsx
export default function WorkspaceServicesLayout() {
  const { serviceId } = useParams();
  const { projectId } = useWorkspaceApplicationScope();

  return (
    <>
      <AppHeader
        header={serviceId ? <WorkspaceServicesBreadcrumbs /> : <ServiceViewHeader workspaceMode />}
        mobileHeader={
          serviceId ? undefined : (
            <ServiceMobileHeader filterKey={WORKSPACE_SERVICE_FILTER_KEY} isGraphDisabled={!projectId} />
          )
        }
      />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

Add imports:

```ts
import { ServiceMobileHeader, ServiceViewHeader } from "@/components/services";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";
```

- [ ] **Step 2: Replace the workspace service page**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TServiceFilters } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { EHeaderVariant, Header } from "@plane/ui";
import { calculateTotalFilters } from "@plane/utils";
// components
import { PageHead } from "@/components/core/page-title";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { ServiceAppliedFiltersList, ServicesListView } from "@/components/services";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspace } from "@/hooks/store/use-workspace";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { WORKSPACE_SERVICE_FILTER_KEY } from "@/lib/workspace-services";

function WorkspaceServicesPage() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { fetchWorkspaceServices } = useService();
  const {
    clearAllFilters,
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    initProjectServiceFilters,
    updateDisplayFilters,
    updateFilters,
    updateSearchQuery,
  } = useServiceFilter();
  const { getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { allowPermissions } = useUserPermissions();

  const filterKey = WORKSPACE_SERVICE_FILTER_KEY;
  const appliedFilters = getFiltersByProjectId(filterKey) ?? {};
  const displayFilters = getDisplayFiltersByProjectId(filterKey);
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const workspaceId = currentWorkspace?.id;
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.service_view === false);
  const canManageFeatures = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );

  useEffect(() => {
    initProjectServiceFilters(filterKey);
    updateSearchQuery("");
  }, [filterKey, initProjectServiceFilters, updateSearchQuery]);

  useEffect(() => {
    if (scopedProjectId || !displayFilters || displayFilters.layout !== "graph") return;
    updateDisplayFilters(filterKey, { layout: "board" });
  }, [displayFilters, filterKey, scopedProjectId, updateDisplayFilters]);

  useEffect(() => {
    if (!workspaceSlug || !workspaceId) return;
    void fetchWorkspaceServices(workspaceSlug, workspaceId);
  }, [workspaceSlug, workspaceId, fetchWorkspaceServices]);

  const handleRemoveFilter = (key: keyof TServiceFilters, value: string | null) => {
    const currentValues = appliedFilters[key] ?? [];
    const nextValues = value ? currentValues.filter((currentValue) => currentValue !== value) : [];
    updateFilters(filterKey, { [key]: nextValues });
  };

  if (!workspaceSlug) return null;

  if (isFeatureDisabled) {
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title="Services are not enabled for the application."
          description="Enable services from application settings to start mapping dependencies."
          primaryButton={{
            text: t("disabled_project.empty_state.module.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !canManageFeatures,
          }}
        />
      </div>
    );
  }

  return (
    <>
      <PageHead title={t("service.catalog_title")} />
      <div className="flex h-full w-full flex-col">
        {calculateTotalFilters(appliedFilters) !== 0 && (
          <Header variant={EHeaderVariant.TERNARY}>
            <ServiceAppliedFiltersList
              appliedFilters={appliedFilters}
              handleClearAllFilters={() => clearAllFilters(filterKey)}
              handleRemoveFilter={handleRemoveFilter}
              alwaysAllowEditing
            />
          </Header>
        )}
        <ServicesListView
          workspaceMode
          projectIds={scopedProjectId ? [scopedProjectId] : undefined}
          filterKey={filterKey}
          scopeProjectId={scopedProjectId ?? undefined}
        />
      </div>
    </>
  );
}

export default observer(WorkspaceServicesPage);
```

- [ ] **Step 3: Delete legacy files**

```bash
rm apps/web/core/components/services/workspace/workspace-services-list-view.tsx
rm apps/web/core/components/services/workspace/service-groups.ts
rm apps/web/core/components/services/workspace/service-groups.test.ts
```

- [ ] **Step 4: Lint/type-check and commit**

```bash
pnpm exec oxlint 'app/(all)/[workspaceSlug]/(projects)/services/layout.tsx' 'app/(all)/[workspaceSlug]/(projects)/services/page.tsx'
pnpm check:types
git add 'apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx' 'apps/web/app/(all)/[workspaceSlug]/(projects)/services/page.tsx'
git rm apps/web/core/components/services/workspace/workspace-services-list-view.tsx apps/web/core/components/services/workspace/service-groups.ts apps/web/core/components/services/workspace/service-groups.test.ts
git commit -m "feat(web): use project service catalog for workspace"
```

---

## Task 5: Full Verification and Deploy

**Files:**

- No source changes unless verification finds a defect.

- [ ] **Step 1: Tests, lint, types**

```bash
pnpm exec vitest run
pnpm check:lint
pnpm check:types
```

Expected: all exit 0.

- [ ] **Step 2: Build, restart, verify routes**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 20
for path in services pages modules cycles work-items; do printf "%s=" "$path"; curl -s -o /dev/null -w "%{http_code}\n" "http://localhost:3000/itsm/$path"; done
```

Expected: builds pass; all routes return 200.

- [ ] **Step 3: Manual smoke**

Verify:

- `/itsm/services` uses the project-style board/graph toolbar, not the old application dropdown/grouped rows.
- All applications: board shows all services with owner application identifiers; graph is disabled with tooltip; Add service opens the modal with an owner picker.
- Scoped application: board/graph work; Add service preselects the scoped owner; filters/search/order/clear-all remain workspace-scoped.
- Empty catalog, no filter matches, and scoped disabled `service_view` states render correctly.
- Service detail route and project service redirects still work.

Fix failures, rerun Steps 1-2, commit:

```bash
git add <changed-files>
git commit -m "fix(web): address workspace service verification"
```

---

## Self-Review Notes

- Spec 7.7: reuses board/graph, filters, and workspace create modal; scope comes from the sidebar; graph is scoped-only.
- Spec 6.3: workspace services are fetched workspace-wide and filtered by owner/link + `__workspace__` filters; graph data is scope-filtered.
- Spec 7.8: existing service empty/no-match states and loader are reused; scoped `service_view=false` gets a disabled state.
- Spec 7.9: filters and layout stay in the header; mobile header keeps board/graph selection (graph disabled for All).
- Spec 8/9: create preselects the scoped owner or leaves the owner picker for All; permissions use the scoped project.
- Project service redirects and service detail routes are unchanged.
- No new i18n keys; the one disabled-service title/description is hardcoded because no service disabled-state copy exists.

---

## Execution notes (2026-10-22)

- Workspace service scope matches the owner application or any linked application; All includes unassigned services.
- The workspace filter key `__workspace__` drives search, health/status/criticality/type/incident filters, and ordering.
- The board uses explicit scoped service ids; the graph is enabled only for one scoped application and uses the workspace graph data.
- Create reuses `WorkspaceCreateServiceModal`: scoped owner is preselected, All leaves the owner picker available; `service_view=false` projects are excluded.
- Deleted the legacy workspace service list and grouping helpers/tests.
- Verification: 511 web tests pass, `pnpm check:types` passes, `pnpm check:lint` exits 0, OxLint reports 0 on touched files, `@plane/constants` + web builds pass, `plane-web-prod` restarted and `/itsm/services`, `/itsm/pages`, `/itsm/modules`, `/itsm/cycles`, `/itsm/work-items` return 200.
- Manual browser smoke test was not run in this environment; the HTTP/build/test checks above are the available evidence.
