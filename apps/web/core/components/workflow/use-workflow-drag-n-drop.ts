/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

/* eslint-disable @typescript-eslint/no-unused-vars */
import { useState } from "react";
import { useTranslation } from "@plane/i18n";
import type { TIssueGroupByOptions } from "@plane/types";
// hooks
import { useProjectState } from "@/hooks/store/use-project-state";

export const useWorkFlowFDragNDrop = (groupBy: TIssueGroupByOptions | undefined, subGroupBy?: TIssueGroupByOptions) => {
  const { t } = useTranslation();
  const { getStateById } = useProjectState();
  const [workflowDisabledSource, setWorkflowDisabledSource] = useState<string | undefined>(undefined);
  const [isWorkflowDropDisabled, setIsWorkflowDropDisabled] = useState(false);

  const isWorkflowAxis = groupBy === "workflow_state" || subGroupBy === "workflow_state";

  const handleWorkFlowState = (
    sourceGroupId: string | undefined,
    destinationGroupId: string | undefined,
    sourceSubGroupId?: string,
    destinationSubGroupId?: string
  ) => {
    if (!isWorkflowAxis) {
      if (isWorkflowDropDisabled) setIsWorkflowDropDisabled(false);
      setWorkflowDisabledSource(undefined);
      return;
    }
    const sourceStateId = groupBy === "workflow_state" ? sourceGroupId : sourceSubGroupId;
    const destinationStateId = groupBy === "workflow_state" ? destinationGroupId : destinationSubGroupId;
    const sourceTypeId = sourceStateId ? getStateById(sourceStateId)?.type_id : undefined;
    const destinationTypeId = destinationStateId ? getStateById(destinationStateId)?.type_id : undefined;
    const isBlocked = Boolean(sourceTypeId && destinationTypeId && sourceTypeId !== destinationTypeId);
    setIsWorkflowDropDisabled(isBlocked);
    setWorkflowDisabledSource(isBlocked ? sourceStateId : undefined);
  };

  return {
    workflowDisabledSource,
    isWorkflowDropDisabled,
    getIsWorkflowWorkItemCreationDisabled: (_groupId: string, _subGroupId?: string) => false,
    workflowDropErrorMessage: isWorkflowDropDisabled ? t("common.workflow_state_wrong_type") : undefined,
    handleWorkFlowState,
  };
};
