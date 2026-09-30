/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import type { IWarRoom, IWarRoomListItem, IWarRoomSummary } from "@plane/types";
// store
import { WarRoomStore } from "./war-room.store";

const makeRoom = (overrides: Partial<IWarRoomListItem> = {}): IWarRoomListItem =>
  ({
    id: "room-1",
    workspace_id: "ws-1",
    project_id: "project-1",
    sequence_id: 1,
    name: "Room 1",
    description_html: "<p></p>",
    notes_html: "",
    severity: "sev3",
    status: "active",
    primary_issue_id: "issue-1",
    started_at: "2026-01-01T00:00:00.000Z",
    resolved_at: null,
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    created_by: "user-1",
    primary_issue: null,
    services: [],
    participants: [],
    service_count: 0,
    participant_count: 0,
    message_count: 0,
    last_activity_at: null,
    ...overrides,
  }) as IWarRoomListItem;

const makeDetail = (overrides: Partial<IWarRoom> = {}): IWarRoom =>
  ({
    ...makeRoom(),
    issues: [],
    runbook_items: [],
    counts: { messages: 0 },
    ...overrides,
  }) as unknown as IWarRoom;

const makeStore = () => {
  const store = new WarRoomStore({} as never);
  const warRoomService = {
    getWarRooms: vi.fn(async () => [makeRoom()]),
    getWarRoomSummary: vi.fn(async (): Promise<IWarRoomSummary> => ({ active: 1, sev1_2: 0, resolved_7d: 0 })),
    getWarRoom: vi.fn(async () => makeDetail()),
    createWarRoom: vi.fn(async () => makeDetail({ id: "room-2", sequence_id: 2 })),
  };
  (store as unknown as { warRoomService: typeof warRoomService }).warRoomService = warRoomService;
  return { store, warRoomService };
};

describe("WarRoomStore.fetchWarRooms", () => {
  it("stores list items, order, and fetched flag", async () => {
    const { store, warRoomService } = makeStore();

    await store.fetchWarRooms("acme", "project-1", { status: "active,monitoring" });

    expect(warRoomService.getWarRooms).toHaveBeenCalledWith("acme", "project-1", { status: "active,monitoring" });
    expect(store.getProjectWarRoomIds("project-1")).toEqual(["room-1"]);
    expect(store.getWarRoomById("room-1")?.name).toBe("Room 1");
    expect(store.fetchedMap["project-1"]).toBe(true);
    expect(store.errorMap["project-1"]).toBe(false);
  });

  it("flags errors and returns undefined", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getWarRooms.mockRejectedValueOnce(new Error("boom"));

    const result = await store.fetchWarRooms("acme", "project-1");

    expect(result).toBeUndefined();
    expect(store.errorMap["project-1"]).toBe(true);
    expect(store.loader).toBe(false);
  });
});

describe("WarRoomStore.getActiveWarRoomByIssue", () => {
  it("matches only active or monitoring rooms for the incident", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getWarRooms.mockResolvedValueOnce([
      makeRoom({ id: "room-resolved", status: "resolved", primary_issue_id: "issue-1" }),
      makeRoom({ id: "room-monitoring", status: "monitoring", primary_issue_id: "issue-1" }),
      makeRoom({ id: "room-other", status: "active", primary_issue_id: "issue-2" }),
    ]);

    await store.fetchWarRooms("acme", "project-1");

    expect(store.getActiveWarRoomByIssue("project-1", "issue-1")?.id).toBe("room-monitoring");
    expect(store.getActiveWarRoomByIssue("project-1", "issue-2")?.id).toBe("room-other");
    expect(store.getActiveWarRoomByIssue("project-1", "issue-3")).toBeNull();
  });
});

describe("WarRoomStore.fetchWarRoomSummary", () => {
  it("stores the summary per project", async () => {
    const { store } = makeStore();

    await store.fetchWarRoomSummary("acme", "project-1");

    expect(store.getProjectSummary("project-1")).toEqual({ active: 1, sev1_2: 0, resolved_7d: 0 });
  });
});

describe("WarRoomStore.fetchWarRoomDetail", () => {
  it("stores detail and flags failures per room", async () => {
    const { store, warRoomService } = makeStore();

    await store.fetchWarRoomDetail("acme", "project-1", "room-1");
    expect(store.getWarRoomDetailById("room-1")?.counts.messages).toBe(0);
    expect(store.detailErrorMap["room-1"]).toBe(false);

    warRoomService.getWarRoom.mockRejectedValueOnce(new Error("boom"));
    const result = await store.fetchWarRoomDetail("acme", "project-1", "room-missing");
    expect(result).toBeUndefined();
    expect(store.detailErrorMap["room-missing"]).toBe(true);
  });
});

describe("WarRoomStore.createWarRoom", () => {
  it("stores the created detail and prepends its id", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRooms("acme", "project-1");

    const room = await store.createWarRoom("acme", "project-1", { primary_issue_id: "issue-2" });

    expect(warRoomService.createWarRoom).toHaveBeenCalledWith("acme", "project-1", { primary_issue_id: "issue-2" });
    expect(room.id).toBe("room-2");
    expect(store.getWarRoomDetailById("room-2")?.sequence_id).toBe(2);
    expect(store.getProjectWarRoomIds("project-1")).toEqual(["room-2", "room-1"]);
  });
});
