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

# Drafts

Status: **Current.**

## Intent

Draft menyimpan ticket setengah jadi sebelum masuk application. Draft berguna
untuk menangkap ide atau laporan cepat tanpa harus memilih application lebih
dulu.

## Route

| Surface          | Route                         |
| ---------------- | ----------------------------- |
| Workspace drafts | `/:workspaceSlug/drafts`      |
| In-app guide     | `/:workspaceSlug/docs/drafts` |

## Behavior

- Draft bersifat workspace-level.
- Draft dapat diedit, dihapus, atau dipublish menjadi ticket.
- Publish membutuhkan pemilihan application tujuan.
- Draft tidak muncul di list ticket application sampai dipublish.
- Draft mengikuti permission user yang membuatnya.

## Stores and components

- Store domain: `apps/web/core/store` (draft-related).
- API: `apps/api-rs/crates/api/src/routes/draft.rs`.
- Tabel: `draft_issues`.

## Edge cases

- Draft yang application tujuannya sudah dihapus tidak boleh dipublish tanpa
  pemilihan ulang.
- Publish harus memvalidasi field wajib ticket.
- Draft bukan tempat menyimpan attachment permanen.

## Changelog

| Tanggal    | Perubahan                                      |
| ---------- | ---------------------------------------------- |
| 2026-10-27 | verify — workspace drafts, publish, permission |
