/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { Controller, useForm } from "react-hook-form";
import { TwitterPicker } from "react-color";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { STATE_GROUPS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TStateGroups } from "@plane/types";
// ui
import { CustomSelect, EModalPosition, EModalWidth, ModalCore, Popover, TextArea } from "@plane/ui";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  workflowId: string;
  isOpen: boolean;
  stateId: string | null;
  onClose: () => void;
};

type TWorkflowStateForm = {
  name: string;
  description: string;
  color: string;
  group: TStateGroups;
};

const defaultValues: TWorkflowStateForm = {
  name: "",
  description: "",
  color: STATE_GROUPS.backlog.color,
  group: "backlog",
};

function ColorPickerButton({ color }: { color?: string }) {
  return (
    <div
      className="group inline-flex h-7 w-7 items-center rounded-sm border border-subtle transition-all focus:outline-none"
      style={{ backgroundColor: color ?? defaultValues.color }}
    />
  );
}

export const StateFormModal = observer(function StateFormModal(props: Props) {
  const { workspaceSlug, workflowId, isOpen, stateId, onClose } = props;
  // store hooks
  const { workflowStates, createWorkflowState, updateWorkflowState } = useWorkflow();
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const state = stateId ? workflowStates[workflowId]?.find((item) => item.id === stateId) : undefined;
  // form
  const {
    control,
    handleSubmit,
    reset,
    formState: { errors, isSubmitting },
  } = useForm<TWorkflowStateForm>({ defaultValues });

  useEffect(() => {
    if (!isOpen) return;
    reset(
      state
        ? {
            name: state.name,
            description: state.description,
            color: state.color,
            group: state.group,
          }
        : defaultValues
    );
  }, [isOpen, state, reset]);

  const handleClose = () => {
    reset(defaultValues);
    onClose();
  };

  const onSubmit = async (formData: TWorkflowStateForm) => {
    const isEdit = Boolean(stateId);

    try {
      if (stateId) await updateWorkflowState(workspaceSlug, workflowId, stateId, formData);
      else await createWorkflowState(workspaceSlug, workflowId, formData);

      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("common.success"),
        message: isEdit
          ? t("entity.update.success", { entity: t("common.state") })
          : t("entity.add.success", { entity: t("common.state") }),
      });
      handleClose();
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message:
          error?.error ??
          (isEdit
            ? t("entity.update.failed", { entity: t("common.state") })
            : t("entity.add.failed", { entity: t("common.state") })),
      });
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={handleClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-5 p-5">
          <h3 className="text-18 font-medium text-secondary">
            {stateId
              ? `${t("common.update")} ${t("common.state")}`
              : t("workspace_settings.settings.workflows.states.add_state")}
          </h3>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-13 font-medium" htmlFor="workflow-state-name">
                {t("workspace_settings.settings.workflows.states.name")}
              </label>
              <Controller
                control={control}
                name="name"
                rules={{ required: t("name_is_required") }}
                render={({ field: { value, onChange } }) => (
                  <Field name="input" invalid={Boolean(errors.name)}>
                    <InputGroup size="2xl">
                      <Input
                        id="workflow-state-name"
                        size="2xl"
                        type="text"
                        value={value ?? ""}
                        onChange={onChange}
                        placeholder={t("workspace_settings.settings.workflows.states.name")}
                        aria-label={t("workspace_settings.settings.workflows.states.name")}
                      />
                    </InputGroup>
                  </Field>
                )}
              />
              {errors.name && <span className="text-11 text-danger-primary">{errors.name.message}</span>}
            </div>
            <div className="space-y-1">
              <label className="text-13 font-medium" htmlFor="workflow-state-description">
                {t("workspace_settings.settings.workflows.form.description")}
              </label>
              <Controller
                control={control}
                name="description"
                render={({ field: { value, onChange } }) => (
                  <TextArea
                    id="workflow-state-description"
                    value={value ?? ""}
                    onChange={onChange}
                    hasError={Boolean(errors.description)}
                    placeholder={t("project_settings.states.describe_this_state_for_your_members")}
                    className="min-h-24 w-full resize-none text-14"
                  />
                )}
              />
            </div>
            <div className="flex flex-wrap items-center gap-6">
              <div className="flex items-center gap-2">
                <span className="text-13 font-medium">{t("workspace_settings.settings.workflows.states.color")}</span>
                <Controller
                  control={control}
                  name="color"
                  render={({ field: { value, onChange } }) => (
                    <Popover button={<ColorPickerButton color={value} />} panelClassName="mt-2 -ml-3">
                      <TwitterPicker color={value} onChange={(color) => onChange(color.hex)} />
                    </Popover>
                  )}
                />
              </div>
              <div className="flex items-center gap-2">
                <span className="text-13 font-medium">{t("workspace_settings.settings.workflows.states.group")}</span>
                <Controller
                  control={control}
                  name="group"
                  render={({ field: { value, onChange } }) => (
                    <CustomSelect
                      value={value}
                      onChange={onChange}
                      input
                      className="w-40"
                      label={<span className="truncate text-13">{STATE_GROUPS[value]?.label}</span>}
                    >
                      {Object.values(STATE_GROUPS).map((group) => (
                        <CustomSelect.Option key={group.key} value={group.key}>
                          {group.label}
                        </CustomSelect.Option>
                      ))}
                    </CustomSelect>
                  )}
                />
              </div>
            </div>
            {state?.is_default && (
              <span className="inline-flex h-5 items-center rounded-xs bg-accent-primary/20 px-2 text-11 font-medium text-accent-primary">
                {t("workspace_settings.settings.workflows.states.default")}
              </span>
            )}
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
          <Button variant="secondary" size="lg" onClick={handleClose}>
            {t("cancel")}
          </Button>
          <Button type="submit" variant="primary" size="lg" loading={isSubmitting}>
            {isSubmitting ? t("saving") : t("save")}
          </Button>
        </div>
      </form>
    </ModalCore>
  );
});
