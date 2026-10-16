# Galileo Auto-Context for Sprints & Tracks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user views a sprint (cycle) or track (module) detail — or opens its peek — Galileo automatically grounds answers in that entity in Agent mode, the same way it already does for tickets, services, and articles.

**Architecture:** Extend the existing tagged-union context model (`TAiAssistantContext`) with two kinds (`cycle`, `module`) and two builder cases in `apps/web/core/lib/ai-context.ts`; the sidebar resolves the on-screen entity from the existing cycle/module stores (route params + `peekCycle`/`peekModule` search params) and sets the single active context. The store, backend, and i18n are untouched.

**Tech Stack:** React + MobX (web), Vitest, TypeScript strict, oxlint/oxfmt.

**Spec:** `docs/superpowers/specs/2026-10-16-galileo-sprint-track-auto-context-design.md`

---

## File structure

- `apps/web/core/lib/ai-context.ts` (modify) — union members `TAiCycleContext` / `TAiModuleContext`, date-range helper, two builder cases, task wording.
- `apps/web/core/lib/ai-context.test.ts` (modify) — sprint/track blocks, date variants, truncation, task wording.
- `apps/web/core/components/ai/assistant-sidebar/root.tsx` (modify) — `cycleId`/`moduleId` params, `peekCycle`/`peekModule` search params, peek/route resolution, context effect, label/noun maps, copy.

No store changes. No backend changes. No i18n changes. No migrations.

---

### Task 1: Sprint & track context model and builder

**Files:**

- Modify: `apps/web/core/lib/ai-context.ts`
- Test: `apps/web/core/lib/ai-context.test.ts`

- [ ] **Step 1: Add the failing tests**

In `apps/web/core/lib/ai-context.test.ts`, extend the `AI_ASSISTANT_TASK` describe block — after the `"is entity-neutral"` test (line 35) insert:

```ts
it("mentions sprints and tracks", () => {
  expect(AI_ASSISTANT_TASK).toContain("sprint");
  expect(AI_ASSISTANT_TASK).toContain("track");
});
```

In the `buildAiContext` describe block, after the `"builds an article context block with id and access"` test (line 128) insert:

```ts
it("builds a sprint context block with status and date range", () => {
  const result = buildAiContext({
    kind: "cycle",
    id: "cyc-1",
    name: "Sprint 12",
    descriptionHtml: "Stabilize billing",
    status: "CURRENT",
    startDate: "2026-10-01",
    endDate: "2026-10-14",
  });
  expect(result).toContain("Sprint context:");
  expect(result).toContain("Sprint: Sprint 12");
  expect(result).toContain("Sprint id: cyc-1");
  expect(result).toContain("Status: CURRENT");
  expect(result).toContain("Dates: 2026-10-01 → 2026-10-14");
  expect(result).toContain("Description: Stabilize billing");
});

it("renders one-sided or missing sprint dates", () => {
  const startOnly = buildAiContext({
    kind: "cycle",
    id: "cyc-1",
    name: "Sprint 12",
    descriptionHtml: "",
    startDate: "2026-10-01",
    endDate: null,
  });
  expect(startOnly).toContain("Dates: from 2026-10-01");

  const endOnly = buildAiContext({
    kind: "cycle",
    id: "cyc-1",
    name: "Sprint 12",
    descriptionHtml: "",
    startDate: null,
    endDate: "2026-10-14",
  });
  expect(endOnly).toContain("Dates: until 2026-10-14");

  const noDates = buildAiContext({
    kind: "cycle",
    id: "cyc-1",
    name: "Sprint 12",
    descriptionHtml: "",
    startDate: null,
    endDate: null,
  });
  expect(noDates).not.toContain("Dates:");
});

it("builds a track context block with status and date range", () => {
  const result = buildAiContext({
    kind: "module",
    id: "mod-1",
    name: "Onboarding",
    descriptionHtml: "<p>Guided setup</p>",
    status: "in-progress",
    startDate: "2026-09-01",
    endDate: "2026-12-31",
  });
  expect(result).toContain("Track context:");
  expect(result).toContain("Track: Onboarding");
  expect(result).toContain("Track id: mod-1");
  expect(result).toContain("Status: in-progress");
  expect(result).toContain("Dates: 2026-09-01 → 2026-12-31");
  expect(result).toContain("Description: Guided setup");
});
```

In the `"truncates long descriptions for every kind"` test (line 130), after the existing `workItem` assertion (line 146) insert:

```ts
const cycle = buildAiContext({
  kind: "cycle",
  id: "cyc-1",
  name: "Sprint 12",
  descriptionHtml: `<p>${longDescription}</p>`,
});
expect(cycle).toContain(`${"x".repeat(CONTEXT_DESCRIPTION_LIMIT)}…`);
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm --filter=web exec vitest run core/lib/ai-context.test.ts`
Expected: FAIL — `"sprint"` not in `AI_ASSISTANT_TASK`; `kind: "cycle"` / `kind: "module"` are not valid `TAiAssistantContext` kinds (type error) and the sprint/track assertions fail.

- [ ] **Step 3: Implement `ai-context.ts`**

In `apps/web/core/lib/ai-context.ts`:

1. Extend the kind union and add the two context types (line 13 area):

Replace:

```ts
export type TAiContextKind = "work_item" | "service" | "article";
```

with:

```ts
export type TAiContextKind = "work_item" | "service" | "article" | "cycle" | "module";
```

2. After `TAiArticleContext` (line 37) insert:

```ts
export type TAiCycleContext = {
  kind: "cycle";
  id: string; // uuid, usable with the get_sprint tool
  name: string;
  descriptionHtml: string; // cycle.description is plain text; stripHtml is a no-op
  status?: string; // CURRENT | UPCOMING | COMPLETED | DRAFT (server-computed)
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
```

3. Extend the union (line 39):

```ts
export type TAiAssistantContext =
  | TAiWorkItemContext
  | TAiServiceContext
  | TAiArticleContext
  | TAiCycleContext
  | TAiModuleContext;
```

4. Update the task wording (line 59):

```ts
export const AI_ASSISTANT_TASK =
  "You are an ITSM assistant. Answer using the context below when present — the work item, service, sprint, track, or knowledge base article the user is viewing. Be concise, use bullet points. If generating text (description/comment), output the text only.";
```

5. Add the date helper after `truncateDescription` (line 77):

```ts
const formatDateRange = (startDate?: string | null, endDate?: string | null): string | undefined => {
  if (startDate && endDate) return `Dates: ${startDate} → ${endDate}`;
  if (startDate) return `Dates: from ${startDate}`;
  if (endDate) return `Dates: until ${endDate}`;
  return undefined;
};
```

6. Add the two `buildContextBlock` cases after the `article` case (line 144):

```ts
    case "cycle": {
      const parts = [`Sprint: ${context.name}`, `Sprint id: ${context.id}`];
      if (context.status) parts.push(`Status: ${context.status}`);
      const dates = formatDateRange(context.startDate, context.endDate);
      if (dates) parts.push(dates);
      if (description) parts.push(`Description: ${description}`);
      return `Sprint context:\n${parts.join("\n")}`;
    }
    case "module": {
      const parts = [`Track: ${context.name}`, `Track id: ${context.id}`];
      if (context.status) parts.push(`Status: ${context.status}`);
      const dates = formatDateRange(context.startDate, context.endDate);
      if (dates) parts.push(dates);
      if (description) parts.push(`Description: ${description}`);
      return `Track context:\n${parts.join("\n")}`;
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm --filter=web exec vitest run core/lib/ai-context.test.ts`
Expected: PASS (all tests in the file, including the pre-existing ones).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/ai-context.ts apps/web/core/lib/ai-context.test.ts
git commit -m "feat(web): add sprint and track kinds to ai context builder"
```

---

### Task 2: Sidebar resolution and strip copy

**Files:**

- Modify: `apps/web/core/components/ai/assistant-sidebar/root.tsx`

No component test harness exists for this sidebar; verification is typecheck, the existing unit suites, and manual smoke.

- [ ] **Step 1: Update imports**

Replace (line 8):

```ts
import { useParams } from "next/navigation";
```

with:

```ts
import { useParams, useSearchParams } from "next/navigation";
```

In the hooks import block, add two imports keeping alphabetical order:

```ts
import { useCycle } from "@/hooks/store/use-cycle"; // after use-app-theme
import { useModule } from "@/hooks/store/use-module"; // after use-issue-detail
```

- [ ] **Step 2: Update the Agent mode hint and kind label maps**

Replace the `MODES` constant (line 47-50):

```ts
const MODES = [
  { value: "classic", label: "Classic", hint: "Single model call grounded in the work item on screen" },
  { value: "agent", label: "Agent", hint: "Looks up projects and work items in this workspace" },
] as const;
```

with:

```ts
const MODES = [
  { value: "classic", label: "Classic", hint: "Single model call grounded in the work item on screen" },
  { value: "agent", label: "Agent", hint: "Looks up projects, work items, sprints, and tracks in this workspace" },
] as const;
```

Replace `CONTEXT_KIND_LABEL` and `CONTEXT_KIND_NOUN` (line 52-61):

```ts
const CONTEXT_KIND_LABEL: Record<TAiAssistantContext["kind"], string> = {
  work_item: "Ticket",
  service: "Service",
  article: "Article",
  cycle: "Sprint",
  module: "Track",
};

const CONTEXT_KIND_NOUN: Record<TAiAssistantContext["kind"], string> = {
  work_item: "work item",
  service: "service",
  article: "article",
  cycle: "sprint",
  module: "track",
};
```

- [ ] **Step 3: Extend routing params and store hooks**

Replace (line 64-76):

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
```

with:

```ts
// router
const { workspaceSlug, workItem, projectId, serviceId, pageId, cycleId, moduleId } = useParams<{
  workspaceSlug: string;
  workItem?: string;
  projectId?: string;
  serviceId?: string;
  pageId?: string;
  cycleId?: string;
  moduleId?: string;
}>();
const searchParams = useSearchParams();
const rawWorkspaceSlug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
const rawWorkItem = Array.isArray(workItem) ? workItem[0] : workItem;
const rawProjectId = Array.isArray(projectId) ? projectId[0] : projectId;
const rawServiceId = Array.isArray(serviceId) ? serviceId[0] : serviceId;
const rawPageId = Array.isArray(pageId) ? pageId[0] : pageId;
const rawCycleId = Array.isArray(cycleId) ? cycleId[0] : cycleId;
const rawModuleId = Array.isArray(moduleId) ? moduleId[0] : moduleId;
const peekCycleId = searchParams.get("peekCycle");
const peekModuleId = searchParams.get("peekModule");
```

In the store-hooks block, after `const { getStateById } = useProjectState();` (line 109) insert:

```ts
const { getCycleById } = useCycle();
const { getModuleById } = useModule();
```

- [ ] **Step 4: Extend resolution and the context effect**

Replace the resolution comment and service/article block (line 125-150):

```ts
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
```

with:

```ts
// resolve active context: issue peek, issue route, service peek, service route, article route,
// cycle peek, cycle route, module peek, module route
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
const peekingCycle = peekCycleId ? getCycleById(peekCycleId) : undefined;
const peekedCycle = peekingCycle && peekingCycle.project_id === rawProjectId ? peekingCycle : undefined;
const cycle = peekedCycle ?? (rawCycleId ? getCycleById(rawCycleId) : undefined);
const peekingModule = peekModuleId ? getModuleById(peekModuleId) : undefined;
const peekedModule = peekingModule && peekingModule.project_id === rawProjectId ? peekingModule : undefined;
const module = peekedModule ?? (rawModuleId ? getModuleById(rawModuleId) : undefined);
const contextStatus = issue
  ? stateName
  : service
    ? service.status
    : article?.access !== undefined
      ? article.access === EPageAccess.PRIVATE
        ? "Private"
        : "Public"
      : cycle
        ? cycle.status
        : module
          ? module.status
          : undefined;
```

Replace the context effect (line 152-195):

```ts
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

with:

```ts
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
        : cycle
          ? {
              kind: "cycle",
              id: cycle.id,
              name: cycle.name,
              descriptionHtml: cycle.description ?? "",
              status: cycle.status,
              startDate: cycle.start_date,
              endDate: cycle.end_date,
            }
          : module?.id
            ? {
                kind: "module",
                id: module.id,
                name: module.name,
                descriptionHtml: module.description_html ?? "",
                status: module.status,
                startDate: module.start_date,
                endDate: module.target_date,
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
  cycle?.id,
  cycle?.name,
  cycle?.description,
  cycle?.status,
  cycle?.start_date,
  cycle?.end_date,
  module?.id,
  module?.name,
  module?.description_html,
  module?.status,
  module?.start_date,
  module?.target_date,
  setActiveContext,
]);
```

- [ ] **Step 5: Update copy in the strip and empty state**

Replace (line 343-345):

```tsx
<span className="truncate text-12 text-tertiary">No ticket, service, or article in view — general answers</span>
```

with:

```tsx
<span className="truncate text-12 text-tertiary">
  No ticket, service, article, sprint, or track in view — general answers
</span>
```

Replace (line 368):

```tsx
                        : "Summaries, drafts, or any question — open a ticket, service, or article to ground answers."}
```

with:

```tsx
                        : "Summaries, drafts, or any question — open a ticket, service, article, sprint, or track to ground answers."}
```

- [ ] **Step 6: Verify typecheck and tests**

Run: `pnpm --filter=web check:types`
Expected: PASS (no TypeScript errors — the `Record<TAiAssistantContext["kind"], string>` maps now cover all five kinds).

Run: `pnpm --filter=web exec vitest run core/lib/ai-context.test.ts`
Expected: PASS.

- [ ] **Step 7: Manual smoke (requires a browser session)**

If a dev server is needed (per `AGENTS.md`, port 3000 defaults to prod):

```bash
systemctl --user stop plane-web-prod.service && systemctl --user start plane-web.service
```

Check each in the browser (switch back with `systemctl --user stop plane-web.service && systemctl --user start plane-web-prod.service` when done):

1. Sprint detail (direct URL `/projects/:projectId/cycles/:cycleId`) in Agent — strip shows `Sprint` + status + name; ask "summarize this sprint" → grounded; dates and id line present.
2. Sprint peek from the cycles list (`?peekCycle=`) — strip follows the peeked sprint; closing the peek clears it.
3. Track detail (`/projects/:projectId/modules/:moduleId`) in Agent — strip shows `Track` + status + name; dates (start → target) present.
4. Track peek from the modules list (`?peekModule=`) — strip follows the peeked track.
5. Classic mode on sprint/track — strip shows the entity + `Switch to Agent`; answers are general until switched.
6. Ticket/service/article behavior unchanged; empty list page shows "No ticket, service, article, sprint, or track in view — general answers".

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/ai/assistant-sidebar/root.tsx
git commit -m "feat(web): auto-context for sprint and track views"
```

---

### Task 3: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Run the full web test suite**

Run: `pnpm --filter=web test`
Expected: PASS (all Vitest files).

- [ ] **Step 2: Run repo checks**

Run: `pnpm check:types`
Expected: PASS.

Run: `pnpm check:lint`
Expected: PASS (no new oxlint findings).

- [ ] **Step 3: Confirm scope**

Verify with `git diff --stat HEAD~2..HEAD` that only these files changed:
`apps/web/core/lib/ai-context.ts`, `apps/web/core/lib/ai-context.test.ts`, `apps/web/core/components/ai/assistant-sidebar/root.tsx`.

No store, backend, i18n, or migration files.

- [ ] **Step 4: (Tunnel only) rebuild prod and restart**

Only when the change must appear on the tunnel demo (port 3000 serves the static build):

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build succeeds; then verify the served bundle contains the new copy (the layout chunk is not referenced from index.html, so locate it on disk first):

```bash
CHUNK=$(rg -l "sprint, or track in view" apps/web/build/client/assets/ | head -1) && \
  curl -s "http://localhost:3000/${CHUNK#apps/web/build/client/}" | rg -c "sprint, or track in view"
```

Expected: `1` (the served chunk is the freshly built one).

If dev/prod services were toggled for smoke testing, confirm `plane-web-prod.service` is running again.
