# ADR AI-1: workspace ai-assistant pindah ke Rust (deviation_accepted)

Date: 2026-09-21
Status: accepted
Format: docs/superpowers/decisions/2026-09-10-f0-parity-decision-format.md
Supersedes: ADR F0-3 (bagian GPT) untuk endpoint workspace — project-level
`ai-assistant` tetap `stays_on_django`.

## Context

`POST /api/workspaces/:slug/ai-assistant/` (`views/external/base.py:184-212`)
sebelumnya `stays_on_django` karena dianggap "external LLM proxy out of scope"
(`search.rs:22`). Setelah cutover Rust, Django tidak ada di jalur request
(Caddy `/api/*` → `api:8000`), jadi endpoint 404 dan fitur AI web mati.

## Decision

Bangun handler Rust (`routes/ai.rs::workspace_ai_assistant`) dengan kontrak
status/body 1:1 Django (400 config/task, 500 generik, 200
`{response, response_html}`), plus deviasi sengaja:

- Provider model OpenAI-compatible: `POST {LLM_BASE_URL}/chat/completions`,
  `LLM_BASE_URL` env-only default `https://api.openai.com/v1`. `LLM_PROVIDER`
  tidak dibaca; allowlist model Django dihapus (model self-hosted/custom).
- Upstream 429 → HTTP 429 `{"error": "Rate limit exceeded for <host>"}`
  (Django menelan semua error upstream jadi 500 generik; FE sudah punya toast
  429 khusus).
- `prompt` hilang/non-string → `""` dan `task` non-string → 400
  `Task is required` (Django TypeError → 500).
- `api_key` di-trim sebelum dipakai (Django mengirim verbatim) dan `base_url`
  di-trim; toleran terhadap key yang di-copy dengan newline.
- Body JSON invalid → body 400 axum (Django body parser DRF).
- Tanpa rate limit bulanan per user (tidak ada di Django OSS).

`has_llm_configured` di `/api/instances/` ikut diperbaiki jadi DB-aware
(sebelumnya env-only) supaya key yang disimpan admin AI form menyalakan gate FE.

## Consequences

- Inventory entry: `deviation_accepted` (rust_handler + ADR + notes DECISION).
- Project-level `ai-assistant` tetap `stays_on_django` (zero FE caller).
- `rephrase-grammar` (FE editor AI) tetap tidak dibangun.
- Rollback: revert kode; key DB tidak berbahaya.
