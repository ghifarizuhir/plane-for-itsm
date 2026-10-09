/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { API_BASE_URL } from "@plane/constants";
import type {
  IWarRoom,
  IWarRoomEvent,
  IWarRoomMessage,
  IWarRoomParticipant,
  IWarRoomRunbookItem,
  TWarRoomCreatePayload,
  TWarRoomEventsParams,
  TWarRoomLinkIssuesPayload,
  TWarRoomLinkResponse,
  TWarRoomLinkServicesPayload,
  TWarRoomMessageCreatePayload,
  TWarRoomMessageUpdatePayload,
  TWarRoomMessagesParams,
  TWarRoomParticipantCreatePayload,
  TWarRoomParticipantUpdatePayload,
  TWarRoomRunbookCreatePayload,
  TWarRoomRunbookUpdatePayload,
  TWarRoomUpdatePayload,
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

  private roomPath(workspaceSlug: string, projectId: string, warRoomId: string): string {
    return `${this.basePath(workspaceSlug, projectId)}/${warRoomId}`;
  }

  async getWarRoom(workspaceSlug: string, projectId: string, warRoomId: string): Promise<IWarRoom> {
    return this.get(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/`)
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

  async updateWarRoom(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomUpdatePayload
  ): Promise<IWarRoom> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteWarRoom(workspaceSlug: string, projectId: string, warRoomId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async addServices(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomLinkServicesPayload
  ): Promise<TWarRoomLinkResponse> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/services/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async removeService(workspaceSlug: string, projectId: string, warRoomId: string, serviceId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/services/${serviceId}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async addIssues(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomLinkIssuesPayload
  ): Promise<TWarRoomLinkResponse> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/issues/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async removeIssue(workspaceSlug: string, projectId: string, warRoomId: string, issueId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/issues/${issueId}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async createParticipant(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomParticipantCreatePayload
  ): Promise<IWarRoomParticipant> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/participants/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateParticipant(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string,
    data: TWarRoomParticipantUpdatePayload
  ): Promise<IWarRoomParticipant> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/participants/${participantId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteParticipant(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    participantId: string
  ): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/participants/${participantId}/`).catch(
      (error) => {
        throw toWarRoomError(error);
      }
    );
  }

  async createRunbookItem(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomRunbookCreatePayload
  ): Promise<IWarRoomRunbookItem> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/runbook-items/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateRunbookItem(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    itemId: string,
    data: TWarRoomRunbookUpdatePayload
  ): Promise<IWarRoomRunbookItem> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/runbook-items/${itemId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteRunbookItem(workspaceSlug: string, projectId: string, warRoomId: string, itemId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/runbook-items/${itemId}/`).catch(
      (error) => {
        throw toWarRoomError(error);
      }
    );
  }

  async getMessages(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomMessagesParams
  ): Promise<IWarRoomMessage[]> {
    return this.get(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async createMessage(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    data: TWarRoomMessageCreatePayload
  ): Promise<IWarRoomMessage> {
    return this.post(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async updateMessage(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    messageId: string,
    data: TWarRoomMessageUpdatePayload
  ): Promise<IWarRoomMessage> {
    return this.patch(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/${messageId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }

  async deleteMessage(workspaceSlug: string, projectId: string, warRoomId: string, messageId: string): Promise<void> {
    await this.delete(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/messages/${messageId}/`).catch((error) => {
      throw toWarRoomError(error);
    });
  }

  async getEvents(
    workspaceSlug: string,
    projectId: string,
    warRoomId: string,
    params?: TWarRoomEventsParams
  ): Promise<IWarRoomEvent[]> {
    return this.get(`${this.roomPath(workspaceSlug, projectId, warRoomId)}/events/`, { params })
      .then((response) => response?.data)
      .catch((error) => {
        throw toWarRoomError(error);
      });
  }
}
