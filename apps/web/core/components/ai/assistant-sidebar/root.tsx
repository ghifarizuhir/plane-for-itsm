/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useEffect, useRef, useState } from "react";
import { useParams } from "next/navigation";
import useSWR from "swr";
import { observer } from "mobx-react";
import { AiStar1Outline, CloseOutline, HistoryOutline, RefreshOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import type { TIssue } from "@plane/types";
// hooks
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { useAppTheme } from "@/hooks/store/use-app-theme";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import type { TAiIssueContext } from "@/lib/ai-context";

export const AiAssistantSidebar = observer(function AiAssistantSidebar() {
  // router
  const { workspaceSlug, workItem } = useParams<{ workspaceSlug: string; workItem?: string }>();
  // store hooks
  const { aiSidebarCollapsed, toggleAiSidebar } = useAppTheme();
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

  // restore conversation for the current workspace
  useEffect(() => {
    if (workspaceSlug) setWorkspace(workspaceSlug.toString());
  }, [workspaceSlug, setWorkspace]);

  // resolve active issue: peek view first, then browse route identifier
  const peekedIssue = peekIssue ? getIssueById(peekIssue.issueId) : undefined;
  const [projectIdentifier, sequenceId] = (workItem ?? "").split("-");
  const shouldFetchRouteIssue = !peekedIssue && !!projectIdentifier && !!sequenceId;
  const { data: routeIssueMeta } = useSWR<TIssue>(
    shouldFetchRouteIssue ? `ISSUE_DETAIL_${workspaceSlug}_${projectIdentifier}_${sequenceId}` : null,
    () => fetchIssueWithIdentifier(workspaceSlug!.toString(), projectIdentifier, sequenceId)
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

  const handleSend = () => {
    const value = question.trim();
    if (!value || isGenerating) return;
    setQuestion("");
    void sendMessage(value);
  };

  if (aiSidebarCollapsed !== false) return null;

  return (
    <aside className="fixed right-0 top-10 bottom-0 z-[30] flex w-[24rem] max-w-full flex-col border-l border-subtle bg-surface-1 shadow-sm">
      {/* header */}
      <div className="flex items-center justify-between border-b border-subtle px-4 py-2">
        <div className="flex items-center gap-2">
          <AiStar1Outline className="size-4 text-primary" />
          <span className="text-sm font-medium text-primary">AI Assistant</span>
        </div>
        <div className="flex items-center gap-1">
          <Tooltip label="Clear conversation" side="bottom">
            <button
              type="button"
              onClick={clearConversation}
              className="flex size-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
            >
              <HistoryOutline className="size-4" />
            </button>
          </Tooltip>
          <Tooltip label="Close" side="bottom">
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
      {/* context indicator */}
      <div className="border-b border-subtle px-4 py-1.5 text-xs text-tertiary">
        {hasActiveIssue && activeIssueContext
          ? `Context: ${activeIssueContext.name}`
          : "No active issue — answers without issue context"}
      </div>
      {/* messages */}
      <div ref={scrollRef} className="flex-1 space-y-3 overflow-y-auto px-4 py-3">
        {messages.length === 0 && !isGenerating && (
          <p className="text-xs text-tertiary">
            Ask anything about the current work item — summaries, descriptions, comment drafts. By using this feature,
            you consent to sharing the message with a 3rd party service.
          </p>
        )}
        {messages.map((message) => (
          <div
            key={message.id}
            className={cn("flex", {
              "justify-end": message.role === "user",
              "justify-start": message.role === "assistant",
            })}
          >
            <div
              className={cn("max-w-[85%] rounded-lg px-3 py-2 text-xs", {
                "bg-accent-primary text-on-color": message.role === "user",
                "border border-subtle bg-layer-1": message.role === "assistant" && !message.isError,
                "border border-danger-primary text-danger-primary": message.role === "assistant" && message.isError,
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
        {isGenerating && <p className="text-xs text-tertiary">Generating response…</p>}
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
      {/* input */}
      <div className="border-t border-subtle p-3">
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
          className="w-full resize-none rounded-md border border-subtle bg-surface-1 px-3 py-2 text-sm text-primary outline-none focus:border-accent-primary"
        />
      </div>
    </aside>
  );
});
