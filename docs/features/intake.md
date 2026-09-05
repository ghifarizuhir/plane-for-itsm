# Intake (Triage)

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/intake` — lihat `apps/web/app/routes/core.ts:213-218` (`projects/(detail)/[projectId]/intake/page.tsx` + `layout.tsx`)
Share: CORE

## Intent

Pintu masuk kerja baru: work items yang masuk (dari form, integrasi, atau anggota) ditampung dulu, lalu di-triage — accept (masuk backlog), decline, snooze, atau tandai duplicate — sebelum mengotori backlog utama. Dari sudut user: "saring dulu yang masuk, yang layak baru jadi work item beneran."

## Current State (snapshot kode)

- Page: `intake/page.tsx:46,83-89` — gate via `currentProjectDetails.inbox_view` (flag bernama inbox, route bernama intake); render `InboxIssueRoot` (`inbox/root.tsx:20`).
- Legacy rename: URL lama `.../inbox` → redirect 302 ke `.../intake/` (`core.ts:385-387` via `routes/redirects/core/inbox.tsx:10-13`). Nama komponen/service masih `inbox-*` di banyak tempat — itu warisan nama, bukan halaman Inbox.
- List: `apps/web/core/components/inbox/sidebar/inbox-list.tsx` + `inbox-list-item.tsx` + `sidebar/root.tsx` — daftar pending + tab open/closed.
- Detail: `inbox/content/issue-root.tsx` + `inbox-issue-header.tsx` + `issue-properties.tsx` + `content/root.tsx`.
- Actions: `inbox/modals/decline-issue-modal.tsx` (reject), `snooze-issue-modal.tsx` (snooze), `select-duplicate.tsx` (duplicate), `inbox-issue-status.tsx`; accept via `updateInboxIssueStatus(ACCEPTED)` → pindah ke default state project (`app/serializers/intake.py:51-69`).
- Working: tab open/closed, accept/decline/snooze/duplicate, properties, description versions.
- Stub: `intake.service.ts:10-14` (`IntakeService` kosong — CRUD via base); `IntakeIssueService.list()` masih hit `/inbox-issues/` (`packages/services/src/intake/issue.service.ts:15-23`).
- Missing (ITSM fork): tidak ada — tidak ada triage request/incident di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### Bukan draft, bukan state Issue biasa

Koreksi penting: Intake bukan `Issue.is_draft` dan bukan state Issue biasa — melainkan bridge `IntakeIssue(intake, issue, status, snoozed_till, duplicate_to)` (`apps/api/plane/db/models/intake.py:42-62`). Status: `PENDING = -2`, `REJECTED = -1`, `SNOOZED = 0`, `ACCEPTED = 1`, `DUPLICATE = 2`. Mirror FE: `EInboxIssueStatus` (`packages/types/src/inbox.ts:19-24`); state khusus triage `IIntakeState{group: "triage"}` (`packages/types/src/intake/state.ts:7-19`, endpoint `app/urls/state.py:23-25` `intake-state/`). `Issue` hanya flag `is_intake` (`app/serializers/issue.py:937-943`).

### Store (MobX)

- `apps/web/core/store/inbox/project-inbox.store.ts:38` `IProjectInboxStore` / `ProjectInboxStore` (list/pagination/filter) + `store/inbox/inbox-issue.store.ts:35-37,99-208` `InboxIssueStore` (`updateInboxIssueStatus` / `updateInboxIssueDuplicateTo` / `updateInboxIssueSnoozeTill`, sinkron `intake_count`).

### Model + API (Django)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- Model: `apps/api/plane/db/models/intake.py:12-18` `Intake(name, is_default, view_props)` + `:50-80` `IntakeIssue`.
- API: `apps/api/plane/app/urls/intake.py:16-55` → `intakes/`, `intake-issues/[/<pk>/]` + alias legacy `inboxes/`, `inbox-issues/`; `:56-65` `intake-work-items/<id>/description-versions/`; views `app/views/intake/base.py:57-80` (`IntakeViewSet` / `IntakeIssueViewSet`).

### Route terkait

- `:workspaceSlug/settings/projects/:projectId/features/intake` (`core.ts:322-325`) — toggle fitur (`settings/projects/[projectId]/features/intake/page.tsx:1-64`).

## Primary View

- Layout: sidebar list (pending + tab open/closed) + content detail (header, properties, description).
- Data visible: title, reporter, properties, status triage, snoozed_till, duplicate_to.
- Interaction: klik item → detail; aksi triage per item (accept/decline/snooze/duplicate).
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action        | Trigger             | Permission | State required  |
| ------------- | ------------------- | ---------- | --------------- |
| Accept        | Status → ACCEPTED   | Member+    | PENDING/SNOOZED |
| Decline       | Modal (reject)      | Member+    | PENDING/SNOOZED |
| Snooze        | Modal (snooze till) | Member+    | PENDING         |
| Duplicate     | Select duplicate    | Member+    | PENDING         |
| View versions | Description history | Member+    | —               |

Accept memindahkan issue ke default state project (keluar dari antrean triage).

## Filters / Sort / Search

- Filters: tab open/closed; filter store project-inbox.
- Sort: default (created desc).
- Search: scope per halaman; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Section: header + properties + description (+ versions) — reuse pola peek/detail Work Items.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`work-items.md`](./work-items.md).

## Permissions

| Role            | Triage | Read | Config (toggle) |
| --------------- | ------ | ---- | --------------- |
| Member          | ✅     | ✅   | ❌              |
| Project admin   | ✅     | ✅   | ✅              |
| Workspace admin | ✅     | ✅   | ✅              |

Catatan: halaman kosong bila fitur intake dimatikan (`inbox_view === false`).

## Empty / Loading / Error

- Empty: antrean kosong → message (inbox zero triage); fitur off → gate di page.
- Loading: skeleton list + content.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                          |
| ---------- | ------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `inbox/` components, store + `intake.py` |
