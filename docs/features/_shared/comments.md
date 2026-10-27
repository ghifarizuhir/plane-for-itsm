---
status: current
last_verified: 2026-10-27
source_hash: sha256:8b99053ab673c7efe03ab11bbc4d4da84b8b3640414b9cd7fab2517aea305d8a
source_paths:
  - apps/web/core/components
  - apps/api-rs/crates/api/src/routes/issue_activity_write.rs
  - apps/api-rs/crates/api/src/routes/history.rs
  - apps/api-rs/crates/api/src/routes/versions.rs
  - apps/api-rs/crates/api/src/routes/subscribe.rs
  - apps/api-rs/crates/api/src/routes/reactions.rs
---

# Comments, Activity, and Versions

Status: **Current.**

## Purpose

Semua entity utama memakai pola interaksi yang sama: komentar, activity
timeline, version, subscription, dan reaction.

## Comments

- Komentar mendukung rich text dan mention.
- Mention membuat notifikasi.
- Edit/hapus komentar mengikuti permission dan audit.
- Attachment mengikuti pipeline asset.

## Activity

- Activity mencatat perubahan field, assignment, state, dan relasi.
- Timeline membaca `history.rs` dan activity writer.
- Activity tidak boleh diedit user; itu audit trail.

## Versions

- Version dibuat saat deskripsi berubah.
- Version dapat dibandingkan dan dipulihkan jika didukung.
- Version bukan pengganti audit activity.

## Subscriptions and reactions

- User dapat subscribe/unsubscribe dari notifikasi entity.
- Reaction mengikuti permission entity dan tidak membuat activity baru yang
  mengganggu timeline.

## Edge cases

- Mention user yang tidak punya akses harus ditolak.
- Komentar pada entity archived harus read-only.
- Version yang dipulihkan harus menghasilkan activity baru.
- Realtime update tidak boleh menduplikasi komentar yang sudah dioptimistic.

## Related

- [`detail-page.md`](./detail-page.md)
- [`../notifications.md`](../notifications.md)
