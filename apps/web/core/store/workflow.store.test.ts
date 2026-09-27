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
