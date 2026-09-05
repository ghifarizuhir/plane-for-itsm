# Pages (Documents)

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/pages` (list) + `:workspaceSlug/projects/:projectId/pages/:pageId` (detail) — lihat `apps/web/app/routes/core.ts:206-211`, `:197-203`
Share: CORE

## Intent

Dokumen/notes kolaboratif per project: tulis spec, meeting notes, atau knowledge base dengan rich-text editor realtime (multi-user), versi, dan sub-pages. Dari sudut user: "tempat nulis dokumen project yang bisa diedit bareng, ada historinya."

## Current State (snapshot kode)

- List page: `projects/(detail)/[projectId]/pages/(list)/page.tsx:37-90` — `ProjectPagesPage`, guard `page_view === false` → empty-state + link ke features; render `PagesListView` + `PagesListRoot` (`storeType = EPageStoreType.PROJECT`, `pageType` dari `?type=public | private | archived`).
- List: `apps/web/core/components/pages/list/{root.tsx,block.tsx,block-item-action.tsx,tab-navigation.tsx:22-30,filters/root.tsx,search-input.tsx,order-by.tsx}` — tab `public | private | archived` + filter favorit + search/sort; `pages-list-view.tsx`, `pages-list-main-content.tsx:58,98-116` (filter `access` by tab).
- Detail page: `pages/(detail)/[pageId]/page.tsx:42-198` — `PageDetailsPage`, `useSWR(PAGE_DETAILS_)` → `fetchPageDetails`, bangun `PageRoot` + `pageRootHandlers` (`create` / `fetchDescriptionBinary` / `fetchAllVersions` / `restoreVersion` / `updateDescription`) + `IssuePeekOverview`.
- Editor: `apps/web/core/components/pages/editor/page-root.tsx:50-80` — `PageRoot` kelola `collaborationState` + `usePageFallback` (binary fallback); `PageEditorBody` + `PageEditorToolbarRoot` + `PageNavigationPaneRoot` + `PageVersionsOverlay`. Komponen: `editor/{editor-body.tsx,title.tsx,header/root.tsx,header/logo-picker.tsx,toolbar/root.tsx,toolbar/toolbar.tsx,toolbar/options-dropdown.tsx}`.
- Header: `header/{root.tsx,actions.tsx,favorite-control.tsx:22-40,lock-control.tsx,copy-link-control.tsx,syncing-badge.tsx,offline-badge.tsx,archived-badge.tsx}` — syncing/offline/archived realtime badges.
- Navigation pane: `navigation-pane/{root.tsx,tabs-list.tsx,tab-panels/{outline.tsx,assets.tsx,info/root.tsx}}` — outline dokumen, assets, info.
- Versions: `version/{root.tsx,editor.tsx,main-content.tsx}` — history + restore (`project-page-version.service.ts:20-42`, model `PageVersion`).
- Aksi: `dropdowns/actions.tsx:45-47,104-141` (`toggle-access`, `copy-link`, `duplicate`, `archive-restore`, delete); modals `modals/{create-page-modal.tsx,page-form.tsx,delete-page-modal.tsx,export-page-modal.tsx}`.
- Recent widget: global home `components/home/widgets/recents/index.tsx:31` (+ `empty-states/recents.tsx:21`) — recent per-visit via `fetchPageDetails(..., {trackVisit})`.
- Working: kolaborasi realtime, 3 tab akses, favorit, lock, sub-pages nesting, duplicate, archive/restore, export (PDF), version history + restore, recent.
- Stub: —
- Missing (ITSM fork): tidak ada — tidak ada template knowledge/incident-postmortem di kode; ide Knowledge diparkir di [`_backlog.md`](./_backlog.md).

### Editor stack (beda dari Work Items)

| Layer    | Implementasi                                                                                                                                                                                                                                                                                                                                               |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Editor   | TipTap 2 (`packages/editor/package.json:39,47-85`: `starter-kit`, `extension-collaboration`, `tiptap-markdown`) + Yjs (`yjs`, `y-prosemirror`, `y-protocols`, `y-indexeddb`, `@hocuspocus/provider`)                                                                                                                                                       |
| Entry    | `packages/editor/src/index.ts:8-13` — `CollaborativeDocumentEditorWithRef`, `DocumentEditorWithRef`, `RichTextEditorWithRef`, `LiteTextEditorWithRef` (`components/editors/{document,rich-text,lite-text}/`)                                                                                                                                               |
| Realtime | `apps/live` — `package.json:5` "realtime collaborative server powers Plane's rich text editor"; `src/hocuspocus.ts:17-51` `HocuspocusServerManager` (`onAuthenticate`, `onStateless`, debounce 10000); `src/server.ts:33-49` Express + `express-ws` + Redis + Hocuspocus (`@hocuspocus/server/extension-redis/extension-database`, `yjs`, `@plane/editor`) |

### Akses: Public/Private, bukan share-link eksternal

Koreksi penting: `access` (0 = Public, 1 = Private) = visibilitas **di dalam project**, bukan link publik eksternal. `Make public/private` via `dropdowns/actions.tsx`. Tidak ada share-link publik anonim di kode.

### Store (MobX)

- Koleksi: `apps/web/core/store/pages/project-page.store.ts:69-102` `class ProjectPageStore` (`data: Record<pageId, TProjectPage>`): `fetchPagesList`, `fetchPageDetails`, `createPage`, `removePage`, `movePage`.
- Instance per-page: `store/pages/base-page.ts` (`TPageInstance`: `updateDescription`, `updatePageLogo`, `is_favorite`, `canCurrentUser*`).
- Favorit: `services/page/project-page.service.ts:82-98` (`favorite-pages/` GET/POST/DELETE) + `favorite.store.ts:59,90`.
- Lock: `service.ts:135-144` (`pages/:id/lock/` POST/DELETE, `is_locked`).
- Service lain: `service.ts:113-178` (`archive` / `unarchive`, `lock` / `unlock`, `duplicate/`).

### Model (Django)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- `apps/api/plane/db/models/page.py:23-55` `class Page(BaseModel)` (db `pages`): `name`, `description_json` / `binary` / `html` / `stripped`, `owned_by`, `access` (0 Public / 1 Private), `color`, `parent` FK self (`child_page`, sub-pages + `sort_order`), `archived_at`, `is_locked`, `view_props` / `logo_props`, `projects` M2M via `ProjectPage`, `moved_to_page` / `project`.
- Pendamping: `PageLog:80-114`, `PageLabel:120-133`, `ProjectPage:135-152`, `PageVersion:158-173` (`description_*` + `sub_pages_data`).

### Route terkait

- `:workspaceSlug/settings/projects/:projectId/features/pages` (`core.ts:318-321`) — toggle fitur (`page_view`).

## Primary View

- Layout: tabbed list (public / private / archived) + favorit/search/sort untuk list; editor full (title + toolbar + body + navigation pane) untuk detail.
- Data visible: title, access, owner, favorite, lock, archived, syncing/offline state, outline, versions.
- Interaction: klik page → editor; kolaborasi realtime (syncing badge); outline jump; version restore.
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action        | Trigger                       | Permission | State required             |
| ------------- | ----------------------------- | ---------- | -------------------------- |
| Create        | Modal + form                  | Member+    | Fitur pages on             |
| Edit          | Editor realtime (Yjs)         | Member+    | Unlocked, own atau admin\* |
| Toggle access | Dropdown (public/private)     | Member+    | Own atau admin\*           |
| Favorite      | Header control                | Member+    | —                          |
| Lock/unlock   | Header control (`lock/`)      | Member+    | Own atau admin\*           |
| Duplicate     | Dropdown                      | Member+    | —                          |
| Archive       | Dropdown / modal              | Member+    | —                          |
| Restore       | Archived tab                  | Member+    | Archived                   |
| Delete        | Confirm modal                 | Admin      | —                          |
| Export        | Export modal (PDF)            | Member+    | —                          |
| Restore vers. | Versions overlay              | Member+    | Own atau admin\*           |
| Move          | `movePage` (nested sub-pages) | Member+    | Own atau admin\*           |

\* Mengikuti hierarchy workspace/project Plane — lihat Permissions di bawah. `canCurrentUser*` di `TPageInstance` (`base-page.ts`) sebagai guard FE.

## Filters / Sort / Search

- Filters: tab `?type=public | private | archived` + filter favorit (`tab-navigation.tsx:22-30`, `filters/root.tsx`, `pages-list-main-content.tsx:58,98-116`).
- Sort: `order-by.tsx` + `sort_order` (nesting).
- Search: `search-input.tsx` per halaman; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Section: title + cover/logo (`logo-picker`), toolbar, body (TipTap collab), navigation pane (outline/assets/info), versions overlay, header badges (syncing/offline/archived/lock/favorite).
- Fallback: `usePageFallback` (binary) bila kolaborasi gagal.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`../ui/editor.md`](../ui/editor.md).

## Permissions

| Role            | Create | Read            | Update | Delete |
| --------------- | ------ | --------------- | ------ | ------ |
| Member          | ✅     | ✅ (own+public) | own    | ❌     |
| Project admin   | ✅     | ✅              | ✅     | ✅     |
| Workspace admin | ✅     | ✅              | ✅     | ✅     |

Catatan: halaman list kosong bila fitur pages dimatikan di project settings (`page_view === false`).

## Empty / Loading / Error

- Empty: no pages → message + CTA create; fitur off → empty-state + link ke project features.
- Loading: skeleton editor + `syncing-badge`; offline → `offline-badge`.
- Error: banner + retry; fallback binary bila sync gagal.

---

## Changelog

| Date       | Change                                                                                 |
| ---------- | -------------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `page-root.tsx`, `apps/live`, store + `page.py` |
