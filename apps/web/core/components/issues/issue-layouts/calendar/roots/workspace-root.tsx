/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import type { TSupportedFilterTypeForUpdate } from "@plane/constants";
import { EIssueGroupByToServerOptions, EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TSupportedFilterForUpdate } from "@plane/types";
import { EIssuesStoreType, type TGroupedIssues } from "@plane/types";
// hooks
import { useCalendarView } from "@/hooks/store/use-calendar-view";
import { useIssues } from "@/hooks/store/use-issues";
import { useUserPermissions } from "@/hooks/store/user";
import { useIssuesActions } from "@/hooks/use-issues-actions";
// local imports
import { AllIssueQuickActions } from "../../quick-action-dropdowns";
import { CalendarChart } from "../calendar";
import { handleDragDrop } from "../utils";

type Props = {
  workspaceSlug: string;
  viewId: string;
  fetchNextPages: (groupId?: string) => void;
};

export const WorkspaceCalendarRoot = observer(function WorkspaceCalendarRoot(props: Props) {
  const { workspaceSlug, viewId, fetchNextPages } = props;
  const {
    issues: { fetchIssues, ...issues },
    issuesFilter,
    issueMap,
  } = useIssues(EIssuesStoreType.GLOBAL);
  const { updateIssue, removeIssue, archiveIssue } = useIssuesActions(EIssuesStoreType.GLOBAL);
  const { allowPermissions } = useUserPermissions();
  const issueCalendarView = useCalendarView();

  const issueFilters = viewId ? issuesFilter?.filters?.[viewId] : undefined;
  const displayFilters = issueFilters?.displayFilters;
  const groupedIssueIds = (issues.groupedIssueIds ?? {}) as TGroupedIssues;
  const layout = displayFilters?.calendar?.layout ?? "month";
  const { startDate, endDate } = issueCalendarView.getStartAndEndDate(layout) ?? {};

  // 1:1 with project BaseCalendarRoot: calendar needs date range + target_date grouping
  const hasFilters = Boolean(issueFilters);
  useEffect(() => {
    if (!workspaceSlug || !hasFilters || !startDate || !endDate || !layout) return;
    fetchIssues(workspaceSlug, viewId, "init-loader", {
      canGroup: true,
      perPageCount: layout === "month" ? 4 : 30,
      before: endDate,
      after: startDate,
      groupedBy: EIssueGroupByToServerOptions["target_date"],
    });
  }, [workspaceSlug, viewId, hasFilters, startDate, endDate, layout, fetchIssues]);

  const canEditProperties = useCallback(
    (projectId: string | undefined) => {
      if (!projectId) return false;
      return allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.PROJECT,
        workspaceSlug,
        projectId
      );
    },
    [allowPermissions, workspaceSlug]
  );

  // CalendarChart reads filters from `issuesFilterStore.issueFilters`; the workspace store's
  // getter is keyed by the route's globalViewId, so bind it to this page's fixed viewId.
  const calendarFilterStore = { issueFilters } as unknown as typeof issuesFilter;
  const handleUpdateFilters = useCallback(
    (projectId: string, filterType: TSupportedFilterTypeForUpdate, filters: TSupportedFilterForUpdate) => {
      if (!workspaceSlug) return Promise.resolve();
      return issuesFilter.updateFilters(workspaceSlug, projectId, filterType, filters, viewId);
    },
    [issuesFilter, workspaceSlug, viewId]
  );

  const handleDragAndDrop = async (
    issueId: string | undefined,
    issueProjectId: string | undefined,
    sourceDate: string | undefined,
    destinationDate: string | undefined
  ) => {
    if (!issueId || !destinationDate || !sourceDate || !issueProjectId) return;
    await handleDragDrop(issueId, sourceDate, destinationDate, workspaceSlug, issueProjectId, updateIssue).catch(
      (err) => {
        setToast({
          title: "Error!",
          type: TOAST_TYPE.ERROR,
          message: err?.detail ?? "Failed to perform this action",
        });
      }
    );
  };

  return (
    <div className="h-full w-full overflow-hidden bg-surface-1 pt-4">
      <CalendarChart
        issuesFilterStore={calendarFilterStore as any}
        issues={issueMap}
        groupedIssueIds={groupedIssueIds}
        layout={displayFilters?.calendar?.layout ?? "month"}
        showWeekends={displayFilters?.calendar?.show_weekends ?? false}
        issueCalendarView={issueCalendarView}
        quickActions={({ issue, parentRef, customActionButton, placement }) => (
          <AllIssueQuickActions
            parentRef={parentRef}
            customActionButton={customActionButton}
            issue={issue}
            handleDelete={async () => removeIssue(issue.project_id, issue.id)}
            handleUpdate={async (data) => updateIssue && updateIssue(issue.project_id, issue.id, data)}
            handleArchive={async () => archiveIssue && archiveIssue(issue.project_id, issue.id)}
            readOnly={!canEditProperties(issue.project_id ?? undefined)}
            placements={placement}
          />
        )}
        loadMoreIssues={fetchNextPages}
        getPaginationData={(groupId) => issues?.getPaginationData(groupId, undefined)}
        getGroupIssueCount={(groupId) => issues?.getGroupIssueCount(groupId, undefined, false)}
        readOnly={false}
        updateFilters={handleUpdateFilters}
        handleDragAndDrop={handleDragAndDrop}
        canEditProperties={canEditProperties}
      />
    </div>
  );
});
