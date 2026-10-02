import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import {
  REVIEW_ATTENDANCE_LABEL_KEYS,
  REVIEW_ATTENDANCE_VALUES,
  REVIEW_PARTICIPANT_ROLE_LABEL_KEYS,
  REVIEW_PARTICIPANT_ROLES,
} from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewParticipant, IReviewSessionDetail, TReviewAttendance, TReviewParticipantRole } from "@plane/types";
import { CustomSelect } from "@plane/ui";
// components
import { WorkspaceMemberSelect } from "./workspace-member-select";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionParticipants = observer(function SessionParticipants({ workspaceSlug, session, canManage }: Props) {
  const { t } = useTranslation();
  const { addSessionParticipant, updateSessionParticipant, removeSessionParticipant } = useReview();
  const [selectedUserId, setSelectedUserId] = useState("");
  const [isAdding, setIsAdding] = useState(false);

  const handleError = (error: unknown, fallback: string) => {
    const apiError = error as { detail?: string; error?: string };
    setToast({
      type: TOAST_TYPE.ERROR,
      title: "Error!",
      message: apiError?.detail ?? apiError?.error ?? fallback,
    });
  };

  const handleAdd = async () => {
    if (!selectedUserId) return;
    setIsAdding(true);
    try {
      await addSessionParticipant(workspaceSlug, session.id, { user_id: selectedUserId, role: "member" });
      setSelectedUserId("");
    } catch (error) {
      handleError(error, t("review.participants.add_failed"));
    } finally {
      setIsAdding(false);
    }
  };

  const handleUpdate = async (
    participant: IReviewParticipant,
    data: Partial<{ role: TReviewParticipantRole; attendance: TReviewAttendance }>
  ) => {
    try {
      await updateSessionParticipant(workspaceSlug, session.id, participant.id, data);
    } catch (error) {
      handleError(error, t("review.participants.update_failed"));
    }
  };

  const handleRemove = async (participantId: string) => {
    try {
      await removeSessionParticipant(workspaceSlug, session.id, participantId);
    } catch (error) {
      handleError(error, t("review.participants.remove_failed"));
    }
  };

  return (
    <div className="space-y-4 p-6">
      <h3 className="text-13 font-semibold text-primary">{t("review.sessions.participants_count")}</h3>
      {canManage && session.status === "scheduled" && (
        <div className="space-y-2">
          <WorkspaceMemberSelect
            workspaceSlug={workspaceSlug}
            value={selectedUserId}
            onChange={setSelectedUserId}
            excludeUserIds={session.participants.map((participant) => participant.user_id)}
            placeholder={t("review.participants.select_member")}
          />
          <Button
            variant="secondary"
            size="sm"
            className="w-full"
            loading={isAdding}
            disabled={!selectedUserId}
            onClick={() => void handleAdd()}
          >
            {t("review.participants.add")}
          </Button>
        </div>
      )}
      {session.participants.length === 0 ? (
        <p className="text-12 text-secondary">{t("review.participants.empty")}</p>
      ) : (
        <ul className="space-y-3">
          {session.participants.map((participant) => (
            <li key={participant.id} className="space-y-2 rounded-md border border-subtle px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate text-12 font-medium text-primary">
                  {participant.display_name || participant.email || participant.user_id}
                </span>
                {canManage && session.status === "scheduled" && (
                  <button
                    type="button"
                    className="text-11 text-tertiary hover:text-danger-primary"
                    onClick={() => void handleRemove(participant.id)}
                  >
                    {t("review.participants.remove")}
                  </button>
                )}
              </div>
              <div className="grid grid-cols-2 gap-2">
                <CustomSelect
                  value={participant.role}
                  onChange={(value: TReviewParticipantRole) => void handleUpdate(participant, { role: value })}
                  label={t(REVIEW_PARTICIPANT_ROLE_LABEL_KEYS[participant.role])}
                  disabled={!canManage || session.status !== "scheduled"}
                >
                  {REVIEW_PARTICIPANT_ROLES.map((role) => (
                    <CustomSelect.Option key={role} value={role}>
                      {t(REVIEW_PARTICIPANT_ROLE_LABEL_KEYS[role])}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
                <CustomSelect
                  value={participant.attendance}
                  onChange={(value: TReviewAttendance) => void handleUpdate(participant, { attendance: value })}
                  label={t(REVIEW_ATTENDANCE_LABEL_KEYS[participant.attendance])}
                  disabled={!canManage || session.status !== "scheduled"}
                >
                  {REVIEW_ATTENDANCE_VALUES.map((attendance) => (
                    <CustomSelect.Option key={attendance} value={attendance}>
                      {t(REVIEW_ATTENDANCE_LABEL_KEYS[attendance])}
                    </CustomSelect.Option>
                  ))}
                </CustomSelect>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
});
