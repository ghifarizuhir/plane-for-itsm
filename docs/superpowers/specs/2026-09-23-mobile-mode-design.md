# Mobile Mode (Responsive Web for Phones) — Design

Date: 2026-09-23
Status: Approved (pending user review of this spec)
Scope: Frontend-only (`apps/web`). No backend, no new dependencies, no PWA/offline.

## Goal

Make the web app usable on phones (`<768px`) for three flows:

1. **View & update work items** — browse lists, open a work item, change status / assignee / priority, basic filtering.
2. **Comments & attachments** — read/write comments, view and upload attachments.
3. **Navigation & search** — workspace/project switching, sidebar navigation, work-item search.

Everything else (create work item, settings/admin, services/intake, pages, board/gantt/spreadsheet) may stay desktop-only. The target is **minimal usable**, not desktop parity.

## Decisions (brainstormed & approved)

1. **Approach A1** — polish the existing partial responsive behavior; no separate mobile shell, no route restructure.
2. **Breakpoint** — `md` (768px). Mobile means viewport `<768px`, matching the existing sidebar auto-collapse logic (`sidebar-wrapper.tsx:38-47`).
3. **Viewport vs UA rule** — layout and visibility decisions use **viewport** (new `matchMedia` hook); hover/drag/tooltip interaction gating keeps using **UA** (`usePlatformOS`, unchanged across ~126 files).
4. **Work-item layout on mobile = List only.** A persisted desktop layout (kanban/calendar/gantt/spreadsheet) falls back to list **render-only** and is never written back to the store.
5. **Peek overview stays the mobile work-item detail surface** — side-peek is already `w-full` at phone width (`view.tsx:126`); only padding and the fixed-width secondary column need work.
6. **No PWA/offline work** — manifests and apple meta already exist (`app/root.tsx:44-72`); the service worker stays unregistered.
7. **Desktop safety first** — every new behavior is gated by `max-md:` or the viewport hook; desktop class strings are left untouched where possible; a desktop regression checklist is part of QA.

## Current state (assets to reuse)

- **10 mobile header components** already exist per route: project issues list, cycle detail/list, module detail/list, views list, projects list/archives, profile issues tab, services list, inbox issue — plus the settings mobile nav (`core/components/settings/mobile/nav.tsx`).
- **Modal bottom-sheet pattern** on small screens (`packages/ui/src/modals/constants.ts:8-22`, `modal-core.tsx:52-56`).
- **Sidebar overlay** — auto-collapses `<768px`, closes on outside click (`sidebar-wrapper.tsx:38-47`), absolute when mobile (`resizable-sidebar.tsx:186,196`), reopen toggle rendered in the app header (`extended-app-header.tsx:29`).
- **List rows are responsive** — flex-col below `md`, property dropdowns pre-rendered (`renderByDefault={isMobile}`, `properties/all-properties.tsx`), mobile quick actions (`list/block.tsx:182,272-276,285-296`).
- **Kanban/calendar/spreadsheet already redirect** to the full work-item page on mobile via `use-issue-peek-overview-redirection.tsx:26-52` (list does not, it peeks).
- **Comments** — sticky comment box on mobile (`comment-create.tsx:95`), responsive attachment grid (`attachment/root.tsx:30`), card truncation (`comment-block.tsx:30-45`).
- **Power-K modal is already phone-usable** (`power-k/ui/modal/wrapper.tsx:136,146`), but only keyboard-triggered (`Cmd/Ctrl+K`).
- **Client-only app** — `apps/web/react-router.config.ts` sets `ssr: false`, so there is no SSR/hydration risk for viewport-dependent rendering.

## Architecture

### Viewport hook

New `apps/web/core/hooks/use-mobile-viewport.ts`:

- `matchMedia("(max-width: 767px)")`, guarded for `typeof window`, subscribed to change events.
- Returns `{ isMobileViewport }`.
- This is the single viewport signal for all layout decisions in this spec.

### Viewport vs UA rule

| Concern                                     | Signal            | Hook                        |
| ------------------------------------------- | ----------------- | --------------------------- |
| Layout, visibility, positioning, fallback   | viewport `<768px` | `useMobileViewport` (new)   |
| Hover/drag/tooltip/touch interaction gating | user agent        | `usePlatformOS` (unchanged) |

The existing inconsistency (`resizable-sidebar.tsx` uses UA while `sidebar-wrapper.tsx` uses `innerWidth`) is resolved by moving sidebar **layout** decisions to the viewport hook. UA-based interaction gating elsewhere is deliberately left alone.

### Render-only layout fallback

Shared helper (e.g. `resolveWorkItemLayout(layout, isMobileViewport)` in `core/components/issues/issue-layouts/utils.ts`):

- Returns `LIST` when `isMobileViewport` is true and the persisted layout is any other value; otherwise returns the persisted layout unchanged.
- Applied where each layout root derives `activeLayout` from `workItemFilters?.displayFilters?.layout`: `project-layout-root.tsx:54`, `cycle-layout-root.tsx:65`, `module-layout-root.tsx:55`, `all-issue-layout-root.tsx:53`, `project-view-layout-root.tsx:57`. (The archived root is already list-only — `archived-issue-layout-root.tsx:60`.)
- **Never calls `updateFilters`** — the user's desktop layout preference is preserved in the store and restored when the viewport widens.

## Changes by area

### 1. Navigation shell

- **App rail**: hide below `md` in `WorkspaceContentWrapper` (`content-wrapper.tsx:29`) using the viewport hook — returns ~48px of screen width.
- **Sidebar**: switch the UA-based `isMobile` in `resizable-sidebar.tsx:186,196` to the viewport hook; add a backdrop that renders only when `isMobileViewport && !sidebarCollapsed` and closes the sidebar on tap; close on Escape; keep the header toggle (`extended-app-header.tsx:29`) as the reopen affordance.
- **Top nav** (`top-navigation-root.tsx:48-88`):
  - Hide the inline Power-K field below `md` (fixed `w-[364px]`/`w-[554px]`, `top-nav-power-k.tsx:212-214` — overflows phones).
  - Replace it with a search icon button that calls `togglePowerKModal(true)` from `usePowerK`. The Power-K modal is already fluid (`max-w-2xl w-full` with `p-4`, `power-k/ui/modal/wrapper.tsx:136,146`) — verify at phone width and fix any inner fixed widths.
  - Cap the workspace menu width (`max-w-[calc(100vw-2rem)]`, currently fixed `w-[19rem]`).
  - Keep inbox, help, user menu.
- Keep the existing per-route mobile headers as the page-level chrome.

### 2. Work items (view & update)

- **Layout list-only on mobile**:
  - `MobileLayoutSelection` options are filtered to `[LIST]` when `isMobileViewport` (`filters.tsx:37-43,104-110`).
  - The render-only fallback (above) covers a persisted desktop layout.
  - Desktop keeps all five options and behavior unchanged.
- **Row click keeps opening the peek** (no redirection change for list):
  - Content padding `px-8` → `px-4 md:px-8` (`view.tsx:177`).
  - Fixed secondary column `!w-[400px]` in modal/full-screen modes → `w-full md:!w-[400px]` so it stacks at phone width (`view.tsx:250`).
  - Verify the close control is reachable and the peek header renders below the top nav without z-index conflict (`z-[25]` peek vs `z-[27]` top nav).
- **Filters**:
  - Add `WorkItemFiltersToggle` to the issues mobile header (`app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/issues/(list)/mobile-header.tsx`), mirroring the desktop header (`issues/header.tsx:108`).
  - Ensure `WorkItemFiltersRow` (rendered in the layout roots) wraps or scrolls horizontally at phone width.
  - Make the legacy `FiltersDropdown` panel fluid on mobile (`w-[calc(100vw-1rem)] max-h-[70vh]`), keeping the desktop `w-[18.75rem] max-h-[30rem]` (`filters/header/helpers/dropdown.tsx:101-110`).
- **Property updates via peek** already pre-render dropdowns on mobile (`properties/all-properties.tsx`) — QA only.

### 3. Comments & attachments

- Pass the touch flag to the editor based on `(pointer: coarse)` rather than viewport, so mouse users in narrow windows never get the touch editor (`editor-wrapper.tsx:39,68,96`; `packages/editor` already supports `isTouchDevice`).
- Add `env(safe-area-inset-bottom)` padding to the sticky comment box (`comment-create.tsx:95`).
- Make attachment/image previews fluid at phone width (no fixed-width panels).
- QA camera/gallery upload from a phone browser.

## Desktop regression guardrails

1. **Render-only fallback** — mobile adaptation never writes to MobX stores (the one hard rule; a persisted-layout overwrite would be a real desktop regression).
2. **`max-md:` overrides** — new mobile-only styles are added as `max-md:` variants; existing desktop class strings are not rewritten. Tailwind 4.1.17 supports this variant.
3. **Backdrop only on mobile viewport** — desktop never renders the sidebar backdrop.
4. **Touch editor gating via `(pointer: coarse)`**, not viewport width.
5. **Desktop regression checklist** (1440 / 1280 / 1024px) — navigation and app rail, sidebar resize/peek/collapse, inline Power-K search, all five layouts including spreadsheet/gantt/kanban, all three peek modes, filter dropdown, comments, attachments.

## Testing & verification

- **Unit tests** (`apps/web` vitest; `vitest.config.ts` includes `core/**/*.test.ts`, `environment: node`; per-file `// @vitest-environment jsdom` where DOM APIs are needed):
  - `resolveWorkItemLayout` fallback logic (all layout values × viewport states, and "does not mutate input").
  - `useMobileViewport` with a mocked `matchMedia`.
- **Manual mobile QA matrix**: iPhone Safari (notch/safe-area, on-screen keyboard, rotation), Android Chrome. Flows: sidebar drawer + backdrop + Escape, search icon → Power-K, list → peek → update status/assignee/priority, comment + attachment upload, filters.
- **Desktop regression checklist** (above) run before sign-off.
- **Gates**: `pnpm check:types`, `pnpm check:lint`, `pnpm --filter=web test`; then `pnpm --filter=web build` + `systemctl --user restart plane-web-prod.service` for a tunnel smoke test (per `AGENTS.md`).
- No new e2e or visual-regression framework in this iteration (none exists in the repo).

## Risks

| Risk                                                                  | Mitigation                                                                                                        |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Fork divergence — touched files also change upstream, merge conflicts | Keep diffs small; prefer new hook/helper over rewriting large files; avoid restructuring `peek-overview/view.tsx` |
| UA vs viewport inconsistency (126 files use UA)                       | One viewport hook for layout; written rule in this spec                                                           |
| No visual-regression automation                                       | Desktop regression checklist as a required QA gate                                                                |
| Scope creep (create/board/settings requests)                          | Out-of-scope list below; follow-ups tracked separately                                                            |
| iPhone keyboard/notch shifting the sticky comment box                 | `dvh` + `safe-area-inset-bottom`, dedicated device QA                                                             |
| Narrow desktop windows (`<768px`) now get the overlay sidebar         | Intended behavior change (fixes UA/width mismatch); documented in QA checklist                                    |

## Effort breakdown

| #   | Task                                                                                           | Estimate                                     |
| --- | ---------------------------------------------------------------------------------------------- | -------------------------------------------- |
| 1   | Viewport hook + viewport/UA rule + unit test                                                   | 1 day                                        |
| 2   | Nav shell: app rail hide, sidebar backdrop/Escape, top nav search icon + Power-K mobile        | 3-4 days                                     |
| 3   | Work items: list-only + render-only fallback, peek padding/column, filter toggle + fluid panel | 3-4 days                                     |
| 4   | Comments/attachments/editor touch                                                              | 2-3 days                                     |
| 5   | Device QA + fixes + desktop regression pass                                                    | 2-3 days                                     |
|     | **Total**                                                                                      | **~11-15 working days (2-3 calendar weeks)** |

## Out of scope (YAGNI)

- Create work item on mobile.
- Settings/admin/profile, services/intake, pages, analytics, home dashboard, notifications list.
- Kanban/calendar/gantt/spreadsheet rendering on mobile (existing redirects to the full work-item page remain as the safety net).
- Bottom navigation, swipe gestures, pull-to-refresh.
- PWA/offline/service worker registration, push notifications, installability.
- Tablet-specific layout.
- Native mobile app.
- New e2e or visual-regression test infrastructure.
