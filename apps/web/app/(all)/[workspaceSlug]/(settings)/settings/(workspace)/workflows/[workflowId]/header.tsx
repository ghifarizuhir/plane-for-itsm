/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { WORKSPACE_SETTINGS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { SettingsPageHeader } from "@/components/settings/page-header";
import { WORKSPACE_SETTINGS_ICONS } from "@/components/settings/workspace/sidebar/item-icon";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  workflowId: string;
};

export const WorkflowEditorWorkspaceSettingsHeader = observer(function WorkflowEditorWorkspaceSettingsHeader(
  props: Props
) {
  const { workspaceSlug, workflowId } = props;
  // translation
  const { t } = useTranslation();
  // store hooks
  const { workflows } = useWorkflow();
  // derived values
  const settingsDetails = WORKSPACE_SETTINGS.workflows;
  const Icon = WORKSPACE_SETTINGS_ICONS.workflows;
  const workflow = workflows?.find((item) => item.id === workflowId);

  return (
    <SettingsPageHeader
      leftItem={
        <div className="flex items-center gap-2">
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label={t(settingsDetails.i18n_label)}
                  icon={<Icon className="size-4 text-tertiary" />}
                  href={`/${workspaceSlug}/settings/workflows`}
                />
              }
            />
            {workflow && <Breadcrumbs.Item component={<BreadcrumbLink label={workflow.name} />} />}
          </Breadcrumbs>
        </div>
      }
    />
  );
});
