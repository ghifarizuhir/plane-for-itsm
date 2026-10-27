---
status: current
last_verified: 2026-10-27
source_hash: sha256:af729c55a8f696500b3607999aa55a0755cd99d639c15d14e7253f62a8db9709
source_paths:
  - apps/web/app/routes/core.ts
  - apps/api-rs/crates/api/src/routes/workspace_settings.rs
  - apps/api-rs/crates/api/src/routes/instance_admin.rs
  - apps/web/core/components/settings
  - apps/web/core/components/workspace/sidebar
---

# Settings

Status: **Current.**

## Intent

Settings menyatukan konfigurasi workspace, application, profil, dan instance
admin. Konfigurasi workspace berlaku lintas application; konfigurasi application
hanya untuk application tersebut.

## Workspace settings

| Surface         | Route                                      |
| --------------- | ------------------------------------------ |
| General         | `/:workspaceSlug/settings`                 |
| Members         | `/:workspaceSlug/settings/members`         |
| Exports         | `/:workspaceSlug/settings/exports`         |
| Webhooks        | `/:workspaceSlug/settings/webhooks`        |
| Work item types | `/:workspaceSlug/settings/work-item-types` |
| Metric sources  | `/:workspaceSlug/settings/metric-sources`  |
| Features        | `/:workspaceSlug/settings/features`        |
| Document export | `/:workspaceSlug/settings/document-export` |

## Application settings

| Surface         | Route                                                        |
| --------------- | ------------------------------------------------------------ |
| General         | `/:workspaceSlug/settings/projects/:projectId`               |
| Members         | `.../members`                                                |
| States          | `.../states`                                                 |
| Labels          | `.../labels`                                                 |
| Estimates       | `.../estimates`                                              |
| Work item types | `.../work-item-types`                                        |
| Intake sources  | `.../intake-sources`                                         |
| Features        | `.../features/cycles`, `modules`, `views`, `pages`, `intake` |
| Automations     | `.../automations`                                            |

## Profile settings

| Surface           | Route                                                      |
| ----------------- | ---------------------------------------------------------- |
| Profile           | `/settings/profile/:profileTabId`                          |
| Legacy API tokens | `/:workspaceSlug/settings/api-tokens` → profile API tokens |

## God-mode

- Instance admin berjalan di `apps/admin` pada `/god-mode/`.
- Mengelola workspace, user, auth provider, email, image, dan AI.
- Perubahan instance-level tidak boleh dilakukan dari web workspace biasa.

## Feature flags

- Workspace settings menyimpan flag fitur.
- Sidebar dan route tertentu dapat disembunyikan berdasarkan flag.
- Flag tidak menggantikan permission; keduanya harus dicek.

## API touchpoints

- `workspace_settings.rs` — settings workspace.
- `instance_admin.rs` — instance admin.
- `apps/web/core/components/settings` — UI settings.
- Inventory: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Workspace settings | Application settings | Instance settings |
| ----------------- | ------------------ | -------------------- | ----------------- |
| Member            | Read terbatas      | Read terbatas        | Tidak             |
| Application admin | Tidak              | Ya                   | Tidak             |
| Workspace admin   | Ya                 | Ya                   | Tidak             |
| Instance admin    | Ya                 | Ya                   | Ya                |

## Edge cases

- Menghapus member harus mempertahankan audit dan ticket ownership.
- Mengubah work item type harus mempertimbangkan ticket yang sudah ada.
- Metric source yang dihapus tidak boleh menghapus metrik historis.
- Feature flag yang dimatikan tidak boleh membuat route inaccessible tanpa
  pesan.

## In-app guide

`/:workspaceSlug/docs/personal-settings` dan
`/:workspaceSlug/docs/project-settings`.

## Changelog

| Tanggal    | Perubahan                                                                       |
| ---------- | ------------------------------------------------------------------------------- |
| 2026-10-27 | rewrite — workspace/application/profile/god-mode, feature flags, metric sources |
