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
  typeId: string;
};

export const WorkItemTypeDetailWorkspaceSettingsHeader = observer(function WorkItemTypeDetailWorkspaceSettingsHeader(
  props: Props
) {
  const { workspaceSlug, typeId } = props;
  // translation
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes } = useWorkflow();
  // derived values
  const settingsDetails = WORKSPACE_SETTINGS.work_item_types;
  const Icon = WORKSPACE_SETTINGS_ICONS.work_item_types;
  const type = workItemTypes?.find((item) => item.id === typeId);
  // keep the crumb informative instead of blank when types are loaded but the id does not exist
  const typeLabel = type
    ? type.name
    : workItemTypes !== undefined
      ? t("workspace_settings.settings.work_item_types.not_found.title")
      : undefined;

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
                  href={`/${workspaceSlug}/settings/work-item-types`}
                />
              }
            />
            {typeLabel && <Breadcrumbs.Item component={<BreadcrumbLink label={typeLabel} />} />}
          </Breadcrumbs>
        </div>
      }
    />
  );
});
