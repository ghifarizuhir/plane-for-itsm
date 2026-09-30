# Workflow State Grouping & Filter Lintas Type — Design

Tanggal: 2026-09-29
Status: disetujui user saat brainstorming (arah A: rapikan tanpa shared state + label komposit + default 5 group di mixed), menunggu review spec tertulis.
Scope: web (`apps/web`, `packages/types`, `packages/constants`, `packages/utils`, `packages/i18n`). Tidak ada perubahan backend/schema/migrasi.
Terkait: `docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md`, `docs/superpowers/plans/2026-09-27-work-item-type-board-filter.md`.

## Latar

State di-materialize per work item type: satu row `states` project per `WorkflowState` dengan `type_id` + `workflow_state_id` sendiri (`apps/api-rs/crates/api/src/routes/workflow.rs:263`), dan nama state yang sama antar type legal (`apps/api/plane/db/models/state.py:128-132`). Akibatnya "Baru" adalah N row — di project demo Terra ada 5 ("Baru" milik Change, Incident, Problem, Service Request, plus legacy untyped), "Ditutup"/"Selesai" 4-5 row.

Gejala yang dilaporkan:

- **Group by State** di project issues page (default view project memakai `group_by: "state"`, `apps/api-rs/crates/api/src/seed.rs:210`) menampilkan kolom "Baru" berkali-kali. `getStateColumns` mengambil seluruh row state project; tanpa filter satu type, `resolveStateColumns` mengembalikan semuanya (`apps/web/core/components/issues/issue-layouts/utils.tsx:245-265`, `apps/web/core/store/workflow.helpers.ts:127-141`). Server membucket per `state_id` dengan universe seluruh row (`apps/api-rs/crates/api/src/routes/grouped.rs:391-400`).
- **Filter State** menampilkan opsi duplikat tanpa konteks type (`apps/web/core/components/issues/issue-layouts/filters-hoc/project-level.tsx:209`, `apps/web/core/store/state.store.ts:196-202`, `packages/utils/src/work-item-filters/configs/filters/state.ts:84-100`), chip name-only sehingga tak bisa dibedakan (`.../rich-filters/.../selected-options-display.tsx:27-29`).
- **Quick-add** dari kolom typed di mixed view mengirim `state_id` tanpa `type_id` (`utils.tsx:263`) dan ditolak backend "State is not valid for this work item type" (`apps/api-rs/crates/api/src/routes/issue_write.rs:115-134`).
- **Drag** antar kolom bernama sama ditolak backend (ownership check `apps/api-rs/crates/api/src/routes/workflow.rs:518-535`) tanpa guard UI (`apps/web/core/components/workflow/use-workflow-drag-n-drop.ts` masih stub no-op).
- Niat spec lama "mixed view = 5 kolom group" (`2026-09-25-work-item-types-workflows-design.md:27`) memblokir karena opsi `state_detail.group` tidak pernah ditawarkan di project page (`packages/constants/src/issue/filter.ts:221-246`).

Keputusan produk: **state tetap per type** (opsi B/C — merge logis by nama / shared state catalog — ditolak karena risiko semantik dan blast radius besar). Yang diperbaiki adalah UI grouping/filter agar state selalu membawa konteks type-nya, mirip praktik Jira (state group sebagai axis kasar lintas type) dan ServiceNow (state per modul, view type-scoped).

## Keputusan yang dikunci saat brainstorming

1. **Unit grouping/filter = workflow state type-scoped.** Tidak ada merge row, tidak ada shared state, tidak ada migrasi data.
2. **Label komposit** `{type_name} · {state_name}` untuk state typed di kolom, opsi filter, dan chip; legacy untyped tetap nama polos.
3. **Mixed view default 5 kolom state group.** Nilai `group_by: "state"` yang tersimpan (view lama/default) di-resolve runtime menjadi `state_detail.group` saat scope mixed.
4. **Raw type-scoped grouping tetap tersedia** lewat nilai baru `workflow_state` — opt-in eksplisit, kolom komposit.
5. **Single type**: `state` (dan `workflow_state`) menampilkan kolom state milik type itu, perilaku sekarang.
6. **Tanpa typed workflow**: tidak ada perubahan perilaku sama sekali.
7. **Filter Type tetap terpisah** ("semua Problem" tetap query sah); yang menyatu adalah state dengan type-nya.
8. **Drag di 5-group view tidak mengubah state** (hanya reorder + toast info); drag transisi penuh hanya di grouping `workflow_state`, dengan guard lintas type.
9. **Backend tidak disentuh** — `state_id`/`state__group` sudah didukung untuk filter, grouping, dan sorting.

## Non-goals

- Tidak mengubah schema, materialization, enforcement transisi, seed, atau migrasi.
- Tidak menyentuh sub-issues widget, inbox/intake, archived issues, analytics, export CSV/XLSX, workspace/global views.
- Tidak menambah grouping `type_id` di web (group by type + sub group by state).
- Tidak mengubah `state_group` filter (5 grup) yang sudah ada.
- Tidak menambah aturan transisi baru atau role restriction.

## Desain

### 1. Nilai grouping & aturan resolusi efektif

Nilai yang dikenal di display filters:

| Nilai tersimpan         | Arti                                                                                                   |
| ----------------------- | ------------------------------------------------------------------------------------------------------ |
| `state`                 | Legacy/alias. Di-resolve runtime (lihat tabel bawah). Opsi ini tidak ditawarkan lagi di project typed. |
| `state_detail.group`    | 5 kolom kanonik (`STATE_GROUPS`), server `state__group`.                                               |
| `workflow_state` (baru) | Kolom per state row type-scoped, label komposit, server `state_id`.                                    |

Helper murni baru `resolveEffectiveDisplayFilters(displayFilters, workflowMap, singleTypeId)` di `apps/web/core/store/workflow.helpers.ts`, mengembalikan `{ group_by, sub_group_by }` efektif:

| Project              | Scope (filter type)    | `group_by` tersimpan          | Efektif                                 |
| -------------------- | ---------------------- | ----------------------------- | --------------------------------------- |
| Punya typed workflow | mixed (0 atau >1 type) | `state`                       | `state_detail.group`                    |
| Punya typed workflow | mixed                  | `workflow_state`              | `workflow_state`                        |
| Punya typed workflow | 1 type                 | `state` atau `workflow_state` | `workflow_state` (kolom state type itu) |
| Punya typed workflow | 1 type                 | `state_detail.group`          | `state_detail.group`                    |
| Tanpa typed workflow | apa pun                | apa pun                       | tidak berubah                           |

Ketentuan tambahan:

- `hasTypedWorkflows = (workflowMap?.types?.length ?? 0) > 0`; `singleTypeId = getSingleWorkItemTypeId(richFilters)` (`workflow.helpers.ts:60-71`).
- Jika hasil resolusi `group_by` dan `sub_group_by` sama, `sub_group_by` di-null-kan (server menolak group == sub, `apps/api-rs/crates/api/src/routes/issue_query.rs:2895-2905`).
- Resolusi dipakai di empat tempat: parameter server (`issue-filter-helper.store.ts:92-126`), pembangunan kolom (`getGroupByColumns`), drag & drop (`useGroupIssuesDragNDrop`), dan dropdown grouping (nilai tercentang + daftar opsi).

### 2. Kolom board

`getStateColumns` (`apps/web/core/components/issues/issue-layouts/utils.tsx:245-265`):

- Effective `state_detail.group` → `getStateGroupColumns` (5 kolom, payload kosong) — perilaku upstream.
- Effective `workflow_state`:
  - Single type → perilaku sekarang: mirror state type itu via `resolveStateColumns` (`workflow.helpers.ts:127-141`), label polos, urut sequence.
  - Mixed → helper baru `resolveCompositeStateColumns(projectStates, workflowMap)`: legacy state lebih dulu (urut sequence), lalu per type mengikuti urutan `workflowMap.types`, masing-masing urut sequence; label `{type_name} · {state_name}`; fallback nama polos untuk row typed yang type-nya tidak ada di map (defensif).
- Payload tiap kolom: `{ state_id, type_id }` untuk state typed (bukan hanya saat single type), `{}` untuk kolom group. Ini memperbaiki quick-add di mixed.

Rendering tetap column-driven: urutan dan label sepenuhnya dari daftar kolom client; server hanya memasok bucket per key (`state_id` / `state__group`).

### 3. Drag & drop

- Root kanban/list meneruskan **nilai efektif** ke `useGroupIssuesDragNDrop` (`apps/web/core/hooks/use-group-dragndrop.ts:30-35`; pemanggil `apps/web/core/components/issues/issue-layouts/kanban/base-kanban-root.tsx:130`, list padanannya).
- Effective `state_detail.group`: drop lintas kolom dibatalkan dengan toast info i18n `common.state_change_requires_workflow_state_grouping` ("Switch to the Workflow states grouping to change state"); reorder dalam kolom yang sama tetap jalan. Blok perubahan group di `handleGroupDragDrop` (`utils.tsx:599-615`) tidak dieksekusi untuk drop lintas kolom, jadi tidak ada request `state__group` (field itu tidak writable; `apps/api-rs/crates/api/src/routes/issue_update.rs` tidak menerimanya).
- Effective `workflow_state`: `ISSUE_FILTER_DEFAULT_DATA["workflow_state"] = "state_id"` membuat drop mengirim `state_id` (berfungsi). Guard lintas type diimplementasikan di `use-workflow-drag-n-drop.ts` dengan memanfaatkan plumbing yang sudah ada (`workflowDisabledSource`, `isWorkflowDropDisabled`, `getIsWorkflowWorkItemCreationDisabled`, `handleWorkFlowState`; konsumen `kanban-group.tsx:128-176`, `list/list-group.tsx`): bandingkan `type_id` state row kolom sumber vs tujuan via state store; beda type → drop ditolak + toast `common.workflow_state_wrong_type` ("Target state belongs to a different work item type"). Backend tetap enforcer terakhir.
- Effective selain kedua nilai di atas: perilaku DnD tidak berubah.

### 4. Filter state (opsi, scoping, chip, pruning)

- **Label**: `getStateMultiSelectConfig` (`packages/utils/src/work-item-filters/configs/filters/state.ts:84-100`) menerima param opsional baru `getOptionLabel?: (state: IState) => string` (default `(state) => state.name`). `useWorkItemFiltersConfig` (`apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx:126-130,176-187`) mengisinya: state typed → `{type_name} · {state_name}` memakai map `workItemTypes` (`:109-118`); legacy → nama polos. Prefix hanya saat project typed dan scope tidak ter-filter ke satu type.
- **Scoping**: `ProjectLevelWorkItemFiltersHOC` (`.../filters-hoc/project-level.tsx:209`) menyaring `stateIds` berdasarkan type ids yang sedang aktif di richFilters: tanpa filter type → semua state; filter type aktif (1+) → hanya state milik type terpilih (legacy disembunyikan).
- **Pruning**: helper murni `pruneStateFilterValues(richFilters, allowedStateIds)`; dipakai saat filter type berubah (jalur update filter type di layer project filters) untuk membuang nilai `state_id__in`/`state_id__exact` yang tidak lagi valid, supaya tidak ada hasil kosong senyap. `state_group` tidak disentuh.
- **Chip**: memakai label opsi secara otomatis (`SelectedOptionsDisplay`), nilai tersimpan tetap state id sehingga saved view tetap kompatibel.
- Filter **State group** tetap tersedia di project page seperti sekarang.

### 5. Create / quick-add

- `getStateColumns` selalu menyertakan `type_id` pada payload kolom typed (Bagian 2), sehingga quick-add dari kolom komposit lolos `validate_create_refs`.
- Kolom 5-group payload kosong; create modal mulai tanpa type, user pilih type → state default (`apps/web/core/components/issues/issue-modal/components/default-properties.tsx:161-180`).
- Hardening create modal: bila `state_id` terisi typed dan `type_id` kosong (entry point lain/`data` lama), derive `type_id` dari state store sebelum submit.
- Backend tidak berubah.

### 6. Kompatibilitas

- **Saved views/user prefs tanpa migrasi**: nilai `state` lama tetap valid; tampilannya berubah mengikuti aturan resolusi (mixed → 5 group; single type → state type). Pengguna yang ingin kolom komposit memilih `workflow_state` secara eksplisit.
- **Visibilitas opsi dropdown**: project typed menampilkan `state_detail.group` + `workflow_state` dan menyembunyikan `state`; project tanpa typed workflow menampilkan `state` saja dan menyembunyikan kedua opsi baru. Daftar opsi dihitung runtime dari workflow map (bukan sekadar daftar statis di `ISSUE_DISPLAY_FILTERS_BY_PAGE`).
- **Project tanpa typed workflow**: kolom, filter, dan DnD tidak berubah.
- **Nilai tercentang** di dropdown grouping dihitung dari nilai efektif, bukan nilai tersimpan.

## Perubahan per paket

**`packages/types`**

- `TIssueGroupByOptions` (`src/view-props.ts:14-26`): tambah `"workflow_state"`.

**`packages/constants`**

- `ISSUE_GROUP_BY_OPTIONS` (`src/issue/common.ts:113-128`): tambah `{ key: "workflow_state", titleTranslationKey: "common.workflow_states" }`.
- `EIssueGroupByToServerOptions` (`src/issue/common.ts:27-40`): `"workflow_state" = "state_id"`.
- `ISSUE_DISPLAY_FILTERS_BY_PAGE.issues` (`src/issue/filter.ts:221-246`): tambah `state_detail.group` + `workflow_state` ke `group_by` list & kanban, dan ke `sub_group_by` kanban.

**`apps/web`**

- `core/store/workflow.helpers.ts`: `resolveEffectiveDisplayFilters`, `resolveCompositeStateColumns`, (opsional) `pruneStateFilterValues` + tes.
- `core/store/issue/helpers/base-issues.store.ts:115-141`: entri `workflow_state` di `ISSUE_GROUP_BY_KEY` & `ISSUE_FILTER_DEFAULT_DATA` (`"state_id"`).
- `core/store/issue/helpers/issue-filter-helper.store.ts:92-126`: serialisasi `group_by`/`sub_group_by` memakai nilai efektif.
- `core/components/issues/issue-layouts/utils.tsx:245-265`: `getStateColumns` memakai resolver + builder komposit + payload `type_id`.
- `core/components/issues/filters.tsx` + `.../display-filters/display-filters-selection.tsx`/`group-by.tsx`: daftar opsi dinamis (typed vs non-typed), nilai tercentang dari efektif.
- `core/hooks/use-group-dragndrop.ts` + root kanban/list: pakai nilai efektif; skip group-change untuk `state_detail.group` + toast info.
- `core/components/workflow/use-workflow-drag-n-drop.ts`: implementasi guard lintas type.
- `core/hooks/work-item-filters/use-work-item-filters-config.tsx` + `.../filters-hoc/project-level.tsx`: label, scoping, pruning.
- `core/components/issues/issue-modal/*`: derive type dari state saat perlu.
- `packages/utils/src/work-item-filters/configs/filters/state.ts`: param `getOptionLabel`.
- `packages/i18n/src/locales/**`: key `common.workflow_states` ("Workflow states"), `common.state_change_requires_workflow_state_grouping`, `common.workflow_state_wrong_type`; sinkron semua locale mengikuti skill translate.

## Testing

- **Vitest** (`apps/web/core/store/workflow.helpers.test.ts` dan sekitarnya):
  - matriks `resolveEffectiveDisplayFilters`: typed/non-typed, mixed/single, seluruh nilai tersimpan, collision group/sub.
  - `resolveCompositeStateColumns`: urutan legacy→type→sequence, label komposit, fallback row typed tanpa map, payload `type_id`.
  - `pruneStateFilterValues`: buang state di luar allowed, pertahankan `state_group` dan field lain.
  - helper label/scoping opsi filter.
  - mapping DnD: `workflow_state` → update `state_id`; `state_detail.group` → tanpa update group.
- Tidak ada tes backend baru.
- **E2E smoke manual** di workspace `terraline-demo` (project Terra) setelah `pnpm --filter=web build` + restart `plane-web-prod.service` sesuai `AGENTS.md`:
  1. Board default mixed → 5 kolom state group, tanpa "Baru" ganda.
  2. Filter satu type (Problem) → kolom state milik Problem.
  3. Pilih grouping "Workflow states" → kolom komposit `Problem · Baru`, urut legacy lalu per type.
  4. Quick-add dari kolom komposit membuat item dengan type + state yang benar (tanpa 400).
  5. Drag di "Workflow states" antar type diblok + toast; transisi valid tetap jalan.
  6. Drag di 5-group → reorder tanpa error + toast info.
  7. Filter state: label `Problem · Baru`, scope mengikuti filter type, chip berlabel, pruning saat filter type berubah.

## Edge cases

- **Legacy state "Baru"** tetap tampil polos di kolom komposit; tidak digabung dengan "Baru" typed.
- **Filter multi-type** → state scope = union type terpilih, semua berlabel.
- **Type dinonaktifkan/di-detach** → mirror state-nya soft-delete; row yatim yang masih tampil diberi label polos (defensif).
- **`sub_group_by` legacy** yang resolusi efektifnya sama dengan `group_by` → di-null-kan.
- **Filter type dihapus** → state values yang tersisa tidak direstorasi otomatis; user memilih ulang (pruning satu arah).
- **Typed workflow dimatikan di project** (feature/type di-unlink): nilai tersimpan `workflow_state` tetap valid dan menampilkan seluruh state project (semuanya legacy) tanpa label type; nilai `state` kembali berperilaku non-typed.
- **Kolom 5-group** tanpa payload state; quick-add membuka modal tanpa type/state.
- **`state_group` filter** tetap type-agnostic dan tidak dipangkas pruning.
- **Workspace/global views, sub-issues, inbox, archived, analytics, export** tidak tersentuh; perilakunya sama seperti sebelum spec.

## Risiko

- **Perubahan tampilan default mixed** dari kolom per state row menjadi 5 group: disengaja (keputusan #3); mitigasi: opsi `workflow_state` eksplisit + toast info pada drop lintas group.
- **Composite view bisa 20+ kolom** pada project dengan banyak type: opt-in, label komposit membuatnya terbaca.
- **Pruning filter** menghapus pilihan state saat filter type berubah: disengaja untuk mencegah hasil kosong; sebut di release note.
- **Exhaustive maps/union types** (`Record<TIssueGroupByOptions, ...>`) akan error compile saat `workflow_state` ditambahkan: dituntun compiler, perbaiki semua map dan fixture tes.
- **Label i18n baru** harus disinkron ke semua locale; ikuti skill translate dan tes formatter i18n repo.

## Peta modul

- **Types/constants/utils**: `packages/types/src/view-props.ts`, `packages/constants/src/issue/common.ts`, `packages/constants/src/issue/filter.ts`, `packages/utils/src/work-item-filters/configs/filters/state.ts`, `packages/i18n/src/locales/**`.
- **Web helpers/store**: `apps/web/core/store/workflow.helpers.ts`, `apps/web/core/store/issue/helpers/base-issues.store.ts`, `apps/web/core/store/issue/helpers/issue-filter-helper.store.ts`.
- **Web layout/filter**: `apps/web/core/components/issues/issue-layouts/utils.tsx`, `apps/web/core/components/issues/filters.tsx`, `apps/web/core/components/issues/issue-layouts/filters/header/display-filters/**`, `apps/web/core/hooks/work-item-filters/use-work-item-filters-config.tsx`, `apps/web/core/components/issues/issue-layouts/filters-hoc/project-level.tsx`.
- **Web DnD/create**: `apps/web/core/hooks/use-group-dragndrop.ts`, `apps/web/core/components/issues/issue-layouts/kanban/**`, `apps/web/core/components/issues/issue-layouts/list/**`, `apps/web/core/components/workflow/use-workflow-drag-n-drop.ts`, `apps/web/core/components/issues/issue-modal/**`.
- **Tes**: `apps/web/core/store/workflow.helpers.test.ts`, tes vitest baru untuk helper filter/DnD.
