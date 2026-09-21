# AI Assistant Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Panel AI assistant di sisi kanan aplikasi (workspace-wide, toggle dari top nav) yang menjawab Q&A + generate teks dengan konteks issue aktif, memakai endpoint `ai-assistant/` Rust yang sudah ada — tanpa perubahan backend.

**Architecture:** Frontend-only. MobX `AIAssistantStore` menyimpan percakapan + konteks issue aktif dan menyusun prompt per giliran via fungsi murni `buildAiPrompt` (`apps/web/core/lib/ai-context.ts`), lalu memanggil `AIService.createGptTask`. Toggle persist di `ThemeStore`. Panel fixed `right-0 z-[30]` di atas peek view, di-mount di `WorkspaceContentWrapper` agar workspace-wide. Komponen UI mengikuti token yang ada (`bg-surface-1`, `border-subtle`, `text-on-color`, dsb.).

**Tech Stack:** React (React Router), MobX (`makeObservable`), SWR, Tailwind tokens `@plane/ui`/propel icons, vitest (ditambahkan ke apps/web — belum ada test runner di sana).

**Spec:** `docs/superpowers/specs/2026-09-21-ai-assistant-sidebar-design.md`

**Konvensi repo:**
- Semua perintah dijalankan dari root repo `/home/ghifari/plane-for-itsm`.
- Internal import pakai `@/` (alias ke `apps/web/core`), external pakai `@plane/*` / `@makeplane/propel`.
- Jangan tambah komentar kode kecuali file lain di sekitarnya punya header copyright — pertahankan header copyright yang sudah ada.
- Verifikasi per task: `pnpm --filter=web exec vitest run <file>` untuk test; `pnpm --filter=web check:types` untuk typecheck.
- Catatan AGENTS.md: setelah kode web berubah dan harus terlihat di tunnel → `pnpm --filter=web build` lalu `systemctl --user restart plane-web-prod.service` (Task 8).

---

### Task 1: Setup vitest untuk apps/web

apps/web belum punya test runner. Tambahkan vitest (versi dari catalog pnpm-workspace) plus config minimal. Tanpa `jsdom` — test nanti men-stub `localStorage` sendiri.

**Files:**
- Modify: `apps/web/package.json`
- Create: `apps/web/vitest.config.ts`

- [ ] **Step 1: Edit `apps/web/package.json` — tambah script `test` dan devDep vitest**

Di dalam `"scripts"` tambahkan (setelah `"check:format"` barisnya atau di akhir blok scripts):

```json
    "test": "vitest run"
```

Di dalam `"devDependencies"` tambahkan:

```json
    "vitest": "catalog:"
```

- [ ] **Step 2: Create `apps/web/vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  test: {
    environment: "node",
    globals: true,
    include: ["core/**/*.test.ts"],
    passWithNoTests: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./core"),
    },
  },
});
```

- [ ] **Step 3: Install + jalankan sanity check**

Run:
```bash
pnpm install && pnpm --filter=web exec vitest run
```
Expected: vitest jalan tanpa error, keluar dengan "no test files" yang di-pass (`passWithNoTests`), exit code 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/package.json apps/web/vitest.config.ts pnpm-lock.yaml
git commit -m "feat(web): add vitest test runner setup"
```

---

### Task 2: Prompt builder murni `ai-context.ts` (TDD)

Fungsi murni tanpa dependency lintas package (regex-based HTML strip agar aman di node test).

**Files:**
- Create: `apps/web/core/lib/ai-context.ts`
- Test: `apps/web/core/lib/ai-context.test.ts`

- [ ] **Step 1: Write the failing test** — `apps/web/core/lib/ai-context.test.ts`

```ts
import { describe, expect, it } from "vitest";
import { AI_ASSISTANT_TASK, buildAiPrompt, stripHtml } from "./ai-context";
import type { TAiIssueContext, TAiMessage } from "./ai-context";

describe("stripHtml", () => {
  it("removes tags and collapses whitespace", () => {
    expect(stripHtml("<p>Some <b>bold</b> text</p>")).toBe("Some bold text");
  });

  it("decodes common entities", () => {
    expect(stripHtml("<p>A &amp; B</p>")).toBe("A & B");
    expect(stripHtml("<p>5 &lt; 6 &gt; 4</p>")).toBe("5 < 6 > 4");
  });

  it("returns empty string for empty html", () => {
    expect(stripHtml("")).toBe("");
  });
});

describe("AI_ASSISTANT_TASK", () => {
  it("is a non-empty instruction", () => {
    expect(AI_ASSISTANT_TASK.length).toBeGreaterThan(20);
  });
});

describe("buildAiPrompt", () => {
  const context: TAiIssueContext = {
    name: "Login fails with SSO",
    descriptionHtml: "<p>User cannot log in via SSO.</p>",
    state: "In Progress",
    priority: "high",
  };

  it("includes issue context, history, and the question", () => {
    const history = [
      { id: "1", role: "user" as const, content: "What causes this?" },
      { id: "2", role: "assistant" as const, content: "Probably token refresh." },
    ];
    const result = buildAiPrompt(context, history, "Summarize the root cause");
    expect(result).toContain("Work item context:");
    expect(result).toContain("Work item: Login fails with SSO");
    expect(result).toContain("Description: User cannot log in via SSO.");
    expect(result).toContain("State: In Progress");
    expect(result).toContain("Priority: high");
    expect(result).toContain("User: What causes this?");
    expect(result).toContain("Assistant: Probably token refresh.");
    expect(result).toContain("User's new question: Summarize the root cause");
  });

  it("omits context block fields that are missing", () => {
    const result = buildAiPrompt({ name: "X", descriptionHtml: "" }, [], "hello");
    expect(result).not.toContain("Description:");
    expect(result).not.toContain("State:");
    expect(result).not.toContain("Priority:");
    expect(result).toContain("(empty)");
  });

  it("uses no-context block when context is undefined", () => {
    const result = buildAiPrompt(undefined, [], "hello");
    expect(result).toContain("No active work item context");
  });

  it("truncates history to the last 8 messages", () => {
    const history = Array.from({ length: 12 }, (_, i) => ({
      id: String(i),
      role: "user" as const,
      content: `msg-${i}`,
    }));
    const result = buildAiPrompt(undefined, history, "final question");
    expect(result).not.toContain("msg-0\n");
    expect(result).not.toContain("User: msg-3");
    expect(result).toContain("User: msg-4");
    expect(result).toContain("User: msg-11");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
pnpm --filter=web exec vitest run core/lib/ai-context.test.ts
```
Expected: FAIL — `Failed to resolve import "./ai-context"` / module not found.

- [ ] **Step 3: Write the implementation** — `apps/web/core/lib/ai-context.ts`

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TAiIssueContext = {
  name: string;
  descriptionHtml: string;
  state?: string;
  priority?: string;
};

export type TAiMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  isError?: boolean;
};

export const AI_ASSISTANT_TASK =
  "You are an ITSM work-item assistant. Answer using the work item context below. Be concise, use bullet points. If generating text (description/comment), output the text only.";

const HISTORY_MESSAGE_LIMIT = 8;

export const stripHtml = (html: string): string =>
  html
    .replace(/<[^>]*>/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();

const buildContextBlock = (context: TAiIssueContext | undefined): string => {
  if (!context) return "No active work item context. Answer from general knowledge.";
  const parts = [`Work item: ${context.name}`];
  const description = stripHtml(context.descriptionHtml ?? "");
  if (description) parts.push(`Description: ${description}`);
  if (context.state) parts.push(`State: ${context.state}`);
  if (context.priority) parts.push(`Priority: ${context.priority}`);
  return `Work item context:\n${parts.join("\n")}`;
};

export const buildAiPrompt = (
  context: TAiIssueContext | undefined,
  history: TAiMessage[],
  question: string
): string => {
  const historyBlock = history
    .slice(-HISTORY_MESSAGE_LIMIT)
    .map((message) => `${message.role === "user" ? "User" : "Assistant"}: ${message.content}`)
    .join("\n");
  return `${buildContextBlock(context)}\n\nConversation so far:\n${historyBlock || "(empty)"}\n\nUser's new question: ${question}`;
};
```

- [ ] **Step 4: Run test to verify it passes**

Run:
```bash
pnpm --filter=web exec vitest run core/lib/ai-context.test.ts
```
Expected: PASS (semua test hijau).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-context.test.ts
git commit -m "feat(web): ai assistant prompt builder"
```

---

### Task 3: Toggle panel di ThemeStore

Tambahkan `aiSidebarCollapsed` + `toggleAiSidebar`, pola persis sidebar lain (persist localStorage).

**Files:**
- Modify: `apps/web/core/store/theme.store.ts` (tambahkan setelah `projectOverviewSidebarCollapsed` di tiga tempat + action baru di akhir)

- [ ] **Step 1: Edit `apps/web/core/store/theme.store.ts`**

1) Di `IThemeStore`, setelah `projectOverviewSidebarCollapsed: boolean | undefined;` (baris 21) tambahkan:

```ts
  aiSidebarCollapsed: boolean | undefined;
```

dan setelah `toggleProjectOverviewSidebar: (collapsed?: boolean) => void;` (baris 33) tambahkan:

```ts
  toggleAiSidebar: (collapsed?: boolean) => void;
```

2) Di class `ThemeStore`, setelah field `projectOverviewSidebarCollapsed: boolean | undefined = undefined;` (baris 48) tambahkan:

```ts
  aiSidebarCollapsed: boolean | undefined = undefined;
```

3) Di `makeObservable` setelah `toggleProjectOverviewSidebar: action,` (baris 75) tambahkan:

```ts
      aiSidebarCollapsed: observable.ref,
```
dan setelah `toggleProjectOverviewSidebar: action,` di bagian action tambahkan:

```ts
      toggleAiSidebar: action,
```

4) Di akhir class (setelah method `toggleProjectOverviewSidebar`, baris 197) tambahkan:

```ts
  /**
   * Toggle the ai assistant sidebar collapsed state
   * @param collapsed
   */
  toggleAiSidebar = (collapsed?: boolean) => {
    if (collapsed === undefined) {
      this.aiSidebarCollapsed = !this.aiSidebarCollapsed;
    } else {
      this.aiSidebarCollapsed = collapsed;
    }
    localStorage.setItem("ai_sidebar_collapsed", this.aiSidebarCollapsed.toString());
  };
```

Semua tambahan panel 3) harus diurutkan: `aiSidebarCollapsed: observable.ref` masuk ke daftar observable, `toggleAiSidebar: action` masuk ke daftar action.

- [ ] **Step 2: Verify typecheck**

Run:
```bash
pnpm --filter=web check:types
```
Expected: sukses tanpa error TS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/store/theme.store.ts
git commit -m "feat(web): theme store toggle for ai sidebar"
```

---

### Task 4: `AIAssistantStore` (TDD)

Store percakapan: messages, isGenerating, activeIssueContext, persist localStorage per workspace, `sendMessage`/`retryLast`/`clearConversation`. Service di-inject lewat constructor agar mudah dites.

**Files:**
- Create: `apps/web/core/store/ai-assistant.store.ts`
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Write the failing test** — `apps/web/core/store/ai-assistant.store.test.ts`

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AIAssistantStore } from "./ai-assistant.store";
import type { TAiIssueContext, TAiMessage } from "@/lib/ai-context";

class LocalStorageStub {
  private store = new Map<string, string>();
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

const makeService = (impl: TCreateGptTask = async () => ({ response: "ok", response_html: "ok" })) => ({
  createGptTask: vi.fn(impl) as unknown as TCreateGptTask,
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
    const service = makeService(async (_slug, data) => ({ response: "ok", response_html: "ok html" }));
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
      throw Object.assign(new Error("fail"), { status: 400, data: { error: "LLM provider API key and model are required" } });
    });
    const store400 = new AIAssistantStore(service400);
    store400.setWorkspace("acme");
    await store400.sendMessage("hi");
    expect(store400.messages[1].isError).toBe(true);
    expect(store400.messages[1].content).toBe("AI is not configured for this instance.");

    const service500 = makeService(async () => {
      throw Object.assign(new Error("fail"), { status: 500 });
    });
    const store500 = new AIAssistantStore(service500);
    store500.setWorkspace("acme");
    await store500.sendMessage("hi");
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

    expect(store.messages).toHaveLength(3);
    expect(store.messages[0].content).toBe("first question");
    expect(store.messages[1].content).toBe("first question");
    expect(store.messages[2].content).toBe("fixed");
    const call = (passing.createGptTask as any).mock.calls[0];
    expect(call[1].prompt).toContain("User's new question: first question");
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

  it("serializes sendMessage while generating", async () => {
    let resolveFirst!: (value: unknown) => void;
    const service = makeService(
      () => new Promise((resolve) => { resolveFirst = resolve; })
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
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts
```
Expected: FAIL — `Failed to resolve import "./ai-assistant.store"`.

- [ ] **Step 3: Write the implementation** — `apps/web/core/store/ai-assistant.store.ts`

```ts
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
```

- [ ] **Step 4: Run test to verify it passes**

Run:
```bash
pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts core/lib/ai-context.test.ts
```
Expected: PASS semua.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): ai assistant conversation store"
```

---

### Task 5: Register store di root + hook akses

**Files:**
- Modify: `apps/web/core/store/root.store.ts`
- Create: `apps/web/core/hooks/store/use-ai-assistant.ts`

- [ ] **Step 1: Edit `apps/web/core/store/root.store.ts`**

Tambahkan import (letakkan dekat import theme.store, baris 71–72):

```ts
import type { IAIAssistantStore } from "./ai-assistant.store";
import { AIAssistantStore } from "./ai-assistant.store";
```

Tambahkan field di class `CoreRootStore` setelah `timelineStore: ITimelineStore;` (baris 111):

```ts
  aiAssistant: IAIAssistantStore;
```

Di constructor setelah `this.timelineStore = new TimeLineStore(this);` (baris 145) tambahkan:

```ts
    this.aiAssistant = new AIAssistantStore();
```

Di `resetOnSignOut()` (setelah baris yang merecreate `timelineStore` — cari `this.timelineStore = new TimeLineStore(this);` di dalam resetOnSignOut, sekitar baris 183) tambahkan baris yang sama:

```ts
    this.aiAssistant = new AIAssistantStore();
```

- [ ] **Step 2: Create `apps/web/core/hooks/store/use-ai-assistant.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// store
import { StoreContext } from "@/lib/store-context";
import type { IAIAssistantStore } from "@/store/ai-assistant.store";

export const useAiAssistant = (): IAIAssistantStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useAiAssistant must be used within StoreProvider");
  return context.aiAssistant;
};
```

- [ ] **Step 3: Verify typecheck + test masih hijau**

Run:
```bash
pnpm --filter=web check:types && pnpm --filter=web exec vitest run
```
Expected: keduanya sukses.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/store/root.store.ts apps/web/core/hooks/store/use-ai-assistant.ts
git commit -m "feat(web): register ai assistant store and hook"
```

---

### Task 6: Tombol toggle di top nav (gated `has_llm_configured`)

**Files:**
- Create: `apps/web/core/components/ai/assistant-sidebar/toggle-button.tsx`
- Modify: `apps/web/core/components/navigation/top-navigation-root.tsx`

- [ ] **Step 1: Create `apps/web/core/components/ai/assistant-sidebar/toggle-button.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { AiStar1Outline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import { useAppTheme } from "@/hooks/store/use-app-theme";

export const AiAssistantSidebarToggle = observer(function AiAssistantSidebarToggle() {
  const { aiSidebarCollapsed, toggleAiSidebar } = useAppTheme();

  return (
    <Tooltip label="AI Assistant" side="bottom">
      <button
        type="button"
        onClick={() => toggleAiSidebar()}
        className={cn("flex size-8 items-center justify-center rounded-md transition-colors hover:bg-layer-1-hover", {
          "bg-layer-1": !aiSidebarCollapsed,
        })}
      >
        <AiStar1Outline className="size-5 text-primary" />
      </button>
    </Tooltip>
  );
});
```

- [ ] **Step 2: Edit `apps/web/core/components/navigation/top-navigation-root.tsx`**

Tambahkan imports:

```tsx
import { AiAssistantSidebarToggle } from "@/components/ai/assistant-sidebar/toggle-button";
import { useInstance } from "@/hooks/store/use-instance";
```

Di dalam komponen, setelah `const { preferences } = useAppRailPreferences();` (baris 31) tambahkan:

```tsx
  const { config } = useInstance();
```

Di JSX, di dalam `<div className="flex flex-1 shrink-0 items-center justify-end gap-1">` (baris 62), SEBELUM `<Tooltip label="Inbox" ...>` tambahkan:

```tsx
        {config?.has_llm_configured && <AiAssistantSidebarToggle />}
```

- [ ] **Step 3: Verify typecheck**

Run:
```bash
pnpm --filter=web check:types
```
Expected: sukses.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/toggle-button.tsx apps/web/core/components/navigation/top-navigation-root.tsx
git commit -m "feat(web): ai assistant sidebar toggle in top nav"
```

---

### Task 7: Komponen panel sidebar

**Files:**
- Create: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

- [ ] **Step 1: Create `apps/web/core/components/ai/assistant-sidebar/root.tsx`**

```tsx
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
          priority: issue.priority,
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

  if (aiSidebarCollapsed) return null;

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
```

Catatan implementasi:
- Tanpa outside-click close (disengaja — panel harus bisa dipakai sambil peek terbuka; close hanya via tombol X / toggle).
- `dangerouslySetInnerHTML` mem-preserve `response_html` paritas dengan popover GPT yang sudah ada.

- [ ] **Step 2: Verify typecheck**

Run:
```bash
pnpm --filter=web check:types
```
Expected: sukses (jika `useParams` generic typing komplain, ganti signature jadi `const params = useParams();` lalu akses `params.workspaceSlug`, `params.workItem`).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): ai assistant sidebar panel component"
```

---

### Task 8: Mount panel di workspace content wrapper + verifikasi akhir

**Files:**
- Modify: `apps/web/core/components/workspace/content-wrapper.tsx`

- [ ] **Step 1: Edit `apps/web/core/components/workspace/content-wrapper.tsx`**

Tambahkan imports:

```tsx
import { AiAssistantSidebar } from "@/components/ai/assistant-sidebar/root";
```

Di JSX, setelah `<TopNavigationRoot />` (baris 25) tambahkan:

```tsx
      <AiAssistantSidebar />
```

Panel fixed-positioning, jadi posisi render tidak memengaruhi layout; render kondisi (collapsed + gating) sudah ditangani di dalam komponen.

- [ ] **Step 2: Jalankan semua test + type + lint**

Run:
```bash
pnpm --filter=web exec vitest run && pnpm --filter=web check:types && pnpm --filter=web check:lint
```
Expected: semuanya sukses (lint threshold repo ada di `--max-warnings=11957`, tidak boleh naik).

- [ ] **Step 3: Build + restart prod web (AGENTS.md)**

Run:
```bash
pnpm --filter=web build && systemctl --user restart plane-web-prod.service
```
Expected: build sukses; service naik dan serve static baru di port 3000.

- [ ] **Step 4: Verifikasi manual di browser**

1. Buka `http://192.168.1.11:3000` → login → workspace `itsm`.
2. Tombol sparkle muncul di top nav kanan (karena `has_llm_configured=true`).
3. Klik sparkle → panel kanan muncul; header + context indicator "No active issue…".
4. Buka list issue → peek salah satu issue → panel terbuka menutupi tepi kanan peek; context indicator menampilkan judul issue.
5. Tanya "Summarize this issue" → bubble user + bubble AI muncul (loading "Generating response…" dulu).
6. Refresh halaman → percakapan masih ada (localStorage).
7. Klik tombol clear (History icon) → pesan kosong.
8. Tutup peek, buka halaman tanpa issue aktif → indicator "No active issue…".

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/workspace/content-wrapper.tsx
git commit -m "feat(web): mount ai assistant sidebar workspace-wide"
```

---

## Checklist spesifikasi → task

| Spec | Task |
| --- | --- |
| ThemeStore toggle + persist | Task 3 |
| AIAssistantStore + localStorage | Task 4–5 |
| Prompt builder / seam | Task 2, 4 |
| Tombol sparkle gated | Task 6 |
| Panel UX + error bubbles + gating | Task 7 |
| Mount workspace-wide | Task 8 |
| Testing (lib, store) | Task 2, 4 (component test panel ditunda — apps/web belum punya environment komponen; vitest node-only ditambahkan di plan ini dan unit store + lib mencakup perilaku inti) |
