#!/usr/bin/env bash
# Live functional smoke for the Rust API (post rust-cutover-v1).
# Exercises reads + writes end-to-end, then cleans up created rows.
# Requires: stack up (api on 8000), a valid token in api_tokens.
# Usage: TOKEN=plane_api_... bash apps/api-rs/scripts/smoke.sh
set -u
BASE="${BASE:-http://127.0.0.1:8000}"
TOKEN="${TOKEN:?set TOKEN to a valid api_tokens.token value}"
H=(-s -m 10 -H "X-Api-Key: $TOKEN" -H 'Content-Type: application/json')
PASS=0; FAIL=0; FAILED=""
SFX="smoke$RANDOM"

NAH=(-s -m 10 -H 'Content-Type: application/json')
check() { # check <label> <expected_status> <curl_args...>
  local label="$1" want="$2"; shift 2
  local code body args=("${H[@]}")
  case "$label" in noauth-401) args=("${NAH[@]}");; esac
  body=$(curl "${args[@]}" -o /tmp/smoke_body -w '%{http_code}' "$@")
  code="$body"
  if [ "$code" = "$want" ]; then PASS=$((PASS+1)); echo "ok   $label -> $code";
  else FAIL=$((FAIL+1)); FAILED="$FAILED $label($code)"; echo "FAIL $label -> $code want $want: $(head -c 200 /tmp/smoke_body)"; fi
}
jid() { python3 -c "import json,sys; d=json.load(open('/tmp/smoke_body')); v=d.get('$1','') if isinstance(d,dict) else ''; print(v)" 2>/dev/null; }

echo "== reads =="
check health 200 "$BASE/health"
check ws-list 200 "$BASE/api/workspaces/"
check noauth-401 401 "$BASE/api/workspaces/"

echo "== writes =="
check ws-create 201 -X POST -d "{\"name\":\"Smoke $SFX\",\"slug\":\"smoke-$SFX\"}" "$BASE/api/workspaces/"
WS="smoke-$SFX"
check ws-detail 200 "$BASE/api/workspaces/$WS/"
check proj-create 201 -X POST -d '{"name":"Smoke Proj","identifier":"SMP"}' "$BASE/api/workspaces/$WS/projects/"
PID=$(jid id)
check state-create 201 -X POST -d '{"name":"Todo","group":"backlog","color":"#ff0000"}' "$BASE/api/workspaces/$WS/projects/$PID/states/"
SID=$(jid id)
check issue-create 201 -X POST -d '{"name":"Smoke issue"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
IID=$(jid id)
check issue-detail 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/"
check cycle-create 201 -X POST -d '{"name":"Smoke cycle"}' "$BASE/api/workspaces/$WS/projects/$PID/cycles/"
check module-create 201 -X POST -d '{"name":"Smoke module"}' "$BASE/api/workspaces/$WS/projects/$PID/modules/"
check label-create 201 -X POST -d '{"name":"Smoke label"}' "$BASE/api/workspaces/$WS/projects/$PID/labels/"
check view-create 201 -X POST -d '{"name":"Smoke view"}' "$BASE/api/workspaces/$WS/projects/$PID/views/"
check page-create 201 -X POST -d '{"name":"Smoke page"}' "$BASE/api/workspaces/$WS/projects/$PID/pages/"
check intake-create 201 -X POST -d '{"name":"Smoke intake"}' "$BASE/api/workspaces/$WS/projects/$PID/intakes/"
check estimate-create 201 -X POST -d '{"name":"Smoke est","type":"points"}' "$BASE/api/workspaces/$WS/projects/$PID/estimates/"
EID=$(jid id)
check estimate-point 201 -X POST -d '{"key":1,"value":"1"}' "$BASE/api/workspaces/$WS/projects/$PID/estimates/$EID/estimate-points/"
check sticky-create 201 -X POST -d '{"name":"Smoke sticky","color":"#ff0000"}' "$BASE/api/workspaces/$WS/stickies/"
check invite-create 201 -X POST -d '{"email":"smoke@example.com","role":15}' "$BASE/api/workspaces/$WS/invitations/"
check token-create 201 -X POST -d '{"label":"smoke2"}' "$BASE/api/users/api-tokens/"
check export-create 200 -X POST -d '{"provider":"csv"}' "$BASE/api/workspaces/$WS/export-issues/"
check notif-unread 200 "$BASE/api/workspaces/$WS/users/notifications/unread/"
check search 200 "$BASE/api/workspaces/$WS/search/?query=smoke"

echo "== cleanup =="
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM api_tokens WHERE label = 'smoke2';" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM projects WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE 'smoke-%'); DELETE FROM workspaces WHERE slug LIKE 'smoke-%';" 2>&1 | head -n 1
echo "PASS=$PASS FAIL=$FAIL"; [ -n "$FAILED" ] && echo "failed:$FAILED"
[ "$FAIL" = 0 ]
