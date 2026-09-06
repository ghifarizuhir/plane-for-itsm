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
check invite-create 200 -X POST -d '{"email":"smoke@example.com","role":15}' "$BASE/api/workspaces/$WS/invitations/"
grep -q 'Emails sent successfully' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   invite-body -> sent-msg"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED invite-body"; echo "FAIL invite-body: $(head -c 200 /tmp/smoke_body)"; }
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

echo "== batch-E =="
CYC="$BASE/api/workspaces/$WS/projects/$PID/cycles"
MOD="$BASE/api/workspaces/$WS/projects/$PID/modules"
PG="$BASE/api/workspaces/$WS/projects/$PID/pages"
MUID2="$MUID"
MEMAIL=$(curl -s -m 10 -H "X-Api-Key: $TOKEN" -H "Origin: $FRONTEND" "$BASE/api/users/me/" | python3 -c "import json,sys; print(json.load(sys.stdin).get('email',''))" 2>/dev/null)

echo "--- E2 cycles ---"
check e2-cyc-create-past 201 -X POST -d '{"name":"E2 past","start_date":"2020-01-01","end_date":"2020-01-15"}' "$CYC/"
CYA=$(jid id)
check e2-cyc-create-future 201 -X POST -d '{"name":"E2 fut","start_date":"2030-01-01","end_date":"2030-01-15"}' "$CYC/"
CYB=$(jid id)
check e2-cyc-list 200 "$CYC/"
check e2-cyc-detail 200 "$CYC/$CYB/"
check e2-cyc-patch 200 -X PATCH -d '{"name":"E2 fut2"}' "$CYC/$CYB/"
check e2-cyc-halfdate-400 400 -X POST -d '{"name":"x","start_date":"2020-01-01"}' "$CYC/"
check e2-cyc-badorder-400 400 -X POST -d '{"name":"x","start_date":"2020-02-01","end_date":"2020-01-01"}' "$CYC/"
check e2-cyc-datecheck-free 200 -X POST -d '{"start_date":"2031-01-01","end_date":"2031-01-15"}' "$CYC/date-check/"
grep -q '"status":true' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e2-datecheck-free-body -> status:true"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e2-datecheck-free-body"; echo "FAIL e2-datecheck-free-body: $(head -c 200 /tmp/smoke_body)"; }
check e2-cyc-datecheck-hit 200 -X POST -d '{"start_date":"2030-01-05","end_date":"2030-01-10"}' "$CYC/date-check/"
grep -q '"status":false' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e2-datecheck-hit-body -> status:false"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e2-datecheck-hit-body"; echo "FAIL e2-datecheck-hit-body: $(head -c 200 /tmp/smoke_body)"; }
check e2-cyc-datecheck-missing 400 -X POST -d '{}' "$CYC/date-check/"
check e2-cyc-transfer 200 -X POST -d "{\"new_cycle_id\":\"$CYB\"}" "$CYC/$CYA/transfer-issues/"
grep -q 'Success' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e2-transfer-body -> Success"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e2-transfer-body"; echo "FAIL e2-transfer-body: $(head -c 200 /tmp/smoke_body)"; }
check e2-cyc-transfer-noid 400 -X POST -d '{}' "$CYC/$CYA/transfer-issues/"
check e2-cyc-issues-add 201 -X POST -d "{\"issues\":[\"$IID\"]}" "$CYC/$CYB/cycle-issues/"
check e2-cyc-issues-list 200 "$CYC/$CYB/cycle-issues/"
check e2-cyc-issues-grouped 400 "$CYC/$CYB/cycle-issues/?group_by=status&sub_group_by=status"
check e2-cyc-issues-del 204 -X DELETE "$CYC/$CYB/cycle-issues/$IID/"
check e2-cyc-fav-add 204 -X POST -d "{\"cycle\":\"$CYB\"}" "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-cycles/"
check e2-cyc-fav-dup 400 -X POST -d "{\"cycle\":\"$CYB\"}" "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-cycles/"
check e2-cyc-fav-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-cycles/$CYB/"
check e2-cyc-fav-del404 404 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-cycles/$CYB/"
check e2-cyc-userprops-get 200 "$CYC/$CYB/user-properties/"
check e2-cyc-userprops-patch 201 -X PATCH -d '{"filters":{}}' "$CYC/$CYB/user-properties/"
check e2-cyc-progress 200 "$CYC/$CYB/progress/"
grep -q 'total_issues' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e2-progress-body -> total_issues"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e2-progress-body"; echo "FAIL e2-progress-body: $(head -c 200 /tmp/smoke_body)"; }
check e2-cyc-analytics 200 "$CYC/$CYB/analytics/"
grep -q 'completion_chart' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e2-analytics-body -> completion_chart"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e2-analytics-body"; echo "FAIL e2-analytics-body: $(head -c 200 /tmp/smoke_body)"; }
check e2-cyc-archive-noncompleted 400 -X POST "$CYC/$CYB/archive/"
check e2-cyc-archive 200 -X POST "$CYC/$CYA/archive/"
check e2-cyc-archived-list 200 "$BASE/api/workspaces/$WS/projects/$PID/archived-cycles/"
grep -q "$CYA" /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e2-archived-body -> CYA"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e2-archived-body"; echo "FAIL e2-archived-body: $(head -c 200 /tmp/smoke_body)"; }
check e2-cyc-unarchive 204 -X DELETE "$CYC/$CYA/archive/"
check e2-cyc-del 204 -X DELETE "$CYC/$CYA/"
check e2-cyc-del2 204 -X DELETE "$CYC/$CYB/"

echo "--- E3 modules ---"
check e3-mod-create 201 -X POST -d '{"name":"E3 mod"}' "$MOD/"
MODA=$(jid id)
check e3-mod-create2 201 -X POST -d '{"name":"E3 mod2"}' "$MOD/"
MODB=$(jid id)
check e3-mod-list 200 "$MOD/"
check e3-mod-detail 200 "$MOD/$MODA/"
check e3-mod-patch 200 -X PATCH -d '{"name":"E3 modA"}' "$MOD/$MODA/"
check e3-mod-badstatus 400 -X PATCH -d '{"status":"nope"}' "$MOD/$MODA/"
check e3-mod-issues-add 201 -X POST -d "{\"issues\":[\"$IID\"]}" "$MOD/$MODA/issues/"
check e3-mod-issues-list 200 "$MOD/$MODA/issues/"
check e3-mod-issues-grouped 400 "$MOD/$MODA/issues/?group_by=status&sub_group_by=status"
check e3-mod-issue-destroy 204 -X DELETE "$MOD/$MODA/issues/$IID/"
check e3-issue-modules-add 201 -X POST -d "{\"modules\":[\"$MODA\"],\"removed_modules\":[]}" "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/modules/"
check e3-issue-modules-remove 201 -X POST -d "{\"modules\":[],\"removed_modules\":[\"$MODA\"]}" "$BASE/api/workspaces/$WS/projects/$PID/issues/$IID/modules/"
check e3-link-create 201 -X POST -d "{\"url\":\"example.com/e3-$SFX\",\"title\":\"t\"}" "$MOD/$MODA/module-links/"
LID=$(jid id)
check e3-link-list 200 "$MOD/$MODA/module-links/"
check e3-link-detail 200 "$MOD/$MODA/module-links/$LID/"
check e3-link-patch 200 -X PATCH -d '{"title":"t2"}' "$MOD/$MODA/module-links/$LID/"
check e3-link-dup 400 -X POST -d "{\"url\":\"example.com/e3-$SFX\",\"title\":\"t\"}" "$MOD/$MODA/module-links/"
check e3-link-del 204 -X DELETE "$MOD/$MODA/module-links/$LID/"
check e3-mod-fav-add 204 -X POST -d "{\"module\":\"$MODA\"}" "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-modules/"
check e3-mod-fav-dup 400 -X POST -d "{\"module\":\"$MODA\"}" "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-modules/"
check e3-mod-fav-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-modules/$MODA/"
check e3-mod-fav-del404 404 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-modules/$MODA/"
check e3-mod-userprops-get 200 "$MOD/$MODA/user-properties/"
check e3-mod-userprops-patch 201 -X PATCH -d '{"filters":{}}' "$MOD/$MODA/user-properties/"
check e3-mod-archive-draft 400 -X POST "$MOD/$MODA/archive/"
check e3-mod-complete 200 -X PATCH -d '{"status":"completed"}' "$MOD/$MODA/"
check e3-mod-archive 200 -X POST "$MOD/$MODA/archive/"
check e3-mod-archived-list 200 "$BASE/api/workspaces/$WS/projects/$PID/archived-modules/"
grep -q "$MODA" /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e3-archived-body -> MODA"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e3-archived-body"; echo "FAIL e3-archived-body: $(head -c 200 /tmp/smoke_body)"; }
check e3-mod-unarchive 204 -X DELETE "$MOD/$MODA/archive/"
check e3-mod-del 204 -X DELETE "$MOD/$MODA/"
check e3-mod-del2 204 -X DELETE "$MOD/$MODB/"

echo "--- E4 pages ---"
check e4-pages-summary 200 "$BASE/api/workspaces/$WS/projects/$PID/pages-summary/"
check e4-pages-list 200 "$PG/"
check e4-page-create 201 -X POST -d '{"name":"E4 page"}' "$PG/"
PG1=$(jid id)
check e4-page-create-tmp 201 -X POST -d '{"name":"E4 tmp"}' "$PG/"
PGX=$(jid id)
check e4-page-detail 200 "$PG/$PG1/"
check e4-page-patch 200 -X PATCH -d '{"name":"E4 page2"}' "$PG/$PG1/"
check e4-page-del-unarchived 400 -X DELETE "$PG/$PG1/"
check e4-desc-get 200 "$PG/$PG1/description/"
check e4-desc-patch 200 -X PATCH -d '{"description_html":"<p>hi</p>"}' "$PG/$PG1/description/"
grep -q '"message"' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e4-desc-body -> message"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e4-desc-body"; echo "FAIL e4-desc-body: $(head -c 200 /tmp/smoke_body)"; }
check e4-duplicate 201 -X POST "$PG/$PG1/duplicate/"
PGD=$(jid id)
check e4-lock 204 -X POST "$PG/$PG1/lock/"
check e4-unlock 204 -X DELETE "$PG/$PG1/lock/"
check e4-access 204 -X POST -d '{"access":1}' "$PG/$PG1/access/"
check e4-access-bad 400 -X POST -d '{"access":"x"}' "$PG/$PG1/access/"
check e4-fav-add 204 -X POST "$BASE/api/workspaces/$WS/projects/$PID/favorite-pages/$PG1/"
check e4-fav-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/favorite-pages/$PG1/"
check e4-versions 200 "$PG/$PG1/versions/"
check e4-archive 200 -X POST "$PG/$PG1/archive/"
check e4-unarchive 204 -X DELETE "$PG/$PG1/archive/"
check e4-tmp-archive 200 -X POST "$PG/$PGX/archive/"
check e4-tmp-del-after-archive 204 -X DELETE "$PG/$PGX/"
check e4-dup-archive 200 -X POST "$PG/$PGD/archive/"
check e4-dup-del 204 -X DELETE "$PG/$PGD/"
check e4-page-archive2 200 -X POST "$PG/$PG1/archive/"
check e4-page-del 204 -X DELETE "$PG/$PG1/"

echo "--- E5 members + invites ---"
check e5-ws-members 200 "$BASE/api/workspaces/$WS/members/"
MID=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT wm.id FROM workspace_members wm JOIN workspaces w ON w.id=wm.workspace_id WHERE w.slug='$WS' AND wm.member_id='$MUID2' AND wm.deleted_at IS NULL;" 2>/dev/null | tr -d ' \n')
check e5-ws-member-detail 200 "$BASE/api/workspaces/$WS/members/$MID/"
check e5-ws-member-selfrole 400 -X PATCH -d '{"role":10}' "$BASE/api/workspaces/$WS/members/$MID/"
check e5-ws-member-badrole 400 -X PATCH -d '{"role":"xx"}' "$BASE/api/workspaces/$WS/members/$MID/"
check e5-ws-leave-guard 400 -X POST "$BASE/api/workspaces/$WS/members/leave/"
check e5-proj-members 200 "$BASE/api/workspaces/$WS/projects/$PID/members/"
check e5-proj-bulk-empty 400 -X POST -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/members/"
PROLE=$(curl -s -m 10 -H "X-Api-Key: $TOKEN" -H "Origin: $FRONTEND" "$BASE/api/workspaces/$WS/projects/$PID/project-members/me/" | python3 -c "import json,sys; print(json.load(sys.stdin).get('role',''))" 2>/dev/null)
TUID=$(docker exec plane-db psql -U plane -d plane -t -A -c "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, avatar, date_joined, token, user_timezone, last_location, created_location, last_login_ip, last_logout_ip, last_login_medium, last_login_uagent, is_active, is_staff, is_superuser, is_managed, is_password_expired, is_email_verified, is_password_autoset, is_bot, is_email_valid, is_password_reset_required, created_at, updated_at) VALUES (gen_random_uuid(), 'temp-member-$SFX@example.com', 'tempmem$SFX', '!', '', '', 'tempmem', '', now(), '', 'UTC', '', '', '', '', 'password', '', true, false, false, false, false, true, false, false, true, false, now(), now()) RETURNING id;" 2>/dev/null | tr -d ' \n')
docker exec plane-db psql -U plane -d plane -q -c "INSERT INTO workspace_members (id, workspace_id, member_id, role, view_props, default_props, issue_props, is_active, explored_features, getting_started_checklist, tips, created_at, updated_at) SELECT gen_random_uuid(), w.id, '$TUID', 15, '{}', '{}', '{}', true, '{}', '{}', '{}', now(), now() FROM workspaces w WHERE w.slug='$WS';" 2>&1 | head -n 1
check e5-proj-bulk-add 201 -X POST -d "{\"members\":[{\"member_id\":\"$TUID\",\"role\":15}]}" "$BASE/api/workspaces/$WS/projects/$PID/members/"
PMID=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT pm.id FROM project_members pm JOIN projects p ON p.id=pm.project_id JOIN workspaces w ON w.id=p.workspace_id WHERE w.slug='$WS' AND p.id='$PID' AND pm.member_id='$TUID' AND pm.deleted_at IS NULL;" 2>/dev/null | tr -d ' \n')
check e5-proj-patch-ladder 403 -X PATCH -d "{\"role\":$PROLE}" "$BASE/api/workspaces/$WS/projects/$PID/members/$PMID/"
check e5-proj-member-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/members/$PMID/"
check e5-ws-invite-create 200 -X POST -d "{\"email\":\"temp-join-$SFX@example.com\",\"role\":5}" "$BASE/api/workspaces/$WS/invitations/"
grep -q 'Emails sent successfully' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e5-wsinvite-body -> sent-msg"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e5-wsinvite-body"; echo "FAIL e5-wsinvite-body: $(head -c 200 /tmp/smoke_body)"; }
check e5-ws-invite-list 200 "$BASE/api/workspaces/$WS/invitations/"
INVID=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT id FROM workspace_member_invites WHERE email='temp-join-$SFX@example.com' AND deleted_at IS NULL;" 2>/dev/null | tr -d ' \n')
INVTOK=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT token FROM workspace_member_invites WHERE email='temp-join-$SFX@example.com' AND deleted_at IS NULL;" 2>/dev/null | tr -d ' \n')
check_auth e5-ws-join-get 200 "$BASE/api/workspaces/$WS/invitations/$INVID/join/"
check e5-ws-join-badtoken 403 -X POST -d '{"token":"deadbeef","accepted":true}' "$BASE/api/workspaces/$WS/invitations/$INVID/join/"
check e5-ws-join-accept 200 -X POST -d "{\"token\":\"$INVTOK\",\"accepted\":true}" "$BASE/api/workspaces/$WS/invitations/$INVID/join/"
grep -q 'Workspace Invitation Accepted' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e5-join-body -> Accepted"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e5-join-body"; echo "FAIL e5-join-body: $(head -c 200 /tmp/smoke_body)"; }
INVLEFT=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT COUNT(*) FROM workspace_member_invites WHERE email='temp-join-$SFX@example.com';" 2>/dev/null | tr -d ' \n')
if [ "$INVLEFT" = "0" ]; then PASS=$((PASS+1)); echo "ok   e5-join-row-deleted -> 0"; else FAIL=$((FAIL+1)); FAILED="$FAILED e5-join-row-deleted($INVLEFT)"; echo "FAIL e5-join-row-deleted -> $INVLEFT"; fi
check e5-proj-invite-create 200 -X POST -d "{\"email\":\"temp-proj-$SFX@example.com\",\"role\":10}" "$BASE/api/workspaces/$WS/projects/$PID/invitations/"
grep -q 'Email sent successfully' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e5-projinvite-body -> sent-msg"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e5-projinvite-body"; echo "FAIL e5-projinvite-body: $(head -c 200 /tmp/smoke_body)"; }
check e5-proj-invite-list 200 "$BASE/api/workspaces/$WS/projects/$PID/invitations/"
PINVID=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT id FROM project_member_invites WHERE email='temp-proj-$SFX@example.com' AND deleted_at IS NULL;" 2>/dev/null | tr -d ' \n')
check_auth e5-proj-join-get 200 "$BASE/api/workspaces/$WS/projects/$PID/join/$PINVID/"
check e5-proj-join-badtoken 403 -X POST -d '{"token":"deadbeef","accepted":true}' "$BASE/api/workspaces/$WS/projects/$PID/join/$PINVID/"
check e5-proj-invite-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/invitations/$PINVID/"
check e5-join-projects 201 -X POST -d '{"project_ids":[]}' "$BASE/api/users/me/workspaces/$WS/projects/invitations/"

echo "--- E6 favorites ---"
check e6-fav-create 200 -X POST -d "{\"entity_type\":\"project\",\"entity_identifier\":\"$PID\",\"name\":\"smoke-fav\"}" "$BASE/api/workspaces/$WS/user-favorites/"
FID=$(jid id)
check e6-fav-dup200 200 -X POST -d "{\"entity_type\":\"project\",\"entity_identifier\":\"$PID\",\"name\":\"smoke-fav\"}" "$BASE/api/workspaces/$WS/user-favorites/"
check e6-fav-invalid 400 -X POST -d '{}' "$BASE/api/workspaces/$WS/user-favorites/"
check e6-fav-list 200 "$BASE/api/workspaces/$WS/user-favorites/"
check e6-fav-folder 200 -X POST -d '{"entity_type":"project","is_folder":true,"name":"smoke-folder"}' "$BASE/api/workspaces/$WS/user-favorites/"
FOLD=$(jid id)
check e6-fav-child 200 -X POST -d "{\"entity_type\":\"project\",\"entity_identifier\":\"$PID\",\"name\":\"smoke-child\",\"parent\":\"$FOLD\"}" "$BASE/api/workspaces/$WS/user-favorites/"
CHILD=$(jid id)
check e6-fav-group 200 "$BASE/api/workspaces/$WS/user-favorites/$FOLD/group/"
check e6-fav-patch 200 -X PATCH -d '{"name":"smoke-fav2"}' "$BASE/api/workspaces/$WS/user-favorites/$FID/"
check e6-fav-del-child 204 -X DELETE "$BASE/api/workspaces/$WS/user-favorites/$CHILD/"
check e6-fav-del-folder 204 -X DELETE "$BASE/api/workspaces/$WS/user-favorites/$FOLD/"
check e6-fav-del 204 -X DELETE "$BASE/api/workspaces/$WS/user-favorites/$FID/"
check e6-view-create 201 -X POST -d '{"name":"E6 view"}' "$BASE/api/workspaces/$WS/projects/$PID/views/"
VID=$(jid id)
check e6-view-fav-add 204 -X POST -d "{\"view\":\"$VID\"}" "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-views/"
check e6-view-fav-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/user-favorite-views/$VID/"
check e6-view-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/views/$VID/"
check e6-favproj-noget 405 "$BASE/api/workspaces/$WS/user-favorite-projects/"

echo "--- E7 prefs ---"
check e7-userprops-get 200 "$BASE/api/workspaces/$WS/user-properties/"
check e7-userprops-patch 200 -X PATCH -d '{"filters":{}}' "$BASE/api/workspaces/$WS/user-properties/"
check e7-sidebar-get 200 "$BASE/api/workspaces/$WS/sidebar-preferences/"
check e7-sidebar-patch 200 -X PATCH -d '[{"key":"views","is_pinned":true}]' "$BASE/api/workspaces/$WS/sidebar-preferences/"
check e7-sidebar-patch-bad 400 -X PATCH -d '{}' "$BASE/api/workspaces/$WS/sidebar-preferences/"
check e7-home-list 200 "$BASE/api/workspaces/$WS/home-preferences/"
check e7-home-patch 200 -X PATCH -d '{"is_enabled":false}' "$BASE/api/workspaces/$WS/home-preferences/quick_links/"
check e7-home-patch-miss 400 -X PATCH -d '{"is_enabled":false}' "$BASE/api/workspaces/$WS/home-preferences/nope/"
check e7-quick-create 201 -X POST -d "{\"url\":\"https://example.com/q-$SFX\",\"title\":\"q\"}" "$BASE/api/workspaces/$WS/quick-links/"
QID=$(jid id)
check e7-quick-list 200 "$BASE/api/workspaces/$WS/quick-links/"
check e7-quick-detail 200 "$BASE/api/workspaces/$WS/quick-links/$QID/"
check e7-quick-patch 200 -X PATCH -d '{"title":"q2"}' "$BASE/api/workspaces/$WS/quick-links/$QID/"
check e7-quick-dup 400 -X POST -d "{\"url\":\"https://example.com/q-$SFX\",\"title\":\"q\"}" "$BASE/api/workspaces/$WS/quick-links/"
check e7-quick-del 204 -X DELETE "$BASE/api/workspaces/$WS/quick-links/$QID/"
check e7-recent 200 "$BASE/api/workspaces/$WS/recent-visits/"
check e7-views-post 204 -X POST -d '{"view_props":{}}' "$BASE/api/workspaces/$WS/workspace-views/"
check e7-estimates 200 "$BASE/api/workspaces/$WS/estimates/"
check e7-slug-check 200 "$BASE/api/workspace-slug-check/?slug=smoke-$SFX"
check e7-slug-missing 400 "$BASE/api/workspace-slug-check/"
check e7-unsplash 200 "$BASE/api/unsplash/"
grep -q '\[\]' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e7-unsplash-body -> []"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e7-unsplash-body"; echo "FAIL e7-unsplash-body: $(head -c 200 /tmp/smoke_body)"; }
check e7-last-visited 200 "$BASE/api/users/last-visited-workspace/"

echo "--- E8 users ---"
check e8-me 200 "$BASE/api/users/me/"
check e8-me-patch 200 -X PATCH -d '{"first_name":"Smoke"}' "$BASE/api/users/me/"
check e8-session 200 "$BASE/api/users/session/"
grep -q 'is_authenticated' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e8-session-body -> is_authenticated"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e8-session-body"; echo "FAIL e8-session-body: $(head -c 200 /tmp/smoke_body)"; }
check e8-profile 200 "$BASE/api/users/me/profile/"
check e8-profile-patch 200 -X PATCH -d '{"bio":"smoke bio"}' "$BASE/api/users/me/profile/"
check e8-settings 200 "$BASE/api/users/me/settings/"
check e8-accounts 200 "$BASE/api/users/me/accounts/"
check e8-onboard 200 -X PATCH -d '{"is_onboarded":true}' "$BASE/api/users/me/onboard/"
check e8-tour 200 -X PATCH -d '{}' "$BASE/api/users/me/tour-completed/"
check e8-notifprefs-get 200 "$BASE/api/users/me/notification-preferences/"
check e8-notifprefs-patch 200 -X PATCH -d '{}' "$BASE/api/users/me/notification-preferences/"
check e8-email-gen-400 400 -X POST -d '{"email":"x"}' "$BASE/api/users/me/email/generate-code/"
check e8-user-stats 200 "$BASE/api/workspaces/$WS/user-stats/$MUID2/"
check e8-user-profile 200 "$BASE/api/workspaces/$WS/user-profile/$MUID2/"
check e8-user-activity 200 "$BASE/api/workspaces/$WS/user-activity/$MUID2/"
check e8-export 200 -X POST -d '{"date":"2026-01-01"}' "$BASE/api/workspaces/$WS/user-activity/$MUID2/export/"
grep -q 'Actor name' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e8-export-body -> Actor name"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e8-export-body"; echo "FAIL e8-export-body: $(head -c 200 /tmp/smoke_body)"; }
check e8-activity-graph 200 "$BASE/api/users/me/workspaces/$WS/activity-graph/"
check e8-completed-graph 200 "$BASE/api/users/me/workspaces/$WS/issues-completed-graph/"
check e8-dashboard 200 "$BASE/api/users/me/workspaces/$WS/dashboard/"

echo "--- E9 assets ---"
check e9-ws-presign 200 -X POST -d '{"entity_type":"PROJECT_COVER","name":"smoke.png","type":"image/png","size":1024}' "$BASE/api/assets/v2/workspaces/$WS/"
AIDS=$(jid asset_id)
grep -q 'upload_data' /tmp/smoke_body && grep -q 'asset_url' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e9-ws-triple -> upload_data+asset_url"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e9-ws-triple"; echo "FAIL e9-ws-triple: $(head -c 200 /tmp/smoke_body)"; }
check e9-user-presign 200 -X POST -d '{"entity_type":"USER_AVATAR","name":"smoke.png","type":"image/png","size":1024}' "$BASE/api/assets/v2/user-assets/"
AIDU=$(jid asset_id)
check e9-proj-presign 200 -X POST -d '{"entity_type":"ISSUE_ATTACHMENT","name":"smoke.png","type":"image/png","size":1024}' "$BASE/api/assets/v2/workspaces/$WS/projects/$PID/"
AIDP=$(jid asset_id)
check e9-issue-presign 200 -X POST -d "{\"entity_type\":\"ISSUE_ATTACHMENT\",\"entity_identifier\":\"$IID\",\"name\":\"smoke.png\",\"type\":\"image/png\",\"size\":1024}" "$BASE/api/assets/v2/workspaces/$WS/projects/$PID/issues/$IID/attachments/"
AIDI=$(jid asset_id)
check e9-presign-badentity 400 -X POST -d '{"entity_type":"NOPE","name":"x.png"}' "$BASE/api/assets/v2/workspaces/$WS/"
check e9-check 200 "$BASE/api/assets/v2/workspaces/$WS/check/$AIDS/"
grep -q 'exists' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e9-check-body -> exists"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e9-check-body"; echo "FAIL e9-check-body: $(head -c 200 /tmp/smoke_body)"; }
check e9-restore 204 -X POST "$BASE/api/assets/v2/workspaces/$WS/restore/$AIDS/"
check e9-download-notuploaded 404 "$BASE/api/assets/v2/workspaces/$WS/download/$AIDS/"
check e9-issue-list 200 "$BASE/api/assets/v2/workspaces/$WS/projects/$PID/issues/$IID/attachments/"
WSID=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT id FROM workspaces WHERE slug='$WS';" 2>/dev/null | tr -d ' \n')
check e9-legacy-quirk 200 "$BASE/api/workspaces/file-assets/$WSID/no-such-key-$SFX/"
grep -q '"status":false' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e9-legacy-body -> status:false"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e9-legacy-body"; echo "FAIL e9-legacy-body: $(head -c 200 /tmp/smoke_body)"; }
check e9-ws-del 204 -X DELETE "$BASE/api/assets/v2/workspaces/$WS/$AIDS/"
check e9-user-del 204 -X DELETE "$BASE/api/assets/v2/user-assets/$AIDU/"
check e9-proj-del 204 -X DELETE "$BASE/api/assets/v2/workspaces/$WS/projects/$PID/$AIDP/"
check e9-issue-del 204 -X DELETE "$BASE/api/assets/v2/workspaces/$WS/projects/$PID/issues/$IID/attachments/$AIDI/"

echo "--- E10 analytics ---"
check e10-default 200 "$BASE/api/workspaces/$WS/default-analytics/"
grep -q 'issue_completed_month_wise' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e10-default-body -> month_wise"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e10-default-body"; echo "FAIL e10-default-body: $(head -c 200 /tmp/smoke_body)"; }
check e10-projstats 200 "$BASE/api/workspaces/$WS/project-stats/"
grep -q 'total_issues' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e10-projstats-body -> total_issues"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e10-projstats-body"; echo "FAIL e10-projstats-body: $(head -c 200 /tmp/smoke_body)"; }
check e10-adv 200 "$BASE/api/workspaces/$WS/advance-analytics/"
grep -q 'completed_work_items' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e10-adv-body -> completed_work_items"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e10-adv-body"; echo "FAIL e10-adv-body: $(head -c 200 /tmp/smoke_body)"; }
check e10-adv-badtab 400 "$BASE/api/workspaces/$WS/advance-analytics/?tab=nope"
check e10-adv-stats 200 "$BASE/api/workspaces/$WS/advance-analytics-stats/?type=work-items"
check e10-adv-charts 200 "$BASE/api/workspaces/$WS/advance-analytics-charts/"
check e10-proj-adv 200 "$BASE/api/workspaces/$WS/projects/$PID/advance-analytics/"
check e10-proj-adv-charts 200 "$BASE/api/workspaces/$WS/projects/$PID/advance-analytics-charts/?type=work-items"
check e10-proj-adv-charts-bad 400 "$BASE/api/workspaces/$WS/projects/$PID/advance-analytics-charts/"
check e10-deploy-list-null 200 "$BASE/api/workspaces/$WS/projects/$PID/project-deploy-boards/"
grep -q '^null$' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e10-deploy-null -> null"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e10-deploy-null"; echo "FAIL e10-deploy-null: $(head -c 200 /tmp/smoke_body)"; }
check e10-deploy-upsert 200 -X POST -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/project-deploy-boards/"
check e10-deploy-list-obj 200 "$BASE/api/workspaces/$WS/projects/$PID/project-deploy-boards/"
DEPB=$(jid id)
if [ -z "$DEPB" ]; then DEPB=$(python3 -c "import json; print(json.load(open('/tmp/smoke_body')).get('id',''))" 2>/dev/null); fi
check e10-deploy-get 200 "$BASE/api/workspaces/$WS/projects/$PID/project-deploy-boards/$DEPB/"
check e10-deploy-patch 200 -X PATCH -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/project-deploy-boards/$DEPB/"
check e10-deploy-del 204 -X DELETE "$BASE/api/workspaces/$WS/projects/$PID/project-deploy-boards/$DEPB/"

echo "--- E1 instance-admin (read-only; no sign-up/disable-email on live) ---"
check e1-admins-list 200 "$BASE/api/instances/admins/"
check e1-admins-me 200 "$BASE/api/instances/admins/me/"
grep -q "$MEMAIL" /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e1-me-body -> $MEMAIL"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e1-me-body"; echo "FAIL e1-me-body: $(head -c 200 /tmp/smoke_body)"; }
check e1-configs 200 "$BASE/api/instances/configurations/"
grep -q 'ENABLE_SIGNUP' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e1-configs-body -> ENABLE_SIGNUP"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e1-configs-body"; echo "FAIL e1-configs-body: $(head -c 200 /tmp/smoke_body)"; }
check e1-instances 200 "$BASE/api/instances/"
check e1-ws-list 200 "$BASE/api/instances/workspaces/"
check e1-slug-check 200 "$BASE/api/instances/workspace-slug-check/?slug=smoke-$SFX"
check e1-slug-missing 400 "$BASE/api/instances/workspace-slug-check/"
check e1-signup-5150 400 -X POST -d '{"email":"x@y.zz","password":"StrongPass123!","first_name":"t"}' "$BASE/api/instances/admins/sign-up/"
grep -q '5150' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e1-signup-body -> 5150"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e1-signup-body"; echo "FAIL e1-signup-body: $(head -c 200 /tmp/smoke_body)"; }
check e1-signin-noadmin 400 -X POST -d '{"email":"no-such-admin-smoke@example.com","password":"x"}' "$BASE/api/instances/admins/sign-in/"
grep -q '5185' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e1-signin-body -> 5185"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e1-signin-body"; echo "FAIL e1-signin-body: $(head -c 200 /tmp/smoke_body)"; }
if [ -n "${SMOKE_EMAIL:-}" ] && [ -n "${SMOKE_PASSWORD:-}" ]; then
  check e1-signin-smoke-401 401 -X POST -d "{\"email\":\"$SMOKE_EMAIL\",\"password\":\"$SMOKE_PASSWORD\"}" "$BASE/api/instances/admins/sign-in/"
  grep -q '5175' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   e1-smoke-signin-body -> 5175 (non-admin)"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED e1-smoke-signin-body"; echo "FAIL e1-smoke-signin-body: $(head -c 200 /tmp/smoke_body)"; }
else
  echo "skip e1-smoke-signin (SMOKE_EMAIL/SMOKE_PASSWORD unset)"
fi
check e1-emailcheck-noreceiver 400 -X POST -d '{}' "$BASE/api/instances/email-credentials-check/"

echo "--- E11 sign-out ---"
SO_CODE=$(curl -s -m 10 -D /tmp/smoke_hdr -o /tmp/smoke_body -w '%{http_code}' -X POST "${NAH[@]}" "$BASE/auth/sign-out/")
if [ "$SO_CODE" = "302" ]; then PASS=$((PASS+1)); echo "ok   e11-signout-unauthed-302 -> 302"; else FAIL=$((FAIL+1)); FAILED="$FAILED e11-signout-unauthed-302($SO_CODE)"; echo "FAIL e11-signout-unauthed-302 -> $SO_CODE: $(head -c 200 /tmp/smoke_body)"; fi
APPB="${APP_BASE_URL:-http://192.168.1.11:3000}"
if grep -qi "^location: ${APPB%/}" /tmp/smoke_hdr; then PASS=$((PASS+1)); echo "ok   e11-location -> $(grep -i '^location:' /tmp/smoke_hdr | tr -d '\r\n')"; else FAIL=$((FAIL+1)); FAILED="$FAILED e11-location"; echo "FAIL e11-location: $(grep -i '^location:' /tmp/smoke_hdr | tr -d '\r\n')"; fi
if grep -qi '^set-cookie: plane_at=' /tmp/smoke_hdr; then PASS=$((PASS+1)); echo "ok   e11-clear-cookie -> plane_at cleared"; else FAIL=$((FAIL+1)); FAILED="$FAILED e11-clear-cookie"; echo "FAIL e11-clear-cookie: $(grep -i '^set-cookie:' /tmp/smoke_hdr | tr -d '\r\n' | head -c 200)"; fi
if [ -n "${SMOKE_EMAIL:-}" ] && [ -n "${SMOKE_PASSWORD:-}" ]; then
  sleep 61
  JAR2=/tmp/smoke_jar2
  rm -f "$JAR2"
  LCODE=$(curl -s -m 10 -c "$JAR2" -o /tmp/smoke_body -w '%{http_code}' -X POST "${NAH[@]}" -H 'Content-Type: application/json' -d "{\"email\":\"$SMOKE_EMAIL\",\"password\":\"$SMOKE_PASSWORD\"}" "$BASE/api/auth/login/")
  if [ "$LCODE" = "200" ]; then
    SO2=$(curl -s -m 10 -D /tmp/smoke_hdr2 -o /tmp/smoke_body -w '%{http_code}' -b "$JAR2" -c "$JAR2" -X POST "${NAH[@]}" "$BASE/auth/sign-out/")
    if [ "$SO2" = "302" ]; then PASS=$((PASS+1)); echo "ok   e11-signout-authed-302 -> 302"; else FAIL=$((FAIL+1)); FAILED="$FAILED e11-signout-authed-302($SO2)"; echo "FAIL e11-signout-authed-302 -> $SO2"; fi
    if grep -qi "^location: ${APPB%/}" /tmp/smoke_hdr2; then PASS=$((PASS+1)); echo "ok   e11-authed-location -> $(grep -i '^location:' /tmp/smoke_hdr2 | tr -d '\r\n')"; else FAIL=$((FAIL+1)); FAILED="$FAILED e11-authed-location"; echo "FAIL e11-authed-location"; fi
  else
    FAIL=$((FAIL+1)); FAILED="$FAILED e11-login($LCODE)"; echo "FAIL e11-login -> $LCODE (rate limit? rerun smoke)"
  fi
  rm -f "$JAR2"
else
  echo "skip e11-authed (SMOKE_EMAIL/SMOKE_PASSWORD unset)"
fi

echo "--- FE-tolerance pins (must not break smoke) ---"
check fe-ai-404 404 -X POST -d '{"prompt":"hi"}' "$BASE/api/workspaces/$WS/ai-assistant/"
check fe-rephrase-404 404 -X POST -d '{"text":"hi"}' "$BASE/api/workspaces/$WS/rephrase-grammar/"
check fe-changelog-404 404 "$BASE/api/instances/changelog/"
check fe-archpages-404 404 "$BASE/api/workspaces/$WS/projects/$PID/archived-pages/"
check fe-bulksub-404 404 -X POST -d '{}' "$BASE/api/workspaces/$WS/projects/$PID/bulk-subscribe-issues/"

echo "== cleanup =="
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM api_tokens WHERE label = 'smoke2';" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DO \$\$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('workspaces','projects') LOOP BEGIN EXECUTE format('DELETE FROM %I WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE ''smoke-%%'')', r.tablename); EXCEPTION WHEN undefined_column THEN NULL; WHEN foreign_key_violation THEN NULL; WHEN invalid_text_representation THEN NULL; END; END LOOP; END \$\$;" 2>&1 | head -n 1
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM projects WHERE workspace_id IN (SELECT id FROM workspaces WHERE slug LIKE 'smoke-%'); DELETE FROM workspaces WHERE slug LIKE 'smoke-%';" 2>&1 | head -n 1
echo "== temp-user cleanup (E5 second member) =="
docker exec plane-db psql -U plane -d plane -q -c "DELETE FROM project_members WHERE member_id IN (SELECT id FROM users WHERE email LIKE 'temp-member-%'); DELETE FROM workspace_members WHERE member_id IN (SELECT id FROM users WHERE email LIKE 'temp-member-%'); DELETE FROM users WHERE email LIKE 'temp-member-%';" 2>&1 | head -n 1
echo "== leftover-proof (must all be 0) =="
proof_zero() { # proof_zero <label> <sql>
  local label="$1" sql="$2" n
  n=$(docker exec plane-db psql -U plane -d plane -t -A -c "$sql" 2>/dev/null | tr -d ' \n')
  if [ "$n" = "0" ]; then PASS=$((PASS+1)); echo "ok   $label -> 0";
  else FAIL=$((FAIL+1)); FAILED="$FAILED $label($n)"; echo "FAIL $label -> $n leftovers"; fi
}
proof_zero z-cycles "SELECT COUNT(*) FROM cycles c JOIN workspaces w ON w.id=c.workspace_id WHERE w.slug LIKE 'smoke-%' AND c.deleted_at IS NULL"
proof_zero z-modules "SELECT COUNT(*) FROM modules m JOIN workspaces w ON w.id=m.workspace_id WHERE w.slug LIKE 'smoke-%' AND m.deleted_at IS NULL"
proof_zero z-pages "SELECT COUNT(*) FROM pages p JOIN workspaces w ON w.id=p.workspace_id WHERE w.slug LIKE 'smoke-%' AND p.deleted_at IS NULL"
proof_zero z-assets "SELECT COUNT(*) FROM file_assets a JOIN workspaces w ON w.id=a.workspace_id WHERE w.slug LIKE 'smoke-%' AND a.deleted_at IS NULL"
proof_zero z-userassets "SELECT COUNT(*) FROM file_assets WHERE user_id='$MUID' AND workspace_id IS NULL AND deleted_at IS NULL AND created_at > now() - interval '30 minutes'"
proof_zero z-invites "SELECT COUNT(*) FROM workspace_member_invites i JOIN workspaces w ON w.id=i.workspace_id WHERE w.slug LIKE 'smoke-%' AND i.deleted_at IS NULL"
proof_zero z-projinvites "SELECT COUNT(*) FROM project_member_invites i JOIN projects p ON p.id=i.project_id JOIN workspaces w ON w.id=p.workspace_id WHERE w.slug LIKE 'smoke-%' AND i.deleted_at IS NULL"
proof_zero z-favs "SELECT COUNT(*) FROM user_favorites f JOIN workspaces w ON w.id=f.workspace_id WHERE w.slug LIKE 'smoke-%' AND f.deleted_at IS NULL"
proof_zero z-drafts "SELECT COUNT(*) FROM draft_issues d JOIN workspaces w ON w.id=d.workspace_id WHERE w.slug LIKE 'smoke-%' AND d.deleted_at IS NULL"
proof_zero z-labels "SELECT COUNT(*) FROM labels l JOIN projects p ON p.id=l.project_id JOIN workspaces w ON w.id=p.workspace_id WHERE w.slug LIKE 'smoke-%' AND l.deleted_at IS NULL"
proof_zero z-tempusers "SELECT COUNT(*) FROM users WHERE email LIKE 'temp-member-%' OR email LIKE 'temp-join-%' OR email LIKE 'temp-proj-%'"
echo "PASS=$PASS FAIL=$FAIL"; [ -n "$FAILED" ] && echo "failed:$FAILED"
[ "$FAIL" = 0 ]
