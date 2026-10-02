import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail } from "@plane/types";
import { AlertModalCore, Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { SessionFormModal } from "../board/session-form-modal";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  boardLabel: string;
  boardHref: string;
  canManage: boolean;
};

export const SessionDetailHeader = observer(function SessionDetailHeader({
  workspaceSlug,
  session,
  boardLabel,
  boardHref,
  canManage,
}: Props) {
  const { t } = useTranslation();
  const { completeSession, cancelSession, fetchSessionDetail } = useReview();
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [confirmAction, setConfirmAction] = useState<"complete" | "cancel" | null>(null);

  const handleConfirm = async () => {
    if (!confirmAction) return;
    try {
      if (confirmAction === "complete") {
        await completeSession(workspaceSlug, session.id);
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.completed") });
      } else {
        await cancelSession(workspaceSlug, session.id);
        setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.cancelled") });
      }
      await fetchSessionDetail(workspaceSlug, session.id);
      setConfirmAction(null);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message:
          apiError?.detail ??
          apiError?.error ??
          (confirmAction === "complete" ? t("review.sessions.complete_failed") : t("review.sessions.cancel_failed")),
      });
    }
  };

  return (
    <>
      <Header>
        <Header.LeftItem>
          <Breadcrumbs>
            <Breadcrumbs.Item
              component={<BreadcrumbLink label={boardLabel} href={boardHref} isLast={false} />}
              isLast={false}
            />
            <Breadcrumbs.Item component={<BreadcrumbLink label={session.title} href="#" isLast />} isLast />
          </Breadcrumbs>
        </Header.LeftItem>
        <Header.RightItem>
          {canManage && (
            <div className="flex items-center gap-2">
              <Button variant="secondary" size="lg" onClick={() => setIsEditOpen(true)}>
                {t("review.sessions.edit")}
              </Button>
              {session.status === "scheduled" && (
                <>
                  <Button variant="secondary" size="lg" onClick={() => setConfirmAction("complete")}>
                    {t("review.sessions.complete")}
                  </Button>
                  <Button
                    variant="secondary"
                    size="lg"
                    className="text-danger-primary"
                    onClick={() => setConfirmAction("cancel")}
                  >
                    {t("review.sessions.cancel")}
                  </Button>
                </>
              )}
            </div>
          )}
        </Header.RightItem>
      </Header>
      <SessionFormModal
        isOpen={isEditOpen}
        onClose={() => setIsEditOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType={session.board_type}
        projectId={session.project_id ?? undefined}
        data={session}
        onSaved={async () => {
          await fetchSessionDetail(workspaceSlug, session.id);
        }}
      />
      <AlertModalCore
        isOpen={confirmAction !== null}
        handleClose={() => setConfirmAction(null)}
        handleSubmit={() => void handleConfirm()}
        isSubmitting={false}
        title={confirmAction === "complete" ? t("review.sessions.complete") : t("review.sessions.cancel")}
        content={
          confirmAction === "complete" ? t("review.sessions.complete_confirm") : t("review.sessions.cancel_confirm")
        }
      />
    </>
  );
});
