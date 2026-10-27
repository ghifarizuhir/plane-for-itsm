---
status: current
last_verified: 2026-10-27
source_hash: sha256:75cf2c25b2e710740a80af0e2eedd123c9c2e49db9cf1a0e9b696123402f0ec0
source_paths:
  - package.json
  - pnpm-workspace.yaml
  - apps/web/app/routes/core.ts
  - apps/api-rs/migrations
---

# Cross-Cutting Decisions — Cheat Sheet

Rangkuman keputusan arsitektur dan produk. Dokumen ini adalah reference cepat,
bukan pengganti dokumen spesifik. Jika ada konflik, dokumen di `design/` dan
inventory generated yang menang.

## Stack

| Layer        | Pilihan                                                 |
| ------------ | ------------------------------------------------------- |
| Monorepo     | pnpm workspaces + Turbo                                 |
| Bahasa       | TypeScript strict + Rust                                |
| Web          | React 19, React Router 8, Vite, Tailwind 4, MobX 6, SWR |
| Editor       | TipTap 2 + Yjs + Hocuspocus                             |
| API utama    | Rust Axum + SQLx di `apps/api-rs`                       |
| API fallback | Django 5 + DRF di `apps/api` (opt-in)                   |
| Database     | PostgreSQL                                              |
| Queue/job    | Redis Stream `plane:jobs`                               |
| Realtime     | `apps/live` (Express + WebSocket + Hocuspocus)          |
| Lint/format  | OxLint + oxfmt                                          |
| Test         | cargo test, pytest baseline, Vitest/RTL                 |
| Deploy lokal | Docker Compose untuk backend; web via pnpm/Turbo        |

## Monorepo layout

Tujuh aplikasi:

| App           | Peran                          |
| ------------- | ------------------------------ |
| `apps/web`    | SPA utama, port 3000           |
| `apps/admin`  | God-mode admin, port 3001      |
| `apps/live`   | Kolaborasi realtime, port 3100 |
| `apps/space`  | Halaman publik Space           |
| `apps/api-rs` | API, worker, dan beat utama    |
| `apps/api`    | Django fallback                |
| `apps/proxy`  | Reverse proxy                  |

Lima belas package:

| Package             | Peran                          |
| ------------------- | ------------------------------ |
| `ui`                | Design system dan komponen     |
| `editor`            | Editor TipTap dan ekstensi     |
| `types`             | Tipe TypeScript bersama        |
| `shared-state`      | Store MobX                     |
| `services`          | Client API                     |
| `constants`         | Konstanta dan enum             |
| `i18n`              | Locale dan terjemahan          |
| `hooks`             | Hook React bersama             |
| `utils`             | Utility bersama                |
| `logger`            | Logging                        |
| `decorators`        | Decorator bersama              |
| `propel`            | Ikon dan primitives            |
| `codemods`          | Codemod internal               |
| `tailwind-config`   | Token dan konfigurasi Tailwind |
| `typescript-config` | Konfigurasi TypeScript         |

## Scope model

```text
Workspace
├── Services                 (katalog lintas application)
├── Intake                   (triage queue)
├── Sprints / Tracks         (agregat workspace)
├── Knowledge / Pages        (page tree workspace)
├── Workspace Views          (queue lintas application)
├── Settings                 (workspace configuration)
└── Applications             (project)
    ├── Tickets              (work items)
    ├── Sprints / Tracks
    ├── Pages
    ├── Intake
    ├── Views
    ├── War Rooms
    └── Archives
```

- Workspace adalah tenant dan batas data utama.
- Application adalah nama user-facing untuk project.
- Ticket dimiliki tepat satu application.
- Service adalah entitas workspace dan dapat dihubungkan sebagai owner atau
  consumer application.

## Backend

- `apps/api-rs` adalah API utama sejak cutover Rust.
- Handler tipis, validator murni, query SQLx.
- Pekerjaan async berjalan di worker melalui Redis Stream.
- `apps/api` dipertahankan untuk boundary yang belum diport; jangan menambah
  fitur baru ke Django tanpa alasan migrasi.
- Migrasi schema ada di `apps/api-rs/migrations/`; Django models menjadi
  referensi historis.

Detail: [`design/01-architecture.md`](design/01-architecture.md).

## Data

- PostgreSQL per-table, bukan satu tabel `entities` JSONB.
- Migrasi `0001` sampai `0025` mencakup baseline, services, war rooms, AI,
  intake sources, penghapusan workflow, workspace services, taxonomy, metrics,
  dan service kind.
- Soft delete memakai `deleted_at`; archive dan restore tersedia per entity.
- Tabel aktual: [`_generated/schema.md`](_generated/schema.md).

Detail: [`design/02-data-model.md`](design/02-data-model.md).

## Auth and permissions

- API utama memakai `X-Api-Key` dan Bearer token.
- Session cookie dan CSRF hanya berlaku di Django fallback.
- Hirarki: instance admin → workspace admin → application admin → member.
- Guest dapat dibatasi per application.
- God-mode berjalan di `apps/admin` pada `/god-mode/`.

Detail: [`design/03-api-contract.md`](design/03-api-contract.md).

## Routing

Route families:

- Workspace: `/:workspaceSlug/services`, `/work-items`, `/intake`, `/cycles`,
  `/modules`, `/pages`, `/analytics/:tabId`, `/scheduler`, `/notifications`,
  `/docs`, `/galileo`, `/settings`.
- Application: `/:workspaceSlug/projects/:projectId/issues`,
  `/cycles`, `/modules`, `/pages`, `/intake`, `/war-rooms`, `/views`,
  `/archives`.
- Canonical ticket URL: `/:workspaceSlug/browse/:workItem`.

Legacy redirects tetap didaftarkan di `apps/web/app/routes/core.ts`. Daftar
lengkap: [`_generated/routes.md`](_generated/routes.md). Detail arsitektur
routing ada di `design/07-routing-and-navigation.md`.

## Testing and ops

- Rust: `cargo test` di `apps/api-rs`; suite scratch tertentu harus serial.
- Django: pytest via `docker-compose-test.yml`.
- Web/packages: Vitest + RTL.
- Docs: `pnpm docs:test`, `pnpm docs:verify`, `pnpm docs:check`.

Detail: [`design/05-testing-strategy.md`](design/05-testing-strategy.md) dan
[`design/06-ops-runbook.md`](design/06-ops-runbook.md).

## Changelog

| Tanggal    | Perubahan                                           |
| ---------- | --------------------------------------------------- |
| 2026-09-03 | fork init                                           |
| 2026-09-05 | cutover Rust                                        |
| 2026-10-27 | rewrite — stack, scope, data, auth, routing current |
