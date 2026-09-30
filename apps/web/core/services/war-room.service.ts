/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IWarRoom,
  IWarRoomListItem,
  IWarRoomSummary,
  TWarRoomCreatePayload,
  TWarRoomListParams,
} from "@plane/types";
// services
import { APIService } from "@/services/api.service";

type TWarRoomErrorBody = { detail?: string; error?: string; war_room_id?: string; [key: string]: unknown };

/**
 * Normalizes axios errors into a real Error carrying `.detail`, `.error`, and
 * `.war_room_id` (the 409 duplicate payload used by the create modal banner).
 */
const toWarRoomError = (error: unknown): Error => {
  const body = (error as { response?: { data?: TWarRoomErrorBody } })?.response?.data;
  let fieldMessage: string | undefined;
  if (body && typeof body === "object") {
    const first = Object.values(body).find((value) => typeof value === "string");
    fieldMessage = typeof first === "string" ? first : undefined;
  }
  const message = body?.detail ?? body?.error ?? fieldMessage ?? "Something went wrong. Please try again.";
  const normalized = new Error(message) as Error & { detail?: string; error?: string; war_room_id?: string };
  normalized.detail = body?.detail;
  normalized.error = body?.error;
  normalized.war_room_id = body?.war_room_id;
  return normalized;
};

export class WarRoomService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  private basePath(workspaceSlug: string, projectId: string): string {
    return `/api/workspaces/${workspaceSlug}/projects/${projectId}/war-rooms`;
  }

  async getWarRooms(
    workspaceSlug: string,
    projectId: string,
    params?: TWarRoomListParams
  ): Promise<IWarRoomListItem[]> {
    return this.get(`${this.basePath(workspaceSlug, projectId)}/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async getWarRoomSummary(workspaceSlug: string, projectId: string): Promise<IWarRoomSummary> {
    return this.get(`${this.basePath(workspaceSlug, projectId)}/summary/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async getWarRoom(workspaceSlug: string, projectId: string, warRoomId: string): Promise<IWarRoom> {
    return this.get(`${this.basePath(workspaceSlug, projectId)}/${warRoomId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async createWarRoom(workspaceSlug: string, projectId: string, data: TWarRoomCreatePayload): Promise<IWarRoom> {
    return this.post(`${this.basePath(workspaceSlug, projectId)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }
}
