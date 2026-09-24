/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { formatDistanceToNow } from "date-fns";
import { CheckDoneOutline, CloseOutline, DeleteOutline, EditOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";

const formatUpdatedAt = (value: string) => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return formatDistanceToNow(date, { addSuffix: true });
};

export const ConversationHistoryPanel = observer(function ConversationHistoryPanel({
  onClose,
}: {
  onClose: () => void;
}) {
  const {
    conversations,
    conversationsLoading,
    activeConversationId,
    openConversation,
    renameConversation,
    deleteConversation,
  } = useAiAssistant();
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [renameErrorId, setRenameErrorId] = useState<string | null>(null);
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [deleteErrorId, setDeleteErrorId] = useState<string | null>(null);
  const confirmDeleteRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (confirmingDeleteId) confirmDeleteRef.current?.focus();
  }, [confirmingDeleteId]);

  const closeRename = () => {
    setRenamingId(null);
    setRenameErrorId(null);
  };

  const closeDeleteConfirm = () => {
    setConfirmingDeleteId(null);
    setDeleteErrorId(null);
  };

  const handleRename = async (conversationId: string) => {
    const saved = await renameConversation(conversationId, renameValue);
    if (saved) {
      setRenamingId(null);
      setRenameErrorId(null);
    } else {
      setRenameErrorId(conversationId);
    }
  };

  const handleDelete = async (conversationId: string) => {
    const removed = await deleteConversation(conversationId);
    if (removed) {
      setConfirmingDeleteId(null);
      setDeleteErrorId(null);
    } else {
      setDeleteErrorId(conversationId);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-subtle px-4 py-2.5">
        <span className="text-sm font-semibold text-primary">Chat history</span>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close history"
          className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
        >
          <CloseOutline className="size-4" />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-2">
        {conversationsLoading && conversations.length === 0 && (
          <p className="text-xs px-2 py-4 text-tertiary">Loading conversations…</p>
        )}
        {!conversationsLoading && conversations.length === 0 && (
          <p className="text-xs px-2 py-4 text-tertiary">No conversations yet.</p>
        )}
        {conversations.map((conversation) => (
          <div
            key={conversation.id}
            className={cn("group rounded-md px-2 py-1.5", {
              "bg-layer-1-hover": conversation.id === activeConversationId,
            })}
          >
            <div className="flex items-center gap-2">
              {renamingId === conversation.id ? (
                <>
                  <input
                    value={renameValue}
                    onChange={(event) => setRenameValue(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        void handleRename(conversation.id);
                      } else if (event.key === "Escape") {
                        event.preventDefault();
                        closeRename();
                      }
                    }}
                    // Autofocus is intentional here: the user just opened inline rename.
                    // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus
                    autoFocus
                    aria-label="Conversation title"
                    className="text-xs min-w-0 flex-1 rounded border border-subtle bg-transparent px-1.5 py-1 text-primary outline-none"
                  />
                  <button
                    type="button"
                    aria-label="Save title"
                    onClick={() => void handleRename(conversation.id)}
                    className="text-secondary hover:text-primary"
                  >
                    <CheckDoneOutline className="size-3.5" />
                  </button>
                  <button
                    type="button"
                    aria-label="Cancel rename"
                    onClick={closeRename}
                    className="text-secondary hover:text-primary"
                  >
                    <CloseOutline className="size-3.5" />
                  </button>
                </>
              ) : (
                <>
                  <button
                    type="button"
                    onClick={() => {
                      void openConversation(conversation.id);
                      onClose();
                    }}
                    className="min-w-0 flex-1 text-left"
                  >
                    <span className="text-xs block truncate text-primary" title={conversation.title}>
                      {conversation.title || "New conversation"}
                    </span>
                    <span className="block text-[10px] text-tertiary">
                      {conversation.mode === "agent" ? "Agent" : "Classic"} · {formatUpdatedAt(conversation.updated_at)}
                    </span>
                  </button>
                  {confirmingDeleteId === conversation.id ? (
                    <>
                      <button
                        type="button"
                        ref={confirmDeleteRef}
                        aria-label="Confirm delete"
                        onClick={() => void handleDelete(conversation.id)}
                        className="text-[10px] text-danger-primary"
                      >
                        Delete?
                      </button>
                      <button
                        type="button"
                        aria-label="Cancel delete"
                        onClick={closeDeleteConfirm}
                        className="text-secondary hover:text-primary"
                      >
                        <CloseOutline className="size-3.5" />
                      </button>
                    </>
                  ) : (
                    <span className="flex items-center gap-1 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                      <button
                        type="button"
                        aria-label="Rename conversation"
                        onClick={() => {
                          setRenamingId(conversation.id);
                          setRenameValue(conversation.title);
                          setRenameErrorId(null);
                        }}
                        className="text-secondary hover:text-primary"
                      >
                        <EditOutline className="size-3.5" />
                      </button>
                      <button
                        type="button"
                        aria-label="Delete conversation"
                        onClick={() => {
                          setConfirmingDeleteId(conversation.id);
                          setDeleteErrorId(null);
                        }}
                        className="text-secondary hover:text-danger-primary"
                      >
                        <DeleteOutline className="size-3.5" />
                      </button>
                    </span>
                  )}
                </>
              )}
            </div>
            {renamingId === conversation.id && renameErrorId === conversation.id && (
              <p className="mt-1 text-[10px] text-danger-primary">Could not rename. Try again.</p>
            )}
            {confirmingDeleteId === conversation.id && deleteErrorId === conversation.id && (
              <p className="mt-1 text-[10px] text-danger-primary">Could not delete. Try again.</p>
            )}
          </div>
        ))}
      </div>
    </div>
  );
});
