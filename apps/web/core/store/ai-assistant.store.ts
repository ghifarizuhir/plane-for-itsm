/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, computed, makeObservable, observable, runInAction } from "mobx";
import { AIService } from "@/services/ai.service";
import { AI_ASSISTANT_TASK, buildAiPrompt } from "@/lib/ai-context";
import type { TAiIssueContext, TAiMessage } from "@/lib/ai-context";

type TAiService = Pick<AIService, "createGptTask">;

export interface IAIAssistantStore {
  messages: TAiMessage[];
  isGenerating: boolean;
  activeIssueContext: TAiIssueContext | undefined;
  hasActiveIssue: boolean;
  setWorkspace: (workspaceSlug: string | undefined) => void;
  setActiveIssueContext: (context: TAiIssueContext | undefined) => void;
  sendMessage: (question: string) => Promise<void>;
  retryLast: () => Promise<void>;
  clearConversation: () => void;
}

const storageKey = (workspaceSlug: string | undefined) => `ai_assistant_messages_${workspaceSlug ?? "unknown"}`;

export class AIAssistantStore implements IAIAssistantStore {
  messages: TAiMessage[] = [];
  isGenerating = false;
  activeIssueContext: TAiIssueContext | undefined = undefined;

  private workspaceSlug: string | undefined = undefined;

  constructor(private aiService: TAiService = new AIService()) {
    makeObservable(this, {
      messages: observable.deep,
      isGenerating: observable.ref,
      activeIssueContext: observable.ref,
      hasActiveIssue: computed,
      setWorkspace: action,
      setActiveIssueContext: action,
      sendMessage: action,
      retryLast: action,
      clearConversation: action,
    });
  }

  get hasActiveIssue(): boolean {
    return !!this.activeIssueContext;
  }

  setWorkspace = (workspaceSlug: string | undefined) => {
    this.workspaceSlug = workspaceSlug;
    this.messages = this.restore();
  };

  setActiveIssueContext = (context: TAiIssueContext | undefined) => {
    runInAction(() => {
      this.activeIssueContext = context;
    });
  };

  sendMessage = async (question: string) => {
    const trimmed = question.trim();
    if (!trimmed || this.isGenerating || !this.workspaceSlug) return;
    const userMessage: TAiMessage = { id: crypto.randomUUID(), role: "user", content: trimmed };
    runInAction(() => {
      this.messages.push(userMessage);
      this.persist();
    });
    await this.request(userMessage.content);
  };

  retryLast = async () => {
    if (this.isGenerating || !this.workspaceSlug) return;
    let lastUserQuestion: string | undefined;
    for (let index = this.messages.length - 1; index >= 0; index--) {
      if (this.messages[index].role === "user") {
        lastUserQuestion = this.messages[index].content;
        break;
      }
    }
    if (!lastUserQuestion) return;
    if (this.messages[this.messages.length - 1]?.isError) {
      runInAction(() => {
        this.messages.pop();
        this.persist();
      });
    }
    const userMessage: TAiMessage = { id: crypto.randomUUID(), role: "user", content: lastUserQuestion };
    runInAction(() => {
      this.messages.push(userMessage);
      this.persist();
    });
    await this.request(lastUserQuestion);
  };

  clearConversation = () => {
    runInAction(() => {
      this.messages = [];
      this.persist();
    });
  };

  private restore(): TAiMessage[] {
    if (!this.workspaceSlug) return [];
    try {
      const raw = localStorage.getItem(storageKey(this.workspaceSlug));
      return raw ? (JSON.parse(raw) as TAiMessage[]) : [];
    } catch {
      return [];
    }
  }

  private persist() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.setItem(storageKey(this.workspaceSlug), JSON.stringify(this.messages));
    } catch {
      // storage unavailable — persistence is best-effort
    }
  }

  private async request(question: string) {
    this.isGenerating = true;
    try {
      const res = await this.aiService.createGptTask(this.workspaceSlug ?? "", {
        task: AI_ASSISTANT_TASK,
        prompt: buildAiPrompt(this.activeIssueContext, this.messages.slice(0, -1), question),
      });
      const assistantMessage: TAiMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: res.response_html ?? "",
        isError: false,
      };
      runInAction(() => {
        this.messages.push(assistantMessage);
        this.persist();
      });
    } catch (err: any) {
      const errorContent =
        err?.status === 429
          ? err?.data?.error || "Rate limit exceeded."
          : err?.status === 400
            ? "AI is not configured for this instance."
            : "An internal error has occurred. Please try again.";
      runInAction(() => {
        this.messages.push({ id: crypto.randomUUID(), role: "assistant", content: errorContent, isError: true });
        this.persist();
      });
    } finally {
      runInAction(() => {
        this.isGenerating = false;
      });
    }
  }
}
