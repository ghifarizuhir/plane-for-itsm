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
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
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
  // states
  const [statesError, setStatesError] = useState(false);
  const [transitionsError, setTransitionsError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workflowStates, workflowTransitions, mapRefreshError, fetchWorkflowStates, fetchWorkflowTransitions } =
    useWorkflow();
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

  useEffect(() => {
    loadStates();
    loadTransitions();
  }, [loadStates, loadTransitions]);

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

  if (states === undefined && statesError) {
    return <WorkflowLoadErrorState onRetry={handleRetryEditor} />;
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
