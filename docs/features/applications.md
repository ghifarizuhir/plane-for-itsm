---
status: current
last_verified: 2026-10-27
source_hash: sha256:bbd1bc373a175bb78f0fbe6f2cfd7333f9d8319ccd24f7a065e85755592f21e4
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/workspace-application-scope.store.ts
  - apps/web/core/store/project
  - apps/api-rs/crates/api/src/routes/project.rs
---

# Applications

Status: **Current.**

## Intent

Application adalah nama user-facing untuk project. Application mengelompokkan
ticket, member, konfigurasi, war room, dan archive, sementara workspace
menyediakan service catalog dan agregat lintas application.

## Routes

| Surface               | Route                                          |
| --------------------- | ---------------------------------------------- |
| Application list      | `/:workspaceSlug/projects`                     |
| Application workspace | `/:workspaceSlug/projects/:projectId/*`        |
| Tickets               | `.../projects/:projectId/issues`               |
| Settings              | `/:workspaceSlug/settings/projects/:projectId` |
| Archived applications | `/:workspaceSlug/projects/archives`            |

## Application scope selector

- Selector disimpan per workspace di
  `workspace-application-scope.store.ts`.
- Semua halaman agregat workspace (tickets, services, intake, sprints, tracks)
  memakai scope yang sama.
- Scope harus tercermin di URL/filter agar bisa dibagikan.

## Membership and permissions

- Workspace role dan application membership adalah dua lapisan.
- Application admin mengelola member, state, label, estimate, type, dan feature
  toggle.
- Guest dapat dibatasi per application.

## Archives

- Ticket, sprint, track, dan application dapat diarsipkan.
- Archive tidak menghapus data; restore tersedia dari halaman archive.
- Archived application tidak muncul di daftar default.

## API touchpoints

- `project.rs` — CRUD, archive, member, identifier.
- Store: `apps/web/core/store/project`.
- Komponen: `apps/web/core/components/projects`.

## Limitations

- Application tidak memiliki service sendiri; service adalah entitas workspace.
- Memindahkan ticket antar application tidak didukung sebagai operasi biasa.
- Identifier application harus unik di workspace.

## In-app guide

`/:workspaceSlug/docs/overview` dan `/:workspaceSlug/docs/project-settings`.

## Changelog

| Tanggal    | Perubahan                                                         |
| ---------- | ----------------------------------------------------------------- |
| 2026-10-27 | new — applications/projects, scope selector, membership, archives |
