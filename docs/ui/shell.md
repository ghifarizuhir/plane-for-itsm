---
status: current
last_verified: 2026-10-27
source_hash: sha256:90ee945b70aac6dafe439a58cc771b25e96fd0992ec208724fe96cc14a59b9c8
source_paths:
  - apps/web/core/components/navigation
  - apps/web/core/components/workspace/sidebar
  - apps/web/core/components/sidebar
---

# Shell

Status: **Current.**

## Structure

```text
AppShell (desktop)
├── App rail
│   ├── Workspace switcher
│   ├── Work / Procedure / Galileo
│   ├── AI assistant, Inbox, Help, Account
│   └── Settings
├── Workspace sidebar
└── Page content
```

Top navigation hanya tampil di mobile; app rail menggantikannya di desktop.

## App rail

| Item      | Tujuan                                          |
| --------- | ----------------------------------------------- |
| Work      | `/:workspaceSlug` dan surface pekerjaan         |
| Procedure | `/:workspaceSlug/docs`                          |
| Galileo   | `/:workspaceSlug/galileo`; hanya jika LLM aktif |
| Settings  | `/:workspaceSlug/settings`                      |

- Rail selalu terlihat di desktop; tidak ada undock.
- Workspace switcher berada di paling atas.
- Mode aktif ditentukan dari route.
- Utility (AI assistant, Inbox, Help, Account) berada di bawah grup mode.
- Settings berada di paling bawah setelah divider.
- Rail memakai density compact: ikon 18px dan label 10px.
- Label dan ikon memakai i18n dan `@plane/propel`.
- Search bar tidak ditampilkan; command palette tetap tersedia lewat shortcut.

## Workspace sidebar

- Item berubah sesuai mode (Work/Procedure/Galileo).
- Sidebar dapat di-resize dan di-collapse.
- Feature flag workspace dapat menyembunyikan item.
- Application list dan scope selector berada di sidebar Work.

## Top navigation

- Hanya tampil di mobile.
- Workspace switcher dan menu mode (Work/Procedure/Galileo).
- Toggle right assistant Galileo, Inbox, Help, dan Account.
- Tidak ada search bar.

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

| Tanggal    | Perubahan                                                |
| ---------- | -------------------------------------------------------- |
| 2026-10-27 | rewrite — app rail, mode, mobile, accessibility          |
| 2026-10-27 | compact rail; top bar pindah ke rail; search bar dihapus |
