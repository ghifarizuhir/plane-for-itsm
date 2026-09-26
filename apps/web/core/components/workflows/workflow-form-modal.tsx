/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
import { Controller, useForm } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkflowPayload } from "@plane/types";
// ui
import { EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  isOpen: boolean;
  workflowId: string | null;
  onClose: () => void;
};

const defaultValues: TWorkflowPayload = {
  name: "",
  description: "",
};

export const WorkflowFormModal = observer(function WorkflowFormModal(props: Props) {
  const { workspaceSlug, isOpen, workflowId, onClose } = props;
  // store hooks
  const { workflows, createWorkflow, updateWorkflow } = useWorkflow();
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const workflow = workflowId ? workflows?.find((item) => item.id === workflowId) : undefined;
  // form
  const {
    control,
    handleSubmit,
    reset,
    formState: { errors, isSubmitting },
  } = useForm<TWorkflowPayload>({ defaultValues });

  useEffect(() => {
    if (!isOpen) return;
    reset(
      workflow
        ? {
            name: workflow.name,
            description: workflow.description,
          }
        : defaultValues
    );
  }, [isOpen, workflow, reset]);

  const handleClose = () => {
    reset(defaultValues);
    onClose();
  };

  const onSubmit = async (formData: TWorkflowPayload) => {
    const isEdit = Boolean(workflowId);

    try {
      if (workflowId) await updateWorkflow(workspaceSlug, workflowId, formData);
      else await createWorkflow(workspaceSlug, formData);

      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: isEdit
          ? t("project_settings.workflows.update.success.title")
          : t("project_settings.workflows.create.success.title"),
        message: isEdit
          ? t("project_settings.workflows.update.success.message")
          : t("project_settings.workflows.create.success.message"),
      });
      handleClose();
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: isEdit
          ? t("project_settings.workflows.update.error.title")
          : t("project_settings.workflows.create.error.title"),
        message:
          error?.error ??
          (isEdit
            ? t("project_settings.workflows.update.error.message")
            : t("project_settings.workflows.create.error.message")),
      });
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={handleClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-5 p-5">
          <h3 className="text-18 font-medium text-secondary">
            {workflowId ? t("common.update") : t("workspace_settings.settings.workflows.add_workflow")}
          </h3>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-13 font-medium" htmlFor="workflow-name">
                {t("workspace_settings.settings.workflows.form.name")}
              </label>
              <Controller
                control={control}
                name="name"
                rules={{ required: t("name_is_required") }}
                render={({ field: { value, onChange } }) => (
                  <Field name="input" invalid={Boolean(errors.name)}>
                    <InputGroup size="2xl">
                      <Input
                        id="workflow-name"
                        size="2xl"
                        type="text"
                        value={value ?? ""}
                        onChange={onChange}
                        placeholder={t("project_settings.workflows.create.name.placeholder")}
                        aria-label={t("workspace_settings.settings.workflows.form.name")}
                      />
                    </InputGroup>
                  </Field>
                )}
              />
              {errors.name && <span className="text-11 text-danger-primary">{errors.name.message}</span>}
            </div>
            <div className="space-y-1">
              <label className="text-13 font-medium" htmlFor="workflow-description">
                {t("workspace_settings.settings.workflows.form.description")}
              </label>
              <Controller
                control={control}
                name="description"
                render={({ field: { value, onChange } }) => (
                  <TextArea
                    id="workflow-description"
                    value={value ?? ""}
                    onChange={onChange}
                    hasError={Boolean(errors.description)}
                    placeholder={t("project_settings.workflows.create.description.placeholder")}
                    className="min-h-24 w-full resize-none text-14"
                  />
                )}
              />
            </div>
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
