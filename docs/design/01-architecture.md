# 01 — Architecture

Status: **Draft.**

References: [`02-data-model.md`](./02-data-model.md), [`03-api-contract.md`](./03-api-contract.md).

Adaptasi dari `terra/docs/design/03-architecture.md:1` — stack Terra (npm+Express+Drizzle+TanStack Query) diganti stack Plane (pnpm+Turbo+Django+MobX).

---

## Design Principles

1. **Monorepo dengan batasan tegas.** `apps/*` adalah user-facing app; `packages/*` adalah reusable library. Apps tidak import apps lain. Packages tidak import apps. Enforce via `pnpm-workspace.yaml` + `catalog:` + `workspace:*`.
2. **Django sebagai source of truth data.** Semua model di `apps/api/plane/db/models/` — bukan `entities` JSONB. Frontend (MobX stores) mirror API shape, bukan define sendiri.
3. **TypeScript strict everywhere.** `strict: true`, `noUncheckedIndexedAccess: true` — `packages/typescript-config` shared. Tidak ada `any`.
4. **Thin views, fat services.** Di `apps/api/plane/api/` — view hanya parse req, call service/serializer, serialize response. Business logic di `services/`/`bgtasks/`.
5. **Feature-colocation di frontend.** `apps/web/core/` + `apps/web/app/routes` per domain — bukan sorted by technical concern.
6. **Postgres-only.** `dj_database_url` (`plane/settings/common.py:204`) — no SQLite.
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
│   ├── api/        (api — Django 5 + DRF + Celery + Postgres + Redis + RabbitMQ, `plane/*`)
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

**Terra vs Plane:** Terra `apps/web/src/features/<domain>/` + `apps/api/src/services/` + `packages/contracts` (Zod). Plane `apps/web/core/store/<domain>.store.ts` (MobX) + `apps/api/plane/db/models/<domain>.py` (Django) + `packages/types` + `packages/services` (axios). Jangan copy Terra layout — pakai Plane idioms.

---

## Workspace Configuration

### `pnpm-workspace.yaml`

```yaml
packages:
  - apps/*
  - packages/*
  - "!apps/api" # Django — bukan pnpm workspace
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

### `apps/api`

| Concern   | Library                                     | Versi | Alasan                                        |
| --------- | ------------------------------------------- | ----- | --------------------------------------------- |
| Runtime   | Python                                      | 3.11+ | Django requirement                            |
| Framework | Django 5 + DRF                              | —     | `plane/settings/common.py:114`                |
| DB driver | psycopg + dj_database_url                   | —     | `plane/settings/common.py:204`                |
| Cache     | django-redis + Redis                        | —     | `plane/settings/common.py:242`                |
| Queue     | Celery + RabbitMQ                           | —     | `plane/settings/common.py:326`                |
| Auth      | SessionAuthentication + django-cors-headers | —     | `plane/settings/common.py:138`                |
| Storage   | S3/MinIO (whitenoise)                       | —     | `plane/settings/common.py:303`                |
| Testing   | pytest + pytest-django                      | —     | `apps/api/tests/` + `docker-compose-test.yml` |

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

## apps/api Architecture

### Request Lifecycle

```
Request
  ↓
middleware: CorsMiddleware → SecurityMiddleware → SessionMiddleware (plane 125)
  ↓
DRF: authentication (SessionAuthentication 139) → throttling (Anonymous/asset_id) → permission (IsAuthenticated 145)
  ↓
view (apps/api/plane/api/views/*): parse + serializer.validate + call service
  ↓
service / bgtasks (plane.bgtasks.*, Celery): business logic + transaksi
  ↓
serializer: serialize response
  ↓
middleware: logger (RequestLoggerMiddleware 134)
  ↓
Response (JSONRenderer 146)
```

### Service Layer (Django — bukan Express service)

```python
# plane/api/views/issue.py (contoh pola — thin view)
class IssueViewSet(viewsets.ModelViewSet):
    serializer_class = IssueSerializer
    permission_classes = [IsAuthenticated]
    def create(self, request, *args, **kwargs):
        serializer = self.get_serializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        serializer.save(created_by=request.user, workspace=request.workspace)
        return Response(serializer.data, status=201)
```

Business logic di `plane/bgtasks/*` (Celery) untuk async; sync logic di `plane/utils/*` atau `plane/api/services/`.

---

## Build & Deploy (overview — detail di `06-ops-runbook.md`)

```
pnpm build  → turbo run build
  1. packages/* → dist/ (tsdown)
  2. apps/web, admin, space, live → build/ (react-router build)
  3. apps/api → collectstatic (whitenoise)
Runtime:
  - web: serve build/client (port 3000)
  - api: gunicorn plane.wsgi (DJANGO_SETTINGS_MODULE=plane.settings)
  - live: node apps/live/dist (Hocuspocus+ws)
  - proxy: nginx reverse
```

Single compose layout: `docker-compose.yml` → web:3000, admin:3001, api:8000, live:\*, postgres, redis, rabbitmq.

---

## Coding Conventions

| Kind            | Convention                        | Example                                      |
| --------------- | --------------------------------- | -------------------------------------------- |
| React component | PascalCase                        | `IssueDetail.tsx`                            |
| MobX store      | camelCase + `.store.ts`           | `issue.store.ts` (`core/store/issue/`)       |
| Django model    | PascalCase, app `plane.db.models` | `Issue`, `Workspace`, `Cycle`, `Module`      |
| DRF view        | `<Domain>ViewSet`                 | `IssueViewSet`                               |
| API path        | kebab-case                        | `/api/workspaces/:slug/projects/:id/issues/` |
| TS type         | PascalCase                        | `Issue`, `Workspace` (`packages/types`)      |

Import order: builtins → external (`catalog:`) → `workspace:*` (`@plane/*`) → relative.

---

## Resolved Decisions

| #   | Topik            | Keputusan                                                                 |
| --- | ---------------- | ------------------------------------------------------------------------- |
| 1   | Monorepo tooling | **pnpm + Turbo** — actual (`pnpm-workspace.yaml:1`, `turbo.json:1`)       |
| 2   | Backend          | **Django 5 + DRF** — actual (`plane/settings/common.py:97`)               |
| 3   | Frontend state   | **MobX + SWR** — actual (`CoreRootStore`, `swr:2.4.2`)                    |
| 4   | Editor           | **TipTap 2 + Yjs + Hocuspocus** — actual (`packages/editor`, `apps/live`) |
| 5   | Lint             | **Oxlint + oxfmt** — actual (`package.json:31`)                           |
| 6   | Realtime         | **apps/live (Express+ws)** — actual (`LIVE_BASE_URL`)                     |

---

## Actual stores & routes (snapshot kode)

Stores (`apps/web/core/store/root.store.ts:18` — `CoreRootStore`): `workspaceRoot`, `projectRoot`, `memberRoot`, `cycle` (`cycle.store.ts`), `cycleFilter`, `dashboard`, `editor/asset`, `estimates/project-estimate`, `favorite`, `global-view`, `inbox/project-inbox`, `instance`, `issue/root` (`IssueRootStore`), `label`, `module` (`module.store.ts`), `moduleFilter`, `multiple_select`, `notifications/workspace-notifications`, `pages/project-page`, `projectRoot`, `project-view`, `router`, `sticky`, `theme`, `user`. Filter generik via `@plane/shared-state` `WorkItemFilterStore`.

Routes (`apps/web/app/routes/core.ts:1`, `extendedRoutes` kosong): auth (`sign-up`, `accounts/forgot-password|reset-password|set-password`, `create-workspace`, `onboarding`, `invitations`), workspace (`:workspaceSlug`, `active-cycles`, `analytics/:tabId`, `browse/:workItem`, `drafts`, `notifications`, `profile/:userId`, `stickies`, `workspace-views`, `projects`), project (`:workspaceSlug/projects/:projectId/issues` + `issues/:issueId`, `cycles` + `cycles/:cycleId`, `modules` + `modules/:moduleId`, `views` + `views/:viewId`, `pages` + `pages/:pageId`, `intake`, `archives/issues|cycles|modules`), settings (`:workspaceSlug/settings`, `settings/projects`, `settings/profile/:profileTabId`). Tidak ada `incidents` / `configuration-items` — itu propose, lihat `features/_backlog.md`.

Layouts: `apps/web/core/layouts/auth-layout` + `default-layout`; admin sidebar `apps/admin/app/(all)/(dashboard)/sidebar.tsx:1` (`290px ↔ 70px`, `border-r border-subtle bg-surface-1`).

## Open Items

1. **API versioning** — Plane belum versioned (`/api/`); defer sampai ada kebutuhan breaking change.

---

## Changelog

| Date       | Change    |
| ---------- | --------- |
| 2026-09-03 | fork init |
