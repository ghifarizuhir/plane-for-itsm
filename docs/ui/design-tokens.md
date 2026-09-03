# Design Tokens — Plane for ITSM (Technical)

> Status: Source of truth untuk Shell + Editor. Adaptasi dari `terra/docs/ui/design-tokens.md:1` — Terra pakai `linear-*` legacy, Plane pakai `theme-*`/`wash-*` (sinkron dengan `design/04-design-system.md`). Stack: Tailwind 4 `@theme` + CSS vars (`packages/tailwind-config`, `packages/ui/styles`).

> Font: `Inter` (sans, `@fontsource/inter` + `@fontsource-variable/inter` — `pnpm-workspace.yaml:18`) + `IBM Plex Mono` (`@fontsource/ibm-plex-mono:20`) / `JetBrains Mono`. Icon: `lucide-react` (`pnpm-workspace.yaml:131`) — tunggal.

---

## 1. Color Tokens

Semua token via `@theme inline` di `packages/ui/styles` + `packages/tailwind-config`. **Wajib pakai token** — jangan hardcode hex.

### 1.1 Core

| Token                 | CSS var                           | Pemakaian                          |
| --------------------- | --------------------------------- | ---------------------------------- |
| `bg-theme-bg`         | `--background`                    | Background halaman (AppShell root) |
| `bg-theme-card`       | `--card`                          | Card, modal, panel, dropdown       |
| `border-theme-border` | `--border` (`oklch(1 0 0 / 10%)`) | Border default, divider            |
| `text-theme-text`     | `--foreground`                    | Teks utama                         |
| `text-theme-muted`    | `--muted-foreground`              | Teks sekunder, label               |

### 1.2 Light Mode

App dark-by-default; light opt-in via `data-theme="light"` pada `<html>` (selector `:root[data-theme='light']`). `wash-*` flip otomatis:

| Token        | Dark                 | Light                | Pemakaian                |
| ------------ | -------------------- | -------------------- | ------------------------ |
| `wash-1`     | `oklch(1 0 0 / 3%)`  | `oklch(0 0 0 / 4%)`  | Hover subtle             |
| `wash-2`     | `oklch(1 0 0 / 6%)`  | `oklch(0 0 0 / 7%)`  | Row hover                |
| `wash-4`     | `oklch(1 0 0 / 15%)` | `oklch(0 0 0 / 18%)` | Active item              |
| `wash-solid` | `oklch(1 0 0 / 90%)` | `oklch(1 0 0)`       | Send solid (tetap white) |

---

## 2. Opacity & Surface

| Class                    | Pemakaian                                                                               |
| ------------------------ | --------------------------------------------------------------------------------------- |
| `border-theme-border/70` | **Structural seams** — sidebar `border-r`, bottom bar `border-t`, header/row `border-b` |
| `border-theme-border/30` | Nested divider, rail row `border-b`                                                     |
| `bg-theme-card/30`       | Inner card surface (metadata cards)                                                     |
| `bg-wash-4`              | Active nav item                                                                         |

---

## 3. Radius

| Token            | Value                     | Pemakaian                                           |
| ---------------- | ------------------------- | --------------------------------------------------- |
| `rounded-chrome` | `0px` (`--radius-chrome`) | **Structural chrome** — sidebar nav rows, rail tabs |
| `rounded-lg`     | `0.5rem` (8px)            | Button, input, card                                 |
| `rounded-xl`     | `0.7rem`                  | Page cards                                          |
| `rounded-full`   | `9999px`                  | Pills, avatar                                       |

> `packages/ui` controls tetap `rounded-lg`; structural shell pakai `rounded-chrome` (boxy console) — jangan ubah controls jadi square.

---

## 4. Font Scale

| Use            | Size      | Class                                                  |
| -------------- | --------- | ------------------------------------------------------ |
| Micro kicker   | 10px      | `text-[10px] font-semibold uppercase tracking-[0.2em]` |
| Filter pill    | 11px      | `text-[11px]`                                          |
| Body/nav       | 12px      | `text-xs font-semibold` (nav), `text-sm` body          |
| Page H1 (list) | `text-xl` | `text-xl font-bold tracking-tight`                     |

Font mono (`JetBrains Mono` / `IBM Plex Mono`) untuk ID `WEB-123`, timestamp, count chip.

---

## 5. Icon Conventions

- **Library tunggal:** `lucide-react` — `size-3.5` (14px) sidebar nav + `strokeWidth={2.25}`; header actions juga 14px. List chrome kompak `size={12}` untuk view toggle (Plan vs Terra sama — jangan campur heroicons).

---

## 6. Motion

| Easing     | Bezier            | Pakai                                           |
| ---------- | ----------------- | ----------------------------------------------- |
| Smooth out | `[0.16,1,0.3,1]`  | Entrance, stagger                               |
| iOS slide  | `[0.32,0.72,0,1]` | Sidebar `duration-300 ease-in-out`, panel slide |

Sidebar width animation `transition-all duration-300 ease-in-out` — label `max-w-0 → max-w-[200px]` + `opacity`.

---

---

## Changelog

| Date       | Change                                        |
| ---------- | --------------------------------------------- |
| 2026-09-03 | fork init — contek terra `design-tokens.md:1` |
