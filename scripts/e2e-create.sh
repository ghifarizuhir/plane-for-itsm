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
#             (none = login only, no flows run)
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

ab() { "$AB_BIN" "$@" || { echo "  [ab] command failed: agent-browser" "$@" >&2; return 1; }; }

# wait_text <text> [tries] — re-poll `wait --text` (single poll can miss on a
# slow dev FE under load); returns 0 on first match, 1 after all tries.
wait_text() {
  local text="$1" tries="${2:-3}" i
  for ((i=1; i<=tries; i++)); do
    if ab wait --text "$text" 2>/dev/null; then return 0; fi
    echo "  [wait] retry $i/$tries for text: $text" >&2
    sleep 3
  done
  return 1
}

# click_untitled — retry the editor-title heading click itself. A bare
# `wait --text Untitled` can match the sidebar row before the editor title
# (h1) has loaded past "Loading version details", so gate on the click.
click_untitled() {
  local i
  for ((i=1; i<=4; i++)); do
    if ab find role heading click --name Untitled 2>/dev/null; then return 0; fi
    echo "  [wait] retry $i/4 for Untitled heading click" >&2
    sleep 3
  done
  return 1
}

api_login() {
  local payload
  payload="$(PLANE_EMAIL="$PLANE_EMAIL" PLANE_PASSWORD="$PLANE_PASSWORD" python3 -c '
import json, os
print(json.dumps({"email": os.environ["PLANE_EMAIL"], "password": os.environ["PLANE_PASSWORD"]}))
')" || return 1
  curl -s -c "$JAR" -X POST "$API/api/auth/login/" \
    -H "Content-Type: application/json" -H "Origin: $WEB" \
    -d "$payload" >/dev/null || return 1
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
  # NOTE: `wait --url` takes no --timeout flag (default timeout applies);
  # errors are intentionally visible here so a login failure is diagnosable.
  for _ in 1 2 3 4 5; do
    if ab wait --url "$WS"; then
      echo "  login OK"
      return 0
    fi
    sleep 2
  done
  ab screenshot "$SNAP_DIR/e2e-login-fail.png"
  return 1
}

verify_api() { # verify_api <wi|mod|pg> <name> -> sets RES_ID, returns 0 if found
  # callers: use "${RES_ID:-}" — empty when not found
  # assumes list response; adjust if API paginates
  RES_ID=""
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
# list endpoints return the paginated envelope {"results": [...], ...};
# accept a bare array too (structure unchanged, shape-tolerant lookup)
items = data.get("results", []) if isinstance(data, dict) else data
items = [o for o in items if isinstance(o, dict)] if isinstance(items, list) else []
for o in items:
    if o.get("name") == name:
        print(o.get("id", ""))
        break
' <<<"$body")"
  if [[ -z "${RES_ID:-}" ]]; then
    [[ "${PLANE_DEBUG:-0}" == "1" ]] && printf '%s\n' "$body" >&2
    return 1
  fi
}

cleanup() { # cleanup <wi|mod|pg> <id> -> echo HTTP code
  local kind="$1" id="$2" ep
  case "$kind" in
    wi)  ep="/api/workspaces/$WS/projects/$PROJECT_ID/issues/$id/" ;;
    mod) ep="/api/workspaces/$WS/projects/$PROJECT_ID/modules/$id/" ;;
    pg)
      # pages must be archived before delete (mirrors Django PageViewSet:
      # destroy returns 400 "should be archived before deleting" otherwise)
      curl -s -o /dev/null -b "$JAR" -X POST --max-time 15 \
        -H "Origin: $WEB" "$API/api/workspaces/$WS/projects/$PROJECT_ID/pages/$id/archive/"
      ep="/api/workspaces/$WS/projects/$PROJECT_ID/pages/$id/" ;;
    *)   return 1 ;;
  esac
  curl -s -o /dev/null -w "%{http_code}" -b "$JAR" -X DELETE --max-time 15 \
    -H "Origin: $WEB" "$API$ep"
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

# flow stubs (module/page implemented in later tasks)
# Discovered live (2026-09-12, session e2e-discover-wi) for work items:
# - create button text "New work item"; modal heading "Create new work item";
#   title input has NO placeholder — stable locator is role textbox "Title";
#   submit button text "Save".
# - State MUST be picked explicitly (Backlog via state dropdown + Search
#   combobox): the form default state_id is "" and a stateless issue is
#   excluded from the issues list API, so API verify would never find it.
# - Project list view fetches .../issues/?group_by=state_id which the Rust
#   API 400s ("group_by not supported"), so the UI-list assert runs on the
#   workspace all-issues spreadsheet view (ungrouped fetch, renders the row).
flow_work_item() {
  local flow="work-item" name="e2e-wi-$(date +%s)" rc="FAIL" detail=""
  echo "== flow work-item: $name"
  if ab open "$WEB/$WS/projects/$PROJECT_ID/issues/list" \
     && ab wait --load networkidle 2>/dev/null \
     && wait_text "New work item" \
     && ab find text "New work item" click \
     && sleep 2 \
     && ab find text "Backlog" click \
     && sleep 2 \
     && ab find role combobox fill "Backlog" --name Search \
     && ab press Enter \
     && sleep 1 \
     && ab find role textbox fill "$name" --name Title \
     && ab find text "Save" click \
     && sleep 2 \
      && ab open "$WEB/$WS/workspace-views/all-issues/" \
      && ab wait --load networkidle 2>/dev/null \
      && wait_text "$name"; then
    detail="UI ok"
    if api_login && verify_api wi "$name"; then
      local code; code="$(cleanup wi "$RES_ID")"
      if [[ "$code" =~ ^2 ]]; then detail="UI+API ok, cleaned ($code)"; rc="PASS"; else detail="cleaned failed $code"; fi
    else
      detail="UI ok, API verify failed"
      # best-effort cleanup by re-lookup
      verify_api wi "$name" && cleanup wi "$RES_ID" >/dev/null
    fi
  else
    detail="UI create failed"
    ab screenshot "$SNAP_DIR/e2e-work-item-fail.png" || true
    ab snapshot -i > "$SNAP_DIR/e2e-work-item-fail.txt" 2>/dev/null || true
    # still try API cleanup if it somehow got created
    api_login && verify_api wi "$name" && cleanup wi "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
  fi
  record "$flow" "$rc" "$detail"
}
# Discovered live (2026-09-12, session e2e-discover-mod) for modules:
# - list route is .../modules (NO /list suffix: ".../modules/list" is treated
#   as a module id and renders "Module does not exist" + "View other modules").
# - create button text "Add Module"; modal heading "Create module"; name input
#   has NO placeholder — stable locator is role textbox "Title"; submit button
#   text "Create Module". No extra required fields (Backlog default is fine).
flow_module() {
  local flow="module" name="e2e-mod-$(date +%s)" rc="FAIL" detail=""
  echo "== flow module: $name"
  if ab open "$WEB/$WS/projects/$PROJECT_ID/modules" \
     && ab wait --load networkidle 2>/dev/null \
     && wait_text "Add Module" \
     && ab find text "Add Module" click \
     && sleep 2 \
     && ab find role textbox fill "$name" --name Title \
     && ab find text "Create Module" click \
     && sleep 2 \
     && wait_text "$name"; then
    detail="UI ok"
    if api_login && verify_api mod "$name"; then
      local code; code="$(cleanup mod "$RES_ID")"
      if [[ "$code" =~ ^2 ]]; then detail="UI+API ok, cleaned ($code)"; rc="PASS"; else detail="cleaned failed $code"; fi
    else
      detail="UI ok, API verify failed"
      # best-effort cleanup by re-lookup
      verify_api mod "$name" && cleanup mod "$RES_ID" >/dev/null
    fi
  else
    detail="UI create failed"
    ab screenshot "$SNAP_DIR/e2e-module-fail.png" || true
    ab snapshot -i > "$SNAP_DIR/e2e-module-fail.txt" 2>/dev/null || true
    # still try API cleanup if it somehow got created
    api_login && verify_api mod "$name" && cleanup mod "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
  fi
  record "$flow" "$rc" "$detail"
}
# Discovered live (2026-09-12, session e2e-discover-pg) for pages:
# - list route is .../pages (NO /list suffix: ".../pages/list" renders an
#   empty view with zero interactive elements — same trap as modules).
# - create button text "Add page"; clicking it instantly creates a blank page
#   (API name "") shown as "Untitled" — NO modal, NO navigation, stays on list.
# - editor route is .../pages/<page-id>/; the title is an h1 inside a textbox
#   (accessible name "Untitled" while blank). Stable rename interaction:
#   `find role heading click --name Untitled`, `press Control+a`,
#   `keyboard type "<name>"` (autosaves; persists despite the editor's
#   "Connection lost" websocket banner — verified via API GET).
# - the editor route needs a beat after networkidle: it first renders
#   "Loading version details", then the title textbox (h1 "Untitled ").
#   A bare `wait --text Untitled` is not enough to gate the title click:
#   the sidebar list already contains an "Untitled" row and matches early,
#   while the editor h1 is still "Loading version details" (seen live
#   2026-09-12) — so click_untitled() retries the heading click itself.
# - the new page id is identified by diffing the API list before/after the
#   "Add page" click, so we never guess which "Untitled" row is ours.
flow_page() {
  local flow="page" name="e2e-pg-$(date +%s)" rc="FAIL" detail=""
  echo "== flow page: $name"
  local ep="/api/workspaces/$WS/projects/$PROJECT_ID/pages/"
  local ids_before="" after="" NEW_ID=""
  api_login || { record "$flow" "$rc" "api login failed"; return; }
  local before_body=""
  before_body="$(curl -s -b "$JAR" --max-time 15 "$API$ep")" || { record "$flow" "$rc" "list pages (before) failed"; return; }
  ids_before="$(python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    raise SystemExit(1)
items = data.get("results", data) if isinstance(data, dict) else data
for o in items if isinstance(items, list) else []:
    if isinstance(o, dict) and o.get("id"):
        print(o["id"])
' <<<"$before_body")" || { record "$flow" "$rc" "list pages (before) failed"; return; }
  if ab open "$WEB/$WS/projects/$PROJECT_ID/pages" \
     && ab wait --load networkidle 2>/dev/null \
     && wait_text "Add page" \
     && ab find text "Add page" click \
     && sleep 2; then
    after="$(curl -s -b "$JAR" --max-time 15 "$API$ep")" || after=""
    NEW_ID="$(IDS_BEFORE="$ids_before" python3 -c '
import json, os, sys
before = set((os.environ.get("IDS_BEFORE") or "").split())
try:
    data = json.load(sys.stdin)
except Exception:
    raise SystemExit(1)
items = data.get("results", data) if isinstance(data, dict) else data
for o in items if isinstance(items, list) else []:
    if isinstance(o, dict) and o.get("id") and o["id"] not in before:
        print(o["id"])
        break
' <<<"$after")" || NEW_ID=""
    if [[ -n "$NEW_ID" ]] \
       && ab open "$WEB/$WS/projects/$PROJECT_ID/pages/$NEW_ID/" \
       && ab wait --load networkidle 2>/dev/null \
       && sleep 2 \
       && click_untitled \
       && ab press Control+a \
       && ab keyboard type "$name" \
       && sleep 3 \
       && wait_text "$name" \
       && ab open "$WEB/$WS/projects/$PROJECT_ID/pages" \
       && ab wait --load networkidle 2>/dev/null \
       && wait_text "$name"; then
      detail="UI ok"
      if api_login && verify_api pg "$name"; then
        local code; code="$(cleanup pg "$RES_ID")"
        if [[ "$code" =~ ^2 ]]; then detail="UI+API ok, cleaned ($code)"; rc="PASS"; else detail="cleaned failed $code"; fi
      else
        detail="UI ok, API verify failed"
        # best-effort cleanup by re-lookup
        verify_api pg "$name" && cleanup pg "$RES_ID" >/dev/null
      fi
    else
      detail="UI create/rename failed"
      ab screenshot "$SNAP_DIR/e2e-page-fail.png" || true
      ab snapshot -i > "$SNAP_DIR/e2e-page-fail.txt" 2>/dev/null || true
      # still try cleanup: prefer the diffed id, fall back to name re-lookup
      if [[ -n "$NEW_ID" ]]; then
        cleanup pg "$NEW_ID" >/dev/null && detail="$detail, cleaned-by-id"
      else
        api_login && verify_api pg "$name" && cleanup pg "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
      fi
    fi
  else
    detail="UI create failed"
    ab screenshot "$SNAP_DIR/e2e-page-fail.png" || true
    ab snapshot -i > "$SNAP_DIR/e2e-page-fail.txt" 2>/dev/null || true
    # still try API cleanup if it somehow got created
    api_login && verify_api pg "$name" && cleanup pg "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
  fi
  record "$flow" "$rc" "$detail"
}

# ---- main ----
login
LOGIN_RC=$?
[[ "$LOGIN_RC" -eq 0 ]] || record "login" "FAIL" "UI login failed"
# no point running flows without a logged-in browser session
[[ "$LOGIN_RC" -eq 0 ]] || { report; exit 1; }
api_login || record "api-login" "FAIL" "curl login failed"

FLOW="${PLANE_FLOW:-all}"
if [[ "$FLOW" == "all" || "$FLOW" == "wi" ]]; then flow_work_item; fi
if [[ "$FLOW" == "all" || "$FLOW" == "mod" ]]; then flow_module; fi
if [[ "$FLOW" == "all" || "$FLOW" == "pg" ]]; then flow_page; fi

report
