# E2E Create Flows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reusable bash script `scripts/e2e-create.sh` that logs into Plane via the browser (agent-browser) and creates one work item, one module, and one page in a target project — verifying each via UI list, via the Rust API, and cleaning up via API DELETE. Continue-on-failure, aggregate report, non-zero exit on any failure.

**Architecture:** Single bash script. Deterministic UI interaction via `agent-browser find <locator> <value> <action>` (stable semantic locators — text/placeholder/role), never raw `@ref` snapshot refs. UI labels confirmed live during discovery; API verify/cleanup via `curl` with its own cookie jar (independent of browser session). JSON parsed with `python3` (repo convention).

**Tech Stack:** bash 5, agent-browser CLI (`/home/ghifari/.nvm/versions/node/v24.16.0/bin/agent-browser`), curl, python3, Rust API `http://192.168.1.11:8000`, FE `http://192.168.1.11:3000`.

**Spec:** `docs/superpowers/specs/2026-09-12-e2e-create-flows-design.md`

**Reference for API endpoints (from parity-inventory.json, all `implemented`):**

- Work item: verify `GET /api/workspaces/:slug/projects/:project_id/issues/`, cleanup `DELETE .../issues/:pk/`
- Module: verify `GET .../modules/`, cleanup `DELETE .../modules/:pk/`
- Page: verify `GET .../pages/`, cleanup `DELETE .../pages/:page_id/`

---

### Task 1: Script skeleton, helpers, and UI login flow

**Files:**

- Create: `scripts/e2e-create.sh`
- Test: run the script (login-only) against the dev stack

- [ ] **Step 1: Write the skeleton with config, helpers, login, report, trap**

Create `scripts/e2e-create.sh`:

```bash
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
  # 2-step auth: email step -> password step. Selectors confirmed during discovery:
  #   step 1: input type=email (placeholder "Enter your email address")
  #   step 2: input type=password (placeholder "Enter your password")
  ab find placeholder "Enter your email address" fill "$PLANE_EMAIL" || return 1
  ab press Enter
  sleep 1
  ab find placeholder "Enter your password" fill "$PLANE_PASSWORD" || return 1
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
```

- [ ] **Step 2: Make executable**

Run: `chmod +x scripts/e2e-create.sh`
Expected: no output, file executable.

- [ ] **Step 3: Discover the login selectors live and fix them in the script**

Run (interactive, in a named session):

```bash
export PATH="/home/ghifari/.nvm/versions/node/v24.16.0/bin:$PATH"
export AGENT_BROWSER_SESSION="e2e-discover-login"
agent-browser open http://192.168.1.11:3000/
agent-browser snapshot -i
```

Expected: identify the email input and its placeholder/aria. Update the two `ab find placeholder ...` lines in `login()` to the real placeholder strings. If the email input has no placeholder, use `ab find role textbox --name "..." fill` instead. Verify the password step appears only after Enter on the email step.

- [ ] **Step 4: Run the script (login-only) and verify it reaches the dashboard**

Run: `PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' PLANE_FLOW=none ./scripts/e2e-create.sh`
Expected: `== login via UI` → `login OK`, `result: 0 passed, 0 failed`, `exit 0`. Screenshot `/tmp/opencode/e2e-login-fail.png` absent.

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e-create.sh
git commit -m "test(e2e): create-flows script skeleton with UI login"
```

---

### Task 2: Work item flow

**Files:**

- Modify: `scripts/e2e-create.sh` (replace `flow_work_item` stub)
- Test: run script with `PLANE_FLOW=wi`

- [ ] **Step 1: Discover the work-item create UI live**

Run:

```bash
export PATH="/home/ghifari/.nvm/versions/node/v24.16.0/bin:$PATH"
export AGENT_BROWSER_SESSION="e2e-discover-wi"
agent-browser open "http://192.168.1.11:3000/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/issues/list"
# (after login) snapshot the page header and find the create button
agent-browser snapshot -i
```

Expected: find the button that opens the create-work-item modal (e.g. text "New work item" / "Add work item"), and inside the modal the title input (placeholder/aria e.g. "Issue Title" or similar) and the submit button text. Record exact strings.

- [ ] **Step 2: Implement `flow_work_item`**

Replace the stub:

```bash
flow_work_item() {
  local flow="work-item" name="e2e-wi-$(date +%s)" rc="FAIL" detail=""
  echo "== flow work-item: $name"
  if ab open "$WEB/$WS/projects/$PROJECT_ID/issues/list" \
     && ab wait --text "New work item" --timeout 10000 2>/dev/null \
     && ab find text "New work item" click \
     && sleep 1 \
     && ab find placeholder "Issue Title" fill "$name" \
     && ab press Enter \
     && ab wait --text "$name" --timeout 8000 2>/dev/null; then
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
    ab screenshot "$SNAP_DIR/e2e-work-item-fail.png"
    ab snapshot -i > "$SNAP_DIR/e2e-work-item-fail.txt" 2>/dev/null || true
    # still try API cleanup if it somehow got created
    api_login && verify_api wi "$name" && cleanup wi "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
  fi
  record "$flow" "$rc" "$detail"
}
```

Note: if Step 1 discovery found different labels, replace `"New work item"` and `"Issue Title"` with the real strings before proceeding.

- [ ] **Step 3: Run the work-item flow and verify it passes**

Run: `PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' PLANE_FLOW=wi ./scripts/e2e-create.sh`
Expected: `work-item PASS  UI+API ok, cleaned (...)` and `result: 1 passed, 0 failed`, exit 0.

- [ ] **Step 4: Confirm cleanup via API directly**

Run:

```bash
PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' PLANE_FLOW=wi ./scripts/e2e-create.sh
```

then verify no `e2e-wi-*` remains:

```bash
curl -s -c /tmp/opencode/j.txt -X POST http://192.168.1.11:8000/api/auth/login/ -H "Content-Type: application/json" -H "Origin: http://192.168.1.11:3000" -d '{"email":"ghifari.zuhir@gmail.com","password":"k9$Lp2@qW8&zX4!v"}' >/dev/null
curl -s -b /tmp/opencode/j.txt http://192.168.1.11:8000/api/workspaces/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/issues/ | grep -c e2e-wi- || echo "0 leftovers"
rm -f /tmp/opencode/j.txt
```

Expected: `0 leftovers`.

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e-create.sh
git commit -m "test(e2e): work-item create flow with API verify + cleanup"
```

---

### Task 3: Module flow

**Files:**

- Modify: `scripts/e2e-create.sh` (replace `flow_module` stub)
- Test: run script with `PLANE_FLOW=mod`

- [ ] **Step 1: Discover the module create UI live**

Run:

```bash
export PATH="/home/ghifari/.nvm/versions/node/v24.16.0/bin:$PATH"
export AGENT_BROWSER_SESSION="e2e-discover-mod"
agent-browser open "http://192.168.1.11:3000/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/modules/list"
# after login, snapshot to find the create-module button + modal fields
agent-browser snapshot -i
```

Expected: create button text (e.g. "New Module" / "Add Module"), and in the modal the name input (placeholder e.g. "Module Name") and submit button. Record exact strings.

- [ ] **Step 2: Implement `flow_module`**

Replace the stub:

```bash
flow_module() {
  local flow="module" name="e2e-mod-$(date +%s)" rc="FAIL" detail=""
  echo "== flow module: $name"
  if ab open "$WEB/$WS/projects/$PROJECT_ID/modules/list" \
     && ab wait --text "New Module" --timeout 10000 2>/dev/null \
     && ab find text "New Module" click \
     && sleep 1 \
     && ab find placeholder "Module Name" fill "$name" \
     && ab press Enter \
     && ab wait --text "$name" --timeout 8000 2>/dev/null; then
    detail="UI ok"
    if api_login && verify_api mod "$name"; then
      local code; code="$(cleanup mod "$RES_ID")"
      if [[ "$code" =~ ^2 ]]; then detail="UI+API ok, cleaned ($code)"; rc="PASS"; else detail="cleaned failed $code"; fi
    else
      detail="UI ok, API verify failed"
      verify_api mod "$name" && cleanup mod "$RES_ID" >/dev/null
    fi
  else
    detail="UI create failed"
    ab screenshot "$SNAP_DIR/e2e-module-fail.png"
    ab snapshot -i > "$SNAP_DIR/e2e-module-fail.txt" 2>/dev/null || true
    api_login && verify_api mod "$name" && cleanup mod "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
  fi
  record "$flow" "$rc" "$detail"
}
```

Note: if Step 1 discovery found different labels, replace `"New Module"` and `"Module Name"` with the real strings before proceeding.

- [ ] **Step 3: Run the module flow and verify it passes**

Run: `PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' PLANE_FLOW=mod ./scripts/e2e-create.sh`
Expected: `module PASS  UI+API ok, cleaned (...)` and `result: 1 passed, 0 failed`, exit 0.

- [ ] **Step 4: Confirm no `e2e-mod-*` leftovers**

Run the same leftover check as Task 2 Step 4 but against `/modules/`.
Expected: `0 leftovers`.

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e-create.sh
git commit -m "test(e2e): module create flow with API verify + cleanup"
```

---

### Task 4: Page flow

**Files:**

- Modify: `scripts/e2e-create.sh` (replace `flow_page` stub)
- Test: run script with `PLANE_FLOW=pg`

- [ ] **Step 1: Discover the page create UI live**

Run:

```bash
export PATH="/home/ghifari/.nvm/versions/node/v24.16.0/bin:$PATH"
export AGENT_BROWSER_SESSION="e2e-discover-pg"
agent-browser open "http://192.168.1.11:3000/itsm/projects/a6152f0b-e445-4e70-b007-dbf267ab9b66/pages/list"
# after login, snapshot to find the create-page button + title input
agent-browser snapshot -i
```

Expected: create button text (e.g. "New Page" / "Add Page"), and the page title input (placeholder e.g. "Page Title" or the editor title field). Record exact strings. Pages create may navigate directly to a page editor instead of a modal — adjust flow accordingly (fill title field, then verify the title in the list header / return to list).

- [ ] **Step 2: Implement `flow_page`**

Replace the stub:

```bash
flow_page() {
  local flow="page" name="e2e-pg-$(date +%s)" rc="FAIL" detail=""
  echo "== flow page: $name"
  if ab open "$WEB/$WS/projects/$PROJECT_ID/pages/list" \
     && ab wait --text "New Page" --timeout 10000 2>/dev/null \
     && ab find text "New Page" click \
     && sleep 1 \
     && ab find placeholder "Page Title" fill "$name" \
     && ab press Enter \
     && ab wait --text "$name" --timeout 8000 2>/dev/null; then
    detail="UI ok"
    if api_login && verify_api pg "$name"; then
      local code; code="$(cleanup pg "$RES_ID")"
      if [[ "$code" =~ ^2 ]]; then detail="UI+API ok, cleaned ($code)"; rc="PASS"; else detail="cleaned failed $code"; fi
    else
      detail="UI ok, API verify failed"
      verify_api pg "$name" && cleanup pg "$RES_ID" >/dev/null
    fi
  else
    detail="UI create failed"
    ab screenshot "$SNAP_DIR/e2e-page-fail.png"
    ab snapshot -i > "$SNAP_DIR/e2e-page-fail.txt" 2>/dev/null || true
    api_login && verify_api pg "$name" && cleanup pg "$RES_ID" >/dev/null && detail="$detail, cleaned-by-relookup"
  fi
  record "$flow" "$rc" "$detail"
}
```

Note: if Step 1 discovery found different labels or a non-modal flow, rewrite the interaction block to match reality (e.g. create button opens the editor; fill title; go back to list; assert title present). Keep the API verify/cleanup block unchanged.

- [ ] **Step 3: Run the page flow and verify it passes**

Run: `PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' PLANE_FLOW=pg ./scripts/e2e-create.sh`
Expected: `page PASS  UI+API ok, cleaned (...)` and `result: 1 passed, 0 failed`, exit 0.

- [ ] **Step 4: Confirm no `e2e-pg-*` leftovers**

Run the same leftover check as Task 2 Step 4 but against `/pages/`.
Expected: `0 leftovers`.

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e-create.sh
git commit -m "test(e2e): page create flow with API verify + cleanup"
```

---

### Task 5: Full-run verification

**Files:**

- Test: run the complete script end-to-end

- [ ] **Step 1: Full run — all three flows**

Run: `PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' ./scripts/e2e-create.sh`
Expected output:

```
== login via UI (...)
  login OK
== flow work-item: e2e-wi-...
== flow module: e2e-mod-...
== flow page: e2e-pg-...
======================
work-item      PASS  UI+API ok, cleaned (...)
module         PASS  UI+API ok, cleaned (...)
page           PASS  UI+API ok, cleaned (...)
----------------------
result: 3 passed, 0 failed
```

and `exit 0`. Check no `e2e-*` leftovers in issues/modules/pages via the Step-4 command pattern.

- [ ] **Step 2: Idempotency — run twice back-to-back**

Run the full script a second time immediately. Expected: still `3 passed, 0 failed`, exit 0 (epoch-unique names, no collisions).

- [ ] **Step 3: Failure path**

Run: `PLANE_EMAIL="ghifari.zuhir@gmail.com" PLANE_PASSWORD='k9$Lp2@qW8&zX4!v' PLANE_PROJECT_ID="00000000-0000-0000-0000-000000000000" ./scripts/e2e-create.sh`
Expected: all three flows FAIL (UI create fails or API verify fails), script continues through all flows, `result: 0 passed, 3 failed`, `exit 1`. Failure screenshots/snapshots written to `/tmp/opencode/e2e-*-fail.{png,txt}`.

- [ ] **Step 4: Commit any final adjustments**

```bash
git add scripts/e2e-create.sh
git commit -m "test(e2e): full-run verification of create flows"
```

- [ ] **Step 5: Push**

```bash
git push origin preview
```
