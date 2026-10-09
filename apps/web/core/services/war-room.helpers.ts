/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IWarRoomMessage, IServiceDependency, TWarRoomSeverity, TWarRoomStatus } from "@plane/types";

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

export type TWarRoomBlastRadius = {
  affectedIds: string[];
  neighborIds: string[];
  includedIds: string[];
};

/** Affected services + every 1-hop neighbor in both dependency directions. */
export const getBlastRadius = (
  affectedServiceIds: string[],
  dependencies: IServiceDependency[]
): TWarRoomBlastRadius => {
  const affected = new Set(affectedServiceIds);
  const neighbors = new Set<string>();
  for (const dependency of dependencies) {
    if (affected.has(dependency.from_service_id) && !affected.has(dependency.to_service_id))
      neighbors.add(dependency.to_service_id);
    if (affected.has(dependency.to_service_id) && !affected.has(dependency.from_service_id))
      neighbors.add(dependency.from_service_id);
  }
  return {
    affectedIds: [...affected],
    neighborIds: [...neighbors],
    includedIds: [...affected, ...neighbors],
  };
};

/** Insert or reconcile a message by `id`, then by `client_id` (optimistic send / WS echo). */
export const upsertMessageInList = (messages: IWarRoomMessage[], message: IWarRoomMessage): IWarRoomMessage[] => {
  const byId = messages.findIndex((candidate) => candidate.id === message.id);
  const index =
    byId !== -1
      ? byId
      : message.client_id
        ? messages.findIndex((candidate) => candidate.client_id === message.client_id)
        : -1;
  if (index === -1) return [...messages, message];
  const next = [...messages];
  next[index] = message;
  return next;
};

/** Append a plain-text note to a notes HTML document, escaped. */
export const appendNoteToHtml = (notesHtml: string, note: string): string => {
  const trimmed = note.trim();
  if (trimmed === "") return notesHtml;
  const escaped = trimmed
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/\n/g, "<br>");
  return `${notesHtml}<p>${escaped}</p>`;
};

export type TWarRoomMention = {
  id: string;
  display_name: string;
};

/** Turn the composer's human-readable `@Display Name` text into server `@{uuid}` tokens. */
export const serializeMentionTokens = (body: string, mentions: TWarRoomMention[]): string => {
  let output = body;
  const ordered = [...mentions]
    .filter((mention) => mention.display_name !== "")
    // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; the spread above already copies the array
    .sort((a, b) => b.display_name.length - a.display_name.length);
  for (const mention of ordered) {
    output = output.split(`@${mention.display_name}`).join(`@{${mention.id}}`);
  }
  return output;
};

export type TWarRoomMessageSegment = { type: "text"; value: string } | { type: "mention"; user_id: string };

const MENTION_TOKEN_PATTERN = /@\{([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\}/gi;

/** Split a stored message body into renderable text/mention segments. */
export const parseMessageSegments = (body: string): TWarRoomMessageSegment[] => {
  const segments: TWarRoomMessageSegment[] = [];
  let lastIndex = 0;
  for (const match of body.matchAll(MENTION_TOKEN_PATTERN)) {
    const index = match.index ?? 0;
    if (index > lastIndex) segments.push({ type: "text", value: body.slice(lastIndex, index) });
    segments.push({ type: "mention", user_id: match[1] });
    lastIndex = index + match[0].length;
  }
  if (lastIndex < body.length) segments.push({ type: "text", value: body.slice(lastIndex) });
  return segments;
};

/** Chat grouping: new header on author change or a >5 minute gap. */
export const shouldShowMessageHeader = (previous: IWarRoomMessage | undefined, current: IWarRoomMessage): boolean => {
  if (!previous) return true;
  if (previous.author_id !== current.author_id) return true;
  return new Date(current.created_at).getTime() - new Date(previous.created_at).getTime() > 5 * 60 * 1000;
};

export const isTypingActive = (expiresAt: number | undefined, now: number = Date.now()): boolean =>
  typeof expiresAt === "number" && expiresAt > now;
