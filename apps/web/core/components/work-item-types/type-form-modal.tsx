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
import { Switch } from "@makeplane/propel/components/switch";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkItemTypePayload } from "@plane/types";
// ui
import { CustomSelect, EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";

type Props = {
  workspaceSlug: string;
  isOpen: boolean;
  typeId: string | null;
  onClose: () => void;
};

const defaultValues: TWorkItemTypePayload = {
  name: "",
  description: "",
  workflow: null,
  is_active: true,
};

export const TypeFormModal = observer(function TypeFormModal(props: Props) {
  const { workspaceSlug, isOpen, typeId, onClose } = props;
  // store hooks
  const { workItemTypes, workflows, createWorkItemType, updateWorkItemType } = useWorkflow();
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const type = typeId ? workItemTypes?.find((item) => item.id === typeId) : undefined;
  // form
  const {
    control,
    handleSubmit,
    reset,
    formState: { errors, isSubmitting },
  } = useForm<TWorkItemTypePayload>({ defaultValues });

  useEffect(() => {
    if (!isOpen) return;
    reset(
      type
        ? {
            name: type.name,
            description: type.description,
            workflow: type.workflow,
            is_active: type.is_active,
          }
        : defaultValues
    );
  }, [isOpen, type, reset]);

  const handleClose = () => {
    reset(defaultValues);
    onClose();
  };

  const onSubmit = async (formData: TWorkItemTypePayload) => {
    try {
      if (typeId) await updateWorkItemType(workspaceSlug, typeId, formData);
      else await createWorkItemType(workspaceSlug, formData);

      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: "Success!",
        message: typeId ? "Work item type updated successfully." : "Work item type created successfully.",
      });
      handleClose();
    } catch (error: any) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: error?.error ?? "Work item type could not be saved. Please try again.",
      });
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={handleClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-5 p-5">
          <h3 className="text-18 font-medium text-secondary">
            {typeId ? t("common.edit") : t("workspace_settings.settings.work_item_types.add_type")}
          </h3>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-13 font-medium" htmlFor="work-item-type-name">
                {t("workspace_settings.settings.work_item_types.form.name")}
              </label>
              <Controller
                control={control}
                name="name"
                rules={{ required: t("title_is_required") }}
                render={({ field: { value, onChange } }) => (
                  <Field name="input" invalid={Boolean(errors.name)}>
                    <InputGroup size="2xl">
                      <Input
                        id="work-item-type-name"
                        size="2xl"
                        type="text"
                        value={value ?? ""}
                        onChange={onChange}
                        placeholder={t("workspace_settings.settings.work_item_types.form.name")}
                        aria-label={t("workspace_settings.settings.work_item_types.form.name")}
                      />
                    </InputGroup>
                  </Field>
                )}
              />
              {errors.name && <span className="text-11 text-danger-primary">{errors.name.message}</span>}
            </div>
            <div className="space-y-1">
              <label className="text-13 font-medium" htmlFor="work-item-type-description">
                {t("workspace_settings.settings.work_item_types.form.description")}
              </label>
              <Controller
                control={control}
                name="description"
                render={({ field: { value, onChange } }) => (
                  <TextArea
                    id="work-item-type-description"
                    value={value ?? ""}
                    onChange={onChange}
                    hasError={Boolean(errors.description)}
                    placeholder={t("workspace_settings.settings.work_item_types.form.description")}
                    className="min-h-24 w-full resize-none text-14"
                  />
                )}
              />
            </div>
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <span className="text-13 font-medium">
                  {t("workspace_settings.settings.work_item_types.form.workflow")}
                </span>
                <Controller
                  control={control}
                  name="workflow"
                  render={({ field: { value, onChange } }) => (
                    <CustomSelect
                      value={value}
                      onChange={onChange}
                      input
                      className="w-56"
                      label={
                        <span className="truncate text-13">
                          {workflows?.find((workflow) => workflow.id === value)?.name ?? "No workflow"}
                        </span>
                      }
                    >
                      <CustomSelect.Option value={null}>No workflow</CustomSelect.Option>
                      {workflows?.map((workflow) => (
                        <CustomSelect.Option key={workflow.id} value={workflow.id}>
                          {workflow.name}
                        </CustomSelect.Option>
                      ))}
                    </CustomSelect>
                  )}
                />
              </div>
              <div className="flex items-center gap-2">
                <span className="text-13 font-medium">
                  {t("workspace_settings.settings.work_item_types.form.active")}
                </span>
                <Controller
                  control={control}
                  name="is_active"
                  render={({ field: { value, onChange } }) => (
                    <Switch
                      size="sm"
                      checked={value ?? false}
                      onCheckedChange={onChange}
                      aria-label={t("workspace_settings.settings.work_item_types.form.active")}
                    />
                  )}
                />
              </div>
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
