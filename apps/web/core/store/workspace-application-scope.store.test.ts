/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY,
  WorkspaceApplicationScopeStore,
} from "./workspace-application-scope.store";

const createStorage = () => {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
    removeItem: (key: string) => void data.delete(key),
  };
};

describe("WorkspaceApplicationScopeStore", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("defaults to All and persists one scope per workspace", () => {
    const storage = createStorage();
    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBeNull();

    store.setProjectId("ws-1", "project-1");
    store.setProjectId("ws-2", "project-2");

    expect(store.getProjectId("ws-1")).toBe("project-1");
    expect(store.getProjectId("ws-2")).toBe("project-2");
    expect(JSON.parse(storage.data.get(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY) ?? "{}")).toEqual({
      "ws-1": "project-1",
      "ws-2": "project-2",
    });
  });

  it("hydrates a persisted scope on construction", () => {
    const storage = createStorage();
    storage.setItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY, JSON.stringify({ "ws-1": "project-1" }));

    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBe("project-1");
    expect(store.getProjectId("ws-2")).toBeNull();
  });

  it("resets one workspace without touching others and can clear everything", () => {
    const storage = createStorage();
    const store = new WorkspaceApplicationScopeStore(storage);
    store.setProjectId("ws-1", "project-1");
    store.setProjectId("ws-2", "project-2");

    store.reset("ws-1");
    expect(store.getProjectId("ws-1")).toBeNull();
    expect(store.getProjectId("ws-2")).toBe("project-2");

    store.reset();
    expect(store.getProjectId("ws-2")).toBeNull();
    expect(JSON.parse(storage.data.get(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY) ?? "{}")).toEqual({});
  });

  it("ignores malformed storage", () => {
    const storage = createStorage();
    storage.setItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY, "not-json");

    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBeNull();
  });

  it("falls back to an in-memory scope when localStorage access throws", () => {
    vi.stubGlobal("window", {
      get localStorage() {
        throw new Error("SecurityError");
      },
    });

    const store = new WorkspaceApplicationScopeStore();

    expect(store.getProjectId("ws-1")).toBeNull();
    store.setProjectId("ws-1", "project-1");
    expect(store.getProjectId("ws-1")).toBe("project-1");
  });

  it("keeps working in memory when storage operations throw", () => {
    const storage = {
      getItem: () => {
        throw new Error("SecurityError");
      },
      setItem: () => {
        throw new Error("QuotaExceededError");
      },
      removeItem: () => {
        throw new Error("SecurityError");
      },
    };

    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.getProjectId("ws-1")).toBeNull();
    expect(() => store.setProjectId("ws-1", "project-1")).not.toThrow();
    expect(store.getProjectId("ws-1")).toBe("project-1");
    expect(() => store.reset()).not.toThrow();
    expect(store.getProjectId("ws-1")).toBeNull();
  });

  it("ignores non-string non-null persisted values", () => {
    const storage = createStorage();
    storage.setItem(
      WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY,
      JSON.stringify({ "ws-1": 42, "ws-2": "project-2", "ws-3": null })
    );

    const store = new WorkspaceApplicationScopeStore(storage);

    expect(store.projectIdByWorkspace).toEqual({ "ws-2": "project-2", "ws-3": null });
  });

  it("removes the storage key on global reset", () => {
    const storage = createStorage();
    const store = new WorkspaceApplicationScopeStore(storage);
    store.setProjectId("ws-1", "project-1");

    store.reset();

    expect(store.getProjectId("ws-1")).toBeNull();
    expect(storage.getItem(WORKSPACE_APPLICATION_SCOPE_STORAGE_KEY)).toBeNull();
  });

  it("does not clear every workspace when reset receives an empty slug", () => {
    const storage = createStorage();
    const store = new WorkspaceApplicationScopeStore(storage);
    store.setProjectId("ws-1", "project-1");

    store.reset("");

    expect(store.getProjectId("ws-1")).toBe("project-1");
  });
});
