---
status: current
last_verified: 2026-10-27
source_hash: sha256:9ab1243bb8fcbd347d04f887d1bc52319a9fedf22e9a4a4b2dffb809da39117c
source_paths:
  - apps/web/app/routes/core.ts
  - apps/web/core/store/war-room.store.ts
  - apps/web/core/components/war-rooms
  - apps/api-rs/crates/api/src/routes/war_room.rs
  - apps/api-rs/migrations/0011_war_rooms.sql
  - apps/live
---

# War Rooms

Status: **Current.**

## Intent

War Room mengubah Incident ticket menjadi ruang koordinasi major incident:
chat realtime, runbook, participant, blast radius, dan work item terkait dalam
satu halaman.

## Routes

| Surface                | Route                                                      |
| ---------------------- | ---------------------------------------------------------- |
| Filtered incident list | `/:workspaceSlug/projects/:projectId/issues?war_room=on`   |
| War room detail        | `/:workspaceSlug/projects/:projectId/war-rooms/:warRoomId` |
| Legacy list            | `.../war-rooms` → filtered issue list                      |

Tidak ada item sidebar "Incident bridge"; akses lewat filter ticket dan detail
war room.

## Lifecycle

1. Buka Incident ticket.
2. Aktifkan properti War Room.
3. Sistem membuat atau membuka kembali room.
4. Koordinasikan respons di room.
5. Nonaktifkan War Room untuk resolve.

Satu incident hanya boleh memiliki satu active room.

## Realtime chat

- Chat, presence, typing indicator, mention, dan history.
- Event realtime diteruskan `apps/live` lewat Redis.
- Pesan tetap tersimpan saat room ditutup.
- Reconnect harus memuat history terbaru.

## Runbook and blast radius

- Runbook disimpan di room.
- Service graph menampilkan service terdampak.
- Work item terkait ditautkan dari room.
- Activity timeline mencatat perubahan penting.

## API touchpoints

- `war_room.rs` — CRUD, message, participant, activity.
- Migrasi `0011`, `0017`.
- Store: `apps/web/core/store/war-room.store.ts`.
- Komponen: `apps/web/core/components/war-rooms`.

Inventory: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Create                  | Read     | Chat  | Resolve |
| ----------------- | ----------------------- | -------- | ----- | ------- |
| Member            | Jika punya akses ticket | Ya       | Ya    | Tidak   |
| Application admin | Ya                      | Ya       | Ya    | Ya      |
| Workspace admin   | Ya                      | Ya       | Ya    | Ya      |
| Guest             | Tidak                   | Terbatas | Tidak | Tidak   |

## Limitations

- Hanya Incident yang dapat memiliki War Room.
- Severity mengikuti priority ticket; tidak ada severity terpisah.
- Service blast radius bergantung pada dependency service yang sudah diisi.
- Chat bukan pengganti incident timeline di sistem eksternal.

## In-app guide

`/:workspaceSlug/docs/war-rooms` dan `/:workspaceSlug/docs/itil4-incident`.

## Changelog

| Tanggal    | Perubahan                                                         |
| ---------- | ----------------------------------------------------------------- |
| 2026-10-27 | rewrite — lifecycle, realtime, runbook, blast radius, permissions |
