/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
// hooks
import { useProject } from "@/hooks/store/use-project";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const useWorkspaceServicePermissions = () => {
  // router
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  // scope
  const { projectId: scopedProjectId } = useWorkspaceApplicationScope();
  // store hooks
  const { getProjectById, workspaceProjectIds } = useProject();
  const { allowPermissions } = useUserPermissions();
  const { projectsWithCreatePermissions } = useUser();

  const canCreateInProject = (projectId: string) => {
    const project = getProjectById(projectId);
    return Boolean(projectsWithCreatePermissions?.[projectId]) && project?.service_view !== false;
  };

  const canCreateService = scopedProjectId
    ? canCreateInProject(scopedProjectId) &&
      allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.PROJECT,
        workspaceSlug,
        scopedProjectId
      )
    : Boolean(workspaceProjectIds?.some(canCreateInProject)) &&
      allowPermissions(
        [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        EUserPermissionsLevel.WORKSPACE,
        workspaceSlug
      );

  return { canCreateService };
};
