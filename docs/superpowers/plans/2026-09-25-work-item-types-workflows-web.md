# Work Item Types & Workflows (Web) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Admin UI workspace untuk work item types + workflow (state & matriks transisi), project opt-in type, dan board hybrid (kolom `group` saat campuran, kolom state type saat difilter satu type) dengan dropdown state yang hanya menampilkan transisi valid.

**Architecture:** Data layer baru (`packages/types/src/workflow`, service, MobX `WorkflowStore`) mengonsumsi endpoint backend dari plan `2026-09-25-work-item-types-workflows-backend.md`. UI admin meniru pola settings Labels/States yang sudah ada. Board membaca `workflow-map` project untuk kolom dan allowed transitions.

**Tech Stack:** React Router v7 framework mode + React + MobX + `@plane/ui` (propel) + vitest. Catatan: `apps/web/vitest.config.ts` hanya menyertakan `core/**/*.test.ts` — test UI `.tsx` tidak dijalankan; karena itu test difokuskan ke helper murni.

**Spec:** `docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md`
**Prasyarat:** plan backend selesai (endpoint `/workflows/`, `/work-item-types/`, `workflow-map/`, `states` dengan `type_id`).

---

## File Structure

| File                                                                                                            | Aksi   | Tanggung jawab                                     |
| --------------------------------------------------------------------------------------------------------------- | ------ | -------------------------------------------------- |
| `packages/types/src/workflow/workflow.ts`                                                                       | Create | Tipe workflow, state, transisi, workflow-map       |
| `packages/types/src/workflow/work-item-type.ts`                                                                 | Create | Tipe work item type                                |
| `packages/types/src/workflow/index.ts`                                                                          | Create | Barrel domain                                      |
| `packages/types/src/index.ts`                                                                                   | Modify | Ekspor domain `workflow`                           |
| `packages/types/src/settings.ts`                                                                                | Modify | Tab settings workspace baru                        |
| `packages/constants/src/fetch-keys.ts`                                                                          | Modify | Fetch key workflow/types/map                       |
| `packages/constants/src/settings/workspace.ts`                                                                  | Modify | Kategori + item "Service management"               |
| `apps/web/core/services/workflow/workflow.service.ts`                                                           | Create | HTTP client workflow/type/map                      |
| `apps/web/core/services/workflow/index.ts`                                                                      | Create | Barrel service                                     |
| `apps/web/core/services/index.ts`                                                                               | Modify | Ekspor service baru (jika file ada)                |
| `apps/web/core/store/workflow.store.ts`                                                                         | Create | MobX store workflow/types/map                      |
| `apps/web/core/store/workflow.helpers.ts`                                                                       | Create | Helper murni (allowed transitions, kolom, matriks) |
| `apps/web/core/store/workflow.helpers.test.ts`                                                                  | Create | Vitest helper                                      |
| `apps/web/core/store/root.store.ts`                                                                             | Modify | Registrasi `workflow` + reset                      |
| `apps/web/core/hooks/store/use-workflow.ts`                                                                     | Create | Hook store                                         |
| `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`                                             | Modify | Ikon item baru                                     |
| `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/{page,header}.tsx`          | Create | Halaman admin type                                 |
| `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/{page,header}.tsx`                | Create | Halaman daftar workflow                            |
| `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/[workflowId]/{page,header}.tsx`   | Create | Halaman editor workflow                            |
| `apps/web/core/components/work-item-types/**`                                                                   | Create | Komponen CRUD type                                 |
| `apps/web/core/components/workflows/**`                                                                         | Create | Komponen daftar + editor workflow                  |
| `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/work-item-types/{page,header}.tsx` | Create | Halaman enable type di project                     |
| `apps/web/core/components/project-work-item-types/**`                                                           | Create | Komponen enable/disable type                       |
| `apps/web/core/components/dropdowns/state/base.tsx`                                                             | Modify | Filter allowed transitions                         |
| `apps/web/core/components/issues/issue-layouts/utils.tsx`                                                       | Modify | Kolom hybrid                                       |
| `packages/i18n/src/locales/en/workspace-settings.json` + locale lain                                            | Modify | String baru                                        |

**Urutan milestone:** W1 data layer → W2 admin UI workspace → W3 settings project → W4 board/views. W2-W4 memakai W1.

---

## Milestone W1 — Data layer

### Task W1.1: Types package

**Files:**

- Create: `packages/types/src/workflow/workflow.ts`
- Create: `packages/types/src/workflow/work-item-type.ts`
- Create: `packages/types/src/workflow/index.ts`
- Modify: `packages/types/src/index.ts`
- Modify: `packages/types/src/settings.ts`

- [ ] **Step 1: Tulis tipe**

```ts
// packages/types/src/workflow/workflow.ts
import type { TStateGroups } from "../state";

export type TWorkflow = {
  id: string;
  name: string;
  description: string;
  is_active: boolean;
  workspace_id: string;
  created_at: string;
  updated_at: string;
};

export type TWorkflowState = {
  id: string;
  workflow_id: string;
  name: string;
  description: string;
  color: string;
  slug: string;
  sequence: number;
  group: TStateGroups;
  is_default: boolean;
};

export type TWorkflowTransition = {
  id: string;
  workflow_id: string;
  from_state_id: string;
  to_state_id: string;
};

/** Mirror state di project (id = `State.id`, bukan `WorkflowState.id`). */
export type TWorkflowMapState = {
  id: string;
  name: string;
  color: string;
  group: TStateGroups;
  sequence: number;
  is_default: boolean;
};

export type TWorkflowMapTransition = {
  from_state_id: string;
  to_state_id: string;
};

export type TWorkflowMapType = {
  type_id: string;
  type_name: string;
  workflow_id: string;
  default_state_id: string | null;
  states: TWorkflowMapState[];
  transitions: TWorkflowMapTransition[];
};

export type TWorkflowMap = {
  types: TWorkflowMapType[];
};

export type TWorkflowPayload = {
  name: string;
  description?: string;
  is_active?: boolean;
};

export type TWorkflowStatePayload = {
  name?: string;
  description?: string;
  color?: string;
  group?: TStateGroups;
  sequence?: number;
  is_default?: boolean;
};
```

```ts
// packages/types/src/workflow/work-item-type.ts
export type TWorkItemType = {
  id: string;
  name: string;
  description: string;
  logo_props: Record<string, unknown>;
  is_epic: boolean;
  is_default: boolean;
  is_active: boolean;
  level: number;
  workflow: string | null;
  workspace: string;
  project_ids: string[];
  external_id: string | null;
  external_source: string | null;
  created_at: string;
  updated_at: string;
};

export type TWorkItemTypePayload = {
  name?: string;
  description?: string;
  is_active?: boolean;
  workflow?: string | null;
  project_ids?: string[];
};
```

```ts
// packages/types/src/workflow/index.ts
export * from "./workflow";
export * from "./work-item-type";
```

- [ ] **Step 2: Ekspor dari barrel + tab settings**

Di `packages/types/src/index.ts`, tambahkan `export * from "./workflow";` (dekat `export * from "./state";`).

Di `packages/types/src/settings.ts`, ubah `TWorkspaceSettingsTabs`:

```ts
export type TWorkspaceSettingsTabs = "general" | "members" | "export" | "webhooks" | "work_item_types" | "workflows";
```

- [ ] **Step 3: Typecheck**

Run: `pnpm check:types`

Expected: PASS (atau hanya error pre-existing yang tidak terkait).

- [ ] **Step 4: Commit**

```bash
git add packages/types/src/workflow packages/types/src/index.ts packages/types/src/settings.ts
git commit -m "feat(types): add workflow and work item type types"
```

---

### Task W1.2: Service layer

**Files:**

- Create: `apps/web/core/services/workflow/workflow.service.ts`
- Create: `apps/web/core/services/workflow/index.ts`
- Modify: `apps/web/core/services/index.ts` (jika ada — cek dengan `ls apps/web/core/services/index.ts`)

- [ ] **Step 1: Tulis service**

```ts
// apps/web/core/services/workflow/workflow.service.ts
import { API_BASE_URL } from "@plane/constants";
import type {
  TWorkflow,
  TWorkflowMap,
  TWorkflowPayload,
  TWorkflowState,
  TWorkflowStatePayload,
  TWorkflowTransition,
  TWorkItemType,
  TWorkItemTypePayload,
} from "@plane/types";
import { APIService } from "@/services/api.service";

export class WorkflowService extends APIService {
  constructor() {
    super(API_BASE_URL);
  }

  async getWorkflows(workspaceSlug: string): Promise<TWorkflow[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/workflows/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkflow(workspaceSlug: string, data: TWorkflowPayload): Promise<TWorkflow> {
    return this.post(`/api/workspaces/${workspaceSlug}/workflows/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateWorkflow(workspaceSlug: string, workflowId: string, data: Partial<TWorkflowPayload>): Promise<TWorkflow> {
    return this.patch(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkflow(workspaceSlug: string, workflowId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async getWorkflowStates(workspaceSlug: string, workflowId: string): Promise<TWorkflowState[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkflowState(
    workspaceSlug: string,
    workflowId: string,
    data: TWorkflowStatePayload
  ): Promise<TWorkflowState> {
    return this.post(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateWorkflowState(
    workspaceSlug: string,
    workflowId: string,
    stateId: string,
    data: Partial<TWorkflowStatePayload>
  ): Promise<TWorkflowState> {
    return this.patch(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/${stateId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkflowState(workspaceSlug: string, workflowId: string, stateId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/states/${stateId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async getWorkflowTransitions(workspaceSlug: string, workflowId: string): Promise<TWorkflowTransition[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/transitions/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkflowTransition(
    workspaceSlug: string,
    workflowId: string,
    data: { from_state_id: string; to_state_id: string }
  ): Promise<TWorkflowTransition> {
    return this.post(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/transitions/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkflowTransition(workspaceSlug: string, workflowId: string, transitionId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/workflows/${workflowId}/transitions/${transitionId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async getWorkItemTypes(workspaceSlug: string): Promise<TWorkItemType[]> {
    return this.get(`/api/workspaces/${workspaceSlug}/work-item-types/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async createWorkItemType(workspaceSlug: string, data: TWorkItemTypePayload): Promise<TWorkItemType> {
    return this.post(`/api/workspaces/${workspaceSlug}/work-item-types/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async updateWorkItemType(
    workspaceSlug: string,
    typeId: string,
    data: Partial<TWorkItemTypePayload>
  ): Promise<TWorkItemType> {
    return this.patch(`/api/workspaces/${workspaceSlug}/work-item-types/${typeId}/`, data)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async deleteWorkItemType(workspaceSlug: string, typeId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/work-item-types/${typeId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async importWorkItemTypes(workspaceSlug: string, projectId: string, typeIds: string[]): Promise<void> {
    return this.post(`/api/workspaces/${workspaceSlug}/projects/${projectId}/import-work-item-types/`, {
      work_item_types: typeIds,
    })
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async unlinkWorkItemType(workspaceSlug: string, projectId: string, typeId: string): Promise<void> {
    return this.delete(`/api/workspaces/${workspaceSlug}/projects/${projectId}/work-item-types/${typeId}/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }

  async getWorkflowMap(workspaceSlug: string, projectId: string): Promise<TWorkflowMap> {
    return this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/workflow-map/`)
      .then((response) => response?.data)
      .catch((error) => {
        throw error?.response?.data;
      });
  }
}
```

```ts
// apps/web/core/services/workflow/index.ts
export * from "./workflow.service";
```

- [ ] **Step 2: Ekspor service**

Jika `apps/web/core/services/index.ts` ada, tambahkan `export * from "./workflow";`. Jika tidak ada, lewati (service diimpor langsung).

- [ ] **Step 3: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/services/workflow apps/web/core/services/index.ts
git commit -m "feat(web): workflow service layer"
```

---

### Task W1.3: Fetch keys + settings constants

**Files:**

- Modify: `packages/constants/src/fetch-keys.ts`
- Modify: `packages/constants/src/settings/workspace.ts`
- Modify: `apps/web/core/components/settings/workspace/sidebar/item-icon.tsx`

- [ ] **Step 1: Fetch keys**

Tambahkan ke `packages/constants/src/fetch-keys.ts` (dekat key workflow yang sudah ada):

```ts
export const WORKSPACE_WORKFLOWS = (workspaceSlug: string) => `WORKSPACE_WORKFLOWS_${workspaceSlug.toUpperCase()}`;
export const WORKSPACE_WORK_ITEM_TYPES = (workspaceSlug: string) =>
  `WORKSPACE_WORK_ITEM_TYPES_${workspaceSlug.toUpperCase()}`;
export const PROJECT_WORKFLOW_MAP = (projectId: string) => `PROJECT_WORKFLOW_MAP_${projectId.toUpperCase()}`;
```

- [ ] **Step 2: Settings constants**

Di `packages/constants/src/settings/workspace.ts`:

1. Tambahkan kategori:

```ts
export enum WORKSPACE_SETTINGS_CATEGORY {
  ADMINISTRATION = "administration",
  SERVICE_MANAGEMENT = "service-management",
  FEATURES = "features",
  DEVELOPER = "developer",
}
```

2. Tambahkan ke `WORKSPACE_SETTINGS_CATEGORIES` setelah ADMINISTRATION:

```ts
  WORKSPACE_SETTINGS_CATEGORY.SERVICE_MANAGEMENT,
```

3. Tambahkan label:

```ts
  [WORKSPACE_SETTINGS_CATEGORY.SERVICE_MANAGEMENT]: "common.service_management",
```

4. Tambahkan dua item ke `WORKSPACE_SETTINGS`:

```ts
  work_item_types: {
    key: "work_item_types",
    i18n_label: "workspace_settings.settings.work_item_types.title",
    href: `/settings/work-item-types`,
    access: [EUserWorkspaceRoles.ADMIN],
    highlight: (pathname: string, baseUrl: string) => pathname === `${baseUrl}/work-item-types/`,
  },
  workflows: {
    key: "workflows",
    i18n_label: "workspace_settings.settings.workflows.title",
    href: `/settings/workflows`,
    access: [EUserWorkspaceRoles.ADMIN],
    highlight: (pathname: string, baseUrl: string) => pathname.startsWith(`${baseUrl}/workflows/`),
  },
```

5. Tambahkan ke `GROUPED_WORKSPACE_SETTINGS`:

```ts
  [WORKSPACE_SETTINGS_CATEGORY.SERVICE_MANAGEMENT]: [
    WORKSPACE_SETTINGS["work_item_types"],
    WORKSPACE_SETTINGS["workflows"],
  ],
```

- [ ] **Step 3: Ikon sidebar**

Di `item-icon.tsx`, tambahkan dua entri (pakai ikon propel yang sudah ada; cek nama valid dengan `rg "export.*Outline" packages/propel/src/icons`):

```ts
  work_item_types: LayersOutline,
  workflows: WorkflowOutline,
```

Ganti nama ikon dengan yang benar-benar tersedia. Jika belum ada ikon yang cocok, pakai ikon yang sudah dipakai item lain (mis. `MembersOutline`).

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/constants/src/fetch-keys.ts packages/constants/src/settings/workspace.ts apps/web/core/components/settings/workspace/sidebar/item-icon.tsx
git commit -m "feat(web): workspace settings entries for types and workflows"
```

---

### Task W1.4: Helper murni + test

**Files:**

- Create: `apps/web/core/store/workflow.helpers.ts`
- Create: `apps/web/core/store/workflow.helpers.test.ts`

- [ ] **Step 1: Tulis test yang gagal**

```ts
// apps/web/core/store/workflow.helpers.test.ts
import { describe, expect, it } from "vitest";
import type { IState, TWorkflowMapType } from "@plane/types";
import {
  allowedTargetStateIds,
  buildTransitionMatrix,
  findWorkflowMapType,
  resolveStateColumns,
} from "./workflow.helpers";

const mapType: TWorkflowMapType = {
  type_id: "type-1",
  type_name: "Incident",
  workflow_id: "wf-1",
  default_state_id: "s-new",
  states: [
    { id: "s-new", name: "New", color: "#60646C", group: "backlog", sequence: 1, is_default: true },
    { id: "s-progress", name: "In Progress", color: "#F59E0B", group: "started", sequence: 2, is_default: false },
    { id: "s-closed", name: "Closed", color: "#46A758", group: "completed", sequence: 3, is_default: false },
  ],
  transitions: [
    { from_state_id: "s-new", to_state_id: "s-progress" },
    { from_state_id: "s-progress", to_state_id: "s-closed" },
  ],
};

describe("allowedTargetStateIds", () => {
  it("mengikuti transisi dari state sekarang", () => {
    expect(allowedTargetStateIds(mapType, "s-new")).toEqual(["s-progress"]);
    expect(allowedTargetStateIds(mapType, "s-progress")).toEqual(["s-closed"]);
    expect(allowedTargetStateIds(mapType, "s-closed")).toEqual([]);
  });

  it("state di luar workflow hanya boleh pindah ke default", () => {
    expect(allowedTargetStateIds(mapType, "legacy-state")).toEqual(["s-new"]);
  });

  it("tanpa map mengembalikan kosong", () => {
    expect(allowedTargetStateIds(undefined, "s-new")).toEqual([]);
  });
});

describe("resolveStateColumns", () => {
  it("memakai urutan + metadata mirror saat map ada", () => {
    const projectStates: IState[] = [
      {
        id: "s-closed",
        name: "Closed",
        color: "#000000",
        group: "completed",
        description: "",
        sequence: 30,
        workspace_id: "w",
        project_id: "p",
      } as IState,
      {
        id: "s-new",
        name: "New",
        color: "#000000",
        group: "backlog",
        description: "",
        sequence: 10,
        workspace_id: "w",
        project_id: "p",
      } as IState,
    ];
    const columns = resolveStateColumns(projectStates, mapType);
    expect(columns.map((c) => c.id)).toEqual(["s-new", "s-closed"]);
    expect(columns[0].color).toBe("#60646C");
  });

  it("tanpa map mengembalikan state apa adanya", () => {
    const projectStates = [{ id: "s-1" } as IState];
    expect(resolveStateColumns(projectStates, undefined)).toBe(projectStates);
  });
});

describe("buildTransitionMatrix", () => {
  it("membuat semua pasangan kecuali self dan menandai yang ada", () => {
    const matrix = buildTransitionMatrix([{ id: "a" }, { id: "b" }], [{ from_state_id: "a", to_state_id: "b" }]);
    expect(matrix).toEqual([
      { from_state_id: "a", to_state_id: "b", exists: true },
      { from_state_id: "b", to_state_id: "a", exists: false },
    ]);
  });
});

describe("findWorkflowMapType", () => {
  it("menemukan type dari map", () => {
    expect(findWorkflowMapType({ types: [mapType] }, "type-1")?.type_name).toBe("Incident");
    expect(findWorkflowMapType({ types: [mapType] }, null)).toBeUndefined();
  });
});
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `pnpm --filter=web exec vitest run core/store/workflow.helpers.test.ts`

Expected: FAIL — modul belum ada.

- [ ] **Step 3: Implementasi helper**

```ts
// apps/web/core/store/workflow.helpers.ts
import type { IState, TWorkflowMap, TWorkflowMapType } from "@plane/types";

export const findWorkflowMapType = (
  map: TWorkflowMap | undefined,
  typeId: string | null | undefined
): TWorkflowMapType | undefined => (typeId ? map?.types.find((type) => type.type_id === typeId) : undefined);

/** State tujuan yang diizinkan dari `currentStateId` (mirror ids). */
export const allowedTargetStateIds = (
  mapType: TWorkflowMapType | undefined,
  currentStateId: string | null | undefined
): string[] => {
  if (!mapType) return [];
  const defaultIds = mapType.default_state_id ? [mapType.default_state_id] : [];
  if (!currentStateId) return defaultIds;
  const current = mapType.states.find((state) => state.id === currentStateId);
  if (!current) return defaultIds;
  const allowed = new Set(
    mapType.transitions.filter((transition) => transition.from_state_id === currentStateId).map((t) => t.to_state_id)
  );
  return mapType.states.filter((state) => allowed.has(state.id)).map((state) => state.id);
};

/** Kolom kanban untuk satu type: urut `sequence` mirror, metadata mirror. */
export const resolveStateColumns = (projectStates: IState[], mapType: TWorkflowMapType | undefined): IState[] => {
  if (!mapType) return projectStates;
  return [...mapType.states]
    .sort((a, b) => a.sequence - b.sequence)
    .map((mirror) => {
      const state = projectStates.find((candidate) => candidate.id === mirror.id);
      return state ? { ...state, name: mirror.name, color: mirror.color, group: mirror.group } : undefined;
    })
    .filter((state): state is IState => Boolean(state));
};

/** Pasangan from → to (tanpa self) + status ada/tidak, untuk matriks transisi. */
export const buildTransitionMatrix = (
  states: { id: string }[],
  transitions: { from_state_id: string; to_state_id: string }[]
): { from_state_id: string; to_state_id: string; exists: boolean }[] => {
  const existing = new Set(transitions.map((transition) => `${transition.from_state_id}:${transition.to_state_id}`));
  return states.flatMap((from) =>
    states
      .filter((to) => to.id !== from.id)
      .map((to) => ({
        from_state_id: from.id,
        to_state_id: to.id,
        exists: existing.has(`${from.id}:${to.id}`),
      }))
  );
};
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `pnpm --filter=web exec vitest run core/store/workflow.helpers.test.ts`

Expected: PASS (6 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/workflow.helpers.ts apps/web/core/store/workflow.helpers.test.ts
git commit -m "feat(web): workflow helper functions with tests"
```

---

### Task W1.5: MobX store + hook + registrasi root

**Files:**

- Create: `apps/web/core/store/workflow.store.ts`
- Create: `apps/web/core/hooks/store/use-workflow.ts`
- Modify: `apps/web/core/store/root.store.ts`

- [ ] **Step 1: Tulis store**

```ts
// apps/web/core/store/workflow.store.ts
import { action, makeObservable, observable, runInAction } from "mobx";
import type {
  TWorkflow,
  TWorkflowMap,
  TWorkflowPayload,
  TWorkflowState,
  TWorkflowStatePayload,
  TWorkflowTransition,
  TWorkItemType,
  TWorkItemTypePayload,
} from "@plane/types";
import { WorkflowService } from "@/services/workflow";
import type { CoreRootStore } from "./root.store";

export interface IWorkflowStore {
  workflows: TWorkflow[] | undefined;
  workItemTypes: TWorkItemType[] | undefined;
  workflowStates: Record<string, TWorkflowState[]>;
  workflowTransitions: Record<string, TWorkflowTransition[]>;
  workflowMap: Record<string, TWorkflowMap>;
  fetchWorkflows(workspaceSlug: string): Promise<TWorkflow[]>;
  createWorkflow(workspaceSlug: string, data: TWorkflowPayload): Promise<TWorkflow>;
  updateWorkflow(workspaceSlug: string, workflowId: string, data: Partial<TWorkflowPayload>): Promise<TWorkflow>;
  deleteWorkflow(workspaceSlug: string, workflowId: string): Promise<void>;
  fetchWorkflowStates(workspaceSlug: string, workflowId: string): Promise<TWorkflowState[]>;
  createWorkflowState(workspaceSlug: string, workflowId: string, data: TWorkflowStatePayload): Promise<TWorkflowState>;
  updateWorkflowState(
    workspaceSlug: string,
    workflowId: string,
    stateId: string,
    data: Partial<TWorkflowStatePayload>
  ): Promise<TWorkflowState>;
  deleteWorkflowState(workspaceSlug: string, workflowId: string, stateId: string): Promise<void>;
  fetchWorkflowTransitions(workspaceSlug: string, workflowId: string): Promise<TWorkflowTransition[]>;
  createWorkflowTransition(
    workspaceSlug: string,
    workflowId: string,
    data: { from_state_id: string; to_state_id: string }
  ): Promise<TWorkflowTransition>;
  deleteWorkflowTransition(workspaceSlug: string, workflowId: string, transitionId: string): Promise<void>;
  fetchWorkItemTypes(workspaceSlug: string): Promise<TWorkItemType[]>;
  createWorkItemType(workspaceSlug: string, data: TWorkItemTypePayload): Promise<TWorkItemType>;
  updateWorkItemType(
    workspaceSlug: string,
    typeId: string,
    data: Partial<TWorkItemTypePayload>
  ): Promise<TWorkItemType>;
  deleteWorkItemType(workspaceSlug: string, typeId: string): Promise<void>;
  importWorkItemTypes(workspaceSlug: string, projectId: string, typeIds: string[]): Promise<void>;
  unlinkWorkItemType(workspaceSlug: string, projectId: string, typeId: string): Promise<void>;
  fetchWorkflowMap(workspaceSlug: string, projectId: string): Promise<TWorkflowMap>;
  getWorkflowMap(projectId: string): TWorkflowMap | undefined;
}

export class WorkflowStore implements IWorkflowStore {
  workflows: TWorkflow[] | undefined = undefined;
  workItemTypes: TWorkItemType[] | undefined = undefined;
  workflowStates: Record<string, TWorkflowState[]> = {};
  workflowTransitions: Record<string, TWorkflowTransition[]> = {};
  workflowMap: Record<string, TWorkflowMap> = {};
  private service = new WorkflowService();

  constructor(private _rootStore: CoreRootStore) {
    makeObservable(this, {
      workflows: observable,
      workItemTypes: observable,
      workflowStates: observable,
      workflowTransitions: observable,
      workflowMap: observable,
      fetchWorkflows: action,
      createWorkflow: action,
      updateWorkflow: action,
      deleteWorkflow: action,
      fetchWorkflowStates: action,
      createWorkflowState: action,
      updateWorkflowState: action,
      deleteWorkflowState: action,
      fetchWorkflowTransitions: action,
      createWorkflowTransition: action,
      deleteWorkflowTransition: action,
      fetchWorkItemTypes: action,
      createWorkItemType: action,
      updateWorkItemType: action,
      deleteWorkItemType: action,
      importWorkItemTypes: action,
      unlinkWorkItemType: action,
      fetchWorkflowMap: action,
    });
  }

  fetchWorkflows = async (workspaceSlug: string) => {
    const workflows = await this.service.getWorkflows(workspaceSlug);
    runInAction(() => {
      this.workflows = workflows;
    });
    return workflows;
  };

  createWorkflow = async (workspaceSlug: string, data: TWorkflowPayload) => {
    const workflow = await this.service.createWorkflow(workspaceSlug, data);
    runInAction(() => {
      this.workflows = [...(this.workflows ?? []), workflow];
    });
    return workflow;
  };

  updateWorkflow = async (workspaceSlug: string, workflowId: string, data: Partial<TWorkflowPayload>) => {
    const workflow = await this.service.updateWorkflow(workspaceSlug, workflowId, data);
    runInAction(() => {
      this.workflows = this.workflows?.map((item) => (item.id === workflowId ? workflow : item));
    });
    return workflow;
  };

  deleteWorkflow = async (workspaceSlug: string, workflowId: string) => {
    await this.service.deleteWorkflow(workspaceSlug, workflowId);
    runInAction(() => {
      this.workflows = this.workflows?.filter((item) => item.id !== workflowId);
      delete this.workflowStates[workflowId];
      delete this.workflowTransitions[workflowId];
    });
  };

  fetchWorkflowStates = async (workspaceSlug: string, workflowId: string) => {
    const states = await this.service.getWorkflowStates(workspaceSlug, workflowId);
    runInAction(() => {
      this.workflowStates[workflowId] = states;
    });
    return states;
  };

  createWorkflowState = async (workspaceSlug: string, workflowId: string, data: TWorkflowStatePayload) => {
    const state = await this.service.createWorkflowState(workspaceSlug, workflowId, data);
    runInAction(() => {
      this.workflowStates[workflowId] = [...(this.workflowStates[workflowId] ?? []), state];
    });
    return state;
  };

  updateWorkflowState = async (
    workspaceSlug: string,
    workflowId: string,
    stateId: string,
    data: Partial<TWorkflowStatePayload>
  ) => {
    const state = await this.service.updateWorkflowState(workspaceSlug, workflowId, stateId, data);
    runInAction(() => {
      this.workflowStates[workflowId] = (this.workflowStates[workflowId] ?? []).map((item) =>
        item.id === stateId ? state : item
      );
    });
    return state;
  };

  deleteWorkflowState = async (workspaceSlug: string, workflowId: string, stateId: string) => {
    await this.service.deleteWorkflowState(workspaceSlug, workflowId, stateId);
    runInAction(() => {
      this.workflowStates[workflowId] = (this.workflowStates[workflowId] ?? []).filter((item) => item.id !== stateId);
      this.workflowTransitions[workflowId] = (this.workflowTransitions[workflowId] ?? []).filter(
        (transition) => transition.from_state_id !== stateId && transition.to_state_id !== stateId
      );
    });
  };

  fetchWorkflowTransitions = async (workspaceSlug: string, workflowId: string) => {
    const transitions = await this.service.getWorkflowTransitions(workspaceSlug, workflowId);
    runInAction(() => {
      this.workflowTransitions[workflowId] = transitions;
    });
    return transitions;
  };

  createWorkflowTransition = async (
    workspaceSlug: string,
    workflowId: string,
    data: { from_state_id: string; to_state_id: string }
  ) => {
    const transition = await this.service.createWorkflowTransition(workspaceSlug, workflowId, data);
    runInAction(() => {
      this.workflowTransitions[workflowId] = [...(this.workflowTransitions[workflowId] ?? []), transition];
    });
    return transition;
  };

  deleteWorkflowTransition = async (workspaceSlug: string, workflowId: string, transitionId: string) => {
    await this.service.deleteWorkflowTransition(workspaceSlug, workflowId, transitionId);
    runInAction(() => {
      this.workflowTransitions[workflowId] = (this.workflowTransitions[workflowId] ?? []).filter(
        (item) => item.id !== transitionId
      );
    });
  };

  fetchWorkItemTypes = async (workspaceSlug: string) => {
    const types = await this.service.getWorkItemTypes(workspaceSlug);
    runInAction(() => {
      this.workItemTypes = types;
    });
    return types;
  };

  createWorkItemType = async (workspaceSlug: string, data: TWorkItemTypePayload) => {
    const type = await this.service.createWorkItemType(workspaceSlug, data);
    runInAction(() => {
      this.workItemTypes = [...(this.workItemTypes ?? []), type];
    });
    return type;
  };

  updateWorkItemType = async (workspaceSlug: string, typeId: string, data: Partial<TWorkItemTypePayload>) => {
    const type = await this.service.updateWorkItemType(workspaceSlug, typeId, data);
    runInAction(() => {
      this.workItemTypes = this.workItemTypes?.map((item) => (item.id === typeId ? type : item));
    });
    return type;
  };

  deleteWorkItemType = async (workspaceSlug: string, typeId: string) => {
    await this.service.deleteWorkItemType(workspaceSlug, typeId);
    runInAction(() => {
      this.workItemTypes = this.workItemTypes?.filter((item) => item.id !== typeId);
    });
  };

  importWorkItemTypes = async (workspaceSlug: string, projectId: string, typeIds: string[]) => {
    await this.service.importWorkItemTypes(workspaceSlug, projectId, typeIds);
    await this.fetchWorkflowMap(workspaceSlug, projectId);
  };

  unlinkWorkItemType = async (workspaceSlug: string, projectId: string, typeId: string) => {
    await this.service.unlinkWorkItemType(workspaceSlug, projectId, typeId);
    await this.fetchWorkflowMap(workspaceSlug, projectId);
  };

  fetchWorkflowMap = async (workspaceSlug: string, projectId: string) => {
    const map = await this.service.getWorkflowMap(workspaceSlug, projectId);
    runInAction(() => {
      this.workflowMap[projectId] = map;
    });
    return map;
  };

  getWorkflowMap = (projectId: string) => this.workflowMap[projectId];
}
```

- [ ] **Step 2: Hook**

```ts
// apps/web/core/hooks/store/use-workflow.ts
import { useContext } from "react";
import { StoreContext } from "@/lib/store-context";
import type { IWorkflowStore } from "@/store/workflow.store";

export const useWorkflow = (): IWorkflowStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useWorkflow must be used within StoreProvider");
  return context.workflow;
};
```

- [ ] **Step 3: Registrasi root store**

Di `apps/web/core/store/root.store.ts`:

1. Import: `import { WorkflowStore, type IWorkflowStore } from "./workflow.store";`
2. Field: `workflow: IWorkflowStore;`
3. Constructor: `this.workflow = new WorkflowStore(this);`
4. Di `resetOnSignOut()`: `this.workflow = new WorkflowStore(this);`

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/workflow.store.ts apps/web/core/hooks/store/use-workflow.ts apps/web/core/store/root.store.ts
git commit -m "feat(web): workflow mobx store"
```

---

## Milestone W2 — Admin UI workspace

### Task W2.1: i18n keys

**Files:**

- Modify: `packages/i18n/src/locales/en/common.json`
- Modify: `packages/i18n/src/locales/en/workspace-settings.json`
- Modify: 19 locale lain (via skill translate)

- [ ] **Step 1: Tambah key `en`**

Di `en/common.json`, tambahkan:

```json
"service_management": "Service management",
```

Di `en/workspace-settings.json`, tambahkan `work_item_types` dan `workflows` **ke dalam objek `settings` yang sudah ada** (jangan membuat key `settings` baru):

```json
  "work_item_types": {
    "title": "Work item types",
    "heading": "Work item types",
    "description": "Define the ITSM work item types available across the workspace and attach a workflow to each.",
    "add_type": "Add type",
    "empty_state": {
      "title": "No work item types yet",
      "description": "Create Incident, Problem, Change, or Improvement types and attach a workflow."
    },
    "form": {
      "name": "Name",
      "description": "Description",
      "workflow": "Workflow",
      "active": "Active"
    }
  },
  "workflows": {
    "title": "Workflows",
    "heading": "Workflows",
    "description": "Define states and the allowed transitions between them for each work item type.",
    "add_workflow": "Add workflow",
    "empty_state": {
      "title": "No workflows yet",
      "description": "Create a workflow, then add states and allowed transitions."
    },
    "form": {
      "name": "Name",
      "description": "Description",
      "active": "Active"
    },
    "states": {
      "heading": "States",
      "add_state": "Add state",
      "name": "Name",
      "color": "Color",
      "group": "Group",
      "default": "Default",
      "set_default": "Set as default"
    },
    "transitions": {
      "heading": "Allowed transitions",
      "description": "Check every from → to pair that is allowed. Unchecked pairs are blocked.",
      "from": "From",
      "to": "To"
    }
  }
```

- [ ] **Step 2: Terjemahkan mengikuti skill translate**

Baca `.claude/skills/translate/SKILL.md`, lalu tambahkan key yang sama (bukan salinan Inggris) ke 19 locale lain. Jalankan:

```bash
pnpm --filter @plane/i18n run generate:types
pnpm --filter @plane/i18n run sync:check
```

Expected: `sync:check` PASS (semua locale punya key yang sama).

- [ ] **Step 3: Commit**

```bash
git add packages/i18n/src/locales packages/i18n/src/types/keys.generated.ts
git commit -m "feat(i18n): workspace settings strings for types and workflows"
```

---

### Task W2.2: Halaman workspace "Work item types"

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/page.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/header.tsx`
- Create: `apps/web/core/components/work-item-types/root.tsx`
- Create: `apps/web/core/components/work-item-types/type-list-item.tsx`
- Create: `apps/web/core/components/work-item-types/type-form-modal.tsx`
- Create: `apps/web/core/components/work-item-types/delete-type-modal.tsx`
- Create: `apps/web/core/components/work-item-types/index.ts`

- [ ] **Step 1: Page + header (tiru webhooks)**

`header.tsx` sama persis pola `.../webhooks/header.tsx`, ganti key ke `WORKSPACE_SETTINGS.work_item_types` dan ikon `WORKSPACE_SETTINGS_ICONS.work_item_types`.

`page.tsx`:

```tsx
// apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/page.tsx
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { PageHead } from "@/components/core/page-title";
import { NotAuthorizedView } from "@/components/auth-screens/not-authorized-view";
import { SettingsContentWrapper } from "@/components/settings/content-wrapper";
import { WorkItemTypesRoot } from "@/components/work-item-types";
import { useUserPermissions } from "@/hooks/store/user";
import { WorkItemTypesWorkspaceSettingsHeader } from "./header";

function WorkItemTypesSettingsPage() {
  const { workspaceSlug } = useParams();
  const { t } = useTranslation();
  const { workspaceUserInfo, allowPermissions } = useUserPermissions();
  const canManage = allowPermissions([EUserPermissions.ADMIN], EUserPermissionsLevel.WORKSPACE);

  if (workspaceUserInfo && !canManage)
    return <NotAuthorizedView section="settings" isProjectView={false} className="h-auto" />;

  return (
    <SettingsContentWrapper header={<WorkItemTypesWorkspaceSettingsHeader />}>
      <PageHead title={t("workspace_settings.settings.work_item_types.title")} />
      {workspaceSlug && <WorkItemTypesRoot workspaceSlug={workspaceSlug.toString()} />}
    </SettingsContentWrapper>
  );
}

export default observer(WorkItemTypesSettingsPage);
```

- [ ] **Step 2: Root + list + modal**

`root.tsx`:

```tsx
// apps/web/core/components/work-item-types/root.tsx
import { observer } from "mobx-react";
import { useEffect, useState } from "react";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/ui";
import { SettingsHeading } from "@/components/settings/heading";
import { useWorkflow } from "@/hooks/store/use-workflow";
import { TypeFormModal } from "./type-form-modal";
import { TypeListItem } from "./type-list-item";
import { DeleteTypeModal } from "./delete-type-modal";

type Props = { workspaceSlug: string };

export const WorkItemTypesRoot = observer(function WorkItemTypesRoot({ workspaceSlug }: Props) {
  const { t } = useTranslation();
  const { workItemTypes, workflows, fetchWorkItemTypes, fetchWorkflows } = useWorkflow();
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingTypeId, setEditingTypeId] = useState<string | null>(null);
  const [deletingTypeId, setDeletingTypeId] = useState<string | null>(null);

  useEffect(() => {
    void fetchWorkItemTypes(workspaceSlug);
    void fetchWorkflows(workspaceSlug);
  }, [workspaceSlug, fetchWorkItemTypes, fetchWorkflows]);

  return (
    <>
      <TypeFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        typeId={editingTypeId}
        onClose={() => {
          setIsFormOpen(false);
          setEditingTypeId(null);
        }}
      />
      <DeleteTypeModal
        workspaceSlug={workspaceSlug}
        isOpen={Boolean(deletingTypeId)}
        typeId={deletingTypeId}
        onClose={() => setDeletingTypeId(null)}
      />
      <SettingsHeading
        title={t("workspace_settings.settings.work_item_types.heading")}
        description={t("workspace_settings.settings.work_item_types.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditingTypeId(null);
              setIsFormOpen(true);
            }}
          >
            {t("workspace_settings.settings.work_item_types.add_type")}
          </Button>
        }
      />
      <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
        {workItemTypes?.map((type) => (
          <TypeListItem
            key={type.id}
            type={type}
            workflowName={workflows?.find((workflow) => workflow.id === type.workflow)?.name}
            onEdit={() => {
              setEditingTypeId(type.id);
              setIsFormOpen(true);
            }}
            onDelete={() => setDeletingTypeId(type.id)}
          />
        ))}
      </div>
    </>
  );
});
```

`type-list-item.tsx`: baris dengan nama, deskripsi, nama workflow (atau "No workflow"), badge `is_epic`/`is_active`, tombol Edit/Hapus. Gunakan `DropdownMenu`/`Button` dari `@plane/ui` seperti `project-setting-label-item.tsx`.

`type-form-modal.tsx`: `ModalCore` + `Controller` react-hook-form (name, description) + `CustomSelect` workflow (opsi: `{ value: workflow.id, label: workflow.name }`) + `ToggleSwitch` is_active. Submit:

```tsx
const onSubmit = async (formData: TWorkItemTypePayload) => {
  try {
    if (typeId) await updateWorkItemType(workspaceSlug, typeId, formData);
    else await createWorkItemType(workspaceSlug, formData);
    setToast({ type: TOAST_TYPE.SUCCESS, message: "Saved" });
    onClose();
  } catch (error: any) {
    setToast({ type: TOAST_TYPE.ERROR, message: error?.error ?? "Something went wrong" });
  }
};
```

`delete-type-modal.tsx`: `AlertModalCore` pola `delete-label-modal.tsx`; submit `deleteWorkItemType(workspaceSlug, typeId)`; tampilkan pesan error API (mis. "Type is in use by work items").

`index.ts`: `export * from "./root"; export * from "./type-form-modal"; export * from "./delete-type-modal";`

- [ ] **Step 3: Jalankan typecheck + lint**

Run: `pnpm check:types && pnpm check:lint`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/app/\(all\)/\[workspaceSlug\]/\(settings\)/settings/\(workspace\)/work-item-types apps/web/core/components/work-item-types
git commit -m "feat(web): workspace work item types settings page"
```

---

### Task W2.3: Halaman daftar + editor workflow

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/{page,header}.tsx`
- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows/[workflowId]/{page,header}.tsx`
- Create: `apps/web/core/components/workflows/{root,workflow-list,workflow-form-modal,workflow-editor,state-list,state-form-modal,transition-matrix}.tsx`
- Create: `apps/web/core/components/workflows/index.ts`

- [ ] **Step 1: Halaman daftar**

`workflows/page.tsx` + `header.tsx` mengikuti pola `work-item-types`, render `WorkflowsRoot`.

`workflow-list.tsx`: list workflow (nama, deskripsi, jumlah state, badge aktif) + tombol "Add workflow" (modal name/description) + aksi Edit/Hapus. Setiap item `<Link href={`/${workspaceSlug}/settings/workflows/${workflow.id}`}>` ke editor.

`workflow-form-modal.tsx`: modal name + description; submit `createWorkflow`/`updateWorkflow`.

- [ ] **Step 2: Halaman editor**

`workflows/[workflowId]/page.tsx`:

```tsx
function WorkflowEditorPage() {
  const { workspaceSlug, workflowId } = useParams();
  return (
    <SettingsContentWrapper header={<WorkflowEditorHeader />}>
      <PageHead title={t("workspace_settings.settings.workflows.title")} />
      {workspaceSlug && workflowId && (
        <WorkflowEditor workspaceSlug={workspaceSlug.toString()} workflowId={workflowId.toString()} />
      )}
    </SettingsContentWrapper>
  );
}
```

`header.tsx` editor: breadcrumb `Workflows / <nama workflow>` dengan link balik ke daftar.

- [ ] **Step 3: Editor (states + transisi)**

`workflow-editor.tsx`:

```tsx
export const WorkflowEditor = observer(function WorkflowEditor({ workspaceSlug, workflowId }: Props) {
  const { workflowStates, workflowTransitions, fetchWorkflowStates, fetchWorkflowTransitions } = useWorkflow();
  const states = workflowStates[workflowId];
  const transitions = workflowTransitions[workflowId];

  useEffect(() => {
    void fetchWorkflowStates(workspaceSlug, workflowId);
    void fetchWorkflowTransitions(workspaceSlug, workflowId);
  }, [workspaceSlug, workflowId, fetchWorkflowStates, fetchWorkflowTransitions]);

  return (
    <div className="mt-6 grid grid-cols-1 gap-8 lg:grid-cols-2">
      <StateList workspaceSlug={workspaceSlug} workflowId={workflowId} states={states ?? []} />
      <TransitionMatrix
        workspaceSlug={workspaceSlug}
        workflowId={workflowId}
        states={states ?? []}
        transitions={transitions ?? []}
      />
    </div>
  );
});
```

`state-list.tsx`: daftar `StateGroupIcon` + nama + badge Default; tombol tambah (modal), edit, hapus, "Set as default" (update `{ is_default: true }`), dan tombol naik/turun urutan (update `sequence` dengan nilai antar tetangga, mis. `(prev + next) / 2`).

`state-form-modal.tsx`: name, description, color (`TwitterPicker` seperti `create-update/form.tsx` project-states), group (`CustomSelect` dari `STATE_GROUPS`), `is_default` hanya ditampilkan sebagai info (default diatur lewat aksi terpisah). Submit `createWorkflowState`/`updateWorkflowState`; tampilkan error API.

`transition-matrix.tsx`:

```tsx
export const TransitionMatrix = observer(function TransitionMatrix({
  workspaceSlug,
  workflowId,
  states,
  transitions,
}: Props) {
  const { createWorkflowTransition, deleteWorkflowTransition } = useWorkflow();
  const matrix = buildTransitionMatrix(states, transitions);

  const toggle = async (fromId: string, toId: string, exists: boolean) => {
    try {
      if (exists) {
        const transition = transitions.find((item) => item.from_state_id === fromId && item.to_state_id === toId);
        if (transition) await deleteWorkflowTransition(workspaceSlug, workflowId, transition.id);
      } else {
        await createWorkflowTransition(workspaceSlug, workflowId, { from_state_id: fromId, to_state_id: toId });
      }
    } catch (error: any) {
      setToast({ type: TOAST_TYPE.ERROR, message: error?.error ?? "Something went wrong" });
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <h4 className="text-14 font-medium">{t("workspace_settings.settings.workflows.transitions.heading")}</h4>
      <div className="grid gap-2" style={{ gridTemplateColumns: `120px repeat(${states.length}, 1fr)` }}>
        <span />
        {states.map((state) => (
          <span key={state.id} className="truncate text-caption-md-medium text-tertiary">
            {state.name}
          </span>
        ))}
        {states.map((from) => (
          <Fragment key={from.id}>
            <span className="truncate text-caption-md-medium text-tertiary">{from.name}</span>
            {states.map((to) => {
              const cell = matrix.find((item) => item.from_state_id === from.id && item.to_state_id === to.id);
              if (!cell) return <span key={to.id} />;
              return (
                <button
                  key={to.id}
                  type="button"
                  className={`h-6 w-6 rounded border ${cell.exists ? "border-accent-strong bg-accent-primary" : "border-subtle"}`}
                  onClick={() => void toggle(from.id, to.id, cell.exists)}
                />
              );
            })}
          </Fragment>
        ))}
      </div>
    </div>
  );
});
```

- [ ] **Step 4: Typecheck + test helper**

Run: `pnpm check:types && pnpm --filter=web exec vitest run core/store/workflow.helpers.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/workflows" apps/web/core/components/workflows
git commit -m "feat(web): workflow list and editor"
```

---

## Milestone W3 — Settings project

### Task W3.1: Feature toggle "Work item types"

**Files:**

- Modify: halaman feature project yang relevan (hasil discovery)

- [ ] **Step 1: Discovery**

Run:

```bash
rg -n "is_issue_type_enabled|work_item_types" apps/web/core/services/project apps/web/core/store/project apps/web/core/components/settings/project -g '*.ts*'
rg -n "features/(cycles|modules|views|pages|intake)" "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]" -g '*.tsx' | head
```

Catat: nama service/store untuk update feature project dan halaman toggle yang menjadi pola (mis. `features/intake`).

- [ ] **Step 2: Implementasi toggle**

Tambahkan toggle "Work item types" di halaman feature project (mengikuti pola feature lain) yang memanggil update feature `work_item_types` (kolom `is_issue_type_enabled`). Jika service project memakai endpoint `PATCH /api/workspaces/:slug/projects/:project_id/features/`, pastikan key body = `work_item_types`.

- [ ] **Step 3: Verifikasi manual**

Run dev web, buka Settings → Project → Features, toggle "Work item types", refresh, pastikan nilai persist (cek response API di network tab).

- [ ] **Step 4: Commit**

```bash
git add <file-yang-diubah>
git commit -m "feat(web): project feature toggle for work item types"
```

---

### Task W3.2: Halaman project "Work item types" (enable/disable)

**Files:**

- Create: `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/work-item-types/{page,header}.tsx`
- Create: `apps/web/core/components/project-work-item-types/{root,type-toggle-item}.tsx`
- Create: `apps/web/core/components/project-work-item-types/index.ts`
- Modify: `packages/constants/src/settings/project.ts`

- [ ] **Step 1: Tambah item settings project**

Di `packages/constants/src/settings/project.ts`:

1. Tambahkan ke `PROJECT_SETTINGS`:

```ts
  work_item_types: {
    key: "work_item_types",
    i18n_label: "workspace_settings.settings.work_item_types.title",
    href: `/work-item-types`,
    access: [EUserProjectRoles.ADMIN],
    highlight: (pathname: string, baseUrl: string) => pathname === `${baseUrl}/work-item-types/`,
  },
```

2. Tambahkan ke `GROUPED_PROJECT_SETTINGS[WORK_STRUCTURE]` (setelah `states`).

3. Tambahkan ikon di `apps/web/core/components/settings/project/sidebar/item-icon.tsx` (pakai ikon propel yang tersedia).

- [ ] **Step 2: Root komponen**

```tsx
// apps/web/core/components/project-work-item-types/root.tsx
export const ProjectWorkItemTypesRoot = observer(function ProjectWorkItemTypesRoot({
  workspaceSlug,
  projectId,
}: Props) {
  const { workItemTypes, workflowMap, fetchWorkItemTypes, fetchWorkflowMap, importWorkItemTypes, unlinkWorkItemType } =
    useWorkflow();

  useEffect(() => {
    void fetchWorkItemTypes(workspaceSlug);
    void fetchWorkflowMap(workspaceSlug, projectId);
  }, [workspaceSlug, projectId, fetchWorkItemTypes, fetchWorkflowMap]);

  const enabledIds = new Set((workflowMap[projectId]?.types ?? []).map((type) => type.type_id));

  return (
    <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
      {workItemTypes?.map((type) => (
        <TypeToggleItem
          key={type.id}
          type={type}
          enabled={enabledIds.has(type.id)}
          disabledReason={!type.workflow ? "Attach a workflow first" : undefined}
          onToggle={async (next) => {
            try {
              if (next) await importWorkItemTypes(workspaceSlug, projectId, [type.id]);
              else await unlinkWorkItemType(workspaceSlug, projectId, type.id);
            } catch (error: any) {
              setToast({ type: TOAST_TYPE.ERROR, message: error?.error ?? "Something went wrong" });
            }
          }}
        />
      ))}
    </div>
  );
});
```

`type-toggle-item.tsx`: nama type + nama workflow + `ToggleSwitch`; disabled bila type belum punya workflow.

- [ ] **Step 3: Page + header**

Pola sama dengan labels/states project settings: `SettingsContentWrapper header={<.../>}`, permission ADMIN project, `PageHead`.

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add "apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/work-item-types" apps/web/core/components/project-work-item-types packages/constants/src/settings/project.ts
git commit -m "feat(web): project work item types settings page"
```

---

### Task W3.3: States project read-only untuk typed state

**Files:**

- Modify: `packages/types/src/state.ts`
- Modify: `apps/web/core/components/project-states/root.tsx` (atau `group-list.tsx`)

- [ ] **Step 1: Tambah field opsional di `IState`**

```ts
  type_id?: string | null;
  workflow_state_id?: string | null;
```

- [ ] **Step 2: Pisahkan typed state dari daftar editable**

Di root/group-list states settings, filter state legacy untuk CRUD:

```ts
const legacyStates = projectStates?.filter((state) => !state.type_id) ?? [];
const typedStates = projectStates?.filter((state) => Boolean(state.type_id)) ?? [];
```

Daftar editable tetap memakai `legacyStates` (semua aksi create/update/delete/drag). Tambahkan section read-only di bawahnya:

```tsx
{
  typedStates.length > 0 && (
    <div className="mt-8 flex flex-col gap-2">
      <h4 className="text-14 font-medium">{t("workspace_settings.settings.work_item_types.title")}</h4>
      <p className="text-caption-md-regular text-tertiary">
        {t("workspace_settings.settings.work_item_types.description")}
      </p>
      <div className="flex flex-wrap gap-2">
        {typedStates.map((state) => (
          <span key={state.id} className="rounded border border-subtle px-2 py-1 text-caption-md-medium">
            {state.name}
          </span>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add packages/types/src/state.ts apps/web/core/components/project-states
git commit -m "feat(web): show typed states as read-only in project states"
```

---

## Milestone W4 — Board/views hybrid

### Task W4.1: Fetch workflow-map di project layout

**Files:**

- Modify: `apps/web/core/layouts/auth-layout/project-wrapper.tsx` (lokasi fetch `PROJECT_STATES`)

- [ ] **Step 1: Discovery**

Run: `rg -n "PROJECT_STATES|fetchProjectStates|useProjectState" apps/web/core/layouts/auth-layout/project-wrapper.tsx`

- [ ] **Step 2: Tambah fetch**

Setelah fetch project states yang ada, tambahkan:

```ts
const { fetchWorkflowMap } = useWorkflow();
...
useSWR(
  workspaceSlug && projectId ? PROJECT_WORKFLOW_MAP(projectId) : null,
  workspaceSlug && projectId ? () => fetchWorkflowMap(workspaceSlug, projectId) : null,
  { revalidateIfStale: false, revalidateOnFocus: false }
);
```

Import `PROJECT_WORKFLOW_MAP` dari `@plane/constants` dan `useWorkflow` dari `@/hooks/store/use-workflow`.

- [ ] **Step 3: Verifikasi**

Buka project board; cek network: `GET .../workflow-map/` dipanggil sekali dan 200.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/layouts/auth-layout/project-wrapper.tsx
git commit -m "feat(web): fetch project workflow map"
```

---

### Task W4.2: Kolom kanban hybrid

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/utils.tsx` (`getStateColumns`, sekitar baris 242-256)
- Modify: `apps/web/core/components/issues/issue-layouts/kanban/default.tsx` / `base-kanban-root.tsx` (sumber filter type)

- [ ] **Step 1: Discovery**

Run:

```bash
rg -n "getStateColumns|getGroupByColumns" apps/web/core/components/issues/issue-layouts/utils.tsx
rg -n "filters|displayFilters|issue_type" apps/web/core/components/issues/issue-layouts/kanban/base-kanban-root.tsx apps/web/core/components/issues/issue-layouts/kanban/default.tsx | head -30
```

Catat bagaimana `filters.issue_type` (array) bisa diakses di jalur kolom.

- [ ] **Step 2: Implementasi**

Di `utils.tsx`, import helper:

```ts
import { findWorkflowMapType, resolveStateColumns } from "@/store/workflow.helpers";
```

Ubah `getStateColumns` agar:

```ts
const getStateColumns = (projectId: string, typeId?: string | null) => {
  const projectStates = store.state.getProjectStates(projectId) ?? [];
  const mapType = findWorkflowMapType(store.workflow.getWorkflowMap(projectId), typeId);
  const states = resolveStateColumns(projectStates, mapType);
  return states.map((state) => ({
    id: state.id,
    name: state.name,
    icon: StateGroupIcon,
    payload: { state_id: state.id },
  }));
};
```

Sambungkan `typeId` dari filter: di jalur yang memanggil `getGroupByColumns`, teruskan `filters?.issue_type?.length === 1 ? filters.issue_type[0] : null`. Jika `getGroupByColumns` tidak menerima filter, tambahkan parameter opsional `typeId` dan teruskan dari `default.tsx`.

- [ ] **Step 3: Default group_by untuk campuran type**

Di tempat default display filter dibuat/di-resolve (hasil discovery; kemungkinan `issue-filter-helper.store.ts` atau `base-issues.store.ts`), tambahkan: jika project punya workflow-map dengan ≥1 type dan tidak ada filter type tunggal, default `group_by = "state_detail.group"`.

```ts
const workflowMap = store.workflow.getWorkflowMap(projectId);
const hasTypedWorkflow = (workflowMap?.types?.length ?? 0) > 0;
const singleType = filters?.issue_type?.length === 1 ? filters.issue_type[0] : null;
if (hasTypedWorkflow && !singleType && !displayFilters?.group_by) {
  displayFilters.group_by = "state_detail.group";
}
```

Jika display filter sudah punya nilai eksplisit dari user, jangan override.

- [ ] **Step 4: Verifikasi manual**

- Project dengan type aktif, tanpa filter type → kolom 5 group.
- Filter satu type → kolom state type itu (New/In Progress/...), urut sequence.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts
git commit -m "feat(web): hybrid kanban columns for typed work items"
```

---

### Task W4.3: Dropdown state hanya transisi valid

**Files:**

- Modify: `apps/web/core/components/dropdowns/state/base.tsx`
- Modify: `apps/web/core/components/dropdowns/state/dropdown.tsx`
- Modify: konsumen utama (issue detail sidebar, properties, spreadsheet state column)

- [ ] **Step 1: Tambah filter di base dropdown**

`TWorkItemStateDropdownBaseProps` tambah:

```ts
  workItemTypeId?: string | null;
  currentStateId?: string | null;
  projectId: string;
```

Di dalam komponen, hitung opsi:

```ts
const workflowMap = useWorkflow().getWorkflowMap(projectId);
const mapType = findWorkflowMapType(workflowMap, workItemTypeId);
const allowedIds = allowedTargetStateIds(mapType, currentStateId);
const effectiveStateIds = mapType
  ? (stateIds ?? []).filter((id) => id === currentStateId || allowedIds.includes(id))
  : stateIds;
```

Pakai `effectiveStateIds` untuk membangun daftar opsi (state sekarang tetap tampil sebagai current).

- [ ] **Step 2: Teruskan props dari konsumen**

Di `issues/issue-detail/sidebar.tsx`, `issues/issue-layouts/properties/all-properties.tsx`, dan `issues/issue-layouts/spreadsheet/columns/state-column.tsx`, tambahkan prop `projectId`, `workItemTypeId={issue.type_id}`, `currentStateId={issue.state_id}` pada `StateDropdown`/`WorkItemStateDropdownBase`.

- [ ] **Step 3: Verifikasi manual**

Buka issue bertipe Incident di state New; dropdown hanya menampilkan New + In Progress (bukan Closed). Pindah ke In Progress; dropdown berubah menampilkan In Progress + Closed + In Progress→(reopen) sesuai definisi.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/dropdowns/state apps/web/core/components/issues
git commit -m "feat(web): restrict state dropdown to allowed transitions"
```

---

### Task W4.4: Type selector di create form

**Files:**

- Create: `apps/web/core/components/dropdowns/work-item-type/dropdown.tsx`
- Modify: `apps/web/core/components/issues/issue-modal/components/default-properties.tsx`

- [ ] **Step 1: Discovery**

Run: `rg -n "state_id|type_id|StateDropdown" apps/web/core/components/issues/issue-modal/components/default-properties.tsx | head -20`

- [ ] **Step 2: Dropdown type**

Buat dropdown sederhana (pola `ComboDropDown` dari dropdown state) yang menampilkan type dari `workflowMap[projectId]?.types` (hanya type enabled + punya workflow):

```tsx
export const WorkItemTypeDropdown = observer(function WorkItemTypeDropdown({ projectId, value, onChange }: Props) {
  const mapTypes = useWorkflow().getWorkflowMap(projectId)?.types ?? [];
  return (
    <ComboDropDown
      label={mapTypes.find((type) => type.type_id === value)?.type_name ?? "Type"}
      ...
    >
      {mapTypes.map((type) => (
        <Combobox.Option key={type.type_id} value={type.type_id} onSelect={() => onChange(type.type_id)}>
          {type.type_name}
        </Combobox.Option>
      ))}
    </ComboDropDown>
  );
});
```

- [ ] **Step 3: Sambungkan ke form**

Di `default-properties.tsx`:

- Tampilkan `WorkItemTypeDropdown` bila project punya workflow-map.
- Saat type berubah: set `type_id` dan reset `state_id` ke `default_state_id` type itu (`findWorkflowMapType(map, typeId)?.default_state_id ?? null`).
- Saat type di-set, teruskan `workItemTypeId`/`currentStateId` ke `StateDropdown` (Task W4.3) agar state yang tersedia hanya state type itu.

- [ ] **Step 4: Verifikasi manual**

Buka modal create issue: pilih type Incident → state otomatis New dan hanya state Incident yang tersedia; ganti ke Change → state reset ke default Change.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/dropdowns/work-item-type apps/web/core/components/issues/issue-modal
git commit -m "feat(web): work item type selector in create form"
```

---

### Task W4.5: Build + smoke web

**Files:** tidak ada perubahan kode.

- [ ] **Step 1: Jalankan check + test**

Run:

```bash
pnpm check:types
pnpm check:lint
pnpm --filter=web test
```

Expected: PASS (termasuk `workflow.helpers.test.ts`).

- [ ] **Step 2: Build + restart web prod (AGENTS.md)**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build sukses; service aktif.

- [ ] **Step 3: Smoke manual di browser**

1. Workspace settings → Service management → Work item types: buat type "Smoke Incident" + pilih workflow Incident.
2. Workflows → Incident Workflow: tambah state, ubah matriks transisi.
3. Project settings → Work item types: enable "Smoke Incident"; matikan lagi saat ada issue bertipe itu → toast error (guard backend).
4. Board: filter type Smoke Incident → kolom state type; tanpa filter → kolom group.
5. Create issue tipe Smoke Incident → state default; dropdown state hanya transisi valid.

- [ ] **Step 4: Commit sisa perubahan (jika ada)**

```bash
git status --short
git add <file-yang-tersisa>
git commit -m "chore: web workflow verification"
```

---

## Catatan self-review

- **Bulk edit state tidak diimplementasikan di UI:** `useBulkOperationStatus()` di repo ini hard-coded `false` dan tidak ada pemanggil `bulkUpdateProperties`; endpoint `bulk-operation-issues` juga belum ada di api-rs. Aturan bulk tetap didokumentasikan di spec dan `validate_state_transition` backend siap dipakai saat bulk diport.
- **Test UI `.tsx` tidak jalan** karena `apps/web/vitest.config.ts` hanya meng-include `core/**/*.test.ts`; karena itu pengujian UI memakai langkah verifikasi manual di setiap task.
- **Discovery steps** disengaja pada W3.1, W4.1, W4.2, W4.4 karena struktur feature project, filter store, dan issue modal belum dipetakan detail; setiap task menyertakan command `rg` dan kode yang harus ditulis setelahnya.
- **Ketergantungan backend:** `states` API harus sudah mengembalikan `type_id` (Task B9 backend) dan `workflow` di payload type (Task B6 backend).
