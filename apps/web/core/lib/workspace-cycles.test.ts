/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { ICycle } from "@plane/types";
import { normalizeWorkspaceCycle, selectWorkspaceCycles, WORKSPACE_CYCLE_FILTER_KEY } from "./workspace-cycles";

const NOW = new Date(2026, 9, 18); // Oct 18 2026, local time
const day = (value: number) => `2026-10-${String(value).padStart(2, "0")}`;

const makeCycle = (overrides: Partial<ICycle>): ICycle =>
  ({
    id: "cycle",
    name: "Cycle",
    project_id: "p1",
    start_date: null,
    end_date: null,
    archived_at: null,
    ...overrides,
  }) as ICycle;

describe("normalizeWorkspaceCycle", () => {
  it("derives a status when the workspace endpoint omits it", () => {
    const cycle = makeCycle({ id: "derived", start_date: day(16), end_date: day(20) });

    expect(normalizeWorkspaceCycle(cycle, NOW).status).toBe("current");
  });

  it("preserves an explicit status", () => {
    const cycle = makeCycle({
      id: "explicit",
      status: "completed",
      start_date: day(16),
      end_date: day(20),
    });

    expect(normalizeWorkspaceCycle(cycle, NOW).status).toBe("completed");
  });
});

describe("selectWorkspaceCycles", () => {
  it("groups and sorts current/upcoming/draft/completed cycles", () => {
    const selection = selectWorkspaceCycles({
      cycles: [
        makeCycle({ id: "current-b", name: "Beta", start_date: day(16), end_date: day(20) }),
        makeCycle({ id: "current-a", name: "Alpha", start_date: day(16), end_date: day(20) }),
        makeCycle({ id: "upcoming-later", name: "Later", start_date: day(26), end_date: day(30) }),
        makeCycle({ id: "upcoming-sooner", name: "Sooner", start_date: day(25), end_date: day(29) }),
        makeCycle({ id: "completed-new", name: "New", start_date: day(10), end_date: day(12) }),
        makeCycle({ id: "completed-old", name: "Old", start_date: day(1), end_date: day(5) }),
        makeCycle({ id: "draft", name: "Draft" }),
      ],
      projectIds: ["p1"],
      filters: {},
      searchQuery: "",
      now: NOW,
    });

    expect(selection.currentCycleIds).toEqual(["current-a", "current-b"]);
    expect(selection.upcomingCycleIds).toEqual(["upcoming-sooner", "upcoming-later"]);
    expect(selection.completedCycleIds).toEqual(["completed-new", "completed-old"]);
    expect(selection.draftCycleIds).toEqual(["draft"]);
    expect(selection.totalCount).toBe(7);
  });

  it("filters by project scope, archived state, and the project feature flag", () => {
    const selection = selectWorkspaceCycles({
      cycles: [
        makeCycle({ id: "p1", project_id: "p1", status: "current" }),
        makeCycle({ id: "p2", project_id: "p2", status: "current" }),
        makeCycle({ id: "p3", project_id: "p3", status: "current" }),
        makeCycle({ id: "archived", project_id: "p1", status: "current", archived_at: "2026-10-01" }),
      ],
      projectIds: ["p1", "p2"],
      filters: {},
      searchQuery: "",
      projectAllowed: (projectId) => projectId !== "p3",
      now: NOW,
    });

    expect(selection.currentCycleIds).toEqual(["p1", "p2"]);
    expect(selection.totalCount).toBe(2);
  });

  it("applies search and status filters", () => {
    const selection = selectWorkspaceCycles({
      cycles: [
        makeCycle({ id: "alpha-current", name: "Alpha", status: "current" }),
        makeCycle({ id: "alpha-completed", name: "Alpha", status: "completed" }),
        makeCycle({ id: "beta-current", name: "Beta", status: "current" }),
      ],
      projectIds: ["p1"],
      filters: { status: ["current"] },
      searchQuery: "alpha",
      now: NOW,
    });

    expect(selection.currentCycleIds).toEqual(["alpha-current"]);
    expect(selection.completedCycleIds).toEqual([]);
    expect(selection.totalCount).toBe(1);
  });

  it("exports the reserved workspace filter key", () => {
    expect(WORKSPACE_CYCLE_FILTER_KEY).toBe("__workspace__");
  });
});
