/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TNotificationData, TNotificationWarRoom } from "@plane/types";

export const isWarRoomNotification = (
  data: TNotificationData | undefined
): data is TNotificationData & { war_room: TNotificationWarRoom } =>
  typeof data?.war_room?.id === "string" && data.war_room.id.length > 0;
