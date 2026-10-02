import { useEffect } from "react";
import { Controller, useForm } from "react-hook-form";
import { observer } from "mobx-react";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSession, TReviewBoardType } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// helpers
import { fromDateTimeLocal, toDateTimeLocal } from "@/services/review.helpers";
// hooks
import { useReview } from "@/hooks/store/use-review";

type FormValues = {
  title: string;
  scheduled_at: string;
  location: string;
};

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
  data?: IReviewSession;
  onSaved?: () => void | Promise<void>;
};

export const SessionFormModal = observer(function SessionFormModal({
  isOpen,
  onClose,
  workspaceSlug,
  boardType,
  projectId,
  data,
  onSaved,
}: Props) {
  const { t } = useTranslation();
  const { createSession, updateSession } = useReview();
  const {
    formState: { errors, isSubmitting },
    handleSubmit,
    control,
    reset,
  } = useForm<FormValues>({
    defaultValues: { title: "", scheduled_at: "", location: "" },
  });

  useEffect(() => {
    reset({
      title: data?.title ?? "",
      scheduled_at: toDateTimeLocal(data?.scheduled_at),
      location: data?.location ?? "",
    });
  }, [data, reset, isOpen]);

  const handleFormSubmit = async (values: FormValues) => {
    const scheduledAt = fromDateTimeLocal(values.scheduled_at);
    if (!scheduledAt) return;
    try {
      if (data) {
        await updateSession(workspaceSlug, data.id, {
          title: values.title.trim(),
          scheduled_at: scheduledAt,
          location: values.location.trim() || null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      } else {
        await createSession(workspaceSlug, {
          board_type: boardType,
          project_id: projectId ?? null,
          title: values.title.trim(),
          scheduled_at: scheduledAt,
          location: values.location.trim() || null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.created") });
      }
      if (onSaved) await onSaved();
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          apiError?.detail ??
          apiError?.error ??
          (data ? t("review.sessions.update_failed") : t("review.sessions.create_failed")),
      });
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <form onSubmit={handleSubmit(handleFormSubmit)}>
        <div className="space-y-5 p-5">
          <div>
            <Controller
              control={control}
              name="title"
              rules={{
                required: t("title_is_required"),
                maxLength: { value: 255, message: t("title_should_be_less_than_255_characters") },
              }}
              render={({ field: { value, onChange } }) => (
                <Field name="title" invalid={Boolean(errors?.title)}>
                  <InputGroup size="2xl">
                    <Input
                      size="2xl"
                      id="title"
                      name="title"
                      type="text"
                      value={value}
                      onChange={onChange}
                      placeholder={t("review.sessions.title_label")}
                    />
                  </InputGroup>
                </Field>
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.title?.message}</span>
          </div>
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("review.sessions.scheduled_at")}</label>
            <Controller
              control={control}
              name="scheduled_at"
              rules={{ required: t("review.sessions.scheduled_at") }}
              render={({ field: { value, onChange } }) => (
                <Input
                  size="2xl"
                  id="scheduled_at"
                  name="scheduled_at"
                  type="datetime-local"
                  value={value}
                  onChange={onChange}
                />
              )}
            />
            <span className="text-11 text-danger-primary">{errors?.scheduled_at?.message}</span>
          </div>
          <div>
            <label className="mb-1 block text-12 text-secondary">{t("review.sessions.location")}</label>
            <Controller
              control={control}
              name="location"
              render={({ field: { value, onChange } }) => (
                <Input
                  size="2xl"
                  id="location"
                  name="location"
                  type="text"
                  value={value}
                  onChange={onChange}
                  placeholder={t("review.sessions.location")}
                />
              )}
            />
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" type="submit" loading={isSubmitting}>
            {data ? t("save") : t("review.sessions.add")}
          </Button>
        </div>
      </form>
    </ModalCore>
  );
});
