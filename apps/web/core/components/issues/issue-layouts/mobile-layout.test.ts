import { describe, expect, it } from "vitest";
import { ALL_ISSUES } from "@plane/constants";
import type { TGroupedIssues } from "@plane/types";
import { EIssueLayoutTypes } from "@plane/types";
import { flattenGroupedIssueIds, resolveWorkItemLayout } from "./mobile-layout";

describe("resolveWorkItemLayout", () => {
  it("returns the persisted layout unchanged on desktop", () => {
    expect(resolveWorkItemLayout(EIssueLayoutTypes.SPREADSHEET, false)).toBe(EIssueLayoutTypes.SPREADSHEET);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.LIST, false)).toBe(EIssueLayoutTypes.LIST);
  });

  it("returns list for desktop-only layouts on mobile", () => {
    expect(resolveWorkItemLayout(EIssueLayoutTypes.KANBAN, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.CALENDAR, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.GANTT, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.SPREADSHEET, true)).toBe(EIssueLayoutTypes.LIST);
  });

  it("keeps list and undefined as-is on mobile", () => {
    expect(resolveWorkItemLayout(EIssueLayoutTypes.LIST, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(undefined, true)).toBeUndefined();
  });
});

describe("flattenGroupedIssueIds", () => {
  it("returns a flat issue map unchanged", () => {
    expect(flattenGroupedIssueIds({ [ALL_ISSUES]: ["a", "b"] })).toEqual(["a", "b"]);
  });

  it("flattens grouped issue ids preserving group order", () => {
    expect(flattenGroupedIssueIds({ backlog: ["a"], unstarted: ["b", "c"] })).toEqual(["a", "b", "c"]);
  });

  it("flattens sub-grouped issue ids", () => {
    const subGrouped = { g1: { s1: ["a"], s2: ["b"] }, g2: { s1: ["c"] } } as unknown as TGroupedIssues;
    expect(flattenGroupedIssueIds(subGrouped)).toEqual(["a", "b", "c"]);
  });

  it("returns an empty array for an empty map", () => {
    expect(flattenGroupedIssueIds({})).toEqual([]);
  });
});
