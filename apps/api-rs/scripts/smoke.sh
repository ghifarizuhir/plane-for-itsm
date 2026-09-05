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
check me-workspaces-200 200 "$BASE/api/users/me/workspaces/"
check ws-invitations-200 200 "$BASE/api/users/me/workspaces/invitations/"
check proj-create 201 -X POST -d '{"name":"Smoke Proj","identifier":"SMP"}' "$BASE/api/workspaces/$WS/projects/"
PID=$(jid id)
check project-roles-200 200 "$BASE/api/users/me/workspaces/$WS/project-roles/"
check state-create 201 -X POST -d '{"name":"Smoke State","group":"backlog","color":"#ff0000"}' "$BASE/api/workspaces/$WS/projects/$PID/states/"
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

echo "== batch-C =="
check proj-details-200 200 "$BASE/api/workspaces/$WS/projects/details/"
check identifiers-200 200 "$BASE/api/workspaces/$WS/project-identifiers/?name=ZZZUNUSED"
check identifiers-400 400 "$BASE/api/workspaces/$WS/project-identifiers/"
check fav-add-204 204 -X POST -d "{\"project\":\"$PID\"}" "$BASE/api/workspaces/$WS/user-favorite-projects/"
check fav-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/user-favorite-projects/$PID/"
check fav-del-404 404 -X DELETE "$BASE/api/workspaces/$WS/user-favorite-projects/$PID/"
check archive-post-200 200 -X POST "$BASE/api/workspaces/$WS/projects/$PID/archive/"
check archive-restore-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/archive/"
check members-me-200 200 "$BASE/api/workspaces/$WS/projects/$PID/project-members/me/"
check mark-default-204 204 -X POST "$BASE/api/workspaces/$WS/projects/$PID/states/$SID/mark-default/"
check issues-list-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/list/?issues=$IID"
check issues-list-400 400 "$BASE/api/workspaces/$WS/projects/$PID/issues/list/"
check issues-detail-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues-detail/"
grep -q '"total_count"' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   issues-detail-envelope -> total_count"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED issues-detail-envelope"; echo "FAIL issues-detail-envelope: $(head -c 200 /tmp/smoke_body)"; }
check bulk-del-400 400 -X DELETE -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/bulk-delete-issues/"
check bulk-tmp-create 201 -X POST -d '{"name":"Bulk tmp"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
BID=$(jid id)
check bulk-del-200 200 -X DELETE -d "{\"issue_ids\":[\"$BID\"]}" "$BASE/api/workspaces/$WS/projects/$PID/bulk-delete-issues/"
grep -q 'issues were deleted' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   bulk-del-body -> message"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED bulk-del-body"; echo "FAIL bulk-del-body: $(head -c 200 /tmp/smoke_body)"; }
check bulk-archive-400 400 -X POST -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/bulk-archive-issues/"
check archived-200 200 "$BASE/api/workspaces/$WS/projects/$PID/archived-issues/"
check deleted-200 200 "$BASE/api/workspaces/$WS/projects/$PID/deleted-issues/"
check sub-get-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/sub-issues/"
# token user is SOLE admin of smoke project -> leave yields 400 (guard proof), not 204; LAST before auth
check leave-400 400 -X POST "$BASE/api/workspaces/$WS/projects/$PID/members/leave/"
grep -q 'only admin' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   leave-body -> sole-admin"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED leave-body"; echo "FAIL leave-body: $(head -c 200 /tmp/smoke_body)"; }

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
  # generate-code invalid gagal di validasi SEBELUM throttle Redis → tak makan budget.
  # Pakai check (X-Api-Key) karena endpoint butuh AuthUser; check_auth tanpa kredensial → 401.
  check email-gen-invalid-400 400 -X POST -d '{"email":"x"}' "$BASE/api/users/me/email/generate-code/"
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

echo "== batch-D =="
check sub-status-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check sub-add-201 201 -X POST "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check sub-dup-400 400 -X POST "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check sub-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/subscribe/"
check subscribers-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/issue-subscribers/"
check history-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/history/"
check meta-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/meta/"
check intake-state-200 200 "$BASE/api/workspaces/$WS/projects/$PID/intake-state/"
check ws-states-200 200 "$BASE/api/workspaces/$WS/states/"
check userprops-get-200 200 "$BASE/api/workspaces/$WS/projects/$PID/user-properties/"
# D5: empty `updates` -> 200 per contract, so the 400 pin uses a missing-`id`
# entry -> 400 {"error":"The required key does not exist."} (base.py:194-198).
check dates-400 400 -X POST -d '{"updates":[{}]}' "$BASE/api/workspaces/$WS/projects/$PID/issue-dates/"
check versions-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/versions/"
check descver-200 200 "$BASE/api/workspaces/$WS/projects/$PID/work-items/$IID/description-versions/"
DSID=$(curl -s -H "X-Api-Key: $TOKEN" -H "Origin: $FRONTEND" "$BASE/api/workspaces/$WS/projects/$PID/states/" | python3 -c "import json,sys; print([s['id'] for s in json.load(sys.stdin) if s['group']=='completed'][0])")
check arch1-tmp-create 201 -X POST -d "{\"name\":\"Arch1 tmp\",\"state_id\":\"$DSID\"}" "$BASE/api/workspaces/$WS/projects/$PID/issues/"
AID=$(jid id)
check arch1-post-200 200 -X POST "$BASE/api/workspaces/$WS/projects/$PID/issues/$AID/archive/"
check arch1-get-200 200 "$BASE/api/workspaces/$WS/projects/$PID/issues/$AID/archive/"
check arch1-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/issues/$AID/archive/"
check react-add-201 201 -X POST -d '{"reaction":"heart"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/reactions/"
check react-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/reactions/heart/"
check rel-a-create 201 -X POST -d '{"name":"Rel A"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
RA=$(jid id)
check rel-b-create 201 -X POST -d '{"name":"Rel B"}' "$BASE/api/workspaces/$WS/projects/$PID/issues/"
RB=$(jid id)
# work_item.rs::create_relations takes {issues:[uuid], relation_type} (NOT related_issue).
check relate-201 201 -X POST -d "{\"issues\":[\"$RB\"],\"relation_type\":\"relates_to\"}" "$BASE/api/workspaces/$WS/projects/$PID/issues/$RA/relations/"
check remrel-204 204 -X POST -d "{\"related_issue\":\"$RB\"}" "$BASE/api/workspaces/$WS/projects/$PID/issues/$RA/remove-relation/"
check draft-create-201 201 -X POST -d "{\"name\":\"Draft tmp\",\"project_id\":\"$PID\"}" "$BASE/api/workspaces/$WS/draft-issues/"
DID=$(jid id)
check draft-get-200 200 "$BASE/api/workspaces/$WS/draft-issues/$DID/"
check draft-patch-204 204 -X PATCH -d '{"name":"Draft tmp2"}' "$BASE/api/workspaces/$WS/draft-issues/$DID/"
check draft-del-204 204 -X DELETE "$BASE/api/workspaces/$WS/draft-issues/$DID/"
check draft-create2-201 201 -X POST -d "{\"name\":\"Draft conv\",\"project_id\":\"$PID\"}" "$BASE/api/workspaces/$WS/draft-issues/"
DID2=$(jid id)
# draft.rs::create_draft_to_issue requires a `name` (missing -> 400).
check draft-to-issue-201 201 -X POST -d '{"name":"Draft conv issue"}' "$BASE/api/workspaces/$WS/draft-to-issue/$DID2/"
check ws-labels-200 200 "$BASE/api/workspaces/$WS/labels/"
check label-create-201 201 -X POST -d '{"name":"SmokeLbl","color":"#ff0000"}' "$BASE/api/workspaces/$WS/projects/$PID/issue-labels/"
check label-dup-400 400 -X POST -d '{"name":"SmokeLbl","color":"#ff0000"}' "$BASE/api/workspaces/$WS/projects/$PID/issue-labels/"
check ws-issues-200 200 "$BASE/api/workspaces/$WS/issues/"
check v2-issues-200 200 "$BASE/api/workspaces/$WS/projects/$PID/v2/issues/"
MUID=$(curl -s -H "X-Api-Key: $TOKEN" -H "Origin: $FRONTEND" "$BASE/api/users/me/" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
check user-issues-200 200 "$BASE/api/workspaces/$WS/user-issues/$MUID/"
# intake.rs::create takes {name} (SFX-suffixed: plain "Smoke intake" already
# exists from == writes == -> 409); create_issue takes {issue:{name}} (no intake_id key).
check inbox-intake-create 201 -X POST -d "{\"name\":\"Smoke intake $SFX\"}" "$BASE/api/workspaces/$WS/projects/$PID/intakes/"
INKID=$(jid id)
check inbox-issue-create 201 -X POST -d '{"issue":{"name":"Smoke inbox issue"}}' "$BASE/api/workspaces/$WS/projects/$PID/intake-issues/"
INISSUE=$(jid id)
# intake.rs::InboxIssuePatch takes nested {issue:{name}} (top-level name ignored).
check inbox-patch-200 200 -X PATCH -d '{"issue":{"name":"Smoke inbox renamed"}}' "$BASE/api/workspaces/$WS/projects/$PID/inbox-issues/$INISSUE/"
check fallback-404 404 "$BASE/api/workspaces/$WS/no-such-path-here/"
grep -q 'Page not found' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   fallback-body -> Page not found"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED fallback-body"; echo "FAIL fallback-body: $(head -c 200 /tmp/smoke_body)"; }

echo "== cleanup =="
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM api_tokens WHERE label = 'smoke2';" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM projects WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE 'smoke-%'); DELETE FROM workspaces WHERE slug LIKE 'smoke-%';" 2>&1 | head -n 1
echo "PASS=$PASS FAIL=$FAIL"; [ -n "$FAILED" ] && echo "failed:$FAILED"
[ "$FAIL" = 0 ]
