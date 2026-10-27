---
status: current
last_verified: 2026-10-27
source_hash: sha256:4415c046c172feee8fdcbd133af38c9fc20fed00e77f1b9fc86a5d8b76faa0a5
source_paths:
  - apps/web/core/components/issues
  - apps/web/core/layouts
  - apps/web/core/components/navigation
---

# Detail Page

Status: **Current.**

## Purpose

Pola detail dipakai oleh ticket, sprint, track, page, dan view. Detail dapat
tampil sebagai halaman penuh atau peek/overlay tergantung surface.

## Behavior

- Header berisi identifier, title, status, dan aksi utama.
- Metadata dan relasi berada di panel samping atau section.
- Activity, komentar, dan version mengikuti shared comments.
- Peek/overlay harus bisa ditutup tanpa kehilangan state list.
- Deep link selalu membuka halaman detail penuh.

## Responsive

- Desktop: panel samping dapat terbuka bersamaan dengan konten.
- Mobile: panel menjadi sheet/full-screen.
- Keyboard: Escape menutup overlay; focus kembali ke trigger.

## Permission

Detail hanya merender aksi yang diizinkan. Aksi yang tidak diizinkan tidak boleh
hanya disembunyikan secara visual tanpa guard di API.

## Edge cases

- Entity yang dihapus/arsip harus menampilkan state yang jelas.
- Data yang gagal dimuat harus punya retry.
- Perubahan dari user lain harus tercermin lewat SWR/realtime tanpa reload
  penuh.

## Related

- [`list.md`](./list.md)
- [`comments.md`](./comments.md)
