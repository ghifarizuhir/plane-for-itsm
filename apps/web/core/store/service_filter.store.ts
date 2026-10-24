/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, computed, observable, makeObservable, runInAction, reaction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type {
  TServiceDisplayFilters,
  TServiceFilters,
  TServiceGroupByOptions,
  TServiceLayoutOptions,
} from "@plane/types";
// helpers
import { storage } from "@/lib/local-storage";
// store
import type { CoreRootStore } from "./root.store";

const SERVICE_DISPLAY_FILTERS_KEY = "service_display_filters";
const SERVICE_FILTERS_KEY = "service_filters";

export interface IServiceFilterStore {
  displayFilters: Record<string, TServiceDisplayFilters>;
  filters: Record<string, TServiceFilters>;
  searchQuery: string;
  currentProjectDisplayFilters: TServiceDisplayFilters | undefined;
  currentProjectFilters: TServiceFilters | undefined;
  getDisplayFiltersByProjectId: (projectId: string) => TServiceDisplayFilters | undefined;
  getFiltersByProjectId: (projectId: string) => TServiceFilters;
  updateDisplayFilters: (projectId: string, displayFilters: Partial<TServiceDisplayFilters>) => void;
  updateFilters: (projectId: string, filters: Partial<TServiceFilters>) => void;
  updateSearchQuery: (query: string) => void;
  initProjectServiceFilters: (projectId: string) => void;
  clearAllFilters: (projectId: string) => void;
}

export class ServiceFilterStore implements IServiceFilterStore {
  displayFilters: Record<string, TServiceDisplayFilters> = {};
  filters: Record<string, TServiceFilters> = {};
  searchQuery: string = "";
  rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      displayFilters: observable,
      filters: observable,
      searchQuery: observable.ref,
      currentProjectDisplayFilters: computed,
      currentProjectFilters: computed,
      updateDisplayFilters: action,
      updateFilters: action,
      updateSearchQuery: action,
      clearAllFilters: action,
    });
    this.rootStore = _rootStore;

    reaction(
      () => this.rootStore.router.projectId,
      (projectId) => {
        if (!projectId) return;
        this.initProjectServiceFilters(projectId);
        this.searchQuery = "";
      }
    );

    this.loadFromLocalStorage();
  }

  loadFromLocalStorage = () => {
    try {
      const displayFiltersData = storage.get(SERVICE_DISPLAY_FILTERS_KEY);
      const filtersData = storage.get(SERVICE_FILTERS_KEY);
      runInAction(() => {
        if (displayFiltersData) {
          const parsed = JSON.parse(displayFiltersData);
          if (typeof parsed === "object" && parsed !== null) this.displayFilters = parsed;
        }
        if (filtersData) {
          const parsed = JSON.parse(filtersData);
          if (typeof parsed === "object" && parsed !== null) this.filters = parsed;
        }
      });
    } catch (error) {
      console.error("Failed to load service filters from localStorage:", error);
      runInAction(() => {
        this.displayFilters = {};
        this.filters = {};
      });
    }
  };

  saveDisplayFiltersToLocalStorage = () => {
    storage.set(SERVICE_DISPLAY_FILTERS_KEY, this.displayFilters);
  };

  saveFiltersToLocalStorage = () => {
    storage.set(SERVICE_FILTERS_KEY, this.filters);
  };

  get currentProjectDisplayFilters() {
    const projectId = this.rootStore.router.projectId;
    if (!projectId) return;
    return this.displayFilters[projectId];
  }

  get currentProjectFilters() {
    const projectId = this.rootStore.router.projectId;
    if (!projectId) return;
    return this.filters[projectId] ?? {};
  }

  getDisplayFiltersByProjectId = computedFn((projectId: string) => this.displayFilters[projectId]);

  getFiltersByProjectId = computedFn((projectId: string) => this.filters[projectId] ?? {});

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

  updateDisplayFilters = (projectId: string, displayFilters: Partial<TServiceDisplayFilters>) => {
    runInAction(() => {
      Object.keys(displayFilters).forEach((key) => {
        set(this.displayFilters, [projectId, key], displayFilters[key as keyof TServiceDisplayFilters]);
      });
    });
    this.saveDisplayFiltersToLocalStorage();
  };

  updateFilters = (projectId: string, filters: Partial<TServiceFilters>) => {
    runInAction(() => {
      Object.keys(filters).forEach((key) => {
        set(this.filters, [projectId, key], filters[key as keyof TServiceFilters]);
      });
    });
    this.saveFiltersToLocalStorage();
  };

  updateSearchQuery = (query: string) => {
    this.searchQuery = query;
  };

  clearAllFilters = (projectId: string) => {
    runInAction(() => {
      this.filters[projectId] = {};
    });
    this.saveFiltersToLocalStorage();
  };
}
