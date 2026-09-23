/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, computed, makeObservable, observable, runInAction } from "mobx";
import { v4 as uuidv4 } from "uuid";
import { AIService } from "@/services/ai.service";
import { AI_ASSISTANT_TASK, buildAiPrompt } from "@/lib/ai-context";
import type { TAiIssueContext, TAiMessage } from "@/lib/ai-context";

export type TAiAssistantMode = "classic" | "agent";

type TAiService = Pick<AIService, "createGptTask" | "createAgentTask">;

export interface IAIAssistantStore {
  messages: TAiMessage[];
  isGenerating: boolean;
  mode: TAiAssistantMode;
  activeIssueContext: TAiIssueContext | undefined;
  hasActiveIssue: boolean;
  setWorkspace: (workspaceSlug: string | undefined) => void;
  setMode: (mode: TAiAssistantMode) => void;
  setActiveIssueContext: (context: TAiIssueContext | undefined) => void;
  sendMessage: (question: string) => Promise<void>;
  retryLast: () => Promise<void>;
  clearConversation: () => void;
}

export const AI_ASSISTANT_STORAGE_PREFIX = "ai_assistant_messages_";
export const clearPersistedAiConversations = () => {
  try {
    const keys: string[] = [];
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index);
      if (key?.startsWith(AI_ASSISTANT_STORAGE_PREFIX)) keys.push(key);
    }
    keys.forEach((key) => localStorage.removeItem(key));
  } catch {
    // storage unavailable — best-effort
  }
};

const storageKey = (workspaceSlug: string | undefined) => `${AI_ASSISTANT_STORAGE_PREFIX}${workspaceSlug ?? "unknown"}`;

export const AI_ASSISTANT_MODE_PREFIX = "ai_assistant_mode_";
const modeStorageKey = (workspaceSlug: string | undefined) =>
  `${AI_ASSISTANT_MODE_PREFIX}${workspaceSlug ?? "unknown"}`;

export class AIAssistantStore implements IAIAssistantStore {
  messages: TAiMessage[] = [];
  isGenerating = false;
  mode: TAiAssistantMode = "classic";
  activeIssueContext: TAiIssueContext | undefined = undefined;

  private workspaceSlug: string | undefined = undefined;

  constructor(private aiService: TAiService = new AIService()) {
    makeObservable(this, {
      messages: observable.deep,
      isGenerating: observable.ref,
      mode: observable.ref,
      activeIssueContext: observable.ref,
      hasActiveIssue: computed,
      setWorkspace: action,
      setMode: action,
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
    if (workspaceSlug !== this.workspaceSlug) {
      this.activeIssueContext = undefined;
      this.isGenerating = false;
    }
    this.workspaceSlug = workspaceSlug;
    this.messages = this.restore();
    this.mode = this.restoreMode();
  };

  setMode = (mode: TAiAssistantMode) => {
    if (mode === this.mode) return;
    this.isGenerating = false;
    this.mode = mode;
    this.persistMode();
    this.clearConversation();
  };

  setActiveIssueContext = (context: TAiIssueContext | undefined) => {
    runInAction(() => {
      this.activeIssueContext = context;
    });
  };

  sendMessage = async (question: string) => {
    const trimmed = question.trim();
    if (!trimmed || this.isGenerating || !this.workspaceSlug) return;
    const slug = this.workspaceSlug;
    const userMessage: TAiMessage = { id: uuidv4(), role: "user", content: trimmed };
    runInAction(() => {
      this.messages.push(userMessage);
      this.persist();
    });
    await this.request(userMessage.content, slug);
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
    if (!this.messages[this.messages.length - 1]?.isError) return;
    runInAction(() => {
      this.messages.pop();
      this.persist();
    });
    const slug = this.workspaceSlug;
    await this.request(lastUserQuestion, slug);
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

  private restoreMode(): TAiAssistantMode {
    if (!this.workspaceSlug) return "classic";
    try {
      return localStorage.getItem(modeStorageKey(this.workspaceSlug)) === "agent" ? "agent" : "classic";
    } catch {
      return "classic";
    }
  }

  private persistMode() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.setItem(modeStorageKey(this.workspaceSlug), this.mode);
    } catch {
      // storage unavailable — persistence is best-effort
    }
  }

  private async request(question: string, slug: string) {
    const mode = this.mode;
    this.isGenerating = true;
    try {
      const payload = {
        task: AI_ASSISTANT_TASK,
        prompt: buildAiPrompt(this.activeIssueContext, this.messages.slice(0, -1), question),
      };
      const res =
        mode === "agent"
          ? await this.aiService.createAgentTask(slug, payload)
          : await this.aiService.createGptTask(slug, payload);
      if (this.workspaceSlug !== slug || this.mode !== mode) return;
      const assistantMessage: TAiMessage = {
        id: uuidv4(),
        role: "assistant",
        content: mode === "agent" ? (res.response_html ?? res.response ?? "") : (res.response_html ?? ""),
        isError: false,
      };
      runInAction(() => {
        this.messages.push(assistantMessage);
        this.persist();
      });
    } catch (err: any) {
      if (this.workspaceSlug !== slug || this.mode !== mode) return;
      const errorContent =
        err?.status === 429
          ? err?.data?.error || "Rate limit exceeded."
          : err?.status === 400
            ? "AI is not configured for this instance."
            : "An internal error has occurred. Please try again.";
      runInAction(() => {
        this.messages.push({ id: uuidv4(), role: "assistant", content: errorContent, isError: true });
        this.persist();
      });
    } finally {
      if (this.workspaceSlug === slug && this.mode === mode) {
        runInAction(() => {
          this.isGenerating = false;
        });
      }
    }
  }
}
