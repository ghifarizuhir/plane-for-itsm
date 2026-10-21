# Workspace Aggregate Parity — Phase 1 (Application Scope + Work Items) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persistent workspace-level Application scope (sidebar selector + header chip) and make workspace Work Items respect it, without changing project-level behavior.

**Architecture:** A new MobX store persists one `projectId | null` per workspace. A shared hook syncs it with `?project=`. The workspace Work Items GLOBAL issue store injects the scoped project into the rich `filters` expression at fetch time (api-rs ignores legacy `?project=`). The sidebar hosts the selector; each workspace page header shows a clearable chip.

**Tech Stack:** React, MobX, Next.js navigation shims, `@plane/ui` `CustomSelect`, vitest, TypeScript, oxlint.

**Spec:** `docs/superpowers/specs/2026-10-21-workspace-parity-design.md`

---

## File structure

Create:

- `apps/web/core/store/workspace-application-scope.store.ts` — persisted scope store.
- `apps/web/core/store/workspace-application-scope.store.test.ts` — store unit tests.
- `apps/web/core/lib/workspace-application-scope.ts` — pure URL builder.
- `apps/web/core/lib/workspace-application-scope.test.ts` — URL builder tests.
- `apps/web/core/hooks/store/use-application-scope.ts` — store hook.
- `apps/web/core/hooks/use-workspace-application-scope.ts` — scope + URL sync hook.
- `apps/web/core/components/workspace/application-scope/select.tsx` — sidebar select.
- `apps/web/core/components/workspace/application-scope/chip.tsx` — page header chip.
- `apps/web/core/components/workspace/application-scope/index.ts` — exports.

Modify:

- `apps/web/core/store/root.store.ts` — register/reset the store.
- `apps/web/core/components/sidebar/sidebar-wrapper.tsx` — `headerContent` slot.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/sidebar.tsx` — render the select.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/header.tsx` — render the chip, scope the create modal.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/page.tsx` — drop the old `projectFilter` prop.
- `apps/web/core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx` — drop the rich-filter preset, refetch on scope change.
- `apps/web/core/components/workspace/sidebar/quick-actions.tsx` — scope the sidebar `New work item` create target.
- `apps/web/core/store/issue/workspace/issue.store.ts` — inject `project_id` into rich filters.
- `packages/constants/src/issue/filter.ts` — remove `project_id` from `my_issues` Add Filter.

Note: the GLOBAL workspace issue store is also used by workspace views (`/workspace-views/:globalViewId`). Applying the scope there is intentional: both are workspace-level aggregate issue surfaces.

---

### Task 1: Persisted application scope store

**Files:**

- Create: `apps/web/core/store/workspace-application-scope.store.ts`
- Create: `apps/web/core/store/workspace-application-scope.store.test.ts`
- Modify: `apps/web/core/store/root.store.ts`

- [ ] **Step 1: Write the failing store test**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY,
  WorkspaceApplicationScopeStore,
} from "./workspace-application-scope.store";

const createStorage = () => {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
    removeItem: (key: string) => void data.delete(key),
  };
};

describe("WorkspaceApplicationScopeStore", () => {
  it("defaults to All and persists one scope per workspace", () => {
    const storage = createStorage();
    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBeNull();

    store.setProjectId("ws-1", "project-1");
    store.setProjectId("ws-2", "project-2");

    expect(store.getProjectId("ws-1")).toBe("project-1");
    expect(store.getProjectId("ws-2")).toBe("project-2");
    expect(JSON.parse(storage.data.get(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY) ?? "{}")).toEqual({
      "ws-1": "project-1",
      "ws-2": "project-2",
    });
  });

  it("hydrates a persisted scope on construction", () => {
    const storage = createStorage();
    storage.setItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY, JSON.stringify({ "ws-1": "project-1" }));

    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBe("project-1");
    expect(store.getProjectId("ws-2")).toBeNull();
  });

  it("resets one workspace without touching others and can clear everything", () => {
    const storage = createStorage();
    const store = new WorkspaceApplicationScopeStore(storage);
    store.setProjectId("ws-1", "project-1");
    store.setProjectId("ws-2", "project-2");

    store.reset("ws-1");
    expect(store.getProjectId("ws-1")).toBeNull();
    expect(store.getProjectId("ws-2")).toBe("project-2");

    store.reset();
    expect(store.getProjectId("ws-2")).toBeNull();
    expect(JSON.parse(storage.data.get(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY) ?? "{}")).toEqual({});
  });

  it("ignores malformed storage", () => {
    const storage = createStorage();
    storage.setItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY, "not-json");

    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm exec vitest run core/store/workspace-application-scope.store.test.ts` (from `apps/web`)

Expected: FAIL with “Cannot find module” / “Failed to resolve import”.

- [ ] **Step 3: Implement the store**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, makeObservable, observable } from "mobx";

export const WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY = "workspace_application_scope";

type TStorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export interface IWorkspaceApplicationScopeStore {
  projectIdByWorkspace: Record<string, string | null>;
  getProjectId: (workspaceSlug: string) => string | null;
  setProjectId: (workspaceSlug: string, projectId: string | null) => void;
  reset: (workspaceSlug?: string) => void;
}

const getDefaultStorage = (): TStorageLike | null =>
  typeof window !== "undefined" ? window.localStorage : null;

export class WorkspaceApplicationScopeStore implements IWorkspaceApplicationScopeStore {
  projectIdByWorkspace: Record<string, string | null> = {};
  private storage: TStorageLike | null;

  constructor(storage: TStorageLike | null = getDefaultStorage()) {
    this.storage = storage;
    this.projectIdByWorkspace = this.readStorage();
    makeObservable(this, {
      projectIdByWorkspace: observable,
      setProjectId: action,
      reset: action,
    });
  }

  private readStorage = (): Record<string, string | null> => {
    if (!this.storage) return {};
    try {
      const raw = this.storage.getItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY);
      if (!raw) return {};
      const parsed: unknown = JSON.parse(raw);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
      return Object.fromEntries(
        Object.entries(parsed as Record<string, unknown>)
          .filter(([, value]) => typeof value === "string" || value === null)
          .map(([workspaceSlug, value]) => [workspaceSlug, value as string | null])
      );
    } catch {
      return {};
    }
  };

  private persist = () => {
    try {
      this.storage?.setItem(
        WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY,
        JSON.stringify(this.projectIdByWorkspace)
      );
    } catch {
      // Best-effort persistence; the in-memory scope still works for this session.
    }
  };

  getProjectId = (workspaceSlug: string) => this.projectIdByWorkspace[workspaceSlug] ?? null;

  setProjectId = (workspaceSlug: string, projectId: string | null) => {
    this.projectIdByWorkspace = { ...this.projectIdByWorkspace, [workspaceSlug]: projectId };
    this.persist();
  };

  reset = (workspaceSlug?: string) => {
    if (workspaceSlug) {
      const next = { ...this.projectIdByWorkspace };
      delete next[workspaceSlug];
      this.projectIdByWorkspace = next;
    } else {
      this.projectIdByWorkspace = {};
    }
    this.persist();
  };
}
```

- [ ] **Step 4: Run the store test to verify it passes**

Run: `pnpm exec vitest run core/store/workspace-application-scope.store.test.ts` (from `apps/web`)

Expected: PASS (4 tests).

- [ ] **Step 5: Register the store on `CoreRootStore`**

In `apps/web/core/store/root.store.ts`:

1. Add imports next to the other store imports:

```ts
import type { IWorkspaceApplicationScopeStore } from "./workspace-application-scope.store";
import { WorkspaceApplicationScopeStore } from "./workspace-application-scope.store";
```

2. Add the property to the class:

```ts
  workspaceApplicationScope: IWorkspaceApplicationScopeStore;
```

3. In the main constructor, after `this.intakeSource = new IntakeSourceStore(this);`, add:

```ts
    this.workspaceApplicationScope = new WorkspaceApplicationScopeStore();
```

4. In `resetOnSignOut`, before the reassignments, add:

```ts
    this.workspaceApplicationScope.reset();
```

5. At the end of `resetOnSignOut`, after `this.intakeSource = new IntakeSourceStore(this);`, add:

```ts
    this.workspaceApplicationScope = new WorkspaceApplicationScopeStore();
```

- [ ] **Step 6: Type-check and lint the touched files**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "workspace-application-scope|root.store" ; echo DONE
```

Expected: only `DONE` (no lines above it).

```bash
pnpm exec oxlint core/store/workspace-application-scope.store.ts core/store/workspace-application-scope.store.test.ts core/store/root.store.ts
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/store/workspace-application-scope.store.ts \
  apps/web/core/store/workspace-application-scope.store.test.ts \
  apps/web/core/store/root.store.ts
git commit -m "feat(web): add persisted workspace application scope store"
```

---

### Task 2: URL builder, store hook, and scope hook

**Files:**

- Create: `apps/web/core/lib/workspace-application-scope.ts`
- Create: `apps/web/core/lib/workspace-application-scope.test.ts`
- Create: `apps/web/core/hooks/store/use-application-scope.ts`
- Create: `apps/web/core/hooks/use-workspace-application-scope.ts`

- [ ] **Step 1: Write the failing URL builder test**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { buildApplicationScopeUrl } from "./workspace-application-scope";

describe("buildApplicationScopeUrl", () => {
  it("adds the project param and preserves existing params", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "layout=list", "project-1")).toBe(
      "/itsm/work-items?layout=list&project=project-1"
    );
  });

  it("removes the project param for All", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "layout=list&project=project-1", null)).toBe(
      "/itsm/work-items?layout=list"
    );
  });

  it("drops the query string when nothing remains", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "project=project-1", null)).toBe("/itsm/work-items");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm exec vitest run core/lib/workspace-application-scope.test.ts` (from `apps/web`)

Expected: FAIL with “Failed to resolve import”.

- [ ] **Step 3: Implement the URL builder**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const buildApplicationScopeUrl = (
  pathname: string,
  queryString: string,
  projectId: string | null
): string => {
  const params = new URLSearchParams(queryString);
  if (projectId) params.set("project", projectId);
  else params.delete("project");
  const query = params.toString();
  return `${pathname}${query ? `?${query}` : ""}`;
};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm exec vitest run core/lib/workspace-application-scope.test.ts` (from `apps/web`)

Expected: PASS (3 tests).

- [ ] **Step 5: Implement the store hook**

Create `apps/web/core/hooks/store/use-application-scope.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// mobx store
import { StoreContext } from "@/lib/store-context";
// types
import type { IWorkspaceApplicationScopeStore } from "@/store/workspace-application-scope.store";

export const useApplicationScope = (): IWorkspaceApplicationScopeStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useApplicationScope must be used within StoreProvider");
  return context.workspaceApplicationScope;
};
```

- [ ] **Step 6: Implement the scope hook**

Create `apps/web/core/hooks/use-workspace-application-scope.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { useParams, usePathname, useSearchParams } from "next/navigation";
// hooks
import { useApplicationScope } from "@/hooks/store/use-application-scope";
import { useProject } from "@/hooks/store/use-project";
import { useAppRouter } from "@/hooks/use-app-router";
// helpers
import { buildApplicationScopeUrl } from "@/lib/workspace-application-scope";

export const useWorkspaceApplicationScope = () => {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useAppRouter();
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  const { getProjectId, setProjectId } = useApplicationScope();
  // derived values
  const storedProjectId = workspaceSlug ? getProjectId(workspaceSlug) : null;
  const projectId =
    storedProjectId && workspaceProjectIds?.includes(storedProjectId) ? storedProjectId : null;

  // Seed from `?project=` and discard scopes that no longer belong to this workspace.
  useEffect(() => {
    if (!workspaceSlug || workspaceProjectIds === undefined) return;
    const paramProjectId = searchParams.get("project");
    if (paramProjectId && workspaceProjectIds.includes(paramProjectId)) {
      if (storedProjectId !== paramProjectId) setProjectId(workspaceSlug, paramProjectId);
      return;
    }
    if (storedProjectId && !workspaceProjectIds.includes(storedProjectId)) {
      setProjectId(workspaceSlug, null);
    }
  }, [workspaceSlug, workspaceProjectIds, searchParams, storedProjectId, setProjectId]);

  const setApplication = useCallback(
    (nextProjectId: string | null) => {
      if (!workspaceSlug) return;
      setProjectId(workspaceSlug, nextProjectId);
      router.replace(buildApplicationScopeUrl(pathname, searchParams.toString(), nextProjectId));
    },
    [workspaceSlug, setProjectId, router, pathname, searchParams]
  );

  const clearApplication = useCallback(() => setApplication(null), [setApplication]);

  const project = projectId ? getProjectById(projectId) : undefined;

  return { workspaceSlug, projectId, project, setApplication, clearApplication };
};
```

- [ ] **Step 7: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "workspace-application-scope|use-application-scope" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/lib/workspace-application-scope.ts core/lib/workspace-application-scope.test.ts core/hooks/store/use-application-scope.ts core/hooks/use-workspace-application-scope.ts
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/lib/workspace-application-scope.ts \
  apps/web/core/lib/workspace-application-scope.test.ts \
  apps/web/core/hooks/store/use-application-scope.ts \
  apps/web/core/hooks/use-workspace-application-scope.ts
git commit -m "feat(web): add workspace application scope hook and url sync"
```

---

### Task 3: Sidebar selector and header chip components

**Files:**

- Create: `apps/web/core/components/workspace/application-scope/select.tsx`
- Create: `apps/web/core/components/workspace/application-scope/chip.tsx`
- Create: `apps/web/core/components/workspace/application-scope/index.ts`
- Modify: `apps/web/core/components/sidebar/sidebar-wrapper.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/sidebar.tsx`

- [ ] **Step 1: Create the sidebar select**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { CloseOutline, ProjectsOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CustomSelect } from "@plane/ui";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const WorkspaceApplicationScopeSelect = observer(function WorkspaceApplicationScopeSelect() {
  // router
  const { projectId: routeProjectId } = useParams();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { workspaceProjectIds, getProjectById } = useProject();
  // scope
  const { projectId, setApplication, clearApplication } = useWorkspaceApplicationScope();

  if (routeProjectId) return null;
  if (workspaceProjectIds === undefined) return null;

  const selectedProject = projectId ? getProjectById(projectId) : undefined;

  return (
    <div className="flex flex-col gap-1 px-2">
      <span className="text-[11px] font-semibold uppercase tracking-wider text-placeholder">Application</span>
      <div className="flex min-w-0 items-center gap-1">
        <CustomSelect
          value={projectId ?? ""}
          onChange={(value: string) => setApplication(value || null)}
          className="min-w-0 flex-1"
          buttonClassName="w-full border border-subtle bg-layer-1"
          input
          label={
            <span className="flex min-w-0 items-center gap-1.5 py-0.5 text-12">
              <ProjectsOutline className="size-3.5 flex-shrink-0 text-tertiary" />
              <span className="truncate">{selectedProject?.name ?? t("service.catalog_all_projects")}</span>
            </span>
          }
        >
          <CustomSelect.Option value="">{t("service.catalog_all_projects")}</CustomSelect.Option>
          {workspaceProjectIds.map((id) => {
            const project = getProjectById(id);
            return (
              <CustomSelect.Option key={id} value={id}>
                <span className="truncate">{project?.name}</span>
              </CustomSelect.Option>
            );
          })}
        </CustomSelect>
        {projectId && (
          <button
            type="button"
            onClick={clearApplication}
            aria-label="Clear application scope"
            className="grid size-6 flex-shrink-0 place-items-center rounded-sm text-tertiary hover:bg-layer-transparent-hover hover:text-primary"
          >
            <CloseOutline className="size-3" />
          </button>
        )}
      </div>
    </div>
  );
});
```

- [ ] **Step 2: Create the header chip**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// icons
import { CloseOutline, ProjectsOutline } from "@makeplane/propel/icons";
// hooks
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const WorkspaceApplicationScopeChip = observer(function WorkspaceApplicationScopeChip() {
  const { project, clearApplication } = useWorkspaceApplicationScope();

  if (!project) return null;

  return (
    <div className="flex max-w-[220px] items-center gap-1 rounded-sm border border-subtle bg-layer-1 px-2 py-0.5 text-11 text-secondary">
      <ProjectsOutline className="size-3 flex-shrink-0 text-tertiary" />
      <span className="truncate">{project.name}</span>
      <button
        type="button"
        onClick={clearApplication}
        aria-label="Clear application scope"
        className="grid flex-shrink-0 place-items-center rounded-xs hover:bg-layer-2"
      >
        <CloseOutline className="size-3" />
      </button>
    </div>
  );
});
```

- [ ] **Step 3: Add the barrel export**

Create `apps/web/core/components/workspace/application-scope/index.ts`:

```ts
export * from "./chip";
export * from "./select";
```

- [ ] **Step 4: Add the `headerContent` slot to `SidebarWrapper`**

In `apps/web/core/components/sidebar/sidebar-wrapper.tsx`:

1. Add to the props type:

```ts
  headerContent?: React.ReactNode;
```

2. Destructure it:

```ts
  const { title, children, quickActions, headerContent, showCustomizeGear = false } = props;
```

3. Render it between the title row and the quick actions:

```tsx
          </div>
          {headerContent}
          {quickActions}
        </div>
```

The resulting block must look like:

```tsx
        <div className="flex flex-col gap-3 px-3">
          <div className="flex items-center justify-between gap-2 px-2">
            <span className="pt-1 text-16 font-medium text-primary">{title}</span>
            <div className="flex items-center gap-2">
              {showCustomizeGear && (
                <IconButton
                  size="base"
                  variant="ghost"
                  icon={PreferencesOutline}
                  onClick={() => setIsCustomizeNavDialogOpen(true)}
                />
              )}
              <AppSidebarToggleButton />
            </div>
          </div>
          {headerContent}
          {quickActions}
        </div>
```

- [ ] **Step 5: Render the selector in the workspace sidebar**

In `apps/web/app/(all)/[workspaceSlug]/(projects)/sidebar.tsx`:

1. Add the import:

```ts
import { WorkspaceApplicationScopeSelect } from "@/components/workspace/application-scope";
```

2. Pass the slot:

```tsx
    <SidebarWrapper
      title={t("sidebar.projects")}
      quickActions={<SidebarQuickActions />}
      headerContent={<WorkspaceApplicationScopeSelect />}
      showCustomizeGear
    >
```

- [ ] **Step 6: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "application-scope|sidebar-wrapper|\(projects\)/sidebar" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/components/workspace/application-scope/*.tsx core/components/workspace/application-scope/index.ts core/components/sidebar/sidebar-wrapper.tsx "app/(all)/[workspaceSlug]/(projects)/sidebar.tsx"
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/workspace/application-scope \
  apps/web/core/components/sidebar/sidebar-wrapper.tsx \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/sidebar.tsx"
git commit -m "feat(web): add sidebar application scope selector"
```

---

### Task 4: Work Items header chip, deep-link cleanup, and scope refetch

**Files:**

- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/header.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/page.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx`

- [ ] **Step 1: Add the chip to the Work Items header**

In `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/header.tsx`:

1. Add the imports:

```ts
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
```

2. Add the scope hook after the store hooks:

```ts
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
```

3. Replace the left item block:

```tsx
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.work_items")}
                href={`/${workspaceSlug}/work-items/`}
                icon={<WorkItemsOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
```

with:

```tsx
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.work_items")}
                  href={`/${workspaceSlug}/work-items/`}
                  icon={<WorkItemsOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          <WorkspaceApplicationScopeChip />
        </div>
      </Header.LeftItem>
```

4. Scope the create button in the same file. Replace:

```tsx
            onClick={() => toggleCreateIssueModal(true, EIssuesStoreType.PROJECT)}
```

with:

```tsx
            onClick={() =>
              toggleCreateIssueModal(
                true,
                EIssuesStoreType.PROJECT,
                scopedProjectId ? [scopedProjectId] : undefined
              )
            }
```

5. Show the chip on mobile too. In `WorkspaceWorkItemsMobileHeader`, add the hook after the existing hooks:

```ts
  const { project: scopedProject } = useWorkspaceApplicationScope();
```

Then replace its `return` wrapper:

```tsx
  return (
    <div className="z-[13] flex justify-evenly border-b border-subtle bg-surface-1 py-2 md:hidden">
```

with:

```tsx
  return (
    <div className="md:hidden">
      {scopedProject && (
        <div className="flex justify-center border-b border-subtle bg-surface-1 py-1">
          <WorkspaceApplicationScopeChip />
        </div>
      )}
      <div className="z-[13] flex justify-evenly border-b border-subtle bg-surface-1 py-2">
```

and close the extra wrapper after the existing mobile bar’s closing `</div>`:

```tsx
      </div>
    </div>
  );
```

- [ ] **Step 2: Scope the sidebar `New work item` button**

In `apps/web/core/components/workspace/sidebar/quick-actions.tsx`:

1. Add the import:

```ts
import { EIssuesStoreType } from "@plane/types";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
```

(Place `EIssuesStoreType` with the other `@plane/types` imports; the existing file already imports `type { TIssue } from "@plane/types"`, so combine them as `import { EIssuesStoreType, type TIssue } from "@plane/types";`.)

2. Add the scope hook after the store hooks:

```ts
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
```

3. Replace the create click handler:

```tsx
          onClick={() => toggleCreateIssueModal(true)}
```

with:

```tsx
          onClick={() =>
            toggleCreateIssueModal(
              true,
              EIssuesStoreType.PROJECT,
              scopedProjectId ? [scopedProjectId] : undefined
            )
          }
```

- [ ] **Step 3: Remove the legacy `projectFilter` prop from the page**

Replace the entire contents of `apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/page.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceWorkItemsLayoutRoot } from "@/components/issues/issue-layouts/roots/workspace-work-items-layout-root";

function WorkspaceWorkItemsPage() {
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.work_items")} />
      <div className="h-full w-full">
        <WorkspaceWorkItemsLayoutRoot />
      </div>
    </>
  );
}

export default observer(WorkspaceWorkItemsPage);
```

- [ ] **Step 4: Remove the rich-filter preset and add the scope refetch effect**

In `apps/web/core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx`:

1. Update the imports:

```ts
import { useCallback, useEffect, useMemo, useRef } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
import useSWR from "swr";
// plane imports
import { ISSUE_DISPLAY_FILTERS_BY_PAGE } from "@plane/constants";
import { EIssuesStoreType, type EIssueLayoutTypes } from "@plane/types";
// components
import { IssuePeekOverview } from "@/components/issues/peek-overview";
import { WorkspaceActiveLayout } from "@/components/views/helper";
import { WorkspaceLevelWorkItemFiltersHOC } from "@/components/work-item-filters/filters-hoc/workspace-level";
import { WorkItemFiltersRow } from "@/components/work-item-filters/filters-row";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useIssues } from "@/hooks/store/use-issues";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { IssuesStoreContext } from "@/hooks/use-issue-layout-store";
import { useMobileViewport } from "@/hooks/use-mobile-viewport";
import { useWorkspaceIssueProperties } from "@/hooks/use-workspace-issue-properties";
// local
import { WORKSPACE_WORK_ITEMS_VIEW_ID } from "@/lib/workspace-work-items";
import { resolveWorkItemLayout } from "../mobile-layout";
```

Removed imports: `useWorkItemFilters`, `useAppRouter`.

2. Remove the `Props` type and the prop:

```ts
type Props = {
  projectFilter?: string;
};

export const WorkspaceWorkItemsLayoutRoot = observer(function WorkspaceWorkItemsLayoutRoot(props: Props) {
  const { projectFilter } = props;
```

becomes:

```ts
export const WorkspaceWorkItemsLayoutRoot = observer(function WorkspaceWorkItemsLayoutRoot() {
```

3. Remove the router variable:

```ts
  const router = useAppRouter();
```

4. Remove the work item filter hook:

```ts
  const { updateFilterExpressionFromConditions } = useWorkItemFilters();
```

5. Add the scope hook after the issues store destructure:

```ts
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
```

6. Remove the whole preset effect:

```ts
  // ?project= preset — migrate legacy WorkspaceTicketsListView query param into rich filter once,
  // then strip it so URL stays clean and shareable (mirrors ProjectLayoutRoot war_room preset).
  const presetApplied = useRef(false);
  useEffect(() => {
    if (presetApplied.current || !workspaceSlug || !workItemFilters) return;
    const paramProject = searchParams.get("project") ?? projectFilter;
    if (!paramProject) return;
    presetApplied.current = true;
    void updateFilterExpressionFromConditions(
      EIssuesStoreType.GLOBAL,
      viewId,
      [{ property: "project_id", operator: "in", value: [paramProject] }],
      updateFilterExpression.bind(updateFilterExpression, workspaceSlug, viewId)
    ).finally(() => {
      router.replace(`/${workspaceSlug}/work-items`);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceSlug, workItemFilters, searchParams]);
```

7. Add the scope-driven refetch effect in its place:

```ts
  // Refetch the current page when the sidebar application scope changes. The first
  // render is skipped because the child layout roots already fetch on mount.
  const previousScope = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    if (previousScope.current === undefined) {
      previousScope.current = scopedProjectId;
      return;
    }
    if (previousScope.current === scopedProjectId) return;
    previousScope.current = scopedProjectId;
    if (workspaceSlug) void fetchIssuesWithExistingPagination(workspaceSlug, viewId, "mutation");
  }, [scopedProjectId, workspaceSlug, viewId, fetchIssuesWithExistingPagination]);
```

Note: `updateFilterExpression` is still used by the HOC (`updateFilters={updateFilterExpression.bind(...)}`); keep it in the destructure.

- [ ] **Step 5: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "work-items|workspace-work-items-layout-root|workspace/sidebar/quick-actions" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx core/components/workspace/sidebar/quick-actions.tsx "app/(all)/[workspaceSlug]/(projects)/work-items/header.tsx" "app/(all)/[workspaceSlug]/(projects)/work-items/page.tsx"
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 6: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/header.tsx" \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/work-items/page.tsx" \
  apps/web/core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx \
  apps/web/core/components/workspace/sidebar/quick-actions.tsx
git commit -m "feat(web): show workspace application scope on work items"
```

---

### Task 5: Inject the scope into Work Items fetches

**Files:**

- Modify: `apps/web/core/store/issue/workspace/issue.store.ts`
- Modify: `packages/constants/src/issue/filter.ts`

- [ ] **Step 1: Add a focused test for the scope expression**

Append this case to `apps/web/core/lib/issue-filter-scope.test.ts`:

```ts
  it("scopes an expression to one application without dropping existing filters", () => {
    expect(appendRichFilterNode('{"priority":"high"}', { project_id: ["project-1"] })).toBe(
      '{"and":[{"priority":"high"},{"project_id":["project-1"]}]}'
    );
  });
```

Run from `apps/web`:

```bash
pnpm exec vitest run core/lib/issue-filter-scope.test.ts
```

Expected: PASS (4 tests).

- [ ] **Step 2: Merge the scope into `fetchIssues`**

In `apps/web/core/store/issue/workspace/issue.store.ts`, inside `fetchIssues`, after the calendar range block:

```ts
      // Calendar range filter: the workspace endpoint ignores the legacy
      // `target_date=after;before` param, so push it through the rich filter
      // tree (supported allowlist key) instead.
      if (options.after && options.before) {
        params.filters = appendRichFilterNode(params.filters, {
          target_date__range: `${options.after},${options.before}`,
        });
      }
```

add:

```ts
      // The sidebar application scope is a workspace-wide project filter. The
      // workspace endpoint ignores legacy `?project=`, so scope via rich filters.
      const scopedProjectId =
        this.rootIssueStore.rootStore.workspaceApplicationScope.getProjectId(workspaceSlug);
      if (scopedProjectId) {
        params.filters = appendRichFilterNode(params.filters, { project_id: [scopedProjectId] });
      }
```

Then apply the same merge to pagination. In the same file, inside `fetchNextIssues`, replace:

```ts
      // get params from stored pagination options
      const params = this.issueFilterStore?.getFilterParams(
        this.paginationOptions,
        viewId,
        this.getNextCursor(groupId, subGroupId),
        groupId,
        subGroupId
      );
      // call the fetch issues API with the params for next page in issues
      const response = await this.workspaceService.getViewIssues(workspaceSlug, params);
```

with:

```ts
      // get params from stored pagination options
      const params = {
        ...this.issueFilterStore?.getFilterParams(
          this.paginationOptions,
          viewId,
          this.getNextCursor(groupId, subGroupId),
          groupId,
          subGroupId
        ),
      };

      // Keep paginated pages scoped to the sidebar application selection.
      const scopedProjectId =
        this.rootIssueStore.rootStore.workspaceApplicationScope.getProjectId(workspaceSlug);
      if (scopedProjectId) {
        params.filters = appendRichFilterNode(params.filters, { project_id: [scopedProjectId] });
      }

      // call the fetch issues API with the params for next page in issues
      const response = await this.workspaceService.getViewIssues(workspaceSlug, params);
```

- [ ] **Step 3: Remove `project_id` from the workspace Add Filter allowlist**

In `packages/constants/src/issue/filter.ts`, locate the `my_issues` filters array (it starts with `filters: [` and contains `"cycle_id"`, `"module_id"`, `"assignee_id"`, `"mention_id"`, `"created_by_id"`, `"subscriber_id"`, `"project_id"`, `"label_id"`, ...). Remove only this line from that array:

```ts
      "project_id",
```

Do NOT remove `project_id` from `project_issues`, `project`, or other filter lists in the same file.

- [ ] **Step 4: Rebuild the constants package**

Run from the repo root:

```bash
pnpm --filter=@plane/constants build
```

Expected: build succeeds.

- [ ] **Step 5: Type-check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "workspace/issue.store|issue-filter-scope" ; echo DONE
```

Expected: only `DONE`.

```bash
pnpm exec oxlint core/store/issue/workspace/issue.store.ts core/lib/issue-filter-scope.test.ts
```

Expected: `Found 0 warnings and 0 errors.`

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/store/issue/workspace/issue.store.ts \
  apps/web/core/lib/issue-filter-scope.test.ts \
  packages/constants/src/issue/filter.ts
git commit -m "feat(web): scope workspace work items by application"
```

---

### Task 6: Full verification and deploy

**Files:** none (verification only).

- [ ] **Step 1: Run the touched unit suites**

Run from `apps/web`:

```bash
pnpm exec vitest run \
  core/store/workspace-application-scope.store.test.ts \
  core/lib/workspace-application-scope.test.ts \
  core/lib/issue-filter-scope.test.ts \
  core/lib/workspace-issue-grouping.test.ts
```

Expected: all suites PASS.

- [ ] **Step 2: Full type check and lint**

Run from `apps/web`:

```bash
pnpm exec tsc --noEmit -p tsconfig.json 2>&1 | grep -E "error TS" | head -n 20 ; echo DONE
pnpm exec oxlint core/store/workspace-application-scope.store.ts \
  core/store/workspace-application-scope.store.test.ts \
  core/lib/workspace-application-scope.ts \
  core/lib/workspace-application-scope.test.ts \
  core/hooks/store/use-application-scope.ts \
  core/hooks/use-workspace-application-scope.ts \
  "core/components/workspace/application-scope/*.tsx" \
  core/components/workspace/application-scope/index.ts \
  core/components/sidebar/sidebar-wrapper.tsx \
  core/components/workspace/sidebar/quick-actions.tsx \
  core/components/issues/issue-layouts/roots/workspace-work-items-layout-root.tsx \
  core/store/issue/workspace/issue.store.ts
```

Expected: no `error TS` lines; `Found 0 warnings and 0 errors.`

- [ ] **Step 3: Production build**

Run from the repo root:

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: `✓ built` and `SPA Mode: Generated build/client/index.html`.

- [ ] **Step 4: Restart the production web service**

```bash
systemctl --user restart plane-web-prod.service
sleep 18
systemctl --user is-active plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/work-items
```

Expected: `active` and `200`. If it returns `000`, wait a few more seconds and retry the curl (the `serve` process takes ~15s to accept connections).

- [ ] **Step 5: Manual verification checklist**

- Sidebar shows `Application: All applications` below the title and above `New work item`.
- Selecting an application updates the URL to include `?project=<id>` and shows the header chip.
- Work Items refetch and show only that application's work items.
- Reload preserves the selection; navigating away and back preserves it.
- `×` on the chip resets to All and removes `?project=`.
- Opening a project detail page hides the sidebar selector.
- With scope All, Work Items show every application again.
- Add Filter no longer offers `Project` on Work Items.

- [ ] **Step 6: Commit fixes if verification exposed an issue**

If a verification command fails, fix the responsible file from Tasks 1–5, rerun the failing command, then stage that file explicitly and commit:

```bash
git add path/to/the/file/you/fixed.ts
git commit -m "fix(web): address workspace application scope verification"
```

If every check passed, skip this step.

---

## Self-review notes

- Spec coverage for Phase 1: store/hook/sidebar/chip/URL sync (Tasks 1–3), Work Items chip + refetch + create targeting (Task 4), rich-filter scope injection + allowlist (Task 5), verification (Task 6).
- The remaining spec phases (Cycles, Modules, Pages, Service Catalog) are intentionally out of this plan and will get separate plans.
- `project_id` is removed only from the `my_issues` allowlist; project-level filter lists are untouched.
- The GLOBAL store scope also applies to workspace views by design; note this in the PR description.
