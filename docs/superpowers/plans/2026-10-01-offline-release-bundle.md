# Offline Release Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a versioned offline release bundle (`release/<version>/` with docker tars + prod compose + env template + load/install scripts).

**Architecture:** Canonical release files live once at `release/` root (`docker-compose.prod.yml`, `.env.prod.example`, `load.sh`, `install.sh`, `README.md`); `scripts/release.sh <version>` builds/pins 10 images, `docker save`s each to `release/<version>/tars/` plus `sha256sums.txt`, and copies the canonical files in. `release/*/tars/` is gitignored. Prod flow: copy dir → `load.sh` → fill `.env.prod` → `install.sh`.

**Tech Stack:** Bash, Docker Compose v2, `docker save/load`, sha256sum, shellcheck.

---

### Task 1: Release skeleton (canonical files + gitignore)

**Files:**

- Modify: `.gitignore`
- Create: `release/docker-compose.prod.yml`
- Create: `release/.env.prod.example`
- Create: `release/load.sh`
- Create: `release/install.sh`
- Create: `release/README.md`

- [ ] **Step 1: Add tars to .gitignore**

Run: `tail -20 .gitignore`
Expected: no `release/` entry yet.

Edit `.gitignore`, append:

```gitignore
# Offline release bundles (built tars, never commit)
release/*/tars/
release/*/.env.prod
```

- [ ] **Step 2: Create `release/docker-compose.prod.yml` (image-only, no build:)**

```yaml
services:
  plane-db:
    image: postgres:15.7-alpine
    restart: always
    command: postgres -c 'max_connections=100'
    volumes:
      - pgdata:/var/lib/postgresql/data
    env_file:
      - .env.prod
    environment:
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_DB: ${POSTGRES_DB}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      PGDATA: /var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER:-plane} -d ${POSTGRES_DB:-plane}"]
      interval: 5s
      timeout: 5s
      retries: 10

  plane-redis:
    image: valkey/valkey:7.2.11-alpine
    restart: always
    volumes:
      - redisdata:/data
    healthcheck:
      test: ["CMD", "valkey-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 10

  plane-minio:
    image: minio/minio:latest
    restart: always
    command: server /export --console-address ":9090"
    volumes:
      - uploads:/export
    environment:
      MINIO_ROOT_USER: ${AWS_ACCESS_KEY_ID}
      MINIO_ROOT_PASSWORD: ${AWS_SECRET_ACCESS_KEY}

  api:
    image: plane-api-rs:${RELEASE_VERSION:-stable}
    restart: always
    command: /usr/local/bin/api
    env_file:
      - .env.prod
    environment:
      PORT: 8000
      DATABASE_URL: postgres://plane:plane@plane-db:5432/plane
      REDIS_URL: redis://plane-redis:6379
    depends_on:
      plane-db:
        condition: service_healthy
      plane-redis:
        condition: service_healthy

  worker:
    image: plane-api-rs:${RELEASE_VERSION:-stable}
    restart: always
    command: /usr/local/bin/worker
    env_file:
      - .env.prod
    environment:
      PORT: 8001
      DATABASE_URL: postgres://plane:plane@plane-db:5432/plane
      REDIS_URL: redis://plane-redis:6379
    depends_on:
      plane-db:
        condition: service_healthy
      plane-redis:
        condition: service_healthy

  beat-worker:
    image: plane-api-rs:${RELEASE_VERSION:-stable}
    restart: always
    command: /usr/local/bin/beat
    env_file:
      - .env.prod
    environment:
      PORT: 8001
      DATABASE_URL: postgres://plane:plane@plane-db:5432/plane
      REDIS_URL: redis://plane-redis:6379
    depends_on:
      plane-db:
        condition: service_healthy
      plane-redis:
        condition: service_healthy

  migrator:
    image: plane-migrator:${RELEASE_VERSION:-stable}
    restart: "no"
    command: ./bin/docker-entrypoint-migrator.sh
    env_file:
      - .env.prod
    depends_on:
      plane-db:
        condition: service_healthy
      plane-redis:
        condition: service_healthy

  web:
    image: plane-web:${RELEASE_VERSION:-stable}
    restart: always
    depends_on:
      - api

  admin:
    image: plane-admin:${RELEASE_VERSION:-stable}
    restart: always
    depends_on:
      - api
      - web

  space:
    image: plane-space:${RELEASE_VERSION:-stable}
    restart: always
    depends_on:
      - api
      - web

  live:
    image: plane-live:${RELEASE_VERSION:-stable}
    restart: always

  proxy:
    image: plane-proxy:${RELEASE_VERSION:-stable}
    restart: always
    ports:
      - ${LISTEN_HTTP_PORT:-80}:80
      - ${LISTEN_HTTPS_PORT:-443}:443
    environment:
      FILE_SIZE_LIMIT: ${FILE_SIZE_LIMIT:-5242880}
      BUCKET_NAME: ${AWS_S3_BUCKET_NAME:-uploads}
    depends_on:
      - web
      - api
      - space
      - admin

volumes:
  pgdata:
  redisdata:
  uploads:
```

- [ ] **Step 3: Create `release/.env.prod.example`**

```bash
# Copy to .env.prod and fill every CHANGE-ME before install.
RELEASE_VERSION=stable

# Domain / URLs (public prod values)
WEB_URL=https://CHANGE-ME
CORS_ALLOWED_ORIGINS=https://CHANGE-ME
SITE_ADDRESS=:80
CERT_EMAIL=
LISTEN_HTTP_PORT=80
LISTEN_HTTPS_PORT=443

# Database
POSTGRES_USER=plane
POSTGRES_PASSWORD=CHANGE-ME
POSTGRES_DB=plane

# Backend (api-rs reads apps/api/.env keys; keep names identical)
SECRET_KEY=CHANGE-ME-generate-with-openssl-rand-hex-32
DATABASE_URL=postgresql://plane:CHANGE-ME@plane-db:5432/plane
REDIS_URL=redis://plane-redis:6379/
WEB_URL_API=http://api:8000

# Storage (minio bundled)
USE_MINIO=1
AWS_REGION=
AWS_ACCESS_KEY_ID=CHANGE-ME
AWS_SECRET_ACCESS_KEY=CHANGE-ME
AWS_S3_ENDPOINT_URL=http://plane-minio:9000
AWS_S3_BUCKET_NAME=uploads
FILE_SIZE_LIMIT=5242880

# Frontend bake-time (VITE_* are baked at docker build, NOT runtime).
# Changing these requires rebuilding web/admin/space images with:
#   VITE_API_BASE_URL=https://CHANGE-ME/api VITE_ADMIN_BASE_URL=... scripts/release.sh <new-version>
VITE_API_BASE_URL=https://CHANGE-ME/api
VITE_ADMIN_BASE_URL=https://CHANGE-ME/god-mode
VITE_SPACE_BASE_URL=https://CHANGE-ME/spaces
VITE_LIVE_BASE_URL=https://CHANGE-ME/live
```

- [ ] **Step 4: Create `release/load.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ ! -f sha256sums.txt ]]; then
  echo "sha256sums.txt not found" >&2
  exit 1
fi
sha256sum -c sha256sums.txt
for tar in tars/*.tar; do
  echo "Loading $tar..."
  docker load -i "$tar"
done
echo "All images loaded."
```

Run: `chmod +x release/load.sh && bash -n release/load.sh && echo OK`
Expected: `OK`

- [ ] **Step 5: Create `release/install.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ ! -f .env.prod ]]; then
  echo "Copy .env.prod.example to .env.prod and fill it first." >&2
  exit 1
fi
if grep -q "CHANGE-ME" .env.prod; then
  echo "Unfilled CHANGE-ME values remain in .env.prod" >&2
  exit 1
fi
docker compose --env-file .env.prod -f docker-compose.prod.yml up -d plane-db plane-redis plane-minio
docker compose --env-file .env.prod -f docker-compose.prod.yml up -d api worker beat-worker web admin space live proxy
docker compose --env-file .env.prod -f docker-compose.prod.yml run --rm migrator
echo "Install complete."
```

Run: `chmod +x release/install.sh && bash -n release/install.sh && echo OK`
Expected: `OK`

- [ ] **Step 6: Create `release/README.md`**

```markdown
# Offline release bundle

Built by `scripts/release.sh <version>` from the repo root.

## Deploy (prod, offline)

1. Copy `release/<version>/` to the server (USB).
2. `./load.sh` — verifies sha256 and `docker load`s every tar.
3. `cp .env.prod.example .env.prod`, fill all `CHANGE-ME`.
4. `./install.sh` — starts infra + app, runs migrator once.

## Notes

- `worker`/`beat-worker` reuse the `plane-api-rs` image (no separate tar).
- `VITE_*` frontend vars are baked at build time; changing the domain needs a rebuild.
- Never commit `tars/` or `.env.prod`.
```

- [ ] **Step 7: Commit skeleton**

```bash
git add .gitignore release/docker-compose.prod.yml release/.env.prod.example release/load.sh release/install.sh release/README.md
git commit -m "feat(release): offline bundle skeleton"
```

---

### Task 2: `scripts/release.sh` build + bundle script

**Files:**

- Create: `scripts/release.sh`

- [ ] **Step 1: Write the script**

```bash
#!/usr/bin/env bash
# Build all images and assemble release/<version>/ for offline deploy.
# Usage: scripts/release.sh <version>  (e.g. scripts/release.sh v1.0.0)
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: scripts/release.sh <version>" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/release/$VERSION"
mkdir -p "$OUT/tars"

# 1. Build app images (frontend args baked from release/.env.prod.example overrides if exported)
docker build -f "$ROOT/apps/web/Dockerfile.web" -t "plane-web:$VERSION" -t plane-web:stable "$ROOT"
docker build -f "$ROOT/apps/admin/Dockerfile.admin" -t "plane-admin:$VERSION" -t plane-admin:stable "$ROOT"
docker build -f "$ROOT/apps/space/Dockerfile.space" -t "plane-space:$VERSION" -t plane-space:stable "$ROOT"
docker build -f "$ROOT/apps/live/Dockerfile.live" -t "plane-live:$VERSION" -t plane-live:stable "$ROOT"
docker build -f "$ROOT/apps/proxy/Dockerfile.ce" -t "plane-proxy:$VERSION" -t plane-proxy:stable "$ROOT/apps/proxy"
docker build -f "$ROOT/apps/api-rs/Dockerfile.rs" -t "plane-api-rs:$VERSION" -t plane-api-rs:stable "$ROOT/apps/api-rs"
docker build -f "$ROOT/apps/api/Dockerfile.api" -t "plane-migrator:$VERSION" -t plane-migrator:stable "$ROOT/apps/api"

# 2. Pin + pull infra images
docker pull postgres:15.7-alpine
docker pull valkey/valkey:7.2.11-alpine
docker pull minio/minio:latest

# 3. Save per-service tars (worker/beat reuse plane-api-rs image)
declare -A TARS=(
  ["plane-web:$VERSION"]="plane-web-$VERSION.tar"
  ["plane-admin:$VERSION"]="plane-admin-$VERSION.tar"
  ["plane-space:$VERSION"]="plane-space-$VERSION.tar"
  ["plane-live:$VERSION"]="plane-live-$VERSION.tar"
  ["plane-proxy:$VERSION"]="plane-proxy-$VERSION.tar"
  ["plane-api-rs:$VERSION"]="plane-api-rs-$VERSION.tar"
  ["plane-migrator:$VERSION"]="plane-migrator-$VERSION.tar"
  ["postgres:15.7-alpine"]="postgres-15.7-alpine.tar"
  ["valkey/valkey:7.2.11-alpine"]="valkey-7.2.11-alpine.tar"
  ["minio/minio:latest"]="minio.tar"
)
for img in "${!TARS[@]}"; do
  docker save -o "$OUT/tars/${TARS[$img]}" "$img"
done

# 4. Checksums + canonical files
(cd "$OUT" && sha256sum tars/*.tar > sha256sums.txt)
cp "$ROOT/release/docker-compose.prod.yml" "$OUT/docker-compose.prod.yml"
cp "$ROOT/release/.env.prod.example" "$OUT/.env.prod.example"
cp "$ROOT/release/load.sh" "$OUT/load.sh"
cp "$ROOT/release/install.sh" "$OUT/install.sh"
cp "$ROOT/release/README.md" "$OUT/README.md"

echo "Release $VERSION assembled in $OUT"
```

- [ ] **Step 2: Lint + syntax check**

Run: `chmod +x scripts/release.sh && bash -n scripts/release.sh && shellcheck scripts/release.sh release/load.sh release/install.sh && echo LINT-OK`
Expected: `LINT-OK` (install shellcheck first if missing: `apt install -y shellcheck` or skip with `bash -n` only)

- [ ] **Step 3: Dry-run compose validation (no build, no daemon changes)**

Run: `docker compose --env-file release/.env.prod.example -f release/docker-compose.prod.yml config > /dev/null && echo COMPOSE-OK`
Expected: `COMPOSE-OK`

- [ ] **Step 4: Commit script**

```bash
git add scripts/release.sh
git commit -m "feat(release): build and bundle script"
```

---

### Task 3: Docs + self-review gate

**Files:**

- Modify: `release/README.md` (only if gaps found)

- [ ] **Step 1: Spec coverage check** — open `docs/superpowers/specs/2026-10-01-offline-release-bundle-design.md`, confirm each section maps: §1 folder → Task 1 steps 1-6; §2 images/tags → Task 2 step 1 + compose Task 1 step 2; §3 env → Task 1 step 3 + install.sh validation; §4 prod flow → load.sh/install.sh. If any gap, add the missing file/content before proceeding.

- [ ] **Step 2: Placeholder scan**

Run: `grep -rn "TBD\|TODO\|FIXME\|XXX\|CHANGE-ME-broken" scripts/release.sh release/load.sh release/install.sh release/docker-compose.prod.yml | grep -v ".env.prod.example" || echo SCAN-CLEAN`
Expected: `SCAN-CLEAN` (`CHANGE-ME` is allowed only inside `.env.prod.example`)

- [ ] **Step 3: Final commit if README changed**

```bash
git status --short
git add -A && git commit -m "docs(release): finalize bundle docs" || echo "nothing to commit"
```
