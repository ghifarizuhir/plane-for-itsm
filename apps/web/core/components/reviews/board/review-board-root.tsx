import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TReviewBoardType } from "@plane/types";
import { cn } from "@plane/utils";
// components
import { ReviewQueueBoard } from "./review-queue-board";
import { ReviewSessionsBoard } from "./review-sessions-board";

type Props = {
  workspaceSlug: string;
  boardType: TReviewBoardType;
  projectId?: string;
};

export const ReviewBoardRoot = observer(function ReviewBoardRoot({ workspaceSlug, boardType, projectId }: Props) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"queue" | "sessions">("queue");

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <div className="flex items-center gap-4 border-b border-subtle px-4 pt-2 text-13">
        {(["queue", "sessions"] as const).map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => setTab(key)}
            className={cn(
              "border-b-2 pb-2 font-medium",
              tab === key
                ? "border-accent-primary text-primary"
                : "border-transparent text-secondary hover:text-primary"
            )}
          >
            {key === "queue" ? t("review.queue_tab") : t("review.sessions_tab")}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        {tab === "queue" ? (
          <ReviewQueueBoard workspaceSlug={workspaceSlug} boardType={boardType} projectId={projectId} />
        ) : (
          <ReviewSessionsBoard workspaceSlug={workspaceSlug} boardType={boardType} projectId={projectId} />
        )}
      </div>
    </div>
  );
});
