/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, computed, makeObservable, observable, runInAction } from "mobx";
import { v4 as uuidv4 } from "uuid";
import { AIService } from "@/services/ai.service";
import { AiSchedulesService } from "@/services/ai-schedules.service";
import { AiConversationsService } from "@/services/ai-conversations.service";
import { AI_ASSISTANT_TASK, buildAiContext } from "@/lib/ai-context";
import { toAiMessage } from "@/lib/ai-conversations";
import { isScheduleCommand } from "@/lib/ai-schedule";
import type { TAiIssueContext, TAiMessage } from "@/lib/ai-context";
import type { TAiConversation, TAiConversationMode } from "@/lib/ai-conversations";

export type TAiAssistantMode = TAiConversationMode;

type TAiService = Pick<AIService, "createGptTask" | "createAgentTask">;
type TAiSchedulesService = Pick<AiSchedulesService, "create">;
type TAiConversationsService = Pick<
  AiConversationsService,
  "list" | "create" | "listMessages" | "update" | "remove" | "updateMessageMetadata"
>;

export interface IAIAssistantStore {
  messages: TAiMessage[];
  isGenerating: boolean;
  mode: TAiAssistantMode;
  activeIssueContext: TAiIssueContext | undefined;
  hasActiveIssue: boolean;
  conversations: TAiConversation[];
  activeConversationId: string | undefined;
  conversationsLoading: boolean;
  setWorkspace: (workspaceSlug: string | undefined) => void;
  setMode: (mode: TAiAssistantMode) => void;
  setActiveIssueContext: (context: TAiIssueContext | undefined) => void;
  loadConversations: () => Promise<void>;
  openConversation: (conversationId: string) => Promise<void>;
  newChat: () => void;
  renameConversation: (conversationId: string, title: string) => Promise<void>;
  deleteConversation: (conversationId: string) => Promise<void>;
  sendMessage: (question: string) => Promise<void>;
  retryLast: () => Promise<void>;
  confirmScheduleProposal: (messageId: string) => Promise<void>;
  resolveScheduleProposal: (messageId: string, decision: "cancelled") => void;
}

export const AI_ASSISTANT_STORAGE_PREFIX = "ai_assistant_messages_";
export const AI_ASSISTANT_ACTIVE_PREFIX = "ai_assistant_active_conversation_";
export const clearPersistedAiConversations = () => {
  try {
    const keys: string[] = [];
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index);
      if (key?.startsWith(AI_ASSISTANT_STORAGE_PREFIX) || key?.startsWith(AI_ASSISTANT_ACTIVE_PREFIX)) {
        keys.push(key);
      }
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

const activeStorageKey = (workspaceSlug: string | undefined, mode: TAiAssistantMode) =>
  `${AI_ASSISTANT_ACTIVE_PREFIX}${workspaceSlug ?? "unknown"}_${mode}`;

export class AIAssistantStore implements IAIAssistantStore {
  messages: TAiMessage[] = [];
  isGenerating = false;
  mode: TAiAssistantMode = "classic";
  activeIssueContext: TAiIssueContext | undefined = undefined;
  conversations: TAiConversation[] = [];
  activeConversationId: string | undefined = undefined;
  conversationsLoading = false;

  private workspaceSlug: string | undefined = undefined;
  private requestSeq = 0;
  private listSeq = 0;

  constructor(
    private aiService: TAiService = new AIService(),
    private schedulesService: TAiSchedulesService = new AiSchedulesService(),
    private conversationsService: TAiConversationsService = new AiConversationsService()
  ) {
    makeObservable(this, {
      messages: observable.deep,
      isGenerating: observable.ref,
      mode: observable.ref,
      activeIssueContext: observable.ref,
      conversations: observable.deep,
      activeConversationId: observable.ref,
      conversationsLoading: observable.ref,
      hasActiveIssue: computed,
      setWorkspace: action,
      setMode: action,
      setActiveIssueContext: action,
      loadConversations: action,
      openConversation: action,
      newChat: action,
      renameConversation: action,
      deleteConversation: action,
      sendMessage: action,
      retryLast: action,
      confirmScheduleProposal: action,
      resolveScheduleProposal: action,
    });
  }

  get hasActiveIssue(): boolean {
    return !!this.activeIssueContext;
  }

  setWorkspace = (workspaceSlug: string | undefined) => {
    if (workspaceSlug !== this.workspaceSlug) {
      this.activeIssueContext = undefined;
      this.isGenerating = false;
      this.requestSeq += 1;
      this.listSeq += 1;
      this.conversations = [];
      this.activeConversationId = undefined;
      this.messages = [];
    }
    this.workspaceSlug = workspaceSlug;
    this.mode = this.restoreMode();
    if (workspaceSlug) {
      this.clearLegacyMessages();
      void this.loadConversations();
    }
  };

  loadConversations = async () => {
    const slug = this.workspaceSlug;
    if (!slug) return;
    const seq = ++this.listSeq;
    this.conversationsLoading = true;
    try {
      const conversations = await this.conversationsService.list(slug);
      if (seq !== this.listSeq) return;
      runInAction(() => {
        this.conversations = conversations;
        this.conversationsLoading = false;
      });
      await this.openLastForMode();
    } catch {
      if (seq !== this.listSeq) return;
      runInAction(() => {
        this.conversationsLoading = false;
      });
    }
  };

  setMode = (mode: TAiAssistantMode) => {
    if (mode === this.mode) return;
    this.requestSeq += 1;
    this.isGenerating = false;
    this.mode = mode;
    this.persistMode();
    void this.openLastForMode();
  };

  openConversation = async (conversationId: string) => {
    const slug = this.workspaceSlug;
    const conversation = this.conversations.find((candidate) => candidate.id === conversationId);
    if (!slug || !conversation) return;
    const seq = ++this.requestSeq;
    this.isGenerating = false;
    this.activeConversationId = conversationId;
    this.mode = conversation.mode;
    this.persistMode();
    this.persistActiveId();
    try {
      const messages = await this.conversationsService.listMessages(slug, conversationId);
      if (seq !== this.requestSeq) return;
      runInAction(() => {
        this.messages = messages.map(toAiMessage);
      });
    } catch (error: any) {
      if (seq !== this.requestSeq) return;
      if (error?.status === 404) {
        runInAction(() => {
          this.conversations = this.conversations.filter((candidate) => candidate.id !== conversationId);
        });
        this.newChat();
      }
    }
  };

  newChat = () => {
    this.requestSeq += 1;
    this.isGenerating = false;
    runInAction(() => {
      this.activeConversationId = undefined;
      this.messages = [];
    });
    this.clearActiveId();
  };

  renameConversation = async (conversationId: string, title: string) => {
    const slug = this.workspaceSlug;
    const trimmed = title.trim();
    if (!slug || !trimmed) return;
    const updated = await this.conversationsService.update(slug, conversationId, trimmed);
    runInAction(() => {
      this.conversations = this.conversations.map((candidate) =>
        candidate.id === conversationId ? updated : candidate
      );
    });
  };

  deleteConversation = async (conversationId: string) => {
    const slug = this.workspaceSlug;
    if (!slug) return;
    await this.conversationsService.remove(slug, conversationId);
    runInAction(() => {
      this.conversations = this.conversations.filter((candidate) => candidate.id !== conversationId);
    });
    if (this.activeConversationId === conversationId) this.newChat();
  };

  setActiveIssueContext = (context: TAiIssueContext | undefined) => {
    runInAction(() => {
      this.activeIssueContext = context;
    });
  };

  sendMessage = async (question: string) => {
    const trimmed = question.trim();
    if (!trimmed || this.isGenerating || !this.workspaceSlug) return;
    if (isScheduleCommand(trimmed) && this.mode !== "agent") {
      this.setMode("agent");
      await this.openLastForMode();
    }
    const slug = this.workspaceSlug;
    const conversationId = await this.ensureConversation();
    if (!conversationId) return;
    const tempId = uuidv4();
    runInAction(() => {
      this.messages.push({ id: tempId, role: "user", content: trimmed });
    });
    await this.request(trimmed, slug, conversationId, tempId);
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
    });
    const slug = this.workspaceSlug;
    const conversationId = await this.ensureConversation();
    if (!conversationId) return;
    const tempId = uuidv4();
    runInAction(() => {
      this.messages.push({ id: tempId, role: "user", content: lastUserQuestion! });
    });
    await this.request(lastUserQuestion, slug, conversationId, tempId);
  };

  confirmScheduleProposal = async (messageId: string) => {
    const slug = this.workspaceSlug;
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!slug || !message?.scheduleProposal || !message.scheduleProposalKey) return;
    if (message.scheduleDecision !== "pending") return;
    const created = await this.schedulesService.create(slug, message.scheduleProposal, message.scheduleProposalKey);
    if (!created?.id) throw new Error("Schedule creation returned no id");
    runInAction(() => {
      message.scheduleDecision = "created";
      message.createdScheduleId = created.id;
    });
    await this.persistDecision(message, {
      schedule_decision: "created",
      created_schedule_id: created.id,
    });
  };

  resolveScheduleProposal = (messageId: string, decision: "cancelled") => {
    const message = this.messages.find((candidate) => candidate.id === messageId);
    if (!message || message.scheduleDecision !== "pending") return;
    runInAction(() => {
      message.scheduleDecision = decision;
    });
    void this.persistDecision(message, { schedule_decision: decision });
  };

  private ensureConversation = async (): Promise<string | undefined> => {
    const slug = this.workspaceSlug;
    if (!slug) return undefined;
    const active = this.conversations.find((candidate) => candidate.id === this.activeConversationId);
    if (active && active.mode === this.mode) return active.id;
    const created = await this.conversationsService.create(slug, this.mode);
    runInAction(() => {
      this.conversations = [created, ...this.conversations];
      this.activeConversationId = created.id;
    });
    this.persistActiveId();
    return created.id;
  };

  private persistDecision = async (
    message: TAiMessage,
    metadata: { schedule_decision: "created" | "cancelled"; created_schedule_id?: string }
  ) => {
    const slug = this.workspaceSlug;
    const conversationId = this.activeConversationId;
    if (!slug || !conversationId) return;
    try {
      await this.conversationsService.updateMessageMetadata(slug, conversationId, message.id, metadata);
    } catch {
      // best-effort: the schedule itself is already the source of truth
    }
  };

  private async request(question: string, slug: string, conversationId: string, optimisticId: string) {
    const mode = this.mode;
    const seq = ++this.requestSeq;
    runInAction(() => {
      this.isGenerating = true;
    });
    try {
      const userTimezone = mode === "agent" ? Intl.DateTimeFormat().resolvedOptions().timeZone : undefined;
      const payload = {
        task: AI_ASSISTANT_TASK,
        prompt: question,
        context: buildAiContext(this.activeIssueContext, userTimezone),
        conversation_id: conversationId,
      };
      const res =
        mode === "agent"
          ? await this.aiService.createAgentTask(slug, payload)
          : await this.aiService.createGptTask(slug, payload);
      if (seq !== this.requestSeq) return;
      const userMessage = toAiMessage(res.user_message);
      const assistantMessage = toAiMessage(res.assistant_message);
      if (mode === "agent" && res.pending_action?.kind === "create_schedule") {
        assistantMessage.scheduleProposal = res.pending_action.proposal;
        if (!assistantMessage.scheduleProposalKey) assistantMessage.scheduleProposalKey = uuidv4();
        if (!assistantMessage.scheduleDecision) assistantMessage.scheduleDecision = "pending";
      }
      runInAction(() => {
        const index = this.messages.findIndex((candidate) => candidate.id === optimisticId);
        if (index >= 0) {
          this.messages.splice(index, 1, userMessage);
        } else {
          this.messages.push(userMessage);
        }
        this.messages.push(assistantMessage);
        this.conversations = [
          res.conversation,
          ...this.conversations.filter((candidate) => candidate.id !== res.conversation.id),
        ];
      });
    } catch (err: any) {
      if (seq !== this.requestSeq) return;
      const errorContent =
        err?.status === 429
          ? err?.data?.error || "Rate limit exceeded."
          : err?.status === 400
            ? err?.data?.error || "AI is not configured for this instance."
            : "An internal error has occurred. Please try again.";
      runInAction(() => {
        this.messages.push({ id: uuidv4(), role: "assistant", content: errorContent, isError: true });
      });
    } finally {
      if (seq === this.requestSeq) {
        runInAction(() => {
          this.isGenerating = false;
        });
      }
    }
  }

  private openLastForMode = async () => {
    const remembered = this.restoreActiveId();
    if (remembered && this.conversations.some((candidate) => candidate.id === remembered)) {
      await this.openConversation(remembered);
      return;
    }
    const newest = this.conversations.find((candidate) => candidate.mode === this.mode);
    if (newest) {
      await this.openConversation(newest.id);
      return;
    }
    this.newChat();
  };

  private persistActiveId() {
    if (!this.workspaceSlug || !this.activeConversationId) return;
    try {
      localStorage.setItem(activeStorageKey(this.workspaceSlug, this.mode), this.activeConversationId);
    } catch {
      // storage unavailable — best-effort
    }
  }

  private restoreActiveId(): string | undefined {
    if (!this.workspaceSlug) return undefined;
    try {
      return localStorage.getItem(activeStorageKey(this.workspaceSlug, this.mode)) ?? undefined;
    } catch {
      return undefined;
    }
  }

  private clearActiveId() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.removeItem(activeStorageKey(this.workspaceSlug, this.mode));
    } catch {
      // storage unavailable — best-effort
    }
  }

  private clearLegacyMessages() {
    if (!this.workspaceSlug) return;
    try {
      localStorage.removeItem(storageKey(this.workspaceSlug));
    } catch {
      // storage unavailable — best-effort
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
      // storage unavailable — best-effort
    }
  }
}
