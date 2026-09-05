#!/usr/bin/env bash
# Live functional smoke for the Rust API (post rust-cutover-v1).
# Exercises reads + writes end-to-end, then cleans up created rows.
# Requires: stack up (api on 8000), a valid token in api_tokens.
# Usage: TOKEN=plane_api_... bash apps/api-rs/scripts/smoke.sh
#
# Auth smoke (Task 9 + email-check): 10 cek siklus email-check/login/me/refresh/
# oauth-start, setelah writes, sebelum cleanup. Kredensial dari env:
#   SMOKE_EMAIL / SMOKE_PASSWORD (JANGAN di-commit).
# Bila keduanya unset, cek auth DI-SKIP (bukan fail).
# Buat user smoke sekali via SQL (hash format Django
# `pbkdf2_sha256$1000000$salt$b64`, 1M iterasi ~1 dtk di python):
#   HASH=$(python3 -c "import hashlib,base64,secrets; s=secrets.token_hex(6); \
#     h=hashlib.pbkdf2_hmac('sha256',b'GANTI-PASSWORD',s.encode(),1000000); \
#     print('pbkdf2_sha256\$1000000\$'+s+'\$'+base64.b64encode(h).decode())")
#   docker exec plane-db psql -U plane -d plane -c "INSERT INTO users (id, email, \
#     username, password, first_name, last_name, display_name, avatar, date_joined, \
#     token, user_timezone, last_location, created_location, last_login_ip, \
#     last_logout_ip, last_login_medium, last_login_uagent, is_active, is_staff, \
#     is_superuser, is_managed, is_password_expired, is_email_verified, \
#     is_password_autoset, is_bot, is_email_valid, is_password_reset_required, \
#     created_at, updated_at) VALUES (gen_random_uuid(), 'smoke@example.com', \
#     'smoke', '$HASH', '', '', 'smoke', '', now(), '', 'UTC', '', '', '', '', \
#     'password', '', true, false, false, false, false, true, false, false, true, \
#     false, now(), now());"
set -u
BASE="${BASE:-http://127.0.0.1:8000}"
TOKEN="${TOKEN:?set TOKEN to a valid api_tokens.token value}"
# Harus SAMA dengan FRONTEND_URL server: middleware origin menolak semua mutasi
# (POST/PATCH/PUT/DELETE) tanpa `Origin:` yang cocok → 403 {"error":"bad origin"}.
FRONTEND="${FRONTEND:-http://localhost:3000}"
H=(-s -m 10 -H "X-Api-Key: $TOKEN" -H 'Content-Type: application/json' -H "Origin: $FRONTEND")
PASS=0; FAIL=0; FAILED=""
SFX="smoke$RANDOM"

NAH=(-s -m 10 -H 'Content-Type: application/json' -H "Origin: $FRONTEND")
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
check_auth() { # check_auth <label> <expected_status> <curl_args...> — murni cookie sesi, TANPA X-Api-Key
  # (AuthUser fallback ke X-Api-Key: memakai check biasa membuat post-logout-401
  # mustahil karena api key tetap valid setelah logout).
  local label="$1" want="$2"; shift 2
  local code
  code=$(curl "${NAH[@]}" -o /tmp/smoke_body -w '%{http_code}' "$@")
  if [ "$code" = "$want" ]; then PASS=$((PASS+1)); echo "ok   $label -> $code";
  else FAIL=$((FAIL+1)); FAILED="$FAILED $label($code)"; echo "FAIL $label -> $code want $want: $(head -c 200 /tmp/smoke_body)"; fi
}

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

echo "== auth =="
if [ -z "${SMOKE_EMAIL:-}" ] || [ -z "${SMOKE_PASSWORD:-}" ]; then
  echo "skip auth checks (SMOKE_EMAIL/SMOKE_PASSWORD unset)"
else
  JAR=/tmp/smoke_jar
  rm -f "$JAR"
  # Budget rate-limit IP 5 hit/mnt (router terbatas): email-check(1) +
  # login(2) + login-bad(3) + forgot(4) + magic(5). csrf unlimited.
  # email-check-bad dihapus dari smoke (dilindungi unit test email_valid).
  check_auth email-check-200 200 -X POST -d "{\"email\":\"$SMOKE_EMAIL\"}" "$BASE/auth/email-check/"
  grep -q '"existing":true' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   email-check-body -> existing:true"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED email-check-body"; echo "FAIL email-check-body: $(head -c 200 /tmp/smoke_body)"; }
  check_auth csrf-200 200 "$BASE/auth/get-csrf-token/"
  check_auth forgot-smtp-400 400 -X POST -d "{\"email\":\"$SMOKE_EMAIL\"}" "$BASE/auth/forgot-password/"
  check_auth magic-smtp-400 400 -X POST -d "{\"email\":\"$SMOKE_EMAIL\"}" "$BASE/auth/magic-generate/"
  check_auth login-200 200 -c "$JAR" -X POST -d "{\"email\":\"$SMOKE_EMAIL\",\"password\":\"$SMOKE_PASSWORD\"}" "$BASE/api/auth/login/"
  check_auth me-200 200 -b "$JAR" "$BASE/api/users/me/"
  check_auth refresh-200 200 -c "$JAR" -b "$JAR" -X POST "$BASE/api/auth/refresh/"
  # NOTE: logout memakai -c + -b (menyimpang dari cuplikan plan Task 9 yang hanya
  # -b): tanpa -c, jar tidak menyimpan Set-Cookie clear sehingga post-logout-401
  # tetap mengirim cookie lama dan gagal (access JWT stateless masih valid 15 mnt).
  check_auth logout-200 200 -c "$JAR" -b "$JAR" -X POST "$BASE/api/auth/logout/"
  check_auth post-logout-401 401 -b "$JAR" "$BASE/api/users/me/"
  check_auth login-bad-401 401 -X POST -d "{\"email\":\"$SMOKE_EMAIL\",\"password\":\"__bad__smoke__\"}" "$BASE/api/auth/login/"
  check oauth-start-302 302 "$BASE/api/auth/oauth/github/start/"
  rm -f "$JAR"
fi

echo "== cleanup =="
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM api_tokens WHERE label = 'smoke2';" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM projects WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE 'smoke-%'); DELETE FROM workspaces WHERE slug LIKE 'smoke-%';" 2>&1 | head -n 1
echo "PASS=$PASS FAIL=$FAIL"; [ -n "$FAILED" ] && echo "failed:$FAILED"
[ "$FAIL" = 0 ]
