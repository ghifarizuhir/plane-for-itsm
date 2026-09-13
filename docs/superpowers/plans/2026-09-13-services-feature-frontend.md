# Services Feature (Frontend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a project-level **Services** feature (service catalog with a directed `depends-on` dependency graph) to the web app, backed by a localStorage mock until the backend exists.

**Architecture:** Mirror the Module feature: types in `packages/types`, an async service layer backed by a localStorage mock repository, MobX stores in `apps/web/core/store`, hooks, route group `services/(list|detail)`, sidebar/tab navigation, and React Flow + dagre for the graph view.

**Tech Stack:** TypeScript, React 19, React Router 8, MobX + mobx-utils, `@plane/types`, `@plane/ui`, `@makeplane/propel/icons`, `@xyflow/react` (v12), `dagre`.

**Spec:** `docs/superpowers/specs/2026-09-13-services-feature-design.md`

**Testing note:** `apps/web` has no test runner. Pure logic (dependency DAG rules, ordering/filter helpers) is verified with a throwaway Node script using `--experimental-strip-types`. Everything else is verified with `check:types`, `check:lint`, and a manual browser checklist.

---

## Phase 0 — Dependencies & Types

### Task 1: Add graph dependencies to the catalog

**Files:**

- Modify: `pnpm-workspace.yaml` (catalog section)
- Modify: `apps/web/package.json` (dependencies + devDependencies)

- [ ] **Step 1: Add to the catalog**

In `pnpm-workspace.yaml` under `catalog:`, add (keep alphabetical grouping):

```yaml
"@xyflow/react": "^12.8.6"
"dagre": "^0.8.5"
"@types/dagre": "^0.7.53"
```

- [ ] **Step 2: Add to the web app**

In `apps/web/package.json`, add to `dependencies`:

```json
    "@xyflow/react": "catalog:",
    "dagre": "catalog:",
```

and to `devDependencies`:

```json
    "@types/dagre": "catalog:",
```

- [ ] **Step 3: Install**

Run: `pnpm install`
Expected: lockfile updates, no peer errors.

- [ ] **Step 4: Commit**

```bash
git add pnpm-workspace.yaml pnpm-lock.yaml apps/web/package.json
git commit -m "chore(services): add @xyflow/react and dagre deps"
```

### Task 2: Service types

**Files:**

- Create: `packages/types/src/service/core.ts`
- Create: `packages/types/src/service/filters.ts`
- Create: `packages/types/src/service/index.ts`
- Modify: `packages/types/src/index.ts` (add export next to `export * from "./module";`)

- [ ] **Step 1: Create `core.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TServiceStatus = "active" | "planned" | "maintenance" | "deprecated" | "retired";

export type TServiceCriticality = "critical" | "high" | "medium" | "low";

export type TServiceType = "internal" | "external" | "infrastructure" | "third_party";

export type TServicePosition = {
  x: number;
  y: number;
};

export interface IService {
  id: string;
  workspace_id: string;
  project_id: string;
  name: string;
  description: string;
  description_html: string;
  status: TServiceStatus;
  criticality: TServiceCriticality;
  type: TServiceType;
  owner_id: string | null;
  repository_url: string | null;
  documentation_url: string | null;
  position: TServicePosition | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
  created_by: string | null;
  updated_by: string | null;
}

export interface IServiceDependency {
  id: string;
  workspace_id: string;
  project_id: string;
  from_service_id: string;
  to_service_id: string;
  created_at: string;
}

export interface TServiceWorkItemLink {
  id: string;
  service_id: string;
  issue_id: string;
  project_id: string;
  workspace_id: string;
  issue_identifier?: string;
  issue_name?: string;
}

export type TServiceGraphData = {
  services: IService[];
  dependencies: IServiceDependency[];
};
```

- [ ] **Step 2: Create `filters.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { TServiceCriticality, TServiceStatus, TServiceType } from "./core";

export type TServiceLayoutOptions = "list" | "grid" | "graph";

export type TServiceOrderByOptions = "name" | "-created_at" | "-updated_at" | "criticality" | "status";

export type TServiceFilters = {
  status?: TServiceStatus[];
  criticality?: TServiceCriticality[];
  type?: TServiceType[];
};

export type TServiceDisplayFilters = {
  layout: TServiceLayoutOptions;
  order_by: TServiceOrderByOptions;
};
```

- [ ] **Step 3: Create `index.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export * from "./core";
export * from "./filters";
```

- [ ] **Step 4: Export from the package root**

In `packages/types/src/index.ts`, add next to the existing `export * from "./module";`:

```ts
export * from "./service";
```

- [ ] **Step 5: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/types/src/service packages/types/src/index.ts
git commit -m "feat(services): add service domain types"
```

---

## Phase 1 — Pure Helpers, Mock Repository, Service Layer

### Task 3: Dependency DAG + filter/order helpers

**Files:**

- Create: `apps/web/core/services/service.helpers.ts`

- [ ] **Step 1: Create the helpers file**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceDependency, TServiceFilters, TServiceOrderByOptions } from "@plane/types";

/**
 * Adding `from -> to` is invalid if it is a self-loop.
 */
export const isSelfDependency = (from: string, to: string) => from === to;

/**
 * Adding `from -> to` is invalid if the exact edge already exists.
 */
export const dependencyExists = (dependencies: IServiceDependency[], from: string, to: string): boolean =>
  dependencies.some((d) => d.from_service_id === from && d.to_service_id === to);

/**
 * Adding `from -> to` creates a cycle if `to` can already reach `from`
 * by following existing edges (A -> B means A depends on B).
 */
export const wouldCreateCycle = (dependencies: IServiceDependency[], from: string, to: string): boolean => {
  const adjacency = new Map<string, string[]>();
  for (const dep of dependencies) {
    const children = adjacency.get(dep.from_service_id) ?? [];
    children.push(dep.to_service_id);
    adjacency.set(dep.from_service_id, children);
  }
  const stack: string[] = [to];
  const seen = new Set<string>([to]);
  while (stack.length > 0) {
    const current = stack.pop() as string;
    if (current === from) return true;
    for (const child of adjacency.get(current) ?? []) {
      if (!seen.has(child)) {
        seen.add(child);
        stack.push(child);
      }
    }
  }
  return false;
};

/**
 * Returns an error message when the dependency cannot be added, else null.
 */
export const validateDependency = (dependencies: IServiceDependency[], from: string, to: string): string | null => {
  if (isSelfDependency(from, to)) return "A service cannot depend on itself.";
  if (dependencyExists(dependencies, from, to)) return "This dependency already exists.";
  if (wouldCreateCycle(dependencies, from, to)) return "This dependency would create a cycle.";
  return null;
};

const CRITICALITY_WEIGHT: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };

const matchesFilters = (service: IService, filters: TServiceFilters): boolean => {
  if (filters.status && filters.status.length > 0 && !filters.status.includes(service.status)) return false;
  if (filters.criticality && filters.criticality.length > 0 && !filters.criticality.includes(service.criticality))
    return false;
  if (filters.type && filters.type.length > 0 && !filters.type.includes(service.type)) return false;
  return true;
};

export const filterServices = (services: IService[], filters: TServiceFilters, searchQuery: string): IService[] =>
  services.filter((s) => s.name.toLowerCase().includes(searchQuery.toLowerCase()) && matchesFilters(s, filters));

export const orderServices = (services: IService[], orderBy: TServiceOrderByOptions = "name"): IService[] => {
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
      case "name":
      default:
        return a.name.localeCompare(b.name);
    }
  });
  return ordered;
};
```

- [ ] **Step 2: Verify the DAG logic with a throwaway script**

Create `/tmp/opencode/service-helpers.check.ts`:

```ts
import { validateDependency } from "/home/ghifari/plane-for-itsm/apps/web/core/services/service.helpers";

const edge = (from: string, to: string) => ({
  id: `${from}-${to}`,
  workspace_id: "w",
  project_id: "p",
  from_service_id: from,
  to_service_id: to,
  created_at: "",
});

const deps = [edge("a", "b"), edge("b", "c")];

console.log("self:", validateDependency(deps, "a", "a"));
console.log("dup:", validateDependency(deps, "a", "b"));
console.log("cycle:", validateDependency(deps, "c", "a"));
console.log("ok:", validateDependency(deps, "a", "c"));
```

Run: `node --experimental-strip-types --no-warnings /tmp/opencode/service-helpers.check.ts`
Expected:

```
self: A service cannot depend on itself.
dup: This dependency already exists.
cycle: This dependency would create a cycle.
ok: null
```

- [ ] **Step 3: Typecheck + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/services/service.helpers.ts
git commit -m "feat(services): add dependency DAG and filter helpers"
```

### Task 4: localStorage mock repository

**Files:**

- Create: `apps/web/core/services/service-mock.repository.ts`

- [ ] **Step 1: Create the repository**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceDependency, TServiceWorkItemLink } from "@plane/types";
// helpers
import { validateDependency } from "./service.helpers";

export type TServiceStoreData = {
  version: 1;
  services: IService[];
  dependencies: IServiceDependency[];
  links: TServiceWorkItemLink[];
};

export type TStorageLike = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
};

const EMPTY_DATA = (): TServiceStoreData => ({ version: 1, services: [], dependencies: [], links: [] });

export const serviceStorageKey = (workspaceSlug: string, projectId: string) =>
  `plane:services:${workspaceSlug}:${projectId}`;

export const createMemoryStorage = (): TStorageLike => {
  const map = new Map<string, string>();
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => {
      map.set(key, value);
    },
  };
};

export const createDefaultStorage = (): TStorageLike => {
  try {
    if (typeof window !== "undefined" && window.localStorage) return window.localStorage;
  } catch {
    // blocked storage (private mode) falls through to memory
  }
  return createMemoryStorage();
};

const nowIso = () => new Date().toISOString();

const uid = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `id-${Math.random().toString(36).slice(2)}-${Date.now()}`;

const seedServices = (workspaceId: string, projectId: string): IService[] =>
  [
    {
      name: "Payment Gateway",
      status: "active",
      criticality: "critical",
      type: "internal",
      description: "Handles all card payments.",
      repo: "https://example.com/payment",
    },
    {
      name: "Auth Service",
      status: "active",
      criticality: "critical",
      type: "internal",
      description: "Authentication and sessions.",
      repo: "https://example.com/auth",
    },
    {
      name: "Notification Service",
      status: "maintenance",
      criticality: "medium",
      type: "internal",
      description: "Email and push notifications.",
      repo: "https://example.com/notify",
    },
    {
      name: "Postgres Primary",
      status: "active",
      criticality: "critical",
      type: "infrastructure",
      description: "Primary relational database.",
      repo: null,
    },
    {
      name: "Email Provider",
      status: "active",
      criticality: "high",
      type: "third_party",
      description: "External SMTP provider.",
      repo: null,
    },
    {
      name: "Analytics Pipeline",
      status: "planned",
      criticality: "low",
      type: "internal",
      description: "Batch analytics ingestion.",
      repo: "https://example.com/analytics",
    },
  ].map(
    (s, index): IService => ({
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      name: s.name,
      description: s.description,
      description_html: `<p>${s.description}</p>`,
      status: s.status as IService["status"],
      criticality: s.criticality as IService["criticality"],
      type: s.type as IService["type"],
      owner_id: null,
      repository_url: s.repo,
      documentation_url: null,
      position: null,
      sort_order: index * 65535,
      created_at: nowIso(),
      updated_at: nowIso(),
      created_by: null,
      updated_by: null,
    })
  );

export class ServiceMockRepository {
  storage: TStorageLike;

  constructor(storage: TStorageLike = createDefaultStorage()) {
    this.storage = storage;
  }

  private read(key: string): TServiceStoreData {
    const raw = this.storage.getItem(key);
    if (!raw) return EMPTY_DATA();
    try {
      const parsed = JSON.parse(raw) as TServiceStoreData;
      if (!parsed || parsed.version !== 1) return EMPTY_DATA();
      return {
        version: 1,
        services: Array.isArray(parsed.services) ? parsed.services : [],
        dependencies: Array.isArray(parsed.dependencies) ? parsed.dependencies : [],
        links: Array.isArray(parsed.links) ? parsed.links : [],
      };
    } catch {
      return EMPTY_DATA();
    }
  }

  private write(key: string, data: TServiceStoreData): TServiceStoreData {
    this.storage.setItem(key, JSON.stringify(data));
    return data;
  }

  seedIfEmpty(workspaceSlug: string, workspaceId: string, projectId: string): TServiceStoreData {
    const key = serviceStorageKey(workspaceSlug, projectId);
    // Key presence (not array length) decides seeding, so an emptied store stays empty.
    if (this.storage.getItem(key) !== null) return this.read(key);

    const services = seedServices(workspaceId, projectId);
    const byName = (name: string) => services.find((s) => s.name === name)?.id as string;
    const makeDep = (from: string, to: string): IServiceDependency => ({
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      from_service_id: from,
      to_service_id: to,
      created_at: nowIso(),
    });
    const dependencies: IServiceDependency[] = [
      makeDep(byName("Payment Gateway"), byName("Auth Service")),
      makeDep(byName("Payment Gateway"), byName("Postgres Primary")),
      makeDep(byName("Auth Service"), byName("Postgres Primary")),
      makeDep(byName("Notification Service"), byName("Email Provider")),
    ];
    const links: TServiceWorkItemLink[] = [
      {
        id: uid(),
        service_id: byName("Payment Gateway"),
        issue_id: "sample-issue-1",
        project_id: projectId,
        workspace_id: workspaceId,
        issue_identifier: "SAMPLE-1",
        issue_name: "Add 3DS support",
      },
      {
        id: uid(),
        service_id: byName("Auth Service"),
        issue_id: "sample-issue-2",
        project_id: projectId,
        workspace_id: workspaceId,
        issue_identifier: "SAMPLE-2",
        issue_name: "Rotate signing keys",
      },
    ];

    return this.write(key, { version: 1, services, dependencies, links });
  }

  getServices(workspaceSlug: string, workspaceId: string, projectId: string): IService[] {
    return this.seedIfEmpty(workspaceSlug, workspaceId, projectId).services;
  }

  getDependencies(workspaceSlug: string, workspaceId: string, projectId: string): IServiceDependency[] {
    return this.seedIfEmpty(workspaceSlug, workspaceId, projectId).dependencies;
  }

  getLinks(workspaceSlug: string, workspaceId: string, projectId: string): TServiceWorkItemLink[] {
    return this.seedIfEmpty(workspaceSlug, workspaceId, projectId).links;
  }

  createService(workspaceSlug: string, workspaceId: string, projectId: string, data: Partial<IService>): IService {
    const key = serviceStorageKey(workspaceSlug, projectId);
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    const service: IService = {
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      name: data.name ?? "Untitled service",
      description: data.description ?? "",
      description_html: data.description_html ?? "",
      status: data.status ?? "planned",
      criticality: data.criticality ?? "medium",
      type: data.type ?? "internal",
      owner_id: data.owner_id ?? null,
      repository_url: data.repository_url ?? null,
      documentation_url: data.documentation_url ?? null,
      position: null,
      sort_order: Math.max(0, ...stored.services.map((s) => s.sort_order)) + 65535,
      created_at: nowIso(),
      updated_at: nowIso(),
      created_by: data.created_by ?? null,
      updated_by: data.updated_by ?? null,
    };
    this.write(key, { ...stored, services: [...stored.services, service] });
    return service;
  }

  updateService(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ): IService | null {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    const current = stored.services.find((s) => s.id === serviceId);
    if (!current) return null;
    // Never allow identity/timestamp fields to be overwritten.
    const safe: Partial<IService> = { ...data };
    delete safe.id;
    delete safe.workspace_id;
    delete safe.project_id;
    delete safe.created_at;
    const updated: IService = { ...current, ...safe, updated_at: nowIso() };
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      services: stored.services.map((s) => (s.id === serviceId ? updated : s)),
    });
    return updated;
  }

  deleteService(workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string): void {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      services: stored.services.filter((s) => s.id !== serviceId),
      dependencies: stored.dependencies.filter((d) => d.from_service_id !== serviceId && d.to_service_id !== serviceId),
      links: stored.links.filter((l) => l.service_id !== serviceId),
    });
  }

  createDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ): IServiceDependency {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    if (!stored.services.some((s) => s.id === fromServiceId)) throw new Error("Source service not found.");
    if (!stored.services.some((s) => s.id === toServiceId)) throw new Error("Target service not found.");
    const error = validateDependency(stored.dependencies, fromServiceId, toServiceId);
    if (error) throw new Error(error);
    const dependency: IServiceDependency = {
      id: uid(),
      workspace_id: workspaceId,
      project_id: projectId,
      from_service_id: fromServiceId,
      to_service_id: toServiceId,
      created_at: nowIso(),
    };
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      dependencies: [...stored.dependencies, dependency],
    });
    return dependency;
  }

  deleteDependency(workspaceSlug: string, workspaceId: string, projectId: string, dependencyId: string): void {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      dependencies: stored.dependencies.filter((d) => d.id !== dependencyId),
    });
  }

  linkWorkItem(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ): TServiceWorkItemLink {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    if (!stored.services.some((s) => s.id === serviceId)) throw new Error("Service not found.");
    const existing = stored.links.find((l) => l.service_id === serviceId && l.issue_id === issue.id);
    if (existing) return existing;
    const link: TServiceWorkItemLink = {
      id: uid(),
      service_id: serviceId,
      issue_id: issue.id,
      project_id: projectId,
      workspace_id: workspaceId,
      issue_identifier: issue.identifier,
      issue_name: issue.name,
    };
    this.write(serviceStorageKey(workspaceSlug, projectId), { ...stored, links: [...stored.links, link] });
    return link;
  }

  unlinkWorkItem(workspaceSlug: string, workspaceId: string, projectId: string, linkId: string): void {
    const stored = this.seedIfEmpty(workspaceSlug, workspaceId, projectId);
    this.write(serviceStorageKey(workspaceSlug, projectId), {
      ...stored,
      links: stored.links.filter((l) => l.id !== linkId),
    });
  }
}
```

- [ ] **Step 2: Typecheck + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/service-mock.repository.ts
git commit -m "feat(services): add localStorage mock repository"
```

### Task 5: `ServiceService` async facade

**Files:**

- Create: `apps/web/core/services/service.service.ts`

- [ ] **Step 1: Create the service**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IService, IServiceDependency, TServiceWorkItemLink } from "@plane/types";
// services
import { ServiceMockRepository } from "@/services/service-mock.repository";

/**
 * Async facade over the localStorage mock.
 * Swap the method bodies for APIService HTTP calls when the backend lands;
 * signatures and return shapes stay identical.
 */
export class ServiceService {
  repository: ServiceMockRepository;

  constructor(repository: ServiceMockRepository = new ServiceMockRepository()) {
    this.repository = repository;
  }

  async getServices(workspaceSlug: string, workspaceId: string, projectId: string): Promise<IService[]> {
    return Promise.resolve(this.repository.getServices(workspaceSlug, workspaceId, projectId));
  }

  async getDependencies(workspaceSlug: string, workspaceId: string, projectId: string): Promise<IServiceDependency[]> {
    return Promise.resolve(this.repository.getDependencies(workspaceSlug, workspaceId, projectId));
  }

  async getWorkItemLinks(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string
  ): Promise<TServiceWorkItemLink[]> {
    return Promise.resolve(this.repository.getLinks(workspaceSlug, workspaceId, projectId));
  }

  async createService(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ): Promise<IService> {
    return Promise.resolve(this.repository.createService(workspaceSlug, workspaceId, projectId, data));
  }

  async updateService(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ): Promise<IService> {
    const updated = this.repository.updateService(workspaceSlug, workspaceId, projectId, serviceId, data);
    if (!updated) throw new Error("Service not found");
    return Promise.resolve(updated);
  }

  async deleteService(workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string): Promise<void> {
    this.repository.deleteService(workspaceSlug, workspaceId, projectId, serviceId);
    return Promise.resolve();
  }

  async createDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ): Promise<IServiceDependency> {
    return Promise.resolve(
      this.repository.createDependency(workspaceSlug, workspaceId, projectId, fromServiceId, toServiceId)
    );
  }

  async deleteDependency(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    dependencyId: string
  ): Promise<void> {
    this.repository.deleteDependency(workspaceSlug, workspaceId, projectId, dependencyId);
    return Promise.resolve();
  }

  async updateNodePosition(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ): Promise<IService> {
    const updated = this.repository.updateService(workspaceSlug, workspaceId, projectId, serviceId, { position });
    if (!updated) throw new Error("Service not found");
    return Promise.resolve(updated);
  }

  async linkWorkItem(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ): Promise<TServiceWorkItemLink> {
    return Promise.resolve(this.repository.linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, issue));
  }

  async unlinkWorkItem(workspaceSlug: string, workspaceId: string, projectId: string, linkId: string): Promise<void> {
    this.repository.unlinkWorkItem(workspaceSlug, workspaceId, projectId, linkId);
    return Promise.resolve();
  }
}
```

- [ ] **Step 2: Typecheck + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/services/service.service.ts
git commit -m "feat(services): add async service facade over mock repository"
```

---

## Phase 2 — Stores & Hooks

### Task 6: `ServiceFilterStore`

**Files:**

- Create: `apps/web/core/store/service_filter.store.ts`

- [ ] **Step 1: Create the store**

Mirror `apps/web/core/store/module_filter.store.ts`, replacing module types/keys with:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set } from "lodash-es";
import { action, computed, observable, makeObservable, runInAction, reaction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { TServiceDisplayFilters, TServiceFilters } from "@plane/types";
// helpers
import { storage } from "@/lib/local-storage";
// store
import type { CoreRootStore } from "./root.store";

const SERVICE_DISPLAY_FILTERS_KEY = "service_display_filters";
const SERVICE_FILTERS_KEY = "service_filters";

export interface IServiceFilterStore {
  displayFilters: Record<string, TServiceDisplayFilters>;
  filters: Record<string, TServiceFilters>;
  searchQuery: string;
  currentProjectDisplayFilters: TServiceDisplayFilters | undefined;
  currentProjectFilters: TServiceFilters | undefined;
  getDisplayFiltersByProjectId: (projectId: string) => TServiceDisplayFilters | undefined;
  getFiltersByProjectId: (projectId: string) => TServiceFilters;
  updateDisplayFilters: (projectId: string, displayFilters: Partial<TServiceDisplayFilters>) => void;
  updateFilters: (projectId: string, filters: Partial<TServiceFilters>) => void;
  updateSearchQuery: (query: string) => void;
  clearAllFilters: (projectId: string) => void;
}

export class ServiceFilterStore implements IServiceFilterStore {
  displayFilters: Record<string, TServiceDisplayFilters> = {};
  filters: Record<string, TServiceFilters> = {};
  searchQuery: string = "";
  rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      displayFilters: observable,
      filters: observable,
      searchQuery: observable.ref,
      currentProjectDisplayFilters: computed,
      currentProjectFilters: computed,
      updateDisplayFilters: action,
      updateFilters: action,
      updateSearchQuery: action,
      clearAllFilters: action,
    });
    this.rootStore = _rootStore;

    reaction(
      () => this.rootStore.router.projectId,
      (projectId) => {
        if (!projectId) return;
        this.initProjectServiceFilters(projectId);
        this.searchQuery = "";
      }
    );

    this.loadFromLocalStorage();
  }

  loadFromLocalStorage = () => {
    try {
      const displayFiltersData = storage.get(SERVICE_DISPLAY_FILTERS_KEY);
      const filtersData = storage.get(SERVICE_FILTERS_KEY);
      runInAction(() => {
        if (displayFiltersData) {
          const parsed = JSON.parse(displayFiltersData);
          if (typeof parsed === "object" && parsed !== null) this.displayFilters = parsed;
        }
        if (filtersData) {
          const parsed = JSON.parse(filtersData);
          if (typeof parsed === "object" && parsed !== null) this.filters = parsed;
        }
      });
    } catch (error) {
      console.error("Failed to load service filters from localStorage:", error);
      runInAction(() => {
        this.displayFilters = {};
        this.filters = {};
      });
    }
  };

  saveDisplayFiltersToLocalStorage = () => {
    storage.set(SERVICE_DISPLAY_FILTERS_KEY, this.displayFilters);
  };

  saveFiltersToLocalStorage = () => {
    storage.set(SERVICE_FILTERS_KEY, this.filters);
  };

  get currentProjectDisplayFilters() {
    const projectId = this.rootStore.router.projectId;
    if (!projectId) return;
    return this.displayFilters[projectId];
  }

  get currentProjectFilters() {
    const projectId = this.rootStore.router.projectId;
    if (!projectId) return;
    return this.filters[projectId] ?? {};
  }

  getDisplayFiltersByProjectId = computedFn((projectId: string) => this.displayFilters[projectId]);

  getFiltersByProjectId = computedFn((projectId: string) => this.filters[projectId] ?? {});

  initProjectServiceFilters = (projectId: string) => {
    const displayFilters = this.getDisplayFiltersByProjectId(projectId);
    runInAction(() => {
      this.displayFilters[projectId] = {
        layout: displayFilters?.layout || "list",
        order_by: displayFilters?.order_by || "name",
      };
      this.filters[projectId] = this.filters[projectId] ?? {};
    });
    this.saveDisplayFiltersToLocalStorage();
    this.saveFiltersToLocalStorage();
  };

  updateDisplayFilters = (projectId: string, displayFilters: Partial<TServiceDisplayFilters>) => {
    runInAction(() => {
      Object.keys(displayFilters).forEach((key) => {
        set(this.displayFilters, [projectId, key], displayFilters[key as keyof TServiceDisplayFilters]);
      });
    });
    this.saveDisplayFiltersToLocalStorage();
  };

  updateFilters = (projectId: string, filters: Partial<TServiceFilters>) => {
    runInAction(() => {
      Object.keys(filters).forEach((key) => {
        set(this.filters, [projectId, key], filters[key as keyof TServiceFilters]);
      });
    });
    this.saveFiltersToLocalStorage();
  };

  updateSearchQuery = (query: string) => {
    this.searchQuery = query;
  };

  clearAllFilters = (projectId: string) => {
    runInAction(() => {
      this.filters[projectId] = {};
    });
    this.saveFiltersToLocalStorage();
  };
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/store/service_filter.store.ts
git commit -m "feat(services): add service filter store"
```

### Task 7: `ServicesStore`

**Files:**

- Create: `apps/web/core/store/service.store.ts`

- [ ] **Step 1: Create the store**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { set, sortBy } from "lodash-es";
import { action, observable, makeObservable, runInAction } from "mobx";
import { computedFn } from "mobx-utils";
// types
import type { IService, IServiceDependency, TServiceGraphData, TServiceWorkItemLink } from "@plane/types";
// helpers
import { filterServices, orderServices } from "@/services/service.helpers";
// services
import { ServiceService } from "@/services/service.service";
// store
import type { CoreRootStore } from "./root.store";

export interface IServiceStore {
  loader: boolean;
  fetchedMap: Record<string, boolean>;
  serviceMap: Record<string, IService>;
  dependencyMap: Record<string, IServiceDependency>;
  workItemLinkMap: Record<string, TServiceWorkItemLink>;
  getServiceById: (serviceId: string) => IService | null;
  getProjectServiceIds: (projectId: string) => string[] | null;
  getFilteredServiceIds: (projectId: string) => string[] | null;
  getDependenciesByProject: (projectId: string) => IServiceDependency[];
  getWorkItemLinksByService: (serviceId: string) => TServiceWorkItemLink[];
  getGraphData: (projectId: string) => TServiceGraphData;
  fetchServices: (workspaceSlug: string, workspaceId: string, projectId: string) => Promise<IService[] | undefined>;
  createService: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    data: Partial<IService>
  ) => Promise<IService>;
  updateService: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ) => Promise<IService>;
  deleteService: (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => Promise<void>;
  addDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ) => Promise<IServiceDependency>;
  removeDependency: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    dependencyId: string
  ) => Promise<void>;
  updateNodePosition: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ) => Promise<void>;
  linkWorkItem: (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ) => Promise<TServiceWorkItemLink>;
  unlinkWorkItem: (workspaceSlug: string, workspaceId: string, projectId: string, linkId: string) => Promise<void>;
}

export class ServicesStore implements IServiceStore {
  loader: boolean = false;
  fetchedMap: Record<string, boolean> = {};
  serviceMap: Record<string, IService> = {};
  dependencyMap: Record<string, IServiceDependency> = {};
  workItemLinkMap: Record<string, TServiceWorkItemLink> = {};
  rootStore;
  serviceService;

  constructor(_rootStore: CoreRootStore) {
    makeObservable(this, {
      loader: observable.ref,
      fetchedMap: observable,
      serviceMap: observable,
      dependencyMap: observable,
      workItemLinkMap: observable,
      fetchServices: action,
      createService: action,
      updateService: action,
      deleteService: action,
      addDependency: action,
      removeDependency: action,
      updateNodePosition: action,
      linkWorkItem: action,
      unlinkWorkItem: action,
    });
    this.rootStore = _rootStore;
    this.serviceService = new ServiceService();
  }

  getServiceById = computedFn((serviceId: string) => this.serviceMap[serviceId] || null);

  getProjectServiceIds = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    const services = sortBy(
      Object.values(this.serviceMap).filter((s) => s.project_id === projectId),
      [(s) => s.sort_order]
    );
    return services.map((s) => s.id);
  });

  getFilteredServiceIds = computedFn((projectId: string) => {
    if (!this.fetchedMap[projectId]) return null;
    const displayFilters = this.rootStore.serviceFilter.getDisplayFiltersByProjectId(projectId);
    const filters = this.rootStore.serviceFilter.getFiltersByProjectId(projectId);
    const searchQuery = this.rootStore.serviceFilter.searchQuery;
    const services = Object.values(this.serviceMap).filter((s) => s.project_id === projectId);
    const filtered = filterServices(services, filters, searchQuery);
    return orderServices(filtered, displayFilters?.order_by).map((s) => s.id);
  });

  getDependenciesByProject = computedFn((projectId: string) =>
    Object.values(this.dependencyMap).filter((d) => d.project_id === projectId)
  );

  getWorkItemLinksByService = computedFn((serviceId: string) =>
    Object.values(this.workItemLinkMap).filter((l) => l.service_id === serviceId)
  );

  getGraphData = computedFn((projectId: string): TServiceGraphData => {
    const serviceIds = this.getFilteredServiceIds(projectId) ?? [];
    const services = serviceIds
      .map((id) => this.serviceMap[id])
      .filter((service): service is IService => Boolean(service));
    const visible = new Set(serviceIds);
    const dependencies = this.getDependenciesByProject(projectId).filter(
      (d) => visible.has(d.from_service_id) && visible.has(d.to_service_id)
    );
    return { services, dependencies };
  });

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
      return services;
    } catch {
      runInAction(() => {
        this.loader = false;
      });
      return undefined;
    }
  };

  createService = async (workspaceSlug: string, workspaceId: string, projectId: string, data: Partial<IService>) => {
    const service = await this.serviceService.createService(workspaceSlug, workspaceId, projectId, data);
    runInAction(() => {
      set(this.serviceMap, [service.id], service);
    });
    return service;
  };

  updateService = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    data: Partial<IService>
  ) => {
    const original = this.getServiceById(serviceId);
    if (!original) throw new Error("Service not found");
    try {
      runInAction(() => {
        set(this.serviceMap, [serviceId], { ...original, ...data });
      });
      const response = await this.serviceService.updateService(workspaceSlug, workspaceId, projectId, serviceId, data);
      runInAction(() => {
        set(this.serviceMap, [serviceId], response);
      });
      return response;
    } catch (error) {
      console.error("Failed to update service in service store", error);
      runInAction(() => {
        set(this.serviceMap, [serviceId], original);
      });
      throw error;
    }
  };

  deleteService = async (workspaceSlug: string, workspaceId: string, projectId: string, serviceId: string) => {
    await this.serviceService.deleteService(workspaceSlug, workspaceId, projectId, serviceId);
    runInAction(() => {
      delete this.serviceMap[serviceId];
      Object.values(this.dependencyMap).forEach((d) => {
        if (d.from_service_id === serviceId || d.to_service_id === serviceId) delete this.dependencyMap[d.id];
      });
      Object.values(this.workItemLinkMap).forEach((l) => {
        if (l.service_id === serviceId) delete this.workItemLinkMap[l.id];
      });
    });
  };

  addDependency = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    fromServiceId: string,
    toServiceId: string
  ) => {
    const dependency = await this.serviceService.createDependency(
      workspaceSlug,
      workspaceId,
      projectId,
      fromServiceId,
      toServiceId
    );
    runInAction(() => {
      set(this.dependencyMap, [dependency.id], dependency);
    });
    return dependency;
  };

  removeDependency = async (workspaceSlug: string, workspaceId: string, projectId: string, dependencyId: string) => {
    await this.serviceService.deleteDependency(workspaceSlug, workspaceId, projectId, dependencyId);
    runInAction(() => {
      delete this.dependencyMap[dependencyId];
    });
  };

  updateNodePosition = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    position: { x: number; y: number }
  ) => {
    const response = await this.serviceService.updateNodePosition(
      workspaceSlug,
      workspaceId,
      projectId,
      serviceId,
      position
    );
    runInAction(() => {
      set(this.serviceMap, [serviceId], response);
    });
  };

  linkWorkItem = async (
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    serviceId: string,
    issue: { id: string; identifier?: string; name?: string }
  ) => {
    const link = await this.serviceService.linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, issue);
    runInAction(() => {
      set(this.workItemLinkMap, [link.id], link);
    });
    return link;
  };

  unlinkWorkItem = async (workspaceSlug: string, workspaceId: string, projectId: string, linkId: string) => {
    await this.serviceService.unlinkWorkItem(workspaceSlug, workspaceId, projectId, linkId);
    runInAction(() => {
      delete this.workItemLinkMap[linkId];
    });
  };
}
```

- [ ] **Step 2: Typecheck + lint**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint`
Expected: PASS (fix any `no-array-sort` / import-order warnings by matching existing files).

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/store/service.store.ts
git commit -m "feat(services): add services store over mock service"
```

### Task 8: Service hooks (root registration already landed in Task 7)

**Files:**

- Create: `apps/web/core/hooks/store/use-service.ts`
- Create: `apps/web/core/hooks/store/use-service-filter.ts`

> Note: the `root.store.ts` registration (imports, fields, constructor + `resetOnSignOut` instantiations) was applied in Task 7 (commit `baccda63d`) to keep the store typecheck self-contained.

- [ ] **Step 1: Verify root registration is present**

Confirm `apps/web/core/store/root.store.ts` contains the `service`/`serviceFilter` imports, fields, and instantiations. If any piece is missing, add it per the original snippet below before proceeding.

```ts
import type { IServiceStore } from "./service.store";
import { ServicesStore } from "./service.store";
import type { IServiceFilterStore } from "./service_filter.store";
import { ServiceFilterStore } from "./service_filter.store";
```

Add fields to the `CoreRootStore` class (next to `module` / `moduleFilter`):

```ts
service: IServiceStore;
serviceFilter: IServiceFilterStore;
```

Add instantiations in **both** the `constructor()` and `resetOnSignOut()` (next to `this.moduleFilter`):

```ts
this.service = new ServicesStore(this);
this.serviceFilter = new ServiceFilterStore(this);
```

- [ ] **Step 2: Create `use-service.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// mobx store
import { StoreContext } from "@/lib/store-context";
// types
import type { IServiceStore } from "@/store/service.store";

export const useService = (): IServiceStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useService must be used within StoreProvider");
  return context.service;
};
```

- [ ] **Step 3: Create `use-service-filter.ts`**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useContext } from "react";
// mobx store
import { StoreContext } from "@/lib/store-context";
// types
import type { IServiceFilterStore } from "@/store/service_filter.store";

export const useServiceFilter = (): IServiceFilterStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useServiceFilter must be used within StoreProvider");
  return context.serviceFilter;
};
```

- [ ] **Step 4: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/hooks/store/use-service.ts apps/web/core/hooks/store/use-service-filter.ts
git commit -m "feat(services): add service store hooks"
```

### Task 9: Add `service_view` to project types

**Files:**

- Modify: `packages/types/src/project/projects.ts`

- [ ] **Step 1: Add the optional flag**

In `IPartialProject`, add next to `module_view`:

```ts
  service_view?: boolean;
```

- [ ] **Step 2: Typecheck + commit**

Run: `pnpm --filter=web check:types`
Expected: PASS.

```bash
git add packages/types/src/project/projects.ts
git commit -m "feat(services): add optional service_view project flag"
```

---

## Phase 3 — Navigation, Routes, Minimal List (visible milestone)

### Task 10: Navigation entries

**Files:**

- Modify: `apps/web/core/components/navigation/use-navigation-items.ts`
- Modify: `apps/web/core/components/workspace/sidebar/project-navigation.tsx`
- Modify: `apps/web/core/components/navigation/tab-navigation-utils.ts`

- [ ] **Step 1: Add the tab navigation item**

In `use-navigation-items.ts`, add an icon import (`Server` from `@makeplane/propel/icons`; verify the exact name exists with `rg "Server" node_modules/@makeplane/propel/dist` — if absent use `Layers`) and add this entry after `modules`:

```ts
      {
        i18n_key: "sidebar.services",
        key: "services",
        name: "Services",
        href: `/${workspaceSlug}/projects/${projectId}/services`,
        icon: Server,
        access: [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
        shouldRender: project?.service_view ?? true,
        sortOrder: 4,
      },
```

Then change the existing `views` entry `sortOrder: 4` → `5`, `pages` `5` → `6`, `intake` `6` → `7`.

- [ ] **Step 2: Mirror in the sidebar navigation**

In `project-navigation.tsx`, add the equivalent `key: "services"` entry to `baseNavigation` (same href, icon, access, `sortOrder: 4`, `shouldRender: project?.service_view ?? true`) and apply the same `views/pages/intake` sortOrder shift (5/6/7).

- [ ] **Step 3: Add the URL map entry**

In `tab-navigation-utils.ts`, add to `tabUrlMap`:

```ts
  services: `${baseUrl}/services`,
```

- [ ] **Step 4: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/navigation/use-navigation-items.ts apps/web/core/components/workspace/sidebar/project-navigation.tsx apps/web/core/components/navigation/tab-navigation-utils.ts
git commit -m "feat(services): add services navigation entries"
```

### Task 11: i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/*` (per the `translate` skill)

- [ ] **Step 1: Load the translate skill**

Invoke the `translate` skill and follow its workflow. Add English source strings under the appropriate namespaces:

- `sidebar.services` = `"Services"`
- `service.title` = `"Services"`
- `service.empty_state.title` / `service.empty_state.description`
- `service.add` = `"Add service"`
- `service.create` = `"Create service"`
- `service.fields.{name,description,status,criticality,type,owner,repository,documentation}`
- `service.tabs.{overview,work_items,dependencies}`
- `service.graph.{re_layout,connect_hint,empty}`

- [ ] **Step 2: Verify**

Run: `pnpm --filter=web check:types`
Expected: PASS. If the i18n package generates typed keys, ensure the new keys are regenerated as the skill instructs.

- [ ] **Step 3: Commit**

```bash
git add packages/i18n
git commit -m "feat(services): add service i18n strings"
```

### Task 12: Minimal list page (visible milestone)

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(list)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(list)/page.tsx`
- Create: `apps/web/core/components/services/index.ts`
- Create: `apps/web/core/components/services/service-list-item.tsx`
- Create: `apps/web/core/components/services/services-list-view.tsx`
- Modify: `apps/web/core/routes/core.ts` (register the `services` list route — without it the route 404s and typegen emits no `+types/page`; landed in Task 12 commit `8738c0b68`)

- [ ] **Step 1: Route layout**

Create `(list)/layout.tsx` mirroring `.../modules/(list)/layout.tsx` (imports `AppHeader` + `ContentWrapper`, renders `<Outlet />`). Change only the import paths if the module layout is self-contained; the shell is feature-agnostic.

- [ ] **Step 2: List page**

Create `(list)/page.tsx` mirroring `.../modules/(list)/page.tsx`:

- Fetch services on mount with `useService().fetchServices(workspaceSlug, workspaceId, projectId)`. Get `workspaceId` from `useWorkspace()` (same source the module page uses) and `projectId` from route params.
- Read ids via `useService().getFilteredServiceIds(projectId)`.
- Render `<ServicesListView />` (applied filters are added in Task 15).

- [ ] **Step 3: `service-list-item.tsx`**

Mirror the visual structure of `apps/web/core/components/modules/module-list-item.tsx`, but render:

- name, a status badge (`EUserPermissions`-independent; use `@plane/ui` `Badge`/`Tag` primitives consistent with module status), criticality label, type label, owner display via `MemberDropdown`/`Avatar` if `owner_id`, and links to `repository_url` / `documentation_url` when present.

- [ ] **Step 4: `services-list-view.tsx`**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useServiceFilter } from "@/hooks/store/use-service-filter";
import { useProject } from "@/hooks/store/use-project";
// components
import { ServiceListItem } from "./service-list-item";

export const ServicesListView = observer(function ServicesListView() {
  const { getFilteredServiceIds } = useService();
  const { currentProjectDisplayFilters } = useServiceFilter();
  const { currentProjectDetails } = useProject();

  const projectId = currentProjectDetails?.id;
  const serviceIds = projectId ? getFilteredServiceIds(projectId) : null;

  if (serviceIds === null) {
    return <div className="p-6 text-sm text-secondary">Loading services…</div>;
  }

  if (serviceIds.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 p-6">
        <p className="text-sm font-medium text-primary">No services yet</p>
        <p className="text-xs text-secondary">Add a service to start mapping dependencies.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1 p-2">
      {serviceIds.map((id) => (
        <ServiceListItem key={id} serviceId={id} />
      ))}
    </div>
  );
});
```

> Confirm the exact hook names for reading the current project (`useProject` vs the pattern used in `modules-list-view.tsx`) and the `ServiceListItem` props during implementation; align them with the module equivalent.

- [ ] **Step 5: Barrel `index.ts`**

```ts
export * from "./services-list-view";
export * from "./service-list-item";
```

- [ ] **Step 6: Manual verification**

Run: `pnpm --filter=web dev`, open `http://localhost:3000/<ws>/projects/<projectId>/services`.
Expected: sidebar "Services" entry appears; the page lists the 6 seeded services and survives a refresh (localStorage).

- [ ] **Step 7: Lint/format/typecheck + commit**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web fix:format`
Expected: PASS.

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services" apps/web/core/components/services
git commit -m "feat(services): add route and list view (mock-backed)"
```

---

## Phase 4 — CRUD, Header, Filters, Grid

### Task 13: Service form + create/edit modal

**Files:**

- Create: `apps/web/core/components/services/service-form.tsx`
- Create: `apps/web/core/components/services/modal.tsx`
- Create: `apps/web/core/components/services/delete-service-modal.tsx`

> Post-review fixes (required, landed in `d21279bc8` follow-up): URL fields must validate (allow empty, else valid URL, error rendered like `errors.name`); submit must set `description_html` (derived from `description`); modal must rethrow on error and the form must reset only on success.

- [ ] **Step 1: Form**

Mirror `apps/web/core/components/modules/form.tsx` for structure and the description input (use the same rich-text/description component the module form uses). Fields:

- `name` (required, controlled)
- `description` (same input component as module form)
- `status` select — options `active | planned | maintenance | deprecated | retired`
- `criticality` select — options `critical | high | medium | low`
- `type` select — options `internal | external | infrastructure | third_party`
- `owner_id` — use the same member dropdown the module form uses for `lead_id`
- `repository_url`, `documentation_url` — plain `Input` (`@plane/ui`)

Submit calls `createService` (create) or `updateService` (edit) from `useService()`.

- [ ] **Step 2: Modal**

Mirror `apps/web/core/components/modules/modal.tsx`: a `@plane/ui` `Modal` wrapping the form, with create/edit titles and a `useServiceFilter`/`useService` store call. Expose `ServiceModal` and a `useServiceModal`-style open/close convention consistent with the module modal (or the simplest existing pattern; do not invent a new global modal store).

- [ ] **Step 3: Delete modal**

Mirror `apps/web/core/components/modules/delete-module-modal.tsx`, calling `deleteService`.

- [ ] **Step 4: Wire "Add service" button**

Add a header/toolbar button that opens the create modal. In Phase 3's page, place it in the list header (Task 14) or directly in the list view for now.

- [ ] **Step 5: Manual verification**

Create a service → appears in list and persists on refresh. Edit it → changes persist. Delete it → removed and its edges/links disappear from the graph/list.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/services
git commit -m "feat(services): add create/edit/delete flows"
```

### Task 14: Header, view toggle, order-by

**Files:**

- Create: `apps/web/core/components/services/service-view-header.tsx`
- Create: `apps/web/core/components/services/service-mobile-header.tsx`
- Create: `apps/web/core/components/services/service-layout-icon.tsx`
- Create: `apps/web/core/components/services/dropdowns/order-by.tsx`

> Carry-overs from Task 12 review (do in this task): mount an `AppHeader` in `(list)/layout.tsx`; in the list page use `fetchedMap[projectId]` (not the whole object) as the fetch-effect dep; add `aria-hidden="true"` to the status dot; use `rel="noopener noreferrer"`; drop the redundant `?.` after the null guard in the list item; move the hardcoded loading string to i18n.

- [ ] **Step 1: Header**

Mirror `apps/web/core/components/modules/module-view-header.tsx` + `modules-list-header`. Include:

- Title (`service.title`)
- "Add service" button (opens create modal)
- View toggle buttons: List / Grid / Graph, calling `useServiceFilter().updateDisplayFilters(projectId, { layout })`
- Layout icon component for each option (mirror `apps/web/core/components/modules/module-layout-icon.tsx`)
- Order-by dropdown (mirror `modules/dropdowns/order-by.tsx`) with options `name`, `-created_at`, `-updated_at`, `criticality`, `status`
- All layout/order-by/mobile labels via i18n (mirror the module `i18n_label` pattern): `service.layout.{list,grid,graph}`, `service.order_by.{name,created,updated,criticality,status}`, `service.layout_label`, `service.graph.coming_soon`. Add English source, translate to all locales (translate skill), keep `sync:check` green.

- [ ] **Step 2: Mobile header**

Mirror `modules/(list)/mobile-header.tsx` / the module mobile header component; reuse the same actions.

- [ ] **Step 3: Grid rendering**

In `services-list-view.tsx`, branch on `currentProjectDisplayFilters?.layout`:

- `"list"` → current `ServiceListItem` rows
- `"grid"` → create `service-card-item.tsx` (mirror `module-card-item.tsx`), render in a responsive grid. Do NOT nest `<a>` inside the card's link — render repo/docs actions outside the link element (mirror how `ListItem` keeps `actionableItems` outside `ControlLink`).
- `"graph"` → render `<ServiceGraph />` (Phase 5); until then a placeholder `div` with "Graph coming soon"

- [ ] **Step 4: Manual verification**

Toggle List/Grid/Graph persists per project after refresh. Order-by changes list order.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services
git commit -m "feat(services): add header, view toggle, grid, order-by"
```

### Task 15: Filters + applied filters

**Files:**

- Create: `apps/web/core/components/services/filters/{root,status,criticality,type}.tsx`
- Create: `apps/web/core/components/services/applied-filters/root.tsx`
- Create: `apps/web/core/components/services/search-input.tsx`

> Carry-overs from Task 12 review (do in this task): branch the list view on unfiltered vs filtered ids — use `getProjectServiceIds` for the true empty state and `getFilteredServiceIds` for a distinct "no matches" state (mirror `modules-list-view.tsx`); move status/criticality/type badge labels to i18n (no raw enum strings).

- [ ] **Step 1: Filter dropdowns**

Mirror `apps/web/core/components/modules/dropdowns/filters/*`:

- `status.tsx` — multi-select of `active|planned|maintenance|deprecated|retired`; on change call `updateFilters(projectId, { status: value })`
- `criticality.tsx` — `critical|high|medium|low`
- `type.tsx` — `internal|external|infrastructure|third_party`
- `root.tsx` — container combining the three, plus `search-input.tsx` calling `updateSearchQuery`

- [ ] **Step 2: Applied filters**

Mirror `apps/web/core/components/modules/applied-filters/root.tsx` + `date/members/status` pattern; render removable chips for active status/criticality/type filters. Add `ServiceAppliedFiltersList` to the list page.

- [ ] **Step 3: Manual verification**

Filter by status/criticality/type → list and graph shrink accordingly. Search filters by name. Clearing filters restores all. Filters persist per project.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/services
git commit -m "feat(services): add filters and applied filters"
```

---

## Phase 5 — Graph View

### Task 16: dagre layout hook

**Files:**

- Create: `apps/web/core/components/services/graph/use-graph-layout.ts`

- [ ] **Step 1: Create the layout helper**

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Edge, Node } from "@xyflow/react";
import dagre from "dagre";
import type { IService, IServiceDependency } from "@plane/types";

export const SERVICE_NODE_WIDTH = 220;
export const SERVICE_NODE_HEIGHT = 72;

export const getLayoutedElements = (
  services: IService[],
  dependencies: IServiceDependency[]
): { nodes: Node[]; edges: Edge[] } => {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 90 });

  const ids = new Set(services.map((s) => s.id));
  services.forEach((s) => graph.setNode(s.id, { width: SERVICE_NODE_WIDTH, height: SERVICE_NODE_HEIGHT }));
  dependencies.forEach((d) => {
    if (ids.has(d.from_service_id) && ids.has(d.to_service_id)) graph.setEdge(d.from_service_id, d.to_service_id);
  });

  dagre.layout(graph);

  const nodes: Node[] = services.map((service) => {
    const pos = graph.node(service.id);
    return {
      id: service.id,
      type: "service",
      data: { service },
      position: service.position ?? {
        x: (pos?.x ?? 0) - SERVICE_NODE_WIDTH / 2,
        y: (pos?.y ?? 0) - SERVICE_NODE_HEIGHT / 2,
      },
      sourcePosition: "right",
      targetPosition: "left",
    } as Node;
  });

  const edges: Edge[] = dependencies
    .filter((d) => ids.has(d.from_service_id) && ids.has(d.to_service_id))
    .map((d) => ({
      id: d.id,
      source: d.from_service_id,
      target: d.to_service_id,
      type: "smoothstep",
    }));

  return { nodes, edges };
};
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter=web check:types`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/services/graph/use-graph-layout.ts
git commit -m "feat(services): add dagre graph layout"
```

### Task 17: Service node component

**Files:**

- Create: `apps/web/core/components/services/graph/service-node.tsx`

- [ ] **Step 1: Create the custom node**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { IService } from "@plane/types";

export type TServiceNodeData = {
  service: IService;
};

const CRITICALITY_CLASS: Record<string, string> = {
  critical: "bg-red-500",
  high: "bg-orange-500",
  medium: "bg-yellow-500",
  low: "bg-green-500",
};

export function ServiceNode({ data }: NodeProps) {
  const { service } = data as TServiceNodeData;
  return (
    <div className="min-w-[180px] rounded-md border border-subtle bg-surface-1 px-3 py-2 shadow-sm">
      <Handle type="target" position={Position.Left} className="!bg-surface-3" />
      <div className="flex items-center gap-2">
        <span className={`h-2 w-2 rounded-full ${CRITICALITY_CLASS[service.criticality] ?? "bg-gray-400"}`} />
        <span className="truncate text-sm font-medium text-primary">{service.name}</span>
      </div>
      <div className="mt-1 text-xs capitalize text-secondary">
        {service.status} · {service.criticality}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-surface-3" />
    </div>
  );
}
```

> Use the exact `@plane/ui`/Tailwind color tokens used by module components (verify `border-subtle`, `bg-surface-1`, `text-primary`, etc. exist in this codebase; substitute the observed tokens).

- [ ] **Step 2: Commit**

```bash
git add apps/web/core/components/services/graph/service-node.tsx
git commit -m "feat(services): add service graph node"
```

### Task 18: Graph canvas

**Files:**

- Create: `apps/web/core/components/services/graph/service-graph.tsx`
- Modify: `apps/web/core/components/services/services-list-view.tsx` (render graph)

- [ ] **Step 1: Create the canvas**

```tsx
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { observer } from "mobx-react";
import {
  addEdge,
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useProject } from "@/hooks/store/use-project";
// components
import { ServiceNode } from "./service-node";
import { getLayoutedElements } from "./use-graph-layout";

const nodeTypes = { service: ServiceNode };

export const ServiceGraph = observer(function ServiceGraph() {
  const { getGraphData, addDependency, removeDependency, updateNodePosition } = useService();
  const { currentProjectDetails, currentWorkspace } = useProject();

  const projectId = currentProjectDetails?.id;
  const workspaceSlug = currentWorkspace?.slug;

  const graphData = projectId ? getGraphData(projectId) : { services: [], dependencies: [] };

  const { nodes: layoutNodes, edges: layoutEdges } = useMemo(
    () => getLayoutedElements(graphData.services, graphData.dependencies),
    [graphData.services, graphData.dependencies]
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutEdges);

  useEffect(() => {
    setNodes(layoutNodes);
    setEdges(layoutEdges);
  }, [layoutNodes, layoutEdges, setNodes, setEdges]);

  const onConnect = useCallback(
    async (connection: Connection) => {
      if (
        !connection.source ||
        !connection.target ||
        !workspaceSlug ||
        !currentProjectDetails?.workspace_id ||
        !projectId
      )
        return;
      try {
        const dependency = await addDependency(
          workspaceSlug,
          currentProjectDetails.workspace_id,
          projectId,
          connection.source,
          connection.target
        );
        setEdges((prev) =>
          addEdge({ id: dependency.id, source: connection.source, target: connection.target, type: "smoothstep" }, prev)
        );
      } catch (error) {
        const message = error instanceof Error ? error.message : "Could not create dependency";
        // use the codebase toast helper here (see other services/components for `setToast`)
        console.error(message);
      }
    },
    [addDependency, currentProjectDetails?.workspace_id, projectId, setEdges, workspaceSlug]
  );

  const onEdgesDelete = useCallback(
    async (deleted: Edge[]) => {
      if (!workspaceSlug || !currentProjectDetails?.workspace_id || !projectId) return;
      await Promise.all(
        deleted.map((edge) => removeDependency(workspaceSlug, currentProjectDetails.workspace_id, projectId, edge.id))
      );
    },
    [currentProjectDetails?.workspace_id, projectId, removeDependency, workspaceSlug]
  );

  const onNodeDragStop = useCallback(
    async (_event: React.MouseEvent, node: Node) => {
      if (!workspaceSlug || !currentProjectDetails?.workspace_id || !projectId) return;
      await updateNodePosition(workspaceSlug, currentProjectDetails.workspace_id, projectId, node.id, node.position);
    },
    [currentProjectDetails?.workspace_id, projectId, updateNodePosition, workspaceSlug]
  );

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onEdgesDelete={onEdgesDelete}
        onNodeDragStop={onNodeDragStop}
        fitView
        deleteKeyCode={["Backspace", "Delete"]}
      >
        <Background />
        <Controls />
        <MiniMap />
      </ReactFlow>
    </div>
  );
});
```

- [ ] **Step 2: Wire into list view**

In `services-list-view.tsx`, when `layout === "graph"`, render `<ServiceGraph />` inside a container with an explicit height (e.g. `h-[calc(100vh-12rem)]`).

- [ ] **Step 3: Re-layout button**

Add a "Re-layout" control (in the graph container) that clears saved positions via `updateNodePosition` for each node (or calls a dedicated reset that sets `position: null`) and recomputes dagre. Simplest: a button that calls `updateService(..., { position: null })` for each service, then the derived layout recomputes.

- [ ] **Step 4: Manual verification**

Graph renders the seeded services with edges. Drag a node → refresh → position persists. Drag from a node's right handle to another node → edge created. Attempt a cycle (e.g. reverse an existing edge) → error logged/toast and no edge. Select an edge + Delete → edge removed. Filters hide nodes.

- [ ] **Step 5: Lint/format/typecheck + commit**

Run: `pnpm --filter=web check:types && pnpm --filter=web check:lint && pnpm --filter=web fix:format`
Expected: PASS.

```bash
git add apps/web/core/components/services
git commit -m "feat(services): add React Flow dependency graph"
```

---

## Phase 6 — Service Detail & Work-Item Linking

### Task 19: Detail route + tabs

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/layout.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)/[serviceId]/page.tsx`
- Create: `apps/web/core/components/services/detail/{root,header,tabs,overview,work-items,dependencies}.tsx`

- [ ] **Step 1: Route files**

Mirror the module detail route group:

- `(detail)/layout.tsx` — same shell as `(list)/layout.tsx`
- `(detail)/[serviceId]/page.tsx` — read `serviceId` from route params, ensure the service exists in the store (it is hydrated by `fetchServices`; if missing, refetch), render `<ServiceDetailRoot />`

- [ ] **Step 2: Detail components**

- `root.tsx` — `ServiceDetailHeader` + `ServiceDetailTabs`; manages active tab state (`overview | work_items | dependencies`)
- `header.tsx` — service name, status/criticality/type badges, owner, links, an Edit button opening the Task 13 modal, Delete button
- `tabs.tsx` — simple tab bar (mirror module/page tab patterns in the codebase)
- `overview.tsx` — description (`dangerouslySetInnerHTML` of `description_html` or the same renderer used by pages), plus a read-only list of "Depends on" and "Depended on by" services computed from `getDependenciesByProject`
- `dependencies.tsx` — editable dependency list: shows outgoing/incoming edges with remove buttons, and an "Add dependency" select listing other project services. Add calls `addDependency`; remove calls `removeDependency`; errors surfaced as toast/log.
- `work-items.tsx` — list of `getWorkItemLinksByService(serviceId)`; each shows `issue_identifier ?? issue_id` and `issue_name ?? "Untitled"` with an unlink button calling `unlinkWorkItem`. "Add work items" uses the existing issue picker/selector pattern; pass `{ id, identifier: issue.project_identifier ? \`${identifier}-\${sequence_id}\` : undefined, name }`into`linkWorkItem`.

- [ ] **Step 3: Make list rows link to detail**

In `service-list-item.tsx` / `service-card-item.tsx`, navigate to `/${workspaceSlug}/projects/${projectId}/services/${serviceId}` on click.

- [ ] **Step 4: Manual verification**

Open a service detail; Overview shows info + dependencies; Dependencies tab add/remove works and reflects in the graph; Work items tab shows seeded links and can unlink.

- [ ] **Step 5: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/services/(detail)" apps/web/core/components/services/detail apps/web/core/components/services/service-list-item.tsx apps/web/core/components/services/service-card-item.tsx
git commit -m "feat(services): add service detail with dependencies and work items"
```

### Task 20: Work-item detail selector

**Files:**

- Create: `apps/web/core/components/services/select/service-select.tsx`
- Modify: the work-item detail properties file that renders `ModuleSelect` (find with `rg -n "ModuleSelect" apps/web/core/components/issues/issue-detail`)

- [ ] **Step 1: Create `service-select.tsx`**

Mirror `apps/web/core/components/issues/issue-detail/module-select.tsx`:

- Read linked services for the current issue from `useService().workItemLinkMap`.
- Render a dropdown listing all project services (from `getProjectServiceIds`), with checkboxes; toggling on calls `linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, { id: issueId, identifier, name })`, toggling off calls `unlinkWorkItem` for the matching link.
- Show selected services as chips.

- [ ] **Step 2: Mount it**

Add `<ServiceSelect issueId={...} disabled={...} />` next to `<ModuleSelect />` in the work-item detail properties list.

- [ ] **Step 3: Manual verification**

Open a work item; link it to a service; open that service's Work items tab → the work item appears. Unlink from either side → disappears both sides.

- [ ] **Step 4: Final full verification**

Run:

```bash
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web check:format
```

Then run the full manual checklist from the spec (sidebar entry, list/grid/graph, filters, CRUD, drag persistence, re-layout, cycle rejection, work-item link/unlink).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/services apps/web/core/components/issues
git commit -m "feat(services): link work items from issue detail"
```

---

## Self-review notes

- **Spec coverage:** Data model → Tasks 2/4; service layer → Tasks 3/4/5; stores/hooks → Tasks 6/7/8; `service_view` flag → Task 9; routes/nav/i18n → Tasks 10/11/12; list/grid/filters/CRUD → Tasks 12–15; graph (React Flow + dagre, DAG rules, drag persistence, re-layout) → Tasks 3/16/17/18; detail + work-item linking → Tasks 19/20; verification → per-task + Task 20 Step 4.
- **Deferred per spec:** backend, public API, settings feature toggle, peek, archive/favorite, bulk ops, activity feed, frontend test runner.
- **Known adaptation:** exact hook names (`useProject`, workspace id source), `@plane/ui` color tokens, icon availability, and the module form's description component must be confirmed against the current files during implementation; each task names the exact module file to mirror.
