/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 *
 * Workspace work-items layout root — 1:1 with project ProjectLayoutRoot.
 * Uses GLOBAL store with fixed viewId "all-issues" so workspace /work-items
 * renders the exact same table stack as project /issues:
 * List / Kanban / Calendar / Gantt / Spreadsheet + filters + peek overview.
 */

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
import { IssuesStoreContext } from "@/hooks/use-issue-layout-store";
import { useMobileViewport } from "@/hooks/use-mobile-viewport";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
import { useWorkspaceIssueProperties } from "@/hooks/use-workspace-issue-properties";
// local
import { WORKSPACE_WORK_ITEMS_VIEW_ID } from "@/lib/workspace-work-items";
import { resolveWorkItemLayout } from "../mobile-layout";

export const WorkspaceWorkItemsLayoutRoot = observer(function WorkspaceWorkItemsLayoutRoot() {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug ? routerWorkspaceSlug.toString() : undefined;
  const viewId = WORKSPACE_WORK_ITEMS_VIEW_ID;
  // hooks
  const isMobileViewport = useMobileViewport();
  const searchParams = useSearchParams();
  useWorkspaceIssueProperties(workspaceSlug);

  const { isCreateIssueModalOpen } = useCommandPalette();
  const {
    issuesFilter: { filters, fetchFilters, updateFilterExpression },
    issues: { fetchIssuesWithExistingPagination, fetchNextIssues },
  } = useIssues(EIssuesStoreType.GLOBAL);
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();

  // derived
  const workItemFilters = filters?.[viewId];
  const activeLayout: EIssueLayoutTypes | undefined = resolveWorkItemLayout(
    workItemFilters?.displayFilters,
    isMobileViewport
  );

  const initialWorkItemFilters = useMemo(
    () => ({
      displayFilters: workItemFilters?.displayFilters,
      displayProperties: workItemFilters?.displayProperties,
      kanbanFilters: workItemFilters?.kanbanFilters,
      richFilters: workItemFilters?.richFilters ?? {},
    }),
    [workItemFilters]
  );

  // fetch filters (mirrors ProjectLayoutRoot fetchFilters)
  useSWR(
    workspaceSlug ? `WORKSPACE_WORK_ITEMS_FILTERS_${workspaceSlug}_${viewId}` : null,
    async () => {
      if (workspaceSlug) {
        await fetchFilters(workspaceSlug, viewId);
      }
    },
    { revalidateIfStale: false, revalidateOnFocus: false }
  );

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

  // Refresh the workspace list once the create modal closes, so a work item created
  // via the header CTA shows up immediately (project-level modal writes to PROJECT store).
  const wasCreateModalOpen = useRef(false);
  useEffect(() => {
    if (wasCreateModalOpen.current && !isCreateIssueModalOpen && workspaceSlug) {
      void fetchIssuesWithExistingPagination(workspaceSlug, viewId, "mutation");
    }
    wasCreateModalOpen.current = isCreateIssueModalOpen;
  }, [isCreateIssueModalOpen, workspaceSlug, viewId, fetchIssuesWithExistingPagination]);

  const fetchNextPages = useCallback(
    (groupId?: string, subGroupId?: string) => {
      if (workspaceSlug) fetchNextIssues(workspaceSlug, viewId, groupId, subGroupId);
    },
    [fetchNextIssues, workspaceSlug, viewId]
  );

  if (!workspaceSlug || !workItemFilters) return null;

  // routeFilters kept for API parity with WorkspaceActiveLayout (AllIssueLayoutRoot pattern)
  const routeFilters: { [key: string]: string } = {};
  searchParams.forEach((value: string, key: string) => {
    routeFilters[key] = value;
  });

  return (
    <IssuesStoreContext.Provider value={EIssuesStoreType.GLOBAL}>
      <WorkspaceLevelWorkItemFiltersHOC
        enableSaveView
        saveViewOptions={{ label: "Save as" }}
        enableUpdateView
        entityId={viewId}
        entityType={EIssuesStoreType.GLOBAL}
        filtersToShowByLayout={ISSUE_DISPLAY_FILTERS_BY_PAGE.my_issues.filters}
        initialWorkItemFilters={initialWorkItemFilters as any}
        updateFilters={updateFilterExpression.bind(updateFilterExpression, workspaceSlug, viewId)}
        workspaceSlug={workspaceSlug}
      >
        {({ filter: workspaceWorkItemsFilter }) => (
          <div className="relative flex h-full w-full flex-col overflow-hidden">
            {workspaceWorkItemsFilter && <WorkItemFiltersRow filter={workspaceWorkItemsFilter} />}
            <div className="relative h-full w-full overflow-auto bg-surface-1">
              <WorkspaceActiveLayout
                activeLayout={activeLayout}
                isDefaultView
                isLoading={false}
                toggleLoading={() => {}}
                workspaceSlug={workspaceSlug}
                globalViewId={viewId}
                routeFilters={routeFilters}
                fetchNextPages={fetchNextPages}
                globalViewsLoading={false}
                issuesLoading={false}
                fetchIssuesOnMount
              />
            </div>
            <IssuePeekOverview />
          </div>
        )}
      </WorkspaceLevelWorkItemFiltersHOC>
    </IssuesStoreContext.Provider>
  );
});
