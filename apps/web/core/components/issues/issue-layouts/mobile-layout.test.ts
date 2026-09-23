import { describe, expect, it } from "vitest";
import { EIssueLayoutTypes } from "@plane/types";
import { resolveWorkItemLayout } from "./mobile-layout";

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
    expect(resolveWorkItemLayout({ layout: EIssueLayoutTypes.KANBAN, sub_group_by: "priority" }, true)).toBe(
      EIssueLayoutTypes.KANBAN
    );
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
