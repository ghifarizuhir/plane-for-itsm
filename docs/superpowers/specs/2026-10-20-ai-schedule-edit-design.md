# Edit Jadwal AI (Galileo Scheduler) — Design

Tanggal: 2026-10-20
Status: disetujui user (4/4 bagian), menunggu review spec tertulis.

## Latar

AI Scheduler saat ini mendukung siklus hidup berikut:

- Membuat jadwal hanya lewat Galileo dengan slash command `/schedule` dan tool
  proposal `create_schedule` (`crates/ai/src/tools/proposals.rs`).
- Mengelola jadwal tersimpan lewat halaman Scheduler: pause/resume, run now,
  dan soft delete (`schedule-detail-modal.tsx`).
- `PATCH /api/workspaces/:slug/ai-schedules/:schedule_id/` hanya menerima
  `{"enabled": bool}` (`routes/ai_schedule.rs`).
- Galileo tidak punya tool untuk melihat, mencari, atau mengubah jadwal
  tersimpan. `PROPOSAL_TOOL_NAMES` hanya memuat `create_schedule`.
- Resep jadwal tersimpan ditampilkan read-only di detail modal dan tidak bisa
  diubah. Satu-satunya cara mengubah isi atau cadence adalah hapus lalu buat
  ulang lewat chat.

Keterbatasan lain yang relevan:

- Kartu konfirmasi create menampilkan cadence read-only, sehingga kesalahan
  timezone/cadence tidak bisa diperbaiki di kartu.
- Worker mem-parse jadwal due dengan `ScheduleProposal::new` yang membatasi
  prompt 2000 karakter, sedangkan resep terstruktur boleh sampai 8000 karakter
  (`SPEC_PROMPT_MAX`). Resep panjang bisa dibuat, lalu dinonaktifkan otomatis
  oleh tick berikutnya.

Tujuan slice ini: jadwal tersimpan bisa diedit penuh dari Scheduler UI dan dari
percakapan Galileo, dengan satu jalur validasi backend dan tanpa kehilangan
riwayat run.

## Keputusan yang dikunci saat brainstorming

1. Cakupan: **UI edit + tool Galileo**, berbagi satu jalur PATCH.
2. Field yang bisa diedit: **semua** — name, resep (description, how_to, tools,
   expected_output), prompt legacy, cadence (frequency/time/day), timezone, dan
   `enabled`.
3. Galileo memakai **proposal + kartu konfirmasi before/after**; tidak ada
   update langsung.
4. Tool Galileo yang ditambahkan hanya **list/get/update**. Pause, resume,
   delete, dan run-now tetap UI-only.
5. Backend memakai **PATCH partial update** yang sudah ada, bukan endpoint PUT
   baru atau revisi immutable.

## Desain

### 1. Kontrak API PATCH

`PATCH /api/workspaces/:slug/ai-schedules/:schedule_id/`

Body partial (semua opsional, minimal satu field):

```json
{
  "enabled": true,
  "name": "Weekly overdue report",
  "description": "...",
  "how_to": ["..."],
  "tools": ["search_work_items"],
  "expected_output": "...",
  "prompt": "...",
  "frequency": "weekly",
  "time": "09:00",
  "day_of_week": 1,
  "day_of_month": null,
  "timezone": "Asia/Jakarta"
}
```

Aturan:

- Key di luar daftar ditolak `400` (`deny_unknown_fields`); body tanpa satu pun
  field non-null juga `400`.
- Baris dimuat dengan `load_schedule` (workspace-scoped, `deleted_at IS NULL`);
  tidak ditemukan → `404`. Otorisasi tetap creator atau workspace admin
  (`can_manage`); selain itu ditolak seperti sekarang.
- Field di-merge dengan nilai tersimpan, lalu **hasil merge** divalidasi.
  Validasi patch secara terpisah tidak cukup (mis. pindah daily → weekly butuh
  `day_of_week`).
- Jadwal terstruktur (`spec` ada):
  - `prompt` langsung ditolak `400`.
  - Bila salah satu dari `name`/`description`/`how_to`/`tools`/
    `expected_output` ikut diubah, merge keempat field resep dengan nilai lama,
    panggil `ScheduleRecipe::new(...)`, lalu render ulang `prompt` secara
    deterministik (`render_schedule_prompt`).
  - Patch yang hanya berisi `enabled`/cadence tidak membangun ulang spec atau
    prompt; spec tersimpan tetap dipakai untuk validasi cadence.
- Jadwal legacy (`spec` NULL):
  - `prompt` boleh diubah (1–2000 karakter, sama dengan batas legacy).
  - Field resep terstruktur ditolak `400` dengan pesan agar membuat ulang
    jadwal sebagai resep.
  - Name/cadence/timezone tetap bisa diubah.
- Normalisasi cadence mengikuti `ScheduleProposal::with_prompt_limit`:
  - `frequency` dinormalisasi ke lowercase dan harus salah satu dari
    `hourly|daily|weekly|monthly`.
  - `time` harus `HH:MM` 24 jam; hourly hanya memakai menit.
  - `weekly` wajib punya `day_of_week` 1–7; `monthly` wajib punya
    `day_of_month` 1–31.
  - `day_of_week`/`day_of_month` yang tidak relevan dinormalkan menjadi NULL.
  - `timezone` harus IANA zone yang valid; default tetap `UTC` hanya saat create.
- `next_run_at`:
  - Recompute dari `now` bila hasil akhir `enabled = true` **dan** salah satu
    dari `enabled` berubah false→true, `frequency`, `time`, `day_of_week`,
    `day_of_month`, atau `timezone` berubah.
  - Edit name/resep/prompt saja tidak menggeser `next_run_at`.
  - Pause (`enabled=false`) tidak mengubah `next_run_at`, konsisten dengan
    perilaku sekarang; resume menghitung ulang.
- Edit berlaku untuk run berikutnya. Run yang sudah `queued`/`running` tetap
  memakai snapshot `prompt` lamanya. Riwayat run tidak dihapus.
- Respons `200` berisi jadwal lengkap setelah update dalam bentuk
  `schedule_json` (bagian jadwal pada list, tanpa `runs`/`last_*`), sehingga FE
  bisa langsung merge.
- Konkurensi memakai last-write-wins; tidak ada optimistic locking di slice ini.

### 2. Perbaikan batas prompt worker

`DueSchedule::proposal()` di `crates/worker/src/handlers/ai_schedule.rs` harus
memakai `ScheduleProposal::with_prompt_limit(..., SPEC_PROMPT_MAX)` alih-alih
`ScheduleProposal::new` (2000). Tanpa ini, hasil edit/create resep panjang bisa
membuat tick menonaktifkan jadwal secara keliru.

### 3. Tool Galileo

Tool baru hanya didaftarkan di `workspace_tools()`. Tidak ada yang masuk
`SPEC_TOOLS` atau `read_tools()`, sehingga run terjadwal tidak bisa mengelola
jadwal.

| Tool              | Jenis    | Argumen                                             | Output                                                                               |
| ----------------- | -------- | --------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `list_schedules`  | read     | `query?`, `enabled?`, `limit?` (default 20, max 50) | id, name, enabled, cadence, timezone, next_run_at, ringkasan description, `can_edit` |
| `get_schedule`    | read     | `schedule_id` (UUID)                                | resep/spec/prompt lengkap, cadence, enabled, next_run_at, `can_edit`                 |
| `update_schedule` | proposal | `schedule_id` + hanya field yang berubah            | dicatat ke `ToolTrace`; tidak menulis DB                                             |

`can_edit` dihitung dari `created_by_id == user_id` atau role workspace admin
(20), mengikuti aturan `can_manage`. Tool list mendukung filter nama
case-insensitive dan urutan `created_at DESC`.

`update_schedule` memvalidasi bentuk argumen tanpa DB:

- `schedule_id` harus UUID.
- Minimal satu field perubahan.
- Field teks/resep divalidasi batasnya; `prompt` dan field resep tidak boleh
  dicampur dalam satu proposal.
- `frequency`, `time`, `day_of_week`, `day_of_month`, dan `timezone` divalidasi
  enum/format/rentangnya.
- Cross-field merge (weekly butuh day, dll.) tetap divalidasi authoritative di
  PATCH.

`UpdateScheduleArgs` memakai field datar (`schedule_id` plus field opsional),
tetapi proposal yang dicatat ke trace dinormalisasi menjadi `schedule_id` +
`changes`:

```rust
pub struct UpdateScheduleArgs {
    pub schedule_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub how_to: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub expected_output: Option<String>,
    pub prompt: Option<String>,
    pub frequency: Option<String>,
    pub time: Option<String>,
    pub day_of_week: Option<i16>,
    pub day_of_month: Option<i16>,
    pub timezone: Option<String>,
}
```

Proposal memperkenalkan `UPDATE_SCHEDULE_NAME` dan menambahkannya ke
`PROPOSAL_TOOL_NAMES`. `pending_action()` mengembalikan aksi terakhir dari
`create_schedule` atau `update_schedule`:

```json
{
  "kind": "update_schedule",
  "proposal": { "schedule_id": "...", "changes": { "frequency": "weekly", "day_of_week": 1 } }
}
```

API handler `/ai-agent/` memperkaya aksi update dengan snapshot sebelum edit
dan izin:

```json
"schedule_update_proposal": {
  "schedule_id": "...",
  "changes": { "frequency": "weekly", "day_of_week": 1 },
  "before": { "id": "...", "name": "...", "frequency": "daily", "spec": {}, "prompt": "...", "enabled": true },
  "can_edit": true
},
"schedule_update_decision": "pending"
```

- `before` berisi `schedule_json` jadwal target; `null` bila jadwal sudah tidak
  ada.
- `can_edit` dihitung di handler memakai role yang sudah dimuat.
- Metadata ini yang dipetakan FE; `pending_action` di response tetap bentuk
  ringkas di atas.

Penyimpanan keputusan menambah key allowlist
`schedule_update_decision: "applied" | "cancelled"` di
`clean_metadata_patch` (`routes/ai_conversations.rs`).

`PREAMBLE` di `crates/ai/src/agent.rs` ditambah instruksi:

- Untuk mengubah jadwal, panggil `list_schedules` (opsional dengan query) lalu
  `get_schedule`; bila ada lebih dari satu kandidat, tanya user dulu.
- Panggil `update_schedule` satu kali dengan `schedule_id` dan hanya field yang
  berubah.
- Jangan pernah mengklaim jadwal sudah diubah sebelum user mengonfirmasi kartu.
- Pause/resume/delete/run-now bukan tool; arahkan user ke halaman Scheduler.

### 4. Frontend

#### Modal edit Scheduler

`ScheduleDetailModal` menambah tombol **Edit** untuk `canManage`. Tombol membuka
`ScheduleEditModal` yang di-prefill dari jadwal:

- Name, frequency, time, weekday/day-of-month, timezone IANA.
- Description, ordered steps, tool checkboxes, expected output.
- Legacy (`spec` NULL): textarea prompt mentah + name/cadence, tanpa field
  resep.
- `enabled` tidak diduplikasi di form edit; pause/resume tetap lewat tombol
  yang ada.

Validasi client memakai `validateScheduleSpec()` yang sudah ada, ditambah
validator name/cadence/timezone baru. Error server ditampilkan inline. Save
memanggil store, menutup modal edit, dan detail modal otomatis ikut berubah
karena `SchedulerView` menurunkan jadwal aktif dari list store.

Field resep (description, steps, tools, expected output) diekstrak menjadi
`ScheduleRecipeFields` dan dipakai bersama oleh kartu create dan modal edit
untuk menghindari duplikasi logika langkah/tool. Field cadence hanya dipakai
modal edit.

#### Store dan service

- `AiSchedulesService.update(slug, id, changes: TAiScheduleUpdate)` menggantikan
  signature boolean-only. `toggleSchedule` mengirim `{ enabled }`.
- `TAiScheduleUpdate` adalah partial bertipe dari name, field resep, `prompt`,
  cadence, timezone, dan `enabled`. `id` dan `next_run_at` tidak diterima.
- `AiSchedulesStore.updateSchedule()` memanggil PATCH, merge respons ke item
  list sambil mempertahankan `last_status`, `last_finished_at`, `last_run_at`,
  dan `runs`, lalu mengembalikan jadwal hasil update.

#### Kartu proposal update Galileo

Komponen baru `ScheduleUpdateProposalCard`:

- Menampilkan nama jadwal dan diff **Before → After** untuk name, cadence, dan
  timezone.
- Untuk description, steps, tools, expected output, atau prompt legacy:
  tampilkan blok before/after.
- **Apply** memanggil `schedulesService.update` dengan `changes`; **Cancel**
  hanya mencatat keputusan. Kartu bersifat read-only agar diff tidak ambigu.
- Apply dinonaktifkan dengan penjelasan bila `before` null (jadwal hilang) atau
  `can_edit` false.
- Sukses menampilkan “Schedule updated” + link Open Scheduler; keputusan
  dipersist lewat `schedule_update_decision`.
- Jika target hilang antara proposal dan Apply, error `404` ditampilkan di
  kartu dan keputusan tetap `pending`.

Tipe pesan diperluas:

- `TAiMessage` / `TAiMessageMetadata`: `scheduleUpdateProposal`,
  `scheduleUpdateDecision` (`"pending" | "applied" | "cancelled"`).
- `TAiMessageMetadataPatch`: menerima `schedule_update_decision`
  (`"applied" | "cancelled"`).
- `TAiAgentPendingAction`: union create dan update.
- `toAiMessage` memetakan metadata baru.
- Assistant store menambah `confirmScheduleUpdate` dan
  `resolveScheduleUpdateProposal`.

Helper diff murni (`diffScheduleUpdate`) ditempatkan di
`core/lib/ai-schedule.ts` agar bisa diuji tanpa React.

### 5. Dokumentasi

- `apps/web/core/components/docs/content/scheduler.tsx`: jelaskan tombol Edit
  dan field yang dapat diubah.
- `apps/web/core/components/docs/content/galileo.tsx`: contoh permintaan ubah
  jadwal dan alur konfirmasi.

## Testing

Rust:

- `crates/ai/src/schedule.rs`: merge/validasi update, transisi frequency,
  normalisasi day, bounds, render prompt ulang.
- `crates/ai/src/tools/mod.rs`: schema dan validasi `list_schedules`,
  `get_schedule`, `update_schedule`; registry memuat tool baru dan
  `read_tools` tidak.
- `crates/ai/src/agent.rs`: `pending_action` mengembalikan update; create tetap
  bekerja.
- `crates/api/tests/ai_schedule_test.rs`: PATCH partial/full, edit aktif vs
  paused (next_run_at), legacy prompt, payload invalid/unknown/empty,
  otorisasi, history tetap, `enabled` + field lain sekaligus.
- `crates/api/tests/ai_agent_test.rs`: metadata update dengan `before` dan
  `can_edit`, allowlist keputusan, bentuk pending action.
- `crates/worker/tests/ai_schedule_test.rs`: resep panjang tetap enabled setelah
  tick.

Frontend (vitest):

- `core/lib/ai-schedule.test.ts`: validasi update dan `diffScheduleUpdate`.
- `core/store/ai-schedules.store.test.ts`: update, merge respons, preservasi
  field run.
- `core/store/ai-assistant.store.test.ts`: apply/cancel, persist keputusan,
  metadata update.
- `core/lib/ai-conversations.test.ts`: pemetaan metadata baru.

Verifikasi manual: edit dari modal dan dari Galileo, cek list/detail berubah,
`next_run_at` benar, riwayat run tetap, dan user non-creator/non-admin ditolak.

## Non-goals

- Tool Galileo untuk pause/resume/delete/run-now.
- Cron/free-form recurrence, one-off schedule, atau trigger event.
- Perubahan sistem notifikasi atau penerima notifikasi.
- Audit/versioning edit, optimistic concurrency, dan rollback.
- Konversi otomatis jadwal legacy menjadi resep terstruktur.
- Menampilkan `tool_calls` run di UI.
- Membatalkan run queued/running saat jadwal diedit.

## Acceptance criteria

1. Creator/admin dapat mengedit name, resep, prompt legacy, cadence, dan
   timezone dari modal Scheduler; pause/resume tetap lewat tombol yang ada.
2. Semua field resep tervalidasi dan `prompt` dirender ulang di server; tidak
   ada klien yang bisa menyimpan prompt terstruktur yang tidak sinkron.
3. Galileo dapat menemukan jadwal, mengusulkan perubahan, menampilkan
   before/after, dan menerapkan perubahan hanya setelah konfirmasi.
4. `next_run_at` dihitung ulang hanya saat cadence/timezone berubah atau resume;
   edit konten tidak menggeser jadwal.
5. Riwayat run tetap ada dan run in-flight memakai snapshot lama.
6. Jadwal legacy tetap bisa diedit untuk prompt/name/cadence tanpa migrasi.
7. User non-creator/non-admin tidak bisa mengubah jadwal lewat UI maupun
   konfirmasi proposal.
8. Resep terstruktur panjang tidak lagi dinonaktifkan oleh tick.
9. `pnpm check:types`, test Rust yang disentuh, dan test web yang disentuh lulus.

## File map

Backend:

- `apps/api-rs/crates/ai/src/schedule.rs`
- `apps/api-rs/crates/ai/src/tools/proposals.rs`
- `apps/api-rs/crates/ai/src/tools/mod.rs`
- `apps/api-rs/crates/ai/src/agent.rs`
- `apps/api-rs/crates/api/src/routes/ai_schedule.rs`
- `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- `apps/api-rs/crates/api/src/routes/ai_conversations.rs`
- `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`

Frontend:

- `apps/web/core/components/ai-scheduler/schedule-detail-modal.tsx`
- `apps/web/core/components/ai-scheduler/schedule-edit-modal.tsx` (baru)
- `apps/web/core/components/ai/assistant-sidebar/schedule-update-proposal-card.tsx` (baru)
- `apps/web/core/components/ai/assistant-sidebar/root.tsx`
- `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx` (ekstraksi field)
- `apps/web/core/components/ai/assistant-sidebar/schedule-recipe-fields.tsx` (baru)
- `apps/web/core/store/ai-schedules.store.ts`
- `apps/web/core/store/ai-assistant.store.ts`
- `apps/web/core/services/ai-schedules.service.ts`
- `apps/web/core/lib/ai-schedule.ts`
- `apps/web/core/lib/ai-context.ts`
- `apps/web/core/lib/ai-conversations.ts`
- `apps/web/core/components/docs/content/scheduler.tsx`
- `apps/web/core/components/docs/content/galileo.tsx`

## Risiko

- Metadata `before` dapat membesar karena memuat spec/prompt; batas resep 8000
  karakter masih wajar untuk satu pesan.
- `update_schedule` hanya memvalidasi bentuk, sehingga kesalahan merge cadence
  baru terlihat saat PATCH; kartu tetap menampilkan error server.
- Last-write-wins dapat menimpa edit bersamaan; diterima untuk slice ini.
- Ekstraksi field form dari kartu create berisiko regresi UI; dijaga dengan test
  store/lib yang ada dan verifikasi manual kartu create.
