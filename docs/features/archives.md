---
status: current
last_verified: 2026-10-27
source_hash: sha256:c98edf602a3f093b70a6804ce393d11039d83321f18a9b9ab9df751cea3b88a1
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store
  - apps/web/core/components
  - apps/api-rs/crates/api/src/routes
---

# Archives

Status: **Current.**

## Intent

Archive menyimpan entity yang dihapus dari tampilan aktif tanpa menghapus data.
User dapat meninjau dan restore entity yang masih diizinkan.

## Routes

| Surface                | Route                                                 |
| ---------------------- | ----------------------------------------------------- |
| Archived tickets       | `/:workspaceSlug/projects/:projectId/archives/issues` |
| Archived ticket detail | `.../archives/issues/:archivedIssueId`                |
| Archived sprints       | `.../archives/cycles`                                 |
| Archived tracks        | `.../archives/modules`                                |
| Archived applications  | `/:workspaceSlug/projects/archives`                   |

## Behavior

- Archive memakai `archived_at`, bukan hard delete.
- Entity archived tidak muncul di list default.
- Restore mengembalikan entity ke list aktif.
- Archived application tidak dapat diakses seperti application aktif.
- Permission archive mengikuti application/workspace role.

## Stores and components

- Store domain: `apps/web/core/store`.
- Komponen archive: `apps/web/core/components/archives` dan komponen domain
  terkait.
- API: `issue_archive_one.rs`, `issue_query.rs`, `project.rs`.

## Edge cases

- Ticket archived yang direstore harus kembali ke application asal.
- Sprint/track archived tidak boleh menghapus ticket terkait.
- Archived application harus tetap bisa dilihat admin.
- Hard delete hanya lewat kebijakan retensi worker.

## In-app guide

Tidak ada topik in-app khusus; gunakan halaman archive application.

## Changelog

| Tanggal    | Perubahan                                           |
| ---------- | --------------------------------------------------- |
| 2026-10-27 | verify — route archive current, restore, permission |
