import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useRelease } from "@/hooks/store/use-release";
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  releaseId: string;
};

export const SubmitReleaseReviewModal = observer(function SubmitReleaseReviewModal({
  isOpen,
  onClose,
  workspaceSlug,
  releaseId,
}: Props) {
  const { t } = useTranslation();
  const { updateRelease } = useRelease();
  const { submitRequest } = useReview();
  const [note, setNote] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    setIsSubmitting(true);
    try {
      await submitRequest(workspaceSlug, {
        board_type: "rcb",
        release_id: releaseId,
        submission_note: note.trim(),
      });
      // Konvensi: submit menandai release in_review (bukan gate).
      await updateRelease(workspaceSlug, releaseId, { status: "in_review" });
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.review.submitted") });
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.review.submit_failed"),
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{t("release.review.submit")}</h3>
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("release.review.note_label")}</label>
          <TextArea
            value={note}
            onChange={(event) => setNote(event.target.value)}
            placeholder={t("release.review.note_placeholder")}
            rows={4}
          />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isSubmitting} onClick={() => void handleSubmit()}>
            {t("release.review.submit")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
