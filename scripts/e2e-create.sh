#!/usr/bin/env bash
# E2E create flows (work item / module / page) — agent-browser + Rust API.
# Usage:
#   PLANE_EMAIL=you@mail.com PLANE_PASSWORD=secret ./scripts/e2e-create.sh
# Env overrides:
#   PLANE_API   API base        (default http://192.168.1.11:8000)
#   PLANE_WEB   FE base         (default http://192.168.1.11:3000)
#   PLANE_WS    workspace slug  (default itsm)
#   PLANE_PROJECT_ID  (default a6152f0b-e445-4e70-b007-dbf267ab9b66)
#   PLANE_FLOW  wi|mod|pg|none  run a single flow only (default: all)
#   PLANE_DEBUG=1  print bodies on failure
set -u
AB_BIN="${AGENT_BROWSER_BIN:-/home/ghifari/.nvm/versions/node/v24.16.0/bin/agent-browser}"
export AGENT_BROWSER_SESSION="e2e-create-$(date +%s)"

API="${PLANE_API:-http://192.168.1.11:8000}"
WEB="${PLANE_WEB:-http://192.168.1.11:3000}"
WS="${PLANE_WS:-itsm}"
PROJECT_ID="${PLANE_PROJECT_ID:-a6152f0b-e445-4e70-b007-dbf267ab9b66}"
: "${PLANE_EMAIL:?set PLANE_EMAIL}"
: "${PLANE_PASSWORD:?set PLANE_PASSWORD}"

JAR="$(mktemp)"
SNAP_DIR=/tmp/opencode
mkdir -p "$SNAP_DIR"
trap 'rm -f "$JAR"; "$AB_BIN" close 2>/dev/null' EXIT

PASS=0; FAIL=0
declare -a RESULTS=()

ab() { "$AB_BIN" "$@" || { echo "  [ab] command failed: agent-browser $*" >&2; return 1; }; }

api_login() {
  curl -s -c "$JAR" -X POST "$API/api/auth/login/" \
    -H "Content-Type: application/json" -H "Origin: $WEB" \
    -d "{\"email\":\"$PLANE_EMAIL\",\"password\":\"$PLANE_PASSWORD\"}" >/dev/null || return 1
}

login() {
  echo "== login via UI ($WEB)"
  ab open "$WEB" || return 1
  ab wait --load networkidle 2>/dev/null || true
  # 2-step auth: email step -> password step. Discovered live (2026-09-12):
  # inputs carry no placeholder text; stable locators are role + accessible
  # name ("Email" / "Password"). Submitting the password step with Enter
  # authenticates (verified live); the "Go to workspace" button is a fallback.
  ab find role textbox fill "$PLANE_EMAIL" --name Email || return 1
  ab press Enter
  sleep 1
  ab find role textbox fill "$PLANE_PASSWORD" --name Password || return 1
  ab press Enter
  # wait until we leave the login page: after success Plane redirects to
  # a workspace URL that contains the workspace slug (e.g. /itsm/...)
  for _ in 1 2 3 4 5; do
    if ab wait --url "$WS" --timeout 3000 2>/dev/null; then
      echo "  login OK"
      return 0
    fi
    sleep 2
  done
  ab screenshot "$SNAP_DIR/e2e-login-fail.png"
  return 1
}

verify_api() { # verify_api <wi|mod|pg> <name> -> sets RES_ID, returns 0 if found
  local kind="$1" name="$2" ep
  case "$kind" in
    wi)  ep="/api/workspaces/$WS/projects/$PROJECT_ID/issues/" ;;
    mod) ep="/api/workspaces/$WS/projects/$PROJECT_ID/modules/" ;;
    pg)  ep="/api/workspaces/$WS/projects/$PROJECT_ID/pages/" ;;
    *)   return 1 ;;
  esac
  local body
  body="$(curl -s -b "$JAR" --max-time 15 "$API$ep")" || return 1
  RES_ID="$(RES_NAME="$name" python3 -c '
import json, os, sys
name = os.environ["RES_NAME"]
data = json.load(sys.stdin)
for o in data:
    if o.get("name") == name:
        print(o.get("id", ""))
        break
')" <<<"$body"
  [[ -n "${RES_ID:-}" ]]
}

cleanup() { # cleanup <wi|mod|pg> <id> -> echo HTTP code
  local kind="$1" id="$2" ep
  case "$kind" in
    wi)  ep="/api/workspaces/$WS/projects/$PROJECT_ID/issues/$id/" ;;
    mod) ep="/api/workspaces/$WS/projects/$PROJECT_ID/modules/$id/" ;;
    pg)  ep="/api/workspaces/$WS/projects/$PROJECT_ID/pages/$id/" ;;
    *)   return 1 ;;
  esac
  curl -s -o /dev/null -w "%{http_code}" -b "$JAR" -X DELETE --max-time 15 "$API$ep"
}

record() { # record <flow> <PASS|FAIL> <detail>
  local flow="$1" rc="$2" detail="${3:-}"
  if [[ "$rc" == "PASS" ]]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
  RESULTS+=("$flow|$rc|$detail")
}

report() {
  echo "======================"
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r f rc detail <<<"$r"
    printf "%-14s %-4s %s\n" "$f" "$rc" "$detail"
  done
  echo "----------------------"
  echo "result: $PASS passed, $FAIL failed"
  [[ "$FAIL" -eq 0 ]]
}

# flow stubs (implemented in later tasks)
flow_work_item() { record "work-item" "PASS" "stub"; }
flow_module()    { record "module"    "PASS" "stub"; }
flow_page()      { record "page"      "PASS" "stub"; }

# ---- main ----
login
LOGIN_RC=$?
[[ "$LOGIN_RC" -eq 0 ]] || record "login" "FAIL" "UI login failed"
api_login || record "api-login" "FAIL" "curl login failed"

FLOW="${PLANE_FLOW:-all}"
if [[ "$FLOW" == "all" || "$FLOW" == "wi" ]]; then flow_work_item; fi
if [[ "$FLOW" == "all" || "$FLOW" == "mod" ]]; then flow_module; fi
if [[ "$FLOW" == "all" || "$FLOW" == "pg" ]]; then flow_page; fi

report
