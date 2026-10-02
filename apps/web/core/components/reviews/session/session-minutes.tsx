import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail } from "@plane/types";
import { TextArea } from "@plane/ui";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionMinutes = observer(function SessionMinutes({ workspaceSlug, session, canManage }: Props) {
  const { t } = useTranslation();
  const { updateSession } = useReview();
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [value, setValue] = useState(session.minutes ?? "");

  useEffect(() => {
    setValue(session.minutes ?? "");
  }, [session.id, session.minutes]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await updateSession(workspaceSlug, session.id, { minutes: value });
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      setIsEditing(false);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.sessions.update_failed"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("review.sessions.minutes")}</h3>
        {canManage && session.status !== "cancelled" && !isEditing && (
          <Button variant="secondary" size="sm" onClick={() => setIsEditing(true)}>
            {t("edit")}
          </Button>
        )}
      </div>
      {isEditing ? (
        <div className="space-y-2">
          <TextArea
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder={t("review.sessions.minutes_placeholder")}
            rows={6}
          />
          <div className="flex items-center justify-end gap-2">
            <Button variant="secondary" size="sm" onClick={() => setIsEditing(false)}>
              {t("cancel")}
            </Button>
            <Button variant="primary" size="sm" loading={isSaving} onClick={() => void handleSave()}>
              {t("save")}
            </Button>
          </div>
        </div>
      ) : session.minutes ? (
        <p className="rounded-md border border-subtle px-3 py-2 text-12 whitespace-pre-wrap text-secondary">
          {session.minutes}
        </p>
      ) : (
        <p className="text-12 text-secondary">{t("review.sessions.minutes_placeholder")}</p>
      )}
    </section>
  );
});
