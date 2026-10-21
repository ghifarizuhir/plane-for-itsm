/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef } from "react";
import { observer } from "mobx-react";
// plane imports
import { EIssueFilterType, EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { EIssuesStoreType, EIssueLayoutTypes } from "@plane/types";
// hooks
import { useIssues } from "@/hooks/store/use-issues";
import { useUserPermissions } from "@/hooks/store/user";
import { useGroupIssuesDragNDrop } from "@/hooks/use-group-dragndrop";
import { useIssuesActions } from "@/hooks/use-issues-actions";
// local imports
import { AllIssueQuickActions } from "../../quick-action-dropdowns";
import { IssueLayoutHOC } from "../../issue-layout-HOC";
import type { TRenderQuickActions } from "../../list/list-view-types";
import { KanBan } from "../default";
import { KanBanSwimLanes } from "../swimlanes";

type Props = {
  workspaceSlug: string;
  viewId: string;
  fetchNextPages: (groupId?: string, subGroupId?: string) => void;
};

export const WorkspaceKanbanRoot = observer(function WorkspaceKanbanRoot(props: Props) {
  const { workspaceSlug, viewId, fetchNextPages } = props;
  const {
    issuesFilter: { filters, updateFilters },
    issues: { groupedIssueIds, fetchIssues, getGroupIssueCount },
    issueMap,
  } = useIssues(EIssuesStoreType.GLOBAL);
  const { updateIssue, removeIssue, archiveIssue } = useIssuesActions(EIssuesStoreType.GLOBAL);
  const { allowPermissions } = useUserPermissions();

  const issueFilters = viewId ? filters?.[viewId] : undefined;
  const displayFilters = issueFilters?.displayFilters;
  const displayProperties = issueFilters?.displayProperties;
  const sub_group_by = displayFilters?.sub_group_by;
  const group_by = displayFilters?.group_by;
  const orderBy = displayFilters?.order_by;
  const collapsedGroups = issueFilters?.kanbanFilters || { group_by: [], sub_group_by: [] };

  const scrollableContainerRef = useRef<HTMLDivElement | null>(null);

  // 1:1 with project BaseKanBanRoot
  const hasFilters = Boolean(issueFilters);
  useEffect(() => {
    if (!workspaceSlug || !hasFilters) return;
    fetchIssues(workspaceSlug, viewId, "init-loader", {
      canGroup: true,
      perPageCount: sub_group_by ? 10 : 30,
    });
  }, [workspaceSlug, viewId, hasFilters, group_by, sub_group_by, fetchIssues]);

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

  const handleOnDrop = useGroupIssuesDragNDrop(EIssuesStoreType.GLOBAL, orderBy, group_by, sub_group_by);

  const renderQuickActions: TRenderQuickActions = useCallback(
    ({ issue, parentRef, customActionButton }) => (
      <AllIssueQuickActions
        parentRef={parentRef}
        customActionButton={customActionButton}
        issue={issue}
        handleDelete={async () => removeIssue(issue.project_id, issue.id)}
        handleUpdate={async (data) => updateIssue && updateIssue(issue.project_id, issue.id, data)}
        handleArchive={async () => archiveIssue && archiveIssue(issue.project_id, issue.id)}
        readOnly={!canEditProperties(issue.project_id ?? undefined)}
      />
    ),
    [canEditProperties, removeIssue, updateIssue, archiveIssue]
  );

  const handleCollapsedGroups = useCallback(
    (toggle: "group_by" | "sub_group_by", value: string) => {
      const current = issueFilters?.kanbanFilters?.[toggle] || [];
      const next = current.includes(value) ? current.filter((v) => v !== value) : [...current, value];
      updateFilters(workspaceSlug, undefined, EIssueFilterType.KANBAN_FILTERS, { [toggle]: next } as any, viewId);
    },
    [issueFilters, updateFilters, workspaceSlug, viewId]
  );

  const KanBanView = sub_group_by ? KanBanSwimLanes : KanBan;

  return (
    <IssueLayoutHOC layout={EIssueLayoutTypes.KANBAN}>
      <div className="horizontal-scrollbar relative scrollbar-lg flex h-full w-full bg-surface-2">
        <div className="relative h-full w-max min-w-full bg-surface-2">
          <div className="h-full w-max">
            <KanBanView
              issuesMap={issueMap}
              groupedIssueIds={(groupedIssueIds ?? {}) as any}
              getGroupIssueCount={getGroupIssueCount}
              displayProperties={displayProperties}
              sub_group_by={sub_group_by as any}
              group_by={group_by as any}
              orderBy={orderBy}
              updateIssue={updateIssue}
              quickActions={renderQuickActions}
              handleCollapsedGroups={handleCollapsedGroups}
              collapsedGroups={collapsedGroups as any}
              enableQuickIssueCreate={false}
              showEmptyGroup={displayFilters?.show_empty_groups ?? true}
              quickAddCallback={undefined}
              disableIssueCreation
              canEditProperties={canEditProperties}
              scrollableContainerRef={scrollableContainerRef}
              handleOnDrop={handleOnDrop}
              loadMoreIssues={fetchNextPages}
            />
          </div>
        </div>
      </div>
    </IssueLayoutHOC>
  );
});
