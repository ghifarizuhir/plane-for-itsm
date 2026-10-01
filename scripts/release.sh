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
