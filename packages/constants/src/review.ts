import type {
  TReviewAttendance,
  TReviewBoardType,
  TReviewOutcome,
  TReviewParticipantRole,
  TReviewRequestStatus,
  TReviewSessionStatus,
} from "@plane/types";

export type TReviewToneConfig = {
  label_key: string;
  pill: string;
};

export const REVIEW_BOARD_TYPES: TReviewBoardType[] = ["tcb", "rcb"];

export const REVIEW_BOARD_LABEL_KEYS: Record<TReviewBoardType, string> = {
  tcb: "review.board_values.tcb",
  rcb: "review.board_values.rcb",
};

export const REVIEW_REQUEST_STATUSES: TReviewRequestStatus[] = ["pending", "scheduled", "decided", "withdrawn"];

export const REVIEW_REQUEST_STATUS_CONFIG: Record<TReviewRequestStatus, TReviewToneConfig> = {
  pending: { label_key: "review.request_status_values.pending", pill: "bg-layer-2 text-secondary" },
  scheduled: { label_key: "review.request_status_values.scheduled", pill: "bg-warning-subtle text-warning-primary" },
  decided: { label_key: "review.request_status_values.decided", pill: "bg-success-subtle text-success-primary" },
  withdrawn: { label_key: "review.request_status_values.withdrawn", pill: "bg-layer-2 text-tertiary" },
};

export const REVIEW_SESSION_STATUSES: TReviewSessionStatus[] = ["scheduled", "completed", "cancelled"];

export const REVIEW_SESSION_STATUS_CONFIG: Record<TReviewSessionStatus, TReviewToneConfig> = {
  scheduled: { label_key: "review.session_status_values.scheduled", pill: "bg-warning-subtle text-warning-primary" },
  completed: { label_key: "review.session_status_values.completed", pill: "bg-success-subtle text-success-primary" },
  cancelled: { label_key: "review.session_status_values.cancelled", pill: "bg-layer-2 text-tertiary" },
};

export const REVIEW_OUTCOMES: TReviewOutcome[] = ["approved", "rejected", "approved_with_notes", "deferred"];

export const REVIEW_OUTCOME_CONFIG: Record<TReviewOutcome, TReviewToneConfig> = {
  approved: { label_key: "review.outcome_values.approved", pill: "bg-success-subtle text-success-primary" },
  rejected: { label_key: "review.outcome_values.rejected", pill: "bg-danger-subtle text-danger-primary" },
  approved_with_notes: {
    label_key: "review.outcome_values.approved_with_notes",
    pill: "bg-warning-subtle text-warning-primary",
  },
  deferred: { label_key: "review.outcome_values.deferred", pill: "bg-layer-2 text-secondary" },
};

export const REVIEW_PARTICIPANT_ROLES: TReviewParticipantRole[] = ["chair", "secretary", "member"];

export const REVIEW_PARTICIPANT_ROLE_LABEL_KEYS: Record<TReviewParticipantRole, string> = {
  chair: "review.participant_role_values.chair",
  secretary: "review.participant_role_values.secretary",
  member: "review.participant_role_values.member",
};

export const REVIEW_ATTENDANCE_VALUES: TReviewAttendance[] = ["invited", "present", "absent"];

export const REVIEW_ATTENDANCE_LABEL_KEYS: Record<TReviewAttendance, string> = {
  invited: "review.attendance_values.invited",
  present: "review.attendance_values.present",
  absent: "review.attendance_values.absent",
};

export const getReleaseControlLink = (workspaceSlug: string) => `/${workspaceSlug}/release-control`;

export const getTestingControlLink = (workspaceSlug: string, projectId: string) =>
  `/${workspaceSlug}/projects/${projectId}/testing-control`;

export const getReviewSessionLink = (
  workspaceSlug: string,
  sessionId: string,
  board: TReviewBoardType,
  projectId?: string
) =>
  board === "rcb"
    ? `${getReleaseControlLink(workspaceSlug)}/sessions/${sessionId}`
    : `${getTestingControlLink(workspaceSlug, projectId ?? "")}/sessions/${sessionId}`;
