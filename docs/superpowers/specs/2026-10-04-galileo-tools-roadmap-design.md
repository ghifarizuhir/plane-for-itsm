# Galileo Tools Roadmap — Work Items + Services, Intake, Sprints, Tracks, Knowledge Base — Design

Tanggal: 2026-10-04
Status: disetujui user saat brainstorming (Approach A), menunggu review spec tertulis.

## 1. Latar

Galileo (AI assistant) saat ini hanya punya lima tool: tiga read
(`list_projects`, `count_work_items`, `search_work_items`) dan dua proposal
(`create_schedule`, `create_work_item`). Semua tool hanya mencakup project dan
work item; domain ITSM lain — services, intake/triage, sprints (cycles),
tracks (modules), dan knowledge base (pages/articles) — belum bisa diakses
sama sekali. Filter `search_work_items`/`count_work_items` juga belum bisa
menyaring per service, type, assignee, sprint, atau track.

Tujuan roadmap ini: melengkapi tool Galileo untuk lima domain tersebut, dengan
pola yang sudah terbukti aman — read tool langsung dieksekusi, semua mutasi
lewat kartu proposal yang di-Confirm user. Driver prioritas: produktivitas
harian tim (cari/ringkas lintas domain, kelola tiket, rencanakan sprint/track,
triage intake), bukan laporan berkala — tapi read tool yang baru otomatis
tersedia untuk scheduled run.

## 2. Keputusan yang dikunci saat brainstorming

1. Scope domain: work items + services, intake, sprints, tracks, knowledge
   base. War rooms dan releases/review (TCB/RCB) **di luar scope**.
2. Use case penentu prioritas: **produktivitas harian tim**.
3. Pola mutasi: **proposal card untuk semua mutasi** — tidak ada jalur tulis
   langsung dari model.
4. Urutan pembangunan (Approach A): fase read foundation lintas domain dulu,
   lalu mutasi ringan, lalu create/triage.
5. Triage: pipeline AI existing (`ai.intake.triage` + `intake_triage_suggestions`)
   tetap satu-satunya generator suggestion; Galileo hanya **membaca** dan
   **meng-apply** lewat Confirm.
6. Tidak ada migration baru: proposal & decision disimpan di metadata pesan
   percakapan (jsonb), pola yang sama dengan fitur create-work-item.
7. Tanpa perubahan worker/Django di luar penyesuaian allowlist tool read.

## 3. Kondisi saat ini

- Tool didefinisikan di `apps/api-rs/crates/ai/src/tools.rs` (satu file,
  ~1.140 baris).
- `workspace_tools()` memasang 5 tool untuk endpoint agent; `read_tools()`
  hanya memasang read tool sesuai allowlist dan dipakai scheduled run
  (`crates/worker/src/handlers/ai_schedule.rs`).
- Allowlist tool untuk schedule hidup di `SPEC_TOOLS`
  (`crates/ai/src/schedule.rs:18`) dan FE `AI_SCHEDULE_TOOLS`
  (`apps/web/core/lib/ai-schedule.ts:12`) — keduanya harus tetap sinkron.
- Pola proposal existing: tool validasi → catat trace → `pending_actions` →
  metadata assistant message (`work_item_proposals`, `schedule_proposal`) →
  kartu FE → Confirm memanggil endpoint REST existing → decision di-PATCH ke
  metadata (`*_decisions`).
- Semua query tool existing workspace-scoped dan **belum** menyaring akses
  project (`project_members` / `guest_view_all_features`).
- Domain yang sudah ada di backend: `services` + `service_dependencies` +
  `service_issues`; `intakes` + `intake_issues` + `intake_triage_suggestions`;
  `cycles` + `cycle_issues`; `modules` + `module_issues` + `module_members`;
  `pages` + `project_pages`; work item lengkap (komentar, relasi, labels,
  states, types, assignees).
- Tidak ada tabel SLA; service health di FE masih mock. Requester/customer
  tidak dimodelkan (hanya `intake_issues.source`/`source_email`).

## 4. Arsitektur & prinsip lintas fase

### 4.1 Struktur kode

`crates/ai/src/tools.rs` dipecah menjadi direktori:

- `tools/mod.rs` — helper bersama (`schema_of`, `optional_text`,
  `clamp_limit`, `db_error`, resolusi project, guard permission) + registry
  `workspace_tools()` / `read_tools()`.
- `tools/work_items.rs`, `tools/services.rs`, `tools/intake.rs`,
  `tools/sprints.rs`, `tools/tracks.rs`, `tools/kb.rs`.

Refactor bertarget tanpa mengubah perilaku tool existing. Test yang ada ikut
pindah ke modul masing-masing.

### 4.2 Dua kelas tool

- **Read**: eksekusi langsung, catat trace, kembalikan JSON ringkas.
- **Proposal**: validasi bentuk → normalisasi → catat trace → masuk
  `pending_actions`; tidak menyentuh DB. FE merender kartu; Confirm memanggil
  endpoint REST existing; decision disimpan di metadata pesan.

### 4.3 Registry & schedule

- Semua read tool didaftarkan di `workspace_tools()` **dan** `read_tools()`.
- `SPEC_TOOLS` dan `AI_SCHEDULE_TOOLS` diperluas dengan seluruh read tool
  baru, sehingga otomatis tersedia untuk laporan berkala.
- Proposal tool tidak pernah terdaftar di `read_tools()` — scheduled run
  secara struktural tetap read-only.
- Sinkronisasi tiga tempat wajib dijaga: `SPEC_TOOLS` (backend), validasi
  spec (`ScheduleSpec::validated`), `AI_SCHEDULE_TOOLS` (FE).
  Schedule lama dengan `spec IS NULL` otomatis mendapat seluruh read tool.

### 4.4 Metadata proposal & decision generik

Jenis proposal baru memakai bentuk generik; bentuk lama tetap dibaca
(backward compatible).

- Assistant message metadata:
  - `proposals: [{key, kind, proposal}]` — `key` UUID digenerate server,
    satu per proposal, urut trace.
  - `proposal_decisions: {<key>: {kind, decision, result?}}`.
- `decision`: `applied` atau `cancelled`. `result` opsional, berisi id hasil
  (mis. `created_service_id`, `created_sprint_id`, `created_project_id`,
  `created_track_id`, `created_article_id`, `created_comment_id`) untuk
  membangun link setelah reload.
- `clean_metadata_patch` (`routes/ai_conversations.rs`) memvalidasi:
  `kind` dikenal, `decision` sesuai jenis, `result` object dengan value UUID
  dan key yang di-allowlist per kind.
- Key lama (`schedule_proposal`, `schedule_decision`, `created_schedule_id`,
  `work_item_proposals`, `work_item_decisions`) tidak berubah; FE membaca
  keduanya dan menggabungkan.
- Idempotency: satu `key` per proposal + guard `decision != pending` +
  Confirm disabled saat in-flight (pola existing).

### 4.5 Scoping & permission

- `workspace_id` selalu di-bind dari auth/run, tidak pernah dari model.
- Tool project-scoped me-resolve `project` dari identifier (exact, lalu case-
  insensitive) atau nama (exact, lalu substring unik). Ambigu → pesan
  model-visible yang meminta identifier; tidak menebak.
- Filter nama yang berpotensi ambigu (label/track/sprint tanpa `project`)
  memakai exact match unik; bila ambigu → tool meminta `project` secara
  eksplisit, tidak menebak.
- Akses project: wajib ada `project_members` aktif (mengikuti endpoint domain
  yang dicerminkan) + gate guest (`guest_view_all_features`). Artikel private
  hanya untuk owner/workspace admin sesuai semantik endpoint page.
- Feature gate project dihormati (`cycle_view`, `module_view`, `page_view`,
  `intake_view`): fitur nonaktif → pesan eksplisit, bukan hasil kosong.
- `search_work_items`/`count_work_items` existing disamakan ke semantik akses
  yang sama (perubahan perilaku kecil untuk keamanan).

### 4.6 Budget output

- Konvensi `limit` existing: default 10, maksimum 25.
- Batas teks: deskripsi work item 4.000 char; body artikel 8.000 char (hanya
  `get_article`); komentar 500 char/baris; daftar item per container maks 10
  di tool `get_*`, 25 di tool `list_*`; snippet artikel 200 char.
- Output hanya field yang dibutuhkan model, memakai nama manusiawi
  (state, assignee, label, service, sprint, track), bukan seluruh row.

### 4.7 Prompt & error handling

- `PREAMBLE` (`crates/ai/src/agent.rs`) diperbarui per fase: kapan memakai
  tool mana, wajib menyebut target eksplisit, larangan mengklaim sukses
  sebelum user Confirm, larangan mengarang saat tool mengembalikan kosong.
- Validasi bentuk → `ToolExecutionError::invalid_args` (model-visible);
  error DB → redacted (`db_error`); project/fitur tidak ditemukan → pesan
  yang bisa ditindaklanjuti model.

## 5. Fase 1 — Read foundation

Semua read tool baru di bawah ini schedule-eligible.

### 5.1 Batch 1.1 — Work items + cross-cutting

| Tool                          | Args                                                                                      | Output / catatan                                                                                                                                                              |
| ----------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_work_item`               | `work_item` (mis. `LTS-42`)                                                               | Detail: name, description (4k), state+group, priority, type, assignees, labels, sprint, tracks, services, parent, tanggal, count sub-item/komentar/relasi, 5 komentar terbaru |
| `list_work_item_comments`     | `work_item`, `limit?` (10/25)                                                             | Komentar terbaru: penulis, waktu, 500 char                                                                                                                                    |
| `list_work_item_relations`    | `work_item`                                                                               | Kelompok relasi: blocked_by, blocking, relates_to, duplicate, dll                                                                                                             |
| `search_work_items` (perluas) | filter lama + `service`, `type`, `assignee` (nama/email/`me`), `sprint`, `track`, `label` | Output ditambah state/assignee/type                                                                                                                                           |
| `count_work_items` (perluas)  | filter yang sama                                                                          | Untuk "berapa tiket Incident minggu ini"                                                                                                                                      |
| `list_members`                | `project?` (kosong = workspace), `query?`, `limit?`                                       | Resolusi orang: name, email, role                                                                                                                                             |
| `list_states`                 | `project`                                                                                 | name, group, default, is_triage                                                                                                                                               |
| `list_labels`                 | `project`                                                                                 | name                                                                                                                                                                          |
| `list_work_item_types`        | `project?`                                                                                | name, is_epic, requires_service                                                                                                                                               |

### 5.2 Batch 1.2 — Services + Intake

| Tool                 | Args                                                                     | Output / catatan                                                                                                  |
| -------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| `list_services`      | `query?`, `status?`, `criticality?`, `type?`, `project?`, `limit?`       | name, status, criticality, type, owner, project                                                                   |
| `get_service`        | `service` (nama/id), `project?`                                          | Detail + dependency (upstream/downstream) + work item terbuka (maks 10) + count per state group                   |
| `list_intake_items`  | `project?`, `status?` (default pending), `priority?`, `query?`, `limit?` | id, name, priority, source, created, project, ringkasan triage suggestion (kategori/severity/service/needs_human) |
| `get_intake_item`    | `intake_item` (id)                                                       | Detail + suggestion terbaru lengkap + `applied_fields`                                                            |
| `count_intake_items` | `project?`, `status?`                                                    | Untuk jam antrian triage                                                                                          |

Tidak ada tool `search work items by service` terpisah — tercakup filter
`service` di Batch 1.1.

### 5.3 Batch 1.3 — Sprints + Tracks + KB

| Tool                     | Args                                                               | Output / catatan                                                                                       |
| ------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| `list_sprints`           | `project?`, `status?` (current/upcoming/completed/draft), `limit?` | name, project, start/end, status, progress total/completed, owner                                      |
| `get_sprint`             | `sprint` (nama/id), `project?`                                     | Detail + progress live (fallback hitung bila `progress_snapshot` kosong/stale) + 10 item belum selesai |
| `list_sprint_work_items` | `sprint`, `project?`, `state_group?`, `assignee?`, `limit?`        | Daftar item sprint dengan filter                                                                       |
| `list_tracks`            | `project?`, `status?`, `query?`, `limit?`                          | name, status, lead, jumlah member, progress                                                            |
| `get_track`              | `track` (nama/id), `project?`                                      | Detail + members + links + progress + 10 item                                                          |
| `list_track_work_items`  | `track`, `project?`, `state_group?`, `assignee?`, `limit?`         | Daftar item track                                                                                      |
| `search_articles`        | `query` (wajib), `project?`, `limit?`                              | id, name, project, updated_at, snippet 200 char; hormati `access` private                              |
| `get_article`            | `article` (id)                                                     | Body (8k), sub-halaman, labels, issue terkait                                                          |

### 5.4 Plumbing Fase 1

- Tambah `user_id` ke konstruktor tool: `workspace_tools(pool, workspace_id,
user_id, trace)` dan `read_tools(pool, workspace_id, user_id, trace,
allowed)`. Endpoint agent memakai `auth.0`; worker memakai
  `ai_schedules.created_by_id`. Diperlukan untuk `assignee: "me"` dan
  "tiket saya".
- Helper resolusi project + guard permission di `tools/mod.rs` dipakai
  bersama semua tool project-scoped.
- `PREAMBLE` menyebut kapan KB vs work item search, dan aturan nama manusiawi.

## 6. Fase 2 — Mutasi ringan (semua proposal card)

### 6.1 Tool

Hanya terdaftar di `workspace_tools()`, tidak pernah di scheduled run.

| Tool                   | Args                                                                                                                                   | Confirm ke endpoint existing                                               |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `update_work_item`     | `work_item`, `changes` (minimal 1: `name`, `description`, `priority`, `state`, `assignees[]`, `labels[]`, `start_date`, `target_date`) | `PATCH .../issues/:pk/`                                                    |
| `add_comment`          | `work_item`, `comment` (1–5.000 char)                                                                                                  | `POST .../issues/:issue_id/comments/`                                      |
| `manage_service_links` | `work_item`, `services[]`, `action: link \| unlink`                                                                                    | `POST/DELETE .../service-issues/`                                          |
| `manage_sprint_items`  | `sprint`, `project?`, `work_items[]` (≤25), `action: add \| remove`                                                                    | `POST .../cycles/:id/cycle-issues/`, `DELETE .../cycle-issues/:issue_id/`  |
| `manage_track_items`   | `track`, `project?`, `work_items[]` (≤25), `action: add \| remove`                                                                     | `POST .../modules/:id/issues/`, `DELETE .../modules/:id/issues/:issue_id/` |

Validasi backend hanya bentuk: format identifier `PROJ-123` per item,
panjang, enum action, minimal satu perubahan. Nama state/assignee/label/
sprint/track tidak di-resolve di backend — FE yang resolve ke UUID memakai
store existing (pola `WorkItemProposalCard`). Proposal dinormalisasi (trim,
dedupe) dan dicatat ke trace setelah validasi.

### 6.2 Metadata

Mengikuti bentuk generik §4.4. `result` per kind:

- `add_comment` → `created_comment_id`.
- `update_work_item` → tanpa id (field yang berubah cukup ditampilkan dari
  proposal).
- `manage_service_links` / `manage_sprint_items` / `manage_track_items` →
  tanpa id; kartu menampilkan status applied/cancelled.

### 6.3 FE — kartu proposal

Direktori baru `apps/web/core/components/ai/assistant-sidebar/proposals/`:

- `WorkItemUpdateProposalCard`: diff field lama → baru, tiap field editable
  (select state/assignee/label dari store project), Confirm disabled bila
  target work item gagal resolve.
- `CommentProposalCard`: textarea + preview; konversi teks → HTML memakai
  helper existing di `ai-work-items.ts`.
- `LinkItemsProposalCard` generik untuk service/sprint/track: daftar item
  yang akan di-link/unlink; item yang sudah sesuai kondisi di-skip dengan
  catatan kecil; Confirm/Cancel.
- Store: satu fungsi generik `confirmProposal(messageId, key)` + dispatch
  table `kind → service call`, dan `cancelProposal`. Sukses → decision
  `applied` + `result`; gagal → error di kartu, decision tetap `pending`.

### 6.4 Error handling

- Target work item/sprint/track tidak ketemu → kartu error, hanya bisa
  Cancel (tidak ada editor target).
- Nama state/assignee/label tidak ketemu → field dikosongkan + catatan "pick
  manually"; tidak memblokir Confirm bila field opsional.
- No-op (item sudah ada / belum ada di container) → di-drop dari payload +
  catatan.
- API gagal (permission/validasi/network) → pesan error di kartu, decision
  tetap `pending` sehingga bisa retry.

### 6.5 PREAMBLE

Tambah aturan: kapan mempropose update/komentar/link; wajib target eksplisit
(kalau ambigu tanya dulu); dilarang bilang "sudah dibuat/diubah" sebelum
Confirm.

## 7. Fase 3 — Create & triage (semua proposal card)

### 7.1 Create/update container

| Tool             | Args                                                                                                                                                                    | Confirm ke endpoint existing                                     |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `create_service` | `project` (wajib), `name` (wajib, unik per project), `description?`, `status?`, `criticality?`, `type?`, `owner?` (nama/email), `repository_url?`, `documentation_url?` | `POST .../services/`                                             |
| `update_service` | `service`, `changes` (minimal 1: status, criticality, type, owner, description, urls)                                                                                   | `PATCH .../services/:pk/`                                        |
| `create_sprint`  | `project` (wajib), `name` (wajib), `description?`, `start_date?`, `end_date?`, `owner?` (default user login)                                                            | `POST .../cycles/`                                               |
| `update_sprint`  | `sprint`, `changes` (minimal 1: name, description, start/end, owner)                                                                                                    | `PATCH .../cycles/:pk/`                                          |
| `create_track`   | `project` (wajib), `name` (wajib), `description?`, `start_date?`, `target_date?`, `status?`, `lead?`, `members[]?`                                                      | `POST .../modules/`                                              |
| `update_track`   | `track`, `changes` (minimal 1: name, description, tanggal, status, lead, members)                                                                                       | `PATCH .../modules/:pk/`                                         |
| `create_article` | `project` (wajib), `name` (wajib), `content` (teks → HTML), `parent_article?`, `access?` (default public)                                                               | Service page existing: `POST .../pages/` lalu simpan description |
| `update_article` | `article` (id), `action: append \| replace` (wajib), `name?`, `content` (1–20.000 char)                                                                                 | `PATCH page` + `PATCH .../description/`                          |

`action` wajib di `update_article` untuk mencegah replace body tak sengaja.

### 7.2 Triage intake

| Tool                      | Args                                                                                               | Confirm                             |
| ------------------------- | -------------------------------------------------------------------------------------------------- | ----------------------------------- |
| `apply_triage_suggestion` | `intake_item`, `fields?` (subset `category\|service\|severity`, default semua yang `ready`)        | `POST .../triage-suggestion/apply/` |
| `triage_intake_item`      | `intake_item`, `action: accept \| reject \| snooze \| duplicate`, `snoozed_till?`, `duplicate_of?` | `PATCH .../inbox-issues/:pk/`       |

Pipeline AI triage existing tetap satu-satunya generator suggestion; Galileo
membaca (Fase 1) dan meng-apply lewat Confirm. `accept` bisa gagal gate
(`requires_service` belum ada service / type belum diset) → kartu menampilkan
error + saran tindakan, decision tetap `pending`.

### 7.3 FE — form engine generik

Ketimbang 8+ kartu bespoke, dipakai satu `proposal-form-card.tsx` berbasis
schema: definisi field per kind (text/textarea/select/date/members/url)
sebagai data, opsi select dari store existing. Kartu bespoke hanya untuk
tampilan khusus: diff update work item, preview artikel, triage suggestion.
Schema form per kind wajib punya test validasi.

## 8. Testing

- Rust unit per tool: validasi bentuk, normalisasi (trim/dedupe/allowlist),
  trace hanya dicatat saat valid, `parameters()` JSON schema benar.
- Rust integrasi fake upstream (`crates/api/tests/ai_agent_test.rs`): satu
  roundtrip per proposal kind baru (proposal muncul di metadata +
  `pending_actions`), plus roundtrip PATCH decision generik.
- Worker: ekspektasi `allowed_tools` diperbarui saat `SPEC_TOOLS` bertambah;
  dipastikan proposal tool tidak pernah terpasang di scheduled run.
- FE vitest: schema form per kind, dispatch confirm/cancel store, persist
  decision map, guard double-confirm.
- Smoke manual per fase: tanya lintas domain (Fase 1), update/komentar/link
  (Fase 2), create/triage (Fase 3), cek scheduled run tidak bisa mutasi.

## 9. Rollout & observability

- Feature sudah di-gate `has_llm_configured`; tidak ada migration.
- Fase 1 read-only dirilis lebih dulu (aman, langsung berguna); Fase 2 dan 3
  menyusul tanpa mengubah kontrak lama.
- Setiap fase = PR terpisah dengan plan sendiri.
- Trace `tool_calls` sudah tersimpan per pesan + warning `tracing` pada error
  DB.
- Risiko utama: prompt drifting (model memanggil tool berlebihan) → mitigasi
  deskripsi tool ketat + `MAX_TURNS` existing 6.
- `search_articles` memakai ILIKE; pada KB besar bisa lambat → indeks
  `pg_trgm` dicatat sebagai future work, bukan sekarang.

## 10. Non-goals

- War rooms, releases/review TCB/RCB.
- Tool delete/archive (pakai UI existing).
- Bulk create.
- Auto-create/mutasi dari scheduled run.
- Ganti work item type dari chat (gate `requires_service` sensitif).
- Grounding KB di mode Classic (desain terpisah).
- SLA dan service health (data belum ada di skema; health FE masih mock).

## 11. File map

| File                                                                           | Aksi                                                                               |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------- |
| `apps/api-rs/crates/ai/src/tools.rs`                                           | Pecah → `tools/mod.rs` + `tools/{work_items,services,intake,sprints,tracks,kb}.rs` |
| `apps/api-rs/crates/ai/src/agent.rs`                                           | `PREAMBLE`, `pending_actions` (kind baru)                                          |
| `apps/api-rs/crates/ai/src/schedule.rs`                                        | `SPEC_TOOLS` diperluas                                                             |
| `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`                            | Metadata `proposals` generik; `user_id` ke tool server                             |
| `apps/api-rs/crates/api/src/routes/ai_conversations.rs`                        | `clean_metadata_patch`: `proposals`/`proposal_decisions`                           |
| `apps/api-rs/crates/worker/src/handlers/ai_schedule.rs`                        | `read_tools(...)` dengan `created_by_id`                                           |
| `apps/web/core/lib/ai-proposals.ts` (baru)                                     | Tipe `TAiProposal`, decision, helper validasi                                      |
| `apps/web/core/lib/ai-schedule.ts`                                             | `AI_SCHEDULE_TOOLS` diperluas                                                      |
| `apps/web/core/lib/ai-conversations.ts`                                        | Mapping metadata `proposals`/`proposal_decisions`                                  |
| `apps/web/core/store/ai-assistant.store.ts`                                    | `confirmProposal`/`cancelProposal` generik + dispatch                              |
| `apps/web/core/components/ai/assistant-sidebar/proposals/*` (baru)             | Kartu proposal                                                                     |
| `apps/web/core/components/ai/assistant-sidebar/root.tsx`                       | Render `message.proposals`                                                         |
| `apps/api-rs/crates/ai/src/tools/` tests + `crates/api/tests/ai_agent_test.rs` | Test unit & integrasi                                                              |
| `apps/web/core/lib/*.test.ts`                                                  | Test FE                                                                            |

## 12. Future work

- Indeks `pg_trgm` untuk `search_articles`.
- Grounding KB di mode Classic.
- Tool war rooms dan releases/review.
- Tool delete/archive dan bulk.
- Idempotency level DB untuk mutasi (selain guard decision).
