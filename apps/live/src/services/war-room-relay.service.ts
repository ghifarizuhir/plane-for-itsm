/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type Redis from "ioredis";
import type { WebSocket } from "ws";
// plane imports
import { logger } from "@plane/logger";
// helpers
import { redisManager } from "@/redis";
// types
import type { TWarRoomRedisPayload, TWarRoomRelayMeta } from "@/types";

export const WAR_ROOM_CHANNEL = "war-room:events";

type RelaySocket = {
  ws: WebSocket;
  meta: TWarRoomRelayMeta;
};

/**
 * In-memory room registry + Redis bridge. api-rs publishes `{room_id, kind,
 * data}`; every live instance relays to its local sockets. Typing/presence
 * publish through the same channel so multi-instance fan-out stays uniform.
 */
export class WarRoomRelay {
  private rooms = new Map<string, RelaySocket[]>();
  private subscriber: Redis | null = null;

  async initialize(): Promise<void> {
    if (this.subscriber) return;
    const base = redisManager.getClient();
    if (!base) {
      logger.warn("WAR_ROOM_RELAY: Redis unavailable, relay disabled");
      return;
    }
    const subscriber = base.duplicate();
    await new Promise<void>((resolve, reject) => {
      subscriber.subscribe(WAR_ROOM_CHANNEL, (error) => {
        if (error) reject(error);
        else resolve();
      });
    });
    subscriber.on("message", (channel: string, message: string) => {
      if (channel !== WAR_ROOM_CHANNEL) return;
      this.handleRedisPayload(message);
    });
    this.subscriber = subscriber;
    logger.info(`WAR_ROOM_RELAY: subscribed to ${WAR_ROOM_CHANNEL}`);
  }

  async destroy(): Promise<void> {
    if (!this.subscriber) return;
    const subscriber = this.subscriber;
    this.subscriber = null;
    try {
      await subscriber.quit();
    } catch (error) {
      logger.error("WAR_ROOM_RELAY: error quitting subscriber", error);
      subscriber.disconnect();
    }
    this.rooms.clear();
  }

  join(roomId: string, ws: WebSocket, meta: TWarRoomRelayMeta, announce = true): void {
    let sockets = this.rooms.get(roomId);
    if (!sockets) {
      sockets = [];
      this.rooms.set(roomId, sockets);
    }
    if (sockets.some((entry) => entry.ws === ws)) return;
    sockets.push({ ws, meta });
    if (announce) {
      void this.publish("presence.joined", roomId, { user_id: meta.userId, name: meta.name });
    }
  }

  leave(roomId: string, ws: WebSocket, announce = true): void {
    const sockets = this.rooms.get(roomId);
    if (!sockets) return;
    const index = sockets.findIndex((entry) => entry.ws === ws);
    if (index === -1) return;
    const [entry] = sockets.splice(index, 1);
    if (sockets.length === 0) this.rooms.delete(roomId);
    if (announce) {
      void this.publish("presence.left", roomId, {
        user_id: entry.meta.userId,
        name: entry.meta.name,
      });
    }
  }

  roomSize(roomId: string): number {
    return this.rooms.get(roomId)?.length ?? 0;
  }

  handleRedisPayload(message: string): void {
    try {
      const payload = JSON.parse(message) as TWarRoomRedisPayload;
      if (!payload?.room_id || !payload.kind) return;
      this.fanOut(payload.room_id, { kind: payload.kind, data: payload.data });
    } catch (error) {
      logger.error("WAR_ROOM_RELAY: malformed Redis payload", error);
    }
  }

  fanOut(roomId: string, event: { kind: string; data: unknown }): void {
    const sockets = this.rooms.get(roomId);
    if (!sockets || sockets.length === 0) return;
    const message = JSON.stringify(event);
    for (const { ws } of sockets) {
      if (ws.readyState !== 1) continue;
      try {
        ws.send(message);
      } catch (error) {
        logger.error("WAR_ROOM_RELAY: send failed", error);
      }
    }
  }

  async publish(kind: string, roomId: string, data: unknown): Promise<void> {
    const client = redisManager.getClient();
    if (!client) return;
    try {
      await client.publish(WAR_ROOM_CHANNEL, JSON.stringify({ room_id: roomId, kind, data }));
    } catch (error) {
      logger.error("WAR_ROOM_RELAY: publish failed", error);
    }
  }
}

export const warRoomRelay = new WarRoomRelay();
