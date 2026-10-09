/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TWarRoomParticipantRole, TWarRoomSeverity, TWarRoomStatus } from "@plane/types";

export type TWarRoomToneConfig = {
  /** i18n key label */
  label_key: string;
  /** pill className */
  pill: string;
  /** left rail / dot className */
  rail: string;
};

export const WAR_ROOM_SEVERITIES: TWarRoomSeverity[] = ["sev1", "sev2", "sev3", "sev4"];

export const WAR_ROOM_SEVERITY_CONFIG: Record<TWarRoomSeverity, TWarRoomToneConfig> = {
  sev1: {
    label_key: "war_room.severity_values.sev1",
    pill: "bg-danger-subtle text-danger-primary",
    rail: "bg-danger-primary",
  },
  sev2: {
    label_key: "war_room.severity_values.sev2",
    pill: "bg-warning-subtle text-warning-primary",
    rail: "bg-warning-primary",
  },
  sev3: {
    label_key: "war_room.severity_values.sev3",
    pill: "bg-layer-2 text-secondary",
    rail: "bg-layer-3",
  },
  sev4: {
    label_key: "war_room.severity_values.sev4",
    pill: "bg-layer-2 text-tertiary",
    rail: "bg-layer-2",
  },
};

export const WAR_ROOM_STATUSES: TWarRoomStatus[] = ["active", "monitoring", "resolved", "archived"];

export const WAR_ROOM_STATUS_CONFIG: Record<TWarRoomStatus, TWarRoomToneConfig> = {
  active: {
    label_key: "war_room.status_values.active",
    pill: "bg-danger-subtle text-danger-primary",
    rail: "bg-danger-primary",
  },
  monitoring: {
    label_key: "war_room.status_values.monitoring",
    pill: "bg-warning-subtle text-warning-primary",
    rail: "bg-warning-primary",
  },
  resolved: {
    label_key: "war_room.status_values.resolved",
    pill: "bg-success-subtle text-success-primary",
    rail: "bg-success-primary",
  },
  archived: {
    label_key: "war_room.status_values.archived",
    pill: "bg-layer-2 text-tertiary",
    rail: "bg-layer-2",
  },
};

export const WAR_ROOM_PARTICIPANT_ROLES: TWarRoomParticipantRole[] = ["commander", "comms", "scribe", "responder"];

export const WAR_ROOM_ROLE_LABEL_KEYS: Record<TWarRoomParticipantRole, string> = {
  commander: "war_room.roles.commander",
  comms: "war_room.roles.comms",
  scribe: "war_room.roles.scribe",
  responder: "war_room.roles.responder",
};

export const getWarRoomLink = (workspaceSlug: string, projectId: string, warRoomId?: string): string =>
  warRoomId
    ? `/${workspaceSlug}/projects/${projectId}/war-rooms/${warRoomId}`
    : `/${workspaceSlug}/projects/${projectId}/issues?war_room=on`;

/** Server-enforced transition map (`war_room.rs::transitions_allowed`); `archived` is terminal. */
export const WAR_ROOM_STATUS_TRANSITIONS: Record<TWarRoomStatus, TWarRoomStatus[]> = {
  active: ["monitoring", "resolved", "archived"],
  monitoring: ["active", "resolved", "archived"],
  resolved: ["active", "archived"],
  archived: [],
};
