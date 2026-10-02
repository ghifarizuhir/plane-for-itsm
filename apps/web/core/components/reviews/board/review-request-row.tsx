import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { getReleaseLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { renderFormattedDate } from "@plane/utils";
// components
import { ReviewRequestStatusPill } from "../request-status-pill";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  requestId: string;
};

export const ReviewRequestRow = observer(function ReviewRequestRow({ workspaceSlug, requestId }: Props) {
  const { t } = useTranslation();
  const { getRequestById } = useReview();

  const request = getRequestById(requestId);
  if (!request) return null;

  const issue = request.subject.issue;
  const release = request.subject.release;
  const subjectLabel =
    request.board_type === "tcb" ? [issue?.identifier, issue?.name].filter(Boolean).join(" ") : (release?.name ?? "");
  const href =
    request.board_type === "tcb" && issue?.identifier
      ? `/${workspaceSlug}/browse/${issue.identifier}`
      : request.release_id
        ? getReleaseLink(workspaceSlug, request.release_id)
        : null;

  const body = (
    <>
      <span className="w-16 shrink-0 font-code text-11 text-tertiary">
        {request.board_type === "tcb" ? (issue?.identifier ?? "—") : `REL-${release?.sequence_id ?? "—"}`}
      </span>
      <span className="min-w-0 flex-1 truncate text-13 font-medium text-primary">
        {subjectLabel || t("review.queue.subject")}
      </span>
      <span className="flex w-28 justify-center">
        <ReviewRequestStatusPill status={request.status} />
      </span>
      <span className="w-40 truncate text-12 text-secondary">{request.session?.title ?? "—"}</span>
      <span className="w-28 text-right text-12 text-secondary">{renderFormattedDate(request.submitted_at)}</span>
    </>
  );

  if (!href) {
    return <div className="flex items-center gap-3 border-b border-subtle px-4 py-3">{body}</div>;
  }

  return (
    <Link href={href} className="group flex items-center gap-3 border-b border-subtle px-4 py-3 hover:bg-layer-1">
      {body}
    </Link>
  );
});
