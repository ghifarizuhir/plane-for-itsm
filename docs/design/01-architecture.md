---
status: current
last_verified: 2026-10-27
source_hash: sha256:35aca197e43e48f574a8519a5bdedcdd250c0443cac0155a8daf6516d80be1fa
source_paths:
  - package.json
  - pnpm-workspace.yaml
  - turbo.json
  - apps/web/package.json
  - apps/live/package.json
  - apps/api-rs/Cargo.toml
---

# 01 — Architecture

Status: **Current.**

Dokumen ini menjelaskan bentuk monorepo dan batas antar komponen. Route web
didaftarkan di [`../_generated/routes.md`](../_generated/routes.md), route API di
[`../_generated/api-routes.md`](../_generated/api-routes.md), dan tabel database
di [`../_generated/schema.md`](../_generated/schema.md).

## Design principles

1. **Workspace sebagai tenant.** Semua data operasional berakar pada workspace.
2. **Application sebagai batas kepemilikan kerja.** Application adalah nama
   user-facing untuk project; ticket dimiliki tepat satu application.
3. **Service bersifat workspace-level.** Satu service dapat dimiliki satu
   application dan dipakai application lain.
4. **Postgres adalah source of truth.** Schema didefinisikan oleh migrasi SQLx;
   Django models hanya referensi historis dan fallback.
5. **Handler tipis.** Handler Axum memvalidasi input, memanggil query/validator,
   lalu serialisasi response. Pekerjaan async berjalan di worker.
6. **TypeScript strict dan Rust typed.** Tidak ada `any` baru; error API
   diterjemahkan ke tipe domain.
7. **Feature colocation di web.** Store, komponen, dan route per domain berada
   dekat satu sama lain.

## Applications

| App           | Peran                                                             | Port default          |
| ------------- | ----------------------------------------------------------------- | --------------------- |
| `apps/web`    | SPA utama: workspace, application, ticket, service, knowledge, AI | 3000                  |
| `apps/admin`  | God-mode instance admin                                           | 3001                  |
| `apps/live`   | Kolaborasi realtime Hocuspocus/Yjs                                | 3100                  |
| `apps/space`  | Halaman publik Space                                              | —                     |
| `apps/api-rs` | API Axum, worker, dan beat                                        | 8000                  |
| `apps/api`    | Django + DRF fallback                                             | 8000 (profile legacy) |
| `apps/proxy`  | Reverse proxy                                                     | —                     |

`apps/api-rs` adalah API utama. `apps/api` dipertahankan untuk boundary yang
belum diport dan tidak boleh menjadi tempat fitur baru tanpa rencana migrasi.

## Packages

| Package               | Peran                               |
| --------------------- | ----------------------------------- |
| `@plane/ui`           | Komponen, token, dan primitives     |
| `@plane/editor`       | TipTap, ekstensi, dan integrasi Yjs |
| `@plane/types`        | Tipe TypeScript bersama             |
| `@plane/shared-state` | Store MobX                          |
| `@plane/services`     | Client API                          |
| `@plane/constants`    | Enum dan konstanta                  |
| `@plane/i18n`         | Locale dan terjemahan               |
| `@plane/hooks`        | Hook React                          |
| `@plane/utils`        | Utility                             |
| `@plane/logger`       | Logging                             |
| `@plane/decorators`   | Decorator bersama                   |
| `@plane/propel`       | Ikon dan primitives                 |
| `codemods`            | Codemod internal                    |
| `tailwind-config`     | Token dan konfigurasi Tailwind      |
| `typescript-config`   | Konfigurasi TypeScript              |

Jumlah package dapat berubah; inventory dan `packages/` adalah sumber aktual.

## Workspace and application model

```text
Workspace
├── services, intake, sprints, tracks, pages, views, settings
└── applications
    ├── tickets (work items)
    ├── application-scoped views, cycles, modules, pages, intake
    ├── war rooms
    └── archives
```

- **Workspace** adalah tenant dan scope query utama.
- **Application** adalah project yang dilihat user. Application memiliki
  identifier, member, state, label, estimate, dan work item type.
- **Ticket** adalah work item yang dimiliki satu application.
- **Service** adalah katalog workspace; application dapat menjadi owner atau
  consumer.

## Backend

- Framework: Axum + SQLx.
- Auth: `X-Api-Key` dan Bearer.
- Job queue: Redis Stream `plane:jobs`.
- Worker dan beat berjalan sebagai proses terpisah dari API.
- Migrasi dijalankan saat startup API melalui SQLx migrate.
- API v1 berada di `/api/v1/` untuk akses publik/MCP; API workspace berada di
  `/api/workspaces/:slug/`.

## Frontend

- React 19 + React Router 8 + Vite.
- State global: MobX di `packages/shared-state` dan store domain di
  `apps/web/core/store`.
- Data fetching: SWR untuk query dan mutasi ringan.
- Editor: TipTap 2, Yjs, dan Hocuspocus.
- Navigasi memakai app rail dengan mode Work, Procedure, Galileo, dan Settings.
- Application scope selector disimpan per workspace dan dipakai bersama oleh
  halaman agregat.

## Realtime and AI

- `apps/live` menangani kolaborasi dokumen dan relay event realtime.
- Redis menjadi transport event dan presence.
- Galileo memakai route AI di `apps/api-rs/crates/api/src/routes/ai*.rs` dan
  crate `apps/api-rs/crates/ai`.
- Scheduler AI berjalan dari route dan worker yang sama.

## Dependency rules

- `apps/*` tidak boleh mengimpor `apps/*` lain.
- `packages/*` tidak boleh mengimpor `apps/*`.
- `apps/*` mengimpor package internal lewat `workspace:*`.
- Dependency eksternal memakai `catalog:` di `pnpm-workspace.yaml`.
- Perubahan schema harus lewat migrasi baru; jangan mengedit migrasi yang sudah
  dirilis.

## Source of truth

- Route web: [`../_generated/routes.md`](../_generated/routes.md)
- Route API: [`../_generated/api-routes.md`](../_generated/api-routes.md)
- Schema: [`../_generated/schema.md`](../_generated/schema.md)
- Data model: [`02-data-model.md`](./02-data-model.md)
- API contract: [`03-api-contract.md`](./03-api-contract.md)

## Changelog

| Tanggal    | Perubahan                                                                 |
| ---------- | ------------------------------------------------------------------------- |
| 2026-09-03 | fork init                                                                 |
| 2026-09-05 | cutover Rust                                                              |
| 2026-10-27 | rewrite — tujuh app, workspace/application model, AI dan realtime current |
