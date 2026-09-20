# Rust AI Assistant (OpenAI-compatible) — Design (2026-09-21)

## Tujuan

Fitur AI (popover GPT assistant di issue modal + tombol "I'm feeling lucky" /
auto-generate description) mati di prod: frontend memanggil
`POST /api/workspaces/:slug/ai-assistant/`, tapi Caddy mengarahkan seluruh
`/api/*` ke `api:8000` (api-rs) sementara Django `api-legacy` opt-in dan tidak
ada di jalur request — jadi endpoint-nya 404 (`fallback_404`, `main.rs:1781`).

Frontend sudah menunjuk satu base URL (`VITE_API_BASE_URL` → Rust), jadi **tidak
ada perubahan URL di FE**; endpoint akan hidup begitu Rust mengimplementasikannya.
Entri `parity-inventory.json` domain `external` menandai dua endpoint AI sebagai
`stays_on_django` (ADR F0-3) dan FE-nya "toleran 404" — slice ini membalik
keputusan itu untuk endpoint workspace.

Keputusan brainstorming:

1. **Provider model:** OpenAI-compatible (`POST {base_url}/chat/completions`)
   dengan base URL dari env `LLM_BASE_URL` (default
   `https://api.openai.com/v1`). Satu jalur kode untuk OpenAI, OpenRouter,
   Groq, Ollama, vLLM, dst.
2. **Scope endpoint:** hanya workspace `ai-assistant`. Project-level
   `ai-assistant` dan `rephrase-grammar` tetap tidak dibangun.
3. **Base URL env-only** — tanpa row `instance_configurations` baru, tanpa
   perubahan admin UI (menghindari masalah seeding karena `configure_instance`
   Django hanya legacy).
4. **Pendekatan A:** modul baru `routes/ai.rs` (bukan menambah `prefs.rs`).
5. **429 upstream dipetakan ke HTTP 429** (deviasi kecil, memanfaatkan toast
   429 yang sudah ada di FE); error upstream lain tetap 500 generik (parity).

Non-goal slice ini: streaming, `rephrase-grammar`, project-level AI, rate limit
50 req/bulan (fitur cloud, tidak ada di Django OSS), field provider di admin UI,
Unsplash.

## Kontrak API

- **Endpoint:** `POST /api/workspaces/:slug/ai-assistant/`
  (`external.py:19`, `views/external/base.py:184-212`).
- **Auth:** `AuthUser` (tanpa token → 401). Gate workspace ADMIN/MEMBER
  (`allow_permission(level="WORKSPACE")`, `permissions/base.py:44-51,81-84`):
  role 20/15 → lolos; selain itu 403
  `{"error": "You don't have the required permissions."}`.
- **Request:** `{ "task": string, "prompt": string }` — `prompt` opsional.
- **Sukses 200:** `{"response": text, "response_html": text.replace("\n","<br/>")}`.
- **Error (byte-exact parity):**

| Kondisi                                                  | Status | Body                                                       |
| -------------------------------------------------------- | ------ | ---------------------------------------------------------- |
| API key / model tidak terkonfigurasi                     | 400    | `{"error": "LLM provider API key and model are required"}` |
| `task` hilang/kosong                                     | 400    | `{"error": "Task is required"}`                            |
| Gagal upstream (non-429), timeout, JSON upstream invalid | 500    | `{"error": "An internal error has occurred."}`             |
| Upstream 429                                             | 429    | `{"error": "Rate limit exceeded for <host>"}`              |

`<host>` = host dari `LLM_BASE_URL` yang dipakai (deviasi dari Django yang
memakai `LLM_PROVIDER`; provider tidak lagi dibaca).

## Alur handler & aturan bisnis

Urutan: gate role → resolusi config → validasi `task` → panggil upstream →
petakan respons.

### Resolusi konfigurasi

`resolve_llm_config(pool)` mencerminkan `get_configuration_value`
(`license/utils/instance_value.py:17-39`):

| Key            | `SKIP_ENV_VAR=1` (default)                                            | `SKIP_ENV_VAR=0`  |
| -------------- | --------------------------------------------------------------------- | ----------------- |
| `LLM_API_KEY`  | row `instance_configurations` (decrypt bila `is_encrypted`), else env | env langsung      |
| `LLM_MODEL`    | row DB, else env, else `gpt-4o-mini`                                  | env, else default |
| `LLM_BASE_URL` | env saja, default `https://api.openai.com/v1`                         | env saja, default |

Pola baca DB + Fernet mengikuti `instance_admin::load_email_config`
(`instance_admin.rs:1584-1605`; `skip_env_vars()` `:1543`, `fernet_secret()`
`:1231`, `decrypt_data` `:1353`). Mapping row → config dipisah sebagai fungsi
murni `llm_config_from_rows(...)` supaya bisa di-unit-test tanpa DB.

`LLM_PROVIDER` tidak dibaca dan allowlist model dihapus — model dipakai verbatim
supaya model self-hosted/custom bisa dipakai.

### Perbaikan `has_llm_configured`

`routes/instance.rs:54` saat ini hanya membaca env
(`!env_str_or("LLM_API_KEY","").is_empty()`), padahal Django menghitung
`bool(LLM_API_KEY)` dari DB (`license/api/views/instance.py:139`). Akibatnya key
yang disimpan lewat admin AI form (PATCH `/api/instances/configurations/`, sudah
ada di Rust) tidak pernah menyalakan gate FE
(`description-editor.tsx:247,266`). Slice ini mengganti sumbernya ke
`resolve_llm_config` yang sama — **hanya `api_key` non-empty yang dicek**,
`model`/`base_url` tidak ikut menentukan (parity `bool(LLM_API_KEY)`).

### Panggilan upstream

- `pub async fn chat_completion(base_url, api_key, model, prompt) -> Result<String, LlmError>`
  — `base_url` eksplisit sebagai parameter supaya test tidak menyentuh env proses.
- `reqwest::Client` dibagikan lewat `OnceLock` (pola `s3proxy.rs::http()`),
  timeout total 60 detik.
- URL: `{base_url tanpa trailing '/'}/chat/completions`.
- Header: `Authorization: Bearer <key>`, `Content-Type: application/json`.
- Body: `{"model": model, "messages":[{"role":"user","content":"{task}\n{prompt}"}]}`
  (Django: `task + "\n" + prompt`, `base.py:125`).
- Ekstraksi: `choices[0].message.content`; hilang/kosong → `""` + 200 (FE
  menampilkan pesan "title isn't informative enough", `description-editor.tsx:130-136`).
- Logging: status upstream + potongan body lewat `tracing::warn`; **tanpa** API key.

### Deviasi/skip terdokumentasi

- `LLM_PROVIDER` diabaikan; allowlist model dihapus (base URL menentukan vendor).
- `prompt` hilang → `""`; Django TypeError → 500 (`base.py:125`).
- `prompt` non-string (mis. number) → `""`; Django TypeError → 500.
- `task` non-string → 400 `"Task is required"`; Django TypeError → 500.
- JSON body invalid → body 400 milik axum (Django memakai body parser DRF).
- Upstream 429 → 429 (Django menelan jadi 500 generik).
- Tanpa rate limit bulanan per user (tidak ada di kode OSS Django).
- Upstream non-2xx selain 429 → 500 generik (parity).

## Perubahan frontend

**Tidak ada.** Web sudah memakai base URL Rust dan service-nya
(`apps/web/core/services/ai.service.ts:29-35`, `createGptTask`) sudah menunjuk
path yang benar. Setelah route + fix `has_llm_configured`, dua permukaan AI yang
ada langsung berfungsi: `gpt-assistant-popover.tsx:110` dan
`description-editor.tsx:123`. Editor AI (`pages/editor/ai`) dibiarkan apa adanya
sesuai keputusan brainstorming.

## Testing

1. **Unit (cargo) di `ai.rs`:** join URL (trailing slash), build body, ekstraksi
   content (missing/empty), mapping 429 vs 500, `llm_config_from_rows` (flag
   encrypted, row hilang → env default, model kosong → `gpt-4o-mini`), gate
   `guard_am` (20/15 lolos, 5/None tolak).
2. **Integration `tests/ai_test.rs`:** server fake OpenAI-compatible di
   `127.0.0.1:0` (axum sudah dependency) — sukses, 429, JSON invalid, content
   kosong.
3. **Smoke live (`apps/api-rs/scripts/smoke.sh`):** cek `POST ai-assistant/`
   **bukan 404** (terima 400/200/500 sesuai konfigurasi stack).
4. **Gate parity:** `cargo test -p api` — `route_inventory_test`
   (`deviation_accepted` wajib terdaftar di `main.rs`, ADR resolve, notes memuat
   `DECISION`) dan `fe_tripwire_test` (evidence `createGptTask` tetap cocok).
5. **Live manual (opsional):** dengan key terkonfigurasi, POST dari UI issue
   modal → respons tampil; negatif: tanpa key → gate FE menyembunyikan tombol.

## Rollout

1. `docker compose build api` (image `plane-api-rs:local`; worker/beat pakai
   image sama) lalu `docker compose up -d api worker beat-worker`; cek `/health`.
2. Set `LLM_API_KEY` + `LLM_MODEL` lewat admin AI form (sudah jalan di Rust)
   atau env; set `LLM_BASE_URL` di `apps/api/.env` bila bukan OpenAI; restart
   `api`.
3. Tanpa migrasi DB.

**Rollback:** revert kode + rebuild; key di DB tidak berbahaya (endpoint kembali
404 seperti sebelumnya).

## File map

- Create: `apps/api-rs/crates/api/src/routes/ai.rs` — handler, resolusi config,
  `chat_completion`, unit test.
- Create: `apps/api-rs/crates/api/tests/ai_test.rs` — integration fake upstream.
- Create: `docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md` — ADR
  (supersede bagian GPT di ADR F0-3 untuk endpoint workspace).
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` — daftarkan modul.
- Modify: `apps/api-rs/crates/api/src/main.rs` — route
  `POST /api/workspaces/:slug/ai-assistant/`.
- Modify: `apps/api-rs/crates/api/src/routes/instance.rs` — `has_llm_configured`
  dari `resolve_llm_config`.
- Modify: `apps/api-rs/crates/api/parity-inventory.json` — domain `ai` baru;
  workspace entry `deviation_accepted` (+ `rust_handler`, ADR, notes `DECISION`);
  project entry tetap `stays_on_django` (ADR F0-3); entri AI keluar dari domain
  `external`.
- Modify: `docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md` — tandai
  bagian GPT superseded untuk endpoint workspace.
- Modify (komentar/boundary list): `routes/search.rs:22`,
  `docker-compose.yml:37-38`, `docs/design/01-architecture.md:141`,
  `docs/business-capabilities.md:16`.
- Modify (opsional): `apps/api-rs/scripts/smoke.sh` — cek not-404.

Referensi: ADR `docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md`,
plan `docs/superpowers/plans/2026-09-06-batch-e-workspace-platform-parity.md:92`
(baris "explicitly OUT" untuk AI), `parity-inventory.json` domain `external`.
