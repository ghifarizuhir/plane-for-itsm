---
status: current
last_verified: 2026-10-27
source_hash: sha256:3385a616fe882933532b7a00f491828315adcd07360f565e2c6f561fc62e5ea7
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store
  - apps/web/core/components
  - apps/api-rs/crates/api/src/routes
---

# Stickies

Status: **Current.**

## Intent

Stickies adalah catatan cepat pribadi di workspace: ide, pengingat, atau potongan
teks yang belum perlu menjadi ticket.

## Route

| Surface      | Route                           |
| ------------ | ------------------------------- |
| Stickies     | `/:workspaceSlug/stickies`      |
| In-app guide | `/:workspaceSlug/docs/stickies` |

## Behavior

- Stickies bersifat personal dan workspace-scoped.
- Catatan mendukung warna dan konten teks.
- Stickies tidak muncul di search lintas user.
- Hapus sticky tidak memengaruhi ticket atau draft.

## Stores and components

- Store domain: `apps/web/core/store/sticky`.
- Komponen: `apps/web/core/components/stickies`.
- API: route sticky di `apps/api-rs/crates/api/src/routes`.

## Edge cases

- Sticky hanya dapat diakses pemiliknya.
- Konten besar harus tetap bisa disimpan tanpa memblokir UI.
- Sticky tidak menggantikan knowledge base atau komentar ticket.

## Changelog

| Tanggal    | Perubahan                                   |
| ---------- | ------------------------------------------- |
| 2026-10-27 | verify — personal scope, behavior, boundary |
