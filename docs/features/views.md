# Views (Saved Filters)

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/views` (list) + `:workspaceSlug/projects/:projectId/views/:viewId` (detail) — lihat `apps/web/app/routes/core.ts:189-195`, `:181-187`
Share: CORE

## Intent

Filter tersimpan yang bisa dipakai ulang: user meracik filter + tampilan sekali, simpan sebagai View (private atau shared), lalu buka kapan saja tanpa meracik ulang. Dari sudut user: "tampilan kerja yang sudah kusimpan — mis. bug urgent milikku, grouping by state."

## Current State (snapshot kode)

- Konsep: View = saved filter. `packages/types/src/views.ts:20-40` `IProjectView {query/query_data, display_filters, display_properties, rich_filters, access, is_locked, owned_by}`; `EViewAccess {PRIVATE = 0, PUBLIC = 1}` (`:15-18`).
- Ad-hoc vs saved: filter ad-hoc di Work Items hanya sementara; View mem-persist `filters + display_filters + display_properties` via `views/form.tsx:29-36,47-50` (`FiltersDropdown`, `DisplayFiltersSelection`, `LayoutDropDown`, default `PUBLIC`).
- List page: `projects/(detail)/[projectId]/views/(list)/page.tsx:32,101` — `ProjectViewsPage` + `<ProjectViewsList/>`; gate `issue_views_view === false` (`:70`).
- List: `apps/web/core/components/views/views-list.tsx:24` `ProjectViewsList` → `view-list-item.tsx:26` `ProjectViewListItem` (link `.../views/${view.id}`, `:47`).
- Aksi item: `view-list-item-action.tsx:34,60-62` — ikon `Globe`/`Lock` by `access`, favorite, edit, delete, publish link `getPublishViewLink(view.anchor)`; `quick-actions.tsx` versi mobile.
- Detail page: `views/(detail)/[viewId]/page.tsx:21,52` — `ProjectViewIssuesPage` fetch `fetchViewDetails` (`:33`) + reuse `<ProjectViewLayoutRoot/>` (issue layout — pola sama seperti cycles/modules).
- Create/edit: `views/modal.tsx:32` (`CreateUpdateProjectViewModal`) → `views/form.tsx:38` (`ProjectViewForm`); delete: `views/delete-view-modal.tsx:25` (`DeleteProjectViewModal` → `deleteView`).
- Publish/share: **stub** — `views/publish/use-view-publish.tsx:8-13` (no-op); field `anchor` sudah ada (`views.ts:39`, `IPublishedProjectView:43-45`).
- Working: CRUD views, private/public access + ikon, lock, favorites, detail reuse issue layouts, publish link (URL tersedia, backend stub).
- Stub: publish backend (`use-view-publish` no-op).
- Missing (ITSM fork): tidak ada — tidak ada saved-view untuk incident queue di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Project views vs Workspace views

| Aspek  | Project views (halaman ini)                    | Workspace views (`:workspaceSlug/workspace-views`, `core.ts:109-115`)                                                                                                              |
| ------ | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope  | Punya `project` FK                             | `project__isnull = True` (`view.py:84-90`)                                                                                                                                         |
| Tipe   | `IProjectView` (`packages/types/src/views.ts`) | `IWorkspaceView` (`workspace-views.ts:15-39`) + `STATIC_VIEW_TYPES = ["all-issues", "assigned", "created", "subscribed"]` (`:41`)                                                  |
| List   | `ProjectViewsList`                             | `workspace-views/page.tsx:22,17-18` — gabungan `DEFAULT_GLOBAL_VIEWS_LIST` (`packages/constants/src/workspace.ts:171-205`, href `workspace-views/all-issues/`) + `GlobalViewsList` |
| Detail | Full issue layouts                             | Global detail hanya `SPREADSHEET` (`views/helper.tsx:38-52` `WorkspaceActiveLayout`)                                                                                               |

Catatan: `browse/:workItem` (`core.ts:76-78`) **bukan view** — itu detail WorkItem global by identifier (`browse/[workItem]/page.tsx:35`), dipakai sebagai link kanonik dari peek/detail. Lihat [`work-items.md`](./work-items.md).

### Store (MobX)

- Project: `apps/web/core/store/project-view.store.ts:52` `ProjectViewStore` — `fetchViews:165`, `fetchViewDetails:191`, `createView:206`, `updateView:224`, `deleteView:248`, `addViewToFavorites:264`.
- Global: `apps/web/core/store/global-view.store.ts:39` `GlobalViewStore` (`fetchAllGlobalViews` / `fetchGlobalViewDetails` / `createGlobalView` / `updateGlobalView` / `deleteGlobalView`, `:26-36`) via `WorkspaceService`.

### Model (Django)

- `apps/api/plane/db/models/view.py:58` `IssueView(WorkspaceBaseModel)`: `name` / `description` / `query` / `filters` / `display_filters` / `display_properties` / `rich_filters` / `access` (0 = Private, 1 = Public) / `owned_by` / `is_locked` / `archived_at` (`:59-71`).
- Defaults: `get_default_filters` / `display_filters` / `display_properties` (`:14-55`); `save()` derivasi `query = issue_filters(filters)` + `sort_order` (`:79-93`).

### Route terkait

- `:workspaceSlug/settings/projects/:projectId/features/views` (`core.ts:314-317`) — toggle fitur (`settings/projects/[projectId]/features/views/page.tsx:24`).

## Primary View

- Layout: list of saved views (private/shared) untuk list; issue layouts (reuse Work Items) dengan filter tersimpan untuk detail.
- Data visible: name, access (Globe/Lock), owner, locked, favorite.
- Interaction: klik view → detail (filter langsung aktif); aksi per item (edit/delete/favorite/publish-link).
- Ref: [`_shared/list.md`](./_shared/list.md), [`work-items.md`](./work-items.md) (layout reuse).

## Actions

| Action        | Trigger                          | Permission | State required   |
| ------------- | -------------------------------- | ---------- | ---------------- |
| Create        | Toolbar / modal (default PUBLIC) | Member+    | Fitur views on   |
| Update        | Form modal                       | Member+    | Own atau admin\* |
| Delete        | Confirm modal                    | Member+    | Own atau admin\* |
| Favorite      | Toggle                           | Member+    | —                |
| Publish link  | Item action (URL ready, BE stub) | Member+    | PUBLIC           |
| Lock / unlock | `is_locked`                      | Admin      | —                |

\* Mengikuti hierarchy workspace/project Plane — lihat Permissions di bawah.

## Filters / Sort / Search

- Filters: View ITU filter tersimpan (`filters` + `rich_filters` di model); form meracik via `FiltersDropdown` + `DisplayFiltersSelection` + `LayoutDropDown`.
- Sort: `sort_order` (derivasi di `save()`).
- Search: scope per halaman via filter; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Section: issue list dengan filter/display tersimpan (reuse `ProjectViewLayoutRoot`) — sama seperti Work Items, tapi konfigurasi datang dari View.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`work-items.md`](./work-items.md).

## Permissions

| Role            | Create | Read            | Update | Delete |
| --------------- | ------ | --------------- | ------ | ------ |
| Member          | ✅     | ✅ (own+public) | own    | ❌     |
| Project admin   | ✅     | ✅              | ✅     | ✅     |
| Workspace admin | ✅     | ✅              | ✅     | ✅     |

Catatan: halaman list kosong bila fitur views dimatikan di project settings (`issue_views_view === false`).

## Empty / Loading / Error

- Empty: no views → message + CTA create; fitur off → gate di list page.
- Loading: skeleton per section.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                     |
| ---------- | -------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `views-list.tsx`, store + `view.py` |
