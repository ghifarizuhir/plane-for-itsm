# Offline Release Bundle Design (Docker tar + prod env)

Date: 2026-10-01. Scope: paket release offline untuk deploy prod (airgapped).

## Context

Repo punya `docker-compose.{infra,backend,frontend}.yml` terpisah, `deployments/` upstream
(aio/cli/swarm/kubernetes) masih asumsi Django + RabbitMQ, belum cocok untuk api-rs.
Belum ada artefak `.tar` — hanya build output lokal (`apps/*/build`) dan image
docker lokal yang tua/parsial. Target: satu folder `release/<version>/` versioned
yang bisa dicopy via USB ke server prod offline.

Keputusan user: offline bawa `.tar`, prod saja (tanpa staging), bundel app + infra.

## Design

### 1. Struktur folder

```text
release/<version>/
  tars/                    # hasil docker save, di-gitignore
  docker-compose.prod.yml  # standalone, image-only, tanpa build:
  .env.prod.example        # template env prod
  load.sh                  # sha256 check + docker load
  install.sh               # validasi env + up -d + migrator sekali
  README.md                # cara build, copy, deploy, ganti domain
```

`release/*/tars/` di-gitignore. Yang dicommit hanya script, compose, template env, README.

### 2. Build & bundle (`scripts/release.sh <version>`)

- Build 7 image app, tag `plane-<svc>:<version>` + `:stable`:
  - `web` ← `apps/web/Dockerfile.web`
  - `admin` ← `apps/admin/Dockerfile.admin`
  - `space` ← `apps/space/Dockerfile.space`
  - `live` ← `apps/live/Dockerfile.live`
  - `proxy` ← `apps/proxy/Dockerfile.ce`
  - `api-rs` ← `apps/api-rs/Dockerfile.rs` (dipakai juga oleh `worker`, `beat-worker` via `command` override — tanpa tar sendiri)
  - `migrator` ← `apps/api/Dockerfile.api`
- Pull + pin infra: `postgres:15.7-alpine`, `valkey/valkey:7.2.11-alpine`, `minio/minio:<pin>`.
- `docker save` per-service ke `tars/plane-<svc>-<version>.tar` + `sha256sums.txt`.
- Estimasi ukuran ~3-4 GB. Script fail-fast, akhiri dengan trial `docker load` di mesin build.
- Build-args frontend (`VITE_*`) dicatat di README — terikat ke domain prod saat build.

### 3. Env / properties (`.env.prod.example`)

Satu file gabungan `.env.example` + `apps/api/.env` yang relevan untuk prod:
domain/URL (`WEB_URL`, `CORS_ALLOWED_ORIGINS`), secret (`SECRET_KEY`,
`POSTGRES_PASSWORD`), storage (`AWS_*`, `BUCKET_NAME`, `FILE_SIZE_LIMIT`).
Operator copy ke `.env.prod`, isi manual, tidak pernah dicommit.
`install.sh` menolak lanjut bila required key kosong/default.
Catatan eksplisit: ganti domain frontend (`VITE_*`) = rebuild image, bukan sekadar ganti env.

### 4. Prod compose + alur deploy

`docker-compose.prod.yml`: semua service `image: plane-<svc>:<version>`,
`restart: always`, satu network `plane-net`, healthcheck DB/redis,
hanya `proxy` expose 80/443, volume data `pgdata`/`redisdata`/`uploads`
(tanpa mount source-code).

Alur di prod:

1. Copy folder `release/<version>/` ke server.
2. `./load.sh` — verifikasi sha256 lalu `docker load` semua tar.
3. `cp .env.prod.example .env.prod`, isi nilai prod.
4. `./install.sh` — validasi env → `docker compose --env-file .env.prod up -d` → jalankan `migrator` sekali.

### 5. Non-goals

Tanpa staging env, tanpa registry push, tanpa DB seed/dump (migrasi dari nol via
`migrator`), tanpa ubah `deployments/` upstream.

## Alternatives considered

- **B. Reuse `deployments/cli/community/`**: konsisten upstream tapi script masih
  Django+RabbitMQ — rombak besar, ditolak.
- **C. Satu monolith tar**: satu file raksasa, gagal copy = ulang semua, tanpa
  checksum per-service — ditolak.
