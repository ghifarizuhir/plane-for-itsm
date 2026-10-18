/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  appendIntakePage,
  buildIntakeDetailUrl,
  buildWorkspaceIntakeParams,
  createIntakePageState,
  flatGroupIntakeResults,
  normalizeIntakeEnvelope,
  normalizeIntakeTab,
  type TWorkspaceIntakeRow,
} from "./workspace-intake";

const row = (id: string) =>
  ({
    id,
    status: -2,
    issue: { id: `issue-${id}`, name: id, project_id: "p1" },
  }) as unknown as TWorkspaceIntakeRow;

describe("normalizeIntakeTab", () => {
  it("defaults unknown and empty tabs to open", () => {
    expect(normalizeIntakeTab(undefined)).toBe("open");
    expect(normalizeIntakeTab("nope")).toBe("open");
  });

  it("passes open and closed through", () => {
    expect(normalizeIntakeTab("open")).toBe("open");
    expect(normalizeIntakeTab("closed")).toBe("closed");
  });
});

describe("buildWorkspaceIntakeParams", () => {
  it("always sends the tab and paging", () => {
    expect(buildWorkspaceIntakeParams({})).toEqual({ type: "open", per_page: "100" });
  });

  it("includes project and trimmed search", () => {
    expect(buildWorkspaceIntakeParams({ typeFilter: "closed", projectId: "p2", search: "  vpn  " })).toEqual({
      type: "closed",
      project_id: "p2",
      search: "vpn",
      per_page: "100",
    });
  });

  it("drops blank search", () => {
    expect(buildWorkspaceIntakeParams({ typeFilter: "nope", search: "   " })).toEqual({
      type: "open",
      per_page: "100",
    });
  });
});

describe("normalizeIntakeEnvelope", () => {
  it("passes through array results and the cursor", () => {
    expect(
      normalizeIntakeEnvelope({
        results: [{ id: "a" }],
        next_cursor: "100:1:0",
        next_page_results: true,
        total_results: 12,
      })
    ).toEqual({
      results: [{ id: "a" }],
      nextCursor: "100:1:0",
      hasMore: true,
      totalResults: 12,
    });
  });

  it("drops non-array results and normalizes missing data", () => {
    expect(normalizeIntakeEnvelope({ results: { a: [] }, next_cursor: null })).toEqual({
      results: [],
      nextCursor: null,
      hasMore: false,
      totalResults: 0,
    });
  });
});

describe("createIntakePageState / appendIntakePage", () => {
  it("starts empty", () => {
    expect(createIntakePageState()).toEqual({ results: [], nextCursor: null, hasMore: false, totalResults: 0 });
  });

  it("appends pages preserving order and updates the cursor", () => {
    const first = createIntakePageState({ results: [row("a")], nextCursor: "100:1:0", hasMore: true, totalResults: 2 });
    const second = appendIntakePage(first, { results: [row("b")], nextCursor: null, hasMore: false, totalResults: 2 });
    expect(second.results.map((r) => r.id)).toEqual(["a", "b"]);
    expect(second.nextCursor).toBeNull();
    expect(second.hasMore).toBe(false);
    expect(second.totalResults).toBe(2);
  });
});

describe("flatGroupIntakeResults", () => {
  it("wraps rows in a single unlabeled group", () => {
    const groups = flatGroupIntakeResults([row("a"), row("b")]);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("all");
    expect(groups[0].label).toBe("");
    expect(groups[0].rows.map((r) => r.id)).toEqual(["a", "b"]);
  });

  it("returns no groups for empty results", () => {
    expect(flatGroupIntakeResults([])).toEqual([]);
  });
});

describe("buildIntakeDetailUrl", () => {
  it("builds the project inbox deep link with tab and issue id", () => {
    expect(buildIntakeDetailUrl("ws", "p1", "closed", "iss-1")).toBe(
      "/ws/projects/p1/intake?currentTab=closed&inboxIssueId=iss-1"
    );
  });
});
