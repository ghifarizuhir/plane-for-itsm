# War Room sebagai Property Work Item — Design

Date: 2026-10-09
Status: Draft (pending user review)
Scope: api-rs (kolom turunan + filter + 1 index migration), web (property toggle, kolom list/spreadsheet, filter, nav, hapus list page), i18n, docs. Room detail + chat realtime + runbook tetap.
Terkait: `docs/features/war-rooms.md`, `docs/superpowers/specs/2026-09-30-war-room-design.md`, plans `2026-09-30-war-room-phase-*`, `2026-10-01-war-room-phase-4-room-page.md`.

## Tujuan

1. War room tidak lagi punya halaman list terpisah ("Incident bridge" list dihapus).
2. Work item bertipe **Incident** punya property **"War room"** on/off.
3. Toggle **ON** → room langsung dibuat otomatis (atau di-reopen bila terakhir resolved). Toggle **OFF** → room di-resolve.
4. Daftar bridge dilihat lewat **filter di Work Items** (`War room = On/Off`); nav "Incident bridge" jadi shortcut ke filter tersebut.
5. Room detail (chat, runbook, blast radius, participants, activity) tidak berubah.

## Keputusan (brainstormed & approved 2026-10-09)

| #   | Keputusan                                                                                                                                                                            |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | **Keep room, remove list** — halaman list war-rooms dihapus; route detail `/war-rooms/:warRoomId` tetap.                                                                             |
| 2   | ~~Nav "Incident bridge" tetap ada sebagai shortcut~~ → dibatalkan 2026-10-10: item nav dihapus total dari sidebar + tab header; daftar bridge hanya via filter manual di Work Items. |
| 3   | Toggle ON = auto-create room tanpa modal; OFF = resolve (bisa reopen).                                                                                                               |
| 4   | Property hanya untuk work item tipe **Incident**.                                                                                                                                    |
| 5   | Pendekatan data: **turunan langsung dari tabel `war_rooms`** (single source of truth, tanpa perubahan schema; hanya 1 index migration).                                              |
| 6   | Bukan N+1: pola correlated subquery sudah dipakai `LIST_SELECT_SQL` (`cycle_id`, counts). Kebutuhan riil = index per lookup (lihat §Data & API).                                     |

## Data & API (api-rs)

### Kolom turunan di response issue (tanpa perubahan tabel)

Field flat baru di response list/detail (mengikuti pola `cycle_id`):

- `war_room_id: Option<Uuid>`
- `war_room_status: Option<String>` — `active | monitoring | resolved | archived`
- `war_room_severity: Option<String>` — `sev1..sev4`

Semantik subquery (satu room deterministik per issue):
`primary_issue_id = i.id AND deleted_at IS NULL`, urutkan `(status IN ('active','monitoring')) DESC, created_at DESC`, `LIMIT 1` — room aktif/monitoring selalu menang; selain itu room terbaru (termasuk `archived`, untuk memutuskan reopen vs create baru).

Lokasi perubahan:

- `apps/api-rs/crates/api/src/routes/issue_query.rs` — `LIST_SELECT_SQL:70`, `DETAIL_SELECT_SQL`, jalur `list_by_ids`.
- `LIST_SCAN_SELECT_SQL:76` **tidak berubah**: war room bukan groupable key, dan phase-2 grouped fetch memakai `LIST_SELECT_SQL` untuk page ids.
- `apps/api-rs/crates/api/src/routes/issue_common.rs` — 3 field baru di `IssueListRow` (append setelah `type_id`; urutan key JSON ternilai oleh test parity).

### Filter rich-filters

- Field baru `war_room__in` di `COMPLEX_FILTER_ALLOWLIST` (`issue_query.rs:1106-1159`), nilai `on | off`.
  - `on` → `EXISTS (SELECT 1 FROM war_rooms r WHERE r.primary_issue_id = i.id AND r.deleted_at IS NULL AND r.status IN ('active','monitoring'))` — ter-cover partial unique index `war_rooms_one_active_per_issue_idx`.
  - `off` → `NOT EXISTS (...)` — termasuk issue yang belum pernah punya room.
- UI filter: "War room: On / Off". Severity/status hanya tampil di chip kolom, bukan filter (non-goal).

### Index migration (0017)

`apps/api-rs/migrations/0017_war_rooms_primary_issue_idx.sql`:

```sql
CREATE INDEX IF NOT EXISTS war_rooms_primary_issue_latest_idx
    ON public.war_rooms (primary_issue_id, created_at DESC)
    WHERE deleted_at IS NULL;
```

Alasan: lookup kolom turunan per baris `WHERE primary_issue_id = ? AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1` belum punya index ber-prefix `primary_issue_id` (index existing ber-prefix `project_id`). Migrasi dijalankan otomatis saat boot (`common::db::migrate`). Tidak ada perubahan kolom/tabel.

### Aksi toggle (orchestrasi FE, endpoint existing — tanpa API baru)

| Transisi | Kondisi room terakhir   | Aksi                                                                                                                                                                                 |
| -------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| OFF → ON | `resolved`              | `PATCH /war-rooms/:id/ {status: "active"}` (reopen)                                                                                                                                  |
| OFF → ON | tidak ada / `archived`  | `POST /war-rooms/` — nama = judul work item, severity dari priority (helper `severityFromPriority`), `service_ids` auto dari service yang ter-link ke incident (perilaku modal lama) |
| ON → OFF | `active` / `monitoring` | `PATCH /war-rooms/:id/ {status: "resolved"}`                                                                                                                                         |

- Optimistic UI + rollback saat gagal (pola store existing).
- 409 `active_war_room_exists` (dua user ON bersamaan) → diperlakukan sukses, refetch.
- Permission tidak berubah: project member bisa toggle; guest read-only (toggle disabled).

## UI (web)

1. **Detail work item** — row "War room" di sidebar Properties, hanya dirender untuk tipe Incident:
   - ON: toggle + chip `SEV{n} · {status}` + link `Open room — WR-n`.
   - OFF: toggle polos.
   - Komponen yang sama dipakai di peek overview.
2. **List layout** — display-property baru `war_room` (default off) → kolom "War room" berisi toggle yang bisa diklik langsung.
3. **Spreadsheet layout** — kolom yang sama.
4. **Kanban** — chip SEV read-only di card saat ON.
5. **Filter** — rich filter "War room" (`on`/`off`) di panel filter; chip filter tampil di toolbar.
6. **Nav** — ~~item "Incident bridge" sebagai shortcut preset~~ → dibatalkan 2026-10-10: item dihapus dari kedua lokasi nav. Mekanisme preset `?war_room=on` di `ProjectLayoutRoot` tetap ada (dipakai redirect route `/war-rooms` lama + deep link manual).

### Yang dihapus / dipertahankan

Dihapus:

- Page list: `apps/web/app/(all)/[workspaceSlug]/(projects)/projects/(detail)/[projectId]/war-rooms/(list)/**`.
- Komponen list: `war-rooms-board.tsx`, `war-rooms-board-row.tsx`, `war-room-summary-chips.tsx`, `war-room-search-input.tsx`, `war-room-load-error-state.tsx`, `war-room-view-header.tsx`.
- `create-war-room-modal.tsx` (create kini otomatis) dan `war-room_filter.store.ts`.
- Field list-only di `war-room.store.ts` (tab status, search, board fetch) — sisakan logic detail.
- Tombol lama `issue-war-room-button.tsx` (redirect ke list + `?primary_issue_id=`) — diganti property row.
- Route tuple list di `apps/web/app/routes/core.ts:196-207`.

Dipertahankan / diubah:

- Route detail `/war-rooms/:warRoomId` + seluruh komponen `room/**` + realtime (`use-war-room-socket`, `apps/live`).
- `getWarRoomLink()` di `packages/constants/src/war-room.ts`.
- Notifikasi mention war room (`workspace-notifications/war-room-detail.tsx`, `war-room-notification.ts`).
- Route `/war-rooms` (list lama) → **redirect** ke `/issues` dengan preset `war_room = on`.
- Modal edit details di dalam room tetap.

## Edge cases

- Room `archived` + toggle ON → buat room baru (bukan reopen; archived tidak bisa dibuka).
- Room `monitoring` → toggle ON; OFF → resolve.
- Resolve dari dalam room → toggle ikut OFF setelah data refresh (tanpa socket baru di work item list; refetch/optimistic cukup).
- Guest → toggle disabled; filter & chip tetap bisa dilihat.
- Issue archived/deleted → room tetap (perilaku lama; CASCADE hanya saat hard delete).
- Non-incident type → property tidak dirender; filter `war_room` tetap valid (hasil `off`).
- Satu active room per issue tetap dijamin partial unique index; tidak ada perubahan constraint.

## Testing

- api-rs:
  - Parity/unit: 3 key baru di list + detail + `list_by_ids`; key order `IssueListRow`.
  - Filter: `war_room__in: on/off` (termasuk issue tanpa room).
  - Extend `apps/api-rs/crates/api/tests/war_room_test.rs`: reopen via PATCH, create default (nama/severity/service auto).
  - Index migration: boot DB bersih + boot ulang (idempotent `IF NOT EXISTS`).
- Web:
  - `war-room.store.test.ts`: toggle optimistic ON/OFF + rollback 409.
  - Render test property row hanya untuk tipe Incident.
- Jalankan `pnpm check:types` + `pnpm check:lint`.

## Docs & i18n

- Update `docs/features/war-rooms.md`: route, actions, filters, current state (hapus list + create modal, tambah property toggle & nav shortcut).
- Update `docs/superpowers/plans/2026-09-30-war-room-phase-3-web-list-create.md` ditandai superseded (historis).
- i18n: keys baru untuk label property/kolom/filter/toggle; prune keys list yang tak terpakai di `packages/i18n/src/locales/*/war-room.json`.

## Non-goals

- Group-by "War room", filter severity, summary chips, board view baru.
- Socket/realtime baru di work item list.
- Perubahan tabel/kolom `war_rooms` atau `issues` (hanya 1 index migration).
- Perubahan pada `apps/live` (chat, presence, dll).

## Risiko & catatan

- **Preset filter menimpa filter user** saat klik nav — konsisten dengan saved view; user bisa ubah/hapus chip setelahnya.
- **Server-side list tetap benar saat pagination**: filter `on/off` dievaluasi di SQL, bukan di client.
- **Rollback**: revert kode + drop index (opsional; index tidak berbahaya bila dibiarkan). Tidak ada migrasi data.

## Verifikasi (ops)

1. Rebuild api-rs: `setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker` (detached + poll log per `AGENTS.md`).
2. `curl http://localhost:8000/health` → 200; restart `plane-live.service`.
3. Build web + restart: `pnpm --filter=web build && systemctl --user restart plane-web-prod.service`.
4. Manual: toggle ON di detail Incident → room terbuat + link WR-n; resolve dari room → toggle OFF; toggle OFF dari list → room resolve; filter `War room: On/Off` benar (termasuk pagination); nav "Incident bridge" → `/issues` dengan chip filter; URL lama `/war-rooms` redirect; mention notifikasi masih membuka room.

## Definisi selesai

- Tidak ada halaman list war-rooms; seluruh akses bridge lewat Work Items (filter/nav) atau room detail.
- Property "War room" hanya muncul di Incident, bisa di-toggle dari detail + list, dan sinkron dua arah dengan lifecycle room.
- Filter `war_room` server-side works untuk `on` dan `off` di semua layout & pagination.
- Index migration terpasang; tidak ada regresi test api-rs/web.
