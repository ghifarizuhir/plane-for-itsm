/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { logger } from "@plane/logger";
// services
import { APIService } from "@/services/api.service";

export class WarRoomService extends APIService {
  

  /**
   * Reuse the REST detail endpoint as the membership gate: 200 = the cookie's
   * user can read the room (guest included), 403/404 = reject the socket.
   */
  async validateAccess({
    workspaceSlug,
    projectId,
    roomId,
    cookie,
  }: {
    workspaceSlug: string;
    projectId: string;
    roomId: string;
    cookie: string;
  }): Promise<boolean> {
    try {
      await this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/war-rooms/${roomId}/`, {
        headers: { Cookie: cookie },
      });
      return true;
    } catch (error) {
      logger.warn("WAR_ROOM_SERVICE: access check failed", error);
      return false;
    }
  }
}
