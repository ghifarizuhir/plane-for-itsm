# Stickies (Quick Notes)

Status: **Approved**
Route: `:workspaceSlug/stickies` — lihat `apps/web/app/routes/core.ts:103-106` (`stickies/page.tsx:11-16` → `StickiesInfinite`)
Share: CORE

## Intent

Catatan cepat pribadi per workspace (sticky notes): tempel ide, simpan warna, cari lagi nanti. Dari sudut user: "post-it digital yang hanya kumiliki."

## Current State (snapshot kode)

- List: `apps/web/core/components/stickies/layout/stickies-infinite.tsx:18-22` — infinite list via `useSticky()`.
- Item: `stickies/sticky/root.tsx:38-40` + `sticky/use-operations.tsx:40-44` (`stickies` + `stickyOperations`: CRUD + warna).
- Personal: API filter `workspace__slug + owner = request.user` (`apps/api/plane/api/views/sticky.py:30-36,48,66,85,96,110` — create/list/retrieve/partial_update/destroy).
- Search server-side: `description_stripped__icontains`, paginate 20/page (`sticky.py:69-76` di views).
- Working: CRUD, warna (`color`/`background_color`), infinite scroll, search.
- Stub: —
- Missing (ITSM fork): tidak ada.

### Store (MobX)

- `apps/web/core/store/sticky/sticky.store.ts:44,84,146,178` `StickyStore` (`getWorkspaceStickyIds`, `fetchWorkspaceStickies`, `createSticky`, ...); hook `core/hooks/use-stickies.tsx:13` (`useSticky(): IStickyStore`); service `core/services/sticky.service.ts:19,32,46,54,62` (`POST/GET/PATCH/DELETE /stickies/`).

### Model + API (Django)

- Model: `apps/api/plane/db/models/sticky.py:16-30` `Sticky` (`name`, `description` JSON/HTML, `color`/`background_color`, `workspace` + `owner` FK, `sort_order`).
- API: `apps/api/plane/app/urls/workspace.py:245,250` → `stickies/`, `stickies/<pk>/`.

## Primary View

- Layout: infinite grid/list of stickies.
- Data visible: name, description, color, updated_at.
- Interaction: klik → edit; warna via picker; hapus langsung.
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action  | Trigger      | Permission | State required |
| ------- | ------------ | ---------- | -------------- |
| Create  | Quick-add    | Own        | —              |
| Edit    | Inline       | Own        | —              |
| Recolor | Color picker | Own        | —              |
| Delete  | Aksi item    | Own        | —              |

Sepenuhnya personal — tidak ada sharing antar user.

## Filters / Sort / Search

- Filters: —
- Sort: `sort_order` + updated.
- Search: server-side `description_stripped__icontains` (20/page).

## Detail View

- Tidak ada detail page — edit inline di sticky.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md).

## Permissions

| Role            | Own stickies | Others' stickies |
| --------------- | ------------ | ---------------- |
| Member          | ✅           | ❌               |
| Project admin   | ✅           | ❌               |
| Workspace admin | ✅           | ❌               |

## Empty / Loading / Error

- Empty: no stickies → message + CTA.
- Loading: infinite loader.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                  |
| ---------- | ----------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `stickies/`, store + `sticky.py` |
