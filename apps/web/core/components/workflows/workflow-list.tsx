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
import type { TWorkflow, TWorkflowState } from "@plane/types";
import { CustomMenu } from "@plane/ui";
import { cn } from "@plane/utils";

type Props = {
  workspaceSlug: string;
  workflows: TWorkflow[];
  workflowStates: Record<string, TWorkflowState[]>;
  onEdit: (workflowId: string) => void;
  onDelete: (workflowId: string) => void;
};

export const WorkflowList = observer(function WorkflowList(props: Props) {
  const { workspaceSlug, workflows, workflowStates, onEdit, onDelete } = props;
  // plane hooks
  const { t } = useTranslation();

  return (
    <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
      {workflows.map((workflow) => {
        const stateCount = workflowStates[workflow.id]?.length;

        return (
          <div key={workflow.id} className="group flex items-center justify-between gap-4 px-4 py-3">
            <Link href={`/${workspaceSlug}/settings/workflows/${workflow.id}`} className="flex min-w-0 flex-1 flex-col">
              <div className="flex items-center gap-2">
                <p className="truncate text-13 font-medium text-primary">{workflow.name}</p>
                {workflow.is_active && (
                  <span className="flex h-4 max-h-fit items-center rounded-xs bg-success-subtle px-2 text-11 font-medium text-success-primary">
                    {t("workspace_settings.settings.workflows.form.active")}
                  </span>
                )}
                {stateCount !== undefined && (
                  <span
                    className={cn("flex h-4 max-h-fit items-center rounded-xs px-2 text-11 font-medium", {
                      "bg-accent-primary/20 text-accent-primary": stateCount > 0,
                      "bg-layer-1 text-placeholder": stateCount === 0,
                    })}
                  >
                    {stateCount} {t("common.states")}
                  </span>
                )}
              </div>
              {workflow.description && <p className="mt-0.5 truncate text-11 text-secondary">{workflow.description}</p>}
            </Link>
            <CustomMenu ellipsis ariaLabel={t("aria_labels.projects_sidebar.toggle_quick_actions_menu")}>
              <CustomMenu.MenuItem onClick={() => onEdit(workflow.id)}>
                <span className="flex items-center justify-start gap-2">
                  <EditOutline width={14} height={14} />
                  <span>{t("common.edit")}</span>
                </span>
              </CustomMenu.MenuItem>
              <CustomMenu.MenuItem onClick={() => onDelete(workflow.id)}>
                <span className="flex items-center justify-start gap-2">
                  <DeleteOutline width={14} height={14} />
                  <span>{t("common.delete")}</span>
                </span>
              </CustomMenu.MenuItem>
            </CustomMenu>
          </div>
        );
      })}
    </div>
  );
});
