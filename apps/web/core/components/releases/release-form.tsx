import { useEffect, useState } from "react";
import { Controller, useForm } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { RELEASE_STATUS_CONFIG, RELEASE_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IRelease, TReleaseStatus } from "@plane/types";
import { CustomSelect } from "@plane/ui";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// services
import { WorkspaceService } from "@/services/workspace.service";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  handleFormSubmit: (values: Partial<IRelease>) => Promise<void>;
  handleClose: () => void;
  status: boolean;
  workspaceSlug: string;
  data?: IRelease;
};

const defaultValues: Partial<IRelease> = {
  name: "",
  version: "",
  status: "draft",
  target_date: null,
  description_html: "<p></p>",
};

const workspaceService = new WorkspaceService();

export function ReleaseForm(props: Props) {
  const { handleFormSubmit, handleClose, status, workspaceSlug, data } = props;
  const { t } = useTranslation();
  const { getWorkspaceBySlug } = useWorkspace();
  const [stableDescriptionHtml] = useState(data?.description_html ?? "<p></p>");
  const {
    formState: { errors, isSubmitting },
    handleSubmit,
    control,
    reset,
  } = useForm<IRelease>({
    defaultValues: {
      ...defaultValues,
      ...data,
      version: data?.version ?? "",
      target_date: data?.target_date ?? null,
      description_html: data?.description_html ?? "<p></p>",
    },
  });

  useEffect(() => {
    reset({
      ...defaultValues,
      ...data,
      version: data?.version ?? "",
      target_date: data?.target_date ?? null,
      description_html: data?.description_html ?? "<p></p>",
    });
  }, [data, reset]);

  const handleCreateUpdateRelease = async (formData: Partial<IRelease>) => {
    await handleFormSubmit({
      ...formData,
      version: formData.version?.trim() ? formData.version.trim() : null,
      target_date: formData.target_date || null,
      description_html: formData.description_html ?? "<p></p>",
    });
  };

  return (
    <form onSubmit={handleSubmit(handleCreateUpdateRelease)}>
      <div className="space-y-5 p-5">
        <div>
          <Controller
            control={control}
            name="name"
            rules={{
              required: t("title_is_required"),
              maxLength: { value: 255, message: t("title_should_be_less_than_255_characters") },
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
                    placeholder={t("release.fields.name")}
                  />
                </InputGroup>
              </Field>
            )}
          />
          <span className="text-11 text-danger-primary">{errors?.name?.message}</span>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("release.fields.version")}</label>
            <Controller
              control={control}
              name="version"
              rules={{ maxLength: { value: 100, message: t("release.errors.version_too_long") } }}
              render={({ field: { value, onChange } }) => (
                <Input
                  size="2xl"
                  id="version"
                  name="version"
                  type="text"
                  value={value ?? ""}
                  onChange={onChange}
                  placeholder="v1.0.0"
                />
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.version?.message}</span>
          </div>
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("release.fields.status")}</label>
            <Controller
              control={control}
              name="status"
              render={({ field: { value, onChange } }) => (
                <CustomSelect
                  value={value}
                  onChange={(next: TReleaseStatus) => onChange(next)}
                  label={t(RELEASE_STATUS_CONFIG[value as TReleaseStatus].label_key)}
                >
                  {RELEASE_STATUSES.map((releaseStatus) => (
                    <CustomSelect.Option key={releaseStatus} value={releaseStatus}>
                      {t(RELEASE_STATUS_CONFIG[releaseStatus].label_key)}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
              )}
            />
          </div>
        </div>

        <div>
          <label className="mb-1 block text-12 text-secondary">{t("release.fields.target_date")}</label>
          <Controller
            control={control}
            name="target_date"
            render={({ field: { value, onChange } }) => (
              <Input
                size="2xl"
                id="target_date"
                name="target_date"
                type="date"
                value={value ?? ""}
                onChange={(event) => onChange(event.target.value || null)}
              />
            )}
          />
        </div>

        <div>
          <label className="mb-1 block text-12 text-secondary">{t("release.fields.description")}</label>
          <Controller
            name="description_html"
            control={control}
            render={({ field: { onChange } }) => (
              <RichTextEditor
                editable
                key={data?.id ?? "release-create"}
                id="release-description-editor"
                initialValue={stableDescriptionHtml}
                value={stableDescriptionHtml}
                workspaceSlug={workspaceSlug}
                workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
                onChange={(_descriptionJson: object, descriptionHtml: string) => onChange(descriptionHtml)}
                placeholder={t("release.fields.description")}
                searchMentionCallback={async (payload) => await workspaceService.searchEntity(workspaceSlug, payload)}
                containerClassName="min-h-24 rounded-md border border-subtle"
                uploadFile={async () => {
                  throw new Error("File upload is disabled for release descriptions.");
                }}
                duplicateFile={async () => {
                  throw new Error("File upload is disabled for release descriptions.");
                }}
              />
            )}
          />
        </div>
      </div>
      <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
        <Button variant="secondary" size="lg" onClick={handleClose}>
          {t("cancel")}
        </Button>
        <Button variant="primary" size="lg" type="submit" loading={isSubmitting}>
          {status ? t("release.actions.save") : t("release.actions.create")}
        </Button>
      </div>
    </form>
  );
}
