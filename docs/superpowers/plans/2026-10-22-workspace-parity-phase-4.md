# Workspace Parity Phase 4 (Pages) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. The user selected inline execution for this repository; do not dispatch subagents.

**Goal:** Bring workspace Pages to 1:1 parity with project Pages, aggregated across applications and controlled by the global Application scope.

**Architecture:** Reuse `PagesListView`, `PagesListHeaderRoot`, `PagesListMainContent`, `PagesListRoot`, `PageListBlock`, tabs, search, filters, ordering, and page actions. Add a dedicated workspace page store backed by the existing workspace pages endpoint, a reserved workspace page mode, project badges when All, and an application-picker create flow for All.

**Tech Stack:** React Router, MobX (`WorkspacePageStore`, `ProjectPageStore`, `ProjectPage` instances), `@plane/utils` page filters/ordering, Vitest.

---

## File Map

**Create**

- `apps/web/core/lib/workspace-pages.ts` — pure workspace page scope/type/search/filter/order selector.
- `apps/web/core/lib/workspace-pages.test.ts` — selector tests.
- `apps/web/core/store/pages/workspace-page.store.ts` — workspace page store backed by `/api/workspaces/:slug/pages/`.
- `apps/web/core/hooks/use-workspace-page-permissions.ts` — scoped/All page create permission.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/create-page-provider.tsx` — create flow/context + application picker.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/header.tsx` — workspace Pages header.

**Modify**

- `apps/web/core/hooks/store/use-page-store.ts` — `EPageStoreType.WORKSPACE`.
- `apps/web/core/store/root.store.ts` — instantiate/reset `workspacePages`.
- `apps/web/core/store/pages/project-page.store.ts` — `createPage(pageData, projectId?)`.
- `apps/web/core/components/pages/pages-list-view.tsx` — workspace fetch key + create/empty props.
- `apps/web/core/components/pages/header/root.tsx` — optional project id.
- `apps/web/core/components/pages/list/tab-navigation.tsx` — workspace tab URLs.
- `apps/web/core/components/pages/pages-list-main-content.tsx` — workspace store, create callback, workspace empty state.
- `apps/web/core/components/pages/list/root.tsx` — project badges.
- `apps/web/core/components/pages/list/block.tsx` — project badge.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/layout.tsx` — provider + new header.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/page.tsx` — scope-aware Pages view.

**Delete**

- `apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx`
- `apps/web/core/lib/workspace-kb.ts`
- `apps/web/core/lib/workspace-kb.test.ts`

**Do not change**

- Project Pages behavior must remain unchanged when workspace props are absent.
- No new i18n keys; reuse `workspace_empty_state.kb`, `project_empty_state.pages`, `common_empty_state.search`, and `disabled_project.empty_state.page`.

---

## Task 1: Workspace Page Selector

**Files:**

- Create: `apps/web/core/lib/workspace-pages.ts`
- Test: `apps/web/core/lib/workspace-pages.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/web/core/lib/workspace-pages.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { TPage } from "@plane/types";
import { selectWorkspacePageIds } from "./workspace-pages";

const makePage = (overrides: Partial<TPage>): TPage =>
  ({
    id: "page",
    name: "Page",
    project_ids: ["p1"],
    access: 0,
    archived_at: null,
    is_favorite: false,
    created_at: new Date("2026-10-01T00:00:00.000Z"),
    updated_at: new Date("2026-10-01T00:00:00.000Z"),
    created_by: "u1",
    owned_by: "u1",
    ...overrides,
  }) as TPage;

const select = (pages: TPage[], overrides: Partial<Parameters<typeof selectWorkspacePageIds>[0]> = {}) =>
  selectWorkspacePageIds({
    pages,
    projectIds: ["p1"],
    pageType: "public",
    searchQuery: "",
    sortKey: "updated_at",
    sortBy: "desc",
    ...overrides,
  });

describe("selectWorkspacePageIds", () => {
  it("includes a page when any linked project is in scope", () => {
    const selection = select([
      makePage({ id: "one", project_ids: ["p1"] }),
      makePage({ id: "shared", project_ids: ["p2", "p1"] }),
      makePage({ id: "other", project_ids: ["p2"] }),
    ]);

    expect(selection).toEqual(["one", "shared"]);
  });

  it("filters by public, private, and archived tab", () => {
    const pages = [
      makePage({ id: "public", access: 0 }),
      makePage({ id: "private", access: 1 }),
      makePage({ id: "archived", access: 0, archived_at: "2026-10-02" }),
    ];

    expect(select(pages, { pageType: "public" })).toEqual(["public"]);
    expect(select(pages, { pageType: "private" })).toEqual(["private"]);
    expect(select(pages, { pageType: "archived" })).toEqual(["archived"]);
  });

  it("searches page names case-insensitively and treats untitled pages as Untitled", () => {
    const selection = select(
      [
        makePage({ id: "sop", name: "SOP" }),
        makePage({ id: "untitled", name: "" }),
        makePage({ id: "other", name: "Runbook" }),
      ],
      { searchQuery: "sop" }
    );

    expect(selection).toEqual(["sop"]);
  });

  it("applies favorites and created-by filters", () => {
    const pages = [
      makePage({ id: "match", is_favorite: true, owned_by: "u1" }),
      makePage({ id: "not-favorite", is_favorite: false, owned_by: "u1" }),
      makePage({ id: "other-owner", is_favorite: true, owned_by: "u2" }),
    ];

    expect(select(pages, { filters: { favorites: true, created_by: ["u1"] } })).toEqual(["match"]);
  });

  it("applies created-at date filters", () => {
    const selection = select(
      [
        makePage({ id: "later", created_at: new Date("2026-10-26T00:00:00.000Z") }),
        makePage({ id: "sooner", created_at: new Date("2026-10-20T00:00:00.000Z") }),
      ],
      { filters: { created_at: ["2026-10-26;after"] } }
    );

    expect(selection).toEqual(["later"]);
  });

  it("orders by updated date descending", () => {
    const selection = select([
      makePage({ id: "older", updated_at: new Date("2026-10-01T00:00:00.000Z") }),
      makePage({ id: "newer", updated_at: new Date("2026-10-20T00:00:00.000Z") }),
    ]);

    expect(selection).toEqual(["newer", "older"]);
  });

  it("returns an empty list when no page is in scope", () => {
    expect(select([makePage({ id: "other", project_ids: ["p2"] })])).toEqual([]);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
pnpm exec vitest run core/lib/workspace-pages.test.ts
```

Expected: FAIL because `./workspace-pages` does not exist.

- [ ] **Step 3: Implement the selector**

Create `apps/web/core/lib/workspace-pages.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TPage, TPageFilters, TPageNavigationTabs } from "@plane/types";
import { filterPagesByPageType, getPageName, orderPages, shouldFilterPage } from "@plane/utils";

type TSelectWorkspacePageIdsArgs = {
  pages: TPage[];
  projectIds: string[];
  pageType: TPageNavigationTabs;
  filters?: TPageFilters["filters"];
  searchQuery: string;
  sortKey: TPageFilters["sortKey"];
  sortBy: TPageFilters["sortBy"];
};

export const selectWorkspacePageIds = ({
  pages,
  projectIds,
  pageType,
  filters,
  searchQuery,
  sortKey,
  sortBy,
}: TSelectWorkspacePageIdsArgs): string[] => {
  const projectIdSet = new Set(projectIds);
  const normalizedSearch = searchQuery.trim().toLowerCase();

  const scopedPages = filterPagesByPageType(pageType, pages).filter((page) =>
    page.project_ids?.some((projectId) => projectIdSet.has(projectId))
  );
  const filteredPages = scopedPages.filter(
    (page) => getPageName(page.name).toLowerCase().includes(normalizedSearch) && shouldFilterPage(page, filters)
  );

  return orderPages(filteredPages, sortKey, sortBy)
    .map((page) => page.id)
    .filter((pageId): pageId is string => Boolean(pageId));
};
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
pnpm exec vitest run core/lib/workspace-pages.test.ts
```

Expected: PASS (7 tests).

- [ ] **Step 5: Lint and commit**

```bash
pnpm exec oxlint core/lib/workspace-pages.ts core/lib/workspace-pages.test.ts
pnpm check:types
git add apps/web/core/lib/workspace-pages.ts apps/web/core/lib/workspace-pages.test.ts
git commit -m "feat(web): add workspace page selector"
```

---

## Task 2: Workspace Page Store

**Files:**

- Create: `apps/web/core/store/pages/workspace-page.store.ts`
- Modify: `apps/web/core/hooks/store/use-page-store.ts`
- Modify: `apps/web/core/store/root.store.ts`
- Modify: `apps/web/core/store/pages/project-page.store.ts`

- [ ] **Step 1: Allow `createPage` to receive an explicit project**

In `project-page.store.ts`, update the interface:

```ts
createPage: (pageData: Partial<TPage>, projectId?: string) => Promise<TPage | undefined>;
```

Update the implementation:

```ts
createPage = async (pageData: Partial<TPage>, projectId?: string) => {
  try {
    const { workspaceSlug, projectId: routeProjectId } = this.store.router;
    const resolvedProjectId = projectId ?? routeProjectId;
    if (!workspaceSlug || !resolvedProjectId) return undefined;

    runInAction(() => {
      this.loader = "mutation-loader";
      this.error = undefined;
    });

    const page = await this.service.create(workspaceSlug, resolvedProjectId, pageData);
    runInAction(() => {
      if (page?.id) set(this.data, [page.id], new ProjectPage(this.store, page));
      this.loader = undefined;
    });

    return page;
  } catch (error) {
    runInAction(() => {
      this.loader = undefined;
      this.error = {
        title: "Failed",
        description: "Failed to create a page, Please try again later.",
      };
    });
    throw error;
  }
};
```

- [ ] **Step 2: Implement the workspace store**

Create `apps/web/core/store/pages/workspace-page.store.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set, unset } from "lodash-es";
import { action, computed, makeObservable, observable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { TPage, TPageFilters, TPageNavigationTabs } from "@plane/types";
// helpers
import { filterPagesByPageType } from "@plane/utils";
import { selectWorkspacePageIds } from "@/lib/workspace-pages";
// services
import { ProjectPageService } from "@/services/page";
// store
import type { CoreRootStore } from "../root.store";
import { ProjectPage } from "./project-page";
import type { IProjectPageStore } from "./project-page.store";
import { ROLE_PERMISSIONS_TO_CREATE_PAGE } from "./project-page.store";

type TLoader = "init-loader" | "mutation-loader" | undefined;
type TError = { title: string; description: string };

export interface IWorkspacePageStore extends IProjectPageStore {
  scopeProjectIds: string[];
  setScopeProjectIds: (projectIds: string[]) => void;
}

export class WorkspacePageStore implements IWorkspacePageStore {
  loader: TLoader = "init-loader";
  data: Record<string, ProjectPage> = {};
  error: TError | undefined = undefined;
  filters: TPageFilters = {
    searchQuery: "",
    sortKey: "updated_at",
    sortBy: "desc",
  };
  scopeProjectIds: string[] = [];
  currentWorkspaceSlug: string | undefined = undefined;
  service: ProjectPageService;
  rootStore: CoreRootStore;

  constructor(store: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      data: observable,
      error: observable,
      filters: observable,
      scopeProjectIds: observable.ref,
      isAnyPageAvailable: computed,
      canCurrentUserCreatePage: computed,
      setScopeProjectIds: action,
      updateFilters: action,
      clearAllFilters: action,
      fetchPagesList: action,
      fetchPageDetails: action,
      createPage: action,
      removePage: action,
      movePage: action,
    });
    this.rootStore = store;
    this.service = new ProjectPageService();
  }

  get isAnyPageAvailable() {
    if (this.loader) return true;
    return Object.keys(this.data).length > 0;
  }

  get canCurrentUserCreatePage() {
    const { workspaceSlug } = this.rootStore.router;
    if (!workspaceSlug) return false;

    return this.scopeProjectIds.some((projectId) => {
      const role = this.rootStore.user.permission.getProjectRoleByWorkspaceSlugAndProjectId(
        workspaceSlug.toString(),
        projectId
      );
      return !!role && ROLE_PERMISSIONS_TO_CREATE_PAGE.includes(role);
    });
  }

  private getScopedPages = () => {
    const projectIdSet = new Set(this.scopeProjectIds);
    return Object.values(this.data).filter((page) =>
      page.project_ids?.some((projectId) => projectIdSet.has(projectId))
    );
  };

  getCurrentProjectPageIdsByTab = computedFn((pageType: TPageNavigationTabs) => {
    if (!this.scopeProjectIds.length) return undefined;
    return filterPagesByPageType(pageType, this.getScopedPages())
      .map((page) => page.id)
      .filter(Boolean) as string[];
  });

  getCurrentProjectPageIds = computedFn((projectId: string) => {
    if (!projectId) return [];
    return Object.values(this.data)
      .filter((page) => page.project_ids?.includes(projectId))
      .map((page) => page.id)
      .filter(Boolean) as string[];
  });

  getCurrentProjectFilteredPageIdsByTab = computedFn((pageType: TPageNavigationTabs) => {
    if (!this.scopeProjectIds.length) return undefined;

    return selectWorkspacePageIds({
      pages: Object.values(this.data),
      projectIds: this.scopeProjectIds,
      pageType,
      filters: this.filters.filters,
      searchQuery: this.filters.searchQuery,
      sortKey: this.filters.sortKey,
      sortBy: this.filters.sortBy,
    });
  });

  getPageById = computedFn((pageId: string) => this.data?.[pageId] || undefined);

  updateFilters = <T extends keyof TPageFilters>(filterKey: T, filterValue: TPageFilters[T]) => {
    runInAction(() => {
      set(this.filters, [filterKey], filterValue);
    });
  };

  clearAllFilters = () =>
    runInAction(() => {
      set(this.filters, ["filters"], {});
    });

  setScopeProjectIds = (projectIds: string[]) => {
    runInAction(() => {
      this.scopeProjectIds = projectIds;
    });
  };

  fetchPagesList = async (workspaceSlug: string, _projectId: string, _pageType?: TPageNavigationTabs) => {
    try {
      if (!workspaceSlug) return undefined;

      const workspaceChanged = this.currentWorkspaceSlug !== workspaceSlug;
      runInAction(() => {
        this.loader = workspaceChanged || Object.keys(this.data).length === 0 ? "init-loader" : "mutation-loader";
        this.error = undefined;
        if (workspaceChanged) {
          this.data = {};
          this.currentWorkspaceSlug = workspaceSlug;
        }
      });

      const pages: TPage[] = [];
      const seenCursors = new Set<string>();
      let cursor: string | null = null;

      do {
        const response = await this.service.getWorkspacePages(workspaceSlug, {
          per_page: 100,
          order_by: "-updated_at",
          ...(cursor ? { cursor } : {}),
        });
        pages.push(...((response?.results ?? []) as TPage[]));

        const nextCursor = response?.next_page_results === true && response.next_cursor ? response.next_cursor : null;
        if (nextCursor && seenCursors.has(nextCursor)) break;
        if (nextCursor) seenCursors.add(nextCursor);
        cursor = nextCursor;
      } while (cursor);

      runInAction(() => {
        for (const page of pages) {
          if (page?.id) {
            const existingPage = this.getPageById(page.id);
            if (existingPage) {
              const { name, ...otherFields } = page;
              existingPage.mutateProperties(otherFields, false);
            } else {
              set(this.data, [page.id], new ProjectPage(this.rootStore, page));
            }
          }
        }
        this.loader = undefined;
      });

      return pages;
    } catch (error) {
      runInAction(() => {
        this.loader = undefined;
        this.error = {
          title: "Failed",
          description: "Failed to fetch the pages, Please try again later.",
        };
      });
      throw error;
    }
  };

  fetchPageDetails = async (
    workspaceSlug: string,
    projectId: string,
    pageId: string,
    options?: { trackVisit?: boolean }
  ) => {
    try {
      if (!workspaceSlug || !projectId || !pageId) return undefined;

      const currentPage = this.getPageById(pageId);
      runInAction(() => {
        this.loader = currentPage ? "mutation-loader" : "init-loader";
        this.error = undefined;
      });

      const page = await this.service.fetchById(workspaceSlug, projectId, pageId, options?.trackVisit ?? true);
      runInAction(() => {
        if (page?.id) {
          const pageInstance = this.getPageById(page.id);
          if (pageInstance) pageInstance.mutateProperties(page, false);
          else set(this.data, [page.id], new ProjectPage(this.rootStore, page));
        }
        this.loader = undefined;
      });

      return page;
    } catch (error) {
      runInAction(() => {
        this.loader = undefined;
        this.error = {
          title: "Failed",
          description: "Failed to fetch the page, Please try again later.",
        };
      });
      throw error;
    }
  };

  createPage = async (pageData: Partial<TPage>, projectId?: string) => {
    try {
      const { workspaceSlug } = this.rootStore.router;
      if (!workspaceSlug || !projectId) return undefined;

      runInAction(() => {
        this.loader = "mutation-loader";
        this.error = undefined;
      });

      const page = await this.service.create(workspaceSlug, projectId, pageData);
      runInAction(() => {
        if (page?.id) set(this.data, [page.id], new ProjectPage(this.rootStore, page));
        this.loader = undefined;
      });

      return page;
    } catch (error) {
      runInAction(() => {
        this.loader = undefined;
        this.error = {
          title: "Failed",
          description: "Failed to create a page, Please try again later.",
        };
      });
      throw error;
    }
  };

  removePage = async ({ pageId, shouldSync: _shouldSync = true }: { pageId: string; shouldSync?: boolean }) => {
    try {
      const { workspaceSlug } = this.rootStore.router;
      const page = this.getPageById(pageId);
      const projectId = page?.project_ids?.[0];
      if (!workspaceSlug || !projectId || !pageId) return undefined;

      await this.service.remove(workspaceSlug, projectId, pageId);
      runInAction(() => {
        unset(this.data, [pageId]);
        if (this.rootStore.favorite.entityMap[pageId]) this.rootStore.favorite.removeFavoriteFromStore(pageId);
      });
    } catch (error) {
      runInAction(() => {
        this.loader = undefined;
        this.error = {
          title: "Failed",
          description: "Failed to delete a page, Please try again later.",
        };
      });
      throw error;
    }
  };

  movePage = async (workspaceSlug: string, projectId: string, pageId: string, newProjectId: string) => {
    try {
      await this.service.move(workspaceSlug, projectId, pageId, newProjectId);
      runInAction(() => {
        unset(this.data, [pageId]);
      });
    } catch (error) {
      console.error("Unable to move page", error);
      throw error;
    }
  };
}
```

- [ ] **Step 3: Register the store type and root store instance**

In `apps/web/core/hooks/store/use-page-store.ts`:

```ts
import type { IProjectPageStore } from "@/store/pages/project-page.store";
import type { IWorkspacePageStore } from "@/store/pages/workspace-page.store";

export enum EPageStoreType {
  PROJECT = "PROJECT_PAGE",
  WORKSPACE = "WORKSPACE_PAGE",
}

export type TReturnType = {
  [EPageStoreType.PROJECT]: IProjectPageStore;
  [EPageStoreType.WORKSPACE]: IWorkspacePageStore;
};

export const usePageStore = <T extends EPageStoreType>(storeType: T): TReturnType[T] => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("usePageStore must be used within StoreProvider");

  if (storeType === EPageStoreType.PROJECT) {
    return context.projectPages;
  }
  if (storeType === EPageStoreType.WORKSPACE) {
    return context.workspacePages;
  }

  throw new Error(`Invalid store type: ${storeType}`);
};
```

In `apps/web/core/store/root.store.ts` add the import:

```ts
import type { IWorkspacePageStore } from "./pages/workspace-page.store";
import { WorkspacePageStore } from "./pages/workspace-page.store";
```

Add the field:

```ts
projectPages: IProjectPageStore;
workspacePages: IWorkspacePageStore;
```

Add to both constructor branches after `this.projectPages = new ProjectPageStore(this);`:

```ts
this.workspacePages = new WorkspacePageStore(this);
```

- [ ] **Step 4: Type-check and commit**

```bash
pnpm exec oxlint core/store/pages/workspace-page.store.ts core/store/pages/project-page.store.ts core/hooks/store/use-page-store.ts core/store/root.store.ts
pnpm check:types
git add apps/web/core/store/pages/workspace-page.store.ts apps/web/core/store/pages/project-page.store.ts apps/web/core/hooks/store/use-page-store.ts apps/web/core/store/root.store.ts
git commit -m "feat(web): add workspace page store"
```

---

## Task 3: Scope-Aware Pages List Shell

**Files:**

- Modify: `apps/web/core/components/pages/pages-list-view.tsx`
- Modify: `apps/web/core/components/pages/header/root.tsx`
- Modify: `apps/web/core/components/pages/list/tab-navigation.tsx`
- Modify: `apps/web/core/components/pages/pages-list-main-content.tsx`

- [ ] **Step 1: Make the view workspace-aware**

Replace `pages-list-view.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import useSWR from "swr";
// plane imports
import type { EPageAccess } from "@plane/constants";
import type { TPageNavigationTabs } from "@plane/types";
// hooks
import { EPageStoreType, usePageStore } from "@/hooks/store";
// local imports
import { PagesListHeaderRoot } from "./header";
import { PagesListMainContent } from "./pages-list-main-content";

type TPageView = {
  children: React.ReactNode;
  pageType: TPageNavigationTabs;
  projectId?: string;
  storeType: EPageStoreType;
  workspaceSlug: string;
  canCreate?: boolean;
  onCreatePage?: (access: EPageAccess) => void | Promise<void>;
  emptyStateVariant?: "workspace";
};

export const PagesListView = observer(function PagesListView(props: TPageView) {
  const { children, pageType, projectId, storeType, workspaceSlug, canCreate, onCreatePage, emptyStateVariant } = props;
  // store hooks
  const { isAnyPageAvailable, fetchPagesList } = usePageStore(storeType);
  const isWorkspaceStore = storeType === EPageStoreType.WORKSPACE;
  const fetchKey =
    workspaceSlug && pageType
      ? isWorkspaceStore
        ? `WORKSPACE_PAGES_${workspaceSlug}`
        : projectId
          ? `PROJECT_PAGES_${projectId}`
          : null
      : null;

  useSWR(fetchKey, fetchKey ? () => fetchPagesList(workspaceSlug, projectId ?? "", pageType) : null);

  return (
    <div className="relative flex h-full w-full flex-col overflow-hidden">
      {isAnyPageAvailable && (
        <PagesListHeaderRoot
          pageType={pageType}
          projectId={projectId}
          storeType={storeType}
          workspaceSlug={workspaceSlug}
        />
      )}
      <PagesListMainContent
        pageType={pageType}
        storeType={storeType}
        createProjectId={projectId}
        canCreate={canCreate}
        onCreatePage={onCreatePage}
        emptyStateVariant={emptyStateVariant}
      >
        {children}
      </PagesListMainContent>
    </div>
  );
});
```

- [ ] **Step 2: Support workspace tab URLs**

In `header/root.tsx`, change the prop type:

```ts
type Props = {
  pageType: TPageNavigationTabs;
  projectId?: string;
  storeType: EPageStoreType;
  workspaceSlug: string;
};
```

No other change is needed in this file.

Replace `list/tab-navigation.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
// types
import type { TPageNavigationTabs } from "@plane/types";
// helpers
import { cn } from "@plane/utils";

type TPageTabNavigation = {
  workspaceSlug: string;
  projectId?: string;
  pageType: TPageNavigationTabs;
};

const pageTabs: { key: TPageNavigationTabs; label: string }[] = [
  { key: "public", label: "Public" },
  { key: "private", label: "Private" },
  { key: "archived", label: "Archived" },
];

export function PageTabNavigation(props: TPageTabNavigation) {
  const { workspaceSlug, projectId, pageType } = props;
  const basePath = projectId ? `/${workspaceSlug}/projects/${projectId}/pages` : `/${workspaceSlug}/pages`;

  const handleTabClick = (e: React.MouseEvent<HTMLAnchorElement>, tabKey: TPageNavigationTabs) => {
    if (tabKey === pageType) e.preventDefault();
  };

  return (
    <div className="relative flex h-full items-center">
      {pageTabs.map((tab) => (
        <Link
          key={tab.key}
          href={`${basePath}?type=${tab.key}`}
          onClick={(e) => handleTabClick(e, tab.key)}
          className="flex h-full flex-col"
        >
          <div
            className={cn("flex flex-1 items-center justify-center px-4 text-13 font-medium transition-all", {
              "text-accent-primary": tab.key === pageType,
            })}
          >
            {tab.label}
          </div>
          <div
            className={cn("w-full rounded-t border-t-2 border-transparent transition-all", {
              "border-accent-strong": tab.key === pageType,
            })}
          />
        </Link>
      ))}
    </div>
  );
}
```

- [ ] **Step 3: Make main content use the selected store and support workspace create/empty**

Replace `pages-list-main-content.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
// plane imports
import { EPageAccess } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TPage, TPageNavigationTabs } from "@plane/types";
// components
import { PageLoader } from "@/components/pages/loaders/page-loader";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { usePageStore } from "@/hooks/store";
import type { EPageStoreType } from "@/hooks/store";

type Props = {
  children: React.ReactNode;
  pageType: TPageNavigationTabs;
  storeType: EPageStoreType;
  createProjectId?: string;
  canCreate?: boolean;
  onCreatePage?: (access: EPageAccess) => void | Promise<void>;
  emptyStateVariant?: "workspace";
};

export const PagesListMainContent = observer(function PagesListMainContent(props: Props) {
  const { children, pageType, storeType, createProjectId, canCreate, onCreatePage, emptyStateVariant } = props;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { currentProjectDetails } = useProject();
  const {
    canCurrentUserCreatePage,
    createPage,
    getCurrentProjectFilteredPageIdsByTab,
    getCurrentProjectPageIdsByTab,
    isAnyPageAvailable,
    loader,
  } = usePageStore(storeType);
  // states
  const [isCreatingPage, setIsCreatingPage] = useState(false);
  // router
  const router = useRouter();
  const { workspaceSlug } = useParams();
  // derived values
  const pageIds = getCurrentProjectPageIdsByTab(pageType);
  const filteredPageIds = getCurrentProjectFilteredPageIdsByTab(pageType);
  const canPerformEmptyStateActions = canCreate ?? canCurrentUserCreatePage;

  const handleCreatePage = async () => {
    setIsCreatingPage(true);
    const access = pageType === "private" ? EPageAccess.PRIVATE : EPageAccess.PUBLIC;

    try {
      if (onCreatePage) {
        await onCreatePage(access);
        return;
      }

      const page = await createPage({ access }, createProjectId);
      const redirectProjectId = createProjectId ?? currentProjectDetails?.id ?? page?.project_ids?.[0];
      if (page?.id && redirectProjectId) {
        router.push(`/${workspaceSlug}/projects/${redirectProjectId}/pages/${page.id}`);
      }
    } catch (error) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          (error as { data?: { error?: string } })?.data?.error || "Page could not be created. Please try again.",
      });
    } finally {
      setIsCreatingPage(false);
    }
  };

  if (loader === "init-loader") return <PageLoader />;

  if (!isAnyPageAvailable) {
    if (emptyStateVariant === "workspace") {
      return (
        <EmptyStateDetailed
          assetKey="page"
          title={t("workspace_empty_state.kb.title")}
          description={t("workspace_empty_state.kb.description")}
        />
      );
    }

    return (
      <EmptyStateDetailed
        assetKey="page"
        title={t("project_empty_state.pages.title")}
        description={t("project_empty_state.pages.description")}
        actions={[
          {
            label: t("project_empty_state.pages.cta_primary"),
            onClick: handleCreatePage,
            variant: "primary",
            disabled: !canPerformEmptyStateActions || isCreatingPage,
          },
        ]}
      />
    );
  }

  if (pageIds?.length === 0) {
    if (pageType === "archived") {
      return (
        <EmptyStateDetailed
          assetKey="page"
          title={t("project_empty_state.archive_pages.title")}
          description={t("project_empty_state.archive_pages.description")}
        />
      );
    }

    return (
      <EmptyStateDetailed
        assetKey="page"
        title={t("project_empty_state.pages.title")}
        description={t("project_empty_state.pages.description")}
        actions={[
          {
            label: t("project_empty_state.pages.cta_primary"),
            onClick: handleCreatePage,
            variant: "primary",
            disabled: !canPerformEmptyStateActions || isCreatingPage,
          },
        ]}
      />
    );
  }

  if (filteredPageIds?.length === 0) {
    return (
      <EmptyStateDetailed
        assetKey="search"
        title={t("common_empty_state.search.title")}
        description={t("common_empty_state.search.description")}
      />
    );
  }

  return <div className="h-full w-full overflow-hidden">{children}</div>;
});
```

- [ ] **Step 4: Verify project Pages and type-check**

```bash
pnpm exec oxlint core/components/pages/pages-list-view.tsx core/components/pages/header/root.tsx core/components/pages/list/tab-navigation.tsx core/components/pages/pages-list-main-content.tsx
pnpm check:types
```

Expected: both exit 0. Project pages still pass the route `projectId` and use `EPageStoreType.PROJECT`.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/pages/pages-list-view.tsx apps/web/core/components/pages/header/root.tsx apps/web/core/components/pages/list/tab-navigation.tsx apps/web/core/components/pages/pages-list-main-content.tsx
git commit -m "feat(web): support workspace page lists"
```

---

## Task 4: Project Badges in Workspace Page Rows

**Files:**

- Modify: `apps/web/core/components/pages/list/root.tsx`
- Modify: `apps/web/core/components/pages/list/block.tsx`

- [ ] **Step 1: Add the badge prop to the list root**

In `list/root.tsx`:

```ts
type TPagesListRoot = {
  pageType: TPageNavigationTabs;
  storeType: EPageStoreType;
  showProjectBadge?: boolean;
};

export const PagesListRoot = observer(function PagesListRoot(props: TPagesListRoot) {
  const { pageType, storeType, showProjectBadge = false } = props;
  // ...
  return (
    <ListLayout>
      {filteredPageIds.map((pageId) => (
        <PageListBlock key={pageId} pageId={pageId} storeType={storeType} showProjectBadge={showProjectBadge} />
      ))}
    </ListLayout>
  );
});
```

- [ ] **Step 2: Render the badge in each row**

In `list/block.tsx`, add:

```ts
import { useProject } from "@/hooks/store/use-project";
```

```ts
type TPageListBlock = {
  pageId: string;
  storeType: EPageStoreType;
  showProjectBadge?: boolean;
};

export const PageListBlock = observer(function PageListBlock(props: TPageListBlock) {
  const { pageId, storeType, showProjectBadge = false } = props;
  // ...
  const { getProjectById } = useProject();
  const page = usePage({ pageId, storeType });
  const project = showProjectBadge && page?.project_ids?.[0] ? getProjectById(page.project_ids[0]) : undefined;
  if (!page) return null;
  // ...
  return (
    <ListItem
      // existing props
      appendTitleElement={
        project ? (
          <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5 text-11 text-tertiary">
            {project.name}
          </span>
        ) : undefined
      }
      // existing props
    />
  );
});
```

Keep `title`, logo, `itemLink`, `actionableItems`, `isMobile`, and `parentRef` unchanged.

- [ ] **Step 3: Lint and commit**

```bash
pnpm exec oxlint core/components/pages/list/root.tsx core/components/pages/list/block.tsx
pnpm check:types
git add apps/web/core/components/pages/list/root.tsx apps/web/core/components/pages/list/block.tsx
git commit -m "feat(web): show project badges on workspace pages"
```

---

## Task 5: Workspace Page Permissions and Create Provider

**Files:**

- Create: `apps/web/core/hooks/use-workspace-page-permissions.ts`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/create-page-provider.tsx`

- [ ] **Step 1: Add the permissions hook**

Create `apps/web/core/hooks/use-workspace-page-permissions.ts`:

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

export const useWorkspacePagePermissions = () => {
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
    return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.page_view !== false;
  };

  const canCreatePage = scopedProjectId
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

  return { canCreatePage };
};
```

- [ ] **Step 2: Add the create provider**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/create-page-provider.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import { createContext, useCallback, useContext, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { EPageAccess } from "@plane/constants";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// components
import { WorkspaceApplicationPickerModal } from "@/components/workspace/application-scope/project-picker-modal";
// hooks
import { usePageStore } from "@/hooks/store";
import { EPageStoreType } from "@/hooks/store";
import { useProject } from "@/hooks/store/use-project";
import { useUser } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspacePagePermissions } from "@/hooks/use-workspace-page-permissions";

type TWorkspacePagesCreateContext = {
  canCreatePage: boolean;
  isCreatingPage: boolean;
  requestCreatePage: (access?: EPageAccess) => Promise<void>;
};

const WorkspacePagesCreateContext = createContext<TWorkspacePagesCreateContext | null>(null);

export const useWorkspacePagesCreate = () => {
  const context = useContext(WorkspacePagesCreateContext);
  if (!context) throw new Error("useWorkspacePagesCreate must be used within WorkspacePagesCreateProvider");
  return context;
};

export const WorkspacePagesCreateProvider = observer(function WorkspacePagesCreateProvider({
  children,
}: {
  children: ReactNode;
}) {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // scope
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  // store hooks
  const { createPage } = usePageStore(EPageStoreType.WORKSPACE);
  const { workspaceProjectIds, getProjectById } = useProject();
  const { projectsWithCreatePermissions } = useUser();
  const { canCreatePage } = useWorkspacePagePermissions();
  // states
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [pendingAccess, setPendingAccess] = useState<EPageAccess>(EPageAccess.PUBLIC);
  const [isCreatingPage, setIsCreatingPage] = useState(false);

  const eligibleProjectIds = useMemo(
    () =>
      (workspaceProjectIds ?? []).filter((projectId) => {
        const project = getProjectById(projectId);
        return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.page_view !== false;
      }),
    [getProjectById, projectsWithCreatePermissions, workspaceProjectIds]
  );

  const createInProject = useCallback(
    async (projectId: string, access: EPageAccess) => {
      if (!workspaceSlug) return;
      setIsCreatingPage(true);
      try {
        const page = await createPage({ access }, projectId);
        if (page?.id) router.push(`/${workspaceSlug}/projects/${projectId}/pages/${page.id}`);
      } catch (error) {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: "Error!",
          message:
            (error as { data?: { error?: string } })?.data?.error || "Page could not be created. Please try again.",
        });
      } finally {
        setIsCreatingPage(false);
      }
    },
    [createPage, router, workspaceSlug]
  );

  const requestCreatePage = useCallback(
    async (access: EPageAccess = EPageAccess.PUBLIC) => {
      if (scopedProjectId) {
        await createInProject(scopedProjectId, access);
        return;
      }
      setPendingAccess(access);
      setIsPickerOpen(true);
    },
    [createInProject, scopedProjectId]
  );

  const handleClose = () => {
    setIsPickerOpen(false);
  };

  const handleSelect = (projectId: string) => {
    setIsPickerOpen(false);
    void createInProject(projectId, pendingAccess);
  };

  const value = useMemo(
    () => ({ canCreatePage, isCreatingPage, requestCreatePage }),
    [canCreatePage, isCreatingPage, requestCreatePage]
  );

  return (
    <WorkspacePagesCreateContext.Provider value={value}>
      {children}
      <WorkspaceApplicationPickerModal
        isOpen={isPickerOpen}
        handleClose={handleClose}
        projectIds={eligibleProjectIds}
        onSelect={handleSelect}
      />
    </WorkspacePagesCreateContext.Provider>
  );
});
```

- [ ] **Step 3: Lint and type-check**

```bash
pnpm exec oxlint core/hooks/use-workspace-page-permissions.ts 'app/(all)/[workspaceSlug]/(projects)/pages/create-page-provider.tsx'
pnpm check:types
```

Expected: both exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/hooks/use-workspace-page-permissions.ts 'apps/web/app/(all)/[workspaceSlug]/(projects)/pages/create-page-provider.tsx'
git commit -m "feat(web): add workspace page create flow"
```

---

## Task 6: Workspace Pages Header, Layout, and Page

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/header.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/layout.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/pages/page.tsx`
- Delete: `apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx`
- Delete: `apps/web/core/lib/workspace-kb.ts`
- Delete: `apps/web/core/lib/workspace-kb.test.ts`

- [ ] **Step 1: Add the workspace header**

Create `header.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { EPageAccess } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { PagesOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { useWorkspacePagesCreate } from "./create-page-provider";

export const WorkspacePagesHeader = observer(function WorkspacePagesHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  // i18n
  const { t } = useTranslation();
  // create flow
  const { canCreatePage, isCreatingPage, requestCreatePage } = useWorkspacePagesCreate();
  const access = searchParams.get("type") === "private" ? EPageAccess.PRIVATE : EPageAccess.PUBLIC;

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.pages")}
                  href={`/${workspaceSlug}/pages/`}
                  icon={<PagesOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          <WorkspaceApplicationScopeChip />
        </div>
      </Header.LeftItem>
      {canCreatePage && (
        <Header.RightItem>
          <Button variant="primary" size="lg" loading={isCreatingPage} onClick={() => void requestCreatePage(access)}>
            Add page
          </Button>
        </Header.RightItem>
      )}
    </Header>
  );
});
```

- [ ] **Step 2: Wrap the layout in the provider**

Replace `layout.tsx` with:

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
import { WorkspacePagesCreateProvider } from "./create-page-provider";
import { WorkspacePagesHeader } from "./header";

function WorkspacePagesLayout() {
  return (
    <WorkspacePagesCreateProvider>
      <AppHeader header={<WorkspacePagesHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </WorkspacePagesCreateProvider>
  );
}

export default observer(WorkspacePagesLayout);
```

- [ ] **Step 3: Replace the workspace page**

Replace `page.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo } from "react";
import { observer } from "mobx-react";
import { useTheme } from "next-themes";
import { useParams, useSearchParams } from "next/navigation";
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TPageNavigationTabs } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
// assets
import darkPagesAsset from "@/app/assets/empty-state/disabled-feature/pages-dark.webp?url";
import lightPagesAsset from "@/app/assets/empty-state/disabled-feature/pages-light.webp?url";
// components
import { PageHead } from "@/components/core/page-title";
import { DetailedEmptyState } from "@/components/empty-state/detailed-empty-state-root";
import { PagesListRoot } from "@/components/pages/list/root";
import { PagesListView } from "@/components/pages/pages-list-view";
import { PageLoader } from "@/components/pages/loaders/page-loader";
// hooks
import { usePageStore } from "@/hooks/store";
import { EPageStoreType } from "@/hooks/store";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
// local imports
import { useWorkspacePagesCreate } from "./create-page-provider";

const getPageType = (pageType?: string | null): TPageNavigationTabs => {
  if (pageType === "private") return "private";
  if (pageType === "archived") return "archived";
  return "public";
};

function WorkspacePagesPage() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const searchParams = useSearchParams();
  const pageType = getPageType(searchParams.get("type"));
  // theme
  const { resolvedTheme } = useTheme();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workspaceProjectIds, getProjectById, isInitializingProjects } = useProject();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { canCreatePage, requestCreatePage } = useWorkspacePagesCreate();
  const { setScopeProjectIds } = usePageStore(EPageStoreType.WORKSPACE);
  const { allowPermissions } = useUserPermissions();

  // derived values
  const projectIds = useMemo(
    () => (scopedProjectId ? [scopedProjectId] : (workspaceProjectIds ?? [])),
    [scopedProjectId, workspaceProjectIds]
  );
  const scopedProject = scopedProjectId ? getProjectById(scopedProjectId) : undefined;
  const resolvedEmptyState = resolvedTheme === "light" ? lightPagesAsset : darkPagesAsset;
  const canPerformFeatureAction = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    scopedProjectId ?? undefined
  );
  const isFeatureDisabled = Boolean(scopedProjectId && scopedProject && scopedProject.page_view === false);

  useEffect(() => {
    setScopeProjectIds(projectIds);
  }, [projectIds, setScopeProjectIds]);

  if (!workspaceSlug) return null;
  if (isInitializingProjects || workspaceProjectIds === undefined) return <PageLoader />;

  if (isFeatureDisabled) {
    return (
      <div className="flex h-full w-full items-center justify-center">
        <DetailedEmptyState
          title={t("disabled_project.empty_state.page.title")}
          description={t("disabled_project.empty_state.page.description")}
          assetPath={resolvedEmptyState}
          primaryButton={{
            text: t("disabled_project.empty_state.page.primary_button.text"),
            onClick: () => router.push(`/${workspaceSlug}/settings/projects/${scopedProjectId}/features`),
            disabled: !canPerformFeatureAction,
          }}
        />
      </div>
    );
  }

  return (
    <>
      <PageHead title={t("sidebar.pages")} />
      <PagesListView
        pageType={pageType}
        projectId={scopedProjectId ?? undefined}
        storeType={EPageStoreType.WORKSPACE}
        workspaceSlug={workspaceSlug}
        canCreate={canCreatePage}
        onCreatePage={requestCreatePage}
        emptyStateVariant={scopedProjectId ? undefined : "workspace"}
      >
        <PagesListRoot pageType={pageType} storeType={EPageStoreType.WORKSPACE} showProjectBadge={!scopedProjectId} />
      </PagesListView>
    </>
  );
}

export default observer(WorkspacePagesPage);
```

- [ ] **Step 4: Delete the legacy implementation**

```bash
rm apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx
rm apps/web/core/lib/workspace-kb.ts
rm apps/web/core/lib/workspace-kb.test.ts
```

- [ ] **Step 5: Lint, type-check, and commit**

```bash
pnpm exec oxlint 'app/(all)/[workspaceSlug]/(projects)/pages/header.tsx' 'app/(all)/[workspaceSlug]/(projects)/pages/page.tsx' 'app/(all)/[workspaceSlug]/(projects)/pages/layout.tsx'
pnpm check:types
git add 'apps/web/app/(all)/[workspaceSlug]/(projects)/pages/header.tsx' 'apps/web/app/(all)/[workspaceSlug]/(projects)/pages/page.tsx' 'apps/web/app/(all)/[workspaceSlug]/(projects)/pages/layout.tsx'
git rm apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx apps/web/core/lib/workspace-kb.ts apps/web/core/lib/workspace-kb.test.ts
git commit -m "feat(web): use project pages view for workspace pages"
```

---

## Task 7: Full Verification and Deploy

**Files:**

- No source changes unless verification finds a defect.

- [ ] **Step 1: Run tests, lint, and type checks**

```bash
pnpm exec vitest run
pnpm check:lint
pnpm check:types
```

Expected: all exit 0.

- [ ] **Step 2: Build and restart**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
sleep 20
for path in pages modules cycles work-items; do printf "%s=" "$path"; curl -s -o /dev/null -w "%{http_code}\n" "http://localhost:3000/itsm/$path"; done
```

Expected: builds exit 0; all routes return 200.

- [ ] **Step 3: Manual smoke**

Verify:

- `/itsm/pages` uses the project Pages tabs/list, not the legacy grouped aggregate list.
- All applications: Public/Private/Archived tabs, search, order, filters, favorites, project badges; Links open the page under its project.
- Scoped application: tabs/chip/search/filters work; Add page creates directly in the scoped application and navigates to it.
- All applications: Add page opens the application picker, then creates and navigates.
- Empty All, empty scoped, filtered-empty, and disabled-feature states match project Pages.
- Project Pages at `/itsm/projects/<projectId>/pages/` still behave exactly as before.

Fix any failure, rerun Steps 1-2, and commit with:

```bash
git add <changed-files>
git commit -m "fix(web): address workspace pages verification"
```

---

## Self-Review Notes

- Spec 7.6: reuses `PagesListView` with Public/Private/Archived tabs, grid/list (list layout in the current implementation), favorites, search, project badges, and application-picker create.
- Spec 6.3: `EPageStoreType.WORKSPACE` is backed by `/api/workspaces/:slug/pages/`; scope filtering is done by the workspace store/selector.
- Spec 6.4: `PagesListRoot`/`PageListBlock` gain optional badge behavior; project mode is unchanged.
- Spec 7.8: workspace empty uses `workspace_empty_state.kb`; scoped empty uses project page keys; disabled feature uses the project page disabled state; loaders are retained.
- Spec 7.9: scope chip remains in the header; tabs and filters stay in the page header.
- Spec 8/9: create resolves the scoped project or opens the picker; permissions use the resolved project.
- No new i18n keys; the add button text is hardcoded like the existing project Pages header.

---

## Execution notes (2026-10-22)

- The workspace page store paginates the workspace pages endpoint with `per_page=100` and `next_cursor`, and resets its data when the workspace changes.
- Workspace tabs preserve the active `?project=` scope, so the URL remains a valid deep link.
- The create provider resolves the scoped project directly; when All is selected it opens the application picker and creates in the selected project.
- Deleted the legacy workspace KB view/helper/tests; project Pages and the `/kb` redirect remain unchanged.
- Verification: 509 web tests pass, `pnpm check:types` passes, `pnpm check:lint` exits 0, OxLint reports 0 on all touched files, `@plane/constants` + web builds pass, `plane-web-prod` restarted and `/itsm/pages`, `/itsm/modules`, `/itsm/cycles`, `/itsm/work-items` return 200.
- Manual browser smoke test was not run in this environment; the HTTP/build/test checks above are the available evidence.
