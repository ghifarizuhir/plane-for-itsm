/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback } from "react";
import { useSearchParams } from "next/navigation";
// plane imports
import {
  DEFAULT_SERVICE_DETAIL_PANE_TAB,
  resolveServiceDetailPaneTab,
  SERVICE_DETAIL_PANE_TABS_QUERY_PARAM,
} from "@/components/services/detail/pane/helpers";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useQueryParams } from "@/hooks/use-query-params";

export const useServiceDetailPane = () => {
  // router
  const router = useAppRouter();
  const searchParams = useSearchParams();
  const { updateQueryParams } = useQueryParams();
  // derived values
  const activeTab = resolveServiceDetailPaneTab(searchParams.get(SERVICE_DETAIL_PANE_TABS_QUERY_PARAM));

  const openPane = useCallback(() => {
    const updatedRoute = updateQueryParams({
      paramsToAdd: {
        [SERVICE_DETAIL_PANE_TABS_QUERY_PARAM]: DEFAULT_SERVICE_DETAIL_PANE_TAB,
      },
    });
    router.push(updatedRoute);
  }, [router, updateQueryParams]);

  const setActiveTab = useCallback(
    (value: string) => {
      const tab = resolveServiceDetailPaneTab(value);
      if (!tab) return;
      const updatedRoute = updateQueryParams({
        paramsToAdd: { [SERVICE_DETAIL_PANE_TABS_QUERY_PARAM]: tab },
      });
      router.push(updatedRoute);
    },
    [router, updateQueryParams]
  );

  const closePane = useCallback(() => {
    const updatedRoute = updateQueryParams({
      paramsToRemove: [SERVICE_DETAIL_PANE_TABS_QUERY_PARAM],
    });
    router.push(updatedRoute);
  }, [router, updateQueryParams]);

  return {
    activeTab: activeTab ?? DEFAULT_SERVICE_DETAIL_PANE_TAB,
    isOpen: activeTab !== null,
    openPane,
    setActiveTab,
    closePane,
  };
};
