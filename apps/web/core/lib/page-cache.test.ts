import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearPageCache, ensureFreshPageCache } from "./page-cache";

const deleteDatabase = vi.fn();

const makeStorage = () => {
  const data = new Map<string, string>();
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => {
      data.set(key, value);
    },
    removeItem: (key: string) => {
      data.delete(key);
    },
  };
};

describe("page-cache", () => {
  beforeEach(() => {
    deleteDatabase.mockReset();
    deleteDatabase.mockImplementation(() => {
      const request: { onsuccess?: () => void; onerror?: () => void; onblocked?: () => void } = {};
      setTimeout(() => request.onsuccess?.(), 0);
      return request;
    });
    vi.stubGlobal("indexedDB", { deleteDatabase });
    vi.stubGlobal("localStorage", makeStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("deletes the cache when the revision changed", async () => {
    await ensureFreshPageCache("pg1", "rev-2");
    expect(deleteDatabase).toHaveBeenCalledWith("pg1");
    expect(localStorage.getItem("page-sync-rev:pg1")).toBe("rev-2");
  });

  it("keeps the cache when the revision matches", async () => {
    localStorage.setItem("page-sync-rev:pg1", "rev-1");
    await ensureFreshPageCache("pg1", "rev-1");
    expect(deleteDatabase).not.toHaveBeenCalled();
  });

  it("no-ops safely without indexedDB", async () => {
    vi.stubGlobal("indexedDB", undefined);
    await expect(clearPageCache("pg1")).resolves.toBe(false);
  });

  it("stores the revision only when the purge succeeds", async () => {
    await ensureFreshPageCache("pg1", "rev-2");
    expect(localStorage.getItem("page-sync-rev:pg1")).toBe("rev-2");
  });

  it("does not store the revision when the purge is blocked", async () => {
    deleteDatabase.mockImplementation(() => {
      const request: { onsuccess?: () => void; onerror?: () => void; onblocked?: () => void } = {};
      setTimeout(() => request.onblocked?.(), 0);
      return request;
    });
    await ensureFreshPageCache("pg1", "rev-2");
    expect(localStorage.getItem("page-sync-rev:pg1")).toBeNull();
  });

  it("normalizes Date revisions to ISO strings", async () => {
    await ensureFreshPageCache("pg1", new Date("2026-10-09T06:49:01.000Z"));
    expect(localStorage.getItem("page-sync-rev:pg1")).toBe("2026-10-09T06:49:01.000Z");
  });

  it("no-ops on falsy pageId or revision", async () => {
    await ensureFreshPageCache("", "rev-1");
    await ensureFreshPageCache("pg1", null);
    await ensureFreshPageCache("pg1", undefined);
    expect(deleteDatabase).not.toHaveBeenCalled();
  });
});
