import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { REVIEW_REQUEST_STATUS_CONFIG, REVIEW_REQUEST_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TReviewBoardType, TReviewRequestStatus } from "@plane/types";
import { CustomSelect } from "@plane/ui";
// components
import { ReviewRequestRow } from "./review-request-row";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
};

export const ReviewQueueBoard = observer(function ReviewQueueBoard({ workspaceSlug, boardType, projectId }: Props) {
  const { t } = useTranslation();
  const { getRequestById, getRequestIds, fetchRequests, errorMap } = useReview();
  const [status, setStatus] = useState("");

  const fetched = getRequestIds({ board_type: boardType, project_id: projectId });
  const hasError = errorMap[`${boardType}:${projectId ?? "ws"}:all:-:-`] ?? false;

  useEffect(() => {
    if (fetched !== null) return;
    void fetchRequests(workspaceSlug, { board_type: boardType, project_id: projectId });
  }, [fetched, fetchRequests, boardType, projectId, workspaceSlug]);

  const requestIds =
    fetched?.filter((requestId) => {
      const request = getRequestById(requestId);
      return Boolean(request) && (!status || request?.status === status);
    }) ?? [];

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-end gap-2 border-b border-subtle px-4 py-2">
        <CustomSelect
          value={status}
          onChange={setStatus}
          label={
            status
              ? t(REVIEW_REQUEST_STATUS_CONFIG[status as TReviewRequestStatus].label_key)
              : t("review.all_statuses")
          }
        >
          <CustomSelect.Option value="">{t("review.all_statuses")}</CustomSelect.Option>
          {REVIEW_REQUEST_STATUSES.map((value) => (
            <CustomSelect.Option key={value} value={value}>
              {t(REVIEW_REQUEST_STATUS_CONFIG[value].label_key)}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto">
        <div className="sticky top-0 z-10 flex items-center gap-3 border-b border-subtle bg-surface-1 px-4 py-2 text-11 font-medium text-tertiary">
          <span className="w-16 shrink-0">&nbsp;</span>
          <span className="flex-1">{t("review.queue.subject")}</span>
          <span className="w-28 text-center">{t("review.queue.status")}</span>
          <span className="w-40">{t("review.queue.session")}</span>
          <span className="w-28 text-right">{t("review.queue.submitted_at")}</span>
        </div>
        {hasError ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.load_error.title")}</p>
            <p className="text-xs text-secondary">{t("review.load_error.description")}</p>
            <button
              type="button"
              className="text-12 text-accent-primary underline"
              onClick={() => void fetchRequests(workspaceSlug, { board_type: boardType, project_id: projectId })}
            >
              {t("review.load_error.retry")}
            </button>
          </div>
        ) : fetched === null ? (
          <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>
        ) : fetched.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.empty_queue.title")}</p>
            <p className="text-xs text-secondary">{t("review.empty_queue.description")}</p>
          </div>
        ) : requestIds.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
            <p className="text-sm font-medium text-primary">{t("review.no_matches.title")}</p>
            <p className="text-xs text-secondary">{t("review.no_matches.description")}</p>
          </div>
        ) : (
          requestIds.map((requestId) => (
            <ReviewRequestRow key={requestId} workspaceSlug={workspaceSlug} requestId={requestId} />
          ))
        )}
      </div>
    </div>
  );
});
