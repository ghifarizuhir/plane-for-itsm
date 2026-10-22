/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo } from "react";
import { observer } from "mobx-react";
import { useParams, useSearchParams } from "next/navigation";
// components
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import { EUserProjectRoles } from "@plane/types";
import { ContentWrapper, ERowVariant, Row } from "@plane/ui";
// components
import { ListLayout } from "@/components/core/list";
import { ModuleCardItem, ModuleListItem, ModulePeekOverview, ModulesListGanttChartView } from "@/components/modules";
import { CycleModuleBoardLayoutLoader } from "@/components/ui/loader/cycle-module-board-loader";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
import { GanttLayoutLoader } from "@/components/ui/loader/layouts/gantt-layout-loader";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useModule } from "@/hooks/store/use-module";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useProject } from "@/hooks/store/use-project";
import { useUserPermissions } from "@/hooks/store/user";
// helpers
import { selectWorkspaceModules } from "@/lib/workspace-modules";

type Props = {
  projectId?: string;
  projectIds?: string[];
  filterKey?: string;
  isGanttDisabled?: boolean;
  canCreate?: boolean;
};

export const ModulesListView = observer(function ModulesListView({
  projectId,
  projectIds,
  filterKey,
  isGanttDisabled = false,
  canCreate,
}: Props) {
  // router
  const { workspaceSlug, projectId: routeProjectId } = useParams();
  const searchParams = useSearchParams();
  const peekModule = searchParams.get("peekModule");
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { toggleCreateModuleModal } = useCommandPalette();
  const { getProjectModuleIds, getFilteredModuleIds, loader, moduleMap } = useModule();
  const { getDisplayFiltersByProjectId, getFiltersByProjectId, searchQuery } = useModuleFilter();
  const { getProjectById } = useProject();
  const { allowPermissions } = useUserPermissions();

  // derived values
  const resolvedProjectId = projectId ?? routeProjectId?.toString();
  const resolvedFilterKey = filterKey ?? resolvedProjectId;
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;
  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
  const layout =
    projectIds && !resolvedProjectId && displayFilters?.layout === "gantt"
      ? "list"
      : (displayFilters?.layout ?? "list");

  const projectAllowed = useMemo(() => {
    if (!projectIds) return undefined;
    return (moduleProjectId: string) => {
      if (!projectIds.includes(moduleProjectId)) return false;
      const project = getProjectById(moduleProjectId);
      return project ? project.module_view !== false : true;
    };
  }, [getProjectById, projectIds]);

  const workspaceSelection = useMemo(() => {
    if (!projectIds) return null;
    return selectWorkspaceModules({
      modules: Object.values(moduleMap ?? {}),
      projectIds,
      displayFilters,
      filters,
      searchQuery,
      projectAllowed,
    });
  }, [displayFilters, filters, moduleMap, projectAllowed, projectIds, searchQuery]);

  const projectModuleIds = resolvedProjectId ? getProjectModuleIds(resolvedProjectId) : null;
  const filteredModuleIds =
    workspaceSelection?.moduleIds ?? (resolvedProjectId ? getFilteredModuleIds(resolvedProjectId) : null);
  const totalModuleCount = workspaceSelection
    ? Object.values(moduleMap ?? {}).filter((module) => {
        if (module.archived_at || !projectIds?.includes(module.project_id)) return false;
        return projectAllowed ? projectAllowed(module.project_id) : true;
      }).length
    : (projectModuleIds?.length ?? 0);

  const canCreateWithPermissions = allowPermissions(
    [EUserProjectRoles.ADMIN, EUserProjectRoles.MEMBER],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug?.toString(),
    resolvedProjectId
  );
  const canPerformCreate = canCreate ?? canCreateWithPermissions;
  const showProjectBadge = Boolean(projectIds && !resolvedProjectId);

  if (loader || (!workspaceSelection && (!projectModuleIds || !filteredModuleIds)))
    return (
      <>
        {layout === "list" && <CycleModuleListLayoutLoader />}
        {layout === "board" && <CycleModuleBoardLayoutLoader />}
        {layout === "gantt" && <GanttLayoutLoader />}
      </>
    );

  if (totalModuleCount === 0)
    return (
      <EmptyStateDetailed
        assetKey="module"
        title={showProjectBadge ? t("workspace_empty_state.tracks.title") : t("project_empty_state.modules.title")}
        description={
          showProjectBadge
            ? t("workspace_empty_state.tracks.description")
            : t("project_empty_state.modules.description")
        }
        actions={
          showProjectBadge
            ? undefined
            : [
                {
                  label: t("project_empty_state.modules.cta_primary"),
                  onClick: () => toggleCreateModuleModal(true),
                  disabled: !canPerformCreate,
                  variant: "primary",
                },
              ]
        }
      />
    );

  if (!filteredModuleIds) return <CycleModuleListLayoutLoader />;

  if (filteredModuleIds.length === 0)
    return (
      <EmptyStateDetailed
        assetKey="search"
        title={t("common_empty_state.search.title")}
        description={t("common_empty_state.search.description")}
      />
    );

  return (
    <ContentWrapper variant={ERowVariant.HUGGING}>
      <div className="flex size-full justify-between">
        {layout === "list" && (
          <ListLayout>
            {filteredModuleIds.map((moduleId) => (
              <ModuleListItem key={moduleId} moduleId={moduleId} showProjectBadge={showProjectBadge} />
            ))}
          </ListLayout>
        )}
        {layout === "board" && (
          <Row
            className={`grid size-full grid-cols-1 gap-6 overflow-y-auto py-page-y ${
              peekModule
                ? "3xl:grid-cols-3 lg:grid-cols-1 xl:grid-cols-2"
                : "3xl:grid-cols-4 lg:grid-cols-2 xl:grid-cols-3"
            } vertical-scrollbar scrollbar-lg auto-rows-max transition-all`}
          >
            {filteredModuleIds.map((moduleId) => (
              <ModuleCardItem key={moduleId} moduleId={moduleId} showProjectBadge={showProjectBadge} />
            ))}
          </Row>
        )}
        {layout === "gantt" && resolvedProjectId && !isGanttDisabled && (
          <div className="size-full overflow-hidden">
            <ModulesListGanttChartView
              projectId={resolvedProjectId}
              moduleIds={filteredModuleIds}
              filterKey={resolvedFilterKey}
            />
          </div>
        )}
        <div className="flex-shrink-0">
          <ModulePeekOverview projectId={resolvedProjectId} workspaceSlug={workspaceSlug?.toString() ?? ""} />
        </div>
      </div>
    </ContentWrapper>
  );
});
