import { describe, expect, it } from "vitest";
import { ALL_ISSUES } from "@plane/constants";
import type { GroupByColumnTypes, TGroupedIssues } from "@plane/types";
import { EIssueLayoutTypes } from "@plane/types";
import { isFlatGroupedIssueData, resolveRenderedGroupBy, resolveWorkItemLayout } from "./mobile-layout";

describe("resolveWorkItemLayout", () => {
  it("returns the persisted layout unchanged on desktop", () => {
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.SPREADSHEET }, false)).toBe(EIssueLayoutTypes.SPREADSHEET);
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.LIST }, false)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.CALENDAR }, false)).toBe(EIssueLayoutTypes.CALENDAR);
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.KANBAN }, false)).toBe(EIssueLayoutTypes.KANBAN);
  });

  it("returns list for spreadsheet and gantt on mobile", () => {
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.SPREADSHEET }, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.GANTT }, true)).toBe(EIssueLayoutTypes.LIST);
  });

  it("falls back from ungrouped kanban but keeps sub-grouped kanban on mobile", () => {
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.KANBAN, sub_group_by: null }, true)).toBe(
      EIssueLayoutTypes.LIST
    );
    expect(
      resolveWorkItemLayout({ layout: EIssueLayoutTypes.KANBAN, group_by: "state", sub_group_by: "priority" }, true)
    ).toBe(EIssueLayoutTypes.KANBAN);
  });

  it("falls back from kanban sub-grouped without a group by on mobile", () => {
    expect(
      resolveWorkItemLayout({ layout: EIssueLayoutTypes.KANBAN, group_by: null, sub_group_by: "priority" }, true)
    ).toBe(EIssueLayoutTypes.LIST);
  });

  it("keeps the calendar layout on mobile", () => {
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.CALENDAR }, true)).toBe(EIssueLayoutTypes.CALENDAR);
  });

  it("keeps list as-is and returns undefined for missing filters on mobile", () => {
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.LIST }, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(undefined, true)).toBeUndefined();
  });

  it("returns undefined when the layout is undefined on mobile", () => {
    expect(resolveWorkItemLayout({ layout: undefined }, true)).toBeUndefined();
  });
});

describe("isFlatGroupedIssueData", () => {
  it("detects flat data keyed by ALL_ISSUES", () => {
    expect(isFlatGroupedIssueData({ [ALL_ISSUES]: ["a"] })).toBe(true);
  });

  it("returns false for grouped, sub-grouped, empty, and undefined data", () => {
    expect(isFlatGroupedIssueData({ backlog: ["a"] })).toBe(false);
    expect(isFlatGroupedIssueData({ g1: { s1: ["a"] } } as unknown as TGroupedIssues)).toBe(false);
    expect(isFlatGroupedIssueData({})).toBe(false);
    expect(isFlatGroupedIssueData(undefined)).toBe(false);
  });
});

describe("resolveRenderedGroupBy", () => {
  it("renders flat data ungrouped", () => {
    expect(resolveRenderedGroupBy({ [ALL_ISSUES]: ["a"] }, "state" as GroupByColumnTypes)).toBeNull();
  });

  it("returns null for flat data when the group by is already null", () => {
    expect(resolveRenderedGroupBy({ [ALL_ISSUES]: ["a"] }, null)).toBeNull();
  });

  it("preserves the group by for grouped data", () => {
    expect(resolveRenderedGroupBy({ backlog: ["a"] }, "state" as GroupByColumnTypes)).toBe("state");
  });

  it("preserves the group by for sub-grouped data", () => {
    expect(
      resolveRenderedGroupBy({ g1: { s1: ["a"] } } as unknown as TGroupedIssues, "state" as GroupByColumnTypes)
    ).toBe("state");
  });

  it("preserves the group by for empty and undefined data", () => {
    expect(resolveRenderedGroupBy({}, "state" as GroupByColumnTypes)).toBe("state");
    expect(resolveRenderedGroupBy(undefined, "state" as GroupByColumnTypes)).toBe("state");
  });
});
