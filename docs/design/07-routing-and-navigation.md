---
status: current
last_verified: 2026-10-27
source_hash: sha256:6a58e6aa1913aad717b7fe8b5cc644315dbcefe7e9da8f7b82682d74d31f1455
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/components/navigation
  - apps/web/app/routes/redirects
---

# 07 — Routing and Navigation

Status: **Current.**

## Scope model

```text
Workspace
├── workspace-level surfaces
└── application (project)
    └── application-level surfaces
```

Workspace adalah tenant. Application adalah nama user-facing untuk project.
Ticket dimiliki satu application. Service adalah katalog workspace.

## Canonical route families

### Workspace

| Surface         | Route                                                                              |
| --------------- | ---------------------------------------------------------------------------------- |
| Home            | `/:workspaceSlug`                                                                  |
| Work items      | `/:workspaceSlug/work-items`                                                       |
| Services        | `/:workspaceSlug/services`, `/:workspaceSlug/services/:serviceId`                  |
| Intake          | `/:workspaceSlug/intake`                                                           |
| Sprints         | `/:workspaceSlug/cycles`                                                           |
| Tracks          | `/:workspaceSlug/modules`                                                          |
| Knowledge       | `/:workspaceSlug/pages`                                                            |
| Workspace views | `/:workspaceSlug/workspace-views`, `/:workspaceSlug/workspace-views/:globalViewId` |
| Analytics       | `/:workspaceSlug/analytics/:tabId`                                                 |
| Scheduler       | `/:workspaceSlug/scheduler`                                                        |
| Notifications   | `/:workspaceSlug/notifications`                                                    |
| Docs            | `/:workspaceSlug/docs`, `/:workspaceSlug/docs/:topicSlug`                          |
| Galileo         | `/:workspaceSlug/galileo`, `/:workspaceSlug/galileo/:conversationId`               |
| Settings        | `/:workspaceSlug/settings`                                                         |
| Browse ticket   | `/:workspaceSlug/browse/:workItem`                                                 |

### Application

| Surface       | Route                                                                              |
| ------------- | ---------------------------------------------------------------------------------- |
| Applications  | `/:workspaceSlug/projects`                                                         |
| Tickets       | `/:workspaceSlug/projects/:projectId/issues`                                       |
| Ticket detail | `/:workspaceSlug/projects/:projectId/issues/:issueId`                              |
| Sprints       | `/:workspaceSlug/projects/:projectId/cycles`, `.../cycles/:cycleId`                |
| Tracks        | `/:workspaceSlug/projects/:projectId/modules`, `.../modules/:moduleId`             |
| Knowledge     | `/:workspaceSlug/projects/:projectId/pages`, `.../pages/:pageId`                   |
| Intake        | `/:workspaceSlug/projects/:projectId/intake`                                       |
| War rooms     | `/:workspaceSlug/projects/:projectId/war-rooms/:warRoomId`                         |
| Views         | `/:workspaceSlug/projects/:projectId/views`, `.../views/:viewId`                   |
| Archives      | `/:workspaceSlug/projects/:projectId/archives/issues`, `.../cycles`, `.../modules` |

## Product term mapping

| Product      | Route/code term            |
| ------------ | -------------------------- |
| Tickets      | `work-items`, `issues`     |
| Sprints      | `cycles`                   |
| Tracks       | `modules`                  |
| Knowledge    | `pages`                    |
| Applications | `projects`                 |
| Queues       | `workspace-views`, `views` |

## App rail

App rail adalah navigasi utama workspace:

1. **Work** → `/:workspaceSlug` dan surface pekerjaan.
2. **Procedure** → `/:workspaceSlug/docs`.
3. **Galileo** → `/:workspaceSlug/galileo`; hanya tampil jika LLM dikonfigurasi.
4. **Settings** → `/:workspaceSlug/settings`, di bawah divider.

Mode aktif ditentukan dari route, bukan state terpisah. Sidebar dalam berubah
sesuai mode. Pada mobile, rail diganti menu mode di top navigation.

## Legacy redirects

Redirect didaftarkan di `apps/web/app/routes/core.ts`:

| Legacy                                                    | Canonical                                        |
| --------------------------------------------------------- | ------------------------------------------------ |
| `/:workspaceSlug/projects/:projectId/settings/*`          | `/:workspaceSlug/settings/projects/:projectId/*` |
| `/:workspaceSlug/analytics`                               | `/:workspaceSlug/analytics/overview`             |
| `/:workspaceSlug/settings/api-tokens`                     | `/settings/profile/api-tokens`                   |
| `/:workspaceSlug/projects/:projectId/inbox`               | `.../intake`                                     |
| `/:workspaceSlug/projects/:projectId/services`            | `/:workspaceSlug/services`                       |
| `/:workspaceSlug/projects/:projectId/services/:serviceId` | `/:workspaceSlug/services/:serviceId`            |
| `/:workspaceSlug/projects/:projectId/war-rooms`           | `.../issues?war_room=on`                         |
| `/:workspaceSlug/sprints`                                 | `/:workspaceSlug/cycles`                         |
| `/:workspaceSlug/tracks`                                  | `/:workspaceSlug/modules`                        |
| `/:workspaceSlug/tickets`                                 | `/:workspaceSlug/work-items`                     |
| `/:workspaceSlug/kb`                                      | `/:workspaceSlug/pages`                          |
| `accounts/sign-up`                                        | `/sign-up`                                       |

Daftar lengkap dan status canonical/redirect:
[`../_generated/routes.md`](../_generated/routes.md).

## Adding a route

1. Daftarkan route di `apps/web/app/routes/core.ts`.
2. Tambahkan item rail/sidebar jika surface user-facing.
3. Tambahkan redirect jika menggantikan URL lama.
4. Update feature doc terkait.
5. Jalankan `pnpm docs:inventory` dan `pnpm docs:check`.

Jangan menambahkan route ke dokumen manual; inventory generated adalah daftar
resmi.

## Related docs

- [`01-architecture.md`](./01-architecture.md)
- [`03-api-contract.md`](./03-api-contract.md)

## Changelog

| Tanggal    | Perubahan                                        |
| ---------- | ------------------------------------------------ |
| 2026-10-27 | new — route families, app rail, legacy redirects |
