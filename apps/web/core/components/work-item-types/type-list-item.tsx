/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { DeleteOutline, EditOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TWorkItemType } from "@plane/types";
import { CustomMenu } from "@plane/ui";
import { cn } from "@plane/utils";

type Props = {
  type: TWorkItemType;
  workflowName?: string;
  onEdit: () => void;
  onDelete: () => void;
};

export const TypeListItem = observer(function TypeListItem(props: Props) {
  const { type, workflowName, onEdit, onDelete } = props;
  // plane hooks
  const { t } = useTranslation();

  return (
    <div className="group flex items-center justify-between gap-4 px-4 py-3">
      <div className="flex min-w-0 flex-col">
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
            {type.is_active ? t("common.active") : "Inactive"}
          </span>
        </div>
        {type.description && <p className="mt-0.5 truncate text-11 text-secondary">{type.description}</p>}
        <p className="mt-0.5 text-11 text-placeholder">{workflowName ?? "No workflow"}</p>
      </div>
      <CustomMenu ellipsis>
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
