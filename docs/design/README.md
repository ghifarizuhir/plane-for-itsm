# Design Docs — Plane for ITSM

Dokumen actual Plane upstream (bukan forward-looking ITSM). Semua keputusan di sini adalah **snapshot kode** — update ketika kode berubah. Struktur adaptasi dari `terra/docs/design/README.md:2`, isi actual-only; propose ITSM di `features/_backlog.md`.

---

## Reading order

Urutan dari fondasi ke cross-cutting:

| #   | Doc                                                  | Status   | Dependency | Deskripsi                                                     |
| --- | ---------------------------------------------------- | -------- | ---------- | ------------------------------------------------------------- |
| 1   | [`01-architecture.md`](./01-architecture.md)         | ✅ Draft | —          | Monorepo pnpm+Turbo, 6 apps, 15 packages, dependency rules    |
| 2   | [`02-data-model.md`](./02-data-model.md)             | ✅ Draft | 01         | Django ORM (`plane.db.models.*`) + Postgres + ITSM extensions |
| 3   | [`03-api-contract.md`](./03-api-contract.md)         | ✅ Draft | 01, 02     | DRF + Session auth + pagination/filter/sort/error shape       |
| 4   | [`04-design-system.md`](./04-design-system.md)       | ✅ Draft | —          | @plane/ui + tailwind-config + tokens + icon                   |
| 5   | [`05-testing-strategy.md`](./05-testing-strategy.md) | ✅ Draft | 01         | pytest + Vitest + Playwright (deferred)                       |
| 6   | [`06-ops-runbook.md`](./06-ops-runbook.md)           | ✅ Draft | 01, 02     | docker-compose + env + deploy + backup                        |

> Terra punya 13 doc (`terra/docs/design/README.md:11` — termasuk 01-erd, 09-realtime, 12-mcp, 13-release-versioning). Plane-for-itsm **ramping jadi 6** — yang tidak relevan (MCP, erd `entities` JSONB) dihapus; yang Plane-specific (Django, MobX, Live) digabung di `01-architecture`.

---

## Core decisions (snapshot)

Rangkuman keputusan besar yang tersebar di dokumen:

| Area        | Keputusan                                                                                                          | Doc    |
| ----------- | ------------------------------------------------------------------------------------------------------------------ | ------ |
| Monorepo    | **pnpm workspaces + Turbo** (`pnpm-workspace.yaml:1`, `turbo.json:1`); exclude `apps/api` + `apps/proxy` dari pnpm | 01     |
| Backend     | **Django 5 + DRF + Celery** (`plane/settings/common.py:97`) — bukan Express                                        | 01, 02 |
| Frontend    | **React 19 + React Router 8 + Vite + Tailwind 4 + MobX 6 + SWR + TipTap 2** (`apps/web/package.json:67`)           | 01     |
| Database    | **Postgres** (`dj_database_url` — `plane/settings/common.py:204`) — single dialect, no SQLite                      | 02     |
| Editor      | **TipTap 2 + Yjs + Hocuspocus** (`packages/editor`, `apps/live`)                                                   | 01, 04 |
| Realtime    | **apps/live (Express+ws)** + Hocuspocus — bukan SSE ticket                                                         | 01     |
| Lint/format | **Oxlint + oxfmt** (`package.json:31`, `docs/linting.md:1`) — 50-100x faster than ESLint                           | 05     |
| Auth        | **SessionAuthentication** + cookie `session-id` (`plane/settings/common.py:139,374`) + OAuth social                | 03     |
| ITSM model  | Django per-table models (bukan `entities` JSONB Terra) — `Incident`/`Problem`/etc. via IssueType atau tabel baru   | 02     |

Detail + rationale ada di doc masing-masing.

---

## Conventions untuk doc baru di folder ini

1. **Numbering monotonic.** Nomor doc naik, tidak pernah di-reuse. Doc baru di tengah boleh numbering tidak sekuensial.
2. **"Resolved Decisions" section wajib** di akhir doc — catat keputusan + alasan sebagai record.
3. **Status header eksplisit** di awal: `Status: **Draft** | **Stable** | **Superseded**`.
4. **Cross-reference relative** (`./01-architecture.md`, bukan path absolut).
5. **Tidak ada code generator.** Kode di doc adalah contoh/kontrak, bukan source of truth — source ada di repo.

---

## Content Boundary

Aturan apa taruh di mana di-define di [`../features/README.md`](../features/README.md) §Content Boundary. Berlaku cross-folder termasuk `design/`.

Ringkasan untuk `design/`:

- **§Open Items** = live questions, hapus saat resolved
- **§Resolved Decisions** = keputusan permanen + rationale
- **Parked engineering ideas** = `design/README.md` §Open Items atau doc spesifik
- **Review discussion** = chat / PR, bukan docs
- **Change history** = git log + `## Changelog` per file (append row, terbaru di bawah)

---

---

## Changelog

| Date       | Change                                                |
| ---------- | ----------------------------------------------------- |
| 2026-09-03 | fork init — ramping dari 13 terra docs → 6 plane docs |
