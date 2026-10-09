/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// helpers
import {
  appendNoteToHtml,
  formatElapsed,
  getBlastRadius,
  getWarRoomIncidentLink,
  isActiveWarRoomStatus,
  isTypingActive,
  parseMessageSegments,
  serializeMentionTokens,
  severityFromPriority,
  shouldShowMessageHeader,
  upsertMessageInList,
} from "./war-room.helpers";
import type { IWarRoomMessage, IServiceDependency } from "@plane/types";

describe("severityFromPriority", () => {
  it("maps every priority to the backend severity default", () => {
    expect(severityFromPriority("urgent")).toBe("sev1");
    expect(severityFromPriority("high")).toBe("sev2");
    expect(severityFromPriority("medium")).toBe("sev3");
    expect(severityFromPriority("low")).toBe("sev4");
    expect(severityFromPriority("none")).toBe("sev4");
  });

  it("returns null for empty or unknown priorities", () => {
    expect(severityFromPriority(null)).toBeNull();
    expect(severityFromPriority(undefined)).toBeNull();
    expect(severityFromPriority("")).toBeNull();
    expect(severityFromPriority("critical")).toBeNull();
  });
});

describe("isActiveWarRoomStatus", () => {
  it("treats active and monitoring as active", () => {
    expect(isActiveWarRoomStatus("active")).toBe(true);
    expect(isActiveWarRoomStatus("monitoring")).toBe(true);
  });

  it("treats resolved and archived as inactive", () => {
    expect(isActiveWarRoomStatus("resolved")).toBe(false);
    expect(isActiveWarRoomStatus("archived")).toBe(false);
  });
});

describe("formatElapsed", () => {
  const startedAt = "2026-01-01T00:00:00.000Z";

  it("formats a running timer with the injected now", () => {
    const now = new Date("2026-01-01T01:02:03.000Z").getTime();
    expect(formatElapsed(startedAt, null, now)).toBe("01:02:03");
  });

  it("uses resolved_at when the room is closed", () => {
    const resolvedAt = "2026-01-01T00:10:00.000Z";
    const now = new Date("2026-01-02T00:00:00.000Z").getTime();
    expect(formatElapsed(startedAt, resolvedAt, now)).toBe("00:10:00");
  });

  it("never returns negative time", () => {
    const now = new Date("2025-12-31T23:59:00.000Z").getTime();
    expect(formatElapsed(startedAt, null, now)).toBe("00:00:00");
  });
});

describe("getWarRoomIncidentLink", () => {
  it("links to the browse work item route", () => {
    expect(getWarRoomIncidentLink("acme", "PROJ-12")).toBe("/acme/browse/PROJ-12/");
  });
});

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

const makeDependency = (from: string, to: string): IServiceDependency =>
  ({
    id: `${from}-${to}`,
    workspace_id: "ws-1",
    project_id: "project-1",
    from_service_id: from,
    to_service_id: to,
    created_at: "2026-01-01T00:00:00.000Z",
  }) as IServiceDependency;

describe("getBlastRadius", () => {
  it("includes affected services plus both directions of one hop", () => {
    const dependencies = [makeDependency("a", "b"), makeDependency("c", "a"), makeDependency("b", "x")];
    const result = getBlastRadius(["a"], dependencies);
    expect(result.affectedIds).toEqual(["a"]);
    // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread copies the array
    expect([...result.neighborIds].sort()).toEqual(["b", "c"]);
    // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread copies the array
    expect([...result.includedIds].sort()).toEqual(["a", "b", "c"]);
  });

  it("does not mark an affected service as its own neighbor", () => {
    const result = getBlastRadius(["a", "b"], [makeDependency("a", "b")]);
    expect(result.neighborIds).toEqual([]);
    // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread copies the array
    expect([...result.includedIds].sort()).toEqual(["a", "b"]);
  });
});

describe("upsertMessageInList", () => {
  it("appends a message that is not present", () => {
    const result = upsertMessageInList([makeMessage()], makeMessage({ id: "message-2" }));
    expect(result.map((message) => message.id)).toEqual(["message-1", "message-2"]);
  });

  it("replaces by id", () => {
    const result = upsertMessageInList([makeMessage()], makeMessage({ body: "edited" }));
    expect(result).toHaveLength(1);
    expect(result[0].body).toBe("edited");
  });

  it("reconciles an optimistic message by client_id", () => {
    const optimistic = makeMessage({ id: "optimistic-1", client_id: "client-1" });
    const result = upsertMessageInList([optimistic], makeMessage({ id: "server-1", client_id: "client-1" }));
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("server-1");
  });

  it("dedupes a websocket echo by id", () => {
    const result = upsertMessageInList([makeMessage()], makeMessage());
    expect(result).toHaveLength(1);
  });
});

describe("appendNoteToHtml", () => {
  it("appends an escaped paragraph", () => {
    expect(appendNoteToHtml("<p>existing</p>", "rolled back & verified")).toBe(
      "<p>existing</p><p>rolled back &amp; verified</p>"
    );
  });

  it("converts newlines to breaks and returns the original when blank", () => {
    expect(appendNoteToHtml("", "line 1\nline 2")).toBe("<p>line 1<br>line 2</p>");
    expect(appendNoteToHtml("<p>x</p>", "   ")).toBe("<p>x</p>");
  });
});

describe("serializeMentionTokens", () => {
  it("replaces display names with uuid tokens", () => {
    const body = "@Ada Lovelace please check @Grace Hopper";
    const mentions = [
      { id: "uuid-ada", display_name: "Ada Lovelace" },
      { id: "uuid-grace", display_name: "Grace Hopper" },
    ];
    expect(serializeMentionTokens(body, mentions)).toBe("@{uuid-ada} please check @{uuid-grace}");
  });

  it("prefers the longest display name when names overlap", () => {
    const body = "@Ada Lovelace and @Ada";
    const mentions = [
      { id: "uuid-short", display_name: "Ada" },
      { id: "uuid-long", display_name: "Ada Lovelace" },
    ];
    expect(serializeMentionTokens(body, mentions)).toBe("@{uuid-long} and @{uuid-short}");
  });
});

describe("parseMessageSegments", () => {
  it("splits text and mention tokens", () => {
    const segments = parseMessageSegments("ping @{123e4567-e89b-12d3-a456-426614174000} now");
    expect(segments).toEqual([
      { type: "text", value: "ping " },
      { type: "mention", user_id: "123e4567-e89b-12d3-a456-426614174000" },
      { type: "text", value: " now" },
    ]);
  });

  it("returns a single text segment when there are no tokens", () => {
    expect(parseMessageSegments("plain body")).toEqual([{ type: "text", value: "plain body" }]);
  });
});

describe("shouldShowMessageHeader", () => {
  it("shows a header for the first message and on author change", () => {
    const first = makeMessage();
    expect(shouldShowMessageHeader(undefined, first)).toBe(true);
    expect(shouldShowMessageHeader(first, makeMessage({ id: "m2", author_id: "user-2" }))).toBe(true);
  });

  it("groups consecutive messages by the same author within five minutes", () => {
    const first = makeMessage();
    const sameGroup = makeMessage({ id: "m2", created_at: "2026-01-01T00:04:00.000Z" });
    const newGroup = makeMessage({ id: "m3", created_at: "2026-01-01T00:06:00.000Z" });
    expect(shouldShowMessageHeader(first, sameGroup)).toBe(false);
    expect(shouldShowMessageHeader(first, newGroup)).toBe(true);
  });
});

describe("isTypingActive", () => {
  it("is true only before the expiry timestamp", () => {
    expect(isTypingActive(1500, 1000)).toBe(true);
    expect(isTypingActive(1000, 1000)).toBe(false);
    expect(isTypingActive(undefined, 1000)).toBe(false);
    expect(isTypingActive(0, 1000)).toBe(false);
  });
});
