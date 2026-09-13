/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { Controller, useForm, type Control } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IService } from "@plane/types";
// ui
import { CustomSelect, Input as UIKitInput, TextArea } from "@plane/ui";
// components
import { MemberDropdown } from "@/components/dropdowns/member/dropdown";

type Props = {
  handleFormSubmit: (values: Partial<IService>) => Promise<void>;
  handleClose: () => void;
  status: boolean;
  projectId: string;
  data?: IService;
};

const SERVICE_STATUS_OPTIONS: IService["status"][] = ["active", "planned", "maintenance", "deprecated", "retired"];

const SERVICE_CRITICALITY_OPTIONS: IService["criticality"][] = ["critical", "high", "medium", "low"];

const SERVICE_TYPE_OPTIONS: IService["type"][] = ["internal", "external", "infrastructure", "third_party"];

const formatOptionLabel = (value: string) => value.replace(/_/g, " ");

type TServiceOptionSelectProps = {
  control: Control<IService, any>;
  name: "status" | "criticality" | "type";
  label: string;
};

function ServiceOptionSelect(props: TServiceOptionSelectProps) {
  const { control, name, label } = props;
  const options =
    name === "status"
      ? SERVICE_STATUS_OPTIONS
      : name === "criticality"
        ? SERVICE_CRITICALITY_OPTIONS
        : SERVICE_TYPE_OPTIONS;
  return (
    <Controller
      control={control}
      rules={{ required: true }}
      name={name}
      render={({ field: { value, onChange } }) => (
        <CustomSelect
          value={value}
          label={
            <span className="flex items-center gap-2 py-0.5 text-11 capitalize">
              {value ? formatOptionLabel(value) : <span className="text-secondary normal-case">{label}</span>}
            </span>
          }
          onChange={onChange}
          noChevron
        >
          {options.map((option) => (
            <CustomSelect.Option key={option} value={option}>
              <span className="capitalize">{formatOptionLabel(option)}</span>
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      )}
    />
  );
}

const defaultValues: Partial<IService> = {
  name: "",
  description: "",
  status: "planned",
  criticality: "medium",
  type: "internal",
  owner_id: null,
  repository_url: null,
  documentation_url: null,
};

export function ServiceForm(props: Props) {
  const { handleFormSubmit, handleClose, status, projectId, data } = props;
  // form info
  const {
    formState: { errors, isSubmitting },
    handleSubmit,
    control,
    reset,
  } = useForm<IService>({
    defaultValues: {
      name: data?.name || "",
      description: data?.description || "",
      status: data?.status || "planned",
      criticality: data?.criticality || "medium",
      type: data?.type || "internal",
      owner_id: data?.owner_id || null,
      repository_url: data?.repository_url || "",
      documentation_url: data?.documentation_url || "",
    },
  });

  const { t } = useTranslation();

  const validateUrl = (value: string | null | undefined) => {
    if (!value) return true;
    const canParse = (URL as unknown as { canParse?: (url: string) => boolean }).canParse;
    if (typeof canParse === "function") {
      return canParse(value) ? true : t("common.url_is_invalid");
    }
    try {
      void new URL(value);
      return true;
    } catch {
      return t("common.url_is_invalid");
    }
  };

  const handleCreateUpdateService = async (formData: Partial<IService>) => {
    const description = formData.description ?? "";
    try {
      await handleFormSubmit({
        ...formData,
        description_html: description ? `<p>${description}</p>` : "",
        repository_url: formData.repository_url || null,
        documentation_url: formData.documentation_url || null,
      });
      reset({
        ...defaultValues,
      });
    } catch {
      // The modal already toasted the error; keep user input so they can retry.
    }
  };

  useEffect(() => {
    reset({
      ...defaultValues,
      ...data,
    });
  }, [data, reset]);

  return (
    <form onSubmit={handleSubmit(handleCreateUpdateService)}>
      <div className="space-y-5 p-5">
        <div className="flex items-center gap-x-3">
          <h3 className="text-18 font-medium text-secondary">{status ? t("common.update") : t("service.create")}</h3>
        </div>
        <div className="space-y-3">
          <div className="space-y-1">
            <Controller
              control={control}
              name="name"
              rules={{
                required: t("title_is_required"),
                maxLength: {
                  value: 255,
                  message: t("title_should_be_less_than_255_characters"),
                },
              }}
              render={({ field: { value, onChange } }) => (
                <Field name="name" invalid={Boolean(errors?.name)}>
                  <InputGroup size="2xl">
                    <Input
                      size="2xl"
                      id="name"
                      name="name"
                      type="text"
                      value={value}
                      onChange={onChange}
                      placeholder={t("service.fields.name")}
                    />
                  </InputGroup>
                </Field>
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.name?.message}</span>
          </div>
          <div>
            <Controller
              name="description"
              control={control}
              render={({ field: { value, onChange } }) => (
                <TextArea
                  id="description"
                  name="description"
                  value={value}
                  onChange={onChange}
                  placeholder={t("service.fields.description")}
                  className="min-h-24 w-full resize-none text-14"
                  hasError={Boolean(errors?.description)}
                />
              )}
            />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <div className="h-7">
              <ServiceOptionSelect control={control} name="status" label={t("service.fields.status")} />
            </div>
            <div className="h-7">
              <ServiceOptionSelect control={control} name="criticality" label={t("service.fields.criticality")} />
            </div>
            <div className="h-7">
              <ServiceOptionSelect control={control} name="type" label={t("service.fields.type")} />
            </div>
            <Controller
              control={control}
              name="owner_id"
              render={({ field: { value, onChange } }) => (
                <div className="h-7">
                  <MemberDropdown
                    value={value}
                    onChange={onChange}
                    projectId={projectId}
                    multiple={false}
                    buttonVariant="border-with-text"
                    placeholder={t("service.fields.owner")}
                  />
                </div>
              )}
            />
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <label htmlFor="repository_url" className="text-11 font-medium text-secondary">
                {t("service.fields.repository")}
              </label>
              <Controller
                name="repository_url"
                control={control}
                rules={{ validate: validateUrl }}
                render={({ field: { value, onChange } }) => (
                  <UIKitInput
                    id="repository_url"
                    name="repository_url"
                    type="url"
                    value={value ?? ""}
                    onChange={onChange}
                    placeholder="https://"
                    className="w-full"
                  />
                )}
              />
              <span className="text-11 text-danger-primary">{errors?.repository_url?.message}</span>
            </div>
            <div className="space-y-1">
              <label htmlFor="documentation_url" className="text-11 font-medium text-secondary">
                {t("service.fields.documentation")}
              </label>
              <Controller
                name="documentation_url"
                control={control}
                rules={{ validate: validateUrl }}
                render={({ field: { value, onChange } }) => (
                  <UIKitInput
                    id="documentation_url"
                    name="documentation_url"
                    type="url"
                    value={value ?? ""}
                    onChange={onChange}
                    placeholder="https://"
                    className="w-full"
                  />
                )}
              />
              <span className="text-11 text-danger-primary">{errors?.documentation_url?.message}</span>
            </div>
          </div>
        </div>
      </div>
      <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
        <Button variant="secondary" size="lg" onClick={handleClose}>
          {t("cancel")}
        </Button>
        <Button variant="primary" size="lg" type="submit" loading={isSubmitting}>
          {status
            ? isSubmitting
              ? t("updating")
              : t("common.update")
            : isSubmitting
              ? t("creating")
              : t("service.create")}
        </Button>
      </div>
    </form>
  );
}
