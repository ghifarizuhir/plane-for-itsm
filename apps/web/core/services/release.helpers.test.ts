import { describe, expect, it } from "vitest";
import type { IRelease } from "@plane/types";
// local imports
import { filterReleases, orderReleases, releaseStatusOrder } from "./release.helpers";

const base = (overrides: Partial<IRelease>): IRelease => ({
  id: "r1",
  workspace_id: "w1",
  sequence_id: 1,
  name: "Rilis",
  version: null,
  description_html: "",
  status: "draft",
  target_date: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  created_by: null,
  updated_by: null,
  ...overrides,
});

describe("filterReleases", () => {
  const releases = [
    base({ id: "a", name: "Rilis Januari", status: "planned", version: "v1.0" }),
    base({ id: "b", name: "Rilis Februari", status: "released", version: "v2.0" }),
  ];

  it("returns all releases when no filters are applied", () => {
    expect(filterReleases(releases, {}, "")).toHaveLength(2);
  });

  it("filters by status", () => {
    const result = filterReleases(releases, { status: ["planned"] }, "");
    expect(result.map((release) => release.id)).toEqual(["a"]);
  });

  it("searches name and version case-insensitively", () => {
    expect(filterReleases(releases, {}, "februari").map((release) => release.id)).toEqual(["b"]);
    expect(filterReleases(releases, {}, "V1.0").map((release) => release.id)).toEqual(["a"]);
  });
});

describe("orderReleases", () => {
  it("orders by created_at descending by default", () => {
    const older = base({ id: "old", created_at: "2026-01-01T00:00:00Z" });
    const newer = base({ id: "new", created_at: "2026-02-01T00:00:00Z" });
    expect(orderReleases([older, newer]).map((release) => release.id)).toEqual(["new", "old"]);
  });

  it("orders by target date with nulls last", () => {
    const noDate = base({ id: "none", target_date: null });
    const later = base({ id: "later", target_date: "2026-12-01" });
    const sooner = base({ id: "sooner", target_date: "2026-10-01" });
    expect(orderReleases([noDate, later, sooner], "target_date").map((release) => release.id)).toEqual([
      "sooner",
      "later",
      "none",
    ]);
  });
});

describe("releaseStatusOrder", () => {
  it("ranks lifecycle statuses", () => {
    expect(releaseStatusOrder("draft")).toBeLessThan(releaseStatusOrder("approved"));
    expect(releaseStatusOrder("approved")).toBeLessThan(releaseStatusOrder("cancelled"));
  });
});
