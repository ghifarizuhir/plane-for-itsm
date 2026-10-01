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
