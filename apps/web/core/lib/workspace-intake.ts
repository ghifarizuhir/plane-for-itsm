/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWorkspaceIntakeTab = "open" | "closed";

export type TWorkspaceIntakeIssue = {
  id: string;
  name: string;
  priority: string;
  sequence_id: number;
  project_id: string;
  created_at: string;
  label_ids: string[];
  created_by: string | null;
};

export type TWorkspaceIntakeRow = {
  id: string;
  status: number;
  duplicate_to_id: string | null;
  snoozed_till: string | null;
  source: string | null;
  created_by: string | null;
  issue: TWorkspaceIntakeIssue;
};

export type TWorkspaceIntakePageState = {
  results: TWorkspaceIntakeRow[];
  nextCursor: string | null;
  hasMore: boolean;
  totalResults: number;
};

export function normalizeIntakeTab(typeParam?: string | null): TWorkspaceIntakeTab {
  return typeParam === "closed" ? "closed" : "open";
}

export function buildWorkspaceIntakeParams({
  typeFilter,
  projectId,
  search,
}: {
  typeFilter?: string;
  projectId?: string;
  search?: string;
}): Record<string, string> {
  const params: Record<string, string> = { type: normalizeIntakeTab(typeFilter), per_page: "100" };
  if (projectId) params.project_id = projectId;
  const normalizedSearch = search?.trim() ?? "";
  if (normalizedSearch) params.search = normalizedSearch;
  return params;
}

export function normalizeIntakeEnvelope(response: unknown): TWorkspaceIntakePageState {
  const data = (response ?? {}) as {
    results?: unknown;
    next_cursor?: unknown;
    next_page_results?: unknown;
    total_results?: unknown;
  };
  const results = Array.isArray(data.results) ? (data.results as TWorkspaceIntakeRow[]) : [];
  const nextCursor = typeof data.next_cursor === "string" ? data.next_cursor : null;
  return {
    results,
    nextCursor,
    hasMore: data.next_page_results === true && nextCursor !== null,
    totalResults: typeof data.total_results === "number" ? data.total_results : 0,
  };
}

export function createIntakePageState(envelope?: TWorkspaceIntakePageState): TWorkspaceIntakePageState {
  if (!envelope) return { results: [], nextCursor: null, hasMore: false, totalResults: 0 };
  return envelope;
}

export function appendIntakePage(
  state: TWorkspaceIntakePageState,
  envelope: TWorkspaceIntakePageState
): TWorkspaceIntakePageState {
  return {
    results: [...state.results, ...envelope.results],
    nextCursor: envelope.nextCursor,
    hasMore: envelope.hasMore,
    totalResults: envelope.totalResults,
  };
}

export function flatGroupIntakeResults(
  rows: TWorkspaceIntakeRow[]
): { key: string; label: string; rows: TWorkspaceIntakeRow[] }[] {
  return rows.length === 0 ? [] : [{ key: "all", label: "", rows }];
}

export function buildIntakeDetailUrl(workspaceSlug: string, projectId: string, tab: string, issueId: string): string {
  return `/${workspaceSlug}/projects/${projectId}/intake?currentTab=${tab}&inboxIssueId=${issueId}`;
}
