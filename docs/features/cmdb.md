# CMDB (Service Map / Configuration Items) — PROPOSE

Status: **Draft** (proposal — belum ada kode di fork ini)
Route (proposed): `:workspaceSlug/configuration-items` (list) + `:workspaceSlug/configuration-items/:ciId` (detail)
Share: CORE

> Ini doc proposal pertama ITSM (pengecualian aturan "jangan buat `features/<itsm-page>.md`" di `_backlog.md`). Ditulis dari implementasi actual Terra, bukan dari klaim docs Terra — lihat §Koreksi vs docs Terra.

## Intent

Registry operasional infrastruktur workspace: apa yang jalan (server, service, db, queue, cache, lb), depend ke apa, dan project mana yang pakai. Saat incident muncul, responder trace "CI X down → project Y terimpact" dalam 1-2 klik. Beda dari Asset (ownership/warranty) — CI = representasi _operational_ (runtime + dependencies + impact).

Target user: SRE/Ops (register + maintain graph), team lead (tau project jalan di CI mana), incident responder (blast-radius trace).

Referensi: `terra/docs/features/services.md:1` (spec), implementasi actual `terra/apps/web/src/features/services/` (21 file) + `terra/apps/api/src/db/schema.pg.ts:527-583`.

## Current State (snapshot kode fork — belum ada)

- Tidak ada route `configuration-items` di `apps/web/app/routes/core.ts`; tidak ada model CI di `apps/api/plane/db/models/`; tidak ada store/listing CI. (Audit 2026-09-03.)
- Pola yang bisa direuse 1:1 dari fork: filter sidebar + list/table (`_shared/list.md`), detail standalone (`_shared/detail-page.md`), peek pattern, permissions workspace/project, `archived_at` soft-delete.
- Yang harus baru: 3 model Django + endpoints + store + (Phase 2) graph. `@xyflow/react` belum ada di fork — tambah sebagai dep baru saat Phase 2.

## Koreksi vs docs Terra (penting — jangan contek klaim mentah)

Audit implementasi actual Terra menemukan docs-nya stale di beberapa titik. Proposal ini ikut **actual**, bukan klaim:

| Klaim `terra/docs/features/services.md`                  | Actual Terra (terverifikasi)                                                                                                                                              |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Route `/services` + `/services/:id` + `/services/editor` | Hanya **1 route**: `services` (`terra/apps/web/src/Router.tsx:89` → `ServiceMapEditorPage`); detail via **`?ci=` query param**, bukan `:id`; `/services/editor` tidak ada |
| ID format `CI-YYYYMM-NNN` (`services.md:109`)            | Actual `CI-001` (`ci.service.ts:220` via `ids.nextLongLived`; format tanggal hanya seed hardcoded `seed-demo/cmdb.ts:17`)                                                 |
| Prefix API `/services/items/...`                         | Actual mount `/api` tanpa prefix services: `GET/POST /cis`, `GET /cis/dependencies`, `POST /cis/:id/dependencies` (`terra/apps/api/src/routes/cis.ts:43-164`)             |
| Kind 8 macam (server/database/api/queue/...)             | Contract **hanya 2**: `server \| service` (`packages/contracts/src/cmdb.ts:3-5`); `environment`/`role` string bebas                                                       |
| Review queue sebagai fitur utama                         | Auto-enqueue saat create (`ci.service.ts:235`); queue panel hanya bila 1 app terpilih                                                                                     |

## Data Model (proposed — Django, adaptasi Terra)

Terra (Drizzle, `schema.pg.ts:527-583`) → Plane (Django):

- `ConfigurationItem` ← `configuration_items:527-545`: `key` (workspace-scoped sequence, format `CI-001` — ikut actual Terra, bukan format tanggal), `workspace` FK, `name`, `ci_kind` (`server | service`, ikut actual), `environment` (string bebas, default `production`), `hostname` (nullable, khusus server), `description_*` (reuse pola Page: json/html/stripped), `metadata` JSON (`services[]`, `specs{}`), `status` (default `active`), `archived_at` (reuse pola fork, bukan `deleted_at` Terra).
- `CIDependency` ← `ci_dependencies:547-565`: `source` + `target` FK CI (CASCADE), `dependency_type` (6: `runs_on / connects_to / replicates_to / depends_on / uses / other`), unique `(source, target, type)` + CHECK `source <> target`. Tanpa soft-delete (junction — remove = hilang permanen, ikut Terra).
- `ProjectCILink` ← `application_ci_links:567-583` (Terra link ke app; fork link ke **project**): PK `(project, ci)`, `role` (string bebas: `primary / replica / cache / worker`).
- Open: review CI (`ciReviews:346-353`) — defer ke Phase 2 (fork belum punya review infra; Terra punya `review-queue.ts` + `routes/reviews.ts:37-55`).

## Primary View (proposed)

Ikut Terra actual (bukan klaim list/detail terpisah — Terra actual = **satu canvas editor** + detail panel/modal):

- Phase 1 (list-first, disederhanakan dari Terra): split view — filter sidebar kiri 220px (Kind / Environment / Status + toggle Has project link) + table kanan (kolom ID monospace `CI-001`, Name, Kind badge, Environment chip, Hostname monospace, Projects chips `+N`, Status badge, `⋯` hover). Default sort `name:asc`. Search scope id/name/hostname (`?q=`). Pagination 50 + URL persist (`?page=&pageSize=`).
- Detail: **standalone page** `:ciId` (full page, bukan peek — rationale Terra `services.md:319-328`: konten teknis multi-section sempit di 580px) — header + badges, description markdown, metadata grid, Services & Ports (dari `metadata.services`), Specs (dari `metadata.specs`), Dependencies outbound/inbound + Add modal (typeahead + type dropdown, guard self + 409 duplicate), Linked Projects + link modal (role dropdown).
- Phase 2 (graph): `@xyflow/react` canvas — node per CI (warna by kind: server blue, service purple), edge dari `GET dependencies` (6 warna by type, animated flow, status-aware dim), hierarchical auto-layout cycle-safe, staged save/discard, zoom/pan. Ikut `EditorGraphCanvas.tsx:131` + `PathfindingEdge.tsx:38` + `useGraphLayout.ts:8`.

## Actions (proposed)

| Action                | Trigger                             | Permission     | State required                                             |
| --------------------- | ----------------------------------- | -------------- | ---------------------------------------------------------- |
| Create CI             | Header / `C`                        | Member+        | — (default `active`)                                       |
| Edit metadata         | Detail inline                       | Member+        | Not archived                                               |
| Change status         | Detail picker / row `⋯`             | Member+        | `active ↔ maintenance` bebas; → `retired` perlu confirm    |
| Retire                | Status → retired                    | Member+        | Confirm modal                                              |
| Delete (soft)         | `⋯` menu                            | Admin          | Prefer retire; delete untuk typo/duplicate (`archived_at`) |
| Add/remove dependency | Detail section / graph connect mode | Member+        | Target ≠ source; no duplicate triple                       |
| Link/unlink project   | Apps section + role                 | Project admin+ | —                                                          |
| Copy link             | Row / detail `⋯`                    | Member+        | —                                                          |

Lifecycle: `active` (default) ↔ `maintenance` → `retired` (terminal, re-activate admin-only untuk correction). Status change = plain field edit (tanpa history table Phase 1 — ikut Terra).

## Filters / Sort / Search (proposed)

- Sidebar always-visible (bukan popover — faceted drill-down = primary flow, ikut Terra): Kind (dengan count per kind dari facets endpoint), Environment, Status (default exclude retired), Has project link toggle, Project scope (dari project selector bila ada).
- URL: `?kind=&environment=&status=&has_project=1&project=&q=&sort=name:asc&page=&pageSize=`.
- Chips aktif di atas table (two-way dengan sidebar).

## Detail View (proposed)

- Standalone `:ciId` (lihat §Primary View). Back → list dengan filter + scroll preserved.
- Recent activity (Phase 1 minimal): header + "No direct links yet" — cross-ref incident→CI via heuristic project link, defer presisi ke Phase 2 (`affected_ci_ids`).
- Ref: [`_shared/detail-page.md`](./_shared/detail-page.md).

## Permissions (proposed — ikut Plane, bukan Terra)

Terra: semua authenticated user CRUD tanpa role gate. Fork ikut hierarchy Plane:

| Role            | Read | Create | Update | Delete | Manage deps | Link project |
| --------------- | ---- | ------ | ------ | ------ | ----------- | ------------ |
| Member          | ✅   | ✅     | own    | ❌     | ✅          | ❌           |
| Project admin   | ✅   | ✅     | ✅     | ✅     | ✅          | ✅           |
| Workspace admin | ✅   | ✅     | ✅     | ✅     | ✅          | ✅           |

CI workspace-level: visible semua member workspace (data dibatasi project yang bisa diakses untuk link section).

## API Touchpoints (proposed — DRF, adaptasi Terra actual)

| FE (MobX/SWR)          | Endpoint (DRF)                                                | Terra actual                           |
| ---------------------- | ------------------------------------------------------------- | -------------------------------------- |
| `useCiList(filters)`   | `GET /api/workspaces/:slug/configuration-items?<filters>`     | `GET /cis` (`cis.ts:48`)               |
| `useCi(id)`            | `GET .../configuration-items/:key` (embed deps + projects)    | `GET /cis/:id` (`:79`)                 |
| `useCreateCi()`        | `POST .../configuration-items`                                | `POST /cis` (`:89`)                    |
| `useUpdateCi(id)`      | `PATCH .../configuration-items/:key`                          | `PATCH /cis/:id` (`:100`)              |
| `useDeleteCi(id)`      | `POST .../configuration-items/:key/archive/` (ikut pola fork) | `DELETE /cis/:id` soft (`:123`)        |
| `useAddDependency`     | `POST .../:key/dependencies`                                  | `POST /cis/:id/dependencies` (`:111`)  |
| `useRemoveDependency`  | `DELETE .../:key/dependencies/:depId`                         | `DELETE .../:depId` (`:133`)           |
| `useLinkProject`       | `POST .../:key/projects` (+ role)                             | `POST /apps/:appId/cis/:ciId` (`:143`) |
| `useCiDependencies()`  | `GET .../configuration-items/dependencies` (flat, graph)      | `GET /cis/dependencies` (`:69`)        |
| `useCiFacets(filters)` | `GET .../configuration-items/facets`                          | Phase 2 ideal Terra (Phase 1 derive)   |

Cache: detail embed deps+projects (single fetch); mutasi sub-resource invalidate parent key di kedua ujung (bilateral — ikut Terra).

## Empty / Loading / Error (proposed)

- Empty first-run: Network icon + "Register your infrastructure CIs..." + CTA (member read-only message); filter-0: "No CIs match [Clear filters]".
- Loading: skeleton table 8 rows + sidebar skeleton; filter applied: progress bar top + dim rows (bukan full skeleton).
- Error: banner + Retry; 404 detail: "CI not found." + Back; 409 duplicate dep: toast; self-link: inline modal error.

## Phase 2 Deferred

- Graph canvas `@xyflow/react` (nodes/edges/staged save — §Primary View).
- Impact analysis endpoint (`:key/impact?depth=`) — recursive inbound + linked projects (sudah di `_backlog.md`).
- Auto-discovery (Dynatrace/Prometheus → auto-register).
- `CIStateHistory` table (audit compliance).
- CI notes/comments terpisah; cross-link incident↔CI presisi (`affected_ci_ids`); global search unifikasi; cycle-detect warn; bulk ops; export CSV/Terraform; metadata schema validation per-kind.
- Review queue (butuh review infra dulu).

## Open Items (blocking proposal → implementasi)

1. Scope route: workspace-level (`:workspaceSlug/configuration-items`) vs project-level (`:projectId/configuration-items`)? Proposal ini workspace-level (infra lintas project) — konfirmasi.
2. ID sequence: `CI-001` workspace-scoped (ikut Terra `ids.nextLongLived`) vs UUID. Butuh generator sequence aman konkurensi (cf. `pg_advisory_xact_lock` di `issue.py:180-214`).
3. `environment`/`role`: string bebas (ikut Terra) vs lookup table (validasi Phase 1)?

---

## Changelog

| Date       | Change                                                                                                      |
| ---------- | ----------------------------------------------------------------------------------------------------------- |
| 2026-09-03 | init proposal — dari Terra actual (`features/services/`, `schema.pg.ts`, `routes/cis.ts`), bukan klaim docs |
