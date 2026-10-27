/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { formatDistanceToNow } from "date-fns";
import { AddOutline, CheckDoneOutline, CloseOutline, MoreHorizontalOutline } from "@makeplane/propel/icons";
import { Button } from "@plane/propel/button";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { groupConversationsByDate, type TGallileoHistoryGroup } from "@/lib/galileo-history";
import { buildGalileoConversationHref, buildGalileoHref } from "@/lib/galileo-routing";

const GROUP_ORDER: TGallileoHistoryGroup[] = ["today", "yesterday", "previous7Days", "older"];

const GROUP_LABEL_KEY: Record<TGallileoHistoryGroup, string> = {
  today: "assistant.galileo.today",
  yesterday: "assistant.galileo.yesterday",
  previous7Days: "assistant.galileo.previous_7_days",
  older: "assistant.galileo.older",
};

const formatUpdatedAt = (value: string): string => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return formatDistanceToNow(date, { addSuffix: true });
};

type Props = {
  className?: string;
  onNavigate?: () => void;
};

export const GalileoHistorySidebar = observer(function GalileoHistorySidebar({ className, onNavigate }: Props) {
  // router
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const router = useRouter();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const {
    conversations,
    conversationsError,
    conversationsLoading,
    activeConversationId,
    isGenerating,
    newChat,
    renameConversation,
    deleteConversation,
    loadConversations,
  } = useAiAssistant();
  // local state
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [renameErrorId, setRenameErrorId] = useState<string | null>(null);
  const [actionsOpenId, setActionsOpenId] = useState<string | null>(null);
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const [deleteErrorId, setDeleteErrorId] = useState<string | null>(null);
  const confirmDeleteRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (confirmingDeleteId) confirmDeleteRef.current?.focus();
  }, [confirmingDeleteId]);

  const classicConversations = conversations.filter((conversation) => conversation.mode === "classic");
  const groups = groupConversationsByDate(classicConversations);
  const hasConversations = classicConversations.length > 0;

  const handleNewChat = () => {
    newChat();
    router.push(buildGalileoHref(slug));
    onNavigate?.();
  };

  const openConversation = (conversationId: string) => {
    setActionsOpenId(null);
    router.push(buildGalileoConversationHref(slug, conversationId));
    onNavigate?.();
  };

  const startRename = (conversationId: string, title: string) => {
    setRenamingId(conversationId);
    setRenameValue(title);
    setRenameErrorId(null);
    setActionsOpenId(null);
  };

  const cancelRename = () => {
    setRenamingId(null);
    setRenameErrorId(null);
  };

  const handleRename = async (conversationId: string) => {
    const saved = await renameConversation(conversationId, renameValue);
    if (saved) {
      cancelRename();
    } else {
      setRenameErrorId(conversationId);
    }
  };

  const handleDelete = async (conversationId: string) => {
    const removed = await deleteConversation(conversationId);
    if (removed) {
      setConfirmingDeleteId(null);
      setDeleteErrorId(null);
      if (conversationId === activeConversationId) router.push(buildGalileoHref(slug));
    } else {
      setDeleteErrorId(conversationId);
    }
  };

  return (
    <aside className={cn("w-[17.5rem] flex-shrink-0 flex-col border-r border-subtle bg-surface-1", className)}>
      <div className="flex flex-col gap-2 border-b border-subtle p-3">
        <h2 className="px-1 text-13 font-semibold text-primary">{t("sidebar.galileo")}</h2>
        <Button
          variant="secondary"
          size="xl"
          className="w-full justify-start"
          prependIcon={<AddOutline />}
          onClick={handleNewChat}
        >
          {t("assistant.galileo.new_chat")}
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {conversationsError && (
          <div className="mx-1 mb-2 rounded-md border border-danger-strong/40 bg-danger-subtle px-2 py-1.5 text-11 text-danger-primary">
            {t("assistant.galileo.load_failed")}{" "}
            <button type="button" className="font-medium underline" onClick={() => void loadConversations()}>
              {t("common.retry")}
            </button>
          </div>
        )}

        {conversationsLoading && !hasConversations && (
          <p className="px-2 py-3 text-12 text-tertiary">{t("common.loading")}…</p>
        )}
        {!conversationsLoading && !hasConversations && (
          <p className="px-2 py-3 text-12 text-tertiary">{t("assistant.galileo.empty_history")}</p>
        )}

        {GROUP_ORDER.map((group) => {
          const items = groups[group];
          if (items.length === 0) return null;

          return (
            <div key={group} className="mb-4 last:mb-0">
              <p className="tracking-wider px-2 pb-1 text-10 font-semibold text-placeholder uppercase">
                {t(GROUP_LABEL_KEY[group])}
              </p>
              <div className="flex flex-col gap-0.5">
                {items.map((conversation) => (
                  <div
                    key={conversation.id}
                    className={cn("group/item rounded-md", {
                      "bg-layer-1-hover": conversation.id === activeConversationId,
                    })}
                  >
                    {renamingId === conversation.id ? (
                      <div className="flex items-center gap-1 p-1">
                        <input
                          value={renameValue}
                          onChange={(event) => setRenameValue(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              event.preventDefault();
                              void handleRename(conversation.id);
                            } else if (event.key === "Escape") {
                              event.preventDefault();
                              cancelRename();
                            }
                          }}
                          // Autofocus is intentional: the user just opened inline rename.
                          // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus
                          autoFocus
                          aria-label={t("assistant.galileo.conversation_title")}
                          className="min-w-0 flex-1 rounded border border-subtle bg-transparent px-1.5 py-1 text-12 text-primary outline-none"
                        />
                        <button
                          type="button"
                          aria-label={t("common.save")}
                          onClick={() => void handleRename(conversation.id)}
                          className="text-secondary hover:text-primary"
                        >
                          <CheckDoneOutline className="size-3.5" />
                        </button>
                        <button
                          type="button"
                          aria-label={t("common.cancel")}
                          onClick={cancelRename}
                          className="text-secondary hover:text-primary"
                        >
                          <CloseOutline className="size-3.5" />
                        </button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          onClick={() => openConversation(conversation.id)}
                          className="min-w-0 flex-1 rounded-md px-2 py-1.5 text-left"
                        >
                          <span
                            className="block truncate text-12 text-primary"
                            title={conversation.title || t("assistant.galileo.new_chat")}
                          >
                            {conversation.title || t("assistant.galileo.new_chat")}
                          </span>
                          <span className="block text-10 text-tertiary">
                            {formatUpdatedAt(conversation.updated_at)}
                          </span>
                        </button>
                        <button
                          type="button"
                          disabled={isGenerating}
                          aria-label={t("assistant.galileo.conversation_actions")}
                          aria-expanded={actionsOpenId === conversation.id}
                          onClick={() => setActionsOpenId(actionsOpenId === conversation.id ? null : conversation.id)}
                          className="mr-1 flex size-6 flex-shrink-0 items-center justify-center rounded-md text-tertiary opacity-100 transition-opacity group-hover/item:opacity-100 hover:bg-layer-1-hover hover:text-primary focus:opacity-100 disabled:opacity-40 md:opacity-0 md:group-hover/item:opacity-100"
                        >
                          <MoreHorizontalOutline className="size-3.5" />
                        </button>
                      </div>
                    )}

                    {actionsOpenId === conversation.id && renamingId !== conversation.id && (
                      <div className="flex items-center gap-3 px-2 pb-1.5">
                        <button
                          type="button"
                          disabled={isGenerating}
                          onClick={() => startRename(conversation.id, conversation.title)}
                          className="text-11 text-secondary hover:text-primary disabled:opacity-40"
                        >
                          {t("assistant.galileo.rename")}
                        </button>
                        <button
                          type="button"
                          disabled={isGenerating}
                          onClick={() => {
                            setConfirmingDeleteId(conversation.id);
                            setActionsOpenId(null);
                          }}
                          className="text-11 text-danger-primary disabled:opacity-40"
                        >
                          {t("common.delete")}
                        </button>
                      </div>
                    )}

                    {confirmingDeleteId === conversation.id && (
                      <div className="flex items-center gap-2 px-2 pb-1.5">
                        <span className="text-11 text-secondary">{t("assistant.galileo.delete_confirm")}</span>
                        <button
                          type="button"
                          ref={confirmDeleteRef}
                          disabled={isGenerating}
                          onClick={() => void handleDelete(conversation.id)}
                          className="text-11 font-medium text-danger-primary disabled:opacity-40"
                        >
                          {t("common.delete")}
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirmingDeleteId(null)}
                          className="text-11 text-tertiary hover:text-primary"
                        >
                          {t("common.cancel")}
                        </button>
                      </div>
                    )}

                    {renameErrorId === conversation.id && (
                      <p className="px-2 pb-1 text-10 text-danger-primary">{t("common.something_went_wrong")}</p>
                    )}
                    {deleteErrorId === conversation.id && (
                      <p className="px-2 pb-1 text-10 text-danger-primary">{t("common.something_went_wrong")}</p>
                    )}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </aside>
  );
});
