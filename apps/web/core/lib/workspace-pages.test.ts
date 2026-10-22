/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { TPage } from "@plane/types";
import { selectWorkspacePageIds } from "./workspace-pages";

const makePage = (overrides: Partial<TPage>): TPage =>
  ({
    id: "page",
    name: "Page",
    project_ids: ["p1"],
    access: 0,
    archived_at: null,
    is_favorite: false,
    created_at: new Date("2026-10-01T00:00:00.000Z"),
    updated_at: new Date("2026-10-01T00:00:00.000Z"),
    created_by: "u1",
    owned_by: "u1",
    ...overrides,
  }) as TPage;

const select = (pages: TPage[], overrides: Partial<Parameters<typeof selectWorkspacePageIds>[0]> = {}) =>
  selectWorkspacePageIds({
    pages,
    projectIds: ["p1"],
    pageType: "public",
    searchQuery: "",
    sortKey: "updated_at",
    sortBy: "desc",
    ...overrides,
  });

describe("selectWorkspacePageIds", () => {
  it("includes a page when any linked project is in scope", () => {
    const selection = select([
      makePage({ id: "one", project_ids: ["p1"], updated_at: new Date("2026-10-02T00:00:00.000Z") }),
      makePage({ id: "shared", project_ids: ["p2", "p1"], updated_at: new Date("2026-10-01T00:00:00.000Z") }),
      makePage({ id: "other", project_ids: ["p2"] }),
    ]);

    expect(selection).toEqual(["one", "shared"]);
  });

  it("filters by public, private, and archived tab", () => {
    const pages = [
      makePage({ id: "public", access: 0 }),
      makePage({ id: "private", access: 1 }),
      makePage({ id: "archived", access: 0, archived_at: "2026-10-02" }),
    ];

    expect(select(pages, { pageType: "public" })).toEqual(["public"]);
    expect(select(pages, { pageType: "private" })).toEqual(["private"]);
    expect(select(pages, { pageType: "archived" })).toEqual(["archived"]);
  });

  it("searches page names case-insensitively and treats untitled pages as Untitled", () => {
    const selection = select(
      [
        makePage({ id: "sop", name: "SOP" }),
        makePage({ id: "untitled", name: "" }),
        makePage({ id: "other", name: "Runbook" }),
      ],
      { searchQuery: "sop" }
    );

    expect(selection).toEqual(["sop"]);
  });

  it("applies favorites and created-by filters", () => {
    const pages = [
      makePage({ id: "match", is_favorite: true, owned_by: "u1" }),
      makePage({ id: "not-favorite", is_favorite: false, owned_by: "u1" }),
      makePage({ id: "other-owner", is_favorite: true, owned_by: "u2" }),
    ];

    expect(select(pages, { filters: { favorites: true, created_by: ["u1"] } })).toEqual(["match"]);
  });

  it("applies created-at date filters", () => {
    const selection = select(
      [
        makePage({ id: "later", created_at: new Date("2026-10-26T00:00:00.000Z") }),
        makePage({ id: "sooner", created_at: new Date("2026-10-20T00:00:00.000Z") }),
      ],
      { filters: { created_at: ["2026-10-26;after"] } }
    );

    expect(selection).toEqual(["later"]);
  });

  it("orders by updated date descending", () => {
    const selection = select([
      makePage({ id: "older", updated_at: new Date("2026-10-01T00:00:00.000Z") }),
      makePage({ id: "newer", updated_at: new Date("2026-10-20T00:00:00.000Z") }),
    ]);

    expect(selection).toEqual(["newer", "older"]);
  });

  it("returns an empty list when no page is in scope", () => {
    expect(select([makePage({ id: "other", project_ids: ["p2"] })])).toEqual([]);
  });
});
