import { describe, expect, it } from "vitest";
import { EIssueLayoutTypes } from "@plane/types";
import { resolveWorkItemLayout } from "./mobile-layout";

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
