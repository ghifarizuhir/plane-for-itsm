# Modules

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/modules` (list) + `:workspaceSlug/projects/:projectId/modules/:moduleId` (detail) — lihat `apps/web/app/routes/core.ts:173-179`, `:165-171`
Share: CORE

## Intent

Pengelompokan kerja lintas-sprint: module adalah container work items untuk sebuah scope/fitur (bisa jalan lebih lama dari satu cycle), dengan lead, members, status manual, dan progress. Dari sudut user: "semua pekerjaan untuk fitur X, siapa yang pegang, sudah sejauh mana?"

## Current State (snapshot kode)

- List page: `projects/(detail)/[projectId]/modules/(list)/page.tsx:30,99` — `ProjectModulesPage`, render `ModulesListView`; gate `module_view === false` → arahkan ke settings features.
- List layouts: `apps/web/core/components/modules/modules-list-view.tsx:82-116` — 3 layout: `list` → `ModuleListItem`, `board` → `ModuleCardItem` (grid), `gantt` → `ModulesListGanttChartView`; + `ModulePeekOverview`.
- Item/aksi: `modules/module-list-item.tsx`, `module-card-item.tsx`, `module-list-item-action.tsx`, `quick-actions.tsx:33-44` (edit, archive, delete, copy-link, `ArchiveModuleModal`).
- Detail page: `modules/(detail)/[moduleId]/page.tsx:25,66,74` — `ModuleIssuesPage`, reuse `ModuleLayoutRoot` (issue layout) + sidebar kanan `ModuleAnalyticsSidebar` 24rem, collapsible via `module_sidebar_collapsed`.
- Detail sidebar: `modules/analytics-sidebar/root.tsx:55,200-373` — edit inline `status`, `start/target_date` (`DateRangeDropdown`), `lead_id` / `member_ids` (`MemberDropdown`), count issues, `ModuleAnalyticsProgress`, `ModuleLinksList`.
- Progress: `analytics-sidebar/issue-progress.tsx:34-37,181-206` — `moduleBurnDownChartOptions = [burndown/issues, points]`, `ProgressChart` (completion_chart, syarat start + end date); `progress-stats.tsx:40` `ModuleProgressStats` (tab assignee/label/state_group); `plotType` default `burndown` (`module.store.ts:250-256`).
- Create/edit: `modules/modal.tsx:30-56` (`CreateUpdateModuleModal`, default `status: backlog`) + `modules/form.tsx`.
- Delete: `modules/delete-module-modal.tsx:28,44-51` (`DeleteModuleModal`, `deleteModule()`, redirect ke `/modules` bila dari detail/peek).
- Archived: `archived-modules/modal.tsx` + `view.tsx` / `header.tsx` / `root.tsx` untuk list archived.
- Working: 3 layout list, CRUD + status manual, lead/members, links (`create/update/deleteModuleLink`), burn-down/progress, archive/restore, favorites, peek overview.
- Stub: —
- Missing (ITSM fork): tidak ada — tidak ada konsep service/component CI di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Modules vs Cycles (perbedaan kunci)

| Aspek         | Modules                                                                                                                                                                                                          | Cycles (`cycles.md`)                                             |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Status        | Manual 6 nilai (`packages/constants/src/module.ts:21-70`): `backlog / planned / in-progress / paused / completed / cancelled`                                                                                    | Dihitung dari tanggal (`current / upcoming / completed / draft`) |
| Date range    | `start_date` + `target_date` (bisa null)                                                                                                                                                                         | `start_date` + `end_date`                                        |
| Ownership     | `lead` + `members` (M2M)                                                                                                                                                                                         | `owned_by` saja                                                  |
| Pindah issues | Tidak ada transfer — tambah via `addIssuesToModule` / `addModulesToIssue` (`issue-layouts/empty-states/module.tsx:51`, `issue-modal/base.tsx:134,212-214`), drag/drop kanban (`kanban/roots/module-root.tsx:29`) | Transfer unfinished via modal + `POST transfer-issues/`          |
| List layouts  | list / board (cards) / gantt                                                                                                                                                                                     | active hero + upcoming/completed disclosure                      |

### Store (MobX)

- `apps/web/core/store/module.store.ts:82,604,627` `class ModulesStore`: `fetchModules` / `fetchModuleDetails` / `fetchArchivedModules`; `createModule` / `updateModuleDetails` / `deleteModule`; `archiveModule` / `restoreModule`; `add/removeModuleToFavorites`; `create/update/deleteModuleLink`; `setPlotType`.

### Model (Django)

- `apps/api/plane/db/models/module.py:58-99`: `ModuleStatus` 6 nilai; field `name`, `description` (+ `_text` / `_html`), `start_date`, `target_date`, `status` (default `planned`), `lead` (FK), `members` (M2M via `ModuleMember`), `archived_at`, `sort_order`, `view_props` / `logo_props`.
- Relasi: `ModuleIssue`, `ModuleLink`, `ModuleUserProperties`.

### Route terkait

- `:workspaceSlug/projects/:projectId/archives/modules` (`core.ts:247-251`) — daftar archived (`ArchivedModulesHeader` + `ArchivedModuleLayoutRoot`, `archives/modules/page.tsx:15,28-29`).
- `:workspaceSlug/settings/projects/:projectId/features/modules` (`core.ts:310-313`) — toggle fitur (admin-only).

## Primary View

- Layout: list / board (cards grid) / gantt untuk list; issue layouts (reuse Work Items) + analytics sidebar untuk detail.
- Data visible: name, status, date range, lead, members, issue count, progress (burndown/issues × count/points).
- Interaction: klik module → detail; quick actions per item; edit inline di sidebar (status, dates, lead, members).
- Ref: [`_shared/list.md`](./_shared/list.md), [`work-items.md`](./work-items.md) (layout reuse).

## Actions

| Action        | Trigger                   | Permission | State required   |
| ------------- | ------------------------- | ---------- | ---------------- |
| Create        | Toolbar / modal           | Member+    | Fitur modules on |
| Update        | Form / sidebar inline     | Member+    | Own atau admin\* |
| Change status | Sidebar inline            | Member+    | Own atau admin\* |
| Add issues    | Empty-state / issue modal | Member+    | —                |
| Manage links  | Sidebar links list        | Member+    | —                |
| Archive       | Quick actions / modal     | Member+    | —                |
| Restore       | Archived list / modal     | Member+    | Archived         |
| Delete        | Quick actions + confirm   | Admin      | —                |
| Favorite      | Toggle                    | Member+    | —                |
| Switch plot   | `setPlotType` (burndown)  | Member+    | start+end date   |

\* Mengikuti hierarchy workspace/project Plane — lihat Permissions di bawah.

## Filters / Sort / Search

- Filters: facet standar issue layouts di detail; grouping per status di list.
- Sort: manual (`sort_order`) + date-based di gantt.
- Search: scope per halaman via filter; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Section: issue list (reuse `ModuleLayoutRoot`) + sidebar kanan (status, date range, lead/members, counts, progress chart + stats tabs, links).
- Chart butuh start + end date; toggle burndown/issues × count/points.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`work-items.md`](./work-items.md).

## Permissions

| Role            | Create | Read | Update | Delete |
| --------------- | ------ | ---- | ------ | ------ |
| Member          | ✅     | ✅   | own    | ❌     |
| Project admin   | ✅     | ✅   | ✅     | ✅     |
| Workspace admin | ✅     | ✅   | ✅     | ✅     |

Catatan: halaman list kosong/diarahkan bila fitur modules dimatikan di project settings (`module_view === false`).

## Empty / Loading / Error

- Empty: no modules → message + CTA create; fitur off → redirect ke project features.
- Loading: skeleton per section.
- Error: banner + retry; delete dari detail/peek → redirect ke `/modules`.

---

## Changelog

| Date       | Change                                                                              |
| ---------- | ----------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `modules-list-view.tsx`, store + `module.py` |
