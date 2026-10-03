# Hapus Workflow & Flatten State — Plan 3: Web & Packages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menghapus seluruh konsumsi workflow/transisi/workflow-map dari web, mengganti grouping `workflow_state` dengan state flat, menyederhanakan state store/type store/dropdown/modal/settings, dan membersihkan packages + i18n.

**Architecture:** Store workflow dipecah: type saja (`work-item-type.store.ts`), state tetap flat (`state.store.ts` tanpa typed-mirror). Helper workflow-map dihapus; `getWorkItemTypeIds` dipertahankan type-only. `pnpm check:types` menjadi penggerak utama: setelah fondasi (types/constants/store/service) diubah, error typecheck menuntun pembersihan consumer per file.

**Tech Stack:** React + Vite + MobX + TypeScript, pnpm workspace, i18n JSON per locale.

**Spec:** `docs/superpowers/specs/2026-10-03-remove-workflows-flatten-states-design.md`
**Prasyarat:** Plan 1 (Django) & Plan 2 (api-rs) selesai; backend baru sudah berjalan saat smoke test akhir.

---

## File Structure

| File                                                                             | Aksi   | Tanggung jawab                                           |
| -------------------------------------------------------------------------------- | ------ | -------------------------------------------------------- |
| `packages/types/src/workflow/workflow.ts`                                        | Delete | TWorkflow\* tidak dipakai                                |
| `packages/types/src/workflow/work-item-type.ts`                                  | Modify | Buang `workflow`                                         |
| `packages/types/src/state.ts`                                                    | Modify | Buang `type_id`/`workflow_state_id`                      |
| `packages/types/src/view-props.ts`, `issues.ts`                                  | Modify | Buang axis/param workflow                                |
| `packages/constants/src/state.ts`                                                | Modify | Buang `DISPLAY_WORKFLOW_PRO_CTA`                         |
| `packages/constants/src/issue/common.ts`, `filter.ts`                            | Modify | Buang `workflow_state` + mapping                         |
| `packages/constants/src/fetch-keys.ts`                                           | Modify | Buang key workflow                                       |
| `apps/web/core/services/work-item-type/work-item-type.service.ts`                | Create | Pindahan method type dari workflow.service               |
| `apps/web/core/services/workflow/`                                               | Delete | Tidak ada lagi                                           |
| `apps/web/core/store/work-item-type.store.ts`                                    | Create | Store type saja                                          |
| `apps/web/core/store/workflow.store.ts`, `workflow.helpers.ts`                   | Delete | Diganti                                                  |
| `apps/web/core/store/work-item-type.helpers.ts`                                  | Create | `getSingleWorkItemTypeId`/`getWorkItemTypeIds` type-only |
| `apps/web/core/store/state.store.ts`                                             | Modify | Buang typed mirror                                       |
| `apps/web/core/store/root.store.ts`, `hooks/store/use-workflow.ts`               | Modify | Ganti ke store/hook type                                 |
| `apps/web/core/layouts/auth-layout/project-wrapper.tsx`, `workspace-wrapper.tsx` | Modify | Buang fetch workflow-map/workspace states                |
| `apps/web/core/components/workflows/`, `workflow/`                               | Delete | Editor/transisi/drag hook                                |
| `apps/web/core/components/work-item-types/`, `project-work-item-types/`          | Modify | Buang workflow                                           |
| `apps/web/core/components/project-states/`                                       | Modify | Flat state manager                                       |
| `apps/web/core/components/dropdowns/{state,work-item-type}/`                     | Modify | Flat options                                             |
| `apps/web/core/components/issues/**`                                             | Modify | Grouping/filter/modal/quick-add                          |
| `apps/web/core/store/issue/**`                                                   | Modify | Filter store/helper flat                                 |
| `packages/i18n/**`                                                               | Modify | Hapus namespace workflow + key mati                      |
| Test store web                                                                   | Modify | Sesuaikan/hapus                                          |

**Urutan milestone:** C1 (packages foundation) → C2 (services/store) → C3 (layout/filter/grouping) → C4 (modal/quick-add/dropdown) → C5 (settings UI) → C6 (i18n) → C7 (test + typecheck + build + smoke). Setiap task diakhiri `pnpm check:types` (error tersisa boleh ada hanya bila task berikutnya yang membereskannya, dan harus nol di akhir C3).

---

## Task C1: Packages foundation (types, constants)

**Files:**

- Delete: `packages/types/src/workflow/workflow.ts`
- Modify: `packages/types/src/workflow/work-item-type.ts`, `packages/types/src/workflow/index.ts`, `packages/types/src/state.ts`, `packages/types/src/view-props.ts`, `packages/types/src/issues.ts`, `packages/types/src/index.ts`
- Modify: `packages/constants/src/state.ts`, `packages/constants/src/issue/common.ts`, `packages/constants/src/issue/filter.ts`, `packages/constants/src/fetch-keys.ts`

- [ ] **Step 1: Hapus tipe workflow**

```bash
git rm packages/types/src/workflow/workflow.ts
```

Di `packages/types/src/workflow/index.ts`, sisakan ekspor `./work-item-type` saja. Di `packages/types/src/workflow/work-item-type.ts`, hapus field `workflow: string | null` dari `TWorkItemType` dan `workflow` dari `TWorkItemTypePayload`. Pindahkan file ke `packages/types/src/work-item-type.ts` (opsional; bila dipindah, perbarui `packages/types/src/index.ts` dan seluruh import `@plane/types` — import lewat barrel sehingga tidak ada yang berubah).

- [ ] **Step 2: Bersihkan `IState`**

Di `packages/types/src/state.ts`, hapus `type_id?: string` dan `workflow_state_id?: string` dari `IState`. Pertahankan `TStateGroups`, `IStateLite`, `IStateResponse`, `TStateOperationsCallbacks`.

- [ ] **Step 3: Bersihkan axis/param workflow**

- `packages/types/src/view-props.ts`: hapus `"workflow_state"` dari `TIssueGroupByOptions`; hapus `state_group`/`state` hanya bila tidak lagi dipakai — **pertahankan** `state`, `state_group`, `state_id`, `type_id` (state flat tetap ada). Hapus referensi tipe workflow lain bila ada.
- `packages/types/src/issues.ts`: pastikan `GroupByColumnTypes` konsisten dengan `TIssueGroupByOptions` (tanpa `workflow_state`).
- `packages/constants/src/state.ts`: hapus `DISPLAY_WORKFLOW_PRO_CTA`; `STATE_GROUPS` dan turunannya tetap.
- `packages/constants/src/issue/common.ts`: hapus entry `workflow_state` dari `EIssueGroupByToServerOptions` dan `ISSUE_GROUP_BY_OPTIONS`; `state` → `state_id`, `state_detail.group` → `state__group` tetap.
- `packages/constants/src/issue/filter.ts`: hapus `workflow_state` dari daftar group-by per halaman; pertahankan `state`/`state_detail.group`.
- `packages/constants/src/fetch-keys.ts`: hapus `WORKSPACE_WORKFLOWS`, `WORKSPACE_WORKFLOW_STATES`, `PROJECT_WORKFLOWS`, `PROJECT_WORKFLOW_MAP`; `PROJECT_STATES`, `WORKSPACE_STATES`, `PROJECT_INTAKE_STATE` tetap.

- [ ] **Step 4: Verifikasi**

Run:

```bash
rg -n "workflow" packages/types/src packages/constants/src
pnpm check:types
```

Expected: tidak ada tipe/konstanta workflow tersisa (hit `./workflow` pada baris import folder boleh ada bila folder `types/src/workflow/` dipertahankan); typecheck gagal di `apps/web` (consumer) — itu target task berikutnya.

- [ ] **Step 5: Commit**

```bash
git add packages/types packages/constants
git commit -m "refactor(packages): remove workflow types and constants"
```

---

## Task C2: Service & store type-only

**Files:**

- Create: `apps/web/core/services/work-item-type/work-item-type.service.ts` + `index.ts`
- Delete: `apps/web/core/services/workflow/`
- Create: `apps/web/core/store/work-item-type.store.ts`
- Create: `apps/web/core/store/work-item-type.helpers.ts`
- Delete: `apps/web/core/store/workflow.store.ts`, `workflow.helpers.ts`, `workflow.store.test.ts`, `workflow.helpers.test.ts`
- Modify: `apps/web/core/store/state.store.ts`, `root.store.ts`, `apps/web/core/hooks/store/use-workflow.ts`, `use-project-state.ts`
- Modify: `apps/web/core/layouts/auth-layout/project-wrapper.tsx`, `workspace-wrapper.tsx`

- [ ] **Step 1: Pindahkan service type**

Buat `work-item-type.service.ts` dengan class `WorkItemTypeService extends APIService` berisi **hanya** method `getWorkItemTypes`, `createWorkItemType`, `updateWorkItemType`, `deleteWorkItemType`, `importWorkItemTypes`, `unlinkWorkItemType` (salin persis dari `workflow.service.ts:103-155`). Hapus folder `services/workflow/`.

- [ ] **Step 2: Buat store type**

```ts
// apps/web/core/store/work-item-type.store.ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { action, makeObservable, observable, runInAction } from "mobx";
import type { TWorkItemType, TWorkItemTypePayload } from "@plane/types";
import { WorkItemTypeService } from "@/services/work-item-type";
import type { CoreRootStore } from "./root.store";

export interface IWorkItemTypeStore {
  workItemTypes: TWorkItemType[] | undefined;
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
}

export class WorkItemTypeStore implements IWorkItemTypeStore {
  workItemTypes: TWorkItemType[] | undefined = undefined;
  private service = new WorkItemTypeService();
  private _rootStore: CoreRootStore;

  constructor(_rootStore: CoreRootStore) {
    this._rootStore = _rootStore;
    makeObservable(this, {
      workItemTypes: observable,
      fetchWorkItemTypes: action,
      createWorkItemType: action,
      updateWorkItemType: action,
      deleteWorkItemType: action,
      importWorkItemTypes: action,
      unlinkWorkItemType: action,
    });
  }

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
  };

  unlinkWorkItemType = async (workspaceSlug: string, projectId: string, typeId: string) => {
    await this.service.unlinkWorkItemType(workspaceSlug, projectId, typeId);
  };
}
```

- [ ] **Step 3: Buat helper type-only**

```ts
// apps/web/core/store/work-item-type.helpers.ts
import type { IIssueFilterOptions, IIssueFilters, TWorkItemFilterExpression } from "@plane/types";

type TLegacyIssueFilterBag = { filters?: IIssueFilterOptions | null };
type TRichIssueFilterBag = { richFilters?: TWorkItemFilterExpression };

const collectTypeIds = (node: unknown, out: Set<string>): void => {
  if (!node || typeof node !== "object") return;
  const record = node as Record<string, unknown>;
  const andChildren = record.and;
  if (Array.isArray(andChildren)) {
    andChildren.forEach((child) => collectTypeIds(child, out));
    return;
  }
  for (const key of ["type_id", "type_id__exact", "type_id__in"] as const) {
    const raw = record[key];
    if (raw === undefined || raw === null) continue;
    const value = Array.isArray(raw) ? raw.join(",") : String(raw);
    value
      .split(",")
      .map((part) => part.trim())
      .filter((part) => part.length > 0)
      .forEach((part) => out.add(part));
  }
};

/** Type tunggal efektif dari filter board (rich atau legacy). */
export const getSingleWorkItemTypeId = (
  issueFilters: TLegacyIssueFilterBag | TRichIssueFilterBag | IIssueFilters | null | undefined
): string | null => {
  if (!issueFilters) return null;
  if ("filters" in issueFilters) {
    const legacyTypeIds = issueFilters.filters?.issue_type;
    return legacyTypeIds?.length === 1 ? (legacyTypeIds[0] ?? null) : null;
  }
  const ids = new Set<string>();
  collectTypeIds((issueFilters as TRichIssueFilterBag).richFilters, ids);
  return ids.size === 1 ? ([...ids][0] ?? null) : null;
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

Hapus `workflow.store.ts`, `workflow.helpers.ts`, dan kedua test-nya.

- [ ] **Step 4: Bersihkan `state.store.ts`**

Hapus seluruh logika typed mirror: field tombstone/epoch typed (sekitar baris 73-109), filter mirror di `getStatePercentageInGroup` (417-435), dan referensi `workflow_state_id`/`isTypedState`. `fetchProjectStates`, CRUD state, `markStateAsDefault`, `moveStatePosition`, `intakeStateMap`, `workspaceStates` tetap. `getStatePercentageInGroup` menghitung persentase tanpa filter typed.

- [ ] **Step 5: Ganti registrasi store & hook**

- `root.store.ts`: ganti `workflow: IWorkflowStore` → `workItemType: IWorkItemTypeStore`; konstruksi `new WorkItemTypeStore(this)`; semua `fetchWorkflowMap`/`fetchWorkflows` di init dihapus.
- `hooks/store/use-workflow.ts` → `use-work-item-type.ts` dengan `useWorkItemType()` mengembalikan `context.workItemType`.
- `project-wrapper.tsx`: hapus blok `useSWR(... PROJECT_WORKFLOW_MAP ...)` dan destructuring `fetchWorkflowMap`. `PROJECT_STATES` + `PROJECT_INTAKE_STATE` tetap.
- `workspace-wrapper.tsx`: hapus fetch `WORKSPACE_STATES` dan destructuring `fetchWorkspaceStates` **bila** `rg "workspaceStates|fetchWorkspaceStates" apps/web/core` tidak menemukan konsumen lain; bila ada, pertahankan fetch.

- [ ] **Step 6: Verifikasi**

Run:

```bash
rg -n "workflowMap|WorkflowStore|workflow\.store|workflow\.service|workflow\.helpers" apps/web/core apps/web/app
pnpm check:types
```

Expected: `rg` menyisakan error consumer UI (task C3-C5); store/service bersih.

- [ ] **Step 7: Commit**

```bash
git add apps/web/core/services apps/web/core/store apps/web/core/hooks apps/web/core/layouts
git commit -m "refactor(web): replace workflow store with work item type store"
```

---

## Task C3: Layout, grouping, filter flat

**Files:**

- Modify: `apps/web/core/components/issues/issue-layouts/utils.tsx`, `apps/web/core/components/issues/filters.tsx`
- Modify: `apps/web/core/store/issue/helpers/issue-filter-helper.store.ts`, `base-issues.store.ts`, `base-issues-utils.ts`
- Modify: `apps/web/core/store/issue/**/filter.store.ts` (project, project-views, cycle, module, archived, workspace)
- Modify: `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`
- Modify: `apps/web/core/components/work-item-filters/filters-hoc/base.tsx`
- Modify: `apps/web/core/components/work-item-filters/filters-hoc/filters-row.tsx` bila memakai helper workflow

- [ ] **Step 1: Sederhanakan `issue-layouts/utils.tsx`**

- `getStateColumns`: hapus parameter/`mapType` dan panggilan `resolveStateColumns`; kolom = `projectStates` yang sudah terurut (`sequence`) tanpa rewrite metadata.
- `getGroupByColumns`: hapus cabang `workflow_state`; `state` tetap memakai kolom state, `state_detail.group` tetap memakai `STATE_GROUPS`.
- Hapus import dari `workflow.helpers` dan ganti ke `work-item-type.helpers` (`getSingleWorkItemTypeId`).

- [ ] **Step 2: Bersihkan filter helper & filter store**

- `issue-filter-helper.store.ts`: hapus import/pemanggilan `resolveEffectiveDisplayFilters` dan parameter workflow map; `state` → `state_id` tetap dari `EIssueGroupByToServerOptions`.
- Semua `filter.store.ts`: hapus `getWorkflowMap(projectId)` dari query params dan `resolveEffectiveDisplayFilters`; default kanban `group_by === null` → `"state"` tetap.
- `filters.tsx`: hapus `getSingleWorkItemTypeId`/`hasTypedWorkflows` untuk swap group-by; opsi group-by tidak lagi menampilkan `workflow_state`.
- `base-issues.store.ts`/`base-issues-utils.ts`: hapus cabang `workflow_state`; `state_detail.group` → `state__group` tetap.

- [ ] **Step 3: Bersihkan config filter & prune**

- `use-work-item-filters-config.tsx`: state options diambil dari `state.store.projectStates` tanpa scoping type; hapus `scopeStateIdsForTypes` dan prefix nama type pada label state. Filter `type_id` tetap dari store type (`workItemTypes`), bukan workflow map.
- `filters-hoc/base.tsx`: hapus efek `scopeStateIdsForTypes`/`pruneStateFilterValues`.
- `packages/utils/src/work-item-filters/configs/filters/work-item-type.ts`: ganti sumber opsi type menjadi daftar `TWorkItemType` dari store (bukan workflow map); hapus referensi `TWorkflowMap`.

- [ ] **Step 4: Verifikasi**

Run:

```bash
rg -n "workflow_state|resolveEffectiveDisplayFilters|hasTypedWorkflows|resolveWorkflowStateColumns|scopeStateIdsForTypes|pruneStateFilterValues|WorkflowMap" apps/web/core packages/utils/src
pnpm check:types
```

Expected: `rg` kosong; typecheck nol error untuk area layout/filter.

- [ ] **Step 5: Commit**

```bash
git add apps/web/core/components/issues apps/web/core/store/issue apps/web/core/hooks apps/web/core/components/work-item-filters packages/utils
git commit -m "refactor(web): flatten issue grouping and filters"
```

---

## Task C4: Dropdown, modal, quick-add

**Files:**

- Modify: `apps/web/core/components/dropdowns/state/base.tsx`, `dropdown.tsx`
- Modify: `apps/web/core/components/dropdowns/work-item-type/dropdown.tsx`
- Modify: `apps/web/core/components/issues/issue-modal/components/default-properties.tsx`, `form.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/kanban/kanban-group.tsx`, `list/list-group.tsx`
- Modify: `apps/web/core/components/issues/issue-layouts/properties/all-properties.tsx`, `issue-detail/sidebar.tsx`, `peek-overview/properties.tsx`, `relations/properties.tsx`, `workspace-draft/draft-issue-properties.tsx`, `sub-issues/issues-list/properties.tsx`, `spreadsheet/columns/state-column.tsx`, `cycles/active-cycle/cycle-stats.tsx`

- [ ] **Step 1: Dropdown state flat**

- `dropdowns/state/base.tsx`: hapus import `allowedTargetStateIds`/`resolveSelectableStateIds`/`findWorkflowMapType` dan prop `workItemTypeId`; opsi = seluruh `projectStates` (opsi `state` store) diurutkan `sequence`; auto-select state `default` tetap.
- `dropdowns/state/dropdown.tsx`: buang pengambilan workflow map.
- `dropdowns/work-item-type/dropdown.tsx`: sumber opsi = `workItemType.workItemTypes` yang live + enabled di project (dari `project_ids`); hapus fallback workflow map dan kondisi "no typed workflows".

- [ ] **Step 2: Modal default properties**

`default-properties.tsx`:

- Hapus `fetchWorkflowMap`/retry (`shouldRetryWorkflowMapFetch`, `MAX_WORKFLOW_MAP_FETCH_RETRIES`) dan `getTypeDefaultStateId`.
- Saat type berubah, jangan ubah `state_id`; default state tetap dari `state.getProjectDefaultStateId(projectId)`.
- Hapus derivasi `type_id` dari state (baris ~132-138).
- Type picker tetap (172-192).

- [ ] **Step 3: Quick-add**

- `kanban-group.tsx` dan `list/list-group.tsx`: prefill `state_id` dari default state project; hapus `getTypeDefaultStateId`; payload `type_id` tetap bila type dipilih pengguna.
- `getGroupByColumns` payload: hapus `type_id` yang diturunkan dari kolom state typed.

- [ ] **Step 4: Consumer dropdown state**

Pada semua file di daftar task: hapus prop `workItemTypeId` (bila ada) dan import helper workflow; perilaku state tetap (dropdown menampilkan semua state project). `shouldHighlightIssueDueDate` tetap memakai `state.group`.

- [ ] **Step 5: Verifikasi**

Run:

```bash
rg -n "workItemTypeId=|getTypeDefaultStateId|fetchWorkflowMap|resolveSelectableStateIds|allowedTargetStateIds" apps/web/core/components
pnpm check:types
```

Expected: `rg` kosong; typecheck nol error untuk area dropdown/modal.

- [ ] **Step 6: Commit**

```bash
git add apps/web/core/components/dropdowns apps/web/core/components/issues
git commit -m "refactor(web): flat state dropdown and create defaults"
```

---

## Task C5: Settings UI (hapus workflow editor)

**Files:**

- Delete: `apps/web/core/components/workflows/`, `apps/web/core/components/workflow/`
- Modify: `apps/web/core/components/work-item-types/detail.tsx`, `root.tsx`, `type-list-item.tsx`, `type-form-modal.tsx`
- Modify: `apps/web/core/components/project-work-item-types/root.tsx`, `type-toggle-item.tsx`
- Modify: `apps/web/core/components/project-states/root.tsx`
- Modify: `apps/web/app/routes/core.ts` (route redirect workflow)
- Delete: `apps/web/app/routes/redirects/core/workflows.tsx` (opsional, lihat langkah 5)

- [ ] **Step 1: Hapus komponen workflow**

```bash
git rm -r apps/web/core/components/workflows apps/web/core/components/workflow
```

- [ ] **Step 2: Sederhanakan detail type**

`work-item-types/detail.tsx`: hapus import dan render `WorkflowEditor`; sisakan detail type (nama, deskripsi, ikon, toggle aktif, epic). Hapus teks "attach workflow first".

- [ ] **Step 3: Sederhanakan project type & states**

- `project-work-item-types/root.tsx`: hapus `fetchWorkflowMap`, `mapRefreshError`, toast refresh; `importWorkItemTypes`/`unlinkWorkItemType` tetap; setelah perubahan, refetch project states (`state.fetchProjectStates`).
- `type-toggle-item.tsx`: hapus nama workflow dan alasan "attach workflow first"; toggle hanya `is_active`/link project.
- `project-states/root.tsx`: hapus split typed vs legacy dan chip read-only typed; tampilkan daftar state flat yang bisa dikelola (group list + CRUD tetap).

- [ ] **Step 4: Hapus sisa referensi**

Run:

```bash
rg -n "WorkflowEditor|workflow.store|useWorkflow\(|workflowMap|workflowTransitions|workflowStates|TransitionMatrix" apps/web/core apps/web/app
```

Perbaiki/hapus setiap hit; ganti `useWorkflow()` dengan `useWorkItemType()` pada consumer type.

- [ ] **Step 5: Route redirect**

`app/routes/core.ts`: hapus route legacy `:workspaceSlug/settings/workflows/*` dan file `app/routes/redirects/core/workflows.tsx` + tipe generated-nya. Bila tidak yakin ada bookmark lama, pertahankan redirect tapi arahkan langsung ke `/settings/work-item-types/` (keputusan: **hapus** sesuai clean break).

- [ ] **Step 6: Verifikasi**

Run:

```bash
pnpm check:types
```

Expected: nol error.

- [ ] **Step 7: Commit**

```bash
git add -A apps/web/core/components apps/web/app
git commit -m "refactor(web): remove workflow editor UI"
```

---

## Task C6: i18n cleanup

**Files:**

- Delete: `packages/i18n/src/locales/*/workflow.json` (20 locale)
- Modify: `packages/i18n/src/constants/namespaces.ts`, locale JSON yang menyisakan key mati, `packages/i18n/src/types/keys.generated.ts`

- [ ] **Step 1: Muat skill translate**

Gunakan skill `translate` untuk seluruh perubahan locale (wajib sebelum menyentuh `src/locales`).

- [ ] **Step 2: Hapus namespace workflow**

- Hapus `workflow.json` di semua locale.
- Hapus registrasi namespace `workflow` di `constants/namespaces.ts`.
- Hapus key mati terkait workflow: `common.workflow_states`, `common.workflows`, `common.state_change_requires_workflow_state_grouping`, `common.workflow_state_wrong_type`, `project-settings.workflows.*`, `workspace-settings.settings.workflows.*`, `empty-state.settings_empty_state.workflows.*`; pertahankan key `states.*` dan `work_item_types.*` yang masih dipakai.

- [ ] **Step 3: Regenerate & verifikasi**

Run:

```bash
pnpm --filter=@plane/i18n generate:types || pnpm --filter=@plane/i18n build
rg -n "workflow_states|workflow_state_wrong_type|state_change_requires_workflow" packages/i18n/src/locales packages/i18n/src/types
```

Expected: regenerasi sukses; `rg` kosong.

- [ ] **Step 4: Commit**

```bash
git add packages/i18n
git commit -m "chore(i18n): remove workflow namespace and dead keys"
```

---

## Task C7: Test web + verifikasi penuh

**Files:**

- Modify: `apps/web/core/store/state.store.test.ts`
- Delete: `workflow.store.test.ts`, `workflow.helpers.test.ts` (bila belum terhapus di C2)

- [ ] **Step 1: Sesuaikan test store**

- `state.store.test.ts`: hapus test typed-mirror/`getStatePercentageInGroup` versi mirror; pertahankan test CRUD/intake.
- Tambah test kecil untuk `work-item-type.helpers.ts` (`getSingleWorkItemTypeId` legacy + rich) di `apps/web/core/store/work-item-type.helpers.test.ts`.

- [ ] **Step 2: Jalankan check & build**

Run:

```bash
pnpm check
pnpm --filter=web build
```

Expected: format/lint/types lulus; build web sukses (dengan `VITE_API_BASE_URL` sesuai env build produksi).

- [ ] **Step 3: Smoke lokal (dev server atau prod build + backend Plan 2)**

Prasyarat: backend Plan 2 berjalan dan DB sudah dimigrasi.

- Login, buka project, pastikan board by state menampilkan 5 state default + triage tersembunyi.
- Buat work item → masuk Backlog.
- Ubah state via dropdown → semua state project tersedia.
- Buka settings → work item types: CRUD type tanpa workflow; project states: CRUD state flat.
- Intake accept → issue pindah ke Backlog.
- Pastikan tidak ada request ke `/workflow-map/` atau `/workflows/` (cek network tab).

- [ ] **Step 4: Commit**

```bash
git add apps/web/core/store
git commit -m "test(web): align store tests with flat state model"
```

---

## Task C8: Rollout web (setelah backend Plan 2 live)

**Files:** tidak ada perubahan kode; operasional.

- [ ] **Step 1: Rebuild web produksi**

```bash
pnpm --filter=web build
systemctl --user restart plane-web-prod.service
```

- [ ] **Step 2: Verifikasi tunnel**

Buka `https://app.terraline.space` (tunnel), login, ulangi smoke C7 Step 3. Pastikan tidak ada error console terkait workflow.

- [ ] **Step 3: Catat rollback**

Rollback = restore dump DB + deploy ulang image/build sebelumnya (web & backend harus versi sama).

---

## Self-Review

- **Spec coverage:** C1 packages, C2 store/service, C3 layout/filter, C4 dropdown/modal, C5 settings, C6 i18n, C7 test/build/smoke, C8 rollout — seluruh poin web/packages di spec tercakup. Route `workflow_state` dihapus; `state`/`state_detail.group` tetap.
- **Placeholder scan:** tidak ada TBD; langkah penghapusan memakai daftar file + grep verifikasi eksplisit. Task C5 langkah 5 memuat keputusan eksplisit (hapus redirect).
- **Type consistency:** `IWorkItemTypeStore`/`WorkItemTypeStore`, `useWorkItemType`, `work-item-type.helpers` dipakai konsisten di C2-C5; helper `getSingleWorkItemTypeId`/`getWorkItemTypeIds` signature tidak berubah dari versi lama sehingga consumer tinggal ganti import path.
