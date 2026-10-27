---
status: current
last_verified: 2026-10-27
source_hash: sha256:cc58f30e1238f05c66c30927b1f1ee75aa2e522b0bdf7da0e09a59ea63cb501e
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/cycle.store.ts
  - apps/api-rs/crates/api/src/routes/cycle.rs
---

# Sprints (Cycles)

Status: **Current.**

## Intent

Sprint adalah periode kerja berulang dengan start/end date, progress, dan
burn-down. Sprint membantu tim merencanakan pekerjaan dalam cadence yang sama
lintas application.

## Routes

| Surface                   | Route                                                 |
| ------------------------- | ----------------------------------------------------- |
| Workspace sprints         | `/:workspaceSlug/cycles`                              |
| Application sprints       | `/:workspaceSlug/projects/:projectId/cycles`          |
| Application sprint detail | `/:workspaceSlug/projects/:projectId/cycles/:cycleId` |
| Legacy slug               | `/:workspaceSlug/sprints` → `/cycles`                 |

## Workspace aggregate versus application detail

- Workspace `/cycles` menampilkan sprint lintas application dengan application
  scope selector.
- Application `/cycles` hanya menampilkan sprint milik application.
- Detail sprint menampilkan ticket, progress, dan analytics terkait.

## Behavior

- Create/edit/delete sprint mengikuti permission application/workspace.
- Ticket dapat dipindahkan ke sprint lain.
- Sprint yang selesai tetap dapat dibuka untuk review.
- Archive sprint tersedia dari halaman archives application.

## Stores and components

- Store: `apps/web/core/store/cycle.store.ts`.
- Komponen: `apps/web/core/components/cycles`.
- Analytics sidebar: `apps/web/core/components/cycles/analytics-sidebar`.

## API touchpoints

- `apps/api-rs/crates/api/src/routes/cycle.rs`.
- Tabel: `cycles`, `cycle_issues`, `cycle_user_properties`.
- Inventory: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Create | Read     | Update            | Delete |
| ----------------- | ------ | -------- | ----------------- | ------ |
| Member            | Tidak  | Ya       | Ticket assignment | Tidak  |
| Application admin | Ya     | Ya       | Ya                | Ya     |
| Workspace admin   | Ya     | Ya       | Ya                | Ya     |
| Guest             | Tidak  | Terbatas | Tidak             | Tidak  |

## Edge cases

- Sprint tanpa ticket tetap valid.
- Tanggal selesai tidak boleh mendahului tanggal mulai.
- Memindahkan ticket tidak boleh menghilangkan relasi lain.
- Workspace aggregate harus menghormati application scope.

## In-app guide

`/:workspaceSlug/docs/sprints`.

## Changelog

| Tanggal    | Perubahan                                                                |
| ---------- | ------------------------------------------------------------------------ |
| 2026-10-27 | new — menggantikan cycles.md, workspace aggregate dan application detail |
