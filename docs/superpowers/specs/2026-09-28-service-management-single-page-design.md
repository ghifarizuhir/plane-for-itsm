# Service Management Satu Halaman (Work Item Types + Workflows) — Design

Tanggal: 2026-09-28
Status: disetujui user saat brainstorming (Approach 2: UI + backend convenience), menunggu review spec tertulis.
Scope: web (`apps/web`, `packages/constants`, `packages/types`, `packages/i18n`), API (`apps/api-rs`), migrasi data Django (`apps/api`).
Terkait: `docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md`.

## Latar

Workspace settings saat ini punya kategori "Service management" dengan dua halaman: **Work item types** (`/settings/work-item-types`) dan **Workflows** (`/settings/workflows`, editor di `/:workflowId`). Padahal kontrak yang dipakai sehari-hari adalah 1 type = 1 workflow:

- Seed default memasangkan Incident, Problem, Change, Improvement masing-masing dengan workflow sendiri (`apps/api/plane/db/migrations/0124_seed_default_workflows.py:7-84`, `apps/api-rs/crates/api/src/seed.rs:296-373`).
- Model tidak memaksa 1:1: `IssueType.workflow` adalah FK nullable many-to-one (`apps/api/plane/db/models/issue_type.py:25-31`); design lama eksplisit menyebut "seed 1:1, model tidak memaksa" (`2026-09-25-work-item-types-workflows-design.md:61`).
- Data nyata workspace `itsm`: 4 seed 1:1, plus type `Request` tanpa workflow dan workflow orphan `Request Workflow`. Workspace scratch punya workflow yang dipakai 2 type (`Type T` + `Type U` → `Workflow B`) dan beberapa orphan.

Tujuan: satu halaman settings, dengan setiap work item type non-epic memiliki tepat satu workflow yang dikelola bersamanya — nama workflow mengikuti type, lifecycle menyatu.

## Keputusan yang dikunci saat brainstorming

1. **Scope**: UI + backend convenience. Model/schema tidak diubah (FK tetap nullable); kontrak 1:1 ditegakkan di api-rs.
2. **Struktur halaman**: list + detail sub-route — `/settings/work-item-types` (list) dan `/settings/work-item-types/:typeId` (type + workflow editor).
3. **Naming**: create type auto-create workflow `{Type} Workflow` + state default; nama workflow tidak bisa diedit dan ikut berubah saat type di-rename. UI menampilkan nama derived, bukan field nama workflow.
4. **Migrasi**: adopt + normalize — type tanpa workflow mengadopsi orphan bernama `{Type} Workflow` bila ada, jika tidak dibuat baru; workflow yang dipakai >1 type di-clone per type; sisa orphan di-soft-delete.
5. **Lifecycle**: delete type ikut soft-delete workflow + states + transitions; workflow tidak bisa dibuat standalone lagi.
6. **Active**: satu switch (type). `workflows.is_active` disinkronkan; enforcement yang ada sudah membaca `issue_types.is_active` (`workflow_map`, `apps/api-rs/crates/api/src/routes/workflow.rs:1265-1272`), tidak berubah.

## Non-goals

- Project settings "Work item types" (opt-in type per project) dan materialisasi state project tidak berubah.
- Tidak ada perubahan schema: kolom `issue_types.workflow_id` tetap nullable, tanpa constraint unique baru.
- Field `workflow` di response v1/MCP tetap ada (read-only).
- Tidak ada guard transisi baru, role restriction, SLA, atau custom field.
- Workflow versioning / template lintas workspace.

## Desain

### 1. UI & routes

**Sidebar** (`packages/constants/src/settings/workspace.ts:61-74`): entri `workflows` dihapus; kategori Service management menyisakan `work_item_types`.

**List** `/settings/work-item-types`:

- Baris: nama type, badge `Epic` (bila `is_epic`), badge Active/Inactive, dan label workflow derived `{Type} Workflow` (epic: tanpa workflow).
- Klik baris → detail. Tombol "Add type" membuka modal yang sama dengan sekarang **tanpa select workflow** (`apps/web/core/components/work-item-types/type-form-modal.tsx:156-190` dihapus).
- Setelah create sukses → navigasi ke detail type baru.
- List tidak lagi fetch daftar workflow (nama derived dari type) dan tidak fetch state count per workflow.

**Detail** `/settings/work-item-types/:typeId`:

- Breadcrumb `Work item types / {Type}`.
- Kartu type: nama, deskripsi, toggle Active (satu-satunya switch), tombol Edit (modal yang sama).
- Section Workflow: judul derived `{Type} Workflow` + `StateList` + `TransitionMatrix` (komponen existing, reused).
- Epic: section workflow diganti info bahwa epic tidak memakai workflow.
- Deep link: fetch types dulu (store), cari by id; tidak ketemu → empty state seperti workflow not found sekarang.

**Routing** (`apps/web/app/routes/core.ts:295-306`): route `settings/workflows` dan `settings/workflows/:workflowId` diganti redirect ke `settings/work-item-types`.

**Komponen**:

- Hapus `apps/web/core/components/workflows/root.tsx`, `workflow-list.tsx`, `workflow-form-modal.tsx`.
- `workflow-editor.tsx` di-refactor: tidak lagi fetch daftar workflow untuk cek existence; menerima `workflowId` (+ nama derived) dari type. `StateList`, `TransitionMatrix`, dan `workflow-load-error-state.tsx` tetap.
- `work-item-types/root.tsx` jadi list (tanpa fetch workflows, tanpa lookup nama workflow).
- Halaman detail baru di `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/[typeId]/`.

**Store & service** (`apps/web/core/store/workflow.store.ts`, `apps/web/core/services/workflow/workflow.service.ts`):

- `createWorkflow`, `updateWorkflow`, `deleteWorkflow`, dan `fetchWorkflows` dihapus bila tidak ada konsumen lain (project settings memakai `workflowMap`, bukan daftar workflow).
- `fetchWorkItemTypes`/create/update/delete tetap; payload type tidak lagi mengirim `workflow` (`packages/types/src/workflow/work-item-type.ts:25-31`).
- Setelah create/update/delete type, refresh workflow map project terkait seperti sekarang (`refreshWorkflowMaps`).

**i18n**: hapus string list/editor workflow yang mati, tambah string halaman detail, sinkronkan semua locale (ikuti skill translate).

### 2. API (api-rs)

File utama: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`, `apps/api-rs/crates/api/src/routes/workflow.rs`.

**Create type** (`POST /api/workspaces/:slug/work-item-types/`, internal & v1):

- Body `workflow` ditolak 400 ("Workflows are managed through work item types").
- Type non-epic: dalam transaksi yang sama, buat (atau adopsi orphan bernama sama) workflow `{name} Workflow` dengan `is_active` mengikuti nilai `is_active` type, pastikan ada tepat satu default state (`New`, group backlog) bila workflow belum punya state, link `workflow_id`, lalu materialize state untuk project yang ikut ter-link (jalur `materialize_type_states` yang ada).
- Type epic: tanpa workflow (guard existing dipertahankan).
- Response tidak berubah (tetap membawa `workflow` id).

**Update type** (`PATCH /api/workspaces/:slug/work-item-types/:type_id/`):

- Body `workflow` ditolak 400; blok clear/switch workflow dihapus (`apps/api-rs/crates/api/src/routes/v1/work_item_type.rs:501-587`).
- Rename type → rename workflow ke `{name} Workflow` (bila nama derived bentrok, workflow lama mempertahankan namanya; UI tetap menampilkan derived).
- `is_active` berubah → `workflows.is_active` ikut berubah.

**Delete type**: guard existing (live work item / project link) dipertahankan; setelah lolos, soft-delete type + workflow + `workflow_states` + `workflow_transitions`-nya dalam satu transaksi.

**Endpoint workflow standalone**: `POST /workflows/`, `PATCH /workflows/:id`, `DELETE /workflows/:id` dihapus dari router (`apps/api-rs/crates/api/src/main.rs:657-706`) — workflow tidak bisa dibuat/diubah/dihapus lewat API. `GET /workflows/` dan `GET /workflows/:id` tetap read-only. CRUD state & transisi tetap seperti sekarang (editor memakainya).

**Dead code**: guard `workflow_link_conflict` (`apps/api-rs/crates/api/src/routes/v1/work_item_type.rs:356-384`) dan cabang switch/clear workflow dihapus karena sharing tidak mungkin lagi.

### 3. Migrasi data Django (`0125_normalize_type_workflows`)

Satu data migration (forward-only, `reverse_code=noop`), berjalan dalam transaksi, untuk semua workspace hidup:

1. **Type non-epic tanpa workflow**: adopsi workflow hidup di workspace yang sama dengan nama `{Type} Workflow` dan 0 type hidup; bila tidak ada, buat workflow + state default.
2. **Workflow dengan >1 type hidup**: pertahankan untuk link type tertua; untuk tiap type tambahan clone workflow + semua state hidup + transisi (mapping state lama → baru), lalu re-point baris `states` mirror project milik type itu (`type_id = type`, `workflow_state_id` lama) ke state clone yang bersesuaian.
3. **Rename** workflow yang ter-link ke `{Type} Workflow` bila berbeda dan namanya bebas; **sync** `workflows.is_active = issue_types.is_active`.
4. **Healing default state**: workflow tanpa state hidup → buat `New` (backlog, `is_default`); ada state tapi tanpa default → jadikan state paling awal by `sequence` sebagai default.
5. **Sisa orphan** (workflow hidup dengan 0 type hidup): soft-delete workflow + states + transitions.
6. Row seed (`external_source = 'plane-default-itsm'`) tidak diubah/diadopsi ulang; marker contract `0124` dan `seed.rs` tetap berlaku.

### 4. Seed workspace baru

`apps/api-rs/crates/api/src/seed.rs` tidak berubah — sudah 1:1. Auto-create workflow di runtime memakai kontrak nama & default state yang sama dengan seed supaya konsisten.

## Edge cases

- **Nama type duplikat** (tidak ada unique constraint `(workspace, name)` di `issue_types`): derived name bisa bentrok. Create mengadopsi orphan bernama sama bila ada; bila nama dipakai workflow type lain, buat dengan suffix internal, mis. `{Type} Workflow (2)` (UI tetap menampilkan derived).
- **Rename type bentrok**: workflow lama mempertahankan nama; tampilan derived tetap `{Type} Workflow`.
- **Adopsi orphan saat create runtime**: bila `{Type} Workflow` sudah ada sebagai orphan (sisa data lama), dipakai ulang, bukan gagal duplicate-name.
- **Workflow tanpa state** (hasil import/manual): diheal ke default `New` saat adopsi/create/migrasi.
- **Type dihapus** dengan live work item/project link: tetap 400 (guard existing).
- **Epic**: tidak pernah punya workflow; tidak tersentuh auto-create/rename/delete cascade.
- **Concurrent edit**: last-write-wins seperti sekarang; tidak ada locking baru.
- **Project archived**: materialisasi idempotent saat project aktif kembali (tidak berubah).

## Testing

- **Rust** (`apps/api-rs/crates/api/tests/`): update `workflow_test.rs` & `v1_work_item_type_test.rs` (endpoint workflow standalone dihapus); test baru: create type auto-create workflow + default state, rename sync, delete cascade, body `workflow` ditolak, epic tanpa workflow, adopsi orphan.
- **Django** (`apps/api/tests`): test data migration — adopsi orphan, clone shared + re-point mirror, healing default, soft-delete orphan, idempotent terhadap row seed.
- **Web** (`vitest`): update `workflow.store.test.ts` + `workflow.helpers.test.ts`; test helper halaman detail (resolve type by id, derived workflow name).
- **E2E smoke**: migrate → rebuild api-rs (`docker compose -f docker-compose-local.yml up -d --build api worker beat-worker`) → `curl /health` → restart `plane-live` → curl create/rename/delete type + cek workflow/states → `pnpm --filter=web build` + restart `plane-web-prod.service`.

## Rollout

1. Django migrate (normalisasi data existing, termasuk workspace `itsm`).
2. Rebuild api-rs sesuai `AGENTS.md`.
3. Build + restart web prod.
4. Tidak ada perubahan pada project settings / perilaku work item; project opt-in tetap seperti sekarang.

## Peta modul & file

- **Web**: `packages/constants/src/settings/workspace.ts`, `apps/web/app/routes/core.ts`, `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/work-item-types/**`, `apps/web/core/components/work-item-types/**`, `apps/web/core/components/workflows/**`, `apps/web/core/store/workflow.store.ts`, `apps/web/core/services/workflow/workflow.service.ts`, `packages/types/src/workflow/work-item-type.ts`, `packages/i18n/src/locales/**`.
- **api-rs**: `crates/api/src/routes/v1/work_item_type.rs`, `crates/api/src/routes/workflow.rs`, `crates/api/src/main.rs`, `crates/api/tests/**`.
- **Django**: `apps/api/plane/db/migrations/0125_*` (data migration), `apps/api/tests/**`.

## Risiko

- **Clone + re-point mirror** adalah bagian migrasi paling berisiko; diuji dengan bentuk data nyata (shared workflow, project mirror).
- **Kontrak nama derived** tidak ditegakkan DB; bentrok diselesaikan app-level (suffix/adopsi). Bila perlu, unique partial index `issue_types(workflow_id) WHERE deleted_at IS NULL` bisa jadi follow-up terpisah.
- **Test existing** banyak yang memakai endpoint workflow standalone; harus diperbarui, bukan dihapus tanpa pengganti.
- **MCP/SDK**: create type via API v1 kini menolak field `workflow`; konsumen lama yang mengirim field itu mendapat 400 (disengaja, pesan jelas).
