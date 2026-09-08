# Batch H: Workspace-Domain Inventory Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inventory all 41 Django `plane/app/urls/workspace.py` paths in `parity-inventory.json` (39 new `workspace`-domain entries), including a 2-line router rename (`:fid` → `:favorite_id`) so the two favorite-detail routes register under their canonical Django form.

**Architecture:** JSON expansion following the Batch G pattern (audit → entries with status via view-vs-handler recipe → gates green), plus one minimal router fix. Audit (2026-09-08) shows 39 of 41 workspace.py paths already registered in `main.rs`; the 2 exceptions differ ONLY by param name (`:fid` vs Django `:favorite_id`) while handlers use positional `Path` tuples — so the rename is behavior-preserving. Any `shape_mismatch` found is RECORDED, not fixed here (follow-up Batch H-fix plan).

**Tech Stack:** JSON (`apps/api-rs/crates/api/parity-inventory.json`), existing gate tests, 2-line `main.rs` edit. No new deps, no handler logic changes.

---

## Audit reference (2026-09-08, static)

41 `workspace.py` paths; 2 already inventoried (Batch F: workspace-themes ×2). Remaining **39 entries** across Tasks H1–H8, plus final audit H9. Scope rule: ONLY the 41 `workspace.py` paths — similarly-named routes from other urls files (`views.py` global views, `search.py`, `webhook.py`, `members-lite/`, `my-issues/`, `project-roles/`, `dashboard/`) belong to later batches, do NOT inventory them here.

The 2 rename targets: `main.rs:126` (`user-favorites/:fid/`) and `main.rs:134` (`user-favorites/:fid/group/`); handlers extract positionally (`favorite.rs:721,899,934` `Path((slug, fid))`), so no handler change is needed.

## Status recipe (same as Batch G, plus two lessons)

For each path, determine `rust_status`:

1. Path not in `main.rs` (exact string) → `missing`.
2. Else read the Django view and the Rust handler(s); compare timeboxed to (a) status codes, (b) response JSON keys, (c) error strings, (d) permission gates.
3. Any delta → `shape_mismatch` (`constraint_mismatch` if validation-only); `notes` MUST describe it with `file:line` both sides.
4. No delta → `implemented`, `notes` = `"verified <View> vs <handler>"` with line refs.
5. **G5 lesson:** `methods` reflects DJANGO-served methods (parity target). Unhandled Django method → record fully + `shape_mismatch`, never scope down. Conversely, extra Rust-only methods (Rust superset) → keep Django methods in `methods`, describe the extras in `notes`.
6. **FE-variant lesson:** if FE calls a URL variant present in NEITHER Django nor Rust (e.g. a `/:key/` suffix), do NOT invent an entry — record the observation in the matching entry's `notes`.
7. `fe_pages`: default `[]`; fill ONLY with single-page verification via grep.

## Worked example (shape of every entry)

```json
{
  "methods": ["GET", "POST"],
  "path": "/api/workspaces/:slug/invitations/",
  "django_source": "app/urls/workspace.py:65",
  "rust_status": "implemented",
  "rust_handler": "routes::invite::ws_list",
  "fe_evidence": [
    { "service": "apps/web/core/services/workspace.service.ts", "method": "workspaceInvitations" },
    { "service": "apps/web/core/services/workspace.service.ts", "method": "inviteWorkspace" }
  ],
  "fe_pages": [],
  "batch_task": "Batch H T2",
  "out_scope": false,
  "notes": "verified WorkspaceInvitationsViewset vs ws_list/ws_create"
}
```

---

### Task 1: H1 workspace core (3 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append 3 objects to `domains.workspace.endpoints`)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing, must stay green)

Entries (`batch_task: "Batch H T1"`, `out_scope: false`, `fe_pages: []` per rule):

| path                         | methods (Django)                                                                                                                                              | rust_handler                | django_source (view)                                            | fe_evidence                                                                                                                                                                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspace-slug-check/` | GET (verify: read `WorkSpaceAvailabilityCheckEndpoint` methods)                                                                                               | `routes::prefs::slug_check` | `app/urls/workspace.py:43` (WorkSpaceAvailabilityCheckEndpoint) | `workspaceSlugCheck` @ `apps/web/core/services/workspace.service.ts` (hits `/api/workspace-slug-check/?slug=`; the `/api/instances/...` twin is a different route — exclude)                                                    |
| `/api/workspaces/`           | GET, POST (`WorkSpaceViewSet` list/create, ws.py:48)                                                                                                          | `routes::workspace::list`   | `app/urls/workspace.py:48` (WorkSpaceViewSet)                   | repo-wide grep for callers of bare `/api/workspaces/` (POST create / GET list); `[]` if confirmed absent (record grep)                                                                                                          |
| `/api/workspaces/:slug/`     | GET, PUT?, PATCH?, DELETE? (read FULL as_view block ws.py:53-63 — Rust serves delete/get/patch; record Django set exactly, note any PUT-only-in-Django delta) | `routes::workspace::detail` | `app/urls/workspace.py:53` (WorkSpaceViewSet)                   | `getWorkspace`, `updateWorkspace`, `deleteWorkspace` @ `apps/web/core/services/workspace.service.ts`; `retrieve`, `update`, `destroy` @ `packages/services/src/workspace/workspace.service.ts` (verify each URL before listing) |

- [ ] **Step 1: Append the 3 entries** after the G-era `workspace-themes/:pk/` object (current tail of `domains.workspace.endpoints`). Status/notes via recipe (compare against `workspace.rs` list/create/detail/patch/destroy and `prefs.rs` slug_check).
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspace-slug-check/',
  '/api/workspaces/',
  '/api/workspaces/:slug/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H1 present: 3/3')
"
```

Expected: `H1 present: 3/3`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace core (Batch H T1)"
```

---

### Task 2: H2 invitations (4 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H1 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T2"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                          | methods (Django)                                              | rust_handler                                 | django_source (view)                                         | fe_evidence                                                                                                                                                                    |
| --------------------------------------------- | ------------------------------------------------------------- | -------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/api/workspaces/:slug/invitations/`          | GET, POST (ws.py:65 list/create)                              | `routes::invite::ws_list`                    | `app/urls/workspace.py:65` (WorkspaceInvitationsViewset)     | `workspaceInvitations`, `inviteWorkspace` @ `apps/web/core/services/workspace.service.ts`; `invite` @ `packages/services/src/workspace/invitation.service.ts`                  |
| `/api/workspaces/:slug/invitations/:pk/`      | GET, PATCH, DELETE (ws.py:70 destroy/retrieve/partial_update) | `routes::invite::ws_detail`                  | `app/urls/workspace.py:70` (WorkspaceInvitationsViewset)     | `deleteWorkspaceInvitations`, `updateWorkspaceInvitation` @ `apps/web/core/services/workspace.service.ts`; `destroy` @ `packages/services/src/workspace/invitation.service.ts` |
| `/api/users/me/workspaces/invitations/`       | GET, POST (ws.py:76 list/create)                              | `routes::users_me::my_workspace_invitations` | `app/urls/workspace.py:76` (UserWorkspaceInvitationsViewset) | repo-wide grep for `users/me/workspaces/invitations` callers; `[]` if confirmed absent (record grep)                                                                           |
| `/api/workspaces/:slug/invitations/:pk/join/` | GET, POST (verify `WorkspaceJoinEndpoint` methods)            | `routes::invite::ws_join_get`                | `app/urls/workspace.py:81` (WorkspaceJoinEndpoint)           | `getWorkspaceInvitation`, `joinWorkspace` @ `apps/web/core/services/workspace.service.ts`; `join` @ `packages/services/src/workspace/invitation.service.ts`                    |

- [ ] **Step 1: Append the 4 entries** (status/notes via recipe; compare against `invite.rs` and `users_me.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/invitations/',
  '/api/workspaces/:slug/invitations/:pk/',
  '/api/users/me/workspaces/invitations/',
  '/api/workspaces/:slug/invitations/:pk/join/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H2 present: 4/4')
"
```

Expected: `H2 present: 4/4`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace invitations (Batch H T2)"
```

---

### Task 3: H3 members (6 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H2 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T3"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                          | methods (Django)                                              | rust_handler                             | django_source (view)                                               | fe_evidence                                                                                                                                                                                                                                                                          |
| --------------------------------------------- | ------------------------------------------------------------- | ---------------------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/api/workspaces/:slug/members/`              | GET (ws.py:87 list)                                           | `routes::member::list_workspace_members` | `app/urls/workspace.py:87` (WorkSpaceMemberViewSet)                | `fetchWorkspaceMembers`, `updateWorkspaceMember`, `deleteWorkspaceMember` @ `apps/web/core/services/workspace.service.ts`; `list`, `update`, `destroy` @ `packages/services/src/workspace/member.service.ts` (verify each URL targets `members/` vs `members/:pk/` before assigning) |
| `/api/workspaces/:slug/project-members/`      | GET (verify `WorkspaceProjectMemberEndpoint` methods)         | `routes::member::ws_project_members`     | `app/urls/workspace.py:92` (WorkspaceProjectMemberEndpoint)        | repo-wide grep for `project-members` callers; `[]` if confirmed absent (record grep)                                                                                                                                                                                                 |
| `/api/workspaces/:slug/members/:pk/`          | GET, PATCH, DELETE (ws.py:97 partial_update/destroy/retrieve) | `routes::member::ws_member_detail`       | `app/urls/workspace.py:97` (WorkSpaceMemberViewSet)                | member-`${memberId}` callers from the two member services above (verify URL has the id segment)                                                                                                                                                                                      |
| `/api/workspaces/:slug/members/leave/`        | POST (ws.py:102 leave)                                        | `routes::member::ws_leave`               | `app/urls/workspace.py:102` (WorkSpaceMemberViewSet)               | `leaveWorkspace` @ `apps/web/core/services/user.service.ts`                                                                                                                                                                                                                          |
| `/api/users/last-visited-workspace/`          | GET (verify `UserLastProjectWithWorkspaceEndpoint` methods)   | `routes::prefs::last_visited`            | `app/urls/workspace.py:107` (UserLastProjectWithWorkspaceEndpoint) | repo-wide grep for `last-visited-workspace` callers; `[]` if confirmed absent (record grep)                                                                                                                                                                                          |
| `/api/workspaces/:slug/workspace-members/me/` | GET (verify `WorkspaceMemberUserEndpoint` methods)            | `routes::member::ws_me`                  | `app/urls/workspace.py:112` (WorkspaceMemberUserEndpoint)          | `workspaceMemberMe` @ `apps/web/core/services/workspace.service.ts`; `myInfo` @ `packages/services/src/workspace/member.service.ts`                                                                                                                                                  |

- [ ] **Step 1: Append the 6 entries** (status/notes via recipe; compare against `member.rs` and `prefs.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/members/',
  '/api/workspaces/:slug/project-members/',
  '/api/workspaces/:slug/members/:pk/',
  '/api/workspaces/:slug/members/leave/',
  '/api/users/last-visited-workspace/',
  '/api/workspaces/:slug/workspace-members/me/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H3 present: 6/6')
"
```

Expected: `H3 present: 6/6`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace members (Batch H T3)"
```

---

### Task 4: H4 user profile (5 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H3 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T4"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                                   | methods (Django — verify each Endpoint's get/post methods) | rust_handler                       | django_source (view)                                              | fe_evidence (all @ `apps/web/core/services/user.service.ts`) |
| ------------------------------------------------------ | ---------------------------------------------------------- | ---------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------ |
| `/api/workspaces/:slug/user-stats/:user_id/`           | GET                                                        | `routes::user::user_stats`         | `app/urls/workspace.py:132` (WorkspaceUserProfileStatsEndpoint)   | `getUserProfileData`                                         |
| `/api/workspaces/:slug/user-activity/:user_id/`        | GET                                                        | `routes::user::user_activity`      | `app/urls/workspace.py:137` (WorkspaceUserActivityEndpoint)       | `getUserProfileActivity`                                     |
| `/api/workspaces/:slug/user-activity/:user_id/export/` | POST                                                       | `routes::user::export_activity`    | `app/urls/workspace.py:142` (ExportWorkspaceUserActivityEndpoint) | `downloadProfileActivity`                                    |
| `/api/workspaces/:slug/user-profile/:user_id/`         | GET                                                        | `routes::user::user_profile`       | `app/urls/workspace.py:147` (WorkspaceUserProfileEndpoint)        | `getUserProfileProjectsSegregation`                          |
| `/api/workspaces/:slug/user-issues/:user_id/`          | GET                                                        | `routes::issue_lists::user_issues` | `app/urls/workspace.py:152` (WorkspaceUserProfileIssuesEndpoint)  | `getUserProfileIssues`                                       |

- [ ] **Step 1: Append the 5 entries** (status/notes via recipe; compare against `user.rs` and `issue_lists.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/user-stats/:user_id/',
  '/api/workspaces/:slug/user-activity/:user_id/',
  '/api/workspaces/:slug/user-activity/:user_id/export/',
  '/api/workspaces/:slug/user-profile/:user_id/',
  '/api/workspaces/:slug/user-issues/:user_id/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H4 present: 5/5')
"
```

Expected: `H4 present: 5/5`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace user profile (Batch H T4)"
```

---

### Task 5: H5 aggregates (6 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H4 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T5"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                     | methods (Django — verify each Endpoint's get/post methods)      | rust_handler                        | django_source (view)                                          | fe_evidence                                                                                                                                                                                                                                               |
| ---------------------------------------- | --------------------------------------------------------------- | ----------------------------------- | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/labels/`          | GET                                                             | `routes::label::ws_labels`          | `app/urls/workspace.py:157` (WorkspaceLabelsEndpoint)         | `getWorkspaceIssueLabels` @ `apps/web/core/services/issue/issue_label.service.ts`                                                                                                                                                                         |
| `/api/workspaces/:slug/user-properties/` | GET (verify; project-level twin is GET+PATCH — do not conflate) | `routes::prefs::user_props_get`     | `app/urls/workspace.py:162` (WorkspaceUserPropertiesEndpoint) | `fetchWorkspaceFilters`, `patchWorkspaceFilters` @ `apps/web/core/services/workspace.service.ts`; `fetchWorkspaceFilters` @ `apps/web/core/services/issue_filter.service.ts` (verify each URL targets workspace-level `/user-properties/` before listing) |
| `/api/workspaces/:slug/states/`          | GET                                                             | `routes::workspace::ws_states`      | `app/urls/workspace.py:167` (WorkspaceStatesEndpoint)         | `getWorkspaceStates` @ `apps/web/core/services/project/project-state.service.ts`                                                                                                                                                                          |
| `/api/workspaces/:slug/estimates/`       | GET                                                             | `routes::prefs::ws_estimates`       | `app/urls/workspace.py:172` (WorkspaceEstimatesEndpoint)      | `fetchWorkspaceEstimates` @ `apps/web/core/services/estimate.service.ts`                                                                                                                                                                                  |
| `/api/workspaces/:slug/modules/`         | GET                                                             | `routes::module::workspace_modules` | `app/urls/workspace.py:177` (WorkspaceModulesEndpoint)        | `getWorkspaceModules` @ `apps/web/core/services/module.service.ts`; `workspaceModulesList` @ `packages/services/src/module/module.service.ts`                                                                                                             |
| `/api/workspaces/:slug/cycles/`          | GET                                                             | `routes::cycle::workspace_cycles`   | `app/urls/workspace.py:182` (WorkspaceCyclesEndpoint)         | `getWorkspaceCycles` @ `apps/web/core/services/cycle.service.ts`; `getWorkspaceCycles` @ `packages/services/src/cycle/cycle.service.ts`                                                                                                                   |

- [ ] **Step 1: Append the 6 entries** (status/notes via recipe)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/labels/',
  '/api/workspaces/:slug/user-properties/',
  '/api/workspaces/:slug/states/',
  '/api/workspaces/:slug/estimates/',
  '/api/workspaces/:slug/modules/',
  '/api/workspaces/:slug/cycles/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H5 present: 6/6')
"
```

Expected: `H5 present: 6/6`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace aggregates (Batch H T5)"
```

---

### Task 6: H6 favorites + `:fid` → `:favorite_id` rename (3 entries + router fix)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H5 entries)
- Modify: `apps/api-rs/crates/api/src/main.rs` (2-line param rename)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T6"`, `out_scope: false`, `fe_pages: []` per rule) — NOTE the canonical `:favorite_id` form (Django), NOT the current `:fid`:

| path                                                       | methods (Django — read FULL as_view blocks ws.py:187-201; Rust currently wires get+post / patch+delete / get) | rust_handler              | django_source (view)                                         | fe_evidence                                                                                                                                             |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/user-favorites/`                    | GET, POST (verify mapping)                                                                                    | `routes::favorite::list`  | `app/urls/workspace.py:187` (WorkspaceFavoriteEndpoint)      | `getFavorites`, `addFavorite` @ `apps/web/core/services/favorite/favorite.service.ts`; `list`, `add` @ `packages/services/src/user/favorite.service.ts` |
| `/api/workspaces/:slug/user-favorites/:favorite_id/`       | per Django mapping (verify; Rust: patch+delete)                                                               | `routes::favorite::patch` | `app/urls/workspace.py:192` (WorkspaceFavoriteEndpoint)      | `updateFavorite`, `deleteFavorite` @ apps favorite.service.ts; `update`, `remove` @ packages favorite.service.ts                                        |
| `/api/workspaces/:slug/user-favorites/:favorite_id/group/` | GET (verify mapping)                                                                                          | `routes::favorite::group` | `app/urls/workspace.py:197` (WorkspaceFavoriteGroupEndpoint) | `getGroupedFavorites` @ apps favorite.service.ts; `groupedList` @ packages favorite.service.ts                                                          |

- [ ] **Step 1: Append the 3 entries with canonical `:favorite_id` paths** (status/notes via recipe; compare against `favorite.rs` handlers)

- [ ] **Step 2: Run the gate to verify RED**

Run: `cargo test -p api --test route_inventory_test`
Expected: FAIL — `implemented_paths_are_registered_in_main_rs` lists the two `:favorite_id` paths (main.rs still has `:fid`). This RED proves the rename is necessary. (If it passes, STOP and report NEEDS_CONTEXT — the router already changed.)

- [ ] **Step 3: Rename `:fid` → `:favorite_id` in main.rs**

First confirm safety:

```bash
grep -rn ":fid" apps/api-rs/crates/api/src/ apps/api-rs/crates/api/tests/ | grep -v target
```

Expected: only `main.rs:126` (`:fid/`), `main.rs:134` (`:fid/group/`), plus `fid` LOCAL variable bindings in `favorite.rs` (`Path((slug, fid))` tuples at :721,:899,:934 — positional extraction, NOT route strings; do NOT touch them).

Then edit `apps/api-rs/crates/api/src/main.rs` (2 lines only):

```rust
"/api/workspaces/:slug/user-favorites/:fid/",
```

→

```rust
"/api/workspaces/:slug/user-favorites/:favorite_id/",
```

```rust
"/api/workspaces/:slug/user-favorites/:fid/group/",
```

→

```rust
"/api/workspaces/:slug/user-favorites/:favorite_id/group/",
```

- [ ] **Step 4: Run the gates to verify GREEN**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 5: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/user-favorites/',
  '/api/workspaces/:slug/user-favorites/:favorite_id/',
  '/api/workspaces/:slug/user-favorites/:favorite_id/group/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H6 present: 3/3')
"
```

Expected: `H6 present: 3/3`.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json apps/api-rs/crates/api/src/main.rs
git commit -m "feat(rs-api): inventory favorites + favorite_id param rename (Batch H T6)"
```

---

### Task 7: H7 drafts (3 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H6 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T7"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                              | methods (Django)                                               | rust_handler                           | django_source (view)                                     | fe_evidence (all @ `apps/web/core/services/issue/workspace_draft.service.ts`) |
| ------------------------------------------------- | -------------------------------------------------------------- | -------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `/api/workspaces/:slug/draft-issues/`             | GET, POST (ws.py:202 list/create)                              | `routes::draft::list`                  | `app/urls/workspace.py:202` (WorkspaceDraftIssueViewSet) | `getIssues`, `createIssue`                                                    |
| `/api/workspaces/:slug/draft-issues/:pk/`         | GET, PATCH, DELETE (ws.py:207 retrieve/partial_update/destroy) | `routes::draft::retrieve`              | `app/urls/workspace.py:207` (WorkspaceDraftIssueViewSet) | `getIssueById`, `updateIssue`, `deleteIssue`                                  |
| `/api/workspaces/:slug/draft-to-issue/:draft_id/` | POST (ws.py:212 create_draft_to_issue)                         | `routes::draft::create_draft_to_issue` | `app/urls/workspace.py:212` (WorkspaceDraftIssueViewSet) | `moveIssue`                                                                   |

- [ ] **Step 1: Append the 3 entries** (status/notes via recipe; compare against `draft.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/draft-issues/',
  '/api/workspaces/:slug/draft-issues/:pk/',
  '/api/workspaces/:slug/draft-to-issue/:draft_id/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H7 present: 3/3')
"
```

Expected: `H7 present: 3/3`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace drafts (Batch H T7)"
```

---

### Task 8: H8 prefs & misc (9 entries)

**Files:**

- Modify: `apps/api-rs/crates/api/parity-inventory.json` (append after the H7 entries)
- Test: `route_inventory_test.rs` + `fe_tripwire_test.rs` (existing)

Entries (`batch_task: "Batch H T8"`, `out_scope: false`, `fe_pages: []` per rule):

| path                                           | methods (Django — read each as_view block / Endpoint methods)  | rust_handler                  | django_source (view)                                           | fe_evidence                                                                                                                                                                                                                                                                                                               |
| ---------------------------------------------- | -------------------------------------------------------------- | ----------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/workspace-views/`       | POST (verify `WorkspaceMemberUserViewsEndpoint` methods)       | `routes::prefs::views_post`   | `app/urls/workspace.py:117` (WorkspaceMemberUserViewsEndpoint) | `updateWorkspaceView` @ `apps/web/core/services/workspace.service.ts`                                                                                                                                                                                                                                                     |
| `/api/workspaces/:slug/quick-links/`           | GET, POST (ws.py:218 list/create)                              | `routes::prefs::quick_list`   | `app/urls/workspace.py:218` (QuickLinkViewSet)                 | `fetchWorkspaceLinks`, `createWorkspaceLink` @ `apps/web/core/services/workspace.service.ts`                                                                                                                                                                                                                              |
| `/api/workspaces/:slug/quick-links/:pk/`       | GET, PATCH, DELETE (ws.py:223 retrieve/partial_update/destroy) | `routes::prefs::quick_detail` | `app/urls/workspace.py:223` (QuickLinkViewSet)                 | `updateWorkspaceLink`, `deleteWorkspaceLink` @ `apps/web/core/services/workspace.service.ts`                                                                                                                                                                                                                              |
| `/api/workspaces/:slug/home-preferences/`      | GET (verify `WorkspaceHomePreferenceViewSet` mapping)          | `routes::prefs::home_list`    | `app/urls/workspace.py:229` (WorkspaceHomePreferenceViewSet)   | `fetchWorkspaceWidgets` @ `apps/web/core/services/workspace.service.ts`                                                                                                                                                                                                                                                   |
| `/api/workspaces/:slug/home-preferences/:key/` | PATCH (verify mapping)                                         | `routes::prefs::home_patch`   | `app/urls/workspace.py:234` (WorkspaceHomePreferenceViewSet)   | `updateWorkspaceWidget` @ `apps/web/core/services/workspace.service.ts`                                                                                                                                                                                                                                                   |
| `/api/workspaces/:slug/recent-visits/`         | GET (ws.py:239 list)                                           | `routes::prefs::recent_list`  | `app/urls/workspace.py:239` (UserRecentVisitViewSet)           | `fetchWorkspaceRecents` @ `apps/web/core/services/workspace.service.ts`                                                                                                                                                                                                                                                   |
| `/api/workspaces/:slug/stickies/`              | GET, POST (ws.py:244 list/create)                              | `routes::misc::list_stickies` | `app/urls/workspace.py:244` (WorkspaceStickyViewSet)           | `getStickies`, `createSticky` @ `apps/web/core/services/sticky.service.ts`                                                                                                                                                                                                                                                |
| `/api/workspaces/:slug/stickies/:pk/`          | GET, PATCH, DELETE (ws.py:249 retrieve/partial_update/destroy) | `routes::misc::get_sticky`    | `app/urls/workspace.py:249` (WorkspaceStickyViewSet)           | `getSticky`, `updateSticky`, `deleteSticky` @ `apps/web/core/services/sticky.service.ts`                                                                                                                                                                                                                                  |
| `/api/workspaces/:slug/sidebar-preferences/`   | GET, PATCH (verify `WorkspaceUserPreferenceViewSet` mapping)   | `routes::prefs::sidebar_get`  | `app/urls/workspace.py:255` (WorkspaceUserPreferenceViewSet)   | `fetchSidebarNavigationPreferences`, `updateBulkSidebarPreferences` @ `apps/web/core/services/workspace.service.ts`. NOTE: FE also calls a `/sidebar-preferences/${key}/` variant (`updateSidebarPreference`) present in NEITHER Django nor Rust — do NOT invent an entry; record the observation in this entry's `notes` |

- [ ] **Step 1: Append the 9 entries** (status/notes via recipe; compare against `prefs.rs` and `misc.rs` handlers)
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

Run:

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/workspace-views/',
  '/api/workspaces/:slug/quick-links/',
  '/api/workspaces/:slug/quick-links/:pk/',
  '/api/workspaces/:slug/home-preferences/',
  '/api/workspaces/:slug/home-preferences/:key/',
  '/api/workspaces/:slug/recent-visits/',
  '/api/workspaces/:slug/stickies/',
  '/api/workspaces/:slug/stickies/:pk/',
  '/api/workspaces/:slug/sidebar-preferences/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('H8 present: 9/9')
"
```

Expected: `H8 present: 9/9`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory workspace prefs & misc (Batch H T8)"
```

---

### Task 9: H9 final audit + full suite

**Files:** none (verification only; fix forward with a new commit only if the audit finds a gap)

- [ ] **Step 1: Full domain audit — every workspace.py path inventoried**

Run:

```bash
python3 -c "
import json, re
src = open('apps/api/plane/app/urls/workspace.py').read()
def canon(s):
    return '/api/' + re.sub(r'<(?:str|uuid|int):(\w+)>', r':\1', s)
dpaths = set()
for m in re.finditer(r'path\(\s*(?:[ru]?\"([^\"]+)\"|ru?\'([^\']+)\')', src):
    dpaths.add(canon(m.group(1) or m.group(2)))
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
have = set(ep['path'] for d in inv['domains'].values() for ep in d['endpoints'])
missing = sorted(dpaths - have)
print('workspace.py paths:', len(dpaths), '| inventoried:', len(dpaths & have))
assert not missing, missing
ws_eps = inv['domains']['workspace']['endpoints']
print('workspace domain entries:', len(ws_eps))
assert len(ws_eps) == 41, len(ws_eps)
print('H9 audit: OK')
"
```

Expected: `workspace.py paths: 41 | inventoried: 41`, `workspace domain entries: 41`, `H9 audit: OK`. If any path is missing (other than nothing — no aliases expected after the H6 rename), report BLOCKED with the list; do not invent entries.

- [ ] **Step 2: Run the full api test suite**

Run: `cargo test -p api`
Expected: 0 failed.

- [ ] **Step 3: Report**

No commit in this task unless the audit forces a fix (then commit with message `fix(rs-api): workspace audit gap (Batch H T9)`). Report the audit output and suite result.

---

## Final state after Batch H

- `workspace` domain: 41 entries (2 Batch F + 39 Batch H), every `workspace.py` path tracked.
- `main.rs` `:fid` → `:favorite_id` (2 lines, behavior-preserving).
- Any `shape_mismatch` found in H1–H8 is RECORDED with notes, NOT fixed (follow-up Batch H-fix plan).

## Out of scope

- Fixing handlers for any `shape_mismatch` found (follow-up plan).
- Other domains (`project`, `cycle`, `module`, `user`, `page`, …) — later batches.
- Similarly-named routes from other urls files (`views.py`, `search.py`, `webhook.py`, `members-lite/`, `my-issues/`, `dashboard/`, `project-roles/`).
- Changing gate tests, shadow.sh, or FE code (except the 2-line router rename).

## Self-review notes

- Spec coverage: matrix design spec §3 (schema) → per-task tables + Batch G worked-example shape; §5 (one domain per batch) → workspace.py only, with explicit exclusions; §6 (FE-impact priority) → workspace chosen on call-site (43) + surface (41) data.
- Placeholder scan: no TBD/TODO; every task has exact paths, main.rs methods+handlers (from wiring), Django view+line, FE evidence (file+method, mined), exact commands with expected output. Django-served `methods` marked "verify" only where the as_view block was truncated in extraction — the recipe + file:line tells exactly where to read.
- Type consistency: `batch_task` `"Batch H T1"`–`"Batch H T9"` unique; canonical `:param` paths match the gate's exact-string comparison; the H6 entries use `:favorite_id` (post-rename form) consistently in table, presence check, and audit.
- Task ordering: H1–H8 append sequentially to the same array — sequential only. H6 contains the router rename with RED-then-GREEN proof. H9 is verification-only.
- Count check: 3+4+6+5+6+3+3+9 = 39 new + 2 existing = 41 = workspace.py path count. Counts verified against extraction output.
