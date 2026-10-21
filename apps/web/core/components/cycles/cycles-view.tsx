/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo } from "react";
import { observer } from "mobx-react";
// components
import { useTranslation } from "@plane/i18n";
// assets
import AllFiltersImage from "@/app/assets/empty-state/cycle/all-filters.svg?url";
import NameFilterImage from "@/app/assets/empty-state/cycle/name-filter.svg?url";
// components
import { CyclesList } from "@/components/cycles/list";
import { CycleModuleListLayoutLoader } from "@/components/ui/loader/cycle-module-list-loader";
// hooks
import { useCycle } from "@/hooks/store/use-cycle";
import { useCycleFilter } from "@/hooks/store/use-cycle-filter";
import { useProject } from "@/hooks/store/use-project";
// helpers
import { selectWorkspaceCycles, WORKSPACE_CYCLE_FILTER_KEY } from "@/lib/workspace-cycles";

export interface ICyclesView {
  workspaceSlug: string;
  projectId?: string;
  projectIds?: string[];
  filterKey?: string;
}

export const CyclesView = observer(function CyclesView(props: ICyclesView) {
  const { workspaceSlug, projectId, projectIds, filterKey: filterKeyProp } = props;
  const filterKey = filterKeyProp ?? projectId ?? WORKSPACE_CYCLE_FILTER_KEY;
  // store hooks
  const { getFilteredCycleIds, getFilteredCompletedCycleIds, loader, currentProjectActiveCycleId, cycleMap } =
    useCycle();
  const { getFiltersByProjectId, searchQuery } = useCycleFilter();
  const { getProjectById } = useProject();
  const { t } = useTranslation();
  // derived values
  const { start_date, end_date, status } = getFiltersByProjectId(filterKey) ?? {};
  const projectAllowed = useMemo(() => {
    if (!projectIds) return undefined;
    return (cycleProjectId: string) => {
      const project = getProjectById(cycleProjectId);
      return project ? project.cycle_view !== false : true;
    };
  }, [getProjectById, projectIds]);

  const workspaceSelection = useMemo(() => {
    if (!projectIds) return null;
    return selectWorkspaceCycles({
      cycles: Object.values(cycleMap ?? {}),
      projectIds,
      filters: { start_date, end_date, status },
      searchQuery,
      projectAllowed,
    });
  }, [cycleMap, start_date, status, end_date, projectAllowed, projectIds, searchQuery]);

  const scopedActiveCycleId = useMemo(() => {
    if (!projectId) return null;
    const selection = selectWorkspaceCycles({
      cycles: Object.values(cycleMap ?? {}),
      projectIds: [projectId],
      filters: {},
      searchQuery: "",
      projectAllowed,
    });
    return selection.currentCycleIds[0] ?? null;
  }, [cycleMap, projectAllowed, projectId]);

  const filteredCycleIds = projectId ? getFilteredCycleIds(projectId, false) : null;
  const filteredCompletedCycleIds = projectId ? getFilteredCompletedCycleIds(projectId) : null;
  const filteredUpcomingCycleIds = (filteredCycleIds ?? []).filter(
    (cycleId) => cycleId !== currentProjectActiveCycleId
  );

  const workspaceCycleIds = workspaceSelection
    ? [
        ...workspaceSelection.currentCycleIds,
        ...workspaceSelection.upcomingCycleIds,
        ...workspaceSelection.draftCycleIds,
      ]
    : null;
  const cycleIds = workspaceCycleIds ?? filteredCycleIds;
  const upcomingCycleIds = workspaceSelection
    ? [...workspaceSelection.upcomingCycleIds, ...workspaceSelection.draftCycleIds]
    : filteredUpcomingCycleIds;
  const completedCycleIds = workspaceSelection?.completedCycleIds ?? filteredCompletedCycleIds ?? [];
  const currentCycleIds = workspaceSelection?.currentCycleIds;
  const activeCycleId = workspaceSelection && projectId ? scopedActiveCycleId : undefined;

  if (loader || (!workspaceSelection && !filteredCycleIds)) return <CycleModuleListLayoutLoader />;

  const hasNoResults = workspaceSelection
    ? workspaceSelection.totalCount === 0
    : (filteredCycleIds?.length ?? 0) === 0 && (filteredCompletedCycleIds?.length ?? 0) === 0;

  if (hasNoResults)
    return (
      <div className="grid h-full w-full place-items-center">
        <div className="text-center">
          <img
            src={searchQuery.trim() === "" ? AllFiltersImage : NameFilterImage}
            className="mx-auto h-36 w-36 object-contain sm:h-48 sm:w-48"
            alt="No matching cycles"
          />
          <h5 className="mt-7 mb-1 text-18 font-medium">{t("project_cycles.no_matching_cycles")}</h5>
          <p className="text-14 text-placeholder">
            {searchQuery.trim() === ""
              ? t("project_cycles.remove_filters_to_see_all_cycles")
              : t("project_cycles.remove_search_criteria_to_see_all_cycles")}
          </p>
        </div>
      </div>
    );

  return (
    <CyclesList
      completedCycleIds={completedCycleIds}
      upcomingCycleIds={upcomingCycleIds}
      currentCycleIds={workspaceSelection && !projectId ? currentCycleIds : undefined}
      cycleIds={cycleIds ?? []}
      activeCycleId={activeCycleId}
      showActiveCycle={!workspaceSelection || Boolean(projectId)}
      showProjectBadge={Boolean(workspaceSelection && !projectId)}
      projectId={projectId}
      workspaceSlug={workspaceSlug}
    />
  );
});
