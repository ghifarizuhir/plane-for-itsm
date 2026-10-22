/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { IModule } from "@plane/types";
import { selectWorkspaceModules, WORKSPACE_MODULE_FILTER_KEY } from "./workspace-modules";

const day = (value: number) => `2026-10-${String(value).padStart(2, "0")}`;

const makeModule = (overrides: Partial<IModule>): IModule =>
  ({
    id: "module",
    name: "Module",
    project_id: "p1",
    archived_at: null,
    status: "planned",
    lead_id: null,
    member_ids: [],
    is_favorite: false,
    start_date: null,
    target_date: null,
    created_at: "2026-10-01",
    sort_order: 1,
    completed_issues: 0,
    total_issues: 0,
    cancelled_issues: 0,
    ...overrides,
  }) as IModule;

const select = (modules: IModule[], overrides: Partial<Parameters<typeof selectWorkspaceModules>[0]> = {}) =>
  selectWorkspaceModules({
    modules,
    projectIds: ["p1"],
    displayFilters: { layout: "list", order_by: "name", favorites: false },
    filters: {},
    searchQuery: "",
    ...overrides,
  });

describe("selectWorkspaceModules", () => {
  it("filters by project scope, archived state, and the project feature flag", () => {
    const selection = select(
      [
        makeModule({ id: "p1", project_id: "p1" }),
        makeModule({ id: "p2", project_id: "p2" }),
        makeModule({ id: "p3", project_id: "p3" }),
        makeModule({ id: "archived", project_id: "p1", archived_at: "2026-10-01" }),
      ],
      {
        projectIds: ["p1", "p2", "p3"],
        projectAllowed: (projectId) => projectId !== "p3",
      }
    );

    expect(selection.moduleIds).toEqual(["p1", "p2"]);
    expect(selection.totalCount).toBe(2);
  });

  it("applies search case-insensitively", () => {
    const selection = select(
      [
        makeModule({ id: "alpha", name: "Alpha" }),
        makeModule({ id: "beta", name: "Beta" }),
        makeModule({ id: "alphabet", name: "Alphabet" }),
      ],
      { searchQuery: "ALPHA" }
    );

    expect(selection.moduleIds).toEqual(["alpha", "alphabet"]);
    expect(selection.totalCount).toBe(2);
  });

  it("applies status, lead, and member filters through shouldFilterModule", () => {
    const modules = [
      makeModule({ id: "match", status: "planned", lead_id: "u1", member_ids: ["u2"] }),
      makeModule({ id: "wrong-status", status: "completed", lead_id: "u1", member_ids: ["u2"] }),
      makeModule({ id: "wrong-lead", status: "planned", lead_id: "u3", member_ids: ["u2"] }),
      makeModule({ id: "wrong-member", status: "planned", lead_id: "u1", member_ids: ["u4"] }),
    ];

    const selection = select(modules, {
      filters: { status: ["planned"], lead: ["u1"], members: ["u2"] },
    });

    expect(selection.moduleIds).toEqual(["match"]);
  });

  it("applies date filters through the shared module filter helper", () => {
    const selection = select(
      [
        makeModule({ id: "later", start_date: day(26), target_date: day(30) }),
        makeModule({ id: "sooner", start_date: day(25), target_date: day(29) }),
      ],
      {
        displayFilters: { order_by: "name" },
        filters: { start_date: [`${day(26)};after`] },
      }
    );

    expect(selection.moduleIds).toEqual(["later"]);
  });

  it("applies the favorites display filter", () => {
    const selection = select(
      [makeModule({ id: "favorite", is_favorite: true }), makeModule({ id: "other", is_favorite: false })],
      {
        displayFilters: { order_by: "name", favorites: true },
      }
    );

    expect(selection.moduleIds).toEqual(["favorite"]);
  });

  it("orders names case-insensitively by default", () => {
    const selection = select(
      [
        makeModule({ id: "upper", name: "Alpha" }),
        makeModule({ id: "lower", name: "alpha" }),
        makeModule({ id: "beta", name: "Beta" }),
      ],
      { displayFilters: {} }
    );

    expect(selection.moduleIds).toEqual(["upper", "lower", "beta"]);
  });

  it("orders by target date", () => {
    const selection = select(
      [
        makeModule({ id: "later", name: "Later", target_date: day(28) }),
        makeModule({ id: "sooner", name: "Sooner", target_date: day(20) }),
      ],
      { displayFilters: { order_by: "target_date" } }
    );

    expect(selection.moduleIds).toEqual(["sooner", "later"]);
  });

  it("returns the filtered count before ordering", () => {
    const selection = select([makeModule({ id: "one" }), makeModule({ id: "two" })]);

    expect(selection.totalCount).toBe(2);
    expect(selection.moduleIds).toHaveLength(2);
  });

  it("exports the reserved workspace filter key", () => {
    expect(WORKSPACE_MODULE_FILTER_KEY).toBe("__workspace__");
  });
});
