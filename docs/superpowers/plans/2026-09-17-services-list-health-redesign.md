# Services List — Health-First Ops Board Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the project Services list page into a health-first operations board with mocked health, incidents and deploy recency, plus health-colored graph nodes.

**Architecture:** Health is derived deterministically on the client from `service.id` (no backend change) and stored in `ServicesStore.healthMap` behind a swappable `ServiceHealthService`. The old `list` + `grid` layouts collapse into a single `board` layout; `graph` stays. Board rows use a color rail, health pill, incident cell and deploy cell built from verified Plane semantic utilities.

**Tech Stack:** React 19 + React Router 7 (`apps/web`), MobX (`makeObservable` / `computedFn`), TypeScript strict, Tailwind v4 with `@makeplane/propel` semantic tokens, `@xyflow/react` + `dagre`, i18next-**icu** (`{count}` interpolation, not `{{count}}`).

**Spec:** `docs/superpowers/specs/2026-09-17-services-list-health-redesign-design.md`

## Verification note (read first)

`apps/web` has **no test runner** (see `apps/web/package.json` — no `vitest`/`jest`). Adding one is explicitly out of scope in the spec. Every task therefore verifies with:

- `pnpm --filter=web check:types` — runs `react-router typegen && tsc --noEmit`
- `pnpm --filter=@plane/types check:types` (Tasks 1–2)
- `pnpm --filter=web check:lint` and `check:format` (or `pnpm fix` to auto-fix)
- A final manual checklist (Task 15) and prod rebuild (Task 16)

There are no failing-test steps because there is no runner to run them in. Do not add a runner.

Tasks are ordered by dependency. Do not reorder.

## File structure

**Create**

- `apps/web/core/services/service-health.helpers.ts` — pure, deterministic health derivation + weights/ranks
- `apps/web/core/services/service-health.service.ts` — mock seam (swappable to HTTP later)
- `apps/web/core/components/services/health/health-config.ts` — health → utility-class map
- `apps/web/core/components/services/health/service-health-dot.tsx`
- `apps/web/core/components/services/health/service-health-pill.tsx`
- `apps/web/core/components/services/health/service-health-summary.tsx`
- `apps/web/core/components/services/health/service-incident-cell.tsx`
- `apps/web/core/components/services/health/service-deploy-cell.tsx`
- `apps/web/core/components/services/board/services-board-row.tsx`
- `apps/web/core/components/services/board/services-board.tsx`
- `apps/web/core/components/services/filters/health.tsx`
- `apps/web/core/components/services/filters/incidents.tsx`

**Modify**

- `packages/types/src/service/core.ts` — health types, `TServiceHealthSummary`, graph data health
- `packages/types/src/service/filters.ts` — layout/order/filter types
- `apps/web/core/services/service.helpers.ts` — health-aware filter + order
- `apps/web/core/store/service.store.ts` — `healthMap`, health fetch, summary, filtering
- `apps/web/core/store/service_filter.store.ts` — legacy layout coercion, default order
- `apps/web/core/components/services/services-list-view.tsx` — board | graph + states
- `apps/web/core/components/services/service-layout-icon.tsx` — board + graph icons
- `apps/web/core/components/services/filters/root.tsx` + `filters/index.ts` — mount new filters
- `apps/web/core/components/services/applied-filters/root.tsx` — health/incidents labels
- `apps/web/core/components/services/dropdowns/order-by.tsx` — `health` option
- `apps/web/core/components/services/graph/service-node.tsx` — health rail + badge
- `apps/web/core/components/services/graph/service-graph.tsx` — health into node data
- `apps/web/core/components/services/index.ts` — barrel
- `packages/i18n/src/locales/en/service.json` + other locales (via translate skill)

**Delete**

- `apps/web/core/components/services/service-list-item.tsx`
- `apps/web/core/components/services/service-card-item.tsx`

---

### Task 1: Service health types

**Files:**

- Modify: `packages/types/src/service/core.ts`

- [ ] **Step 1: Add the health types and extend `TServiceGraphData`**

Append to `packages/types/src/service/core.ts` (after `TServiceType`, before `TServicePosition`):

```ts
export type TServiceHealth = "healthy" | "degraded" | "down" | "unknown";

export type TServiceIncidentSeverity = "sev1" | "sev2" | "sev3" | "sev4";

export interface IServiceIncident {
  id: string;
  service_id: string;
  severity: TServiceIncidentSeverity;
  opened_at: string;
}

export interface IServiceHealthSnapshot {
  service_id: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
  last_deployed_at: string | null;
}

export type TServiceHealthSummary = {
  down: number;
  degraded: number;
  healthy: number;
  unknown: number;
  criticalImpacted: number;
};
```

Then replace the existing `TServiceGraphData` block at the bottom of the same file:

```ts
export type TServiceGraphData = {
  services: IService[];
  dependencies: IServiceDependency[];
  health: Record<string, IServiceHealthSnapshot>;
};
```

- [ ] **Step 2: Verify types compile**

Run: `pnpm --filter=@plane/types check:types`
Expected: PASS (exit 0). The web app will now fail typecheck until Task 5 adds `health` to `getGraphData` — that is expected and fixed in Task 5.

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/service/core.ts
git commit -m "feat(services): add health types"
```

---

### Task 2: Filter, layout and ordering types

**Files:**

- Modify: `packages/types/src/service/filters.ts`

- [ ] **Step 1: Replace the file body**

Replace the whole of `packages/types/src/service/filters.ts` with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceCriticality, TServiceHealth, TServiceStatus, TServiceType } from "./core";

export type TServiceLayoutOptions = "board" | "graph";

export type TServiceOrderByOptions = "health" | "name" | "-created_at" | "-updated_at" | "criticality" | "status";

export type TServiceIncidentFilter = "active";

export type TServiceFilters = {
  status?: TServiceStatus[];
  criticality?: TServiceCriticality[];
  type?: TServiceType[];
  health?: TServiceHealth[];
  incidents?: TServiceIncidentFilter[];
};

export type TServiceDisplayFilters = {
  layout: TServiceLayoutOptions;
  order_by: TServiceOrderByOptions;
};
```

- [ ] **Step 2: Verify types compile**

Run: `pnpm --filter=@plane/types check:types`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add packages/types/src/service/filters.ts
git commit -m "feat(services): health in filter and layout types"
```

---

### Task 3: Deterministic health helpers

**Files:**

- Create: `apps/web/core/services/service-health.helpers.ts`

- [ ] **Step 1: Create the helpers module**

Create `apps/web/core/services/service-health.helpers.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type {
  IService,
  IServiceHealthSnapshot,
  IServiceIncident,
  TServiceCriticality,
  TServiceHealth,
  TServiceIncidentSeverity,
} from "@plane/types";

const FNV_OFFSET_BASIS = 2166136261;
const FNV_PRIME = 16777619;
const DAY_MS = 24 * 60 * 60 * 1000;

/** FNV-1a 32-bit hash — small, dependency-free, stable across runs. */
export const fnv1aHash = (value: string): number => {
  let hash = FNV_OFFSET_BASIS;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, FNV_PRIME);
  }
  return hash >>> 0;
};

/** mulberry32 PRNG — deterministic sequence from a 32-bit seed. */
export const mulberry32 = (seed: number): (() => number) => {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

/** down/degraded probability ceilings, weighted by criticality. */
const HEALTH_THRESHOLDS: Record<TServiceCriticality, { down: number; degraded: number }> = {
  critical: { down: 0.12, degraded: 0.34 },
  high: { down: 0.1, degraded: 0.3 },
  medium: { down: 0.08, degraded: 0.26 },
  low: { down: 0.06, degraded: 0.22 },
};

export const HEALTH_WEIGHT: Record<TServiceHealth, number> = {
  down: 0,
  degraded: 1,
  unknown: 2,
  healthy: 3,
};

export const INCIDENT_SEVERITY_RANK: Record<TServiceIncidentSeverity, number> = {
  sev1: 0,
  sev2: 1,
  sev3: 2,
  sev4: 3,
};

const makeIncident = (
  serviceId: string,
  severity: TServiceIncidentSeverity,
  index: number,
  updatedAt: number,
  random: () => number
): IServiceIncident => ({
  id: `${serviceId}-incident-${index}`,
  service_id: serviceId,
  severity,
  opened_at: new Date(updatedAt - Math.floor(random() * 5 * DAY_MS)).toISOString(),
});

/**
 * Derives a stable health snapshot from a service. The PRNG is seeded from the
 * service id and consumed in a fixed order (health, incidents, deploy offset),
 * so the result is identical across reloads, list order and filter changes.
 */
export const buildHealthSnapshot = (service: IService): IServiceHealthSnapshot => {
  const random = mulberry32(fnv1aHash(service.id));
  const parsedUpdatedAt = new Date(service.updated_at).getTime();
  const updatedAt = Number.isNaN(parsedUpdatedAt) ? Date.now() : parsedUpdatedAt;

  // Lifecycle states with no runtime signal.
  if (service.status === "planned" || service.status === "retired") {
    return { service_id: service.id, health: "unknown", incidents: [], last_deployed_at: null };
  }

  const thresholds = HEALTH_THRESHOLDS[service.criticality] ?? HEALTH_THRESHOLDS.medium;
  const roll = random();
  const health: TServiceHealth = roll < thresholds.down ? "down" : roll < thresholds.degraded ? "degraded" : "healthy";

  const incidents: IServiceIncident[] = [];
  if (health === "down") {
    incidents.push(makeIncident(service.id, "sev1", 0, updatedAt, random));
  } else if (health === "degraded") {
    const count = random() < 0.5 ? 1 : 2;
    for (let index = 0; index < count; index += 1) {
      const severity: TServiceIncidentSeverity = index === 0 && random() < 0.7 ? "sev2" : "sev3";
      incidents.push(makeIncident(service.id, severity, index, updatedAt, random));
    }
  }

  const deployOffsetDays = Math.floor(random() * 15);
  const lastDeployedAt = new Date(updatedAt - deployOffsetDays * DAY_MS).toISOString();

  return { service_id: service.id, health, incidents, last_deployed_at: lastDeployedAt };
};

/** Highest-severity incident in a list (`sev1` is highest). */
export const getHighestSeverityIncident = (incidents: IServiceIncident[]): IServiceIncident | null =>
  incidents.reduce<IServiceIncident | null>(
    (highest, incident) =>
      !highest || INCIDENT_SEVERITY_RANK[incident.severity] < INCIDENT_SEVERITY_RANK[highest.severity]
        ? incident
        : highest,
    null
  );
```

- [ ] **Step 2: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: FAIL only with the pre-existing `getGraphData` return-type error from Task 1 (missing `health`). No errors in `service-health.helpers.ts`. If you see errors in the new file, fix them before continuing.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/service-health.helpers.ts
git commit -m "feat(services): deterministic health helpers"
```

---

### Task 4: Mock health service seam

**Files:**

- Create: `apps/web/core/services/service-health.service.ts`

- [ ] **Step 1: Create the service class**

Create `apps/web/core/services/service-health.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceHealthSnapshot } from "@plane/types";
// helpers
import { buildHealthSnapshot } from "@/services/service-health.helpers";

/**
 * Mock health source. The backend has no health data yet, so snapshots are
 * derived locally. This class is the single seam to replace with an HTTP call
 * (`GET /api/workspaces/:slug/projects/:projectId/services/health/`) later —
 * stores and components depend only on this signature.
 */
export class ServiceHealthService {
  async getHealth(
    _workspaceSlug: string,
    _workspaceId: string,
    _projectId: string,
    services: IService[]
  ): Promise<IServiceHealthSnapshot[]> {
    return services.map((service) => buildHealthSnapshot(service));
  }
}
```

- [ ] **Step 2: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: same single pre-existing `getGraphData` error, nothing new.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/service-health.service.ts
git commit -m "feat(services): mock service health seam"
```

---

### Task 5: Health-aware filtering and ordering helpers

**Files:**

- Modify: `apps/web/core/services/service.helpers.ts`

- [ ] **Step 1: Update imports**

In `apps/web/core/services/service.helpers.ts`, change the top import block from:

```ts
import type { TExtensions } from "@plane/editor";
import type { IService, IServiceDependency, TServiceFilters, TServiceOrderByOptions } from "@plane/types";
```

to:

```ts
import type { TExtensions } from "@plane/editor";
import type {
  IService,
  IServiceDependency,
  IServiceHealthSnapshot,
  TServiceFilters,
  TServiceOrderByOptions,
} from "@plane/types";
// helpers
import { HEALTH_WEIGHT } from "@/services/service-health.helpers";
```

- [ ] **Step 2: Replace `matchesFilters`, `filterServices` and `orderServices`**

Replace the existing `const CRITICALITY_WEIGHT` / `matchesFilters` / `filterServices` / `orderServices` block with:

```ts
const CRITICALITY_WEIGHT: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };

const matchesFilters = (
  service: IService,
  filters: TServiceFilters,
  health: IServiceHealthSnapshot | undefined
): boolean => {
  if (filters.status && filters.status.length > 0 && !filters.status.includes(service.status)) return false;
  if (filters.criticality && filters.criticality.length > 0 && !filters.criticality.includes(service.criticality))
    return false;
  if (filters.type && filters.type.length > 0 && !filters.type.includes(service.type)) return false;
  if (filters.health && filters.health.length > 0) {
    const state = health?.health ?? "unknown";
    if (!filters.health.includes(state)) return false;
  }
  if (filters.incidents && filters.incidents.length > 0 && filters.incidents.includes("active")) {
    if ((health?.incidents.length ?? 0) === 0) return false;
  }
  return true;
};

export const filterServices = (
  services: IService[],
  filters: TServiceFilters,
  searchQuery: string,
  healthMap: Record<string, IServiceHealthSnapshot> = {}
): IService[] =>
  services.filter(
    (service) =>
      service.name.toLowerCase().includes(searchQuery.toLowerCase()) &&
      matchesFilters(service, filters, healthMap[service.id])
  );

export const orderServices = (
  services: IService[],
  orderBy: TServiceOrderByOptions = "health",
  healthMap: Record<string, IServiceHealthSnapshot> = {}
): IService[] => {
  const ordered = [...services];
  // oxlint-disable-next-line unicorn/no-array-sort
  ordered.sort((a, b) => {
    switch (orderBy) {
      case "-created_at":
        return b.created_at.localeCompare(a.created_at);
      case "-updated_at":
        return b.updated_at.localeCompare(a.updated_at);
      case "criticality":
        return (CRITICALITY_WEIGHT[a.criticality] ?? 9) - (CRITICALITY_WEIGHT[b.criticality] ?? 9);
      case "status":
        return a.status.localeCompare(b.status);
      case "health": {
        const healthDiff =
          (HEALTH_WEIGHT[healthMap[a.id]?.health ?? "unknown"] ?? 2) -
          (HEALTH_WEIGHT[healthMap[b.id]?.health ?? "unknown"] ?? 2);
        if (healthDiff !== 0) return healthDiff;
        const criticalityDiff = (CRITICALITY_WEIGHT[a.criticality] ?? 9) - (CRITICALITY_WEIGHT[b.criticality] ?? 9);
        if (criticalityDiff !== 0) return criticalityDiff;
        return a.name.localeCompare(b.name);
      }
      case "name":
      default:
        return a.name.localeCompare(b.name);
    }
  });
  return ordered;
};
```

- [ ] **Step 3: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: same single pre-existing `getGraphData` error, nothing new.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/services/service.helpers.ts
git commit -m "feat(services): health-aware filter and ordering"
```

---

### Task 6: Store — health map, fetch, summary and filtering

**Files:**

- Modify: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Update imports**

Replace the type/services import block:

```ts
import type { IService, IServiceDependency, TServiceGraphData, TServiceWorkItemLink } from "@plane/types";
// helpers
import { filterServices, orderServices } from "@/services/service.helpers";
// services
import { ServiceService } from "@/services/service.service";
```

with:

```ts
import type {
  IService,
  IServiceDependency,
  IServiceHealthSnapshot,
  TServiceGraphData,
  TServiceHealthSummary,
  TServiceWorkItemLink,
} from "@plane/types";
// helpers
import { filterServices, orderServices } from "@/services/service.helpers";
// services
import { ServiceHealthService } from "@/services/service-health.service";
import { ServiceService } from "@/services/service.service";
```

- [ ] **Step 2: Extend `IServiceStore`**

In the `IServiceStore` interface, after `workItemLinkMap: Record<string, TServiceWorkItemLink>;` add:

```ts
healthMap: Record<string, IServiceHealthSnapshot>;
```

and after `getServiceById: (serviceId: string) => IService | null;` add:

```ts
getServiceHealth: (serviceId: string) => IServiceHealthSnapshot | null;
getProjectHealthSummary: (projectId: string) => TServiceHealthSummary;
```

- [ ] **Step 3: Add observable + service instance**

In `ServicesStore`, after `workItemLinkMap: Record<string, TServiceWorkItemLink> = {};` add:

```ts
healthMap: Record<string, IServiceHealthSnapshot> = {};
```

Change the two class fields:

```ts
rootStore;
serviceService;
```

to:

```ts
rootStore;
serviceService;
serviceHealthService;
```

In `makeObservable`, after `workItemLinkMap: observable,` add:

```ts
      healthMap: observable,
```

In the constructor after `this.serviceService = new ServiceService();` add:

```ts
this.serviceHealthService = new ServiceHealthService();
```

- [ ] **Step 4: Add health computed functions**

After `getServiceById = computedFn(...)` add:

```ts
getServiceHealth = computedFn((serviceId: string) => this.healthMap[serviceId] || null);

getProjectHealthSummary = computedFn((projectId: string): TServiceHealthSummary => {
  const summary: TServiceHealthSummary = { down: 0, degraded: 0, healthy: 0, unknown: 0, criticalImpacted: 0 };
  Object.values(this.serviceMap)
    .filter((service) => service.project_id === projectId)
    .forEach((service) => {
      const state = this.healthMap[service.id]?.health ?? "unknown";
      summary[state] += 1;
      if (state !== "healthy" && state !== "unknown" && service.criticality === "critical") {
        summary.criticalImpacted += 1;
      }
    });
  return summary;
});
```

- [ ] **Step 5: Thread health into filtering**

Replace `getFilteredServiceIds` with:

```ts
getFilteredServiceIds = computedFn((projectId: string) => {
  if (!this.fetchedMap[projectId]) return null;
  const displayFilters = this.rootStore.serviceFilter.getDisplayFiltersByProjectId(projectId);
  const filters = this.rootStore.serviceFilter.getFiltersByProjectId(projectId);
  const searchQuery = this.rootStore.serviceFilter.searchQuery;
  const services = Object.values(this.serviceMap).filter((s) => s.project_id === projectId);
  const filtered = filterServices(services, filters, searchQuery, this.healthMap);
  return orderServices(filtered, displayFilters?.order_by, this.healthMap).map((s) => s.id);
});
```

- [ ] **Step 6: Include health in graph data**

Replace `getGraphData` with:

```ts
getGraphData = computedFn((projectId: string): TServiceGraphData => {
  const serviceIds = this.getFilteredServiceIds(projectId) ?? [];
  const services = serviceIds
    .map((id) => this.serviceMap[id])
    .filter((service): service is IService => Boolean(service));
  const visible = new Set(serviceIds);
  const dependencies = this.getDependenciesByProject(projectId).filter(
    (d) => visible.has(d.from_service_id) && visible.has(d.to_service_id)
  );
  const health: Record<string, IServiceHealthSnapshot> = {};
  services.forEach((service) => {
    const snapshot = this.healthMap[service.id];
    if (snapshot) health[service.id] = snapshot;
  });
  return { services, dependencies, health };
});
```

- [ ] **Step 7: Fetch health alongside services**

Replace the body of `fetchServices` with:

```ts
fetchServices = async (workspaceSlug: string, workspaceId: string, projectId: string) => {
  try {
    this.loader = true;
    const [services, dependencies, links] = await Promise.all([
      this.serviceService.getServices(workspaceSlug, workspaceId, projectId),
      this.serviceService.getDependencies(workspaceSlug, workspaceId, projectId),
      this.serviceService.getWorkItemLinks(workspaceSlug, workspaceId, projectId),
    ]);
    runInAction(() => {
      services.forEach((s) => set(this.serviceMap, [s.id], { ...this.serviceMap[s.id], ...s }));
      dependencies.forEach((d) => set(this.dependencyMap, [d.id], d));
      links.forEach((l) => set(this.workItemLinkMap, [l.id], l));
      set(this.fetchedMap, projectId, true);
      this.loader = false;
    });
    try {
      const health = await this.serviceHealthService.getHealth(workspaceSlug, workspaceId, projectId, services);
      runInAction(() => {
        health.forEach((snapshot) => set(this.healthMap, [snapshot.service_id], snapshot));
      });
    } catch (error) {
      // Health is supplementary; the board falls back to "unknown" without it.
      console.error("Failed to derive service health", error);
    }
    return services;
  } catch {
    runInAction(() => {
      this.loader = false;
    });
    return undefined;
  }
};
```

Note: health is fetched after the loader clears so the list is not blocked on it.

- [ ] **Step 8: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS (exit 0) — the Task 1 `getGraphData` error is now resolved.

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/store/service.store.ts
git commit -m "feat(services): health map, summary and health-aware filtering in store"
```

---

### Task 7: Filter store — legacy layout coercion + default ordering

**Files:**

- Modify: `apps/web/core/store/service_filter.store.ts`

- [ ] **Step 1: Import the layout type**

Change:

```ts
import type { TServiceDisplayFilters, TServiceFilters } from "@plane/types";
```

to:

```ts
import type { TServiceDisplayFilters, TServiceFilters, TServiceLayoutOptions } from "@plane/types";
```

- [ ] **Step 2: Coerce legacy persisted layouts and default to health ordering**

Replace `initProjectServiceFilters` with:

```ts
initProjectServiceFilters = (projectId: string) => {
  const displayFilters = this.getDisplayFiltersByProjectId(projectId);
  // Legacy persisted layouts ("list" | "grid") collapse into the board.
  const persistedLayout = displayFilters?.layout as string | undefined;
  const layout: TServiceLayoutOptions = persistedLayout === "graph" ? "graph" : "board";
  runInAction(() => {
    this.displayFilters[projectId] = {
      layout,
      order_by: displayFilters?.order_by || "health",
    };
    this.filters[projectId] = this.filters[projectId] ?? {};
  });
  this.saveDisplayFiltersToLocalStorage();
  this.saveFiltersToLocalStorage();
};
```

- [ ] **Step 3: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/store/service_filter.store.ts
git commit -m "feat(services): default board layout and health ordering"
```

---

### Task 8: English i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/service.json`

- [ ] **Step 1: Add fields, replace layout, add health/incidents/order/summary/board keys**

In `packages/i18n/src/locales/en/service.json`, inside the `service` object:

Add to `fields` (keep existing keys):

```json
      "health": "Health",
      "incidents": "Incidents",
      "deploy": "Last deploy",
```

Replace the `layout` object:

```json
    "layout": {
      "board": "Board",
      "graph": "Graph"
    },
```

Add to `order_by`:

```json
      "health": "Health",
```

Add these sibling keys after `type_values`:

```json
    "health_values": {
      "healthy": "Healthy",
      "degraded": "Degraded",
      "down": "Down",
      "unknown": "Unknown"
    },
    "incident_severity": {
      "sev1": "SEV1",
      "sev2": "SEV2",
      "sev3": "SEV3",
      "sev4": "SEV4"
    },
    "incidents": {
      "active": "Has active incidents"
    },
    "summary": {
      "down": "{count} down",
      "degraded": "{count} degraded",
      "healthy": "{count} healthy",
      "critical_impacted": "{count} critical impacted"
    },
    "board": {
      "service": "Service",
      "health": "Health",
      "incidents": "Incidents",
      "deploy": "Last deploy",
      "owner": "Owner"
    },
```

**These use ICU `{count}` interpolation** (the app uses `i18next-icu`). Do NOT use `{{count}}`.

- [ ] **Step 2: Verify the JSON parses and types regenerate**

Run: `pnpm --filter=@plane/i18n check:types`
Expected: PASS (`generate:types` then `tsc --noEmit`).

- [ ] **Step 3: Commit**

```bash
git add packages/i18n/src/locales/en/service.json
git commit -m "feat(services): English i18n keys for health board"
```

---

### Task 9: Health primitives

**Files:**

- Create: `apps/web/core/components/services/health/health-config.ts`
- Create: `apps/web/core/components/services/health/service-health-dot.tsx`
- Create: `apps/web/core/components/services/health/service-health-pill.tsx`
- Create: `apps/web/core/components/services/health/service-health-summary.tsx`
- Create: `apps/web/core/components/services/health/service-incident-cell.tsx`
- Create: `apps/web/core/components/services/health/service-deploy-cell.tsx`

All the semantic utility classes below (`bg-danger-primary`, `bg-warning-subtle`, `text-success-primary`, `bg-layer-3`, `font-code`, `tabular-nums`, `text-on-color`) are verified to exist in the built CSS. Do not substitute `border-l-*` color utilities — the rail is an absolutely positioned element for this reason.

- [ ] **Step 1: Create the health config**

Create `apps/web/core/components/services/health/health-config.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceHealth } from "@plane/types";

export const DEFAULT_HEALTH: TServiceHealth = "unknown";

export const HEALTH_CONFIG: Record<TServiceHealth, { dot: string; rail: string; pill: string; label_key: string }> = {
  down: {
    dot: "bg-danger-primary",
    rail: "bg-danger-primary",
    pill: "bg-danger-subtle text-danger-primary",
    label_key: "service.health_values.down",
  },
  degraded: {
    dot: "bg-warning-primary",
    rail: "bg-warning-primary",
    pill: "bg-warning-subtle text-warning-primary",
    label_key: "service.health_values.degraded",
  },
  healthy: {
    dot: "bg-success-primary",
    rail: "bg-success-primary",
    pill: "bg-success-subtle text-success-primary",
    label_key: "service.health_values.healthy",
  },
  unknown: {
    dot: "bg-layer-3",
    rail: "bg-layer-3",
    pill: "bg-layer-2 text-tertiary",
    label_key: "service.health_values.unknown",
  },
};
```

- [ ] **Step 2: Create the dot**

Create `apps/web/core/components/services/health/service-health-dot.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "./health-config";

type Props = {
  health?: TServiceHealth | null;
  className?: string;
};

export function ServiceHealthDot({ health, className }: Props) {
  const state = health ?? DEFAULT_HEALTH;
  return (
    <span
      aria-hidden="true"
      className={cn("h-2 w-2 flex-shrink-0 rounded-full", HEALTH_CONFIG[state].dot, className)}
    />
  );
}
```

- [ ] **Step 3: Create the pill**

Create `apps/web/core/components/services/health/service-health-pill.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "./health-config";

type Props = {
  health?: TServiceHealth | null;
  className?: string;
};

export function ServiceHealthPill({ health, className }: Props) {
  const { t } = useTranslation();
  const state = health ?? DEFAULT_HEALTH;
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-11 font-medium",
        HEALTH_CONFIG[state].pill,
        className
      )}
    >
      {t(HEALTH_CONFIG[state].label_key)}
    </span>
  );
}
```

- [ ] **Step 4: Create the summary strip**

Create `apps/web/core/components/services/health/service-health-summary.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceHealthSummary } from "@plane/types";
import { cn } from "@plane/utils";

type Props = {
  summary: TServiceHealthSummary;
};

const SUMMARY_CHIPS: { key: keyof TServiceHealthSummary; className: string; label_key: string }[] = [
  { key: "down", className: "bg-danger-subtle text-danger-primary", label_key: "service.summary.down" },
  { key: "degraded", className: "bg-warning-subtle text-warning-primary", label_key: "service.summary.degraded" },
  { key: "healthy", className: "bg-success-subtle text-success-primary", label_key: "service.summary.healthy" },
];

export function ServiceHealthSummary({ summary }: Props) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-subtle px-3 py-2">
      {SUMMARY_CHIPS.map((chip) => (
        <span key={chip.key} className={cn("rounded-full px-2 py-0.5 text-11 font-medium", chip.className)}>
          {t(chip.label_key, { count: summary[chip.key] })}
        </span>
      ))}
      {summary.criticalImpacted > 0 && (
        <span className="rounded-full border border-subtle px-2 py-0.5 text-11 text-secondary">
          {t("service.summary.critical_impacted", { count: summary.criticalImpacted })}
        </span>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Create the incident cell**

Create `apps/web/core/components/services/health/service-incident-cell.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import type { IServiceIncident } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { getHighestSeverityIncident } from "@/services/service-health.helpers";

type Props = {
  incidents: IServiceIncident[];
};

export function ServiceIncidentCell({ incidents }: Props) {
  const { t } = useTranslation();
  const highest = getHighestSeverityIncident(incidents);
  if (incidents.length === 0 || !highest) {
    return <span className="text-12 text-tertiary">—</span>;
  }
  const isSevere = highest.severity === "sev1" || highest.severity === "sev2";
  return (
    <span
      className={cn(
        "flex items-center gap-1 text-12 font-medium",
        isSevere ? "text-danger-primary" : "text-warning-primary"
      )}
    >
      <WarningTriangleOutline className="h-3.5 w-3.5" />
      <span className="font-code tabular-nums">{incidents.length}</span>
      <span className="text-tertiary">·</span>
      <span>{t(`service.incident_severity.${highest.severity}`)}</span>
    </span>
  );
}
```

- [ ] **Step 6: Create the deploy cell**

Create `apps/web/core/components/services/health/service-deploy-cell.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ClockOutline } from "@makeplane/propel/icons";
import { calculateTimeAgo } from "@plane/utils";

type Props = {
  lastDeployedAt: string | null;
};

export function ServiceDeployCell({ lastDeployedAt }: Props) {
  if (!lastDeployedAt) return <span className="text-12 text-tertiary">—</span>;
  return (
    <span className="flex items-center gap-1 text-12 text-secondary">
      <ClockOutline className="h-3.5 w-3.5 text-tertiary" />
      <span className="font-code tabular-nums">{calculateTimeAgo(lastDeployedAt)}</span>
    </span>
  );
}
```

- [ ] **Step 7: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/web/core/components/services/health
git commit -m "feat(services): health dot, pill, summary, incident and deploy cells"
```

---

### Task 10: Board row and board

**Files:**

- Create: `apps/web/core/components/services/board/services-board-row.tsx`
- Create: `apps/web/core/components/services/board/services-board.tsx`

- [ ] **Step 1: Create the row**

Create `apps/web/core/components/services/board/services-board-row.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
// components
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceDeployCell } from "../health/service-deploy-cell";
import { ServiceHealthDot } from "../health/service-health-dot";
import { ServiceHealthPill } from "../health/service-health-pill";
import { ServiceIncidentCell } from "../health/service-incident-cell";

type Props = {
  serviceId: string;
};

export const ServicesBoardRow = observer(function ServicesBoardRow(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getServiceHealth } = useService();
  // derived values
  const service = getServiceById(serviceId);
  const health = getServiceHealth(serviceId);

  if (!service) return null;

  const state = health?.health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];
  const serviceLink = `/${workspaceSlug?.toString()}/projects/${service.project_id}/services/${service.id}`;
  const isCritical = service.criticality === "critical";

  return (
    <Link
      href={serviceLink}
      className="relative flex items-center gap-3 border-b border-subtle px-3 py-2.5 transition-colors hover:bg-layer-transparent-hover"
    >
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", config.rail)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <ServiceHealthDot health={state} />
        <span className="truncate text-13 font-medium text-primary">{service.name}</span>
        <span
          className={cn(
            "shrink-0 rounded-xs border px-1 text-10 font-medium uppercase tracking-wide",
            isCritical ? "border-danger-strong text-danger-primary" : "border-subtle text-tertiary"
          )}
        >
          {t(`service.criticality_values.${service.criticality}`)}
        </span>
      </div>
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
    </Link>
  );
});
```

- [ ] **Step 2: Create the board**

Create `apps/web/core/components/services/board/services-board.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { ServicesBoardRow } from "./services-board-row";

export const ServicesBoard = observer(function ServicesBoard() {
  // router
  const { projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getFilteredServiceIds } = useService();
  // derived values
  const serviceIds = projectId ? (getFilteredServiceIds(projectId.toString()) ?? []) : [];

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center gap-3 border-b border-subtle px-3 py-1.5 text-10 font-medium tracking-wide text-tertiary uppercase">
        <span className="flex-1">{t("service.board.service")}</span>
        <span className="hidden w-[120px] shrink-0 sm:block">{t("service.board.health")}</span>
        <span className="hidden w-[120px] shrink-0 md:block">{t("service.board.incidents")}</span>
        <span className="hidden w-[110px] shrink-0 lg:block">{t("service.board.deploy")}</span>
        <span className="hidden w-[56px] shrink-0 text-right xl:block">{t("service.board.owner")}</span>
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1">
        {serviceIds.map((id) => (
          <ServicesBoardRow key={id} serviceId={id} />
        ))}
      </div>
    </div>
  );
});
```

- [ ] **Step 3: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/board
git commit -m "feat(services): health board rows and board layout"
```

---

### Task 11: List view, layout icons, delete old components

**Files:**

- Modify: `apps/web/core/components/services/services-list-view.tsx`
- Modify: `apps/web/core/components/services/service-layout-icon.tsx`
- Modify: `apps/web/core/components/services/index.ts`
- Delete: `apps/web/core/components/services/service-list-item.tsx`
- Delete: `apps/web/core/components/services/service-card-item.tsx`

- [ ] **Step 1: Rewrite the list view**

Replace the whole of `apps/web/core/components/services/services-list-view.tsx` with:

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
// hooks
import { useService } from "@/hooks/store/use-service";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
// components
import { ServicesBoard } from "./board/services-board";
import { ServiceGraph } from "./graph/service-graph";
import { ServiceHealthSummary } from "./health/service-health-summary";
import { CreateUpdateServiceModal } from "./modal";

export const ServicesListView = observer(function ServicesListView() {
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getProjectServiceIds, getFilteredServiceIds, getProjectHealthSummary, loader } = useService();
  const { currentProjectDisplayFilters, clearAllFilters } = useServiceFilter();
  // states
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // derived values
  const layout = currentProjectDisplayFilters?.layout ?? "board";
  const projectServiceIds = projectId ? getProjectServiceIds(projectId.toString()) : null;
  const serviceIds = projectId ? getFilteredServiceIds(projectId.toString()) : null;
  const summary = projectId ? getProjectHealthSummary(projectId.toString()) : null;

  const openCreateModal = () => setIsCreateModalOpen(true);
  const closeCreateModal = () => setIsCreateModalOpen(false);

  const renderContent = () => {
    if (loader || projectServiceIds === null || serviceIds === null) {
      return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    }
    if (projectServiceIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.description")}</p>
          <Button variant="primary" size="sm" onClick={openCreateModal}>
            {t("service.add")}
          </Button>
        </div>
      );
    }
    if (serviceIds.length === 0) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6">
          <p className="text-sm font-medium text-primary">{t("service.empty_state.no_matches.title")}</p>
          <p className="text-xs text-secondary">{t("service.empty_state.no_matches.description")}</p>
          {projectId && (
            <Button variant="secondary" size="sm" onClick={() => clearAllFilters(projectId.toString())}>
              {t("common.clear_all")}
            </Button>
          )}
        </div>
      );
    }
    if (layout === "graph") {
      return (
        <div className="h-[calc(100vh-12rem)] w-full">
          <ServiceGraph />
        </div>
      );
    }
    return (
      <>
        {summary && <ServiceHealthSummary summary={summary} />}
        <ServicesBoard />
      </>
    );
  };

  return (
    <div className="flex h-full w-full flex-col">
      {renderContent()}
      {workspaceSlug && projectId && (
        <CreateUpdateServiceModal
          isOpen={isCreateModalOpen}
          onClose={closeCreateModal}
          workspaceSlug={workspaceSlug.toString()}
          projectId={projectId.toString()}
        />
      )}
    </div>
  );
});
```

- [ ] **Step 2: Update layout icons**

Replace the body of `apps/web/core/components/services/service-layout-icon.tsx`:

Change the import line:

```ts
import { GridOutline, ListOutline, WorkgraphOutline } from "@makeplane/propel/icons";
```

to:

```ts
import { BoardOutline, WorkgraphOutline } from "@makeplane/propel/icons";
```

Change `SERVICE_VIEW_LAYOUTS`:

```ts
export const SERVICE_VIEW_LAYOUTS: { key: TServiceLayoutOptions; i18n_label: string }[] = [
  { key: "board", i18n_label: "service.layout.board" },
  { key: "graph", i18n_label: "service.layout.graph" },
];
```

Change the icons map:

```ts
const icons = {
  board: BoardOutline,
  graph: WorkgraphOutline,
};
```

Leave the rest of the file unchanged. `service-view-header.tsx` and `service-mobile-header.tsx` need **no changes** — they both iterate `SERVICE_VIEW_LAYOUTS`.

- [ ] **Step 3: Update the barrel**

Replace the `service-list-item` and `service-card-item` exports in `apps/web/core/components/services/index.ts` with the new modules. The file becomes:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./services-list-view";
export * from "./service-view-header";
export * from "./service-mobile-header";
export * from "./service-layout-icon";
export * from "./dropdowns/order-by";
export * from "./service-form";
export * from "./modal";
export * from "./delete-service-modal";
export * from "./filters";
export * from "./applied-filters";
export * from "./search-input";
export * from "./graph/service-graph";
export * from "./graph/service-node";
export * from "./graph/use-graph-layout";
export * from "./detail";
export * from "./select";
export * from "./board/services-board";
export * from "./board/services-board-row";
export * from "./health/service-health-dot";
export * from "./health/service-health-pill";
export * from "./health/service-health-summary";
export * from "./health/service-incident-cell";
export * from "./health/service-deploy-cell";
```

- [ ] **Step 4: Delete the old components**

```bash
git rm apps/web/core/components/services/service-list-item.tsx apps/web/core/components/services/service-card-item.tsx
```

- [ ] **Step 5: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS. If anything still imports `ServiceListItem`/`ServiceCardItem`, add the import of `ServicesBoard`/`ServicesBoardRow` there instead — the only consumer was `services-list-view.tsx`.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services
git commit -m "feat(services): board/graph views, drop list and grid layouts"
```

---

### Task 12: Health and incidents filters

**Files:**

- Create: `apps/web/core/components/services/filters/health.tsx`
- Create: `apps/web/core/components/services/filters/incidents.tsx`
- Modify: `apps/web/core/components/services/filters/root.tsx`
- Modify: `apps/web/core/components/services/filters/index.ts`

- [ ] **Step 1: Create the health filter**

Create `apps/web/core/components/services/filters/health.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useTranslation } from "@plane/i18n";
import type { TServiceHealth } from "@plane/types";
// components
import { FilterHeader, FilterOption } from "@/components/issues/issue-layouts/filters";

export const SERVICE_HEALTH_OPTIONS: { value: TServiceHealth; i18n_label: string }[] = [
  { value: "down", i18n_label: "service.health_values.down" },
  { value: "degraded", i18n_label: "service.health_values.degraded" },
  { value: "healthy", i18n_label: "service.health_values.healthy" },
  { value: "unknown", i18n_label: "service.health_values.unknown" },
];

type Props = {
  appliedFilters: TServiceHealth[] | null;
  handleUpdate: (val: string) => void;
  searchQuery: string;
};

export const FilterServiceHealth = observer(function FilterServiceHealth(props: Props) {
  const { appliedFilters, handleUpdate, searchQuery } = props;
  // states
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const { t } = useTranslation();

  const filteredOptions = SERVICE_HEALTH_OPTIONS.filter((option) =>
    t(option.i18n_label).toLowerCase().includes(searchQuery.toLowerCase())
  );
  const appliedFiltersCount = appliedFilters?.length ?? 0;

  return (
    <>
      <FilterHeader
        title={`${t("service.fields.health")}${appliedFiltersCount > 0 ? ` (${appliedFiltersCount})` : ""}`}
        isPreviewEnabled={previewEnabled}
        handleIsPreviewEnabled={() => setPreviewEnabled(!previewEnabled)}
      />
      {previewEnabled && (
        <div>
          {filteredOptions.length > 0 ? (
            filteredOptions.map((option) => (
              <FilterOption
                key={option.value}
                isChecked={appliedFilters?.includes(option.value) ?? false}
                onClick={() => handleUpdate(option.value)}
                title={t(option.i18n_label)}
              />
            ))
          ) : (
            <p className="text-11 italic text-placeholder">{t("common.search.no_matches_found")}</p>
          )}
        </div>
      )}
    </>
  );
});
```

- [ ] **Step 2: Create the incidents filter**

Create `apps/web/core/components/services/filters/incidents.tsx`:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useTranslation } from "@plane/i18n";
import type { TServiceIncidentFilter } from "@plane/types";
// components
import { FilterHeader, FilterOption } from "@/components/issues/issue-layouts/filters";

export const SERVICE_INCIDENT_OPTIONS: { value: TServiceIncidentFilter; i18n_label: string }[] = [
  { value: "active", i18n_label: "service.incidents.active" },
];

type Props = {
  appliedFilters: TServiceIncidentFilter[] | null;
  handleUpdate: (val: string) => void;
  searchQuery: string;
};

export const FilterServiceIncidents = observer(function FilterServiceIncidents(props: Props) {
  const { appliedFilters, handleUpdate, searchQuery } = props;
  // states
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const { t } = useTranslation();

  const filteredOptions = SERVICE_INCIDENT_OPTIONS.filter((option) =>
    t(option.i18n_label).toLowerCase().includes(searchQuery.toLowerCase())
  );
  const appliedFiltersCount = appliedFilters?.length ?? 0;

  return (
    <>
      <FilterHeader
        title={`${t("service.fields.incidents")}${appliedFiltersCount > 0 ? ` (${appliedFiltersCount})` : ""}`}
        isPreviewEnabled={previewEnabled}
        handleIsPreviewEnabled={() => setPreviewEnabled(!previewEnabled)}
      />
      {previewEnabled && (
        <div>
          {filteredOptions.length > 0 ? (
            filteredOptions.map((option) => (
              <FilterOption
                key={option.value}
                isChecked={appliedFilters?.includes(option.value) ?? false}
                onClick={() => handleUpdate(option.value)}
                title={t(option.i18n_label)}
              />
            ))
          ) : (
            <p className="text-11 italic text-placeholder">{t("common.search.no_matches_found")}</p>
          )}
        </div>
      )}
    </>
  );
});
```

- [ ] **Step 3: Mount the filters**

In `apps/web/core/components/services/filters/root.tsx`, change the import:

```ts
import { FilterServiceCriticality, FilterServiceStatus, FilterServiceType } from "@/components/services";
```

to:

```ts
import {
  FilterServiceCriticality,
  FilterServiceHealth,
  FilterServiceIncidents,
  FilterServiceStatus,
  FilterServiceType,
} from "@/components/services";
```

Then, inside the scroll container, immediately after the `{/* status */}` block, insert:

```tsx
{
  /* health */
}
<div className="py-2">
  <FilterServiceHealth
    appliedFilters={filters.health ?? null}
    handleUpdate={(val) => handleFiltersUpdate("health", val)}
    searchQuery={filtersSearchQuery}
  />
</div>;

{
  /* incidents */
}
<div className="py-2">
  <FilterServiceIncidents
    appliedFilters={filters.incidents ?? null}
    handleUpdate={(val) => handleFiltersUpdate("incidents", val)}
    searchQuery={filtersSearchQuery}
  />
</div>;
```

- [ ] **Step 4: Update the filters barrel**

In `apps/web/core/components/services/filters/index.ts` add:

```ts
export * from "./health";
export * from "./incidents";
```

- [ ] **Step 5: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services/filters
git commit -m "feat(services): health and incidents filters"
```

---

### Task 13: Applied-filter labels and order-by option

**Files:**

- Modify: `apps/web/core/components/services/applied-filters/root.tsx`
- Modify: `apps/web/core/components/services/dropdowns/order-by.tsx`

- [ ] **Step 1: Add health/incidents value labels**

In `apps/web/core/components/services/applied-filters/root.tsx`, replace `SERVICE_FILTER_VALUE_I18N` with:

```ts
const SERVICE_FILTER_VALUE_I18N: Record<keyof TServiceFilters, (value: string) => string> = {
  status: (value) => `service.status_values.${value}`,
  criticality: (value) => `service.criticality_values.${value}`,
  type: (value) => `service.type_values.${value}`,
  health: (value) => `service.health_values.${value}`,
  incidents: (value) => `service.incidents.${value}`,
};
```

The component already renders the field label via `t(\`service.fields.${key}\`)`, and `service.fields.health`/`service.fields.incidents` exist from Task 8.

- [ ] **Step 2: Add the health order-by option**

In `apps/web/core/components/services/dropdowns/order-by.tsx`, replace `SERVICE_ORDER_BY_OPTIONS` with:

```ts
export const SERVICE_ORDER_BY_OPTIONS: { key: TServiceOrderByOptions; i18n_label: string }[] = [
  { key: "health", i18n_label: "service.order_by.health" },
  { key: "name", i18n_label: "service.order_by.name" },
  { key: "-created_at", i18n_label: "service.order_by.created" },
  { key: "-updated_at", i18n_label: "service.order_by.updated" },
  { key: "criticality", i18n_label: "service.order_by.criticality" },
  { key: "status", i18n_label: "service.order_by.status" },
];
```

- [ ] **Step 3: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/applied-filters/root.tsx apps/web/core/components/services/dropdowns/order-by.tsx
git commit -m "feat(services): health filter labels and health ordering option"
```

---

### Task 14: Health-colored graph nodes

**Files:**

- Modify: `apps/web/core/components/services/graph/service-node.tsx`
- Modify: `apps/web/core/components/services/graph/service-graph.tsx`

- [ ] **Step 1: Rewrite the node**

Replace the whole of `apps/web/core/components/services/graph/service-node.tsx` with:

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
// plane imports
import type { IService, IServiceIncident, TServiceHealth } from "@plane/types";
import { cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceHealthDot } from "../health/service-health-dot";

export type TServiceNodeData = {
  service: IService;
  statusLabel: string;
  criticalityLabel: string;
  health: TServiceHealth;
  incidents: IServiceIncident[];
};

export function ServiceNode({ data }: NodeProps<Node<TServiceNodeData>>) {
  const service = (data as TServiceNodeData | undefined)?.service;
  if (!service) return null;
  const { statusLabel, criticalityLabel, health, incidents } = data as TServiceNodeData;
  const state = health ?? DEFAULT_HEALTH;
  const config = HEALTH_CONFIG[state];

  return (
    <div className="shadow-sm relative w-[220px] overflow-hidden rounded-md border border-subtle bg-surface-1 px-3 py-2">
      <Handle type="target" position={Position.Left} className="!bg-surface-2" />
      <span aria-hidden="true" className={cn("absolute inset-y-0 left-0 w-[3px]", config.rail)} />
      {incidents.length > 0 && (
        <span className="absolute -top-1.5 -right-1.5 grid h-4 min-w-4 place-items-center rounded-full bg-danger-primary px-1 text-10 font-medium text-on-color">
          {incidents.length}
        </span>
      )}
      <div className="flex min-w-0 items-center gap-2 pl-1">
        <ServiceHealthDot health={state} />
        <span className="text-sm min-w-0 flex-1 truncate font-medium text-primary" title={service.name}>
          {service.name}
        </span>
      </div>
      <div className="text-xs mt-1 pl-1 text-secondary capitalize">
        {statusLabel} · {criticalityLabel}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-surface-2" />
    </div>
  );
}
```

- [ ] **Step 2: Feed health into node data**

In `apps/web/core/components/services/graph/service-graph.tsx`, add `TServiceGraphData` to the existing `@plane/types` import:

```ts
import type { IService, TServiceGraphData } from "@plane/types";
```

Then replace the `graphData` fallback line:

```ts
const graphData = pid ? getGraphData(pid) : { services: [], dependencies: [] };
```

with (the explicit annotation keeps `.health` indexable in the fallback branch):

```ts
const graphData: TServiceGraphData = pid ? getGraphData(pid) : { services: [], dependencies: [], health: {} };
```

Then replace the `translatedNodes` memo (and its eslint-disable comment) with:

```tsx
const translatedNodes = useMemo(
  () =>
    layoutNodes.map((node) => {
      const service = (node.data as { service?: IService } | undefined)?.service;
      if (!service) return node;
      const health = graphData.health[service.id];
      return {
        ...node,
        data: {
          ...(node.data as Record<string, unknown>),
          service,
          statusLabel: t(`service.status_values.${service.status}`),
          criticalityLabel: t(`service.criticality_values.${service.criticality}`),
          health: health?.health ?? "unknown",
          incidents: health?.incidents ?? [],
        },
      };
    }),
  // eslint-disable-next-line react-hooks/exhaustive-deps -- currentLocale re-runs labels on language change (t is re-created per render)
  [layoutNodes, currentLocale, graphData.health]
);
```

Then, in the `useEffect` that syncs nodes, extend the label comparison so health changes are not skipped. Replace:

```ts
const prevData = node.data as { statusLabel?: string; criticalityLabel?: string } | undefined;
const nextData = next?.data as { statusLabel?: string; criticalityLabel?: string } | undefined;
return prevData?.statusLabel === nextData?.statusLabel && prevData?.criticalityLabel === nextData?.criticalityLabel;
```

with:

```ts
const prevData = node.data as
  | { statusLabel?: string; criticalityLabel?: string; health?: string; incidents?: unknown[] }
  | undefined;
const nextData = next?.data as
  | { statusLabel?: string; criticalityLabel?: string; health?: string; incidents?: unknown[] }
  | undefined;
return (
  prevData?.statusLabel === nextData?.statusLabel &&
  prevData?.criticalityLabel === nextData?.criticalityLabel &&
  prevData?.health === nextData?.health &&
  (prevData?.incidents?.length ?? 0) === (nextData?.incidents?.length ?? 0)
);
```

- [ ] **Step 3: Verify types compile**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services/graph
git commit -m "feat(services): health-colored graph nodes"
```

---

### Task 15: Full check + manual verification

**Files:**

- No source changes (fix-ups only if a check fails).

- [ ] **Step 1: Run the full check**

Run: `pnpm check`
Expected: PASS for format, lint and types across packages. If `check:format` or `check:lint` fails, run `pnpm fix` and re-run `pnpm check`.

- [ ] **Step 2: Build the web app**

Run: `pnpm --filter=web build`
Expected: build succeeds.

- [ ] **Step 3: Restart the prod web service (per `AGENTS.md`)**

```bash
systemctl --user restart plane-web-prod.service
```

- [ ] **Step 4: Manual checklist (browser)**

Open the project Services page and verify:

- Board is the default view and the grid is gone; Graph still renders.
- Rails, dots and pills use the correct health color in **both light and dark** themes.
- Default sort is health-first (down → degraded → unknown → healthy), then criticality, then name; other order-by options still work.
- Health filter and "Has active incidents" filter apply; both appear in the applied-filters bar with readable labels; Clear all works.
- Summary strip counts match the seeded health values and are independent of filters.
- Deploy column shows relative time; services without a deploy show `—`.
- No-matches state shows a working **Clear filters** button; empty catalog shows **Add service**.
- A legacy persisted layout (`list` or `grid`) opens the Board without a broken state.
- Graph nodes show the health rail + incident badge; drag, connect, edge-delete and re-layout still work.

- [ ] **Step 5: Commit any fix-ups**

```bash
git add -A
git commit -m "fix(services): health board polish from manual verification"
```

Only commit if Step 4 required changes.

---

### Task 16: Translate new i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/*/service.json` (all locales with a `service.json`)

- [ ] **Step 1: Load the translate skill**

Use the **translate** skill before touching any locale file other than `en`. It defines do-not-translate terms, CLDR plural rules, placeholder/tag preservation, and the per-locale review workflow. The new keys are:

- `service.fields.{health,incidents,deploy}`
- `service.health_values.{healthy,degraded,down,unknown}`
- `service.incident_severity.{sev1,sev2,sev3,sev4}` — keep the literal `SEV1`…`SEV4` values; do not translate.
- `service.incidents.active`
- `service.summary.{down,degraded,healthy,critical_impacted}` — preserve the ICU `{count}` placeholder exactly.
- `service.board.{service,health,incidents,deploy,owner}`
- `service.order_by.health`
- `service.layout.board`; remove `service.layout.list` and `service.layout.grid`

- [ ] **Step 2: Apply the translations**

Follow the translate skill's workflow for every locale that has `service.json`. Do not machine-translate `SEV1`–`SEV4`.

- [ ] **Step 3: Verify i18n types/keys**

Run: `pnpm --filter=@plane/i18n check:types`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n/src/locales
git commit -m "i18n(services): translate health board keys"
```

---

## Self-review

- **Spec coverage:** health model → Tasks 1–6; filters/layout/order → Tasks 2, 5–7, 12–13; board layout → Tasks 9–11; graph → Task 14; i18n → Tasks 8, 16; testing/verification → Task 15; out-of-scope items are untouched.
- **Placeholders:** none — every code step contains complete code and every command has an expected result.
- **Type consistency:** `IServiceHealthSnapshot`, `TServiceHealth`, `TServiceHealthSummary`, `TServiceGraphData.health`, `HEALTH_WEIGHT`, `INCIDENT_SEVERITY_RANK`, `getHighestSeverityIncident`, `ServiceHealthService.getHealth`, `healthMap`, `getServiceHealth`, `getProjectHealthSummary`, `SERVICE_VIEW_LAYOUTS` keys (`board`/`graph`) and filter keys (`health`/`incidents`) are used with the same names in every task.
- **Known deviation from spec:** `service-view-header.tsx` and `service-mobile-header.tsx` are listed as "changed" in the spec but require no edits — both iterate `SERVICE_VIEW_LAYOUTS`, which Task 11 updates. Recorded here so a reviewer does not treat it as a missed task.
- **ICU:** summary strings use `{count}` (i18next-icu), not `{{count}}`.
