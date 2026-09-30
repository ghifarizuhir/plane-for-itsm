/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWarRoomRelayMeta = {
  userId: string;
  name: string;
};

export type TWarRoomRedisPayload = {
  room_id: string;
  kind: string;
  data: unknown;
};

export type TWarRoomClientMessage = { type: "typing"; is_typing?: boolean } | { type: "ping" };

export type TWarRoomHandshake = {
  userId: string;
  cookie: string;
  roomId: string;
  workspaceSlug: string;
  projectId: string;
};
