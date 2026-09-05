# Inbox / Notifications

Status: **Approved**
Route: `:workspaceSlug/notifications` — lihat `apps/web/app/routes/core.ts:86-88` (`notifications/page.tsx` render `NotificationsRoot`, + `notifications/layout.tsx`)
Share: CORE

## Intent

Satu tempat untuk semua pemberitahuan workspace: mention, assignment, update work item yang di-subscribe. Dari sudut user: "apa yang butuh perhatianku — baca, snooze, atau arsipkan."

## Current State (snapshot kode)

- Root: `apps/web/core/components/workspace-notifications/root.tsx:29-118` — `NotificationsRoot`: fetch via SWR `WORKSPACE_NOTIFICATION_*` → `getNotifications()`; klik notif → `setCurrentSelectedNotificationId` + peek (`PeekOverviewComponent` via `useNotificationPreview`, atau `InboxContentRoot` bila `is_inbox_issue`).
- Sidebar: `sidebar/root.tsx:28-122` — `NotificationsSidebarRoot`: tab All/Mentions (`NOTIFICATION_TABS`) + `CountChip` unread; list via `notificationIdsByWorkspaceId(workspace.id)` + `notification-card/root.tsx` (`NotificationCardListRoot`).
- Item: `sidebar/notification-card/item.tsx:28-60` — `NotificationItem`: klik → `setPeekIssue` / `setCurrentSelectedNotificationId` + auto `markNotificationAsRead` bila `read_at === null`. Tidak ada grouping unread/read eksplisit — sorting desc `created_at`, filter client `archived_at` / `snoozed_till` (store `:128-154`).
- Filter: `sidebar/filters/menu/root.tsx` + `menu-option-item.tsx` (tipe assigned/created/subscribed); `header/options/menu-option/root.tsx:38-79` (show read/archived/snoozed); `header/options/root.tsx:32-48` — mark-all-as-read.
- Item actions: `notification-card/options/read.tsx`, `archive.tsx:32` (archive/unarchive), `snooze/root.tsx` + `modal.tsx` (snooze till), `options/root.tsx`.
- Bell header: `core/components/navigation/top-navigation-root.tsx:18,30-45,63-79` — ikon `InboxOutline` tooltip "Inbox", link `/{workspaceSlug}/notifications/`, dot `bg-danger-primary` bila `total > 0`; count via `useWorkspaceNotifications().unreadNotificationsCount` (mentions diprioritaskan); fetch SWR `WORKSPACE_UNREAD_NOTIFICATION_COUNT`. Duplikat di sidebar: `workspace-notifications/notification-app-sidebar-option.tsx:25-36`.
- Working: tab All/Mentions + unread chip, read/unread, archive/unarchive, snooze, mark-all-read, peek ke work item, bell + count.
- Stub: —
- Missing (ITSM fork): tidak ada — tidak ada notifikasi SLA/incident-escalation di kode; ide diparkir di [`_backlog.md`](./_backlog.md).

### "Inbox" vs "Intake" vs "Notifications" (terminologi)

| Istilah        | Arti actual                                                                                                                                             |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Notifications  | Halaman ini — workspace-level (`:workspaceSlug/notifications`)                                                                                          |
| Intake         | Triage work item masuk, project-level (`:workspaceSlug/projects/:projectId/intake`, `core.ts:213-216`) — doc terpisah [`intake.md`](./intake.md) (todo) |
| Inbox (legacy) | URL lama `:workspaceSlug/projects/:projectId/inbox` → redirect 302 ke `.../intake/` (`core.ts:385-387` via `routes/redirects/core/inbox.tsx:10-12`)     |
| Inbox (UI)     | Ikon bell tooltip "Inbox" (top-nav) + `components/inbox/*` dipakai sebagai UI Intake + embed preview notifikasi                                         |

### Tanpa realtime socket

Koreksi penting: tidak ada websocket/centrifugo/live untuk notifikasi — hanya SWR fetch on-mount + refetch `getUnreadNotificationsCount` tiap `getNotifications` (store `:345`). Email async via Rust worker (`crates/worker`, Stream `plane:jobs`; cron stack tiap 5 mnt via `crates/beat`) — padanan Django `bgtasks/email_notification_task.stack_email_notification` hanya referensi/fallback.

### Store (MobX)

- Koleksi: `apps/web/core/store/notifications/workspace-notifications.store.ts:63-402` `WorkspaceNotificationStore`: `getNotifications:337` (cursor, `per_page: 300`), `getUnreadNotificationsCount:317`, `markAllNotificationsAsRead:370`, `setCurrentNotificationTab` All/Mentions (`:264`), `filters {type, snoozed, archived, read}` (`:76-85`).
- Item: `store/notifications/notification.ts:33-324` `class Notification`: `markNotificationAsRead` / `UnRead` (`:194` / `:219`, optimistic + rollback count), `archive` / `unArchive` (`:244` / `:267`), `snooze` / `unSnooze` (`:291` / `:311`), `updateNotification` (`:174`).

### Model + API (Django)

> Cutover `rust-cutover-v1`: tabel/skema tidak berubah — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`), kontrak 1:1 (shadow + parity gate). Path Django di bawah = referensi skema.

- Model: `apps/api/plane/db/models/notification.py` — `Notification:13-39` (`workspace` FK, `project` FK null, `data` JSON, `entity_identifier` UUID, `entity_name`, `title` Text, `message` JSON / `message_html` / `message_stripped`, `sender` Char, `triggered_by` FK User SET_NULL, `receiver` FK User CASCADE, `read_at` / `snoozed_till` / `archived_at` nullable; `db_table = "notifications"`, ordering `-created_at`); `UserNotificationPreference:81-108` (`user` / `workspace` / `project` FK + `property_change` / `state_change` / `comment` / `mention` / `issue_completed` Bool default True); `EmailNotificationLog:121-149`.
- API: `apps/api/plane/app/urls/notification.py:16-52` (views `app/views/notification/base.py:33-296`) → `GET users/notifications/` (list, `NotificationViewSet.list:49`), `GET/PATCH/DELETE <uuid:pk>/`, `POST/DELETE <pk>/read/` (`mark_read:169` / `mark_unread:177`), `POST/DELETE <pk>/archive/` (`archive:185` / `unarchive:193`), `GET unread/` (`UnreadNotificationEndpoint:201`), `POST mark-all-read/` (`MarkAllReadNotificationViewSet.create:239`), `users/me/notification-preferences/` (`UserNotificationPreferenceEndpoint:296`).
- FE service 1:1: `web/core/services/workspace-notification.service.ts:25-119`.
- Preferensi email: `web/core/components/settings/profile/content/pages/notifications/root.tsx:21-42` + `email-notification-form.tsx` (`NotificationsProfileSettings`, SWR `CURRENT_USER_EMAIL_NOTIFICATION_SETTINGS` via `UserService.currentUserEmailNotificationSettings()`, GET/PATCH `users/me/notification-preferences/`); toggle per kategori (property/state/comment/mention).

## Primary View

- Layout: sidebar list (tab All/Mentions + filter) + peek panel untuk konten notifikasi.
- Data visible: title, message, sender/triggered_by, entity (work item link), created_at, read/snoozed/archived state.
- Interaction: klik → peek + auto-mark-read; aksi per item (read/unread, snooze, archive); mark-all-read di header.
- Ref: [`_shared/list.md`](./_shared/list.md).

## Actions

| Action        | Trigger                 | Permission       | State required     |
| ------------- | ----------------------- | ---------------- | ------------------ |
| Mark read     | Klik item (auto) / aksi | Owner (receiver) | `read_at === null` |
| Mark unread   | Aksi item               | Owner            | Read               |
| Snooze        | Modal (snooze till)     | Owner            | —                  |
| Unsnooze      | Aksi item               | Owner            | Snoozed            |
| Archive       | Aksi item               | Owner            | —                  |
| Unarchive     | Aksi item               | Owner            | Archived           |
| Mark all read | Header                  | Owner            | Ada unread         |
| Open entity   | Klik → peek             | Member+          | Read project       |
| Email prefs   | Profile settings        | Self             | —                  |

Semua aksi item milik receiver — tidak ada peran admin (notifikasi personal, bukan konten project).

## Filters / Sort / Search

- Filters: tab All/Mentions + tipe (assigned/created/subscribed) + show read/archived/snoozed (`filters {type, snoozed, archived, read}`).
- Sort: desc `created_at` (ordering model).
- Search: tidak ada search per halaman; global via Cmd+K (lihat `_shared/global-search.md` saat ditulis).

## Detail View

- Peek: `PeekOverviewComponent` (via `useNotificationPreview`) atau `InboxContentRoot` bila `is_inbox_issue` — reuse peek Work Items.
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md), [`work-items.md`](./work-items.md).

## Permissions

| Role            | Read own | Actions on own | Email prefs |
| --------------- | -------- | -------------- | ----------- |
| Member          | ✅       | ✅             | ✅ self     |
| Project admin   | ✅       | ✅             | ✅ self     |
| Workspace admin | ✅       | ✅             | ✅ self     |

Notifikasi personal per receiver — tidak ada akses lintas user.

## Empty / Loading / Error

- Empty: no notifications → message (inbox zero) ; tab Mentions kosong → message terpisah.
- Loading: skeleton list + count chip.
- Error: banner + retry.

---

## Changelog

| Date       | Change                                                                                       |
| ---------- | -------------------------------------------------------------------------------------------- |
| 2026-09-03 | init — snapshot actual dari `core.ts`, `workspace-notifications/`, store + `notification.py` |
