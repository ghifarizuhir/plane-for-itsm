import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewRequest, TReviewBoardType, TReviewRequestCreatePayload } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  boardType: TReviewBoardType;
  subjectId: string;
  title: string;
  submitLabel: string;
  onSubmitted?: (request: IReviewRequest) => void | Promise<void>;
};

export const SubmitReviewModal = observer(function SubmitReviewModal({
  isOpen,
  onClose,
  workspaceSlug,
  boardType,
  subjectId,
  title,
  submitLabel,
  onSubmitted,
}: Props) {
  const { t } = useTranslation();
  const { submitRequest } = useReview();
  const [note, setNote] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async () => {
    setIsSubmitting(true);
    try {
      const payload: TReviewRequestCreatePayload =
        boardType === "rcb"
          ? { board_type: "rcb", release_id: subjectId, submission_note: note.trim() }
          : { board_type: "tcb", change_issue_id: subjectId, submission_note: note.trim() };
      const request = await submitRequest(workspaceSlug, payload);
      if (onSubmitted) await onSubmitted(request);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.request.submitted") });
      setNote("");
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.request.submit_failed"),
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{title}</h3>
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("review.request.submission_note")}</label>
          <TextArea
            value={note}
            onChange={(event) => setNote(event.target.value)}
            placeholder={t("review.request.note_placeholder")}
            rows={4}
          />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isSubmitting} onClick={() => void handleSubmit()}>
            {submitLabel}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
