# Workspace Service Catalog Grouping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Group the workspace service catalog by owner application with sticky headers and compact service rows, while keeping the project-scoped list flat but compact.

**Architecture:** A pure helper builds sorted service groups from ids and store getters. The workspace list renders those groups with sticky headers; the shared service row gains compact styling plus `showProject`/`showOwner` flags. The project board keeps its flat structure with a compacted column header.

**Tech Stack:** React, MobX, React Router (`next/navigation` shim), Tailwind, Vitest, pnpm.

---

## File Structure

New:

- `apps/web/core/components/services/workspace/service-groups.ts` — pure grouping/sorting helper.
- `apps/web/core/components/services/workspace/service-groups.test.ts` — unit tests.

Modified:

- `apps/web/core/components/services/board/services-board-row.tsx` — compact row + visibility flags.
- `apps/web/core/components/services/board/services-board.tsx` — compact column header.
- `apps/web/core/components/services/workspace/workspace-services-list-view.tsx` — grouped rendering.

---

### Task 1: Grouping helper

**Files:**

- Create: `apps/web/core/components/services/workspace/service-groups.ts`
- Test: `apps/web/core/components/services/workspace/service-groups.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/core/components/services/workspace/service-groups.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
// local imports
import { buildWorkspaceServiceGroups } from "./service-groups";

const services: Record<string, { name: string; owner_project_id: string | null }> = {
  "s-1": { name: "Payment Gateway", owner_project_id: "p-2" },
  "s-2": { name: "Billing API", owner_project_id: "p-2" },
  "s-3": { name: "Customer Portal", owner_project_id: "p-1" },
  "s-4": { name: "Legacy Worker", owner_project_id: null },
  "s-5": { name: "Orphan", owner_project_id: "p-missing" },
  "s-6": { name: "Auth", owner_project_id: "p-3" },
};

const projects: Record<string, { name: string; identifier: string }> = {
  "p-1": { name: "Customer", identifier: "CRM" },
  "p-2": { name: "Payments", identifier: "PAY" },
  "p-3": { name: "Identity", identifier: "IAM" },
};

const getServiceById = (id: string) => services[id];
const getProjectById = (id: string) => projects[id];

describe("buildWorkspaceServiceGroups", () => {
  it("groups services by owner project and sorts groups by name", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-2", "s-3", "s-6"], getServiceById, getProjectById);

    expect(groups.map((group) => group.key)).toEqual(["p-1", "p-3", "p-2"]);
    expect(groups.map((group) => group.name)).toEqual(["Customer", "Identity", "Payments"]);
  });

  it("sorts services alphabetically inside a group", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-2"], getServiceById, getProjectById);

    expect(groups).toHaveLength(1);
    expect(groups[0].serviceIds).toEqual(["s-2", "s-1"]);
  });

  it("places unassigned and unresolved owners in a final unassigned group", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-4", "s-5", "s-3"], getServiceById, getProjectById);

    const lastGroup = groups[groups.length - 1];
    expect(lastGroup.key).toBe("unassigned");
    expect(lastGroup.projectId).toBeNull();
    expect(lastGroup.identifier).toBeNull();
    expect(lastGroup.serviceIds).toEqual(["s-4", "s-5"]);
  });

  it("deduplicates ids and skips services that cannot be resolved", () => {
    const groups = buildWorkspaceServiceGroups(["s-1", "s-1", "s-missing", "s-2"], getServiceById, getProjectById);

    expect(groups).toHaveLength(1);
    expect(groups[0].serviceIds).toEqual(["s-2", "s-1"]);
  });

  it("omits empty groups", () => {
    const groups = buildWorkspaceServiceGroups(["s-3"], getServiceById, getProjectById);

    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("p-1");
  });
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run from the repo root:

```bash
pnpm --filter=web exec vitest run core/components/services/workspace/service-groups.test.ts
```

Expected: FAIL because `./service-groups` cannot be resolved.

- [ ] **Step 3: Write the helper**

Create `apps/web/core/components/services/workspace/service-groups.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceGroup = {
  key: string;
  projectId: string | null;
  identifier: string | null;
  name: string;
  serviceIds: string[];
};

type TServiceLike = {
  name: string;
  owner_project_id: string | null;
};

type TProjectLike = {
  name: string;
  identifier: string;
};

const UNASSIGNED_GROUP_KEY = "unassigned";

const compareNames = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

export function buildWorkspaceServiceGroups(
  serviceIds: string[],
  getServiceById: (id: string) => TServiceLike | undefined,
  getProjectById: (id: string) => TProjectLike | undefined
): TServiceGroup[] {
  const groups = new Map<string, TServiceGroup>();
  const seenServiceIds = new Set<string>();

  for (const serviceId of serviceIds) {
    if (seenServiceIds.has(serviceId)) continue;
    seenServiceIds.add(serviceId);

    const service = getServiceById(serviceId);
    if (!service) continue;

    const ownerProjectId = service.owner_project_id;
    const project = ownerProjectId ? getProjectById(ownerProjectId) : undefined;
    const projectId = ownerProjectId && project ? ownerProjectId : null;
    const key = projectId ?? UNASSIGNED_GROUP_KEY;
    const existingGroup = groups.get(key);

    if (existingGroup) {
      existingGroup.serviceIds.push(serviceId);
      continue;
    }

    groups.set(key, {
      key,
      projectId,
      identifier: project?.identifier ?? null,
      name: project?.name ?? "",
      serviceIds: [serviceId],
    });
  }

  return [...groups.values()]
    .map((group) => ({
      ...group,
      serviceIds: [...group.serviceIds].sort((a, b) => {
        const nameA = getServiceById(a)?.name ?? "";
        const nameB = getServiceById(b)?.name ?? "";
        return compareNames(nameA, nameB) || a.localeCompare(b);
      }),
    }))
    .sort((a, b) => {
      const aUnassigned = a.projectId === null;
      const bUnassigned = b.projectId === null;
      if (aUnassigned !== bUnassigned) return aUnassigned ? 1 : -1;
      return compareNames(a.name, b.name) || a.key.localeCompare(b.key);
    });
}
```

- [ ] **Step 4: Run the test and verify it passes**

```bash
pnpm --filter=web exec vitest run core/components/services/workspace/service-groups.test.ts
```

Expected: `Test Files  1 passed (1)` and all five tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/workspace/service-groups.ts apps/web/core/components/services/workspace/service-groups.test.ts
git commit -m "feat(web): add workspace service grouping helper"
```

---

### Task 2: Compact service row

**Files:**

- Modify: `apps/web/core/components/services/board/services-board-row.tsx`

- [ ] **Step 1: Add row visibility props and compact styling**

Replace the current props:

```ts
type Props = {
  serviceId: string;
};
```

with:

```ts
type Props = {
  serviceId: string;
  showProject?: boolean;
  showOwner?: boolean;
};
```

Replace:

```tsx
export const ServicesBoardRow = observer(function ServicesBoardRow(props: Props) {
  const { serviceId } = props;
```

with:

```tsx
export const ServicesBoardRow = observer(function ServicesBoardRow(props: Props) {
  const { serviceId, showProject = true, showOwner = true } = props;
```

- [ ] **Step 2: Make owner lookup conditional and compact the row**

Replace:

```tsx
const ownerProject = service.owner_project_id ? getProjectById(service.owner_project_id) : undefined;
const isCritical = service.criticality === "critical";
```

with:

```tsx
const ownerProject = showProject && service.owner_project_id ? getProjectById(service.owner_project_id) : undefined;
const isCritical = service.criticality === "critical";
```

Replace the row className:

```tsx
className =
  "relative flex items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover";
```

with:

```tsx
className =
  "relative flex items-center gap-3 border-b border-subtle px-3 py-1.5 transition-colors hover:bg-layer-transparent-hover";
```

Replace the name:

```tsx
<span className="truncate text-13 font-medium text-primary">{service.name}</span>
```

with:

```tsx
<span className="truncate text-12 font-medium text-primary">{service.name}</span>
```

- [ ] **Step 3: Shrink badges and show criticality only when critical**

Replace:

```tsx
{
  ownerProject && (
    <span
      className="shrink-0 rounded-xs border border-subtle px-1 text-10 font-medium tracking-wide text-tertiary"
      title={ownerProject.name}
    >
      {ownerProject.identifier}
    </span>
  );
}
<span
  className={cn(
    "shrink-0 rounded-xs border px-1 text-10 font-medium tracking-wide uppercase",
    isCritical ? "border-danger-strong text-danger-primary" : "border-subtle text-tertiary"
  )}
>
  {t(`service.criticality_values.${service.criticality}`)}
</span>;
```

with:

```tsx
{
  ownerProject && (
    <span
      className="shrink-0 rounded-xs border border-subtle px-1 text-9 font-medium tracking-wide text-tertiary"
      title={ownerProject.name}
    >
      {ownerProject.identifier}
    </span>
  );
}
{
  isCritical && (
    <span className="shrink-0 rounded-xs border border-danger-strong px-1 text-9 font-medium tracking-wide text-danger-primary uppercase">
      {t(`service.criticality_values.${service.criticality}`)}
    </span>
  );
}
```

- [ ] **Step 4: Align columns and hide the owner column when requested**

Replace:

```tsx
      <div className="hidden w-[120px] shrink-0 sm:block">
        <ServiceHealthPill health={state} />
      </div>
      <div className="hidden w-[120px] shrink-0 md:block">
        <ServiceIncidentCell incidents={health?.incidents ?? []} />
      </div>
      <div className="hidden w-[110px] shrink-0 lg:block">
        <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
      </div>
      <div className="hidden w-[56px] shrink-0 items-center justify-end xl:flex">
        {service.owner_id && <ButtonAvatars showTooltip userIds={service.owner_id} />}
      </div>
```

with:

```tsx
      <div className="hidden w-[90px] shrink-0 sm:block">
        <ServiceHealthPill health={state} />
      </div>
      <div className="hidden w-[100px] shrink-0 md:block">
        <ServiceIncidentCell incidents={health?.incidents ?? []} />
      </div>
      <div className="hidden w-[90px] shrink-0 lg:block">
        <ServiceDeployCell lastDeployedAt={health?.last_deployed_at ?? null} />
      </div>
      {showOwner && (
        <div className="hidden w-[40px] shrink-0 items-center justify-end xl:flex">
          {service.owner_id && <ButtonAvatars showTooltip userIds={service.owner_id} />}
        </div>
      )}
```

- [ ] **Step 5: Lint the file**

```bash
pnpm --filter=web exec oxlint core/components/services/board/services-board-row.tsx
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/board/services-board-row.tsx
git commit -m "feat(web): compact shared service row layout"
```

---

### Task 3: Compact project board header

**Files:**

- Modify: `apps/web/core/components/services/board/services-board.tsx`

- [ ] **Step 1: Match header density and column widths to the row**

Replace:

```tsx
<div className="flex items-center gap-3 border-b border-subtle px-3 py-1.5 text-10 font-medium tracking-wide text-tertiary uppercase">
  <span className="flex-1">{t("service.board.service")}</span>
  <span className="hidden w-[120px] shrink-0 sm:block">{t("service.board.health")}</span>
  <span className="hidden w-[120px] shrink-0 md:block">{t("service.board.incidents")}</span>
  <span className="hidden w-[110px] shrink-0 lg:block">{t("service.board.deploy")}</span>
  <span className="hidden w-[56px] shrink-0 text-right xl:block">{t("service.board.owner")}</span>
</div>
```

with:

```tsx
<div className="flex items-center gap-3 border-b border-subtle px-3 py-1 text-9 font-medium tracking-wide text-tertiary uppercase">
  <span className="flex-1">{t("service.board.service")}</span>
  <span className="hidden w-[90px] shrink-0 sm:block">{t("service.board.health")}</span>
  <span className="hidden w-[100px] shrink-0 md:block">{t("service.board.incidents")}</span>
  <span className="hidden w-[90px] shrink-0 lg:block">{t("service.board.deploy")}</span>
  <span className="hidden w-[40px] shrink-0 text-right xl:block">{t("service.board.owner")}</span>
</div>
```

- [ ] **Step 2: Lint the file**

```bash
pnpm --filter=web exec oxlint core/components/services/board/services-board.tsx
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/services/board/services-board.tsx
git commit -m "feat(web): compact project service board header"
```

---

### Task 4: Group the workspace catalog

**Files:**

- Modify: `apps/web/core/components/services/workspace/workspace-services-list-view.tsx`

- [ ] **Step 1: Import the grouping helper**

Add after the existing service component imports:

```tsx
import { buildWorkspaceServiceGroups } from "@/components/services/workspace/service-groups";
```

- [ ] **Step 2: Build groups from the filtered ids**

After the `filteredIds` memo, add:

```ts
const groups = buildWorkspaceServiceGroups(filteredIds, getServiceById, getProjectById);
```

- [ ] **Step 3: Render sticky group headers and grouped rows**

Replace:

```tsx
<div className="min-h-0 flex-1 overflow-y-auto">
  {filteredIds.map((id) => (
    <ServicesBoardRow key={id} serviceId={id} />
  ))}
</div>
```

with:

```tsx
<div className="min-h-0 flex-1 overflow-y-auto">
  {groups.map((group) => (
    <section key={group.key}>
      <div className="sticky top-0 z-[5] flex items-center justify-between gap-2 border-b border-subtle bg-layer-2 px-3 py-1">
        <div className="flex min-w-0 items-center gap-1.5">
          {group.identifier && (
            <span className="shrink-0 rounded-xs border border-subtle px-1 text-9 font-medium tracking-wide text-tertiary">
              {group.identifier}
            </span>
          )}
          <span className="truncate text-11 font-medium text-secondary">
            {group.projectId ? group.name : t("common.unassigned")}
          </span>
        </div>
        <span className="shrink-0 rounded-xs bg-layer-3 px-1.5 text-9 text-tertiary tabular-nums">
          {group.serviceIds.length}
        </span>
      </div>
      {group.serviceIds.map((id) => (
        <ServicesBoardRow key={id} serviceId={id} showProject={false} showOwner={false} />
      ))}
    </section>
  ))}
</div>
```

- [ ] **Step 4: Typecheck and lint**

```bash
pnpm --filter=web check:types
pnpm --filter=web exec oxlint core/components/services/workspace
```

Expected: both exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services/workspace/workspace-services-list-view.tsx
git commit -m "feat(web): group workspace service catalog by application"
```

---

### Task 5: Full verification

**Files:** none expected; fix any failures found.

- [ ] **Step 1: Run the grouping unit test**

```bash
pnpm --filter=web exec vitest run core/components/services/workspace/service-groups.test.ts
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

Open `/:workspaceSlug/services` and a project services board:

1. Workspace catalog shows groups by application, A–Z, with Unassigned last.
2. Group headers stay visible while scrolling inside a group.
3. Group count matches visible services after search/filter.
4. Rows are compact; grouped rows do not repeat the project badge or owner avatar.
5. Criticality badge appears only on critical services.
6. Project filter shows a single group (or Unassigned).
7. Search still filters and groups update.
8. Clicking a row opens the peek as before.
9. The project-scoped services board is still flat and uses the compact header/rows.

- [ ] **Step 7: Commit fixes only if verification required changes**

If a verification step required edits, stage the modified web files and commit:

```bash
git add -u apps/web
git commit -m "fix(web): address service catalog grouping verification issues"
```

If verification passes with no changes, do not create an empty commit.

---

## Self-Review Checklist

- Spec coverage: grouping by owner project, Unassigned last, A–Z group and service ordering, sticky headers, compact rows, hidden project/owner inside groups, critical-only badge, project list flat but compact, no i18n/API changes — all covered by Tasks 1–5.
- No placeholders: every file and code block is complete.
- Type consistency: `TServiceGroup`, `buildWorkspaceServiceGroups`, `showProject`, and `showOwner` are used consistently across tasks.
