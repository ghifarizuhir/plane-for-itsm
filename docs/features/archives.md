# Archives (Soft-Delete + Restore)

Status: **Approved**
Route:

- `:workspaceSlug/projects/:projectId/archives/issues` (list) + `.../archives/issues/:archivedIssueId` (detail) — `apps/web/app/routes/core.ts:222-236`
- `:workspaceSlug/projects/:projectId/archives/cycles` — `core.ts:238-244`
- `:workspaceSlug/projects/:projectId/archives/modules` — `core.ts:246-252`
- `:workspaceSlug/projects/archives` (archived projects) — `core.ts:117-123`

Share: CORE

## Intent

Tempat sampah yang bisa dikembalikan: issues/cycles/modules/projects yang di-archive hilang dari view aktif tapi tetap bisa di-restore — delete permanen adalah aksi terpisah dan eksplisit. Dari sudut user: "kembalikan yang terlanjur diarsip, atau hapus permanen kalau yakin."

## Current State (snapshot kode)

- Issues list: `archives/issues/(list)/page.tsx:29-30` — `ArchivedIssuesHeader` + `ArchivedIssueLayoutRoot` (`core/components/issues/archived-issues-header.tsx:23`, `issue-layouts/roots/archived-issue-layout-root.tsx:24` → `ArchivedIssueListLayout` di `list/roots/archived-issue-root.tsx:12` + `ArchivedIssueQuickActions`).
- Issue detail: `archives/issues/(detail)/[archivedIssueId]/page.tsx:37-39,86-91` — `fetchIssue` via SWR + `IssueDetailRoot is_archived` + `Banner` restore.
- Cycles: `archives/cycles/page.tsx:29-30` — `ArchivedCyclesHeader` + `ArchivedCycleLayoutRoot` (`core/components/cycles/archived-cycles/root.tsx`, `header.tsx`, `view.tsx`, `modal.tsx`).
- Modules: `archives/modules/page.tsx:28-29` — `ArchivedModulesHeader` + `ArchivedModuleLayoutRoot` (`core/components/modules/archived-modules/root.tsx`, `header.tsx`, `view.tsx`, `modal.tsx`).
- Projects: `archives/page.tsx:7-11` — `ProjectPageRoot` (`core/components/projects/page.tsx:16`).
- Archive modal: `core/components/issues/archive-issue-modal.tsx:27` (`ArchiveIssueModal`); menu restore `quick-action-dropdowns/helper.tsx:244-248,377-382`, `archived-issue.tsx:24`.
- Working: list + detail archived per tipe, restore via banner/menu, delete permanen terpisah.
- Stub: —
- Missing (ITSM fork): tidak ada.

### Konsep: `archived_at`, bukan flag

Archive = set `archived_at` (non-null = arsip); restore = null-kan; delete permanen = aksi terpisah (`deleteProject`, `removeArchivedIssue`). Query default exclude arsip (`issue.py:98-99`).

### Store (MobX)

- Issues: `store/issue/archived/issue.store.ts:94,135,188-197` (`fetchIssues` / `fetchNextIssues`, `restoreIssue` → `removeIssueFromList`); aktif: `issue/project-views | workspace/issue.store.ts:50-53` (`archiveIssue = issueArchive`, `archiveBulkIssues`).
- Cycles: `cycle.store.ts:437,682-722` (`fetchArchivedCycles`, `archiveCycle:688`, `restoreCycle:711` set `archived_at` / null).
- Modules: `module.store.ts:338,598-638` (`fetchArchivedModules`, `archiveModule:604`, `restoreModule:627`).
- Projects: `project/project.store.ts:73-76,581-625` (`deleteProject:581` vs `archiveProject:602` vs `restoreProject:623`).

### Service + Model + API (Django)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- Service (pola sama `POST .../archive/` + `DELETE .../archive|unarchive/`): `issue/issue_archive.service.ts:24,36-51` (`GET archived-issues/`, `POST/DELETE .../archive/` = `restoreIssue`, `bulk-archive-issues/` di `issue.service.ts:368-370`); `cycle_archive.service.ts:20,42,50`; `module_archive.service.ts:20,42,50`; `project/project-archive.service.ts:23,31`.
- Model: `archived_at` di `issue.py:160` (DateField null), `cycle.py:75`, `module.py:98`, `project.py:114` (DateTimeField null).
- API: `urls/issue.py:99-101,224-230` (`bulk-archive-issues/`, `archived-issues/` + `.../<pk>/archive/`, `IssueArchiveViewSet` di `views/issue/archive.py:53`: list/retrieve/archive/unarchive); `urls/cycle.py:83-93`, `urls/module.py:92-102`, `urls/project.py:124` (`archived-cycles | modules` + `archive/unarchive`; impl `views/cycle/archive.py:40`, `views/module/archive.py:42`, `views/project/base.py:427` — `archived_at = now()` vs `= None`).

## Primary View

- Layout: list archived per tipe (issues reuse layout khusus; cycles/modules/projects list sendiri) + detail archived (issues: full detail + restore banner).
- Data visible: sama dengan tipe aslinya + archived state.
- Interaction: restore via banner/menu; delete permanen via confirm terpisah.
- Ref: [`work-items.md`](./work-items.md), [`cycles.md`](./cycles.md), [`modules.md`](./modules.md).

## Actions

| Action          | Trigger               | Permission | State required          |
| --------------- | --------------------- | ---------- | ----------------------- |
| Archive         | Modal / quick actions | Member+    | Aktif, own atau admin\* |
| Bulk archive    | Select + bulk bar     | Member+    | Aktif                   |
| Restore         | Banner / menu         | Member+    | Archived                |
| Delete permanen | Confirm terpisah      | Admin      | Archived                |

\* Mengikuti hierarchy workspace/project Plane.

## Filters / Sort / Search

- Filters: minimal (list archived read-only + restore).
- Sort: default (archived desc).
- Search: tidak ada search per halaman; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Issues archived punya detail penuh (`IssueDetailRoot is_archived` + restore `Banner`); cycles/modules/projects archived list-only.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md).

## Permissions

| Role            | Archive | Restore | Delete permanen |
| --------------- | ------- | ------- | --------------- |
| Member          | ✅ own  | ✅ own  | ❌              |
| Project admin   | ✅      | ✅      | ✅              |
| Workspace admin | ✅      | ✅      | ✅              |

## Empty / Loading / Error

- Empty: no archived → message (arsip kosong).
- Loading: skeleton per section.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                            |
| ---------- | --------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, archived roots, store + `archive.py` views |
