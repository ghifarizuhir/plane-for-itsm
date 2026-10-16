# Galileo Auto-Context for Services & Knowledge Base Articles — Design

Date: 2026-10-16
Status: Approved in brainstorming (6/6 sections, 2026-10-16); awaiting written-spec review.
Scope: web only (`apps/web/core/lib`, `apps/web/core/store`, `apps/web/core/components/ai`). No backend changes, no migrations, no new tools.
Related: `2026-09-21-ai-assistant-sidebar-design.md` (context strip origin), `2026-09-23-galileo-agent-dual-mode-design.md` (classic ↔ agent), `2026-10-04-galileo-tools-roadmap-design.md` (read tools incl. `get_service` / `get_article`), `2026-09-21-rust-ai-assistant-design.md` (opaque `context` contract).

## Background

The Galileo sidebar already grounds answers in the work item on screen:

- Resolution: the sidebar derives the active ticket from the issue peek store, else the `/browse/PROJ-123` route identifier — `apps/web/core/components/ai/assistant-sidebar/root.tsx:100`.
- Transport: every message sends a plain-text `context` block built by `buildAiContext` — `apps/web/core/store/ai-assistant.store.ts:788`, `apps/web/core/lib/ai-context.ts:103`.
- Backend treats `context` as opaque text: it appends the caller identity (agent mode) and prepends it to the prompt, with no parsing or truncation — `apps/api-rs/crates/api/src/routes/ai.rs:157`, `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs:220`.

Services and knowledge base (KB) articles have **no** auto-context. The sidebar strip falls back to "No work item in view — general answers" even on a service or article detail page. Everything needed to fix this already exists:

- Service store with peek state and detail cache — `apps/web/core/store/service.store.ts` (`TServicePeek`, `getServiceById`, `peekService`).
- Project page (KB article) store — `apps/web/core/store/pages/project-page.store.ts` (`getPageById`, `fetchPageDetails`).
- Agent tools that accept ids: `get_service` (name-or-id) — `apps/api-rs/crates/ai/src/tools/services.rs:373`; `get_article` (uuid) — `apps/api-rs/crates/ai/src/tools/kb.rs:206`.

## Locked decisions (brainstorming 2026-10-16)

1. **Scope: services + KB articles.** Service detail route and peek, plus project KB article detail route. Ticket (work item) context is unchanged.
2. **Context content mirrors tickets**: name, description as plain text, plus a status-like field (service `status`; article `access` as Public/Private, since articles have no workflow status). No relations, no activity, no linked work items in v1.
3. **Mode gating**: service/article context attaches in **Agent mode only**. Ticket context keeps attaching in both Classic and Agent, exactly as today.
4. **Approach A — generalize the client-side context block.** Rejected: B (structured `context_entity` payload + backend-built prompt) and C (reference-only, agent must call a tool for every answer — adds latency and answers are ungrounded whenever the model skips the tool).
5. **One context at a time** with first-match precedence: issue peek → issue browse route → service peek → service route → article route. Service peek is scoped to the current workspace + project (mirrors the peek overview guard).
6. **Read-only resolution from existing stores.** The sidebar reads the same store the detail UI reads (the detail UI fetches on mount, including deep links). The sidebar does not fetch. If the entity is not loaded yet at send time, that turn goes without context — same race as tickets today.
7. **Truncation cap 2,000 characters** on the plain-text description for all kinds, including tickets, to bound per-message prompt growth. The entity id is included so the agent can re-fetch full text via `get_service` / `get_article` when needed.
8. **Strip UI shows the entity**: kind chip (Ticket / Service / Article) + status chip + truncatable name with tooltip. In Classic mode with a service/article in view, the strip shows the entity plus a "Switch to Agent" affordance (context is not attached in Classic).
9. **`AI_ASSISTANT_TASK` becomes entity-neutral** and the no-context fallback wording drops the work-item-only phrasing.
10. **No i18n migration of assistant chrome.** New/changed strip strings stay hardcoded English like the rest of the sidebar chrome.

## Non-goals

- Structured context payload / backend prompt building (approach B).
- Reference-only context with mandatory tool fetch (approach C).
- Auto-context for sprints, tracks, intake items, or workspace/team pages.
- Grounding service/article in Classic mode.
- Fresh re-fetch of entity data at send time; tool usage stays a model decision.
- Auto-invoking tools because an entity is in view; ids are a fallback signal only.
- Relations/activity in the context block (linked work items, sub-articles, labels, comments).
- Migrating assistant-sidebar chrome to i18n.

## Design

### 1. Context model (`apps/web/core/lib/ai-context.ts`)

Replace `TAiIssueContext` with a tagged union (update imports in `root.tsx`, the store, and tests):

```ts
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
  id: string; // uuid, usable with the get_service tool
  name: string;
  descriptionHtml: string;
  status?: string; // active | planned | maintenance | deprecated | retired
};

export type TAiArticleContext = {
  kind: "article";
  id: string; // uuid, usable with the get_article tool
  name: string;
  descriptionHtml: string;
  access?: EPageAccess; // numeric enum: 0 = public, 1 = private
};

export type TAiAssistantContext = TAiWorkItemContext | TAiServiceContext | TAiArticleContext;
```

`TAiArticleContext.descriptionHtml` may be `undefined` upstream; the builder treats it as empty.

### 2. Builder & task wording (`ai-context.ts`)

`buildContextBlock` switches on `kind`:

- `work_item` — output format unchanged (existing tests keep passing apart from fixtures gaining `kind`):
  `Work item context:` / `Work item: {name}` / `Description: …` / `State: …` / `Priority: …`
- `service` — `Service context:` / `Service: {name}` / `Service id: {id}` / `Status: {status}` / `Description: …`
- `article` — `Article context:` / `Article: {name}` / `Article id: {id}` / `Access: {access}` / `Description: …`, where `access` is rendered as `Public` / `Private` (mapping `EPageAccess`, numeric: 0 = public, 1 = private).

Rules:

- Description goes through `stripHtml` and is capped at `CONTEXT_DESCRIPTION_LIMIT = 2000` characters; a cut description gets a trailing `…`. Fields absent upstream are omitted.
- No-context fallback becomes: `No active context. Answer from general knowledge.`
- `AI_ASSISTANT_TASK` becomes: "You are an ITSM assistant. Answer using the context below when present — the work item, service, or knowledge base article the user is viewing. Be concise, use bullet points. If generating text (description/comment), output the text only."

### 3. Resolution (`apps/web/core/components/ai/assistant-sidebar/root.tsx`)

Extend the existing `useParams` read to `{ workspaceSlug, workItem, projectId, serviceId, pageId }` and derive at most one context, first match wins:

1. **Issue peek** (existing): `peekIssue` → `getIssueById`.
2. **Issue browse route** (existing): `workItem` identifier → SWR fetch → `getIssueById`.
3. **Service peek** (new): `peekService` where `workspaceSlug === rawWorkspaceSlug && projectId === rawProjectId` → `getServiceById(peekService.serviceId)`.
4. **Service route** (new): `serviceId` → `getServiceById(serviceId)`.
5. **Article route** (new): `pageId` → project page store `getPageById(pageId)`.

Store hooks: `useService()` for the service store; `usePageStore(EPageStoreType.PROJECT)` for articles (`usePage` cannot be used — it throws when `pageId` is absent, and hooks must be unconditional).

The existing context effect is generalized: derive the tagged context from the resolved entity fields (name, description, status/state/access) and call `setActiveContext(context)`; it clears automatically when the view has no entity, exactly like tickets today. Mode is not consulted during resolution — the strip shows what is in view in both modes; gating happens at send time (§4).

The current `hasActiveIssue` empty-state copy is derived from the new kind instead (e.g. title "Ask about this ticket/service/article"). Strip copy with no entity: "No ticket, service, or article in view — general answers". The suggestion list is unchanged.

### 4. Mode gating (`apps/web/core/store/ai-assistant.store.ts`)

Rename for clarity, keeping behavior:

- `activeIssueContext` → `activeContext: TAiAssistantContext | undefined`
- `setActiveIssueContext` → `setActiveContext`; `hasActiveIssue` → `hasActiveContext`
- `setWorkspace` already clears the context on workspace switch — keep, pointing at `activeContext`.

In `private request()`, before building the payload:

```ts
const attachable =
  mode === "agent" || this.activeContext?.kind === "work_item" ? this.activeContext : undefined;
const payload = { …, context: buildAiContext(attachable, userTimezone), … };
```

So Classic drops service/article contexts (general answers), Agent sends all kinds, and tickets stay as-is in both modes. Everything else in the request path (timezone, flow fields, proposals, persistence) is untouched.

### 5. UI strip (`root.tsx`)

- Kind chip: `Ticket` / `Service` / `Article`, alongside the existing status chip — ticket: state name; service: raw status value; article: `Public` / `Private`.
- Name: truncated with `title` tooltip (existing pattern).
- Classic mode with a service/article in view: the strip shows the entity plus an inline **Switch to Agent** button wired to the existing `setMode("agent")`, making the non-attached state explicit.
- No entity: "No ticket, service, or article in view — general answers".

### 6. Edge cases

- **Peek scoping**: a service peek from another workspace/project is ignored (same guard as `services/peek-overview/root.tsx`). Issue peek keeps its existing behavior.
- **Permissions**: the sidebar reads whatever the detail UI can read; private articles are simply absent from the store when the user lacks access, so no context is sent.
- **Not loaded yet**: entity still fetching → that turn sends no context (documented race, same as tickets today).
- **Long descriptions**: truncated at 2,000 chars; the id line lets the agent re-fetch full text via tool.
- **Classic mode**: service/article context is dropped at request time; the strip advertises switching to Agent.
- **Workspace switch**: context cleared by the existing `setWorkspace` reset.

## Files touched

- `apps/web/core/lib/ai-context.ts` — union model, per-kind builder, cap, task wording.
- `apps/web/core/lib/ai-context.test.ts` — per-kind blocks, truncation, fallback, task wording.
- `apps/web/core/store/ai-assistant.store.ts` — rename + attach gating in `request()`.
- `apps/web/core/store/ai-assistant.store.test.ts` — fixtures gain `kind`; gating tests; rename.
- `apps/web/core/components/ai/assistant-sidebar/root.tsx` — resolution for service/article, generalized effect, strip UI, empty-state copy.
- No backend files; no migrations.

## Testing & verification

Unit (`pnpm --filter=web test`):

- `ai-context`: service block (header, id line, status, description); article block (access, id); truncation at 2,000 chars with `…`; work-item block unchanged; fallback wording; `AI_ASSISTANT_TASK` wording.
- `ai-assistant.store`: Classic drops service/article context but still sends work-item context; Agent sends all kinds; context cleared on workspace switch; existing conversation/proposal tests still pass after the rename.

Checks: `pnpm check` (format, lint, types).

Manual smoke:

1. Ticket browse route + ticket peek, both modes — strip shows Ticket + state + name; agent answers grounded; classic unchanged.
2. Service detail (direct URL) in Agent — strip shows Service + status + name; "summarize this service" is grounded; id line present.
3. Service peek from the services list — strip follows the peeked service.
4. Article detail (direct URL) in Agent — strip shows Article + Public/Private; "summarize this article" is grounded.
5. Classic on service/article — strip shows the entity + Switch to Agent; answers are general until switched.
6. Navigate to a list — strip returns to the empty message.
