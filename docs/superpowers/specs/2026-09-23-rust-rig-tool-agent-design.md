# Rust Rig Tool-Calling Agent (Prototipe) — Design (2026-09-23)

## Tujuan

Mendemokan nilai tambah **Rig** (`rig.rs`, library agent LLM untuk Rust) di
`apps/api-rs`: endpoint AI yang ada (`routes/ai.rs`) hanya satu panggilan
`{base_url}/chat/completions` tanpa tool. Slice ini menambah endpoint baru
`POST /api/workspaces/:slug/ai-agent/` yang menjalankan **agent Rig dengan tiga
tool Rust typed** yang membaca data workspace langsung dari Postgres.

Endpoint `/ai-assistant/` existing **tidak disentuh**: kontrak parity dan
test-nya tetap apa adanya (design `2026-09-21-rust-ai-assistant-design.md`).

Keputusan brainstorming:

1. **Tujuan:** demo capability baru Rig (tool-calling agent), bukan parity.
2. **Surface:** route baru `POST /api/workspaces/:slug/ai-agent/` di modul baru
   `routes/ai_agent/`; reuse `AuthUser`, `ws_role`/`guard_am`, dan
   `resolve_llm_config()`.
3. **Provider:** OpenAI-compatible saja via `openai::CompletionsClient` Rig
   (bukan `openai::Client` default yang menembak Responses API dan tidak
   didukung OpenRouter) dengan `base_url` dari `resolve_llm_config()`
   (`LLM_BASE_URL`, deployment ini: `https://openrouter.ai/api/v1`).
4. **Tools:** tiga tool read-only ter-scope workspace — `list_projects`,
   `count_work_items`, `search_work_items`.
5. **Arsitektur:** typed tool structs (`impl Tool` Rig) yang menyimpan
   `PgPool` + `workspace_id`; runner agent dipisah sebagai fungsi yang menerima
   `ToolSet` supaya bisa dites dengan tool palsu.
6. **Testing:** unit + integration tanpa DB (fake upstream axum stateful);
   tool DB asli diverifikasi manual/live.

Non-goal: streaming, SSE, multi-agent, RAG, write tool (create/update/delete),
rate limit bulanan, perubahan FE, perubahan `parity-inventory.json` (route ini
tidak punya counterpart Django dan tidak dipakai FE), provider native
(Anthropic/Gemini key terpisah), dan perubahan `rephrase-grammar`.

## Kontrak API

- **Endpoint:** `POST /api/workspaces/:slug/ai-agent/`.
- **Auth:** `AuthUser` (tanpa token → 401). Gate workspace ADMIN/MEMBER
  (`ws_role` + `guard_am`, `routes/project.rs:12-38`, `routes/module.rs:156`);
  selain itu 403 `{"error": "You don't have the required permissions."}`.
- **Request:** `{"prompt": string}`; kosong/non-string → 400.
- **Sukses 200:** `{"response": "<teks final>", "tool_calls": [{"name": "...",
"arguments": {...}}]}`.
- **Error:**

| Kondisi                                                              | Status | Body                                                    |
| -------------------------------------------------------------------- | ------ | ------------------------------------------------------- |
| AI belum dikonfigurasi (`api_key`/`model` kosong)                    | 400    | `{"error": "AI is not configured for this workspace."}` |
| `prompt` hilang/kosong/non-string                                    | 400    | `{"error": "Prompt is required"}`                       |
| Upstream Rig 429                                                     | 429    | `{"error": "Rate limit exceeded for <host>"}`           |
| Error Rig lain (upstream non-429, timeout, JSON invalid, turn limit) | 500    | `{"error": "An internal error has occurred."}`          |

`<host>` memakai `routes::ai::host_of` yang diubah jadi `pub(crate)` (deviasi
pesan sama dengan endpoint `/ai-assistant/`).

## Alur handler & aturan bisnis

Urutan: auth → gate role → resolve `workspace_id` → resolusi config → validasi
`prompt` → build tools + client → runner → petakan hasil.

1. `AuthUser` → 401 bila tanpa token.
2. `ws_role(&pool, auth.0, &slug)`; `guard_am(role).is_err()` → `deny()` (403).
3. `SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL` → dipakai
   sebagai `workspace_id` tool; **tidak pernah** diambil dari input model/user.
4. `routes::ai::resolve_llm_config(&pool)` (fungsi existing, publik): bila
   `api_key` atau `model` kosong → 400.
5. Body `Json<Value>`: `prompt` harus string non-empty → 400.
6. Build `openai::CompletionsClient` (api_key + base_url dari config, HTTP
   client reqwest dengan timeout 60 detik), `ToolSet` berisi tiga tool (pool
   clone + `workspace_id` + trace handle), lalu `run_agent(...)`.
7. Sukses → 200 dengan `response` + `tool_calls`; error → mapping tabel di atas
   dengan `tracing::warn` (status upstream + potongan body; **tanpa API key**).

## Tools (`routes/ai_agent/tools.rs`)

Semua tool: `impl Tool` Rig dengan `Args` yang menurunkan
`Deserialize + Serialize + schemars::JsonSchema` (deskripsi field lewat doc
comment), `Output = String` berisi JSON compact, `Error` = `ToolExecutionError`
Rig sehingga kegagalan DB/validasi jadi tool error yang terbaca model (bukan
500). Setiap tool menyimpan `PgPool`, `workspace_id: Uuid`, dan
`Arc<Mutex<Vec<ToolCallTrace>>>`; trace di-push dari dalam `call()` (tidak
bergantung API hook Rig).

| Tool                | Args                                                               | Output                                                           |
| ------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `list_projects`     | –                                                                  | `{"items":[{"identifier","name"}]}` (maks 50, non-archived)      |
| `count_work_items`  | `project?`, `state_group?`, `priority?`, `include_archived?=false` | `{"count":N,"filters":{...}}`                                    |
| `search_work_items` | `project?`, `state_group?`, `priority?`, `query?`, `limit?=10`     | `{"items":[{"identifier","name","state","priority","project"}]}` |

Aturan:

- **Scoping:** semua SQL memfilter `workspace_id = $1` dari handler; model tidak
  punya parameter workspace.
- **SQL statis + filter opsional nullable** (`$n::text IS NULL OR ...`) — tanpa
  dynamic SQL; join `projects` (filter `project` cocok `identifier` atau `name`
  ILIKE) dan `LEFT JOIN states` (filter `states."group"`, ambil `states.name`).
  `deleted_at IS NULL` selalu; `archived_at IS NULL` kecuali
  `include_archived=true`. Semua nilai dari model di-bind sebagai parameter.
- **Allowlist:** `state_group` ∈ backlog/unstarted/started/completed/cancelled,
  `priority` ∈ urgent/high/medium/low/none → invalid = tool error.
- **Batas:** `limit` di-clamp 1–25 (default 10); `list_projects` LIMIT 50.
- `identifier` hasil = `projects.identifier || '-' || issues.sequence_id`.
- Read-only; tidak ada tool yang menulis.

Skema yang dirujuk (`migrations/0001_initial.sql`): `issues:1490` (`workspace_id`,
`project_id`, `state_id`, `priority`, `sequence_id`, `name`, `archived_at`,
`deleted_at`), `projects:2014` (`identifier`, `name`, `archived_at`),
`states:2116` (`"group"`, `name`, `deleted_at`).

## Runner (`routes/ai_agent/mod.rs`)

```rust
pub struct ToolCallTrace { pub name: String, pub arguments: serde_json::Value }

pub async fn run_agent(
    client: &openai::CompletionsClient,
    model: &str,
    preamble: &str,
    tools: ToolSet,                       // produksi: 3 tool DB; test: tool palsu
    prompt: &str,
    trace: Arc<Mutex<Vec<ToolCallTrace>>>,
) -> Result<String, routes::ai::LlmError>
```

- Batas turn agent dinaikkan ke 6 memakai API turn-limit Rig pada versi yang
  di-pin; bila tidak tersedia, pakai default Rig dan catat di kode.
- Preamble: asisten workspace read-only; wajib memakai tool untuk pertanyaan
  faktual soal project/work item; dilarang mengarang identifier; sebut kosong
  bila tool tidak menemukan apa pun; jawab ringkas dalam bahasa user.
- Error Rig dipetakan: status HTTP 429 (via helper inspeksi provider response
  Rig) → `LlmError::RateLimited`; sisanya → `LlmError::Upstream`.

## Dependency

- `apps/api-rs/Cargo.toml` `[workspace.dependencies]`: `rig` (root facade) dan
  `schemars = "1"`; **versi Rig di-pin exact** (0.x masih bergerak; maintainer
  Rig sendiri mewanti breaking changes) dan `Cargo.lock` di-commit.
- `crates/api/Cargo.toml`: `rig = { workspace = true }`,
  `schemars = { workspace = true }` mengikuti gaya repo.
- **Langkah pertama implementasi adalah spike kompilasi + test round-trip**:
  verifikasi versi pinned terhadap Rust 1.96, jalur `CompletionsClient`,
  penyerahan `ToolSet` ke AgentBuilder (`tool_server_handle`/`dynamic_tools`
  sebagai fallback, atau enum wrapper `AgentTool`), bentuk trait `Tool`
  (`call` beserta parameternya), dan API turn limit.

## Testing

1. **Unit `tools.rs`:** setiap SQL memuat `workspace_id = $1`; clamp `limit`
   1–25; validasi allowlist `state_group`/`priority`; `project`/`query` selalu
   jadi bind param; format output JSON.
2. **Unit `mod.rs`:** parsing body (`prompt` kosong/non-string → 400), preamble
   non-kosong, mapping error Rig (bila error bisa dikonstruksi) — jika tidak,
   dipindah ke integration.
3. **Integration `tests/ai_agent_test.rs` (tanpa DB):** fake upstream axum
   stateful — request #1 balas `tool_calls` untuk tool palsu, request #2
   memastikan tool result ada di body lalu balas teks final; assert `response`,
   trace `tool_calls`, dan mapping 429/500/JSON invalid. Client Rig diarahkan
   ke fake `base_url`.
4. **Gate:** `cargo test -p api`, `cargo fmt --check`, `cargo clippy -p api`
   (bila bersih). `route_inventory_test` aman karena gate hanya mengecek arah
   inventory → `main.rs`; route ini sengaja tidak masuk inventory.
5. **Live smoke manual:** curl dengan token + key terkonfigurasi (model
   OpenRouter yang mendukung tool calling). Negatif: tanpa token 401,
   non-member 403, prompt kosong 400, AI belum dikonfigurasi 400.

## Rollout & rollback

1. `docker compose build api` lalu `docker compose up -d api`; tanpa migrasi DB,
   tanpa perubahan FE.
2. Rollback: revert commit + rebuild (route hilang, tidak ada state yang
   tertinggal).
3. Catatan: build time/ukuran binary naik karena Rig; API key tidak pernah
   di-log; permukaan prompt injection dibatasi (tool read-only + scoped
   workspace + limit hasil).

## Risiko

- **API Rig 0.x bergerak** (pernah pindah `call(&self, args)` →
  `call(&self, &mut ToolContext, args)`, split `rig-core`/`rig-agent`, error
  enum `#[non_exhaustive]`). Mitigasi: pin exact + lockfile + spike test di
  langkah pertama; trace tidak bergantung hook.
- **Model OpenRouter harus support tool calling**; kalau tidak, agent balas
  teks tanpa tool (terlihat dari `tool_calls` kosong).
- Nama/kolom DB diverifikasi ke migrasi saat implementasi (`states."group"`,
  `archived_at`).

## File map

- Create: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` — handler,
  preamble, runner, mapping error, unit test.
- Create: `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs` — tiga tool,
  SQL statis, validasi, unit test.
- Create: `apps/api-rs/crates/api/tests/ai_agent_test.rs` — integration fake
  upstream.
- Modify: `apps/api-rs/Cargo.toml`, `apps/api-rs/crates/api/Cargo.toml` — dep
  `rig` + `schemars`.
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` — daftarkan `ai_agent`.
- Modify: `apps/api-rs/crates/api/src/main.rs` — route
  `POST /api/workspaces/:slug/ai-agent/`.
- Modify: `apps/api-rs/crates/api/src/routes/ai.rs` — `host_of` jadi
  `pub(crate)` (tanpa perubahan perilaku).
- Modify: `apps/api-rs/Cargo.lock` — hasil `cargo build`.

## Referensi

- Design endpoint existing: `docs/superpowers/specs/2026-09-21-rust-ai-assistant-design.md`.
- Implementasi existing: `apps/api-rs/crates/api/src/routes/ai.rs`,
  `apps/api-rs/crates/api/tests/ai_test.rs`.
- Rig: https://rig.rs/docs, docs.rs `rig` / `rig-core` / `rig-agent`.
- Skema DB: `apps/api-rs/migrations/0001_initial.sql`.
