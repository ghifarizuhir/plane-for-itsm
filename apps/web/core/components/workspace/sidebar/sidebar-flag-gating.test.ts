/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// plane imports
import { WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS } from "@plane/constants";
// local imports
import { isNavItemVisibleByFlag } from "./sidebar-flag-gating";

describe("workspace sidebar flag gating", () => {
  it("maps sprints/tracks/kb/intake to their workspace feature flags", () => {
    expect(WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["sprints"].flag).toBe("cycle_view");
    expect(WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["tracks"].flag).toBe("module_view");
    expect(WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["kb"].flag).toBe("page_view");
    expect(WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["intake"].flag).toBe("intake_view");
  });

  it("keeps items without a flag always visible", () => {
    expect(isNavItemVisibleByFlag({ flag: undefined }, null)).toBe(true);
    expect(isNavItemVisibleByFlag({ flag: undefined }, { cycle_view: false } as never)).toBe(true);
  });

  it("hides flagged items when the flag is off and shows them otherwise", () => {
    const settings = {
      cycle_view: false,
      module_view: true,
      page_view: true,
      intake_view: true,
    };
    expect(isNavItemVisibleByFlag({ flag: "cycle_view" }, settings)).toBe(false);
    expect(isNavItemVisibleByFlag({ flag: "module_view" }, settings)).toBe(true);
  });

  it("fails open when settings are missing", () => {
    expect(isNavItemVisibleByFlag({ flag: "cycle_view" }, null)).toBe(true);
    expect(isNavItemVisibleByFlag({ flag: "cycle_view" }, undefined)).toBe(true);
  });
});
