import { beforeEach, describe, expect, it, vi } from "vitest";
import { AIAssistantStore, clearPersistedAiConversations } from "./ai-assistant.store";
import type { TAiIssueContext } from "@/lib/ai-context";
import type { TAiConversation, TAiStoredMessage } from "@/lib/ai-conversations";

class LocalStorageStub {
  private store = new Map<string, string>();
  get length() {
    return this.store.size;
  }
  key(index: number) {
    return Array.from(this.store.keys())[index] ?? null;
  }
  getItem(key: string) {
    return this.store.get(key) ?? null;
  }
  setItem(key: string, value: string) {
    this.store.set(key, value);
  }
  removeItem(key: string) {
    this.store.delete(key);
  }
  clear() {
    this.store.clear();
  }
}

const conversation = (
  id: string,
  mode: "classic" | "agent",
  overrides: Partial<TAiConversation> = {}
): TAiConversation => ({
  id,
  title: `conv ${id}`,
  mode,
  created_at: "2026-09-24T09:00:00Z",
  updated_at: "2026-09-24T09:00:00Z",
  ...overrides,
});

const storedMessage = (id: string, role: "user" | "assistant", content: string): TAiStoredMessage => ({
  id,
  role,
  content,
  content_html: role === "assistant" ? content : null,
  metadata: {},
  created_at: "2026-09-24T09:00:00Z",
});

const chatResponse = (conversationId: string, question: string, answer: string) => ({
  response: answer,
  response_html: answer,
  conversation: conversation(conversationId, "agent", { title: question, updated_at: "2026-09-24T10:00:00Z" }),
  user_message: storedMessage("srv-user", "user", question),
  assistant_message: storedMessage("srv-assistant", "assistant", answer),
});

const makeServices = (overrides: Partial<Record<string, any>> = {}) => ({
  ai: {
    createGptTask: vi.fn(async (_slug: string, data: any) =>
      chatResponse(data.conversation_id, data.prompt, "classic ok")
    ),
    createAgentTask: vi.fn(async (_slug: string, data: any) =>
      chatResponse(data.conversation_id, data.prompt, "agent ok")
    ),
    ...overrides.ai,
  },
  schedules: { create: vi.fn(async () => ({ id: "s1" })), ...overrides.schedules },
  conversations: {
    list: vi.fn(async () => [conversation("c-agent", "agent"), conversation("c-classic", "classic")]),
    create: vi.fn(async (_slug: string, mode: "classic" | "agent") => conversation(`c-new-${mode}`, mode)),
    listMessages: vi.fn(async () => [
      storedMessage("srv-1", "user", "old question"),
      storedMessage("srv-2", "assistant", "old answer"),
    ]),
    update: vi.fn(async (_slug: string, id: string, title: string) => conversation(id, "agent", { title })),
    remove: vi.fn(async () => undefined),
    updateMessageMetadata: vi.fn(async () => storedMessage("srv-assistant", "assistant", "ok")),
    ...overrides.conversations,
  },
});

const makeStore = (services = makeServices()) =>
  new AIAssistantStore(services.ai as any, services.schedules as any, services.conversations as any);

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

const CONTEXT: TAiIssueContext = {
  name: "Login fails with SSO",
  descriptionHtml: "<p>SSO broken.</p>",
  state: "In Progress",
  priority: "high",
};

const scheduleProposal = {
  name: "Daily",
  prompt: "Report",
  frequency: "daily" as const,
  time: "09:00",
  timezone: "UTC",
};

beforeEach(() => {
  vi.stubGlobal("localStorage", new LocalStorageStub());
});

describe("conversation history", () => {
  it("loads conversations on workspace set and opens the newest of the restored mode", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    expect(services.conversations.list).toHaveBeenCalledWith("acme");
    expect(store.conversations).toHaveLength(2);
    expect(store.activeConversationId).toBe("c-classic");
    expect(store.messages.map((m) => m.content)).toEqual(["old question", "old answer"]);
    expect(services.conversations.listMessages).toHaveBeenCalledWith("acme", "c-classic");
  });

  it("switching mode opens the newest conversation of that mode without clearing anything", async () => {
    const store = makeStore();
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    expect(store.activeConversationId).toBe("c-agent");
    expect(store.mode).toBe("agent");
  });

  it("newChat clears the active conversation without calling the API", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.newChat();
    expect(store.activeConversationId).toBeUndefined();
    expect(store.messages).toEqual([]);
    expect(services.conversations.create).not.toHaveBeenCalled();
  });

  it("rename and delete update the local list", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.renameConversation("c-classic", "Renamed");
    expect(store.conversations.find((c) => c.id === "c-classic")?.title).toBe("Renamed");
    await store.deleteConversation("c-classic");
    expect(store.conversations.find((c) => c.id === "c-classic")).toBeUndefined();
    expect(store.activeConversationId).toBeUndefined();
  });

  it("a 404 while opening a conversation resets to a new chat", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => [conversation("gone", "classic")]),
        listMessages: vi.fn(async () => {
          throw { status: 404 };
        }),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    expect(store.conversations).toEqual([]);
    expect(store.activeConversationId).toBeUndefined();
    expect(store.messages).toEqual([]);
  });

  it("clears legacy localStorage messages and keeps server history on sign-out", async () => {
    localStorage.setItem("ai_assistant_messages_acme", JSON.stringify([{ id: "legacy" }]));
    const store = makeStore();
    store.setWorkspace("acme");
    await flush();
    expect(localStorage.getItem("ai_assistant_messages_acme")).toBeNull();
    clearPersistedAiConversations();
    expect(store.conversations).toHaveLength(2);
  });
});

describe("stateful send flow", () => {
  it("creates a conversation on first send and replaces the optimistic bubble", async () => {
    const services = makeServices({
      conversations: {
        list: vi.fn(async () => []),
        create: vi.fn(async (_slug: string, mode: "classic" | "agent") => conversation("c-new", mode)),
        listMessages: vi.fn(async () => []),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.sendMessage("hello");
    expect(services.conversations.create).toHaveBeenCalledWith("acme", "agent");
    const payload = services.ai.createAgentTask.mock.calls[0][1];
    expect(payload.conversation_id).toBe("c-new");
    expect(payload.prompt).toBe("hello");
    expect(payload.context).toContain("No active work item context");
    expect(store.messages.map((m) => m.id)).toEqual(["srv-user", "srv-assistant"]);
    expect(store.conversations[0].id).toBe("c-new");
    expect(store.activeConversationId).toBe("c-new");
  });

  it("reuses the active conversation and updates the list title", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("second question");
    const payload = services.ai.createGptTask.mock.calls[0][1];
    expect(payload.conversation_id).toBe("c-classic");
    expect(store.conversations.find((c) => c.id === "c-classic")?.title).toBe("second question");
    expect(services.conversations.create).not.toHaveBeenCalled();
  });

  it("keeps an error bubble and re-sends on retry", async () => {
    const services = makeServices({
      ai: {
        createGptTask: vi
          .fn()
          .mockRejectedValueOnce(Object.assign(new Error("boom"), { status: 500 }))
          .mockResolvedValueOnce(chatResponse("c-classic", "retry me", "recovered")),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("retry me");
    expect(store.messages.at(-1)?.isError).toBe(true);
    await store.retryLast();
    expect(store.messages.at(-1)?.content).toBe("recovered");
    expect(store.messages.at(-1)?.isError).toBe(false);
  });

  it("confirming a proposal persists the decision metadata", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "proposal ready"),
          pending_action: {
            kind: "create_schedule",
            proposal: {
              name: "Laporan",
              prompt: "ringkas overdue",
              frequency: "weekly",
              time: "09:00",
              timezone: "Asia/Jakarta",
            },
          },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal");
    const message = store.messages.find((m) => m.scheduleProposal);
    expect(message?.scheduleProposalKey).toBeTruthy();

    await store.confirmScheduleProposal(message!.id);
    expect(services.schedules.create).toHaveBeenCalled();
    expect(message!.scheduleDecision).toBe("created");
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith(
      "acme",
      "c-agent",
      message!.id,
      expect.objectContaining({ schedule_decision: "created", created_schedule_id: "s1" })
    );
  });

  it("cancelling a proposal persists the cancelled decision", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "proposal ready"),
          pending_action: {
            kind: "create_schedule",
            proposal: {
              name: "Laporan",
              prompt: "ringkas overdue",
              frequency: "daily",
              time: "09:00",
              timezone: "UTC",
            },
          },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal");
    const message = store.messages.find((m) => m.scheduleProposal)!;
    store.resolveScheduleProposal(message.id, "cancelled");
    await flush();
    expect(message.scheduleDecision).toBe("cancelled");
    expect(services.conversations.updateMessageMetadata).toHaveBeenCalledWith("acme", "c-agent", message.id, {
      schedule_decision: "cancelled",
    });
  });
});

describe("AIAssistantStore", () => {
  it("sendMessage replaces the optimistic bubble and builds the context block", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setActiveIssueContext(CONTEXT);

    await store.sendMessage("what is wrong?");
    expect(store.isGenerating).toBe(false);
    expect(store.messages.at(-2)?.role).toBe("user");
    expect(store.messages.at(-2)?.content).toBe("what is wrong?");
    expect(store.messages.at(-1)?.role).toBe("assistant");
    expect(store.messages.at(-1)?.content).toBe("classic ok");
    expect(store.messages.at(-1)?.isError).toBe(false);

    const call = (services.ai.createGptTask as any).mock.calls[0];
    expect(call[0]).toBe("acme");
    expect(call[1].task).toContain("ITSM work-item assistant");
    expect(call[1].prompt).toBe("what is wrong?");
    expect(call[1].context).toContain("Work item context:");
    expect(call[1].context).toContain("Work item: Login fails with SSO");
    expect(call[1].context).toContain("Description: SSO broken.");
    expect(call[1].context).not.toContain("User timezone:");
  });

  it("sendMessage ignores empty or whitespace questions", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await store.sendMessage("   ");
    expect(store.messages).toHaveLength(0);
    expect((services.ai.createGptTask as any).mock.calls).toHaveLength(0);
  });

  it("maps errors to error bubbles and keeps the user message", async () => {
    const makeFailing = (status: number, data?: { error: string }) =>
      makeServices({
        ai: {
          createGptTask: vi.fn(async () => {
            throw Object.assign(new Error("fail"), { status, data });
          }),
        },
      });

    const rateLimited = makeStore(makeFailing(429, { error: "Rate limit exceeded for openrouter.ai" }));
    rateLimited.setWorkspace("acme-429");
    await flush();
    await rateLimited.sendMessage("hi");
    expect(rateLimited.messages.at(-2)?.role).toBe("user");
    expect(rateLimited.messages.at(-2)?.content).toBe("hi");
    expect(rateLimited.messages.at(-1)?.isError).toBe(true);
    expect(rateLimited.messages.at(-1)?.content).toBe("Rate limit exceeded for openrouter.ai");

    const badRequest = makeStore(makeFailing(400, { error: "LLM provider API key and model are required" }));
    badRequest.setWorkspace("acme-400");
    await flush();
    await badRequest.sendMessage("hi");
    expect(badRequest.messages.at(-2)?.content).toBe("hi");
    expect(badRequest.messages.at(-1)?.isError).toBe(true);
    expect(badRequest.messages.at(-1)?.content).toBe("LLM provider API key and model are required");

    const unconfigured = makeStore(makeFailing(400));
    unconfigured.setWorkspace("acme-400-plain");
    await flush();
    await unconfigured.sendMessage("hi");
    expect(unconfigured.messages.at(-1)?.isError).toBe(true);
    expect(unconfigured.messages.at(-1)?.content).toBe("AI is not configured for this instance.");

    const serverError = makeStore(makeFailing(500));
    serverError.setWorkspace("acme-500");
    await flush();
    await serverError.sendMessage("hi");
    expect(serverError.messages.at(-2)?.content).toBe("hi");
    expect(serverError.messages.at(-1)?.isError).toBe(true);
    expect(serverError.messages.at(-1)?.content).toContain("internal error");
  });

  it("retryLast drops the trailing error and resends the last user question", async () => {
    const services = makeServices({
      ai: {
        createGptTask: vi
          .fn()
          .mockRejectedValueOnce(Object.assign(new Error("fail"), { status: 500 }))
          .mockResolvedValueOnce(chatResponse("c-classic", "first question", "fixed")),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("first question");
    expect(store.messages.at(-1)?.isError).toBe(true);

    await store.retryLast();
    expect(store.messages.at(-1)?.role).toBe("assistant");
    expect(store.messages.at(-1)?.content).toBe("fixed");
    expect(store.messages.at(-1)?.isError).toBe(false);
    const call = (services.ai.createGptTask as any).mock.calls[1];
    expect(call[1].prompt).toBe("first question");
    expect(call[1].conversation_id).toBe("c-classic");
  });

  it("retryLast is a no-op when the last message is not an error", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("hello");
    expect(store.messages).toHaveLength(4);
    await store.retryLast();
    expect((services.ai.createGptTask as any).mock.calls).toHaveLength(1);
    expect(store.messages).toHaveLength(4);
  });

  it("drops stale responses after workspace switch", async () => {
    let resolvePending!: (value: unknown) => void;
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(
          () =>
            new Promise((resolve) => {
              resolvePending = resolve;
            })
        ),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    const pending = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);
    store.setWorkspace("other");
    resolvePending(chatResponse("c-classic", "q1", "late answer"));
    await pending;
    await flush();
    expect(store.messages.map((m) => m.content)).not.toContain("late answer");
  });

  it("setWorkspace resets context and generation flag", async () => {
    let resolvePending!: (value: unknown) => void;
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(
          () =>
            new Promise((resolve) => {
              resolvePending = resolve;
            })
        ),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setActiveIssueContext(CONTEXT);
    const pending = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);
    store.setWorkspace("other");
    expect(store.hasActiveIssue).toBe(false);
    expect(store.isGenerating).toBe(false);
    resolvePending(chatResponse("c-classic", "q1", "late"));
    await pending;
  });

  it("serializes sendMessage while generating", async () => {
    let resolveFirst!: (value: unknown) => void;
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(
          () =>
            new Promise((resolve) => {
              resolveFirst = resolve;
            })
        ),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    const first = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);
    await store.sendMessage("q2");
    expect(store.messages).toHaveLength(3);
    expect(store.messages.at(-1)?.content).toBe("q1");
    resolveFirst(chatResponse("c-classic", "q1", "ok"));
    await first;
    expect(store.isGenerating).toBe(false);
  });

  it("defaults to classic mode and persists the mode per workspace", () => {
    const store = makeStore();
    store.setWorkspace("acme");
    expect(store.mode).toBe("classic");

    store.setMode("agent");
    expect(store.mode).toBe("agent");

    const rehydrated = makeStore();
    rehydrated.setWorkspace("acme");
    expect(rehydrated.mode).toBe("agent");

    const other = makeStore();
    other.setWorkspace("other-ws");
    expect(other.mode).toBe("classic");
  });

  it("agent mode calls createAgentTask and prefers response_html", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "plain answer"),
          assistant_message: {
            ...storedMessage("srv-assistant", "assistant", "plain answer"),
            content_html: "agent <b>answer</b>",
          },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setActiveIssueContext(CONTEXT);
    store.setMode("agent");
    await flush();

    await store.sendMessage("how many urgent?");

    expect(store.messages.at(-2)?.content).toBe("how many urgent?");
    expect(store.messages.at(-1)?.content).toBe("agent <b>answer</b>");
    expect(store.messages.at(-1)?.isError).toBe(false);

    const call = (services.ai.createAgentTask as any).mock.calls[0];
    expect(call[0]).toBe("acme");
    expect(call[1].task).toContain("ITSM work-item assistant");
    expect(call[1].prompt).toBe("how many urgent?");
    expect(call[1].context).toContain("Work item context:");
    expect(call[1].context).toContain("User timezone:");
    expect((services.ai.createGptTask as any).mock.calls).toHaveLength(0);
  });

  it("passes the browser timezone to agent prompts", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("hello");
    const call = (services.ai.createAgentTask as any).mock.calls[0];
    expect(call[1].context).toContain("User timezone:");
  });

  it("agent mode falls back to stored plain content when html is missing", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "plain text"),
          assistant_message: { ...storedMessage("srv-assistant", "assistant", "plain text"), content_html: null },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.sendMessage("hi");

    expect(store.messages.at(-1)?.content).toBe("plain text");
    expect(store.messages.at(-1)?.isError).toBe(false);
  });

  it("agent mode maps errors to error bubbles and retries in the same mode", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi
          .fn()
          .mockRejectedValueOnce(
            Object.assign(new Error("fail"), {
              status: 429,
              data: { error: "Rate limit exceeded for openrouter.ai" },
            })
          )
          .mockResolvedValueOnce(chatResponse("c-agent", "hi", "fixed html")),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();

    await store.sendMessage("hi");
    expect(store.messages.at(-1)?.isError).toBe(true);
    expect(store.messages.at(-1)?.content).toBe("Rate limit exceeded for openrouter.ai");

    await store.retryLast();
    expect(store.messages.at(-1)?.content).toBe("fixed html");
    expect(store.messages.at(-1)?.isError).toBe(false);
    expect((services.ai.createAgentTask as any).mock.calls).toHaveLength(2);
    expect((services.ai.createGptTask as any).mock.calls).toHaveLength(0);
  });

  it("drops stale responses after a mode switch", async () => {
    let resolvePending!: (value: unknown) => void;
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(
          () =>
            new Promise((resolve) => {
              resolvePending = resolve;
            })
        ),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    const pending = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);

    store.setMode("agent");
    resolvePending(chatResponse("c-classic", "q1", "late answer"));
    await pending;
    await flush();

    expect(store.messages.map((m) => m.content)).not.toContain("late answer");
    expect(store.isGenerating).toBe(false);
  });

  it("auto-switches to agent mode for /schedule commands", async () => {
    const services = makeServices();
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("/schedule daily overdue report");
    expect(store.mode).toBe("agent");
    expect(services.ai.createAgentTask).toHaveBeenCalled();
    expect(services.ai.createGptTask).not.toHaveBeenCalled();
    expect(store.messages.some((m) => m.role === "user" && m.content.includes("/schedule"))).toBe(true);
  });

  it("ignores pending_action in classic mode", async () => {
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "ok"),
          pending_action: { kind: "create_schedule", proposal: scheduleProposal },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    await store.sendMessage("hello");
    const message = store.messages.at(-1);
    expect(message?.scheduleProposal).toBeUndefined();
    expect(message?.scheduleProposalKey).toBeUndefined();
    expect(message?.scheduleDecision).toBeUndefined();
  });

  it("records pending_action metadata and confirms a proposal", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "ok"),
          pending_action: { kind: "create_schedule", proposal: scheduleProposal },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal harian");
    const message = store.messages.at(-1)!;
    expect(message.scheduleProposal?.name).toBe("Daily");
    expect(message.scheduleDecision).toBe("pending");
    expect(message.scheduleProposalKey).toBeTruthy();

    await store.confirmScheduleProposal(message.id);
    expect(services.schedules.create).toHaveBeenCalledWith(
      "acme",
      expect.objectContaining({ name: "Daily" }),
      message.scheduleProposalKey
    );
    expect(message.scheduleDecision).toBe("created");
    expect(message.createdScheduleId).toBe("s1");
  });

  it("keeps the proposal pending when confirm fails", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "ok"),
          pending_action: { kind: "create_schedule", proposal: scheduleProposal },
        })),
      },
      schedules: {
        create: vi.fn(async () => {
          throw new Error("boom");
        }),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal harian");
    const message = store.messages.at(-1)!;
    await expect(store.confirmScheduleProposal(message.id)).rejects.toThrow("boom");
    expect(message.scheduleDecision).toBe("pending");
  });

  it("cancels a proposal without calling the service", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "ok"),
          pending_action: { kind: "create_schedule", proposal: scheduleProposal },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("buat jadwal harian");
    const message = store.messages.at(-1)!;
    store.resolveScheduleProposal(message.id, "cancelled");
    await flush();
    expect(services.schedules.create).not.toHaveBeenCalled();
    expect(message.scheduleDecision).toBe("cancelled");
  });

  it("does not confirm a cancelled proposal", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "ok"),
          pending_action: { kind: "create_schedule", proposal: scheduleProposal },
        })),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("hello");
    const message = store.messages.at(-1)!;
    store.resolveScheduleProposal(message.id, "cancelled");
    await store.confirmScheduleProposal(message.id);
    expect(services.schedules.create).not.toHaveBeenCalled();
    expect(message.scheduleDecision).toBe("cancelled");
  });

  it("rejects a malformed create response and keeps the proposal pending", async () => {
    const services = makeServices({
      ai: {
        createAgentTask: vi.fn(async (_slug: string, data: any) => ({
          ...chatResponse(data.conversation_id, data.prompt, "ok"),
          pending_action: { kind: "create_schedule", proposal: scheduleProposal },
        })),
      },
      schedules: { create: vi.fn(async () => ({})) },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();
    store.setMode("agent");
    await flush();
    await store.sendMessage("hello");
    const message = store.messages.at(-1)!;
    await expect(store.confirmScheduleProposal(message.id)).rejects.toThrow("no id");
    expect(message.scheduleDecision).toBe("pending");
  });

  it("drops stale responses after a mode switch away and back", async () => {
    let resolvePending!: (value: unknown) => void;
    const services = makeServices({
      ai: {
        createGptTask: vi.fn(
          () =>
            new Promise((resolve) => {
              resolvePending = resolve;
            })
        ),
      },
    });
    const store = makeStore(services);
    store.setWorkspace("acme");
    await flush();

    const pending = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);

    store.setMode("agent");
    store.setMode("classic");
    resolvePending(chatResponse("c-classic", "q1", "late answer"));
    await pending;
    await flush();

    expect(store.messages.map((m) => m.content)).not.toContain("late answer");
    expect(store.isGenerating).toBe(false);
  });
});
