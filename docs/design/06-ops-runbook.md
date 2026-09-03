# 06 — Ops Runbook

Status: **Draft.**

References: [`01-architecture.md`](./01-architecture.md), [`02-data-model.md`](./02-data-model.md), `plane/settings/common.py`.

---

## Environments

| Env                | URL                                                           | Compose                    | DB                                    |
| ------------------ | ------------------------------------------------------------- | -------------------------- | ------------------------------------- |
| Local              | `http://localhost:3000` (web), `:3001` (admin), `:8000` (api) | `docker-compose-local.yml` | Postgres (compose) + Redis + RabbitMQ |
| Docker (prod-like) | `WEB_URL` (`plane/settings/common.py:418`)                    | `docker-compose.yml`       | Postgres (external or compose)        |

`AGENTS.md:28` prereq: `./setup.sh` → `apps/api/.env` dari `.env.example`.

---

## Env Vars (validasi di `plane/settings/common.py:32`)

| Var                                                   | Required   | Default            | Deskripsi                                                                               |
| ----------------------------------------------------- | ---------- | ------------------ | --------------------------------------------------------------------------------------- |
| `SECRET_KEY`                                          | Prod wajib | random fallback    | Django secret — jangan pakai `60gp0byfz...` placeholder (`plane/settings/common.py:37`) |
| `DATABASE_URL` / `POSTGRES_*`                         | Wajib      | —                  | Postgres — `plane/settings/common.py:204`                                               |
| `REDIS_URL`                                           | Wajib      | —                  | `plane/settings/common.py:242` (`rediss` → SSL)                                         |
| `WEB_URL`                                             | Wajib prod | `http://localhost` | `plane/settings/common.py:418`; validate via `plane.utils.url.is_valid_url`             |
| `ADMIN_BASE_URL` / `SPACE_BASE_URL` / `LIVE_BASE_URL` | Opsional   | `None`             | `plane/settings/common.py:391` — validate URL, fallback `None`                          |
| `USE_MINIO`                                           | Opsional   | `0`                | `1` → S3/MinIO (`plane/settings/common.py:301`); `AWS_S3_BUCKET_NAME=uploads:307`       |
| `DEBUG`                                               | —          | `0`                | `plane/settings/common.py:51`                                                           |
| `HARD_DELETE_AFTER_DAYS`                              | —          | `60`               | Soft-delete retention (`plane/settings/common.py:420`)                                  |
| `CORS_ALLOWED_ORIGINS`                                | —          | `*` (all)          | `plane/settings/common.py:182`                                                          |

Fail fast bila `SECRET_KEY` insecure atau `DATABASE_URL` invalid — Zod-like validate di Django startup via `plane/settings`.

---

## Day-to-Day

### Build & Run

```bash
pnpm dev              # turbo run dev --concurrency=18 (AGENTS.md:5) — web:3000, admin:3001
pnpm build            # turbo run build
pnpm check            # format+lint+types (AGENTS.md:6) — oxfmt + oxlint + tsc
pnpm fix              # auto-fix
```

Backend Docker:

```bash
./setup.sh
docker compose -f docker-compose-local.yml up --build
# api: http://localhost:8000, web: http://localhost:3000
```

### Migrations

```bash
# Django — bukan drizzle-kit Terra
docker compose -f docker-compose-local.yml exec api python manage.py makemigrations
docker compose -f docker-compose-local.yml exec api python manage.py migrate
docker compose -f docker-compose-local.yml exec api python manage.py createsuperuser
```

Rules: migrations immutable setelah commit; `plane/db/models/*.py` + migration file commit atomic.

### Seeding

```bash
python manage.py loaddata plane/seeds/*   # SEED_DIR=plane/seeds (plane/settings/common.py:559)
```

---

## Observability

- **Logs:** `plane.middleware.logger.RequestLoggerMiddleware` (`plane/settings/common.py:134`) + `APITokenLogMiddleware` + Celery logs. Structured JSON di prod, pretty di dev. `x-request-id` header (kalau ditambah — contek Terra `09-realtime` pattern).
- **Retention:** `API_ACTIVITY_LOG_RETENTION_DAYS=14` (`plane/settings/common.py:441`), `WEBHOOK_LOG_RETENTION_DAYS=14`, `EMAIL_LOG_RETENTION_DAYS=7` — via `_retention_days` helper.
- **Sentry (opsional):** `SENTRY_DSN` (`turbo.json:9` globalEnv) — belum wiring ITSM, defer.

---

## Backup & Restore

- **DB:** `pg_dump` via Postgres image / external. Retention: 7 daily + 4 weekly (manual cron di `deployments/`).
- **Storage:** S3/MinIO bucket `uploads` (`plane/settings/common.py:307`) — versioning + lifecycle di provider.
- **Restore drill:** quarterly di staging — restore `pg_dump` ke `DATABASE_READ_REPLICA_URL` (`plane/settings/common.py:222`).

---

## Deploy (Docker — bukan Render Terra)

Terra deploy ke Render free-tier (`terra/CHEAT-SHEET.md:256`). Plane: **Docker Compose** (`docker-compose.yml`) atau `deployments/` (K8s/Railway/Render via Docker).

```bash
docker compose -f docker-compose.yml up -d --build
docker compose exec api python manage.py migrate --no-input
docker compose exec api python manage.py collectstatic --no-input
```

`apps/web/Dockerfile.web`, `apps/admin/Dockerfile.admin`, `apps/api/Dockerfile.api` — multi-stage.

---

## Scaling Notes

- **Read replica:** `ENABLE_READ_REPLICA=1` + `DATABASE_READ_REPLICA_URL` → `ReadReplicaRouter` (`plane/settings/common.py:221,236`).
- **Celery:** `CELERY_BROKER_URL` RabbitMQ (`plane/settings/common.py:327`) + `CELERY_IMPORTS` (`plane/settings/common.py:338` — issue_automation, exporter, cleanup, telemetry).
- **Live:** `apps/live` scale via Redis pub/sub (`@hocuspocus/extension-redis:2.15.2`).

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
