import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { getReviewSessionLink } from "@plane/constants";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// components
import { ReviewSessionStatusPill } from "../session-status-pill";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  sessionId: string;
};

export const ReviewSessionRow = observer(function ReviewSessionRow({ workspaceSlug, sessionId }: Props) {
  const { getSessionById } = useReview();

  const session = getSessionById(sessionId);
  if (!session) return null;

  return (
    <Link
      href={getReviewSessionLink(workspaceSlug, session.id, session.board_type, session.project_id ?? undefined)}
      className="group flex items-center gap-3 border-b border-subtle px-4 py-3 hover:bg-layer-1"
    >
      <span className="min-w-0 flex-1 truncate text-13 font-medium text-primary">{session.title}</span>
      <span className="flex w-28 justify-center">
        <ReviewSessionStatusPill status={session.status} />
      </span>
      <span className="w-40 text-12 text-secondary">
        {renderFormattedDate(session.scheduled_at)} {renderFormattedTime(session.scheduled_at)}
      </span>
      <span className="w-24 text-center text-12 text-secondary">{session.counts?.items ?? 0}</span>
      <span className="w-24 text-center text-12 text-secondary">{session.counts?.participants ?? 0}</span>
    </Link>
  );
});
