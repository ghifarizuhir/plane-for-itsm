/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
import { EditOutline } from "@makeplane/propel/icons";
import { Switch } from "@makeplane/propel/components/switch";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// assets
import emptyModule from "@/app/assets/empty-state/module.svg?url";
// ui
import { Spinner } from "@plane/ui";
// components
import { EmptyState } from "@/components/common/empty-state";
import { WorkflowEditor } from "@/components/workflows";
import { WorkflowLoadErrorState } from "@/components/workflows/workflow-load-error-state";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { TypeFormModal } from "./type-form-modal";

type Props = {
  workspaceSlug: string;
  typeId: string;
};

export const WorkItemTypeDetail = observer(function WorkItemTypeDetail(props: Props) {
  const { workspaceSlug, typeId } = props;
  // router
  const router = useAppRouter();
  // states
  const [hasFetchError, setHasFetchError] = useState(false);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [isActiveLoading, setIsActiveLoading] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes, fetchWorkItemTypes, updateWorkItemType } = useWorkflow();
  // derived values
  const type = workItemTypes?.find((item) => item.id === typeId);
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");

  const loadTypes = useCallback(() => {
    setHasFetchError(false);
    void fetchWorkItemTypes(workspaceSlug).catch((error: any) => {
      setHasFetchError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, fetchWorkItemTypes, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    if (workItemTypes === undefined) loadTypes();
  }, [workItemTypes, loadTypes]);

  const handleToggleActive = async (isActive: boolean) => {
    if (!type) return;
    setIsActiveLoading(true);
    try {
      await updateWorkItemType(workspaceSlug, type.id, { is_active: isActive });
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    } finally {
      setIsActiveLoading(false);
    }
  };

  if (workItemTypes === undefined && hasFetchError) {
    return <WorkflowLoadErrorState onRetry={loadTypes} />;
  }

  if (workItemTypes === undefined) {
    return (
      <div className="mt-6 flex h-40 items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (!type) {
    return (
      <div className="mt-6 flex h-80 items-center justify-center">
        <EmptyState
          image={emptyModule}
          title={t("workspace_settings.settings.work_item_types.not_found.title")}
          description={t("workspace_settings.settings.work_item_types.not_found.description")}
          primaryButton={{
            text: t("common.go_back"),
            onClick: () => router.push(`/${workspaceSlug}/settings/work-item-types`),
          }}
        />
      </div>
    );
  }

  return (
    <>
      <TypeFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        typeId={type.id}
        // TODO(Task 5): drop once TypeFormModal no longer takes isWorkflowsLoading.
        isWorkflowsLoading={false}
        onClose={() => setIsFormOpen(false)}
      />
      <div className="mt-6 flex items-start justify-between gap-4 rounded-lg border border-subtle p-5">
        <div className="flex min-w-0 flex-col">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-16 font-medium text-primary">{type.name}</h3>
            {type.is_epic && (
              <span className="flex h-4 max-h-fit items-center rounded-xs bg-accent-primary/20 px-2 text-11 font-medium text-accent-primary">
                {t("common.epic")}
              </span>
            )}
          </div>
          {type.description && <p className="mt-1 text-13 text-secondary">{type.description}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-4">
          <div className="flex items-center gap-2">
            <span className="text-13 font-medium">{t("workspace_settings.settings.work_item_types.form.active")}</span>
            <Switch
              size="sm"
              checked={type.is_active}
              disabled={isActiveLoading}
              onCheckedChange={(value) => void handleToggleActive(value)}
              aria-label={t("workspace_settings.settings.work_item_types.form.active")}
            />
          </div>
          <Button variant="secondary" size="lg" onClick={() => setIsFormOpen(true)}>
            <EditOutline width={14} height={14} className="mr-1" />
            {t("common.edit")}
          </Button>
        </div>
      </div>
      {type.is_epic ? (
        <p className="mt-6 text-13 text-tertiary">
          {t("workspace_settings.settings.work_item_types.detail.epic_no_workflow")}
        </p>
      ) : type.workflow ? (
        <div className="mt-8">
          <h4 className="text-14 font-medium text-primary">{`${type.name} Workflow`}</h4>
          <WorkflowEditor workspaceSlug={workspaceSlug} workflowId={type.workflow} />
        </div>
      ) : (
        <p className="mt-6 text-13 text-tertiary">
          {t("workspace_settings.settings.work_item_types.detail.no_workflow")}
        </p>
      )}
    </>
  );
});
