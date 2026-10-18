/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  appendKbPage,
  buildWorkspaceKbParams,
  createKbPageState,
  groupWorkspaceKbPages,
  normalizeKbResponse,
  normalizeWorkspaceKbType,
  type TWorkspaceKbPage,
} from "./workspace-kb";

const page = (id: string, projectIds: string[]) =>
  ({
    id,
    name: id,
    project_ids: projectIds,
  }) as unknown as TWorkspaceKbPage;

describe("normalizeWorkspaceKbType", () => {
  it("defaults unknown and empty values to all", () => {
    expect(normalizeWorkspaceKbType(undefined)).toBe("all");
    expect(normalizeWorkspaceKbType("nope")).toBe("all");
  });

  it("passes through known tabs", () => {
    expect(normalizeWorkspaceKbType("public")).toBe("public");
    expect(normalizeWorkspaceKbType("private")).toBe("private");
    expect(normalizeWorkspaceKbType("archived")).toBe("archived");
  });
});

describe("buildWorkspaceKbParams", () => {
  it("always sends paging and ordering", () => {
    expect(buildWorkspaceKbParams({})).toEqual({ per_page: "100", order_by: "-updated_at" });
  });

  it("includes type, project and trimmed search", () => {
    expect(buildWorkspaceKbParams({ typeFilter: "public", projectId: "p2", search: "  sop  " })).toEqual({
      per_page: "100",
      order_by: "-updated_at",
      type: "public",
      project_id: "p2",
      search: "sop",
    });
  });

  it("drops unknown types and blank search", () => {
    expect(buildWorkspaceKbParams({ typeFilter: "nope", search: "   " })).toEqual({
      per_page: "100",
      order_by: "-updated_at",
    });
  });
});

describe("normalizeKbResponse", () => {
  it("passes through array results and the cursor", () => {
    expect(
      normalizeKbResponse({
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
    expect(normalizeKbResponse({ results: { a: [] }, next_cursor: null })).toEqual({
      results: [],
      nextCursor: null,
      hasMore: false,
      totalResults: 0,
    });
  });
});

describe("createKbPageState / appendKbPage", () => {
  it("starts empty", () => {
    expect(createKbPageState()).toEqual({ results: [], nextCursor: null, hasMore: false, totalResults: 0 });
  });

  it("appends pages preserving order and updates the cursor", () => {
    const first = createKbPageState({
      results: [page("a", ["p1"])],
      nextCursor: "100:1:0",
      hasMore: true,
      totalResults: 2,
    });
    const second = appendKbPage(first, {
      results: [page("b", ["p2"])],
      nextCursor: null,
      hasMore: false,
      totalResults: 2,
    });
    expect(second.results.map((row) => row.id)).toEqual(["a", "b"]);
    expect(second.nextCursor).toBeNull();
    expect(second.hasMore).toBe(false);
    expect(second.totalResults).toBe(2);
  });
});

describe("groupWorkspaceKbPages", () => {
  const names: Record<string, string> = { p1: "Platform", p2: "Billing", p3: "Auth" };
  const projectName = (id: string) => names[id] ?? id;

  it("dedupes multi-linked pages and groups by the first alphabetical project", () => {
    const groups = groupWorkspaceKbPages(
      [page("a", ["p2", "p1"]), page("c", ["p1"]), page("b", ["p2"]), page("a", ["p2", "p1"])],
      projectName
    );
    expect(groups.map((group) => group.key)).toEqual(["p2", "p1"]);
    expect(groups[0].label).toBe("Billing");
    expect(groups[0].pages.map((row) => row.id)).toEqual(["a", "b"]);
    expect(groups[1].label).toBe("Platform");
    expect(groups[1].pages.map((row) => row.id)).toEqual(["c"]);
  });

  it("skips pages without member projects", () => {
    expect(groupWorkspaceKbPages([page("a", [])], projectName)).toEqual([]);
  });
});
