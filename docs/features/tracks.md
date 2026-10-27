---
status: current
last_verified: 2026-10-27
source_hash: sha256:5e9f99737d882274d41768b4a55530c6a353538c7b3117109486b170e8dd13b2
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/module.store.ts
  - apps/api-rs/crates/api/src/routes/module.rs
---

# Tracks (Modules)

Status: **Current.**

## Intent

Track mengelompokkan ticket menuju satu outcome atau fase yang lebih besar.
Track dapat berjalan lebih lama dari satu sprint dan menjadi cara tim melihat
progress lintas periode.

## Routes

| Surface                  | Route                                                   |
| ------------------------ | ------------------------------------------------------- |
| Workspace tracks         | `/:workspaceSlug/modules`                               |
| Application tracks       | `/:workspaceSlug/projects/:projectId/modules`           |
| Application track detail | `/:workspaceSlug/projects/:projectId/modules/:moduleId` |
| Legacy slug              | `/:workspaceSlug/tracks` → `/modules`                   |

## Workspace aggregate versus application detail

- Workspace `/modules` menampilkan track lintas application dengan application
  scope selector.
- Application `/modules` hanya menampilkan track milik application.
- Detail track menampilkan ticket, progress, lead, member, dan status.

## Behavior

- Track memiliki nama, deskripsi, lead, member, target date, dan status.
- Ticket dapat ditautkan ke track.
- Track dapat diarsipkan dan dipulihkan.
- Progress dihitung dari ticket yang tertaut.

## Stores and components

- Store: `apps/web/core/store/module.store.ts`.
- Komponen: `apps/web/core/components/modules`.
- Analytics sidebar: `apps/web/core/components/modules/analytics-sidebar`.

## API touchpoints

- `apps/api-rs/crates/api/src/routes/module.rs`.
- Tabel: `modules`, `module_issues`, `module_user_properties`.
- Inventory: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Create | Read     | Update            | Delete |
| ----------------- | ------ | -------- | ----------------- | ------ |
| Member            | Tidak  | Ya       | Ticket assignment | Tidak  |
| Application admin | Ya     | Ya       | Ya                | Ya     |
| Workspace admin   | Ya     | Ya       | Ya                | Ya     |
| Guest             | Tidak  | Terbatas | Tidak             | Tidak  |

## Edge cases

- Track tanpa ticket tetap valid.
- Menghapus track tidak boleh menghapus ticket.
- Track archived tidak muncul di list default.
- Workspace aggregate harus menghormati application scope.

## In-app guide

`/:workspaceSlug/docs/tracks`.

## Changelog

| Tanggal    | Perubahan                                                                 |
| ---------- | ------------------------------------------------------------------------- |
| 2026-10-27 | new — menggantikan modules.md, workspace aggregate dan application detail |
