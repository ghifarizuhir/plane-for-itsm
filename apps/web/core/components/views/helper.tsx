/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { EIssueLayoutTypes } from "@plane/types";
import { WorkspaceSpreadsheetRoot } from "@/components/issues/issue-layouts/spreadsheet/roots/workspace-root";
import { WorkspaceListRoot } from "@/components/issues/issue-layouts/list/roots/workspace-root";
import { WorkspaceKanbanRoot } from "@/components/issues/issue-layouts/kanban/roots/workspace-root";
import { WorkspaceCalendarRoot } from "@/components/issues/issue-layouts/calendar/roots/workspace-root";
import { WorkspaceGanttRoot } from "@/components/issues/issue-layouts/gantt/roots/workspace-root";

export type TWorkspaceLayoutProps = {
  activeLayout: EIssueLayoutTypes | undefined;
  isDefaultView: boolean;
  isLoading?: boolean;
  toggleLoading: (value: boolean) => void;
  workspaceSlug: string;
  globalViewId: string;
  routeFilters: {
    [key: string]: string;
  };
  fetchNextPages: (groupId?: string, subGroupId?: string) => void;
  globalViewsLoading: boolean;
  issuesLoading: boolean;
  fetchIssuesOnMount?: boolean;
};

export function WorkspaceActiveLayout(props: TWorkspaceLayoutProps) {
  const {
    activeLayout = EIssueLayoutTypes.SPREADSHEET,
    isDefaultView,
    isLoading,
    toggleLoading,
    workspaceSlug,
    globalViewId,
    routeFilters,
    fetchNextPages,
    globalViewsLoading,
    issuesLoading,
    fetchIssuesOnMount,
  } = props;
  // 1:1 with project ProjectIssueLayout — same layout switcher (list/kanban/calendar/gantt/spreadsheet)
  switch (activeLayout) {
    case EIssueLayoutTypes.LIST:
      return <WorkspaceListRoot workspaceSlug={workspaceSlug} viewId={globalViewId} fetchNextPages={fetchNextPages} />;
    case EIssueLayoutTypes.KANBAN:
      return <WorkspaceKanbanRoot workspaceSlug={workspaceSlug} viewId={globalViewId} fetchNextPages={fetchNextPages} />;
    case EIssueLayoutTypes.CALENDAR:
      return (
        <WorkspaceCalendarRoot workspaceSlug={workspaceSlug} viewId={globalViewId} fetchNextPages={fetchNextPages} />
      );
    case EIssueLayoutTypes.GANTT:
      return <WorkspaceGanttRoot workspaceSlug={workspaceSlug} viewId={globalViewId} fetchNextPages={fetchNextPages} />;
    case EIssueLayoutTypes.SPREADSHEET:
      return (
        <WorkspaceSpreadsheetRoot
          isDefaultView={isDefaultView}
          isLoading={isLoading}
          toggleLoading={toggleLoading}
          workspaceSlug={workspaceSlug}
          globalViewId={globalViewId}
          routeFilters={routeFilters}
          fetchNextPages={fetchNextPages}
          globalViewsLoading={globalViewsLoading}
          issuesLoading={issuesLoading}
          fetchOnMount={fetchIssuesOnMount}
        />
      );
    default:
      return <></>;
  }
}

export type TLayoutSelectionProps = {
  onChange: (layout: EIssueLayoutTypes) => void;
  selectedLayout: EIssueLayoutTypes;
  workspaceSlug: string;
};
