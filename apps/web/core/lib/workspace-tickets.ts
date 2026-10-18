/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { STATE_GROUPS } from "@plane/constants";
import type { TBaseIssue, TIssuesResponse, TStateGroups } from "@plane/types";

export type TWorkspaceTicketRow = TBaseIssue & {
  state__group?: string | null;
};

export type TWorkspaceTicketsEnvelope = {
  results: TWorkspaceTicketRow[];
  next_cursor: string | null;
  next_page_results: boolean;
  total_results: number;
};

export type TWorkspaceTicketsPage = {
  results: TWorkspaceTicketRow[];
  nextCursor: string | null;
  hasMore: boolean;
  totalResults: number;
};

export const WORKSPACE_TICKET_GROUP_ORDER: TStateGroups[] = [
  "backlog",
  "unstarted",
  "started",
  "completed",
  "cancelled",
];

export function formatIssueKey(identifier: string, sequenceId: number): string {
  return `${identifier}-${sequenceId}`;
}

export function buildWorkspaceTicketsFilter({
  projectId,
  search,
}: {
  projectId?: string;
  search?: string;
}): string | undefined {
  const conditions: Record<string, string>[] = [];
  if (projectId) conditions.push({ project_id: projectId });
  const normalizedSearch = search?.trim() ?? "";
  if (normalizedSearch) conditions.push({ name__icontains: normalizedSearch });
  if (conditions.length === 0) return undefined;
  return JSON.stringify({ and: conditions });
}

export function normalizeTicketsEnvelope(response: TIssuesResponse): TWorkspaceTicketsEnvelope {
  const results = Array.isArray(response.results) ? (response.results as TWorkspaceTicketRow[]) : [];
  return {
    results,
    next_cursor: response.next_cursor ?? null,
    next_page_results: Boolean(response.next_page_results),
    total_results: response.total_results ?? 0,
  };
}

export function createTicketsPageState(envelope?: TWorkspaceTicketsEnvelope): TWorkspaceTicketsPage {
  if (!envelope) return { results: [], nextCursor: null, hasMore: false, totalResults: 0 };
  return {
    results: envelope.results,
    nextCursor: envelope.next_cursor,
    hasMore: envelope.next_page_results && Boolean(envelope.next_cursor),
    totalResults: envelope.total_results,
  };
}

export function appendTicketsPage(
  state: TWorkspaceTicketsPage,
  envelope: TWorkspaceTicketsEnvelope
): TWorkspaceTicketsPage {
  return {
    results: [...state.results, ...envelope.results],
    nextCursor: envelope.next_cursor,
    hasMore: envelope.next_page_results && Boolean(envelope.next_cursor),
    totalResults: envelope.total_results,
  };
}

export function deriveTicketGroup(ticket: Pick<TWorkspaceTicketRow, "state__group">): TStateGroups {
  const group = ticket["state__group"];
  return group && group in STATE_GROUPS ? (group as TStateGroups) : "backlog";
}

export function groupWorkspaceTickets(
  tickets: TWorkspaceTicketRow[]
): { key: TStateGroups; tickets: TWorkspaceTicketRow[] }[] {
  const buckets: Record<TStateGroups, TWorkspaceTicketRow[]> = {
    backlog: [],
    unstarted: [],
    started: [],
    completed: [],
    cancelled: [],
  };
  tickets.forEach((ticket) => buckets[deriveTicketGroup(ticket)].push(ticket));
  const groups: { key: TStateGroups; tickets: TWorkspaceTicketRow[] }[] = [];
  for (const key of WORKSPACE_TICKET_GROUP_ORDER) {
    if (buckets[key].length > 0) groups.push({ key, tickets: buckets[key] });
  }
  return groups;
}
