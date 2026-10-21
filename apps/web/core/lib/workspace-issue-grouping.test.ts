/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TIssuesResponse } from "@plane/types";
import { describe, expect, it } from "vitest";
import type { TWorkspaceIssueRow } from "./workspace-issue-grouping";
import {
  buildGroupedWorkspaceIssuesResponse,
  filterWorkspaceIssuesByDateRange,
  getWorkspaceIssueGroupKeys,
  resolveWorkspaceIssueGrouping,
} from "./workspace-issue-grouping";

const row = (overrides: Partial<TWorkspaceIssueRow>): TWorkspaceIssueRow =>
  ({
    id: "issue-1",
    project_id: "project-1",
    state_id: "state-1",
    priority: "none",
    label_ids: [],
    assignee_ids: [],
    module_ids: [],
    created_by: "user-1",
    target_date: null,
    ...overrides,
  }) as TWorkspaceIssueRow;

const envelope = (results: TWorkspaceIssueRow[]): TIssuesResponse =>
  ({
    grouped_by: null,
    sub_grouped_by: null,
    total_count: results.length,
    next_cursor: "200:1:0",
    prev_cursor: "200:-1:0",
    next_page_results: true,
    prev_page_results: false,
    count: results.length,
    total_pages: 1,
    total_results: results.length,
    extra_stats: null,
    results,
  }) as unknown as TIssuesResponse;

describe("getWorkspaceIssueGroupKeys", () => {
  it("groups scalar fields and falls back to None", () => {
    expect(getWorkspaceIssueGroupKeys(row({ state__group: "started" }), "state_detail.group")).toEqual(["started"]);
    expect(getWorkspaceIssueGroupKeys(row({ state__group: null }), "state_detail.group")).toEqual(["None"]);
    expect(getWorkspaceIssueGroupKeys(row({ priority: "high" }), "priority")).toEqual(["high"]);
  });

  it("groups m2m fields into every membership", () => {
    expect(getWorkspaceIssueGroupKeys(row({ label_ids: ["l1", "l2"] }), "labels")).toEqual(["l1", "l2"]);
    expect(getWorkspaceIssueGroupKeys(row({ assignee_ids: [] }), "assignees")).toEqual(["None"]);
  });
});

describe("buildGroupedWorkspaceIssuesResponse", () => {
  it("builds the flat grouped envelope consumed by the issue store", () => {
    const response = envelope([
      row({ id: "a", state__group: "started" }),
      row({ id: "b", state__group: "started" }),
      row({ id: "c", state__group: "backlog" }),
      row({ id: "d", state__group: null }),
    ]);

    const grouped = buildGroupedWorkspaceIssuesResponse(response, "state_detail.group") as unknown as {
      grouped_by: string;
      next_page_results: boolean;
      results: Record<string, { results: TWorkspaceIssueRow[]; total_results: number }>;
    };

    expect(grouped.grouped_by).toBe("state_detail.group");
    expect(grouped.next_page_results).toBe(false);
    expect(grouped.results.started.total_results).toBe(2);
    expect(grouped.results.started.results.map((issue) => issue.id)).toEqual(["a", "b"]);
    expect(grouped.results.backlog.total_results).toBe(1);
    expect(grouped.results.None.results.map((issue) => issue.id)).toEqual(["d"]);
  });

  it("builds nested sub-groups", () => {
    const response = envelope([
      row({ id: "a", state__group: "started", priority: "high" }),
      row({ id: "b", state__group: "started", priority: "low" }),
      row({ id: "c", state__group: "backlog", priority: "high" }),
    ]);

    const grouped = buildGroupedWorkspaceIssuesResponse(response, "state_detail.group", "priority") as unknown as {
      results: Record<
        string,
        { results: Record<string, { results: TWorkspaceIssueRow[] }>; total_results: number }
      >;
    };

    expect(grouped.results.started.total_results).toBe(2);
    expect(grouped.results.started.results.high.results.map((issue) => issue.id)).toEqual(["a"]);
    expect(grouped.results.started.results.low.results.map((issue) => issue.id)).toEqual(["b"]);
    expect(grouped.results.backlog.results.high.results.map((issue) => issue.id)).toEqual(["c"]);
  });

  it("groups calendar issues by target date", () => {
    const response = envelope([
      row({ id: "a", target_date: "2026-10-21" }),
      row({ id: "b", target_date: "2026-10-21" }),
      row({ id: "c", target_date: "2026-10-22" }),
    ]);

    const grouped = buildGroupedWorkspaceIssuesResponse(response, "target_date") as unknown as {
      results: Record<string, { total_results: number }>;
    };

    expect(grouped.results["2026-10-21"].total_results).toBe(2);
    expect(grouped.results["2026-10-22"].total_results).toBe(1);
  });
});

describe("filterWorkspaceIssuesByDateRange", () => {
  it("keeps issues whose target date is inside the inclusive range", () => {
    const issues = [
      row({ id: "a", target_date: "2026-10-01" }),
      row({ id: "b", target_date: "2026-10-21" }),
      row({ id: "c", target_date: "2026-11-01" }),
      row({ id: "d", target_date: null }),
    ];

    const filtered = filterWorkspaceIssuesByDateRange(issues, "2026-10-01", "2026-10-31");
    expect(filtered.map((issue) => issue.id)).toEqual(["a", "b"]);
  });
});

describe("resolveWorkspaceIssueGrouping", () => {
  it("always groups calendar by target date", () => {
    expect(resolveWorkspaceIssueGrouping("calendar", "priority", "assignees")).toEqual({
      groupBy: "target_date",
      subGroupBy: null,
    });
  });

  it("uses display group filters for list/kanban", () => {
    expect(resolveWorkspaceIssueGrouping("kanban", "state_detail.group", "priority")).toEqual({
      groupBy: "state_detail.group",
      subGroupBy: "priority",
    });
    expect(resolveWorkspaceIssueGrouping("list", null, null)).toEqual({ groupBy: null, subGroupBy: null });
  });

  it("does not group spreadsheet/gantt", () => {
    expect(resolveWorkspaceIssueGrouping("spreadsheet", "priority", null)).toEqual({
      groupBy: null,
      subGroupBy: null,
    });
  });
});
