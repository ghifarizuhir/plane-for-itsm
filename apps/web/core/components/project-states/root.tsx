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
import { useWorkflow } from "@/hooks/store/use-workflow";
import { useUserPermissions } from "@/hooks/store/user";
// store
import { isTypedState } from "@/store/workflow.helpers";

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
  const typedStates = useMemo(() => projectStates?.filter((state) => isTypedState(state)) ?? [], [projectStates]);
  const legacyGroupedStates = useMemo(() => {
    if (!groupedProjectStates) return;
    const grouped: Record<string, IState[]> = {};
    Object.entries(groupedProjectStates).forEach(([group, states]) => {
      grouped[group] = states.filter((state) => !isTypedState(state));
    });
    return grouped;
  }, [groupedProjectStates]);
  const { getWorkflowMap } = useWorkflow();
  const workflowMap = getWorkflowMap(projectId);
  const typedStateGroups = useMemo(() => {
    const groups = new Map<string, IState[]>();
    typedStates.forEach((state) => {
      const key = state.type_id ?? "untyped";
      groups.set(key, [...(groups.get(key) ?? []), state]);
    });
    return [...groups.entries()];
  }, [typedStates]);

  // Fetching all project states
  useSWR(
    workspaceSlug && projectId ? `PROJECT_STATES_${workspaceSlug}_${projectId}` : null,
    workspaceSlug && projectId ? () => fetchProjectStates(workspaceSlug.toString(), projectId.toString()) : null,
    { revalidateOnMount: true, revalidateOnFocus: false }
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
          <h4 className="text-14 font-medium">{t("project_settings.work_item_types.heading")}</h4>
          <p className="text-caption-md-regular text-tertiary">{t("project_settings.work_item_types.description")}</p>
          <div className="flex flex-col gap-3">
            {typedStateGroups.map(([typeId, states]) => (
              <div key={typeId} className="flex flex-col gap-1">
                <span className="text-caption-md-medium text-tertiary">
                  {workflowMap?.types.find((type) => type.type_id === typeId)?.type_name ?? ""}
                </span>
                <div className="flex flex-wrap gap-2">
                  {states.map((state) => (
                    <span key={state.id} className="rounded border border-subtle px-2 py-1 text-caption-md-medium">
                      {state.name}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
});
