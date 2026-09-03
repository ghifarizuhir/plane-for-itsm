# 04 — Design System

Status: **Draft.**

References: [`01-architecture.md`](./01-architecture.md), `../ui/design-tokens.md`, `../ui/shell.md`.

Stack: Tailwind 4 + `@plane/tailwind-config` + `@plane/ui` (Radix) + `lucide-react` + `motion`. Adaptasi dari `terra/docs/design/08-design-system.md:1` (Terra banyak `linear-*` tokens — Plane pakai `@plane/ui` tokens).

---

## Design Tokens

Tokens di `packages/tailwind-config` + `packages/ui/styles` via Tailwind `@theme`. **Wajib pakai token** — jangan hardcode hex.

### Core — Plane palette (actual — cek `packages/tailwind-config/src` + `packages/ui/styles`)

| Token                 | Tailwind                                       | Pemakaian                                       |
| --------------------- | ---------------------------------------------- | ----------------------------------------------- |
| `bg-theme-bg`         | `oklch(0.135 0 0)` dark / `0.985` light        | Background halaman                              |
| `bg-theme-card`       | `oklch(0.205 ...)`                             | Card, modal, panel                              |
| `border-theme-border` | `oklch(1 0 0 / 10%)`                           | Border default                                  |
| `text-theme-text`     | `oklch(0.985 0 0)`                             | Teks utama                                      |
| `text-theme-muted`    | `oklch(0.708 0 0)`                             | Teks sekunder                                   |
| `wash-1..6`           | `oklch(1 0 0 / 3%..60%)` / `0 0 0 / ...` light | Hover/overlay (flip per mode)                   |
| `rounded-chrome: 0px` | `--radius-chrome`                              | Structural chrome (sidebar nav rows, rail tabs) |
| `rounded-lg: 8px`     | `--radius: 0.5rem`                             | Button, card, input                             |

> **Jangan copy `linear-*` Terra (`terra/ui/design-tokens.md:7`) mentah** — Plane sudah punya `theme-*`/`wash-*`. Sinkronkan via `ui/design-tokens.md` (source of truth aktual).

### Entity differentiation (actual: Work Item types)

Work Items tidak dibedakan via warna — monochrome + icon + label + `project.identifier` prefix (e.g. `WEB-123`). Custom types via `IssueType` (`issue_type.py:11` — `name` + `logo_props`) — bukan prefix ITSM `INC-/PRB-/CHG-` (itu propose, lihat `_backlog.md`).

### Radius / Shadow / Font

- Radius: `rounded-chrome (0px)` structural, `rounded-lg (8px)` controls, `rounded-full` pills. — `terra/docs/ui/design-tokens.md:131`.
- Shadow: non-floating `shadow-xs + ring-1 ring-theme-border/30`; overlay `shadow-xl/2xl` — jangan ubah tanpa review.
- Font: `Inter` (sans, dari `@fontsource/inter` + `@fontsource-variable/inter` — `pnpm-workspace.yaml:18`) + `JetBrains Mono` / `IBM Plex Mono` untuk ID/timestamp.

---

## Aesthetic Primitives

| Primitive       | Pemakaian                                                     | Token                                         |
| --------------- | ------------------------------------------------------------- | --------------------------------------------- |
| Icon Chip       | eyebrow header, section divider (`h-5 w-5 rounded-md border`) | `border-theme-border/70 bg-theme-bg/60`       |
| Eyebrow         | `UPPERCASE tracking-[0.2em] text-[11px]` di atas H1           | `font-mono text-theme-muted/80`               |
| Page H1         | `text-xl font-bold tracking-tight` (list) / `text-4xl` (hero) | `text-theme-text`                             |
| Section Divider | icon chip + `UPPERCASE` + `h-px flex-1` line                  | `bg-theme-border/50`                          |
| Meta Row        | `grid grid-cols-[140px_1fr] gap-y-5`                          | label `text-[10px] uppercase tracking-widest` |
| Mono Chip       | `<code> rounded-md border bg-theme-bg/60`                     | `font-mono text-[11px]` untuk ID `INC-...`    |

---

## Page Anatomy

```
┌──────────────────────────────────────────────────┐
│ Tier-1: Nav Strip + Filter (sticky, translucent)  │  ← breadcrumbs + filter + actions, border-b /70
├──────────────────────────────────────────────────┤
│ Tier-2: Page Header (bila ada title)              │  ← IconChip + kicker + H1 + subtitle + count chip
├──────────────────────────────────────────────────┤
│ Primary Surface                                   │  ← DataTable / card grid / chain stack
│  Footer: pagination mono "1–N of M"               │
└──────────────────────────────────────────────────┘
```

- List routes: two-tier sticky via `ListPageHeader` (contek `terra/features/_shared/list.md` two-tier) — tapi pakai Plane `packages/ui` primitives, bukan custom.
- Detail routes: `EntityPageShell` + `PageChrome` (one-tier breadcrumb/action strip).
- Service Map: dedicated graph/editor chrome (`@xyflow/react` bila pakai, atau `@hocuspocus` untuk Pages).

---

## Component Inventory (Plane actual — `packages/ui/src/*`)

| Path                         | Status | Catatan                                            |
| ---------------------------- | ------ | -------------------------------------------------- |
| `packages/ui/src/button.tsx` | ✅     | Variants: primary/secondary/ghost/icon             |
| `packages/ui/src/avatar/*`   | ✅     | —                                                  |
| `packages/ui/src/dropdown*`  | ✅     | Radix-based                                        |
| `packages/ui/src/tables/*`   | ✅     | `@tanstack/react-table` (`pnpm-workspace.yaml:49`) |
| `packages/ui/src/editor/*`   | —      | TipTip ada di `packages/editor`, bukan `ui`        |
| `apps/web/core/layouts/*`    | ✅     | AppShell, Sidebar                                  |

> Terra extract `pulse-tile`, `summary-tile`, `filter-rail` ke `components/data/*` (`terra/docs/design/08-design-system.md:599`). Plane sudah punya `@plane/ui` — **jangan duplicate**; extend `@plane/ui` via `packages/ui/src/*` + Storybook (`pnpm --filter=@plane/ui storybook` — `AGENTS.md:11`).

---

## Resolved Decisions

| #   | Topik        | Keputusan                                                                                                    |
| --- | ------------ | ------------------------------------------------------------------------------------------------------------ |
| 1   | Tokens       | **`theme-*`/`wash-*`** (Plane) — bukan `linear-*` Terra mentah; sinkron via `ui/design-tokens.md`            |
| 2   | Entity color | **Monochrome + icon/prefix** — hierarchy via opacity, bukan hue                                              |
| 3   | Primitives   | **Extend `@plane/ui`** — jangan bikin `@terra/ui` terpisah                                                   |
| 4   | Page anatomy | **Two-tier sticky** untuk list, `EntityPageShell` untuk detail — konsisten Issues/Cycles/Modules/Views/Pages |

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
