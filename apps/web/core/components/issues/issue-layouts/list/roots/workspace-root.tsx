/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef } from "react";
import { observer } from "mobx-react";
// plane imports
import { EIssueFilterType, EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { EIssuesStoreType, EIssueLayoutTypes, type GroupByColumnTypes } from "@plane/types";
// hooks
import { useIssues } from "@/hooks/store/use-issues";
import { useUserPermissions } from "@/hooks/store/user";
import { useGroupIssuesDragNDrop } from "@/hooks/use-group-dragndrop";
import { useIssuesActions } from "@/hooks/use-issues-actions";
// local imports
import { AllIssueQuickActions } from "../../quick-action-dropdowns";
import { IssueLayoutHOC } from "../../issue-layout-HOC";
import { resolveRenderedGroupBy } from "../../mobile-layout";
import type { TRenderQuickActions } from "../list-view-types";
import { List } from "../default";

type Props = {
  workspaceSlug: string;
  viewId: string;
  fetchNextPages: (groupId?: string, subGroupId?: string) => void;
};

export const WorkspaceListRoot = observer(function WorkspaceListRoot(props: Props) {
  const { workspaceSlug, viewId, fetchNextPages } = props;
  // store hooks — same GLOBAL store as workspace spreadsheet, 1:1 with project list
  const {
    issuesFilter: { filters, updateFilters },
    issues: { groupedIssueIds, fetchIssues },
    issueMap,
  } = useIssues(EIssuesStoreType.GLOBAL);
  const { updateIssue, removeIssue, archiveIssue } = useIssuesActions(EIssuesStoreType.GLOBAL);
  const { allowPermissions } = useUserPermissions();

  // derived values
  const issueFilters = viewId ? filters?.[viewId] : undefined;
  const displayFilters = issueFilters?.displayFilters;
  const displayProperties = issueFilters?.displayProperties;
  const orderBy = displayFilters?.order_by || undefined;
  const group_by = (displayFilters?.group_by || null) as GroupByColumnTypes | null;
  const showEmptyGroup = displayFilters?.show_empty_groups ?? false;
  const collapsedGroups = issueFilters?.kanbanFilters || { group_by: [], sub_group_by: [] };

  const renderedGroupBy = resolveRenderedGroupBy(groupedIssueIds as any, group_by);

  // 1:1 with project BaseListRoot: fetch once per view/group, after filters are ready
  const hasFilters = Boolean(issueFilters);
  const lastFetchedRef = useRef<{
    workspaceSlug?: string;
    viewId?: string;
    groupBy?: GroupByColumnTypes | null;
  } | null>(null);
  useEffect(() => {
    if (!workspaceSlug || !hasFilters) return;
    const last = lastFetchedRef.current;
    if (last !== null && last.workspaceSlug === workspaceSlug && last.viewId === viewId && last.groupBy === group_by)
      return;
    fetchIssues(workspaceSlug, viewId, "init-loader", {
      canGroup: true,
      perPageCount: group_by ? 50 : 100,
    });
    lastFetchedRef.current = { workspaceSlug, viewId, groupBy: group_by };
  }, [workspaceSlug, viewId, hasFilters, group_by, fetchIssues]);

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

  const handleOnDrop = useGroupIssuesDragNDrop(EIssuesStoreType.GLOBAL, orderBy, group_by);

  const renderQuickActions: TRenderQuickActions = useCallback(
    ({ issue, parentRef, customActionButton, placement, portalElement }) => (
      <AllIssueQuickActions
        parentRef={parentRef}
        customActionButton={customActionButton}
        issue={issue}
        handleDelete={async () => removeIssue(issue.project_id, issue.id)}
        handleUpdate={async (data) => updateIssue && updateIssue(issue.project_id, issue.id, data)}
        handleArchive={async () => archiveIssue && archiveIssue(issue.project_id, issue.id)}
        portalElement={portalElement}
        readOnly={!canEditProperties(issue.project_id ?? undefined)}
        placements={placement}
      />
    ),
    [canEditProperties, removeIssue, updateIssue, archiveIssue]
  );

  return (
    <IssueLayoutHOC layout={EIssueLayoutTypes.LIST}>
      <div className="relative size-full bg-surface-2">
        <List
          issuesMap={issueMap}
          displayProperties={displayProperties}
          group_by={renderedGroupBy}
          orderBy={orderBy}
          updateIssue={updateIssue}
          quickActions={renderQuickActions}
          groupedIssueIds={(groupedIssueIds ?? {}) as any}
          loadMoreIssues={fetchNextPages}
          showEmptyGroup={showEmptyGroup}
          quickAddCallback={undefined}
          enableIssueQuickAdd={false}
          canEditProperties={canEditProperties}
          disableIssueCreation
          isCompletedCycle={false}
          handleOnDrop={handleOnDrop}
          handleCollapsedGroups={(value: string) => {
            const next = collapsedGroups?.group_by?.includes(value)
              ? collapsedGroups.group_by.filter((v) => v !== value)
              : [...(collapsedGroups?.group_by || []), value];
            updateFilters(
              workspaceSlug,
              undefined,
              EIssueFilterType.KANBAN_FILTERS,
              {
                group_by: next,
              } as any,
              viewId
            );
          }}
          collapsedGroups={collapsedGroups as any}
        />
      </div>
    </IssueLayoutHOC>
  );
});
