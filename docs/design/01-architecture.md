# 01 — Architecture

Status: **Draft.**

References: [`02-data-model.md`](./02-data-model.md), [`03-api-contract.md`](./03-api-contract.md).

Adaptasi dari `terra/docs/design/03-architecture.md:1` — stack Terra (npm+Express+Drizzle+TanStack Query) diganti stack Plane (pnpm+Turbo+Rust Axum+MobX). Sejak cutover `rust-cutover-v1` (2026-09-05), API utama adalah **Rust Axum + SQLx** (`apps/api-rs`); Django (`apps/api`) tinggal sebagai fallback opt-in `api-legacy` untuk boundary yang belum di-port.

---

## Design Principles

1. **Monorepo dengan batasan tegas.** `apps/*` adalah user-facing app; `packages/*` adalah reusable library. Apps tidak import apps lain. Packages tidak import apps. Enforce via `pnpm-workspace.yaml` + `catalog:` + `workspace:*`.
2. **Postgres sebagai source of truth data.** Skema didefinisikan Django (`apps/api/plane/db/models/`, 123 migrasi → baseline idempoten `apps/api-rs/migrations/0001_initial.sql`); Rust SQLx query tabel yang sama — bukan `entities` JSONB. Frontend (MobX stores) mirror API shape, bukan define sendiri.
3. **TypeScript strict everywhere.** `strict: true`, `noUncheckedIndexedAccess: true` — `packages/typescript-config` shared. Tidak ada `any`.
4. **Thin handlers, pure validators.** Di `apps/api-rs/crates/api/src/routes/` — handler hanya parse req, call validator murni (`validate_*`), query SQLx, serialize response. Business logic async di `crates/worker/src/handlers/` via Redis Stream `plane:jobs`. (Django `plane/api/` + `plane/bgtasks/` hanya referensi/fallback.)
5. **Feature-colocation di frontend.** `apps/web/core/` + `apps/web/app/routes` per domain — bukan sorted by technical concern.
6. **Postgres-only.** `DATABASE_URL` (pool SQLx 5 koneksi, `apps/api-rs/crates/common/src/db.rs`) — no SQLite.
7. **Dev parity dengan prod.** Docker Compose (`docker-compose.yml`, `docker-compose-local.yml`) sama dengan prod image.

---

## Monorepo Structure

```
plane-for-itsm/                            ← pnpm + turbo (terra: npm workspaces)
├── pnpm-workspace.yaml                    ← workspaces: apps/* + packages/*, exclude api/proxy
├── turbo.json                             ← tasks: build, dev, check:lint/types/format
├── package.json                           ← scripts: dev (concurrency 18), build, check, fix
├── .oxlintrc.json + .oxfmtrc.json         ← Oxlint + oxfmt (bukan ESLint/Prettier)
│
├── apps/
│   ├── web/        (@web — React Router 8 SPA, port 3000, `apps/web/app/*` + `core/*`)
│   ├── admin/      (@admin — god-mode, port 3001, `apps/admin/app/(all)/(dashboard)/*`)
│   ├── live/       (live — Express+ws + @hocuspocus/server, Yjs collab, port LIVE_BASE_URL)
│   ├── space/      (space — Pages public, `SPACE_BASE_PATH=/spaces/`)
│   ├── api-rs/     (api — Rust Axum + SQLx + Redis Stream, `crates/api|worker|beat`, port 8000)
│   ├── api/        (api-legacy — Django 5 + DRF, fallback opt-in `--profile legacy`, `plane/*`)
│   └── proxy/      (proxy — reverse)
│
└── packages/
    ├── ui/             (@plane/ui — Radix + Tailwind, Storybook port 6006)
    ├── editor/         (@plane/editor — TipTap 2 + Yjs)
    ├── types/          (@plane/types — shared TS types, issues/cycles/modules)
    ├── shared-state/   (@plane/shared-state — MobX stores, root.store.ts)
    ├── services/       (@plane/services — API clients, axios)
    ├── constants/      (@plane/constants — enums, prefixes)
    ├── i18n/           (@plane/i18n — locales, i18next 25)
    ├── hooks/          (@plane/hooks)
    ├── utils/          (@plane/utils)
    └── tailwind-config/ + typescript-config/ + logger/ + ...
```

**Terra vs Plane:** Terra `apps/web/src/features/<domain>/` + `apps/api/src/services/` + `packages/contracts` (Zod). Plane `apps/web/core/store/<domain>.store.ts` (MobX) + `apps/api-rs/crates/api/src/routes/<domain>.rs` (Axum, validator murni) + `packages/types` + `packages/services` (axios). Skema tabel referensi `apps/api/plane/db/models/<domain>.py` (Django). Jangan copy Terra layout — pakai Plane idioms.

---

## Workspace Configuration

### `pnpm-workspace.yaml`

```yaml
packages:
  - apps/*
  - packages/*
  - "!apps/api" # Django legacy — bukan pnpm workspace
  - "!apps/api-rs" # Rust cargo workspace — bukan pnpm workspace
  - "!apps/proxy"
catalog: # 70+ deps pinned (terra: package.json workspaces)
  react: "19.2.8"
  mobx: "6.12.0"
  "@tiptap/core": "^2.22.3"
  # ... 190 baris
```

### `turbo.json`

```json
{
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**", "build/**", ".react-router/**"] },
    "check": { "dependsOn": ["check:format", "check:lint", "check:types"] },
    "dev": { "cache": false, "persistent": true, "dependsOn": ["^build"] }
  }
}
```

### `package.json` scripts

```json
{
  "scripts": {
    "dev": "turbo run dev --concurrency=18",
    "build": "turbo run build",
    "check": "turbo run check",
    "fix": "turbo run fix",
    "doctor": "npx react-doctor@latest"
  }
}
```

### `apps/web/package.json`

- `dependencies: @plane/* workspace:*` — semua internal via `workspace:*`, external via `catalog:`.
- `dev: react-router dev --port 3000`, `check:types: react-router typegen && tsc --noEmit`, `check:lint: oxlint --max-warnings=...`.

---

## Tech Stack Commitments

### `apps/web`

| Concern        | Library                             | Versi   | Alasan                                     |
| -------------- | ----------------------------------- | ------- | ------------------------------------------ |
| Framework      | React                               | 19      | Plane upstream                             |
| Router         | React Router                        | 8.3.0   | SSR + file-based `app/routes.ts`           |
| State (server) | SWR                                 | 2.4.2   | Cache + revalidate                         |
| State (client) | MobX 6 + mobx-react 9               | 6.12.0  | Stores di `core/store/*` (`root.store.ts`) |
| Editor         | TipTap 2 + Yjs + Hocuspocus         | 2.22.3  | `packages/editor` + `apps/live`            |
| Styling        | Tailwind 4 + @plane/tailwind-config | 4.1.17  | Shared config                              |
| UI             | @plane/ui (Radix) + lucide-react    | —       | Storybook terpisah                         |
| i18n           | i18next 25 + react-i18next 16       | 25.10.9 | `packages/i18n`                            |
| Build          | React Router dev + Vite 8           | 8.0.16  | `react-router.config.ts`                   |
| Lint           | Oxlint 1.51 + oxfmt 0.35            | —       | `docs/linting.md`                          |

### `apps/api-rs` (cutover `rust-cutover-v1` — API utama)

| Concern    | Library                                              | Versi | Alasan                                                  |
| ---------- | ---------------------------------------------------- | ----- | ------------------------------------------------------- |
| Runtime    | Rust                                                 | 1.96  | `rust-toolchain.toml`, musl static                      |
| Framework  | Axum                                                 | 0.7   | `crates/api/src/main.rs`                                |
| DB driver  | SQLx (compile-time checked) + `PgPool` (max 5)       | 0.7   | `crates/common/src/db.rs`                               |
| Migrasi    | sqlx migrate (baseline idempoten `migrations/`)      | —     | boot-migrate di api/worker                              |
| Queue      | Redis Stream `plane:jobs` (consumer group + DLQ)     | —     | `crates/common/src/stream.rs`, ganti RabbitMQ (dihapus) |
| Scheduler  | tokio-cron-scheduler (11 jadwal ala celery beat)     | —     | `crates/beat`                                           |
| Auth       | `X-Api-Key` (DB `api_tokens`) + `Bearer`             | —     | `crates/api/src/middleware/auth.rs`                     |
| Rate-limit | token-bucket per-key, 600/mnt, 429 langsung          | —     | `crates/api/src/middleware/rate_limit.rs`               |
| Alloc      | jemalloc + LTO + strip (binary api ~6,7 MB)          | —     | RSS api+worker+beat ~9 MiB (<150)                       |
| Testing    | cargo test workspace + shadow + parity/cutover gates | —     | `crates/*/tests/`, `scripts/shadow.sh`                  |

Django (`apps/api`: Python 3.11, Django 5 + DRF, Celery + RabbitMQ) tinggal sebagai **fallback opt-in** `api-legacy` (`--profile legacy`) untuk boundary belum di-port: asset S3 upload/download, Unsplash/GPT external, analytic export, notification sending, OAuth — lihat `03-api-contract.md`.

---

## Dependency Rules

| From \ To    | `apps/web` | `apps/admin` | `apps/api` | `packages/*`        |
| ------------ | ---------- | ------------ | ---------- | ------------------- |
| `apps/web`   | —          | ❌           | ❌         | ✅ (`workspace:*`)  |
| `apps/admin` | ❌         | —            | ❌         | ✅                  |
| `packages/*` | ❌         | ❌           | —          | ✅ (antar packages) |

Apps tidak saling import — kalau butuh share, pindahkan ke `packages/*`. Enforce via `pnpm` isolated linker + review (belum ada ESLint `no-restricted-imports` seperti Terra — tambahkan bila perlu).

---

## apps/web Architecture

### Store Pattern (MobX — beda Terra TanStack Query)

```ts
// apps/web/core/store/root.store.ts
export class RootStore {
  workspaceStore = new WorkspaceStore(this);
  projectStore = new ProjectStore(this);
  issueStore = new IssueStore(this);
  cycleStore = new CycleStore(this);
  // ... 30 stores
}
// apps/web/core/store/issue/issue.store.ts — MobX observable + action
```

Tiga level state:

1. **Server state** → **SWR** (`apps/web/core/services/*` via `packages/services`) + **MobX store** (cache di store).
2. **Client state complex** → **MobX** (`core/store/*`).
3. **UI local** → React `useState`.

### Routing (React Router 8 — file-based)

```
apps/web/app/
├── routes.ts                // route config
├── routes/                  // file-based routes
├── (all)/ + (home)/ + assets/ + compat/
├── layout.tsx + root.tsx + provider.tsx
└── types/
```

`apps/web/react-router.config.ts` + `vite.config.ts` (dengan `vite-tsconfig-paths`).

---

## apps/api-rs Architecture (Django `apps/api` = referensi/fallback)

### Request Lifecycle

```
Request
  ↓
tower-http: Trace + CORS + body limit 5 MB
  ↓
middleware: rate_limit (token-bucket 600/mnt → 429) → auth (X-Api-Key → owner user_id, atau Bearer)
  ↓
handler (crates/api/src/routes/<domain>.rs): parse + validate_* murni + query SQLx
  ↓
async? → XADD plane:jobs → worker consumer group → handlers (cleanup, email, webhook, …)
  ↓
Response JSON (kontrak 1:1 Django — shadow.sh + parity gate)
```

### Handler Pattern (Rust — bukan DRF ViewSet)

```rust
// crates/api/src/routes/issue.rs (pola — thin handler)
pub async fn create(State(s): State<AppState>, auth: AuthUser, Json(b): Json<CreateIssue>) -> ... {
    validate_create(&b).map_err(bad_request)?;   // validator murni, unit-testable
    let row = sqlx::query!(...).fetch_one(&s.pool).await?;  // tabel sama dengan Django
    Ok((StatusCode::CREATED, Json(row)))
}
```

Business logic async di `crates/worker/src/handlers/*` (Redis Stream); cron di `crates/beat` (11 jadwal). Validasi sync di fungsi `validate_*` per route.

---

## Build & Deploy (overview — detail di `06-ops-runbook.md`)

```
pnpm build  → turbo run build
  1. packages/* → dist/ (tsdown)
  2. apps/web, admin, space, live → build/ (react-router build)
  3. apps/api-rs → cargo build --release (LTO+strip; Dockerfile.rs, butuh build-base make perl)
Runtime:
  - web: serve build/client (port 3000)
  - api: /usr/local/bin/api (Rust, PORT=8000, boot-migrate sqlx)
  - worker: /usr/local/bin/worker (Redis Stream plane:jobs)
  - beat: /usr/local/bin/beat (11 cron)
  - live: node apps/live/dist (Hocuspocus+ws)
  - proxy: Caddy reverse (`/api/*`, `/auth/*`, `/static/*` → api:8000, tidak berubah saat cutover)
```

Single compose layout: `docker-compose.yml` → web:3000, admin:3001, api:8000 (Rust), live:\*, postgres (`max_connections=100`), redis (Valkey, Stream + cache). `plane-mq` (RabbitMQ) **dihapus**; Django hanya via `--profile legacy` (`api-legacy:8000` internal).

---

## Coding Conventions

| Kind            | Convention                        | Example                                                                   |
| --------------- | --------------------------------- | ------------------------------------------------------------------------- |
| React component | PascalCase                        | `IssueDetail.tsx`                                                         |
| MobX store      | camelCase + `.store.ts`           | `issue.store.ts` (`core/store/issue/`)                                    |
| Django model    | PascalCase, app `plane.db.models` | `Issue`, `Workspace`, `Cycle`, `Module` (referensi skema — dilayani Rust) |
| DRF view        | `<Domain>ViewSet`                 | `IssueViewSet` (legacy — padanan Rust: `routes/issue.rs`)                 |
| Rust route      | snake_case file + handler         | `routes/cycle.rs::list/create/detail/patch/delete`                        |
| Rust validator  | `validate_*` murni                | `validate_archive`, `validate_upload_init`                                |
| API path        | kebab-case                        | `/api/workspaces/:slug/projects/:id/issues/`                              |
| TS type         | PascalCase                        | `Issue`, `Workspace` (`packages/types`)                                   |

Import order: builtins → external (`catalog:`) → `workspace:*` (`@plane/*`) → relative.

---

## Resolved Decisions

| #   | Topik            | Keputusan                                                                                      |
| --- | ---------------- | ---------------------------------------------------------------------------------------------- |
| 1   | Monorepo tooling | **pnpm + Turbo** — actual (`pnpm-workspace.yaml:1`, `turbo.json:1`)                            |
| 2   | Backend          | **Rust Axum + SQLx** — actual (`apps/api-rs`, tag `rust-cutover-v1`); Django = fallback opt-in |
| 3   | Frontend state   | **MobX + SWR** — actual (`CoreRootStore`, `swr:2.4.2`)                                         |
| 4   | Editor           | **TipTap 2 + Yjs + Hocuspocus** — actual (`packages/editor`, `apps/live`)                      |
| 5   | Lint             | **Oxlint + oxfmt** — actual (`package.json:31`)                                                |
| 6   | Realtime         | **apps/live (Express+ws)** — actual (`LIVE_BASE_URL`)                                          |

---

## Actual stores & routes (snapshot kode)

Stores (`apps/web/core/store/root.store.ts:18` — `CoreRootStore`): `workspaceRoot`, `projectRoot`, `memberRoot`, `cycle` (`cycle.store.ts`), `cycleFilter`, `dashboard`, `editor/asset`, `estimates/project-estimate`, `favorite`, `global-view`, `inbox/project-inbox`, `instance`, `issue/root` (`IssueRootStore`), `label`, `module` (`module.store.ts`), `moduleFilter`, `multiple_select`, `notifications/workspace-notifications`, `pages/project-page`, `projectRoot`, `project-view`, `router`, `sticky`, `theme`, `user`. Filter generik via `@plane/shared-state` `WorkItemFilterStore`.

Routes (`apps/web/app/routes/core.ts:1`, `extendedRoutes` kosong): auth (`sign-up`, `accounts/forgot-password|reset-password|set-password`, `create-workspace`, `onboarding`, `invitations`), workspace (`:workspaceSlug`, `active-cycles`, `analytics/:tabId`, `browse/:workItem`, `drafts`, `notifications`, `profile/:userId`, `stickies`, `workspace-views`, `projects`), project (`:workspaceSlug/projects/:projectId/issues` + `issues/:issueId`, `cycles` + `cycles/:cycleId`, `modules` + `modules/:moduleId`, `views` + `views/:viewId`, `pages` + `pages/:pageId`, `intake`, `archives/issues|cycles|modules`), settings (`:workspaceSlug/settings`, `settings/projects`, `settings/profile/:profileTabId`). Tidak ada `incidents` / `configuration-items` — itu propose, lihat `features/_backlog.md`.

Layouts: `apps/web/core/layouts/auth-layout` + `default-layout`; admin sidebar `apps/admin/app/(all)/(dashboard)/sidebar.tsx:1` (`290px ↔ 70px`, `border-r border-subtle bg-surface-1`).

## Open Items

1. **API versioning** — Plane belum versioned (`/api/`); defer sampai ada kebutuhan breaking change.

---

## Changelog

| Date       | Change                                                                                                           |
| ---------- | ---------------------------------------------------------------------------------------------------------------- |
| 2026-09-03 | fork init                                                                                                        |
| 2026-09-05 | cutover `rust-cutover-v1`: API utama Rust Axum+SQLx (api:8000), Django → `api-legacy` opt-in, `plane-mq` dihapus |
