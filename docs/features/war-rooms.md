# War Rooms

Status: **Approved**
Route: `:workspaceSlug/projects/:projectId/war-rooms` (list) · `:workspaceSlug/projects/:projectId/war-rooms/:warRoomId` (detail) — lihat `apps/web/app/routes/core.ts:196-207`
Share: CORE

## Intent

Ruang koordinasi insiden per project: peta blast radius, chat realtime, runbook, work item terkait, catatan, dan aktivitas dalam satu halaman — supaya respons insiden tidak tersebar di chat dan issue.

## Current State (snapshot kode)

- Halaman: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/**` (list + detail).
- Komponen: `apps/web/core/components/war-rooms/**` — list/board + summary, create modal, room header (lifecycle + join/leave), `service-map` (blast radius), chat, context panel (Notes/Work items/Runbook/Activity/People), modal resolve/archive/delete/edit details.
- Store/service/hook: `apps/web/core/store/war-room.store.ts`, `apps/web/core/services/war-room.service.ts`, `apps/web/core/hooks/use-war-room-socket.ts`.
- Backend: `apps/api-rs/crates/api/src/routes/war_room.rs`; migrasi `apps/api-rs/migrations/0011_war_rooms.sql`.
- Realtime: controller `war-rooms` di `apps/live`, relay event lewat Redis `war-room:events`.
- Working: list + tab status + search + summary; create (primary incident wajib, 409 bila sudah ada room aktif untuk incident itu); detail map-first; chat (kirim/edit/hapus, mention `@{user_id}`, typing, presence, reconnect); runbook; notes autosave; work items; activity; participants + role; lifecycle transitions; notifikasi mention in-app.
- Stub: —
- Missing (ITSM fork): — (Problem/Change/Request/Knowledge terpisah belum ada; ide diparkir di [`_backlog.md`](./_backlog.md)).

## Primary View

- Layout B (map first): panel kiri peta blast radius read-only (canvas `service-graph-canvas.tsx`; node affected + 1-hop neighbor + legenda); panel kanan chat + tab konteks.
- Header: identifier `WR-{sequence_id}`, nama, badge severity/status, timer elapsed, aksi lifecycle (Resolve/Reopen/Archive), edit details, join/leave, menu delete.
- List: board row per room (identifier, nama, severity, status, incident utama, peserta, waktu) + summary chip (active / SEV1–2 / resolved 7d).

## Actions

| Action                         | Trigger               | Permission                      | State required                                 |
| ------------------------------ | --------------------- | ------------------------------- | ---------------------------------------------- |
| Create                         | Toolbar / empty state | Project member                  | —                                              |
| Open room                      | Row click             | Project member; guest read-only | —                                              |
| Send message                   | Composer Enter        | Project member                  | status != archived                             |
| Edit/delete message            | Hover pesan sendiri   | Author                          | status != archived                             |
| Mention                        | `@` di composer       | Project member                  | Mentioned harus workspace member               |
| Change status                  | Header lifecycle menu | Project member                  | transisi valid (`WAR_ROOM_STATUS_TRANSITIONS`) |
| Resolve                        | Header                | Project member                  | status active/monitoring                       |
| Reopen                         | Header                | Project member                  | status resolved                                |
| Archive                        | Header/menu           | Project member                  | status != archived                             |
| Delete                         | Header menu           | Project member                  | konfirmasi modal                               |
| Edit details                   | Header menu           | Project member                  | status != archived                             |
| Link/unlink services           | Header map            | Project member                  | status != archived                             |
| Add participant / change role  | People tab            | Project member                  | status != archived                             |
| Join/Leave                     | Header / People tab   | Project member                  | status != archived                             |
| Edit notes                     | Notes tab             | Project member                  | status != archived                             |
| Runbook toggle/add/edit/delete | Runbook tab           | Project member                  | status != archived                             |
| Link/unlink work items         | Work items tab        | Project member                  | status != archived                             |

## Filters / Sort / Search

- Tabs: Active (`active,monitoring`), Resolved (`resolved`), All.
- Search: nama room / identifier + nama incident (server-side `search`).
- Sort: terbaru dulu (`-created_at`); summary chip dari endpoint `summary/`.

## Detail View

- Panel kiri: peta blast radius (affected + 1-hop neighbor, dua arah dependency).
- Panel kanan: chat + tab Notes (rich-text autosave 1 detik), Work items (primary incident pinned), Runbook (progress + checklist), Activity (feed kronologis live), People (peserta + role + indikator online).
- Chat: pagination `before_id`, optimistic send + `client_id`, edit/delete author-only, typing/presence, banner reconnect.

## Permissions

| Role           | Create | Read | Update | Delete                   |
| -------------- | ------ | ---- | ------ | ------------------------ |
| Project member | ✅     | ✅   | ✅     | ✅ (room; pesan sendiri) |
| Guest          | ❌     | ✅   | ❌     | ❌                       |
| Non-member     | ❌     | ❌   | ❌     | ❌                       |

## Empty / Loading / Error

- Empty: list → pesan + CTA create; map tanpa services → placeholder; chat/runbook/activity/notes/work items punya empty state masing-masing.
- Loading: skeleton list/room; spinner saat load pesan lama.
- Error: banner + retry (`war_room.load_error`, `chat.load_failed`, `activity.load_failed`, dst.); 409 duplicate active room → tawaran buka room existing; aksi saat archived ditolak (`errors.room_archived`).

## Realtime & Notifikasi

- Socket: `ws(s)://<live>/war-rooms/<roomId>?workspaceSlug=&projectId=&token=`; event `message.created|updated|deleted`, `room.changed`, `activity.created`, `typing`, `presence.joined|left`; publish best-effort via Redis `war-room:events`.
- Status koneksi: `connecting|connected|reconnecting|disabled`; banner `chat.reconnect_banner` hanya saat gagal jaringan; auth gagal (close 4403) → `disabled` tanpa banner.
- Mention: server mem-parse `@{user_id}` (workspace member saja), mengisi `mentions`, dan menulis row `notifications` (`entity_name = 'war_room'`, `sender = 'in_app:war_room:mentioned'`) kecuali author; klik notifikasi membuka room.

## Edge Cases

- Incident dihapus/archived: room tetap; chip incident menampilkan state archived.
- Maksimum satu commander per room (partial unique); room tanpa commander valid.
- Service dihapus: link soft-delete; peta kehilangan node (fallback empty state).
- Dua create untuk incident sama: unique index partial → 409 `active_war_room_exists`.
- Kirim pesan saat archived: 409 `room_archived`.
- WS putus saat kirim: REST tetap sukses; pesan ditandai terkirim + banner reconnect.
- Presence multi-instance: best-effort, bukan locking.
- Rate limit pesan memakai middleware existing; typing di-throttle client (maks 1 event/2 detik).
- Name > 255 karakter: 400 server; UI `maxLength`.

## API Touchpoints

Base `/api/workspaces/:slug/projects/:project_id/war-rooms/`: list/create, `summary/`, `:pk/` (GET/PATCH/DELETE), `links/`, `services/`, `issues/`, `participants/`, `runbook/`, `messages/`, `events/` — lihat `apps/api-rs/crates/api/src/routes/war_room.rs` dan `docs/superpowers/specs/2026-09-30-war-room-design.md`.

## Changelog

| Date       | Change                                                                                                                              |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| 2026-10-01 | Doc dibuat — Fase 1–4 sudah shipped (backend, chat realtime, list+create, room page); Fase 5 menambah notifikasi mention + doc ini. |
