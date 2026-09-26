/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// components
import { SettingsHeading } from "@/components/settings/heading";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// local imports
import { DeleteTypeModal } from "./delete-type-modal";
import { TypeFormModal } from "./type-form-modal";
import { TypeListItem } from "./type-list-item";

type Props = {
  workspaceSlug: string;
};

export const WorkItemTypesRoot = observer(function WorkItemTypesRoot(props: Props) {
  const { workspaceSlug } = props;
  // states
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingTypeId, setEditingTypeId] = useState<string | null>(null);
  const [deletingTypeId, setDeletingTypeId] = useState<string | null>(null);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes, workflows, fetchWorkItemTypes, fetchWorkflows } = useWorkflow();

  useEffect(() => {
    void fetchWorkItemTypes(workspaceSlug);
    void fetchWorkflows(workspaceSlug);
  }, [workspaceSlug, fetchWorkItemTypes, fetchWorkflows]);

  return (
    <>
      <TypeFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        typeId={editingTypeId}
        onClose={() => {
          setIsFormOpen(false);
          setEditingTypeId(null);
        }}
      />
      <DeleteTypeModal
        workspaceSlug={workspaceSlug}
        isOpen={Boolean(deletingTypeId)}
        typeId={deletingTypeId}
        onClose={() => setDeletingTypeId(null)}
      />
      <SettingsHeading
        title={t("workspace_settings.settings.work_item_types.heading")}
        description={t("workspace_settings.settings.work_item_types.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditingTypeId(null);
              setIsFormOpen(true);
            }}
          >
            {t("workspace_settings.settings.work_item_types.add_type")}
          </Button>
        }
      />
      {workItemTypes?.length === 0 ? (
        <div className="mt-6 flex flex-col items-center justify-center gap-1 rounded-lg border border-subtle py-16 text-center">
          <p className="text-13 font-medium text-primary">
            {t("workspace_settings.settings.work_item_types.empty_state.title")}
          </p>
          <p className="text-11 text-tertiary">
            {t("workspace_settings.settings.work_item_types.empty_state.description")}
          </p>
        </div>
      ) : (
        <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
          {workItemTypes?.map((type) => (
            <TypeListItem
              key={type.id}
              type={type}
              workflowName={workflows?.find((workflow) => workflow.id === type.workflow)?.name}
              onEdit={() => {
                setEditingTypeId(type.id);
                setIsFormOpen(true);
              }}
              onDelete={() => setDeletingTypeId(type.id)}
            />
          ))}
        </div>
      )}
    </>
  );
});
