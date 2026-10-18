/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  applyWorkspaceFilters,
  deriveCycleStatus,
  groupWorkspaceCycles,
  groupWorkspaceModules,
} from "./workspace-aggregate";

// plain yyyy-mm-dd strings, which is exactly what `getDate` consumes
const dateString = (year: number, monthIndex: number, day: number) =>
  `${year}-${String(monthIndex + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
const START = (day: number) => dateString(2026, 9, day);
const START_SEP = (day: number) => dateString(2026, 8, day);
const START_NOV = (day: number) => dateString(2026, 10, day);
const NOW = new Date(2026, 9, 18); // Oct 18 2026, local time

const makeCycle = (id: string, start: string | null, end: string | null) => ({
  id,
  name: id,
  project_id: "p1",
  start_date: start,
  end_date: end,
});

const makeModule = (id: string, status: "backlog" | "planned" | "completed" | undefined, target: string | null) => ({
  id,
  name: id,
  project_id: "p1",
  status,
  target_date: target,
});

describe("deriveCycleStatus", () => {
  it("returns draft when a date is missing or invalid", () => {
    expect(deriveCycleStatus({ start_date: null, end_date: null }, NOW)).toBe("draft");
    expect(deriveCycleStatus({ start_date: START(18), end_date: null }, NOW)).toBe("draft");
    expect(deriveCycleStatus({ start_date: "not-a-date", end_date: START(18) }, NOW)).toBe("draft");
  });

  it("returns draft for a reversed range", () => {
    expect(deriveCycleStatus({ start_date: START(20), end_date: START(18) }, NOW)).toBe("draft");
  });

  it("returns current when today is inside the range, inclusive", () => {
    expect(deriveCycleStatus({ start_date: START(18), end_date: START(20) }, NOW)).toBe("current");
    expect(deriveCycleStatus({ start_date: START(16), end_date: START(18) }, NOW)).toBe("current");
  });

  it("returns upcoming when the range starts after today", () => {
    expect(deriveCycleStatus({ start_date: START(19), end_date: START(20) }, NOW)).toBe("upcoming");
  });

  it("returns completed when the range ended before today", () => {
    expect(deriveCycleStatus({ start_date: START(16), end_date: START(17) }, NOW)).toBe("completed");
  });
});

describe("groupWorkspaceCycles", () => {
  it("returns non-empty groups in canonical order and sorts within groups", () => {
    const groups = groupWorkspaceCycles(
      [
        makeCycle("draft-2", null, null),
        makeCycle("draft-1", null, null),
        makeCycle("completed-old", START_SEP(1), START_SEP(10)),
        makeCycle("completed-new", START(1), START(10)),
        makeCycle("current-late", START(10), START(30)),
        makeCycle("current-early", START(10), START(20)),
        makeCycle("upcoming-later", START_NOV(1), START_NOV(10)),
        makeCycle("upcoming-sooner", START(25), START_NOV(1)),
      ],
      NOW
    );

    expect(groups.map((group) => group.key)).toEqual(["draft", "current", "upcoming", "completed"]);
    expect(groups[0].items.map((item) => item.id)).toEqual(["draft-2", "draft-1"]);
    expect(groups[1].items.map((item) => item.id)).toEqual(["current-early", "current-late"]);
    expect(groups[2].items.map((item) => item.id)).toEqual(["upcoming-sooner", "upcoming-later"]);
    expect(groups[3].items.map((item) => item.id)).toEqual(["completed-new", "completed-old"]);
  });

  it("skips empty groups", () => {
    const groups = groupWorkspaceCycles([makeCycle("draft-1", null, null)], NOW);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("draft");
  });
});

describe("groupWorkspaceModules", () => {
  it("groups in MODULE_STATUS order, defaults missing status to backlog, sorts by target date then name", () => {
    const groups = groupWorkspaceModules([
      makeModule("planned-b", "planned", START(20)),
      makeModule("planned-a", "planned", START(20)),
      makeModule("planned-later", "planned", START_NOV(20)),
      makeModule("planned-no-date", "planned", null),
      makeModule("completed-1", "completed", null),
      makeModule("backlog-1", undefined, null),
    ]);

    expect(groups.map((group) => group.key)).toEqual(["backlog", "planned", "completed"]);
    expect(groups[1].items.map((item) => item.id)).toEqual([
      "planned-a",
      "planned-b",
      "planned-later",
      "planned-no-date",
    ]);
  });
});

describe("applyWorkspaceFilters", () => {
  const items = [
    { id: "a", name: "Alpha Sprint", project_id: "p1" },
    { id: "b", name: "beta sprint", project_id: "p2" },
    { id: "c", name: "Gamma", project_id: "p3" },
  ];

  it("filters by project id", () => {
    expect(applyWorkspaceFilters(items, { projectId: "p2" }).map((item) => item.id)).toEqual(["b"]);
  });

  it("filters by case-insensitive name search", () => {
    expect(applyWorkspaceFilters(items, { search: "SPRINT" }).map((item) => item.id)).toEqual(["a", "b"]);
  });

  it("applies the projectAllowed predicate (membership + feature flag)", () => {
    expect(
      applyWorkspaceFilters(items, { projectAllowed: (projectId) => projectId !== "p3" }).map((item) => item.id)
    ).toEqual(["a", "b"]);
  });

  it("composes filters", () => {
    expect(
      applyWorkspaceFilters(items, { projectId: "p1", search: "alpha", projectAllowed: () => true }).map(
        (item) => item.id
      )
    ).toEqual(["a"]);
  });
});
