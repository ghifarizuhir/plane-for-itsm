/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { FeaturesSettingsForm } from "@/components/settings/workspace-features/form";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";
// local imports
import type { Route } from "./+types/page";
import { FeaturesWorkspaceSettingsHeader } from "./header";

function FeaturesSettingsPage({ params }: Route.ComponentProps) {
  // router
  const { workspaceSlug } = params;
  // plane hooks
  const { t } = useTranslation();
  // mobx store
  const { currentWorkspace } = useWorkspace();
  // derived values
  const pageTitle = currentWorkspace?.name
    ? `${currentWorkspace.name} - ${t("workspace_settings.settings.features.title")}`
    : undefined;

  return (
    <SettingsContentWrapper header={<FeaturesWorkspaceSettingsHeader />}>
      <PageHead title={pageTitle} />
      {workspaceSlug && <FeaturesSettingsForm workspaceSlug={workspaceSlug.toString()} />}
    </SettingsContentWrapper>
  );
}

export default observer(FeaturesSettingsPage);
