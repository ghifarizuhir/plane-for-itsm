import { describe, expect, it, vi } from "vitest";
import type { TWorkflowMap } from "@plane/types";
import { WorkflowStore } from "./workflow.store";

const makeStore = () => {
  const fetchProjectStates = vi.fn(async () => [] as never[]);
  const rootStore = { state: { fetchProjectStates } };
  const store = new WorkflowStore(rootStore as never);
  const service = {
    importWorkItemTypes: vi.fn(async () => undefined),
    unlinkWorkItemType: vi.fn(async () => undefined),
    getWorkflowMap: vi.fn(async (): Promise<TWorkflowMap> => ({ types: [] })),
  };
  (store as unknown as { service: typeof service }).service = service;
  return { store, fetchProjectStates, service };
};

describe("WorkflowStore.importWorkItemTypes", () => {
  it("refresh state project setelah import sukses", async () => {
    const { store, fetchProjectStates, service } = makeStore();

    await store.importWorkItemTypes("acme", "p-1", ["type-1"]);

    expect(service.importWorkItemTypes).toHaveBeenCalledWith("acme", "p-1", ["type-1"]);
    expect(fetchProjectStates).toHaveBeenCalledWith("acme", "p-1");
  });

  it("tidak gagal saat refresh state gagal", async () => {
    const { store, fetchProjectStates } = makeStore();
    fetchProjectStates.mockRejectedValueOnce(new Error("boom"));

    await expect(store.importWorkItemTypes("acme", "p-1", ["type-1"])).resolves.toBeUndefined();
    expect(fetchProjectStates).toHaveBeenCalledWith("acme", "p-1");
  });
});

describe("WorkflowStore.unlinkWorkItemType", () => {
  it("refresh state project setelah unlink sukses", async () => {
    const { store, fetchProjectStates, service } = makeStore();

    await store.unlinkWorkItemType("acme", "p-1", "type-1");

    expect(service.unlinkWorkItemType).toHaveBeenCalledWith("acme", "p-1", "type-1");
    expect(fetchProjectStates).toHaveBeenCalledWith("acme", "p-1");
  });

  it("tidak gagal saat refresh state gagal", async () => {
    const { store, fetchProjectStates } = makeStore();
    fetchProjectStates.mockRejectedValueOnce(new Error("boom"));

    await expect(store.unlinkWorkItemType("acme", "p-1", "type-1")).resolves.toBeUndefined();
    expect(fetchProjectStates).toHaveBeenCalledWith("acme", "p-1");
  });
});

describe("WorkflowStore.map refresh failures", () => {
  it("mencatat kegagalan refresh map tanpa menggagalkan mutasi", async () => {
    const { store, service } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    service.getWorkflowMap.mockRejectedValueOnce(new Error("boom"));

    await expect(store.importWorkItemTypes("acme", "p-1", ["type-1"])).resolves.toBeUndefined();
    expect(store.mapRefreshError["p-1"]).toBe("refresh_failed");
  });

  it("membersihkan flag setelah refresh sukses", async () => {
    const { store } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    store.mapRefreshError["p-1"] = "refresh_failed";

    await store.importWorkItemTypes("acme", "p-1", ["type-1"]);
    expect(store.mapRefreshError["p-1"]).toBeNull();
  });

  it("refresh basi yang gagal tidak menimpa flag refresh sukses yang lebih baru", async () => {
    const { store, service } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    const deferreds: { resolve: (map: TWorkflowMap) => void; reject: (error: Error) => void }[] = [];
    service.getWorkflowMap.mockImplementation(
      () =>
        new Promise<TWorkflowMap>((resolve, reject) => {
          deferreds.push({ resolve, reject });
        })
    );

    const first = store.importWorkItemTypes("acme", "p-1", ["type-1"]);
    const second = store.importWorkItemTypes("acme", "p-1", ["type-1"]);
    await vi.waitFor(() => expect(deferreds.length).toBe(2));
    // newer refresh (second) succeeds first; the older one fails afterwards
    deferreds[1].resolve({ types: [] });
    await second;
    deferreds[0].reject(new Error("boom"));
    await first;

    expect(store.mapRefreshError["p-1"]).toBeNull();
  });

  it("refresh gagal terbaru tetap menandai flag walau refresh lama sukses", async () => {
    const { store, service } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    const deferreds: { resolve: (map: TWorkflowMap) => void; reject: (error: Error) => void }[] = [];
    service.getWorkflowMap.mockImplementation(
      () =>
        new Promise<TWorkflowMap>((resolve, reject) => {
          deferreds.push({ resolve, reject });
        })
    );

    const first = store.importWorkItemTypes("acme", "p-1", ["type-1"]);
    const second = store.importWorkItemTypes("acme", "p-1", ["type-1"]);
    await vi.waitFor(() => expect(deferreds.length).toBe(2));
    deferreds[0].resolve({ types: [] });
    await first;
    deferreds[1].reject(new Error("boom"));
    await second;

    expect(store.mapRefreshError["p-1"]).toBe("refresh_failed");
  });

  it("kegagalan refresh daftar state tidak menelan warning map", async () => {
    const { store, service, fetchProjectStates } = makeStore();
    store.workflowMap["p-1"] = { types: [] };
    fetchProjectStates.mockRejectedValueOnce(new Error("states down"));
    service.getWorkflowMap.mockRejectedValueOnce(new Error("map down"));

    await expect(store.importWorkItemTypes("acme", "p-1", ["type-1"])).resolves.toBeUndefined();
    expect(store.mapRefreshError["p-1"]).toBe("refresh_failed");
  });
});
