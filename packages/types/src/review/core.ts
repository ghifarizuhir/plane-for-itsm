export type TReviewBoardType = "tcb" | "rcb";
export type TReviewRequestStatus = "pending" | "scheduled" | "decided" | "withdrawn";
export type TReviewSessionStatus = "scheduled" | "completed" | "cancelled";
export type TReviewOutcome = "approved" | "rejected" | "approved_with_notes" | "deferred";
export type TReviewParticipantRole = "chair" | "secretary" | "member";
export type TReviewAttendance = "invited" | "present" | "absent";
export type TReviewSubjectKind = "change" | "release";

export interface IReviewSubjectIssue {
  id: string | null;
  identifier: string | null;
  name: string | null;
  project_id: string | null;
  project_identifier: string | null;
}

export interface IReviewSubjectRelease {
  id: string | null;
  sequence_id: number | null;
  name: string | null;
  version: string | null;
  status: string | null;
}

export interface IReviewSubject {
  kind: TReviewSubjectKind;
  id: string | null;
  issue: IReviewSubjectIssue | null;
  release: IReviewSubjectRelease | null;
}

export interface IReviewSessionRef {
  id: string;
  title: string | null;
  scheduled_at: string | null;
}

export interface IReviewRequestSummary {
  id: string;
  board_type: TReviewBoardType;
  status: TReviewRequestStatus;
  submission_note: string;
  submitted_by: string | null;
  submitted_at: string;
}

export interface IReviewRequest extends IReviewRequestSummary {
  workspace_id: string;
  change_issue_id: string | null;
  release_id: string | null;
  project_id: string | null;
  created_at: string;
  updated_at: string;
  subject: IReviewSubject;
  session: IReviewSessionRef | null;
}

export interface IReviewHistoryEntry {
  session_id: string;
  title: string;
  scheduled_at: string;
  session_status: TReviewSessionStatus;
  outcome: TReviewOutcome | null;
  outcome_note: string;
  decided_at: string | null;
}

export interface IReviewRequestDetail extends IReviewRequest {
  history: IReviewHistoryEntry[];
}

export interface IReviewSessionCounts {
  items: number;
  participants: number;
  pending_outcome: number;
}

export interface IReviewSession {
  id: string;
  workspace_id: string;
  board_type: TReviewBoardType;
  project_id: string | null;
  title: string;
  scheduled_at: string;
  status: TReviewSessionStatus;
  minutes: string;
  location: string | null;
  completed_at: string | null;
  cancelled_at: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  counts: IReviewSessionCounts;
}

export interface IReviewItemSubject {
  kind: TReviewSubjectKind;
  identifier: string | null;
  name: string | null;
  version: string | null;
}

export interface IReviewSessionItem {
  id: string;
  session_id: string;
  review_request_id: string;
  position: number;
  outcome: TReviewOutcome | null;
  outcome_note: string;
  decided_by: string | null;
  decided_at: string | null;
  created_at: string;
  request_status: TReviewRequestStatus;
  submission_note: string;
  subject: IReviewItemSubject;
}

export interface IReviewParticipant {
  id: string;
  user_id: string;
  role: TReviewParticipantRole;
  attendance: TReviewAttendance;
  created_at: string;
  display_name: string;
  email: string | null;
}

export interface IReviewSessionDetail extends IReviewSession {
  items: IReviewSessionItem[];
  participants: IReviewParticipant[];
}

export type TReviewRequestCreatePayload = {
  board_type: TReviewBoardType;
  change_issue_id?: string | null;
  release_id?: string | null;
  submission_note?: string;
};

export type TReviewRequestListParams = {
  board_type: TReviewBoardType;
  project_id?: string;
  status?: TReviewRequestStatus;
  change_issue_id?: string;
  release_id?: string;
};

export type TReviewSessionCreatePayload = {
  board_type: TReviewBoardType;
  project_id?: string | null;
  title: string;
  scheduled_at: string;
  location?: string | null;
  minutes?: string;
};

export type TReviewSessionUpdatePayload = {
  title?: string;
  scheduled_at?: string;
  location?: string | null;
  minutes?: string;
};

export type TReviewSessionListParams = {
  board_type: TReviewBoardType;
  project_id?: string;
  status?: TReviewSessionStatus;
};

export type TReviewItemUpdatePayload = {
  outcome: TReviewOutcome;
  outcome_note?: string;
};

export type TReviewParticipantCreatePayload = {
  user_id: string;
  role?: TReviewParticipantRole;
  attendance?: TReviewAttendance;
};

export type TReviewParticipantUpdatePayload = {
  role?: TReviewParticipantRole;
  attendance?: TReviewAttendance;
};
