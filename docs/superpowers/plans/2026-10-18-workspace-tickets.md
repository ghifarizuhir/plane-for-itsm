# Workspace-level Tickets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only workspace-level Tickets page (aggregated across the user's member projects, grouped by state group, project filter + search, 100-per-page Load more, click-through to the browse detail page).

**Architecture:** Web-only. Reuse the existing `GET /api/workspaces/:slug/issues/` endpoint through `workspaceService.getViewIssues` (cursor pagination + complex `filters` tree). Reuse the `WorkspaceAggregateList` shell from the Sprints & Tracks pilot (one small addition: optional `footer` slot). Pure logic (filter tree builder, envelope normalization, pagination reducer, grouping) lives in a tested `core/lib/workspace-tickets.ts`.

**Tech Stack:** React Router v7 (apps/web), MobX stores, `@plane/propel` icons/avatar, `@plane/ui`, Vitest (`core/**/*.test.ts`).

**Spec:** `docs/superpowers/specs/2026-10-18-workspace-tickets-design.md`

---

## CRITICAL: Commit hygiene

The working tree already contains many unrelated pre-existing modifications.
**Never run `git add -A` or `git add .`.** Stage only the exact paths listed in
each task and verify with `git status --short <path>` before committing.

## Verified API facts (do not re-derive)

- `workspaceService.getViewIssues(slug, params, config)` →
  `GET /api/workspaces/:slug/issues/` (`apps/web/core/services/workspace.service.ts:272`).
  Import: `import { WorkspaceService } from "@/services/workspace.service";`
  Other components instantiate it module-level: `const workspaceService = new WorkspaceService();`
- Response envelope: `results`, `next_cursor`, `next_page_results`,
  `total_results` (also `total_count`). `results` is `TBaseIssue[]` for the
  flat (ungrouped) query.
- Row JSON keys come from `ArchiveRow` (`issue_common.rs:489`): `id`, `name`,
  `state_id`, `priority`, `sequence_id`, `project_id`, `parent_id`,
  `cycle_id`, `created_at`, `updated_at`, **`state__group`** (double
  underscore, serde rename), `assignee_ids`, `label_ids`, `module_ids`, etc.
- `filters` query param is a JSON string validated by `parse_complex_filter`
  (`issue_query.rs:1184`); `{"and":[{...},{...}]}` with `project_id` and
  `name__icontains` is valid.
- `STATE_GROUPS` (`@plane/constants`) order: backlog, unstarted, started,
  completed, cancelled; label via `STATE_GROUPS[group].label`.
- `StateGroupIcon` (`@plane/propel/icons`) defaults to `EIconSize.SM` and
  `STATE_GROUP_COLORS[stateGroup]` — no `size`/`color` props needed.
- `PriorityIcon` (`@plane/propel/icons`): `priority: TIssuePriorities | undefined | null`, `size?: number`.
- Avatar stack: `AvatarGroupOverflow` (`@/components/common/avatar-group-overflow`)
  with `Avatar` from `@makeplane/propel/components/avatar`; `src={getFileURL(member?.avatar_url ?? "")}`,
  `getFileURL` from `@plane/utils`.
- i18n keys already exist: `common.load_more`, `common.loading`,
  `service.catalog_all_projects`, `service.catalog_search_placeholder`,
  `sidebar.work_items` ("Tickets").
- `useProjectState().getStateById(stateId)` returns `IState | undefined` (with `name`).

## File structure

New files:

- `apps/web/core/lib/workspace-tickets.ts` — pure helpers.
- `apps/web/core/lib/workspace-tickets.test.ts` — Vitest unit tests.
- `apps/web/core/components/issues/workspace/workspace-tickets-list-view.tsx` — list view + row.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/layout.tsx`
- `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/page.tsx`

Modified files:

- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx` — `footer` prop.
- `apps/web/app/routes/core.ts` — one route block.
- `packages/constants/src/workspace.ts` — sidebar nav item.
- `apps/web/core/components/workspace/sidebar/sidebar-item.tsx` — static item key.
- `apps/web/core/components/workspace/sidebar/helper.tsx` — icon case.
- `packages/i18n/src/locales/en/empty-state.json` — `tickets` keys merged into `workspace_empty_state`.

Out of scope (do NOT touch): api-rs/Django code, global views/Queues, the
workspace issue stores, DB/docker.

---

### Task 1: Workspace tickets helpers (TDD)

**Files:**

- Create: `apps/web/core/lib/workspace-tickets.ts`
- Test: `apps/web/core/lib/workspace-tickets.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/lib/workspace-tickets.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import {
  appendTicketsPage,
  buildWorkspaceTicketsFilter,
  createTicketsPageState,
  formatIssueKey,
  groupWorkspaceTickets,
  normalizeTicketsEnvelope,
  type TWorkspaceTicketRow,
} from "./workspace-tickets";

const ticket = (id: string, stateGroup: string | null | undefined) =>
  ({
    id,
    name: id,
    state__group: stateGroup,
  }) as unknown as TWorkspaceTicketRow;

describe("formatIssueKey", () => {
  it("formats identifier and sequence id", () => {
    expect(formatIssueKey("ENG", 123)).toBe("ENG-123");
  });
});

describe("buildWorkspaceTicketsFilter", () => {
  it("returns undefined when no filter is active", () => {
    expect(buildWorkspaceTicketsFilter({})).toBeUndefined();
    expect(buildWorkspaceTicketsFilter({ search: "   " })).toBeUndefined();
  });

  it("builds a project-only filter", () => {
    expect(JSON.parse(buildWorkspaceTicketsFilter({ projectId: "p1" }) ?? "null")).toEqual({
      and: [{ project_id: "p1" }],
    });
  });

  it("builds a trimmed search filter", () => {
    expect(JSON.parse(buildWorkspaceTicketsFilter({ search: "  login  " }) ?? "null")).toEqual({
      and: [{ name__icontains: "login" }],
    });
  });

  it("builds a combined filter", () => {
    expect(JSON.parse(buildWorkspaceTicketsFilter({ projectId: "p1", search: "login" }) ?? "null")).toEqual({
      and: [{ project_id: "p1" }, { name__icontains: "login" }],
    });
  });
});

describe("normalizeTicketsEnvelope", () => {
  it("passes through array results", () => {
    const envelope = normalizeTicketsEnvelope({
      results: [{ id: "t1" }],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 250,
    } as never);
    expect(envelope).toEqual({
      results: [{ id: "t1" }],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 250,
    });
  });

  it("drops grouped (non-array) results and normalizes a missing cursor", () => {
    const envelope = normalizeTicketsEnvelope({
      results: { backlog: { results: [] } },
      next_cursor: null,
      next_page_results: false,
      total_results: 0,
    } as never);
    expect(envelope).toEqual({
      results: [],
      next_cursor: null,
      next_page_results: false,
      total_results: 0,
    });
  });
});

describe("createTicketsPageState / appendTicketsPage", () => {
  it("starts empty", () => {
    expect(createTicketsPageState()).toEqual({
      results: [],
      nextCursor: null,
      hasMore: false,
      totalResults: 0,
    });
  });

  it("maps a first page and its cursor", () => {
    const state = createTicketsPageState({
      results: [ticket("t1", "backlog")],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 250,
    });
    expect(state.results.map((row) => row.id)).toEqual(["t1"]);
    expect(state.nextCursor).toBe("100:1:0");
    expect(state.hasMore).toBe(true);
    expect(state.totalResults).toBe(250);
  });

  it("marks hasMore false when there are no next results", () => {
    const state = createTicketsPageState({
      results: [],
      next_cursor: null,
      next_page_results: false,
      total_results: 5,
    });
    expect(state.hasMore).toBe(false);
  });

  it("appends pages preserving order and updates the cursor", () => {
    const first = createTicketsPageState({
      results: [ticket("t1", "backlog")],
      next_cursor: "100:1:0",
      next_page_results: true,
      total_results: 2,
    });
    const second = appendTicketsPage(first, {
      results: [ticket("t2", "started")],
      next_cursor: null,
      next_page_results: false,
      total_results: 2,
    });
    expect(second.results.map((row) => row.id)).toEqual(["t1", "t2"]);
    expect(second.nextCursor).toBeNull();
    expect(second.hasMore).toBe(false);
    expect(second.totalResults).toBe(2);
  });
});

describe("groupWorkspaceTickets", () => {
  it("groups in STATE_GROUPS order and skips empty groups", () => {
    const groups = groupWorkspaceTickets([ticket("t1", "started"), ticket("t2", "backlog"), ticket("t3", "started")]);
    expect(groups.map((group) => group.key)).toEqual(["backlog", "started"]);
    expect(groups[0].tickets.map((row) => row.id)).toEqual(["t2"]);
    expect(groups[1].tickets.map((row) => row.id)).toEqual(["t1", "t3"]);
  });

  it("falls back to backlog for missing or unknown groups", () => {
    const groups = groupWorkspaceTickets([ticket("t1", null), ticket("t2", "unknown-group")]);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("backlog");
    expect(groups[0].tickets.map((row) => row.id)).toEqual(["t1", "t2"]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web test core/lib/workspace-tickets`
Expected: FAIL — cannot resolve `./workspace-tickets`.

- [ ] **Step 3: Write the implementation**

Create `apps/web/core/lib/workspace-tickets.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { STATE_GROUPS } from "@plane/constants";
import type { TBaseIssue, TIssuesResponse, TStateGroups } from "@plane/types";

export type TWorkspaceTicketRow = TBaseIssue & {
  state__group?: string | null;
};

export type TWorkspaceTicketsEnvelope = {
  results: TWorkspaceTicketRow[];
  next_cursor: string | null;
  next_page_results: boolean;
  total_results: number;
};

export type TWorkspaceTicketsPage = {
  results: TWorkspaceTicketRow[];
  nextCursor: string | null;
  hasMore: boolean;
  totalResults: number;
};

export const WORKSPACE_TICKET_GROUP_ORDER: TStateGroups[] = [
  "backlog",
  "unstarted",
  "started",
  "completed",
  "cancelled",
];

export function formatIssueKey(identifier: string, sequenceId: number): string {
  return `${identifier}-${sequenceId}`;
}

export function buildWorkspaceTicketsFilter({
  projectId,
  search,
}: {
  projectId?: string;
  search?: string;
}): string | undefined {
  const conditions: Record<string, string>[] = [];
  if (projectId) conditions.push({ project_id: projectId });
  const normalizedSearch = search?.trim() ?? "";
  if (normalizedSearch) conditions.push({ name__icontains: normalizedSearch });
  if (conditions.length === 0) return undefined;
  return JSON.stringify({ and: conditions });
}

export function normalizeTicketsEnvelope(response: TIssuesResponse): TWorkspaceTicketsEnvelope {
  const results = Array.isArray(response.results) ? (response.results as TWorkspaceTicketRow[]) : [];
  return {
    results,
    next_cursor: response.next_cursor ?? null,
    next_page_results: Boolean(response.next_page_results),
    total_results: response.total_results ?? 0,
  };
}

export function createTicketsPageState(envelope?: TWorkspaceTicketsEnvelope): TWorkspaceTicketsPage {
  if (!envelope) return { results: [], nextCursor: null, hasMore: false, totalResults: 0 };
  return {
    results: envelope.results,
    nextCursor: envelope.next_cursor,
    hasMore: envelope.next_page_results && Boolean(envelope.next_cursor),
    totalResults: envelope.total_results,
  };
}

export function appendTicketsPage(
  state: TWorkspaceTicketsPage,
  envelope: TWorkspaceTicketsEnvelope
): TWorkspaceTicketsPage {
  return {
    results: [...state.results, ...envelope.results],
    nextCursor: envelope.next_cursor,
    hasMore: envelope.next_page_results && Boolean(envelope.next_cursor),
    totalResults: envelope.total_results,
  };
}

export function deriveTicketGroup(ticket: Pick<TWorkspaceTicketRow, "state__group">): TStateGroups {
  const group = ticket["state__group"];
  return group && group in STATE_GROUPS ? (group as TStateGroups) : "backlog";
}

export function groupWorkspaceTickets(
  tickets: TWorkspaceTicketRow[]
): { key: TStateGroups; tickets: TWorkspaceTicketRow[] }[] {
  const buckets: Record<TStateGroups, TWorkspaceTicketRow[]> = {
    backlog: [],
    unstarted: [],
    started: [],
    completed: [],
    cancelled: [],
  };
  tickets.forEach((ticket) => buckets[deriveTicketGroup(ticket)].push(ticket));
  const groups: { key: TStateGroups; tickets: TWorkspaceTicketRow[] }[] = [];
  for (const key of WORKSPACE_TICKET_GROUP_ORDER) {
    if (buckets[key].length > 0) groups.push({ key, tickets: buckets[key] });
  }
  return groups;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web test core/lib/workspace-tickets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/lib/workspace-tickets.ts apps/web/core/lib/workspace-tickets.test.ts
git commit -m "feat(web): workspace ticket list helpers"
```

---

### Task 2: Shell footer slot

**Files:**

- Modify: `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`

- [ ] **Step 1: Add the optional footer prop**

In `Props`, add:

```ts
  footer?: ReactNode;
```

Destructure `footer` from props, and render it after the groups inside the scroll area:

```tsx
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {groups.map((group) => (
            <div key={group.key}>
              <div className="flex items-center gap-2 border-b border-subtle bg-layer-1 px-4 py-2">
                <span className="text-12 font-medium text-secondary">{group.label}</span>
                <span className="text-11 text-tertiary">{group.count}</span>
              </div>
              {group.items}
            </div>
          ))}
          {footer}
        </div>
      )}
```

- [ ] **Step 2: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx
git commit -m "feat(web): workspace aggregate list footer slot"
```

---

### Task 3: Tickets page (list view, page, layout, route, i18n)

**Files:**

- Create: `apps/web/core/components/issues/workspace/workspace-tickets-list-view.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/page.tsx`
- Modify: `apps/web/app/routes/core.ts` (insert after the workspace tracks block)
- Modify: `packages/i18n/src/locales/en/empty-state.json` (merge into `workspace_empty_state`)

- [ ] **Step 1: Create the list view**

Create `apps/web/core/components/issues/workspace/workspace-tickets-list-view.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Avatar } from "@makeplane/propel/components/avatar";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { STATE_GROUPS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { PriorityIcon, StateGroupIcon } from "@plane/propel/icons";
import { getFileURL, renderFormattedDate } from "@plane/utils";
// components
import { AvatarGroupOverflow } from "@/components/common/avatar-group-overflow";
import { ListItem } from "@/components/core/list";
import {
  WorkspaceAggregateList,
  type TWorkspaceAggregateGroupItem,
} from "@/components/workspace/aggregate-list/workspace-aggregate-list";
// helpers
import {
  appendTicketsPage,
  buildWorkspaceTicketsFilter,
  createTicketsPageState,
  deriveTicketGroup,
  formatIssueKey,
  groupWorkspaceTickets,
  normalizeTicketsEnvelope,
  type TWorkspaceTicketRow,
} from "@/lib/workspace-tickets";
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useProject } from "@/hooks/store/use-project";
import { useProjectState } from "@/hooks/store/use-project-state";
import { useAppRouter } from "@/hooks/use-app-router";
// services
import { WorkspaceService } from "@/services/workspace.service";

const workspaceService = new WorkspaceService();

type TTicketRowProps = {
  ticket: TWorkspaceTicketRow;
  workspaceSlug: string;
};

const TicketRow = observer(function TicketRow({ ticket, workspaceSlug }: TTicketRowProps) {
  const parentRef = useRef(null);
  const { getProjectById } = useProject();
  const { getStateById } = useProjectState();
  const { getUserDetails } = useMember();

  const project = ticket.project_id ? getProjectById(ticket.project_id) : undefined;
  const state = getStateById(ticket.state_id);
  const stateGroup = deriveTicketGroup(ticket);
  const issueKey = project ? formatIssueKey(project.identifier, ticket.sequence_id) : `#${ticket.sequence_id}`;

  return (
    <ListItem
      title={`${issueKey} ${ticket.name}`}
      itemLink={`/${workspaceSlug}/browse/${issueKey}`}
      prependTitleElement={
        <Tooltip label={state?.name ?? STATE_GROUPS[stateGroup].label} disabled={!state}>
          <span className="flex items-center">
            <StateGroupIcon stateGroup={stateGroup} />
          </span>
        </Tooltip>
      }
      appendTitleElement={
        <div className="flex items-center gap-3 text-11 text-tertiary">
          {project && (
            <span className="max-w-[120px] truncate rounded-sm bg-layer-2 px-1.5 py-0.5">{project.name}</span>
          )}
          <PriorityIcon priority={ticket.priority} size={14} />
          {ticket.assignee_ids.length > 0 && (
            <AvatarGroupOverflow size="xs">
              {ticket.assignee_ids.map((assigneeId) => {
                const member = getUserDetails(assigneeId);
                return (
                  <Avatar
                    key={assigneeId}
                    alt={member?.display_name}
                    fallback={member?.display_name?.[0]?.toUpperCase()}
                    src={getFileURL(member?.avatar_url ?? "")}
                  />
                );
              })}
            </AvatarGroupOverflow>
          )}
          <span className="hidden md:inline">{renderFormattedDate(ticket.updated_at)}</span>
        </div>
      }
      parentRef={parentRef}
    />
  );
});

type Props = {
  projectFilter?: string;
};

export const WorkspaceTicketsListView = observer(function WorkspaceTicketsListView({ projectFilter }: Props) {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const router = useAppRouter();
  const { workspaceProjectIds } = useProject();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [pageState, setPageState] = useState(() => createTicketsPageState());
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  const slug = workspaceSlug?.toString();
  const normalizedProjectFilter =
    projectFilter && (workspaceProjectIds ?? []).includes(projectFilter) ? projectFilter : undefined;

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(timer);
  }, [search]);

  useEffect(() => {
    if (!slug) return;
    const controller = new AbortController();
    setIsLoading(true);
    const filters = buildWorkspaceTicketsFilter({ projectId: normalizedProjectFilter, search: debouncedSearch });
    const params: Record<string, string | number> = { per_page: 100, order_by: "-created_at" };
    if (filters) params.filters = filters;
    void workspaceService
      .getViewIssues(slug, params, { signal: controller.signal })
      .then((response) => setPageState(createTicketsPageState(normalizeTicketsEnvelope(response))))
      .catch(() => undefined)
      .finally(() => {
        if (!controller.signal.aborted) setIsLoading(false);
      });
    return () => controller.abort();
  }, [slug, normalizedProjectFilter, debouncedSearch]);

  const groups = useMemo(() => groupWorkspaceTickets(pageState.results), [pageState.results]);

  const renderedGroups: TWorkspaceAggregateGroupItem[] = groups.map((group) => ({
    key: group.key,
    label: STATE_GROUPS[group.key].label,
    count: group.tickets.length,
    items: (
      <>
        {group.tickets.map((ticket) => (
          <TicketRow key={ticket.id} ticket={ticket} workspaceSlug={slug ?? ""} />
        ))}
      </>
    ),
  }));

  const handleProjectFilterChange = (projectId: string) => {
    if (!slug) return;
    router.push(projectId ? `/${slug}/tickets?project=${projectId}` : `/${slug}/tickets`);
  };

  const handleLoadMore = () => {
    if (!slug || !pageState.hasMore || !pageState.nextCursor || isLoadingMore) return;
    setIsLoadingMore(true);
    const filters = buildWorkspaceTicketsFilter({ projectId: normalizedProjectFilter, search: debouncedSearch });
    const params: Record<string, string | number> = {
      per_page: 100,
      order_by: "-created_at",
      cursor: pageState.nextCursor,
    };
    if (filters) params.filters = filters;
    void workspaceService
      .getViewIssues(slug, params, {})
      .then((response) => setPageState((state) => appendTicketsPage(state, normalizeTicketsEnvelope(response))))
      .catch(() => undefined)
      .finally(() => setIsLoadingMore(false));
  };

  return (
    <WorkspaceAggregateList
      projectFilter={normalizedProjectFilter}
      onProjectFilterChange={handleProjectFilterChange}
      search={search}
      onSearchChange={setSearch}
      searchPlaceholder={t("service.catalog_search_placeholder")}
      groups={renderedGroups}
      totalCount={pageState.totalResults}
      isLoading={isLoading}
      emptyTitle={t("workspace_empty_state.tickets.title")}
      emptyDescription={t("workspace_empty_state.tickets.description")}
      footer={
        pageState.hasMore ? (
          <div className="flex items-center justify-center border-b border-subtle py-3">
            <button
              type="button"
              onClick={handleLoadMore}
              disabled={isLoadingMore}
              className="rounded-sm border border-subtle px-3 py-1 text-12 text-secondary transition-colors hover:bg-layer-transparent-hover disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isLoadingMore ? t("common.loading") : t("common.load_more")}
            </button>
          </div>
        ) : undefined
      }
    />
  );
});
```

- [ ] **Step 2: Create the layout**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/layout.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Outlet } from "react-router";
// icons
import { WorkItemsOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";

const WorkspaceTicketsBreadcrumbs = observer(function WorkspaceTicketsBreadcrumbs() {
  const router = useAppRouter();
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs onBack={router.back}>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.work_items")}
                href={`/${workspaceSlug}/tickets/`}
                icon={<WorkItemsOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});

export default function WorkspaceTicketsLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceTicketsBreadcrumbs />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
```

- [ ] **Step 3: Create the page**

Create `apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/page.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceTicketsListView } from "@/components/issues/workspace/workspace-tickets-list-view";

function WorkspaceTicketsPage() {
  const searchParams = useSearchParams();
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.work_items")} />
      <WorkspaceTicketsListView projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceTicketsPage);
```

- [ ] **Step 4: Add the route**

In `apps/web/app/routes/core.ts`, right after the "Workspace Tracks" block, insert:

```ts
        // Workspace Tickets
        layout("./(all)/[workspaceSlug]/(projects)/tickets/layout.tsx", [
          route(":workspaceSlug/tickets", "./(all)/[workspaceSlug]/(projects)/tickets/page.tsx"),
        ]),
```

- [ ] **Step 5: Add the empty-state keys**

In `packages/i18n/src/locales/en/empty-state.json`, inside the **existing**
`workspace_empty_state` namespace (next to `sprints` and `tracks`), add:

```json
    "tickets": {
      "title": "No tickets yet",
      "description": "Tickets from projects you are a member of will show up here."
    },
```

- [ ] **Step 6: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 7: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/layout.tsx" \
  "apps/web/app/(all)/[workspaceSlug]/(projects)/tickets/page.tsx" \
  apps/web/core/components/issues/workspace/workspace-tickets-list-view.tsx \
  apps/web/app/routes/core.ts packages/i18n/src/locales/en/empty-state.json
git commit -m "feat(web): workspace tickets page"
```

---

### Task 4: Sidebar entry

**Files:**

- Modify: `packages/constants/src/workspace.ts`
- Modify: `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/helper.tsx`

- [ ] **Step 1: Add the nav item**

In `packages/constants/src/workspace.ts`, inside `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS`, add after the `home` entry:

```ts
  tickets: {
    key: "tickets",
    labelTranslationKey: "sidebar.work_items",
    href: `/tickets/`,
    access: [EUserWorkspaceRoles.ADMIN, EUserWorkspaceRoles.MEMBER, EUserWorkspaceRoles.GUEST],
    highlight: (pathname: string, url: string) => pathname.includes(url),
  },
```

And update the static links list to:

```ts
export const WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS_LINKS: IWorkspaceSidebarNavigationItem[] = [
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["home"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["tickets"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["sprints"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["tracks"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["services"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["ai_scheduler"],
  WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS["docs"],
];
```

- [ ] **Step 2: Register the item key**

In `apps/web/core/components/workspace/sidebar/sidebar-item.tsx`, update `staticItems`:

```ts
const staticItems = [
  "home",
  "pi_chat",
  "projects",
  "tickets",
  "sprints",
  "tracks",
  "services",
  "your_work",
  "stickies",
  "drafts",
  "ai_scheduler",
  "docs",
  ...(additionalStaticItems || []),
];
```

- [ ] **Step 3: Add the icon**

In `apps/web/core/components/workspace/sidebar/helper.tsx`, add `WorkItemsOutline` to the
`@makeplane/propel/icons` import and add a case to the switch:

```tsx
    case "tickets":
      return <WorkItemsOutline className={cn("size-4 flex-shrink-0", className)} />;
```

- [ ] **Step 4: Rebuild the constants package**

Web consumes the built `@plane/constants` dist.

Run: `pnpm --filter=@plane/constants build`
Expected: build succeeds.

- [ ] **Step 5: Type-check**

Run: `pnpm --filter=web check:types`
Expected: exit 0.

- [ ] **Step 6: Commit**

```bash
git add packages/constants/src/workspace.ts \
  apps/web/core/components/workspace/sidebar/sidebar-item.tsx \
  apps/web/core/components/workspace/sidebar/helper.tsx
git commit -m "feat(web): workspace tickets sidebar entry"
```

---

### Task 5: Full verification and deploy

- [ ] **Step 1: Run the full web test suite**

Run: `pnpm --filter=web test`
Expected: all test files pass (382 existing + the new helper tests).

- [ ] **Step 2: Lint**

Run: `pnpm --filter=web check:lint`
Expected: exit 0 (warnings may exist at baseline; ensure none point at the new files).

- [ ] **Step 3: Build constants and web**

```bash
pnpm --filter=@plane/constants build
pnpm --filter=web build
```

Expected: both succeed. Do not change the build env (`VITE_API_BASE_URL` stays
as configured for prod).

- [ ] **Step 4: Restart the prod web service and smoke test**

```bash
systemctl --user restart plane-web-prod.service
curl -s -o /dev/null -w "%{http_code}" http://localhost:3000
```

Expected: `200`.

Manual smoke on the tunnel:

1. Sidebar shows **Tickets** between Home and Sprints.
2. `/[workspaceSlug]/tickets` loads, grouped Backlog → Unstarted → Started → Completed → Cancelled; rows show key + name, project badge, priority, assignees, updated date.
3. `?project=<id>` filters (server-side) and is shareable; invalid id falls back to all.
4. Search filters after the debounce; clearing it restores results.
5. Load more appears when `next_page_results` is true and appends the next page.
6. Clicking a row opens `/[workspaceSlug]/browse/KEY-n`.
7. A workspace/project with no visible tickets shows the empty state.

- [ ] **Step 5: Report**

Report the smoke results. No commit for this task.

---

## Self-review notes

- Spec coverage: route/sidebar (Tasks 3-4), grouped rows + click-through (Task 3),
  filters + pagination + envelope handling (Tasks 1, 3), footer slot (Task 2),
  invalid-project normalization (Task 3), i18n merged keys (Task 3), tests (Task 1),
  verification (Task 5). No backend tasks — spec is web-only.
- Type consistency: helpers use `TWorkspaceTicketRow`
  (`TBaseIssue & { "state__group"?: string | null }`); the list view maps
  `groupWorkspaceTickets` results into the shell's `TWorkspaceAggregateGroupItem`
  (`{ key, label, count, items }`); `normalizeTicketsEnvelope` accepts the
  service's `TIssuesResponse` and returns the plan's envelope type.
- `state__group` (double underscore) is used consistently in the helper, tests,
  and row grouping.
- The project filter is normalized against `workspaceProjectIds` before it is
  used for both the API filter and the shell label, so a stale `?project=` cannot
  produce a silent empty list.
