# AI Scheduler (`/schedule` dari Chat Agent) — Design

Tanggal: 2026-09-24
Status: disetujui user (3/3 bagian), menunggu review spec tertulis.

## Latar

Fork ini sudah punya agen Rig di `POST /api/workspaces/:slug/ai-agent/` (tools
read-only: `list_projects`, `count_work_items`, `search_work_items`) dan sidebar
chat Galileo dual-mode (Klasik ↔ Agent). Belum ada kemampuan menjalankan agen
secara berkala.

Infrastruktur background yang ada: crate `beat` (tokio-cron-scheduler, 11 job
hardcoded) mem-push job ke Redis Stream `plane:jobs`; crate `worker` mengonsumsi
stream tersebut. **Temuan penting:** `worker/src/handlers/mod.rs::handle_by_id`
saat ini stub — hanya log lalu ACK, tidak pernah membaca payload atau memanggil
handler. Jadi belum ada satu pun job beat yang benar-benar dieksekusi.

Tujuan fitur: user membuat jadwal rutin untuk agen lewat chat (`/schedule`),
mengelolanya di halaman Scheduler, dan melihat riwayat hasil run.

## Keputusan yang dikunci saat brainstorming

1. Bentuk fitur = **halaman UI "Scheduler"** berisi daftar jadwal + riwayat run
   (bukan penjadwalan Pages/wiki; Pages tidak disentuh).
2. `/schedule` diketik user di chat dan **dikirim ke agen**; agen memanggil tool
   baru `create_schedule`. "Skill" di sini = kemampuan/tool agen, bukan registry
   skill terpisah. Tanpa picker skill di FE.
3. Hasil run v1 = **riwayat run saja**; tanpa notifikasi in-app maupun email.
4. **Konfirmasi dulu:** tool hanya menghasilkan _proposal_ (tidak menulis DB);
   chat menampilkan kartu ringkasan + tombol Confirm/Batal; baris jadwal dibuat
   setelah Confirm.
5. Frekuensi = **preset terstruktur** (`hourly|daily|weekly|monthly` + jam HH:MM
   - timezone IANA), bukan cron bebas.
6. Visibilitas: **semua member workspace boleh melihat**; pause/resume, hapus,
   dan run now hanya untuk **pembuat atau admin workspace**. Pembuatan jadwal
   baru lewat chat boleh oleh member mana pun (gate endpoint agen ADMIN/MEMBER
   yang sudah ada).
7. Lokasi halaman: **sidebar workspace level teratas**, sejajar Projects /
   Your work.
8. Aksi halaman v1: **Pause/Resume + Hapus + Run now**; tanpa edit
   prompt/frekuensi dari halaman (buat ulang lewat chat).
9. Eksekusi: **beat tick tiap menit + klaim atomik di worker** (pendekatan 1).
10. Worker dispatch: **allowlist** — `handle_by_id` diperbaiki untuk membaca
    payload stream, tetapi hanya job fitur ini (`ai.schedule.tick`,
    `ai.schedule.run`) yang dieksekusi; job beat lain tetap log+skip sampai
    divalidasi terpisah.
11. Bila `/schedule` dikirim saat mode Klasik: FE **otomatis pindah ke Agent**
    (chat baru, sesuai perilaku toggle) lalu mengirim pesan; draft textarea tidak
    hilang karena input adalah state lokal komponen.

## Desain

### 1. Refactor prasyarat — crate `crates/ai`

Runtime agen diangkat dari `crates/api/src/routes/ai_agent/` ke crate library
baru `crates/ai` supaya bisa dipakai `api` dan `worker`:

- Pindah: preamble, `run_agent`, `workspace_tools` + ketiga tool, `ToolTrace`,
  serta `LlmConfig`/`LlmError`/`resolve_llm_config`/`host_of` dari `routes/ai.rs`.
- `crates/api` memakai crate ini lewat pembungkus tipis; perilaku
  `/ai-agent/` dan `/ai-assistant/` **tidak berubah** (test lama tetap lolos).
- Tool baru `create_schedule` (proposal-only) hidup di crate yang sama.

### 2. Model data — migrasi `apps/api-rs/migrations/0006_ai_schedules.sql`

Mengikuti konvensi `0003_services.sql`: tabel plural, UUID di-generate aplikasi,
`timestamptz`, soft delete `deleted_at`, index parsial.

`ai_schedules`:

- `id uuid PK`, `workspace_id` → `workspaces(id)` ON DELETE CASCADE,
  `created_by_id` → `users(id)` ON DELETE CASCADE
- `name varchar(120)`, `prompt text` (≤2000 divalidasi)
- `frequency varchar(10)` ∈ `hourly|daily|weekly|monthly`
- `time_of_day varchar(5)` "HH:MM" (untuk hourly hanya bagian menit dipakai)
- `day_of_week smallint NULL` (1=Senin…7=Minggu, wajib untuk weekly)
- `day_of_month smallint NULL` (1–31, wajib untuk monthly)
- `timezone varchar(64)` IANA (default dari timezone browser user)
- `enabled boolean NOT NULL DEFAULT true`
- `next_run_at timestamptz NOT NULL`
- `proposal_key uuid NOT NULL` dengan unique index parsial
  `(workspace_id, proposal_key) WHERE deleted_at IS NULL` (idempotensi
  konfirmasi per workspace; key tidak terpakai selamanya setelah soft delete).
- CHECK konsistensi preset: `weekly` wajib `day_of_week`, `monthly` wajib
  `day_of_month`, dan `time_of_day` harus `HH:MM` valid — supaya baris yang
  tidak bisa dijalankan tidak mungkin ada (tick tidak pernah macet).
- `created_at/updated_at/deleted_at timestamptz`
- Index parsial `(next_run_at) WHERE enabled AND deleted_at IS NULL`

`ai_schedule_runs`:

- `id uuid PK`, `schedule_id` → `ai_schedules(id)` ON DELETE CASCADE,
  `workspace_id uuid`
- `status varchar(10)` ∈ `queued|running|success|failed`
- `trigger varchar(10)` ∈ `scheduled|manual`
- `prompt text` (snapshot saat run), `response text`, `response_html text`,
  `error text`, `tool_calls jsonb`
- `created_at`, `started_at`, `finished_at`
- Index `(schedule_id, created_at DESC)`; retensi 20 run terakhir per jadwal.

Perhitungan `next_run_at` (helper murni, mudah diuji):

- hourly → jam berikutnya pada menit `MM`
- daily → hari berikutnya pada `HH:MM`
- weekly → hari `day_of_week` berikutnya pada `HH:MM`
- monthly → tanggal `day_of_month` berikutnya pada `HH:MM`; bulan pendek
  di-clamp ke hari terakhir bulan itu
- Dihitung dalam timezone jadwal lalu disimpan UTC; saat klaim dihitung dari
  `now` (tidak ada backlog menumpuk bila beat sempat mati).

### 3. Eksekusi — beat tick + klaim worker

1. `crates/beat/src/main.rs` +1 job tetap `ai.schedule.tick` tiap menit →
   `common::stream::push_job`.
2. `handle_by_id` (worker) diperbaiki: baca entry stream berdasarkan id,
   parse `job` + `payload`; dispatch hanya `ai.schedule.tick` dan
   `ai.schedule.run`; job lain `tracing::warn` + skip (perilaku stub lama).
3. Handler `tick`:
   - Klaim atomik:
     `UPDATE ai_schedules SET next_run_at = <next_occurrence> WHERE id IN (SELECT id FROM ai_schedules WHERE enabled AND deleted_at IS NULL AND next_run_at <= now() FOR UPDATE SKIP LOCKED) RETURNING id`
   - Untuk tiap id yang terklaim: insert run `queued` (`trigger='scheduled'`)
     lalu push `ai.schedule.run {run_id}`.
   - Sweep: run `queued`/`running` yang lebih tua dari 15 menit → `failed`
     (pesan "run stuck/dihentikan").
4. Handler `run`: set `running` + `started_at`; load schedule; resolve LLM
   config (DB `instance_configurations` + env); jalankan agen read-only
   (tools workspace-scoped, tanpa konteks user); tulis
   `success`/`failed` + `response`/`response_html`/`error`/`tool_calls`/
   `finished_at`. Prune run >20.
5. Konsekuensi diterima v1: konsumer worker seri — satu run agen (maks 180 dtk)
   menahan job lain sesaat; preset minimum per jam sehingga antrean aman;
   "Run now" memakai stream yang sama (asinkron).

### 4. Tool `create_schedule`, `pending_action`, dan kartu konfirmasi

- Tool `create_schedule` (proposal-only, tanpa tulisan DB) dengan argumen:
  `name`, `prompt`, `frequency`, `time` ("HH:MM"), `day_of_week` (opsional,
  1–7), `day_of_month` (opsional, 1–31), `timezone` (IANA). Mengembalikan
  proposal terstruktur; trace tetap merekam pemanggilan.
- Preamble ditambah aturan: pesan berawalan `/schedule` = permintaan membuat
  jadwal; kumpulkan info yang kurang dengan bertanya; panggil tool setelah
  jelas; jangan mengarang.
- FE menambahkan baris `User timezone: <IANA>` pada konteks prompt
  (`buildAiPrompt`) **hanya di mode Agent** — mode Klasik tidak berubah. Default
  timezone lain bila user tidak menyebut: `time_of_day` = "09:00" untuk
  daily/weekly/monthly, dan untuk hourly bagian menit = "00" (jam diabaikan).
- Respons `/ai-agent/` diperluas dengan
  `pending_action: {kind:"create_schedule", proposal:{…}} | null`, dibangun
  dari pemanggilan tool terakhir di trace. `/ai-assistant/` tidak berubah.
- Kartu konfirmasi dirender dari metadata pesan (`pendingAction` + status
  keputusan) sehingga bertahan setelah reload. Aksi:
  - Confirm → `POST .../ai-schedules/` dengan proposal + `proposal_key` (UUID
    dibuat FE) → kartu "Jadwal dibuat" + tautan ke halaman Scheduler.
  - Batal → kartu "Dibatalkan", tanpa panggilan API.
  - Gagal → pesan error + tombol Retry.
  - Field proposal dirender sebagai teks React (bukan HTML mentah).

### 5. Endpoint API (di bawah `/api/workspaces/:slug/`)

- `GET ai-schedules/` — daftar + ringkasan run terakhir (semua member)
- `POST ai-schedules/` — konfirmasi pembuatan (idempoten via `proposal_key`)
- `GET ai-schedules/:id/` — detail + 20 run terakhir
- `PATCH ai-schedules/:id/` — `{enabled: bool}`; resume menghitung ulang
  `next_run_at`
- `DELETE ai-schedules/:id/` — soft delete
- `POST ai-schedules/:id/run/` — insert run `manual` + push job (asinkron)

Validasi create (server, tidak mempercayai proposal klien): gate ADMIN/MEMBER;
`name` 1–120; `prompt` 1–2000; enum frekuensi; format jam; `day_of_week`/
`day_of_month` konsisten dengan frekuensi; timezone valid (chrono-tz); maksimum
20 jadwal non-deleted per workspace; `proposal_key` sudah ada → kembalikan
jadwal lama (idempoten).

Otorisasi: lihat = semua member; membuat jadwal baru lewat `POST` = member mana
pun (tercatat sebagai pembuat); mengubah/menghapus/menjalankan jadwal yang
**sudah ada** = pembuat atau admin workspace (helper `ws_role` yang ada).

### 6. FE

- **Composer** (`assistant-sidebar/root.tsx`): deteksi `/` di awal pesan →
  hint `/schedule — Buat jadwal AI`; pesan `/schedule …` dikirim ke mode Agent;
  dari mode Klasik otomatis pindah mode dulu (keputusan #11).
- **Kartu konfirmasi** (`schedule-proposal-card.tsx`): nama, jadwal
  ter-humanize ("Setiap Senin 09:00 · Asia/Jakarta"), prompt, tombol
  Confirm/Batal, state `pending → submitting → created/error/cancelled`.
- **Halaman Scheduler**: route workspace-level (mis. `/[workspaceSlug]/scheduler`,
  grup route mengikuti pola yang ada) + entry baru di sidebar workspace.
  - List: nama, cuplikan prompt, frekuensi ter-humanize, next run (waktu lokal),
    status run terakhir, pembuat; aksi Pause/Resume, Run now, Delete (dialog
    konfirmasi) — tombol hanya tampil untuk pembuat/admin (backend tetap
    menegakkan).
  - Detail: 20 riwayat run (status, trigger, waktu, durasi, jawaban tersanitasi
    via `sanitizeAssistantHtml`, error bila gagal).
  - Refresh saat mount + setelah aksi; polling 30 dtk saat halaman aktif.
  - Empty state: "Buat jadwal dari chat: ketik `/schedule …`".
  - Service + MobX store mengikuti pola modul Pages.

### 7. Error handling & guardrail

- Maksimum 20 jadwal non-deleted/workspace; interval minimum per jam (preset).
- Run timeout 180 dtk (timeout agen yang ada); sweep stuck >15 menit → failed.
- Tidak ada double-run satu jadwal: klaim memajukan `next_run_at` secara atomik.
  Run yang masih jalan saat jadwal berikutnya jatuh tempo akan ter-antre dan
  berjalan setelahnya (diterima v1).
- LLM config invalid saat run → run `failed` dengan pesan jelas, bukan crash.
- Retensi 20 run/jadwal.
- Kegagalan FE confirm: kartu error + Retry dengan `proposal_key` yang sama
  (tidak menggandakan jadwal).

## Testing dan verifikasi

- Rust unit: `next_occurrence` (semua preset, timezone, clamp bulan pendek),
  validasi proposal, parsing argumen tool, ekstraksi `pending_action` dari
  trace.
- Rust integration: endpoint CRUD + authz (pembuat/admin/member lain), create
  idempoten `proposal_key`, run-now, klaim tick (tidak dobel), agen dengan
  upstream fake → respons memuat `pending_action`.
- FE vitest: deteksi `/schedule` + auto-switch mode, state machine kartu
  (confirm/batal/error/retry, anti double-click), humanize jadwal, service/store.
- Verifikasi repo: `cargo test -p api -- --test-threads=1`, `cargo clippy`,
  `pnpm check` untuk paket yang tersentuh, rebuild image API + web sesuai
  AGENTS.md, lalu smoke manual di tunnel: buat via chat → Run now → riwayat
  muncul → pause/resume → delete; plus cek `/ai-assistant/` & `/ai-agent/`
  tidak berubah bagi klien lama.

## Out of scope (eksplisit bukan bagian desain ini)

- Notifikasi in-app/email untuk hasil run.
- Edit jadwal (prompt/frekuensi) dari halaman Scheduler.
- Cron expression bebas / interval sub-jam.
- Output jadwal ke Page, work item, komentar, atau webhook.
- Retry otomatis saat run gagal; paralelisasi run di worker.
- Mengaktifkan job beat lain (cleanup, archive, dll) — follow-up terpisah.
- Skill `/…` lain di composer selain `/schedule`.

## Risiko dan mitigasi

- **Salah inferensi frekuensi/timezone oleh LLM** → dikonfirmasi user lewat
  kartu sebelum tersimpan; validasi server ketat; timezone default dari browser.
- **Worker seri tertahan run panjang** → diterima v1 (preset minimum per jam);
  bila perlu, worker bisa dipecah per-consumer group di iterasi berikutnya.
- **Refactor `crates/ai` menyentuh jalur produksi** → move-only; test lama harus
  tetap lolos tanpa perubahan perilaku; diff besar tapi mekanis.
- **Proposal hilang bila localStorage dibersihkan** → jadwal tidak jadi dibuat;
  user tinggal minta ulang ke agen (tidak ada state server yang menggantung).
- **Klaim ganda saat beberapa replika worker** → `FOR UPDATE SKIP LOCKED` +
  memajukan `next_run_at` dalam satu UPDATE.
- **Prompt terjadwal dijalankan tanpa pengawasan** → tools tetap read-only dan
  workspace-scoped; tidak ada kemampuan tulis sampai ada tool tulis lain.
