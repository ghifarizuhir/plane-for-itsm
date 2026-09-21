/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useEffect, useRef, useState } from "react";
import { useParams } from "next/navigation";
import useSWR from "swr";
import { observer } from "mobx-react";
import { AiStar1Outline, CloseOutline, NewChatOutline, RefreshOutline, SendOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import type { TIssue } from "@plane/types";
// hooks
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { useAppTheme } from "@/hooks/store/use-app-theme";
import { useInstance } from "@/hooks/store/use-instance";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import type { TAiIssueContext } from "@/lib/ai-context";

const SUGGESTIONS = [
  "Summarize this work item in 3 bullets",
  "Draft a status comment for this work item",
  "Suggest acceptance criteria for this work item",
];

export const AiAssistantSidebar = observer(function AiAssistantSidebar() {
  // router
  const { workspaceSlug, workItem } = useParams<{ workspaceSlug: string; workItem?: string }>();
  const rawWorkspaceSlug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const rawWorkItem = Array.isArray(workItem) ? workItem[0] : workItem;
  // store hooks
  const { aiSidebarCollapsed, toggleAiSidebar } = useAppTheme();
  const { config } = useInstance();
  const {
    messages,
    isGenerating,
    activeIssueContext,
    hasActiveIssue,
    setWorkspace,
    setActiveIssueContext,
    sendMessage,
    retryLast,
    clearConversation,
  } = useAiAssistant();
  const { peekIssue, fetchIssueWithIdentifier, issue: { getIssueById } } = useIssueDetail();
  const { getStateById } = useProjectState();
  // local state
  const [question, setQuestion] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  const isOpen = aiSidebarCollapsed === false;

  // restore conversation for the current workspace
  useEffect(() => {
    if (rawWorkspaceSlug) setWorkspace(rawWorkspaceSlug.toString());
  }, [rawWorkspaceSlug, setWorkspace]);

  // resolve active issue: peek view first, then browse route identifier
  const peekedIssue = peekIssue ? getIssueById(peekIssue.issueId) : undefined;
  const [projectIdentifier, sequenceId] = (rawWorkItem ?? "").split("-");
  const shouldFetchRouteIssue =
    isOpen &&
    !!config?.has_llm_configured &&
    !!rawWorkspaceSlug &&
    !peekedIssue &&
    !!projectIdentifier &&
    !!sequenceId;
  const { data: routeIssueMeta } = useSWR<TIssue>(
    shouldFetchRouteIssue ? `ISSUE_DETAIL_${rawWorkspaceSlug}_${projectIdentifier}_${sequenceId}` : null,
    () => fetchIssueWithIdentifier(rawWorkspaceSlug!.toString(), projectIdentifier, sequenceId)
  );
  const issue = peekedIssue ?? (routeIssueMeta?.id ? getIssueById(routeIssueMeta.id) : undefined);
  const stateName = getStateById(issue?.state_id ?? null)?.name;

  useEffect(() => {
    const context: TAiIssueContext | undefined = issue
      ? {
          name: issue.name ?? "",
          descriptionHtml: issue.description_html ?? "",
          state: stateName,
          priority: issue.priority ?? undefined,
        }
      : undefined;
    setActiveIssueContext(context);
  }, [issue?.id, issue?.name, issue?.description_html, issue?.priority, stateName, setActiveIssueContext]);

  // keep newest message visible
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages.length, isGenerating]);

  const handleSend = (preset?: string) => {
    const value = (preset ?? question).trim();
    if (!value || isGenerating) return;
    setQuestion("");
    void sendMessage(value);
  };

  if (!config?.has_llm_configured) return null;

  return (
    <aside
      className={cn(
        "relative flex h-full shrink-0 flex-col overflow-hidden border-subtle bg-surface-1 transition-[width] duration-300 ease-in-out",
        isOpen ? "w-[24rem] max-w-[85vw] border-l" : "w-0 border-l-0"
      )}
      aria-hidden={!isOpen}
    >
      {isOpen && (
        <div className="flex h-full w-[24rem] max-w-[85vw] flex-col">
          {/* header */}
          <div className="flex items-center justify-between border-b border-subtle px-4 py-2.5">
            <div className="flex items-center gap-2.5">
              <span
                className={cn(
                  "size-2 rounded-full",
                  isGenerating ? "ai-status-orb bg-accent-primary" : "bg-accent-primary"
                )}
              />
              <span className="text-sm font-semibold text-primary">AI Assistant</span>
              {isGenerating && (
                <span className="font-mono text-[10px] uppercase tracking-widest text-tertiary">thinking</span>
              )}
            </div>
            <div className="flex items-center gap-1">
              <Tooltip label="New conversation" side="bottom">
                <button
                  type="button"
                  onClick={clearConversation}
                  className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
                >
                  <NewChatOutline className="size-4" />
                </button>
              </Tooltip>
              <Tooltip label="Close panel" side="bottom">
                <button
                  type="button"
                  onClick={() => toggleAiSidebar(true)}
                  className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
                >
                  <CloseOutline className="size-4" />
                </button>
              </Tooltip>
            </div>
          </div>
          {/* context strip */}
          <div className="flex items-center gap-2 border-b border-subtle px-4 py-2">
            <span className="shrink-0 font-mono text-[10px] uppercase tracking-widest text-tertiary">ctx</span>
            {hasActiveIssue && activeIssueContext ? (
              <>
                {stateName && (
                  <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-subtle bg-layer-1 px-2 py-0.5 text-[11px] text-secondary">
                    <span className="size-1.5 rounded-full bg-accent-primary" />
                    {stateName}
                  </span>
                )}
                <span className="truncate text-xs text-secondary" title={activeIssueContext.name}>
                  {activeIssueContext.name}
                </span>
              </>
            ) : (
              <span className="truncate text-xs text-tertiary">No issue in view — general answers</span>
            )}
          </div>
          {/* messages */}
          <div ref={scrollRef} className="flex-1 space-y-3 overflow-y-auto px-4 py-4">
            {messages.length === 0 && !isGenerating && (
              <div className="ai-rise flex flex-col items-center px-2 pt-8 text-center">
                <span className="flex size-11 items-center justify-center rounded-2xl border border-subtle bg-layer-1">
                  <AiStar1Outline className="size-5 text-accent-primary" />
                </span>
                <p className="mt-3 text-sm font-medium text-primary">Ask about this work item</p>
                <p className="mt-1 text-xs leading-relaxed text-tertiary">
                  Summaries, descriptions, comment drafts — grounded in the issue on screen.
                </p>
                <div className="mt-4 flex flex-col gap-1.5 self-stretch">
                  {SUGGESTIONS.map((suggestion) => (
                    <button
                      key={suggestion}
                      type="button"
                      onClick={() => handleSend(suggestion)}
                      className="rounded-lg border border-subtle bg-layer-1 px-3 py-2 text-left text-xs text-secondary transition-colors hover:border-accent-primary hover:text-primary"
                    >
                      {suggestion}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {messages.map((message, index) => (
              <div
                key={message.id}
                style={{ animationDelay: `${Math.min(index, 5) * 45}ms` }}
                className={cn("ai-rise flex", {
                  "justify-end": message.role === "user",
                  "justify-start": message.role === "assistant",
                })}
              >
                <div
                  className={cn("max-w-[88%] rounded-xl px-3 py-2 text-[13px] leading-relaxed", {
                    "rounded-br-sm bg-accent-primary text-on-color": message.role === "user",
                    "rounded-bl-sm border border-subtle border-l-2 border-l-accent-primary bg-layer-1 text-primary":
                      message.role === "assistant" && !message.isError,
                    "rounded-bl-sm border border-danger-primary text-danger-primary":
                      message.role === "assistant" && message.isError,
                  })}
                >
                  {message.role === "assistant" && !message.isError ? (
                    <div dangerouslySetInnerHTML={{ __html: message.content }} />
                  ) : (
                    <p className="whitespace-pre-wrap">{message.content}</p>
                  )}
                </div>
              </div>
            ))}
            {isGenerating && (
              <div className="flex items-center gap-1.5 px-1 py-1" aria-label="Generating response">
                <span className="ai-typing-dot size-1.5 rounded-full bg-accent-primary" />
                <span className="ai-typing-dot size-1.5 rounded-full bg-accent-primary" />
                <span className="ai-typing-dot size-1.5 rounded-full bg-accent-primary" />
              </div>
            )}
            {!isGenerating && messages[messages.length - 1]?.isError && (
              <button
                type="button"
                onClick={() => void retryLast()}
                className="flex items-center gap-1 text-xs text-accent-primary"
              >
                <RefreshOutline className="size-3.5" /> Retry
              </button>
            )}
          </div>
          {/* composer */}
          <div className="border-t border-subtle p-3">
            <div className="rounded-xl border border-subtle bg-layer-1 transition-colors focus-within:border-accent-primary">
              <textarea
                value={question}
                onChange={(event) => setQuestion(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    handleSend();
                  }
                }}
                placeholder="Ask AI anything…"
                rows={2}
                className="w-full resize-none bg-transparent px-3 pt-2.5 text-sm text-primary outline-none placeholder:text-tertiary"
              />
              <div className="flex items-center justify-between px-2 pb-2">
                <span className="pl-1 font-mono text-[10px] uppercase tracking-widest text-tertiary">
                  {hasActiveIssue ? "grounded" : "general"}
                </span>
                <button
                  type="button"
                  onClick={() => handleSend()}
                  disabled={!question.trim() || isGenerating}
                  aria-label="Send message"
                  className="flex size-8 items-center justify-center rounded-full bg-accent-primary text-on-color transition-opacity hover:opacity-90 disabled:opacity-40"
                >
                  <SendOutline className="size-4" />
                </button>
              </div>
            </div>
            <p className="mt-2 px-1 text-[11px] leading-snug text-tertiary">
              By using this feature, you consent to sharing the message with a 3rd party service.
            </p>
          </div>
        </div>
      )}
    </aside>
  );
});
