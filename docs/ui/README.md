# UI Docs — Component Specifications

Dokumen teknis untuk global shell + editor. Source of truth untuk implementasi — bukan aspirasi. Kalau bertentangan dengan `design/04-design-system.md`, **doc di sini menang** untuk actual runtime (CSS tokens, font, icon).

Adaptasi dari `terra/docs/ui/README.md:1`.

---

## Reading Order

### Global Shell

| #   | Doc                                      | Status    | Content                                                                                                                    |
| --- | ---------------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------- |
| 1   | [`design-tokens.md`](./design-tokens.md) | ✅ Stable | Color tokens, opacity, radius, font, icon, motion, spacing — actual CSS (`packages/tailwind-config`, `packages/ui/styles`) |
| 2   | [`shell.md`](./shell.md)                 | ✅ Draft  | AppShell + Sidebar + Topbar — `apps/web/core/layouts`                                                                      |
| 3   | [`editor.md`](./editor.md)               | ✅ Draft  | TipTap editor (`packages/editor`) + Yjs collaboration (`apps/live`)                                                        |

### Page Patterns

| #   | Doc | Status | Content                                                                                     |
| --- | --- | ------ | ------------------------------------------------------------------------------------------- |
| 4   | —   | —      | Service Map graph, DataTable patterns — lihat `features/_shared/list.md` + `detail-page.md` |

---

## Organisasi

```
ui/
├── README.md              ← ini
├── design-tokens.md       ← token definitions (actual CSS, bukan aspirasi)
├── shell.md               ← AppShell + Sidebar + Topbar
└── editor.md              ← TipTap + Yjs editor
```

> Terra `ui/` punya `sidebar.md`, `globaltopbar.md` (removed), `incidents.md`, `services.md`, `audit/known-issues-*.md` (`terra/docs/ui/README.md:9`). Plane ramping — `shell.md` gabung sidebar+topbar; `incidents.md`/`services.md` tidak duplikasi di `ui/` (sudah di `features/`); `audit/` ditambah bila perlu tapi tidak pre-build.

---

## Conventions

1. **Technical, not aspirational.** Classes, tokens, struktur DOM yang benar-benar dipakai — bukan target ideal.
2. **Status per doc:** `✅ Stable` (sesuai codebase) / `📝 Draft`.
3. **Cross-reference `design-tokens.md`** untuk semua token — jangan duplikasi definisi.
4. **Actual file refs** (`apps/web/core/layouts/*`, `packages/ui/src/*`) — bukan `apps/web/src/legacy`.

---

## Content Boundary

| Jenis konten                            | Lokasi                                         |
| --------------------------------------- | ---------------------------------------------- |
| Token definitions (color, radius, font) | `design-tokens.md`                             |
| Component anatomy + exact classes       | `shell.md`, `editor.md`                        |
| Known issues                            | `audit/known-issues-<component>.md` (bila ada) |
| Visual intent / reasoning               | `../design/04-design-system.md`                |

---

---

## Changelog

| Date       | Change    |
| ---------- | --------- |
| 2026-09-03 | fork init |
