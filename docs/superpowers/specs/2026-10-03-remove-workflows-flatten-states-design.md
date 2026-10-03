# Hapus Workflow & Flatten State (Work Item = Type + State Flat) — Design

Date: 2026-10-03
Status: Draft (pending user review)
Scope: Schema Django (`apps/api`), API (`apps/api-rs`), web (`apps/web`, `packages/types`, `packages/constants`, `packages/utils`, `packages/i18n`), scripts & tests.
Terkait: `docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md`, `docs/superpowers/specs/2026-09-29-workflow-state-grouping-filter-design.md`.

Spec ini membatalkan/menggantikan bagian workflow (Workflow, WorkflowState, WorkflowTransition, mirror per type, workflow-map, grouping `workflow_state`) dari dua spec di atas. Work item type tetap.

## Tujuan

Menyederhanakan model work item dari `type → workflow → state` menjadi:

```
WorkItem ──type_id──▶ WorkItemType (workspace, di-enable per project via project_issue_types)
WorkItem ──state_id─▶ State (flat, project-level, group: backlog/unstarted/started/completed/cancelled + triage)
Project.default_state_id ─▶ State
```

Tidak ada lagi entity Workflow, WorkflowState, WorkflowTransition, transisi wajib, maupun state mirror per type. Work item type tetap sebagai klasifikasi; state kembali menjadi daftar status sederhana milik project.

## Konteks saat ini

- Chain sekarang: `Issue.type_id` → `IssueType.workflow_id` (1:1 non-epic) → `Workflow` → `WorkflowState` + `WorkflowTransition`; `State` project-level adalah mirror dari `WorkflowState` per `(project, type)` (`states.type_id`, `states.workflow_state_id`).
- Tabel `workflows`, `workflow_states`, `workflow_transitions` **hanya** dibuat oleh Django migrations `0123`–`0125`; baseline api-rs `migrations/0001_initial.sql` (squash s/d `0122`) tidak memuat DDL-nya. Django = migrator; api-rs menjalankan baseline + delta `0002_*` saat boot (`crates/common/src/db.rs:22`).
- Data live: 197 workflows, 595 workflow states, 618 transisi, 568 states (129 typed mirror, 25 dipakai issue), 584 issues (571 punya state, 495 state legacy tanpa type), 6 issue di triage.
- `state.group` dipakai lintas fitur yang tetap dipertahankan: `completed_at`, gating arsip (`completed`/`cancelled`), progress cycle/module, analytics, sub-issue distribution, highlight overdue, auto-close automation.
- Triage/intake memakai row `State` dengan `group='triage'` (`TriageStateManager`, `intake-state`, accept flow) — tetap dipertahankan.

## Keputusan (brainstormed & approved)

1. **Type tetap, workflow + transisi + mirror per type dihapus.**
2. **State menjadi daftar flat per project** (model Plane original), tanpa keterikatan ke type.
3. **Data state di-reset ke default**: seluruh status existing dihapus, tiap project dibuatkan 6 state default, semua issue/draft di-remap ke Backlog.
4. **Clean break API**: endpoint workflow/transitions/workflow-map dan field `workflow` dihapus tanpa shim; PQL/filter/group-by `state` tetap karena state flat masih ada.
5. **Satu rilis (Opsi A)**: migrasi forward + hapus kode api-rs + bersihkan web/packages dalam satu train rilis; web & backend naik bersamaan.
6. **Intake/triage tetap** sebagai state dengan `group='triage'`.
7. **Halaman project states settings tetap** sebagai pengelola state flat; workflow editor dihapus.
8. **Fitur berbasis `state.group` tidak berubah** (completed, arsip, progress, analytics, overdue, auto-close).

## Non-goals

- Custom field/ITSM per type, guard transisi, approval/CAB, SLA — sudah non-goal sejak awal dan tetap begitu.
- Menghapus konsep state dari work item (state flat tetap ada).
- Menghapus state di `apps/space` (public sites) atau fitur Services.
- Migrasi/preservasi status existing (secara sadar di-reset).
- Perubahan ke intake/triage selain penyesuaian default state saat accept.

## Model data target

### Tabel dihapus

| Tabel                  | Sumber DDL          |
| ---------------------- | ------------------- |
| `workflows`            | Django `0123:16-35` |
| `workflow_states`      | Django `0123:36-59` |
| `workflow_transitions` | Django `0123:60-74` |

### Kolom dihapus

| Kolom                      | Sumber                | Catatan                          |
| -------------------------- | --------------------- | -------------------------------- |
| `states.type_id`           | Django `0123:83-87`   | FK → `issue_types`, SET_NULL     |
| `states.workflow_state_id` | Django `0123:123-127` | FK → `workflow_states`, SET_NULL |
| `issue_types.workflow_id`  | Django `0123:103-107` | FK → `workflows`, SET_NULL       |

Constraint state yang menyentuh kolom di atas ikut dihapus: `state_unique_name_project_type_when_deleted_at_null`, `state_unique_project_workflow_state_when_deleted_at_null`, `state_unique_default_project_type_when_deleted_at_null`, dan `state_unique_legacy_name_project_when_deleted_at_null` (menyentuh `type`).

### Yang dipertahankan

| Entitas                                                                                     | Catatan                                                                                             |
| ------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `issue_types`                                                                               | tanpa `workflow_id`; field lain tetap (`is_epic`, `is_default`, `is_active`, `level`, `external_*`) |
| `project_issue_types`                                                                       | enable/disable type per project, tidak berubah                                                      |
| `issues.type_id`, `issues.state_id`                                                         | tetap                                                                                               |
| `draft_issues.type_id`, `draft_issues.state_id`                                             | tetap                                                                                               |
| `projects.default_state_id`, `projects.is_issue_type_enabled`                               | tetap                                                                                               |
| `states` (name, slug, color, sequence, group, default, is*triage, description, external*\*) | flat per project                                                                                    |
| `issue_versions.state`, `issue_versions.type`                                               | snapshot UUID, tanpa FK                                                                             |

### Constraint baru `states`

- `state_unique_name_project_when_deleted_at_null`: unique `(project, name)` where `deleted_at IS NULL`.
- `state_unique_default_project_when_deleted_at_null`: unique `(project)` where `default = true AND deleted_at IS NULL` — tepat satu default per project.
- Tidak ada constraint slug (sama seperti sekarang; UI memakai `id`).

## Migrasi data

### Django `0126_remove_workflows_and_flatten_states` (kanonik)

`RunPython` reset per project, urutan wajib:

1. `UPDATE issues SET state_id = NULL`, `UPDATE draft_issues SET state_id = NULL`, `UPDATE projects SET default_state_id = NULL` untuk project tersebut (mencegah CASCADE menghapus issue saat state dihapus).
2. Hard-delete **semua** row `states` project tersebut (termasuk yang `deleted_at` terisi).
3. Insert 6 state default (`DEFAULT_STATES`, `apps/api/plane/db/models/state.py:24-62`): Backlog (`default=True`), Todo, In Progress, Done, Cancelled, Triage (`group='triage'`).
4. Set `projects.default_state_id` = Backlog.
5. `UPDATE issues SET state_id = <Backlog>` dan `UPDATE draft_issues SET state_id = <Backlog>` untuk project tersebut.
6. `UPDATE issue_versions SET state = NULL` (snapshot historis; `type` tetap). Baris `issue_activities` dengan `field='state'` dibiarkan (teks audit).

Schema ops setelah RunPython:

1. Drop 4 constraint state yang menyentuh `type`/`workflow_state`.
2. Drop kolom `states.type_id`, `states.workflow_state_id`, `issue_types.workflow_id`.
3. Drop tabel berurutan: `workflow_transitions` → `workflow_states` → `workflows`.
4. Add constraint baru `state_unique_name_project_when_deleted_at_null` + `state_unique_default_project_when_deleted_at_null`.

Perubahan model Django:

- `State` (`apps/api/plane/db/models/state.py`): buang FK `type` dan `workflow_state`; ganti blok `constraints` dengan dua constraint baru.
- `IssueType` (`apps/api/plane/db/models/issue_type.py`): buang FK `workflow`.
- Hapus model `Workflow`, `WorkflowState`, `WorkflowTransition` (`apps/api/plane/db/models/workflow.py`) + export di `models/__init__.py`.
- Serializer/view state Django yang menyentuh `type`/`workflow_state` disederhanakan (HTTP legacy Django tidak dipakai di deployment, tapi harus tetap konsisten).
- Migrasi `0123`–`0125` tidak diubah (riwayat); fresh install menjalankannya lalu `0126` menghapus.

### api-rs `migrations/0002_remove_workflows.sql`

DDL drop yang sama dengan guard `IF EXISTS` (kolom + tabel), aman dijalankan pada DB fresh (baseline `0001` tidak punya tabel workflow) maupun setelah Django `0126` (menjadi no-op). Tidak ada data reset di sini — DB tanpa Django memang tidak punya data workflow.

## Backend api-rs

### Route dihapus (`crates/api/src/main.rs:644-707`)

- `/workflows/` list/retrieve/create/patch/delete.
- `/workflows/:id/states/` CRUD.
- `/workflows/:id/transitions/` CRUD.
- `/projects/:id/workflow-map/`.
- Semua route khusus workflow lain.

### Route dipertahankan (disimplifikasi)

- Project states CRUD (`routes/state.rs`): list/create/retrieve/update/delete, `mark-default`, `intake-state`; buang guard typed-mirror/`workflow_state`; validasi tinggal kepemilikan state ↔ project, unique name, satu default per project.
- `ws_states` (`routes/workspace.rs:598`) — tetap.
- Work item types CRUD (`routes/v1/work_item_type.rs`): buang `workflow` dari `TYPE_COLS` dan JSON; buang mirror materialization, `sync_type_workflow`, cascade workflow saat delete, `detach_type_from_project` yang membersihkan workflow. `import`/`unlink` type per project tetap, logika dipangkas.
- PQL `state` + `type` (`routes/v1/pql.rs`), filter/group-by `state_id`/`state__group`, analytics, progress cycle/module, `completed_at`, gating arsip, notifikasi `state_change`, activity `field:"state"` — tidak berubah.

### Kode dihapus/disatukan

- `routes/workflow.rs` (1.509 baris) hampir seluruhnya; `validate_name` yang dipakai `release.rs`/`review.rs` dipindah ke helper bersama.
- Engine transisi (`allowed_target_state_ids`, `evaluate_transition`, `validate_state_transition`, `validate_initial_transition`, `fetch_transition_context`, `epic_and_target_ownership`) + semua pemanggil di `issue_update.rs:798-825`, `v1/work_item.rs:987-1017`, `draft.rs`.
- `resolve_issue_state` (`issue_common.rs:858-925`) disederhanakan: default state = `project.default_state` (tanpa type/workflow). Pemanggil di `issue_write.rs`, `issue_update.rs`, `draft.rs`, `intake.rs`, `v1/work_item.rs` ikut disederhanakan.
- Intake accept (`intake.rs:1588-1660`) → project default state; lookup/create state `group='triage'` tetap.
- Ownership check state: dari "state milik workflow type" menjadi "state milik project".
- `seed.rs`: `WORKFLOW_SEEDS` + inserter workflow dihapus; seed type (Incident/Problem/Change/Improvement) tetap. `project.rs DEFAULT_STATES_SEED` tetap.
- `draft.rs`: buang validasi workflow/transisi; default = project default state.

### Kontrak API yang berubah

- `V1WorkItemTypeRow` kehilangan `workflow`; `workflow-map` dan endpoint workflow hilang.
- Response issue tetap membawa `state_id`/`type_id`; tidak ada perubahan bentuk lain untuk konsumen MCP/SDK selain hilangnya endpoint workflow.

## Web & packages

### Store & service

- `workflow.store.ts` → `work-item-type.store.ts`: hanya `workItemTypes` + CRUD/import/unlink; buang `workflows`, `workflowStates`, `workflowTransitions`, `workflowMap`.
- `workflow.service.ts` → `work-item-type.service.ts` (type CRUD + import/unlink).
- `workflow.helpers.ts`: hapus hampir semua; sisakan `getWorkItemTypeIds` versi type-only (dipakai filter/kanban).
- `state.store.ts`: tetap CRUD state flat + intake; buang tombstone/epoch typed-mirror, `isTypedState`, filter mirror di `getStatePercentageInGroup`.
- `root.store.ts` + `use-workflow.ts` → daftarkan/ganti ke store type.
- `project-wrapper.tsx`: buang fetch `PROJECT_WORKFLOW_MAP`; `PROJECT_STATES` + `PROJECT_INTAKE_STATE` tetap. Fetch `WORKSPACE_STATES` dihapus bila audit menunjukkan tidak ada konsumen tersisa.

### UI

- Hapus `core/components/workflows/` (workflow-editor, state-list, state-form-modal, transition-matrix, dll) dan `core/components/workflow/use-workflow-drag-n-drop.ts`.
- `work-item-types/detail.tsx`: buang seksi WorkflowEditor; detail type tetap. `project-work-item-types/`: buang label workflow & logika "attach workflow first".
- `project-states/`: tetap sebagai pengelola state flat; `root.tsx` disederhanakan (tidak ada split typed vs legacy).
- `dropdowns/state`: tampilkan semua state project (tanpa filter type/transisi); `dropdowns/work-item-type`: sumber dari store type, bukan workflow map.
- `getStateColumns` & util layout: kolom = state project terurut per group; buang resolusi workflow-map dan opsi grouping `workflow_state`; grouping `state` dan `state_detail.group` tetap.
- Filter (`filters.tsx`, `issue-filter-helper.store.ts`, semua `filter.store.ts`): buang `resolveEffectiveDisplayFilters`; state tidak lagi di-scope per type.
- `issue-modal/default-properties.tsx`: buang fetch/retry workflow-map + linkage type↔state; default state = project default. Quick-add tetap prefill default state/type.
- Power-K state menu, intake state, highlight overdue, auto-close, progress/analytics: tetap.
- Route: halaman workspace `/settings/work-item-types/[typeId]` tetap tanpa editor workflow; halaman project `/settings/.../states` tetap.

### Packages

- `types`: hapus `workflow/workflow.ts` (TWorkflow\*), field `workflow` dari `TWorkItemType`, field `type_id`/`workflow_state_id` dari `IState`; `work-item-type.ts` dipindah keluar folder workflow.
- `constants`: buang `workflow_state` dari group-by, fetch key workflow (`WORKSPACE_WORKFLOWS`, `WORKSPACE_WORKFLOW_STATES`, `PROJECT_WORKFLOWS`, `PROJECT_WORKFLOW_MAP`), `DISPLAY_WORKFLOW_PRO_CTA`; `STATE_GROUPS` dan semua state key tetap.
- `utils`: buang helper workflow-map; `work-item/state.ts` (sort/order group) tetap.
- `i18n`: hapus namespace `workflow` + `workflow.json` di 20 locale, bersihkan key mati (`common.workflow_states`, `project-settings.workflows`, `workspace-settings.workflows`, dll), regenerate `keys.generated.ts` — pakai skill `translate` untuk locale.
- `propel`, `packages/services`, `apps/space`, `apps/admin`: tidak berubah.

## Testing

- **Django**: test migrasi `0126` (pola `tests/unit/migrations/test_normalize_type_workflows.py`) — assert workflow tables hilang, tiap project punya 6 state default, `default_state_id` = Backlog, issue/draft ter-remap, `issue_versions.state` NULL. Hapus `tests/unit/models/test_workflow_models.py`; update contract tests (`test_projects.py`, `test_issues.py`, dll). Jalankan via `docker compose -f docker-compose-test.yml`.
- **api-rs**: hapus `workflow_test.rs` (2.605 baris) & `workflow_transition_test.rs` (3.280 baris); rewrite bagian state di `module_state_test.rs`, `intake_triage_test.rs`, `issue_create_test.rs`, `issue_patch_test.rs`, `issue_test.rs`, `workspace_seed_test.rs`, `parity_gate_test.rs`, `route_inventory_test.rs`, `v1_work_item_type_test.rs`, dll. Suite scratch-workspace jalan serial (`-- --test-threads=1`).
- **Web**: rewrite/hapus `workflow.store.test.ts`/`workflow.helpers.test.ts`; sederhanakan `state.store.test.ts`; `pnpm check` (format, lint, types) + `pnpm --filter=web build`.
- **Scripts**: update `scripts/smoke.sh`, `scripts/shadow.sh`, `scripts/v1-smoke.py` (hapus endpoint states/workflow yang hilang).

## Rollout & rollback

1. **Backup DB dulu** (`pg_dump` container `plane-db`) — migrasi `0126` destruktif; ini satu-satunya jalan rollback.
2. Rebuild backend detached: `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 < /dev/null &` (link LTO 10+ menit tanpa output; jangan di-abort).
3. Migrator Django jalan otomatis sebelum api; verifikasi `curl localhost:8000/health` → 200.
4. `systemctl --user restart plane-live.service` + cek `localhost:3100/live/health/`.
5. Rebuild web (`VITE_API_BASE_URL=https://api.terraline.space`) → `systemctl --user restart plane-web-prod.service`.
6. Smoke manual: login, list issue, dropdown state berisi 6 default, kanban per state, buat issue baru → Backlog, intake accept → Backlog, CRUD type tanpa workflow, PQL `state`/`type` jalan.

Rollback = restore dump + deploy ulang build sebelumnya. Clean break berarti web & backend harus versi sama; tidak ada kompat silang.

## Risiko & area rawan

- **Data loss status existing** — disengaja (keputusan #3). Backup wajib.
- **Lockstep deploy** — web lama tidak kompatibel dengan backend baru (workflow-map hilang); deploy dalam satu window.
- **`issue_versions.state`** — di-NULL-kan; UI versi historis kehilangan snapshot state.
- **`issue_activities.field='state'`** — dibiarkan berisi nama state lama; tidak ada FK, aman sebagai teks audit.
- **Intake/triage** — tetap, tapi accept flow harus diubah ke project default state; test `intake_triage_test.rs`/`intake_triage_routes_test.rs` diperbarui.
- **Konsumen eksternal** — MCP/SDK yang memakai endpoint workflow atau field `workflow` harus ikut update; PQL `state`/`type` tetap.
- **`getSingleWorkItemTypeId`** — membaca filter legacy + rich; harus di-rewrite type-only tanpa merusak filter type.
- **Heuristik nama type** (`"change"` untuk Testing Control/TCB) — tidak terkait state/workflow, tetap; jangan ikut terhapus.
- **i18n 20 locale** — penghapusan key harus membedakan key hidup vs legacy; ikuti skill `translate`.
- **Constraint saat drop kolom** — constraint state yang menyentuh `type` harus di-drop eksplisit sebelum kolom; jangan andalkan cascade DB saja agar state migrasi Django konsisten.

## File inventory utama

| Area           | File kunci                                                                                                                                                                                     |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Django migrasi | `apps/api/plane/db/migrations/0126_remove_workflows_and_flatten_states.py` (baru)                                                                                                              |
| Django model   | `apps/api/plane/db/models/{state,issue_type,workflow,__init__}.py`                                                                                                                             |
| api-rs migrasi | `apps/api-rs/migrations/0002_remove_workflows.sql` (baru)                                                                                                                                      |
| api-rs route   | `crates/api/src/main.rs`, `routes/{workflow,state,issue_common,issue_write,issue_update,draft,intake,seed}.rs`, `routes/v1/{work_item,work_item_type,pql}.rs`                                  |
| Web store      | `apps/web/core/store/{workflow.store,workflow.helpers,state.store,root.store}.ts`                                                                                                              |
| Web service    | `apps/web/core/services/{workflow/workflow.service,project/project-state.service}.ts`                                                                                                          |
| Web UI         | `apps/web/core/components/{workflows,workflow,work-item-types,project-work-item-types,project-states,dropdowns/state,dropdowns/work-item-type}/`                                               |
| Web layout     | `apps/web/core/components/issues/issue-layouts/utils.tsx`, `issue-modal/components/default-properties.tsx`, `filters.tsx`                                                                      |
| Packages       | `packages/types/src/workflow/`, `packages/constants/src/{state,issue/common,issue/filter,fetch-keys}.ts`, `packages/utils/src/work-item-filters/`, `packages/i18n/src/locales/*/workflow.json` |
| Tests          | `apps/api-rs/crates/api/tests/{workflow_test,workflow_transition_test,...}.rs`, `apps/api/plane/tests/unit/models/test_workflow_models.py`, `apps/web/core/store/*.test.ts`                    |
