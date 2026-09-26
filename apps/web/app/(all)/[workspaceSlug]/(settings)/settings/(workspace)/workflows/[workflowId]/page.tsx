/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
// components
import { NotAuthorizedView } from "@/components/auth-screens/not-authorized-view";
import { PageHead } from "@/components/core/page-title";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
import { WorkflowEditor } from "@/components/workflows";
// hooks
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import type { Route } from "./+types/page";
import { WorkflowEditorWorkspaceSettingsHeader } from "./header";

function WorkflowEditorPage({ params }: Route.ComponentProps) {
  // router
  const { workspaceSlug, workflowId } = params;
  // plane hooks
  const { t } = useTranslation();
  // mobx store
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const canManageWorkflows = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.workflows.title")}`
    : undefined;

  if (workspaceUserInfo && !canManageWorkflows) {
    return <NotAuthorizedView section="settings" className="h-auto" />;
  }

  return (
    <SettingsContentWrapper
      header={<WorkflowEditorWorkspaceSettingsHeader workspaceSlug={workspaceSlug} workflowId={workflowId} />}
    >
      <PageHead title={pageTitle} />
      {workspaceSlug && workflowId && (
        <WorkflowEditor workspaceSlug={workspaceSlug.toString()} workflowId={workflowId.toString()} />
      )}
    </SettingsContentWrapper>
  );
}

export default observer(WorkflowEditorPage);
