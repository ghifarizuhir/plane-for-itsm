# Workflow State Grouping & Filter Lintas Type — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Grouping dan filter state di project issues page mengenali konteks work item type: mixed view default 5 state group, opsi grouping `workflow_state` menampilkan kolom komposit `Problem · Baru`, filter state berlabel type dan di-scope oleh filter type, quick-add membawa `type_id`, dan drag lintas type diblok dengan pesan.

**Architecture:** Tidak ada perubahan backend/schema. Web memakai nilai grouping efektif yang dihitung runtime oleh helper murni (`resolveEffectiveDisplayFilters`) dari display filters tersimpan + workflow map project; nilai efektif dipakai konsisten oleh kolom, parameter server, drag & drop, dan dropdown grouping. Label komposit dibangun dari state rows (`type_id`) + workflow map (`type_name`). Filter state di-scope dan dipangkas lewat helper murni yang diuji vitest.

**Tech Stack:** TypeScript, React, MobX, vitest, oxlint/oxfmt, i18n `@plane/i18n`.

**Spec:** `docs/superpowers/specs/2026-09-29-workflow-state-grouping-filter-design.md`

---

## File Structure

| File                                                                                                         | Tanggung jawab                                                    |
| ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `packages/types/src/view-props.ts`                                                                           | Nilai grouping baru `workflow_state`                              |
| `packages/constants/src/issue/common.ts`                                                                     | Opsi grouping + mapping ke server (`state_id`)                    |
| `packages/constants/src/issue/filter.ts`                                                                     | Opsi grouping project page (list/kanban/sub)                      |
| `packages/i18n/src/locales/**/common.json`                                                                   | String baru semua locale                                          |
| `apps/web/core/store/workflow.helpers.ts`                                                                    | Helper murni: resolusi efektif, kolom komposit, scope/prune state |
| `apps/web/core/store/workflow.helpers.test.ts`                                                               | Tes vitest helper                                                 |
| `apps/web/core/store/issue/helpers/base-issues.store.ts`                                                     | Map key grouping `workflow_state`                                 |
| `apps/web/core/store/issue/helpers/issue-filter-helper.store.ts`                                             | Serialisasi param server memakai nilai efektif                    |
| `apps/web/core/store/issue/project/filter.store.ts`                                                          | Kirim workflow map ke serialisasi                                 |
| `apps/web/core/store/issue/project-views/filter.store.ts`                                                    | Kirim workflow map ke serialisasi                                 |
| `apps/web/core/components/issues/issue-layouts/utils.tsx`                                                    | Kolom state komposit + payload `type_id`                          |
| `apps/web/core/components/issues/issue-layouts/list/default.tsx`                                             | Gate `typeId` untuk `workflow_state`                              |
| `apps/web/core/components/issues/issue-layouts/kanban/swimlanes.tsx`                                         | Gate `typeId` per axis untuk `workflow_state`                     |
| `apps/web/core/components/issues/issue-layouts/kanban/base-kanban-root.tsx`                                  | Pasang grouping efektif                                           |
| `apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx`                                      | Pasang grouping efektif                                           |
| `apps/web/core/hooks/use-group-dragndrop.ts`                                                                 | Batalkan drop lintas kolom di state-group view                    |
| `apps/web/core/components/workflow/use-workflow-drag-n-drop.ts`                                              | Guard drag lintas type                                            |
| `apps/web/core/components/issues/issue-layouts/kanban/kanban-group.tsx`                                      | Pesan guard kanban                                                |
| `apps/web/core/components/issues/issue-layouts/list/list-group.tsx`                                          | Pesan guard list                                                  |
| `apps/web/core/components/issues/filters.tsx`                                                                | Opsi grouping dinamis + nilai efektif                             |
| `apps/web/core/components/issues/issue-layouts/filters/header/display-filters/display-filters-selection.tsx` | Teruskan opsi grouping custom                                     |
| `packages/utils/src/work-item-filters/configs/filters/state.ts`                                              | Label opsi state custom                                           |
| `apps/web/core/components/work-item-filters/filters-hoc/base.tsx`                                            | Live expression, scope, prune                                     |
| `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`                                     | Scope + label opsi state                                          |
| `apps/web/core/components/issues/issue-modal/components/default-properties.tsx`                              | Derive type dari state typed                                      |

---

### Task 1: Nilai grouping `workflow_state` + string i18n

**Files:**

- Modify: `packages/types/src/view-props.ts:14-26`
- Modify: `packages/constants/src/issue/common.ts:27-40,113-128`
- Modify: `packages/constants/src/issue/filter.ts:221-246`
- Modify: `apps/web/core/store/issue/helpers/base-issues.store.ts:114-141`
- Modify: `packages/i18n/src/locales/*/common.json` (semua 19 locale)

- [ ] **Step 1: Tambah nilai union `workflow_state`**

`packages/types/src/view-props.ts`:

```ts
export type TIssueGroupByOptions =
  | "state"
  | "workflow_state"
  | "priority"
  | "labels"
  | "created_by"
  | "state_detail.group"
  | "project"
  | "assignees"
  | "cycle"
  | "module"
  | "target_date"
  | "team_project"
  | null;
```

- [ ] **Step 2: Daftarkan opsi grouping + mapping server**

`packages/constants/src/issue/common.ts`, di dalam `EIssueGroupByToServerOptions` (setelah baris `"state" = "state_id",`):

```ts
  // eslint-disable-next-line @typescript-eslint/no-duplicate-enum-values
  "workflow_state" = "state_id",
```

dan di `ISSUE_GROUP_BY_OPTIONS` (setelah baris `{ key: "state", ... }`):

```ts
  { key: "workflow_state", titleTranslationKey: "common.workflow_states" },
```

- [ ] **Step 3: Tambah opsi ke project page (list + kanban + sub group)**

`packages/constants/src/issue/filter.ts` pada blok `issues.layoutOptions`:

list ():

```ts
        display_filters: {
          group_by: ["state", "workflow_state", "state_detail.group", "priority", "cycle", "module", "labels", "assignees", "created_by", null],
```

kanban `group_by` dan `sub_group_by`:

```ts
          group_by: ["state", "workflow_state", "state_detail.group", "priority", "cycle", "module", "labels", "assignees", "created_by"],
          sub_group_by: ["state", "workflow_state", "state_detail.group", "priority", "cycle", "module", "labels", "assignees", "created_by", null],
```

- [ ] **Step 4: Tambah entri map store**

`apps/web/core/store/issue/helpers/base-issues.store.ts` pada `ISSUE_GROUP_BY_KEY`:

```ts
  state: "state_id",
  workflow_state: "state_id",
  "state_detail.group": "state_id", // state_detail.group is only being used for state_group display,
```

dan `ISSUE_FILTER_DEFAULT_DATA`:

```ts
  state: "state_id",
  workflow_state: "state_id",
  "state_detail.group": "state__group", // state_detail.group is only being used for state_group display,
```

- [ ] **Step 5: Tambah string i18n (semua locale)**

String baru:

| Key                                                    | `en`                                                     | `id`                                                                         |
| ------------------------------------------------------ | -------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `common.workflow_states`                               | `Workflow states`                                        | `Status alur kerja`                                                          |
| `common.state_change_requires_workflow_state_grouping` | `Switch to the Workflow states grouping to change state` | `Ganti ke pengelompokan berdasarkan status alur kerja untuk mengubah status` |
| `common.workflow_state_wrong_type`                     | `Target state belongs to a different work item type`     | `Status tujuan milik tipe item kerja yang berbeda`                           |

Wajib baca dan ikuti skill `translate` sebelum menyentuh file locale (istilah do-not-translate, plural CLDR, register per locale). Tambahkan ketiga key di namespace `common` (`workflow_states` dekat `states`/`state_groups`; dua key pesan dekat `warning`) untuk **semua** locale: `cs, de, en, es, fr, id, it, ja, ka-ge, ko, pl, pt-BR, ro, ru, sk, tr-TR, ua, vi-VN, zh-CN, zh-TW`.

- [ ] **Step 6: Regenerasi tipe i18n dan jalankan typecheck**

Run:

```bash
pnpm --filter=@plane/i18n build
pnpm check
```

Expected: PASS (tidak ada error tuple union `workflow_state`; map `Record<TIssueDisplayFilterOptions, ...>` sudah lengkap).

- [ ] **Step 7: Commit**

```bash
git add packages/types/src/view-props.ts packages/constants/src/issue/common.ts packages/constants/src/issue/filter.ts apps/web/core/store/issue/helpers/base-issues.store.ts packages/i18n
git commit -m "feat(web): register workflow_state grouping option"
```

---

### Task 2: Helper murni — resolusi efektif, kolom komposit, type ids

**Files:**

- Modify: `apps/web/core/store/workflow.helpers.ts`
- Test: `apps/web/core/store/workflow.helpers.test.ts`

- [ ] **Step 1: Tulis tes yang gagal**

Di `apps/web/core/store/workflow.helpers.test.ts`, tambahkan `TWorkflowMap` ke import type `@plane/types` yang sudah ada (baris 2) dan tambahkan nama fungsi baru ke import `./workflow.helpers` yang sudah ada (baris 3-14):

```ts
import type { IIssueFilters, IState, TWorkflowMap, TWorkflowMapType } from "@plane/types";
import {
  allowedTargetStateIds,
  buildTransitionMatrix,
  findWorkflowMapType,
  getSingleWorkItemTypeId,
  getTypeDefaultStateId,
  getWorkItemTypeIds,
  isTypedState,
  MAX_WORKFLOW_MAP_FETCH_RETRIES,
  resolveEffectiveDisplayFilters,
  resolveSelectableStateIds,
  resolveStateColumns,
  resolveWorkflowStateColumns,
  shouldRetryWorkflowMapFetch,
} from "./workflow.helpers";
```

Tambahkan fixture + describe di akhir file:

```ts
const typedMapType = (typeId: string, typeName: string, stateId: string, name: string): TWorkflowMapType => ({
  type_id: typeId,
  type_name: typeName,
  workflow_id: `wf-${typeId}`,
  default_state_id: stateId,
  states: [{ id: stateId, name, color: "#000000", group: "backlog", sequence: 1, is_default: true }],
  transitions: [],
});

const compositeWorkflowMap: TWorkflowMap = {
  types: [
    {
      ...typedMapType("type-p", "Problem", "p-1", "Baru"),
      states: [
        { id: "p-1", name: "Baru", color: "#111111", group: "backlog", sequence: 1, is_default: true },
        { id: "p-2", name: "Investigasi", color: "#222222", group: "started", sequence: 2, is_default: false },
      ],
    },
    {
      ...typedMapType("type-c", "Change", "c-1", "Baru"),
      states: [{ id: "c-1", name: "Baru", color: "#333333", group: "backlog", sequence: 1, is_default: true }],
    },
  ],
};

const projectStates: IState[] = [
  {
    id: "l-1",
    name: "Baru",
    color: "#000000",
    group: "backlog",
    description: "",
    sequence: 5,
    workspace_id: "w",
    project_id: "p",
  },
  {
    id: "p-1",
    name: "Baru",
    color: "#111111",
    group: "backlog",
    description: "",
    sequence: 1,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-p",
    workflow_state_id: "ws-p-1",
  },
  {
    id: "p-2",
    name: "Investigasi",
    color: "#222222",
    group: "started",
    description: "",
    sequence: 2,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-p",
    workflow_state_id: "ws-p-2",
  },
  {
    id: "c-1",
    name: "Baru",
    color: "#333333",
    group: "backlog",
    description: "",
    sequence: 1,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-c",
    workflow_state_id: "ws-c-1",
  },
  {
    id: "z-1",
    name: "Ghost",
    color: "#444444",
    group: "started",
    description: "",
    sequence: 9,
    workspace_id: "w",
    project_id: "p",
    type_id: "type-z",
    workflow_state_id: "ws-z-1",
  },
] as IState[];

describe("resolveEffectiveDisplayFilters", () => {
  it("project tanpa typed workflow tidak berubah", () => {
    const filters = { group_by: "state" as const, sub_group_by: null };
    expect(resolveEffectiveDisplayFilters(filters, undefined, null)).toEqual(filters);
    expect(resolveEffectiveDisplayFilters(filters, { types: [] }, "type-p")).toEqual(filters);
  });

  it("mixed typed workflow: state → state_detail.group", () => {
    const result = resolveEffectiveDisplayFilters(
      { group_by: "state", sub_group_by: null },
      compositeWorkflowMap,
      null
    );
    expect(result?.group_by).toBe("state_detail.group");
  });

  it("satu type: state → workflow_state", () => {
    const result = resolveEffectiveDisplayFilters(
      { group_by: "state", sub_group_by: null },
      compositeWorkflowMap,
      "type-p"
    );
    expect(result?.group_by).toBe("workflow_state");
  });

  it("nilai workflow_state dan state_detail.group eksplisit dipertahankan", () => {
    expect(
      resolveEffectiveDisplayFilters({ group_by: "workflow_state", sub_group_by: null }, compositeWorkflowMap, null)
        ?.group_by
    ).toBe("workflow_state");
    expect(
      resolveEffectiveDisplayFilters({ group_by: "state_detail.group", sub_group_by: null }, compositeWorkflowMap, null)
        ?.group_by
    ).toBe("state_detail.group");
  });

  it("menghapus sub_group_by yang menjadi sama dengan group_by efektif", () => {
    const result = resolveEffectiveDisplayFilters(
      { group_by: "state", sub_group_by: "state_detail.group" },
      compositeWorkflowMap,
      null
    );
    expect(result?.group_by).toBe("state_detail.group");
    expect(result?.sub_group_by).toBeNull();
  });

  it("undefined tetap undefined", () => {
    expect(resolveEffectiveDisplayFilters(undefined, compositeWorkflowMap, null)).toBeUndefined();
  });
});

describe("resolveWorkflowStateColumns", () => {
  it("single type memakai kolom type itu dengan label polos", () => {
    const columns = resolveWorkflowStateColumns(projectStates, compositeWorkflowMap.types[0], compositeWorkflowMap);
    expect(columns.map((column) => [column.state.id, column.label])).toEqual([
      ["p-1", "Baru"],
      ["p-2", "Investigasi"],
    ]);
  });

  it("mixed: legacy dulu, lalu per type urut sequence, label komposit", () => {
    const columns = resolveWorkflowStateColumns(projectStates, undefined, compositeWorkflowMap);
    expect(columns.map((column) => [column.state.id, column.label])).toEqual([
      ["l-1", "Baru"],
      ["p-1", "Problem · Baru"],
      ["p-2", "Problem · Investigasi"],
      ["c-1", "Change · Baru"],
      ["z-1", "Ghost"],
    ]);
  });

  it("tanpa typed workflow mengembalikan label polos apa adanya", () => {
    const columns = resolveWorkflowStateColumns(projectStates, undefined, { types: [] });
    expect(columns.map((column) => column.label)).toEqual(["Baru", "Baru", "Investigasi", "Baru", "Ghost"]);
  });
});

describe("getWorkItemTypeIds", () => {
  it("mengumpulkan semua type id dari richFilters", () => {
    expect(getWorkItemTypeIds({ richFilters: { and: [{ type_id__in: "t-1,t-2" }] } }).toSorted()).toEqual([
      "t-1",
      "t-2",
    ]);
  });

  it("membaca bentuk legacy filters.issue_type dan input kosong", () => {
    expect(getWorkItemTypeIds({ filters: { issue_type: ["t-9"] } })).toEqual(["t-9"]);
    expect(getWorkItemTypeIds(undefined)).toEqual([]);
    expect(getWorkItemTypeIds({ richFilters: {} })).toEqual([]);
  });
});
```

- [ ] **Step 2: Run tes untuk memastikan gagal**

Run: `pnpm --filter=web test workflow.helpers`

Expected: FAIL — `resolveEffectiveDisplayFilters` tidak ditemukan.

- [ ] **Step 3: Implementasi helper**

Di `apps/web/core/store/workflow.helpers.ts`, tambahkan `TIssueGroupByOptions` + `TWorkflowMap` pada import type dari `@plane/types` (biarkan yang sudah ada), lalu tambahkan:

```ts
/** Project punya minimal satu type ber-workflow di workflow map. */
export const hasTypedWorkflows = (map: TWorkflowMap | undefined): boolean => (map?.types?.length ?? 0) > 0;

/**
 * Nilai grouping efektif: nilai `state` legacy di-resolve runtime supaya view
 * lama tetap valid. Typed project + mixed → 5 state group; typed project +
 * satu type → workflow_state; non-typed → apa adanya.
 */
export const resolveEffectiveDisplayFilters = (
  displayFilters: IIssueDisplayFilterOptions | undefined,
  workflowMap: TWorkflowMap | undefined,
  singleTypeId: string | null | undefined
): IIssueDisplayFilterOptions | undefined => {
  if (!displayFilters) return displayFilters;
  const typed = hasTypedWorkflows(workflowMap);
  const resolveAxis = (axis: TIssueGroupByOptions): TIssueGroupByOptions => {
    if (axis !== "state" || !typed) return axis;
    return singleTypeId ? "workflow_state" : "state_detail.group";
  };
  const group_by = resolveAxis(displayFilters.group_by);
  let sub_group_by = resolveAxis(displayFilters.sub_group_by);
  if (group_by && sub_group_by && group_by === sub_group_by) sub_group_by = null;
  return { ...displayFilters, group_by, sub_group_by };
};

export type TWorkflowStateColumn = { state: IState; label: string };

/**
 * Kolom state untuk grouping `workflow_state`: single type memakai mirror type
 * itu (label polos); mixed menggabungkan legacy (label polos) lalu state per
 * type (label `{type_name} · {state_name}`), urut sequence.
 */
export const resolveWorkflowStateColumns = (
  projectStates: IState[],
  mapType: TWorkflowMapType | undefined,
  workflowMap: TWorkflowMap | undefined
): TWorkflowStateColumn[] => {
  if (mapType) return resolveStateColumns(projectStates, mapType).map((state) => ({ state, label: state.name }));
  if (!hasTypedWorkflows(workflowMap)) return projectStates.map((state) => ({ state, label: state.name }));
  const columns: TWorkflowStateColumn[] = [];
  const used = new Set<string>();
  for (const state of projectStates) {
    if (isTypedState(state)) continue;
    columns.push({ state, label: state.name });
    used.add(state.id);
  }
  for (const type of workflowMap?.types ?? []) {
    // oxlint-disable-next-line unicorn/no-array-sort -- filter() already copies the array
    const typedStates = projectStates
      .filter((state) => state.type_id === type.type_id)
      .sort((a, b) => a.sequence - b.sequence);
    for (const state of typedStates) {
      if (used.has(state.id)) continue;
      used.add(state.id);
      columns.push({ state, label: `${type.type_name} · ${state.name}` });
    }
  }
  for (const state of projectStates) {
    if (!used.has(state.id)) columns.push({ state, label: state.name });
  }
  return columns;
};

/** Semua type id yang sedang difilter (rich maupun legacy). */
export const getWorkItemTypeIds = (
  issueFilters: TLegacyIssueFilterBag | TRichIssueFilterBag | IIssueFilters | null | undefined
): string[] => {
  if (!issueFilters) return [];
  const ids = new Set<string>();
  if ("filters" in issueFilters) {
    (issueFilters.filters?.issue_type ?? []).forEach((typeId) => ids.add(typeId));
    return [...ids];
  }
  collectTypeIds((issueFilters as TRichIssueFilterBag).richFilters, ids);
  return [...ids];
};
```

Tambahkan `IIssueDisplayFilterOptions` ke import type `@plane/types` yang sudah ada di file.

- [ ] **Step 4: Run tes untuk memastikan lulus**

Run: `pnpm --filter=web test workflow.helpers`

Expected: PASS (semua describe).

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/workflow.helpers.ts apps/web/core/store/workflow.helpers.test.ts
git commit -m "feat(web): add effective workflow grouping helpers"
```

---

### Task 3: Serialisasi param server memakai grouping efektif

**Files:**

- Modify: `apps/web/core/store/issue/helpers/issue-filter-helper.store.ts:50-56,92-126`
- Modify: `apps/web/core/store/issue/project/filter.store.ts:107-121`
- Modify: `apps/web/core/store/issue/project-views/filter.store.ts:115-129`

- [ ] **Step 1: Tambah parameter workflow map di helper**

`issue-filter-helper.store.ts`:

Tambahkan `TWorkflowMap` ke import type dari `@plane/types`, dan import helper:

```ts
import { getSingleWorkItemTypeId, resolveEffectiveDisplayFilters } from "@/store/workflow.helpers";
```

Ubah signature interface (baris ~52-56):

```ts
  computedFilteredParams(
    richFilters: TWorkItemFilterExpression,
    displayFilters: IIssueDisplayFilterOptions | undefined,
    acceptableParamsByLayout: TIssueParams[],
    workflowMap?: TWorkflowMap
  ): Partial<Record<TIssueParams, string | boolean>>;
```

Ubah implementasi (baris ~92-104):

```ts
  computedFilteredParams = (
    richFilters: TWorkItemFilterExpression,
    displayFilters: IIssueDisplayFilterOptions | undefined,
    acceptableParamsByLayout: TIssueParams[],
    workflowMap?: TWorkflowMap
  ): Partial<Record<TIssueParams, string | boolean>> => {
    const effectiveDisplayFilters = resolveEffectiveDisplayFilters(
      displayFilters,
      workflowMap,
      getSingleWorkItemTypeId({ richFilters })
    );
    const computedDisplayFilters: Partial<Record<TIssueParams, undefined | string[] | boolean | string>> = {
      group_by: effectiveDisplayFilters?.group_by
        ? EIssueGroupByToServerOptions[effectiveDisplayFilters.group_by]
        : undefined,
      sub_group_by: effectiveDisplayFilters?.sub_group_by
        ? EIssueGroupByToServerOptions[effectiveDisplayFilters.sub_group_by]
        : undefined,
      order_by: effectiveDisplayFilters?.order_by || undefined,
      sub_issue: effectiveDisplayFilters?.sub_issue ?? true,
    };
```

(sisa method tidak berubah; referensi `displayFilters` setelah blok ini tidak ada.)

- [ ] **Step 2: Kirim workflow map dari project filter store**

`project/filter.store.ts` `getAppliedFilters` (baris ~114-118):

```ts
const filteredRouteParams: Partial<Record<TIssueParams, string | boolean>> = this.computedFilteredParams(
  userFilters?.richFilters,
  userFilters?.displayFilters,
  filteredParams,
  projectId ? this.rootIssueStore.rootStore.workflow.getWorkflowMap(projectId) : undefined
);
```

- [ ] **Step 3: Kirim workflow map dari project-views filter store**

`project-views/filter.store.ts` `getAppliedFilters` (baris ~122-126):

```ts
const filteredRouteParams: Partial<Record<TIssueParams, string | boolean>> = this.computedFilteredParams(
  userFilters?.richFilters,
  userFilters?.displayFilters,
  filteredParams,
  this.rootIssueStore.projectId
    ? this.rootIssueStore.rootStore.workflow.getWorkflowMap(this.rootIssueStore.projectId)
    : undefined
);
```

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/store/issue/helpers/issue-filter-helper.store.ts apps/web/core/store/issue/project/filter.store.ts apps/web/core/store/issue/project-views/filter.store.ts
git commit -m "feat(web): serialize effective group_by to server params"
```

---

### Task 4: Kolom state komposit + payload type pada quick-add

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/utils.tsx:245-265`
- Modify: `apps/web/core/components/issues/issue-layouts/list/default.tsx:102,179`
- Modify: `apps/web/core/components/issues/issue-layouts/kanban/swimlanes.tsx:301-319`

- [ ] **Step 1: Ganti builder kolom state**

`utils.tsx` — ubah import helper (baris 57):

```ts
import { findWorkflowMapType, resolveWorkflowStateColumns } from "@/store/workflow.helpers";
```

Ganti seluruh `getStateColumns` (baris 245-265) dengan:

```tsx
const getStateColumns = ({ projectId, typeId }: TGetColumns): IGroupByColumn[] | undefined => {
  const { getProjectStates, projectStates } = store.state;
  const _states = projectId ? getProjectStates(projectId) : projectStates;
  if (!_states) return;
  // typed state columns come from the project workflow map: sequence + label are workspace-owned
  const workflowMap = projectId ? store.workflow.getWorkflowMap(projectId) : undefined;
  const mapType = findWorkflowMapType(workflowMap, typeId);
  const columns = resolveWorkflowStateColumns(_states, mapType, workflowMap);
  // map state columns to group by columns
  return columns.map(({ state, label }) => ({
    id: state.id,
    name: label,
    icon: (
      <div className="size-4 rounded-full">
        <StateGroupIcon stateGroup={state.group} color={state.color} size={EIconSize.LG} percentage={state.order} />
      </div>
    ),
    // type_id ikut payload supaya header "+"/quick-add membuat item bertipe benar
    payload: { state_id: state.id, ...(state.type_id ? { type_id: state.type_id } : {}) },
  }));
};
```

- [ ] **Step 2: Gate `typeId` untuk `workflow_state` di list**

`list/default.tsx` baris 102:

```tsx
    typeId: group_by === "state" || group_by === "workflow_state" ? workItemTypeId : null,
```

baris 179:

```tsx
                    workItemTypeId={group_by === "state" || group_by === "workflow_state" ? workItemTypeId : null}
```

- [ ] **Step 3: Gate `typeId` per axis di swimlanes**

`swimlanes.tsx` baris 302-303:

```tsx
// typed state columns only apply to a state/workflow axis; other axes keep their own grouping
const groupTypeId = group_by === "state" || group_by === "workflow_state" ? workItemTypeId : null;
const subGroupTypeId = sub_group_by === "state" || sub_group_by === "workflow_state" ? workItemTypeId : null;
```

- [ ] **Step 4: Typecheck dan lint**

Run: `pnpm check:types && pnpm check:lint`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/utils.tsx apps/web/core/components/issues/issue-layouts/list/default.tsx apps/web/core/components/issues/issue-layouts/kanban/swimlanes.tsx
git commit -m "feat(web): render composite workflow state columns"
```

---

### Task 5: Grouping efektif di root + drop state-group tidak mengubah state

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/kanban/base-kanban-root.tsx:88-100,130`
- Modify: `apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx:78-92,141`
- Modify: `apps/web/core/hooks/use-group-dragndrop.ts:1-36,100-127`

- [ ] **Step 1: Kanban root pakai grouping efektif**

`base-kanban-root.tsx` — tambah import:

```ts
import { useWorkflow } from "@/hooks/store/use-workflow";
import { getSingleWorkItemTypeId, resolveEffectiveDisplayFilters } from "@/store/workflow.helpers";
```

Ganti baris 93-94:

```ts
const { getWorkflowMap } = useWorkflow();
const workflowMap = projectId ? getWorkflowMap(projectId) : undefined;
const workItemTypeId = getSingleWorkItemTypeId(issuesFilter?.issueFilters);
const effectiveDisplayFilters = resolveEffectiveDisplayFilters(displayFilters, workflowMap, workItemTypeId);

const sub_group_by = effectiveDisplayFilters?.sub_group_by;
const group_by = effectiveDisplayFilters?.group_by;
```

(`useGroupIssuesDragNDrop(storeType, orderBy, group_by, sub_group_by)` di baris 130 otomatis memakai nilai efektif.)

- [ ] **Step 2: List root pakai grouping efektif**

`base-list-root.tsx` — tambah import:

```ts
import { useWorkflow } from "@/hooks/store/use-workflow";
import { getSingleWorkItemTypeId, resolveEffectiveDisplayFilters } from "@/store/workflow.helpers";
```

Ganti baris 82:

```ts
const { getWorkflowMap } = useWorkflow();
const workflowMap = projectId ? getWorkflowMap(projectId) : undefined;
const effectiveDisplayFilters = resolveEffectiveDisplayFilters(
  displayFilters,
  workflowMap,
  getSingleWorkItemTypeId(issuesFilter?.issueFilters)
);
const group_by = (effectiveDisplayFilters?.group_by || null) as GroupByColumnTypes | null;
```

(`useGroupIssuesDragNDrop(storeType, orderBy, group_by)` di baris 141 otomatis efektif.)

- [ ] **Step 3: Drop lintas kolom di state-group view = info + batal**

`use-group-dragndrop.ts` — tambah import:

```ts
import { useTranslation } from "@plane/i18n";
```

di dalam hook (setelah `const { workspaceSlug } = useParams();`):

```ts
const { t } = useTranslation();
```

dan di `handleOnDrop`, tepat setelah guard `destination.id === source.id`:

```ts
// state group view has no concrete state to move to; keep the drop a no-op with a hint
if (groupBy === "state_detail.group" && source.groupId !== destination.groupId) {
  setToast({
    type: TOAST_TYPE.INFO,
    title: t("common.warning"),
    message: t("common.state_change_requires_workflow_state_grouping"),
  });
  return;
}
```

- [ ] **Step 4: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues/issue-layouts/kanban/base-kanban-root.tsx apps/web/core/components/issues/issue-layouts/list/base-list-root.tsx apps/web/core/hooks/use-group-dragndrop.ts
git commit -m "feat(web): apply effective grouping on project boards"
```

---

### Task 6: Guard drag lintas work item type

**Files:**

- Modify: `apps/web/core/components/workflow/use-workflow-drag-n-drop.ts` (whole file)
- Modify: `apps/web/core/components/issues/issue-layouts/kanban/kanban-group.tsx:128,167-174`
- Modify: `apps/web/core/components/issues/issue-layouts/list/list-group.tsx:119-120,224-236`

- [ ] **Step 1: Implementasi hook guard**

Ganti isi `use-workflow-drag-n-drop.ts` (pertahankan header lisensi + komentar eslint-disable no-unused-vars di atas file):

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

/* eslint-disable @typescript-eslint/no-unused-vars */
import { useState } from "react";
import { useTranslation } from "@plane/i18n";
import type { TIssueGroupByOptions } from "@plane/types";
// hooks
import { useProjectState } from "@/hooks/store/use-project-state";

export const useWorkFlowFDragNDrop = (groupBy: TIssueGroupByOptions | undefined, subGroupBy?: TIssueGroupByOptions) => {
  const { t } = useTranslation();
  const { getStateById } = useProjectState();
  const [workflowDisabledSource, setWorkflowDisabledSource] = useState<string | undefined>(undefined);
  const [isWorkflowDropDisabled, setIsWorkflowDropDisabled] = useState(false);

  const isWorkflowAxis = groupBy === "workflow_state" || subGroupBy === "workflow_state";

  const handleWorkFlowState = (
    sourceGroupId: string | undefined,
    destinationGroupId: string | undefined,
    sourceSubGroupId?: string,
    destinationSubGroupId?: string
  ) => {
    if (!isWorkflowAxis) {
      if (isWorkflowDropDisabled) setIsWorkflowDropDisabled(false);
      setWorkflowDisabledSource(undefined);
      return;
    }
    const sourceStateId = groupBy === "workflow_state" ? sourceGroupId : sourceSubGroupId;
    const destinationStateId = groupBy === "workflow_state" ? destinationGroupId : destinationSubGroupId;
    const sourceTypeId = sourceStateId ? getStateById(sourceStateId)?.type_id : undefined;
    const destinationTypeId = destinationStateId ? getStateById(destinationStateId)?.type_id : undefined;
    const isBlocked = Boolean(sourceTypeId && destinationTypeId && sourceTypeId !== destinationTypeId);
    setIsWorkflowDropDisabled(isBlocked);
    setWorkflowDisabledSource(isBlocked ? sourceStateId : undefined);
  };

  return {
    workflowDisabledSource,
    isWorkflowDropDisabled,
    getIsWorkflowWorkItemCreationDisabled: (_groupId: string, _subGroupId?: string) => false,
    workflowDropErrorMessage: isWorkflowDropDisabled ? t("common.workflow_state_wrong_type") : undefined,
    handleWorkFlowState,
  };
};
```

- [ ] **Step 2: Pakai pesan guard di kanban**

`kanban-group.tsx` baris 128:

```ts
const {
  workflowDisabledSource,
  isWorkflowDropDisabled,
  handleWorkFlowState,
  getIsWorkflowWorkItemCreationDisabled,
  workflowDropErrorMessage,
} = useWorkFlowFDragNDrop(group_by, sub_group_by);
```

baris 167-174:

```ts
const workflowDropError = workflowDropErrorMessage ?? dropErrorMessage;

if ((isWorkflowDropDisabled || isDropDisabled) && workflowDropError) {
  setToast({
    type: TOAST_TYPE.WARNING,
    title: t("common.warning"),
    message: workflowDropError,
  });
  return;
}
```

- [ ] **Step 3: Pakai pesan guard di list**

`list-group.tsx` baris 119-120:

```ts
const {
  workflowDisabledSource,
  isWorkflowDropDisabled,
  handleWorkFlowState,
  getIsWorkflowWorkItemCreationDisabled,
  workflowDropErrorMessage,
} = useWorkFlowFDragNDrop(group_by);
```

blok drop (baris ~224-231):

```ts
if (isWorkflowDropDisabled || group.isDropDisabled) {
  const workflowDropError = workflowDropErrorMessage ?? group.dropErrorMessage;
  if (workflowDropError)
    setToast({
      type: TOAST_TYPE.WARNING,
      title: t("common.warning"),
      message: workflowDropError,
    });
  return;
}
```

- [ ] **Step 4: Typecheck dan lint**

Run: `pnpm check:types && pnpm check:lint`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/workflow/use-workflow-drag-n-drop.ts apps/web/core/components/issues/issue-layouts/kanban/kanban-group.tsx apps/web/core/components/issues/issue-layouts/list/list-group.tsx
git commit -m "feat(web): guard workflow state drag across work item types"
```

---

### Task 7: Dropdown grouping dinamis + nilai tercentang efektif

**Files:**

- Modify: `apps/web/core/components/issues/filters.tsx:45-63,117-126`
- Modify: `apps/web/core/components/issues/issue-layouts/filters/header/display-filters/display-filters-selection.tsx:25-35,78-109`
- Modify: `apps/web/core/components/issues/issue-layouts/filters/header/display-filters/group-by.tsx` (tidak berubah; opsi lewat prop)
- Modify: `apps/web/core/components/issues/issue-layouts/filters/header/display-filters/sub-group-by.tsx` (tidak berubah; opsi lewat prop)

- [ ] **Step 1: Hitung opsi + display filters efektif di HeaderFilters**

`filters.tsx` — ubah import:

```ts
import { useCallback, useMemo, useState } from "react";
import type { IIssueDisplayFilterOptions, IIssueDisplayProperties, TIssueGroupByOptions, TProject } from "@plane/types";
import { useWorkflow } from "@/hooks/store/use-workflow";
import { getSingleWorkItemTypeId, resolveEffectiveDisplayFilters } from "@/store/workflow.helpers";
```

(hapus import `TProject` terpisah yang lama; gabungkan seperti di atas.)

Di dalam komponen, setelah `const layoutDisplayFiltersOptions = ...`:

```ts
const { getWorkflowMap } = useWorkflow();
const workflowMap = storeType === EIssuesStoreType.PROJECT && projectId ? getWorkflowMap(projectId) : undefined;
const workItemTypeId = getSingleWorkItemTypeId(issueFilters);
const effectiveDisplayFilters = useMemo(
  () => resolveEffectiveDisplayFilters(issueFilters?.displayFilters, workflowMap, workItemTypeId),
  [issueFilters?.displayFilters, workflowMap, workItemTypeId]
);
const hasTypedWorkflow = (workflowMap?.types?.length ?? 0) > 0;
const groupByOptions = useMemo<TIssueGroupByOptions[]>(() => {
  const base = layoutDisplayFiltersOptions?.display_filters.group_by ?? [];
  if (!hasTypedWorkflow) return base.filter((key) => key !== "state_detail.group" && key !== "workflow_state");
  return [
    ...new Set<TIssueGroupByOptions>([
      ...base.filter((key) => key !== "state"),
      "state_detail.group",
      "workflow_state",
    ]),
  ];
}, [layoutDisplayFiltersOptions, hasTypedWorkflow]);
const subGroupByOptions = useMemo<TIssueGroupByOptions[]>(() => {
  const base = layoutDisplayFiltersOptions?.display_filters.sub_group_by ?? [];
  if (!hasTypedWorkflow) return base.filter((key) => key !== "state_detail.group" && key !== "workflow_state");
  return [
    ...new Set<TIssueGroupByOptions>([
      ...base.filter((key) => key !== "state"),
      "state_detail.group",
      "workflow_state",
    ]),
  ];
}, [layoutDisplayFiltersOptions, hasTypedWorkflow]);
```

Ganti props `DisplayFiltersSelection`:

```tsx
<DisplayFiltersSelection
  layoutDisplayFiltersOptions={layoutDisplayFiltersOptions}
  displayFilters={effectiveDisplayFilters ?? {}}
  groupByOptions={groupByOptions}
  subGroupByOptions={subGroupByOptions}
  handleDisplayFiltersUpdate={handleDisplayFilters}
  displayProperties={issueFilters?.displayProperties ?? {}}
  handleDisplayPropertiesUpdate={handleDisplayProperties}
  cycleViewDisabled={!currentProjectDetails?.cycle_view}
  moduleViewDisabled={!currentProjectDetails?.module_view}
  isEpic={storeType === EIssuesStoreType.EPIC}
/>
```

- [ ] **Step 2: Teruskan opsi custom di DisplayFiltersSelection**

`display-filters-selection.tsx`:

Tambahkan ke `Props`:

```ts
  groupByOptions?: TIssueGroupByOptions[];
  subGroupByOptions?: TIssueGroupByOptions[];
```

Destructure keduanya, lalu:

```tsx
<FilterGroupBy
  displayFilters={displayFilters}
  groupByOptions={groupByOptions ?? layoutDisplayFiltersOptions?.display_filters.group_by ?? []}
  handleUpdate={(val) =>
    handleDisplayFiltersUpdate({
      group_by: val,
    })
  }
  ignoreGroupedFilters={[...ignoreGroupedFilters, ...computedIgnoreGroupedFilters]}
/>
```

dan:

```tsx
<FilterSubGroupBy
  displayFilters={displayFilters}
  handleUpdate={(val) =>
    handleDisplayFiltersUpdate({
      sub_group_by: val,
    })
  }
  subGroupByOptions={subGroupByOptions ?? layoutDisplayFiltersOptions?.display_filters.sub_group_by ?? []}
  ignoreGroupedFilters={[...ignoreGroupedFilters, ...computedIgnoreGroupedFilters]}
/>
```

- [ ] **Step 3: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/components/issues/filters.tsx apps/web/core/components/issues/issue-layouts/filters/header/display-filters/display-filters-selection.tsx
git commit -m "feat(web): dynamic grouping options for typed projects"
```

---

### Task 8: Filter state — label type, scope, pruning

**Files:**

- Modify: `apps/web/core/store/workflow.helpers.ts` (+ tes)
- Modify: `packages/utils/src/work-item-filters/configs/filters/state.ts:74-100`
- Modify: `apps/web/core/components/work-item-filters/filters-hoc/base.tsx:49-111`
- Modify: `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx:80-95,126-130,176-187`

- [ ] **Step 1: Tulis tes helper scope + prune**

Tambahkan import di `workflow.helpers.test.ts`:

```ts
import { pruneStateFilterValues, scopeStateIdsForTypes } from "./workflow.helpers";
```

lalu describe:

```ts
describe("scopeStateIdsForTypes", () => {
  const getStateById = (stateId: string) =>
    ({
      "l-1": { type_id: null },
      "p-1": { type_id: "type-p" },
      "c-1": { type_id: "type-c" },
    })[stateId];

  it("tanpa selected type mengembalikan apa adanya", () => {
    expect(scopeStateIdsForTypes(["l-1", "p-1"], [], getStateById)).toEqual(["l-1", "p-1"]);
  });

  it("menyaring ke state milik type terpilih dan membuang legacy", () => {
    expect(scopeStateIdsForTypes(["l-1", "p-1", "c-1"], ["type-p"], getStateById)).toEqual(["p-1"]);
  });

  it("undefined tetap undefined", () => {
    expect(scopeStateIdsForTypes(undefined, ["type-p"], getStateById)).toBeUndefined();
  });
});

describe("pruneStateFilterValues", () => {
  it("membuang state yang tidak diizinkan dari state_id__in", () => {
    expect(
      pruneStateFilterValues({ and: [{ state_id__in: "s-1,s-2" }, { priority__in: "urgent" }] }, new Set(["s-2"]))
    ).toEqual({ and: [{ state_id__in: "s-2" }, { priority__in: "urgent" }] });
  });

  it("menghapus condition state yang kosong dan menormalkan grup kosong", () => {
    expect(pruneStateFilterValues({ and: [{ state_id__in: "s-1" }] }, new Set(["s-9"]))).toEqual({});
  });

  it("menangani state_id__exact dan field lain yang tidak disentuh", () => {
    expect(
      pruneStateFilterValues({ and: [{ state_id__exact: "s-1" }, { state_group__in: "backlog" }] }, new Set([]))
    ).toEqual({ and: [{ state_group__in: "backlog" }] });
  });

  it("undefined tetap undefined", () => {
    expect(pruneStateFilterValues(undefined, new Set(["s-1"]))).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run tes untuk memastikan gagal**

Run: `pnpm --filter=web test workflow.helpers`

Expected: FAIL — fungsi belum ada.

- [ ] **Step 3: Implementasi helper scope + prune**

Tambahkan di `workflow.helpers.ts`:

```ts
/** Saring state ids ke type terpilih; legacy (tanpa type) dibuang saat type difilter. */
export const scopeStateIdsForTypes = (
  stateIds: string[] | undefined,
  selectedTypeIds: readonly string[],
  getStateById: (stateId: string) => Partial<Pick<IState, "type_id">> | undefined
): string[] | undefined => {
  if (!stateIds || selectedTypeIds.length === 0) return stateIds;
  const allowed = new Set(selectedTypeIds);
  return stateIds.filter((stateId) => {
    const typeId = getStateById(stateId)?.type_id;
    return typeId ? allowed.has(typeId) : false;
  });
};

const pruneStateCondition = (
  condition: Record<string, unknown>,
  allowedStateIds: ReadonlySet<string>
): Record<string, unknown> => {
  const next: Record<string, unknown> = { ...condition };
  for (const key of ["state_id", "state_id__exact", "state_id__in"]) {
    if (!(key in next)) continue;
    const raw = next[key];
    if (key === "state_id__in") {
      const values = (Array.isArray(raw) ? raw : String(raw).split(","))
        .map((value) => String(value).trim())
        .filter((value) => value.length > 0 && allowedStateIds.has(value));
      if (values.length > 0) next[key] = values.join(",");
      else delete next[key];
    } else {
      const value = Array.isArray(raw) ? raw[0] : raw;
      if (value == null || !allowedStateIds.has(String(value))) delete next[key];
    }
  }
  return next;
};

const pruneFilterNode = (node: unknown, allowedStateIds: ReadonlySet<string>): unknown => {
  if (Array.isArray(node)) return node.map((child) => pruneFilterNode(child, allowedStateIds));
  if (node && typeof node === "object") {
    const record = node as Record<string, unknown>;
    const next: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(record)) {
      if (key === "and" || key === "or") {
        const children = (Array.isArray(value) ? value : [])
          .map((child) => pruneFilterNode(child, allowedStateIds))
          .filter((child) => typeof child === "object" && child !== null && Object.keys(child as object).length > 0);
        if (children.length > 0) next[key] = children;
        continue;
      }
      if (key === "not") {
        const child = pruneFilterNode(value, allowedStateIds);
        if (typeof child === "object" && child !== null && Object.keys(child as object).length > 0) next[key] = child;
        continue;
      }
      next[key] = value;
    }
    return pruneStateCondition(next, allowedStateIds);
  }
  return node;
};

/** Buang nilai filter state yang tidak lagi valid (mis. setelah filter type berubah). */
export const pruneStateFilterValues = (
  expression: TWorkItemFilterExpression | undefined,
  allowedStateIds: ReadonlySet<string>
): TWorkItemFilterExpression | undefined => {
  if (!expression) return expression;
  return (pruneFilterNode(expression, allowedStateIds) ?? {}) as TWorkItemFilterExpression;
};
```

- [ ] **Step 4: Run tes untuk memastikan lulus**

Run: `pnpm --filter=web test workflow.helpers`

Expected: PASS.

- [ ] **Step 5: Tambah label opsi custom di config state**

`packages/utils/src/work-item-filters/configs/filters/state.ts`:

```ts
export type TCreateStateFilterParams = TCreateFilterConfigParams &
  IFilterIconConfig<IState> & {
    states: IState[];
    getOptionLabel?: (state: IState) => string;
  };
```

dan di `getStateMultiSelectConfig`:

```ts
      getId: (state) => state.id,
      getLabel: (state) => (params.getOptionLabel ? params.getOptionLabel(state) : state.name),
      getValue: (state) => state.id,
```

- [ ] **Step 6: Scope + label di hook config**

`use-work-item-filters-config.tsx`:

Tambahkan `TWorkItemFiltersEntityProps` prop baru (di tipe, lihat deklarasi `TUseWorkItemFiltersConfigProps`):

```ts
export type TUseWorkItemFiltersConfigProps = {
  allowedFilters: TWorkItemFilterProperty[];
  selectedTypeIds?: string[];
} & TWorkItemFiltersEntityProps;
```

Destructure `selectedTypeIds` di hook, lalu setelah `workItemStates` lama ganti menjadi:

```ts
const selectedTypeIdSet = useMemo(() => new Set(selectedTypeIds ?? []), [selectedTypeIds]);
const typeNameByTypeId = useMemo(
  () => new Map(workItemTypes.map((type) => [type.type_id, type.type_name])),
  [workItemTypes]
);
const workItemStates: IState[] | undefined = useMemo(() => {
  if (!stateIds) return undefined;
  const scopedStateIds =
    scopeStateIdsForTypes(
      stateIds,
      [...selectedTypeIdSet],
      getStateById as (stateId: string) => Partial<Pick<IState, "type_id">> | undefined
    ) ?? [];
  return scopedStateIds.map((stateId) => getStateById(stateId)).filter((state): state is IState => Boolean(state));
}, [stateIds, selectedTypeIdSet, getStateById]);
const shouldPrefixStateType = workItemTypes.length > 0 && selectedTypeIdSet.size !== 1;
const getStateOptionLabel = useCallback(
  (state: IState) => {
    const typeName = state.type_id ? typeNameByTypeId.get(state.type_id) : undefined;
    return typeName && shouldPrefixStateType ? `${typeName} · ${state.name}` : state.name;
  },
  [typeNameByTypeId, shouldPrefixStateType]
);
```

import `scopeStateIdsForTypes` dari `@/store/workflow.helpers`, dan `useCallback` bila belum ada.

Lalu di `stateFilterConfig`:

```ts
      getStateFilterConfig<TWorkItemFilterProperty>("state_id")({
        isEnabled: isFilterEnabled("state_id") && workItemStates !== undefined,
        filterIcon: StateOutline,
        getOptionIcon: (state) => <StateGroupIcon stateGroup={state.group} color={state.color} />,
        getOptionLabel: getStateOptionLabel,
        states: workItemStates ?? [],
        ...operatorConfigs,
      }),
```

dan dependency `useMemo` ditambah `getStateOptionLabel`.

- [ ] **Step 7: Live expression + prune di WorkItemFilterRoot**

`filters-hoc/base.tsx`:

Import tambahan:

```ts
import { isEqual } from "lodash-es";
import { getWorkItemTypeIds, pruneStateFilterValues } from "@/store/workflow.helpers";
import { useProjectState } from "@/hooks/store/use-project-state";
```

Di `WorkItemFilterRoot`, pindahkan pembuatan instance ke **atas** blok config dan **hapus** blok `workItemLayoutFilter` yang lama (baris 77-92), lalu susun ulang menjadi:

```ts
// get or create filter instance
const workItemLayoutFilter = useMemo(
  () =>
    getOrCreateFilter({
      entityType,
      entityId: workItemEntityID,
      initialExpression: initialUserFilters,
      onExpressionChange: updateFilters,
      expressionOptions: {
        saveViewOptions,
        updateViewOptions,
      },
      showOnMount,
    }),
  // eslint-disable-next-line react-hooks/exhaustive-deps
  [entityType, workItemEntityID, saveViewOptions, updateViewOptions, updateFilters]
);

const { getStateById } = useProjectState();
const { stateIds } = entityConfigProps;
const liveExpression = workItemLayoutFilter.adapter.toExternal(workItemLayoutFilter.expression);
const selectedTypeIds = useMemo(() => getWorkItemTypeIds({ richFilters: liveExpression }), [liveExpression]);

// keep the persisted expression consistent when the active type filter narrows the state options
useEffect(() => {
  if (!stateIds || stateIds.length === 0 || selectedTypeIds.length === 0) return;
  const allowedStateIds = new Set(scopeStateIdsForTypes(stateIds, selectedTypeIds, getStateById) ?? []);
  const prunedExpression = pruneStateFilterValues(liveExpression, allowedStateIds);
  if (prunedExpression && !isEqual(prunedExpression, liveExpression)) {
    workItemLayoutFilter.resetExpression(prunedExpression, false);
  }
}, [liveExpression, selectedTypeIds, stateIds, getStateById, workItemLayoutFilter]);

const workItemFiltersConfig = useWorkItemFiltersConfig({
  allowedFilters: filtersToShowByLayout ? filtersToShowByLayout : [],
  selectedTypeIds,
  ...entityConfigProps,
});
```

Tambahkan `scopeStateIdsForTypes` ke import helper di atas. Pastikan efek `registerAll` yang lama tetap setelah blok ini.

- [ ] **Step 8: Typecheck + tes**

Run: `pnpm --filter=web test workflow.helpers && pnpm check:types`

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add apps/web/core/store/workflow.helpers.ts apps/web/core/store/workflow.helpers.test.ts packages/utils/src/work-item-filters/configs/filters/state.ts apps/web/core/components/work-item-filters/filters-hoc/base.tsx apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx
git commit -m "feat(web): scope and label state filter by work item type"
```

---

### Task 9: Create modal derive type dari state typed

**Files:**

- Modify: `apps/web/core/components/issues/issue-modal/components/default-properties.tsx:32-41,81-96`

- [ ] **Step 1: Derive type**

Tambahkan import hook:

```ts
import { useProjectState } from "@/hooks/store/use-project-state";
```

Di dalam komponen, setelah `const { fetchWorkflowMap, getWorkflowMap } = useWorkflow();`:

```ts
const { getStateById } = useProjectState();
```

Setelah `const typeId = watch("type_id");` tambahkan:

```ts
const stateId = watch("state_id");
```

dan setelah efek workflow-map (baris ~118), tambahkan efek baru:

```ts
// a quick-add column can prefill a typed state without its type (e.g. composite
// workflow state column); derive the type so create never sends an invalid pair
useEffect(() => {
  if (id || typeId || !stateId) return;
  const derivedTypeId = getStateById(stateId)?.type_id;
  if (derivedTypeId) setValue("type_id", derivedTypeId, { shouldValidate: true });
}, [id, typeId, stateId, getStateById, setValue]);
```

- [ ] **Step 2: Typecheck**

Run: `pnpm check:types`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/core/components/issues/issue-modal/components/default-properties.tsx
git commit -m "fix(web): derive work item type from prefilled state"
```

---

### Task 10: Verifikasi akhir + E2E smoke

**Files:** tidak ada perubahan kode.

- [ ] **Step 1: Check penuh**

Run:

```bash
pnpm check
pnpm --filter=@plane/i18n check:sync
```

Expected: PASS (format, lint, types, sinkronisasi locale).

- [ ] **Step 2: Build web prod dan restart service**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

Expected: build sukses; service aktif (`systemctl --user status plane-web-prod.service`).

- [ ] **Step 3: E2E smoke di workspace `terraline-demo` (project Terra)**

1. Board default (mixed, `group_by: "state"` tersimpan) → 5 kolom `Backlog/Unstarted/Started/Completed/Cancelled`, tidak ada "Baru" ganda.
2. Filter satu type (Problem) → kolom berubah jadi state milik Problem (`Baru`, `Investigasi`, `Known Error`, `Ditutup`).
3. Ganti grouping ke "Workflow states" → kolom komposit (`Problem · Baru`, `Change · Baru`, legacy `Baru` polos), urut legacy lalu per type.
4. Quick-add (+) dari kolom komposit → modal terbuka dengan state kolom + type terisi; submit sukses (tidak ada error "State is not valid for this work item type").
5. Drag di "Workflow states" dari kolom Problem ke kolom Change → ditolak + toast "Target state belongs to a different work item type"; transisi valid (Problem `Baru` → `Investigasi`) tetap jalan.
6. Drag di 5-group view lintas kolom → toast info "Switch to ..." dan kartu tidak berpindah state.
7. Filter state: opsi berlabel (`Problem · Baru`), scope mengikuti filter type aktif, chip berlabel sama, dan nilai state yang tidak valid hilang saat filter type diganti.
8. Regresi: project tanpa type aktif (mis. Cardlink) → grouping "State" tetap seperti sebelumnya; workspace Global/My issues, sub-issues, inbox, archived tidak berubah.

- [ ] **Step 4: Catat hasil**

Tulis ringkasan hasil smoke (tanggal, workspace, temuan) ke deskripsi PR saat PR dibuat — **jangan** commit perubahan kode tambahan bila tidak ada temuan. Bila ada bug, buat task perbaikan baru mengikuti debugging skill; jangan patch langsung di task verifikasi.

---

## Self-Review

- **Spec coverage:** nilai `workflow_state` + opsi dropdown (Task 1, 7); resolver efektif + kompatibilitas saved view (Task 2, 3, 5); kolom komposit + default 5 group (Task 1, 2, 4, 5); drag nonaktif di 5-group + guard lintas type (Task 5, 6); label/scope/prune filter state (Task 2-8); quick-add + derive type (Task 4, 9); i18n + verifikasi (Task 1, 10). Tidak ada perubahan backend — sesuai spec.
- **Placeholder scan:** tidak ada TBD; semua step berisi kode/command nyata.
- **Type consistency:** `resolveEffectiveDisplayFilters`, `resolveWorkflowStateColumns`, `getWorkItemTypeIds`, `scopeStateIdsForTypes`, `pruneStateFilterValues`, `workflowDropErrorMessage`, `selectedTypeIds`, `getOptionLabel` konsisten antar task.
- **Catatan urutan:** Task 3 dan 4 tidak bergantung pada Task 2 secara runtime, tetapi keduanya mengimpor helper dari Task 2 — kerjakan berurutan.
