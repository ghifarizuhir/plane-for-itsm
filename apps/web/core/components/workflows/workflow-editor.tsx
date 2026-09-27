/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// assets
import emptyModule from "@/app/assets/empty-state/module.svg?url";
// components
import { EmptyState } from "@/components/common/empty-state";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { StateList } from "./state-list";
import { TransitionMatrix } from "./transition-matrix";
import { WorkflowLoadErrorState } from "./workflow-load-error-state";

type Props = {
  workspaceSlug: string;
  workflowId: string;
};

export const WorkflowEditor = observer(function WorkflowEditor(props: Props) {
  const { workspaceSlug, workflowId } = props;
  // router
  const router = useAppRouter();
  // states
  const [statesError, setStatesError] = useState(false);
  const [transitionsError, setTransitionsError] = useState(false);
  const [workflowsError, setWorkflowsError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    workflows,
    workflowStates,
    workflowTransitions,
    mapRefreshError,
    fetchWorkflows,
    fetchWorkflowStates,
    fetchWorkflowTransitions,
  } = useWorkflow();
  // projects whose failed map refresh already raised a toast; re-armed when the map recovers
  const warnedMapFailures = useRef(new Set<string>());
  // derived values
  const states = workflowStates[workflowId];
  const transitions = workflowTransitions[workflowId];
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");
  const mapRefreshWarningTitle = t("common.warning");
  const mapRefreshWarningMessage = t("workspace_settings.settings.work_item_types.map_refresh_failed");
  const workflowExists = workflows?.some((workflow) => workflow.id === workflowId) ?? false;
  const isWorkflowNotFound = workflows !== undefined && !workflowExists;

  const loadStates = useCallback(() => {
    setStatesError(false);
    void fetchWorkflowStates(workspaceSlug, workflowId).catch((error: any) => {
      setStatesError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflowId, fetchWorkflowStates, fetchErrorTitle, fetchErrorMessage]);

  const loadTransitions = useCallback(() => {
    setTransitionsError(false);
    void fetchWorkflowTransitions(workspaceSlug, workflowId).catch((error: any) => {
      setTransitionsError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflowId, fetchWorkflowTransitions, fetchErrorTitle, fetchErrorMessage]);

  const loadWorkflows = useCallback(() => {
    if (workflows !== undefined) return;
    setWorkflowsError(false);
    void fetchWorkflows(workspaceSlug).catch((error: any) => {
      setWorkflowsError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflows, fetchWorkflows, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    // wait until the workflow is known to exist so an unknown deep link never fires 404 fetches
    if (!workflowExists) return;
    loadStates();
    loadTransitions();
  }, [workflowExists, loadStates, loadTransitions]);

  useEffect(() => {
    loadWorkflows();
  }, [loadWorkflows]);

  // every editor mutation refreshes the affected project workflow maps; if that
  // refresh failed, board columns can be stale even though the edit landed
  useEffect(() => {
    Object.entries(mapRefreshError).forEach(([projectId, failure]) => {
      if (!failure) {
        warnedMapFailures.current.delete(projectId);
        return;
      }
      if (warnedMapFailures.current.has(projectId)) return;
      warnedMapFailures.current.add(projectId);
      setToast({
        type: TOAST_TYPE.WARNING,
        title: mapRefreshWarningTitle,
        message: mapRefreshWarningMessage,
      });
    });
  }, [mapRefreshError, mapRefreshWarningTitle, mapRefreshWarningMessage]);

  const handleRetryEditor = () => {
    loadStates();
    loadTransitions();
  };

  if (workflows === undefined && workflowsError) {
    return <WorkflowLoadErrorState onRetry={loadWorkflows} />;
  }

  if (isWorkflowNotFound) {
    return (
      <div className="mt-6 flex h-80 items-center justify-center">
        <EmptyState
          image={emptyModule}
          title={t("workspace_settings.settings.workflows.not_found.title")}
          description={t("workspace_settings.settings.workflows.not_found.description")}
          primaryButton={{
            text: t("common.go_back"),
            onClick: () => router.push(`/${workspaceSlug}/settings/workflows`),
          }}
        />
      </div>
    );
  }

  return (
    <div className="mt-6 grid grid-cols-1 gap-8 lg:grid-cols-2">
      <StateList
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        states={states}
        hasError={statesError}
        onRetry={loadStates}
      />
      <TransitionMatrix
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        states={states}
        transitions={transitions}
        hasError={statesError || transitionsError}
        onRetry={handleRetryEditor}
      />
    </div>
  );
});
