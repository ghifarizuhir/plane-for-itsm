# Work Items (Issues)

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/issues` (list) + `:workspaceSlug/projects/:projectId/issues/:issueId` (detail redirector) — lihat `apps/web/app/routes/core.ts:137-147`
Share: CORE

## Intent

Halaman utama kerja per project: lihat, filter, dan kelola work items (Issue) dalam berbagai layout. Dari sudut user: "semua pekerjaan project ini, tampilkan sesuai caraku kerja."

## Current State (snapshot kode)

- Komponen list: `apps/web/core/components/issues/issue-layouts/roots/project-layout-root.tsx:28-43` — `ProjectIssueLayout({activeLayout})` switch `LIST → <ListLayout/>`, `KANBAN → <KanBanLayout/>`, `CALENDAR → <CalendarLayout/>`, `GANTT → <BaseGanttRoot/>`, `SPREADSHEET → <ProjectSpreadsheetLayout/>`. Layout aktif dari `issuesFilter.getIssueFilters(projectId).displayFilters.layout` (`:51-54`).
- Toolbar/filter: `apps/web/core/components/issues/filters.tsx:37-45` — `LAYOUTS = [LIST, KANBAN, CALENDAR, SPREADSHEET, GANTT]` + `HeaderFilters` (`FiltersDropdown`, `DisplayFiltersSelection`, `LayoutSelection`, `WorkItemFiltersToggle`, `WorkItemFiltersRow`, `ProjectLevelWorkItemFiltersHOC`). Facet filter di `issue-layouts/filters/header/filters/{priority,state,assignee,labels,cycle,module,due-date,start-date,created-by}.tsx`.
- Detail = peek overlay, bukan halaman penuh: `apps/web/core/components/issues/peek-overview/root.tsx:27` `IssuePeekOverview` (mode `side-peek | modal | full-screen`, `view.tsx:57`, `header.tsx:38-76`), dibuka via `hooks/use-issue-peek-overview-redirection.tsx:26-50` (`setPeekIssue(...)`, mobile: `router.push(workItemLink)`), dirender global di `issue-detail/root.tsx:61,267-268`.
- Route detail `:issueId` hanya redirector: `issues/(detail)/[issueId]/page.tsx:25-44` — `clientLoader` resolve `issueService.getIssueMetaFromURL` lalu `redirect(/{slug}/browse/{identifier}-{seq})`; gagal → `EmptyState` + tombol ke `workspace-views/all-issues/` (`:58-61`).
- Bulk: `apps/web/core/components/issues/bulk-operations/root.tsx:9` + `hooks/use-bulk-operation-status`; aksi store `project/issue.store.ts:50` (`removeBulkIssues`, `TBulkOperationsPayload`).
- Working: 5 layout + spreadsheet columns (state/priority/assignee/label/cycle/module/estimate/dates/link/attachment/sub-issue), filter multi-facet, display-properties/group/order, quick-add, update inline, peek detail, bulk edit/delete/archive.
- Stub: —
- Missing (ITSM fork): tidak ada — overlay ITSM (incident/priority SLA, war room) belum ada di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Store (MobX)

- List: `apps/web/core/store/issue/project/issue.store.ts:25-50` `IProjectIssues extends BaseIssuesStore` (`fetchIssues` / `fetchNextIssues` / `createIssue` / `updateIssue` / `archiveIssue` / `quickAddIssue` / `removeBulkIssues`); filter: `project/filter.store.ts` `IProjectIssuesFilter` (`fetchFilters` / `updateFilters` / `getIssueFilters`); base: `store/issue/helpers/base-issues.store.ts` + `issue-filter-helper.store.ts`; agregator `store/issue/root.store.ts`; akses via `hooks/store/use-issues` (`useIssues(EIssuesStoreType.PROJECT)`).
- Detail: `apps/web/core/store/issue/issue-details/root.store.ts:43-49` (`TPeekIssue`, agregasi `comment` / `activity` / `attachment` / `link` / `relation` / `sub_issues` / `subscription` / `reaction`); akses via `hooks/store/use-issue-detail` (`useIssueDetail(EIssueServiceType.ISSUES)`).

### Model (Django)

- `apps/api/plane/db/models/issue.py:104-170` `class Issue(ProjectBaseModel)`: `name:136`, `priority:141-146` (`urgent/high/medium/low/none`, default `none`), `state` FK `State:121-127`, `assignees` M2M via `IssueAssignee:149-155`, `labels` M2M via `IssueLabel:157`, `parent` FK self (`parent_issue:114-120`, sub-issue), `point:128` + `estimate_point` FK:129-135, `start_date/target_date:147-148`, `sequence_id:156` (auto-increment per-project via `pg_advisory_xact_lock` di `save():180-214`), `sort_order:158`, `type` FK `IssueType:164-170`, `is_draft:161`, `archived_at:160`.
- Server defaults: `get_default_properties():29-44` (assignee/dates/labels/key/priority/state/sub_issue_count/link/attachment/estimate/created_on/updated_on); `get_default_filters():47-58` (priority/state/state_group/assignees/created_by/labels/start_date/target_date/subscriber); `get_default_display_filters():61-70` (`group_by null`, `order_by -created_at`, `layout list`, `sub_issue true`, `show_empty_groups true`).

## Primary View

- Layout: list / kanban / calendar / gantt / spreadsheet (user-switchable, persist server-side).
- Data visible: kolom spreadsheet = state, priority, assignee, label, cycle, module, estimate, dates, link, attachment, sub-issue (`spreadsheet/columns/*.tsx`, `base-spreadsheet-root.tsx`).
- Interaction: klik row → peek overlay; update inline per kolom; drag di kanban; switch layout via `LayoutSelection`.
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action        | Trigger                | Permission | State required   |
| ------------- | ---------------------- | ---------- | ---------------- |
| Create        | Toolbar / quick-add    | Member+    | —                |
| Update inline | Cell / peek overview   | Member+    | Own atau admin\* |
| Switch layout | LayoutSelection        | Member+    | —                |
| Bulk edit     | Select + bulk bar      | Member+    | Own atau admin\* |
| Bulk delete   | Select + bulk bar      | Admin      | —                |
| Archive       | Row action / bulk      | Member+    | Own atau admin\* |
| Open detail   | Klik row (peek) / link | Member+    | Read project     |

\* Mengikuti hierarchy workspace/project Plane — lihat Permissions di bawah.

## Filters / Sort / Search

- Filters: facet `priority`, `state` (+ `state-group`), `assignee`, `labels`, `cycle`, `module`, `due-date`, `start-date`, `created-by` (`issue-layouts/filters/header/filters/`).
- Display: `display-filters/{group-by,order-by,sub-group-by,extra-options}.tsx`.
- Persist: **server-side via `IssueFilter` API** (`fetchFilters` / `updateFilters`) — bukan query params URL. Tidak ada `?state=` / `?priority=` / `?layout=` di halaman ini; `hooks/use-query-params.ts:14-21` hanya dipakai pane/pages & `next_path` OAuth.
- Search: scope per halaman via filter; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Peek overlay: metadata / description / comments / timeline-activity / attachments / links / relations / sub-issues (`issue-detail-widgets/{attachments,links,relations,sub-issues}/root.tsx`, `issue-detail/{issue-activity,links/*,reactions/issue-comment.tsx,parent/*,main-content.tsx:31,184}`).
- Link kanonik: `generateWorkItemLink` (`@plane/utils`) → `/{slug}/browse/{identifier}-{seq}` (route `:workspaceSlug/browse/:workItem`, `core.ts:76-78`).
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`_shared/comments.md`](./_shared/comments.md) (saat ditulis).

## Permissions

| Role            | Create | Read | Update | Delete |
| --------------- | ------ | ---- | ------ | ------ |
| Member          | ✅     | ✅   | own    | ❌     |
| Project admin   | ✅     | ✅   | ✅     | ✅     |
| Workspace admin | ✅     | ✅   | ✅     | ✅     |

## Empty / Loading / Error

- Empty: message + CTA (buat work item pertama).
- Loading: skeleton per layout (`issue-layout-HOC.tsx:12-16` — `Calendar/Gantt/Kanban/SpreadsheetLayoutLoader`, `@plane/ui`).
- Error: banner + retry; detail redirect gagal → `EmptyState` + tombol ke `workspace-views/all-issues/`.

---

## Changelog

| Date       | Change                                                                               |
| ---------- | ------------------------------------------------------------------------------------ |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `project-layout-root.tsx`, store + `issue.py` |
