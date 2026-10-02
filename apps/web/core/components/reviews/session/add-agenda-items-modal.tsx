import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TReviewBoardType } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  sessionId: string;
  boardType: TReviewBoardType;
  projectId?: string;
  excludeRequestIds: string[];
  onAdded?: () => void | Promise<void>;
};

export const AddAgendaItemsModal = observer(function AddAgendaItemsModal({
  isOpen,
  onClose,
  workspaceSlug,
  sessionId,
  boardType,
  projectId,
  excludeRequestIds,
  onAdded,
}: Props) {
  const { t } = useTranslation();
  const { getRequestById, getRequestIds, fetchRequests, addSessionItems } = useReview();
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [isSaving, setIsSaving] = useState(false);

  const fetched = getRequestIds({ board_type: boardType, project_id: projectId, status: "pending" });

  useEffect(() => {
    if (!isOpen) return;
    setSelectedIds([]);
    void fetchRequests(workspaceSlug, { board_type: boardType, project_id: projectId, status: "pending" });
  }, [isOpen, fetchRequests, boardType, projectId, workspaceSlug]);

  const candidates = (fetched ?? []).filter((requestId) => !excludeRequestIds.includes(requestId));

  const toggle = (requestId: string) => {
    setSelectedIds((current) =>
      current.includes(requestId) ? current.filter((id) => id !== requestId) : [...current, requestId]
    );
  };

  const handleAdd = async () => {
    if (selectedIds.length === 0) return;
    setIsSaving(true);
    try {
      await addSessionItems(workspaceSlug, sessionId, selectedIds);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.sessions.updated") });
      if (onAdded) await onAdded();
      onClose();
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.items.add_failed"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.LG}>
      <div className="space-y-4 p-5">
        <h3 className="text-13 font-semibold text-primary">{t("review.items.add")}</h3>
        <div className="max-h-80 space-y-1 overflow-y-auto">
          {candidates.length === 0 ? (
            <p className="text-12 text-secondary">{t("review.sessions.empty_items")}</p>
          ) : (
            candidates.map((requestId) => {
              const request = getRequestById(requestId);
              if (!request) return null;
              const label =
                request.board_type === "tcb"
                  ? [request.subject.issue?.identifier, request.subject.issue?.name].filter(Boolean).join(" ")
                  : (request.subject.release?.name ?? "");
              return (
                <button
                  key={requestId}
                  type="button"
                  onClick={() => toggle(requestId)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md border border-subtle px-3 py-2 text-left text-12",
                    selectedIds.includes(requestId) ? "bg-accent-primary/10" : "hover:bg-layer-1"
                  )}
                >
                  <input type="checkbox" checked={selectedIds.includes(requestId)} readOnly />
                  <span className="truncate text-primary">{label}</span>
                </button>
              );
            })
          )}
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            loading={isSaving}
            disabled={selectedIds.length === 0}
            onClick={() => void handleAdd()}
          >
            {t("review.items.add_confirm")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
