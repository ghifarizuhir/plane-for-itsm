---
status: current
last_verified: 2026-10-27
source_hash: sha256:30cfd4c9ab562db69dffec843b03a7e35767cb344a84f9efdcb470cf33178fe3
source_paths:
  - packages/ui/src
  - packages/editor
  - packages/tailwind-config
---

# UI Docs

Dokumentasi teknis untuk shell, token, dan editor. Fokus pada implementasi yang
benar-benar dipakai, bukan aspirasi desain.

## Reading order

| #   | Dokumen                                  | Isi                                     | Status  |
| --- | ---------------------------------------- | --------------------------------------- | ------- |
| 1   | [`design-tokens.md`](./design-tokens.md) | Token warna, radius, font, icon, motion | current |
| 2   | [`shell.md`](./shell.md)                 | App rail, sidebar, topbar, mobile       | current |
| 3   | [`editor.md`](./editor.md)               | TipTap, Yjs, Hocuspocus                 | current |

Untuk keputusan desain lintas UI, lihat
[`../design/04-design-system.md`](../design/04-design-system.md). Untuk pola
list/detail, lihat [`../features/_shared/`](../features/_shared/README.md).

## Conventions

- Tulis class/token yang benar-benar dipakai di kode.
- Jangan menduplikasi definisi token; link ke `design-tokens.md`.
- Rujuk path komponen dan package, bukan line number.
- Status dokumen memakai frontmatter `status: current`.
- Jika dokumen bertentangan dengan kode, kode menang; update dokumen.

## Boundary

| Jenis konten             | Lokasi                          |
| ------------------------ | ------------------------------- |
| Token dan CSS variable   | `design-tokens.md`              |
| Shell, rail, sidebar     | `shell.md`                      |
| Editor dan collaboration | `editor.md`                     |
| Alasan desain lintas UI  | `../design/04-design-system.md` |
| Pola list/detail         | `../features/_shared/`          |

## Changelog

| Tanggal    | Perubahan                                   |
| ---------- | ------------------------------------------- |
| 2026-09-03 | fork init                                   |
| 2026-10-27 | rewrite — reading order, boundary, app rail |
