---
status: current
last_verified: 2026-10-27
source_hash: sha256:123fefe4cf7bde00daa3522a83dc4ebf6f677481ba5b1798b8bbe6515083ee27
source_paths:
  - apps/web/core/components/navigation
  - apps/web/core/components/workspace/sidebar
  - apps/web/core/components/sidebar
---

# Shell

Status: **Current.**

## Structure

```text
AppShell
├── App rail (desktop)
├── Workspace sidebar
├── Top navigation
└── Page content
```

Pada mobile, app rail diganti menu mode di top navigation.

## App rail

| Item      | Tujuan                                          |
| --------- | ----------------------------------------------- |
| Work      | `/:workspaceSlug` dan surface pekerjaan         |
| Procedure | `/:workspaceSlug/docs`                          |
| Galileo   | `/:workspaceSlug/galileo`; hanya jika LLM aktif |
| Settings  | `/:workspaceSlug/settings`                      |

- Rail selalu terlihat di desktop; tidak ada undock.
- Mode aktif ditentukan dari route.
- Settings berada di bawah divider.
- Label dan ikon memakai i18n dan `@plane/propel`.

## Workspace sidebar

- Item berubah sesuai mode (Work/Procedure/Galileo).
- Sidebar dapat di-resize dan di-collapse.
- Feature flag workspace dapat menyembunyikan item.
- Application list dan scope selector berada di sidebar Work.

## Top navigation

- Breadcrumb dan judul halaman.
- Application scope selector untuk halaman agregat.
- Search/command palette.
- Aksi halaman (create, filter, view).
- Toggle right assistant Galileo.

## Mobile

- Rail tidak ditampilkan.
- Menu mode ada di top navigation.
- Sidebar menjadi drawer.
- Detail/peek menjadi sheet atau halaman penuh.

## Accessibility

- Semua item rail/sidebar dapat difokuskan keyboard.
- Escape menutup drawer/overlay.
- Focus kembali ke trigger setelah overlay ditutup.
- Hormati `prefers-reduced-motion`.

## Related

- [`design-tokens.md`](./design-tokens.md)
- [`../design/04-design-system.md`](../design/04-design-system.md)
- [`../design/07-routing-and-navigation.md`](../design/07-routing-and-navigation.md)

## Changelog

| Tanggal    | Perubahan                                       |
| ---------- | ----------------------------------------------- |
| 2026-10-27 | rewrite — app rail, mode, mobile, accessibility |
