/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  appendTicketsPage,
  buildWorkspaceTicketsFilter,
  createTicketsPageState,
  formatIssueKey,
  groupWorkspaceTickets,
  normalizeTicketsEnvelope,
  type TWorkspaceTicketRow,
} from "./workspace-tickets";

const ticket = (id: string, stateGroup: string | null | undefined) =>
  ({
    id,
    name: id,
    state__group: stateGroup,
  }) as unknown as TWorkspaceTicketRow;

describe("formatIssueKey", () => {
  it("formats identifier and sequence id", () => {
    expect(formatIssueKey("ENG", 123)).toBe("ENG-123");
  });
});

describe("buildWorkspaceTicketsFilter", () => {
  it("returns undefined when no filter is active", () => {
    expect(buildWorkspaceTicketsFilter({})).toBeUndefined();
    expect(buildWorkspaceTicketsFilter({ search: "   " })).toBeUndefined();
  });

  it("builds a project-only filter", () => {
    expect(JSON.parse(buildWorkspaceTicketsFilter({ projectId: "p1" }) ?? "null")).toEqual({
      and: [{ project_id: "p1" }],
    });
  });

  it("builds a trimmed search filter", () => {
    expect(JSON.parse(buildWorkspaceTicketsFilter({ search: "  login  " }) ?? "null")).toEqual({
      and: [{ name__icontains: "login" }],
    });
  });

  it("builds a combined filter", () => {
    expect(JSON.parse(buildWorkspaceTicketsFilter({ projectId: "p1", search: "login" }) ?? "null")).toEqual({
      and: [{ project_id: "p1" }, { name__icontains: "login" }],
    });
  });
});

describe("normalizeTicketsEnvelope", () => {
  it("passes through array results", () => {
    const envelope = normalizeTicketsEnvelope({
      results: [{ id: "t1" }],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 250,
    } as never);
    expect(envelope).toEqual({
      results: [{ id: "t1" }],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 250,
    });
  });

  it("drops grouped (non-array) results and normalizes a missing cursor", () => {
    const envelope = normalizeTicketsEnvelope({
      results: { backlog: { results: [] } },
      next_cursor: null,
      next_page_results: false,
      total_results: 0,
    } as never);
    expect(envelope).toEqual({
      results: [],
      next_cursor: null,
      next_page_results: false,
      total_results: 0,
    });
  });
});

describe("createTicketsPageState / appendTicketsPage", () => {
  it("starts empty", () => {
    expect(createTicketsPageState()).toEqual({
      results: [],
      nextCursor: null,
      hasMore: false,
      totalResults: 0,
    });
  });

  it("maps a first page and its cursor", () => {
    const state = createTicketsPageState({
      results: [ticket("t1", "backlog")],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 250,
    });
    expect(state.results.map((row) => row.id)).toEqual(["t1"]);
    expect(state.nextCursor).toBe("100:1:0");
    expect(state.hasMore).toBe(true);
    expect(state.totalResults).toBe(250);
  });

  it("marks hasMore false when there are no next results", () => {
    const state = createTicketsPageState({
      results: [],
      next_cursor: null,
      next_page_results: false,
      total_results: 5,
    });
    expect(state.hasMore).toBe(false);
  });

  it("appends pages preserving order and updates the cursor", () => {
    const first = createTicketsPageState({
      results: [ticket("t1", "backlog")],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 2,
    });
    const second = appendTicketsPage(first, {
      results: [ticket("t2", "started")],
      next_cursor: null,
      next_page_results: false,
      total_results: 2,
    });
    expect(second.results.map((row) => row.id)).toEqual(["t1", "t2"]);
    expect(second.nextCursor).toBeNull();
    expect(second.hasMore).toBe(false);
    expect(second.totalResults).toBe(2);
  });
});

describe("groupWorkspaceTickets", () => {
  it("groups in STATE_GROUPS order and skips empty groups", () => {
    const groups = groupWorkspaceTickets([ticket("t1", "started"), ticket("t2", "backlog"), ticket("t3", "started")]);
    expect(groups.map((group) => group.key)).toEqual(["backlog", "started"]);
    expect(groups[0].tickets.map((row) => row.id)).toEqual(["t2"]);
    expect(groups[1].tickets.map((row) => row.id)).toEqual(["t1", "t3"]);
  });

  it("falls back to backlog for missing or unknown groups", () => {
    const groups = groupWorkspaceTickets([ticket("t1", null), ticket("t2", "unknown-group")]);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("backlog");
    expect(groups[0].tickets.map((row) => row.id)).toEqual(["t1", "t2"]);
  });
});
