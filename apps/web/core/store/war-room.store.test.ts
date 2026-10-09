/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it, vi } from "vitest";
import type {
  IWarRoom,
  IWarRoomEvent,
  IWarRoomListItem,
  IWarRoomMessage,
  IWarRoomParticipant,
  IWarRoomRunbookItem,
  TIssue,
} from "@plane/types";
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

const makeIssue = (overrides: Partial<TIssue> = {}): TIssue =>
  ({
    id: "issue-1",
    name: "Checkout latency spike",
    priority: "urgent",
    type_id: "type-1",
    project_id: "project-1",
    sequence_id: 102,
    sort_order: 0,
    state_id: null,
    label_ids: [],
    assignee_ids: [],
    estimate_point: null,
    sub_issues_count: 0,
    attachment_count: 0,
    link_count: 0,
    parent_id: null,
    cycle_id: null,
    module_ids: [],
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    start_date: null,
    target_date: null,
    completed_at: null,
    archived_at: null,
    created_by: "user-1",
    updated_by: "user-1",
    is_draft: false,
    ...overrides,
  }) as TIssue;

const makeDetail = (overrides: Partial<IWarRoom> = {}): IWarRoom =>
  ({
    ...makeRoom(),
    issues: [],
    participants: [makeParticipant()],
    runbook_items: [makeRunbookItem()],
    counts: { messages: 0 },
    ...overrides,
  }) as unknown as IWarRoom;

const makeParticipant = (overrides: Partial<IWarRoomParticipant> = {}): IWarRoomParticipant =>
  ({
    id: "participant-1",
    member_id: "user-1",
    role: "commander",
    joined_at: "2026-01-01T00:00:00.000Z",
    display_name: "User One",
    avatar_url: null,
    ...overrides,
  }) as IWarRoomParticipant;

const makeRunbookItem = (overrides: Partial<IWarRoomRunbookItem> = {}): IWarRoomRunbookItem =>
  ({
    id: "item-1",
    title: "Triage",
    sort_order: 65535,
    is_done: false,
    done_by_id: null,
    done_at: null,
    template_key: "triage",
    ...overrides,
  }) as IWarRoomRunbookItem;

const makeMessage = (overrides: Partial<IWarRoomMessage> = {}): IWarRoomMessage =>
  ({
    id: "message-1",
    war_room_id: "room-1",
    author_id: "user-1",
    author: null,
    body: "hello",
    mentions: [],
    edited_at: null,
    created_at: "2026-01-01T00:00:00.000Z",
    client_id: null,
    ...overrides,
  }) as IWarRoomMessage;

const makeEvent = (overrides: Partial<IWarRoomEvent> = {}): IWarRoomEvent =>
  ({
    id: "event-1",
    actor_id: "user-1",
    event_type: "room.status_changed",
    payload: { from: "active", to: "resolved" },
    created_at: "2026-01-01T00:00:00.000Z",
    ...overrides,
  }) as IWarRoomEvent;

const makeStore = () => {
  const store = new WarRoomStore({} as never);
  const refetchIssues = vi.fn();
  const warRoomService = {
    getWarRoom: vi.fn(async () => makeDetail()),
    createWarRoom: vi.fn(async () => makeDetail({ id: "room-2", sequence_id: 2 })),
    updateWarRoom: vi.fn(async () => makeDetail({ name: "Updated room" })),
    deleteWarRoom: vi.fn(async () => undefined),
    addServices: vi.fn(async () => ({ linked: 1 })),
    removeService: vi.fn(async () => undefined),
    addIssues: vi.fn(async () => ({ linked: 1 })),
    removeIssue: vi.fn(async () => undefined),
    createParticipant: vi.fn(async () => makeParticipant({ id: "participant-2", member_id: "user-2" })),
    updateParticipant: vi.fn(async () => makeParticipant({ id: "participant-1", role: "comms" })),
    deleteParticipant: vi.fn(async () => undefined),
    createRunbookItem: vi.fn(async () => makeRunbookItem({ id: "item-2" })),
    updateRunbookItem: vi.fn(async () => makeRunbookItem({ id: "item-1", is_done: true })),
    deleteRunbookItem: vi.fn(async () => undefined),
    getMessages: vi.fn(async () => [makeMessage()]),
    createMessage: vi.fn(async () => makeMessage()),
    updateMessage: vi.fn(async () => makeMessage({ body: "edited" })),
    deleteMessage: vi.fn(async () => undefined),
    getEvents: vi.fn(async () => [makeEvent()]),
  };
  (store as unknown as { warRoomService: typeof warRoomService }).warRoomService = warRoomService;
  (store as unknown as { rootStore: unknown }).rootStore = {
    issue: { projectIssues: { fetchIssuesWithExistingPagination: refetchIssues } },
  };
  return { store, warRoomService, refetchIssues };
};

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
  it("stores the created detail", async () => {
    const { store, warRoomService } = makeStore();

    const room = await store.createWarRoom("acme", "project-1", { primary_issue_id: "issue-2" });

    expect(warRoomService.createWarRoom).toHaveBeenCalledWith("acme", "project-1", { primary_issue_id: "issue-2" });
    expect(room.id).toBe("room-2");
    expect(store.getWarRoomDetailById("room-2")?.sequence_id).toBe(2);
  });
});

describe("WarRoomStore messages", () => {
  it("replaces the first page and tracks hasMore", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getMessages.mockResolvedValueOnce([makeMessage()]);

    await store.fetchMessages("acme", "project-1", "room-1");

    expect(store.getMessages("room-1").map((message) => message.id)).toEqual(["message-1"]);
    expect(store.hasMoreMessages("room-1")).toBe(false);
  });

  it("prepends older pages without duplicating ids", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getMessages.mockResolvedValueOnce([makeMessage({ id: "m2" })]);
    await store.fetchMessages("acme", "project-1", "room-1");

    warRoomService.getMessages.mockResolvedValueOnce([makeMessage({ id: "m0" }), makeMessage({ id: "m1" })]);
    await store.fetchMessages("acme", "project-1", "room-1", { before_id: "m2" });

    expect(store.getMessages("room-1").map((message) => message.id)).toEqual(["m0", "m1", "m2"]);
  });

  it("reconciles the optimistic message by client_id", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.createMessage.mockResolvedValueOnce(makeMessage({ id: "server-1", client_id: "client-1" }));

    await store.sendMessage("acme", "project-1", "room-1", "hello", "client-1");

    expect(store.getMessages("room-1")).toHaveLength(1);
    expect(store.getMessages("room-1")[0].id).toBe("server-1");
  });

  it("removes the optimistic message when the send fails", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.createMessage.mockRejectedValueOnce(new Error("boom"));

    await expect(store.sendMessage("acme", "project-1", "room-1", "hello", "client-1")).rejects.toThrow("boom");
    expect(store.getMessages("room-1")).toHaveLength(0);
  });

  it("updates and deletes messages", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchMessages("acme", "project-1", "room-1");
    warRoomService.updateMessage.mockResolvedValueOnce(makeMessage({ body: "edited" }));

    await store.updateMessage("acme", "project-1", "room-1", "message-1", "edited");
    expect(store.getMessages("room-1")[0].body).toBe("edited");

    await store.deleteMessage("acme", "project-1", "room-1", "message-1");
    expect(store.getMessages("room-1")).toHaveLength(0);
  });
});

describe("WarRoomStore events", () => {
  it("replaces the first page and appends older pages", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.getEvents.mockResolvedValueOnce([makeEvent({ id: "e2" })]);
    await store.fetchEvents("acme", "project-1", "room-1");

    warRoomService.getEvents.mockResolvedValueOnce([makeEvent({ id: "e0" }), makeEvent({ id: "e1" })]);
    await store.fetchEvents("acme", "project-1", "room-1", { before_id: "e2" });

    expect(store.getEvents("room-1").map((event) => event.id)).toEqual(["e2", "e0", "e1"]);
  });
});

describe("WarRoomStore.applySocketEvent", () => {
  it("dedupes message.created and applies update/delete", () => {
    const { store } = makeStore();

    store.applySocketEvent("acme", "project-1", "room-1", { kind: "message.created", data: makeMessage() });
    store.applySocketEvent("acme", "project-1", "room-1", { kind: "message.created", data: makeMessage() });
    expect(store.getMessages("room-1")).toHaveLength(1);

    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "message.updated",
      data: makeMessage({ body: "edited" }),
    });
    expect(store.getMessages("room-1")[0].body).toBe("edited");

    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "message.deleted",
      data: { id: "message-1", war_room_id: "room-1" },
    });
    expect(store.getMessages("room-1")).toHaveLength(0);
  });

  it("prepends activity.created exactly once", () => {
    const { store } = makeStore();
    store.applySocketEvent("acme", "project-1", "room-1", { kind: "activity.created", data: makeEvent() });
    store.applySocketEvent("acme", "project-1", "room-1", { kind: "activity.created", data: makeEvent() });
    expect(store.getEvents("room-1")).toHaveLength(1);
  });

  it("refetches detail on room.changed and drops state on deleted", async () => {
    const { store, warRoomService } = makeStore();
    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "room.changed",
      data: { reasons: ["notes"] },
    });
    await vi.waitFor(() => expect(warRoomService.getWarRoom).toHaveBeenCalled());

    await store.fetchWarRoomDetail("acme", "project-1", "room-1");
    store.applySocketEvent("acme", "project-1", "room-1", {
      kind: "room.changed",
      data: { reasons: ["deleted"] },
    });
    expect(store.getWarRoomDetailById("room-1")).toBeNull();
  });

  it("tracks typing expiry, presence and unread", () => {
    const { store } = makeStore();
    store.applyTyping("room-1", "user-2", true);
    expect(store.getTypingUserIds("room-1", Date.now())).toContain("user-2");
    store.applyTyping("room-1", "user-2", false);
    expect(store.getTypingUserIds("room-1", Date.now())).toEqual([]);

    store.applyPresence("room-1", "user-2", true);
    store.applyPresence("room-1", "user-3", true);
    store.applyPresence("room-1", "user-2", false);
    expect(store.getOnlineUserIds("room-1")).toEqual(["user-3"]);

    store.incrementUnread("room-1");
    store.incrementUnread("room-1");
    expect(store.getUnreadCount("room-1")).toBe(2);
    store.clearUnread("room-1");
    expect(store.getUnreadCount("room-1")).toBe(0);
  });
});

describe("WarRoomStore room mutations", () => {
  it("updates the stored detail", async () => {
    const { store, warRoomService } = makeStore();
    warRoomService.updateWarRoom.mockResolvedValueOnce(makeDetail({ name: "Updated room", status: "resolved" }));

    await store.updateWarRoom("acme", "project-1", "room-1", { status: "resolved" });

    expect(store.getWarRoomDetailById("room-1")?.name).toBe("Updated room");
    expect(store.getWarRoomDetailById("room-1")?.status).toBe("resolved");
  });

  it("applies runbook mutations locally", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRoomDetail("acme", "project-1", "room-1");

    await store.createRunbookItem("acme", "project-1", "room-1", "New step");
    expect(store.getWarRoomDetailById("room-1")?.runbook_items.map((item) => item.id)).toContain("item-2");

    warRoomService.updateRunbookItem.mockResolvedValueOnce(makeRunbookItem({ id: "item-1", is_done: true }));
    await store.updateRunbookItem("acme", "project-1", "room-1", "item-1", { is_done: true });
    expect(store.getWarRoomDetailById("room-1")?.runbook_items.find((item) => item.id === "item-1")?.is_done).toBe(
      true
    );

    await store.deleteRunbookItem("acme", "project-1", "room-1", "item-2");
    expect(store.getWarRoomDetailById("room-1")?.runbook_items.map((item) => item.id)).toEqual(["item-1"]);
  });

  it("applies participant mutations and demotes other commanders", async () => {
    const { store, warRoomService } = makeStore();
    await store.fetchWarRoomDetail("acme", "project-1", "room-1");

    warRoomService.createParticipant.mockResolvedValueOnce(
      makeParticipant({ id: "participant-2", member_id: "user-2", role: "commander" })
    );
    await store.addParticipant("acme", "project-1", "room-1", { member_id: "user-2", role: "commander" });

    const participants = store.getWarRoomDetailById("room-1")?.participants ?? [];
    expect(participants.find((p) => p.id === "participant-2")?.role).toBe("commander");
    expect(participants.find((p) => p.id === "participant-1")?.role).toBe("responder");
  });
});

describe("WarRoomStore.toggleWarRoom", () => {
  it("resolves the active room when toggled off", async () => {
    const { store, warRoomService, refetchIssues } = makeStore();

    await store.toggleWarRoom(
      "acme",
      "project-1",
      makeIssue({ war_room_id: "room-1", war_room_status: "active", war_room_severity: "sev1" })
    );

    expect(warRoomService.updateWarRoom).toHaveBeenCalledWith("acme", "project-1", "room-1", { status: "resolved" });
    expect(warRoomService.createWarRoom).not.toHaveBeenCalled();
    expect(refetchIssues).toHaveBeenCalledWith("acme", "project-1", "mutation");
  });

  it("reopens the resolved room when toggled on", async () => {
    const { store, warRoomService } = makeStore();

    await store.toggleWarRoom(
      "acme",
      "project-1",
      makeIssue({ war_room_id: "room-1", war_room_status: "resolved", war_room_severity: "sev2" })
    );

    expect(warRoomService.updateWarRoom).toHaveBeenCalledWith("acme", "project-1", "room-1", { status: "active" });
  });

  it("creates a room when the issue has none", async () => {
    const { store, warRoomService } = makeStore();

    await store.toggleWarRoom("acme", "project-1", makeIssue({ war_room_id: null }), { serviceIds: ["service-1"] });

    expect(warRoomService.createWarRoom).toHaveBeenCalledWith("acme", "project-1", {
      primary_issue_id: "issue-1",
      name: "Checkout latency spike",
      severity: "sev1",
      service_ids: ["service-1"],
    });
  });

  it("adopts the existing room on a 409 duplicate", async () => {
    const { store, warRoomService } = makeStore();
    const duplicate = Object.assign(new Error("active_war_room_exists"), {
      error: "active_war_room_exists",
      war_room_id: "room-9",
    });
    warRoomService.createWarRoom.mockRejectedValueOnce(duplicate);

    await store.toggleWarRoom("acme", "project-1", makeIssue({ war_room_id: null }));

    expect(warRoomService.getWarRoom).toHaveBeenCalledWith("acme", "project-1", "room-9");
  });
});
