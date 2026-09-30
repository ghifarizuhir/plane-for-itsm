/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWarRoomStatus = "active" | "monitoring" | "resolved" | "archived";

export type TWarRoomSeverity = "sev1" | "sev2" | "sev3" | "sev4";

export type TWarRoomParticipantRole = "commander" | "comms" | "scribe" | "responder";

export interface IWarRoomPrimaryIssue {
  id: string;
  identifier: string;
  name: string;
  priority: string | null;
  state_group: string | null;
}

export interface IWarRoomService {
  id: string;
  name: string;
  status: string;
}

export interface IWarRoomLinkedIssue {
  id: string;
  identifier: string;
  name: string;
  priority: string | null;
}

export interface IWarRoomParticipant {
  id: string;
  member_id: string;
  role: TWarRoomParticipantRole;
  joined_at: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface IWarRoomRunbookItem {
  id: string;
  title: string;
  sort_order: number;
  is_done: boolean;
  done_by_id: string | null;
  done_at: string | null;
  template_key: string | null;
}

export interface IWarRoomBase {
  id: string;
  workspace_id: string;
  project_id: string;
  sequence_id: number;
  name: string;
  description_html: string;
  notes_html: string;
  severity: TWarRoomSeverity;
  status: TWarRoomStatus;
  primary_issue_id: string;
  started_at: string;
  resolved_at: string | null;
  created_at: string;
  updated_at: string;
  created_by: string | null;
}

export interface IWarRoomListItem extends IWarRoomBase {
  primary_issue: IWarRoomPrimaryIssue | null;
  services: IWarRoomService[];
  participants: IWarRoomParticipant[];
  service_count: number;
  participant_count: number;
  message_count: number;
  last_activity_at: string | null;
}

export interface IWarRoom extends IWarRoomBase {
  primary_issue: IWarRoomPrimaryIssue | null;
  services: IWarRoomService[];
  issues: IWarRoomLinkedIssue[];
  participants: IWarRoomParticipant[];
  runbook_items: IWarRoomRunbookItem[];
  counts: {
    messages: number;
  };
}

export interface IWarRoomSummary {
  active: number;
  sev1_2: number;
  resolved_7d: number;
}

export interface IWarRoomEvent {
  id: string;
  actor_id: string | null;
  event_type: string;
  payload: Record<string, unknown>;
  created_at: string;
}

export interface IWarRoomMessageAuthor {
  id: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface IWarRoomMessage {
  id: string;
  war_room_id: string;
  author_id: string | null;
  author: IWarRoomMessageAuthor | null;
  body: string;
  mentions: string[];
  edited_at: string | null;
  created_at: string;
  client_id?: string | null;
}

export type TWarRoomCreatePayload = {
  name?: string;
  primary_issue_id: string;
  severity?: TWarRoomSeverity;
  description_html?: string;
  service_ids?: string[];
};

export type TWarRoomUpdatePayload = {
  name?: string;
  severity?: TWarRoomSeverity;
  status?: TWarRoomStatus;
  description_html?: string;
  notes_html?: string;
};
