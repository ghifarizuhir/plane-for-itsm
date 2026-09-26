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
});
