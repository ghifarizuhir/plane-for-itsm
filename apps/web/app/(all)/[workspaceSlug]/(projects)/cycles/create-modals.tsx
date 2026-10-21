/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// components
import { CycleCreateUpdateModal } from "@/components/cycles/modal";
import { WorkspaceApplicationPickerModal } from "@/components/workspace/application-scope/project-picker-modal";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useProject } from "@/hooks/store/use-project";
import { useUser } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";
// helpers
import { resolveWorkspaceCreateProject } from "@/lib/workspace-create-project";

export const WorkspaceCyclesCreateModals = observer(function WorkspaceCyclesCreateModals() {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // store hooks
  const { isCreateCycleModalOpen, toggleCreateCycleModal } = useCommandPalette();
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { workspaceProjectIds, getProjectById } = useProject();
  const { projectsWithCreatePermissions } = useUser();
  // states
  const [pickedProjectId, setPickedProjectId] = useState<string | null>(null);

  const eligibleProjectIds = useMemo(
    () =>
      (workspaceProjectIds ?? []).filter((projectId) => {
        const project = getProjectById(projectId);
        return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.cycle_view !== false;
      }),
    [getProjectById, projectsWithCreatePermissions, workspaceProjectIds]
  );

  const createProjectId = resolveWorkspaceCreateProject(scopedProjectId, pickedProjectId);
  const showPicker = isCreateCycleModalOpen && !createProjectId;

  const handleClose = () => {
    setPickedProjectId(null);
    toggleCreateCycleModal(false);
  };

  useEffect(() => {
    if (!isCreateCycleModalOpen) setPickedProjectId(null);
  }, [isCreateCycleModalOpen]);

  useEffect(
    () => () => {
      toggleCreateCycleModal(false);
    },
    [toggleCreateCycleModal]
  );

  if (!workspaceSlug) return null;

  return (
    <>
      <WorkspaceApplicationPickerModal
        isOpen={showPicker}
        handleClose={handleClose}
        projectIds={eligibleProjectIds}
        onSelect={setPickedProjectId}
      />
      <CycleCreateUpdateModal
        isOpen={isCreateCycleModalOpen && Boolean(createProjectId)}
        handleClose={handleClose}
        workspaceSlug={workspaceSlug}
        projectId={createProjectId ?? ""}
      />
    </>
  );
});
