# Business Capabilities — Plane for ITSM

Observational snapshot: apa yang **benar-benar ada di kode** saat ini (`apps/api/plane`, `apps/web`, `packages/*`), bukan yang direncanakan. Tujuan: jawab "produk ini bisa apa" berdasar fakta kode saja.---

## Platform secara umum

Monorepo **pnpm + Turbo** (`pnpm-workspace.yaml:1`, `turbo.json:1`) — 6 apps + 15 packages:

| Komponen                | Stack                                                                      | Peran                                                            |
| ----------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `apps/web`              | React 19 + React Router 8 + Vite + Tailwind 4 + MobX + SWR                 | SPA utama (Work Items, Cycles, dll)                              |
| `apps/admin`            | React Router 8 + Vite                                                      | God-mode admin (`/god-mode/`)                                    |
| `apps/space`            | React Router 8                                                             | Space / Pages public                                             |
| `apps/live`             | Express 4 + express-ws + Hocuspocus + Yjs                                  | Realtime collaboration (Pages)                                   |
| `apps/api-rs`           | Rust Axum + SQLx + Redis Stream `plane:jobs` (api + worker + beat, ~9 MiB) | API utama sejak `rust-cutover-v1` (kontrak 1:1 Django)           |
| `apps/api`              | Django 5 + DRF (fallback opt-in `api-legacy`)                              | Boundary belum di-port: asset S3, external, export, notif, OAuth |
| `apps/proxy`            | —                                                                          | Reverse proxy                                                    |
| `packages/ui`           | React + Radix + Tailwind                                                   | Design system (Storybook `pnpm --filter=@plane/ui storybook`)    |
| `packages/editor`       | TipTap 2 + Yjs                                                             | Rich-text + collaboration                                        |
| `packages/types`        | TypeScript                                                                 | Shared types                                                     |
| `packages/shared-state` | MobX 6                                                                     | Stores (`apps/web/core/store/*`)                                 |
| `packages/services`     | —                                                                          | API clients                                                      |
| `packages/i18n`         | i18next 25                                                                 | Locales (`packages/i18n/src/locales`)                            |

Model data inti Postgres per-table: `plane.db.models` — `WorkItem` (`issue.py`), `Project` (`project.py`), `Workspace` (`workspace.py`), `State` (`state.py`), `Cycle` (`cycle.py`), `Module` (`module.py`), `Page` (`page.py`), `Label` (`label.py`), dll. — dilayani Rust Axum (`apps/api-rs/crates/api/src/routes/`). Bukan `entities` JSONB tunggal seperti Terra — tiap entity adalah tabel sendiri.

---

## 1. Work Items (Issues)

Lokasi: `apps/api/plane/api/views/issue.py` (impl cek `apps/api/plane/db/models/issue.py`), `apps/web/core/store/issue/*`, `packages/types/src/issues.ts`.

- CRUD penuh + filter (state, assignee, label, priority, estimate) + Views (saved filters) + export.
- Rich description via `packages/editor` (TipTap) + versions (`description_version.ts`, `plane/db/models/description.py`) + Yjs collaboration (`@hocuspocus/*`, `yjs`).
- Sub-issues, relations, attachments (`plane/db/models/asset.py`), comments/activity.

## 2. Cycles (Sprints)

Lokasi: `plane.db.models.cycle.Cycle` (`cycle.py`), `apps/web/core/store/cycle.store.ts`, `packages/types/src/cycle`.

- CRUD + filter + burn-down/analytics (`plane.analytics`, `apps/web/core/store/analytics.store.ts`).
- Transfer issues antar cycles, progress tracking.

## 3. Modules

Lokasi: `plane.db.models.module.Module` (`module.py`), `apps/web/core/store/module.store.ts`.

- Divide complex projects; link issues ke modules; progress + analytics.

## 4. Views (Custom filters)

Lokasi: `plane.db.models.view.View` (`view.py`), `apps/web/core/store/project-view.store.ts`.

- Buat filter custom (state/assignee/label/priority/date) → save + share. Global + project scope.

## 5. Pages

Lokasi: `plane.db.models.page.Page` (`page.py`), `plane.db.models.description`, `packages/editor`.

- TipTap rich text + AI capabilities; convert notes → actionable items; Yjs realtime via `apps/live` + Hocuspocus.

## 6. Analytics

Lokasi: `plane.analytics`, `apps/web/core/store/analytics.store.ts`.

- Realtime insights, trends, blockers — agregasi Work Items/Cycles/Modules.

## 7. Workspaces / Projects / Infrastructure

Lokasi: `plane.db.models.workspace.Workspace` (`workspace.py`), `project.py`, `state.py`, `label.py`, `estimate.py`, `member`, `apps/web/core/store/workspace/*`, `project/*`.

- Workspace → Projects → States/Labels/Estimates/Members. Multi-tenant via `workspace_id` scoping.
- Instance admin (`apps/admin`) — workspaces, auth providers (GitHub/GitLab/Google/Gitea — `apps/admin/app/(all)/(dashboard)/authentication/*`), email, general settings.

## 8. ITSM Extensions (belum ada — actual-only)

Tidak ada model/route/store ITSM di kode (`apps/web/app/routes/core.ts` tidak punya `incidents`; `plane/db/models/` tidak punya `ConfigurationItem`). Propose diparkir di `features/_backlog.md`.

## 9. Auth & Multi-tenant

Lokasi: Rust `apps/api-rs/crates/api/src/middleware/auth.rs` (aktual) + Django `plane.authentication` (fallback `api-legacy`).

- `X-Api-Key` (token DB `api_tokens` → owner) + `Bearer`; tanpa token → 401. Session cookie/CSRF hanya di Django fallback.
- OAuth social: GitHub/GitLab/Google/Gitea (`apps/admin/app/(all)/(dashboard)/authentication/*`) — via Django fallback (belum di-port).
- Workspace/Project scoping via `workspace_id`/`project_id` FK — semua query filter by workspace.

## 10. Realtime & Collaboration

Lokasi: `apps/live` (Express + ws), `@hocuspocus/server`, `yjs`, `apps/web/core/store/*` (MobX).

- Pages collaboration via Hocuspocus + Yjs (bukan SSE `GET /events` Terra).
- `apps/live` sebagai standalone app — `LIVE_BASE_URL`/`LIVE_BASE_PATH` (`plane/settings/common.py:409`).

## 11. Admin & Governance

Lokasi: `apps/admin`, `plane.license`, `plane.api`.

- God-mode: workspaces, users, auth, email, general, image, AI (`apps/admin/app/(all)/(dashboard)/*`).
- `ADMIN_BASE_PATH=/god-mode/` (`plane/settings/common.py:395`), `ADMIN_SESSION_COOKIE_NAME=admin-session-id:379`.

---

## Ringkasan capability blocks

| #   | Capability block              | Endpoint/komponen utama                                       |
| --- | ----------------------------- | ------------------------------------------------------------- |
| 1   | Work Items (issues)           | `plane.db.models.issue`, `apps/web/core/store/issue`          |
| 2   | Cycles (sprints)              | `plane.db.models.cycle`, `apps/web/core/store/cycle.store.ts` |
| 3   | Modules                       | `plane.db.models.module`                                      |
| 4   | Views (saved filters)         | `plane.db.models.view`                                        |
| 5   | Pages (TipTap + Yjs)          | `plane.db.models.page`, `packages/editor`, `apps/live`        |
| 6   | Analytics                     | `plane.analytics`                                             |
| 7   | Workspace/Project/State/Label | `plane.db.models.workspace/project/state/label`               |
| 8   | ITSM (fork)                   | `features/*` (incidents/problems/changes/…)                   |
| 9   | Auth + multi-tenant           | `plane.authentication`, `plane.db.models.session`             |
| 10  | Realtime                      | `apps/live`, `@hocuspocus/*`, `yjs`                           |
| 11  | Admin (god-mode)              | `apps/admin`, `ADMIN_BASE_PATH`                               |

---

## Catatan

- Dokumen ini observasional terhadap kode Plane upstream + fork delta; path/line bisa bergeser seiring refactor.
- Perbedaan Plane vs Terra: Plane = Postgres per-table + Rust Axum + MobX + pnpm+Turbo + 6 apps; Terra = Express single `entities` JSONB + Drizzle + TanStack Query + 2 apps. Docs Plane tidak copy ERD `entities.data` — pakai idiom tabel-per-entity.
- Klaim dari `README.md:48` (Work Items/Cycles/Modules/Views/Pages/Analytics) verified ada di `plane.db.models/*` + `apps/web/core/store/*`.

---

## Changelog

| Date       | Change                                                           |
| ---------- | ---------------------------------------------------------------- |
| 2026-09-03 | fork init                                                        |
| 2026-09-05 | cutover `rust-cutover-v1`: API utama Rust, auth X-Api-Key/Bearer |
