# Service Detail Tabbed Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move service relationships and tickets into a collapsible, URL-state-driven right pane with Properties, Relationships, and Tickets tabs, leaving the main column for title and description.

**Architecture:** Add a service-specific pane modeled on the Knowledge Base navigation pane. A pure helper module owns tab keys and validation; a hook maps `paneTab` query params to open/close/tab-change handlers; the pane composes the existing service detail sections with new `layout="pane"` variants. The peek overview keeps the default `layout="page"` behavior.

**Tech Stack:** React, MobX, React Router (`next/navigation` shim), `@plane/propel/tabs`, Tailwind, Vitest, pnpm.

---

## File Structure

New:

- `apps/web/core/components/services/detail/pane/helpers.ts` — constants, tab metadata, validation.
- `apps/web/core/components/services/detail/pane/helpers.test.ts` — unit tests.
- `apps/web/core/components/services/detail/pane/index.ts` — barrel.
- `apps/web/core/components/services/detail/pane/root.tsx` — aside chrome and Tabs.
- `apps/web/core/components/services/detail/pane/tabs-list.tsx` — tab triggers.
- `apps/web/core/components/services/detail/pane/tab-panels/root.tsx` — tab content switch.
- `apps/web/core/components/services/detail/pane/tab-panels/properties.tsx` — properties + project links.
- `apps/web/core/components/services/detail/pane/tab-panels/relationships.tsx` — relationships wrapper.
- `apps/web/core/components/services/detail/pane/tab-panels/tickets.tsx` — tickets wrapper.
- `apps/web/core/hooks/services/use-service-detail-pane.ts` — URL state hook.

Modified:

- `apps/web/core/components/services/detail/sidebar.tsx` — add `layout="pane"`.
- `apps/web/core/components/services/detail/relationships.tsx` — add pane layout.
- `apps/web/core/components/services/detail/work-items.tsx` — add pane layout.
- `apps/web/core/components/services/detail/root.tsx` — main column + pane.
- `apps/web/core/components/services/detail/index.ts` — export pane.
- `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx` — header open button.

---

### Task 1: Pane state helpers

**Files:**

- Create: `apps/web/core/components/services/detail/pane/helpers.ts`
- Test: `apps/web/core/components/services/detail/pane/helpers.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/components/services/detail/pane/helpers.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import {
  DEFAULT_SERVICE_DETAIL_PANE_TAB,
  isServiceDetailPaneTab,
  resolveServiceDetailPaneTab,
  SERVICE_DETAIL_PANE_TAB_KEYS,
} from "./helpers";

describe("service detail pane helpers", () => {
  it("lists the approved tabs in order", () => {
    expect(SERVICE_DETAIL_PANE_TAB_KEYS).toEqual(["properties", "relationships", "tickets"]);
  });

  it("accepts valid tab keys", () => {
    expect(isServiceDetailPaneTab("properties")).toBe(true);
    expect(isServiceDetailPaneTab("relationships")).toBe(true);
    expect(isServiceDetailPaneTab("tickets")).toBe(true);
  });

  it("rejects missing, empty, and unknown values", () => {
    expect(isServiceDetailPaneTab(null)).toBe(false);
    expect(isServiceDetailPaneTab(undefined)).toBe(false);
    expect(isServiceDetailPaneTab("")).toBe(false);
    expect(isServiceDetailPaneTab("outline")).toBe(false);
  });

  it("resolves invalid values to closed", () => {
    expect(resolveServiceDetailPaneTab("outline")).toBeNull();
    expect(resolveServiceDetailPaneTab(null)).toBeNull();
    expect(resolveServiceDetailPaneTab(undefined)).toBeNull();
  });

  it("defaults to properties when opening", () => {
    expect(DEFAULT_SERVICE_DETAIL_PANE_TAB).toBe("properties");
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run from the repo root:

```bash
pnpm --filter=web exec vitest run core/components/services/detail/pane/helpers.test.ts
```

Expected: FAIL because `./helpers` cannot be resolved.

- [ ] **Step 3: Write the helper**

Create `apps/web/core/components/services/detail/pane/helpers.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceDetailPaneTab = "properties" | "relationships" | "tickets";

export const SERVICE_DETAIL_PANE_WIDTH = 320;

export const SERVICE_DETAIL_PANE_TABS_QUERY_PARAM = "paneTab";

export const SERVICE_DETAIL_PANE_TABS: {
  key: TServiceDetailPaneTab;
  i18n_label: string;
}[] = [
  { key: "properties", i18n_label: "common.properties" },
  { key: "relationships", i18n_label: "service.relationships.title" },
  { key: "tickets", i18n_label: "service.tabs.work_items" },
];

export const SERVICE_DETAIL_PANE_TAB_KEYS = SERVICE_DETAIL_PANE_TABS.map((tab) => tab.key);

export const DEFAULT_SERVICE_DETAIL_PANE_TAB: TServiceDetailPaneTab = "properties";

export const isServiceDetailPaneTab = (value: string | null | undefined): value is TServiceDetailPaneTab =>
  Boolean(value) && SERVICE_DETAIL_PANE_TAB_KEYS.includes(value as TServiceDetailPaneTab);

export const resolveServiceDetailPaneTab = (value: string | null | undefined): TServiceDetailPaneTab | null =>
  isServiceDetailPaneTab(value) ? value : null;
```

- [ ] **Step 4: Run the test and verify it passes**

```bash
pnpm --filter=web exec vitest run core/components/services/detail/pane/helpers.test.ts
```

Expected: `Test Files  1 passed (1)` and all five tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/detail/pane/helpers.ts apps/web/core/components/services/detail/pane/helpers.test.ts
git commit -m "feat(web): add service detail pane state helpers"
```

---

### Task 2: Pane URL state hook

**Files:**

- Create: `apps/web/core/hooks/services/use-service-detail-pane.ts`

- [ ] **Step 1: Implement the hook**

Create `apps/web/core/hooks/services/use-service-detail-pane.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback } from "react";
import { useSearchParams } from "next/navigation";
// plane imports
import {
  DEFAULT_SERVICE_DETAIL_PANE_TAB,
  resolveServiceDetailPaneTab,
  SERVICE_DETAIL_PANE_TABS_QUERY_PARAM,
} from "@/components/services/detail/pane/helpers";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useQueryParams } from "@/hooks/use-query-params";

export const useServiceDetailPane = () => {
  // router
  const router = useAppRouter();
  const searchParams = useSearchParams();
  const { updateQueryParams } = useQueryParams();
  // derived values
  const activeTab = resolveServiceDetailPaneTab(searchParams.get(SERVICE_DETAIL_PANE_TABS_QUERY_PARAM));

  const openPane = useCallback(() => {
    const updatedRoute = updateQueryParams({
      paramsToAdd: {
        [SERVICE_DETAIL_PANE_TABS_QUERY_PARAM]: DEFAULT_SERVICE_DETAIL_PANE_TAB,
      },
    });
    router.push(updatedRoute);
  }, [router, updateQueryParams]);

  const setActiveTab = useCallback(
    (value: string) => {
      const tab = resolveServiceDetailPaneTab(value);
      if (!tab) return;
      const updatedRoute = updateQueryParams({
        paramsToAdd: { [SERVICE_DETAIL_PANE_TABS_QUERY_PARAM]: tab },
      });
      router.push(updatedRoute);
    },
    [router, updateQueryParams]
  );

  const closePane = useCallback(() => {
    const updatedRoute = updateQueryParams({
      paramsToRemove: [SERVICE_DETAIL_PANE_TABS_QUERY_PARAM],
    });
    router.push(updatedRoute);
  }, [router, updateQueryParams]);

  return {
    activeTab: activeTab ?? DEFAULT_SERVICE_DETAIL_PANE_TAB,
    isOpen: activeTab !== null,
    openPane,
    setActiveTab,
    closePane,
  };
};
```

- [ ] **Step 2: Lint the new file**

```bash
pnpm --filter=web exec oxlint core/hooks/services/use-service-detail-pane.ts
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/hooks/services/use-service-detail-pane.ts
git commit -m "feat(web): add service detail pane URL state hook"
```

---

### Task 3: Pane layout for the properties sidebar

**Files:**

- Modify: `apps/web/core/components/services/detail/sidebar.tsx`

- [ ] **Step 1: Extend the layout prop**

Replace the existing `Props` type:

```ts
type Props = {
  serviceId: string;
  layout?: "sidebar" | "stacked";
};
```

with:

```ts
type Props = {
  serviceId: string;
  layout?: "sidebar" | "stacked" | "pane";
};
```

- [ ] **Step 2: Drop heading and padding for the pane layout**

Inside `ServiceDetailSidebar`, after `const health = getServiceHealth(serviceId);`, add:

```ts
const showHeading = layout !== "pane";
```

Then replace the current return block:

```tsx
  return (
    <div className={cn("w-full", layout === "sidebar" ? "px-6 md:h-full md:overflow-y-auto" : "px-0")}>
      <h5 className="mt-5 text-body-xs-medium">{t("common.properties")}</h5>
      <div className="mt-4 mb-2 space-y-2.5 truncate">
```

with:

```tsx
  return (
    <div
      className={cn("w-full", {
        "px-6 md:h-full md:overflow-y-auto": layout === "sidebar",
        "px-0": layout === "stacked" || layout === "pane",
      })}
    >
      {showHeading && <h5 className="mt-5 text-body-xs-medium">{t("common.properties")}</h5>}
      <div className={cn("mb-2 space-y-2.5 truncate", showHeading ? "mt-4" : "mt-0")}>
```

- [ ] **Step 3: Lint the file**

```bash
pnpm --filter=web exec oxlint core/components/services/detail/sidebar.tsx
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/detail/sidebar.tsx
git commit -m "feat(web): add pane layout to service properties sidebar"
```

---

### Task 4: Pane layout for relationships

**Files:**

- Modify: `apps/web/core/components/services/detail/relationships.tsx`

- [ ] **Step 1: Add the layout prop**

Replace:

```ts
type Props = {
  serviceId: string;
};
```

with:

```ts
type Props = {
  serviceId: string;
  layout?: "page" | "pane";
};
```

- [ ] **Step 2: Read the layout flag**

Replace:

```tsx
export const ServiceRelationships = observer(function ServiceRelationships(props: Props) {
  const { serviceId } = props;
```

with:

```tsx
export const ServiceRelationships = observer(function ServiceRelationships(props: Props) {
  const { serviceId, layout = "page" } = props;
  const isPane = layout === "pane";
```

- [ ] **Step 3: Make each relationship row compact and two-line in the pane**

Replace the row wrapper:

```tsx
      <div
        key={dependency.id}
        className="flex items-center justify-between gap-2 rounded-sm border border-subtle px-2 py-1.5"
      >
        <span className="flex min-w-0 items-center gap-1.5" title={name}>
```

with:

```tsx
      <div
        key={dependency.id}
        className={cn(
          "flex gap-2 rounded-sm border border-subtle px-2 py-1.5",
          isPane ? "flex-col" : "items-center justify-between"
        )}
      >
        <span className={cn("flex min-w-0 items-center gap-1.5", isPane && "flex-wrap")} title={name}>
```

Then replace the actions wrapper:

```tsx
        {!lite && (
          <span className="flex shrink-0 items-center gap-2">
```

with:

```tsx
        {!lite && (
          <span className={cn("flex shrink-0 items-center gap-2", isPane && "justify-between")}>
```

- [ ] **Step 4: Stack the add form and hide the redundant heading in the pane**

Replace:

```tsx
  return (
    <div className="flex flex-col gap-3">
      <h4 className="text-13 font-medium text-secondary">{t("service.relationships.title")}</h4>
      <div className="flex items-center gap-2">
        <CustomSelect
          value={selectedType}
          onChange={(value: TServiceDependencyType) => setSelectedType(value)}
          className="h-7.5 w-[160px] shrink-0"
```

with:

```tsx
  return (
    <div className="flex flex-col gap-3">
      {!isPane && <h4 className="text-13 font-medium text-secondary">{t("service.relationships.title")}</h4>}
      <div className={cn("flex items-center gap-2", isPane && "flex-col items-stretch")}>
        <CustomSelect
          value={selectedType}
          onChange={(value: TServiceDependencyType) => setSelectedType(value)}
          className={cn("h-7.5 shrink-0", isPane ? "w-full" : "w-[160px]")}
```

Then replace the service selector className:

```tsx
className = "h-7.5 min-w-0 grow basis-0";
```

with:

```tsx
          className={cn("h-7.5 min-w-0", isPane ? "w-full" : "grow basis-0")}
```

Then replace the Add button:

```tsx
<Button variant="secondary" size="sm" onClick={handleAdd} disabled={!canAdd} loading={isAdding}>
  {t("add")}
</Button>
```

with:

```tsx
<Button
  variant="secondary"
  size="sm"
  onClick={handleAdd}
  disabled={!canAdd}
  loading={isAdding}
  className={cn(isPane && "w-full")}
>
  {t("add")}
</Button>
```

- [ ] **Step 5: Lint the file**

```bash
pnpm --filter=web exec oxlint core/components/services/detail/relationships.tsx
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/detail/relationships.tsx
git commit -m "feat(web): add pane layout to service relationships"
```

---

### Task 5: Pane layout for tickets

**Files:**

- Modify: `apps/web/core/components/services/detail/work-items.tsx`

- [ ] **Step 1: Add imports and the layout prop**

Add after the existing plane imports:

```tsx
import { cn } from "@plane/utils";
```

Replace:

```ts
type Props = {
  serviceId: string;
};
```

with:

```ts
type Props = {
  serviceId: string;
  layout?: "page" | "pane";
};
```

- [ ] **Step 2: Read the layout flag**

Replace:

```tsx
export const ServiceWorkItems = observer(function ServiceWorkItems(props: Props) {
  const { serviceId } = props;
```

with:

```tsx
export const ServiceWorkItems = observer(function ServiceWorkItems(props: Props) {
  const { serviceId, layout = "page" } = props;
  const isPane = layout === "pane";
```

- [ ] **Step 3: Compact the header**

Replace:

```tsx
<div className="flex items-center justify-between gap-2">
  <h4 className="text-13 font-medium text-secondary">{t("service.tabs.work_items")}</h4>
  <Button variant="secondary" size="sm" onClick={() => setIsPickerOpen(true)} disabled={!slug || !pid || !workspaceId}>
    {t("service.detail.add_work_items")}
  </Button>
</div>
```

with:

```tsx
<div className={cn("flex items-center gap-2", isPane ? "justify-end" : "justify-between")}>
  {!isPane && <h4 className="text-13 font-medium text-secondary">{t("service.tabs.work_items")}</h4>}
  <Button
    variant="secondary"
    size="sm"
    onClick={() => setIsPickerOpen(true)}
    disabled={!slug || !pid || !workspaceId}
    className={cn(isPane && "w-full")}
  >
    {t("service.detail.add_work_items")}
  </Button>
</div>
```

- [ ] **Step 4: Compact ticket rows**

Replace:

```tsx
className = "flex items-center justify-between gap-2 rounded-md border border-subtle px-3 py-2";
```

with:

```tsx
              className={cn(
                "flex items-center justify-between gap-2 rounded-md border border-subtle",
                isPane ? "px-2 py-1.5" : "px-3 py-2"
              )}
```

- [ ] **Step 5: Lint the file**

```bash
pnpm --filter=web exec oxlint core/components/services/detail/work-items.tsx
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/detail/work-items.tsx
git commit -m "feat(web): add pane layout to service tickets"
```

---

### Task 6: Pane shell and tab panels

**Files:**

- Create: `apps/web/core/components/services/detail/pane/index.ts`
- Create: `apps/web/core/components/services/detail/pane/root.tsx`
- Create: `apps/web/core/components/services/detail/pane/tabs-list.tsx`
- Create: `apps/web/core/components/services/detail/pane/tab-panels/root.tsx`
- Create: `apps/web/core/components/services/detail/pane/tab-panels/properties.tsx`
- Create: `apps/web/core/components/services/detail/pane/tab-panels/relationships.tsx`
- Create: `apps/web/core/components/services/detail/pane/tab-panels/tickets.tsx`

- [ ] **Step 1: Create the barrel**

Create `apps/web/core/components/services/detail/pane/index.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./helpers";
export * from "./root";
```

- [ ] **Step 2: Create the pane root**

Create `apps/web/core/components/services/detail/pane/root.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ArrowRightCircle } from "lucide-react";
// plane imports
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { useTranslation } from "@plane/i18n";
import { Tabs } from "@plane/propel/tabs";
import { cn } from "@plane/utils";
// hooks
import { useServiceDetailPane } from "@/hooks/services/use-service-detail-pane";
// local imports
import { SERVICE_DETAIL_PANE_WIDTH } from "./helpers";
import { ServiceDetailPaneTabPanelsRoot } from "./tab-panels/root";
import { ServiceDetailPaneTabsList } from "./tabs-list";

type Props = {
  serviceId: string;
};

export function ServiceDetailPane(props: Props) {
  const { serviceId } = props;
  // plane hooks
  const { t } = useTranslation();
  // hooks
  const { activeTab, isOpen, closePane, setActiveTab } = useServiceDetailPane();

  return (
    <aside
      className={cn(
        "flex w-full shrink-0 flex-col border-t border-subtle bg-surface-1 pt-3.5 transition-all duration-300 ease-out md:h-full md:w-80 md:border-t-0 md:border-l",
        !isOpen && "hidden md:flex"
      )}
      style={{ marginRight: isOpen ? "0px" : `-${SERVICE_DETAIL_PANE_WIDTH}px` }}
    >
      <div className="mb-3.5 px-3.5">
        <Tooltip label={t("page_navigation_pane.close_button")}>
          <button
            type="button"
            className="grid size-3.5 place-items-center text-secondary transition-colors hover:text-primary"
            onClick={closePane}
            aria-label={t("page_navigation_pane.close_button")}
          >
            <ArrowRightCircle className="size-3.5" />
          </button>
        </Tooltip>
      </div>

      <div className="animate-slide-in-right flex min-h-0 flex-1 flex-col overflow-hidden">
        {isOpen ? (
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <ServiceDetailPaneTabsList />
            <ServiceDetailPaneTabPanelsRoot serviceId={serviceId} />
          </Tabs>
        ) : null}
      </div>
    </aside>
  );
}
```

- [ ] **Step 3: Create the tab triggers**

Create `apps/web/core/components/services/detail/pane/tabs-list.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { useTranslation } from "@plane/i18n";
import { Tabs } from "@plane/propel/tabs";
// local imports
import { SERVICE_DETAIL_PANE_TABS } from "./helpers";

export function ServiceDetailPaneTabsList() {
  // translation
  const { t } = useTranslation();

  return (
    <div className="mx-3.5">
      <Tabs.List>
        {SERVICE_DETAIL_PANE_TABS.map((tab) => (
          <Tabs.Trigger key={tab.key} value={tab.key} size="sm" className="min-w-0 px-1">
            {t(tab.i18n_label)}
          </Tabs.Trigger>
        ))}
        <Tabs.Indicator />
      </Tabs.List>
    </div>
  );
}
```

- [ ] **Step 4: Create the tab content switch**

Create `apps/web/core/components/services/detail/pane/tab-panels/root.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { Tabs } from "@plane/propel/tabs";
// local imports
import { SERVICE_DETAIL_PANE_TABS } from "../helpers";
import { ServicePropertiesTabPanel } from "./properties";
import { ServiceRelationshipsTabPanel } from "./relationships";
import { ServiceTicketsTabPanel } from "./tickets";

type Props = {
  serviceId: string;
};

export function ServiceDetailPaneTabPanelsRoot(props: Props) {
  const { serviceId } = props;

  return (
    <>
      {SERVICE_DETAIL_PANE_TABS.map((tab) => (
        <Tabs.Content key={tab.key} value={tab.key} className="flex-1 overflow-y-auto p-3.5">
          {tab.key === "properties" && <ServicePropertiesTabPanel serviceId={serviceId} />}
          {tab.key === "relationships" && <ServiceRelationshipsTabPanel serviceId={serviceId} />}
          {tab.key === "tickets" && <ServiceTicketsTabPanel serviceId={serviceId} />}
        </Tabs.Content>
      ))}
    </>
  );
}
```

- [ ] **Step 5: Create the properties panel**

Create `apps/web/core/components/services/detail/pane/tab-panels/properties.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// local imports
import { ServiceProjectLinks } from "../../project-links";
import { ServiceDetailSidebar } from "../../sidebar";

type Props = {
  serviceId: string;
};

export function ServicePropertiesTabPanel(props: Props) {
  const { serviceId } = props;

  return (
    <div className="flex flex-col gap-4">
      <ServiceDetailSidebar serviceId={serviceId} layout="pane" />
      <div className="border-t border-subtle pt-4">
        <ServiceProjectLinks serviceId={serviceId} />
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Create the relationships panel**

Create `apps/web/core/components/services/detail/pane/tab-panels/relationships.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// local imports
import { ServiceRelationships } from "../../relationships";

type Props = {
  serviceId: string;
};

export function ServiceRelationshipsTabPanel(props: Props) {
  const { serviceId } = props;

  return <ServiceRelationships serviceId={serviceId} layout="pane" />;
}
```

- [ ] **Step 7: Create the tickets panel**

Create `apps/web/core/components/services/detail/pane/tab-panels/tickets.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// local imports
import { ServiceWorkItems } from "../../work-items";

type Props = {
  serviceId: string;
};

export function ServiceTicketsTabPanel(props: Props) {
  const { serviceId } = props;

  return <ServiceWorkItems serviceId={serviceId} layout="pane" />;
}
```

- [ ] **Step 8: Typecheck the new pane**

```bash
pnpm --filter=web check:types
```

Expected: exit code 0. If it fails, fix the reported prop/types and rerun.

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/components/services/detail/pane
git commit -m "feat(web): add service detail tabbed pane shell"
```

---

### Task 7: Wire the detail page and header control

**Files:**

- Modify: `apps/web/core/components/services/detail/root.tsx`
- Modify: `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx`
- Modify: `apps/web/core/components/services/detail/index.ts`

- [ ] **Step 1: Replace the detail root layout**

Replace the imports:

```tsx
// local imports
import { ServiceLoadErrorState } from "../service-load-error-state";
import { ServiceDescription } from "./description";
import { ServiceRelationships } from "./relationships";
import { ServiceDetailSidebar } from "./sidebar";
import { ServiceProjectLinks } from "./project-links";
import { ServiceTitleInput } from "./title-input";
import { ServiceWorkItems } from "./work-items";
```

with:

```tsx
// local imports
import { ServiceLoadErrorState } from "../service-load-error-state";
import { ServiceDescription } from "./description";
import { ServiceDetailPane } from "./pane";
import { ServiceTitleInput } from "./title-input";
```

Then replace the final return block:

```tsx
return (
  <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
    <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
      <ServiceTitleInput serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
      <ServiceDescription serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
      <ServiceProjectLinks serviceId={serviceId} />
      <ServiceRelationships serviceId={serviceId} />
      <ServiceWorkItems serviceId={serviceId} />
    </div>
    <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:w-1/4 md:min-w-80 md:border-t-0 md:border-l xl:min-w-96">
      <ServiceDetailSidebar serviceId={serviceId} />
    </div>
  </div>
);
```

with:

```tsx
return (
  <div className="flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden">
    <div className="w-full space-y-6 px-9 py-5 md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto">
      <ServiceTitleInput serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
      <ServiceDescription serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
    </div>
    <ServiceDetailPane serviceId={serviceId} />
  </div>
);
```

- [ ] **Step 2: Add the header open button**

In `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx`, add imports:

```tsx
import { RightSidePaneOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
```

Add the hook import:

```tsx
import { useServiceDetailPane } from "@/hooks/services/use-service-detail-pane";
```

Inside `WorkspaceServicesBreadcrumbs`, after `const { getServiceById } = useService();`, add:

```tsx
const { isOpen, openPane } = useServiceDetailPane();
```

Then replace the closing of the breadcrumbs header:

```tsx
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
```

with:

```tsx
        </Breadcrumbs>
      </Header.LeftItem>
      <Header.RightItem>
        {serviceId && !isOpen && (
          <Tooltip label={t("page_navigation_pane.open_button")}>
            <button
              type="button"
              className="grid size-6 shrink-0 place-items-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-primary"
              onClick={openPane}
              aria-label={t("page_navigation_pane.open_button")}
            >
              <RightSidePaneOutline className="size-3.5" />
            </button>
          </Tooltip>
        )}
      </Header.RightItem>
    </Header>
```

- [ ] **Step 3: Export the pane barrel**

Add to `apps/web/core/components/services/detail/index.ts`:

```ts
export * from "./pane";
```

- [ ] **Step 4: Typecheck and lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/services/detail "app/(all)/[workspaceSlug]/(projects)/services/layout.tsx"
```

Expected: both exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/detail/root.tsx apps/web/core/components/services/detail/index.ts "apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx"
git commit -m "feat(web): wire service detail tabbed pane and header control"
```

---

### Task 8: Full verification

**Files:** none expected; fix any failures found.

- [ ] **Step 1: Run the pane unit test**

```bash
pnpm --filter=web exec vitest run core/components/services/detail/pane/helpers.test.ts
```

Expected: all tests pass.

- [ ] **Step 2: Run the full web test suite**

```bash
pnpm --filter=web test
```

Expected: exit code 0.

- [ ] **Step 3: Run typecheck**

```bash
pnpm --filter=web check:types
```

Expected: exit code 0.

- [ ] **Step 4: Run lint**

```bash
pnpm --filter=web check:lint
```

Expected: exit code 0.

- [ ] **Step 5: Run the web build**

```bash
pnpm --filter=web build
```

Expected: build completes successfully.

- [ ] **Step 6: Manual verification**

Start the dev server per the repo instructions and check:

1. Open a service detail page at `/:workspaceSlug/services/:serviceId`.
2. Confirm the main column only shows title and description.
3. Click the panel icon in the header; URL gains `?paneTab=properties`.
4. Switch to Relationships and Tickets; the URL updates and refresh keeps the tab.
5. Use browser back/forward; the pane tab follows history.
6. Close the pane; the query param is removed and the header icon returns.
7. Properties: edit status/criticality/type/owner/URLs; add/remove/transfer a project link.
8. Relationships: add a relationship, change its type, remove it.
9. Tickets: link a ticket through the modal and unlink one.
10. Resize below 768px; the pane appears full-width below the description and disappears when closed.
11. Open the services list, click a service to open the peek overview; confirm the old stacked layout is unchanged and has no tabs.

- [ ] **Step 7: Commit fixes only if verification required changes**

If a verification step required edits, stage the modified web files and commit:

```bash
git add -u apps/web
git commit -m "fix(web): address service detail pane verification issues"
```

If verification passes with no changes, do not create an empty commit.

---

## Self-Review Checklist

- Spec coverage: pane layout, 3 tabs, project links in Properties, URL state, 320px width, header control, mobile stacking, peek unchanged, no new i18n, no backend changes — all covered by Tasks 1–8.
- No placeholders: every file and code block is complete.
- Type consistency: `TServiceDetailPaneTab`, `SERVICE_DETAIL_PANE_TAB_KEYS`, `layout="pane"`, and `useServiceDetailPane` names are used consistently across tasks.
