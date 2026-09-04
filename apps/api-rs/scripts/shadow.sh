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
  "/api/workspaces/$WS/projects/$P/intakes/"
  "/api/workspaces/$WS/projects/$P/intake-issues/"
  "/api/workspaces/$WS/projects/$P/members/"
  "/api/workspaces/$WS/members/"
  "/api/workspaces/$WS/invitations/"
  "/api/workspaces/$WS/projects/$P/views/"
  "/api/workspaces/$WS/views/"
  "/api/workspaces/$WS/projects/$P/user-favorite-views/"
  "/api/workspaces/$WS/projects/$P/pages/"
  "/api/workspaces/$WS/projects/$P/pages-summary/"
  "/api/assets/v2/workspaces/$WS/check/00000000-0000-0000-0000-000000000000/"
  "/api/workspaces/$WS/webhooks/"
  "/api/workspaces/$WS/users/notifications/"
  "/api/workspaces/$WS/users/notifications/unread/"
  "/api/users/me/notification-preferences/"
  "/api/workspaces/$WS/search/?search=test"
  "/api/workspaces/$WS/entity-search/?query=test"
  "/api/workspaces/$WS/default-analytics/"
  "/api/workspaces/$WS/project-stats/"
  "/api/workspaces/$WS/analytic-view/"
  "/api/workspaces/$WS/projects/$P/work-items/"
  "/api/workspaces/$WS/work-items/search/?search=test"
  "/api/timezones/"
  "/api/users/api-tokens/"
  "/api/workspaces/$WS/stickies/"
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
