import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import {
  EUserPermissions,
  EUserPermissionsLevel,
  getReleaseControlLink,
  getTestingControlLink,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
// components
import { SessionDetailHeader } from "./session-detail-header";
// hooks
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";

export const SessionDetailBreadcrumbs = observer(function SessionDetailBreadcrumbs() {
  const { workspaceSlug, sessionId } = useParams();
  const { t } = useTranslation();
  const { getSessionDetailById } = useReview();
  const { allowPermissions } = useUserPermissions();

  const slug = workspaceSlug?.toString() ?? "";
  const id = sessionId?.toString() ?? "";
  const session = id ? getSessionDetailById(id) : null;

  if (!session) return null;

  const canManage =
    session.board_type === "rcb"
      ? allowPermissions([EUserPermissions.ADMIN, EUserPermissions.MEMBER], EUserPermissionsLevel.WORKSPACE)
      : allowPermissions(
          [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
          EUserPermissionsLevel.PROJECT,
          slug,
          session.project_id ?? ""
        );
  const boardLabel = t(session.board_type === "rcb" ? "review.board_values.rcb" : "review.board_values.tcb");
  const boardHref =
    session.board_type === "rcb" ? getReleaseControlLink(slug) : getTestingControlLink(slug, session.project_id ?? "");

  return (
    <SessionDetailHeader
      workspaceSlug={slug}
      session={session}
      boardLabel={boardLabel}
      boardHref={boardHref}
      canManage={canManage}
    />
  );
});
