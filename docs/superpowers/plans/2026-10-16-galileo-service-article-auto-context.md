# Galileo Auto-Context for Services & KB Articles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user views a service or KB article detail, Galileo automatically grounds answers in that entity (Agent mode), the same way it already does for tickets.

**Architecture:** Generalize the client-side context block into a tagged union (`work_item | service | article`) in `apps/web/core/lib/ai-context.ts`; the sidebar resolves the on-screen entity from existing stores and sets a single active context; the store attaches service/article context only in Agent mode. Backend is untouched — `context` stays an opaque string.

**Tech Stack:** React + MobX (web), Vitest, TypeScript strict, oxlint/oxfmt.

**Spec:** `docs/superpowers/specs/2026-10-16-galileo-service-article-auto-context-design.md`

---

## File structure

- `apps/web/core/lib/ai-context.ts` (modify) — context union types, per-kind builder, 2,000-char truncation, neutral task wording.
- `apps/web/core/lib/ai-context.test.ts` (modify) — per-kind block tests, truncation, fallback, task wording.
- `apps/web/core/store/ai-assistant.store.ts` (modify) — rename to `activeContext`/`setActiveContext`/`hasActiveContext`; attach gating in `request()`.
- `apps/web/core/store/ai-assistant.store.test.ts` (modify) — fixtures gain `kind`; rename; attach-gating tests.
- `apps/web/core/components/ai/assistant-sidebar/root.tsx` (modify) — service/article resolution + strip UI + empty-state copy.

No backend changes. No i18n changes.

---

### Task 1: Per-entity context model & builder

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts`
- Test: `apps/web/core/lib/ai-context.test.ts`

- [ ] **Step 1: Rewrite the tests first**

Replace the entire contents of `apps/web/core/lib/ai-context.test.ts` with:

```ts
import { describe, expect, it } from "vitest";
import { EPageAccess } from "@plane/types";
import {
  AI_ASSISTANT_TASK,
  buildAiContext,
  CONTEXT_DESCRIPTION_LIMIT,
  sanitizeAssistantHtml,
  stripHtml,
} from "./ai-context";
import type { TAiAssistantContext } from "./ai-context";

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

  it("is entity-neutral", () => {
    expect(AI_ASSISTANT_TASK).toContain("ITSM assistant");
    expect(AI_ASSISTANT_TASK).not.toContain("work-item");
  });
});

describe("sanitizeAssistantHtml", () => {
  it("keeps formatting tags", () => {
    expect(sanitizeAssistantHtml("<b>bold</b><br/>line")).toBe("<b>bold</b><br />line");
    expect(sanitizeAssistantHtml("<ul><li>one</li></ul>")).toBe("<ul><li>one</li></ul>");
  });

  it("strips scripts, event handlers, and unsafe URLs", () => {
    expect(sanitizeAssistantHtml("<script>alert(1)</script>")).toBe("");
    expect(sanitizeAssistantHtml('<img src=x onerror="alert(1)">')).toBe("");
    expect(sanitizeAssistantHtml('<a href="javascript:alert(1)">x</a>')).toBe(
      '<a target="_blank" rel="noopener noreferrer">x</a>'
    );
  });

  it("forces safe link attributes and preserves tables", () => {
    expect(sanitizeAssistantHtml('<a href="https://example.com" target="_self" rel="opener">x</a>')).toBe(
      '<a href="https://example.com" target="_blank" rel="noopener noreferrer">x</a>'
    );
    expect(sanitizeAssistantHtml('<a href="//evil.example">x</a>')).toBe(
      '<a target="_blank" rel="noopener noreferrer">x</a>'
    );
    expect(sanitizeAssistantHtml('<p onclick="x()" style="color:red">hi</p>')).toBe("<p>hi</p>");
    expect(sanitizeAssistantHtml("<table><tr><td>a</td><td>b</td></tr></table>")).toBe(
      "<table><tr><td>a</td><td>b</td></tr></table>"
    );
  });
});

describe("buildAiContext", () => {
  const context: TAiAssistantContext = {
    kind: "work_item",
    name: "Login fails with SSO",
    descriptionHtml: "<p>User cannot log in via SSO.</p>",
    state: "In Progress",
    priority: "high",
  };

  it("includes work item context and timezone but not history", () => {
    const result = buildAiContext(context, "Asia/Jakarta");
    expect(result).toContain("Work item context:");
    expect(result).toContain("Work item: Login fails with SSO");
    expect(result).toContain("State: In Progress");
    expect(result).toContain("User timezone: Asia/Jakarta");
    expect(result).not.toContain("Conversation so far:");
  });

  it("omits work item fields that are missing", () => {
    const result = buildAiContext({ kind: "work_item", name: "X", descriptionHtml: "" });
    expect(result).not.toContain("Description:");
    expect(result).not.toContain("State:");
    expect(result).not.toContain("User timezone:");
  });

  it("builds a service context block with id and status", () => {
    const result = buildAiContext({
      kind: "service",
      id: "svc-1",
      name: "Payments API",
      descriptionHtml: "<p>Handles card payments.</p>",
      status: "active",
    });
    expect(result).toContain("Service context:");
    expect(result).toContain("Service: Payments API");
    expect(result).toContain("Service id: svc-1");
    expect(result).toContain("Status: active");
    expect(result).toContain("Description: Handles card payments.");
  });

  it("builds an article context block with id and access", () => {
    const publicArticle = buildAiContext({
      kind: "article",
      id: "art-1",
      name: "On-call runbook",
      descriptionHtml: "<p>Steps.</p>",
      access: EPageAccess.PUBLIC,
    });
    expect(publicArticle).toContain("Article context:");
    expect(publicArticle).toContain("Article: On-call runbook");
    expect(publicArticle).toContain("Article id: art-1");
    expect(publicArticle).toContain("Access: Public");
    expect(publicArticle).toContain("Description: Steps.");

    const privateArticle = buildAiContext({
      kind: "article",
      id: "art-2",
      name: "Secrets",
      descriptionHtml: "",
      access: EPageAccess.PRIVATE,
    });
    expect(privateArticle).toContain("Access: Private");
  });

  it("truncates long descriptions for every kind", () => {
    const longDescription = "x".repeat(CONTEXT_DESCRIPTION_LIMIT + 500);
    const service = buildAiContext({
      kind: "service",
      id: "svc-1",
      name: "Payments API",
      descriptionHtml: `<p>${longDescription}</p>`,
    });
    expect(service).toContain(`${"x".repeat(CONTEXT_DESCRIPTION_LIMIT)}…`);
    expect(service).not.toContain("x".repeat(CONTEXT_DESCRIPTION_LIMIT + 1));

    const workItem = buildAiContext({
      kind: "work_item",
      name: "X",
      descriptionHtml: `<p>${longDescription}</p>`,
    });
    expect(workItem).toContain(`${"x".repeat(CONTEXT_DESCRIPTION_LIMIT)}…`);
  });

  it("falls back to general knowledge without an active context", () => {
    expect(buildAiContext(undefined)).toBe("No active context. Answer from general knowledge.");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm --filter=web exec vitest run core/lib/ai-context.test.ts`
Expected: FAIL — `CONTEXT_DESCRIPTION_LIMIT` is not exported, `TAiAssistantContext` does not exist, old fallback string mismatch.

- [ ] **Step 3: Implement `ai-context.ts`**

Replace the entire contents of `apps/web/core/lib/ai-context.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { EPageAccess } from "@plane/types";
import sanitizeHtml from "sanitize-html";
import type { TAiProposal, TAiProposalDecision } from "@/lib/ai-proposals";
import type { TAiScheduleProposal } from "@/lib/ai-schedule";
import type { TAiWorkItemDecision, TAiWorkItemProposalEntry } from "@/lib/ai-work-items";

export type TAiContextKind = "work_item" | "service" | "article";

export type TAiWorkItemContext = {
  kind: "work_item";
  name: string;
  descriptionHtml: string;
  state?: string;
  priority?: string;
};

export type TAiServiceContext = {
  kind: "service";
  id: string;
  name: string;
  descriptionHtml: string;
  status?: string;
};

export type TAiArticleContext = {
  kind: "article";
  id: string;
  name: string;
  descriptionHtml: string;
  access?: EPageAccess;
};

export type TAiAssistantContext = TAiWorkItemContext | TAiServiceContext | TAiArticleContext;

export type TAiMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  isError?: boolean;
  scheduleProposal?: TAiScheduleProposal;
  scheduleProposalKey?: string;
  scheduleDecision?: "pending" | "created" | "cancelled";
  createdScheduleId?: string;
  workItemProposals?: TAiWorkItemProposalEntry[];
  workItemDecisions?: Record<string, TAiWorkItemDecision>;
  proposals?: TAiProposal[];
  proposalDecisions?: Record<string, TAiProposalDecision>;
  flow?: string | null;
  flowRef?: string | null;
  createdAt?: string;
};

export const AI_ASSISTANT_TASK =
  "You are an ITSM assistant. Answer using the context below when present — the work item, service, or knowledge base article the user is viewing. Be concise, use bullet points. If generating text (description/comment), output the text only.";

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

export const CONTEXT_DESCRIPTION_LIMIT = 2000;

const truncateDescription = (text: string): string =>
  text.length <= CONTEXT_DESCRIPTION_LIMIT ? text : `${text.slice(0, CONTEXT_DESCRIPTION_LIMIT)}…`;

const ASSISTANT_ALLOWED_TAGS = [
  "b",
  "strong",
  "i",
  "em",
  "u",
  "s",
  "br",
  "p",
  "ul",
  "ol",
  "li",
  "code",
  "pre",
  "blockquote",
  "h1",
  "h2",
  "h3",
  "h4",
  "hr",
  "a",
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "th",
  "td",
];

/** Allowlist-sanitize model output before it is injected as HTML. */
export const sanitizeAssistantHtml = (html: string): string =>
  sanitizeHtml(html, {
    allowedTags: ASSISTANT_ALLOWED_TAGS,
    allowedAttributes: { a: ["href", "target", "rel"] },
    transformTags: {
      a: sanitizeHtml.simpleTransform("a", { target: "_blank", rel: "noopener noreferrer" }),
    },
    allowProtocolRelative: false,
  });

const buildContextBlock = (context: TAiAssistantContext | undefined): string => {
  if (!context) return "No active context. Answer from general knowledge.";
  const description = truncateDescription(stripHtml(context.descriptionHtml ?? ""));
  switch (context.kind) {
    case "work_item": {
      const parts = [`Work item: ${context.name}`];
      if (description) parts.push(`Description: ${description}`);
      if (context.state) parts.push(`State: ${context.state}`);
      if (context.priority) parts.push(`Priority: ${context.priority}`);
      return `Work item context:\n${parts.join("\n")}`;
    }
    case "service": {
      const parts = [`Service: ${context.name}`, `Service id: ${context.id}`];
      if (context.status) parts.push(`Status: ${context.status}`);
      if (description) parts.push(`Description: ${description}`);
      return `Service context:\n${parts.join("\n")}`;
    }
    case "article": {
      const parts = [`Article: ${context.name}`, `Article id: ${context.id}`];
      if (context.access !== undefined) {
        parts.push(`Access: ${context.access === EPageAccess.PRIVATE ? "Private" : "Public"}`);
      }
      if (description) parts.push(`Description: ${description}`);
      return `Article context:\n${parts.join("\n")}`;
    }
  }
};

export const buildAiContext = (context: TAiAssistantContext | undefined, userTimezone?: string): string => {
  const timezoneBlock = userTimezone ? `\nUser timezone: ${userTimezone}` : "";
  return `${buildContextBlock(context)}${timezoneBlock}`;
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=web exec vitest run core/lib/ai-context.test.ts`
Expected: PASS (all tests in the file).

Note: the store test file still references `TAiIssueContext` / `setActiveIssueContext` and will fail typecheck until Task 2. That is expected.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-context.test.ts
git commit -m "feat(web): per-entity ai context builder for services and articles"
```

---

### Task 2: Store rename + Agent-only attach gating

**Files:**

- Modify: `apps/web/core/store/ai-assistant.store.ts`
- Test: `apps/web/core/store/ai-assistant.store.test.ts`

- [ ] **Step 1: Update the store tests first**

In `apps/web/core/store/ai-assistant.store.test.ts`:

1. Replace the imports (lines 1–4):

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { convertBase64StringToBinaryData, getAllDocumentFormatsFromDocumentEditorBinaryData } from "@plane/editor";
import { EPageAccess } from "@plane/types";
import { AIAssistantStore, AI_ASSISTANT_ACTIVE_PREFIX, clearPersistedAiConversations } from "./ai-assistant.store";
import type { TAiAssistantContext } from "@/lib/ai-context";
```

2. Replace the `CONTEXT` fixture (around line 152):

```ts
const CONTEXT: TAiAssistantContext = {
  kind: "work_item",
  name: "Login fails with SSO",
  descriptionHtml: "<p>SSO broken.</p>",
  state: "In Progress",
  priority: "high",
};

const SERVICE_CONTEXT: TAiAssistantContext = {
  kind: "service",
  id: "svc-1",
  name: "Payments API",
  descriptionHtml: "<p>Handles card payments.</p>",
  status: "active",
};

const ARTICLE_CONTEXT: TAiAssistantContext = {
  kind: "article",
  id: "art-1",
  name: "On-call runbook",
  descriptionHtml: "<p>Steps.</p>",
  access: EPageAccess.PUBLIC,
};
```

3. Replace every remaining reference:
   - `store.setActiveIssueContext(CONTEXT);` → `store.setActiveContext(CONTEXT);` (3 occurrences)
   - `expect(store.hasActiveIssue).toBe(false);` → `expect(store.hasActiveContext).toBe(false);` (1 occurrence)
   - `expect(payload.context).toContain("No active work item context");` → `expect(payload.context).toContain("No active context");` (1 occurrence)
   - `expect(call[1].task).toContain("ITSM work-item assistant");` → `expect(call[1].task).toContain("ITSM assistant");` (2 occurrences)

4. Add a new gating test right after the first `sendMessage` test. Anchor — after this block:

```ts
    expect(call[1].context).toContain("Description: SSO broken.");
    expect(call[1].context).not.toContain("User timezone:");
  });
```

insert:

```ts
it("attaches service and article context in agent mode only", async () => {
  const services = makeServices();
  const store = makeStore(services);
  store.setWorkspace("acme");
  await flush();
  store.setActiveContext(SERVICE_CONTEXT);

  await store.sendMessage("first");
  const classicCall = (services.ai.createGptTask as any).mock.calls[0];
  expect(classicCall[1].context).toContain("No active context");

  store.setActiveContext(ARTICLE_CONTEXT);
  store.setMode("agent");
  await flush();

  await store.sendMessage("second");
  const articleCall = (services.ai.createAgentTask as any).mock.calls[0];
  expect(articleCall[1].context).toContain("Article context:");

  store.setActiveContext(SERVICE_CONTEXT);
  await store.sendMessage("third");
  const serviceCall = (services.ai.createAgentTask as any).mock.calls[1];
  expect(serviceCall[1].context).toContain("Service context:");
  expect((services.ai.createGptTask as any).mock.calls).toHaveLength(1);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts`
Expected: FAIL — `store.setActiveContext` / `store.hasActiveContext` do not exist (store still exposes the old names).

- [ ] **Step 3: Implement the store changes**

In `apps/web/core/store/ai-assistant.store.ts`:

1. Import (line 28):

```ts
import type { TAiAssistantContext, TAiMessage } from "@/lib/ai-context";
```

2. Interface (`IAIAssistantStore`) — rename fields:

```ts
  activeContext: TAiAssistantContext | undefined;
  hasActiveContext: boolean;
  ...
  setActiveContext: (context: TAiAssistantContext | undefined) => void;
```

3. Class field:

```ts
  activeContext: TAiAssistantContext | undefined = undefined;
```

4. `makeObservable` — rename entries:

```ts
      activeContext: observable.ref,
      hasActiveContext: computed,
      setActiveContext: action,
```

5. Computed getter:

```ts
  get hasActiveContext(): boolean {
    return !!this.activeContext;
  }
```

6. `setWorkspace` reset — inside the `if (workspaceSlug !== this.workspaceSlug)` branch:

```ts
this.activeContext = undefined;
```

7. Setter:

```ts
setActiveContext = (context: TAiAssistantContext | undefined) => {
  runInAction(() => {
    this.activeContext = context;
  });
};
```

8. `request()` payload — insert the `attachable` gate and use it:

```ts
const userTimezone = mode === "agent" ? Intl.DateTimeFormat().resolvedOptions().timeZone : undefined;
const attachable = mode === "agent" || this.activeContext?.kind === "work_item" ? this.activeContext : undefined;
const payload = {
  task: AI_ASSISTANT_TASK,
  prompt: question,
  context: buildAiContext(attachable, userTimezone),
  conversation_id: conversationId,
  ...(mode === "agent" && this.activeFlow ? { flow: this.activeFlow.id, flow_ref: this.activeFlow.ref } : {}),
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=web exec vitest run core/store/ai-assistant.store.test.ts`
Expected: PASS (full file, including pre-existing conversation/proposal tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/ai-assistant.store.ts apps/web/core/store/ai-assistant.store.test.ts
git commit -m "feat(web): gate service and article context to agent mode"
```

---

### Task 3: Sidebar resolution + context strip UI

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

No component test harness exists for this sidebar; verification is typecheck, the existing unit suites, and manual smoke.

- [ ] **Step 1: Update imports**

Replace:

```ts
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import type { TIssue } from "@plane/types";
import { useTranslation } from "@plane/i18n";
// hooks
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { useAppTheme } from "@/hooks/store/use-app-theme";
import { useInstance } from "@/hooks/store/use-instance";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useProjectState } from "@/hooks/store/use-project-state";
// lib
import { sanitizeAssistantHtml, type TAiIssueContext } from "@/lib/ai-context";
```

with:

```ts
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import { EPageAccess, type TIssue } from "@plane/types";
import { useTranslation } from "@plane/i18n";
// hooks
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { useAppTheme } from "@/hooks/store/use-app-theme";
import { useInstance } from "@/hooks/store/use-instance";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { EPageStoreType, usePageStore } from "@/hooks/store/use-page-store";
import { useProjectState } from "@/hooks/store/use-project-state";
import { useService } from "@/hooks/store/use-service";
// lib
import { sanitizeAssistantHtml, type TAiAssistantContext } from "@/lib/ai-context";
```

- [ ] **Step 2: Add kind label/noun constants**

After the `MODES` constant add:

```ts
const CONTEXT_KIND_LABEL: Record<TAiAssistantContext["kind"], string> = {
  work_item: "Ticket",
  service: "Service",
  article: "Article",
};

const CONTEXT_KIND_NOUN: Record<TAiAssistantContext["kind"], string> = {
  work_item: "work item",
  service: "service",
  article: "article",
};
```

- [ ] **Step 3: Replace routing, store hooks, resolution, and the context effect**

Replace this block (from `// router` through the context `useEffect`):

```ts
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
  flowHint,
  flowNotice,
  setWorkspace,
  setMode,
  setActiveIssueContext,
  clearFlowFeedback,
  sendMessage,
  retryLast,
  newChat,
  historyOpen,
  setHistoryOpen,
  confirmScheduleProposal,
  resolveScheduleProposal,
  confirmWorkItemProposal,
  resolveWorkItemProposal,
  confirmProposal,
  cancelProposal,
} = useAiAssistant();
const {
  peekIssue,
  fetchIssueWithIdentifier,
  issue: { getIssueById },
} = useIssueDetail();
const { getStateById } = useProjectState();
const { t } = useTranslation();
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
```

with:

```ts
// router
const { workspaceSlug, workItem, projectId, serviceId, pageId } = useParams<{
  workspaceSlug: string;
  workItem?: string;
  projectId?: string;
  serviceId?: string;
  pageId?: string;
}>();
const rawWorkspaceSlug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
const rawWorkItem = Array.isArray(workItem) ? workItem[0] : workItem;
const rawProjectId = Array.isArray(projectId) ? projectId[0] : projectId;
const rawServiceId = Array.isArray(serviceId) ? serviceId[0] : serviceId;
const rawPageId = Array.isArray(pageId) ? pageId[0] : pageId;
// store hooks
const { aiSidebarCollapsed, toggleAiSidebar } = useAppTheme();
const { config } = useInstance();
const {
  messages,
  isGenerating,
  mode,
  activeContext,
  hasActiveContext,
  flowHint,
  flowNotice,
  setWorkspace,
  setMode,
  setActiveContext,
  clearFlowFeedback,
  sendMessage,
  retryLast,
  newChat,
  historyOpen,
  setHistoryOpen,
  confirmScheduleProposal,
  resolveScheduleProposal,
  confirmWorkItemProposal,
  resolveWorkItemProposal,
  confirmProposal,
  cancelProposal,
} = useAiAssistant();
const {
  peekIssue,
  fetchIssueWithIdentifier,
  issue: { getIssueById },
} = useIssueDetail();
const { getStateById } = useProjectState();
const { peekService, getServiceById } = useService();
const projectPageStore = usePageStore(EPageStoreType.PROJECT);
const { t } = useTranslation();
// local state
const [question, setQuestion] = useState("");
const scrollRef = useRef<HTMLDivElement>(null);
const composerRef = useRef<HTMLTextAreaElement>(null);

const isOpen = aiSidebarCollapsed === false;

// restore conversation for the current workspace
useEffect(() => {
  if (rawWorkspaceSlug) setWorkspace(rawWorkspaceSlug.toString());
}, [rawWorkspaceSlug, setWorkspace]);

// resolve active context: issue peek, issue browse route, service peek, service route, article route
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
const peekedService =
  peekService && peekService.workspaceSlug === rawWorkspaceSlug && peekService.projectId === rawProjectId
    ? getServiceById(peekService.serviceId)
    : undefined;
const service = peekedService ?? (rawServiceId ? getServiceById(rawServiceId) : undefined);
const article = rawPageId ? projectPageStore.getPageById(rawPageId) : undefined;
const contextStatus = issue
  ? stateName
  : service
    ? service.status
    : article?.access !== undefined
      ? article.access === EPageAccess.PRIVATE
        ? "Private"
        : "Public"
      : undefined;

useEffect(() => {
  const context: TAiAssistantContext | undefined = issue
    ? {
        kind: "work_item",
        name: issue.name ?? "",
        descriptionHtml: issue.description_html ?? "",
        state: stateName,
        priority: issue.priority ?? undefined,
      }
    : service
      ? {
          kind: "service",
          id: service.id,
          name: service.name,
          descriptionHtml: service.description_html ?? "",
          status: service.status,
        }
      : article?.id
        ? {
            kind: "article",
            id: article.id,
            name: article.name ?? "",
            descriptionHtml: article.description_html ?? "",
            access: article.access,
          }
        : undefined;
  setActiveContext(context);
  // oxlint-disable-next-line eslint-plugin-react-hooks/exhaustive-deps
}, [
  issue?.id,
  issue?.name,
  issue?.description_html,
  issue?.priority,
  stateName,
  service?.id,
  service?.name,
  service?.description_html,
  service?.status,
  article?.id,
  article?.name,
  article?.description_html,
  article?.access,
  setActiveContext,
]);
```

- [ ] **Step 4: Replace the context strip JSX**

Replace:

```tsx
{
  /* context strip */
}
<div className="flex items-center gap-2 border-b border-subtle px-4 py-2">
  <span className="font-mono shrink-0 text-10 font-medium tracking-wide text-tertiary">Context</span>
  {hasActiveIssue && activeIssueContext ? (
    <>
      {stateName && (
        <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-subtle bg-layer-1 px-2 py-0.5 text-11 text-secondary">
          <span className="size-1.5 rounded-full bg-accent-primary" />
          {stateName}
        </span>
      )}
      <span className="truncate text-12 text-secondary" title={activeIssueContext.name}>
        {activeIssueContext.name}
      </span>
    </>
  ) : (
    <span className="truncate text-12 text-tertiary">No work item in view — general answers</span>
  )}
</div>;
```

with:

```tsx
{
  /* context strip */
}
<div className="flex items-center gap-2 border-b border-subtle px-4 py-2">
  <span className="font-mono shrink-0 text-10 font-medium tracking-wide text-tertiary">Context</span>
  {hasActiveContext && activeContext ? (
    <>
      <span className="flex shrink-0 items-center rounded-full border border-subtle bg-layer-1 px-2 py-0.5 text-11 text-secondary">
        {CONTEXT_KIND_LABEL[activeContext.kind]}
      </span>
      {contextStatus && (
        <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-subtle bg-layer-1 px-2 py-0.5 text-11 text-secondary">
          <span className="size-1.5 rounded-full bg-accent-primary" />
          {contextStatus}
        </span>
      )}
      <span className="truncate text-12 text-secondary" title={activeContext.name}>
        {activeContext.name}
      </span>
      {mode === "classic" && activeContext.kind !== "work_item" && (
        <button
          type="button"
          onClick={() => setMode("agent")}
          className="ml-auto shrink-0 text-11 font-medium text-accent-primary transition-opacity hover:opacity-80"
        >
          Switch to Agent
        </button>
      )}
    </>
  ) : (
    <span className="truncate text-12 text-tertiary">No ticket, service, or article in view — general answers</span>
  )}
</div>;
```

- [ ] **Step 5: Replace the empty-state copy**

Replace:

```tsx
                    <p className="mt-3 text-13 font-medium text-primary">
                      {hasActiveIssue ? "Ask about this work item" : "Ask Galileo anything"}
                    </p>
                    <p className="mt-1 text-12 leading-relaxed text-tertiary">
                      {hasActiveIssue
                        ? "Summaries, descriptions, comment drafts — grounded in the work item on screen."
                        : "Summaries, drafts, or any question — open a work item to ground answers."}
                    </p>
```

with:

```tsx
                    <p className="mt-3 text-13 font-medium text-primary">
                      {hasActiveContext && activeContext
                        ? `Ask about this ${CONTEXT_KIND_NOUN[activeContext.kind]}`
                        : "Ask Galileo anything"}
                    </p>
                    <p className="mt-1 text-12 leading-relaxed text-tertiary">
                      {hasActiveContext && activeContext
                        ? `Summaries, descriptions, comment drafts — grounded in the ${CONTEXT_KIND_NOUN[activeContext.kind]} on screen.`
                        : "Summaries, drafts, or any question — open a ticket, service, or article to ground answers."}
                    </p>
```

- [ ] **Step 6: Verify typecheck and tests**

Run: `pnpm --filter=web check:types`
Expected: PASS (no TypeScript errors).

Run: `pnpm --filter=web exec vitest run core/lib/ai-context.test.ts core/store/ai-assistant.store.test.ts`
Expected: PASS (both files).

- [ ] **Step 7: Manual smoke (requires a browser session)**

If dev server is needed (per `AGENTS.md`, port 3000 defaults to prod):

```bash
systemctl --user stop plane-web-prod.service && systemctl --user start plane-web.service
```

Check each in the browser (switch back with `systemctl --user stop plane-web.service && systemctl --user start plane-web-prod.service` when done):

1. Ticket detail (`/browse/PROJ-123`) and ticket peek — Classic + Agent: strip shows `Ticket` + state + name; Agent answer is grounded.
2. Service detail (open from the services list and via direct URL) in Agent — strip shows `Service` + status + name; ask "summarize this service" → grounded.
3. Service peek from the services list — strip follows the peeked service.
4. Article detail (`/projects/:id/pages/:pageId`) in Agent — strip shows `Article` + Public/Private; "summarize this article" → grounded.
5. Classic mode on service/article — strip shows the entity + `Switch to Agent`; clicking it switches mode and subsequent answers are grounded.
6. Navigate to a list page — strip shows "No ticket, service, or article in view — general answers".

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): auto-context for service and article detail views"
```

---

### Task 4: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Run the full web test suite**

Run: `pnpm --filter=web test`
Expected: PASS (all Vitest files).

- [ ] **Step 2: Run repo checks**

Run: `pnpm check:types`
Expected: PASS.

Run: `pnpm check:lint`
Expected: PASS (no new oxlint findings; pre-existing unrelated issues, if any, were present before this work).

- [ ] **Step 3: Confirm scope**

Verify with `git diff --stat HEAD~3..HEAD` that only these files changed:
`apps/web/core/lib/ai-context.ts`, `apps/web/core/lib/ai-context.test.ts`, `apps/web/core/store/ai-assistant.store.ts`, `apps/web/core/store/ai-assistant.store.test.ts`, `apps/web/core/components/ai/assistant-sidebar/root.tsx`.

No backend, no i18n, no migrations. If dev/prod services were toggled for smoke testing, confirm `plane-web-prod.service` is running again.
