# Browse (Work Item by Identifier)

Status: **Approved**
Route: `:workspaceSlug/browse/:workItem` — lihat `apps/web/app/routes/core.ts:75-78` (`browse/[workItem]/page.tsx`)
Share: CORE

## Intent

URL kanonik stabil untuk satu work item (`{IDENTIFIER}-{seq}`, mis. `ENG-123`): dipakai sebagai target share link, redirect dari route detail `issues/:issueId`, dan hasil Cmd+K. Dari sudut user: "link permanen ke satu work item, buka dari mana saja."

## Current State (snapshot kode)

- Resolve: `browse/[workItem]/page.tsx:50,53-63` — `workItem.split("-")`, SWR `fetchIssueWithIdentifier(workspaceSlug, projectIdentifier, seq)` → `getProjectByIdentifier` → `getIssueById` (resolve `projectId`/`issueId`).
- Render penuh (bukan peek): `core/components/browse/workItem-detail.tsx:18-28` — `WorkItemDetailRoot` reuse penuh `IssueDetailRoot`; page `:130-138` bungkus `ProjectAuthWrapper + WorkItemDetailRoot`.
- Intake branch: bila `is_intake` → redirect ke `projects/{id}/intake/?inboxIssueId=` (`:88-92`).
- States: error → `EmptyState`, loading → `Loader` (`:94-124`).
- Working: resolve identifier → project + issue, render detail penuh, intake redirect, empty/error states.
- Stub: —
- Missing (ITSM fork): tidak ada.

### Store + Service + API

- Store: `core/store/issue/issue-details/issue.store.ts:270-274` `fetchIssueWithIdentifier → retrieveWithIdentifier`, cache identifier→id (`:295`).
- Service: `core/services/issue/issue.service.ts:439-445` `GET /work-items/{identifier}-{seq}/`.
- API: `apps/api/plane/app/urls/issue.py:282` → `work-items/<project_identifier>-<issue_identifier>/`.

## Primary View

- Layout: detail penuh satu work item (sama seperti peek-overview versi full-screen).
- Data visible: sama dengan detail Work Items (metadata, description, comments, activity, attachments, links, relations, sub-issues).
- Interaction: sama dengan detail Work Items.
- Ref: [`work-items.md`](./work-items.md), [`_shared/detail-page.md`](./_shared/detail-page.md).

## Actions

| Action            | Trigger             | Permission        | State required |
| ----------------- | ------------------- | ----------------- | -------------- |
| View              | Buka link langsung  | Member+           | Read project   |
| Semua aksi detail | Sama spt Work Items | (ikut Work Items) | —              |

## Filters / Sort / Search

- Tidak ada — halaman resolve satu entity (masuk via link atau Cmd+K global).

## Detail View

- Halaman ini ADALAH detail view (`WorkItemDetailRoot` penuh, bukan peek).
- Ref: [`work-items.md`](./work-items.md) §Detail View.

## Permissions

| Role            | View                           |
| --------------- | ------------------------------ |
| Member          | ✅ (project yang bisa diakses) |
| Project admin   | ✅                             |
| Workspace admin | ✅                             |

`ProjectAuthWrapper` sebagai guard; identifier tidak bisa ditebak lintas project tanpa akses.

## Empty / Loading / Error

- Empty: identifier tidak ketemu → `EmptyState`.
- Loading: `Loader`.
- Error: `EmptyState` + retry.

---

## Changelog

| Date       | Change                                                                              |
| ---------- | ----------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `browse/[workItem]/`, store + `issue.py:282` |
