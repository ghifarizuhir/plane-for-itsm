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
