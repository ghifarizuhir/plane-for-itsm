/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
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
});
