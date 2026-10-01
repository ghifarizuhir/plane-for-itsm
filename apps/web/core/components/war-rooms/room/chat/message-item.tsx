/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IWarRoomMessage } from "@plane/types";
import { CustomMenu } from "@plane/ui";
import { cn, getFileURL } from "@plane/utils";
// helpers
import {
  parseMessageSegments,
  shouldShowMessageHeader,
  type TWarRoomMessageSegment,
} from "@/services/war-room.helpers";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useWarRoom } from "@/hooks/store/use-war-room";

type Props = {
  message: IWarRoomMessage;
  previousMessage?: IWarRoomMessage;
  workspaceSlug: string;
  projectId: string;
  roomId: string;
  canWrite: boolean;
  currentUserId?: string;
};

export const WarRoomMessageItem = observer(function WarRoomMessageItem({
  message,
  previousMessage,
  workspaceSlug,
  projectId,
  roomId,
  canWrite,
  currentUserId,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { updateMessage, deleteMessage } = useWarRoom();
  const { getUserDetails } = useMember();
  // states
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState(message.body);
  // derived values
  const showHeader = shouldShowMessageHeader(previousMessage, message);
  const isOwn = Boolean(currentUserId) && message.author_id === currentUserId;
  const authorName =
    message.author?.display_name ?? (message.author_id ? getUserDetails(message.author_id)?.display_name : null);
  const segments = parseMessageSegments(message.body);
  // Content-based keys: stable across renders and unique within one message.
  const seenKeys = new Map<string, number>();
  const keyForSegment = (segment: TWarRoomMessageSegment): string => {
    const base = segment.type === "mention" ? `mention-${segment.user_id}` : `text-${segment.value}`;
    const count = (seenKeys.get(base) ?? 0) + 1;
    seenKeys.set(base, count);
    return `${base}-${count}`;
  };

  const handleSave = async () => {
    const body = editValue.trim();
    if (body === "" || body === message.body) {
      setIsEditing(false);
      return;
    }
    try {
      await updateMessage(workspaceSlug, projectId, roomId, message.id, body);
      setIsEditing(false);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.errors.generic") });
    }
  };

  const handleDelete = async () => {
    try {
      await deleteMessage(workspaceSlug, projectId, roomId, message.id);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("toast.error"), message: t("war_room.errors.generic") });
    }
  };

  return (
    <div className={cn("group/message flex gap-2 px-1", showHeader ? "mt-3" : "mt-0.5")}>
      <div className="w-6 shrink-0">
        {showHeader && (
          <Avatar
            size="sm"
            src={message.author?.avatar_url ? getFileURL(message.author.avatar_url) : undefined}
            alt={authorName ?? ""}
            fallback={authorName?.[0]?.toUpperCase()}
          />
        )}
      </div>
      <div className="min-w-0 flex-1">
        {showHeader && (
          <div className="flex items-center gap-2">
            <span className="text-12 font-medium text-primary">
              {authorName ?? t("war_room.activity.actor_unknown")}
            </span>
            <span className="text-10 text-tertiary">
              {new Date(message.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </span>
            {isOwn && canWrite && !isEditing && (
              <span className="opacity-0 transition-opacity group-hover/message:opacity-100">
                <CustomMenu ellipsis className="h-4" placement="bottom-end">
                  <CustomMenu.MenuItem
                    onClick={() => {
                      setEditValue(message.body);
                      setIsEditing(true);
                    }}
                  >
                    {t("war_room.chat.edit")}
                  </CustomMenu.MenuItem>
                  <CustomMenu.MenuItem onClick={() => void handleDelete()}>
                    {t("war_room.chat.delete")}
                  </CustomMenu.MenuItem>
                </CustomMenu>
              </span>
            )}
          </div>
        )}
        {isEditing ? (
          <div className="mt-1 space-y-1">
            <textarea
              value={editValue}
              onChange={(event) => setEditValue(event.target.value)}
              className="w-full resize-none rounded-sm border border-subtle bg-surface-1 px-2 py-1 text-13 text-primary outline-none focus:border-strong"
              rows={2}
            />
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" onClick={() => void handleSave()}>
                {t("war_room.chat.save")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setIsEditing(false)}>
                {t("war_room.chat.cancel")}
              </Button>
            </div>
          </div>
        ) : (
          <p className="text-13 break-words whitespace-pre-wrap text-primary">
            {segments.map((segment) =>
              segment.type === "mention" ? (
                <span key={keyForSegment(segment)} className="rounded-xs bg-accent-subtle px-1 text-accent-primary">
                  @{getUserDetails(segment.user_id)?.display_name ?? segment.user_id}
                </span>
              ) : (
                <span key={keyForSegment(segment)}>{segment.value}</span>
              )
            )}
            {message.edited_at && <span className="ml-1 text-10 text-tertiary">({t("war_room.chat.edited")})</span>}
          </p>
        )}
      </div>
    </div>
  );
});
