/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, it, expect } from "vitest";
import { parseWarRoomHandshake } from "@/lib/war-room-auth";

describe("parseWarRoomHandshake", () => {
  const roomId = "11111111-1111-1111-1111-111111111111";

  it("extracts userId from token and cookie from headers", () => {
    const result = parseWarRoomHandshake({
      headers: { cookie: "session=abc" },
      params: { roomId },
      query: { workspaceSlug: "acme", projectId: "p-1", token: JSON.stringify({ id: "u-1" }) },
    });
    expect(result).toEqual({
      userId: "u-1",
      cookie: "session=abc",
      roomId,
      workspaceSlug: "acme",
      projectId: "p-1",
    });
  });

  it("falls back to a cookie embedded in the token", () => {
    const result = parseWarRoomHandshake({
      headers: {},
      params: { roomId },
      query: {
        workspaceSlug: "acme",
        projectId: "p-1",
        token: JSON.stringify({ id: "u-1", cookie: "session=token-cookie" }),
      },
    });
    expect(result?.cookie).toBe("session=token-cookie");
  });

  it("rejects missing query params and malformed tokens", () => {
    expect(
      parseWarRoomHandshake({
        headers: { cookie: "session=abc" },
        params: { roomId },
        query: { projectId: "p-1", token: JSON.stringify({ id: "u-1" }) },
      })
    ).toBeNull();
    expect(
      parseWarRoomHandshake({
        headers: { cookie: "session=abc" },
        params: { roomId },
        query: { workspaceSlug: "acme", projectId: "p-1", token: "not-json" },
      })
    ).toBeNull();
  });

  it("rejects when no cookie is available at all", () => {
    expect(
      parseWarRoomHandshake({
        headers: {},
        params: { roomId },
        query: { workspaceSlug: "acme", projectId: "p-1", token: JSON.stringify({ id: "u-1" }) },
      })
    ).toBeNull();
  });
});
