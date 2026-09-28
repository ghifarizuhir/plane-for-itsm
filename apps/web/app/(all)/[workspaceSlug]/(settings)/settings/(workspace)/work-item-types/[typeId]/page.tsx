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
import { WorkItemTypeDetail } from "@/components/work-item-types";
// hooks
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import type { Route } from "./+types/page";
import { WorkItemTypeDetailWorkspaceSettingsHeader } from "./header";

function WorkItemTypeDetailSettingsPage({ params }: Route.ComponentProps) {
  // router
  const { workspaceSlug, typeId } = params;
  // plane hooks
  const { t } = useTranslation();
  // mobx store
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const canManageWorkItemTypes = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.work_item_types.title")}`
    : undefined;

  if (workspaceUserInfo && !canManageWorkItemTypes) {
    return <NotAuthorizedView section="settings" className="h-auto" />;
  }

  return (
    <SettingsContentWrapper
      header={<WorkItemTypeDetailWorkspaceSettingsHeader workspaceSlug={workspaceSlug} typeId={typeId} />}
    >
      <PageHead title={pageTitle} />
      {workspaceSlug && typeId && (
        <WorkItemTypeDetail workspaceSlug={workspaceSlug.toString()} typeId={typeId.toString()} />
      )}
    </SettingsContentWrapper>
  );
}

export default observer(WorkItemTypeDetailSettingsPage);
