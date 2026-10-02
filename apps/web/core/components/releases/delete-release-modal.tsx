import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IRelease } from "@plane/types";
import { AlertModalCore } from "@plane/ui";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  release: IRelease;
};

export const DeleteReleaseModal = observer(function DeleteReleaseModal({
  isOpen,
  onClose,
  workspaceSlug,
  release,
}: Props) {
  const { t } = useTranslation();
  const router = useAppRouter();
  const { deleteRelease } = useRelease();
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    setIsSubmitting(true);
    try {
      await deleteRelease(workspaceSlug, release.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.toast.deleted") });
      onClose();
      router.push(`/${workspaceSlug}/releases`);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.errors.delete_failed"),
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <AlertModalCore
      handleClose={onClose}
      handleSubmit={handleSubmit}
      isSubmitting={isSubmitting}
      isOpen={isOpen}
      title={t("release.delete_modal.title")}
      content={t("release.delete_modal.description")}
    />
  );
});
