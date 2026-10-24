/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useParams } from "next/navigation";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
// components
import { NotAuthorizedView } from "@/components/auth-screens/not-authorized-view";
import { PageHead } from "@/components/core/page-title";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
import { MetricSourcesRoot } from "@/components/services/metrics/sources";
// hooks
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import { MetricSourcesSettingsHeader } from "./header";

function MetricSourcesPage() {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const { currentWorkspace } = useWorkspace();
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const canManage = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.metric_sources.heading")}`
    : undefined;

  if (workspaceUserInfo && !canManage) {
    return <NotAuthorizedView section="settings" className="h-auto" />;
  }

  return (
    <SettingsContentWrapper header={<MetricSourcesSettingsHeader />}>
      <PageHead title={pageTitle} />
      <MetricSourcesRoot workspaceSlug={workspaceSlug?.toString() ?? ""} />
    </SettingsContentWrapper>
  );
}

export default observer(MetricSourcesPage);
