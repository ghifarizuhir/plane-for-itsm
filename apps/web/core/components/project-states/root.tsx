/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMemo } from "react";
import { observer } from "mobx-react";
import useSWR from "swr";
// components
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { IState, TStateOperationsCallbacks } from "@plane/types";
import { EUserProjectRoles } from "@plane/types";
import { ProjectStateLoader, GroupList } from "@/components/project-states";
// hooks
import { useProjectState } from "@/hooks/store/use-project-state";
import { useUserPermissions } from "@/hooks/store/user";

type TProjectState = {
  workspaceSlug: string;
  projectId: string;
};

export const ProjectStateRoot = observer(function ProjectStateRoot(props: TProjectState) {
  const { workspaceSlug, projectId } = props;
  // hooks
  const {
    groupedProjectStates,
    getProjectStates,
    fetchProjectStates,
    createState,
    moveStatePosition,
    updateState,
    deleteState,
    markStateAsDefault,
  } = useProjectState();
  const { allowPermissions } = useUserPermissions();
  const { t } = useTranslation();
  // derived values
  const isEditable = allowPermissions(
    [EUserProjectRoles.ADMIN],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    projectId
  );
  const projectStates = getProjectStates(projectId);
  const typedStates = useMemo(() => projectStates?.filter((state) => Boolean(state.type_id)) ?? [], [projectStates]);
  const legacyGroupedStates = useMemo(() => {
    if (!groupedProjectStates) return;
    const grouped: Record<string, IState[]> = {};
    Object.entries(groupedProjectStates).forEach(([group, states]) => {
      grouped[group] = states.filter((state) => !state.type_id);
    });
    return grouped;
  }, [groupedProjectStates]);

  // Fetching all project states
  useSWR(
    workspaceSlug && projectId ? `PROJECT_STATES_${workspaceSlug}_${projectId}` : null,
    workspaceSlug && projectId ? () => fetchProjectStates(workspaceSlug.toString(), projectId.toString()) : null,
    { revalidateIfStale: false, revalidateOnFocus: false }
  );

  // State operations callbacks
  const stateOperationsCallbacks: TStateOperationsCallbacks = useMemo(
    () => ({
      createState: async (data: Partial<IState>) => createState(workspaceSlug, projectId, data),
      updateState: async (stateId: string, data: Partial<IState>) =>
        updateState(workspaceSlug, projectId, stateId, data),
      deleteState: async (stateId: string) => deleteState(workspaceSlug, projectId, stateId),
      moveStatePosition: async (stateId: string, data: Partial<IState>) =>
        moveStatePosition(workspaceSlug, projectId, stateId, data),
      markStateAsDefault: async (stateId: string) => markStateAsDefault(workspaceSlug, projectId, stateId),
    }),
    [workspaceSlug, projectId, createState, moveStatePosition, updateState, deleteState, markStateAsDefault]
  );

  // Loader
  if (!legacyGroupedStates) return <ProjectStateLoader />;

  return (
    <>
      <GroupList
        groupedStates={legacyGroupedStates}
        stateOperationsCallbacks={stateOperationsCallbacks}
        isEditable={isEditable}
      />
      {typedStates.length > 0 && (
        <div className="mt-8 flex flex-col gap-2">
          <h4 className="text-14 font-medium">{t("workspace_settings.settings.work_item_types.title")}</h4>
          <p className="text-caption-md-regular text-tertiary">
            {t("workspace_settings.settings.work_item_types.description")}
          </p>
          <div className="flex flex-wrap gap-2">
            {typedStates.map((state) => (
              <span key={state.id} className="rounded border border-subtle px-2 py-1 text-caption-md-medium">
                {state.name}
              </span>
            ))}
          </div>
        </div>
      )}
    </>
  );
});
