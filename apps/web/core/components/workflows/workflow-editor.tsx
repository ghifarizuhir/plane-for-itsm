/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// local imports
import { StateList } from "./state-list";
import { TransitionMatrix } from "./transition-matrix";

type Props = {
  workspaceSlug: string;
  workflowId: string;
};

export const WorkflowEditor = observer(function WorkflowEditor(props: Props) {
  const { workspaceSlug, workflowId } = props;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    workflows,
    workflowStates,
    workflowTransitions,
    fetchWorkflows,
    fetchWorkflowStates,
    fetchWorkflowTransitions,
  } = useWorkflow();
  // derived values
  const states = workflowStates[workflowId];
  const transitions = workflowTransitions[workflowId];
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");

  useEffect(() => {
    void fetchWorkflowStates(workspaceSlug, workflowId).catch((error: any) => {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
    void fetchWorkflowTransitions(workspaceSlug, workflowId).catch((error: any) => {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflowId, fetchWorkflowStates, fetchWorkflowTransitions, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    if (workflows !== undefined) return;
    void fetchWorkflows(workspaceSlug).catch((error: any) => {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, workflows, fetchWorkflows, fetchErrorTitle, fetchErrorMessage]);

  return (
    <div className="mt-6 grid grid-cols-1 gap-8 lg:grid-cols-2">
      <StateList workspaceSlug={workspaceSlug} workflowId={workflowId} states={states} />
      <TransitionMatrix
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        states={states ?? []}
        transitions={transitions ?? []}
      />
    </div>
  );
});
