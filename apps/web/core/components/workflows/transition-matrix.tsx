/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Fragment } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkflowState, TWorkflowTransition } from "@plane/types";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// store
import { buildTransitionMatrix } from "@/store/workflow.helpers";

type Props = {
  workspaceSlug: string;
  workflowId: string;
  states: TWorkflowState[];
  transitions: TWorkflowTransition[];
};

export const TransitionMatrix = observer(function TransitionMatrix(props: Props) {
  const { workspaceSlug, workflowId, states, transitions } = props;
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { createWorkflowTransition, deleteWorkflowTransition } = useWorkflow();
  // derived values
  const matrix = buildTransitionMatrix(states, transitions);

  const toggle = async (fromId: string, toId: string, exists: boolean) => {
    try {
      if (exists) {
        const transition = transitions.find((item) => item.from_state_id === fromId && item.to_state_id === toId);
        if (transition) await deleteWorkflowTransition(workspaceSlug, workflowId, transition.id);
      } else {
        await createWorkflowTransition(workspaceSlug, workflowId, { from_state_id: fromId, to_state_id: toId });
      }
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: error?.error ?? t("common.error.message"),
      });
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <h4 className="text-14 font-medium">{t("workspace_settings.settings.workflows.transitions.heading")}</h4>
        <p className="text-11 text-tertiary">{t("workspace_settings.settings.workflows.transitions.description")}</p>
      </div>
      {states.length === 0 ? (
        <p className="rounded-lg border border-dashed border-subtle px-4 py-6 text-center text-11 text-tertiary">
          {t("common.no_items_in_this_group")}
        </p>
      ) : (
        <div
          className="grid items-center gap-2 overflow-x-auto"
          style={{ gridTemplateColumns: `120px repeat(${states.length}, minmax(24px, 1fr))` }}
        >
          <span />
          {states.map((state) => (
            <span key={state.id} className="truncate text-caption-md-medium text-tertiary">
              {state.name}
            </span>
          ))}
          {states.map((from) => (
            <Fragment key={from.id}>
              <span className="truncate text-caption-md-medium text-tertiary">{from.name}</span>
              {states.map((to) => {
                const cell = matrix.find((item) => item.from_state_id === from.id && item.to_state_id === to.id);
                if (!cell) return <span key={to.id} />;
                return (
                  <button
                    key={to.id}
                    type="button"
                    aria-label={`${from.name} → ${to.name}`}
                    aria-pressed={cell.exists}
                    className={`h-6 w-6 rounded border ${cell.exists ? "border-accent-strong bg-accent-primary" : "border-subtle"}`}
                    onClick={() => void toggle(from.id, to.id, cell.exists)}
                  />
                );
              })}
            </Fragment>
          ))}
        </div>
      )}
    </div>
  );
});
