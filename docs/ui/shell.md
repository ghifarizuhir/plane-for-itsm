# Shell — AppShell + Sidebar + Topbar

Status: **Draft.**

File: `apps/web/core/layouts/*` + `apps/web/app/layout.tsx` + `packages/ui/src/*`.

Adaptasi dari `terra/docs/ui/sidebar.md:1` — Terra punya 180px↔44px collapsible sidebar dengan `border-r /70` structural seam + ghost rows; Plane pakai pola serupa di `core/layouts` (cek `apps/web/core/layouts` actual vs doc ini saat divergen, doc ini menang untuk runtime).

---

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│ Topbar (workspace + project switcher + search Cmd+K)         │  sticky h-9 border-b /70, bg-theme-bg/85 backdrop-blur
├──────────┬───────────────────────────────────────────────────┤
│ Sidebar  │ Page Content (flex-1 min-h-0 overflow-hidden)     │
│ 180↔44px │  List two-tier / Detail EntityPageShell / Board   │
│ border-r │                                                   │
│ /70      │                                                   │
└──────────┴───────────────────────────────────────────────────┘
```

- **Topbar:** workspace switcher + project switcher + Cmd+K + user menu. Height `h-9` (36px) fixed — sama contract `terra/ui/design-tokens.md:271` Tier-1 strip.
- **Sidebar:** groups — Pulse/Home → Work Items/Cycles/Modules/Views/Pages/Analytics → ITSM (Incidents/Service Map bila fork) → Settings (workspace admin) → god-mode link (instance admin).
- **Page:** `flex h-full min-h-0 flex-col` — tanpa `max-w` wrapper; header self-contained `px-6 sm:px-8`.

---

## Sidebar Spec

### Container

```
<aside class="self-stretch shrink-0 flex flex-col border-r border-theme-border/70
             transition-all duration-300 ease-in-out overflow-hidden
             w-[180px] | w-[44px] | w-0 opacity-0">
```

- `w-[180px]` expanded, `w-[44px]` collapsed (icons only), `w-0` hidden.
- `border-r border-theme-border/70` structural seam — tidak ada saat hidden.
- `self-stretch` — docked bottom, flush top (gutter dihapus untuk edge-to-edge).

### Nav Item

```
<NavLink class="flex items-center rounded-chrome text-xs font-semibold mx-2 px-2 py-1
                text-theme-muted hover:bg-theme-border/40 hover:text-theme-text
                aria-[current=page]:bg-wash-4 aria-[current=page]:text-theme-text
                aria-[current=page]:shadow-[inset_2px_0_0_0_var(--foreground)]">
  <Icon class="size-3.5 shrink-0" strokeWidth={2.25} />
  <span class="ml-1.5 truncate">Label</span>
</NavLink>
```

- Radius `rounded-chrome` (0px) structural — controls tetap `rounded-lg`.
- Active: `bg-wash-4` + left inset bar `shadow-[inset_2px_0_0_0_var(--foreground)]` (monochrome, kedua theme).
- Icon `lucide-react` — actual Plane nav; mapping ITSM (`Flame`/`Bug`/`Share2`) hanya referensi Terra, belum dipakai.

### Groups

| Group       | Items                                                   | Visibility                      |
| ----------- | ------------------------------------------------------- | ------------------------------- |
| Workspace   | Workspace switcher (~Apps row)                          | always                          |
| Main        | Inbox, Work Items, Cycles, Modules, Views, Pages        | always                          |
| ITSM (fork) | (belum ada di kode — propose di `features/_backlog.md`) | fork — tambah bila ITSM enabled |
| Admin       | Settings (workspace admin), god-mode (instance admin)   | role-gated                      |

---

## Topbar Spec

```
<header class="sticky top-0 z-30 flex h-9 items-center gap-2 border-b border-theme-border/70
               bg-theme-bg/85 backdrop-blur-sm px-6 sm:px-8">
  <Breadcrumbs /> <Filters /> <Search /> <Actions />
</header>
```

- Search expand `h-8 w-40` (icon 14px `size={14}`); view toggle 12px `size={12}`; create "+" `h-7 w-7` + icon 14px — kontrak `terra/ui/design-tokens.md:271` §7.5 ListPageHeader (pakai langsung bila ITSM list ramping ke two-tier).

---

## Resolved

- Sidebar 14px + `strokeWidth 2.25` adalah scale referensi untuk header actions (Terra §5.2).
- `rounded-chrome` untuk structural, `rounded-lg` untuk controls — jangan tukar.

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
