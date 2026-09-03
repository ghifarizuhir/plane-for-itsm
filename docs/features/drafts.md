# Drafts

Status: **Approved**
Route: `:workspaceSlug/drafts` — lihat `apps/web/app/routes/core.ts:80-83` (`drafts/page.tsx:12-20` → `WorkspaceDraftIssuesRoot workspaceSlug`)
Share: CORE

## Intent

Coretan work item lintas project yang belum siap jadi issue beneran: tulis cepat, simpan sebagai draft, publish (`moveToIssue`) kalau sudah matang atau discard. Dari sudut user: "notulen ide-mentah sebelum masuk project mana pun."

## Current State (snapshot kode)

- List: `apps/web/core/components/issues/workspace-draft/root.tsx:32` — root list + `DraftIssueBlock`, empty-state, loader.
- Item: `workspace-draft/draft-issue-block.tsx:39-44,58,107` — state `moveToIssue`, flag `is_draft: true`, `deleteIssue()` = discard.
- Publish = pindah: `moveIssue()` menghapus dari map draft + decrement count (`core/store/issue/workspace-draft/issue.store.ts:115,339-343` `class WorkspaceDraftIssues`); aksi via `useWorkspaceDraftIssueActions(EIssuesStoreType.WORKSPACE_DRAFT)` (`core/hooks/use-issues-actions.tsx:737-743`); registrasi `workspaceDraftIssues{,Filter}` (`core/store/issue/root.store.ts:74-75,236-237`).
- Scope workspace-level (belum terikat project) — beda dari Intake yang project-level dan terikat triage.
- Working: CRUD draft, publish ke issue, discard, empty-state.
- Stub: —
- Missing (ITSM fork): tidak ada.

### Draft pindah tabel, bukan flag

Koreksi penting: `Issue.is_draft` (`issue.py:100,161`, manager exclude `is_draft = True`) adalah legacy — aktual sudah migrasi ke tabel `draft_issues` (`migrations/0077_draftissue_*:10-49`). Service: `core/services/issue/workspace_draft.service.ts:22,30,41,53,61,69` (`GET/POST/PATCH/DELETE draft-issues/`, `POST draft-to-issue/{id}/` = publish).

### Store (MobX)

- `apps/web/core/store/issue/workspace-draft/issue.store.ts:115` `class WorkspaceDraftIssues` (`fetch`, `moveIssue`, delete, count).

### Model + API (Django)

- ViewSet: `apps/api/plane/app/views/workspace/draft.py:46-51` (`WorkspaceDraftIssueViewSet`, `model = DraftIssue`).
- API: `apps/api/plane/app/urls/workspace.py:203,208,213` → `draft-issues/`, `draft-issues/<pk>/`, `draft-to-issue/<draft_id>/`.

## Primary View

- Layout: list of draft blocks (workspace scope).
- Data visible: title/description mentah, created_at.
- Interaction: klik → edit; publish → pilih project (moveToIssue); discard → delete.
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action  | Trigger                       | Permission | State required |
| ------- | ----------------------------- | ---------- | -------------- |
| Create  | Quick-add                     | Member+    | —              |
| Edit    | Draft block                   | Own        | Draft          |
| Publish | `moveToIssue` (pilih project) | Own        | Draft          |
| Discard | `deleteIssue()`               | Own        | Draft          |

Draft personal per user dalam workspace.

## Filters / Sort / Search

- Filters: `workspaceDraftFilter` (minimal).
- Sort: created desc.
- Search: tidak ada search per halaman; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Tidak ada detail page sendiri — edit inline di block.
- Ref: [`work-items.md`](./work-items.md) (hasil publish).

## Permissions

| Role            | Own drafts | Others' drafts |
| --------------- | ---------- | -------------- |
| Member          | ✅         | ❌             |
| Project admin   | ✅         | ❌             |
| Workspace admin | ✅         | ❌             |

## Empty / Loading / Error

- Empty: no drafts → message + CTA quick-add.
- Loading: loader.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                        |
| ---------- | ----------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `workspace-draft/`, store + `draft.py` |
