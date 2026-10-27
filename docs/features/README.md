---
status: current
last_verified: 2026-10-27
source_hash: sha256:853923d339a4bd50230878de625ab5b5b22f1deff5bef783b8c927646d6ee47e
source_paths:
  - apps/web/app/routes/core.ts
---

# Feature Documentation

Dokumentasi implementasi per capability. Untuk alur pemakaian, lihat in-app docs
di `/:workspaceSlug/docs`; untuk daftar route resmi, lihat
[`../_generated/routes.md`](../_generated/routes.md).

## Inventory

| Capability           | Dokumen                                    | Route utama                                      | Status  |
| -------------------- | ------------------------------------------ | ------------------------------------------------ | ------- |
| Tickets / work items | [`work-items.md`](./work-items.md)         | `/work-items`, `/projects/:projectId/issues`     | current |
| Services             | [`services.md`](./services.md)             | `/services`                                      | current |
| Intake               | [`intake.md`](./intake.md)                 | `/intake`, `/projects/:projectId/intake`         | current |
| Knowledge            | [`knowledge.md`](./knowledge.md)           | `/pages`, `/projects/:projectId/pages`           | current |
| Sprints              | [`sprints.md`](./sprints.md)               | `/cycles`, `/projects/:projectId/cycles`         | current |
| Tracks               | [`tracks.md`](./tracks.md)                 | `/modules`, `/projects/:projectId/modules`       | current |
| War rooms            | [`war-rooms.md`](./war-rooms.md)           | `/projects/:projectId/war-rooms/:warRoomId`      | current |
| AI                   | [`ai.md`](./ai.md)                         | `/galileo`, `/scheduler`                         | current |
| Analytics            | [`analytics.md`](./analytics.md)           | `/analytics/:tabId`                              | current |
| Notifications        | [`notifications.md`](./notifications.md)   | `/notifications`                                 | current |
| Applications         | [`applications.md`](./applications.md)     | `/projects`                                      | current |
| Settings             | [`settings.md`](./settings.md)             | `/settings`                                      | current |
| Archives             | [`archives.md`](./archives.md)             | `/projects/:projectId/archives/*`                | current |
| Drafts               | [`drafts.md`](./drafts.md)                 | `/drafts`                                        | current |
| Stickies             | [`stickies.md`](./stickies.md)             | `/stickies`                                      | current |
| Views / queues       | [`views.md`](./views.md)                   | `/workspace-views`, `/projects/:projectId/views` | current |
| Browse               | [`browse.md`](./browse.md)                 | `/browse/:workItem`                              | current |
| Shared concerns      | [`_shared/README.md`](./_shared/README.md) | —                                                | current |

## Boundary

- `docs/features` menjelaskan route, store, API, permission, dan edge case.
- In-app docs menjelaskan langkah penggunaan untuk end user.
- Generated inventory adalah daftar route/API yang authoritative.
- Setiap dokumen current memiliki frontmatter dan source hash.

## Changelog

| Tanggal    | Perubahan                                        |
| ---------- | ------------------------------------------------ |
| 2026-10-27 | rewrite — inventory current, hapus dokumen usang |
