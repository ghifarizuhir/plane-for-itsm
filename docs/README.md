---
status: current
last_verified: 2026-10-27
source_hash: sha256:a3c0aee78922ec89e83669255eda6eda8b6bd3e1b6ce1e6b88c4a73287c75c55
source_paths:
  - README.md
  - apps/web/app/routes/core.ts
  - apps/api-rs/migrations
---

# Terraline — Dokumentasi

Dokumentasi engineering untuk Terraline, fork Plane untuk IT service management.
Dokumen di folder ini menjelaskan implementasi current; alur pemakaian user ada
di in-app docs `/:workspaceSlug/docs`.

## Untuk siapa dokumen ini

- **Engineer dan contributor** yang perlu memahami arsitektur, route, data, API,
  testing, dan operasi.
- **AI coding agent** yang membutuhkan konteks ringkas dan path source yang
  dapat diverifikasi.
- **Maintainer** yang perlu tahu dokumen mana yang harus diperbarui setelah
  perubahan kode.

Dokumen produk untuk end user tidak ditulis di sini; gunakan in-app docs.

## Sumber kebenaran

Kode adalah sumber kebenaran. Dokumen ini merangkum dan menautkan, bukan
menggantikan kode.

- Route web: [`_generated/routes.md`](_generated/routes.md)
- Route API: [`_generated/api-routes.md`](_generated/api-routes.md)
- Tabel database: [`_generated/schema.md`](_generated/schema.md)
- Arsitektur: [`design/01-architecture.md`](design/01-architecture.md)
- Data model: [`design/02-data-model.md`](design/02-data-model.md)
- API contract: [`design/03-api-contract.md`](design/03-api-contract.md)

Generated inventory tidak boleh diedit manual. Jalankan `pnpm docs:inventory`
setelah route, API, atau migrasi berubah.

## Status dokumen

| Dokumen                                                                      | Status     | Catatan                                        |
| ---------------------------------------------------------------------------- | ---------- | ---------------------------------------------- |
| [`PRODUCT.md`](PRODUCT.md)                                                   | current    | Produk, user, ITIL4 map, capability, non-goals |
| [`CHEAT-SHEET.md`](CHEAT-SHEET.md)                                           | current    | Ringkasan stack, scope, data, auth, routing    |
| [`linting.md`](linting.md)                                                   | current    | OxLint dan oxfmt                               |
| [`design/01-architecture.md`](design/01-architecture.md)                     | current    | Monorepo dan batas komponen                    |
| [`design/02-data-model.md`](design/02-data-model.md)                         | current    | Migrasi, domain, soft delete                   |
| [`design/03-api-contract.md`](design/03-api-contract.md)                     | current    | Auth, scope, route family, error               |
| [`design/04-design-system.md`](design/04-design-system.md)                   | current    | Token, primitives, shell                       |
| [`design/05-testing-strategy.md`](design/05-testing-strategy.md)             | current    | Layer test dan command                         |
| [`design/06-ops-runbook.md`](design/06-ops-runbook.md)                       | current    | Local stack, web, live, tunnel                 |
| [`design/07-routing-and-navigation.md`](design/07-routing-and-navigation.md) | current    | Route family dan app rail                      |
| [`features/README.md`](features/README.md)                                   | current    | Inventory capability                           |
| [`features/work-items.md`](features/work-items.md)                           | current    | Tickets dan work item                          |
| [`features/services.md`](features/services.md)                               | current    | Service catalog, graph, health                 |
| [`features/intake.md`](features/intake.md)                                   | current    | Triage dan source                              |
| [`features/knowledge.md`](features/knowledge.md)                             | current    | Pages, docs, Space                             |
| [`features/sprints.md`](features/sprints.md)                                 | current    | Cycles                                         |
| [`features/tracks.md`](features/tracks.md)                                   | current    | Modules                                        |
| [`features/war-rooms.md`](features/war-rooms.md)                             | current    | Major incident room                            |
| [`features/ai.md`](features/ai.md)                                           | current    | Galileo dan scheduler                          |
| [`features/analytics.md`](features/analytics.md)                             | current    | Reports                                        |
| [`features/notifications.md`](features/notifications.md)                     | current    | Notifications                                  |
| [`features/applications.md`](features/applications.md)                       | current    | Applications dan scope                         |
| [`features/settings.md`](features/settings.md)                               | current    | Workspace/application/profile/god-mode         |
| [`features/archives.md`](features/archives.md)                               | current    | Archive dan restore                            |
| [`features/drafts.md`](features/drafts.md)                                   | current    | Workspace drafts                               |
| [`features/stickies.md`](features/stickies.md)                               | current    | Personal stickies                              |
| [`features/views.md`](features/views.md)                                     | current    | Queues dan views                               |
| [`features/browse.md`](features/browse.md)                                   | current    | Canonical ticket URL                           |
| [`features/_shared/README.md`](features/_shared/README.md)                   | current    | Shared concerns                                |
| [`ui/README.md`](ui/README.md)                                               | current    | UI docs index                                  |
| [`ui/design-tokens.md`](ui/design-tokens.md)                                 | current    | Token                                          |
| [`ui/shell.md`](ui/shell.md)                                                 | current    | App shell                                      |
| [`ui/editor.md`](ui/editor.md)                                               | current    | Editor                                         |
| [`_generated/routes.md`](_generated/routes.md)                               | generated  | Route web                                      |
| [`_generated/api-routes.md`](_generated/api-routes.md)                       | generated  | Route API                                      |
| [`_generated/schema.md`](_generated/schema.md)                               | generated  | Tabel database                                 |
| [`superpowers/README.md`](superpowers/README.md)                             | historical | Arsip spec, plan, decision                     |

## Reading paths

### Engineer / AI agent baru

1. [`../README.md`](../README.md) — positioning dan capability.
2. [`PRODUCT.md`](PRODUCT.md) — istilah dan batas produk.
3. [`design/01-architecture.md`](design/01-architecture.md) — bentuk repo.
4. [`design/07-routing-and-navigation.md`](design/07-routing-and-navigation.md) — route dan scope.
5. Feature doc sesuai area kerja.
6. [`CHEAT-SHEET.md`](CHEAT-SHEET.md) — ringkasan keputusan.

### Implementasi fitur

1. [`features/README.md`](features/README.md).
2. Feature doc terkait.
3. [`_generated/routes.md`](_generated/routes.md) dan
   [`_generated/api-routes.md`](_generated/api-routes.md).
4. [`design/05-testing-strategy.md`](design/05-testing-strategy.md).

### Operasi

1. [`design/06-ops-runbook.md`](design/06-ops-runbook.md).
2. [`../AGENTS.md`](../AGENTS.md).
3. [`design/05-testing-strategy.md`](design/05-testing-strategy.md).

## Struktur

```text
docs/
├── README.md
├── PRODUCT.md
├── CHEAT-SHEET.md
├── linting.md
├── design/                  01–07 + index
├── features/                capability docs + _shared/
├── ui/                      token, shell, editor
├── _generated/              routes, api-routes, schema
└── superpowers/             arsip historis
```

## Aturan update

1. Setelah mengubah route, API, atau migrasi, jalankan `pnpm docs:inventory`.
2. Setelah mengubah perilaku yang didokumentasikan, update doc terkait.
3. Jalankan `pnpm docs:verify <doc>` untuk menghitung ulang `source_hash`.
4. Jalankan `pnpm docs:check` sebelum commit.
5. Jangan mengedit `_generated/*` atau dokumen di `superpowers/`.

Jangan memakai line number di dokumen current; rujuk path dan nama simbol.

## Changelog

| Tanggal    | Perubahan                                                          |
| ---------- | ------------------------------------------------------------------ |
| 2026-09-03 | init                                                               |
| 2026-09-05 | cutover Rust                                                       |
| 2026-10-27 | reset corpus — current sources, generated inventories, docs checks |
