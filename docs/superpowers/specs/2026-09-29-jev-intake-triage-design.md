# Saran Triage Intake dengan Jev (TypeSafe) — Design (2026-09-29)

## Tujuan

Model AI di `apps/api-rs` saat ini hanya satu: `LLM_MODEL` + `LLM_API_KEY` dengan
endpoint chat OpenAI-compatible. Slice ini menambah **model kedua** untuk
keputusan terstruktur — **Jev**, model "System One" TypeSafe — dan memakainya
untuk **saran triage otomatis** pada item intake: kategori (Choice atas tipe
work item project), severity (Score atas 5 priority), dan needs-human (Noul).
Jawaban Jev disimpan sebagai **saran**, ditampilkan di detail intake, dan
**tidak pernah** ditulis otomatis ke work item; hanya priority yang bisa
di-apply satu klik.

Jev bukan LLM: ia tidak menghasilkan teks. Ia mengevaluasi `state` + pertanyaan
bertipe (noul/choice/score) dalam satu panggilan paralel dan mengembalikan
probabilitas terkalibrasi + confidence. Deployment ini sudah memakai OpenRouter
(`LLM_BASE_URL=https://openrouter.ai/api/v1`), sehingga Jev bisa dipakai dengan
API key yang sama — tanpa akun TypeSafe terpisah.

Keputusan brainstorming:

1. **Tujuan:** Jev decisions alongside LLMs; use case pertama = saran triage
   intake.
2. **Konfigurasi:** key instance baru `LLM_DECISION_MODEL`; reuse `LLM_API_KEY`
   - `LLM_BASE_URL`; endpoint `{LLM_BASE_URL}/systemone`. Kosong = fitur mati.
     Model OpenRouter: `typesafe/jev-1.13` (alias `~typesafe/jev-latest`).
3. **Trigger:** otomatis. Job worker di-push setelah intake dibuat, plus sweep
   beat tiap menit untuk backfill (item lama / dibuat saat model belum diset)
   dan retry (worker at-most-once: job error di-ack dan dibuang).
4. **Aplikasi:** suggest-only. Apply satu klik hanya `severity` → `priority`;
   `category` dan `needs_human` informasional + bisa dismiss. Apply tipe ditunda
   karena perubahan tipe menyentuh aturan workflow/state
   (`routes/v1/work_item.rs:949-1007`).
5. **Kosakata:** tipe non-Epic milik project (maks 20) untuk kategori; rubrik 5
   priority none→urgent untuk severity; Noul untuk needs-human. Tanpa UI
   konfigurasi kosakata.
6. **Arsitektur:** Rust-first (live server). Endpoint intake existing tidak
   diubah kontraknya; saran diekspos lewat subresource baru. Django hanya
   dipakai untuk seed key config lewat data migration.
7. **Testing:** unit murni untuk builder/parsing/mapping; integration Rust
   dengan fake upstream (tanpa jaringan); test FE vitest mengikuti pola
   store/service existing.

Non-goal (v1): auto-apply & confidence threshold, apply `category`, tombol
re-classify manual, kosakata configurable per workspace, badge saran di list
intake, key/base URL decision terpisah (TypeSafe direct), surface keputusan
lain (agent gating, dedup, enrichment), penjelasan natural-language, dan model
registry umum.

## Konfigurasi

| Key                  | Kategori | Encrypted | Default | Catatan                    |
| -------------------- | -------- | --------- | ------- | -------------------------- |
| `LLM_DECISION_MODEL` | AI       | tidak     | `""`    | Kosong = saran triage mati |

- **Seed:** data migration baru di `apps/api/plane/license/migrations/`
  (`get_or_create`, category `AI`, non-encrypted) **dan** entri baru di
  `apps/api/plane/utils/instance_config_variables/core.py` (~216-242) untuk
  instalasi baru. Alasan: `configs_patch` Rust hanya meng-UPDATE key yang
  row-nya sudah ada (`routes/instance_admin.rs:1346-1405`), dan entrypoint
  migrator hanya menjalankan `migrate` — bukan `configure_instance`
  (`apps/api/bin/docker-entrypoint-migrator.sh`). Tanpa seed, PATCH admin
  mengabaikan key baru dan form tidak bisa menyimpan.
- **Admin UI:** field "Decision model" di
  `apps/admin/app/(all)/(dashboard)/ai/form.tsx` (setelah `LLM_MODEL`),
  placeholder `typesafe/jev-1.13`, deskripsi: "Model System One (keputusan) di
  base URL LLM. Kosongkan untuk mematikan saran triage."
  `TInstanceAIConfigurationKeys` di `packages/types/src/instance/ai.ts:7`
  ditambah `LLM_DECISION_MODEL`.
- **Endpoint:** `{LLM_BASE_URL}/systemone`, memakai join URL yang sama dengan
  `chat_url()` (`routes/ai.rs:34`). Default `LLM_BASE_URL` chat tetap
  `https://api.openai.com/v1`.
- Flag `has_llm_configured` (chat) tidak berubah. Fitur triage bergantung pada
  `LLM_DECISION_MODEL` non-kosong, bukan pada `LLM_MODEL`.

## Wire format Jev

`POST {base_url}/systemone`, header `Authorization: Bearer {api_key}` dan
`Content-Type: application/json`:

```json
{
  "model": "typesafe/jev-1.13",
  "state": {
    "work_item": { "name": "...", "description": "..." },
    "project": { "name": "..." },
    "source": "IN_APP"
  },
  "questions": {
    "category": {
      "type": "choice",
      "instructions": "Which work item type best fits this intake request?",
      "criteria": { "Incident": "Something is broken...", "Problem": "..." }
    },
    "severity": {
      "type": "score",
      "instructions": "How severe is this request?",
      "criteria": ["...", "...", "...", "...", "..."]
    },
    "needs_human": {
      "type": "noul",
      "instructions": "Does this request need a human to handle it?",
      "criteria": { "true": "...", "false": "..." }
    }
  }
}
```

Respons 200:

```json
{
  "model": "jev-1.13.0",
  "answers": {
    "category": {
      "type": "choice",
      "choice": "Incident",
      "confidence": 0.87,
      "probabilities": { "Incident": 0.87, "Problem": 0.08 }
    },
    "severity": {
      "type": "score",
      "score": 2.9,
      "confidence": 0.91,
      "legend": { "0": "none", "1": "low", "2": "medium", "3": "high", "4": "urgent" },
      "probabilities": { "0": 0.0, "1": 0.02, "2": 0.1, "3": 0.83, "4": 0.05 }
    },
    "needs_human": { "type": "noul", "noul": 0.78 }
  },
  "usage": { "input_tokens": 421, "output_tokens": 37 }
}
```

Batasan: Choice maks 255 opsi, Score 2–10 level, konteks 32k token, output
tidak dibebankan. Error yang dipetakan: 401/403 (auth), 422 (body invalid),
429 (rate limit), 5xx/timeout (upstream).

## Model data

Migrasi sqlx baru `apps/api-rs/migrations/0010_intake_triage_suggestions.sql`,
tabel `intake_triage_suggestions`, satu row per intake issue:

| Kolom                                                        | Tipe                 | Catatan                                          |
| ------------------------------------------------------------ | -------------------- | ------------------------------------------------ |
| `id`                                                         | uuid PK              | default `gen_random_uuid()`                      |
| `intake_issue_id`                                            | uuid NOT NULL        | FK `intake_issues(id)` ON DELETE CASCADE, UNIQUE |
| `project_id`, `workspace_id`                                 | uuid NOT NULL        | scope auth endpoint                              |
| `status`                                                     | text NOT NULL        | `pending` (claim), `ready`, `failed`             |
| `model`                                                      | text                 | id versi yang menjawab (`jev-1.13.0`)            |
| `answers`                                                    | jsonb                | respons mentah `answers`                         |
| `category_type_id`, `category_label`, `category_confidence`  | uuid, text, float8   | hasil Choice + mapping tipe                      |
| `severity_priority`, `severity_score`, `severity_confidence` | text, float8, float8 | hasil Score + mapping `round(clamp(0..4))`       |
| `needs_human`                                                | float8               | probabilitas Noul                                |
| `applied_fields`, `dismissed_fields`                         | text[]               | subset `category`, `severity`, `needs_human`     |
| `attempts`, `last_error`                                     | int, text            | retry sweep; error dipotong 500 char             |
| `input_tokens`, `output_tokens`                              | int                  | usage per request                                |
| `created_at`, `updated_at`                                   | timestamptz          | default `now()`                                  |

Index parsial untuk sweep: `(status, updated_at) WHERE status = 'failed'`,
plus UNIQUE `intake_issue_id` yang sudah menutup kasus `pending`/`ready`.

## Klien Jev (`apps/api-rs/crates/ai/src/decision.rs`)

- `DecisionConfig { api_key, model, base_url }`;
  `resolve_decision_config(pool) -> Option<DecisionConfig>` membaca row
  `LLM_DECISION_MODEL` (fallback env var dengan nama yang sama, mengikuti pola
  `llm_config_from_rows` di `llm.rs:30-62`) dan reuse resolusi `LLM_API_KEY`,
  `None` bila model kosong **atau** key kosong. `base_url` dari `LLM_BASE_URL`
  (default sama dengan chat).
- `ask(cfg, state, questions) -> Result<DecisionOutcome, DecisionError>`:
  `reqwest` POST `{base_url}/systemone`, timeout 30 detik,
  `DecisionOutcome { model, answers, input_tokens, output_tokens }`.
- Tipe typed untuk question/answer (`noul`, `choice`, `score`);
  `DecisionError`: `NotConfigured`, `RateLimited`, `InvalidRequest` (422),
  `Upstream`, `Timeout`. Pesan error tidak pernah memuat API key (mengikuti
  `llm.rs`/`routes/ai.rs`).
- **Seam transport** untuk test: `ask` dibangun di atas fungsi yang menerima
  transport (pola injeksi seperti test `routes/ai.rs`), supaya integration test
  memakai fake upstream tanpa jaringan.
- Builder state + pertanyaan:
  - `state`: `work_item.name`, `work_item.description` (teks polos, dipotong
    4.000 char), `project.name`, `source`.
  - `category`: Choice; opsi = tipe project `is_epic = false`, maks 20, criteria
    `name` → deskripsi tipe (fallback nama). Bila project tidak punya tipe,
    question `category` dihilangkan.
  - `severity`: Score; criteria berurutan `[none, low, medium, high, urgent]`
    dengan deskripsi dampak; mapping `round(score).clamp(0, 4)` → priority.
  - `needs_human`: Noul dengan criteria true/false.
- Unit test: urutan criteria, exclude Epic, fallback tanpa tipe, mapping batas
  0.5/1.5/2.5/3.5, parsing tiap tipe answer, truncation.

## Eksekusi (push + sweep)

1. **Push saat create.** `routes/intake.rs::create_issue` (573-704): setelah
   `tx.commit()` (693), push job `ai.intake.triage` `{intake_issue_id}` lewat
   `common::stream::push_job` dengan `state.redis_client()` (pola
   `routes/ai_schedule.rs:536-573`). Kegagalan push hanya di-`tracing::warn`,
   tidak menggagalkan create.
2. **Handler `ai.intake.triage`** (`crates/worker/src/handlers/intake_triage.rs`):
   - `resolve_decision_config` → `None`: keluar tanpa membuat row, agar sweep
     mencoba lagi setelah model diset.
   - Claim idempotent: `INSERT ... ON CONFLICT (intake_issue_id) DO NOTHING
RETURNING id`; kalah klaim → keluar (aman terhadap race push vs sweep).
   - Load intake item + tipe project, bangun state/questions, panggil Jev.
   - Sukses: `UPDATE status='ready'` + kolom hasil + usage.
   - Gagal: `attempts += 1`, `status='failed'`, `last_error` dipotong 500 char,
     `tracing::warn` tanpa key.
3. **Sweep `ai.intake.triage.sweep`** — beat tiap menit (pola `ai.schedule.tick`,
   `beat/src/main.rs:99-115`): gate config; pilih maks 10 item intake dengan
   `intake_issues.status = -2` yang belum punya row saran **atau** punya row
   saran `failed` dengan `attempts < 3` dan `updated_at < now() - interval '5
minutes'`; push satu job `ai.intake.triage` per item. Ini backfill item lama
   dan pemulihan karena worker at-most-once (`worker/src/main.rs:30-36`).
4. **Allowlist worker:** tambah `ai.intake.triage` dan `ai.intake.triage.sweep`
   di `is_enabled_job` + dispatch (`worker/src/handlers/mod.rs:47-78`).
5. Item intake yang sudah di-accept/decline (status != -2) tidak diproses sweep;
   saran yang sudah ada tetap bisa dibaca/di-dismiss.

## API

Base path: `/api/workspaces/:slug/projects/:project_id/intake-issues/:pk`
(auth `AuthUser` + gate project member+, mengikuti gate issue-field pada
`patch_issue`).

| Method | Path                             | Body                                                | 200                    | Error                                        |
| ------ | -------------------------------- | --------------------------------------------------- | ---------------------- | -------------------------------------------- |
| GET    | `.../triage-suggestion/`         | –                                                   | `{"data": suggestion}` | 401, 403, 404                                |
| POST   | `.../triage-suggestion/apply/`   | `{"fields": ["severity"]}`                          | `{"data": suggestion}` | 400 (field tak didukung / sudah `dismissed`) |
| POST   | `.../triage-suggestion/dismiss/` | `{"fields": ["category","severity","needs_human"]}` | `{"data": suggestion}` | 400 (sudah `applied`)                        |

Bentuk `suggestion`:

```json
{
  "id": "...",
  "status": "ready",
  "model": "jev-1.13.0",
  "category": {
    "type_id": "...",
    "label": "Incident",
    "confidence": 0.87,
    "probabilities": { "Incident": 0.87, "Problem": 0.08 }
  },
  "severity": {
    "priority": "high",
    "score": 2.9,
    "confidence": 0.91,
    "probabilities": { "none": 0.0, "low": 0.02, "medium": 0.1, "high": 0.83, "urgent": 0.05 }
  },
  "needs_human": { "probability": 0.78 },
  "applied_fields": [],
  "dismissed_fields": [],
  "created_at": "..."
}
```

- GET mengembalikan row `pending` atau `ready`; row `failed` → `null` (tidak
  tampil di UI). Field hasil `null` saat masih `pending`.
- `apply` hanya menerima `severity`: tulis `issues.priority =
severity.priority` (validasi nilai sama dengan `patch_issue`,
  `intake.rs:1402-1412`) + tambahkan `applied_fields`, satu transaksi,
  idempotent bila sudah applied. `category`/`needs_human` → 400 di v1.
  `updated_by_id` = user pemanggil. Tidak menulis `issue_activity` (konsisten
  dengan deviasi intake Rust yang memang skip activity, `intake.rs:1045-1047`).
- `dismiss` idempotent; field yang sudah `applied` → 400. `apply`/`dismiss`
  menolak row yang belum `ready`.
- Endpoint intake existing (`detail_issue`, `patch_issue`) tidak diubah, jadi
  kontrak parity `docs/features/intake.md` tetap; route baru didaftarkan di
  blok intake `main.rs` (~851-869).

## UI

- Komponen baru `apps/web/core/components/inbox/content/triage-suggestion.tsx`,
  dirender di detail intake (area `content/`) hanya bila GET mengembalikan data:
  - **Category:** label + confidence + bar distribusi, tombol Dismiss
    (informasional).
  - **Severity:** priority + skor + confidence, tombol **Apply** dan Dismiss.
  - **Needs human:** ya/tidak + probabilitas, tombol Dismiss (informasional).
  - Footer: `Suggested by {model} · {waktu relatif}` + catatan "saran tidak
    otomatis diterapkan".
  - Status `pending`: baris ringkas "Classifying…" (tanpa aksi).
- `apps/web/core/services/inbox/inbox-issue.service.ts` +
  `apps/web/core/store/inbox/inbox-issue.store.ts`: method get/apply/dismiss;
  state di store; fetch saat detail intake dibuka; response apply/dismiss
  menggantikan state.
- Tipe baru `TInboxIssueTriageSuggestion` di `packages/types/src/inbox.ts`.
- i18n: key baru di `packages/i18n/src/locales/en/inbox.json`; sinkronisasi
  locale lain memakai skill translate (workflow repo).
- Tanpa perubahan list/sidebar intake (tanpa badge) di v1.

## Error & biaya

- Fitur mati saat `LLM_DECISION_MODEL` kosong: tidak ada row, tidak ada
  panggilan, panel tidak muncul.
- Jev error/timeout/429 → row `failed`, diretry sweep maks 3x dengan cooldown
  5 menit, lalu diam.
- State dipotong 4.000 char (bound biaya; OpenRouter Jev $0.042/1M input token,
  output gratis); `usage` disimpan per saran untuk audit.
- Sweep dibatasi 10 item/menit untuk membatasi biaya burst saat backfill.

## Testing

- **Rust unit** (`decision.rs`): builder criteria (urut, exclude Epic, fallback
  tanpa tipe), mapping skor→priority di batas, parsing `noul`/`choice`/`score`,
  truncation, mapping error (422/429/timeout), redaksi key.
- **Rust integration** (`apps/api-rs/crates/api/tests/intake_triage_test.rs`,
  pola `intake_test.rs` + `ai_*_test.rs`): fake upstream System One (axum
  stateful) lewat seam transport; kasus: create memicu push, handler menulis
  `ready` + usage, claim konflik idempotent, error → `failed` + attempts, sweep
  memilih item belum terklasifikasi dan membackfill, apply `severity` menulis
  priority + `applied_fields`, apply field lain → 400, dismiss idempotent,
  GET `failed` → `null`. Jalankan seri bila memakai fixture scratch (AGENTS.md).
- **FE** (vitest): service memanggil path/method yang benar; store meng-update
  state setelah apply/dismiss.
- **Manual/live:** set `LLM_DECISION_MODEL=typesafe/jev-1.13`, rebuild/restart
  api+worker+beat, buat intake item, panel muncul (≤ ~beberapa detik), apply
  priority, cek `usage` dan nama model versi di row.

## File yang disentuh (ringkas)

- **Rust:** `crates/ai/src/decision.rs` (baru), `crates/ai/src/lib.rs`,
  `crates/worker/src/handlers/intake_triage.rs` (baru) + `handlers/mod.rs`,
  `crates/beat/src/main.rs`, `crates/api/src/main.rs`,
  `crates/api/src/routes/intake.rs` (push + handler subresource),
  `migrations/0010_intake_triage_suggestions.sql` (baru), test baru.
- **Django:** data migration di `apps/api/plane/license/migrations/`,
  `apps/api/plane/utils/instance_config_variables/core.py`.
- **Admin:** `apps/admin/app/(all)/(dashboard)/ai/form.tsx`,
  `packages/types/src/instance/ai.ts`.
- **Web:** `apps/web/core/components/inbox/content/triage-suggestion.tsx`
  (baru), `apps/web/core/services/inbox/inbox-issue.service.ts`,
  `apps/web/core/store/inbox/inbox-issue.store.ts`,
  `packages/types/src/inbox.ts`, `packages/i18n/src/locales/en/inbox.json`.
