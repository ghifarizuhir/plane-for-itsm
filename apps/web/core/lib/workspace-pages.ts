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
