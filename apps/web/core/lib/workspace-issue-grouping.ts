/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 *
 * Client-side grouping for workspace-level issues.
 *
 * The api-rs workspace issues endpoint (`/api/workspaces/:slug/issues/`)
 * always returns a flat list and ignores `group_by` / date-range params, so
 * board (kanban), grouped list, and calendar need to build their buckets on
 * the client. Field accessors mirror the server's grouped contract
 * (`grouped.rs`: scalar keys, m2m keys, `"None"` for empty membership).
 */

import type { TIssue, TIssuesResponse } from "@plane/types";

export type TWorkspaceIssueRow = TIssue & {
  state__group?: string | null;
};

type TGroupField = {
  key: keyof TWorkspaceIssueRow;
  multi?: boolean;
};

const GROUP_FIELD_MAP: Record<string, TGroupField> = {
  state: { key: "state_id" },
  "state_detail.group": { key: "state__group" },
  priority: { key: "priority" },
  project: { key: "project_id" },
  labels: { key: "label_ids", multi: true },
  assignees: { key: "assignee_ids", multi: true },
  created_by: { key: "created_by" },
  cycle: { key: "cycle_id" },
  module: { key: "module_ids", multi: true },
  target_date: { key: "target_date" },
};

export function getWorkspaceIssueGroupKeys(row: TWorkspaceIssueRow, groupBy: string): string[] {
  const config = GROUP_FIELD_MAP[groupBy];
  if (!config) return ["None"];

  const value = row[config.key];

  if (config.multi) {
    const values = Array.isArray(value) ? value : [];
    if (values.length === 0) return ["None"];
    return values.map((item) => String(item));
  }

  if (value === null || value === undefined || value === "") return ["None"];
  return [String(value)];
}

export function filterWorkspaceIssuesByDateRange(
  issues: TWorkspaceIssueRow[],
  after?: string,
  before?: string
): TWorkspaceIssueRow[] {
  if (!after || !before) return issues;
  return issues.filter((issue) => {
    if (!issue.target_date) return false;
    const targetDate = issue.target_date.slice(0, 10);
    return targetDate >= after && targetDate <= before;
  });
}

type TBucket = {
  issues: TWorkspaceIssueRow[];
  subs: Map<string, TBucket>;
};

function createBucket(): TBucket {
  return { issues: [], subs: new Map() };
}

/**
 * Convert a flat `TIssuesResponse` into the grouped envelope shape consumed
 * by `BaseIssuesStore.processIssueResponse`:
 *
 *   results: { "<group>": { results: Issue[], total_results } }
 *   results: { "<group>": { results: { "<sub>": {...} }, total_results } }
 */
export function buildGroupedWorkspaceIssuesResponse(
  response: TIssuesResponse,
  groupBy: string,
  subGroupBy?: string | null
): TIssuesResponse {
  const rows = (Array.isArray(response.results) ? response.results : []) as unknown as TWorkspaceIssueRow[];

  const buckets = new Map<string, TBucket>();

  rows.forEach((row) => {
    const groupKeys = getWorkspaceIssueGroupKeys(row, groupBy);
    const subKeys = subGroupBy ? getWorkspaceIssueGroupKeys(row, subGroupBy) : [null];

    groupKeys.forEach((groupKey) => {
      const bucket = buckets.get(groupKey) ?? createBucket();
      subKeys.forEach((subKey) => {
        if (subKey === null) {
          bucket.issues.push(row);
          return;
        }
        const subBucket = bucket.subs.get(subKey) ?? createBucket();
        subBucket.issues.push(row);
        bucket.subs.set(subKey, subBucket);
      });
      buckets.set(groupKey, bucket);
    });
  });

  const results: Record<string, unknown> = {};
  buckets.forEach((bucket, groupKey) => {
    if (!subGroupBy) {
      results[groupKey] = {
        results: bucket.issues,
        total_results: bucket.issues.length,
      };
      return;
    }

    const subResults: Record<string, unknown> = {};
    let groupTotal = 0;
    bucket.subs.forEach((subBucket, subKey) => {
      groupTotal += subBucket.issues.length;
      subResults[subKey] = {
        results: subBucket.issues,
        total_results: subBucket.issues.length,
      };
    });
    results[groupKey] = {
      results: subResults,
      total_results: groupTotal,
    };
  });

  return {
    ...response,
    grouped_by: groupBy,
    sub_grouped_by: subGroupBy ?? null,
    // Workspace flat API paginates globally, so per-group infinite scroll is
    // disabled: one generous first page is loaded and paginated layouts that
    // rely on server grouping (board/calendar) render from it.
    next_page_results: false,
    results,
  } as unknown as TIssuesResponse;
}

/**
 * Resolve the effective grouping for a workspace layout.
 * Calendar always groups by target date; list/kanban use the display filters.
 */
export function resolveWorkspaceIssueGrouping(
  layout: string | undefined,
  groupBy: string | null | undefined,
  subGroupBy: string | null | undefined
): { groupBy: string | null; subGroupBy: string | null } {
  if (layout === "calendar") {
    return { groupBy: "target_date", subGroupBy: null };
  }
  if (layout === "list" || layout === "kanban") {
    return { groupBy: groupBy ?? null, subGroupBy: subGroupBy ?? null };
  }
  return { groupBy: null, subGroupBy: null };
}
