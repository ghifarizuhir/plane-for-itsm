#!/bin/bash
# Shadow parity: compare Django :8000 vs Rust :8001 for strangler paths.
# Usage: WS=<slug> P=<project_uuid> bash apps/api-rs/scripts/shadow.sh
set -e
WS=${WS:-test-ws}
P=${P:-00000000-0000-0000-0000-000000000000}
paths=(
  "/api/workspaces/"
  "/api/workspaces/$WS/projects/"
  "/api/workspaces/$WS/projects/$P/issues/"
  "/api/workspaces/$WS/projects/$P/cycles/"
  "/api/workspaces/$WS/projects/$P/modules/"
  "/api/workspaces/$WS/projects/$P/states/"
  "/api/workspaces/$WS/projects/$P/labels/"
  "/api/workspaces/$WS/projects/$P/estimates/"
)
fail=0
for path in "${paths[@]}"; do
  dj=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:8000$path" || echo "down")
  rs=$(curl -s "http://localhost:8001$path" || echo "down")
  echo "path=$path django_http=$dj rust_body=$(echo "$rs" | head -c 120)"
  if [ -z "$rs" ]; then fail=1; fi
done
# health must be ok on Rust
curl -sf http://localhost:8001/health | grep -q ok || { echo "rust /health NOT ok"; fail=1; }
[ "$fail" -eq 0 ] && echo "shadow ok" || { echo "shadow mismatch"; exit 1; }
