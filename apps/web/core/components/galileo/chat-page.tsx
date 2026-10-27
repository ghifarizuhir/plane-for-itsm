/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { HistoryOutline, NewChatOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { Button } from "@plane/propel/button";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { buildGalileoConversationHref, buildGalileoHref, shouldNavigateToNewConversation } from "@/lib/galileo-routing";
import { GalileoComposer } from "./composer";
import { GalileoEmptyState } from "./empty-state";
import { useGalileoMobileHistory } from "./layout";
import { GalileoMessage } from "./message";

type TLoadStatus = "idle" | "loading" | "not_found" | "error";

export const GalileoChatPage = observer(function GalileoChatPage() {
  // router
  const { workspaceSlug, conversationId: routeConversationId } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const conversationId = routeConversationId?.toString();
  const router = useRouter();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const store = useAiAssistant();
  const { messages, isGenerating, activeConversationId, conversations } = store;
  // mobile drawer
  const { openHistory } = useGalileoMobileHistory();
  // local state
  const [status, setStatus] = useState<TLoadStatus>(conversationId ? "loading" : "idle");
  const [hasSent, setHasSent] = useState(false);
  const [isPinned, setIsPinned] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  const activeConversation = conversations.find((conversation) => conversation.id === activeConversationId);
  const title = activeConversation?.title || t("assistant.galileo.new_chat");

  const loadConversation = useCallback(async () => {
    if (!conversationId) return;
    if (activeConversationId === conversationId) {
      setStatus("idle");
      return;
    }
    setStatus("loading");
    const result = await store.openConversationById(conversationId);
    if (result === "opened") setStatus("idle");
    else if (result === "not_found") setStatus("not_found");
    else setStatus("error");
  }, [activeConversationId, conversationId, store]);

  useEffect(() => {
    if (conversationId) return;
    store.newChat();
    setStatus("idle");
  }, [conversationId, store]);

  useEffect(() => {
    if (!conversationId) return;
    void loadConversation();
  }, [conversationId, loadConversation]);

  useEffect(() => {
    if (!hasSent) return;
    if (!shouldNavigateToNewConversation(conversationId, activeConversationId)) return;
    if (!activeConversationId) return;
    router.replace(buildGalileoConversationHref(slug, activeConversationId));
  }, [activeConversationId, conversationId, hasSent, router, slug]);

  useEffect(() => {
    if (!isPinned) return;
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
  }, [isGenerating, isPinned, messages]);

  const handleScroll = () => {
    const element = scrollRef.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    setIsPinned(distance < 48);
  };

  const handleSend = async (question: string) => {
    setHasSent(true);
    setIsPinned(true);
    await store.sendGalileoMessage(question);
    if (conversationId && !store.activeConversationId) {
      router.replace(buildGalileoHref(slug));
    }
  };

  const handleNewChat = () => {
    store.newChat();
    router.push(buildGalileoHref(slug));
  };

  const scrollToBottom = () => {
    setIsPinned(true);
    const element = scrollRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  };

  const renderBody = () => {
    if (status === "loading") {
      return (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-12 text-tertiary">{t("common.loading")}…</p>
        </div>
      );
    }

    if (status === "not_found") {
      return (
        <div className="flex flex-1 flex-col items-center justify-center px-6 text-center">
          <h2 className="text-16 font-semibold text-primary">{t("assistant.galileo.not_found_title")}</h2>
          <p className="mt-1 max-w-md text-13 text-tertiary">{t("assistant.galileo.not_found_body")}</p>
          <Button variant="secondary" size="xl" className="mt-4" onClick={handleNewChat}>
            {t("assistant.galileo.start_new_chat")}
          </Button>
        </div>
      );
    }

    if (status === "error") {
      return (
        <div className="flex flex-1 flex-col items-center justify-center px-6 text-center">
          <p className="text-13 text-tertiary">{t("common.something_went_wrong")}</p>
          <Button variant="secondary" size="xl" className="mt-4" onClick={() => void loadConversation()}>
            {t("common.retry")}
          </Button>
        </div>
      );
    }

    if (messages.length === 0 && !isGenerating) {
      return <GalileoEmptyState onSelect={(prompt) => void handleSend(prompt)} />;
    }

    return (
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        role="log"
        aria-label={t("sidebar.galileo")}
        aria-live="polite"
        className="flex flex-1 flex-col gap-4 overflow-y-auto px-4 py-4 md:px-6"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
          {messages.map((message, index) => (
            <GalileoMessage
              key={message.id}
              message={message}
              showRetry={
                message.isError === true &&
                index === messages.length - 1 &&
                !isGenerating &&
                messages.some((candidate) => candidate.role === "user")
              }
              onRetry={() => void store.retryLast()}
            />
          ))}
          {isGenerating && (
            <div className="flex items-center gap-2 text-11 text-tertiary">
              <span className="flex size-6 items-center justify-center rounded-md border border-subtle bg-layer-1">
                <span className="size-1.5 rounded-full bg-accent-primary motion-safe:animate-pulse" />
              </span>
              {t("assistant.galileo.typing")}
            </div>
          )}
          {!isPinned && (
            <button
              type="button"
              onClick={scrollToBottom}
              className="sticky bottom-0 mx-auto rounded-full border border-subtle bg-surface-1 px-3 py-1 text-11 text-secondary shadow-raised-100 hover:text-primary"
            >
              {t("assistant.galileo.scroll_to_bottom")}
            </button>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
      <header className="flex h-12 flex-shrink-0 items-center gap-2 border-b border-subtle px-3">
        <button
          type="button"
          onClick={openHistory}
          aria-label={t("assistant.galileo.history")}
          className="flex size-8 items-center justify-center rounded-md text-secondary hover:bg-layer-1-hover hover:text-primary md:hidden"
        >
          <HistoryOutline className="size-4" />
        </button>
        <h1 className="min-w-0 flex-1 truncate text-13 font-semibold text-primary">{title}</h1>
        <Tooltip label={t("assistant.galileo.new_chat")} side="bottom">
          <button
            type="button"
            onClick={handleNewChat}
            aria-label={t("assistant.galileo.new_chat")}
            className="flex size-8 items-center justify-center rounded-md text-secondary hover:bg-layer-1-hover hover:text-primary"
          >
            <NewChatOutline className="size-4" />
          </button>
        </Tooltip>
      </header>

      {renderBody()}

      <div className={cn("mx-auto w-full max-w-3xl", status !== "idle" ? "hidden" : "")}>
        <GalileoComposer
          disabled={isGenerating || status !== "idle"}
          onSend={(question) => void handleSend(question)}
        />
      </div>
    </div>
  );
});
