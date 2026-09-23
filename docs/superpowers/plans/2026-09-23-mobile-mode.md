# Mobile Mode (Responsive Web for Phones) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the web app usable on phones (`<768px`) for viewing/updating work items, comments/attachments, and navigation/search — without regressing desktop.

**Architecture:** Add a viewport-based media-query hook pair (`useMobileViewport`, `useTouchPointer`) and a pure render-only layout resolver. All mobile layout behavior is gated on the viewport; UA-based interaction gating (`usePlatformOS`) is left untouched. On mobile the work-item layout is always List; a persisted desktop layout (kanban/calendar/gantt/spreadsheet) falls back to list in render only and is never written back to the store.

**Tech Stack:** React 19, React Router 7 (client-only, `ssr: false`), MobX, Tailwind CSS 4.1.17, Vitest (node env), TypeScript strict.

**Refinements vs the approved spec** (`docs/superpowers/specs/2026-09-23-mobile-mode-design.md`):

- The mobile layout restriction lives in the issues mobile header (`layouts={[LIST]}`), not in `core/components/issues/filters.tsx`. That component's `MobileLayoutSelection` only renders inside the desktop header (`hidden md:flex`), so it never appears at phone viewport — filtering it there would be dead code.
- New files `apps/web/core/hooks/use-mobile-viewport.ts` (both hooks, one cohesive media-query module) and `apps/web/core/components/issues/issue-layouts/mobile-layout.ts` (pure resolver) replace "add to `utils.tsx`" — keeps unit tests free of the heavy spreadsheet/gantt import graph.
- Mobile-only styles use the repo's existing `md:hidden` / `hidden md:block` convention instead of introducing `max-md:` variants.

---

## File Structure

**Create:**

- `apps/web/core/hooks/use-mobile-viewport.ts` — media-query hooks: `useMobileViewport`, `useTouchPointer`, pure `getIsMobileViewport`, `getIsTouchPointer`.
- `apps/web/core/hooks/use-mobile-viewport.test.ts` — unit tests for the pure getters.
- `apps/web/core/components/issues/issue-layouts/mobile-layout.ts` — pure `resolveWorkItemLayout` fallback.
- `apps/web/core/components/issues/issue-layouts/mobile-layout.test.ts` — unit tests for the resolver.

**Modify (in task order):**

- `apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx` — resolver
- `apps/web/core/components/issues/issue-layouts/roots/cycle-layout-root.tsx` — resolver
- `apps/web/core/components/issues/issue-layouts/roots/module-layout-root.tsx` — resolver
- `apps/web/core/components/issues/issue-layouts/roots/all-issue-layout-root.tsx` — resolver
- `apps/web/core/components/issues/issue-layouts/roots/project-view-layout-root.tsx` — resolver
- `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/issues/(list)/mobile-header.tsx` — list-only + filters toggle
- `apps/web/core/components/issues/peek-overview/view.tsx` — phone padding + stacked secondary column
- `apps/web/core/components/issues/issue-layouts/filters/header/helpers/dropdown.tsx` — fluid panel
- `apps/web/core/components/rich-filters/filters-row.tsx` — wrap filter row on mobile
- `apps/web/core/components/workspace/content-wrapper.tsx` — hide app rail
- `apps/web/core/components/sidebar/resizable-sidebar.tsx` — viewport detection, backdrop, Escape
- `apps/web/core/components/navigation/top-navigation-root.tsx` — mobile search entry
- `apps/web/core/components/workspace/sidebar/workspace-menu-root.tsx` — cap menu width
- `apps/web/core/components/comments/comment-create.tsx` — safe-area padding
- `apps/web/core/components/editor/lite-text/editor.tsx` — touch flag
- `apps/web/core/components/editor/rich-text/editor.tsx` — touch flag

---

### Task 1: Media-query hooks (viewport + touch pointer)

**Files:**

- Create: `apps/web/core/hooks/use-mobile-viewport.ts`
- Test: `apps/web/core/hooks/use-mobile-viewport.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/hooks/use-mobile-viewport.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { getIsMobileViewport, getIsTouchPointer } from "./use-mobile-viewport";

const stubMatchMedia = (matches: (query: string) => boolean) => {
  vi.stubGlobal("window", {
    matchMedia: vi.fn((query: string) => ({ matches: matches(query) })),
  });
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("getIsMobileViewport", () => {
  it("returns false when window is unavailable", () => {
    expect(getIsMobileViewport()).toBe(false);
  });

  it("returns true when the mobile media query matches", () => {
    stubMatchMedia((query) => query === "(max-width: 767px)");
    expect(getIsMobileViewport()).toBe(true);
  });

  it("returns false when the mobile media query does not match", () => {
    stubMatchMedia(() => false);
    expect(getIsMobileViewport()).toBe(false);
  });
});

describe("getIsTouchPointer", () => {
  it("returns true when the coarse pointer query matches", () => {
    stubMatchMedia((query) => query === "(pointer: coarse)");
    expect(getIsTouchPointer()).toBe(true);
  });

  it("returns false when matchMedia is missing", () => {
    vi.stubGlobal("window", {});
    expect(getIsTouchPointer()).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=web exec vitest run core/hooks/use-mobile-viewport.test.ts`
Expected: FAIL — `Failed to resolve import "./use-mobile-viewport"`.

- [ ] **Step 3: Write minimal implementation**

Create `apps/web/core/hooks/use-mobile-viewport.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useSyncExternalStore } from "react";

export const MOBILE_VIEWPORT_QUERY = "(max-width: 767px)";
export const TOUCH_POINTER_QUERY = "(pointer: coarse)";

const getMatches = (query: string): boolean => {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  return window.matchMedia(query).matches;
};

export const getIsMobileViewport = (): boolean => getMatches(MOBILE_VIEWPORT_QUERY);
export const getIsTouchPointer = (): boolean => getMatches(TOUCH_POINTER_QUERY);

export const useMediaQuery = (query: string): boolean => {
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
      const mediaQueryList = window.matchMedia(query);
      mediaQueryList.addEventListener("change", onStoreChange);
      return () => mediaQueryList.removeEventListener("change", onStoreChange);
    },
    [query]
  );
  const getSnapshot = useCallback(() => getMatches(query), [query]);
  return useSyncExternalStore(subscribe, getSnapshot, () => false);
};

export const useMobileViewport = (): boolean => useMediaQuery(MOBILE_VIEWPORT_QUERY);
export const useTouchPointer = (): boolean => useMediaQuery(TOUCH_POINTER_QUERY);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web exec vitest run core/hooks/use-mobile-viewport.test.ts`
Expected: PASS — 5 tests.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/hooks/use-mobile-viewport.ts apps/web/core/hooks/use-mobile-viewport.test.ts
git commit -m "feat(web): add viewport and touch-pointer media query hooks"
```

---

### Task 2: Render-only mobile layout resolver

**Files:**

- Create: `apps/web/core/components/issues/issue-layouts/mobile-layout.ts`
- Test: `apps/web/core/components/issues/issue-layouts/mobile-layout.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/components/issues/issue-layouts/mobile-layout.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { EIssueLayoutTypes } from "@plane/types";
import { resolveWorkItemLayout } from "./mobile-layout";

describe("resolveWorkItemLayout", () => {
  it("returns the persisted layout unchanged on desktop", () => {
    expect(resolveWorkItemLayout(EIssueLayoutTypes.SPREADSHEET, false)).toBe(EIssueLayoutTypes.SPREADSHEET);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.LIST, false)).toBe(EIssueLayoutTypes.LIST);
  });

  it("returns list for desktop-only layouts on mobile", () => {
    expect(resolveWorkItemLayout(EIssueLayoutTypes.KANBAN, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.CALENDAR, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.GANTT, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(EIssueLayoutTypes.SPREADSHEET, true)).toBe(EIssueLayoutTypes.LIST);
  });

  it("keeps list and undefined as-is on mobile", () => {
    expect(resolveWorkItemLayout(EIssueLayoutTypes.LIST, true)).toBe(EIssueLayoutTypes.LIST);
    expect(resolveWorkItemLayout(undefined, true)).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm --filter=@plane/types build && pnpm --filter=web exec vitest run core/components/issues/issue-layouts/mobile-layout.test.ts`
Expected: FAIL — `Failed to resolve import "./mobile-layout"`. (The `@plane/types` build step is required because its `dist/` is gitignored; skip it if `packages/types/dist/index.mjs` already exists.)

- [ ] **Step 3: Write minimal implementation**

Create `apps/web/core/components/issues/issue-layouts/mobile-layout.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { EIssueLayoutTypes } from "@plane/types";

const MOBILE_WORK_ITEM_LAYOUT = "list" as EIssueLayoutTypes;

/**
 * Render-only fallback for the work item layout.
 *
 * On a mobile viewport a persisted desktop-only layout (kanban/calendar/gantt/spreadsheet)
 * renders as list. The persisted value is never written back, so widening the viewport
 * restores the user's chosen layout.
 */
export const resolveWorkItemLayout = (
  layout: EIssueLayoutTypes | undefined,
  isMobileViewport: boolean
): EIssueLayoutTypes | undefined => {
  if (!isMobileViewport || !layout) return layout;
  return layout === MOBILE_WORK_ITEM_LAYOUT ? layout : MOBILE_WORK_ITEM_LAYOUT;
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm --filter=web exec vitest run core/components/issues/issue-layouts/mobile-layout.test.ts`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/mobile-layout.ts apps/web/core/components/issues/issue-layouts/mobile-layout.test.ts
git commit -m "feat(web): add render-only mobile work item layout resolver"
```

---

### Task 3: Apply the resolver in all five layout roots

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/roots/cycle-layout-root.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/roots/module-layout-root.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/roots/all-issue-layout-root.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/roots/project-view-layout-root.tsx`

The hook call must sit with the other hooks, above any early return. In each file add these two imports:

```ts
import { useMobileViewport } from "@/hooks/use-mobile-viewport";
```

and, in the local imports group (next to the other `../` imports):

```ts
import { resolveWorkItemLayout } from "../mobile-layout";
```

- [ ] **Step 1: project-layout-root.tsx**

Add the hook next to the existing store hooks (`apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx:51`):

```ts
const { issues, issuesFilter } = useIssues(EIssuesStoreType.PROJECT);
const isMobileViewport = useMobileViewport();
```

Replace line 54:

```ts
const activeLayout = workItemFilters?.displayFilters?.layout;
```

with:

```ts
const activeLayout = resolveWorkItemLayout(workItemFilters?.displayFilters?.layout, isMobileViewport);
```

- [ ] **Step 2: cycle-layout-root.tsx**

Add the hook after `const { getCycleById } = useCycle();` (`cycle-layout-root.tsx:60`):

```ts
const isMobileViewport = useMobileViewport();
```

Replace line 65:

```ts
const activeLayout = workItemFilters?.displayFilters?.layout;
```

with:

```ts
const activeLayout = resolveWorkItemLayout(workItemFilters?.displayFilters?.layout, isMobileViewport);
```

- [ ] **Step 3: module-layout-root.tsx**

Add the hook after `const { issuesFilter } = useIssues(EIssuesStoreType.MODULE);` (`module-layout-root.tsx:52`):

```ts
const isMobileViewport = useMobileViewport();
```

Replace line 55:

```ts
const activeLayout = workItemFilters?.displayFilters?.layout || undefined;
```

with:

```ts
const activeLayout = resolveWorkItemLayout(workItemFilters?.displayFilters?.layout, isMobileViewport);
```

- [ ] **Step 4: all-issue-layout-root.tsx**

Add the hook after `const { fetchAllGlobalViews, getViewDetailsById } = useGlobalView();` (`all-issue-layout-root.tsx:50`):

```ts
const isMobileViewport = useMobileViewport();
```

Replace line 53:

```ts
const activeLayout: EIssueLayoutTypes | undefined = workItemFilters?.displayFilters?.layout;
```

with:

```ts
const activeLayout: EIssueLayoutTypes | undefined = resolveWorkItemLayout(
  workItemFilters?.displayFilters?.layout,
  isMobileViewport
);
```

- [ ] **Step 5: project-view-layout-root.tsx**

Add the hook after `const { getViewById } = useProjectView();` (`project-view-layout-root.tsx:52`):

```ts
const isMobileViewport = useMobileViewport();
```

Replace line 57:

```ts
const activeLayout = workItemFilters?.displayFilters?.layout;
```

with:

```ts
const activeLayout = resolveWorkItemLayout(workItemFilters?.displayFilters?.layout, isMobileViewport);
```

- [ ] **Step 6: Verify types**

Run: `pnpm turbo run check:types --filter=web`
Expected: PASS, no TypeScript errors.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx \
  apps/web/core/components/issues/issue-layouts/roots/cycle-layout-root.tsx \
  apps/web/core/components/issues/issue-layouts/roots/module-layout-root.tsx \
  apps/web/core/components/issues/issue-layouts/roots/all-issue-layout-root.tsx \
  apps/web/core/components/issues/issue-layouts/roots/project-view-layout-root.tsx
git commit -m "feat(web): fall back to list layout on mobile viewports"
```

---

### Task 4: Issues mobile header — list-only layout + filters toggle

**Files:**

- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/issues/(list)/mobile-header.tsx`

- [ ] **Step 1: Add the filters toggle import**

After the existing component imports (`mobile-header.tsx:22`), add:

```ts
import { WorkItemFiltersToggle } from "@/components/work-item-filters/filters-toggle";
```

- [ ] **Step 2: Restrict the layout options to list**

Replace lines 72-75:

```tsx
<MobileLayoutSelection
  layouts={[EIssueLayoutTypes.LIST, EIssueLayoutTypes.KANBAN, EIssueLayoutTypes.CALENDAR]}
  onChange={handleLayoutChange}
/>
```

with:

```tsx
<MobileLayoutSelection layouts={[EIssueLayoutTypes.LIST]} onChange={handleLayoutChange} />
```

- [ ] **Step 3: Add the filters cell**

Insert this cell between the Display cell (which ends at `</div>` on line 99) and the analytics button (line 101):

```tsx
<div className="flex flex-grow items-center justify-center border-l border-subtle text-13 text-secondary">
  <WorkItemFiltersToggle entityType={EIssuesStoreType.PROJECT} entityId={projectId ?? ""} />
</div>
```

`projectId` is already destructured from `useParams()` at line 31; no new hook is needed.

- [ ] **Step 4: Verify types and lint**

Run: `pnpm turbo run check:types check:lint --filter=web`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/issues/(list)/mobile-header.tsx"
git commit -m "feat(web): list-only layout and filters toggle in issues mobile header"
```

---

### Task 5: Work item peek at phone width

**Files:**

- Modify: `apps/web/core/components/issues/peek-overview/view.tsx`

- [ ] **Step 1: Reduce content padding on mobile**

Replace line 177:

```tsx
                  <div className="relative flex flex-col gap-3 space-y-3 px-8 py-5">
```

with:

```tsx
                  <div className="relative flex flex-col gap-3 space-y-3 px-4 py-5 md:px-8">
```

- [ ] **Step 2: Stack the modal/full-screen secondary column on mobile**

Replace line 216:

```tsx
                  <div className="vertical-scrollbar flex h-full w-full overflow-auto">
```

with:

```tsx
                  <div className="vertical-scrollbar flex h-full w-full flex-col overflow-auto md:flex-row">
```

- [ ] **Step 3: Make the secondary column fluid on mobile**

Replace the class string on line 250:

```tsx
                      className={`vertical-scrollbar scrollbar-sm h-full !w-[400px] flex-shrink-0 overflow-hidden border-l border-subtle p-4 py-5 ${
```

with:

```tsx
                      className={`vertical-scrollbar scrollbar-sm h-full w-full flex-shrink-0 overflow-hidden border-subtle p-4 py-5 md:!w-[400px] md:border-l ${
```

- [ ] **Step 4: Verify types**

Run: `pnpm turbo run check:types --filter=web`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/peek-overview/view.tsx
git commit -m "fix(web): make work item peek usable at phone width"
```

---

### Task 6: Fluid filter panels on mobile

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/filters/header/helpers/dropdown.tsx`
- Modify: `apps/web/core/components/rich-filters/filters-row.tsx`

- [ ] **Step 1: Fluid legacy filter dropdown panel**

Replace line 108 of `dropdown.tsx`:

```tsx
                <div className="flex max-h-[30rem] w-[18.75rem] flex-col overflow-hidden lg:max-h-[37.5rem]">
```

with:

```tsx
                <div className="flex max-h-[30rem] w-[calc(100vw-2rem)] flex-col overflow-hidden md:w-[18.75rem] lg:max-h-[37.5rem]">
```

- [ ] **Step 2: Let the rich filter row wrap on mobile**

Replace the `mainContent` class in `filters-row.tsx` (around line 96):

```tsx
    <div className="flex w-full items-start gap-2 rounded-lg bg-layer-1 px-4 py-2">
```

with:

```tsx
    <div className="flex w-full flex-wrap items-start gap-2 rounded-lg bg-layer-1 px-4 py-2 md:flex-nowrap">
```

- [ ] **Step 3: Verify types**

Run: `pnpm turbo run check:types --filter=web`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/filters/header/helpers/dropdown.tsx apps/web/core/components/rich-filters/filters-row.tsx
git commit -m "fix(web): make filter panels fluid at phone width"
```

---

### Task 7: Hide the app rail on mobile

**Files:**

- Modify: `apps/web/core/components/workspace/content-wrapper.tsx`

- [ ] **Step 1: Add the viewport hook and derive rail visibility**

Replace lines 10-22:

```tsx
import { cn } from "@plane/utils";
import { AppRailRoot } from "@/components/navigation";
import { useAppRailVisibility } from "@/lib/app-rail";
import { TopNavigationRoot } from "@/components/navigation/top-navigation-root";
import { AiAssistantSidebar } from "@/components/ai/assistant-sidebar/root";

export const WorkspaceContentWrapper = observer(function WorkspaceContentWrapper({
  children,
}: {
  children: React.ReactNode;
}) {
  // Use the context to determine if app rail should render
  const { shouldRenderAppRail } = useAppRailVisibility();
```

with:

```tsx
import { cn } from "@plane/utils";
import { AppRailRoot } from "@/components/navigation";
import { useAppRailVisibility } from "@/lib/app-rail";
import { TopNavigationRoot } from "@/components/navigation/top-navigation-root";
import { AiAssistantSidebar } from "@/components/ai/assistant-sidebar/root";
import { useMobileViewport } from "@/hooks/use-mobile-viewport";

export const WorkspaceContentWrapper = observer(function WorkspaceContentWrapper({
  children,
}: {
  children: React.ReactNode;
}) {
  // Use the context to determine if app rail should render
  const { shouldRenderAppRail } = useAppRailVisibility();
  // hooks
  const isMobileViewport = useMobileViewport();
  // derived values
  const showAppRail = shouldRenderAppRail && !isMobileViewport;
```

- [ ] **Step 2: Use the derived value in the render**

Replace line 29:

```tsx
{
  shouldRenderAppRail && <AppRailRoot />;
}
```

with:

```tsx
{
  showAppRail && <AppRailRoot />;
}
```

Replace the `pl-0!` condition on line 34:

```tsx
              "pl-0!": shouldRenderAppRail,
```

with:

```tsx
              "pl-0!": showAppRail,
```

- [ ] **Step 3: Verify types**

Run: `pnpm turbo run check:types --filter=web`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/workspace/content-wrapper.tsx
git commit -m "feat(web): hide app rail on mobile viewports"
```

---

### Task 8: Sidebar drawer — viewport detection, backdrop, Escape close

**Files:**

- Modify: `apps/web/core/components/sidebar/resizable-sidebar.tsx`

- [ ] **Step 1: Switch layout detection from UA to viewport**

Replace line 10:

```ts
import { usePlatformOS } from "@plane/hooks";
```

with:

```ts
import { useMobileViewport } from "@/hooks/use-mobile-viewport";
```

Replace line 60:

```ts
const { isMobile } = usePlatformOS();
```

with:

```ts
const isMobile = useMobileViewport();
```

Keep the variable name `isMobile` — it is used for positioning (`absolute`) and `data-prevent-outside-click` only, and the semantics are now viewport-based.

- [ ] **Step 2: Add Escape-to-close**

Insert after the unmount cleanup effect (after line 143):

```ts
// Close the sidebar drawer with Escape on mobile viewports
useEffect(() => {
  if (!isMobile || isCollapsed) return;
  const handleKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") toggleCollapsed();
  };
  window.addEventListener("keydown", handleKeyDown);
  return () => window.removeEventListener("keydown", handleKeyDown);
}, [isMobile, isCollapsed, toggleCollapsed]);
```

- [ ] **Step 3: Add the backdrop**

Insert immediately after the opening `<>` of the return (line 178, before the `{/* Main Sidebar */}` comment):

```tsx
{
  /* Mobile drawer backdrop */
}
{
  isMobile && !isCollapsed && (
    <button
      type="button"
      aria-label="Close sidebar"
      className="fixed inset-0 z-[19] cursor-default bg-black/20"
      onClick={() => toggleCollapsed()}
    />
  );
}
```

- [ ] **Step 4: Verify types and lint**

Run: `pnpm turbo run check:types check:lint --filter=web`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/sidebar/resizable-sidebar.tsx
git commit -m "feat(web): sidebar drawer backdrop and escape close on mobile"
```

---

### Task 9: Mobile search entry point in the top navigation

**Files:**

- Modify: `apps/web/core/components/navigation/top-navigation-root.tsx`
- Modify: `apps/web/core/components/workspace/sidebar/workspace-menu-root.tsx`

- [ ] **Step 1: Add imports and the Power-K toggle**

In `top-navigation-root.tsx`, add to the existing imports:

```ts
import { SearchOutline } from "@makeplane/propel/icons";
import { IconButton } from "@plane/propel/icon-button";
import { usePowerK } from "@/hooks/store/use-power-k";
```

Inside the component, after `const { config } = useInstance();` (line 32), add:

```ts
const { togglePowerKModal } = usePowerK();
```

- [ ] **Step 2: Hide the inline search field below md**

Replace lines 58-61:

```tsx
{
  /* Power K Search */
}
<div className="shrink-0">
  <TopNavPowerK />
</div>;
```

with:

```tsx
{
  /* Power K Search — inline field on desktop only */
}
<div className="hidden shrink-0 md:block">
  <TopNavPowerK />
</div>;
```

- [ ] **Step 3: Add the mobile search button**

Insert as the first child of the "Additional Actions" group (after line 63):

```tsx
<IconButton
  size="base"
  variant="ghost"
  icon={SearchOutline}
  className="md:hidden"
  aria-label="Search"
  onClick={() => togglePowerKModal(true)}
/>
```

- [ ] **Step 4: Cap the workspace menu width**

In `workspace-menu-root.tsx`, replace the class on the menu container (around line 149):

```tsx
                    "fixed z-21 mt-1 flex w-[19rem] origin-top-left flex-col divide-y divide-subtle rounded-md border-[0.5px] border-strong bg-surface-1 shadow-raised-200 outline-none",
```

with:

```tsx
                    "fixed z-21 mt-1 flex w-[19rem] max-w-[calc(100vw-2rem)] origin-top-left flex-col divide-y divide-subtle rounded-md border-[0.5px] border-strong bg-surface-1 shadow-raised-200 outline-none",
```

- [ ] **Step 5: Verify types and lint**

Run: `pnpm turbo run check:types check:lint --filter=web`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/navigation/top-navigation-root.tsx apps/web/core/components/workspace/sidebar/workspace-menu-root.tsx
git commit -m "feat(web): mobile search entry point in top navigation"
```

---

### Task 10: Safe-area padding for the sticky comment box

**Files:**

- Modify: `apps/web/core/components/comments/comment-create.tsx`

- [ ] **Step 1: Add safe-area padding**

Replace line 95:

```tsx
      className={cn("sticky bottom-0 z-[4] bg-surface-1 sm:static")}
```

with:

```tsx
      className={cn("sticky bottom-0 z-[4] bg-surface-1 pb-[env(safe-area-inset-bottom)] sm:static sm:pb-0")}
```

- [ ] **Step 2: Verify types**

Run: `pnpm turbo run check:types --filter=web`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/comments/comment-create.tsx
git commit -m "fix(web): safe-area padding for sticky comment box"
```

---

### Task 11: Pass the touch-pointer flag to editors

**Files:**

- Modify: `apps/web/core/components/editor/lite-text/editor.tsx`
- Modify: `apps/web/core/components/editor/rich-text/editor.tsx`

- [ ] **Step 1: Lite text editor**

In `lite-text/editor.tsx`, add the hook import next to the other `@/hooks` imports:

```ts
import { useTouchPointer } from "@/hooks/use-mobile-viewport";
```

Destructure the prop out of `props` — add `isTouchDevice: isTouchDeviceProp,` to the destructuring block (after `editorClassName = "",`, around line 78):

```ts
    editorClassName = "",
    isTouchDevice: isTouchDeviceProp,
```

Add the hook next to `const { getUserDetails } = useMember();` (around line 93):

```ts
const isTouchPointer = useTouchPointer();
```

Then compute the resolved value after `const { getEditorMetaData } = useParseEditorContent({...});` (around line 100):

```ts
// derived values
const isTouchDevice = isTouchDeviceProp ?? isTouchPointer;
```

Finally, pass it to the editor — add after `editable={editable}` on `LiteTextEditorWithRef` (line 139):

```tsx
isTouchDevice = { isTouchDevice };
```

- [ ] **Step 2: Rich text editor**

In `rich-text/editor.tsx`, add the hook import next to the other `@/hooks` imports:

```ts
import { useTouchPointer } from "@/hooks/use-mobile-viewport";
```

Destructure the prop — add `isTouchDevice: isTouchDeviceProp,` after `projectId,` in the destructuring block (line 50):

```ts
    projectId,
    isTouchDevice: isTouchDeviceProp,
```

Add the hook and derived value after `const { getUserDetails } = useMember();` (line 55):

```ts
const isTouchPointer = useTouchPointer();
// derived values
const isTouchDevice = isTouchDeviceProp ?? isTouchPointer;
```

Pass it to the editor — add after `editable={editable}` on `RichTextEditorWithRef` (line 84):

```tsx
isTouchDevice = { isTouchDevice };
```

- [ ] **Step 3: Verify types and lint**

Run: `pnpm turbo run check:types check:lint --filter=web`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/editor/lite-text/editor.tsx apps/web/core/components/editor/rich-text/editor.tsx
git commit -m "fix(web): pass touch pointer flag to editors"
```

---

### Task 12: Verification, device QA, desktop regression

**Files:** none (verification only)

- [ ] **Step 1: Run all automated gates**

```bash
pnpm turbo run check:types --filter=web
pnpm turbo run check:lint --filter=web
pnpm turbo run test --filter=web
```

Expected: all PASS. `test` runs the full `apps/web` vitest suite (existing tests plus the 8 new ones).

- [ ] **Step 2: Run the app for manual QA**

Dev server (local):

```bash
pnpm --filter=web dev
```

If testing through the tunnel instead, follow `AGENTS.md`: `pnpm --filter=web build` then `systemctl --user restart plane-web-prod.service`.

- [ ] **Step 3: Mobile QA (DevTools 375×667 + real phone)**

Check each item at 375px width:

1. App rail is gone; content spans the full width.
2. Sidebar toggle (header) opens the drawer; backdrop dims the page and closes it on tap; Escape closes it.
3. Search icon in the top nav opens the Power-K modal full-width; search returns work items and navigates.
4. Workspace menu opens without horizontal overflow.
5. Issues list renders; layout selector shows List only; clicking a row opens the peek full-width; status/assignee/priority can be changed; close control works.
6. With a desktop-persisted layout (e.g. set Spreadsheet on desktop, then reload at 375px) the list renders and the desktop preference survives (reload at 1440px shows Spreadsheet again).
7. Filters toggle in the mobile header shows the filter row; conditions can be added/removed and the row does not overflow horizontally.
8. Comments: sticky comment box sits above the home indicator; typing, submitting, and uploading an attachment work; the attachment preview and the editor full-screen image modal (already fluid: `fixed inset-0 size-full` in `packages/editor/src/extensions/custom-image/components/toolbar/full-screen/modal.tsx`) render without horizontal overflow.
9. Editor on a touch device shows the touch behavior (image block / toolbar) — real device check.
10. Rotation to landscape does not break the layout.

- [ ] **Step 4: Desktop regression checklist (1440 / 1280 / 1024px)**

1. App rail renders and its collapse preference still works.
2. Sidebar resize, collapse, and hover-peek work; no backdrop ever appears.
3. Inline Power-K search works; mobile search icon is hidden.
4. All five layout options are selectable and render (list, kanban, calendar, gantt, spreadsheet).
5. Peek renders in all three modes (side-peek, modal, full-screen) with the original 400px secondary column.
6. Filter dropdown panel is still `18.75rem` wide.
7. Comment box is not sticky and has no extra padding.
8. Editors behave as before with a mouse.

- [ ] **Step 5: Report results**

Record pass/fail per checklist item. If any item fails, fix and re-run the relevant automated gates before re-checking. No commit for this task unless a fix was required.

---

## Self-Review Notes

- **Spec coverage:** all spec sections map to tasks — viewport/UA rule (Tasks 1, 7-9, 11), render-only fallback (Tasks 2-3), mobile nav shell (Tasks 7-9), work items (Tasks 3-6), comments/attachments (Tasks 10-11), filters/search (Tasks 4, 6, 9), desktop guardrails (all tasks, verified in Task 12 Step 4), testing (Tasks 1-2, 12).
- **Known spec deviation:** the app-rail/backdrop gating uses `useMobileViewport` and the repo's `md:` convention rather than `max-md:` variants — recorded in the header refinements.
- **Spec item with no code change:** "attachment/image previews fluid at phone width" — verified during planning that there is no fixed-width preview panel (`apps/web/core/components/issues/attachment/*` has none; the editor image modal is `fixed inset-0 size-full`). Covered by Task 12 Step 3 item 8 instead.
- **Type consistency:** `resolveWorkItemLayout(layout, isMobileViewport)` and `useMobileViewport()`/`useTouchPointer()` names are identical across Tasks 1-3, 7-9, 11.
