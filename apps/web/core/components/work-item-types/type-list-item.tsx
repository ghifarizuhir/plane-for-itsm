/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { DeleteOutline, EditOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TWorkItemType } from "@plane/types";
import { CustomMenu } from "@plane/ui";
import { cn } from "@plane/utils";

type Props = {
  workspaceSlug: string;
  type: TWorkItemType;
  onEdit: () => void;
  onDelete: () => void;
};

export const TypeListItem = observer(function TypeListItem(props: Props) {
  const { workspaceSlug, type, onEdit, onDelete } = props;
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const workflowLabel = type.workflow
    ? `${type.name} Workflow`
    : t("workspace_settings.settings.work_item_types.no_workflow");

  return (
    <div className="group flex items-center justify-between gap-4 px-4 py-3">
      <Link href={`/${workspaceSlug}/settings/work-item-types/${type.id}`} className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2">
          <p className="truncate text-13 font-medium text-primary">{type.name}</p>
          {type.is_epic && (
            <span className="flex h-4 max-h-fit items-center rounded-xs bg-accent-primary/20 px-2 text-11 font-medium text-accent-primary">
              {t("common.epic")}
            </span>
          )}
          <span
            className={cn("flex h-4 max-h-fit items-center rounded-xs px-2 text-11 font-medium", {
              "bg-success-subtle text-success-primary": type.is_active,
              "bg-layer-1 text-placeholder": !type.is_active,
            })}
          >
            {type.is_active ? t("common.active") : t("workspace_settings.settings.work_item_types.inactive")}
          </span>
        </div>
        {type.description && <p className="mt-0.5 truncate text-11 text-secondary">{type.description}</p>}
        <p className="mt-0.5 text-11 text-placeholder">{workflowLabel}</p>
      </Link>
      <CustomMenu ellipsis ariaLabel={t("aria_labels.projects_sidebar.toggle_quick_actions_menu")}>
        <CustomMenu.MenuItem onClick={onEdit}>
          <span className="flex items-center justify-start gap-2">
            <EditOutline width={14} height={14} />
            <span>{t("common.edit")}</span>
          </span>
        </CustomMenu.MenuItem>
        <CustomMenu.MenuItem onClick={onDelete}>
          <span className="flex items-center justify-start gap-2">
            <DeleteOutline width={14} height={14} />
            <span>{t("common.delete")}</span>
          </span>
        </CustomMenu.MenuItem>
      </CustomMenu>
    </div>
  );
});
