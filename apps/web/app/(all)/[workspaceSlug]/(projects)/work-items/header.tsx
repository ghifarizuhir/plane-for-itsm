/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useMemo } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { ChevronDownOutline, PreferencesOutline, WorkItemsOutline } from "@makeplane/propel/icons";
// plane imports
import {
  EIssueFilterType,
  EUserPermissions,
  EUserPermissionsLevel,
  ISSUE_DISPLAY_FILTERS_BY_PAGE,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IIssueDisplayFilterOptions, IIssueDisplayProperties, TIssueGroupByOptions } from "@plane/types";
import { EIssuesStoreType, EIssueLayoutTypes } from "@plane/types";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import {
  DisplayFiltersSelection,
  FiltersDropdown,
  LayoutSelection,
  MobileLayoutSelection,
} from "@/components/issues/issue-layouts/filters";
import { WorkItemFiltersToggle } from "@/components/work-item-filters/filters-toggle";
import { WorkspaceApplicationScopeChip } from "@/components/workspace/application-scope";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useIssues } from "@/hooks/store/use-issues";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useAppRouter } from "@/hooks/use-app-router";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
// helpers
import { WORKSPACE_WORK_ITEMS_VIEW_ID } from "@/lib/workspace-work-items";

const LAYOUTS = [
  EIssueLayoutTypes.LIST,
  EIssueLayoutTypes.KANBAN,
  EIssueLayoutTypes.CALENDAR,
  EIssueLayoutTypes.SPREADSHEET,
  EIssueLayoutTypes.GANTT,
];

export const WorkspaceWorkItemsHeader = observer(function WorkspaceWorkItemsHeader() {
  // router
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const viewId = WORKSPACE_WORK_ITEMS_VIEW_ID;
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { toggleCreateIssueModal } = useCommandPalette();
  const { allowPermissions } = useUserPermissions();
  const { projectsWithCreatePermissions } = useUser();
  const {
    issuesFilter: { filters, updateFilters },
  } = useIssues(EIssuesStoreType.GLOBAL);
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();

  // derived values
  const issueFilters = filters?.[viewId];
  const activeLayout = issueFilters?.displayFilters?.layout ?? EIssueLayoutTypes.SPREADSHEET;
  const layoutDisplayFiltersOptions = ISSUE_DISPLAY_FILTERS_BY_PAGE.my_issues.layoutOptions[activeLayout];
  const groupByOptions = useMemo<TIssueGroupByOptions[]>(
    () => layoutDisplayFiltersOptions?.display_filters.group_by ?? [],
    [layoutDisplayFiltersOptions]
  );
  const subGroupByOptions = useMemo<TIssueGroupByOptions[]>(
    () => layoutDisplayFiltersOptions?.display_filters.sub_group_by ?? [],
    [layoutDisplayFiltersOptions]
  );
  const canCreateInScope = !scopedProjectId || Boolean(projectsWithCreatePermissions?.[scopedProjectId]);
  const canUserCreateIssue =
    canCreateInScope &&
    (scopedProjectId
      ? allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          workspaceSlug,
          scopedProjectId
        )
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.WORKSPACE,
          workspaceSlug
        ));

  // handlers
  const handleLayoutChange = useCallback(
    (layout: EIssueLayoutTypes) => {
      if (!workspaceSlug) return;
      updateFilters(workspaceSlug, undefined, EIssueFilterType.DISPLAY_FILTERS, { layout }, viewId);
    },
    [workspaceSlug, updateFilters, viewId]
  );

  const handleDisplayFilters = useCallback(
    (updatedDisplayFilter: Partial<IIssueDisplayFilterOptions>) => {
      if (!workspaceSlug) return;
      updateFilters(workspaceSlug, undefined, EIssueFilterType.DISPLAY_FILTERS, updatedDisplayFilter, viewId);
    },
    [workspaceSlug, updateFilters, viewId]
  );

  const handleDisplayProperties = useCallback(
    (property: Partial<IIssueDisplayProperties>) => {
      if (!workspaceSlug) return;
      updateFilters(workspaceSlug, undefined, EIssueFilterType.DISPLAY_PROPERTIES, property, viewId);
    },
    [workspaceSlug, updateFilters, viewId]
  );

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex min-w-0 items-center gap-2">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t("sidebar.work_items")}
                  href={`/${workspaceSlug}/work-items/`}
                  icon={<WorkItemsOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          <WorkspaceApplicationScopeChip />
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        <div className="hidden gap-2 md:flex">
          <div className="hidden @4xl:flex">
            <LayoutSelection layouts={LAYOUTS} onChange={handleLayoutChange} selectedLayout={activeLayout} />
          </div>
          <div className="flex @4xl:hidden">
            <MobileLayoutSelection layouts={LAYOUTS} onChange={handleLayoutChange} activeLayout={activeLayout} />
          </div>
          <WorkItemFiltersToggle entityType={EIssuesStoreType.GLOBAL} entityId={viewId} />
          <FiltersDropdown
            miniIcon={<PreferencesOutline className="size-3.5" />}
            title={t("common.display")}
            placement="bottom-end"
          >
            <DisplayFiltersSelection
              layoutDisplayFiltersOptions={layoutDisplayFiltersOptions}
              displayFilters={issueFilters?.displayFilters ?? {}}
              groupByOptions={groupByOptions}
              subGroupByOptions={subGroupByOptions}
              handleDisplayFiltersUpdate={handleDisplayFilters}
              displayProperties={issueFilters?.displayProperties ?? {}}
              handleDisplayPropertiesUpdate={handleDisplayProperties}
            />
          </FiltersDropdown>
        </div>
        {canUserCreateIssue && (
          <Button
            variant="primary"
            size="lg"
            onClick={() =>
              toggleCreateIssueModal(true, EIssuesStoreType.PROJECT, scopedProjectId ? [scopedProjectId] : undefined)
            }
          >
            <div className="block sm:hidden">{t("issue.label", { count: 1 })}</div>
            <div className="hidden sm:block">{t("issue.add.label")}</div>
          </Button>
        )}
      </Header.RightItem>
    </Header>
  );
});

export const WorkspaceWorkItemsMobileHeader = observer(function WorkspaceWorkItemsMobileHeader() {
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const viewId = WORKSPACE_WORK_ITEMS_VIEW_ID;
  // i18n
  const { t } = useTranslation();
  // store hooks
  const {
    issuesFilter: { filters, updateFilters },
  } = useIssues(EIssuesStoreType.GLOBAL);

  const issueFilters = filters?.[viewId];
  const activeLayout = issueFilters?.displayFilters?.layout ?? EIssueLayoutTypes.SPREADSHEET;
  const layoutDisplayFiltersOptions = ISSUE_DISPLAY_FILTERS_BY_PAGE.my_issues.layoutOptions[activeLayout];

  const handleLayoutChange = useCallback(
    (layout: EIssueLayoutTypes) => {
      if (!workspaceSlug) return;
      updateFilters(workspaceSlug, undefined, EIssueFilterType.DISPLAY_FILTERS, { layout }, viewId);
    },
    [workspaceSlug, updateFilters, viewId]
  );

  const handleDisplayFilters = useCallback(
    (updatedDisplayFilter: Partial<IIssueDisplayFilterOptions>) => {
      if (!workspaceSlug) return;
      updateFilters(workspaceSlug, undefined, EIssueFilterType.DISPLAY_FILTERS, updatedDisplayFilter, viewId);
    },
    [workspaceSlug, updateFilters, viewId]
  );

  const handleDisplayProperties = useCallback(
    (property: Partial<IIssueDisplayProperties>) => {
      if (!workspaceSlug) return;
      updateFilters(workspaceSlug, undefined, EIssueFilterType.DISPLAY_PROPERTIES, property, viewId);
    },
    [workspaceSlug, updateFilters, viewId]
  );

  return (
    <div className="z-[13] flex justify-evenly border-b border-subtle bg-surface-1 py-2 md:hidden">
      <MobileLayoutSelection layouts={LAYOUTS} onChange={handleLayoutChange} activeLayout={activeLayout} />
      <div className="flex flex-grow items-center justify-center border-l border-subtle text-13 text-secondary">
        <FiltersDropdown
          title={t("common.display")}
          placement="bottom-end"
          menuButton={
            <span className="flex items-center text-13 text-secondary">
              {t("common.display")}
              <ChevronDownOutline className="ml-2 h-4 w-4 text-secondary" />
            </span>
          }
        >
          <DisplayFiltersSelection
            layoutDisplayFiltersOptions={layoutDisplayFiltersOptions}
            displayFilters={issueFilters?.displayFilters ?? {}}
            handleDisplayFiltersUpdate={handleDisplayFilters}
            displayProperties={issueFilters?.displayProperties ?? {}}
            handleDisplayPropertiesUpdate={handleDisplayProperties}
          />
        </FiltersDropdown>
      </div>
      <div className="flex flex-grow items-center justify-center border-l border-subtle text-13 text-secondary">
        <WorkItemFiltersToggle entityType={EIssuesStoreType.GLOBAL} entityId={viewId} />
      </div>
    </div>
  );
});
