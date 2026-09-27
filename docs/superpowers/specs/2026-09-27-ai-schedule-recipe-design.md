# AI Schedule Recipe (Kerangka Wajib Jadwal) — Design

Tanggal: 2026-09-27
Status: disetujui user (4/4 bagian), menunggu review spec tertulis.

## Latar

Fitur AI Scheduler yang ada (`2026-09-24-ai-scheduler-design.md`) menyimpan
jadwal sebagai `name` + `prompt` bebas + preset waktu. Saat run, prompt mentah
langsung dikirim ke agen Rig dengan **semua** tool read-only ter-expose
(`workspace_tools`), termasuk `create_schedule` yang tidak relevan untuk run
terjadwal.

Akibatnya:

- Instruksi jadwal bisa ambigu — kualitas hasil bergantung pada satu kalimat
  prompt yang ditulis saat chat.
- Tidak ada kontrak eksplisit tentang langkah kerja dan tool yang dipakai, jadi
  tiap run bisa berbeda-beda dan sulit diaudit.
- Tool yang boleh dipakai tidak terbatas (least privilege tidak ada), padahal
  run berjalan tanpa pengawasan.

Tujuan slice ini: jadwal bukan lagi sekadar "perintah", tapi **resep
terstruktur** dengan kerangka wajib — `description`, `how_to` (langkah
berurutan), `tools` (deklarasi tool), dan `expected_output` — yang diisi agen,
bisa diedit user di kartu konfirmasi, divalidasi server, dan ditegakkan saat run
(hard allowlist).

## Keputusan yang dikunci saat brainstorming

1. Masalah yang diselesaikan: **konsistensi eksekusi, kontrol & audit, kualitas
   prompt** (bukan semata least privilege).
2. Bentuk kerangka = **structured + tervalidasi**: field terpisah, divalidasi
   backend, bisa diedit per-field di UI — bukan template teks baku.
3. Field wajib: `description`, `how_to`, `tools`, `expected_output`
   (tanpa `constraints`/`scope` di v1).
4. `tools` = **hard allowlist**: runtime hanya mengekspos tool yang
   dideklarasikan; legacy fallback ke semua tool read-only.
5. Pengisian: **agen mengisi semua field; user bisa mengedit di kartu
   konfirmasi** sebelum simpan. Tidak ada edit schedule tersimpan di v1
   (tetap hapus + buat ulang via chat, konsisten dengan keputusan #8 desain
   scheduler).
6. **Legacy aman**: schedule lama tanpa `how_to` tetap jalan apa adanya —
   prompt mentah + tiga tool read-only (tanpa `create_schedule`, sama seperti
   semua run terjadwal), tanpa backfill.
7. Penyimpanan: **satu kolom `spec jsonb`** (versi di dalam spec), validasi di
   Rust, `prompt` tetap diisi hasil render kanonik saat create (kompatibilitas
   UI/worker/history + audit).
8. Runtime `create_schedule` tidak lagi ikut ter-expose di run terjadwal.

## Desain

### 1. Model data & validasi (`crates/ai/src/schedule.rs`)

```rust
pub const SPEC_VERSION: u8 = 1;
pub const SPEC_TOOLS: [&str; 3] = ["list_projects", "count_work_items", "search_work_items"];

pub struct ScheduleSpec {
    pub version: u8,              // harus SPEC_VERSION
    pub description: String,      // 1-500 char
    pub how_to: Vec<String>,      // 1-10 langkah, tiap langkah 1-500 char
    pub tools: Vec<String>,       // subset SPEC_TOOLS, min 1, dedup + urutan kanonik
    pub expected_output: String,  // 1-1000 char
}
```

Aturan validasi `ScheduleSpec::new` (pola sama dengan `ScheduleProposal::new`):

- Semua string di-trim; kosong = error dengan pesan per-field.
- `how_to`: minimal 1, maksimal 10 langkah; tiap langkah ≤500 char; langkah
  kosong ditolak.
- `tools`: minimal 1; nilai di luar `SPEC_TOOLS` ditolak (bukan diabaikan
  diam-diam); didedup lalu diurutkan ke urutan kanonik `SPEC_TOOLS` agar prompt
  deterministik. `create_schedule` tidak pernah menjadi pilihan.
- `version` selain `SPEC_VERSION` ditolak (evolusi = naikkan versi, bukan
  longgarkan validasi).

`ScheduleRecipe` = `ScheduleSpec` + `ScheduleProposal` (name + preset), dengan
`#[serde(flatten)]` sehingga JSON-nya datar dan bentuknya tetap kompatibel
dengan `TAiScheduleProposal` FE (hanya bertambah 4 field).

**Render prompt kanonik** — pure function
`render_schedule_prompt(name, spec) -> String`:

```
Task: <name>

Description:
<description>

Steps:
1. <langkah 1>
2. <langkah 2>

Expected output:
<expected_output>

Allowed tools: <tool a>, <tool b>
```

Prompt hasil render disimpan ke kolom `prompt` saat create. Ini menjaga worker,
UI, dan history yang ada tetap bekerja, sekaligus merekam prompt yang benar-benar
dipakai: `ai_schedule_runs.prompt` tetap snapshot per run (audit).

### 2. Migrasi `apps/api-rs/migrations/0009_ai_schedule_spec.sql`

- `ALTER TABLE public.ai_schedules ADD COLUMN IF NOT EXISTS spec jsonb NULL;`
- `CHECK (spec IS NULL OR jsonb_typeof(spec) = 'object')` (idempoten seperti pola
  `0007`).
- Tanpa backfill: `spec IS NULL` berarti legacy.

### 3. Tool `create_schedule` (`crates/ai/src/tools.rs`)

Argumen berubah — `prompt` dihapus, diganti field kerangka:

```rust
pub struct CreateScheduleArgs {
    pub name: String,
    pub description: String,
    pub how_to: Vec<String>,
    pub tools: Vec<String>,
    pub expected_output: String,
    pub frequency: String,
    pub time: Option<String>,
    pub day_of_week: Option<i16>,
    pub day_of_month: Option<i16>,
    pub timezone: Option<String>,
}
```

- Tool memvalidasi spec + preset lewat `ScheduleRecipe::new`, mengembalikan JSON
  recipe datar, dan mencatatnya ke trace (proposal-only, tanpa tulisan DB —
  perilaku lama dipertahankan).
- Deskripsi tool menjelaskan kerangka wajib dan kapan dipanggil (hanya untuk
  permintaan recurring; kumpulkan yang kurang dulu).

### 4. Hard allowlist runtime

- `workspace_tools(pool, workspace_id, trace)` tetap 4 tool — dipakai **chat
  agent** yang memang butuh `create_schedule`.
- Tambah `read_tools(pool, workspace_id, trace, allowed: &[&str])`: hanya
  membangun tool dari subset yang diizinkan.
- Worker `ai_schedule::run`:
  1. Setelah klaim run, ambil `spec` dari `ai_schedules` berdasarkan
     `schedule_id`.
  2. `spec = Some` → parse + validasi `ScheduleSpec`:
     - valid → tool server = `read_tools(..., &spec.tools)`.
     - tidak valid → run `failed` dengan pesan `"invalid schedule spec"`
       (data rusak harus terlihat; bukan diam-diam longgar).
  3. `spec = NULL` (legacy) → `read_tools(..., &SPEC_TOOLS)` (ketiga read tool),
     prompt mentah apa adanya.
- Efek yang diinginkan: run terjadwal tidak lagi bisa memanggil
  `create_schedule`.

### 5. Preamble chat agent (`crates/ai/src/agent.rs`)

Ditambah (bukan diganti):

- Pesan berawalan `/schedule` = permintaan membuat jadwal: kumpulkan dulu
  kerangka lengkap — deskripsi singkat, langkah how-to berurutan, tool yang
  dibutuhkan (minimal satu dari tiga read tool), dan output yang diharapkan —
  baru panggil `create_schedule` **sekali**.
- Sampaikan bahwa semua field bisa diedit user di kartu sebelum konfirmasi, dan
  jangan mengklaim jadwal sudah dibuat.

Tidak berubah: `run_agent`, `MAX_TURNS = 6`, `AGENT_TIMEOUT = 180s`, pemetaan
error LLM, preamble run terjadwal.

### 6. API (`crates/api/src/routes/ai_schedule.rs`)

- `CreateScheduleBody` diperluas dengan field spec (`description`, `how_to`,
  `tools`, `expected_output`) — semuanya opsional untuk kompatibilitas.
  - Semua field spec ada → validasi `ScheduleSpec` + `ScheduleProposal`, prompt
    di-render **server-side**; `prompt` kiriman klien diabaikan dan `version`
    ditetapkan server (`SPEC_VERSION`), bukan dari klien.
  - Tidak ada field spec tapi `prompt` ada → jalur legacy: `spec = NULL`,
    prompt disimpan apa adanya.
  - Campuran (sebagian ada, sebagian tidak) → 400
    `"incomplete schedule spec"`.
- `ScheduleRow` + `schedule_json` menyertakan `spec` (nullable) di list dan
  detail.
- `PATCH` (`{enabled}`) dan `POST /:id/run/` tidak berubah; run manual tetap
  memakai prompt snapshot schedule.

### 7. FE

- `lib/ai-schedule.ts`:
  - `TAiScheduleSpec = { version, description, how_to, tools, expected_output }`
  - `TAiScheduleProposal = TAiScheduleSpec & { name, frequency, time, day_of_week, day_of_month, timezone }`
  - `TAiSchedule` dapat `spec: TAiScheduleSpec | null`
  - Helper murni `validateScheduleSpec(spec)` yang mencerminkan aturan backend
    (dipakai untuk disable Confirm), plus `isScheduleCommand` /
    `humanizeSchedule` yang ada tidak berubah.
- `schedule-proposal-card.tsx` menjadi form ringkas:
  - Editable: `name`, `description`, `how_to` (daftar langkah, tambah/hapus,
    minimal 1), `tools` (checkbox tiga read tool, minimal 1), `expected_output`.
  - Preset frekuensi/jam/timezone tetap read-only (humanized).
  - Confirm nonaktif sampai valid; submit mengirim proposal hasil edit +
    `proposal_key` yang sama (idempoten; Retry aman).
  - State machine tetap `pending → submitting → created/error/cancelled`.
  - Pesan lama yang `pending_action`-nya tanpa field spec → kartu fallback
    read-only (jalur legacy backend).
- `schedule-item.tsx`:
  - Cuplikan `prompt` diganti `spec.description` (fallback `prompt` untuk
    legacy) + badge tool yang dideklarasikan.
  - Disclosure read-only **"Recipe"**: daftar langkah how-to + expected output.
- `schedule-runs-list.tsx`, service, dan store tidak berubah struktural.

### 8. Batas & error handling

- Jalur spec: `description ≤500`, `how_to 1–10 × ≤500`, `expected_output ≤1000`,
  dan **total prompt render ≤8000 char** — cap ini divalidasi di
  `ScheduleRecipe::new` setelah `render_schedule_prompt` (prompt gabungan bisa
  melebihi limit lama 2000). Jalur legacy tetap `prompt ≤2000`.
- Pesan 400 spesifik per-field (mis. `"tools must be a subset of: list_projects,
count_work_items, search_work_items"`).
- Spec rusak saat runtime → run `failed` `"invalid schedule spec"`; sweep stuck
  run (15 menit running / 6 jam queued) tetap berlaku.
- Cap 20 jadwal/workspace, idempotensi `proposal_key` per workspace, otorisasi
  creator/admin — tidak berubah.

## Testing dan verifikasi

- Rust unit (`crates/ai`): `ScheduleSpec::new` (bounds, trim, dedup + urutan
  kanonik, tool tak dikenal ditolak, versi); `render_schedule_prompt` golden
  string; bentuk JSON flat `ScheduleRecipe`; `CreateSchedule` menolak spec
  invalid tanpa trace; fungsi murni pemilih subset tool.
- Rust integration (`crates/api/tests`): create dengan spec → `spec` tersimpan +
  `prompt` hasil render; legacy prompt-only → `spec NULL`; body campuran → 400;
  list/detail memuat `spec`; create idempoten `proposal_key`.
- Rust worker: spec invalid → run `failed`; subset tool terbentuk dari spec
  (mengikuti harness test worker yang ada).
- FE vitest: `validateScheduleSpec`; kartu (edit → Confirm mengirim hasil edit,
  Confirm nonaktif saat invalid, fallback kartu legacy); item scheduler
  (description, badge tools, disclosure Recipe).
- Gate repo: `cargo test -p api -- --test-threads=1`, `cargo clippy`, `pnpm
check` + test paket web, lalu rebuild api/worker/beat-worker dan web sesuai
  AGENTS.md, smoke di tunnel: `/schedule` → edit kartu → Confirm → Run now →
  history; plus pastikan schedule legacy masih jalan.

## Rollout & rollback

- Migrasi `0009` diterapkan otomatis saat boot (`common::db::migrate`); kolom
  nullable tanpa backfill.
- Rebuild `api`, `worker`, `beat-worker` (worker memakai allowlist baru) dan
  rebuild web (`pnpm --filter=web build` + restart prod).
- Rollback: revert kode; kolom `spec` yang tertinggal diabaikan versi lama
  (nullable, tidak dibaca).

## Out of scope (eksplisit)

- Edit schedule tersimpan (tetap hapus + buat ulang via chat).
- Versioning/riwayat perubahan resep — `spec_version` disiapkan, belum dipakai.
- Field tambahan (`constraints`, `scope` per-project) — jalur `spec_version`
  berikutnya.
- Validasi otomatis output run terhadap `expected_output` (masih panduan prompt).
- Notifikasi in-app/email hasil run; retry otomatis.
- Tool tulis baru; perubahan mode Klasik / `/ai-assistant/`.
- Perubahan preset/frekuensi atau cron bebas.

## Risiko dan mitigasi

- **LLM gagal mengisi kerangka lengkap** → tool error terbaca model, agen
  bertanya ulang; user juga bisa melengkapi di kartu sebelum Confirm.
- **User mendeklarasikan tool terlalu sedikit** → run gagal terlihat di history;
  agen diarahkan mengusulkan tool yang relevan dan user bisa menambah di kartu.
- **Prompt lebih panjang → token naik** → bound per-field + cap total render
  8000 char; jumlah langkah dibatasi 10.
- **Kartu pending lama rusak setelah deploy** → jalur legacy body + fallback
  kartu read-only.
- **Spec jsonb drift** (skema berubah di masa depan) → `spec_version` + validasi
  runtime gagal dengan pesan jelas, bukan perilaku tak terduga.
- **Hard allowlist mematikan run yang butuh tool lain** → kegagalan terlihat di
  history + pesan run; user membuat ulang resep dengan tool yang benar.

## File map

- Create: `apps/api-rs/migrations/0009_ai_schedule_spec.sql`.
- Modify: `apps/api-rs/crates/ai/src/schedule.rs` — `ScheduleSpec`,
  `ScheduleRecipe`, `render_schedule_prompt`, validasi + unit test.
- Modify: `apps/api-rs/crates/ai/src/tools.rs` — `CreateScheduleArgs` baru,
  `read_tools`, unit test.
- Modify: `apps/api-rs/crates/ai/src/agent.rs` — preamble.
- Modify: `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs` — load spec,
  validasi, `read_tools`.
- Modify: `apps/api-rs/crates/api/src/routes/ai_schedule.rs` — body + validasi +
  `spec` di JSON.
- Modify: `apps/web/core/lib/ai-schedule.ts` — tipe + `validateScheduleSpec`.
- Modify: `apps/web/core/components/ai/assistant-sidebar/schedule-proposal-card.tsx`
  — form editable + fallback legacy.
- Modify: `apps/web/core/components/ai-scheduler/schedule-item.tsx` —
  description + badge tools + disclosure Recipe.
- Modify (test): `apps/web/core/lib/ai-schedule.test.ts`, test kartu/item yang
  relevan.
