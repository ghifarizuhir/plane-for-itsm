/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IncomingHttpHeaders } from "http";
// types
import type { TWarRoomHandshake } from "@/types";

/**
 * Parse the war room WS handshake from the express request. Mirrors the
 * collaboration auth: the token is `JSON.stringify(user)`, the session cookie
 * comes from the handshake headers (same host, different port → cookie is
 * sent), with an optional cookie embedded in the token as fallback.
 */
export const parseWarRoomHandshake = ({
  headers,
  params,
  query,
}: {
  headers: IncomingHttpHeaders;
  params: Record<string, string | undefined>;
  query: Record<string, unknown>;
}): TWarRoomHandshake | null => {
  const roomId = params.roomId;
  const workspaceSlug = typeof query.workspaceSlug === "string" ? query.workspaceSlug : undefined;
  const projectId = typeof query.projectId === "string" ? query.projectId : undefined;
  const rawToken = typeof query.token === "string" ? query.token : undefined;
  if (!roomId || !workspaceSlug || !projectId || !rawToken) return null;

  let userId: string | undefined;
  let tokenCookie: string | undefined;
  try {
    const parsed = JSON.parse(rawToken) as { id?: string; cookie?: string };
    userId = parsed.id;
    tokenCookie = parsed.cookie;
  } catch {
    return null;
  }
  const cookie = tokenCookie || headers.cookie?.toString();
  if (!userId || !cookie) return null;

  return { userId, cookie, roomId, workspaceSlug, projectId };
};
