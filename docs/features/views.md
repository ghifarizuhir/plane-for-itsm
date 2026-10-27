---
status: current
last_verified: 2026-10-27
source_hash: sha256:a9d5866d73eace005b66201244e112cbc37e45e9e57366282fa056a98a785507
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/global-view.store.ts
  - apps/web/core/store/project-view.store.ts
  - apps/api-rs/crates/api/src/routes/view.rs
---

# Views and Queues

Status: **Current.**

## Intent

View menyimpan filter dan konfigurasi tampilan yang dapat dipakai ulang. Di
workspace, kumpulan view atas ticket berfungsi sebagai queue; di application,
view menyimpan filter khusus application.

## Routes

| Surface                  | Route                                                                              |
| ------------------------ | ---------------------------------------------------------------------------------- |
| Workspace views / queues | `/:workspaceSlug/workspace-views`, `/:workspaceSlug/workspace-views/:globalViewId` |
| Application views        | `/:workspaceSlug/projects/:projectId/views`, `.../views/:viewId`                   |
| In-app guide             | `/:workspaceSlug/docs/queues` dan `/:workspaceSlug/docs/workspace-views`           |

## Behavior

- View menyimpan filter, grouping, layout, dan sort.
- View dapat bersifat private atau shared sesuai permission.
- Queue adalah view workspace yang dipakai untuk triage atau monitoring.
- Application view hanya menampilkan ticket application tersebut.
- Perubahan filter harus tercermin di URL agar bisa dibagikan.

## Stores and components

- Store: `global-view.store.ts`, `project-view.store.ts`.
- Komponen: `apps/web/core/components/views`.
- API: `view.rs`.
- Tabel: `views`, `global_views`.

## Permissions

| Role              | Create       | Read             | Update       | Delete       |
| ----------------- | ------------ | ---------------- | ------------ | ------------ |
| Member            | Private view | Shared view      | View sendiri | View sendiri |
| Application admin | Ya           | Ya               | Ya           | Ya           |
| Workspace admin   | Ya           | Ya               | Ya           | Ya           |
| Guest             | Tidak        | Shared read-only | Tidak        | Tidak        |

## Edge cases

- View yang mereferensikan state/label yang dihapus harus gagal dengan pesan
  jelas, bukan menampilkan data salah.
- Shared view mengikuti permission data, bukan permission view.
- Application scope selector harus konsisten dengan view workspace.

## Changelog

| Tanggal    | Perubahan                                                        |
| ---------- | ---------------------------------------------------------------- |
| 2026-10-27 | rewrite — queues, workspace views, application views, permission |
