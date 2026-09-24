/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useEffect, useRef, useState } from "react";
import { useParams } from "next/navigation";
import useSWR from "swr";
import { observer } from "mobx-react";
import {
  AiStar1Outline,
  CloseOutline,
  HistoryOutline,
  NewChatOutline,
  RefreshOutline,
  SendOutline,
} from "@makeplane/propel/icons";
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
import { sanitizeAssistantHtml, type TAiIssueContext } from "@/lib/ai-context";
// components
import { ConversationHistoryPanel } from "./conversation-history-panel";
import { ScheduleProposalCard } from "./schedule-proposal-card";

const SUGGESTIONS = [
  "Summarize this work item in 3 bullets",
  "Draft a status comment for this work item",
  "Suggest resolution steps for this work item",
];

const MODES = [
  { value: "classic", label: "Classic", hint: "Single model call grounded in the work item on screen" },
  { value: "agent", label: "Agent", hint: "Looks up projects and work items in this workspace" },
] as const;

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
    mode,
    activeIssueContext,
    hasActiveIssue,
    setWorkspace,
    setMode,
    setActiveIssueContext,
    sendMessage,
    retryLast,
    newChat,
    historyOpen,
    setHistoryOpen,
    confirmScheduleProposal,
    resolveScheduleProposal,
  } = useAiAssistant();
  const {
    peekIssue,
    fetchIssueWithIdentifier,
    issue: { getIssueById },
  } = useIssueDetail();
  const { getStateById } = useProjectState();
  // local state
  const [question, setQuestion] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  const isOpen = aiSidebarCollapsed === false;

  // restore conversation for the current workspace
  useEffect(() => {
    if (rawWorkspaceSlug) setWorkspace(rawWorkspaceSlug.toString());
  }, [rawWorkspaceSlug, setWorkspace]);

  // resolve active issue: peek view first, then browse route identifier
  const peekedIssue = peekIssue ? getIssueById(peekIssue.issueId) : undefined;
  const [projectIdentifier, sequenceId] = (rawWorkItem ?? "").split("-");
  const shouldFetchRouteIssue =
    isOpen && !!config?.has_llm_configured && !!rawWorkspaceSlug && !peekedIssue && !!projectIdentifier && !!sequenceId;
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
    // oxlint-disable-next-line eslint-plugin-react-hooks/exhaustive-deps
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

  const trimmedQuestion = question.trimStart().toLowerCase();
  const showScheduleHint = trimmedQuestion.startsWith("/") && !trimmedQuestion.startsWith("/schedule");

  return (
    <aside
      className={cn(
        "relative flex h-full shrink-0 flex-col overflow-hidden bg-surface-1 transition-[width] duration-300 ease-in-out",
        isOpen ? "mr-2 mb-2 w-[24rem] max-w-[85vw] rounded-xl border border-subtle" : "w-0 border-0"
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
              <span className="text-sm font-semibold text-primary">Galileo</span>
              <div
                role="group"
                aria-label="Assistant mode"
                className="flex items-center rounded-md border border-subtle bg-layer-1 p-0.5"
              >
                {MODES.map(({ value, label, hint }) => (
                  <Tooltip key={value} label={hint} side="bottom">
                    <button
                      type="button"
                      aria-pressed={mode === value}
                      disabled={isGenerating}
                      onClick={() => setMode(value)}
                      className={cn(
                        "rounded-[5px] px-2 py-0.5 text-[11px] font-medium transition-colors disabled:opacity-50",
                        mode === value ? "bg-accent-primary text-on-color" : "text-secondary hover:text-primary"
                      )}
                    >
                      {label}
                    </button>
                  </Tooltip>
                ))}
              </div>
              {isGenerating && (
                <span className="font-mono tracking-widest text-[10px] text-tertiary uppercase">thinking</span>
              )}
            </div>
            <div className="flex items-center gap-1">
              <Tooltip label="Chat history" side="bottom">
                <button
                  type="button"
                  onClick={() => setHistoryOpen(!historyOpen)}
                  aria-pressed={historyOpen}
                  className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
                >
                  <HistoryOutline className="size-4" />
                </button>
              </Tooltip>
              <Tooltip label="New conversation" side="bottom">
                <button
                  type="button"
                  onClick={newChat}
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
          {historyOpen ? (
            <ConversationHistoryPanel onClose={() => setHistoryOpen(false)} />
          ) : (
            <>
              {/* context strip */}
              <div className="flex items-center gap-2 border-b border-subtle px-4 py-2">
                <span className="font-mono tracking-widest shrink-0 text-[10px] text-tertiary uppercase">ctx</span>
                {hasActiveIssue && activeIssueContext ? (
                  <>
                    {stateName && (
                      <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-subtle bg-layer-1 px-2 py-0.5 text-[11px] text-secondary">
                        <span className="size-1.5 rounded-full bg-accent-primary" />
                        {stateName}
                      </span>
                    )}
                    <span className="text-xs truncate text-secondary" title={activeIssueContext.name}>
                      {activeIssueContext.name}
                    </span>
                  </>
                ) : (
                  <span className="text-xs truncate text-tertiary">No work item in view — general answers</span>
                )}
              </div>
              {/* messages */}
              <div ref={scrollRef} className="flex-1 space-y-3 overflow-y-auto px-4 py-4">
                {messages.length === 0 && !isGenerating && (
                  <div className="ai-rise flex flex-col items-center px-2 pt-8 text-center">
                    <span className="flex size-11 items-center justify-center rounded-2xl border border-subtle bg-layer-1">
                      <AiStar1Outline className="size-5 text-accent-primary" />
                    </span>
                    <p className="text-sm mt-3 font-medium text-primary">Ask about this work item</p>
                    <p className="text-xs mt-1 leading-relaxed text-tertiary">
                      Summaries, descriptions, comment drafts — grounded in the work item on screen.
                    </p>
                    <div className="mt-4 flex flex-col gap-1.5 self-stretch">
                      {SUGGESTIONS.map((suggestion) => (
                        <button
                          key={suggestion}
                          type="button"
                          onClick={() => handleSend(suggestion)}
                          className="text-xs hover:border-accent-primary rounded-lg border border-subtle bg-layer-1 px-3 py-2 text-left text-secondary transition-colors hover:text-primary"
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
                      className={cn("flex max-w-[88%] flex-col", {
                        "items-end": message.role === "user",
                        "items-start": message.role === "assistant",
                      })}
                    >
                      <div
                        className={cn("rounded-xl px-3 py-2 text-[13px] leading-relaxed", {
                          "rounded-br-sm bg-accent-primary text-on-color": message.role === "user",
                          "border-l-accent-primary rounded-bl-sm border border-l-2 border-subtle bg-layer-1 text-primary":
                            message.role === "assistant" && !message.isError,
                          "border-danger-primary rounded-bl-sm border text-danger-primary":
                            message.role === "assistant" && message.isError,
                        })}
                      >
                        {message.role === "assistant" && !message.isError ? (
                          <div dangerouslySetInnerHTML={{ __html: sanitizeAssistantHtml(message.content) }} />
                        ) : (
                          <p className="whitespace-pre-wrap">{message.content}</p>
                        )}
                      </div>
                      {message.role === "assistant" && message.scheduleProposal && (
                        <ScheduleProposalCard
                          proposal={message.scheduleProposal}
                          decision={message.scheduleDecision}
                          onConfirm={() => confirmScheduleProposal(message.id)}
                          onCancel={() => resolveScheduleProposal(message.id, "cancelled")}
                        />
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
                    className="text-xs flex items-center gap-1 text-accent-primary"
                  >
                    <RefreshOutline className="size-3.5" /> Retry
                  </button>
                )}
              </div>
              {/* composer */}
              <div className="border-t border-subtle p-3">
                <div className="focus-within:border-accent-primary rounded-xl border border-subtle bg-layer-1 transition-colors">
                  {showScheduleHint && !isGenerating && (
                    <button
                      type="button"
                      onClick={() => {
                        setQuestion("/schedule ");
                        composerRef.current?.focus();
                      }}
                      className="text-xs w-full border-b border-subtle px-3 py-2 text-left text-secondary transition-colors hover:text-primary"
                    >
                      /schedule — <span className="text-tertiary">Schedule a recurring AI report</span>
                    </button>
                  )}
                  <textarea
                    ref={composerRef}
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
                    className="text-sm w-full resize-none bg-transparent px-3 pt-2.5 text-primary outline-none placeholder:text-tertiary"
                  />
                  <div className="flex items-center justify-between px-2 pb-2">
                    <span className="font-mono tracking-widest pl-1 text-[10px] text-tertiary uppercase">
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
            </>
          )}
        </div>
      )}
    </aside>
  );
});
