---
status: current
last_verified: 2026-10-27
source_hash: sha256:0c4cc8924330a491a957027cf09c3fac9e937402fd04604cba0502e4682a12fd
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/issue
  - apps/web/core/components
  - apps/api-rs/crates/api/src/routes
---

# Browse (Canonical Ticket URL)

Status: **Current.**

## Intent

Browse adalah URL kanonik untuk satu ticket berdasarkan identifier, misalnya
`ENG-123`. URL ini dipakai untuk share link, search result, dan redirect dari
detail ticket application.

## Route

| Surface      | Route                              |
| ------------ | ---------------------------------- |
| Browse       | `/:workspaceSlug/browse/:workItem` |
| In-app guide | `/:workspaceSlug/docs/search`      |

## Behavior

- `:workItem` adalah identifier ticket, bukan UUID.
- Browse membuka detail ticket yang sama dengan detail application.
- Jika user tidak punya akses ke application, tampilkan state unauthorized/not
  found yang konsisten.
- Search dan command palette memakai URL ini sebagai target.

## Stores and components

- Store: `apps/web/core/store/issue`.
- Komponen: `apps/web/core/components/issues` dan browse route.
- API: issue detail/query di `apps/api-rs/crates/api/src/routes`.

## Edge cases

- Identifier milik workspace lain harus ditolak.
- Ticket archived tidak boleh membuka detail aktif tanpa konteks archive.
- Perubahan identifier application harus mempertimbangkan link lama.

## Changelog

| Tanggal    | Perubahan                                                  |
| ---------- | ---------------------------------------------------------- |
| 2026-10-27 | rewrite — canonical identifier URL, permission, edge cases |
