# 06 — Ops Runbook

Status: **Draft.**

References: [`01-architecture.md`](./01-architecture.md), [`02-data-model.md`](./02-data-model.md), `plane/settings/common.py`.

---

## Environments

| Env                | URL                                                           | Compose              | DB                                         |
| ------------------ | ------------------------------------------------------------- | -------------------- | ------------------------------------------ |
| Local              | `http://localhost:3000` (web), `:3001` (admin), `:8000` (api) | `docker-compose.yml` | Postgres (compose) + Valkey Redis (Stream) |
| Docker (prod-like) | `WEB_URL`                                                     | `docker-compose.yml` | Postgres (external or compose)             |

`AGENTS.md:28` prereq: `./setup.sh` → `apps/api/.env` dari `.env.example` (dibaca juga oleh Rust `api/worker/beat` + `DATABASE_URL`/`REDIS_URL`/`PORT` override).

---

## Env Vars (Rust: `crates/common/src/config.rs`; Django fallback: `plane/settings/common.py:32`)

| Var                                                   | Required   | Default            | Deskripsi                                                                               |
| ----------------------------------------------------- | ---------- | ------------------ | --------------------------------------------------------------------------------------- |
| `SECRET_KEY`                                          | Prod wajib | random fallback    | Django secret — jangan pakai `60gp0byfz...` placeholder (`plane/settings/common.py:37`) |
| `DATABASE_URL` / `POSTGRES_*`                         | Wajib      | —                  | Postgres — Rust pool 5 koneksi; `plane-db` `max_connections=100`                        |
| `REDIS_URL`                                           | Wajib      | —                  | Valkey: cache + Stream `plane:jobs` (ganti RabbitMQ)                                    |
| `PORT`                                                | —          | `8001` (Rust)      | Compose set `8000` untuk `api`                                                          |
| `WEB_URL`                                             | Wajib prod | `http://localhost` | Django `plane/settings/common.py:418`; validate via `plane.utils.url.is_valid_url`      |
| `ADMIN_BASE_URL` / `SPACE_BASE_URL` / `LIVE_BASE_URL` | Opsional   | `None`             | Django `plane/settings/common.py:391` — validate URL, fallback `None`                   |
| `USE_MINIO`                                           | Opsional   | `0`                | `1` → S3/MinIO (Django `plane/settings/common.py:301`); `AWS_S3_BUCKET_NAME=uploads`    |
| `DEBUG`                                               | —          | `0`                | Django `plane/settings/common.py:51`                                                    |
| `HARD_DELETE_AFTER_DAYS`                              | —          | `60`               | Soft-delete retention (Django + Rust worker cron `hard_delete`)                         |
| `CORS_ALLOWED_ORIGINS`                                | —          | `*` (all)          | Django `plane/settings/common.py:182` (Rust: tower-http CORS)                           |

Fail fast bila `SECRET_KEY` insecure atau `DATABASE_URL` invalid — validate di Django startup via `plane/settings`; Rust `expect("pg connect failed")` saat boot.

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
docker compose up -d --build api worker beat-worker
# api: http://localhost:8000 (Rust), web: http://localhost:3000
# fallback Django (boundary belum di-port): docker compose --profile legacy up -d api-legacy
```

> `docker-compose-local.yml` = stack dev Django legacy (bind-mount `./apps/api`, Django di 8000, masih pakai `plane-mq`) — hanya untuk kerjakan fallback `api-legacy`, bukan stack utama.

### Migrations (sqlx — boot-migrate idempoten)

```bash
# Baseline: apps/api-rs/migrations/0001_initial.sql (squash 123 migrasi Django via pg_dump)
# Delta baru: apps/api-rs/migrations/0002_*.sql (IF NOT EXISTS agar idempoten — lihat migrations/README.md)
# Dijalankan otomatis saat boot api/worker via common::db::migrate; aman di DB live.
# Django (fallback saja): docker compose --profile legacy exec api-legacy python manage.py migrate
```

Rules: migrasi sqlx immutable setelah commit; tabel baru → tambah file `NNNN_*.sql` + test idempotency di `crates/common/tests/migrate_test.rs`.

### Seeding

```bash
python manage.py loaddata plane/seeds/*   # SEED_DIR=plane/seeds (plane/settings/common.py:559)
```

---

## Observability

- **Logs:** Rust JSON via `tracing-subscriber` (`RUST_LOG`); Django `RequestLoggerMiddleware` + Celery logs hanya di fallback. `x-request-id` header (kalau ditambah — contek Terra `09-realtime` pattern).
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
# migrate otomatis (boot-migrate); verifikasi:
curl -s http://localhost:8000/health  # {"service":"rust-api","status":"ok"}
docker stats --no-stream  # api+worker+beat <150 MiB (aktual ~9 MiB)
```

`apps/web/Dockerfile.web`, `apps/admin/Dockerfile.admin`, `apps/api-rs/Dockerfile.rs` (musl static, LTO+strip) — multi-stage.

---

## Scaling Notes

- **Read replica:** Django `ENABLE_READ_REPLICA=1` (fallback saja — Rust pakai pool tunggal 5 koneksi).
- **Worker:** Redis Stream `plane:jobs` (consumer group `workers`, DLQ `plane:dlq` setelah 3 retry) — ganti Celery+RabbitMQ (`plane-mq` dihapus). Cron: `crates/beat` 11 jadwal (email stack `*/5`, hard_delete `0 0 *`, dst.).
- **DB:** `plane-db` `max_connections=100`; tambah index komposit bila filter panas baru muncul (putusan kajian: keep schema — `docs/superpowers/specs/2026-09-04-db-kajian.md`).
- **Live:** `apps/live` scale via Redis pub/sub (`@hocuspocus/extension-redis:2.15.2`).

---

---

## Changelog

| Date       | Change                                                                  |
| ---------- | ----------------------------------------------------------------------- |
| 2026-09-03 | —                                                                       |
| 2026-09-05 | cutover `rust-cutover-v1`: sqlx boot-migrate, tanpa MQ, pool 5 / PG 100 |
