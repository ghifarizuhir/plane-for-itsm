/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// ui
import { Spinner } from "@plane/ui";
// components
import { WorkflowLoadErrorState } from "@/components/workflows/workflow-load-error-state";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// local imports
import { TypeToggleItem } from "./type-toggle-item";

type Props = {
  workspaceSlug: string;
  projectId: string;
};

export const ProjectWorkItemTypesRoot = observer(function ProjectWorkItemTypesRoot(props: Props) {
  const { workspaceSlug, projectId } = props;
  // states
  const [hasFetchError, setHasFetchError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    workItemTypes,
    workflows,
    workflowMap,
    mapRefreshError,
    fetchWorkItemTypes,
    fetchWorkflows,
    fetchWorkflowMap,
    importWorkItemTypes,
    unlinkWorkItemType,
  } = useWorkflow();
  // derived values
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");
  const workflowMapType = workflowMap[projectId];
  // status enable mengikuti link project (project_issue_types), bukan workflow-map:
  // map menyembunyikan type nonaktif, sementara link-nya tetap hidup dan harus
  // tetap bisa di-unlink dari halaman ini.
  const enabledIds = new Set(
    (workItemTypes ?? []).filter((type) => type.project_ids?.includes(projectId)).map((type) => type.id)
  );

  const loadData = useCallback(() => {
    setHasFetchError(false);
    void Promise.all([
      fetchWorkItemTypes(workspaceSlug),
      fetchWorkflows(workspaceSlug),
      fetchWorkflowMap(workspaceSlug, projectId),
    ]).catch((error: any) => {
      setHasFetchError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [
    workspaceSlug,
    projectId,
    fetchWorkItemTypes,
    fetchWorkflows,
    fetchWorkflowMap,
    fetchErrorTitle,
    fetchErrorMessage,
  ]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleToggle = async (typeId: string, next: boolean) => {
    try {
      if (next) await importWorkItemTypes(workspaceSlug, projectId, [typeId]);
      else await unlinkWorkItemType(workspaceSlug, projectId, typeId);
      if (mapRefreshError[projectId]) {
        setToast({
          type: TOAST_TYPE.WARNING,
          title: t("common.warning"),
          message: t("workspace_settings.settings.work_item_types.map_refresh_failed"),
        });
      }
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    }
  };

  if (workItemTypes === undefined || workflows === undefined || workflowMapType === undefined) {
    if (hasFetchError) {
      return (
        <div className="mt-6">
          <WorkflowLoadErrorState onRetry={loadData} />
        </div>
      );
    }
    return (
      <div className="mt-6 flex h-40 items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (workItemTypes.length === 0) {
    return (
      <div className="mt-6 flex flex-col items-center justify-center gap-1 rounded-lg border border-subtle py-16 text-center">
        <p className="text-13 font-medium text-primary">
          {t("workspace_settings.settings.work_item_types.empty_state.title")}
        </p>
        <p className="text-11 text-tertiary">
          {t("workspace_settings.settings.work_item_types.empty_state.description")}
        </p>
      </div>
    );
  }

  return (
    <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
      {workItemTypes.map((type) => (
        <TypeToggleItem
          key={type.id}
          type={type}
          workflowName={workflows.find((workflow) => workflow.id === type.workflow)?.name}
          enabled={enabledIds.has(type.id)}
          disabledReason={
            !type.workflow ? t("workspace_settings.settings.work_item_types.attach_workflow_first") : undefined
          }
          onToggle={(next) => handleToggle(type.id, next)}
        />
      ))}
    </div>
  );
});
