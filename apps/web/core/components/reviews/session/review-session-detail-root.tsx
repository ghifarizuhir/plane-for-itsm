import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// components
import { ReviewSessionStatusPill } from "../session-status-pill";
import { SessionAgenda } from "./session-agenda";
import { SessionMinutes } from "./session-minutes";
import { SessionParticipants } from "./session-participants";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  sessionId: string;
};

export const ReviewSessionDetailRoot = observer(function ReviewSessionDetailRoot({ workspaceSlug, sessionId }: Props) {
  const { t } = useTranslation();
  const { getSessionDetailById, fetchSessionDetail } = useReview();
  const { allowPermissions } = useUserPermissions();

  const session = getSessionDetailById(sessionId);

  useEffect(() => {
    if (session) return;
    void fetchSessionDetail(workspaceSlug, sessionId);
  }, [session, fetchSessionDetail, sessionId, workspaceSlug]);

  if (!session) {
    return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
  }

  const canManage =
    session.board_type === "rcb"
      ? allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          workspaceSlug,
          session.project_id ?? ""
        );
  return (
    <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
      <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-16 font-semibold text-primary">{session.title}</h1>
            <ReviewSessionStatusPill status={session.status} />
          </div>
          <p className="mt-1 text-12 text-secondary">
            {renderFormattedDate(session.scheduled_at)} {renderFormattedTime(session.scheduled_at)}
            {session.location ? ` · ${session.location}` : ""}
          </p>
        </div>
        <SessionAgenda workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
        <SessionMinutes workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
      </div>
      <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
        <SessionParticipants workspaceSlug={workspaceSlug} session={session} canManage={canManage} />
      </div>
    </div>
  );
});
