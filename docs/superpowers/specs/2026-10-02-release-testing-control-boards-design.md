# Release & Testing Control Boards (RCB/TCB) — Design

Date: 2026-10-02
Status: Draft (menunggu review user)
Scope: Migrasi api-rs, API (`apps/api-rs`), web (`apps/web`, `packages/types`, `packages/constants`, `packages/i18n`).
Terkait: `docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md`, `docs/superpowers/specs/2026-09-13-services-feature-design.md`, `docs/superpowers/specs/2026-09-30-war-room-design.md`, `docs/features/_backlog.md`.

## Tujuan

Menyediakan **Release Control Board (RCB)** dan **Testing Control Board (TCB)** sebagai forum review + visibilitas — bukan penegakan proses:

- **TCB (project-level):** antrean review per change item, rapat, keputusan, notulen.
- **RCB (workspace-level):** antrean review per release (bundel change lintas project), rapat, keputusan, notulen.
- Jejak audit: siapa submit, kapan, keputusan apa, catatan apa, di rapat mana.

Terinspirasi praktik ITIL (service validation & testing, release management, change enablement) namun sengaja tanpa hard gate: keputusan board tercatat dan terlihat, tidak memblokir transisi workflow atau deploy.

## Konteks saat ini

- Work item types + workflow sudah ada (workspace-level, materialized per project) dengan enforcement transisi tanpa role restriction (`apps/api-rs/crates/api/src/routes/workflow.rs`). Seeded Change workflow: `New → Assessment → Approval → Implementation → Review → Closed` (`apps/api-rs/crates/api/src/seed.rs:337-357`).
- Belum ada: entitas release, approval/review engine, decision record, custom properties (`docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md:29,36` — non-goals v1 workflow).
- Services (katalog + dependency DAG + link work item) dan War Rooms (chat realtime, participants, runbook, event feed) sudah ada sebagai pola fitur Rust-only.
- Sistem notifikasi sudah ada di api-rs (`notifications`, `apps/api-rs/migrations/0001_initial.sql:1657`); war room menulis baris notifikasi langsung (`apps/api-rs/crates/api/src/routes/war_room.rs:1909`).
- Migrasi fitur baru memakai sqlx api-rs (`apps/api-rs/migrations/`, terakhir `0011_war_rooms.sql`) tanpa model Django — pola Services/War Rooms.

## Keputusan (brainstormed & approved)

1. **Forum + visibilitas, tanpa hard gate.** Board view "menunggu review", rapat, notulen; keputusan tercatat tapi tidak memblokir.
2. **Alur:** TCB mereview change item individual → change dibundel jadi **release** → RCB mereview release.
3. **Release workspace-level** — menggabungkan change lintas project.
4. **TCB project-level** — tiap project/aplikasi punya TCB sendiri; session hanya berisi change project itu.
5. **Review Session first-class** — entitas rapat (tipe board, jadwal, peserta, agenda dari queue, notulen, outcome per agenda item).
6. **Submit eksplisit** — aksi "Submit to TCB/RCB" membuat review request di queue, terpisah dari workflow state, mendukung siklus submit → reject → revisi → resubmit.
7. **Keputusan tunggal per agenda item** oleh session owner/notulis: `approved | rejected | approved_with_notes | deferred` + catatan.
8. **Satu engine review generik** — satu set tabel/route/komponen untuk TCB & RCB, dibedakan `board_type`.

## Non-goals (v1)

- Penegakan keras: blokir deploy/transisi, role-gated approval, quorum/voting.
- SLA/timer, reminder otomatis, kalender change blackout.
- Test plan/test case terstruktur; evidence disimpan di subject (deskripsi/komentar/attachment existing) + `submission_note`.
- Custom field (risk rating, rollback plan, environment) — spec terpisah.
- Rich text notulen (v1 plain text; editor menyusul).
- Auto-ubah status release dari keputusan (status manual, keputusan = badge).
- Notifikasi email/eksternal.
- Board membership formal (roster); peserta per session saja.

## Model data

### Tabel baru (6, migrasi api-rs `0012_releases_review_control.sql`)

#### 1. `releases` — workspace-scoped

| Field              | Tipe                                                                       | Catatan                                                                                                   |
| ------------------ | -------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `id`               | uuid PK                                                                    | `gen_random_uuid()`                                                                                       |
| `workspace_id`     | FK `workspaces`                                                            | required                                                                                                  |
| `sequence`         | integer                                                                    | display `REL-{n}` per workspace; unique `(workspace_id, sequence)`                                        |
| `name`             | varchar(255)                                                               | required                                                                                                  |
| `version`          | varchar(100)                                                               | nullable                                                                                                  |
| `description_html` | text                                                                       | nullable                                                                                                  |
| `status`           | varchar(20)                                                                | `draft \| planned \| in_review \| approved \| released \| cancelled`, default `draft`, validasi app-level |
| `target_date`      | date                                                                       | nullable                                                                                                  |
| audit              | `created_by_id`, `updated_by_id`, `created_at`, `updated_at`, `deleted_at` | soft delete                                                                                               |

Index: `(workspace_id, deleted_at)`, `(workspace_id, status)`.

#### 2. `release_changes` — bundel release ↔ work item

`id`, `workspace_id`, `release_id` FK `releases`, `issue_id` FK `issues`, `project_id` FK `projects` (denormalisasi untuk query lintas project), audit.

- Partial unique `(release_id, issue_id) WHERE deleted_at IS NULL`.
- Index `(issue_id)`, `(release_id)`.
- Pola mengikuti `module_issues`/`service_issues`.
- Schema tidak memaksa type `Change`; UI default memfilter type `Change`.

#### 3. `review_requests` — queue generik TCB/RCB

| Field                | Tipe                   | Catatan                                                           |
| -------------------- | ---------------------- | ----------------------------------------------------------------- |
| `id`, `workspace_id` |                        |                                                                   |
| `board_type`         | varchar(3)             | `tcb \| rcb`                                                      |
| `change_issue_id`    | FK `issues` nullable   | subject TCB                                                       |
| `release_id`         | FK `releases` nullable | subject RCB                                                       |
| `project_id`         | FK `projects` nullable | terisi untuk TCB, null untuk RCB                                  |
| `status`             | varchar(10)            | `pending \| scheduled \| decided \| withdrawn`, default `pending` |
| `submission_note`    | text                   | nullable                                                          |
| `submitted_by_id`    | FK users               |                                                                   |
| `submitted_at`       | timestamptz            |                                                                   |
| audit                |                        | soft delete                                                       |

`subject_kind` tidak dipakai — `board_type` sudah menentukan subject (tcb↔change, rcb↔release).

CHECK:

- `board_type = 'tcb'` ⇒ `change_issue_id IS NOT NULL AND project_id IS NOT NULL AND release_id IS NULL`
- `board_type = 'rcb'` ⇒ `release_id IS NOT NULL AND change_issue_id IS NULL AND project_id IS NULL`

Partial unique (satu request aktif per subject per board):

- `(change_issue_id) WHERE board_type = 'tcb' AND status IN ('pending','scheduled') AND deleted_at IS NULL`
- `(release_id) WHERE board_type = 'rcb' AND status IN ('pending','scheduled') AND deleted_at IS NULL`

Index: `(workspace_id, board_type, status)`, `(project_id)`.

#### 4. `review_sessions` — rapat TCB/RCB

| Field                          | Tipe                   | Catatan                                                    |
| ------------------------------ | ---------------------- | ---------------------------------------------------------- |
| `id`, `workspace_id`           |                        |                                                            |
| `board_type`                   | varchar(3)             | `tcb \| rcb`                                               |
| `project_id`                   | FK `projects` nullable | CHECK: tcb wajib, rcb null                                 |
| `title`                        | varchar(255)           |                                                            |
| `scheduled_at`                 | timestamptz            |                                                            |
| `status`                       | varchar(10)            | `scheduled \| completed \| cancelled`, default `scheduled` |
| `minutes`                      | text                   | nullable, plain text v1                                    |
| `location`                     | varchar(255)           | nullable                                                   |
| `completed_at`, `cancelled_at` | timestamptz            | nullable                                                   |
| audit                          |                        | soft delete                                                |

CHECK board↔scope: `tcb ⇒ project_id IS NOT NULL`; `rcb ⇒ project_id IS NULL`.

#### 5. `review_session_participants`

`id`, `session_id` FK, `user_id` FK, `role` (`chair | secretary | member`, default `member`), `attendance` (`invited | present | absent`, default `invited`), audit.

- Partial unique `(session_id, user_id) WHERE deleted_at IS NULL`.

#### 6. `review_session_items` — agenda + hasil rapat

`id`, `workspace_id`, `session_id` FK, `review_request_id` FK, `position` integer, `outcome` nullable (`approved | rejected | approved_with_notes | deferred`), `outcome_note` text nullable, `decided_by_id` FK `users` nullable, `decided_at` timestamptz nullable, audit.

- Partial unique `(session_id, review_request_id) WHERE deleted_at IS NULL`.
- Index `(review_request_id)`.

### Lifecycle

**Review request**

- `pending` (di queue board) → `scheduled` (masuk agenda session berstatus `scheduled`) → `decided` (outcome final) | kembali `pending` (outcome `deferred`, atau session cancelled) | `withdrawn` (hanya dari `pending`/`scheduled`).
- Status request diperbarui **saat outcome dicatat** (tidak menunggu complete session): final → `decided`; `deferred` → `pending` (tetap di agenda sampai session selesai/dibatalkan).
- **Resubmit** setelah rejected = request **baru** untuk subject yang sama; request lama tetap sebagai riwayat. Halaman subject menampilkan request terbaru + timeline review.

**Release**

- Transisi manual: `draft → planned → in_review → approved → released`; `cancelled` kapan saja.
- Submit ke RCB = konvensi set `in_review` (bukan gate).
- Keputusan RCB tampil sebagai badge; **tidak** auto-mengubah status.

**Review session**

- `scheduled → completed` (wajib ≥1 agenda item; item tanpa outcome auto-`deferred`).
- `scheduled → cancelled` hanya bila belum ada outcome final; semua request `scheduled` kembali `pending`.

## Flow

### TCB (project-level)

1. Change owner menyelesaikan change, klik **Submit to TCB** di detail change → request `pending` + `submission_note`.
2. Project admin buat session TCB (judul, `scheduled_at`, peserta) → pilih request dari queue ke agenda secara manual (bisa "add all pending"; tidak ada auto-populate oleh sistem) → request jadi `scheduled`.
3. Saat rapat, session owner/secretary catat `outcome` + catatan per agenda item, lalu isi notulen.
4. Complete session → outcome final set request `decided`; `deferred` balik `pending`; cancel → semua `scheduled` balik `pending`.
5. Ditolak → owner revisi change → submit request baru (siklus tercatat).
6. Change TCB-approved bisa dipilih ke release. Picker menampilkan badge status TCB; change belum approved tetap bisa dimasukkan (warning, bukan blokir).

### RCB (workspace-level)

1. Release manager buat release (`draft`), link change lintas project.
2. **Submit to RCB** → request `pending`; status release ikut `in_review` (konvensi, bukan gate).
3. Workspace admin buat session RCB; agenda dari queue RCB; outcome per release.
4. Keputusan tampil sebagai badge di release; status `approved`/`released` diubah manual oleh release manager.
5. Rejected → revisi release (tukar change, ubah jadwal) → submit request baru.

## Guard & aturan

- Satu subject hanya boleh punya **1 request aktif** (`pending`/`scheduled`) per board → submit kedua 400 (dijaga partial unique index).
- Satu request hanya boleh ada di satu agenda session berstatus `scheduled`.
- Hapus/arsip subject (change/release) selama ada request aktif → 400.
- Edit lingkup release (link/unlink change) saat review aktif → diizinkan, dicatat di activity + warning di UI (tidak dibekukan, konsisten "tanpa penegakan").
- Withdraw hanya saat `pending`/`scheduled`; `decided` tidak bisa.
- Complete session: wajib ≥1 item; item kosong auto-`deferred`; item dengan outcome final tidak bisa dihapus dari agenda (blank/`deferred` bisa).
- Cancel session hanya bila belum ada outcome final.
- Withdraw request yang `scheduled` → otomatis keluar dari agenda.
- Delete subject setelah `decided` → riwayat request tetap hidup (soft delete).

## Permission

| Aksi                                            | Siapa                                                        |
| ----------------------------------------------- | ------------------------------------------------------------ |
| Lihat queue/session TCB                         | project member                                               |
| Lihat release, queue/session RCB                | workspace member                                             |
| Submit change → TCB                             | project member                                               |
| Submit release → RCB                            | workspace member (pembuat release)                           |
| Withdraw request                                | submitter; project admin (TCB) / workspace admin (RCB)       |
| Buat/edit/complete/cancel session, atur peserta | session creator; project admin (TCB) / workspace admin (RCB) |
| Catat outcome & notulen                         | session creator/secretary; admin                             |
| CRUD release + link change                      | workspace member; delete: creator/workspace admin            |

## API (api-rs)

Semua route di bawah `/api/workspaces/:slug/...`, cookie-auth internal + workspace member check. Route TCB menambah validasi project membership. Enum divalidasi app-level; guard 400 `{"error": "..."}` (pola workflow). List memakai pagination pola Services.

### Releases — `routes/release.rs`

| Route                                     | Fungsi                                                                                |
| ----------------------------------------- | ------------------------------------------------------------------------------------- |
| `GET/POST /releases/`                     | list (filter `status`, `target_date`) / create                                        |
| `GET/PATCH/DELETE /releases/:id/`         | detail (linked changes + review history) / update / soft delete (guard request aktif) |
| `GET/POST /releases/:id/changes/`         | list / link `issue_ids[]`                                                             |
| `DELETE /releases/:id/changes/:issue_id/` | unlink                                                                                |

### Review requests — `routes/review.rs`

| Route                                 | Fungsi                                                                                                                                                                                                         |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /review-requests/`               | list — filter `board_type`, `project_id`, `status`, `change_issue_id`, `release_id` (queue + badge); response menyertakan subject summary (issue: id/identifier/name/project; release: id/name/version/status) |
| `POST /review-requests/`              | submit (`board_type`, `change_issue_id` \| `release_id`, `submission_note`)                                                                                                                                    |
| `GET /review-requests/:id/`           | detail + riwayat siklus                                                                                                                                                                                        |
| `POST /review-requests/:id/withdraw/` | tarik                                                                                                                                                                                                          |

### Review sessions — `routes/review.rs`

| Route                                                         | Fungsi                                                                                         |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `GET/POST /review-sessions/`                                  | list (filter board, project, status) / create                                                  |
| `GET/PATCH /review-sessions/:id/`                             | detail (agenda, peserta, notulen) / update                                                     |
| `POST /review-sessions/:id/complete/`                         | tutup (≥1 item; kosong auto-deferred)                                                          |
| `POST /review-sessions/:id/cancel/`                           | batalkan (hanya tanpa outcome final)                                                           |
| `GET/POST /review-sessions/:id/items/`                        | agenda: list / add `request_ids[]`                                                             |
| `PATCH/DELETE /review-sessions/:id/items/:item_id/`           | set outcome+catatan / hapus dari agenda (selama `scheduled`, outcome final tidak bisa dihapus) |
| `GET/POST/PATCH/DELETE /review-sessions/:id/participants/...` | kelola peserta + attendance                                                                    |

## Frontend (web)

### Route

| Route                                                    | Isi                                                                                    |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `/[workspaceSlug]/releases/`                             | list release (status, target date, progress TCB change)                                |
| `/[workspaceSlug]/releases/[releaseId]/`                 | detail: overview, daftar change (picker + badge TCB), riwayat review, Submit to RCB    |
| `/[workspaceSlug]/release-control/`                      | board RCB: tab Queue (pending/scheduled/decided) + Sessions                            |
| `/[workspaceSlug]/release-control/sessions/[sessionId]/` | detail rapat: agenda + outcome, notulen, peserta, complete/cancel                      |
| `/[workspaceSlug]/projects/[projectId]/testing-control/` | board TCB (Queue + Sessions); nav muncul hanya bila type `Change` di-import ke project |
| `.../testing-control/sessions/[sessionId]/`              | detail rapat TCB                                                                       |

### Integrasi work item

- Detail Change: tombol **Submit to TCB** + badge status TCB terakhir (pending/scheduled/approved/rejected).
- Detail release: tombol **Submit to RCB** + badge keputusan.

### Implementasi

- Komponen board/session dibangun sekali dan diparameterisasi `board_type` (queue, session list, session detail, submit dialog).
- Store MobX + service + `packages/types/src/release/*` dan `packages/types/src/review/*`.
- String baru via `packages/i18n` mengikuti skill translate.
- TCB subject: API tidak memvalidasi nama type (soft); UI memfilter type `Change` dan nav kondisional.

## Notifikasi

Reuse tabel `notifications` (pola `routes/war_room.rs:1909`), `entity_name` baru `review_request` / `review_session`:

| Event (`sender`)                  | Penerima                                |
| --------------------------------- | --------------------------------------- |
| `in_app:review:agenda_added`      | submitter saat request masuk agenda     |
| `in_app:review:decided`           | submitter saat outcome final dicatat    |
| `in_app:review:session_scheduled` | peserta saat session dibuat/dijadwalkan |

`project_id` terisi untuk TCB, null untuk RCB. Web menambah kartu render untuk dua entity baru (pola komponen notifikasi war room).

## Edge cases

- Satu subject = satu request aktif (partial unique index per board).
- Complete session 0 item → 400; item tanpa outcome → auto-`deferred`.
- Cancel session dengan outcome final → 400.
- Item outcome final tidak bisa dihapus dari agenda.
- Withdraw saat `scheduled` → keluar dari agenda otomatis.
- Submit TCB: API tidak cek type; UI filter `Change`, nav kondisional (case-insensitive).
- Release delete: guard request aktif; soft delete; baris `release_changes` ikut tersembunyi.
- Delete subject setelah `decided` → riwayat request tetap hidup.
- `scheduled_at` timestamptz; list pagination pola Services.
- Concurrent edit: last-write-wins (konsisten repo); tidak ada locking baru.

## Testing

- **Rust** (`apps/api-rs/crates/api/tests/release_review_test.rs`): CRUD release + guard, submit/duplikat/withdraw, agenda & outcome, complete/cancel semantics, CHECK board↔scope, authorization project membership. Scratch-workspace suite → `-- --test-threads=1`.
- **Web** (vitest): helper badge status, grouping queue, validasi outcome item.
- **E2E smoke**: migrasi → rebuild api-rs → curl release → link change → submit TCB → session → outcome → complete → submit RCB → build web + restart service sesuai `AGENTS.md`.

## Rollout

1. Satu migrasi api-rs baru `0012_releases_review_control.sql` (pola Services/War Rooms; tanpa model Django — fitur Rust-only).
2. Rebuild `api worker beat-worker` + restart `plane-live` (sesuai `AGENTS.md`).
3. Build web prod + restart `plane-web-prod`.
4. Tanpa feature flag: halaman Releases/RCB selalu tampil (empty state); TCB kondisional type `Change`.

## Risiko

- **Abstraksi berlebih** model generik bila TCB/RCB divergen → mitigasi: discriminator `board_type` + CHECK ketat; divergensi ditangani sebagai kolom/route tambahan, bukan fork tabel.
- **Query lintas project** untuk release → potensi N+1; mitigasi: `project_id` denormalisasi di `release_changes` + index.
- **Notifikasi entity baru** butuh render web → kerja kecil tapi wajib agar tidak blank.
- **Dependensi type Change** di TCB → project yang rename type kehilangan nav; mitigasi: match case-insensitive + empty state.

## Peta modul & file

- **api-rs**: `migrations/0012_releases_review_control.sql` (baru); `crates/api/src/routes/release.rs`, `routes/review.rs` (baru); `routes/mod.rs`; `main.rs`; `crates/api/tests/release_review_test.rs`.
- **Web**: `apps/web/app/routes/core.ts` + halaman `app/(all)/[workspaceSlug]/...`; komponen `core/components/releases/**`, `core/components/review/**`; store/service/hooks; `packages/types/src/release/*` + `packages/types/src/review/*`; `packages/constants`; `packages/i18n/src/locales/*`.
