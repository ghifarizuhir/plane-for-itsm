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
  // oxlint-disable-next-line unicorn/no-array-sort
  return [...requests].sort((a, b) => b.submitted_at.localeCompare(a.submitted_at))[0];
};

export const activeRequestForSubject = <T extends { status: string; submitted_at: string }>(requests: T[]): T | null =>
  latestRequestForSubject(requests.filter((request) => request.status === "pending" || request.status === "scheduled"));

export const sortSessionsByScheduledAt = (sessions: IReviewSession[]): IReviewSession[] =>
  // oxlint-disable-next-line unicorn/no-array-sort
  [...sessions].sort((a, b) => b.scheduled_at.localeCompare(a.scheduled_at));

const pad = (value: number) => String(value).padStart(2, "0");

export const toDateTimeLocal = (value: string | null | undefined): string => {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(
    date.getMinutes()
  )}`;
};

export const fromDateTimeLocal = (value: string): string | null => {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
};
