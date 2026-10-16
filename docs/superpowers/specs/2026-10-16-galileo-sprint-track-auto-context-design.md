# Galileo Auto-Context for Sprints (Cycles) & Tracks (Modules) — Design

Date: 2026-10-16
Status: Approved in brainstorming (approach + details, 2026-10-16); awaiting written-spec review.
Scope: web only (`apps/web/core/lib`, `apps/web/core/components/ai`). No backend changes, no migrations, no store changes.
Related: `2026-10-16-galileo-service-article-auto-context-design.md` (the pattern being extended), `2026-09-23-galileo-agent-dual-mode-design.md` (classic ↔ agent gating), `2026-10-04-galileo-tools-roadmap-design.md` (read tools).

## Background

The previous feature grounded services and KB articles in the sidebar's Agent mode. Sprints (cycles) and tracks (modules) still fall back to "No ticket, service, or article in view" even though:

- The agent already has read tools: `list_sprints` / `get_sprint` / `list_sprint_work_items` and `list_tracks` / `get_track` / `list_track_work_items` — `apps/api-rs/crates/ai/src/tools/mod.rs:439-472`.
- Entity data is already in client stores, populated by the detail pages and peeks:
  - Cycles: `getCycleById` — `apps/web/core/store/cycle.store.ts:326`; fields `description` (plain text), `status` (`current | upcoming | completed | draft`, server-computed), `start_date` / `end_date`.
  - Modules: `getModuleById` — `apps/web/core/store/module.store.ts:215`; fields `description_html`, `status` (`backlog | planned | in-progress | paused | completed | cancelled`), `start_date` / `target_date`.
- Routes and peeks exist: `:workspaceSlug/projects/:projectId/cycles/:cycleId` and `.../modules/:moduleId` — `apps/web/app/routes/core.ts:150,166`; peeks via `?peekCycle=` (`cycles/cycle-peek-overview.tsx:29`) and `?peekModule=` (module list views).

This mirrors the service/article motivation: grounding is cheap, client-side, and the agent re-fetches via tools when it needs more than the block.

## Locked decisions (brainstorming 2026-10-16)

1. **Scope: sprints (cycles) + tracks (modules).** Existing kinds (work item, service, article) are unchanged.
2. **Context content**: name, id, plain-text description (2,000-char cap, same as other kinds), status, and the date range — plus dates is the one field new relative to service/article. No issue counts, no members/leads, no progress.
3. **Internal kind names `cycle` / `module`** (match routes, stores, and types); **UI labels Sprint / Track** (match the agent's tool/proposal vocabulary: `create_sprint`, `manage_track_items`).
4. **Agent-only gating unchanged**: context still attaches only in Agent mode; Classic shows the entity strip with the existing "Switch to Agent" affordance. No store change.
5. **Peek support included**, first-match precedence extended: issue peek → issue route → service peek → service route → article route → **cycle peek → cycle route → module peek → module route**. Cycle and module route params are mutually exclusive with the other entities, so appending is safe.
6. **Approach A — extend the existing pattern.** Rejected: B (per-kind resolver registry — premature abstraction for five kinds) and C (sidebar SWR fetch — stores are already populated by detail/peek components; the sidebar stays read-only).
7. **Copy updates**: strip empty state becomes "No ticket, service, article, sprint, or track in view — general answers"; kind chip labels add Sprint / Track; Agent mode hint mentions sprints and tracks. `AI_ASSISTANT_TASK` entity list gains sprint/track.
8. **No i18n migration, no backend changes, no migrations.** Strip strings stay hardcoded English like the rest of the sidebar chrome.

## Non-goals

- Structured payloads, tool auto-invocation, or backend prompt building (unchanged from the previous spec).
- Issue counts, completion progress, members/leads, or labels in the context block.
- Auto-context for intake items and other workspace-level pages.
- Changing ticket/service/article resolution or gating.
- Fresh re-fetch at send time; tool usage stays a model decision.

## Design

### 1. Context model (`apps/web/core/lib/ai-context.ts`)

Extend the union:

```ts
export type TAiContextKind = "work_item" | "service" | "article" | "cycle" | "module";

export type TAiCycleContext = {
  kind: "cycle";
  id: string; // uuid, usable with the get_sprint tool
  name: string;
  descriptionHtml: string; // cycle.description is plain text; stripHtml is a no-op
  status?: string; // current | upcoming | completed | draft (server-computed)
  startDate?: string | null;
  endDate?: string | null;
};

export type TAiModuleContext = {
  kind: "module";
  id: string; // uuid, usable with the get_track tool
  name: string;
  descriptionHtml: string;
  status?: string; // backlog | planned | in-progress | paused | completed | cancelled
  startDate?: string | null;
  endDate?: string | null; // module.target_date
};

export type TAiAssistantContext =
  | TAiWorkItemContext
  | TAiServiceContext
  | TAiArticleContext
  | TAiCycleContext
  | TAiModuleContext;
```

### 2. Builder & task wording (`ai-context.ts`)

`buildContextBlock` gains two cases (same rules as existing kinds: `stripHtml`, 2,000-char cap with `…`, omit absent fields):

- `cycle`:
  - `Sprint context:` / `Sprint: {name}` / `Sprint id: {id}` / `Status: {status}`
  - `Dates:` line from `startDate` / `endDate` (raw `YYYY-MM-DD` strings):
    - both → `Dates: 2026-10-01 → 2026-10-31`
    - start only → `Dates: from 2026-10-01`
    - end only → `Dates: until 2026-10-31`
    - neither → line omitted
  - `Description: …`
- `module`: identical shape with `Track context:` / `Track: {name}` / `Track id: {id}`.

`AI_ASSISTANT_TASK` becomes: "You are an ITSM assistant. Answer using the context below when present — the work item, service, sprint, track, or knowledge base article the user is viewing. Be concise, use bullet points. If generating text (description/comment), output the text only."

### 3. Resolution (`apps/web/core/components/ai/assistant-sidebar/root.tsx`)

- Extend the `useParams` read to `{ workspaceSlug, workItem, projectId, serviceId, pageId, cycleId, moduleId }`.
- Read `useSearchParams()` for `peekCycle` / `peekModule`.
- New resolvers, appended after the article route (first match wins): 6. **Cycle peek**: `peekCycle` → `getCycleById` — apply only when the entity's `project_id === rawProjectId` (mirrors the service peek guard). 7. **Cycle route**: `cycleId` → `getCycleById(cycleId)`. 8. **Module peek**: `peekModule` → `getModuleById` with the same project guard. 9. **Module route**: `moduleId` → `getModuleById(moduleId)`.

Store hooks: `useCycle()` and `useModule()`. No fetch in the sidebar — the detail pages and peek components already populate the maps (documented race unchanged: not-loaded entity → that turn sends no context).

The context effect maps entity fields: `startDate: entity.start_date ?? null`, `endDate: cycle.end_date ?? module.target_date ?? null`, status passed raw. Dependency list gains the new ids/fields.

### 4. UI strip (`root.tsx`)

- `CONTEXT_KIND_LABEL` adds `cycle: "Sprint"`, `module: "Track"`; `CONTEXT_KIND_NOUN` adds `cycle: "sprint"`, `module: "track"` (noun used by the empty-state phrasing).
- Status chip shows the raw `status` value for both kinds (same style as service).
- Classic mode with sprint/track in view: entity strip + existing "Switch to Agent" button (unchanged behavior).
- Empty state: "No ticket, service, article, sprint, or track in view — general answers"; the grounded-answer hint keeps the same list ("Summaries, drafts, or any question — open a ticket, service, article, sprint, or track to ground answers.").
- `MODES` agent hint becomes "Looks up projects, work items, sprints, and tracks in this workspace".

### 5. Edge cases

- **Peek from another project**: ignored via the `project_id` guard (same principle as service peek).
- **Missing dates / status**: omitted from the block; chip shows nothing extra.
- **Not loaded yet**: no context that turn (same as tickets/services today).
- **Long descriptions**: capped at 2,000 chars; the id line lets the agent re-fetch full text via `get_sprint` / `get_track`.
- **Classic mode**: context is dropped at send time by the existing gating; strip advertises switching to Agent.
- **Workspace switch**: cleared by the existing `setWorkspace` reset.

## Files touched

- `apps/web/core/lib/ai-context.ts` — union members, two builder cases, date line helper, task wording.
- `apps/web/core/lib/ai-context.test.ts` — cycle/module blocks, date variants, truncation, task wording.
- `apps/web/core/components/ai/assistant-sidebar/root.tsx` — params, peek resolution, context effect, label/noun maps, copy.
- No store, backend, migration, or i18n files.

## Testing & verification

Unit (`pnpm --filter=web test`):

- `ai-context`: cycle block (header, id, status, both/one/no dates, description); module block (same); truncation at 2,000 chars with `…`; no-context fallback; `AI_ASSISTANT_TASK` wording includes sprint/track; existing kinds unchanged.

Checks: `pnpm check` (format, lint, types).

Manual smoke:

1. Sprint detail (direct URL) in Agent — strip shows Sprint + status + name; "summarize this sprint" grounded; dates and id line present.
2. Sprint peek from the cycles list — strip follows the peeked sprint; closing peek clears it.
3. Track detail (direct URL) in Agent — strip shows Track + status + name; dates (start → target) present.
4. Track peek from the modules list — strip follows the tracked module.
5. Classic on sprint/track — strip shows entity + Switch to Agent; answers general until switched.
6. Ticket/service/article behavior unchanged.
