/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// PLane
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { GANTT_TIMELINE_TYPE } from "@plane/types";
import type { IBlockUpdateData, IBlockUpdateDependencyData, IModule } from "@plane/types";
// components
import { GanttChartRoot, ModuleGanttSidebar } from "@/components/gantt-chart";
import { TimeLineTypeContext } from "@/components/gantt-chart/contexts";
import { ModuleGanttBlock } from "@/components/modules";
// hooks
import { useModule } from "@/hooks/store/use-module";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  projectId?: string;
  moduleIds?: string[] | null;
  filterKey?: string;
};

export const ModulesListGanttChartView = observer(function ModulesListGanttChartView({
  projectId,
  moduleIds,
  filterKey,
}: Props) {
  // router
  const { workspaceSlug, projectId: routeProjectId } = useParams();
  const resolvedProjectId = projectId ?? routeProjectId?.toString();
  const resolvedFilterKey = filterKey ?? resolvedProjectId;
  // store
  const { getFilteredModuleIds, getModuleById, updateModuleDetails } = useModule();
  const { getDisplayFiltersByProjectId } = useModuleFilter();
  const { allowPermissions } = useUserPermissions();
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;

  // derived values
  const filteredModuleIds = moduleIds ?? (resolvedProjectId ? getFilteredModuleIds(resolvedProjectId) : null);

  const isAllowed = resolvedProjectId
    ? allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.PROJECT,
        workspaceSlug?.toString(),
        resolvedProjectId
      )
    : false;

  const handleModuleUpdate = async (module: IModule, data: IBlockUpdateData) => {
    if (!workspaceSlug || !module) return;

    const payload: any = { ...data };
    if (data.sort_order) payload.sort_order = data.sort_order.newSortOrder;

    await updateModuleDetails(workspaceSlug.toString(), module.project_id, module.id, payload);
  };

  const updateBlockDates = async (blockUpdates: IBlockUpdateDependencyData[]) => {
    const blockUpdate = blockUpdates[0];

    if (!blockUpdate || !workspaceSlug) return;

    const module = getModuleById(blockUpdate.id);

    if (!module) return;

    const payload: Partial<IModule> = {};

    if (blockUpdate.start_date) payload.start_date = blockUpdate.start_date;
    if (blockUpdate.target_date) payload.target_date = blockUpdate.target_date;

    await updateModuleDetails(workspaceSlug.toString(), module.project_id, blockUpdate.id, payload);
  };

  if (!resolvedProjectId || !filteredModuleIds) return null;

  return (
    <TimeLineTypeContext.Provider value={GANTT_TIMELINE_TYPE.MODULE}>
      <GanttChartRoot
        title="Modules"
        loaderTitle="Modules"
        blockIds={filteredModuleIds}
        sidebarToRender={(props) => <ModuleGanttSidebar {...props} />}
        blockUpdateHandler={(block, payload) => handleModuleUpdate(block, payload)}
        blockToRender={(data: IModule) => <ModuleGanttBlock moduleId={data.id} />}
        enableBlockLeftResize={isAllowed}
        enableBlockRightResize={isAllowed}
        enableBlockMove={isAllowed}
        enableReorder={isAllowed && displayFilters?.order_by === "sort_order"}
        enableAddBlock={isAllowed}
        updateBlockDates={updateBlockDates}
        showAllBlocks
      />
    </TimeLineTypeContext.Provider>
  );
});
