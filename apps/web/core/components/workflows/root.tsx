/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// ui
import { AlertModalCore, Spinner } from "@plane/ui";
// components
import { SettingsHeading } from "@/components/settings/heading";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// local imports
import { WorkflowFormModal } from "./workflow-form-modal";
import { WorkflowList } from "./workflow-list";
import { WorkflowLoadErrorState } from "./workflow-load-error-state";

type Props = {
  workspaceSlug: string;
};

export const WorkflowsRoot = observer(function WorkflowsRoot(props: Props) {
  const { workspaceSlug } = props;
  // states
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingWorkflowId, setEditingWorkflowId] = useState<string | null>(null);
  const [deletingWorkflowId, setDeletingWorkflowId] = useState<string | null>(null);
  const [isDeleteLoading, setIsDeleteLoading] = useState(false);
  const [hasFetchError, setHasFetchError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workflows, workflowStates, fetchWorkflows, fetchWorkflowStates, deleteWorkflow } = useWorkflow();
  // derived values
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");
  const deletingWorkflow = deletingWorkflowId ? workflows?.find((item) => item.id === deletingWorkflowId) : undefined;

  const loadWorkflows = useCallback(() => {
    setHasFetchError(false);
    void (async () => {
      try {
        const fetchedWorkflows = await fetchWorkflows(workspaceSlug);
        // state counts are shown in the list, so fetch states for every workflow
        await Promise.all(
          fetchedWorkflows.map((workflow) => fetchWorkflowStates(workspaceSlug, workflow.id).catch(() => undefined))
        );
      } catch (error: any) {
        setHasFetchError(true);
        setToast({
          type: TOAST_TYPE.ERROR,
          title: fetchErrorTitle,
          message: error?.error ?? fetchErrorMessage,
        });
      }
    })();
  }, [workspaceSlug, fetchWorkflows, fetchWorkflowStates, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    loadWorkflows();
  }, [loadWorkflows]);

  const handleCloseDeleteModal = () => {
    setDeletingWorkflowId(null);
    setIsDeleteLoading(false);
  };

  const handleDeleteWorkflow = async () => {
    if (!deletingWorkflow) return;

    setIsDeleteLoading(true);

    try {
      await deleteWorkflow(workspaceSlug, deletingWorkflow.id);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("common.success"),
        message: t("entity.delete.success", { entity: deletingWorkflow.name }),
      });
      handleCloseDeleteModal();
    } catch (error: any) {
      setIsDeleteLoading(false);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? t("entity.delete.failed", { entity: deletingWorkflow.name }),
      });
    }
  };

  return (
    <>
      <WorkflowFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        workflowId={editingWorkflowId}
        onClose={() => {
          setIsFormOpen(false);
          setEditingWorkflowId(null);
        }}
      />
      <AlertModalCore
        isOpen={Boolean(deletingWorkflowId)}
        handleClose={handleCloseDeleteModal}
        handleSubmit={handleDeleteWorkflow}
        isSubmitting={isDeleteLoading}
        title={t("workspace_settings.settings.workflows.delete_confirmation.title")}
        content={t("workspace_settings.settings.workflows.delete_confirmation.description")}
        primaryButtonText={{
          loading: t("deleting"),
          default: t("common.delete"),
        }}
      />
      <SettingsHeading
        title={t("workspace_settings.settings.workflows.heading")}
        description={t("workspace_settings.settings.workflows.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditingWorkflowId(null);
              setIsFormOpen(true);
            }}
          >
            {t("workspace_settings.settings.workflows.add_workflow")}
          </Button>
        }
      />
      {workflows === undefined && hasFetchError ? (
        <div className="mt-6">
          <WorkflowLoadErrorState onRetry={loadWorkflows} />
        </div>
      ) : workflows === undefined ? (
        <div className="mt-6 flex h-40 items-center justify-center">
          <Spinner />
        </div>
      ) : workflows.length === 0 ? (
        <div className="mt-6 flex flex-col items-center justify-center gap-1 rounded-lg border border-subtle py-16 text-center">
          <p className="text-13 font-medium text-primary">
            {t("workspace_settings.settings.workflows.empty_state.title")}
          </p>
          <p className="text-11 text-tertiary">{t("workspace_settings.settings.workflows.empty_state.description")}</p>
        </div>
      ) : (
        <WorkflowList
          workspaceSlug={workspaceSlug}
          workflows={workflows}
          workflowStates={workflowStates}
          onEdit={(workflowId) => {
            setEditingWorkflowId(workflowId);
            setIsFormOpen(true);
          }}
          onDelete={(workflowId) => setDeletingWorkflowId(workflowId)}
        />
      )}
    </>
  );
});
