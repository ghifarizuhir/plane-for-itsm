# Cycles (Sprints)

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/cycles` (list) + `:workspaceSlug/projects/:projectId/cycles/:cycleId` (detail) — lihat `apps/web/app/routes/core.ts:157-163`, `:149-155`
Share: CORE

## Intent

Time-boxed sprint per project: kelompokkan work items ke periode (start–end date), pantau progress/burn-down, lalu transfer sisa kerja ke cycle berikut. Dari sudut user: "apa yang harus selesai sprint ini, dan seberapa jauh progresnya?"

## Current State (snapshot kode)

- List page: `projects/(detail)/[projectId]/cycles/(list)/page.tsx:37-137` — guard `cycle_view === false` → empty-state + link ke project features; render `CycleCreateUpdateModal` + `CyclesView` + `CycleAppliedFiltersList`.
- Grouping list: `apps/web/core/components/cycles/cycles-view.tsx:32-36` split `filteredCycleIds` (active) vs `completed` vs `upcoming`; render `cycles/list/root.tsx:29-70` → `ActiveCycleRoot` + Disclosure Upcoming + Completed. Item: `list/cycles-list-item.tsx`, `list/cycles-list-map.tsx`, header `list/cycle-list-group-header.tsx`.
- Detail page: `cycles/(detail)/[cycleId]/page.tsx:68-86` — **reuse layout issues** `CycleLayoutRoot` (`issues/issue-layouts/roots/cycle-layout-root`) + sidebar kanan `CycleDetailsSidebar` (collapsible, state di localStorage `cycle_sidebar_collapsed`).
- Detail analytics: `cycles/analytics-sidebar/root.tsx` → `sidebar-header.tsx`, `sidebar-chart.tsx:26-49` (`ProgressChart` burn-down/distribution, toggle points vs count via `EstimateTypeDropdown`, data dari `progress_snapshot`, validasi `issue-progress.tsx`), `progress-stats.tsx`, `issue-progress.tsx`, `sidebar-details.tsx` (date range, owned_by).
- Active cycle hero: `cycles/active-cycle/root.tsx` (`progress.tsx`, `cycle-stats.tsx`, `productivity.tsx`) + `hooks/use-cycles-details.ts` (fetch detail + progress + analytics).
- Create/edit: `cycles/modal.tsx` (`CycleCreateUpdateModal`) + `cycles/form.tsx` (name, description, start/end date, validasi `cycleDateCheck`); delete `delete-modal.tsx`; archive/restore `archived-cycles/modal.tsx`.
- Transfer unfinished: `cycles/transfer-issues-modal.tsx:29-52` → `transferIssuesFromCycle(EIssuesStoreType.CYCLE)` + `POST .../cycles/:id/transfer-issues/` (`core/services/cycle.service.ts:175-183`, payload `{new_cycle_id}`); varian row `transfer-issues.tsx`.
- Quick actions: `cycles/quick-actions.tsx:36-56` (`CycleQuickActions`: edit, copy link, archive, delete, restore) via `useCycleMenuItems`; archive list `archived-cycles/root.tsx` + `view.tsx` + `header.tsx`.
- Working: grouping active/upcoming/completed, CRUD + date validation, transfer issues, burn-down/progress sidebar, archive/restore, favorites, applied-filters list.
- Stub: —
- Missing (ITSM fork): tidak ada — tidak ada konsep sprint-due SLA incident di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Status dihitung, bukan disimpan

Koreksi penting vs asumsi umum: model `Cycle` **tidak punya field status**. Status current/upcoming/completed dihitung frontend dari tanggal via `orderCycles` / `shouldFilterCycle` (`packages/utils/src/cycle.ts:21,50`). Tidak ada tombol "complete" eksplisit — cycle lewat `end_date` otomatis masuk grup Completed (day-based).

### Store (MobX)

- `apps/web/core/store/cycle.store.ts:31-96` `class CycleStore`: fetch `fetchAllCycles` / `fetchActiveCycle` / `fetchArchivedCycles` / `fetchCycleDetails` / `fetchActiveCycleProgress(+Pro)` / `fetchActiveCycleAnalytics` / `fetchWorkspaceCycles`; CRUD `createCycle` / `updateCycleDetails` / `deleteCycle`; `archiveCycle` / `restoreCycle`; favorit `add/removeCycleFromFavorites`; computed `currentProjectActiveCycleId` / `ActiveCycle` / `Completed` / `Incomplete` / `Archived`, `getFilteredCycleIds` / `Completed` / `Archived`; filter store `cycle_filter.store.ts`.

### Model (Django)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- `apps/api/plane/db/models/cycle.py:60-80` `class Cycle`: `name`, `description`, `start_date`, `end_date` (nullable), `owned_by` (FK user), `archived_at`, `timezone`, `progress_snapshot` / `view_props` / `logo_props` (JSON), `sort_order`, `version`.
- Relasi: `CycleIssue:104-124`, preferensi per user `CycleUserProperties:130-157`.

### Route terkait

- `:workspaceSlug/active-cycles` (`core.ts:66-68`) — hanya render `WorkspaceActiveCyclesUpgrade` (upsell), bukan list cycle.
- `:workspaceSlug/projects/:projectId/archives/cycles` (`core.ts:239-244`) — daftar archived.
- `:workspaceSlug/settings/projects/:projectId/features/cycles` (`core.ts:307-309`) — toggle fitur cycles per project (+ `header.tsx`).

## Primary View

- Layout: grouped list (Active hero + Upcoming disclosure + Completed) untuk list; issue layouts (reuse Work Items) + analytics sidebar untuk detail.
- Data visible: name, date range, owned_by, progress (points/count), issue distribution, applied filters.
- Interaction: klik cycle → detail; disclosure expand/collapse; sidebar collapse via localStorage.
- Ref: [`_shared/list.md`](./_shared/list.md), [`work-items.md`](./work-items.md) (layout reuse).

## Actions

| Action          | Trigger                 | Permission | State required   |
| --------------- | ----------------------- | ---------- | ---------------- |
| Create          | Toolbar / modal         | Member+    | Fitur cycles on  |
| Update (dates)  | Form + `cycleDateCheck` | Member+    | Own atau admin\* |
| Transfer issues | Modal → pilih new cycle | Member+    | Cycle aktif      |
| Archive         | Quick actions / modal   | Member+    | —                |
| Restore         | Archived list / modal   | Member+    | Archived         |
| Delete          | Quick actions + confirm | Admin      | —                |
| Favorite        | Toggle                  | Member+    | —                |
| Toggle estimate | `EstimateTypeDropdown`  | Member+    | —                |

\* Mengikuti hierarchy workspace/project Plane — lihat Permissions di bawah.

## Filters / Sort / Search

- Filters: applied-filters list (`CycleAppliedFiltersList`); filter store `cycle_filter.store.ts`.
- Sort: grouping date-based (active / upcoming / completed) via `orderCycles` / `shouldFilterCycle` (`@plane/utils`).
- Search: scope per halaman via filter; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Section: issue list (reuse `CycleLayoutRoot`) + sidebar kanan (header, burn-down `ProgressChart`, progress stats, issue progress, details: date range/owned_by).
- Data chart dari `progress_snapshot` (model JSON) + live issue progress; toggle points vs count.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`work-items.md`](./work-items.md).

## Permissions

| Role            | Create | Read | Update | Delete |
| --------------- | ------ | ---- | ------ | ------ |
| Member          | ✅     | ✅   | own    | ❌     |
| Project admin   | ✅     | ✅   | ✅     | ✅     |
| Workspace admin | ✅     | ✅   | ✅     | ✅     |

Catatan: halaman list kosong bila fitur cycles dimatikan di project settings (`cycle_view === false` → empty-state + link features).

## Empty / Loading / Error

- Empty: no cycles → message + CTA create; fitur off → empty-state + link ke project features.
- Loading: skeleton per section.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                       |
| ---------- | ---------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `cycles-view.tsx`, store + `cycle.py` |
