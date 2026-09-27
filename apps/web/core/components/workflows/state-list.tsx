/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { ChevronDownOutline, ChevronUpOutline, DeleteOutline, EditOutline } from "@makeplane/propel/icons";
// plane imports
import { EIconSize } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { StateGroupIcon } from "@plane/propel/icons";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkflowState } from "@plane/types";
// ui
import { AlertModalCore, Spinner } from "@plane/ui";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
// local imports
import { StateFormModal } from "./state-form-modal";
import { WorkflowLoadErrorState } from "./workflow-load-error-state";

type Props = {
  workspaceSlug: string;
  workflowId: string;
  states?: TWorkflowState[];
  hasError: boolean;
  onRetry: () => void;
};

export const StateList = observer(function StateList(props: Props) {
  const { workspaceSlug, workflowId, states, hasError, onRetry } = props;
  // states
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingStateId, setEditingStateId] = useState<string | null>(null);
  const [deletingStateId, setDeletingStateId] = useState<string | null>(null);
  const [isDeleteLoading, setIsDeleteLoading] = useState(false);
  const [busyStateId, setBusyStateId] = useState<string | null>(null);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateWorkflowState, deleteWorkflowState } = useWorkflow();
  // derived values
  const sortedStates = states
    ? // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread already copies the array
      [...states].sort((a, b) => a.sequence - b.sequence)
    : undefined;
  const deletingState = deletingStateId ? sortedStates?.find((item) => item.id === deletingStateId) : undefined;

  const handleCloseDeleteModal = () => {
    setDeletingStateId(null);
    setIsDeleteLoading(false);
  };

  const handleDeleteState = async () => {
    if (!deletingState) return;

    setIsDeleteLoading(true);

    try {
      await deleteWorkflowState(workspaceSlug, workflowId, deletingState.id);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("common.success"),
        message: t("entity.delete.success", { entity: deletingState.name }),
      });
      handleCloseDeleteModal();
    } catch (error: any) {
      setIsDeleteLoading(false);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: error?.error ?? t("entity.delete.failed", { entity: deletingState.name }),
      });
    }
  };

  const handleSetDefault = async (state: TWorkflowState) => {
    setBusyStateId(state.id);

    try {
      await updateWorkflowState(workspaceSlug, workflowId, state.id, { is_default: true });
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("common.success"),
        message: t("entity.update.success", { entity: t("common.state") }),
      });
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: error?.error ?? t("entity.update.failed", { entity: t("common.state") }),
      });
    } finally {
      setBusyStateId(null);
    }
  };

  const handleMove = async (state: TWorkflowState, direction: "up" | "down") => {
    if (!sortedStates) return;

    const index = sortedStates.findIndex((item) => item.id === state.id);
    if (index === -1) return;

    // place the state between its new neighbours; boundary moves step beyond the edge value
    let sequence: number;
    if (direction === "up") {
      if (index === 0) return;
      const previous = sortedStates[index - 2];
      const next = sortedStates[index - 1];
      sequence = previous ? (previous.sequence + next.sequence) / 2 : next.sequence - 1;
    } else {
      if (index === sortedStates.length - 1) return;
      const previous = sortedStates[index + 1];
      const next = sortedStates[index + 2];
      sequence = next ? (previous.sequence + next.sequence) / 2 : previous.sequence + 1;
    }

    setBusyStateId(state.id);

    try {
      await updateWorkflowState(workspaceSlug, workflowId, state.id, { sequence });
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("common.success"),
        message: t("entity.update.success", { entity: t("common.state") }),
      });
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: error?.error ?? t("entity.update.failed", { entity: t("common.state") }),
      });
    } finally {
      setBusyStateId(null);
    }
  };

  return (
    <>
      <StateFormModal
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        isOpen={isFormOpen}
        stateId={editingStateId}
        onClose={() => {
          setIsFormOpen(false);
          setEditingStateId(null);
        }}
      />
      <AlertModalCore
        isOpen={Boolean(deletingStateId)}
        handleClose={handleCloseDeleteModal}
        handleSubmit={handleDeleteState}
        isSubmitting={isDeleteLoading}
        title={t("workspace_settings.settings.workflows.states.delete_confirmation.title")}
        content={t("workspace_settings.settings.workflows.states.delete_confirmation.description")}
        primaryButtonText={{
          loading: t("deleting"),
          default: t("common.delete"),
        }}
      />
      {hasError ? (
        <WorkflowLoadErrorState onRetry={onRetry} />
      ) : (
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2">
            <h4 className="text-14 font-medium">{t("workspace_settings.settings.workflows.states.heading")}</h4>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setEditingStateId(null);
                setIsFormOpen(true);
              }}
            >
              {t("workspace_settings.settings.workflows.states.add_state")}
            </Button>
          </div>
          {sortedStates === undefined ? (
            <div className="flex h-40 items-center justify-center">
              <Spinner />
            </div>
          ) : sortedStates.length === 0 ? (
            <div className="flex items-center justify-center py-8 text-13 text-tertiary">
              {t("workspace_settings.settings.workflows.no_states")}
            </div>
          ) : (
            <div className="flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
              {sortedStates.map((state, index) => {
                const isFirst = index === 0;
                const isLast = index === sortedStates.length - 1;

                return (
                  <div key={state.id} className="group flex items-center justify-between gap-2 px-4 py-2.5">
                    <div className="flex min-w-0 items-center gap-2">
                      <StateGroupIcon stateGroup={state.group} color={state.color} size={EIconSize.LG} />
                      <div className="flex min-w-0 flex-col">
                        <div className="flex items-center gap-2">
                          <p className="truncate text-13 font-medium text-primary">{state.name}</p>
                          {state.is_default && (
                            <span className="flex h-4 max-h-fit items-center rounded-xs bg-accent-primary/20 px-2 text-11 font-medium text-accent-primary">
                              {t("workspace_settings.settings.workflows.states.default")}
                            </span>
                          )}
                        </div>
                        {state.description && <p className="truncate text-11 text-secondary">{state.description}</p>}
                      </div>
                    </div>
                    <div className="flex flex-shrink-0 items-center gap-1">
                      {!state.is_default && (
                        <button
                          type="button"
                          className="rounded-sm px-2 py-1 text-11 text-accent-primary transition-colors hover:bg-layer-transparent-hover disabled:cursor-not-allowed disabled:text-placeholder"
                          disabled={Boolean(busyStateId)}
                          onClick={() => void handleSetDefault(state)}
                        >
                          {t("workspace_settings.settings.workflows.states.set_default")}
                        </button>
                      )}
                      <button
                        type="button"
                        aria-label={t("common.move_up")}
                        className="flex size-6 items-center justify-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-primary disabled:cursor-not-allowed disabled:text-placeholder"
                        disabled={isFirst || Boolean(busyStateId)}
                        onClick={() => void handleMove(state, "up")}
                      >
                        <ChevronUpOutline className="size-3.5" />
                      </button>
                      <button
                        type="button"
                        aria-label={t("common.move_down")}
                        className="flex size-6 items-center justify-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-primary disabled:cursor-not-allowed disabled:text-placeholder"
                        disabled={isLast || Boolean(busyStateId)}
                        onClick={() => void handleMove(state, "down")}
                      >
                        <ChevronDownOutline className="size-3.5" />
                      </button>
                      <button
                        type="button"
                        className="flex size-6 items-center justify-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-primary"
                        aria-label={t("common.edit")}
                        onClick={() => {
                          setEditingStateId(state.id);
                          setIsFormOpen(true);
                        }}
                      >
                        <EditOutline className="size-3.5" />
                      </button>
                      <button
                        type="button"
                        className="flex size-6 items-center justify-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-danger-primary"
                        aria-label={t("common.delete")}
                        onClick={() => setDeletingStateId(state.id)}
                      >
                        <DeleteOutline className="size-3.5" />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </>
  );
});
