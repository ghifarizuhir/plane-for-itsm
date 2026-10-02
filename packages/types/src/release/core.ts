import type { IReviewRequestSummary } from "../review";

export type TReleaseStatus = "draft" | "planned" | "in_review" | "approved" | "released" | "cancelled";

export interface IRelease {
  id: string;
  workspace_id: string;
  sequence_id: number;
  name: string;
  version: string | null;
  description_html: string;
  status: TReleaseStatus;
  target_date: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  updated_by: string | null;
}

export interface IReleaseChange {
  id: string;
  issue_id: string;
  project_id: string;
  issue_name: string | null;
  issue_identifier: string | null;
  project_identifier: string | null;
}

export interface IReleaseDetail extends IRelease {
  changes: IReleaseChange[];
  review_requests: IReviewRequestSummary[];
}

export type TReleaseCreatePayload = {
  name: string;
  version?: string | null;
  description_html?: string;
  status?: TReleaseStatus;
  target_date?: string | null;
};

export type TReleaseUpdatePayload = Partial<TReleaseCreatePayload>;
