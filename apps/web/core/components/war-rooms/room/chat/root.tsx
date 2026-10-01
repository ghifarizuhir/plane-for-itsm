/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoom, TWarRoomConnectionStatus } from "@plane/types";
// components
import { WarRoomChatComposer } from "./composer";
import { WarRoomMessageItem } from "./message-item";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useUser } from "@/hooks/store/user";

type Props = {
  workspaceSlug: string;
  projectId: string;
  room: IWarRoom;
  canWrite: boolean;
  connectionStatus: TWarRoomConnectionStatus;
  sendTyping: (isTyping: boolean) => void;
};

const AT_BOTTOM_THRESHOLD_PX = 80;
const TYPING_RENDER_INTERVAL_MS = 1000;

export const WarRoomChat = observer(function WarRoomChat({
  workspaceSlug,
  projectId,
  room,
  canWrite,
  connectionStatus,
  sendTyping,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getMessages,
    hasMoreMessages,
    fetchMessages,
    sendMessage,
    getUnreadCount,
    incrementUnread,
    clearUnread,
    getTypingUserIds,
  } = useWarRoom();
  const { getUserDetails } = useMember();
  const { data: currentUser } = useUser();
  // states
  const [, setTypingTick] = useState(0);
  // refs
  const scrollRef = useRef<HTMLDivElement>(null);
  const isAtBottomRef = useRef(true);
  const previousLengthRef = useRef(0);
  // derived values
  const messages = getMessages(room.id);
  const hasMore = hasMoreMessages(room.id);
  const unreadCount = getUnreadCount(room.id);
  const currentUserId = currentUser?.id;
  const typingUserIds = getTypingUserIds(room.id).filter((userId) => userId !== currentUserId);
  const typingNames = typingUserIds.map((userId) => getUserDetails(userId)?.display_name).filter(Boolean) as string[];

  useEffect(() => {
    const timer = setInterval(() => setTypingTick((tick) => tick + 1), TYPING_RENDER_INTERVAL_MS);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    clearUnread(room.id);
    previousLengthRef.current = 0;
    void fetchMessages(workspaceSlug, projectId, room.id).catch(() => {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.chat.load_failed") });
    });
    // oxlint-disable-next-line exhaustive-deps -- fetch once per room
  }, [room.id, workspaceSlug, projectId]);

  const handleScroll = () => {
    const node = scrollRef.current;
    if (!node) return;
    const distance = node.scrollHeight - node.scrollTop - node.clientHeight;
    isAtBottomRef.current = distance < AT_BOTTOM_THRESHOLD_PX;
    if (isAtBottomRef.current) clearUnread(room.id);
  };

  useEffect(() => {
    const length = messages.length;
    const grew = length > previousLengthRef.current;
    previousLengthRef.current = length;
    if (!grew) return;
    const node = scrollRef.current;
    if (!node) return;
    const lastMessage = messages[messages.length - 1];
    if (isAtBottomRef.current) {
      node.scrollTop = node.scrollHeight;
    } else if (lastMessage && lastMessage.author_id !== currentUserId) {
      incrementUnread(room.id);
    }
  }, [messages, room.id, currentUserId, incrementUnread]);

  const handleLoadOlder = async () => {
    const oldest = messages[0];
    if (!oldest) return;
    try {
      await fetchMessages(workspaceSlug, projectId, room.id, { before_id: oldest.id });
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.chat.load_failed") });
    }
  };

  const handleSend = useCallback(
    async (body: string): Promise<boolean> => {
      try {
        await sendMessage(workspaceSlug, projectId, room.id, body, crypto.randomUUID());
        return true;
      } catch {
        setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.chat.send_failed") });
        return false;
      }
    },
    [projectId, room.id, sendMessage, t, workspaceSlug]
  );

  const scrollToBottom = () => {
    const node = scrollRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    isAtBottomRef.current = true;
    clearUnread(room.id);
  };

  return (
    <div className="flex h-full w-full flex-col bg-surface-1">
      <div className="flex items-center justify-between gap-2 border-b border-subtle px-3 py-2">
        <div className="flex shrink-0 items-baseline gap-2">
          <span className="font-code text-10 font-medium tracking-[0.14em] text-tertiary uppercase">
            {t("war_room.chat.title")}
          </span>
          <span className="font-code text-10 text-tertiary tabular-nums">{messages.length}</span>
        </div>
        <span className="min-w-0 truncate text-10 text-accent-primary">
          {typingNames.length === 1
            ? t("war_room.chat.typing_single", { name: typingNames[0] })
            : typingNames.length > 1
              ? t("war_room.chat.typing_multiple", { count: typingNames.length })
              : ""}
        </span>
      </div>
      {connectionStatus === "reconnecting" && (
        <div className="border-b border-subtle bg-warning-subtle px-3 py-1 text-11 text-warning-primary">
          {t("war_room.chat.reconnect_banner")}
        </div>
      )}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto px-2 py-2"
      >
        {hasMore && (
          <div className="flex justify-center py-1">
            <Button variant="secondary" size="sm" onClick={() => void handleLoadOlder()}>
              {t("war_room.chat.load_older")}
            </Button>
          </div>
        )}
        {messages.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
            <p className="text-13 font-medium text-primary">{t("war_room.chat.empty_title")}</p>
            <p className="text-12 text-secondary">{t("war_room.chat.empty_description")}</p>
          </div>
        ) : (
          messages.map((message, index) => (
            <WarRoomMessageItem
              key={message.id}
              message={message}
              previousMessage={messages[index - 1]}
              workspaceSlug={workspaceSlug}
              projectId={projectId}
              roomId={room.id}
              canWrite={canWrite}
              currentUserId={currentUserId}
            />
          ))
        )}
      </div>
      {unreadCount > 0 && (
        <div className="flex justify-center">
          <button
            type="button"
            onClick={scrollToBottom}
            className="shadow-md rounded-full bg-accent-primary px-3 py-1 text-11 font-medium text-on-color"
          >
            {t("war_room.chat.new_messages", { count: unreadCount })}
          </button>
        </div>
      )}
      <WarRoomChatComposer
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        disabled={!canWrite}
        onSend={handleSend}
        onTyping={sendTyping}
      />
    </div>
  );
});
