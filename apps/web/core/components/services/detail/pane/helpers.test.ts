/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import {
  DEFAULT_SERVICE_DETAIL_PANE_TAB,
  isServiceDetailPaneTab,
  resolveServiceDetailPaneTab,
  SERVICE_DETAIL_PANE_TAB_KEYS,
} from "./helpers";

describe("service detail pane helpers", () => {
  it("lists the approved tabs in order", () => {
    expect(SERVICE_DETAIL_PANE_TAB_KEYS).toEqual(["properties", "relationships", "tickets"]);
  });

  it("accepts valid tab keys", () => {
    expect(isServiceDetailPaneTab("properties")).toBe(true);
    expect(isServiceDetailPaneTab("relationships")).toBe(true);
    expect(isServiceDetailPaneTab("tickets")).toBe(true);
  });

  it("rejects missing, empty, and unknown values", () => {
    expect(isServiceDetailPaneTab(null)).toBe(false);
    expect(isServiceDetailPaneTab(undefined)).toBe(false);
    expect(isServiceDetailPaneTab("")).toBe(false);
    expect(isServiceDetailPaneTab("outline")).toBe(false);
  });

  it("resolves invalid values to closed", () => {
    expect(resolveServiceDetailPaneTab("outline")).toBeNull();
    expect(resolveServiceDetailPaneTab(null)).toBeNull();
    expect(resolveServiceDetailPaneTab(undefined)).toBeNull();
  });

  it("defaults to properties when opening", () => {
    expect(DEFAULT_SERVICE_DETAIL_PANE_TAB).toBe("properties");
  });
});
