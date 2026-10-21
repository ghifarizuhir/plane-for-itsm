/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { buildApplicationScopeUrl, resolveWorkspaceApplicationScope } from "./workspace-application-scope";

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

  it("replaces an existing project param", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "layout=list&project=old", "new")).toBe(
      "/itsm/work-items?layout=list&project=new"
    );
  });

  it("collapses duplicate project params to one", () => {
    expect(buildApplicationScopeUrl("/itsm/work-items", "project=old&project=stale", "new")).toBe(
      "/itsm/work-items?project=new"
    );
  });
});

describe("resolveWorkspaceApplicationScope", () => {
  it("trusts the stored scope while projects are not ready", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-2",
        workspaceProjectIds: ["project-1", "project-2"],
        projectsReady: false,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null, shouldClearStored: false });
  });

  it("seeds from a valid param that differs from the stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-2",
        workspaceProjectIds: ["project-1", "project-2"],
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: "project-2", shouldClearStored: false });
  });

  it("does not seed when the param matches the stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-1",
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null, shouldClearStored: false });
  });

  it("ignores an invalid param and keeps a valid stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "missing",
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null, shouldClearStored: false });
  });

  it("clears an invalid stored scope once projects are ready", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "missing",
        paramProjectId: null,
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: null, seedProjectId: null, shouldClearStored: true });
  });

  it("returns no scope when nothing is stored or requested", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: null,
        paramProjectId: null,
        workspaceProjectIds: [],
        projectsReady: true,
      })
    ).toEqual({ projectId: null, seedProjectId: null, shouldClearStored: false });
  });
});
