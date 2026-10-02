import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { REVIEW_OUTCOME_CONFIG, REVIEW_OUTCOMES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionItem, TReviewOutcome } from "@plane/types";
import { CustomSelect, EModalPosition, EModalWidth, ModalCore, TextArea } from "@plane/ui";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  sessionId: string;
  item: IReviewSessionItem | null;
  onSaved?: () => void | Promise<void>;
};

export const RecordOutcomeModal = observer(function RecordOutcomeModal({
  isOpen,
  onClose,
  workspaceSlug,
  sessionId,
  item,
  onSaved,
}: Props) {
  const { t } = useTranslation();
  const { updateSessionItem } = useReview();
  const [outcome, setOutcome] = useState<TReviewOutcome>("approved");
  const [note, setNote] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (!item) return;
    setOutcome(item.outcome ?? "approved");
    setNote(item.outcome_note ?? "");
  }, [item]);

  const handleSave = async () => {
    if (!item) return;
    setIsSaving(true);
    try {
      await updateSessionItem(workspaceSlug, sessionId, item.id, { outcome, outcome_note: note.trim() });
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      if (onSaved) await onSaved();
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.items.save_failed"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{t("review.items.record")}</h3>
        {item && (
          <p className="text-12 text-secondary">
            {[item.subject.identifier, item.subject.name].filter(Boolean).join(" ")}
          </p>
        )}
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("review.items.outcome_label")}</label>
          <CustomSelect
            value={outcome}
            onChange={(value: TReviewOutcome) => setOutcome(value)}
            label={t(REVIEW_OUTCOME_CONFIG[outcome].label_key)}
          >
            {REVIEW_OUTCOMES.map((value) => (
              <CustomSelect.Option key={value} value={value}>
                {t(REVIEW_OUTCOME_CONFIG[value].label_key)}
              </CustomSelect.Option>
            ))}
          </CustomSelect>
        </div>
        <div>
          <label className="mb-1 block text-12 text-secondary">{t("review.items.note_label")}</label>
          <TextArea value={note} onChange={(event) => setNote(event.target.value)} rows={3} />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" size="lg" loading={isSaving} onClick={() => void handleSave()}>
            {t("review.items.save")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
