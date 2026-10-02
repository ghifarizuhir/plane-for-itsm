import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TReviewBoardType } from "@plane/types";
// components
import { ReviewSessionRow } from "./review-session-row";
import { SessionFormModal } from "./session-form-modal";
// helpers
import { sortSessionsByScheduledAt } from "@/services/review.helpers";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
};

export const ReviewSessionsBoard = observer(function ReviewSessionsBoard({
  workspaceSlug,
  boardType,
  projectId,
}: Props) {
  const { t } = useTranslation();
  const { getSessions, fetchSessions, errorMap } = useReview();
  const { allowPermissions } = useUserPermissions();
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  const sessions = getSessions({ board_type: boardType, project_id: projectId });
  const hasError = errorMap[`sessions:${boardType}:${projectId ?? "ws"}:all`] ?? false;
  const canCreate =
    boardType === "rcb"
      ? allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          workspaceSlug,
          projectId ?? ""
        );

  useEffect(() => {
    if (sessions !== null) return;
    void fetchSessions(workspaceSlug, { board_type: boardType, project_id: projectId });
  }, [sessions, fetchSessions, boardType, projectId, workspaceSlug]);

  const ordered = sessions ? sortSessionsByScheduledAt(sessions) : null;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-end border-b border-subtle px-4 py-2">
        {canCreate && (
          <Button variant="primary" size="sm" onClick={() => setIsCreateOpen(true)}>
            {t("review.sessions.add")}
          </Button>
        )}
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto">
        <div className="sticky top-0 z-10 flex items-center gap-3 border-b border-subtle bg-surface-1 px-4 py-2 text-11 font-medium text-tertiary">
          <span className="flex-1">{t("review.sessions.title_label")}</span>
          <span className="w-28 text-center">{t("review.sessions.status")}</span>
          <span className="w-40">{t("review.sessions.scheduled_at")}</span>
          <span className="w-24 text-center">{t("review.sessions.items_count")}</span>
          <span className="w-24 text-center">{t("review.sessions.participants_count")}</span>
        </div>
        {hasError ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.load_error.title")}</p>
            <p className="text-xs text-secondary">{t("review.load_error.description")}</p>
            <button
              type="button"
              className="text-12 text-accent-primary underline"
              onClick={() => void fetchSessions(workspaceSlug, { board_type: boardType, project_id: projectId })}
            >
              {t("review.load_error.retry")}
            </button>
          </div>
        ) : ordered === null ? (
          <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>
        ) : ordered.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.empty_sessions.title")}</p>
            <p className="text-xs text-secondary">{t("review.empty_sessions.description")}</p>
          </div>
        ) : (
          ordered.map((session) => (
            <ReviewSessionRow key={session.id} workspaceSlug={workspaceSlug} sessionId={session.id} />
          ))
        )}
      </div>
      <SessionFormModal
        isOpen={isCreateOpen}
        onClose={() => setIsCreateOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType={boardType}
        projectId={projectId}
        onSaved={async () => {
          await fetchSessions(workspaceSlug, { board_type: boardType, project_id: projectId });
        }}
      />
    </div>
  );
});
