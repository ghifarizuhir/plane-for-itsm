---
status: current
last_verified: 2026-10-27
source_hash: sha256:7e52258fc71005a92297b6eab2a7a0f9516d14409919b1ba21496d9813104bfd
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/components
  - apps/api-rs/crates/api/src/routes
---

# Shared Concerns

Perilaku yang dipakai lebih dari satu feature. Dokumen di sini menjelaskan
kontrak bersama; detail feature tetap berada di dokumen masing-masing.

## Inventory

| Concern                        | Dokumen                              | Dipakai oleh                            |
| ------------------------------ | ------------------------------------ | --------------------------------------- |
| List, filter, sort, pagination | [`list.md`](./list.md)               | Tickets, sprints, tracks, views, queues |
| Detail page                    | [`detail-page.md`](./detail-page.md) | Ticket, sprint, track, page, view       |
| Comments, activity, versions   | [`comments.md`](./comments.md)       | Semua entity dengan activity            |
| Search and command palette     | [`search.md`](./search.md)           | Semua surface                           |

## Rules

- Jangan menduplikasi perilaku shared ke dokumen feature.
- Perubahan shared harus dicek dampaknya ke semua consumer.
- Permission tetap milik API; shared UI hanya merender aksi yang diizinkan.
