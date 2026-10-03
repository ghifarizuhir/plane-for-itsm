# Review Briefing (AI) — Design

Date: 2026-10-03
Status: Draft (menunggu review user)
Scope: Migrasi api-rs, API (`apps/api-rs`), web (`apps/web`, `packages/types`, `packages/i18n`).
Terkait: `docs/superpowers/specs/2026-10-02-release-testing-control-boards-design.md`, `docs/superpowers/specs/2026-09-30-war-room-design.md`.

## Tujuan

Menambahkan **briefing pra-rapat** untuk sesi TCB/RCB: satu artefak bersama yang tersimpan di sesi, dibaca peserta sebelum sesi dimulai. AI merangkum tiap item agenda dan menandai poin diskusi/risiko, **tanpa menyarankan keputusan**.

Arah produk yang disepakati saat brainstorm:

- RCB/TCB fokus pada **pencatatan** keputusan (audit trail terstruktur), bukan automasi/gating.
- **AI assistant** adalah nilai tambah di atas catatan; titik pertama yang dipilih adalah briefing pra-rapat.
- Keputusan tetap sepenuhnya di manusia; AI tidak mengubah state apa pun.

## Konteks saat ini

- Engine review sudah ada: `review_requests`, `review_sessions`, `review_session_participants`, `review_session_items` (`apps/api-rs/migrations/0012_releases_review_control.sql`), route di `apps/api-rs/crates/api/src/routes/review.rs`, UI sesi di `apps/web/core/components/reviews/session/`.
- `ReviewSessionDetailRoot` menyusun halaman sesi: header → `SessionAgenda` → `SessionMinutes`, dengan right pane `SessionParticipants` (`apps/web/core/components/reviews/session/review-session-detail-root.tsx:46-65`).
- Infrastruktur AI sudah ada dan tidak perlu subsistem baru:
  - Resolusi config: `resolve_llm_config` (`apps/api-rs/crates/ai/src/llm.rs:75`) — `SKIP_ENV_VAR` default `true`, jadi API key/model dibaca dari `instance_configurations`, `LLM_BASE_URL` dari env.
  - Panggilan LLM single-shot: `chat_completion` (`apps/api-rs/crates/api/src/routes/ai.rs:71`), body `task + "\n" + prompt` (`build_body`, baris 40), timeout klien 60 detik.
  - Agent tool-calling (`apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`) **tidak dipakai** di v1; disimpan untuk pendekatan Q&A menyusul.
- Deployment stack ini sudah aktif untuk AI: `instance_configurations` berisi `LLM_API_KEY`, `LLM_MODEL`; container `api` menerima `LLM_BASE_URL` (`docker-compose-local.yml:78-80`).
- Pola test LLM palsu sudah ada: `TcpListener` lokal + `set_llm_env` (`apps/api-rs/crates/api/tests/ai_agent_test.rs:48,608`).
- Konteks insiden tersedia untuk change: `war_room_issues` → `war_rooms` (nama, severity, status).

## Keputusan (brainstormed & approved)

1. **Briefing = artefak sesi yang tersimpan**, bukan jawaban personal di sidebar. Semua peserta membaca versi yang sama, ikut terarsip bersama sesi.
2. **Digenerate manual** oleh pengelola sesi (chair/secretary/pembuat/admin) setelah agenda siap; hanya untuk sesi `scheduled`. Tidak ada generate otomatis saat sesi dibuat.
3. **Peran AI: ringkasan + poin diskusi/risiko** berdasarkan fakta yang diberikan. AI **dilarang menyarankan** approve/reject dan dilarang mengarang fakta di luar konteks.
4. **Fakta live, teks AI snapshot.** Hanya teks AI yang disimpan; status/keputusan/konteks dirender live dari data yang ada. Briefing adalah bahan persiapan, bukan alat bukti — bukti tetap keputusan terstruktur.
5. **Satu panggilan LLM per generate** untuk seluruh agenda (maks 20 item), output JSON tervalidasi. Non-JSON → retry sekali → fallback teks. Gagal total → tidak menyimpan apa pun.
6. **Tanpa history/versioning, tanpa edit manual, tanpa notifikasi baru** di v1.
7. Memakai konfigurasi dan klien LLM yang sudah ada; tidak menambah provider/endpoint AI baru.

## Non-goals (v1)

- Automasi/gating: keputusan mengubah workflow state, memblokir transisi/deploy.
- Edit manual briefing oleh manusia (briefing murni artefak AI; regenerate adalah satu-satunya cara mengubah).
- Versioning/history briefing (regenerate menimpa).
- Notifikasi "briefing siap" (dalam-app/email).
- Q&A record lewat agent/assistant sidebar (pendekatan C — menyusul, tanpa mengubah model ini).
- Snapshot fakta untuk reproduktibilitas audit.
- Redaksi konten tambahan — batas kepercayaan sama dengan AI assistant yang sudah berjalan.
- Briefing untuk board di luar TCB/RCB.

## Model data

### Kolom baru `review_sessions` (migrasi api-rs `0013_review_briefing.sql`)

| Field                      | Tipe         | Catatan                                               |
| -------------------------- | ------------ | ----------------------------------------------------- |
| `briefing`                 | jsonb NULL   | Kontrak v1 di bawah; `NULL` = belum pernah digenerate |
| `briefing_generated_at`    | timestamptz  | Waktu generate terakhir                               |
| `briefing_generated_by_id` | uuid         | FK `users` (SET NULL); aktor generate terakhir        |
| `briefing_model`           | varchar(100) | Model yang dipakai saat generate terakhir             |

Satu briefing aktif per sesi; regenerate menimpa seluruh field di atas. Tidak ada tabel versi (YAGNI — bisa dipecah tanpa mengubah kontrak API bila nanti dibutuhkan).

### Kontrak `briefing` (v1)

```json
{
  "version": 1,
  "generated_at": "2026-10-03T09:15:00Z",
  "generated_by_name": "Budi",
  "model": "gpt-4o-mini",
  "language": "id",
  "format": "json",
  "overall": "Ringkasan 2-4 kalimat untuk seluruh sesi.",
  "items": [
    {
      "session_item_id": "uuid",
      "summary": "2-3 kalimat: apa perubahannya dan kenapa diajukan.",
      "discussion_points": ["Poin yang perlu dibahas berdasarkan fakta."],
      "risks": ["Risiko yang terlihat dari fakta yang diberikan."]
    }
  ],
  "included_items": 12,
  "skipped_items": 0
}
```

- `format`: `"json"` (normal) atau `"text"` (fallback — `items` kosong, `overall` berisi teks mentah model).
- `generated_by_name` adalah snapshot nama saat generate, supaya UI tidak perlu join.
- Item yang tidak muncul/tidak valid di output model cukup tidak punya entri AI; fakta tetap tampil di UI.

## API

### `POST /api/workspaces/:slug/review-sessions/:session_id/briefing/`

Generate/regenerate briefing.

- **Akses**: `can_manage_session` (`review.rs:575`: pembuat, admin project/workspace, chair/secretary) + read gate existing (`gate_session_read`, `review.rs:559`).
- **Prasyarat status**: hanya `scheduled`. `completed`/`cancelled` → 400 `"Session is not scheduled"`.
- **Body**: `{ "language": "id" }` opsional. Disanitasi `[a-z-]{1,16}`, default `en`.
- **Respons**: objek `briefing` lengkap (kontrak v1) → web meng-update cache sesi tanpa refetch.
- **Error** (idiom route existing):
  - 403 deny (read/manage gagal), 404 sesi tidak ditemukan.
  - 400 `"AI is not configured for this workspace."` bila `resolve_llm_config` menghasilkan api key/model kosong.
  - 400 sesi bukan `scheduled`.
  - 429 rate limit upstream; 500 upstream error/timeout.
  - Kegagalan apa pun tidak menyimpan apa pun — briefing lama tetap utuh.
- **Batas**: maks 20 item agenda per generate (item pertama menurut posisi; sisanya `skipped_items` dan ditampilkan di UI). Total konteks per item (deskripsi + catatan) dipotong ±1.500 karakter. Daftar change di dalam release dicap dengan hitungan.
- **Sinkron** di dalam request; timeout 60 detik klien existing. Tanpa job/antrian.

### `GET` session detail (existing)

Field `briefing` ditambahkan pada payload detail. Query briefing diambil terpisah di handler detail — **bukan** ditambahkan ke `SESSION_SELECT` (`review.rs:511`) supaya list sesi tidak ikut membawa jsonb besar.

Tidak ada endpoint GET khusus dan tidak ada PATCH — briefing bukan dokumen yang diedit manusia di v1.

## Generation: perakitan konteks → prompt → parsing

Modul baru `apps/api-rs/crates/api/src/routes/review_briefing.rs`.

### Perakitan konteks (deterministik, server-side)

Untuk tiap item agenda, sesuai posisi:

- **TCB (change)**: identifier + judul, deskripsi (HTML di-strip, dipotong), priority, state, target date, assignee, project; catatan pengajuan + pengaju + waktu; riwayat review (outcome, catatan, pemutus, sesi sebelumnya — termasuk item yang pernah `deferred`); konteks insiden: war room terkait (nama, severity, status) lewat `war_room_issues`.
- **RCB (release)**: nama/versi/status/target date, deskripsi; daftar change di dalamnya (identifier, judul, project, outcome review terakhir) dengan cap jumlah + hitungan pending; riwayat review release sebelumnya.
- **Sesi**: `board_type`, judul, jadwal, jumlah item.

### Prompt & panggilan

- `task` = instruksi: ringkas tiap item dari data; tulis `discussion_points` dan `risks` hanya berdasarkan fakta (keputusan lama, item deferred, service/war room terkait, catatan pengaju); **dilarang menyarankan approved/rejected**; dilarang mengarang; balas hanya JSON sesuai skema; tulis dalam bahasa `language`.
- `prompt` = konteks JSON deterministik + daftar `session_item_id` yang valid.
- Satu panggilan `chat_completion` (≤20 item). Model/temperature mengikuti konfigurasi workspace; tidak ada tuning per fitur.

### Parsing & degradasi

1. Parse JSON. Item dengan `session_item_id` tak dikenal atau dobel dibuang.
2. Bukan JSON → retry sekali dengan nudge "balas hanya JSON".
3. Masih gagal → simpan `format: "text"` (`overall` = teks mentah, `items: []`); UI menandainya sebagai teks.
4. Gagal upstream/timeout/not configured → tidak menyimpan apa pun; briefing lama tetap utuh.

## Web UI

### Penempatan & struktur

- Komponen baru `SessionBriefing` di `apps/web/core/components/reviews/session/`, dirender di `ReviewSessionDetailRoot` **setelah header dan sebelum `SessionAgenda`** — bahan bacaan dulu, baru agenda operasional.
- Section header: judul "Briefing", badge AI, `generated_at` + model, tombol Generate/Regenerate (hanya `canManage` dan `status === scheduled`), disclaimer kecil: "Dihasilkan AI — verifikasi fakta sebelum memutuskan."
- Body: ringkasan umum (paragraf), lalu kartu per item **urut posisi agenda**:
  - Baris fakta live: identifier + judul (link ke change/release), state pill, priority, assignee, target date, outcome pill keputusan terakhir bila ada, link war room bila ada.
  - Blok AI: Ringkasan, Poin diskusi (bullet), Risiko (bullet). Item tanpa teks AI tetap tampil faktanya saja.
- Right pane `SessionParticipants` tidak berubah. Briefing bisa dibaca siapa pun yang bisa membaca sesi; sesi `completed`/`cancelled` tetap menampilkan briefing tanpa tombol Regenerate.

### State UI

- Kosong + `canManage`: empty card + tombol Generate. Kosong + bukan `canManage`: teks halus "Belum ada briefing."
- Generating: tombol disabled + status "AI sedang menyusun briefing…" (sinkron, bisa sampai 1 menit). Tanpa konten optimistis.
- Ready: render seperti di atas.
- Teks mode (`format: "text"`): banner "Output AI tidak terstruktur", `overall` dirender pre-wrap, kartu per item disembunyikan.
- `skipped_items > 0`: banner "N item tidak disertakan (batas 20 per generate)".
- Stale hint best-effort: bila jumlah item agenda saat ini ≠ `included_items + skipped_items` → "Agenda berubah sejak briefing dibuat — regenerate." (tidak menangkap hapus + tambah dengan jumlah sama).
- Gagal: toast error dari server; briefing lama tidak terhapus.

### Workstream pendukung

- Types (`@plane/types`): `IReviewSessionBriefing`, `IReviewBriefingItem`, field `briefing?` pada `IReviewSessionDetail`.
- `ReviewService.generateBriefing` + action store `useReview` (update `session.briefing` di cache dari respons, tanpa refetch).
- i18n: key baru `review.briefing.*` (±18 key) ke 20 locale lewat alur translate yang ada.
- Tanpa modal/form baru; helper existing dipakai ulang (`outcome-pill`, `session-status-pill`, `Button`).

## Testing & operasional

### Rust

- Unit test: parser (JSON valid, id tak dikenal/dobel, item hilang, non-JSON → fallback), perakitan konteks + prompt builder.
- Integration `apps/api-rs/crates/api/tests/review_briefing_test.rs` mengikuti pola `ai_agent_test.rs` (fake LLM `TcpListener` + `set_llm_env`). Env bersifat process-global → suite dijalankan serial (`--test-threads=1`) seperti suite AI lain.
  - Happy path + regenerate menimpa (`generated_at` berubah).
  - `AI is not configured` → 400; non-manager → deny; sesi completed/cancelled → 400.
  - Fake LLM balas non-JSON dua kali → teks fallback tersimpan (`format: "text"`).
  - Fake LLM balas 500 → endpoint 500, briefing lama tetap utuh.
- Migrasi `0013` diterapkan boot migrator; tanpa langkah manual.

### Web (vitest)

- Render tiap state (kosong/readonly/generating/ready/teks/`skipped`/stale), visibilitas tombol vs `canManage` + status, dan action store yang meng-update cache sesi dari respons.

### Operasional

- Konfigurasi AI sudah terverifikasi di stack ini (lihat Konteks). Bila tidak dikonfigurasi, endpoint 400 dengan pesan jelas dan UI menampilkan toast.
- Biaya: satu panggilan LLM per generate, selalu dipicu manusia.
- Verifikasi E2E manual: rebuild api (`docker compose -f docker-compose-local.yml up -d --build api worker beat-worker`), buat sesi TCB dengan 2–3 item, generate → cek kartu per item, regenerate, cek sesi `completed` tetap bisa membaca briefing; web build + restart `plane-web-prod`.
