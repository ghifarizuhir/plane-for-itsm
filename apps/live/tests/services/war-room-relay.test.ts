/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

process.env.API_BASE_URL = "http://localhost:8000";
process.env.LIVE_SERVER_SECRET_KEY = "test-secret";

const { WarRoomRelay } = await import("@/services/war-room-relay.service");

type FakeSocket = {
  readyState: number;
  sent: string[];
  send: (message: string) => void;
};

const fakeSocket = (): FakeSocket => ({
  readyState: 1,
  sent: [],
  send(message: string) {
    this.sent.push(message);
  },
});

describe("WarRoomRelay", () => {
  it("fans out Redis payloads to sockets of the matching room only", () => {
    const relay = new WarRoomRelay();
    const a = fakeSocket();
    const b = fakeSocket();
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    relay.join("room-2", b as never, { userId: "u-2", name: "Two" }, false);

    relay.handleRedisPayload(JSON.stringify({ room_id: "room-1", kind: "message.created", data: { body: "hi" } }));

    expect(a.sent).toHaveLength(1);
    expect(JSON.parse(a.sent[0])).toEqual({ kind: "message.created", data: { body: "hi" } });
    expect(b.sent).toHaveLength(0);
  });

  it("ignores malformed payloads", () => {
    const relay = new WarRoomRelay();
    const a = fakeSocket();
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    expect(() => relay.handleRedisPayload("not-json")).not.toThrow();
    expect(a.sent).toHaveLength(0);
  });

  it("join and leave are idempotent and drop empty rooms", () => {
    const relay = new WarRoomRelay();
    const a = fakeSocket();
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    expect(relay.roomSize("room-1")).toBe(1);
    relay.leave("room-1", a as never);
    relay.leave("room-1", a as never);
    expect(relay.roomSize("room-1")).toBe(0);
  });

  it("skips sockets that are not open", () => {
    const relay = new WarRoomRelay();
    const closed = fakeSocket();
    closed.readyState = 3;
    relay.join("room-1", closed as never, { userId: "u-1", name: "One" }, false);
    relay.handleRedisPayload(JSON.stringify({ room_id: "room-1", kind: "typing", data: {} }));
    expect(closed.sent).toHaveLength(0);
  });
});
