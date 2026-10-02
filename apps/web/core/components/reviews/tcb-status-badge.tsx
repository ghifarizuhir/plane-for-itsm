import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TReviewRequestStatus } from "@plane/types";
// helpers
import { cn } from "@plane/utils";
// hooks
import { useReview } from "@/hooks/store/use-review";

const TCB_BADGE_KEYS: Record<TReviewRequestStatus, string> = {
  pending: "review.tcb.badge_pending",
  scheduled: "review.tcb.badge_scheduled",
  decided: "review.tcb.badge_decided",
  withdrawn: "review.tcb.badge_withdrawn",
};

const TCB_BADGE_CLASSES: Record<TReviewRequestStatus, string> = {
  pending: "bg-layer-2 text-secondary",
  scheduled: "bg-warning-subtle text-warning-primary",
  decided: "bg-success-subtle text-success-primary",
  withdrawn: "bg-layer-2 text-tertiary",
};

type Props = {
  changeIssueId: string;
  projectId: string;
  workspaceSlug: string;
  className?: string;
};

export const TcbStatusBadge = observer(function TcbStatusBadge({
  changeIssueId,
  projectId,
  workspaceSlug,
  className,
}: Props) {
  const { t } = useTranslation();
  const { getRequestIds, getLatestRequestForChange, fetchRequests } = useReview();

  const fetched = getRequestIds({ board_type: "tcb", project_id: projectId });

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: "tcb", project_id: projectId });
  }, [fetched, fetchRequests, projectId, workspaceSlug]);

  const request = getLatestRequestForChange(changeIssueId);
  if (!request) return null;

  return (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", TCB_BADGE_CLASSES[request.status], className)}>
      {t(TCB_BADGE_KEYS[request.status])}
    </span>
  );
});
