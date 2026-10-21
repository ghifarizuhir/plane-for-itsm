/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { buildApplicationScopeUrl } from "./workspace-application-scope";

describe("buildApplicationScopeUrl", () => {
  it("adds the project param and preserves existing params", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "layout=list", "project-1")).toBe(
      "/itsm/work-items?layout=list&project=project-1"
    );
  });

  it("removes the project param for All", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "layout=list&project=project-1", null)).toBe(
      "/itsm/work-items?layout=list"
    );
  });

  it("drops the query string when nothing remains", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "project=project-1", null)).toBe("/itsm/work-items");
  });
});
