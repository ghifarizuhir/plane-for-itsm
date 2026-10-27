---
status: current
last_verified: 2026-10-27
source_hash: sha256:087e64347caa4b3129330fb019636b9408326c6c4e5904dca742e23890b5bd48
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
