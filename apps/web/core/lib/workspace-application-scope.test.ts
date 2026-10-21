/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  buildApplicationScopeUrl,
  resolveWorkspaceApplicationScope,
  shouldHandleApplicationScopeParam,
} from "./workspace-application-scope";

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
    ).toEqual({ projectId: "project-1", seedProjectId: null });
  });

  it("trusts the stored scope while the project list is unavailable", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-2",
        workspaceProjectIds: undefined,
        projectsReady: false,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null });
  });

  it("trusts the stored scope when ids are unavailable even if readiness is inconsistent", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-2",
        workspaceProjectIds: undefined,
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null });
  });

  it("seeds from a valid param that differs from the stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-2",
        workspaceProjectIds: ["project-1", "project-2"],
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: "project-2" });
  });

  it("does not seed when the param matches the stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "project-1",
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null });
  });

  it("ignores an invalid param when a valid stored scope exists", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "project-1",
        paramProjectId: "missing",
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: "project-1", seedProjectId: null });
  });

  it("defers a deep-link seed until the project list is available", () => {
    const input = {
      storedProjectId: null,
      paramProjectId: "project-1",
      projectsReady: true,
    };
    expect(resolveWorkspaceApplicationScope({ ...input, workspaceProjectIds: [] })).toEqual({
      projectId: null,
      seedProjectId: null,
    });
    expect(resolveWorkspaceApplicationScope({ ...input, workspaceProjectIds: ["project-1"] })).toEqual({
      projectId: null,
      seedProjectId: "project-1",
    });
  });

  it("seeds a valid param while ignoring an invalid stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "missing",
        paramProjectId: "project-1",
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: null, seedProjectId: "project-1" });
  });

  it("returns no scope for an invalid stored scope", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "missing",
        paramProjectId: null,
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: null, seedProjectId: null });
  });

  it("treats empty strings as null", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: "",
        paramProjectId: "",
        workspaceProjectIds: ["project-1"],
        projectsReady: true,
      })
    ).toEqual({ projectId: null, seedProjectId: null });
  });

  it("returns no scope when nothing is stored or requested", () => {
    expect(
      resolveWorkspaceApplicationScope({
        storedProjectId: null,
        paramProjectId: null,
        workspaceProjectIds: [],
        projectsReady: true,
      })
    ).toEqual({ projectId: null, seedProjectId: null });
  });
});

describe("shouldHandleApplicationScopeParam", () => {
  it("does not handle while projects are not ready", () => {
    expect(
      shouldHandleApplicationScopeParam({
        lastHandledWorkspaceSlug: undefined,
        lastHandledProjectParam: undefined,
        workspaceSlug: "workspace-1",
        projectParam: "project-1",
        projectsReady: false,
      })
    ).toBe(false);
  });

  it("does not handle without a workspace slug", () => {
    expect(
      shouldHandleApplicationScopeParam({
        lastHandledWorkspaceSlug: undefined,
        lastHandledProjectParam: undefined,
        workspaceSlug: undefined,
        projectParam: "project-1",
        projectsReady: true,
      })
    ).toBe(false);
  });

  it("does not handle the same workspace and param twice", () => {
    expect(
      shouldHandleApplicationScopeParam({
        lastHandledWorkspaceSlug: "workspace-1",
        lastHandledProjectParam: "project-1",
        workspaceSlug: "workspace-1",
        projectParam: "project-1",
        projectsReady: true,
      })
    ).toBe(false);
  });

  it("handles a changed param in the same workspace", () => {
    expect(
      shouldHandleApplicationScopeParam({
        lastHandledWorkspaceSlug: "workspace-1",
        lastHandledProjectParam: "project-1",
        workspaceSlug: "workspace-1",
        projectParam: "project-2",
        projectsReady: true,
      })
    ).toBe(true);
  });

  it("handles the same param after switching workspaces", () => {
    expect(
      shouldHandleApplicationScopeParam({
        lastHandledWorkspaceSlug: "workspace-1",
        lastHandledProjectParam: "project-1",
        workspaceSlug: "workspace-2",
        projectParam: "project-1",
        projectsReady: true,
      })
    ).toBe(true);
  });
});
