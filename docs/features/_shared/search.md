---
status: current
last_verified: 2026-10-27
source_hash: sha256:c48e97217d2ea9d51a41a9f6e61d9ef59aabbddbcdb7fbcc4686432706371eb8
source_paths:
  - apps/web/core/components
  - apps/web/core/store/base-command-palette.store.ts
  - apps/api-rs/crates/api/src/routes/search.rs
---

# Search and Command Palette

Status: **Current.**

## Purpose

Search menyediakan akses cepat ke ticket, page, service, dan aksi navigasi
melalui command palette.

## Behavior

- Shortcut: Cmd/Ctrl + K.
- Hasil dikelompokkan berdasarkan tipe entity.
- Search menghormati workspace dan permission user.
- Aksi cepat (create, navigate, open settings) dapat dijalankan dari palette.
- Query dan hasil tidak boleh membocorkan entity dari workspace lain.

## API

- `apps/api-rs/crates/api/src/routes/search.rs`.
- Store: `base-command-palette.store.ts`.
- Komponen search/palette di `apps/web/core/components`.

## Edge cases

- Query kosong menampilkan aksi dan recent items, bukan semua data.
- Debounce input untuk menghindari request berlebihan.
- Hasil yang sudah tidak valid (deleted/archived) harus diabaikan.
- Keyboard navigation harus tetap bekerja saat hasil berubah.

## Related

- [`../browse.md`](../browse.md)
- [`../work-items.md`](../work-items.md)
