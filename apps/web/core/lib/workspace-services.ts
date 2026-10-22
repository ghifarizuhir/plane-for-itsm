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
