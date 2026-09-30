# War Room (Incident Command Room) — Design

Tanggal: 2026-09-30
Status: disetujui user saat brainstorming (Approach 1: REST persist + Redis relay; layout B "Map First"); menunggu review spec tertulis.
Scope: web (`apps/web`, `packages/types`, `packages/constants`, `packages/i18n`), live server (`apps/live`), API (`apps/api-rs` + migrasi sqlx).
Terkait: ide war room diparkir di `docs/features/_backlog.md`; pola backend mengikuti `2026-09-13-services-backend-design.md`; layout & komponen service mengikuti `2026-09-17-services-list-health-redesign-design.md`.

## Latar

War room adalah ruang koordinasi insiden per project: satu halaman yang menyatukan peta service terdampak (blast radius), chat realtime, work item terkait, runbook, ringkasan (notes), activity feed, dan peserta dengan perannya. Ini menjawab parkir ide "War Room (`isWarRoom`)" di backlog ITSM.

Basis yang sudah ada dan dipakai ulang:

- **Services** project-level (`projects/:projectId/services`) dengan health, dependency graph `@xyflow/react` (`apps/web/core/components/services/graph/service-graph.tsx`), detail service, dan `service-issues` link.
- **Work item picker** `ExistingIssuesListModal` (`apps/web/core/components/core/modals/existing-issues-list-modal.tsx`), dipakai service detail.
- **Live server** (`apps/live`) Hocuspocus + Redis pub/sub + `@Controller`/`@WSDecorator` (`apps/live/src/controllers/index.ts`), auth `handleAuthentication` (`apps/live/src/lib/auth.ts`).
- **api-rs** sudah punya koneksi Redis (`apps/api-rs/crates/api/src/state.rs: redis_client()`), helper RBAC project (`apps/api-rs/crates/api/src/routes/issue_common.rs: fetch_project_member_role, is_workspace_admin, project_gate_allows`), pola route services (`routes/service.rs`) dan sequence issue (`routes/issue_write.rs: insert_issue`).

## Keputusan yang dikunci saat brainstorming

1. **Scope route**: project-level `:workspaceSlug/projects/:projectId/war-rooms`.
2. **Entitas**: tabel baru (bukan reuse work item Incident), backend nyata.
3. **Realtime**: REST persist ke api-rs → publish Redis → live server relay ke WebSocket. Presence/typing ephemeral.
4. **Layout room**: Map First — header, peta blast radius di atas, chat kiri bawah, panel konteks kanan bawah.
5. **Relasi incident**: satu incident utama wajib (`primary_issue_id` FK) + work item tambahan opsional.
6. **Lifecycle**: `active → monitoring → resolved → archived` (+ reopen), severity sendiri `SEV1–SEV4` (default dari priority incident).
7. **Panel MVP**: peta, chat, work items, runbook/checklist, notes, activity feed, participants & role.
8. **Create**: modal ringkas — incident utama, name (auto dari incident), severity (auto), services terdampak (default dari link incident), deskripsi.
9. **Chat**: teks + `@mention` + edit/hapus pesan sendiri + indikator online/typing; riwayat di DB; attachment fase 2.
10. **Peta**: service terdampak + tetangga 1 hop (dua arah), read-only, klik node → detail service.
11. **Izin**: semua member project bisa lihat/chat/join/centang runbook; aksi kunci (status, severity, peserta, link, notes/runbook edit, resolve, archive) commander + project/workspace admin.
12. **Runbook**: template bawaan per tipe work item, auto-seed saat create, item bisa ditambah/diubah/dicentang di room.
13. **Notes**: satu dokumen rich-text pinned, autosave, last-write-wins (bukan kolaboratif). Ditempatkan sebagai tab di panel konteks (mockup v2).
14. **Identifier**: `WR-n` sequence per project (pola `issue_sequences`/advisory lock issue).
15. **Notifikasi mention**: in-app via tabel `notifications` (`entity_name = war_room`), klik membuka room.
16. **Non-goals fase ini**: attachment/reactions/thread chat, template runbook editable, AI panel, related intake/alerts, war room lintas project, notifikasi eksternal.

## Desain

### 1. Model data & migrasi

Migrasi baru `apps/api-rs/migrations/0011_war_rooms.sql` (delta plain non-idempotent, gaya kolom Django: `created_at/updated_at NOT NULL`, `*_by_id`, `deleted_at` soft delete; UUID PK di-generate aplikasi). Semua tabel membawa `workspace_id` + `project_id`.

**`war_rooms`**

| Kolom                            | Tipe                                  | Catatan                                         |
| -------------------------------- | ------------------------------------- | ----------------------------------------------- |
| `id`                             | uuid PK                               |                                                 |
| `workspace_id`                   | uuid NOT NULL                         | FK `workspaces(id)` CASCADE                     |
| `project_id`                     | uuid NOT NULL                         | FK `projects(id)` CASCADE                       |
| `sequence_id`                    | bigint NOT NULL                       | per project; unique `(project_id, sequence_id)` |
| `name`                           | varchar(255) NOT NULL                 |                                                 |
| `description_html`               | text NOT NULL DEFAULT ''              | rich text dari modal create                     |
| `notes_html`                     | text NOT NULL DEFAULT ''              | dokumen notes tab                               |
| `severity`                       | varchar(10) NOT NULL DEFAULT 'sev3'   | CHECK `sev1..sev4`                              |
| `status`                         | varchar(20) NOT NULL DEFAULT 'active' | CHECK `active/monitoring/resolved/archived`     |
| `primary_issue_id`               | uuid NOT NULL                         | FK `issues(id)` ON DELETE CASCADE               |
| `started_at`                     | timestamptz NOT NULL DEFAULT now()    | dasar timer                                     |
| `resolved_at`                    | timestamptz NULL                      | diisi saat resolve, dikosongkan saat reopen     |
| `created_at`, `updated_at`       | timestamptz NOT NULL                  |                                                 |
| `created_by_id`, `updated_by_id` | uuid NULL                             |                                                 |
| `deleted_at`                     | timestamptz NULL                      |                                                 |

Index: `(project_id, status) WHERE deleted_at IS NULL`, `(primary_issue_id)`, partial unique `(project_id, primary_issue_id) WHERE status IN ('active','monitoring') AND deleted_at IS NULL` (satu room aktif per incident).

**`war_room_services`** — `id`, `workspace_id`, `project_id`, `war_room_id` FK CASCADE, `service_id` FK `services(id)` CASCADE, audit + `deleted_at`. Partial unique `(war_room_id, service_id) WHERE deleted_at IS NULL`.

**`war_room_issues`** — sama seperti di atas dengan `issue_id` FK `issues(id)` CASCADE (work item tambahan; incident utama tetap di `war_rooms.primary_issue_id`).

**`war_room_participants`** — `id`, `workspace_id`, `project_id`, `war_room_id` FK CASCADE, `member_id` FK `users(id)` CASCADE, `role` varchar(20) CHECK `commander/comms/scribe/responder`, `joined_at` timestamptz NOT NULL DEFAULT now(), audit + `deleted_at`. Partial unique `(war_room_id, member_id) WHERE deleted_at IS NULL` dan `(war_room_id) WHERE role = 'commander' AND deleted_at IS NULL` (tepat satu commander).

**`war_room_messages`** — `id`, `workspace_id`, `project_id`, `war_room_id` FK CASCADE, `author_id` FK `users(id)` ON DELETE SET NULL, `body` text NOT NULL, `mentions` jsonb NOT NULL DEFAULT '[]' (array user id), `edited_at` timestamptz NULL, audit + `deleted_at`. Index `(war_room_id, created_at DESC) WHERE deleted_at IS NULL`.

**`war_room_runbook_items`** — `id`, `workspace_id`, `project_id`, `war_room_id` FK CASCADE, `title` varchar(500) NOT NULL, `sort_order` double precision NOT NULL DEFAULT 65535, `is_done` boolean NOT NULL DEFAULT false, `done_by_id` uuid NULL, `done_at` timestamptz NULL, `template_key` varchar(100) NULL (penanda item hasil seed), audit + `deleted_at`.

**`war_room_events`** — append-only: `id`, `workspace_id`, `project_id`, `war_room_id` FK CASCADE, `actor_id` uuid NULL, `event_type` varchar(50) NOT NULL, `payload` jsonb NOT NULL DEFAULT '{}', `created_at` timestamptz NOT NULL DEFAULT now(). Index `(war_room_id, created_at DESC)`. Tanpa soft delete.

Tipe event: `room.created`, `room.status_changed`, `room.severity_changed`, `room.resolved`, `room.reopened`, `room.archived`, `participant.joined`, `participant.left`, `participant.role_changed`, `service.linked`, `service.unlinked`, `issue.linked`, `issue.unlinked`, `runbook.item_done`, `runbook.item_reopened`.

**Sequence `WR-n`**: saat create, `SELECT id FROM projects WHERE id = $1 FOR UPDATE` lalu `COALESCE(MAX(sequence_id), 0) + 1` dari `war_rooms` project itu (termasuk yang soft-delete supaya nomor tidak pernah dipakai ulang) — mirror pola `insert_issue` (`routes/issue_write.rs:204-213`).

**Template runbook bawaan** (server-side, saat create; key = nama tipe work item ternormalisasi lowercase):

| Tipe     | Item                                                                                                                                |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| incident | Triage & assess impact · Mitigate (rollback/redeploy) · Communicate status update · Verify recovery & monitor · Schedule postmortem |
| problem  | Confirm root cause hypothesis · Collect evidence & timeline · Identify permanent fix · Create change plan · Update knowledge base   |
| change   | Pre-change verification · Execute change steps · Validate service health · Rollback if needed · Close change record                 |
| request  | Confirm requester details · Check fulfilment steps · Execute fulfilment · Notify requester                                          |

Tipe lain → tanpa item (room tetap bisa; item ditambah manual). Teks disimpan sebagai baris saat create (`template_key` = slug item), UI copy EN.

**Delete semantics**: soft delete room = soft delete room + participants + messages + runbook items + links (satu transaksi) — event feed tetap tersimpan. Delete = project admin.

### 2. API (session auth, `/api/`)

Prefix: `workspaces/:slug/projects/:project_id/war-rooms/`. `:pk` = uuid room. Reuse helper RBAC: `fetch_project_member_role`, `is_workspace_admin`, `project_gate_allows`; helper baru lokal `gate_commander(pool, user, slug, project_id, room_id)` = member project DAN (project admin+ ATAU participant room dengan role `commander`).

**Room**

| Method | Path                  | Body / Query                                                                            | Permission    | Respons                         |
| ------ | --------------------- | --------------------------------------------------------------------------------------- | ------------- | ------------------------------- |
| GET    | `/war-rooms/`         | `status` (csv), `severity` (csv), `q`                                                   | member        | array list item                 |
| GET    | `/war-rooms/summary/` | —                                                                                       | member        | `{active, sev1_2, resolved_7d}` |
| POST   | `/war-rooms/`         | `name?`, `primary_issue_id` (wajib), `severity?`, `description_html?`, `service_ids?[]` | member        | 201 detail                      |
| GET    | `/war-rooms/:pk/`     | —                                                                                       | member        | detail lengkap                  |
| PATCH  | `/war-rooms/:pk/`     | `name?`, `severity?`, `status?`, `description_html?`, `notes_html?`                     | commander     | 200 detail                      |
| DELETE | `/war-rooms/:pk/`     | —                                                                                       | project admin | 204                             |

List item: `id, sequence_id, name, severity, status, primary_issue {id, identifier, name, priority, state_group}, services[{id,name,status}], participants[{id, display_name, avatar_url, role}], service_count, participant_count, message_count, started_at, resolved_at, last_activity_at`. Default sort: status group (active, monitoring, resolved, archived), severity asc, `started_at` desc. `q` match nama room + identifier/nama incident. Respons list berupa array penuh (mengikuti `GET services/`, tanpa pagination di MVP; pagination jadi follow-up bila volume room sudah besar).

Create validasi: incident ada dan se-project; tolak 409 `{"error": "active_war_room_exists", "war_room_id": ...}` bila sudah ada room active/monitoring untuk incident itu; `name` default nama incident; `severity` default mapping priority (`urgent→sev1`, `high→sev2`, `medium→sev3`, `low/none→sev4`); `service_ids` divalidasi se-project; peseta awal: creator (`commander`) + assignee incident (`responder`, skip duplikat creator). Event `room.created` + `participant.joined` dicatat.

PATCH status memakai transition map: `active ↔ monitoring`, `active|monitoring → resolved`, `resolved → active` (reopen), `active|monitoring|resolved → archived`. `resolved_at` diisi/dikosongkan otomatis; event sesuai. Semua write PATCH tercatat event + publish Redis `room.changed`.

**Sub-resource**

| Method | Path                                 | Body                                          | Permission                                      |
| ------ | ------------------------------------ | --------------------------------------------- | ----------------------------------------------- | ------ |
| POST   | `/:pk/services/`                     | `{service_ids: []}` (idempotent)              | commander                                       |
| DELETE | `/:pk/services/:service_id/`         | —                                             | commander                                       |
| POST   | `/:pk/issues/`                       | `{issue_ids: []}` (idempotent, bukan primary) | commander                                       |
| DELETE | `/:pk/issues/:issue_id/`             | —                                             | commander                                       |
| POST   | `/:pk/participants/`                 | `{member_id, role?}`                          | commander (member: self join, role `responder`) |
| PATCH  | `/:pk/participants/:participant_id/` | `{role}`                                      | commander                                       |
| DELETE | `/:pk/participants/:participant_id/` | —                                             | commander atau self (leave)                     |
| POST   | `/:pk/runbook-items/`                | `{title}`                                     | commander                                       |
| PATCH  | `/:pk/runbook-items/:item_id/`       | `{title?, is_done?}`                          | member (is_done), commander (title)             |
| DELETE | `/:pk/runbook-items/:item_id/`       | —                                             | commander                                       |
| POST   | `/:pk/messages/`                     | `{body, client_id?}`                          | member (room belum archived)                    |
| PATCH  | `/:pk/messages/:message_id/`         | `{body}`                                      | author                                          |
| DELETE | `/:pk/messages/:message_id/`         | —                                             | author atau admin                               |
| GET    | `/:pk/messages/`                     | `?before=<created_at                          | id>&limit=50`                                   | member |
| GET    | `/:pk/events/`                       | `?before_id=&limit=50`                        | member                                          |

Guard participant: commander terakhir tidak boleh di-demote/di-remove/leave (400 `last_commander`); self join dua kali idempotent. Archived room bersifat terminal: PATCH status dari `archived` tidak diizinkan, dan chat/link/runbook/participant write ditolak 409 `room_archived`. `POST /:pk/issues/` menolak `primary_issue_id` dengan 400 `primary_issue_not_linkable` (incident utama tidak boleh muncul dua kali sebagai link biasa).

`mentions jsonb`: server mem-parse token `@{user_id}` di `body`, memvalidasi user adalah member workspace, mengisi `mentions`, dan membuat row `notifications` untuk tiap mentioned (kecuali author).

### 3. Realtime

**Channel Redis**: `war-room:events`, payload `{room_id, kind, data}`.

- api-rs publish setelah commit untuk: `message.created` (payload message lengkap + `client_id`), `message.updated`, `message.deleted`, `room.changed` (`{reasons: ["status","severity","notes","links","participants","runbook",...]}`), `activity.created` (row event).
- live server publish: `typing` (`{user_id, is_typing}`), `presence.joined`, `presence.left`.

**Live server** (`apps/live`):

- Controller baru `war-room.controller.ts`, `@Controller("/war-rooms")`, `@WSDecorator("/:roomId")`, didaftarkan di `CONTROLLERS` (`apps/live/src/controllers/index.ts`).
- Auth di connect: token JSON.stringify(user) + cookie handshake, validasi `handleAuthentication` (`lib/auth.ts`); lalu validasi akses member via REST call `GET /api/workspaces/:slug/projects/:project_id/war-rooms/:pk/` memakai cookie (parse `workspaceSlug`/`projectId` dari query param). Gagal → close 4403. Pola token sama dengan editor (`packages/editor` → `JSON.stringify(user)`).
- Registry in-memory `Map<roomId, Set<WebSocket>>`; subscribe Redis channel sekali saat boot (pola `extensions/redis.ts: onConfigure`); setiap pesan Redis di-fan-out ke socket room terkait (termasuk pengirim — klien idempotent/dedupe).
- Pesan masuk dari klien (`typing`) → publish ke Redis (bukan persist).
- Heartbeat ping 30 detik dari klien; socket mati → broadcast `presence.left`.

**Klien** (`apps/web/core/hooks/use-war-room-socket.ts`): buka `${LIVE_BASE_URL}${LIVE_BASE_PATH}/war-rooms/{roomId}?token=&workspaceSlug=&projectId=`, reconnect exponential backoff, state `connecting|connected|reconnecting`. Event handler store:

- `message.created` → append bila belum ada (dedupe `id` + reconcile optimistic by `client_id`), tandai unread bila user scroll ke atas.
- `message.updated/deleted` → replace/tombstone.
- `room.changed` → refetch detail room (murah).
- `activity.created` → prepend feed.
- `presence.*` → map online user id; `typing` → indicator dengan timeout 3 detik.

Urutan ketahanan: REST create message mengembalikan row final; echo WS di-dedupe. Multi-instance aman untuk pesan via Redis; presence in-memory per instance (live server saat ini satu instance — dicatat sebagai batasan).

### 4. Frontend (web)

**Routes** (`apps/web/app/routes/core.ts`, blok di dekat services `:179-193`):

- `:workspaceSlug/projects/:projectId/war-rooms` → list
- `:workspaceSlug/projects/:projectId/war-rooms/:warRoomId` → room

Halaman di `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list|detail)/...` (pola services).

**Types** (`packages/types/src/war-room/` + barrel): `TWarRoomStatus`, `TWarRoomSeverity`, `TWarRoomParticipantRole`, `IWarRoomListItem`, `IWarRoom`, `IWarRoomMessage`, `IWarRoomParticipant`, `IWarRoomRunbookItem`, `IWarRoomEvent`, payload create/update, `TWarRoomSocketEvent`.

**Constants** (`packages/constants`): link helper `getWarRoomLink(workspaceSlug, projectId, warRoomId)`, config warna/label severity-status-role, `WAR_ROOM_TEMPLATE` tidak di FE (server-side).

**Service + store**: `apps/web/core/services/war-room.service.ts` (extends `APIService`), `apps/web/core/store/war-room.store.ts` (MobX, pola `service.store.ts`) + registrasi di `root.store.ts`, hook `use-war-room`.

**Halaman list** (`apps/web/core/components/war-rooms/list/`):

- Header: judul, stat chips (dari `/summary/`: Active, SEV1–2, Resolved 7d), tombol **Create war room**.
- Tab status: Active (default; `status=active,monitoring`) / Resolved / All (arched tampil badge "Archived"); search `q` (debounce); skeleton rows; empty state CTA; filtered-empty + Clear.
- Baris ops-board (gaya services health board): rail warna severity, `WR-n`, nama + incident chip (`identifier` link ke peek work item), badge severity, pill status, chips services (maks 3 +N), avatar participants (maks 3 +N), durasi (ticking untuk active/monitoring), jumlah pesan. Klik baris → room.

**Modal create** (`war-rooms/create-modal.tsx`):

- Incident picker: `ExistingIssuesListModal` dengan mode single-select baru (prop opsional `selectionMode="single"`, backward-compatible; pilih = submit langsung) + `searchParams={{workspace_search: false}}`.
- Saat incident dipilih: autofill name (bila belum disentuh), severity default dari priority, services default dari link `service-issues` incident (client filter dari list project), assignee info.
- Field: name (wajib), severity select, services multi-select (reuse `services/select/service-multi-select.tsx`), deskripsi rich text (pola service form).
- Submit sukses → buka room. 409 → banner inline "Active war room sudah ada untuk incident ini" + tombol **Open WR-n**.
- Entry point tambahan: tombol **Open war room** di header quick actions work item tipe Incident (`apps/web/core/components/issues/issue-detail/issue-detail-quick-actions.tsx`) — membuat room dengan incident prefill, atau membuka room aktif bila ada (query `?primary_issue_id=`).

**Room page** (`war-rooms/room/`, layout mockup v2):

- **Header**: back, `WR-n` + nama (inline edit commander), severity picker, status control (dropdown transition), timer `HH:MM:SS` (berhenti di `resolved_at`), avatar online + `+N`, tombol **Join** (non-participant), **Resolve** (modal konfirmasi + catatan resolve → append `notes_html`), menu `⋯` (Edit details, Archive, Delete admin).
- **Peta blast radius** (`service-map.tsx`): kalkulasi di client — services terdampak + tetangga 1 hop dua arah dari `getGraphData(projectId)`; reuse canvas read-only hasil refactor (lihat bawah); node diklik membuka detail service di tab baru; tombol collapse; empty state "No affected services yet" + Add services.
- **Chat** (`chat-panel.tsx`, `message-item.tsx`, `chat-composer.tsx`): daftar pesan grouped by author/time, mention chip, item menu (edit/hapus sesuai izin), pagination "Load older" saat scroll atas, indicator `X typing…` + dot online, composer `@` autocomplete member project, Enter kirim, Shift+Enter baris baru, optimistic send + banner reconnect saat WS putus.
- **Panel konteks** (`context-panel.tsx`, tab; tab terakhir diingat per user via `localStorage`):
  - **Notes** (default): editor rich-text (`@plane/editor` non-collab), autosave debounce 1 detik ke `notes_html`, label "Edited Xm ago"; read-only bila archived atau non-commander? — edit untuk commander/admin, member lain read-only.
  - **Work items**: incident utama pinned (badge "Primary incident") + link items; tambah via `ExistingIssuesListModal` single/multi; unlink; klik → peek work item.
  - **Runbook**: progress bar `x/y`, checkbox toggle (member), tambah item inline (commander), edit/hapus via hover menu (commander), auto-scroll item selesai.
  - **Activity**: feed event kronologis (aktor + waktu + deskripsi), live append dari `activity.created`.
  - **People**: daftar peserta + role dropdown (commander), Join/Leave, indikator online, role labels (Incident Commander, Comms, Scribe, Responder).
- **Refactor graph**: ekstrak canvas presentational `apps/web/core/components/services/graph/service-graph-canvas.tsx` (props: nodes, edges, health, `readOnly`, `onNodeClick`); `service-graph.tsx` (editor halaman Services) memakai canvas + logika edit existing; war room memakai canvas read-only.

**Sidebar**: item **War rooms** di `apps/web/core/components/workspace/sidebar/project-navigation.tsx` (pola services `:113-120`) dan `apps/web/core/components/navigation/use-navigation-items.ts` (`:75-78`); badge jumlah active dari summary store bila sudah difetch (tidak ada fetch khusus sidebar).

**Notifikasi mention** (fase akhir): row `notifications` (`entity_name = "war_room"`, `entity_identifier = room id`, `sender = "war_room_mentioned"` agar masuk filter mentioned, `data.war_room = {id, project_id, workspace_slug, name, sequence_id}`); `NotificationItem` (`apps/web/core/components/workspace-notifications/sidebar/notification-card/item.tsx`) menambah cabang buka room.

**i18n**: semua string UI baru masuk `packages/i18n` (en + locale lain sesuai skill translate); template runbook disimpan server-side EN.

### 5. Testing

- **Rust** (`apps/api-rs/crates/api/tests/war_room_test.rs`, scratch workspace seri `--test-threads=1` bila memakai purge): create validasi (incident se-project, 409 duplicate active, default severity/name, sequence increment, seed runbook per tipe, participants awal), transition status (resolve/reopen/archive + resolved_at), permission matrix (member vs commander vs admin), participant last-commander guard, messages (pagination, edit/delete izin, parse mention + notification row), links idempotent, list filter/summary, publish Redis diuji dengan subscriber `redis` di test stack (bila tidak tersedia, assert payload builder + smoke manual).
- **Web vitest**: store (apply event dedupe/reconcile, unread), blast-radius helper, mention tokenizer, timer format, single-select modal behavior, create-form default mapping.
- **Live**: typecheck/lint + smoke manual dua browser (chat realtime, typing/presence, reconnect).
- **E2E smoke** (per `AGENTS.md`): migrate via boot api-rs → `docker compose -f docker-compose-local.yml up -d --build api worker beat-worker` (detached, log ke file) → `curl /health` → `pnpm --filter=live build && systemctl --user restart plane-live.service` → `curl /live/health/` → `pnpm --filter=web build && systemctl --user restart plane-web-prod.service` → buka room, kirim pesan dari dua sesi, resolve, cek notifikasi mention.

### 6. Rollout

1. Rebuild api-rs (migrasi 0011 jalan saat boot).
2. Build + restart live server (controller baru).
3. Build + restart web prod.
4. Tambah `docs/features/war-rooms.md` (status Approved) + tandai ide war room di `_backlog.md`/`work-items.md` sudah ada kodenya.

### 7. Urutan implementasi (untuk plan)

Feature ini besar; plan akan dipecah menjadi fase berurutan yang masing-masing bisa diverifikasi sendiri:

1. **Fondasi backend**: migrasi + route room/links/participants/runbook + RBAC + tests.
2. **Chat + realtime**: messages + mentions + publish Redis; controller live + tests/manual smoke.
3. **Web list + create**: types/constants/store/service, halaman list, modal create, single-select picker, sidebar, entry point work item.
4. **Room page**: header/lifecycle, graph refactor + peta blast radius, chat panel + socket hook, tab Notes/Work items/Runbook/Activity/People.
5. **Notifikasi mention + docs**: cabang notification card, `docs/features/war-rooms.md`, update backlog.

## Edge cases

- **Incident dihapus/archived**: room tetap ada (FK CASCADE hanya untuk hard delete; soft delete issue tidak menyentuh room). UI incident chip menampilkan state archived.
- **Commander terakhir**: demote/remove/leave ditolak 400 `last_commander`; reassign harus menunjuk commander baru dulu (PATCH participant lama → role lain setelah participant baru jadi commander? aturan: set commander baru dalam transaksi yang sama, lalu demote lama).
- **Services terdampak dihapus**: link di-soft-delete; peta otomatis kehilangan node (fallback empty state).
- **Dua create bersamaan untuk incident sama**: unique index partial menolak; handler menangkap unique violation → 409.
- **Message dikirim saat archived**: 409 `room_archived`.
- **WS putus saat kirim**: REST tetap sukses; UI menandai pesan terkirim (bukan failed) dan banner reconnect.
- **Presence multi-instance**: dicatat sebagai batasan; pesan/event tetap realtime.
- **Rate limit**: pesan chat memakai rate limit middleware existing; flood typing di-throttle client (max 1 event/2 detik).
- **Room tanpa services**: peta empty state; create tetap valid.
- **Name room > 255**: validasi server 400; UI maxLength.

## Peta modul & file

- **Migrasi/API**: `apps/api-rs/migrations/0011_war_rooms.sql`, `apps/api-rs/crates/api/src/routes/war_room.rs` (+ daftar di `routes/mod.rs`/`main.rs`), helper RBAC lokal, publish Redis memakai `AppState::redis_client()`, `apps/api-rs/crates/api/tests/war_room_test.rs`.
- **Live**: `apps/live/src/controllers/war-room.controller.ts` + registrasi `apps/live/src/controllers/index.ts`; subscriber Redis (pola `extensions/redis.ts`).
- **Web**: `apps/web/app/routes/core.ts`, halaman di `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/**`, komponen `apps/web/core/components/war-rooms/**`, refactor `apps/web/core/components/services/graph/service-graph-canvas.tsx`, store `apps/web/core/store/war-room.store.ts` (+ `root.store.ts`), service `apps/web/core/services/war-room.service.ts`, hook `apps/web/core/hooks/use-war-room-socket.ts`, entry work item `issues/issue-detail/issue-detail-quick-actions.tsx`, notifikasi `workspace-notifications/sidebar/notification-card/item.tsx`, sidebar `workspace/sidebar/project-navigation.tsx` + `navigation/use-navigation-items.ts`.
- **Shared**: `packages/types/src/war-room/**`, `packages/constants/src/war-room.ts` (+ barrel), `packages/i18n/src/locales/**`.

## Risiko

- **Live server menyentuh jalur baru** (WS non-Hocuspocus): auth & fan-out harus diuji smoke dua browser; risiko utama regresi ada batas (controller baru, tidak mengubah collaboration).
- **Refactor graph** menyentuh halaman Services: canvas diekstrak tanpa mengubah perilaku editor; jaga test/smoke halaman Services.
- **Beban query list** (services/participants/message_count per row): pakai subquery agregat; pagination default 50 bila perlu — diukur saat implementasi.
- **Chat tanpa pemangkasan riwayat**: index `(room_id, created_at DESC)`; retensi dicatat sebagai follow-up.
- **Kontrak notifikasi mention** menumpang tabel existing; pastikan filter mentioned (`sender ILIKE '%mentioned%'`) dan kartu notifikasi tidak menyembunyikan row (guard `issue_activity`).

---

## Changelog

| Date       | Change                                                                                                                                    |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-09-30 | init — hasil brainstorming: project-level entity, Approach 1 realtime, layout Map First, 7 panel, lifecycle/izin/runbook/notes, WR-number |
