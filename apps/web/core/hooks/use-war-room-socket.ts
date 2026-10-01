/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
// plane imports
import { LIVE_BASE_PATH, LIVE_BASE_URL } from "@plane/constants";
import type { TWarRoomConnectionStatus, TWarRoomSocketEvent } from "@plane/types";
// hooks
import { useUser } from "@/hooks/store/user";

const HEARTBEAT_INTERVAL_MS = 30_000;
const MAX_RECONNECT_DELAY_MS = 30_000;
const AUTH_CLOSE_CODE = 4403;

type TUseWarRoomSocketArgs = {
  workspaceSlug: string;
  projectId: string;
  warRoomId: string;
  enabled?: boolean;
  onEvent: (event: TWarRoomSocketEvent) => void;
};

/**
 * One socket per war room. Connects to the Phase 2 live relay
 * (`/live/war-rooms/:roomId`), answers heartbeats, reconnects with exponential
 * backoff and stops permanently on the 4403 auth close.
 */
export const useWarRoomSocket = ({
  workspaceSlug,
  projectId,
  warRoomId,
  enabled = true,
  onEvent,
}: TUseWarRoomSocketArgs) => {
  // store hooks
  const { data: currentUser } = useUser();
  // states
  const [status, setStatus] = useState<TWarRoomConnectionStatus>("connecting");
  // refs
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const heartbeatTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const attemptRef = useRef(0);
  const onEventRef = useRef(onEvent);
  const currentUserId = currentUser?.id;

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  const buildUrl = useCallback(() => {
    if (typeof window === "undefined" || !currentUserId) return null;
    try {
      const base = LIVE_BASE_URL?.trim() || window.location.origin;
      const url = new URL(base);
      url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      url.pathname = `${LIVE_BASE_PATH}/war-rooms/${warRoomId}`;
      url.searchParams.set("workspaceSlug", workspaceSlug);
      url.searchParams.set("projectId", projectId);
      url.searchParams.set("token", JSON.stringify({ id: currentUserId }));
      return url.toString();
    } catch {
      return null;
    }
  }, [currentUserId, projectId, warRoomId, workspaceSlug]);

  const clearTimers = useCallback(() => {
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    if (heartbeatTimerRef.current) {
      clearInterval(heartbeatTimerRef.current);
      heartbeatTimerRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!enabled) {
      setStatus("disabled");
      return;
    }
    let disposed = false;

    const connect = () => {
      if (disposed) return;
      const url = buildUrl();
      if (!url) {
        setStatus("connecting");
        return;
      }
      setStatus(attemptRef.current === 0 ? "connecting" : "reconnecting");
      const socket = new WebSocket(url);
      socketRef.current = socket;

      const handleOpen = () => {
        if (disposed) {
          socket.close();
          return;
        }
        attemptRef.current = 0;
        setStatus("connected");
        heartbeatTimerRef.current = setInterval(() => {
          if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: "ping" }));
        }, HEARTBEAT_INTERVAL_MS);
      };

      const handleMessage = (message: MessageEvent) => {
        try {
          const event = JSON.parse(message.data as string) as TWarRoomSocketEvent;
          if (event?.kind) onEventRef.current(event);
        } catch {
          // ignore malformed payloads
        }
      };

      const handleClose = (event: CloseEvent) => {
        clearTimers();
        socketRef.current = null;
        if (disposed) return;
        if (event.code === AUTH_CLOSE_CODE) {
          setStatus("disabled");
          return;
        }
        setStatus("reconnecting");
        const delay = Math.min(MAX_RECONNECT_DELAY_MS, 1000 * 2 ** attemptRef.current);
        attemptRef.current += 1;
        reconnectTimerRef.current = setTimeout(connect, delay);
      };

      const handleError = () => {
        socket.close();
      };

      socket.addEventListener("open", handleOpen);
      socket.addEventListener("message", handleMessage);
      socket.addEventListener("close", handleClose);
      socket.addEventListener("error", handleError);
    };

    connect();

    return () => {
      disposed = true;
      clearTimers();
      const socket = socketRef.current;
      socketRef.current = null;
      if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING))
        socket.close();
    };
  }, [buildUrl, clearTimers, enabled]);

  const sendTyping = useCallback((isTyping: boolean) => {
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN) return;
    socket.send(JSON.stringify({ type: "typing", is_typing: isTyping }));
  }, []);

  return useMemo(() => ({ status, sendTyping }), [sendTyping, status]);
};
