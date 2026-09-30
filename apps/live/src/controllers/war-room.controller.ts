/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Request } from "express";
import type { RawData, WebSocket } from "ws";
// plane imports
import { Controller, WebSocket as WSDecorator } from "@plane/decorators";
import { logger } from "@plane/logger";
// helpers
import { handleAuthentication } from "@/lib/auth";
import { parseWarRoomHandshake } from "@/lib/war-room-auth";
// services
import { warRoomRelay } from "@/services/war-room-relay.service";
import { WarRoomService } from "@/services/war-room.service";
// types
import type { TWarRoomClientMessage } from "@/types";

@Controller("/war-rooms")
export class WarRoomController {
  [key: string]: unknown;

  @WSDecorator("/:roomId")
  handleConnection(ws: WebSocket, req: Request) {
    void this.upgrade(ws, req).catch((error) => {
      logger.error("WAR_ROOM_CONTROLLER: connection rejected", error);
      try {
        ws.close(4403, "Unauthorized");
      } catch {
        // socket already closed
      }
    });
  }

  private async upgrade(ws: WebSocket, req: Request) {
    const handshake = parseWarRoomHandshake({
      headers: req.headers,
      params: req.params,
      query: req.query,
    });
    if (!handshake) {
      ws.close(4403, "Unauthorized");
      return;
    }
    const auth = await handleAuthentication({
      cookie: handshake.cookie,
      userId: handshake.userId,
    });
    const warRoomService = new WarRoomService();
    const allowed = await warRoomService.validateAccess({
      workspaceSlug: handshake.workspaceSlug,
      projectId: handshake.projectId,
      roomId: handshake.roomId,
      cookie: handshake.cookie,
    });
    if (!allowed) {
      ws.close(4403, "Forbidden");
      return;
    }

    warRoomRelay.join(handshake.roomId, ws, { userId: auth.user.id, name: auth.user.name });
    ws.on("message", (raw: RawData) => this.handleClientMessage(handshake.roomId, auth.user.id, ws, raw));
    ws.on("close", () => warRoomRelay.leave(handshake.roomId, ws));
    ws.on("error", (error: Error) => {
      logger.error("WAR_ROOM_CONTROLLER: socket error", error);
      warRoomRelay.leave(handshake.roomId, ws);
    });
  }

  private handleClientMessage(roomId: string, userId: string, ws: WebSocket, raw: RawData) {
    let parsed: TWarRoomClientMessage;
    try {
      parsed = JSON.parse(raw.toString()) as TWarRoomClientMessage;
    } catch {
      return;
    }
    if (parsed.type === "typing") {
      void warRoomRelay.publish("typing", roomId, {
        user_id: userId,
        is_typing: parsed.is_typing !== false,
      });
    }
    // App-level heartbeat (client sends every 30s); answer directly, no Redis.
    if (parsed.type === "ping" && ws.readyState === 1) {
      ws.send(JSON.stringify({ kind: "pong" }));
    }
  }
}
