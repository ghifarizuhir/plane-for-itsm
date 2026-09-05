# Cross-Cutting Decisions — Cheat Sheet

Single-page reference keputusan arsitektur + product yang tersebar di `design/` + `features/` + `ui/`. Dipakai sebagai **final sign-off checklist** sebelum coding dan reference cepat selama implementasi.

Kalau konflik dengan doc spesifik, **doc spesifik menang** — ini rangkuman, bukan source of truth. Adaptasi dari `terra/docs/CHEAT-SHEET.md:1`.

---

## 1. Stack & Tooling

| Layer       | Pilihan                                                                                                                                                                                                 | Rujukan                         |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| Monorepo    | **pnpm workspaces + Turbo** (`pnpm-workspace.yaml:1`, `turbo.json:1`) — bukan npm workspaces Terra                                                                                                      | `design/01-architecture.md`     |
| Backend     | **Rust Axum + SQLx + Redis Stream** (`apps/api-rs`, tag `rust-cutover-v1`) — skema dari Django 123 migrasi (baseline idempoten); Django = fallback opt-in `api-legacy`                                  | `design/01-architecture.md`     |
| Frontend    | **React 19 + React Router 8 + Vite + TS + Tailwind 4 + Radix + MobX 6 + SWR + TipTap 2** (`apps/web/package.json:67`, `pnpm-workspace.yaml:52`)                                                         | `design/01-architecture.md`     |
| Database    | **Postgres** (prod + tests via `TEST_PG_URL` / `DATABASE_URL`) — `plane/settings/common.py:204`                                                                                                         | `design/02-data-model.md`       |
| Test runner | **cargo test** (workspace `apps/api-rs`: unit + shadow + parity/cutover gates) + **pytest** (Django baseline via `docker-compose-test.yml`) + **Vitest 4** (web/packages) + RTL + Playwright (deferred) | `design/05-testing-strategy.md` |
| Lint/format | **Oxlint 1.51 + oxfmt 0.35** (`pnpm-workspace.yaml:137`, `package.json:31`) — bukan ESLint+Prettier                                                                                                     | `docs/linting.md`               |
| Editor      | **TipTap 2 + Yjs + Hocuspocus 2.15** (`pnpm-workspace.yaml:28`, `packages/editor`)                                                                                                                      | `ui/editor.md`                  |
| Realtime    | **apps/live (Express+ws)** + Hocuspocus (`plane/settings/common.py:409` LIVE_BASE_URL) — bukan SSE ticket Terra                                                                                         | `design/01-architecture.md`     |
| Process     | **Docker Compose** (`docker-compose.yml`, `docker-compose-local.yml`) + `deployments/`                                                                                                                  | `design/06-ops-runbook.md`      |

---

## 2. Monorepo Layout

```
plane-for-itsm/                         ← pnpm + turbo (Terra: npm workspaces)
├── pnpm-workspace.yaml                 ← workspaces: apps/* + packages/* (exclude api/proxy)
├── turbo.json                          ← tasks: build, dev, check:lint/types/format
├── package.json                        ← scripts: pnpm dev/build/check/fix (AGPL-3.0)
├── apps/
│   ├── web/        (web — React Router SPA, port 3000)
│   ├── admin/      (admin — god-mode, port 3001)
│   ├── live/       (live — Express+ws collaboration, Hocuspocus)
│   ├── space/      (space — Pages public)
│   ├── api-rs/     (api — Rust Axum+SQLx, worker, beat, cargo test, port 8000)
│   ├── api/        (api-legacy — Django opt-in `--profile legacy`, pytest baseline)
│   └── proxy/      (proxy — reverse)
├── packages/
│   ├── ui/             (@plane/ui — Radix + Tailwind, Storybook)
│   ├── editor/         (@plane/editor — TipTap 2 + Yjs)
│   ├── types/          (@plane/types — shared TS types)
│   ├── shared-state/   (@plane/shared-state — MobX stores)
│   ├── services/       (@plane/services — API clients)
│   ├── constants/      (@plane/constants — enums, prefixes)
│   ├── i18n/           (@plane/i18n — locales)
│   └── utils/hooks/logger/...
└── docs/
    ├── design/     (engineering cross-cutting)
    ├── features/   (product per-halaman + _shared)
    └── ui/         (shell + editor specs)
```

**Dependency rules** (enforce via `pnpm-workspace.yaml:1` `catalog:` + `workspace:*`):

- `apps/*` ❌ import `apps/*` lain
- `packages/*` ❌ import `apps/*`
- `apps/*` ✅ import `packages/*` via `workspace:*`

---

## 3. Data Model Core

**Paradigm Terra (referensi, jangan copy):** satu tabel `entities` JSONB (`terra/docs/design/01-erd.md:12`).

**Paradigm Plane (aktual, pakai ini):** Postgres per-table — tiap entity tabel sendiri, skema didefinisikan Django di `apps/api/plane/db/models/`, **dilayani Rust Axum** (`apps/api-rs/crates/api/src/routes/`):

| Model               | File                             | Kunci                                                                                  |
| ------------------- | -------------------------------- | -------------------------------------------------------------------------------------- |
| `Workspace`         | `workspace.py`                   | multi-tenant root                                                                      |
| `Project`           | `project.py`                     | scope untuk issues/cycles/modules                                                      |
| `State`             | `state.py`                       | workflow states per project                                                            |
| `Issue` (Work Item) | `issue.py`                       | core work item                                                                         |
| `Cycle`             | `cycle.py`                       | sprint, burn-down                                                                      |
| `Module`            | `module.py`                      | sub-project grouping                                                                   |
| `Page`              | `page.py`                        | TipTap + Yjs                                                                           |
| `Label`             | `label.py`                       | tagging                                                                                |
| ITSM (fork)         | `issue.py` extended / new models | Incident/Problem/Change via IssueType atau tabel baru (putuskan di `02-data-model.md`) |

**Actual:** Postgres per-table (skema Django `plane/db/models/` — `workspace`, `project`, `issue`, `issue_type`, `cycle`, `module`, `page`, `view`, `state`, `label`, ...; migrasi sqlx `apps/api-rs/migrations/`). Tidak ada `entities` JSONB, tidak ada `ConfigurationItem`. Propose ITSM di `features/_backlog.md`.

---

## 4. Auth & Authorization

| Aspek   | Plane (aktual)                                                                                                                                             | Terra (referensi — jangan pakai)                   |
| ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Auth    | Rust `X-Api-Key` (token DB `api_tokens` → owner) + `Bearer` (`middleware/auth.rs:12`); tanpa token → 401. Session cookie/CSRF hanya di Django `api-legacy` | JWT bearer 15m + DPoP (`terra/CHEAT-SHEET.md:84`)  |
| Social  | GitHub/GitLab/Google/Gitea (`apps/admin/app/(all)/(dashboard)/authentication/*`)                                                                           | Username+password + device-code MCP                |
| Scoping | `workspace_id` + `project_id` FK di semua model; instance admin override via `apps/admin` god-mode (`ADMIN_BASE_PATH=/god-mode/:395`)                      | `org_id` di 30+ tabel + scope `any/dept/team/self` |
| Roles   | Instance admin → Workspace admin → Project admin → Member (Plane hierarchy)                                                                                | `gm/dept_head/team_lead/member`                    |

**Enforcement:** Axum extractor `AuthUser` + validator `validate_*` per route + rate-limit token-bucket 600/mnt (429); kontrak 1:1 Django dijaga shadow + parity gate. Detail: `design/03-api-contract.md`.

---

## 5. URL & Routing

**Terra:** `/o/:orgId/<type>/<id>` (`terra/CHEAT-SHEET.md:152`) + `?q=&status=&sort=&page=` + `withAppScope`.

**Plane (aktual):** React Router 8 (`apps/web/app/routes.ts`, `apps/web/core/helper.ts`):

- Workspaces: `/workspaces` / `/:workspaceSlug` / `/:workspaceSlug/projects/:projectId/issues/:issueId`
- Admin: `/god-mode/*` (`ADMIN_BASE_PATH`)
- Live: `/live/*` (`LIVE_BASE_PATH`)
- Space: `/spaces/*` (`SPACE_BASE_PATH`)
- ITSM fork: tambahkan di bawah workspace scope — `/:workspaceSlug/itsm/incidents/:id` atau `/projects/:projectId/incidents` (putuskan di `features/README.md`; jangan pakai `/o/:orgId` Terra — Plane pakai slug).

Query: `?q=&state=&priority=&sort=&page=` — sama dengan Terra, tapi param names ikut Plane (`state` bukan `status`).

---

## 6. UI Patterns

| Pattern    | Plane (aktual)                                                                                            | Terra referensi                                |
| ---------- | --------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Shell      | `apps/web/app/layout.tsx` + `apps/web/core/layouts` + `packages/ui` (`ui/shell.md`)                       | `docs/ui/sidebar.md:40` 180px↔44px sidebar     |
| Editor     | `packages/editor` TipTap 2 + Yjs collab (`ui/editor.md`)                                                  | `ui/editor.md` TipTap mention                  |
| Tokens     | `packages/tailwind-config` + `packages/ui/styles` (`ui/design-tokens.md`) — `theme-*` / `wash-*`          | `ui/design-tokens.md:32` `bg-theme-*`          |
| State      | **MobX** (`packages/shared-state`, `apps/web/core/store/root.store.ts`) + SWR — bukan TanStack Query only | `design/03-architecture.md:140` TanStack Query |
| Components | `@plane/ui` Storybook (`pnpm --filter=@plane/ui storybook` — `AGENTS.md:11`)                              | `@terra/ui` Storybook                          |

---

## 7. Testing Targets

| Layer                     | Target   | Tool                                                                                          | Rujukan                         |
| ------------------------- | -------- | --------------------------------------------------------------------------------------------- | ------------------------------- |
| `apps/api-rs` routes      | **100%** | `cargo test --workspace` — TDD, 0 failed gate + shadow/parity/cutover                         | `design/05-testing-strategy.md` |
| `apps/api/plane` services | **70%**  | `pytest` via `docker-compose-test.yml` (`AGENTS.md:28`) — baseline kontrak Django (573 green) | `design/05-testing-strategy.md` |
| `apps/api` middleware     | 80%      | pytest                                                                                        | —                               |
| `apps/web/core/store`     | 50%      | Vitest + RTL                                                                                  | —                               |
| `packages/ui`             | 60%      | Vitest + Storybook                                                                            | —                               |
| Global                    | 55%      | —                                                                                             | —                               |

**Commands (AGENTS.md:7):**

- `pnpm check` — all (format+lint+types)
- `pnpm turbo run check:lint --filter=@plane/ui` — per-package
- `DATABASE_URL=postgres://plane:plane@<plane-db-ip>:5432/plane_test cargo test --workspace` — Rust (catatan: IP bridge berubah tiap recreate; dari dalam compose pakai hostname `plane-db`)
- `docker compose -f docker-compose-test.yml up --build --abort-on-container-exit --exit-code-from api-tests` — full pytest baseline Django

---

## 8. Ops

- **Env:** `apps/api/.env` dari `.env.example` via `./setup.sh` (`AGENTS.md:28`). Rust baca `DATABASE_URL` / `REDIS_URL` / `PORT` (`crates/common/src/config.rs`).
- **DB:** Postgres (`max_connections=100`); pool SQLx 5 koneksi; boot-migrate idempoten `apps/api-rs/migrations/`. Redis Valkey: cache + Stream `plane:jobs` (ganti RabbitMQ/Celery — `plane-mq` dihapus).
- **Storage:** `STORAGES` S3/MinIO (`plane/settings/common.py:303` `USE_MINIO`, `AWS_S3_BUCKET_NAME`) — sama dengan Terra R2.
- **Deploy:** `docker-compose.yml` + `deployments/` + `Dockerfile.*` (bukan `systemd` Terra).

---

## 9. ITSM Scope (future — actual-only)

Belum ada kode ITSM (tidak ada route `incidents`, model `ConfigurationItem`). Semua scope ITSM diparkir di `features/_backlog.md` — jangan implementasi dari CHEAT-SHEET ini.

---

---

## Changelog

| Date       | Change                                                                                   |
| ---------- | ---------------------------------------------------------------------------------------- |
| 2026-09-03 | fork init — adaptasi dari terra `CHEAT-SHEET.md:1`                                       |
| 2026-09-05 | cutover `rust-cutover-v1`: backend Rust Axum+SQLx, auth X-Api-Key/Bearer, tanpa RabbitMQ |
