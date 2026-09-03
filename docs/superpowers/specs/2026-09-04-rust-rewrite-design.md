# Rust Rewrite Design — Plane ITSM API (Django → Axum + SQLx)

**Date:** 2026-09-04
**Status:** Approved (user: hybrid Strangler Fig + Worker-First, Redis Stream)
**Goal:** <150 MiB total for `api+worker+beat` (baseline 509 MiB: worker 214 + api 159 + beat 135 MiB via `docker stats`)
**Scope:** Full `apps/api` → Rust, incremental, solo, keep API contract 100%, keep DB schema default with kajian redesign, deploy tetap `docker-compose.yml`

---

## 1. Context & Constraints

- **Current stack:** `apps/api/Dockerfile.api:1` `python:3.12.10-alpine`, Django 5.2.15 + DRF 3.17.1 (`requirements/base.txt:4`), Celery 5.5.3 + `django_celery_beat` 2.9.0, Gunicorn 23 + Uvicorn 0.29, PG 15.7 (`docker-compose.yml:112`), Valkey 7.2.11, RabbitMQ 3.13.6, Minio.
- **Monolith:** 653 `.py` files (`plane/` 5.9M), `INSTALLED_APPS` `plane/settings/common.py:319`, URLs ~2300 lines (`plane/api/urls/*`, `plane/app/urls/*`), models ~30 files `plane/db/models/`, ~40 `plane/bgtasks/*.py` `@shared_task`.
- **Beat schedule:** `plane/celery.py:44-95` 11 jobs (every 5min email stack, daily 00:00 hard_delete, 01:00 archive, 02:00-03:30 cleanup, 360min telemetry).
- **Broker:** `plane/settings/common.py:328` `CELERY_BROKER_URL=amqp://` → RabbitMQ. User approved ganti ke Redis Stream (reuse `plane-redis`).
- **Contract:** Frontend `web:3000`, `admin:3001`, `space` expect exact JSON from `plane/api/urls/*` + `plane/app/urls/*` — must not break.
- **Deploy:** Tetap `docker-compose.yml:37-97` 4 services sharing 1 image → Rust will provide 1 crate 3 binaries (`api`, `worker`, `beat`) with same compose shape.

## 2. Architecture

```
[Next.js web/admin/space] → [proxy:80 apps/proxy/Dockerfile.ce] ┬→ [rust-api:8000 Axum] ─┐
                                                             └→ [django-api:8000] (fallback legacy) → [plane-db:5432]
[runtime]  rust-worker (tokio) ← Redis Stream `plane:jobs` ← rust-beat (tokio-cron) → Pg
           │ SQLx PgPool (5-10 conns) → S3/Minio (boto→ aws-sdk-rust), Redis cache (redis-rs)
```

- **Crate layout:** `apps/api-rs/` (new) — workspace:
  - `crates/common` — `sqlx::FromRow` models mirroring `plane/db/models/*.py`, `serde` DTOs, `config` crate reading `apps/api/.env`, JWT (PyJWT → `jsonwebtoken`), error types.
  - `crates/api` — Axum routers per domain (`/api/workspaces`, `/api/issues` etc), `tower-http` `TraceLayer`, `CorsLayer`, `RequestBodyLimit`, auth extractor (replace `plane/api/middleware/api_authentication.py`, `plane/authentication/middleware/session.py`).
  - `crates/worker` — Redis Stream consumer group `plane-worker`, handlers 1:1 `plane/bgtasks/*.py`.
  - `crates/beat` — `tokio-cron-scheduler` + `sqlx` table `periodic_tasks` (migrate from `django_celery_beat`), pushes to Stream.
- **Docker:** Builder `rust:1.78-alpine` + runtime `alpine` + `jemalloc`, `RUSTFLAGS`, profile `lto=true, codegen-units=1, panic=abort, strip=true` → ~8MB binary, ~15MiB RSS idle.

## 3. Component Details

### 3.1 API (Axum)

- Routers mirror file-per-domain: `crates/api/src/routes/workspace.rs` ↔ `plane/app/urls/workspace.py:260`, `issue.rs` ↔ `issue.py:286` etc.
- State: `AppState { pool: PgPool, redis: redis::Client, s3: aws_sdk_s3::Client }` via `Arc`.
- Auth: Extract `Authorization: Bearer` JWT, validate via `jsonwebtoken` + `REDIS` session lookup (`plane/settings/redis.py:10` `redis_instance()` → `redis-rs`).
- Validation: `validator` crate replaces `IssueSerializer` logic (`plane/api/serializers/issue.py`).
- Static: Replace `whitenoise` with `tower-http::services::ServeDir` if needed.

### 3.2 Worker (Redis Stream)

- Stream: `XADD plane:jobs * job <name> payload <json>`, consumer group `workers`, `XREADGROUP BLOCK 5000`.
- Keep RabbitMQ removal: delete `plane-mq` service (-72 MiB), reuse `plane-redis` (already 4.6 MiB). Library `redis = { features = ["tokio-comp", "streams"] }`.
- Handlers: `email_notification`, `webhook_task` (4 tasks `plane/bgtasks/webhook_task.py`), `export_task` (use `umya-spreadsheet` vs `openpyxl`), `cleanup_task` (7 subtasks), `issue_automation`, etc — `sqlx::query_as!` compile-time checked.
- Retry/DLQ: `XACK` on success, `XADD plane:dlq` on 3 retries, `tracing` json logs (replace `python-json-logger`).

### 3.3 Beat

- Replace `django_celery_beat.schedulers.DatabaseScheduler` (`plane/celery.py:118`) with `tokio-cron-scheduler` + DB poll `SELECT * FROM periodic_tasks WHERE enabled AND next_run <= now()`.
- Keep same crontab: `*/5` email stack, `0 0 * * *` hard_delete, `0 1 * * *` archive, etc. Config via `METRICS_PUSH_INTERVAL_MINUTES` env (`plane/celery.py:26`).

### 3.4 Common / DB

- `sqlx prepare` from live PG — `cargo sqlx prepare` checks `SELECT` at compile time.
- Migrations: `sqlx migrate` reusing `plane/db/migrations/*.py` SQL extracted (no Django ORM). Keep `django_migrations` table for backward-compat during strangler.
- Pool: `PgPoolOptions::new().max_connections(10).min_connections(2)` vs Django `max_connections=1000` (`docker-compose.yml:112` → tune to 100).

## 4. Data Flow

- **API request:** Proxy → Axum extractor → `sqlx` query → `serde_json` response (same shape as DRF). Latency path no Python GIL.
- **Async job:** API handler `XADD` to Stream (was `task.delay()`), worker `XREADGROUP` → handler → `XACK`, beat pushes scheduled jobs same path.
- **File/S3:** `aws-sdk-rust` replaces `boto3`/`django-storages`; presigned URLs kept.

## 5. Migration — Strangler Fig Incremental (Solo)

| Phase              | Duration  | Deliverable                                                                                                                                                         | Memory Impact                                           |
| ------------------ | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| 0 Bootstrap        | 1w        | `apps/api-rs` workspace, `docker-compose.yml` add `rust-api:8001`, `rust-worker`, `rust-beat`, proxy route `/health`→Rust, `sqlx prepare`, CI contract test harness | baseline                                                |
| 1 Worker-First     | 3-4w      | 40 handlers → Rust worker+beat, switch `XADD` in Django to Redis Stream, `worker` 214→~20 MiB, `beat` 135→~15 MiB, delete `plane-mq`                                | -250 MiB                                                |
| 2 API Domain Slice | 2w/domain | Strangler order: `workspace`→`project`→`issue`→`cycle/module`→`auth/user`→remaining. Each: copy DRF view logic, add Axum route, proxy 10%→100%, contract test green | -30 MiB per domain, Django `GUNICORN_WORKERS` step-down |
| 3 DB Kajian        | 1w        | Benchmark: keep schema vs `jsonb` merge for `issue.properties`, GIN index, remove `django_celery_beat` tables; decision doc                                         | optional -5 MiB                                         |
| 4 Cutover          | 1w        | Proxy 100% Rust, remove `apps/api` image, rename `rust-api`→`api` in `docker-compose.yml:37`, `docker stats` <150 MiB verified                                      | target met                                              |

Rollback: proxy flip env `LEGACY_FALLBACK=1` routes back to Django in <30s.

## 6. Memory Efficiency Measures (<150 MiB)

- `jemalloc` + `MALLOC_CONF=dirty_decay_ms:1000`
- Release LTO, `strip`, `opt-level=3`
- Tokio `worker_threads = num_cpus`, pool 10 not 1000
- Remove `plane-mq` (-72 MiB)
- Single static binary vs Python per-worker prefork (Gunicorn `w $GUNICORN_WORKERS`).

## 7. Testing & Verification

- **Contract:** Keep `apps/api/tests/` `pytest` + `docker compose -f docker-compose-test.yml` as black-box: `pytest` hits `proxy:80` assert JSON equal (snapshot `plane/tests/conftest_external.py` redis mock → real Redis).
- **Unit:** `cargo test` + `sqlx::test` with `#[sqlx::test]`.
- **Perf/Mem:** `docker stats --no-stream --format` in CI, assert total <150 MiB; `cargo bloat`, `heaptrack`.
- **Observability:** `tracing` + `tracing-subscriber` json (replace `python-json-logger`), OpenTelemetry `opentelemetry-sdk` kept (`requirements/base.txt:66`).

## 8. Risks & Mitigations

- **DRF serializer parity** (e.g., `IssueSerializer` silently drops invalid assignees `#9526`) → snapshot tests for every endpoint before cut.
- **SQLx compile-time** needs live DB → `sqlx prepare` committed, CI with `plane-db`.
- **Solo bandwidth** → strict phase gating, no parallel domains.

## 9. Future Spec Self-Review Checklist

- [x] No TBD/TODO
- [x] Architecture matches strangler + Redis Stream
- [x] Scope decomposed (phases) for solo
- [x] No contradictions (keep schema default + kajian optional)

---

**Approval:** User 2026-09-04: hybrid A+B, Redis Stream, incremental solo.

**Next:** `writing-plans` skill to produce `docs/superpowers/plans/2026-09-04-rust-rewrite-plan.md` with per-phase tasks, file list, verification commands.
