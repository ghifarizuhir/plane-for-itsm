/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TLogoProps } from "@plane/types";

export type TWorkspaceKbTab = "all" | "public" | "private" | "archived";

export type TWorkspaceKbPage = {
  id: string;
  name: string;
  access: number;
  owned_by: string;
  parent: string | null;
  is_favorite: boolean;
  is_locked: boolean;
  archived_at: string | null;
  created_at: string;
  updated_at: string;
  logo_props: TLogoProps | null;
  project_ids: string[];
};

export type TWorkspaceKbPageState = {
  results: TWorkspaceKbPage[];
  nextCursor: string | null;
  hasMore: boolean;
  totalResults: number;
};

export function normalizeWorkspaceKbType(typeParam?: string | null): TWorkspaceKbTab {
  return typeParam === "public" || typeParam === "private" || typeParam === "archived" ? typeParam : "all";
}

export function buildWorkspaceKbParams({
  typeFilter,
  projectId,
  search,
}: {
  typeFilter?: string;
  projectId?: string;
  search?: string;
}): Record<string, string> {
  const params: Record<string, string> = { per_page: "100", order_by: "-updated_at" };
  const type = normalizeWorkspaceKbType(typeFilter);
  if (type !== "all") params.type = type;
  if (projectId) params.project_id = projectId;
  const normalizedSearch = search?.trim() ?? "";
  if (normalizedSearch) params.search = normalizedSearch;
  return params;
}

export function normalizeKbResponse(response: unknown): TWorkspaceKbPageState {
  const data = (response ?? {}) as {
    results?: unknown;
    next_cursor?: unknown;
    next_page_results?: unknown;
    total_results?: unknown;
  };
  const results = Array.isArray(data.results) ? (data.results as TWorkspaceKbPage[]) : [];
  const nextCursor = typeof data.next_cursor === "string" ? data.next_cursor : null;
  return {
    results,
    nextCursor,
    hasMore: data.next_page_results === true && nextCursor !== null,
    totalResults: typeof data.total_results === "number" ? data.total_results : 0,
  };
}

export function createKbPageState(envelope?: TWorkspaceKbPageState): TWorkspaceKbPageState {
  if (!envelope) return { results: [], nextCursor: null, hasMore: false, totalResults: 0 };
  return envelope;
}

export function appendKbPage(state: TWorkspaceKbPageState, envelope: TWorkspaceKbPageState): TWorkspaceKbPageState {
  return {
    results: [...state.results, ...envelope.results],
    nextCursor: envelope.nextCursor,
    hasMore: envelope.hasMore,
    totalResults: envelope.totalResults,
  };
}

export function groupWorkspaceKbPages(
  pages: TWorkspaceKbPage[],
  projectName: (projectId: string) => string
): { key: string; label: string; pages: TWorkspaceKbPage[] }[] {
  const seen = new Set<string>();
  const unique: TWorkspaceKbPage[] = [];
  for (const page of pages) {
    if (seen.has(page.id)) continue;
    seen.add(page.id);
    unique.push(page);
  }
  const byProject = new Map<string, TWorkspaceKbPage[]>();
  for (const page of unique) {
    // oxlint-disable-next-line no-array-sort -- sorting a fresh copy; toSorted() is unavailable in this TS lib target
    const linked = [...(page.project_ids ?? [])].sort((a, b) => projectName(a).localeCompare(projectName(b)));
    const primary = linked[0];
    if (!primary) continue;
    const list = byProject.get(primary) ?? [];
    list.push(page);
    byProject.set(primary, list);
  }
  return (
    [...byProject.entries()]
      .map(([key, list]) => ({ key, label: projectName(key), pages: list }))
      // oxlint-disable-next-line no-array-sort -- sorting a fresh copy; toSorted() is unavailable in this TS lib target
      .sort((a, b) => a.label.localeCompare(b.label))
  );
}
