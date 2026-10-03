import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// components
import { SubmitReviewModal } from "./submit-review-modal";
import { TcbStatusBadge } from "./tcb-status-badge";
// hooks
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useReview } from "@/hooks/store/use-review";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";

type Props = {
  workspaceSlug: string;
  projectId: string;
  issueId: string;
};

export const ChangeTcbControl = observer(function ChangeTcbControl({ workspaceSlug, projectId, issueId }: Props) {
  const { t } = useTranslation();
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();
  const { allowPermissions } = useUserPermissions();
  const { getRequestIds, getLatestRequestForChange, fetchRequests, withdrawRequest } = useReview();
  const [isSubmitOpen, setIsSubmitOpen] = useState(false);

  const issue = getIssueById(issueId);

  useEffect(() => {
    if (workItemTypes) return;
    void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
  }, [workItemTypes, fetchWorkItemTypes, workspaceSlug]);

  const fetched = getRequestIds({ board_type: "tcb", project_id: projectId });

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: "tcb", project_id: projectId });
  }, [fetched, fetchRequests, projectId, workspaceSlug]);

  const issueType = workItemTypes?.find((type) => type.id === issue?.type_id);
  if (!issue || issue.archived_at || issueType?.name.toLowerCase() !== "change") return <></>;

  const canWrite = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.PROJECT,
    workspaceSlug,
    projectId
  );
  const latest = getLatestRequestForChange(issueId);
  const hasActive = latest !== null && (latest.status === "pending" || latest.status === "scheduled");

  const handleWithdraw = async () => {
    if (!latest) return;
    try {
      await withdrawRequest(workspaceSlug, latest.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.request.withdrawn") });
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.request.withdraw_failed"),
      });
    }
  };

  return (
    <>
      <TcbStatusBadge changeIssueId={issueId} projectId={projectId} workspaceSlug={workspaceSlug} />
      {canWrite && !hasActive && (
        <Button variant="secondary" size="lg" onClick={() => setIsSubmitOpen(true)}>
          {latest ? t("review.change.resubmit") : t("review.change.submit")}
        </Button>
      )}
      {canWrite && hasActive && (
        <Button variant="secondary" size="lg" onClick={() => void handleWithdraw()}>
          {t("review.change.withdraw")}
        </Button>
      )}
      <SubmitReviewModal
        isOpen={isSubmitOpen}
        onClose={() => setIsSubmitOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType="tcb"
        subjectId={issueId}
        title={t("review.change.submit")}
        submitLabel={t("review.request.submit")}
      />
    </>
  );
});
