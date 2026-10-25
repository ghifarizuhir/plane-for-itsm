/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import type { IService } from "@plane/types";
// store
import { ServicesStore } from "./service.store";

const makeService = (id: string, position: IService["position"]): IService =>
  ({
    id,
    workspace_id: "workspace-1",
    project_id: "project-1",
    owner_project_id: "project-1",
    project_links: [],
    access_level: "full",
    name: id,
    description: "",
    description_html: "",
    status: "active",
    criticality: "medium",
    type: "internal",
    kind: "service",
    owner_id: null,
    repository_url: null,
    documentation_url: null,
    position,
    sort_order: 1,
    created_at: "2026-10-25T00:00:00Z",
    updated_at: "2026-10-25T00:00:00Z",
  }) as unknown as IService;

const makeStore = () => {
  const store = new ServicesStore({} as never);
  const serviceService = {
    deleteService: vi.fn(async () => undefined),
    deleteWorkspaceService: vi.fn(async () => undefined),
    createService: vi.fn(async (_slug: string, _workspaceId: string, _projectId: string, _data: Partial<IService>) =>
      makeService("service-3", null)
    ),
    createWorkspaceService: vi.fn(async (_slug: string, _workspaceId: string, _data: Partial<IService>) =>
      makeService("service-3", null)
    ),
    createDependency: vi.fn(async () => ({
      id: "dep-1",
      workspace_id: "workspace-1",
      project_id: null,
      from_service_id: "service-1",
      to_service_id: "service-2",
      dependency_type: "depends_on",
      created_at: "2026-10-25T00:00:00Z",
    })),
    deleteDependency: vi.fn(async () => undefined),
    updateService: vi.fn(
      async (_slug: string, _workspaceId: string, _projectId: string, serviceId: string, data: Partial<IService>) => ({
        ...makeService(serviceId, null),
        ...data,
      })
    ),
    updateNodePosition: vi.fn(async (_slug: string, _workspaceId: string, _projectId: string, serviceId: string) =>
      makeService(serviceId, { x: 5, y: 6 })
    ),
    updateWorkspaceService: vi.fn(async (_slug: string, serviceId: string) => makeService(serviceId, null)),
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

describe("ServicesStore layout reset", () => {
  it("clears positions and bumps the epoch after a dependency is added", async () => {
    const { store } = makeStore();
    store.serviceMap = {
      "service-1": makeService("service-1", { x: 1, y: 2 }),
      "service-2": makeService("service-2", null),
    };

    await store.addDependency("acme", "workspace-1", null, "service-1", "service-2");

    expect(store.serviceMap["service-1"].position).toBeNull();
    expect(store.serviceMap["service-2"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });

  it("clears positions and bumps the epoch after a dependency is removed", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };
    store.dependencyMap = {
      "dep-1": {
        id: "dep-1",
        workspace_id: "workspace-1",
        project_id: null,
        from_service_id: "service-1",
        to_service_id: "service-2",
        dependency_type: "depends_on",
        created_at: "2026-10-25T00:00:00Z",
      },
    };

    await store.removeDependency("acme", "workspace-1", null, "dep-1");

    expect(store.serviceMap["service-1"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });

  it("clears positions after a service is created", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };

    await store.createService("acme", "workspace-1", "project-1", { name: "service-3" });

    expect(store.serviceMap["service-1"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });

  it("clears remaining positions after a service is deleted", async () => {
    const { store } = makeStore();
    store.serviceMap = {
      "service-1": makeService("service-1", { x: 1, y: 2 }),
      "service-2": makeService("service-2", { x: 3, y: 4 }),
    };

    await store.deleteService("acme", "workspace-1", "project-1", "service-1");

    expect(store.serviceMap["service-1"]).toBeUndefined();
    expect(store.serviceMap["service-2"].position).toBeNull();
    expect(store.layoutEpoch).toBe(1);
  });
});

describe("ServicesStore.updateNodePosition", () => {
  it("uses the workspace endpoint when projectId is null", async () => {
    const { store, serviceService } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };

    await store.updateNodePosition("acme", "workspace-1", null, "service-1", null);

    expect(serviceService.updateWorkspaceService).toHaveBeenCalledWith("acme", "service-1", { position: null });
    expect(serviceService.updateNodePosition).not.toHaveBeenCalled();
    expect(store.serviceMap["service-1"].position).toBeNull();
  });

  it("uses the project endpoint when projectId is present", async () => {
    const { store, serviceService } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", { x: 1, y: 2 }) };

    await store.updateNodePosition("acme", "workspace-1", "project-1", "service-1", { x: 5, y: 6 });

    expect(serviceService.updateNodePosition).toHaveBeenCalledWith("acme", "workspace-1", "project-1", "service-1", {
      x: 5,
      y: 6,
    });
    expect(serviceService.updateWorkspaceService).not.toHaveBeenCalled();
  });

  it("restores the original position when the request fails", async () => {
    const { store, serviceService } = makeStore();
    const original = makeService("service-1", { x: 1, y: 2 });
    store.serviceMap = { "service-1": original };
    serviceService.updateWorkspaceService.mockRejectedValueOnce(new Error("nope"));

    await expect(store.updateNodePosition("acme", "workspace-1", null, "service-1", null)).rejects.toThrow("nope");

    expect(store.serviceMap["service-1"]).toStrictEqual(original);
  });
});

describe("ServicesStore.updateService layout", () => {
  it("bumps the layout epoch when the kind changes", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", null) };

    await store.updateService("acme", "workspace-1", "project-1", "service-1", { kind: "database" });

    expect(store.serviceMap["service-1"].kind).toBe("database");
    expect(store.layoutEpoch).toBe(1);
  });

  it("does not bump the layout epoch for other fields", async () => {
    const { store } = makeStore();
    store.serviceMap = { "service-1": makeService("service-1", null) };

    await store.updateService("acme", "workspace-1", "project-1", "service-1", { status: "maintenance" });

    expect(store.serviceMap["service-1"].status).toBe("maintenance");
    expect(store.layoutEpoch).toBe(0);
  });
});
