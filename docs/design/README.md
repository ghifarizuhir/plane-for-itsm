---
status: current
last_verified: 2026-10-27
source_hash: sha256:8e243ec0cacf4a590d573d58cb082da3e2ef1615dbe961f7da43bf434d5c1d3d
source_paths:
  - apps/web/app/routes/core.ts
  - apps/api-rs/migrations
---

# Design Docs — Terraline

Dokumen engineering lintas domain. Semua dokumen di folder ini adalah
**current** dan harus diverifikasi ulang saat source yang dirujuk berubah.

## Reading order

| #   | Dokumen                                                          | Dependency | Isi                                                   |
| --- | ---------------------------------------------------------------- | ---------- | ----------------------------------------------------- |
| 1   | [`01-architecture.md`](./01-architecture.md)                     | —          | Monorepo, apps, packages, workspace/application model |
| 2   | [`02-data-model.md`](./02-data-model.md)                         | 01         | Tabel Postgres, migrasi, soft delete, ITSM extensions |
| 3   | [`03-api-contract.md`](./03-api-contract.md)                     | 01, 02     | Auth, scope, route families, error, rate limit        |
| 4   | [`04-design-system.md`](./04-design-system.md)                   | —          | Token, primitives, shell, motion                      |
| 5   | [`05-testing-strategy.md`](./05-testing-strategy.md)             | 01         | Rust, Django fallback, web, docs checks               |
| 6   | [`06-ops-runbook.md`](./06-ops-runbook.md)                       | 01, 02     | Local stack, prod/dev web, live, tunnel, recovery     |
| 7   | [`07-routing-and-navigation.md`](./07-routing-and-navigation.md) | 01, 03     | Route families, app rail, redirect                    |

## Source of truth

- Route web: [`../_generated/routes.md`](../_generated/routes.md)
- Route API: [`../_generated/api-routes.md`](../_generated/api-routes.md)
- Schema: [`../_generated/schema.md`](../_generated/schema.md)

Generated inventory tidak boleh diedit manual. Jalankan `pnpm docs:inventory`
setelah route, API, atau migrasi berubah.

## Documentation rules

- Setiap dokumen current memakai frontmatter dengan `status`, `last_verified`,
  `source_hash`, dan `source_paths`.
- Jangan memakai line number; rujuk path dan nama simbol.
- Jangan menyalin daftar route/endpoint/tabel; link ke inventory.
- Dokumen historis berada di [`../superpowers/`](../superpowers/README.md) dan
  bukan referensi perilaku current.

## Changelog

| Tanggal    | Perubahan                                                          |
| ---------- | ------------------------------------------------------------------ |
| 2026-09-03 | fork init                                                          |
| 2026-09-05 | cutover Rust                                                       |
| 2026-10-27 | rewrite — reading order current, generated inventories, docs rules |
