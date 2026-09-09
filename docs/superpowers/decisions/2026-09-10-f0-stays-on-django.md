# ADR F0-3: stays_on_django batch awal (4 entri)

Date: 2026-09-10
Status: accepted
Format: docs/superpowers/decisions/2026-09-10-f0-parity-decision-format.md

## GPT proxy ×2

- `POST /api/workspaces/:slug/projects/:project_id/ai-assistant/` (`external.py:14`,
  `views/external/base.py:148-181`) dan `POST /api/workspaces/:slug/ai-assistant/`
  (`external.py:19`, `:184-212`): proxy LLM eksternal. Tidak ada handler/route Rust
  per `search.rs:22` (out of scope). FE: project-assistant tanpa caller,
  workspace-assistant via `ai.service.ts:29-35` tetap ke Django selama strangler.
- DECISION: stays on Django. Tidak dibangun di Rust.

## Legacy file-assets POST ×2

- `POST /api/workspaces/:slug/file-assets/` (`asset.py:27-31`, `base.py:38-46`) dan
  `POST /api/users/file-assets/` (`asset.py:37`, `base.py:81-86`): create legacy
  via multipart. Sengaja dikecualikan per `main.rs:1054-1057`; V2 presign覆盖 uploads.
  Repo-wide grep: tanpa FE caller.
- DECISION: stays on Django kecuali ada caller baru. GET/DELETE legacy tetap di Rust.
