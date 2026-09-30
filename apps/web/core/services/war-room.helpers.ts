/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TWarRoomSeverity, TWarRoomStatus, TWarRoomStatusTab } from "@plane/types";

/** Mirrors the server-side `severity_from_priority` mapping. */
export const severityFromPriority = (priority: string | null | undefined): TWarRoomSeverity | null => {
  switch (priority) {
    case "urgent":
      return "sev1";
    case "high":
      return "sev2";
    case "medium":
      return "sev3";
    case "low":
    case "none":
      return "sev4";
    default:
      return null;
  }
};

export const isActiveWarRoomStatus = (status: TWarRoomStatus): boolean =>
  status === "active" || status === "monitoring";

export const statusFilterForTab = (tab: TWarRoomStatusTab): string | undefined => {
  switch (tab) {
    case "active":
      return "active,monitoring";
    case "resolved":
      return "resolved";
    case "all":
      return undefined;
  }
};

/** `HH:MM:SS`; `endAt` kosong = timer berjalan memakai `now`. */
export const formatElapsed = (startedAt: string, endAt: string | null, now: number = Date.now()): string => {
  const startMs = new Date(startedAt).getTime();
  const endMs = endAt ? new Date(endAt).getTime() : now;
  const totalSeconds = Number.isNaN(startMs) ? 0 : Math.max(0, Math.floor((endMs - startMs) / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":");
};

export const getWarRoomIncidentLink = (workspaceSlug: string, identifier: string): string =>
  `/${workspaceSlug}/browse/${identifier}/`;
