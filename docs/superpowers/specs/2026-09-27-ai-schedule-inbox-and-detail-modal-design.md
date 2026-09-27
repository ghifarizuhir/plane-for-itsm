# AI Schedule: Notifikasi Run di Inbox + Modal Detail Schedule — Design

Tanggal: 2026-09-27
Status: disetujui user (2/2 bagian), menunggu review spec tertulis.

## Latar

Slice sebelumnya (`2026-09-27-ai-schedule-recipe-design.md`) membuat jadwal
sebagai resep terstruktur dan menegakkan hard allowlist saat run. Yang belum
ada:

- Hasil run terjadwal tidak memberi tahu apa pun. Pemilik jadwal baru tahu
  laporan siap (atau run gagal) kalau membuka halaman Scheduler dan menekan
  History.
- Halaman Scheduler berupa satu list dengan history + recipe inline. Dengan
  bertambahnya jadwal, item menjadi ramai, tidak ada search/filter, dan cap 20
  jadwal/workspace terlalu kecil untuk pemakaian ITSM nyata.

Tujuan slice ini: (1) notifikasi per-user di Inbox saat run terjadwal selesai,
dengan klik menuju modal detail jadwal; (2) halaman Scheduler yang skalabel —
list ringkas dengan search/filter, detail (recipe + history + aksi) dipindah ke
modal, dan cap dinaikkan.

## Keputusan yang dikunci saat brainstorming

1. Peran history di Inbox = **notifikasi per-user saat run selesai** (badge
   unread, archive, snooze). History lengkap tetap di halaman Scheduler; Inbox
   bukan reader laporan.
2. Run yang memicu notifikasi: **hanya `trigger='scheduled'`**, status terminal
   success maupun failed. Run manual (`Run now`) tidak memberi notifikasi.
3. Penerima: **hanya pembuat jadwal** (`created_by_id`).
4. Klik notifikasi → **`/{slug}/scheduler?schedule=<id>`**, modal detail jadwal
   terbuka. Inbox tetap ringkas sebagai notifikasi.
5. Struktur halaman Scheduler: **list + modal detail per jadwal**. Recipe
   read-only (edit jadwal tersimpan tetap out of scope), history 20 run
   terakhir, aksi (toggle, Run now, Delete) pindah ke modal.
6. Skala: **cap 20 → 100 jadwal/workspace**, search/filter/sort di client, tanpa
   pagination server (YAGNI).
7. Notifikasi memakai **tipe baru** `entity_name='ai_schedule_run'` di tabel
   `notifications` yang ada — bukan menumpang notifikasi issue, bukan tabel feed
   baru. Mekanisme unread/archive/snooze/badge yang generik ikut terpakai.
8. Jadwal dihapus (soft delete) → notifikasi terkait ikut di-soft-delete agar
   Inbox tidak menyimpan tautan mati.
9. Tanpa perubahan pada kartu konfirmasi resep, worker allowlist, atau endpoint
   create; slice ini murni notifikasi + UI Scheduler.

## Desain

### 1. Row notifikasi & penulisannya (worker Rust)

Satu helper SQL idempotent di `crates/worker/src/handlers/ai_schedule.rs`:

```
insert_run_notification(pool, run_id) ->
INSERT INTO notifications (...)
SELECT gen_random_uuid(), r.workspace_id, s.created_by_id,
       'ai_schedule_run', r.id, s.name, 'in_app:ai_schedule:run',
       jsonb_build_object('ai_schedule', jsonb_build_object(
         'schedule_id', s.id, 'run_id', r.id, 'name', s.name,
         'status', r.status, 'finished_at', r.finished_at, 'error', r.error)),
       now(), now()
FROM ai_schedule_runs r JOIN ai_schedules s ON s.id = r.schedule_id
WHERE r.id = $1 AND r.trigger = 'scheduled'
  AND r.status IN ('success','failed') AND s.deleted_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM notifications n
                  WHERE n.entity_name = 'ai_schedule_run'
                    AND n.entity_identifier = r.id)
```

- Kolom: `project_id=NULL`, `triggered_by_id=NULL` (sistem),
  `message_html` memakai default DB, `created_at`/`updated_at` = `now()`.
  `receiver_id`/`workspace_id` dari jadwal; `title` = nama jadwal;
  `entity_identifier` = `run_id`.
- Idempoten per run lewat `NOT EXISTS`; aman dipanggil berulang.
- Dipanggil di semua jalur terminal run yang dijalankan worker: success, failed
  (LLM error), timeout 180 s, spec invalid, dan AI belum dikonfigurasi.
  `finish_failed` memanggil helper setelah update berhasil
  (`rows_affected == 1`).
- `sweep_stuck_runs` juga memanggil helper untuk run yang baru saja ditandai
  failed: kedua `UPDATE` diberi `RETURNING id` lalu helper dipanggil per id.
  Run yang di-sweep dan berstatus `scheduled` tetap memberi tahu pemiliknya.
- Run yang tidak diklaim (`missing run_id`, `run already claimed or swept`) tidak
  menotifikasi karena bukan transisi terminal dari handler ini.
- Worker tidak perlu field baru di `RunRow`; helper mengambil trigger, status,
  dan data jadwal langsung lewat join.

### 2. Filter list Inbox (API)

- Rust `crates/api/src/routes/notification.rs`: `n.entity_name = 'issue'`
  (baris ~265) → `n.entity_name IN ('issue', 'ai_schedule_run')`.
- Django parity `apps/api/plane/app/views/notification/base.py`: filter
  `entity_name="issue"` → `entity_name__in=["issue", "ai_schedule_run"]`
  (kontrak suite; runtime live tetap Rust).
- Yang otomatis tetap benar tanpa perubahan:
  - `unread` count tidak memfilter `entity_name`, jadi badge sudah konsisten
    begitu list menampilkan tipe baru.
  - Filter `type` (Assigned/Created/Subscribed) hanya mencocokkan issue, jadi
    notifikasi run otomatis tersisih saat filter itu aktif.
  - Tab Mentions memakai `sender ILIKE '%mentioned%'`; sender kita tidak cocok.
  - `is_inbox_issue`/`is_intake_issue` mengecek `entity_identifier` terhadap
    `issues`; `run_id` tidak akan cocok → `false`.

### 3. Cleanup saat jadwal dihapus

`destroy` di `crates/api/src/routes/ai_schedule.rs` (soft delete jadwal) juga
menjalankan:

```
UPDATE notifications SET deleted_at = now(), updated_at = now()
WHERE entity_name = 'ai_schedule_run' AND deleted_at IS NULL
  AND data->'ai_schedule'->>'schedule_id' = $1
```

Run yang sudah dipruning (`prune_runs` menyimpan 20 run terakhir) tidak
menghapus notifikasinya; klik tetap membuka modal jadwal dan history terbaru
tetap tersedia.

### 4. FE — kartu notifikasi di Inbox

- `packages/types/src/workspace-notifications.ts`: `issue` dan `issue_activity`
  pada `TNotificationData` menjadi optional; tambah
  `ai_schedule?: { schedule_id, run_id, name, status, finished_at, error? }`.
- `notification-card/item.tsx`:
  - Guard lama yang `return <></>` saat `data.issue_activity.field` tidak ada
    diubah: cabang `data.ai_schedule` dirender sebagai kartu notifikasi sistem.
  - Isi kartu: ikon sistem (bukan avatar; `triggered_by_details` null), judul =
    nama jadwal, baris kedua = badge status (`Success`/`Failed`, error
    terpotong) + time-ago. Unread dot dan aksi hover (read/archive/snooze)
    memakai mekanisme lama.
  - Klik: tandai read (handler lama) lalu
    `router.push('/{workspaceSlug}/scheduler?schedule=<schedule_id>')`. Tidak
    menyentuh `setPeekIssue` / intake embed.
- `notification-card/content.tsx`: cabang kalimat untuk tipe ini ("Scheduled run
  finished" / "Scheduled run failed").
- `store/notifications/workspace-notifications.store.ts`: `issue_id` pada
  `notificationLiteByNotificationId` menjadi nullable (dipakai hanya untuk
  intake embed; tipe baru selalu `null`).
- String baru hardcoded English mengikuti komponen `ai-scheduler`; tidak
  menambah key i18n di slice ini.

### 5. FE — halaman Scheduler: list ringkas

- `scheduler-view.tsx`:
  - Header: judul, Refresh, input search nama, filter status
    (All/Active/Paused), count hasil.
  - Search/filter/sort murni di client lewat helper di `lib/ai-schedule.ts`:
    active lebih dulu, lalu `next_run_at` ascending, paused di bawah; item tanpa
    `next_run_at` di akhir.
  - State loading/error/empty yang ada dipertahankan; tambah state "tidak ada
    hasil" saat filter/search tidak cocok.
- `schedule-item.tsx` menjadi ringkas: nama, badge status, humanized schedule +
  next run, `scheduleDescription`, last-run status/time, creator. Klik item
  membuka modal detail. Tombol Run now / toggle / Delete dan disclosure Recipe
  & history inline dihapus dari item (pindah ke modal).
- Cap: `MAX_SCHEDULES_PER_WORKSPACE` 20 → 100 di
  `crates/api/src/routes/ai_schedule.rs` (+ update test cap yang ada).

### 6. FE — modal detail jadwal

- Komponen baru `ai-scheduler/schedule-detail-modal.tsx`, memakai `ModalCore`
  dari `@plane/ui` (width XXL), state buka/tutup dimiliki `scheduler-view.tsx`.
- Isi:
  - Header: nama, badge enabled/paused, jadwal humanized, next run, creator.
  - Recipe read-only: description, How to (ordered list), Tools (badge), Expected
    output. Jadwal legacy tanpa `spec` menampilkan fallback read-only (prompt).
  - History: reuse `ScheduleRunsList`; `fetchRuns` dipanggil saat modal dibuka;
    loading/error history ditangani di dalam modal.
  - Footer (hanya `canManage` = creator atau workspace ADMIN): toggle
    enable/disable, Run now, Delete (`AlertModalCore` yang sudah ada). Member
    lain melihat modal read-only.
- Tanpa perubahan API: endpoint detail sudah membawa `spec` + 20 run terakhir,
  dan list sudah membawa seluruh field jadwal termasuk `spec`.

### 7. FE — deep link

- `?schedule=<id>` dibaca `scheduler-view.tsx` saat mount dan saat berubah:
  jika id ada di list yang termuat → buka modal; jika tidak dikenal → abaikan
  dan bersihkan param.
- Menutup modal → `router.replace` tanpa param `schedule` (tidak menambah
  history browser).
- Klik notifikasi di Inbox mengandalkan mekanisme ini; tidak ada endpoint atau
  route baru.

### 8. Batas & error handling

- Notifikasi hanya untuk `trigger='scheduled'`; run manual tidak.
- Volume notifikasi mengikuti jumlah run terjadwal (≤100 jadwal/workspace).
  Tidak ada retensi notifikasi baru; notifikasi ikut ter-soft-delete saat jadwal
  dihapus.
- Sweep stuck run tetap berlaku (15 menit running / 6 jam queued) dan kini ikut
  menotifikasi untuk run scheduled.
- `NOT EXISTS` menjamin satu notifikasi per run walau helper dipanggil dari
  beberapa jalur.
- Rollback parsial (worker baru + API lama) membuat badge unread menghitung row
  yang tidak terlihat; karena itu api, worker, beat-worker di-rebuild bersama
  dalam satu perintah compose.

## Testing dan verifikasi

- Worker integration (`apps/api-rs/crates/worker/tests/ai_schedule_test.rs`):
  scheduled success → 1 notifikasi
  dengan `entity_name`, `receiver`, `data.ai_schedule` benar; scheduled failed →
  1; manual success → 0; helper dipanggil dua kali → tetap 1; jadwal
  ter-soft-delete → 0; sweep stuck scheduled → 1.
- API Rust (`apps/api-rs/crates/api/tests/ai_schedule_test.rs`): list Inbox
  memuat `ai_schedule_run`; filter
  archived/read/mentioned tetap benar; unread count konsisten; `destroy` jadwal
  → notifikasi terkait ter-soft-delete; cap 100 (create ke-101 ditolak).
- Django contract (`apps/api/plane/tests/contract/api/test_issue_notifications.py`):
  parity filter list `entity_name__in`.
- FE vitest: helper search/filter/sort; type guard `isScheduleRunNotification`
  dan pembentuk URL target; store notifications tetap lolos test setelah `issue`
  nullable; `fetchRuns` tidak berubah.
- Gate repo: `cargo test -p api -- --test-threads=1`, `cargo clippy`, test paket
  web, lalu rebuild api/worker/beat-worker + web sesuai AGENTS.md.
- Smoke manual di tunnel: buat jadwal → `Run now` (tidak ada notif) → tunggu run
  terjadwal berikutnya atau paksa lewat tick → notifikasi muncul di Inbox →
  klik → halaman Scheduler terbuka dengan modal jadwal yang benar → history
  tampil → toggle/Run now/Delete bekerja → legacy jadwal (tanpa spec) tetap
  tampil; member non-creator melihat modal read-only.

## Rollout & rollback

- Tanpa migrasi baru: tabel `notifications` dan kolom `ai_schedules.spec` sudah
  ada.
- Rebuild `api`, `worker`, `beat-worker` dalam satu perintah compose (filter
  list API dan penulis notifikasi worker harus naik bersamaan), lalu rebuild web
  - restart prod.
- Rollback: revert kode. Row `entity_name='ai_schedule_run'` yang sudah terlanjur
  dibuat tidak terlihat oleh list lama tapi ikut terhitung di unread badge;
  bersihkan manual bila perlu:
  `UPDATE notifications SET deleted_at = now() WHERE entity_name='ai_schedule_run';`

## Out of scope (eksplisit)

- Pagination history >20 run per jadwal; endpoint history terpisah.
- Search/filter/sort server-side; pagination list jadwal.
- Edit jadwal tersimpan (tetap hapus + buat ulang via chat).
- Konfigurasi penerima notifikasi per jadwal; notifikasi email.
- Reader laporan di pane kanan Inbox; grouping/rollup notifikasi.
- Retensi/pembersihan notifikasi berkala.
- Key i18n untuk string baru (mengikuti pola hardcoded komponen `ai-scheduler`).

## Risiko dan mitigasi

- **Tipe non-issue pertama** menyentuh asumsi issue yang hardcoded (filter list,
  guard kartu, routing klik) → tiap titik diubah eksplisit + test Rust/FE.
- **Badge unread tidak sinkron** bila worker naik sebelum API/FE → rebuild
  api/worker/beat-worker bersama; rollback SQL disediakan.
- **Notifikasi menunjuk run yang sudah dipruning** → klik tetap membuka modal
  jadwal; history 20 terakhir tetap ada.
- **Sweep menotifikasi run lama saat backlog** → hanya run `scheduled` yang
  sudah lewat horizon; volume kecil dan terlihat di Inbox.
- **Cap 100 membuat list panjang** → search/filter client-side + modal detail;
  payload list tetap kecil (≤100 baris tanpa runs).

## File map

- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs` — helper
  `insert_run_notification`, panggilan di jalur terminal + sweep `RETURNING`.
- Modify: `apps/api-rs/crates/api/src/routes/notification.rs` — filter list
  `entity_name IN (...)`.
- Modify: `apps/api-rs/crates/api/src/routes/ai_schedule.rs` — cleanup notifikasi
  saat destroy; cap 100.
- Modify: `apps/api/plane/app/views/notification/base.py` — parity filter.
- Modify: `packages/types/src/workspace-notifications.ts` — data varian
  `ai_schedule`.
- Modify: `apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`
  dan `content.tsx` — cabang render + klik.
- Modify: `apps/web/core/store/notifications/workspace-notifications.store.ts` —
  `issue_id` nullable.
- Modify: `apps/web/core/components/ai-scheduler/scheduler-view.tsx` — header
  search/filter, deep link, pemilik state modal.
- Modify: `apps/web/core/components/ai-scheduler/schedule-item.tsx` — item
  ringkas, klik membuka modal.
- Create: `apps/web/core/components/ai-scheduler/schedule-detail-modal.tsx`.
- Modify: `apps/web/core/lib/ai-schedule.ts` (+ `.test.ts`) — helper
  search/filter/sort, type guard, URL target.
- Modify (test): `apps/api-rs/crates/api/tests/ai_schedule_test.rs`,
  `apps/api-rs/crates/worker/tests/ai_schedule_test.rs`,
  `apps/api/plane/tests/contract/api/test_issue_notifications.py`, dan test web
  yang relevan (`lib/ai-schedule.test.ts`, store notifikasi).
