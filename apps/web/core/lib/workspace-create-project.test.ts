/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { resolveWorkspaceCreateProject } from "./workspace-create-project";

describe("resolveWorkspaceCreateProject", () => {
  it("uses the scoped application immediately", () => {
    expect(resolveWorkspaceCreateProject("project-1", null)).toBe("project-1");
  });

  it("uses the picked application when the scope is All", () => {
    expect(resolveWorkspaceCreateProject(null, "project-2")).toBe("project-2");
  });

  it("prefers the scope over a stale pick", () => {
    expect(resolveWorkspaceCreateProject("project-1", "project-2")).toBe("project-1");
  });

  it("returns null when neither is available", () => {
    expect(resolveWorkspaceCreateProject(null, null)).toBeNull();
  });
});
