/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
// store
import { ServicesStore } from "./service.store";

const makeStore = () => {
  const store = new ServicesStore({} as never);
  const serviceService = {
    deleteService: vi.fn(async () => undefined),
    updateDependency: vi.fn(
      async (
        _slug: string,
        _workspaceId: string,
        _projectId: string | null,
        dependencyId: string,
        dependencyType: string
      ) => ({
        id: dependencyId,
        workspace_id: "workspace-1",
        project_id: null,
        from_service_id: "service-1",
        to_service_id: "service-2",
        dependency_type: dependencyType,
        created_at: "2026-10-20T00:00:00Z",
      })
    ),
  };
  (store as unknown as { serviceService: typeof serviceService }).serviceService = serviceService;
  return { store, serviceService };
};

describe("ServicesStore peek state", () => {
  it("sets, clears, and matches the peeked service", () => {
    const { store } = makeStore();

    expect(store.peekService).toBeUndefined();
    expect(store.getIsServicePeeked("service-1")).toBe(false);

    store.setPeekService({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });

    expect(store.peekService).toEqual({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });
    expect(store.getIsServicePeeked("service-1")).toBe(true);
    expect(store.getIsServicePeeked("service-2")).toBe(false);

    store.setPeekService(undefined);
    expect(store.peekService).toBeUndefined();
  });
});

describe("ServicesStore.deleteService", () => {
  it("clears the peek when the peeked service is deleted", async () => {
    const { store, serviceService } = makeStore();
    store.setPeekService({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });

    await store.deleteService("acme", "workspace-1", "project-1", "service-1");

    expect(serviceService.deleteService).toHaveBeenCalledWith("acme", "workspace-1", "project-1", "service-1");
    expect(store.peekService).toBeUndefined();
  });

  it("keeps the peek when another service is deleted", async () => {
    const { store } = makeStore();
    store.setPeekService({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });

    await store.deleteService("acme", "workspace-1", "project-1", "service-2");

    expect(store.peekService).toEqual({ workspaceSlug: "acme", projectId: "project-1", serviceId: "service-1" });
  });
});

describe("ServicesStore.updateDependency", () => {
  it("updates the dependency type in place", async () => {
    const { store, serviceService } = makeStore();
    store.dependencyMap = {
      "dep-1": {
        id: "dep-1",
        workspace_id: "workspace-1",
        project_id: null,
        from_service_id: "service-1",
        to_service_id: "service-2",
        dependency_type: "depends_on",
        created_at: "2026-10-20T00:00:00Z",
      },
    };

    await store.updateDependency("acme", "workspace-1", null, "dep-1", "runs_on");

    expect(serviceService.updateDependency).toHaveBeenCalledWith("acme", "workspace-1", null, "dep-1", "runs_on");
    expect(store.dependencyMap["dep-1"].dependency_type).toBe("runs_on");
  });
});
