import { describe, expect, it, vi } from "vitest";
import type { IState } from "@plane/types";
import { StateStore } from "./state.store";

const makeState = (id: string, projectId: string): IState => ({
  id,
  color: "#000000",
  default: false,
  description: "",
  group: "backlog",
  name: id,
  project_id: projectId,
  sequence: 1,
  workspace_id: "ws-id",
  order: 0,
});

const makeStore = () => new StateStore({ router: { workspaceSlug: "acme", projectId: "p-1" } } as never);

describe("StateStore.fetchProjectStates", () => {
  it("membuang state project yang sudah tidak dikembalikan server", async () => {
    const store = makeStore();
    store.stateMap = {
      "s-stale": makeState("s-stale", "p-1"),
      "s-kept": makeState("s-kept", "p-1"),
      "s-other": makeState("s-other", "p-2"),
    };
    store.stateService = {
      getStates: vi.fn(async () => [makeState("s-kept", "p-1")]),
    } as never;

    await store.fetchProjectStates("acme", "p-1");

    expect(store.stateMap["s-stale"]).toBeUndefined();
    expect(store.stateMap["s-kept"]).toBeDefined();
    expect(store.stateMap["s-other"]).toBeDefined();
    expect(store.fetchedMap["p-1"]).toBe(true);
  });

  it("meng-upsert state yang dikembalikan server", async () => {
    const store = makeStore();
    store.stateMap = { "s-old": makeState("s-old", "p-1") };
    store.stateService = {
      getStates: vi.fn(async () => [makeState("s-old", "p-1"), makeState("s-new", "p-1")]),
    } as never;

    await store.fetchProjectStates("acme", "p-1");

    expect(store.stateMap["s-old"]).toBeDefined();
    expect(store.stateMap["s-new"]).toBeDefined();
  });

  it("tidak men-prune state yang dibuat saat GET masih in-flight", async () => {
    const store = makeStore();
    store.stateMap = {};
    let resolveGet!: (states: IState[]) => void;
    store.stateService = {
      getStates: vi.fn(() => new Promise<IState[]>((resolve) => (resolveGet = resolve))),
      createState: vi.fn(async () => makeState("s-new", "p-1")),
    } as never;

    const fetchPromise = store.fetchProjectStates("acme", "p-1");
    await store.createState("acme", "p-1", { name: "New" } as Partial<IState>);
    resolveGet([makeState("s-old", "p-1")]);
    await fetchPromise;

    expect(store.stateMap["s-new"]).toBeDefined();
    expect(store.stateMap["s-old"]).toBeDefined();
  });

  it("GET basi tidak menimpa state yang sudah di-update lokal", async () => {
    const store = makeStore();
    store.stateMap = { "s-1": makeState("s-1", "p-1") };
    let resolveGet!: (states: IState[]) => void;
    store.stateService = {
      getStates: vi.fn(() => new Promise<IState[]>((resolve) => (resolveGet = resolve))),
      patchState: vi.fn(async () => makeState("s-1", "p-1")),
    } as never;

    const fetchPromise = store.fetchProjectStates("acme", "p-1");
    await store.updateState("acme", "p-1", "s-1", { name: "Local Name" });
    resolveGet([makeState("s-1", "p-1")]);
    await fetchPromise;

    expect(store.stateMap["s-1"].name).toBe("Local Name");
  });

  it("GET basi tidak menghidupkan kembali state yang dihapus saat in-flight", async () => {
    const store = makeStore();
    store.stateMap = { "s-del": makeState("s-del", "p-1") };
    let resolveGet!: (states: IState[]) => void;
    store.stateService = {
      getStates: vi.fn(() => new Promise<IState[]>((resolve) => (resolveGet = resolve))),
      deleteState: vi.fn(async () => undefined),
    } as never;

    const fetchPromise = store.fetchProjectStates("acme", "p-1");
    await store.deleteState("acme", "p-1", "s-del");
    resolveGet([makeState("s-del", "p-1")]);
    await fetchPromise;

    expect(store.stateMap["s-del"]).toBeUndefined();
  });

  it("delete gagal tidak menandai tombstone permanen", async () => {
    const store = makeStore();
    store.stateMap = { "s-1": makeState("s-1", "p-1") };
    store.stateService = {
      getStates: vi.fn(async () => [makeState("s-1", "p-1")]),
      deleteState: vi.fn(async () => {
        throw new Error("boom");
      }),
    } as never;

    await expect(store.deleteState("acme", "p-1", "s-1")).rejects.toThrow("boom");
    await store.fetchProjectStates("acme", "p-1");

    expect(store.stateMap["s-1"]).toBeDefined();
  });
});

describe("StateStore.getStatePercentageInGroup", () => {
  it("mengabaikan typed mirror state di denominator", () => {
    const store = makeStore();
    store.fetchedMap["p-1"] = true;
    store.stateMap = {
      "s-1": { ...makeState("s-1", "p-1"), sequence: 1, order: 1 },
      "s-2": { ...makeState("s-2", "p-1"), sequence: 2, order: 2 },
      "s-mirror": { ...makeState("s-mirror", "p-1"), sequence: 3, order: 3, type_id: "type-1" },
    };

    expect(store.getStatePercentageInGroup("s-1")).toBe(50);
    expect(store.getStatePercentageInGroup("s-2")).toBe(100);
    // typed mirror states are filtered out of the legacy list → no position
    expect(store.getStatePercentageInGroup("s-mirror")).toBeUndefined();
  });
});
