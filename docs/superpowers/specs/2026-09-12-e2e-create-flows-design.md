# E2E Create Flows (Work Item / Module / Page) — Design

Date: 2026-09-12
Status: Approved
Related: `scripts/e2e-login-check.sh` (login smoke test), agent-browser CLI (global at `~/.nvm/versions/node/v24.16.0/bin/agent-browser`)

## Goal

Reusable browser-driven E2E script that, against the running dev stack (Rust API `:8000` + FE `:3000`), performs full UI login and then creates one **work item**, one **module**, and one **page** in the target project — verifying each via UI list, via the Rust API, and cleaning up the created data. Continue-on-failure with an aggregate report and non-zero exit when any flow fails.

## Decisions (brainstormed & approved)

1. Verification depth: UI + API + cleanup (API DELETE).
2. Form: single bash script `scripts/e2e-create.sh`, 3 flows sequential, login once per run.
3. Login: full UI login per run via `http://192.168.1.11:3000/`; target project default PREPAID (`a6152f0b-e445-4e70-b007-dbf267ab9b66`) in workspace `itsm`, env-overridable.
4. Cleanup via API (fast, deterministic); continue on flow failure; aggregate report; exit non-zero if any flow failed.
5. Driver: bash + agent-browser CLI (approach A), consistent with `scripts/e2e-login-check.sh`; no new dependencies.

## Architecture

Single file: `scripts/e2e-create.sh` (bash; `set -u`, no `set -e`; `trap 'agent-browser close' EXIT`).

```
scripts/e2e-create.sh
├─ Config & env overrides
│   PLANE_EMAIL, PLANE_PASSWORD          (required)
│   PLANE_API    default http://192.168.1.11:8000
│   PLANE_WEB    default http://192.168.1.11:3000
│   PLANE_WS     default itsm
│   PLANE_PROJECT_ID  default a6152f0b-e445-4e70-b007-dbf267ab9b66
│   PLANE_DEBUG=1  print response bodies on failure
├─ Helpers
│   ab() … agent-browser wrapper with per-run AGENT_BROWSER_SESSION
│   api_login() … curl POST /api/auth/login/ (Origin: PLANE_WEB) → cookie jar
│   flow_work_item(), flow_module(), flow_page()
│   verify_api <kind> <name> … GET list endpoint, return id
│   cleanup <kind> <id> … DELETE via API, then GET-confirm 404
│   report() … PASS/FAIL summary per flow
└─ Main: UI login → flow×3 → report → exit 1 if any FAIL, else 0
```

- Per-run unique names: `e2e-wi-<epoch>`, `e2e-mod-<epoch>`, `e2e-pg-<epoch>` → unambiguous API lookup and cleanup.
- API verification uses its own cookie jar (separate `curl` login), independent of the browser session.

## Data flow per flow

**Login (once per run):**

1. `agent-browser open $PLANE_WEB` → snapshot → `fill` email → `fill` password → `press Enter`.
2. Wait for dashboard (re-snapshot; confirm logged-in state, e.g. workspace/user element visible).

**Create (same pattern for work item / module / page):**

1. Navigate: `$PLANE_WEB/itsm/projects/<id>/issues/list` | `/modules/list` | `/pages/list`.
2. Snapshot → locate create button ("New work item" / "New module" / "New page"; exact label confirmed during implementation) → `click`.
3. Snapshot modal → `fill` name → submit (primary button / Enter).
4. Re-snapshot → assert name visible in list.
5. `api_login` → GET list endpoint (`/issues/`, `/modules/`, `/pages/`) → find name → capture `id`.
6. DELETE `/issues/<id>/`, `/modules/<id>/`, `/pages/<id>/` → expect 2xx, GET again → expect 404.

**Per-flow specifics:**

- Work item: fill title only (required); leave default state/priority.
- Module: name + optional description; defaults for status etc.
- Page: title; blank content, default format.

**API verification endpoints (workspace `itsm`, project `<id>`):**

- Work item → `GET /api/workspaces/itsm/projects/<id>/issues/` → matching `name` and `created_by` = logged-in user.
- Module → `GET /api/workspaces/itsm/projects/<id>/modules/` → matching `name`.
- Page → `GET /api/workspaces/itsm/projects/<id>/pages/` → matching `name`.

## Error handling

- Continue-on-failure: each flow is an independent function; `set -u` only.
- Timeouts/retries: `agent-browser wait --load networkidle` + `wait <ms>` after actions; element search retried max 3× (re-snapshot) before failing.
- Failure capture: screenshot `/tmp/opencode/e2e-<flow>-fail.png` + last snapshot to `/tmp/opencode/e2e-<flow>-fail.txt`.
- API verify miss: flow FAIL; still attempt cleanup (re-lookup; if id unknown, log "cleanup skipped").
- Cleanup best-effort: runs even after FAIL; cleanup errors only logged.
- Report: per-flow `PASS/FAIL (<failed step>)` + `N passed, M failed`; exit 1 if any FAIL.
- `trap EXIT` closes the browser session.

## Testing

1. Primary: run the script once against the running dev stack → 3 PASS, exit 0, no `e2e-*` leftovers in project.
2. Idempotency: run twice back-to-back → second run still PASS (epoch-unique names).
3. Failure path (once): `PLANE_PROJECT_ID=<invalid uuid>` → flows FAIL but script continues, exit 1.

## Out of scope (YAGNI)

- No Playwright / CI integration; no parallel browser sessions; no external reporting beyond failure screenshots/snapshots; no cleanup via UI.
