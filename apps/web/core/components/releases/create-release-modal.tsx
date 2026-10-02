import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IRelease } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ReleaseForm } from "./release-form";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  data?: IRelease;
};

export const CreateReleaseModal = observer(function CreateReleaseModal({
  isOpen,
  onClose,
  workspaceSlug,
  data,
}: Props) {
  const { t } = useTranslation();
  const { createRelease, updateRelease } = useRelease();

  const handleFormSubmit = async (payload: Partial<IRelease>) => {
    try {
      if (data) {
        await updateRelease(workspaceSlug, data.id, {
          name: payload.name,
          version: payload.version ?? null,
          description_html: payload.description_html,
          status: payload.status,
          target_date: payload.target_date ?? null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.toast.updated") });
      } else {
        await createRelease(workspaceSlug, {
          name: payload.name ?? "",
          version: payload.version ?? null,
          description_html: payload.description_html ?? "<p></p>",
          status: payload.status,
          target_date: payload.target_date ?? null,
        });
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.toast.created") });
      }
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          apiError?.detail ??
          apiError?.error ??
          (data ? t("release.errors.update_failed") : t("release.errors.create_failed")),
      });
      throw error;
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XL}>
      <ReleaseForm
        handleFormSubmit={handleFormSubmit}
        handleClose={onClose}
        status={!!data}
        workspaceSlug={workspaceSlug}
        data={data}
      />
    </ModalCore>
  );
});
