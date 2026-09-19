# Service Work Item Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the raw-UUID work-item linking flow in the service detail Work items tab with the existing multi-select issue picker, and surface a recoverable error state when the services fetch fails.

**Architecture:** Reuse `ExistingIssuesListModal` (no picker changes) behind an **Add work items** button. Add a batch `linkWorkItems` to `ServiceService`/`ServicesStore` that fans out over the existing idempotent `service-issues/` POST. Add `ServicesStore.errorMap` so `ServicesListView` and `ServiceDetailRoot` render an error + Retry instead of an infinite spinner. No backend change.

**Tech Stack:** React 19 + React Router (`apps/web`), MobX (`makeObservable` / `computedFn`), TypeScript strict, `@makeplane/propel` components, i18next-icu (`packages/i18n`).

**Spec:** `docs/superpowers/specs/2026-09-20-services-work-item-picker-design.md`

## Verification note (read first)

`apps/web` has **no test runner** (see `apps/web/package.json` — no `vitest`/`jest`). Adding one is explicitly out of scope in the spec. Every task therefore verifies with:

- `pnpm --filter=web check:types` — runs `react-router typegen && tsc --noEmit`
- `pnpm --filter=web check:lint` and `pnpm fix:format` (or `pnpm fix` to auto-fix)
- A final manual checklist in Task 9

There are no failing-test steps because there is no runner to run them in. Do not add a runner.

Tasks are ordered by dependency. Do not reorder.

## File structure

**Create**

- `apps/web/core/components/services/service-load-error-state.tsx` — shared error + Retry block for both services entry points

**Modify**

- `apps/web/core/services/service.service.ts` — batch `linkWorkItems`
- `apps/web/core/store/service.store.ts` — `linkWorkItems`, `errorMap`, fetch failure state
- `apps/web/core/components/services/index.ts` — barrel export
- `apps/web/core/components/services/services-list-view.tsx` — error branch + Retry
- `apps/web/core/components/services/detail/root.tsx` — error branch + Retry
- `apps/web/core/components/services/detail/work-items.tsx` — picker replaces UUID input
- `packages/i18n/src/locales/*/service.json` — new keys, remove dead key (via translate skill)

**No change:** Rust backend, DB, `ExistingIssuesListModal`, `ServiceSelect`, dependencies tab, forms, health mock.

---

### Task 1: Batch `linkWorkItems` on `ServiceService`

**Files:**

- Modify: `apps/web/core/services/service.service.ts` (insert after `linkWorkItem`, which ends at line 162)

- [ ] **Step 1: Add the batch method**

Insert immediately after the closing brace of `linkWorkItem` and before `unlinkWorkItem`:

```ts
  async linkWorkItems(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issues: { id: string; identifier?: string; name?: string }[]
  ): Promise<TServiceWorkItemLink[]> {
    return Promise.all(
      issues.map((issue) => this.linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, issue))
    );
  }
```

`linkWorkItem` already POSTs `service-issues/` and the backend returns the existing row when the pair is already linked, so this is idempotent. `TServiceWorkItemLink` is already imported at the top of the file.

- [ ] **Step 2: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS (exit 0). The new method has no callers yet.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/service.service.ts
git commit -m "feat(services): add batch linkWorkItems service method"
```

---

### Task 2: Batch `linkWorkItems` on `ServicesStore`

**Files:**

- Modify: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Extend the `IServiceStore` interface**

In the interface, immediately after the `linkWorkItem` signature (lines 77–83), add:

```ts
linkWorkItems: (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string,
  serviceId: string,
  issues: { id: string; identifier?: string; name?: string }[]
) => Promise<TServiceWorkItemLink[]>;
```

- [ ] **Step 2: Register the observable action**

In the `makeObservable` call, immediately after `linkWorkItem: action,` (line 113), add:

```ts
      linkWorkItems: action,
```

- [ ] **Step 3: Implement the store method**

Immediately after the `linkWorkItem` class method (which ends at line 322) and before `unlinkWorkItem`, add:

```ts
linkWorkItems = async (
  workspaceSlug: string,
  workspaceId: string,
  projectId: string,
  serviceId: string,
  issues: { id: string; identifier?: string; name?: string }[]
) => {
  const links = await this.serviceService.linkWorkItems(workspaceSlug, workspaceId, projectId, serviceId, issues);
  runInAction(() => {
    links.forEach((link) => set(this.workItemLinkMap, [link.id], link));
  });
  return links;
};
```

- [ ] **Step 4: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS (exit 0). `ServicesStore implements IServiceStore` now satisfies the new member.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/service.store.ts
git commit -m "feat(services): add store linkWorkItems action"
```

---

### Task 3: `errorMap` + fetch failure state on `ServicesStore`

**Files:**

- Modify: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Extend the `IServiceStore` interface**

Immediately after `healthMap: Record<string, IServiceHealthSnapshot>;` (line 33), add:

```ts
errorMap: Record<string, boolean>;
```

- [ ] **Step 2: Add the observable field**

Immediately after `healthMap: Record<string, IServiceHealthSnapshot> = {};` (line 93), add:

```ts
errorMap: Record<string, boolean> = {};
```

- [ ] **Step 3: Register the observable**

In the `makeObservable` call, immediately after `healthMap: observable,` (line 105), add:

```ts
      errorMap: observable,
```

- [ ] **Step 4: Clear the error at fetch start**

In `fetchServices`, change the first two lines of the `try` block:

```ts
    try {
      this.loader = true;
```

to:

```ts
    try {
      set(this.errorMap, projectId, false);
      this.loader = true;
```

This runs synchronously inside the action, so a direct `set` is valid (same as `this.loader = true`).

- [ ] **Step 5: Flag the error on fetch failure**

Replace the `catch` block:

```ts
    } catch {
      runInAction(() => {
        this.loader = false;
      });
      return undefined;
    }
```

with:

```ts
    } catch {
      runInAction(() => {
        this.loader = false;
        set(this.errorMap, projectId, true);
      });
      return undefined;
    }
```

`fetchedMap[projectId]` is deliberately left unset so the existing page `useEffect` (guarded on `hasFetched`) can retry.

- [ ] **Step 6: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS (exit 0).

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/store/service.store.ts
git commit -m "fix(services): surface fetch failures via errorMap instead of infinite loading"
```

---

### Task 4: Shared `ServiceLoadErrorState` component

**Files:**

- Create: `apps/web/core/components/services/service-load-error-state.tsx`
- Modify: `apps/web/core/components/services/index.ts`

- [ ] **Step 1: Create the component**

Create `apps/web/core/components/services/service-load-error-state.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";

type Props = {
  onRetry: () => void;
};

export function ServiceLoadErrorState({ onRetry }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <WarningTriangleOutline className="size-8 text-tertiary" />
      <p className="text-sm font-medium text-primary">{t("service.detail.load_error_title")}</p>
      <p className="text-xs text-secondary">{t("service.detail.load_error_description")}</p>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        {t("service.detail.retry")}
      </Button>
    </div>
  );
}
```

Mirrors `apps/web/core/components/common/layout-error-boundary.tsx:22-34` (same icon, button variant and spacing).

- [ ] **Step 2: Export from the barrel**

In `apps/web/core/components/services/index.ts`, immediately after `export * from "./services-list-view";` (line 7), add:

```ts
export * from "./service-load-error-state";
```

- [ ] **Step 3: Verify types and lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS (exit 0). Missing i18n keys are runtime fallbacks only; Task 5 adds them.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/service-load-error-state.tsx apps/web/core/components/services/index.ts
git commit -m "feat(services): add load error state component"
```

---

### Task 5: i18n keys (use the `translate` skill)

**Files:**

- Modify: `packages/i18n/src/locales/*/service.json` (20 locales)

- [ ] **Step 1: Invoke the `translate` skill**

Use the project's `translate` skill (`.claude/skills/translate/SKILL.md`) before touching any locale file. It enforces do-not-translate terms, placeholder preservation and per-locale register.

- [ ] **Step 2: Add the new keys**

In `packages/i18n/src/locales/en/service.json`, inside `"detail"` (after `"unlink_work_item_error"`), add the following — and add a trailing comma to the existing `"unlink_work_item_error"` entry so the JSON stays valid:

```json
      "add_work_items": "Add work items",
      "load_error_title": "Couldn't load services",
      "load_error_description": "Something went wrong while fetching services. Please try again.",
      "retry": "Retry"
```

The English file is the source of truth. Then produce the other 19 locales
(`cs de es fr id it ja ka-ge ko pl pt-BR ro ru sk tr-TR ua vi-VN zh-CN zh-TW`) through the
translate skill — one `service.json` edit per locale, same key paths. No placeholders are
involved in these four strings.

- [ ] **Step 3: Remove the dead key**

Delete `"issue_id_placeholder"` from the `"detail"` object in every one of the 20
`service.json` files (its only consumer, the UUID input, is removed in Task 8).

- [ ] **Step 4: Verify the keys are consistent**

Run: `rg -n "issue_id_placeholder" apps packages`
Expected: no matches.

Run: `rg -l '"add_work_items"' packages/i18n/src/locales/*/service.json | wc -l`
Expected: `20`

Run: `pnpm fix:format`
Expected: exit 0, JSON reformatted if needed.

- [ ] **Step 5: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "i18n(services): add work-item picker and load-error strings"
```

---

### Task 6: Error branch in `ServicesListView`

**Files:**

- Modify: `apps/web/core/components/services/services-list-view.tsx`

- [ ] **Step 1: Add imports and store hooks**

After the existing `useServiceFilter` import, add:

```tsx
import { useWorkspace } from "@/hooks/store/use-workspace";
```

After the `CreateUpdateServiceModal` import, add:

```tsx
import { ServiceLoadErrorState } from "./service-load-error-state";
```

In the store hooks block, change:

```tsx
const { getProjectServiceIds, getFilteredServiceIds, getProjectHealthSummary, loader } = useService();
const { currentProjectDisplayFilters, clearAllFilters } = useServiceFilter();
```

to:

```tsx
const { getProjectServiceIds, getFilteredServiceIds, getProjectHealthSummary, loader, errorMap, fetchServices } =
  useService();
const { currentProjectDisplayFilters, clearAllFilters } = useServiceFilter();
const { currentWorkspace } = useWorkspace();
```

- [ ] **Step 2: Compute the error state and retry handler**

After the `summary` derived value (line 37), add:

```tsx
const workspaceId = currentWorkspace?.id;
const hasError = projectId ? errorMap[projectId.toString()] : false;

const handleRetry = () => {
  if (!workspaceSlug || !workspaceId || !projectId) return;
  fetchServices(workspaceSlug.toString(), workspaceId, projectId.toString());
};
```

- [ ] **Step 3: Render the error state before the loading branch**

At the top of `renderContent`, change:

```tsx
  const renderContent = () => {
    if (loader || projectServiceIds === null || serviceIds === null) {
```

to:

```tsx
  const renderContent = () => {
    if (hasError) {
      return <ServiceLoadErrorState onRetry={handleRetry} />;
    }
    if (loader || projectServiceIds === null || serviceIds === null) {
```

- [ ] **Step 4: Verify types and lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS (exit 0).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/services-list-view.tsx
git commit -m "fix(services): show retryable error state on list fetch failure"
```

---

### Task 7: Error branch in `ServiceDetailRoot`

**Files:**

- Modify: `apps/web/core/components/services/detail/root.tsx`

- [ ] **Step 1: Add imports**

After the `useService` import (line 17), add:

```tsx
import { useWorkspace } from "@/hooks/store/use-workspace";
```

Before the `./dependencies` import (line 20), add:

```tsx
import { ServiceLoadErrorState } from "../service-load-error-state";
```

- [ ] **Step 2: Read `errorMap` and build the retry handler**

Change:

```tsx
const { fetchedMap, getServiceById } = useService();
```

to:

```tsx
const { fetchedMap, getServiceById, errorMap, fetchServices } = useService();
const { currentWorkspace } = useWorkspace();
```

After the `hasFetched` derived value (line 44), add:

```tsx
const workspaceId = currentWorkspace?.id;
const hasError = pid ? errorMap[pid] : false;

const handleRetry = () => {
  if (!workspaceSlug || !workspaceId || !pid) return;
  fetchServices(workspaceSlug.toString(), workspaceId, pid);
};
```

- [ ] **Step 3: Render the error state before the loading branch**

Change the `!service` branch:

```tsx
  if (!service) {
    if (!hasFetched) return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
```

to:

```tsx
  if (!service) {
    if (hasError) return <ServiceLoadErrorState onRetry={handleRetry} />;
    if (!hasFetched) return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
```

- [ ] **Step 4: Verify types and lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS (exit 0).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/detail/root.tsx
git commit -m "fix(services): show retryable error state on detail fetch failure"
```

---

### Task 8: Replace UUID input with the issue picker in `work-items.tsx`

**Files:**

- Modify: `apps/web/core/components/services/detail/work-items.tsx` (full rewrite, 124 lines)

- [ ] **Step 1: Replace the file contents**

Replace the whole file with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse } from "@plane/types";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
};

export const ServiceWorkItems = observer(function ServiceWorkItems(props: Props) {
  const { serviceId } = props;
  // states
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getWorkItemLinksByService, linkWorkItems, unlinkWorkItem } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = currentWorkspace?.id;
  const links = getWorkItemLinksByService(serviceId);
  const linkedIssueIds = links.map((link) => link.issue_id);

  const handleAddWorkItems = async (issues: ISearchIssueResponse[]) => {
    if (!slug || !workspaceId || !pid || issues.length === 0) return;
    try {
      await linkWorkItems(
        slug,
        workspaceId,
        pid,
        serviceId,
        issues.map((issue) => ({
          id: issue.id,
          identifier: `${issue.project__identifier}-${issue.sequence_id}`,
          name: issue.name,
        }))
      );
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.link_work_item_error"),
      });
    }
  };

  const handleUnlink = async (linkId: string) => {
    if (!slug || !workspaceId || !pid) return;
    try {
      await unlinkWorkItem(slug, workspaceId, pid, linkId);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.unlink_work_item_error"),
      });
    }
  };

  return (
    <div className="flex max-w-3xl flex-col gap-3">
      <div className="flex items-center gap-2">
        <Button variant="primary" size="sm" onClick={() => setIsPickerOpen(true)} disabled={!slug || !pid}>
          {t("service.detail.add_work_items")}
        </Button>
      </div>
      {links.length === 0 ? (
        <p className="text-13 text-tertiary">{t("service.detail.no_work_items")}</p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {links.map((link) => (
            <div
              key={link.id}
              className="flex items-center justify-between gap-2 rounded-md border border-subtle px-3 py-2"
            >
              <div className="flex min-w-0 flex-col">
                <span className="text-13 font-medium text-primary">{link.issue_identifier ?? link.issue_id}</span>
                <span
                  className="truncate text-12 text-secondary"
                  title={link.issue_name ?? t("service.detail.untitled")}
                >
                  {link.issue_name ?? t("service.detail.untitled")}
                </span>
              </div>
              <button
                type="button"
                onClick={() => handleUnlink(link.id)}
                className="shrink-0 text-12 text-tertiary hover:text-primary"
              >
                {t("remove")}
              </button>
            </div>
          ))}
        </div>
      )}
      <ExistingIssuesListModal
        isOpen={isPickerOpen}
        handleClose={() => setIsPickerOpen(false)}
        workspaceSlug={slug}
        projectId={pid}
        searchParams={{ workspace_search: false }}
        selectedWorkItemIds={linkedIssueIds}
        shouldHideIssue={(issue) => linkedIssueIds.includes(issue.id)}
        handleOnSubmit={handleAddWorkItems}
      />
    </div>
  );
});
```

Notes:

- `handleAddWorkItems` swallows the error after toasting so `ExistingIssuesListModal`'s
  `await handleOnSubmit(...).finally(...)` never sees a rejection and always closes.
- `searchParams.workspace_search: false` keeps the search project-scoped; the Rust
  `service-issues/` POST rejects an issue that does not belong to the project.
- `shouldHideIssue` removes already-linked issues from the results; `selectedWorkItemIds`
  keeps the modal's own selection state consistent.

- [ ] **Step 2: Verify types and lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS (exit 0).

- [ ] **Step 3: Verify no stale references remain**

Run: `rg -n "service-work-item-id|issue_id_placeholder" apps packages`
Expected: no matches.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/detail/work-items.tsx
git commit -m "feat(services): replace UUID input with multi-select work item picker"
```

---

### Task 9: Full verification and manual checklist

**Files:** none (verification only)

- [ ] **Step 1: Run all static checks**

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm fix:format
```

Expected: all exit 0, no diff after `fix:format` (`git status --short` unchanged for code files).

- [ ] **Step 2: Manual checklist against the dev server**

Start `pnpm --filter=web dev` (or use the running dev server), then:

1. Open a project → Services → a service → **Work items**.
2. Click **Add work items** → the modal opens with project issues; typing filters them.
3. Select two issues → **Add selected** → both appear in the list with identifier + name.
4. Re-open the picker → the two linked issues are no longer listed.
5. Click **Remove** on one link → it disappears.
6. Stop the API (`docker compose stop api` or point `VITE_API_BASE_URL` at a dead port) and
   reload the Services list → error state with **Retry**, not a spinner. Restore the API,
   click **Retry** → the list renders.
7. Open a service detail URL while the API is down → same error state + **Retry**.

- [ ] **Step 3 (conditional): Rebuild prod for the tunnel demo**

Only if the change must appear on the tunnel (AGENTS.md): run

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build exits 0 and the service restarts. Skip when working against the dev server.

- [ ] **Step 4: Commit any formatting fallout**

```bash
git status --short
# if oxfmt changed files:
git add -A
git commit -m "chore(services): apply formatting"
```

If `git status --short` is clean, skip the commit.
