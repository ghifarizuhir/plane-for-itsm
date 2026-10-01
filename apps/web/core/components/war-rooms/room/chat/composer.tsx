/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react";
import { observer } from "mobx-react";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { getFileURL } from "@plane/utils";
// helpers
import { serializeMentionTokens, type TWarRoomMention } from "@/services/war-room.helpers";
// hooks
import { useMember } from "@/hooks/store/use-member";

const TYPING_THROTTLE_MS = 2000;
const MAX_SUGGESTIONS = 6;

type Props = {
  workspaceSlug: string;
  projectId: string;
  disabled?: boolean;
  onSend: (body: string) => Promise<boolean>;
  onTyping: (isTyping: boolean) => void;
};

export const WarRoomChatComposer = observer(function WarRoomChatComposer({
  workspaceSlug,
  projectId,
  disabled = false,
  onSend,
  onTyping,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const {
    getUserDetails,
    project: { getProjectMemberIds, fetchProjectMembers },
  } = useMember();
  // states
  const [value, setValue] = useState("");
  const [mentions, setMentions] = useState<TWarRoomMention[]>([]);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [isSending, setIsSending] = useState(false);
  // refs
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lastTypingRef = useRef(0);
  // derived values
  const memberIds = getProjectMemberIds(projectId, true) ?? [];
  const candidates =
    mentionQuery === null
      ? []
      : memberIds
          .map((memberId) => ({ id: memberId, display_name: getUserDetails(memberId)?.display_name ?? "" }))
          .filter((member) => member.display_name.toLowerCase().includes(mentionQuery.toLowerCase()))
          .slice(0, MAX_SUGGESTIONS);

  useEffect(() => {
    if (memberIds.length === 0) void fetchProjectMembers(workspaceSlug, projectId);
    // oxlint-disable-next-line exhaustive-deps -- hydrate once per project
  }, [projectId, workspaceSlug]);

  const handleChange = (event: ChangeEvent<HTMLTextAreaElement>) => {
    const nextValue = event.target.value;
    setValue(nextValue);
    const cursor = event.target.selectionStart ?? nextValue.length;
    const match = nextValue.slice(0, cursor).match(/(?:^|\s)@([^\s@]*)$/);
    setMentionQuery(match ? match[1] : null);
    setActiveIndex(0);
    const now = Date.now();
    if (now - lastTypingRef.current > TYPING_THROTTLE_MS) {
      lastTypingRef.current = now;
      onTyping(true);
    }
  };

  const insertMention = (member: TWarRoomMention) => {
    const textarea = textareaRef.current;
    const cursor = textarea?.selectionStart ?? value.length;
    const before = value.slice(0, cursor).replace(/@([^\s@]*)$/, "");
    const after = value.slice(cursor);
    setValue(`${before}@${member.display_name} ${after}`);
    setMentions((current) => [...current.filter((candidate) => candidate.id !== member.id), member]);
    setMentionQuery(null);
    requestAnimationFrame(() => textarea?.focus());
  };

  const handleSubmit = async () => {
    if (disabled || isSending) return;
    const body = serializeMentionTokens(value, mentions).trim();
    if (body === "") return;
    setIsSending(true);
    const sent = await onSend(body);
    setIsSending(false);
    if (sent) {
      setValue("");
      setMentions([]);
      setMentionQuery(null);
      onTyping(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (mentionQuery !== null && candidates.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((index) => (index + 1) % candidates.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((index) => (index - 1 + candidates.length) % candidates.length);
        return;
      }
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        insertMention(candidates[activeIndex]);
        return;
      }
      if (event.key === "Escape") {
        setMentionQuery(null);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void handleSubmit();
    }
  };

  return (
    <div className="relative border-t border-subtle p-2">
      {mentionQuery !== null && candidates.length > 0 && (
        <div className="shadow-lg absolute right-2 bottom-full left-2 z-10 mb-1 max-h-56 overflow-y-auto rounded-md border border-subtle bg-surface-1 py-1">
          {candidates.map((candidate, index) => (
            <button
              key={candidate.id}
              type="button"
              onClick={() => insertMention(candidate)}
              className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-12 hover:bg-layer-transparent-hover ${
                index === activeIndex ? "bg-layer-transparent-hover" : ""
              }`}
            >
              <Avatar
                size="xs"
                src={
                  getUserDetails(candidate.id)?.avatar_url
                    ? getFileURL(getUserDetails(candidate.id)?.avatar_url ?? "")
                    : undefined
                }
                alt={candidate.display_name}
                fallback={candidate.display_name[0]?.toUpperCase()}
              />
              <span className="truncate text-primary">{candidate.display_name}</span>
            </button>
          ))}
        </div>
      )}
      {mentionQuery !== null && candidates.length === 0 && (
        <div className="shadow-lg absolute right-2 bottom-full left-2 z-10 mb-1 rounded-md border border-subtle bg-surface-1 px-2 py-1.5 text-12 text-tertiary">
          {t("war_room.chat.mentions_empty")}
        </div>
      )}
      <div className="flex items-end gap-2">
        <textarea
          ref={textareaRef}
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onBlur={() => onTyping(false)}
          disabled={disabled}
          rows={2}
          placeholder={disabled ? t("war_room.archived_notice") : t("war_room.chat.composer_placeholder")}
          className="min-h-9 flex-1 resize-none rounded-sm border border-subtle bg-surface-1 px-2 py-1.5 text-13 text-primary outline-none focus:border-strong disabled:cursor-not-allowed disabled:opacity-60"
        />
        <Button
          variant="primary"
          size="sm"
          disabled={disabled || isSending || value.trim() === ""}
          onClick={() => void handleSubmit()}
        >
          {t("war_room.chat.send")}
        </Button>
      </div>
    </div>
  );
});
