---
status: current
last_verified: 2026-10-27
source_hash: sha256:9dae3a54066596e11dc3bcb93be128c6ad0363995cc9fd8f358f150eaca30b1a
source_paths:
  - packages/tailwind-config
  - packages/ui/src
  - apps/web/core/components/navigation
---

# 04 — Design System

Status: **Current.**

Design system terdiri dari token Tailwind di `packages/tailwind-config`,
komponen dan primitives di `packages/ui`, serta shell aplikasi di
`apps/web/core/components/navigation`.

## Design tokens

- Warna memakai token semantik `theme-*`, `layer-*`, dan `wash-*`; jangan
  hardcode hex di komponen.
- Token di-override untuk light dan dark mode.
- Radius struktural memakai `rounded-chrome`; kontrol memakai `rounded-lg`.
- Spacing, typography, dan shadow mengikuti token package, bukan nilai acak.
- Ikon berasal dari `@plane/propel`; jangan mencampur library ikon lain tanpa
  alasan.

Source aktual:

- `packages/tailwind-config`
- `packages/ui/src`
- `packages/ui/styles`

## Aesthetic primitives

| Primitive       | Pemakaian                                 |
| --------------- | ----------------------------------------- |
| Icon chip       | Eyebrow, section divider, dan label kecil |
| Eyebrow         | Label uppercase di atas heading           |
| Page heading    | Judul halaman list/detail                 |
| Section divider | Pemisah section dengan garis tipis        |
| Meta row        | Grid label/value untuk metadata           |
| Mono chip       | ID, key, dan nilai teknis                 |

Primitives harus memakai token yang sama dengan komponen lain. Jika sebuah
primitive hanya dipakai satu halaman, taruh dekat halaman tersebut; jika dipakai
lebih dari satu domain, pindahkan ke `@plane/ui`.

## Shell

Shell workspace terdiri dari:

1. **App rail** di kiri: Work, Procedure, Galileo, dan Settings.
2. **Workspace sidebar** yang berubah sesuai mode.
3. **Top navigation** untuk breadcrumb, application scope, search, dan aksi
   halaman.
4. **Mobile mode menu** yang menggantikan rail pada layar kecil.

App rail tidak di-dock/undock; mode aktif ditentukan dari route. Galileo hanya
muncul jika LLM dikonfigurasi. Settings berada di bawah divider.

## Typography and motion

- Font UI: Inter; teks teknis memakai monospace.
- Heading memakai tracking rapat; body memakai line-height yang nyaman untuk
  teks operasional.
- Motion dipakai untuk transisi panel, hover service graph, dan feedback
  interaksi; hormati `prefers-reduced-motion`.
- Animasi tidak boleh menunda aksi atau menyembunyikan state penting.

## Component boundaries

- `@plane/ui` menyimpan komponen reusable dan tidak mengimpor `apps/*`.
- Komponen domain berada di `apps/web/core/components/<domain>`.
- Store MobX tidak boleh diimpor oleh `@plane/ui`.
- Semua komponen baru harus memiliki tipe props eksplisit dan tidak memakai
  `any`.

## Related docs

- [`../ui/design-tokens.md`](../ui/design-tokens.md)
- [`../ui/shell.md`](../ui/shell.md)
- [`../ui/editor.md`](../ui/editor.md)
- [`01-architecture.md`](./01-architecture.md)

## Changelog

| Tanggal    | Perubahan                                               |
| ---------- | ------------------------------------------------------- |
| 2026-09-03 | fork init                                               |
| 2026-10-27 | rewrite — token semantik, app rail, mobile mode, motion |
