# Work Item Types & Workflows (ITSM) — Design

Date: 2026-09-25
Status: Draft (pending user review)
Scope: Schema Django (`apps/api`), API (`apps/api-rs`), web (`apps/web`, `packages/types`, `packages/constants`, `packages/i18n`).
Terkait: `docs/superpowers/specs/2026-09-13-services-feature-design.md`, `docs/superpowers/specs/2026-09-18-mcp-public-api-v1-core-design.md`.

## Tujuan

Membuat work item "fit to ITSM": admin workspace bisa mendefinisikan **work item type** (Incident, Problem, Change, Improvement, ...), masing-masing dengan **workflow** sendiri — daftar state dan aturan transisi yang wajib dipatuhi. Project meng-opt-in type yang dipakai, sehingga lifecycle incident di semua project konsisten.

## Konteks saat ini

- Work item = `Issue`, project-scoped (`ProjectBaseModel`, `apps/api/plane/db/models/project.py:180`). Sudah punya FK `type` → `IssueType` (`apps/api/plane/db/models/issue.py:164`).
- `IssueType` workspace-owned (`BaseModel`), punya `is_epic`, `is_default`, `is_active`, `level`, dan di-link ke project lewat `project_issue_types` (`apps/api/plane/db/models/issue_type.py`).
- `State` project-scoped (`apps/api/plane/db/models/state.py:79`), dipakai bersama semua work item, punya `group` (`StateGroup`: backlog/unstarted/started/completed/cancelled), `is_triage`, `default`, `sequence`, `slug`. Belum ada model workflow/transisi.
- Default state di-resolve saat create (`Issue._ensure_default_state`, `apps/api/plane/db/models/issue.py:228`) dan di api-rs (`apps/api-rs/crates/api/src/routes/issue_write.rs`).
- `projects.is_issue_type_enabled` sudah ada dan sudah dipetakan sebagai feature `work_item_types` di `GET/PATCH projects/{id}/features` (`apps/api-rs/crates/api/src/routes/v1/project.rs:229`). Belum ada UI-nya di web.
- Backend live = api-rs; Django hanya migrator (lihat `docs/superpowers/specs/2026-09-20-rust-auth-signup-design.md`).
- Sudah ada fitur Services (katalog service, project-level) yang work item-nya bisa di-link — tidak diubah spec ini.

## Keputusan (brainstormed & approved)

1. **State per type + transisi wajib.** Tiap type punya daftar state sendiri; perpindahan state divalidasi terhadap pasangan transisi `from → to` yang didefinisikan.
2. **Definisi workspace-level.** Workflow + state + transisi didefinisikan sekali di workspace settings; project meng-opt-in type lewat `project_issue_types` (mekanisme yang sudah ada).
3. **v1 tanpa custom field.** Impact/urgency/risk/workaround dan custom property per type dibuat sebagai spec terpisah.
4. **Board hybrid.** View campuran beberapa type memakai kolom `group` (5 kolom seperti sekarang); saat difilter ke satu type, kolom memakai state asli type itu.
5. **Pendekatan model A — materialized per project.** Workflow/state/transisi workspace-level; state di-materialize menjadi row `states` project saat type diaktifkan.
6. **Transisi v1: allowed pairs saja.** Semua member project boleh melakukan transisi yang valid; tanpa role restriction, guard field, approval, atau SLA.
7. **Rollout opt-in per project.** Tidak ada auto-enable; project yang tidak opt-in berperilaku persis seperti sekarang.
8. **Implementasi di api-rs**, schema via migrasi Django (Django = migrator).

## Non-goals (v1)

- Custom fields / field ITSM (impact, urgency, risk, workaround, root cause, resolution code).
- Guard transisi: role restriction, approval/CAB, wajib isi field, aksi otomatis.
- SLA/timer, eskalasi, automation per transisi.
- Workflow versioning dan template lintas workspace.
- Migrasi state legacy project ke dalam workflow.
- Board/halaman terpisah per type.
- Perubahan ke fitur Services.

## Model data

### Model baru (workspace-scoped, `BaseModel`, tabel baru)

File baru: `apps/api/plane/db/models/workflow.py`, didaftarkan di `apps/api/plane/db/models/__init__.py`.

#### `Workflow` (`workflows`)

| Field                            | Tipe                                                     | Catatan          |
| -------------------------------- | -------------------------------------------------------- | ---------------- |
| `workspace`                      | FK `Workspace`                                           | required         |
| `name`                           | CharField(255)                                           |                  |
| `description`                    | TextField                                                | blank            |
| `is_active`                      | BooleanField                                             | default `True`   |
| `external_source`, `external_id` | CharField                                                | nullable         |
| audit                            | `created_by/updated_by/created_at/updated_at/deleted_at` | dari `BaseModel` |

- Unique `(workspace, name)` where `deleted_at IS NULL`.
- Satu workflow boleh dipakai beberapa type (seed 1:1, model tidak memaksa).

#### `WorkflowState` (`workflow_states`)

| Field                            | Tipe                           | Catatan                   |
| -------------------------------- | ------------------------------ | ------------------------- |
| `workflow`                       | FK `Workflow`                  | related_name `states`     |
| `name`                           | CharField(255)                 |                           |
| `description`                    | TextField                      | blank                     |
| `color`                          | CharField(255)                 |                           |
| `group`                          | CharField choices `StateGroup` | default `backlog`         |
| `sequence`                       | FloatField                     | default 65535             |
| `is_default`                     | BooleanField                   | default `False`           |
| `slug`                           | SlugField(100)                 | `slugify(name)` saat save |
| `external_source`, `external_id` | CharField                      | nullable                  |

- Unique `(workflow, name)` where `deleted_at IS NULL`.
- Unique `(workflow, slug)` where `deleted_at IS NULL`.
- Partial unique `(workflow)` where `is_default = true AND deleted_at IS NULL` — tepat satu default per workflow.
- Tidak ada `is_triage`; triage tetap konsep project/intake.

#### `WorkflowTransition` (`workflow_transitions`)

| Field        | Tipe               | Catatan                             |
| ------------ | ------------------ | ----------------------------------- |
| `workflow`   | FK `Workflow`      | related_name `transitions`          |
| `from_state` | FK `WorkflowState` | related_name `outgoing_transitions` |
| `to_state`   | FK `WorkflowState` | related_name `incoming_transitions` |

- Unique `(workflow, from_state, to_state)` where `deleted_at IS NULL`.
- `CheckConstraint` `from_state != to_state`.
- Validasi app-level: `from_state.workflow == to_state.workflow == workflow`.

### Perubahan model existing

#### `IssueType` (`apps/api/plane/db/models/issue_type.py`)

- Tambah `workflow` → FK `Workflow`, `null=True, blank=True`, `on_delete=SET_NULL`, related_name `issue_types`.
- Validasi app-level: type dengan `is_epic=True` tidak boleh punya workflow. Type tanpa workflow = legacy.

#### `State` (`apps/api/plane/db/models/state.py`)

- Tambah `type` → FK `IssueType`, `null=True, blank=True`, `on_delete=SET_NULL`, related_name `states`. `null` = state legacy (dipakai work item tanpa type dan epic).
- Tambah `workflow_state` → FK `WorkflowState`, `null=True, blank=True`, `on_delete=SET_NULL`, related_name `state_mirrors`. Wajib terisi bila `type` terisi; hanya diisi sistem (bukan input user).
- Ganti constraint lama:
  - Hapus `unique_together = ["name", "project", "deleted_at"]` dan constraint `state_unique_name_project_when_deleted_at_null` (keduanya memblokir nama state yang sama antar type dalam satu project).
  - Tambah partial unique: `(project, name)` where `type IS NULL AND deleted_at IS NULL`.
  - Tambah partial unique: `(project, type, name)` where `type IS NOT NULL AND deleted_at IS NULL`.
  - Tambah partial unique: `(project, workflow_state)` where `workflow_state IS NOT NULL AND deleted_at IS NULL`.
  - Tambah partial unique: `(project, type)` where `default = true AND type IS NOT NULL AND deleted_at IS NULL` — tepat satu default per type per project.
- `State.slug` tetap `slugify(name)`; duplikat slug antar type dalam satu project diperbolehkan (tidak ada constraint slug, UI memakai `id`).
- `StateManager`/`triage_objects` dan `is_triage` tidak berubah.

#### `Issue` (`apps/api/plane/db/models/issue.py`)

- `_ensure_default_state` menjadi type-aware: pakai `type` issue → cari `State` dengan `type == issue.type`, `default=True`, project sama, bukan triage; fallback state pertama by `sequence` milik type itu. `type IS NULL` → perilaku lama.
- Tidak ada kolom baru di `issues`; `Issue.state` tetap FK ke `State` project.

### Resolusi default state (aturan)

1. Issue punya `type` dengan workflow → default state = mirror `State` dari `WorkflowState.is_default` milik type itu di project tersebut.
2. `type IS NULL` atau `is_epic` → default state legacy project (perilaku sekarang, exclude triage).
3. Jika `is_default` workflow diubah admin, semua mirror ikut berubah (sinkron).
4. Workflow wajib selalu punya tepat satu `is_default` (divalidasi API; hapus/ubah default terakhir ditolak).

### Seed default

Data migration membuat untuk semua workspace existing; jalur pembuatan workspace baru (`apps/api-rs/crates/api/src/seed.rs::seed_workspace`, dipanggil `routes/workspace.rs:332`) ikut membuat seed yang sama. `is_issue_type_enabled` project tetap `false` — seed tidak mengaktifkan apa pun di project.

| Type        | Workflow             | States (group)                                                                                                                   | Transisi                                                                                                                               |
| ----------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Incident    | Incident Workflow    | New (backlog, default), In Progress (started), On Hold (started), Resolved (completed), Closed (completed)                       | New→In Progress; In Progress→On Hold, Resolved; On Hold→In Progress; Resolved→Closed, In Progress                                      |
| Problem     | Problem Workflow     | New (backlog, default), Investigating (started), Known Error (started), Resolved (completed), Closed (completed)                 | New→Investigating; Investigating→Known Error, Resolved; Known Error→Resolved, Investigating; Resolved→Closed, Investigating            |
| Change      | Change Workflow      | New (backlog, default), Assessment (started), Approval (started), Implementation (started), Review (started), Closed (completed) | New→Assessment; Assessment→Approval, Closed; Approval→Implementation, Assessment; Implementation→Review; Review→Closed, Implementation |
| Improvement | Improvement Workflow | New (backlog, default), In Progress (started), Done (completed)                                                                  | New→In Progress; In Progress→Done, New; Done→In Progress                                                                               |

- Warna default mengikuti palet state yang ada; admin bebas mengubah.
- "Closed" untuk Change langsung `completed` (tidak ada state Resolved terpisah).

### Materialization (`State` mirror)

- Saat `project_issue_types` link dibuat (import type ke project) → untuk tiap `WorkflowState` hidup dari workflow type tersebut, upsert satu row `State`:
  - match by `(project, workflow_state)`,
  - copy `name`, `color`, `group`, `sequence`, `is_default`,
  - set `type` dan `workflow_state`,
  - `slug = slugify(name)`.
- Materialization idempotent dan dipanggil ulang saat:
  - workflow state ditambah/diubah/dihapus,
  - `GET workflow-map`,
  - `GET states` project,
  - project di-unarchive.
- Sync fan-out v1 dilakukan sinkron dalam request (single upsert); kalau jumlah project besar, offload ke worker menyusul tanpa perubahan kontrak.

## Sinkronisasi & lifecycle

- **Tambah/ubah state workflow** → sync ke semua project hidup yang mengaktifkan type (kecuali project archived; akan di-materialize ulang saat dibutuhkan).
- **Hapus state workflow** → 400 bila masih dipakai live work item di project mana pun; bila tidak, soft-delete mirror + soft-delete `WorkflowTransition` yang menyentuh state itu.
- **Hapus workflow** → 400 selama masih ada live `IssueType` yang memakainya.
- **`Workflow.is_active = false`** → type tidak bisa dipilih untuk work item baru; work item lama tetap bisa transisi sesuai definisi (tidak ada state terkunci).
- **Un-enable type dari project** (soft-delete `project_issue_types`) → 400 bila project masih punya live work item dengan type itu; admin harus reassign/archive dulu.
- **Hapus type** → 400 bila masih ada live work item yang memakainya atau live project link.
- **Item legacy** (`type IS NULL`) dan **epic** tidak pernah tersentuh sync/enforcement.

## API (api-rs)

### Route baru (internal, cookie-auth untuk web; admin workspace role ≥ 20)

File baru `apps/api-rs/crates/api/src/routes/workflow.rs`, didaftarkan di `main.rs`.

| Route                                                                        | Fungsi                               |
| ---------------------------------------------------------------------------- | ------------------------------------ |
| `GET/POST /api/workspaces/:slug/workflows/`                                  | list/create workflow                 |
| `GET/PATCH/DELETE /api/workspaces/:slug/workflows/:id/`                      | CRUD workflow (delete diguard)       |
| `GET/POST /api/workspaces/:slug/workflows/:id/states/`                       | list/create state                    |
| `GET/PATCH/DELETE /api/workspaces/:slug/workflows/:id/states/:state_id/`     | CRUD state + sync mirror             |
| `GET/POST /api/workspaces/:slug/workflows/:id/transitions/`                  | list/create transisi                 |
| `GET/DELETE /api/workspaces/:slug/workflows/:id/transitions/:transition_id/` | retrieve/delete transisi             |
| `GET/POST /api/workspaces/:slug/work-item-types/`                            | list/create type + assign `workflow` |
| `GET/PATCH/DELETE /api/workspaces/:slug/work-item-types/:type_id/`           | CRUD type                            |
| `POST /api/workspaces/:slug/projects/:project_id/import-work-item-types/`    | opt-in type ke project (materialize) |
| `GET /api/workspaces/:slug/projects/:project_id/workflow-map/`               | peta workflow project untuk web      |

- Logika type CRUD di-refactor dari `routes/v1/work_item_type.rs` (helper SQL bersama), bukan diduplikasi.
- `v1::work_item_type` payload (`TYPE_COLS` / `v1_work_item_type_json`) ditambah `workflow` (nullable) agar MCP/SDK melihat assignment, tanpa endpoint baru.
- `GET states` project (`routes/state.rs`) tetap; row typed state membawa `type_id` + `workflow_state_id` supaya web bisa membedakan read-only.

### `workflow-map`

Response (hanya type yang live dan enabled di project):

```json
{
  "types": [
    {
      "type_id": "uuid",
      "workflow_id": "uuid",
      "default_state_id": "uuid",
      "states": [
        { "id": "uuid", "name": "New", "color": "#...", "group": "backlog", "sequence": 1.0, "is_default": true }
      ],
      "transitions": [{ "from_state_id": "uuid", "to_state_id": "uuid" }]
    }
  ]
}
```

### Enforcement transisi

Satu helper `validate_state_transition(issue, target_state_id)` dipakai di semua jalur perubahan state:

- `PATCH /api/workspaces/:slug/projects/:project_id/issues/:id/` (`routes/issue_update.rs`),
- create issue (`routes/issue_write.rs`) dan draft confirm (`routes/draft.rs::create_draft_to_issue`),
- accept intake (`routes/intake.rs`),
- endpoint bulk yang menyentuh state bila/ketika diport.

Aturan:

1. `issue.type_id IS NULL` atau type `is_epic` → skip (legacy).
2. `state_id` tidak berubah (same state) → skip validasi (no-op).
3. Target state wajib hidup, project sama, `type_id == issue.type_id`; target terhapus/arsip → 400.
4. Resolve `workflow_state_id` state sekarang dan target; wajib ada `WorkflowTransition` dari current → target untuk workflow type itu; jika tidak → 400 + daftar `allowed_state_ids`.
5. Jika state sekarang tidak punya `workflow_state` (mis. issue lama yang baru diberi type tanpa ganti state) dan issue sudah punya type → satu-satunya target valid adalah default state type itu; selain itu 400.
6. Ganti `type` pada issue existing: tanpa cek transisi; `state` wajib ikut state milik type baru — kalau tidak dikirim, auto default type baru.
7. Create issue/draft confirm dengan `type` terisi dan `state` kosong → default type; `state` dikirim tapi tidak cocok type → 400.
8. Bulk: **atomic** — bila ada satu item invalid, seluruh request ditolak dengan `invalid_issue_ids`.

`IssueActivity` tetap tercatat otomatis (`Issue.TRACKED_FIELDS = ["state_id"]`, `ChangeTrackerMixin`) — tidak ada perubahan.

### Error contract

```json
{
  "error": "Invalid state transition",
  "allowed_state_ids": ["uuid", "..."],
  "invalid_issue_ids": ["uuid", "..."]
}
```

- `allowed_state_ids` untuk single-issue; `invalid_issue_ids` hanya untuk bulk.
- Status 400; guard delete/un-enable juga 400 dengan `{"error": "..."}`. Mengikuti bentuk error handler repo (`(StatusCode::BAD_REQUEST, Json(json!({"error": ...})))`), tanpa `error_code` baru.

### Guard

Semua guard delete/un-enable (state, workflow, type, `project_issue_types`) mengembalikan 400 dengan pesan spesifik; tidak ada cascade ke work item.

## Frontend (web)

### Workspace settings — kategori baru "Service management"

- **Work item types**: list + create/edit (name, description, icon/`logo_props`, `is_active`), assign workflow.
- **Workflows**: editor per workflow — daftar `WorkflowState` (name, color, group, sequence, default, drag reorder) + matriks transisi from → to (checkbox per pasangan). State default tidak bisa dihapus; hapus state yang dipakai mengembalikan error API.
- Routes di `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/...`, item sidebar via `packages/constants/src/settings/workspace.ts`.
- Semua string baru lewat `packages/i18n` (ikuti skill translate).

### Project settings

- Features page: toggle **Work item types** (`is_issue_type_enabled`).
- Page baru **Work item types**: daftar type workspace dengan toggle enable → `project_issue_types`.
- Halaman **States** existing tetap untuk state legacy; state typed tampil read-only + badge "Managed at workspace level".

### Board/views

- Filter `type_id` = satu type → kolom kanban = state milik type itu (urutan `sequence`, dari `workflow-map`).
- Mixed/tanpa filter → kolom = 5 `group` (seperti sekarang).
- Dropdown state di issue detail & quick-edit: hanya state tujuan yang diizinkan transisi (+ current), dikelompokkan per group.
- Kanban drag ke kolom invalid: kolom tidak di-highlight, drop ditolak + toast.
- Create form: pilih type (hanya enabled) → state otomatis default type; ganti type → state reset ke default type baru.
- Bulk edit state: hanya bila selection satu type dan target valid untuk semua item; selain itu disabled dengan pesan (konsisten dengan aturan atomic API).
- Draft: `type` sudah ada di draft; saat confirm, state resolve ke default type.
- Badge type di row list/spreadsheet; filter `type_id` yang sudah ada tetap.

## Testing

- **Rust** (`apps/api-rs/crates/api/tests/`): unit test validator transisi (valid, invalid, same-state no-op, type null, epic, lintas workflow, target terhapus), unit test materialization mapping, route test untuk endpoint baru + guard admin/delete + `workflow-map`.
- **Django** (`apps/api/tests`): test constraint parsial `State` (`(project, type, name)`, `(project, workflow_state)`, default per type), test resolusi default state type-aware, test data migration seed idempotent.
- **Web** (`vitest`): helper pemetaan kolom hybrid (single type vs mixed), selector allowed-next-states, reset state saat type berubah.
- **E2E smoke**: migrasi → rebuild api-rs → curl buat workflow/state/transisi → assign type → import ke project → create issue → transisi valid (200) & invalid (400 + `allowed_state_ids`) → `pnpm --filter=web build` + restart service sesuai `AGENTS.md`.

## Rollout

1. Migrasi Django: tabel `workflows`/`workflow_states`/`workflow_transitions`, kolom `issue_types.workflow_id`, `states.type_id`, `states.workflow_state_id`, constraint baru, hapus constraint lama, data migration seed workflow/type default untuk semua workspace.
2. Rebuild api-rs (`docker compose -f docker-compose-local.yml up -d --build api worker beat-worker`), verifikasi `curl http://localhost:8000/health`, restart `plane-live`.
3. Build + restart web sesuai `AGENTS.md` (prod service di port 3000).
4. Feature default `false` → tidak ada project yang berubah sampai admin opt-in.

## Edge cases

- **Legacy/epic** tidak pernah kena enforcement; setelah project import type, item lama tetap di state lama sampai type-nya diubah.
- **Nama state sama antar type** dalam satu project kini legal; UI wajib menampilkan konteks type (dropdown per type / kolom workflow).
- **Slug duplikat** antar typed state dalam satu project diperbolehkan (tidak ada constraint); referensi selalu by `id`.
- **Project archived**: sync dilewati; materialization idempotent saat project aktif kembali.
- **Concurrent edit**: validasi memakai state saat request (last-write-wins); tidak ada locking baru.
- **Ganti `group` state** setelah ada item: diizinkan; `completed_at` item lama tidak dihitung ulang.
- **Unset default terakhir**: ditolak; workflow wajib punya satu default.
- **Intake accept**: state tujuan = default type item (atau default legacy bila type null).
- **Item tanpa type di project yang sudah punya type**: tetap legacy; enforcement hanya setelah type diisi.
- **Hapus state yang dipakai item di project archived**: tetap ditolak (row item masih hidup).

## Risiko

- **Fan-out sync** ke banyak project — mitigasi: single upsert per operasi, offload worker bila perlu.
- **Bulk atomic** terasa kasar — mitigasi: `invalid_issue_ids` eksplisit di error.
- **Constraint migration** pada data existing aman karena constraint lama `(name, project)` lebih ketat daripada constraint baru; tetap diverifikasi test migrasi.
- **Dua sumber kebenaran** (workflow workspace + mirror project) — mitigasi: materialization idempotent + mapping `workflow_state`, validasi transisi tidak bergantung pada mirror.

## Peta modul & file

- **Django**: `apps/api/plane/db/models/workflow.py` (baru), `models/__init__.py`, `models/issue_type.py`, `models/state.py`, `models/issue.py`, `apps/api/plane/db/migrations/` (schema + data seed).
- **api-rs**: `crates/api/src/routes/workflow.rs` (baru), `routes/mod.rs`, `main.rs`, `seed.rs`, `routes/issue_write.rs`, `routes/issue_update.rs`, `routes/intake.rs`, `routes/draft.rs`, `routes/state.rs`, `routes/v1/work_item_type.rs`, `crates/api/tests/`.
- **Web**: `packages/types/src/workflow/*`, `packages/constants/src/settings/workspace.ts` + `project.ts`, `apps/web/core/services/workflow.service.ts`, `apps/web/core/store/workflow.store.ts`, halaman settings workspace/project, komponen issues (state dropdown/kanban/bulk), `packages/i18n/src/locales/en/*.json`.
