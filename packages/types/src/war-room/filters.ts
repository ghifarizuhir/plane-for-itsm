/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWarRoomStatusTab = "active" | "resolved" | "all";

export type TWarRoomListParams = {
  status?: string;
  q?: string;
};
