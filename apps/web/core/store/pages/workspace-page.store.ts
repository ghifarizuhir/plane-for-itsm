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
        // oxlint-disable-next-line no-await-in-loop -- cursor pagination is inherently sequential
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
