/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { ALL_ISSUES, EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { EIssuesStoreType, EIssueLayoutTypes, GANTT_TIMELINE_TYPE, type TIssue } from "@plane/types";
// components
import { TimeLineTypeContext } from "@/components/gantt-chart/contexts";
import { GanttChartRoot } from "@/components/gantt-chart/root";
import { IssueGanttSidebar } from "@/components/gantt-chart/sidebar/issues/sidebar";
// hooks
import { useIssues } from "@/hooks/store/use-issues";
import { useUserPermissions } from "@/hooks/store/user";
import { useIssuesActions } from "@/hooks/use-issues-actions";
import { useTimeLineChart } from "@/hooks/use-timeline-chart";
import { useBulkOperationStatus } from "@/hooks/use-bulk-operation-status";
// local imports
import { IssueLayoutHOC } from "../../issue-layout-HOC";
import { IssueGanttBlock } from "../../gantt/blocks";

type Props = {
  workspaceSlug: string;
  viewId: string;
  fetchNextPages: () => void;
};

export const WorkspaceGanttRoot = observer(function WorkspaceGanttRoot(props: Props) {
  const { workspaceSlug, viewId, fetchNextPages } = props;
  const { t } = useTranslation();
  const {
    issues: { fetchIssues, ...issues },
    issuesFilter,
    issueMap,
  } = useIssues(EIssuesStoreType.GLOBAL);
  const { updateIssue } = useIssuesActions(EIssuesStoreType.GLOBAL);
  const { initGantt } = useTimeLineChart(GANTT_TIMELINE_TYPE.ISSUE);
  const { allowPermissions } = useUserPermissions();
  const isBulkOperationsEnabled = useBulkOperationStatus();
  const issueFilters = viewId ? issuesFilter?.filters?.[viewId] : undefined;

  useEffect(() => {
    initGantt();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 1:1 with project BaseGanttRoot
  const hasFilters = Boolean(issueFilters);
  useEffect(() => {
    if (!workspaceSlug || !hasFilters) return;
    fetchIssues(workspaceSlug, viewId, "init-loader", { canGroup: false, perPageCount: 100 });
  }, [workspaceSlug, viewId, hasFilters, fetchIssues]);

  const issuesIds = (issues.groupedIssueIds?.[ALL_ISSUES] as string[]) ?? [];
  const nextPageResults = issues.getPaginationData(undefined, undefined)?.nextPageResults;

  const updateIssueBlockStructure = async (issue: TIssue, data: any) => {
    if (!workspaceSlug) return;
    const payload: any = { ...data };
    if (data.sort_order) payload.sort_order = data.sort_order.newSortOrder;
    if (updateIssue) await updateIssue(issue.project_id, issue.id, payload);
  };

  const isAllowed = allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.PROJECT);

  const updateBlockDates = useCallback(
    (updates: { id: string; start_date?: string; target_date?: string }[]) => {
      // group by project since workspace gantt spans multiple projects — 1:1 with project behavior per issue
      const byProject = new Map<string, { id: string; start_date?: string; target_date?: string }[]>();
      updates.forEach((u) => {
        const pid = issueMap?.[u.id]?.project_id;
        if (!pid) return;
        const arr = byProject.get(pid) ?? [];
        arr.push(u);
        byProject.set(pid, arr);
      });
      const promises = Array.from(byProject.entries()).map(([pid, arr]) =>
        issues.updateIssueDates(workspaceSlug.toString(), arr, pid)
      );
      return Promise.all(promises)
        .then(() => undefined)
        .catch(() => {
          setToast({
            type: TOAST_TYPE.ERROR,
            title: t("toast.error"),
            message: "Error while updating work item dates, Please try again Later",
          });
        });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [issues, workspaceSlug, issueMap]
  );

  return (
    <IssueLayoutHOC layout={EIssueLayoutTypes.GANTT}>
      <TimeLineTypeContext.Provider value={GANTT_TIMELINE_TYPE.ISSUE}>
        <div className="h-full w-full">
          <GanttChartRoot
            border={false}
            title={t("issue.label", { count: 2 })}
            loaderTitle={t("issue.label", { count: 2 })}
            blockIds={issuesIds}
            blockUpdateHandler={updateIssueBlockStructure}
            blockToRender={(data: TIssue) => <IssueGanttBlock issueId={data.id} />}
            sidebarToRender={(sidebarProps) => <IssueGanttSidebar {...sidebarProps} showAllBlocks />}
            enableBlockLeftResize={isAllowed}
            enableBlockRightResize={isAllowed}
            enableBlockMove={isAllowed}
            enableReorder={issueFilters?.displayFilters?.order_by === "sort_order" && isAllowed}
            enableAddBlock={false}
            enableSelection={isBulkOperationsEnabled && isAllowed}
            loadMoreBlocks={fetchNextPages}
            canLoadMoreBlocks={nextPageResults}
            updateBlockDates={updateBlockDates}
            showAllBlocks
            enableDependency
          />
        </div>
      </TimeLineTypeContext.Provider>
    </IssueLayoutHOC>
  );
});
