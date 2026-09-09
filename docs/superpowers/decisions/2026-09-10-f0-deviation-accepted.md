# ADR F0-2: deviation_accepted batch awal (3 entri murni)

Date: 2026-09-10
Status: accepted
Format: docs/superpowers/decisions/2026-09-10-f0-parity-decision-format.md

Aturan: hanya entri yang SEMUA deltanya adalah bug Django / sane-mapping.
Entri campuran (bug + KEYS/GATE/METHOD) tetap `shape_mismatch`.

## 1. `DELETE /api/workspaces/:slug/members/:pk/`

- Django `app/views/workspace/member.py:122-141` (esp `:128`): guard
  sole-project-admin membandingkan `project_projectmember__member_id=workspace_member.id`
  (ROW id, bukan user id) → hanya fire saat UUID coincidence → dead code → Django lanjut 204.
- Rust `member.rs:434-449,953-960`: pakai user id → 400 `SOLE_PROJ_ADMIN_USER_MSG` verbatim.
- DECISION: pertahankan Rust. GET/PATCH sudah verified sama. FE `workspace.service.ts:144-157`
  - `member.service.ts:62-78` tidak bergantung pada quirk 204.

## 2. `GET /api/users/file-assets/:key/`

- Django `app/views/asset/base.py:73` serialize queryset TANPA `many=True` di atas
  plain `ModelSerializer` (`serializers/asset.py:9-13`) → `AttributeError` → 500
  `Something went wrong` via `views/base.py:101-109`.
- Rust `asset.rs:2252-2258`: 200 `{data:single,status:true}`.
- DECISION: pertahankan Rust. GET-miss quirk, DELETE, scope verified sama.
  GET tanpa FE caller (`file.service.ts` hanya DELETE).

## 3. `GET /api/unsplash/`

- Django `views/external/base.py:235`: bug `page=${page}` (literal `$`).
  Rust `prefs.rs:258-264` fixed. Upstream unreachable/non-JSON: Django 500
  (raised) vs Rust 502 `{error}` (`prefs.rs:1644-1663`) — sane mapping.
- DECISION: pertahankan Rust. `[]` tanpa key, passthrough, auth-only verified sama.
