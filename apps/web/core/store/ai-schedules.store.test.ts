import { describe, expect, it, vi } from "vitest";
import { AiSchedulesStore } from "./ai-schedules.store";

const makeService = () => ({
  list: vi.fn(async () => [{ id: "s1", name: "Daily", enabled: true }]),
  create: vi.fn(async () => ({ id: "s1" })),
  retrieve: vi.fn(async () => ({ id: "s1", runs: [] })),
  update: vi.fn(async () => undefined),
  remove: vi.fn(async () => undefined),
  runNow: vi.fn(async () => ({ run_id: "r1" })),
});

describe("AiSchedulesStore", () => {
  it("fetches schedules for a workspace", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    await store.fetchSchedules("acme");
    expect(service.list).toHaveBeenCalledWith("acme");
    expect(store.schedules).toHaveLength(1);
    expect(store.loader).toBe(false);
    expect(store.error).toBeNull();
  });

  it("toggles, removes and runs now", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.schedules = [{ id: "s1", enabled: true } as never];
    await store.toggleSchedule("acme", "s1", false);
    expect(service.update).toHaveBeenCalledWith("acme", "s1", false);
    expect(store.schedules[0].enabled).toBe(false);
    await store.runNow("acme", "s1");
    expect(service.runNow).toHaveBeenCalledWith("acme", "s1");
    await store.deleteSchedule("acme", "s1");
    expect(service.remove).toHaveBeenCalledWith("acme", "s1");
    expect(store.schedules).toHaveLength(0);
  });

  it("fetches runs for one schedule", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    await store.fetchRuns("acme", "s1");
    expect(service.retrieve).toHaveBeenCalledWith("acme", "s1");
    expect(store.runsBySchedule["s1"]).toEqual([]);
  });

  it("keeps state usable after a failed fetch", async () => {
    const service = makeService();
    service.list = vi.fn(async () => {
      throw new Error("boom");
    }) as never;
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    await expect(store.fetchSchedules("acme")).rejects.toThrow("boom");
    expect(store.loader).toBe(false);
    expect(store.error).toBe("Could not load schedules.");
  });

  it("stale responses are dropped", async () => {
    const service = makeService();
    let resolveFirst!: (value: unknown) => void;
    const first = new Promise((resolve) => {
      resolveFirst = resolve;
    });
    service.list = vi
      .fn()
      .mockReturnValueOnce(first)
      .mockResolvedValueOnce([{ id: "B" }]) as never;
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    const fetchA = store.fetchSchedules("acme");
    const fetchB = store.fetchSchedules("acme");
    await fetchB;
    resolveFirst([{ id: "A" }]);
    await fetchA;
    expect(store.schedules).toEqual([{ id: "B" }]);
  });

  it("toggle failure keeps local state unchanged", async () => {
    const service = makeService();
    service.update = vi.fn(async () => {
      throw new Error("nope");
    }) as never;
    const store = new AiSchedulesStore(service as never);
    store.schedules = [{ id: "s1", enabled: true } as never];
    await expect(store.toggleSchedule("acme", "s1", false)).rejects.toThrow("nope");
    expect(store.schedules[0].enabled).toBe(true);
  });

  it("delete removes cached runs", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.schedules = [{ id: "s1", enabled: true } as never];
    store.runsBySchedule["s1"] = [{ id: "r1" } as never];
    await store.deleteSchedule("acme", "s1");
    expect(store.runsBySchedule["s1"]).toBeUndefined();
  });

  it("fetchRuns error propagates", async () => {
    const service = makeService();
    service.retrieve = vi.fn(async () => {
      throw new Error("boom");
    }) as never;
    const store = new AiSchedulesStore(service as never);
    await expect(store.fetchRuns("acme", "s1")).rejects.toThrow("boom");
  });

  it("a failed poll keeps the loaded list", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    await store.fetchSchedules("acme");
    service.list = vi.fn(async () => {
      throw new Error("boom");
    }) as never;
    await expect(store.fetchSchedules("acme")).rejects.toThrow("boom");
    expect(store.schedules).toHaveLength(1);
    expect(store.error).toBe("Could not load schedules.");
  });

  it("clears the error when a new fetch starts", async () => {
    const service = makeService();
    service.list = vi.fn(async () => {
      throw new Error("boom");
    }) as never;
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    await expect(store.fetchSchedules("acme")).rejects.toThrow("boom");
    expect(store.error).toBe("Could not load schedules.");
    service.list = vi.fn(() => new Promise(() => {})) as never;
    void store.fetchSchedules("acme");
    expect(store.error).toBeNull();
  });

  it("setWorkspace clears state when the slug changes", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    await store.fetchSchedules("acme");
    store.runsBySchedule["s1"] = [{ id: "r1" } as never];
    store.setWorkspace("other");
    expect(store.schedules).toEqual([]);
    expect(store.runsBySchedule).toEqual({});
    expect(store.error).toBeNull();
    expect(store.loader).toBe(false);
  });

  it("setWorkspace keeps state for the same slug", async () => {
    const service = makeService();
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    await store.fetchSchedules("acme");
    store.setWorkspace("acme");
    expect(store.schedules).toHaveLength(1);
  });

  it("delete invalidates an in-flight list response", async () => {
    const service = makeService();
    let resolveList!: (value: unknown) => void;
    service.list = vi.fn(
      () =>
        new Promise((resolve) => {
          resolveList = resolve;
        })
    ) as never;
    const store = new AiSchedulesStore(service as never);
    store.setWorkspace("acme");
    const pending = store.fetchSchedules("acme");
    store.schedules = [{ id: "s1", enabled: true } as never];
    await store.deleteSchedule("acme", "s1");
    resolveList([{ id: "s1" }, { id: "s2" }]);
    await pending;
    expect(store.schedules).toEqual([]);
    expect(store.loader).toBe(false);
  });

  it("runNow resolves even when the refresh fails", async () => {
    const service = makeService();
    service.retrieve = vi.fn(async () => {
      throw new Error("boom");
    }) as never;
    service.list = vi.fn(async () => {
      throw new Error("boom");
    }) as never;
    const store = new AiSchedulesStore(service as never);
    await expect(store.runNow("acme", "s1")).resolves.toBeUndefined();
  });
});
