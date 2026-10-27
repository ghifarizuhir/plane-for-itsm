---
status: current
last_verified: 2026-10-27
source_hash: sha256:f9293f97417463c5e50e2797783c81ffe19afd7cf22b73f97355763fae22e64f
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/issue
  - apps/api-rs/crates/api/src/routes
---

# Work Items (Tickets)

Status: **Current.**

## Intent

Ticket adalah unit kerja operasional. Satu ticket dapat berupa Incident,
Problem, Change, Improvement, atau Request, dan dapat dilihat dari workspace
maupun dari application yang memilikinya.

## Routes

| Surface                   | Route                                                 |
| ------------------------- | ----------------------------------------------------- |
| Workspace ticket list     | `/:workspaceSlug/work-items`                          |
| Application ticket list   | `/:workspaceSlug/projects/:projectId/issues`          |
| Application ticket detail | `/:workspaceSlug/projects/:projectId/issues/:issueId` |
| Canonical shared link     | `/:workspaceSlug/browse/:workItem`                    |
| Legacy tickets slug       | `/:workspaceSlug/tickets` → `/work-items`             |

## Current behavior

- Workspace list mengagregasi ticket lintas application dengan application
  scope selector.
- Application list hanya menampilkan ticket milik application tersebut.
- Ticket detail dapat dibuka dari list, browse, search, atau deep link.
- Ticket mendukung komentar, activity, version, attachment, sub-item, dan
  relation.
- War Room toggle tersedia pada Incident.
- Request dapat mewajibkan service link saat intake triage.

## Layouts

Lima layout ticket:

1. List
2. Kanban
3. Calendar
4. Gantt
5. Spreadsheet

Layout dan filter disimpan di URL atau view; jangan mengandalkan state lokal
saja.

## Stores and services

- Store domain: `apps/web/core/store/issue`.
- Service client: `apps/web/core/services` dan `packages/services`.
- Application scope: `workspace-application-scope.store.ts`.
- Komponen list/detail: `apps/web/core/components/issues`.

## API touchpoints

- `apps/api-rs/crates/api/src/routes/issue_query.rs`
- `issue_lists.rs`
- `issue_write.rs`
- `issue_update.rs`
- `issue_sub.rs`
- `issue_activity_write.rs`
- `issue_version_write.rs`
- `history.rs`, `versions.rs`, `reactions.rs`, `subscribe.rs`

Daftar method/path aktual: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Create                            | Read                   | Update                  | Delete |
| ----------------- | --------------------------------- | ---------------------- | ----------------------- | ------ |
| Member            | Ya, sesuai application membership | Ya, sesuai scope       | Ticket sendiri/assigned | Tidak  |
| Application admin | Ya                                | Ya                     | Ya                      | Ya     |
| Workspace admin   | Ya                                | Ya                     | Ya                      | Ya     |
| Guest             | Tidak                             | Read-only sesuai scope | Tidak                   | Tidak  |

## Edge cases

- Ticket dari application yang tidak visible tidak boleh muncul di workspace
  aggregate.
- Perubahan application scope harus konsisten dengan filter URL.
- Ticket yang diarsipkan tidak muncul di list default; buka dari archives.
- War Room hanya valid untuk Incident dan satu active room per incident.
- Perubahan state/label/type harus menghormati konfigurasi application.

## In-app guide

`/:workspaceSlug/docs/tickets` dan `/:workspaceSlug/docs/itil4-incident`.

## Changelog

| Tanggal    | Perubahan                                                                         |
| ---------- | --------------------------------------------------------------------------------- |
| 2026-10-27 | rewrite — workspace/application scope, current layouts, war room dan service gate |
