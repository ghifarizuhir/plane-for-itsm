/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { EmptyStateDetailed } from "@plane/propel/empty-state";
import { EIssuesStoreType, EUserWorkspaceRoles } from "@plane/types";
// hooks
import { useCommandPalette } from "@/hooks/store/use-command-palette";
import { useProject } from "@/hooks/store/use-project";
import { useUser, useUserPermissions } from "@/hooks/store/user";
import { useWorkspaceApplicationScope } from "@/hooks/use-workspace-application-scope";

export const GlobalViewEmptyState = observer(function GlobalViewEmptyState() {
  // plane imports
  const { t } = useTranslation();
  // store hooks
  const { workspaceProjectIds } = useProject();
  const { toggleCreateIssueModal, toggleCreateProjectModal } = useCommandPalette();
  const { allowPermissions } = useUserPermissions();
  const { workspaceSlug, projectId: scopedProjectId } = useWorkspaceApplicationScope();
  const { projectsWithCreatePermissions } = useUser();
  // derived values
  const hasWorkspaceMemberPermission = allowPermissions(
    [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
    EUserPermissionsLevel.WORKSPACE,
    workspaceSlug
  );
  const canCreateInScope = !scopedProjectId || Boolean(projectsWithCreatePermissions?.[scopedProjectId]);
  const canCreateIssue =
    canCreateInScope &&
    (scopedProjectId
      ? allowPermissions(
          [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER],
          EUserPermissionsLevel.PROJECT,
          workspaceSlug,
          scopedProjectId
        )
      : hasWorkspaceMemberPermission);

  if (workspaceProjectIds?.length === 0) {
    return (
      <EmptyStateDetailed
        title={t("workspace_projects.empty_state.no_projects.title")}
        description={t("workspace_projects.empty_state.no_projects.description")}
        assetKey="project"
        assetClassName="size-40"
        actions={[
          {
            label: t("workspace_projects.empty_state.no_projects.primary_button.text"),
            onClick: () => {
              toggleCreateProjectModal(true);
            },
            disabled: !hasWorkspaceMemberPermission,
            variant: "primary",
          },
        ]}
      />
    );
  }

  return (
    <EmptyStateDetailed
      title={t(`workspace_empty_state.views.title`)}
      description={t(`workspace_empty_state.views.description`)}
      assetKey="project"
      assetClassName="size-40"
      actions={[
        {
          label: t(`workspace_empty_state.views.cta_primary`),
          onClick: () => {
            toggleCreateIssueModal(true, EIssuesStoreType.PROJECT, scopedProjectId ? [scopedProjectId] : undefined);
          },
          disabled: !canCreateIssue,
          variant: "primary",
        },
      ]}
    />
  );
});
