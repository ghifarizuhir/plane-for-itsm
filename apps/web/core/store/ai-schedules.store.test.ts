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
    await store.fetchSchedules("acme");
    expect(service.list).toHaveBeenCalledWith("acme");
    expect(store.schedules).toHaveLength(1);
    expect(store.loader).toBe(false);
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
    await expect(store.fetchSchedules("acme")).rejects.toThrow("boom");
    expect(store.loader).toBe(false);
  });
});
