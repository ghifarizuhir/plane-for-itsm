import { describe, expect, it } from "vitest";

import { getSingleWorkItemTypeId, getWorkItemTypeIds } from "./work-item-type.helpers";

describe("getSingleWorkItemTypeId", () => {
  it("returns null when no filters", () => {
    expect(getSingleWorkItemTypeId(null)).toBeNull();
    expect(getSingleWorkItemTypeId(undefined)).toBeNull();
  });

  it("reads legacy issue_type filter", () => {
    expect(getSingleWorkItemTypeId({ filters: { issue_type: ["t-1"] } })).toBe("t-1");
    expect(getSingleWorkItemTypeId({ filters: { issue_type: ["t-1", "t-2"] } })).toBeNull();
  });

  it("reads rich type_id filters", () => {
    expect(getSingleWorkItemTypeId({ richFilters: { type_id__in: "t-1" } as never })).toBe("t-1");
    expect(getSingleWorkItemTypeId({ richFilters: { and: [{ type_id__exact: "t-9" }] } as never })).toBe("t-9");
  });
});

describe("getWorkItemTypeIds", () => {
  it("collects all type ids", () => {
    expect(getWorkItemTypeIds(null)).toEqual([]);
    expect(getWorkItemTypeIds({ filters: { issue_type: ["t-1", "t-2"] } })).toEqual(["t-1", "t-2"]);
    expect(getWorkItemTypeIds({ richFilters: { type_id__in: "t-1,t-2" } as never })).toEqual(["t-1", "t-2"]);
  });
});
