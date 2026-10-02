import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail, IReviewSessionItem } from "@plane/types";
// components
import { ReviewOutcomePill } from "../outcome-pill";
import { ReviewRequestStatusPill } from "../request-status-pill";
import { AddAgendaItemsModal } from "./add-agenda-items-modal";
import { RecordOutcomeModal } from "./record-outcome-modal";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionAgenda = observer(function SessionAgenda({ workspaceSlug, session, canManage }: Props) {
  const { t } = useTranslation();
  const { removeSessionItem, fetchSessionDetail } = useReview();
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [outcomeItem, setOutcomeItem] = useState<IReviewSessionItem | null>(null);

  const refresh = async () => {
    await fetchSessionDetail(workspaceSlug, session.id);
  };

  const handleRemove = async (itemId: string) => {
    try {
      await removeSessionItem(workspaceSlug, session.id, itemId);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.items.remove_failed"),
      });
    }
  };

  // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; sorting a fresh copied list
  const items = [...session.items].sort((a, b) => a.position - b.position);

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("review.sessions.agenda")}</h3>
        {canManage && session.status === "scheduled" && (
          <Button variant="secondary" size="sm" onClick={() => setIsAddOpen(true)}>
            {t("review.items.add")}
          </Button>
        )}
      </div>
      {items.length === 0 ? (
        <p className="text-12 text-secondary">{t("review.sessions.empty_items")}</p>
      ) : (
        <ul className="space-y-2">
          {items.map((item) => {
            const label = [item.subject.identifier, item.subject.name].filter(Boolean).join(" ");
            const canRemove = !item.outcome || item.outcome === "deferred";
            return (
              <li key={item.id} className="flex items-center gap-3 rounded-md border border-subtle px-3 py-2">
                <span className="w-6 text-center font-code text-11 text-tertiary">{item.position + 1}</span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-13 text-primary">{label}</p>
                  {item.outcome_note && <p className="truncate text-11 text-tertiary">{item.outcome_note}</p>}
                </div>
                <ReviewRequestStatusPill status={item.request_status} />
                {item.outcome && <ReviewOutcomePill outcome={item.outcome} />}
                {canManage && session.status === "scheduled" && (
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      className="text-11 text-accent-primary hover:underline"
                      onClick={() => setOutcomeItem(item)}
                    >
                      {t("review.items.record")}
                    </button>
                    {canRemove && (
                      <button
                        type="button"
                        className="text-11 text-tertiary hover:text-danger-primary"
                        onClick={() => void handleRemove(item.id)}
                      >
                        {t("review.items.remove")}
                      </button>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <AddAgendaItemsModal
        isOpen={isAddOpen}
        onClose={() => setIsAddOpen(false)}
        workspaceSlug={workspaceSlug}
        sessionId={session.id}
        boardType={session.board_type}
        projectId={session.project_id ?? undefined}
        excludeRequestIds={items.map((item) => item.review_request_id)}
        onAdded={refresh}
      />
      <RecordOutcomeModal
        isOpen={outcomeItem !== null}
        onClose={() => setOutcomeItem(null)}
        workspaceSlug={workspaceSlug}
        sessionId={session.id}
        item={outcomeItem}
        onSaved={refresh}
      />
    </section>
  );
});
