import type { IReviewSession, TReviewRequestListParams, TReviewSessionListParams } from "@plane/types";

export const reviewRequestsKey = (params: TReviewRequestListParams): string =>
  [
    params.board_type,
    params.project_id ?? "ws",
    params.status ?? "all",
    params.change_issue_id ?? "-",
    params.release_id ?? "-",
  ].join(":");

export const reviewSessionsKey = (params: TReviewSessionListParams): string =>
  [params.board_type, params.project_id ?? "ws", params.status ?? "all"].join(":");

export const latestRequestForSubject = <T extends { submitted_at: string }>(requests: T[]): T | null => {
  if (requests.length === 0) return null;
  return [...requests].toSorted((a, b) => b.submitted_at.localeCompare(a.submitted_at))[0];
};

export const activeRequestForSubject = <T extends { status: string; submitted_at: string }>(requests: T[]): T | null =>
  latestRequestForSubject(requests.filter((request) => request.status === "pending" || request.status === "scheduled"));

export const sortSessionsByScheduledAt = (sessions: IReviewSession[]): IReviewSession[] =>
  [...sessions].toSorted((a, b) => b.scheduled_at.localeCompare(a.scheduled_at));
