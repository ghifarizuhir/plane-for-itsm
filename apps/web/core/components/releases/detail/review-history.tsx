import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// components
import { ReviewOutcomePill, ReviewRequestStatusPill, SubmitReviewModal } from "@/components/reviews";
// hooks
import { useRelease } from "@/hooks/store/use-release";
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  releaseId: string;
  canWrite: boolean;
};

export const ReleaseReviewHistory = observer(function ReleaseReviewHistory({
  workspaceSlug,
  releaseId,
  canWrite,
}: Props) {
  const { t } = useTranslation();
  const { updateRelease } = useRelease();
  const {
    fetchRequests,
    fetchRequestDetail,
    getRequestIds,
    getRequestDetailById,
    getRequestsForRelease,
    getLatestRequestForRelease,
    withdrawRequest,
  } = useReview();
  const [isSubmitOpen, setIsSubmitOpen] = useState(false);

  const fetched = getRequestIds({ board_type: "rcb", release_id: releaseId });

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: "rcb", release_id: releaseId });
  }, [fetched, fetchRequests, releaseId, workspaceSlug]);

  // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; sorting a fresh filtered copy
  const requests = getRequestsForRelease(releaseId).sort((a, b) => b.submitted_at.localeCompare(a.submitted_at));
  const latest = getLatestRequestForRelease(releaseId);
  const hasActive = latest !== null && (latest.status === "pending" || latest.status === "scheduled");

  // Decided requests need their cycle history to show the latest outcome.
  useEffect(() => {
    requests
      .filter((request) => request.status === "decided" && !getRequestDetailById(request.id))
      .forEach((request) => {
        void fetchRequestDetail(workspaceSlug, request.id);
      });
  }, [requests, fetchRequestDetail, getRequestDetailById, workspaceSlug]);

  const handleWithdraw = async () => {
    if (!latest) return;
    try {
      await withdrawRequest(workspaceSlug, latest.id);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("release.review.withdrawn") });
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.review.withdraw_failed"),
      });
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("release.fields.review_history")}</h3>
        {canWrite && !hasActive && (
          <Button variant="secondary" size="sm" onClick={() => setIsSubmitOpen(true)}>
            {requests.length === 0 ? t("release.review.submit") : t("release.review.resubmit")}
          </Button>
        )}
        {canWrite && hasActive && (
          <Button variant="secondary" size="sm" onClick={() => void handleWithdraw()}>
            {t("release.review.withdraw")}
          </Button>
        )}
      </div>
      {requests.length === 0 ? (
        <p className="text-12 text-secondary">{t("release.review.empty_history")}</p>
      ) : (
        <ul className="space-y-2">
          {requests.map((request) => {
            const detail = getRequestDetailById(request.id);
            const latestOutcome = detail?.history
              .filter((entry) => entry.outcome)
              // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; sorting a fresh filtered copy
              .sort((a, b) => (b.decided_at ?? "").localeCompare(a.decided_at ?? ""))[0]?.outcome;
            return (
              <li
                key={request.id}
                className="flex flex-col gap-1 rounded-md border border-subtle px-3 py-2 text-12 text-secondary"
              >
                <div className="flex items-center gap-2">
                  <ReviewRequestStatusPill status={request.status} />
                  {latestOutcome && <ReviewOutcomePill outcome={latestOutcome} />}
                  <span className="ml-auto text-11 text-tertiary">
                    {renderFormattedDate(request.submitted_at)} {renderFormattedTime(request.submitted_at)}
                  </span>
                </div>
                {request.submission_note && <p className="text-12 text-secondary">{request.submission_note}</p>}
              </li>
            );
          })}
        </ul>
      )}
      <SubmitReviewModal
        isOpen={isSubmitOpen}
        onClose={() => setIsSubmitOpen(false)}
        workspaceSlug={workspaceSlug}
        boardType="rcb"
        subjectId={releaseId}
        title={t("release.review.submit")}
        submitLabel={t("release.review.submit")}
        onSubmitted={async () => {
          await updateRelease(workspaceSlug, releaseId, { status: "in_review" });
        }}
      />
    </section>
  );
});
