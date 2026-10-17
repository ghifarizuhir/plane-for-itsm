# Workspace-Level Services — Web Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the workspace-level service catalog UI (cross-project list with owner badges and project filter), move service detail to `:workspaceSlug/services/:serviceId`, render lite (restricted) dependency endpoints, and let project admins manage consumer links / workspace admins transfer ownership.

**Architecture:** Canonical workspace routes use new ServiceService client methods + store state (same `serviceMap`, workspace fetch flag). Legacy project-scoped pages keep working through the compat endpoints (pickers, old list page). Existing components are reused with small, surgical edits (board row, detail root, dependency rows, graph node, form).

**Tech Stack:** React Router v7 file routes, MobX store, TypeScript (strict), vitest (`pnpm --filter=web test`), oxfmt/oxlint (`pnpm check`).

**Spec:** `docs/superpowers/specs/2026-10-17-workspace-level-services-design.md` (§4 Frontend & UX). **Depends on:** backend plan (`2026-10-17-workspace-level-services-backend.md`) — the canonical endpoints and JSON contract must be live.

**Backend JSON contract used here:** full service JSON keeps `project_id` (= owner project id, deprecated) and adds `owner_project_id`, `project_links: [{id, project_id, link_type}]`, `access_level: "full" | "lite"`; lite services only carry `{id, name, status, criticality, access_level}`; dependency rows embed `from_service`/`to_service` objects.

**Global constraints:** no new npm deps; reuse existing components/hooks (`useProject`, `useService`, `useWorkspace`, `useUserPermissions`); all new user-facing strings go through `t(...)` with keys added to the existing service namespace (English locale; other locales may fall back); run `pnpm check` (format/lint/types) and `pnpm --filter=web test` before each commit; commit per task with `feat(web): ...` / `test(web): ...`.

---

### Task 1: Types, access helper, vitest tests

**Files:**

- Modify: `packages/types/src/service/core.ts`
- Modify: `apps/web/core/services/service.helpers.ts`
- Modify: `apps/web/core/services/service.helpers.test.ts`

- [ ] **Step 1: Extend the shared types** (`packages/types/src/service/core.ts`)

```ts
export type TServiceAccessLevel = "full" | "lite";

export interface IServiceProjectLink {
  id: string;
  project_id: string;
  link_type: "owner" | "consumer";
}

/** Restricted projection returned for dependency endpoints the caller cannot fully access. */
export interface ILiteService {
  id: string;
  name: string;
  status: TServiceStatus;
  criticality: TServiceCriticality;
  access_level: "lite";
}

export interface IService {
  id: string;
  workspace_id: string;
  /** @deprecated equals `owner_project_id`; kept for backward compatibility. */
  project_id: string | null;
  owner_project_id: string | null;
  project_links: IServiceProjectLink[];
  access_level: "full";
  name: string;
  /* …existing fields unchanged (description, description_html, status, criticality,
     type, owner_id, repository_url, documentation_url, position, sort_order,
     created_at, updated_at, created_by, updated_by)… */
}

export interface IServiceDependency {
  id: string;
  workspace_id: string;
  /** Legacy dual-write column; compat endpoints echo the queried project. */
  project_id: string | null;
  from_service_id: string;
  to_service_id: string;
  created_at: string;
  from_service?: IService | ILiteService;
  to_service?: IService | ILiteService;
}

export type TServiceGraphData = {
  services: (IService | ILiteService)[];
  dependencies: IServiceDependency[];
  health: Record<string, IServiceHealthSnapshot>;
};
```

- [ ] **Step 2: Add the access helper with a failing test**

Append to `apps/web/core/services/service.helpers.ts`:

```ts
import type { ILiteService, IService } from "@plane/types";

/** True when the service is the restricted (lite) projection. */
export const isLiteService = (service: IService | ILiteService): service is ILiteService =>
  service.access_level === "lite";
```

Add to `apps/web/core/services/service.helpers.test.ts`:

```ts
import { isLiteService } from "./service.helpers";

describe("isLiteService", () => {
  it("detects lite projections", () => {
    expect(isLiteService({ access_level: "lite" } as never)).toBe(true);
    expect(isLiteService({ access_level: "full" } as never)).toBe(false);
  });
});
```

Run: `pnpm --filter=web test -- service.helpers` — the helper exists so this passes once added; write the test first and watch it fail on the missing import, then add the helper.

- [ ] **Step 3: Run tests + checks, commit**

Run: `pnpm --filter=web test -- service.helpers && pnpm check`
Expected: PASS.

```bash
git add packages/types/src/service/core.ts apps/web/core/services/service.helpers.ts apps/web/core/services/service.helpers.test.ts
git commit -m "feat(web): workspace service types and lite access helper"
```

---

### Task 2: ServiceService client + store workspace state

**Files:**

- Modify: `apps/web/core/services/service.service.ts`
- Modify: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Add client methods** (`service.service.ts`, after the existing methods)

```ts
  async getWorkspaceServices(
    workspaceSlug: string,
    params?: { project?: string; q?: string; status?: string; criticality?: string; link?: string }
  ): Promise<IService[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/services/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkspaceDependencies(workspaceSlug: string): Promise<IServiceDependency[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/service-dependencies/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async getWorkspaceWorkItemLinks(workspaceSlug: string): Promise<TServiceWorkItemLink[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/service-issues/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async createWorkspaceService(workspaceSlug: string, data: Partial<IService>): Promise<IService> {
    return this.post(`/api/workspaces/${workspaceSlug}/services/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async updateWorkspaceService(workspaceSlug: string, serviceId: string, data: Partial<IService>): Promise<IService> {
    return this.patch(`/api/workspaces/${workspaceSlug}/services/${serviceId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async deleteWorkspaceService(workspaceSlug: string, serviceId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/services/${serviceId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async addProjectLink(
    workspaceSlug: string,
    serviceId: string,
    projectId: string
  ): Promise<IServiceProjectLink> {
    return this.post(`/api/workspaces/${workspaceSlug}/services/${serviceId}/project-links/`, { project_id: projectId })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async removeProjectLink(workspaceSlug: string, serviceId: string, linkId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/services/${serviceId}/project-links/${linkId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }

  async transferServiceOwner(workspaceSlug: string, serviceId: string, projectId: string): Promise<IService> {
    return this.post(`/api/workspaces/${workspaceSlug}/services/${serviceId}/transfer-owner/`, { project_id: projectId })
      .then((response) => response?.data)
      .catch((error) => {
        throw toServiceError(error);
      });
  }
```

Add `IServiceProjectLink` to the existing type import in this file.

- [ ] **Step 2: Store — workspace fetch + link actions** (`service.store.ts`)

2a. Interface additions to `IServiceStore`:

```ts
  workspaceFetched: boolean;
  getWorkspaceServiceIds: () => string[];
  fetchWorkspaceServices: (
    workspaceSlug: string,
    workspaceId: string,
    projectId?: string
  ) => Promise<IService[] | undefined>;
  createWorkspaceService: (workspaceSlug: string, workspaceId: string, data: Partial<IService>) => Promise<IService>;
  addProjectLink: (
    workspaceSlug: string,
    workspaceId: string,
    serviceId: string,
    projectId: string
  ) => Promise<IServiceProjectLink>;
  removeProjectLink: (workspaceSlug: string, workspaceId: string, serviceId: string, linkId: string) => Promise<void>;
  transferOwner: (workspaceSlug: string, workspaceId: string, serviceId: string, projectId: string) => Promise<IService>;
```

2b. Class state + `makeObservable` entries:

```ts
  workspaceFetched: boolean = false;
  // in makeObservable:
  workspaceFetched: observable.ref,
```

2c. Implementation:

```ts
getWorkspaceServiceIds = computedFn(() => {
  if (!this.workspaceFetched) return [];
  return sortBy(Object.values(this.serviceMap), [(s) => s.sort_order]).map((s) => s.id);
});

fetchWorkspaceServices = async (workspaceSlug: string, workspaceId: string, projectId?: string) => {
  try {
    this.loader = true;
    const [services, dependencies, links] = await Promise.all([
      this.serviceService.getWorkspaceServices(workspaceSlug, projectId ? { project: projectId } : undefined),
      this.serviceService.getWorkspaceDependencies(workspaceSlug),
      this.serviceService.getWorkspaceWorkItemLinks(workspaceSlug),
    ]);
    runInAction(() => {
      services.forEach((s) => set(this.serviceMap, [s.id], s));
      dependencies.forEach((d) => set(this.dependencyMap, [d.id], d));
      links.forEach((l) => set(this.workItemLinkMap, [l.id], l));
      this.workspaceFetched = true;
      this.loader = false;
    });
    return services;
  } catch {
    runInAction(() => {
      this.loader = false;
    });
    return undefined;
  }
};

createWorkspaceService = async (workspaceSlug: string, _workspaceId: string, data: Partial<IService>) => {
  const service = await this.serviceService.createWorkspaceService(workspaceSlug, data);
  runInAction(() => {
    set(this.serviceMap, [service.id], service);
  });
  return service;
};

addProjectLink = async (workspaceSlug: string, _workspaceId: string, serviceId: string, projectId: string) => {
  const link = await this.serviceService.addProjectLink(workspaceSlug, serviceId, projectId);
  const service = this.getServiceById(serviceId);
  if (service) {
    runInAction(() => {
      set(this.serviceMap, [serviceId], { ...service, project_links: [...service.project_links, link] });
    });
  }
  return link;
};

removeProjectLink = async (workspaceSlug: string, _workspaceId: string, serviceId: string, linkId: string) => {
  await this.serviceService.removeProjectLink(workspaceSlug, serviceId, linkId);
  const service = this.getServiceById(serviceId);
  if (service) {
    runInAction(() => {
      set(this.serviceMap, [serviceId], {
        ...service,
        project_links: service.project_links.filter((l) => l.id !== linkId),
      });
    });
  }
};

transferOwner = async (workspaceSlug: string, _workspaceId: string, serviceId: string, projectId: string) => {
  const updated = await this.serviceService.transferServiceOwner(workspaceSlug, serviceId, projectId);
  runInAction(() => {
    set(this.serviceMap, [serviceId], updated);
  });
  return updated;
};
```

Note: the existing project-scoped `fetchServices` filters `getProjectServiceIds` by `s.project_id === projectId`; the workspace fetch intentionally does NOT filter (server scoping is authoritative).

2d. Make the project-scoped service lookup link-aware (pickers show owner + consumer services after the compat response starts returning cross-project links). Replace the filter inside `getProjectServiceIds`:

```ts
getProjectServiceIds = computedFn((projectId: string) => {
  if (!this.fetchedMap[projectId]) return null;
  const services = sortBy(
    Object.values(this.serviceMap).filter(
      (s) => s.project_id === projectId || s.project_links?.some((l) => l.project_id === projectId)
    ),
    [(s) => s.sort_order]
  );
  return services.map((s) => s.id);
});
```

Make the same replacement inside `getFilteredServiceIds` (same predicate, then existing filter/order pipeline).

- [ ] **Step 3: Checks + commit**

Run: `pnpm check && pnpm --filter=web test -- service.helpers`
Expected: PASS (types clean).

```bash
git add apps/web/core/services/service.service.ts apps/web/core/store/service.store.ts
git commit -m "feat(web): workspace service client and store state"
```

### Task 3: Owner-project selector in the form + workspace create modal

**Files:**

- Modify: `apps/web/core/components/services/service-form.tsx`
- Create: `apps/web/core/components/services/workspace/create-service-modal.tsx`

- [ ] **Step 1: Extend `ServiceForm` with an optional owner-project field**

Add to `Props`:

```ts
  ownerProjectField?: {
    value: string;
    onChange: (projectId: string) => void;
    projects: { id: string; name: string }[];
    disabled?: boolean;
  };
```

Render it above the name field (inside the `space-y-3` block, before the name `Controller`):

```tsx
{
  ownerProjectField && (
    <div className="space-y-1">
      <label htmlFor="owner_project" className="text-11 font-medium text-secondary">
        {t("service.fields.owner_project")}
      </label>
      <CustomSelect
        value={ownerProjectField.value}
        onChange={(val: string) => ownerProjectField.onChange(val)}
        disabled={ownerProjectField.disabled}
        label={
          <span className="flex items-center gap-2 py-0.5 text-12">
            {ownerProjectField.projects.find((p) => p.id === ownerProjectField.value)?.name ?? (
              <span className="text-secondary">{t("service.select_owner_project")}</span>
            )}
          </span>
        }
      >
        {ownerProjectField.projects.map((project) => (
          <CustomSelect.Option key={project.id} value={project.id}>
            <span className="truncate">{project.name}</span>
          </CustomSelect.Option>
        ))}
      </CustomSelect>
    </div>
  );
}
```

Destructure `ownerProjectField` from `props` in the component signature.

- [ ] **Step 2: Create the workspace create modal**

`apps/web/core/components/services/workspace/create-service-modal.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IService } from "@plane/types";
// ui
import { ModalCore } from "@plane/ui";
// components
import { ServiceForm } from "@/components/services/service-form";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  /** Pre-selected owner project (e.g. the project filter of the catalog). */
  defaultOwnerProjectId?: string;
};

export const WorkspaceCreateServiceModal = observer(function WorkspaceCreateServiceModal(props: Props) {
  const { isOpen, onClose, workspaceSlug, defaultOwnerProjectId } = props;
  // states
  const [ownerProjectId, setOwnerProjectId] = useState(defaultOwnerProjectId ?? "");
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  const { createWorkspaceService } = useService();
  const { currentWorkspace } = useWorkspace();
  const { allowPermissions } = useUserPermissions();

  // list only projects the user may create services in (member+ or ws admin)
  const projects = useMemo(
    () =>
      (workspaceProjectIds ?? [])
        .map((id) => getProjectById(id))
        .filter((p): p is NonNullable<typeof p> => Boolean(p))
        .filter((p) =>
          allowPermissions(
            [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
            EUserPermissionsLevel.PROJECT,
            workspaceSlug,
            p.id
          )
        )
        .map((p) => ({ id: p.id, name: p.name })),
    [workspaceProjectIds, getProjectById, allowPermissions, workspaceSlug]
  );

  useEffect(() => {
    if (!ownerProjectId && projects.length > 0) setOwnerProjectId(projects[0].id);
  }, [ownerProjectId, projects]);

  useEffect(() => {
    if (isOpen && defaultOwnerProjectId) setOwnerProjectId(defaultOwnerProjectId);
  }, [isOpen, defaultOwnerProjectId]);

  const handleFormSubmit = async (values: Partial<IService>) => {
    if (!currentWorkspace?.id || !ownerProjectId) {
      throw new Error("Owner project is required");
    }
    await createWorkspaceService(workspaceSlug, currentWorkspace.id, {
      ...values,
      owner_project_id: ownerProjectId,
    });
    setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: "Service created successfully." });
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} width="lg" className="max-w-3xl">
      <ServiceForm
        handleFormSubmit={handleFormSubmit}
        handleClose={onClose}
        status={false}
        projectId={ownerProjectId}
        workspaceSlug={workspaceSlug}
        ownerProjectField={{ value: ownerProjectId, onChange: setOwnerProjectId, projects }}
      />
    </ModalCore>
  );
});
```

Notes for the executor:

- `ModalCore` import path/props: copy the exact wrapper used by the existing service modal (`apps/web/core/components/services/modal/` — read it and mirror the shell; only the form content and submit handler differ). If the existing modal uses a different modal primitive, use it here verbatim.
- `createWorkspaceService` signature is `(workspaceSlug, workspaceId, data)` (workspaceId is accepted and ignored server-side).

- [ ] **Step 3: Add the i18n keys**

Locate the service namespace file: `grep -rln '"service"' packages/i18n/src/locales/en` → open the file that defines `service.title`. Add under the `service` object:

```json
"catalog_title": "Service catalog",
"fields.owner_project": "Owner project",
"select_owner_project": "Select owner project"
```

(If the file uses nested objects, place them as nested keys `fields: { owner_project: ... }` etc., matching the surrounding style.)

- [ ] **Step 4: Checks + commit**

Run: `pnpm check`
Expected: clean.

```bash
git add apps/web/core/components/services/service-form.tsx apps/web/core/components/services/workspace/create-service-modal.tsx packages/i18n/src/locales/en
git commit -m "feat(web): owner project selector and workspace create service modal"
```

---

### Task 4: Routes, navigation, redirects, catalog + detail pages

**Files:**

- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`
- Modify: `apps/web/core/components/navigation/use-navigation-items.ts`
- Modify: `apps/web/app/routes/core.ts`
- Create: `apps/web/app/routes/redirects/core/project-services.tsx`
- Create: `apps/web/app/routes/redirects/core/project-service-detail.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/services/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/services/[serviceId]/page.tsx`
- Create: `apps/web/core/components/services/workspace/workspace-services-list-view.tsx`
- Delete: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/` (old list+detail routes)
- Modify: `apps/web/core/components/services/detail/root.tsx` (fetched fallback for workspace routes)

- [ ] **Step 1: Sidebar nav item**

In `packages/constants/src/workspace.ts` add to `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`:

```ts
  services: {
    key: "services",
    labelTranslationKey: "service.catalog_title",
    href: `/services/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

and append `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["services"],` to `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS` (after `home`).

In `sidebar-item.tsx` add `"services"` to the `staticItems` array.

In `helper.tsx` add a case:

```tsx
    case "services":
      return <ServerOutline className={cn("size-4 flex-shrink-0", className)} />;
```

and add `ServerOutline` to the `@makeplane/propel/icons` import list.

- [ ] **Step 2: Project sidebar shortcut → filtered catalog**

In `use-navigation-items.ts` (line ~78) change the services href:

```ts
        href: `/${workspaceSlug}/services?project=${projectId}`,
```

- [ ] **Step 3: Route registration + redirects**

In `apps/web/app/routes/core.ts` replace the existing Service Detail + Services List blocks (lines ~178-193) with:

```ts
          // Workspace Service Catalog
          layout("./(all)/[workspaceSlug]/(projects)/services/layout.tsx", [
            route(":workspaceSlug/services", "./(all)/[workspaceSlug]/(projects)/services/page.tsx"),
            route(
              ":workspaceSlug/services/:serviceId",
              "./(all)/[workspaceSlug]/(projects)/services/[serviceId]/page.tsx"
            ),
          ]),
```

and add to the redirects section (near the other legacy redirects, e.g. after the inbox redirect at line ~434):

```ts
  // Legacy project service routes → workspace catalog/detail
  route(":workspaceSlug/projects/:projectId/services", "routes/redirects/core/project-services.tsx"),
  route(
    ":workspaceSlug/projects/:projectId/services/:serviceId",
    "routes/redirects/core/project-service-detail.tsx"
  ),
```

`apps/web/app/routes/redirects/core/project-services.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/project-services";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const { workspaceSlug, projectId } = params;
  throw redirect(`/${workspaceSlug}/services?project=${projectId}`);
};

export default function ProjectServicesRedirect() {
  return null;
}
```

`apps/web/app/routes/redirects/core/project-service-detail.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { redirect } from "react-router";
import type { Route } from "./+types/project-service-detail";

export const clientLoader = ({ params }: Route.ClientLoaderArgs) => {
  const { workspaceSlug, serviceId } = params;
  throw redirect(`/${workspaceSlug}/services/${serviceId}`);
};

export default function ProjectServiceDetailRedirect() {
  return null;
}
```

Delete the old route files: `git rm -r "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services"`.

- [ ] **Step 4: `ServiceDetailRoot` fetched fallback**

In `apps/web/core/components/services/detail/root.tsx`:

```ts
type Props = {
  serviceId: string;
  /** When rendering outside a project route (workspace catalog detail). */
  fetched?: boolean;
};
```

```ts
const hasFetched = pid ? fetchedMap[pid] : props.fetched;
```

and retry:

```ts
const handleRetry = () => {
  if (!workspaceSlug || !workspaceId) return;
  if (pid) void fetchServices(workspaceSlug.toString(), workspaceId, pid);
  else void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId);
};
```

Add `fetchWorkspaceServices` to the `useService()` destructure.

- [ ] **Step 5: Layout + pages**

`services/layout.tsx`:

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
import { ServerOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceServicesBreadcrumbs = observer(function WorkspaceServicesBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug, serviceId } = useParams();
  const { t } = useTranslation();
  const { getServiceById } = useService();
  const service = serviceId ? getServiceById(serviceId.toString()) : null;

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("service.catalog_title")}
                href={`/${workspaceSlug}/services/`}
                icon={<ServerOutline className="h-4 w-4 text-tertiary" />}
                isLast={!serviceId}
              />
            }
            isLast={!serviceId}
          />
          {serviceId && <Breadcrumbs.Item component={<BreadcrumbLink label={service?.name ?? ""} isLast />} isLast />}
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceServicesLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceServicesBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

`services/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceServicesListView } from "@/components/services/workspace/workspace-services-list-view";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

function WorkspaceServicesPage() {
  const { workspaceSlug } = useParams();
  const searchParams = useSearchParams();
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();
  const { currentWorkspace } = useWorkspace();
  const { fetchWorkspaceServices } = useService();
  const workspaceId = currentWorkspace?.id;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId) return;
    void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId, projectFilter);
  }, [workspaceSlug, workspaceId, projectFilter, fetchWorkspaceServices]);

  return (
    <>
      <PageHead title={t("service.catalog_title")} />
      <WorkspaceServicesListView projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceServicesPage);
```

`services/[serviceId]/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// components
import { PageHead } from "@/components/core/page-title";
import { ServiceDetailRoot } from "@/components/services";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

function WorkspaceServiceDetailPage() {
  const { workspaceSlug, serviceId } = useParams();
  const { currentWorkspace } = useWorkspace();
  const { workspaceFetched, fetchWorkspaceServices, getServiceById } = useService();
  const service = getServiceById(serviceId?.toString() ?? "");
  const workspaceId = currentWorkspace?.id;

  useEffect(() => {
    if (!workspaceSlug || !workspaceId || workspaceFetched) return;
    void fetchWorkspaceServices(workspaceSlug.toString(), workspaceId);
  }, [workspaceSlug, workspaceId, workspaceFetched, fetchWorkspaceServices]);

  return (
    <>
      <PageHead title={service ? `${service.name} - Service` : undefined} />
      <ServiceDetailRoot serviceId={serviceId?.toString() ?? ""} fetched={workspaceFetched} />
    </>
  );
}

export default observer(WorkspaceServiceDetailPage);
```

`workspace-services-list-view.tsx` (mirrors the empty/loading behavior of `services-list-view.tsx` but catalog-scoped):

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// ui
import { CustomSelect } from "@plane/ui";
// components
import { ServicesBoardRow } from "@/components/services/board/services-board-row";
import { WorkspaceCreateServiceModal } from "@/components/services/workspace/create-service-modal";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  projectFilter?: string;
};

export const WorkspaceServicesListView = observer(function WorkspaceServicesListView({ projectFilter }: Props) {
  // router
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWorkspaceServiceIds, getServiceById, loader, workspaceFetched } = useService();
  const { workspaceProjectIds, getProjectById } = useProject();
  // states
  const [search, setSearch] = useState("");
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  const serviceIds = getWorkspaceServiceIds();
  const filteredIds = useMemo(
    () =>
      serviceIds.filter((id) => {
        const service = getServiceById(id);
        return !search || (service?.name ?? "").toLowerCase().includes(search.toLowerCase());
      }),
    [serviceIds, search, getServiceById]
  );

  const handleProjectFilter = (projectId: string) => {
    if (!workspaceSlug) return;
    router.push(projectId ? `/${workspaceSlug}/services?project=${projectId}` : `/${workspaceSlug}/services`);
  };

  if (loader && !workspaceFetched) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      <div className="flex items-center justify-between gap-2 border-b border-subtle px-3 py-2">
        <div className="flex items-center gap-2">
          <CustomSelect
            value={projectFilter ?? ""}
            onChange={handleProjectFilter}
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
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("service.catalog_search_placeholder")}
            className="h-7 rounded-sm border border-subtle bg-layer-1 px-2 text-12 text-primary outline-none"
          />
        </div>
        <Button variant="primary" size="sm" onClick={() => setIsCreateModalOpen(true)}>
          {t("service.add")}
        </Button>
      </div>
      {filteredIds.length === 0 ? (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {filteredIds.map((id) => (
            <ServicesBoardRow key={id} serviceId={id} />
          ))}
        </div>
      )}
      {workspaceSlug && (
        <WorkspaceCreateServiceModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          workspaceSlug={workspaceSlug.toString()}
          defaultOwnerProjectId={projectFilter}
        />
      )}
    </div>
  );
});
```

Add i18n keys: `catalog_all_projects: "All projects"`, `catalog_search_placeholder: "Search services"` in the same English namespace as Task 3.

- [ ] **Step 6: Checks + commit**

Run: `pnpm check`
Expected: clean. Fix route typegen if React Router complains (`pnpm --filter=web dev` generates types on demand; the repo's typegen runs via build/dev — run `pnpm --filter=web build` if `./+types` imports fail).

```bash
git add packages/constants/src/workspace.ts apps/web/core/components/workspace/sidebar/sidebar-item.tsx \
  apps/web/core/components/workspace/sidebar/helper.tsx apps/web/core/components/navigation/use-navigation-items.ts \
  apps/web/app/routes/core.ts apps/web/app/routes/redirects/core/project-services.tsx \
  apps/web/app/routes/redirects/core/project-service-detail.tsx \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/services" \
  apps/web/core/components/services/workspace/workspace-services-list-view.tsx \
  apps/web/core/components/services/detail/root.tsx packages/i18n/src/locales/en
git rm -r "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services"
git commit -m "feat(web): workspace service catalog routes and navigation"
```

### Task 5: URL wiring (board row, delete modal, peek, war rooms) + workspace graph scope

**Files:**

- Modify: `apps/web/core/components/services/board/services-board-row.tsx`
- Modify: `apps/web/core/components/services/delete-service-modal.tsx`
- Modify: `apps/web/core/store/service.store.ts` (deleteWorkspaceService + getWorkspaceGraphData)
- Modify: `apps/web/core/hooks/use-service-peek-overview-redirection.ts`
- Modify: `apps/web/core/components/war-rooms/room/service-map.tsx` (line ~93)
- Modify: `apps/web/core/components/services/graph/service-graph.tsx`
- Modify: `apps/web/core/components/services/workspace/workspace-services-list-view.tsx` (graph toggle)

- [ ] **Step 1: Board row → workspace detail + owner badge**

In `services-board-row.tsx`:

```tsx
// add hook
import { useProject } from "@/hooks/store/use-project";
// inside component
const { getProjectById } = useProject();
const ownerProject = service.owner_project_id ? getProjectById(service.owner_project_id) : undefined;
const serviceLink = `/${workspaceSlug?.toString()}/services/${service.id}`;
```

Render the badge next to the criticality chip:

```tsx
{
  ownerProject && (
    <span className="shrink-0 rounded-xs border border-subtle px-1 text-10 font-medium text-tertiary">
      {ownerProject.identifier}
    </span>
  );
}
```

- [ ] **Step 2: Store — workspace delete + workspace graph data**

Extract the local cleanup from `deleteService` into a private helper and reuse:

```ts
  private removeLocalService = (serviceId: string) => {
    delete this.serviceMap[serviceId];
    Object.values(this.dependencyMap).forEach((d) => {
      if (d.from_service_id === serviceId || d.to_service_id === serviceId) delete this.dependencyMap[d.id];
    });
    Object.values(this.workItemLinkMap).forEach((l) => {
      if (l.service_id === serviceId) delete this.workItemLinkMap[l.id];
    });
    if (this.peekService?.serviceId === serviceId) this.peekService = undefined;
  };

  deleteWorkspaceService = async (workspaceSlug: string, _workspaceId: string, serviceId: string) => {
    await this.serviceService.deleteWorkspaceService(workspaceSlug, serviceId);
    runInAction(() => {
      this.removeLocalService(serviceId);
    });
  };
```

(`deleteService` keeps its behavior, calling `this.removeLocalService(serviceId)` inside its `runInAction`.) Add both to `IServiceStore` and `makeObservable`.

```ts
getWorkspaceGraphData = computedFn((projectId?: string): TServiceGraphData => {
  const serviceIds = this.getWorkspaceServiceIds();
  const services = serviceIds
    .map((id) => this.serviceMap[id])
    .filter((service): service is IService => Boolean(service))
    .filter((service) => !projectId || service.project_links?.some((l) => l.project_id === projectId));
  const visible = new Set(services.map((s) => s.id));
  const dependencies = Object.values(this.dependencyMap).filter(
    (d) => visible.has(d.from_service_id) || visible.has(d.to_service_id)
  );
  const endpoints = new Map<string, IService | ILiteService>();
  for (const dep of dependencies) {
    if (dep.from_service && !visible.has(dep.from_service_id)) endpoints.set(dep.from_service_id, dep.from_service);
    if (dep.to_service && !visible.has(dep.to_service_id)) endpoints.set(dep.to_service_id, dep.to_service);
  }
  const allServices: (IService | ILiteService)[] = [...services, ...endpoints.values()];
  const allIds = new Set(allServices.map((s) => s.id));
  const health: Record<string, IServiceHealthSnapshot> = {};
  services.forEach((service) => {
    const snapshot = this.healthMap[service.id];
    if (snapshot) health[service.id] = snapshot;
  });
  return {
    services: allServices,
    dependencies: dependencies.filter((d) => allIds.has(d.from_service_id) && allIds.has(d.to_service_id)),
    health,
  };
});
```

Add `getWorkspaceGraphData: (projectId?: string) => TServiceGraphData;` to the interface and import `ILiteService` into the store.

- [ ] **Step 3: Delete modal → canonical delete + workspace redirect**

In `delete-service-modal.tsx`:

```tsx
const { deleteWorkspaceService } = useService();
// handleDeletion:
const handleDeletion = async () => {
  if (!workspaceSlug || !currentWorkspace?.id) return;
  setIsDeleteLoading(true);
  try {
    await deleteWorkspaceService(workspaceSlug.toString(), currentWorkspace.id, data.id);
    if (serviceId) router.push(`/${workspaceSlug}/services`);
    setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: "Service deleted successfully." });
  } catch {
    setToast({ type: TOAST_TYPE.ERROR, title: "Error!", message: "Service could not be deleted. Please try again." });
  } finally {
    handleClose();
  }
};
```

(`projectId` from params is no longer required.)

- [ ] **Step 4: Peek/redirection + war room URL**

`use-service-peek-overview-redirection.ts`:

```ts
const serviceLink = `/${workspaceSlug}/services/${id}`;
```

`war-rooms/room/service-map.tsx` line ~93:

```tsx
window.open(`/${workspaceSlug}/services/${serviceId}`, "_blank", "noopener,noreferrer");
```

`peek-overview/view.tsx` line ~47 (consistency; peek only renders on project pages after this migration):

```ts
const serviceLink = `/${workspaceSlug}/services/${serviceId}`;
```

- [ ] **Step 5: Graph scope prop + catalog graph toggle**

`service-graph.tsx`:

```tsx
type Props = {
  scope?: "project" | "workspace";
  projectIdOverride?: string;
};

export const ServiceGraph = observer(function ServiceGraph({ scope = "project", projectIdOverride }: Props) {
```

Inside:

```tsx
  const pid = projectIdOverride ?? projectId?.toString();
  const { getGraphData, getWorkspaceGraphData, ... } = useService();
  const graphData: TServiceGraphData =
    scope === "workspace"
      ? getWorkspaceGraphData(projectIdOverride)
      : pid
        ? getGraphData(pid)
        : { services: [], dependencies: [], health: {} };
```

`handleReLayout`/`handleConnect`/`handleEdgeDelete` keep using `pid` — with `projectIdOverride` set on the workspace catalog they resolve to the filtered project (compat endpoints accept it).

In `workspace-services-list-view.tsx` add a view toggle next to the buttons:

```tsx
  const [layout, setLayout] = useState<"board" | "graph">("board");
  // render:
      {layout === "graph" && projectFilter ? (
        <div className="min-h-0 w-full flex-1">
          <ServiceGraph scope="workspace" projectIdOverride={projectFilter} />
        </div>
      ) : (
        /* existing board block */
      )}
```

and a small toggle button group (reuse `SERVICE_VIEW_LAYOUTS` + `ServiceLayoutIcon` from `components/services/service-layout-icon`) where the graph button is disabled when `!projectFilter` (tooltip: `t("service.graph.connect_hint")` is fine as a placeholder title, or add `catalog_select_project_for_graph` key).

- [ ] **Step 6: Checks + commit**

Run: `pnpm check && pnpm --filter=web test`
Expected: PASS.

```bash
git add apps/web/core/components/services apps/web/core/store/service.store.ts \
  apps/web/core/hooks/use-service-peek-overview-redirection.ts apps/web/core/components/war-rooms/room/service-map.tsx
git commit -m "feat(web): workspace service URLs, delete flow, workspace graph scope"
```

---

### Task 6: Linked projects section + lite dependency rendering

**Files:**

- Create: `apps/web/core/components/services/detail/project-links.tsx`
- Modify: `apps/web/core/components/services/detail/root.tsx` (render section)
- Modify: `apps/web/core/components/services/detail/dependencies.tsx` (lite rows)
- Modify: `apps/web/core/components/services/graph/service-node.tsx` + `use-graph-layout.ts` (lite nodes)
- Modify: `apps/web/core/components/services/detail/index.ts` (export new component)

- [ ] **Step 1: Linked projects section**

`detail/project-links.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { CustomSelect } from "@plane/ui";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
};

export const ServiceProjectLinks = observer(function ServiceProjectLinks({ serviceId }: Props) {
  // router
  const { workspaceSlug } = useParams();
  // states
  const [selectedProjectId, setSelectedProjectId] = useState("");
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, addProjectLink, removeProjectLink, transferOwner } = useService();
  const { workspaceProjectIds, getProjectById } = useProject();
  const { currentWorkspace } = useWorkspace();
  const { allowPermissions } = useUserPermissions();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const workspaceId = currentWorkspace?.id;
  const ownerProjectId = service.owner_project_id;
  const isWorkspaceAdmin = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE, slug);
  const isOwnerProjectAdmin = ownerProjectId
    ? allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.PROJECT, slug, ownerProjectId)
    : false;
  const canManageLinks = isWorkspaceAdmin || isOwnerProjectAdmin;
  const canAddForProject = (projectId: string) =>
    isWorkspaceAdmin ||
    isOwnerProjectAdmin ||
    allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.PROJECT, slug, projectId);
  const addableProjects = (workspaceProjectIds ?? [])
    .filter((id) => !service.project_links.some((l) => l.project_id === id))
    .filter((id) => canAddForProject(id))
    .map((id) => getProjectById(id))
    .filter((p): p is NonNullable<typeof p> => Boolean(p));

  const handleAdd = async () => {
    if (!slug || !workspaceId || !selectedProjectId) return;
    try {
      await addProjectLink(slug, workspaceId, serviceId, selectedProjectId);
      setSelectedProjectId("");
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: error instanceof Error ? error.message : t("service.project_links.add_error"),
      });
    }
  };

  const handleRemove = async (linkId: string) => {
    if (!slug || !workspaceId) return;
    try {
      await removeProjectLink(slug, workspaceId, serviceId, linkId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("error"), message: t("service.project_links.remove_error") });
    }
  };

  const handleTransfer = async (projectId: string) => {
    if (!slug || !workspaceId) return;
    try {
      await transferOwner(slug, workspaceId, serviceId, projectId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("error"), message: t("service.project_links.transfer_error") });
    }
  };

  return (
    <div className="mt-3 flex flex-col gap-2">
      {service.project_links.map((link) => {
        const project = getProjectById(link.project_id);
        const isOwner = link.link_type === "owner";
        return (
          <div
            key={link.id}
            className="flex items-center justify-between gap-2 rounded-sm border border-subtle px-2 py-1.5"
          >
            <span className="flex min-w-0 items-center gap-2">
              <span className="truncate text-12 text-primary">{project?.name ?? link.project_id}</span>
              {isOwner && (
                <span className="shrink-0 rounded-xs border border-accent-strong px-1 text-10 text-accent-primary">
                  {t("service.project_links.owner")}
                </span>
              )}
            </span>
            <span className="flex shrink-0 items-center gap-2">
              {isOwner && isWorkspaceAdmin && addableProjects.length > 0 && (
                <CustomSelect
                  value=""
                  onChange={handleTransfer}
                  label={<span className="text-11 text-tertiary">{t("service.project_links.transfer")}</span>}
                  noChevron
                >
                  {addableProjects.map((project) => (
                    <CustomSelect.Option key={project.id} value={project.id}>
                      <span className="truncate">{project.name}</span>
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
              )}
              {!isOwner && canManageLinks && (
                <button
                  type="button"
                  onClick={() => handleRemove(link.id)}
                  className="text-11 text-tertiary hover:text-primary"
                >
                  {t("remove")}
                </button>
              )}
            </span>
          </div>
        );
      })}
      {addableProjects.length > 0 && (
        <div className="flex items-center gap-2">
          <CustomSelect
            value={selectedProjectId}
            onChange={setSelectedProjectId}
            className="h-7.5 min-w-0 grow basis-0"
            label={
              <span className="text-12">
                {selectedProjectId ? (
                  getProjectById(selectedProjectId)?.name
                ) : (
                  <span className="text-secondary">{t("service.project_links.add_project")}</span>
                )}
              </span>
            }
          >
            {addableProjects.map((project) => (
              <CustomSelect.Option key={project.id} value={project.id}>
                <span className="truncate">{project.name}</span>
              </CustomSelect.Option>
            ))}
          </CustomSelect>
          <Button variant="secondary" size="sm" onClick={handleAdd} disabled={!selectedProjectId}>
            {t("add")}
          </Button>
        </div>
      )}
      <p className="text-11 text-tertiary">{t("service.project_links.hint")}</p>
    </div>
  );
});
```

Add i18n keys: `project_links.owner`, `project_links.transfer`, `project_links.add_project`, `project_links.hint`, `project_links.add_error`, `project_links.remove_error`, `project_links.transfer_error`.

Render it in `detail/root.tsx` under the description (before `ServiceWorkItems`):

```tsx
<ServiceProjectLinks serviceId={serviceId} />
```

and export from `detail/index.ts` (`export * from "./project-links";`) plus add it to the `@/components/services` barrel if that barrel re-exports detail components (check `components/services/index.ts`).

- [ ] **Step 2: Lite rows in the dependencies section**

In `detail/dependencies.tsx` `renderEdgeRow`, use the embedded endpoint objects:

```tsx
const renderEdgeRow = (dependency: IServiceDependency, otherId: string) => {
  const embedded = dependency.from_service_id === otherId ? dependency.from_service : dependency.to_service;
  const isLite = embedded && isLiteService(embedded);
  const name = embedded?.name ?? otherId;
  return (
    <div
      key={dependency.id}
      className="flex items-center justify-between gap-2 rounded-sm border border-subtle px-2 py-1.5"
    >
      <span className="flex min-w-0 items-center gap-1.5" title={name}>
        {isLite && <LockIcon className="h-3 w-3 shrink-0 text-tertiary" />}
        <span className={cn("truncate text-12", isLite ? "text-tertiary" : "text-primary")}>{name}</span>
        {isLite && <span className="shrink-0 text-10 text-tertiary">{t("service.lite.restricted")}</span>}
      </span>
      {!isLite && (
        <button
          type="button"
          onClick={() => handleRemove(dependency.id)}
          className="shrink-0 text-11 text-tertiary hover:text-primary"
        >
          {t("remove")}
        </button>
      )}
    </div>
  );
};
```

Update the two call sites:

```tsx
outgoing.map((d) => renderEdgeRow(d, d.to_service_id));
incoming.map((d) => renderEdgeRow(d, d.from_service_id));
```

Imports: `import { cn } from "@plane/utils";`, `import { LockIcon } from "@makeplane/propel/icons";` (verify the exact lock icon export name used elsewhere in the repo; grep `LockIcon` or use `LockOutline`), `import { isLiteService } from "@/services/service.helpers";`, `import type { IServiceDependency } from "@plane/types";`. Add i18n key `service.lite.restricted: "Restricted"`.

Also the add-candidate dropdown stays `getProjectServiceIds(pid)` (owner project) — cross-project targets require the catalog; that is acceptable for v1 (document in the PR).

- [ ] **Step 3: Lite nodes in the graph**

`service-node.tsx`:

```tsx
export type TServiceNodeData = {
  service: IService | ILiteService;
  accessLevel?: "full" | "lite";
  statusLabel: string;
  criticalityLabel: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
  dimmed?: boolean;
};
```

In the card body, when `data.accessLevel === "lite"`: add `lock` icon before the name and a `cursor-not-allowed` class plus `title={t(...)}`-free tooltip via native `title` attr ("Restricted — ask a member of this service's project" is already user-facing copy from the design; hardcode English for v1 or add `service.lite.tooltip`). Keep `ServiceHealthDot` but the node is visually muted (`opacity-80 border-dashed`).

`use-graph-layout.ts`: when building each node's `data`, include `accessLevel: isLiteService(service) ? "lite" : "full"` (import the helper; the services list may now contain `ILiteService` entries — widen the local typing accordingly).

`service-graph.tsx` `handleNodeClick` already no-ops for lite endpoints because `getServiceById(id)` returns null (lite services are not in `serviceMap`) — no change needed; keep it that way.

- [ ] **Step 4: Checks + tests + commit**

Run: `pnpm check && pnpm --filter=web test`
Expected: PASS.

```bash
git add apps/web/core/components/services packages/i18n/src/locales/en
git commit -m "feat(web): linked projects section and lite dependency rendering"
```

---

### Task 7: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Static checks + unit tests**

```bash
pnpm check
pnpm --filter=web test
```

Expected: green.

- [ ] **Step 2: Web build + deploy per `AGENTS.md`**

Prod (port 3000) serves a static build; rebuild and restart:

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build succeeds (VITE_API_BASE_URL baked per current mode), service active.

- [ ] **Step 3: Manual smoke (LAN/catalog mode)**

1. Workspace sidebar shows **Service catalog**; opening it lists services visible to the current user with owner-project badges.
2. Filter by project; create a service (owner selector defaults to the filtered project).
3. Open a service detail from the workspace catalog; work items and dependencies render; **Linked projects** shows owner + consumers; add a consumer link as that project's admin; remove it.
4. As workspace admin, transfer owner; badges/rows update.
5. Member of a project with no link sees no stranger services; a dependency endpoint from another team renders lite with lock and no navigation (both in the dependency list and the graph when a project filter is selected).
6. Old URLs redirect: `/projects/:pid/services` → catalog with `?project=`, `/projects/:pid/services/:id` → workspace detail.
7. Issue/intake/war-room service pickers still list services linked to that project (compat endpoint).

- [ ] **Step 4: Follow-up (new spec, not this plan)**

Remove legacy client methods + compat endpoints usage in a cleanup release; add `Service catalog` docs topic update; consider workspace graph on "All projects" (currently requires a project filter).

---

## Plan self-review notes (already applied)

- Spec coverage §4: catalog routes/nav (Task 4), redirects (Task 4), linked-projects UI + transfer (Task 6), lite nodes/list (Tasks 5-6), owner badge (Task 5), pickers unchanged via compat (no task needed), form owner selector (Task 3).
- Type consistency: `IService.client project_id` is nullable; `ILiteService` used in `TServiceGraphData`; `isLiteService` used in deps rows and graph layout; store methods named `createWorkspaceService`/`deleteWorkspaceService`/`addProjectLink`/`removeProjectLink`/`transferOwner`/`fetchWorkspaceServices`/`getWorkspaceServiceIds`/`getWorkspaceGraphData` throughout.
- Known acceptable deviations: icon component names (`LockIcon` vs `LockOutline`) and modal shell primitives must be matched to the repo (`grep` first); i18n key nesting follows the existing service namespace file; route `+types` may need a dev/build run to regenerate.

<!-- plan-w3-end -->
