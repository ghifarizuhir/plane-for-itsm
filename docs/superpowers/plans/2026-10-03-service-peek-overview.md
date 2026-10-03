# Service Peek Overview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a service in the Services board or graph opens a peek overview panel (side-peek / modal / full-screen) over the current view, mirroring the work-item peek, instead of navigating to the detail page.

**Architecture:** A global `peekService` observable + `setPeekService` action on `ServicesStore` holds `{ workspaceSlug, projectId, serviceId }`. `ServicePeekOverview` (rendered in `ServicesListView`, portaled to `#full-screen-portal`) reads it, renders the existing detail sections, and closes via X / ESC / outside click. Board rows and graph nodes call a new redirection hook that peeks on desktop and routes to the detail page on mobile. The URL is never modified.

**Tech Stack:** React 19, MobX (`mobx` / `mobx-react`), React Flow v12 (graph), Tailwind + Plane tokens, Vitest (node env, `core/**/*.test.ts`), React Router v7 SPA mode (`next/navigation` shims).

**Spec:** `docs/superpowers/specs/2026-10-03-service-peek-overview-design.md`

---

## Pre-flight (Task 0): Existing uncommitted graph changes

`git status` currently shows uncommitted dark-mode/graph work from a previous task:

- `apps/web/core/components/services/graph/service-graph-canvas.tsx`
- `apps/web/core/components/services/graph/service-node.tsx`
- `apps/web/core/components/services/graph/use-graph-layout.ts`
- `apps/web/core/components/services/health/health-config.ts`
- `apps/web/core/components/services/services-list-view.tsx`
- `apps/web/styles/globals.css`

Task 5 also modifies `services-list-view.tsx`. **Do not use `git add .` anywhere in this plan.** Recommended: commit the existing work first so the peek commits stay clean.

- [ ] **Step 0.1: Confirm with the user whether to commit the existing graph dark-mode work now**

If yes:

```bash
git add apps/web/core/components/services/graph/service-graph-canvas.tsx \
  apps/web/core/components/services/graph/service-node.tsx \
  apps/web/core/components/services/graph/use-graph-layout.ts \
  apps/web/core/components/services/health/health-config.ts \
  apps/web/core/components/services/services-list-view.tsx \
  apps/web/styles/globals.css
git commit -m "fix(web): polish services graph dark mode and layout"
```

If the user prefers to keep them uncommitted, note that the Task 5 commit for `services-list-view.tsx` will include those prior changes; get explicit consent before committing.

---

## File structure

**New files**

| File                                                           | Responsibility                                                         |
| -------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `apps/web/core/hooks/use-service-peek-overview-redirection.ts` | Desktop → `setPeekService`; mobile → `router.push` to the detail route |
| `apps/web/core/components/services/peek-overview/root.tsx`     | Route guard, data ensure, status derivation                            |
| `apps/web/core/components/services/peek-overview/header.tsx`   | Close, open full page, mode dropdown, quick actions                    |
| `apps/web/core/components/services/peek-overview/view.tsx`     | Panel chrome, modes, ESC/outside-click, body composition               |
| `apps/web/core/components/services/peek-overview/index.ts`     | Barrel                                                                 |
| `apps/web/core/store/service.store.test.ts`                    | Peek state unit tests                                                  |

**Modified files**

| File                                                             | Change                                                                                |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `apps/web/core/store/service.store.ts`                           | `TServicePeek`, `peekService`, `setPeekService`, `getIsServicePeeked`, delete cleanup |
| `apps/web/core/hooks/use-peek-overview-outside-click.tsx`        | Optional `targetElementId` parameter                                                  |
| `apps/web/core/components/services/detail/sidebar.tsx`           | Optional `layout?: "sidebar" \| "stacked"`                                            |
| `apps/web/core/components/services/services-list-view.tsx`       | Render `ServicePeekOverview`                                                          |
| `apps/web/core/components/services/index.ts`                     | Export the peek barrel                                                                |
| `apps/web/core/components/services/board/services-board-row.tsx` | `ControlLink` + redirect hook                                                         |
| `apps/web/core/components/services/graph/service-graph.tsx`      | Node click uses the redirect hook                                                     |

---

### Task 1: Store peek state (TDD)

**Files:**

- Create: `apps/web/core/store/service.store.test.ts`
- Modify: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/store/service.store.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
// store
import { ServicesStore } from "./service.store";

const makeStore = () => {
  const store = new ServicesStore({} as never);
  const serviceService = {
    deleteService: vi.fn(async () => undefined),
  };
  (store as unknown as { serviceService: typeof serviceService }).serviceService = serviceService;
  return { store, serviceService };
};

describe("ServicesStore peek state", () => {
  it("sets, clears, and matches the peeked service", () => {
    const { store } = makeStore();

    expect(store.peekService).toBeUndefined();
    expect(store.getIsServicePeeked("service-1")).toBe(false);

    store.setPeekService({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });

    expect(store.peekService).toEqual({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });
    expect(store.getIsServicePeeked("service-1")).toBe(true);
    expect(store.getIsServicePeeked("service-2")).toBe(false);

    store.setPeekService(undefined);
    expect(store.peekService).toBeUndefined();
  });
});

describe("ServicesStore.deleteService", () => {
  it("clears the peek when the peeked service is deleted", async () => {
    const { store, serviceService } = makeStore();
    store.setPeekService({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });

    await store.deleteService("acme", "workspace-1", "project-1", "service-1");

    expect(serviceService.deleteService).toHaveBeenCalledWith("acme", "workspace-1", "project-1", "service-1");
    expect(store.peekService).toBeUndefined();
  });

  it("keeps the peek when another service is deleted", async () => {
    const { store } = makeStore();
    store.setPeekService({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });

    await store.deleteService("acme", "workspace-1", "project-1", "service-2");

    expect(store.peekService).toEqual({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm --filter=web exec vitest run core/store/service.store.test.ts`
Expected: FAIL — `store.setPeekService is not a function` (and `peekService` undefined assertions later).

- [ ] **Step 3: Add the peek type and interface members**

In `apps/web/core/store/service.store.ts`, insert the type after the imports (right before `export interface IServiceStore {`):

```ts
export type TServicePeek = {
  workspaceSlug: string;
  projectId: string;
  serviceId: string;
};
```

Inside `IServiceStore`, add after `errorMap: Record<string, boolean>;`:

```ts
  peekService?: TServicePeek;
```

and after `getGraphData: (projectId: string) => TServiceGraphData;`:

```ts
getIsServicePeeked: (serviceId: string) => boolean;
```

and after `fetchServices: (...) => Promise<IService[] | undefined>;`:

```ts
  setPeekService: (peek?: TServicePeek) => void;
```

- [ ] **Step 4: Add the observable, action registration, and methods**

In `ServicesStore`, add the property after `errorMap: Record<string, boolean> = {};`:

```ts
  peekService: TServicePeek | undefined = undefined;
```

In `makeObservable`, add after `errorMap: observable,`:

```ts
      peekService: observable.ref,
```

and after `fetchServices: action,`:

```ts
      setPeekService: action,
```

Add the methods after `getGraphData` (before `fetchServices`):

```ts
getIsServicePeeked = computedFn((serviceId: string) => this.peekService?.serviceId === serviceId);

setPeekService = (peek?: TServicePeek) => {
  this.peekService = peek;
};
```

- [ ] **Step 5: Clear the peek on delete**

In `deleteService`, inside the existing `runInAction` block, add after the `workItemLinkMap` cleanup loop:

```ts
if (this.peekService?.serviceId === serviceId) this.peekService = undefined;
```

The block becomes:

```ts
runInAction(() => {
  delete this.serviceMap[serviceId];
  Object.values(this.dependencyMap).forEach((d) => {
    if (d.from_service_id === serviceId || d.to_service_id === serviceId) delete this.dependencyMap[d.id];
  });
  Object.values(this.workItemLinkMap).forEach((l) => {
    if (l.service_id === serviceId) delete this.workItemLinkMap[l.id];
  });
  if (this.peekService?.serviceId === serviceId) this.peekService = undefined;
});
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `pnpm --filter=web exec vitest run core/store/service.store.test.ts`
Expected: PASS — 3 tests.

- [ ] **Step 7: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/store/service.store.ts apps/web/core/store/service.store.test.ts
git commit -m "feat(web): add service peek state to services store"
```

---

### Task 2: Extend the outside-click hook with `targetElementId`

**Files:**

- Modify: `apps/web/core/hooks/use-peek-overview-outside-click.tsx`

- [ ] **Step 1: Replace the hook signature and target check**

Replace lines 10-15:

```ts
const usePeekOverviewOutsideClickDetector = (
  ref: React.RefObject<HTMLElement | null>,
  callback: () => void,
  issueId: string,
  excludePreventionElementIds?: string[]
) => {
```

with:

```ts
const usePeekOverviewOutsideClickDetector = (
  ref: React.RefObject<HTMLElement | null>,
  callback: () => void,
  peekId: string,
  excludePreventionElementIds?: string[],
  targetElementId?: string
) => {
  const resolvedTargetElementId = targetElementId ?? `issue-${peekId}`;
```

- [ ] **Step 2: Use the resolved id in the walk-up check**

Replace lines 35-43:

```ts
// check if the click target is the current issue element or its children
let targetElement: HTMLElement | null = event.target;
while (targetElement) {
  if (targetElement.id === `issue-${issueId}`) {
    // if the click target is the current issue element, return
    return;
  }
  targetElement = targetElement.parentElement;
}
```

with:

```ts
// check if the click target is the current peek trigger element or its children
let targetElement: HTMLElement | null = event.target;
while (targetElement) {
  if (targetElement.id === resolvedTargetElementId) {
    // if the click target is the current peek trigger element, return
    return;
  }
  targetElement = targetElement.parentElement;
}
```

- [ ] **Step 3: Update the callback deps**

Replace:

```ts
[ref, callback, issueId, excludePreventionElementIds];
```

with:

```ts
[ref, callback, peekId, excludePreventionElementIds, resolvedTargetElementId];
```

- [ ] **Step 4: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors (issue peek calls are positionally unchanged).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/hooks/use-peek-overview-outside-click.tsx
git commit -m "refactor(web): allow custom trigger element in outside-click hook"
```

---

### Task 3: Service peek redirection hook

**Files:**

- Create: `apps/web/core/hooks/use-service-peek-overview-redirection.ts`

- [ ] **Step 1: Create the hook**

Create `apps/web/core/hooks/use-service-peek-overview-redirection.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback } from "react";
// types
import type { IService } from "@plane/types";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
// store
import { useService } from "./store/use-service";

const useServicePeekOverviewRedirection = () => {
  // router
  const router = useAppRouter();
  // store hooks
  const { getIsServicePeeked, setPeekService } = useService();

  const handleRedirection = useCallback(
    (workspaceSlug: string | undefined, service: IService | undefined, isMobile = false) => {
      if (!workspaceSlug || !service) return;
      const { project_id, id } = service;
      if (getIsServicePeeked(id)) return;
      const serviceLink = `/${workspaceSlug}/projects/${project_id}/services/${id}`;
      if (isMobile) router.push(serviceLink);
      else setPeekService({ workspaceSlug, projectId: project_id, serviceId: id });
    },
    [getIsServicePeeked, router, setPeekService]
  );

  return { handleRedirection };
};

export default useServicePeekOverviewRedirection;
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/hooks/use-service-peek-overview-redirection.ts
git commit -m "feat(web): add service peek redirection hook"
```

---

### Task 4: Sidebar stacked layout support

**Files:**

- Modify: `apps/web/core/components/services/detail/sidebar.tsx:6-23,41-56,75-76`

- [ ] **Step 1: Add the `cn` import**

After `import type { IService } from "@plane/types";` add:

```ts
import { cn } from "@plane/utils";
```

- [ ] **Step 2: Add the prop**

Replace:

```ts
type Props = {
  serviceId: string;
};

export const ServiceDetailSidebar = observer(function ServiceDetailSidebar(props: Props) {
  const { serviceId } = props;
```

with:

```ts
type Props = {
  serviceId: string;
  layout?: "sidebar" | "stacked";
};

export const ServiceDetailSidebar = observer(function ServiceDetailSidebar(props: Props) {
  const { serviceId, layout = "sidebar" } = props;
```

- [ ] **Step 3: Make the container layout-aware**

Replace:

```tsx
    <div className="w-full px-6 md:h-full md:overflow-y-auto">
```

with:

```tsx
    <div className={cn("w-full", layout === "sidebar" ? "px-6 md:h-full md:overflow-y-auto" : "px-0")}>
```

- [ ] **Step 4: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: no errors (detail page call site is unchanged; default keeps current behavior).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/detail/sidebar.tsx
git commit -m "feat(web): support stacked layout in service detail sidebar"
```

---

### Task 5: Peek components and mount

**Files:**

- Create: `apps/web/core/components/services/peek-overview/root.tsx`
- Create: `apps/web/core/components/services/peek-overview/view.tsx`
- Create: `apps/web/core/components/services/peek-overview/index.ts`
- Modify: `apps/web/core/components/services/services-list-view.tsx`
- Modify: `apps/web/core/components/services/index.ts`

- [ ] **Step 1: Create `root.tsx`**

Create `apps/web/core/components/services/peek-overview/root.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import { ServicePeekView } from "./view";

export const ServicePeekOverview = observer(function ServicePeekOverview() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // store hooks
  const { peekService, setPeekService, fetchedMap, errorMap, fetchServices } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString();
  const isMatching = Boolean(peekService && peekService.workspaceSlug === slug && peekService.projectId === pid);

  // Drop the peek when the route moves to another workspace/project
  useEffect(() => {
    if (peekService && !isMatching) setPeekService(undefined);
  }, [peekService, isMatching, setPeekService]);

  // Ensure the service list (and health) is hydrated for this project
  useEffect(() => {
    if (!isMatching || !slug || !pid) return;
    if (fetchedMap[pid] || !currentWorkspace?.id) return;
    void fetchServices(slug, currentWorkspace.id, pid);
  }, [isMatching, slug, pid, fetchedMap, currentWorkspace?.id, fetchServices]);

  if (!isMatching || !peekService || !slug || !pid) return null;

  const status: "loading" | "error" | "ready" = errorMap[pid] ? "error" : fetchedMap[pid] ? "ready" : "loading";

  const handleRetry = () => {
    if (!slug || !pid || !currentWorkspace?.id) return;
    void fetchServices(slug, currentWorkspace.id, pid);
  };

  return (
    <ServicePeekView
      workspaceSlug={slug}
      projectId={pid}
      serviceId={peekService.serviceId}
      status={status}
      onRetry={handleRetry}
    />
  );
});
```

- [ ] **Step 2: Create `header.tsx`**

Create `apps/web/core/components/services/peek-overview/header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
// icons
import {
  ArrowNarrowRightOutline,
  DragDropOutline,
  FullScreenPeekOutline,
  ModalPeekOutline,
  SidePeekOutline,
} from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { IService, TNameDescriptionLoader } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";
// components
import { NameDescriptionUpdateStatus } from "@/components/issues/issue-update-status";
// local imports
import { ServiceDetailQuickActions } from "../detail/quick-actions";

export type TServicePeekModes = "side-peek" | "modal" | "full-screen";

const PEEK_OPTIONS: { key: TServicePeekModes; icon: typeof SidePeekOutline; i18n_title: string }[] = [
  { key: "side-peek", icon: SidePeekOutline, i18n_title: "common.side_peek" },
  { key: "modal", icon: ModalPeekOutline, i18n_title: "common.modal" },
  { key: "full-screen", icon: FullScreenPeekOutline, i18n_title: "common.full_screen" },
];

type Props = {
  peekMode: TServicePeekModes;
  setPeekMode: (mode: TServicePeekModes) => void;
  closePeek: () => void;
  serviceLink: string;
  service?: IService;
  isSubmitting: TNameDescriptionLoader;
};

export function ServicePeekHeader(props: Props) {
  const { peekMode, setPeekMode, closePeek, serviceLink, service, isSubmitting } = props;
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const currentMode = PEEK_OPTIONS.find((mode) => mode.key === peekMode) ?? PEEK_OPTIONS[0];

  return (
    <div className="relative flex items-center justify-between p-4">
      <div className="flex items-center gap-4">
        <Tooltip label={t("common.close_peek_view")}>
          <button type="button" onClick={closePeek}>
            <ArrowNarrowRightOutline className="h-4 w-4 text-tertiary hover:text-secondary" />
          </button>
        </Tooltip>
        <Tooltip label={t("common.open_in_full_screen", { page: t("service.title") })}>
          <Link href={serviceLink} onClick={closePeek}>
            <DragDropOutline className="h-4 w-4 text-tertiary hover:text-secondary" />
          </Link>
        </Tooltip>
        <CustomSelect
          value={peekMode}
          onChange={(value: TServicePeekModes) => setPeekMode(value)}
          customButton={
            <Tooltip label={t("common.toggle_peek_view_layout")}>
              <button type="button">
                <currentMode.icon className="h-4 w-4 text-tertiary hover:text-secondary" />
              </button>
            </Tooltip>
          }
        >
          {PEEK_OPTIONS.map((mode) => (
            <CustomSelect.Option key={mode.key} value={mode.key}>
              <div
                className={cn(
                  "flex items-center gap-1.5",
                  mode.key === currentMode.key ? "text-secondary" : "text-placeholder hover:text-secondary"
                )}
              >
                <mode.icon className="-my-1 h-4 w-4 flex-shrink-0" />
                {t(mode.i18n_title)}
              </div>
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      </div>
      {service && (
        <div className="flex items-center gap-x-4">
          <NameDescriptionUpdateStatus isSubmitting={isSubmitting} />
          <ServiceDetailQuickActions serviceId={service.id} />
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Create `view.tsx`**

Create `apps/web/core/components/services/peek-overview/view.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRef, useState } from "react";
import { observer } from "mobx-react";
import { createPortal } from "react-dom";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
import { cn } from "@plane/utils";
// hooks
import { useService } from "@/hooks/store/use-service";
import useKeypress from "@/hooks/use-keypress";
import usePeekOverviewOutsideClickDetector from "@/hooks/use-peek-overview-outside-click";
// local imports
import { ServiceDescription } from "../detail/description";
import { ServiceDetailSidebar } from "../detail/sidebar";
import { ServiceTitleInput } from "../detail/title-input";
import { ServiceWorkItems } from "../detail/work-items";
import { ServiceLoadErrorState } from "../service-load-error-state";
import { ServicePeekHeader, type TServicePeekModes } from "./header";

type Props = {
  workspaceSlug: string;
  projectId: string;
  serviceId: string;
  status: "loading" | "error" | "ready";
  onRetry: () => void;
};

export const ServicePeekView = observer(function ServicePeekView(props: Props) {
  const { workspaceSlug, projectId, serviceId, status, onRetry } = props;
  // states
  const [peekMode, setPeekMode] = useState<TServicePeekModes>("side-peek");
  const [isSubmitting, setIsSubmitting] = useState<TNameDescriptionLoader>("saved");
  // refs
  const peekRef = useRef<HTMLDivElement>(null);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, setPeekService } = useService();
  // derived values
  const service = getServiceById(serviceId);
  const serviceLink = `/${workspaceSlug}/projects/${projectId}/services/${serviceId}`;

  const closePeek = () => {
    setPeekService(undefined);
    document.getElementById(`service-${serviceId}`)?.focus();
  };

  usePeekOverviewOutsideClickDetector(
    peekRef,
    () => {
      if (document.querySelector('[role="dialog"]')) return;
      closePeek();
    },
    serviceId,
    ["main-sidebar"],
    `service-${serviceId}`
  );

  useKeypress("Escape", () => {
    if (document.querySelector('[role="dialog"]')) return;
    const activeElement = document.activeElement;
    if (
      activeElement instanceof HTMLElement &&
      (activeElement.tagName === "INPUT" || activeElement.tagName === "TEXTAREA" || activeElement.isContentEditable)
    )
      return;
    closePeek();
  });

  const renderBody = () => {
    if (status === "error") return <ServiceLoadErrorState onRetry={onRetry} />;
    if (status === "loading") return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    if (!service)
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
          <p className="text-sm font-medium text-primary">{t("service.detail.not_found_title")}</p>
          <p className="text-xs text-secondary">{t("service.detail.not_found_description")}</p>
        </div>
      );

    const mainContent = (
      <>
        <ServiceTitleInput serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceDescription serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceWorkItems serviceId={serviceId} />
      </>
    );

    if (peekMode === "full-screen")
      return (
        <div className="flex h-full w-full flex-col overflow-hidden md:flex-row">
          <div className="w-full space-y-6 overflow-y-auto p-4 py-5 md:h-full md:min-w-0 md:flex-1 md:px-8">
            {mainContent}
          </div>
          <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:!w-[400px] md:border-t-0 md:border-l">
            <ServiceDetailSidebar serviceId={serviceId} />
          </div>
        </div>
      );

    return (
      <div className="relative flex flex-col gap-6 px-4 py-5 md:px-8">
        {mainContent}
        <div className="border-t border-subtle pt-4">
          <ServiceDetailSidebar serviceId={serviceId} layout="stacked" />
        </div>
      </div>
    );
  };

  const content = (
    <div
      ref={peekRef}
      className={cn(
        "absolute z-[25] flex flex-col overflow-hidden rounded-sm border border-subtle bg-surface-1 transition-all duration-300",
        {
          "top-0 right-0 bottom-0 w-full border-0 border-l md:w-[50%]": peekMode === "side-peek",
          "top-[8.33%] left-[8.33%] size-5/6": peekMode === "modal",
          "absolute inset-0 m-4": peekMode === "full-screen",
        }
      )}
      style={{
        boxShadow:
          "0px 4px 8px 0px rgba(0, 0, 0, 0.12), 0px 6px 12px 0px rgba(16, 24, 40, 0.12), 0px 1px 16px 0px rgba(16, 24, 40, 0.12)",
      }}
    >
      <ServicePeekHeader
        peekMode={peekMode}
        setPeekMode={setPeekMode}
        closePeek={closePeek}
        serviceLink={serviceLink}
        service={service}
        isSubmitting={isSubmitting}
      />
      <div className="vertical-scrollbar relative h-full w-full overflow-hidden overflow-y-auto">{renderBody()}</div>
    </div>
  );

  const portalContainer = document.getElementById("full-screen-portal");

  return <>{portalContainer ? createPortal(content, portalContainer) : content}</>;
});
```

- [ ] **Step 4: Create the barrel**

Create `apps/web/core/components/services/peek-overview/index.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./root";
export * from "./header";
export * from "./view";
```

- [ ] **Step 5: Mount in the list view**

In `apps/web/core/components/services/services-list-view.tsx`, add the import after `import { CreateUpdateServiceModal } from "./modal";`:

```ts
import { ServicePeekOverview } from "./peek-overview";
```

Replace the root return:

```tsx
  return (
    <div className="relative flex min-h-0 w-full flex-1 flex-col">
      {renderContent()}
      <ServicePeekOverview />
      {workspaceSlug && projectId && (
```

(The `relative` wrapper gives the panel a positioned ancestor when `#full-screen-portal` is unavailable.)

- [ ] **Step 6: Export from the services barrel**

In `apps/web/core/components/services/index.ts`, add after `export * from "./modal";`:

```ts
export * from "./peek-overview";
```

- [ ] **Step 7: Typecheck and lint**

Run: `pnpm --filter=web check:types`
Expected: no errors.

Run: `pnpm --filter=web exec oxlint core/components/services/peek-overview core/components/services/services-list-view.tsx core/components/services/index.ts`
Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/services/peek-overview \
  apps/web/core/components/services/services-list-view.tsx \
  apps/web/core/components/services/index.ts
git commit -m "feat(web): add service peek overview panel"
```

---

### Task 6: Board row and graph node triggers

**Files:**

- Modify: `apps/web/core/components/services/board/services-board-row.tsx`
- Modify: `apps/web/core/components/services/graph/service-graph.tsx`

- [ ] **Step 1: Swap the board row link for a ControlLink**

In `apps/web/core/components/services/board/services-board-row.tsx`:

Remove `import Link from "next/link";` and add:

```ts
import { ControlLink } from "@plane/ui";
// hooks
import { usePlatformOS } from "@/hooks/use-platform-os";
import useServicePeekOverviewRedirection from "@/hooks/use-service-peek-overview-redirection";
```

Add inside the component after `const { t } = useTranslation();`:

```ts
// peek overview
const { isMobile } = usePlatformOS();
const { handleRedirection } = useServicePeekOverviewRedirection();
```

Replace the `<Link ...>` opening tag:

```tsx
    <Link
      href={serviceLink}
      className="relative flex items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover"
    >
```

with:

```tsx
    <ControlLink
      id={`service-${service.id}`}
      href={serviceLink}
      onClick={() => handleRedirection(workspaceSlug?.toString(), service, isMobile)}
      className="relative flex items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover"
    >
```

and the closing `</Link>` with `</ControlLink>`.

- [ ] **Step 2: Route graph node clicks to the peek**

In `apps/web/core/components/services/graph/service-graph.tsx`:

Remove `import { useAppRouter } from "@/hooks/use-app-router";` and `const router = useAppRouter();`.

Add to the hooks imports:

```ts
import { usePlatformOS } from "@/hooks/use-platform-os";
import useServicePeekOverviewRedirection from "@/hooks/use-service-peek-overview-redirection";
```

Change the store destructure:

```ts
const { getGraphData, getServiceById, addDependency, removeDependency, updateNodePosition, updateService } =
  useService();
```

Add after `const workspaceId = currentWorkspace?.id;`:

```ts
const { isMobile } = usePlatformOS();
const { handleRedirection } = useServicePeekOverviewRedirection();
```

Replace `handleNodeClick`:

```tsx
const handleNodeClick = useCallback(
  (serviceId: string) => {
    if (!slug) return;
    const service = getServiceById(serviceId);
    if (!service) return;
    handleRedirection(slug, service, isMobile);
  },
  [getServiceById, handleRedirection, isMobile, slug]
);
```

- [ ] **Step 3: Typecheck and lint**

Run: `pnpm --filter=web check:types`
Expected: no errors.

Run: `pnpm --filter=web exec oxlint core/components/services/board/services-board-row.tsx core/components/services/graph/service-graph.tsx`
Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/board/services-board-row.tsx \
  apps/web/core/components/services/graph/service-graph.tsx
git commit -m "feat(web): open service peek from board rows and graph nodes"
```

---

### Task 7: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run all web checks**

Run: `pnpm --filter=web check:lint`
Expected: 0 errors.

Run: `pnpm --filter=web check:types`
Expected: no errors.

Run: `pnpm --filter=web test`
Expected: all tests pass, including the 3 new `ServicesStore` peek tests.

Run: `pnpm --filter=web exec oxfmt --check core/components/services core/hooks/use-service-peek-overview-redirection.ts core/hooks/use-peek-overview-outside-click.tsx core/store/service.store.ts`
Expected: `All matched files use the correct format.`

- [ ] **Step 2: Build and restart prod**

Run: `pnpm --filter=web build`
Expected: build succeeds.

Run: `systemctl --user restart plane-web-prod.service`
Expected: `active`; `curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000/` → `200`.

- [ ] **Step 3: Manual checklist (browser, on the services page)**

- Board row left click → peek opens over the list; row stays visible.
- Cmd/Ctrl+click board row → detail page opens in a new tab.
- Graph node click → peek opens; node drag / edge create / Re-layout still work.
- X button, ESC, and outside click each close the peek.
- Clicking the same row/graph node again after close reopens; no close-then-reopen flicker.
- Mode dropdown switches side-peek ⇄ modal ⇄ full-screen; full-screen shows the two-column layout.
- Edit and Delete from the peek header: dialogs open above the panel; interacting with them does not close the peek.
- Add/remove work items and dependencies from inside the peek; title/description edits persist.
- Delete the peeked service → panel closes.
- Mobile viewport (narrow, touch) → tapping a row navigates to the detail page.
- Navigate to another project's services list and back → no stale peek panel for the old project.
- "Open full screen" in the peek header → navigates to the detail route and closes the peek.
