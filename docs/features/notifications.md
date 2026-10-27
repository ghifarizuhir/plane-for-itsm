---
status: current
last_verified: 2026-10-27
source_hash: sha256:1b00b9993f39dbf7a917fe95671bff8115ca6fd28ec80188a3e55c6cc523f662
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/notifications
  - apps/api-rs/crates/api/src/routes/notification.rs
  - apps/web/core/components/workspace-notifications
---

# Notifications (Inbox)

Status: **Current.**

## Intent

Notifications mengumpulkan mention, assignment, komentar, dan perubahan ticket
yang relevan bagi user. Intake adalah alur terpisah untuk laporan baru; inbox
legacy diarahkan ke intake application.

## Route

| Surface              | Route                           |
| -------------------- | ------------------------------- |
| Notifications        | `/:workspaceSlug/notifications` |
| Legacy project inbox | `.../inbox` → `.../intake`      |
| In-app guide         | `/:workspaceSlug/docs/inbox`    |

## Notification types

- Mention di komentar atau deskripsi.
- Assignment ticket.
- Perubahan state/priority yang di-subscribe.
- Komentar pada ticket yang diikuti.
- Update dari activity yang relevan.

## Behavior

- Read/unread per notification.
- Snooze menunda notifikasi dari daftar aktif.
- Archive menyembunyikan notifikasi tanpa menghapus data ticket.
- Klik notifikasi membuka ticket atau activity terkait.

## Stores and components

- Store: `apps/web/core/store/notifications`.
- Komponen: `apps/web/core/components/workspace-notifications`.
- API: `apps/api-rs/crates/api/src/routes/notification.rs`.

## Permissions

Notifikasi hanya menampilkan data yang boleh dilihat user. Guest tidak menerima
notifikasi workspace-wide.

## In-app guide

`/:workspaceSlug/docs/inbox`.

## Changelog

| Tanggal    | Perubahan                                                      |
| ---------- | -------------------------------------------------------------- |
| 2026-10-27 | new — menggantikan inbox.md, notifications dan redirect intake |
