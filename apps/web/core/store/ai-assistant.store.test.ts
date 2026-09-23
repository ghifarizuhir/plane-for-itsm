import { beforeEach, describe, expect, it, vi } from "vitest";
import { AIAssistantStore, clearPersistedAiConversations } from "./ai-assistant.store";
import type { TAiIssueContext, TAiMessage } from "@/lib/ai-context";

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

type TCreateGptTask = (workspaceSlug: string, data: { prompt: string; task: string }) => Promise<any>;
type TCreateAgentTask = (workspaceSlug: string, data: { prompt: string; task: string }) => Promise<any>;

const makeService = (
  gptImpl: TCreateGptTask = async () => ({ response: "ok", response_html: "ok" }),
  agentImpl: TCreateAgentTask = async () => ({ response: "agent ok", response_html: "agent ok html", tool_calls: [] })
) => ({
  createGptTask: vi.fn(gptImpl) as unknown as TCreateGptTask,
  createAgentTask: vi.fn(agentImpl) as unknown as TCreateAgentTask,
});

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

const CONTEXT: TAiIssueContext = {
  name: "Login fails with SSO",
  descriptionHtml: "<p>SSO broken.</p>",
  state: "In Progress",
  priority: "high",
};

beforeEach(() => {
  vi.stubGlobal("localStorage", new LocalStorageStub());
});

describe("AIAssistantStore", () => {
  it("restores persisted messages when workspace is set", async () => {
    const first = new AIAssistantStore(makeService());
    first.setWorkspace("acme");
    await first.sendMessage("hello");
    expect(first.messages).toHaveLength(2);

    const second = new AIAssistantStore(makeService());
    second.setWorkspace("acme");
    expect(second.messages).toHaveLength(2);

    const third = new AIAssistantStore(makeService());
    third.setWorkspace("other-ws");
    expect(third.messages).toHaveLength(0);
  });

  it("sendMessage appends user + assistant message and builds prompt with context", async () => {
    const service = makeService(async (_slug, _data) => ({ response: "ok", response_html: "ok html" }));
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    store.setActiveIssueContext(CONTEXT);

    await store.sendMessage("what is wrong?");
    expect(store.isGenerating).toBe(false);
    expect(store.messages[0].role).toBe("user");
    expect(store.messages[0].content).toBe("what is wrong?");
    expect(store.messages[1].role).toBe("assistant");
    expect(store.messages[1].content).toBe("ok html");
    expect(store.messages[1].isError).toBe(false);

    const call = (service.createGptTask as any).mock.calls[0];
    expect(call[0]).toBe("acme");
    expect(call[1].task).toContain("ITSM work-item assistant");
    expect(call[1].prompt).toContain("Work item context:");
    expect(call[1].prompt).toContain("User's new question: what is wrong?");
  });

  it("sendMessage ignores empty or whitespace questions", async () => {
    const service = makeService();
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    await store.sendMessage("   ");
    expect(store.messages).toHaveLength(0);
    expect((service.createGptTask as any).mock.calls).toHaveLength(0);
  });

  it("maps errors to error bubbles and keeps the user message", async () => {
    const service = makeService(async () => {
      throw Object.assign(new Error("fail"), { status: 429, data: { error: "Rate limit exceeded for openrouter.ai" } });
    });
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    await store.sendMessage("hi");
    expect(store.messages).toHaveLength(2);
    expect(store.messages[1].isError).toBe(true);
    expect(store.messages[1].content).toBe("Rate limit exceeded for openrouter.ai");

    const service400 = makeService(async () => {
      throw Object.assign(new Error("fail"), {
        status: 400,
        data: { error: "LLM provider API key and model are required" },
      });
    });
    const store400 = new AIAssistantStore(service400);
    store400.setWorkspace("acme-400");
    await store400.sendMessage("hi");
    expect(store400.messages).toHaveLength(2);
    expect(store400.messages[1].isError).toBe(true);
    expect(store400.messages[1].content).toBe("AI is not configured for this instance.");

    const service500 = makeService(async () => {
      throw Object.assign(new Error("fail"), { status: 500 });
    });
    const store500 = new AIAssistantStore(service500);
    store500.setWorkspace("acme-500");
    await store500.sendMessage("hi");
    expect(store500.messages).toHaveLength(2);
    expect(store500.messages[1].isError).toBe(true);
    expect(store500.messages[1].content).toContain("internal error");
  });

  it("retryLast drops the trailing error and resends the last user question", async () => {
    const failing = makeService(async () => {
      throw Object.assign(new Error("fail"), { status: 500 });
    });
    const store = new AIAssistantStore(failing);
    store.setWorkspace("acme");
    await store.sendMessage("first question");
    await flush();

    const passing = makeService(async () => ({ response: "ok", response_html: "fixed" }));
    (store as unknown as { aiService: unknown }).aiService = passing;
    await store.retryLast();

    expect(store.messages).toHaveLength(2);
    expect(store.messages[0].role).toBe("user");
    expect(store.messages[0].content).toBe("first question");
    expect(store.messages[1].role).toBe("assistant");
    expect(store.messages[1].content).toBe("fixed");
    const call = (passing.createGptTask as any).mock.calls[0];
    expect(call[1].prompt).toContain("User's new question: first question");
  });

  it("retryLast is a no-op when the last message is not an error", async () => {
    const service = makeService(async () => ({ response: "ok", response_html: "ok" }));
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    await store.sendMessage("hello");
    expect(store.messages).toHaveLength(2);
    await store.retryLast();
    expect((service.createGptTask as any).mock.calls).toHaveLength(1);
    expect(store.messages).toHaveLength(2);
  });

  it("drops stale responses after workspace switch", async () => {
    let resolvePending!: (value: unknown) => void;
    const service = makeService(
      () =>
        new Promise((resolve) => {
          resolvePending = resolve;
        })
    );
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    const pending = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);
    store.setWorkspace("other");
    resolvePending({ response: "ok", response_html: "late answer" });
    await pending;
    store.setWorkspace("acme");
    expect(store.messages).toHaveLength(1);
    expect(store.messages[0].role).toBe("user");
    expect(store.messages[0].content).toBe("q1");
    store.setWorkspace("other");
    expect(store.messages).toHaveLength(0);
  });

  it("setWorkspace resets context and generation flag", async () => {
    let resolvePending!: (value: unknown) => void;
    const service = makeService(
      () =>
        new Promise((resolve) => {
          resolvePending = resolve;
        })
    );
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    store.setActiveIssueContext(CONTEXT);
    const pending = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);
    store.setWorkspace("other");
    expect(store.hasActiveIssue).toBe(false);
    expect(store.isGenerating).toBe(false);
    resolvePending({ response: "ok", response_html: "late" });
    await pending;
  });

  it("clearConversation empties messages and persists", async () => {
    const store = new AIAssistantStore(makeService());
    store.setWorkspace("acme");
    await store.sendMessage("hi");
    store.clearConversation();
    expect(store.messages).toHaveLength(0);

    const rehydrated = new AIAssistantStore(makeService());
    rehydrated.setWorkspace("acme");
    expect(rehydrated.messages).toHaveLength(0);
  });

  it("clearPersistedAiConversations purges stored chats across workspaces", async () => {
    const store = new AIAssistantStore(makeService());
    store.setWorkspace("acme");
    await store.sendMessage("hi");
    expect(store.messages).toHaveLength(2);
    clearPersistedAiConversations();
    const rehydrated = new AIAssistantStore(makeService());
    rehydrated.setWorkspace("acme");
    expect(rehydrated.messages).toHaveLength(0);
  });

  it("serializes sendMessage while generating", async () => {
    let resolveFirst!: (value: unknown) => void;
    const service = makeService(
      () =>
        new Promise((resolve) => {
          resolveFirst = resolve;
        })
    );
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    const first = store.sendMessage("q1");
    await flush();
    expect(store.isGenerating).toBe(true);
    await store.sendMessage("q2");
    expect(store.messages.map((m: TAiMessage) => m.content)).toEqual(["q1"]);
    resolveFirst?.({ response: "ok", response_html: "ok" });
    await first;
    expect(store.isGenerating).toBe(false);
  });

  it("defaults to classic mode and persists the mode per workspace", () => {
    const store = new AIAssistantStore(makeService());
    store.setWorkspace("acme");
    expect(store.mode).toBe("classic");

    store.setMode("agent");
    expect(store.mode).toBe("agent");

    const rehydrated = new AIAssistantStore(makeService());
    rehydrated.setWorkspace("acme");
    expect(rehydrated.mode).toBe("agent");

    const other = new AIAssistantStore(makeService());
    other.setWorkspace("other-ws");
    expect(other.mode).toBe("classic");
  });

  it("setMode clears the conversation and is a no-op for the active mode", async () => {
    const store = new AIAssistantStore(makeService());
    store.setWorkspace("acme");
    await store.sendMessage("hi");
    expect(store.messages).toHaveLength(2);

    store.setMode("agent");
    expect(store.mode).toBe("agent");
    expect(store.messages).toHaveLength(0);

    await store.sendMessage("again");
    expect(store.messages).toHaveLength(2);
    store.setMode("agent");
    expect(store.messages).toHaveLength(2);
  });

  it("agent mode calls createAgentTask and prefers response_html", async () => {
    const service = makeService(undefined, async () => ({
      response: "plain answer",
      response_html: "agent <b>answer</b>",
      tool_calls: [{ name: "count_work_items", arguments: { priority: "urgent" } }],
    }));
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    store.setActiveIssueContext(CONTEXT);
    store.setMode("agent");

    await store.sendMessage("how many urgent?");

    expect(store.messages).toHaveLength(2);
    expect(store.messages[1].content).toBe("agent <b>answer</b>");
    expect(store.messages[1].isError).toBe(false);

    const call = (service.createAgentTask as any).mock.calls[0];
    expect(call[0]).toBe("acme");
    expect(call[1].task).toContain("ITSM work-item assistant");
    expect(call[1].prompt).toContain("Work item context:");
    expect(call[1].prompt).toContain("User's new question: how many urgent?");
    expect((service.createGptTask as any).mock.calls).toHaveLength(0);
  });

  it("agent mode falls back to response when response_html is missing", async () => {
    const service = makeService(undefined, async () => ({ response: "plain text" }));
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    store.setMode("agent");

    await store.sendMessage("hi");

    expect(store.messages[1].content).toBe("plain text");
    expect(store.messages[1].isError).toBe(false);
  });

  it("agent mode maps errors to error bubbles and retries in the same mode", async () => {
    const service = makeService(undefined, async () => {
      throw Object.assign(new Error("fail"), {
        status: 429,
        data: { error: "Rate limit exceeded for openrouter.ai" },
      });
    });
    const store = new AIAssistantStore(service);
    store.setWorkspace("acme");
    store.setMode("agent");

    await store.sendMessage("hi");

    expect(store.messages).toHaveLength(2);
    expect(store.messages[1].isError).toBe(true);
    expect(store.messages[1].content).toBe("Rate limit exceeded for openrouter.ai");

    const passing = makeService(undefined, async () => ({ response: "fixed", response_html: "fixed html" }));
    (store as unknown as { aiService: unknown }).aiService = passing;
    await store.retryLast();

    expect(store.messages).toHaveLength(2);
    expect(store.messages[1].content).toBe("fixed html");
    expect((passing.createAgentTask as any).mock.calls).toHaveLength(1);
    expect((passing.createGptTask as any).mock.calls).toHaveLength(0);
  });
});
