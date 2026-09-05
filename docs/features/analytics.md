# Analytics

Status: **Approved**
Route: `:workspaceSlug/analytics/:tabId` — lihat `apps/web/app/routes/core.ts:71-73` (+ redirect `:workspaceSlug/analytics` → `/analytics/overview/`, `core.ts:378-379` via `routes/redirects/core/analytics.tsx:10-13`)
Share: CORE

## Intent

Insight lintas project dalam satu workspace: total work items, created-vs-resolved, dan breakdown custom (by priority/dll) untuk jawab "bagaimana kesehatan delivery workspace ini?" Dari sudut user: "lihat angka + grafik, filter per project, export kalau perlu."

## Current State (snapshot kode)

- Layout: `analytics/[tabId]/layout.tsx:13-22` (AppHeader + Outlet); tab nav `analytics/[tabId]/page.tsx:55-114` (`Tabs.List/Trigger/Content`, `handleTabChange` push `/{slug}/analytics/{value}`); header breadcrumb `[tabId]/header.tsx:15-33`; daftar tab via `use-analytics-tabs.tsx:11-17` → `getAnalyticsTabs(t)`.
- **Hanya 2 tab valid**: `packages/types/src/analytics.ts:40` `TAnalyticsTabsBase = "overview" | "work-items"`; definisi `analytics/tabs.tsx:11-14` (`Overview`, `WorkItems`). Tidak ada tab `scope`/`custom`.
- Overview: `analytics/overview/root.tsx:13-25` → `TotalInsights(overview)` + `ProjectInsights` (`overview/project-insights.tsx`) + `ActiveProjects` (`overview/active-projects.tsx`).
- Work-items: `analytics/work-items/root.tsx:14-25` → `TotalInsights(work-items)` + `CreatedVsResolved` (AreaChart, `created-vs-resolved.tsx:13,26`) + `CustomizedInsights` + `WorkItemsInsightTable`.
- "Custom" = seksi dalam tab work-items, bukan tab: `work-items/customized-insights.tsx:29-57` form `x_axis / y_axis / group_by` (default `PRIORITY` / `WORK_ITEM_COUNT`) render `PriorityChart` (`work-items/priority-chart.tsx:19,52` pakai `BarChart`); kontrol `select/analytics-params.tsx:30`, `select/select-x-axis.tsx:21`, `select/select-y-axis.tsx:24`, `select/duration.tsx:57`, `select/project.tsx:23`.
- Filter workspace-level (multi-project): `analytics-filter-actions.tsx:15-35` hanya `ProjectSelect`; `DurationDropdown` di-comment-out; store default `last_30_days` (`core/store/analytics.store.ts:38`). Tidak ada year selector / burn-up-down / pie di halaman ini.
- Export CSV (frontend): `analytics/export.ts:18-32` (`export-to-csv`, filename `{slug}-analytics`) dipakai `workitems-insight-table.tsx:26,205` + `priority-chart.tsx:28,232` + `insight-table/root.tsx:24-45`.
- Working: 2 tab, total insights, created-vs-resolved area chart, customized bar chart + insight table, project multi-filter, CSV export.
- Stub: `DurationDropdown` (di-comment-out — durasi terkunci default).
- Missing (ITSM fork): tidak ada — tidak ada dashboard SLA/incident di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Halaman ini tidak pakai ProgressChart

Koreksi penting: `ProgressChart` (`components/core/sidebar/progress-chart.tsx:20`) hanya dipakai sidebar Cycles (`cycles/analytics-sidebar/sidebar-chart.tsx:15,75`, `cycles/active-cycle/productivity.tsx:19,77`) & Modules (`modules/analytics-sidebar/issue-progress.tsx:21,186`). Halaman Analytics pakai chart terpisah: Area/Bar + `InsightCard` / `TotalInsights` / `DataTable`.

### Store vs Service (pisah tanggung jawab)

- Store = filter UI saja, tanpa fetch: `apps/web/core/store/analytics.store.ts:34-63` (`BaseAnalyticsStore`: `currentTab` / `selectedProjects` / `selectedDuration` / `selectedCycle` / `selectedModule` / `isPeekView` / `isEpic` + `update*`); hook `core/hooks/store/use-analytics.ts`.
- Fetch = service: `apps/web/core/services/analytics.service.ts:23-111` → `getAdvanceAnalytics` / `getAdvanceAnalyticsStats` / `getAdvanceAnalyticsCharts` ke `/api/workspaces/{slug}[/projects/{id}]/{advance-analytics | stats | charts}` (`processUrl:91-111`, peek-view khusus `work-items` / `custom-work-items`).

### Model + API (Django)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- Model: `apps/api/plane/db/models/analytic.py:11-26` `AnalyticView` (workspace FK `related_name="analytics"`, `name`, `description`, `query` JSON, `query_dict`); app `plane/analytics/apps.py:9`, terdaftar `settings/common.py:103`. Catatan: model ini saved analytic view (CRUD API ada) — bukan definisi tab.
- API: `apps/api/plane/app/urls/analytic.py:24-89` → `GET analytics/`, `analytic-view/` + `analytic-view/<pk>/` (CRUD `AnalyticViewViewset`), `saved-analytic-view/<id>/`, `export-analytics/`, `default-analytics/`, `project-stats/`, `advance-analytics[/-stats/-charts]` (workspace-level) + `projects/<id>/advance-analytics[/-stats/-charts]` (peek-view); plus `urls/cycle.py:102` (`.../cycles/<id>/analytics/` untuk sidebar cycle).

## Primary View

- Layout: tabbed (Overview | Work Items) + filter bar (project select) untuk workspace-level insight.
- Data visible: total insights (cards), created-vs-resolved (area), customized breakdown (bar + table).
- Interaction: ganti tab (URL push), pilih projects, atur x/y/group-by di customized, export CSV per widget.
- Ref: [`_shared/list.md`](./_shared/list.md) (untuk insight table).

## Actions

| Action       | Trigger                  | Permission | State required |
| ------------ | ------------------------ | ---------- | -------------- |
| Switch tab   | Tabs nav (URL push)      | Member+    | —              |
| Filter proje | `ProjectSelect`          | Member+    | —              |
| Customize    | x/y/group-by form        | Member+    | Tab work-items |
| Export CSV   | Per widget (table/chart) | Member+    | Data loaded    |

Halaman ini read-only — tidak ada create/update/delete konten (CRUD hanya untuk saved `AnalyticView` via API, tanpa UI khusus di halaman ini).

## Filters / Sort / Search

- Filters: `selectedProjects` (multi-project), `selectedDuration` (default `last_30_days`; dropdown di-comment-out), `selectedCycle` / `selectedModule` (peek-view).
- Sort: default per widget (insight table).
- Search: tidak ada search per halaman; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Tidak ada detail page sendiri (`:tabId` hanya 2 nilai valid). Drill-down via peek-view (`isPeekView`, `projects/<id>/advance-analytics`) dan link ke halaman sumber (project/cycle/module).
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md).

## Permissions

| Role            | View | Customize | Export |
| --------------- | ---- | --------- | ------ |
| Member          | ✅   | ✅        | ✅     |
| Project admin   | ✅   | ✅        | ✅     |
| Workspace admin | ✅   | ✅        | ✅     |

Halaman workspace-level — visible untuk semua member workspace (data dibatasi project yang bisa diakses user).

## Empty / Loading / Error

- Empty: no data → message + CTA (buat work items dulu).
- Loading: skeleton per widget.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                     |
| ---------- | -------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `tabs.tsx`, service + `analytic.py` |
