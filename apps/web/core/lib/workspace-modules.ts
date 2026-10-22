/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IModule, TModuleDisplayFilters, TModuleFilters } from "@plane/types";
import { orderModules, shouldFilterModule } from "@plane/utils";

export const WORKSPACE_MODULE_FILTER_KEY = "__workspace__";

export type TWorkspaceModuleSelection = {
  moduleIds: string[];
  totalCount: number;
};

type TSelectWorkspaceModulesArgs = {
  modules: IModule[];
  projectIds: string[];
  displayFilters?: TModuleDisplayFilters;
  filters?: TModuleFilters;
  searchQuery: string;
  projectAllowed?: (projectId: string) => boolean;
};

export const selectWorkspaceModules = ({
  modules,
  projectIds,
  displayFilters,
  filters,
  searchQuery,
  projectAllowed,
}: TSelectWorkspaceModulesArgs): TWorkspaceModuleSelection => {
  const projectIdSet = new Set(projectIds);
  const normalizedSearch = searchQuery.trim().toLowerCase();

  const filteredModules = modules
    .filter((module) => projectIdSet.has(module.project_id))
    .filter((module) => !module.archived_at)
    .filter((module) => (projectAllowed ? projectAllowed(module.project_id) : true))
    .filter((module) => module.name.toLowerCase().includes(normalizedSearch))
    .filter((module) => shouldFilterModule(module, displayFilters ?? {}, filters ?? {}));

  const orderedModules = orderModules(filteredModules, displayFilters?.order_by ?? "name");

  return {
    moduleIds: orderedModules.map((module) => module.id),
    totalCount: filteredModules.length,
  };
};
