# Batch L — workflow entities inventory expansion

Date: 2026-09-09 | Branch: `preview` | Label: `Batch L Tn`

## Source

Three urls files, **21 `path()` entries**, 0 already inventoried → **21 new entries** across 3 new domains:

| file                                | paths            | new domain                    |
| ----------------------------------- | ---------------- | ----------------------------- |
| `apps/api/plane/app/urls/intake.py` | 10 (lines 16–65) | `domains.intake` (10 entries) |
| `apps/api/plane/app/urls/state.py`  | 4 (lines 12–31)  | `domains.state` (4 entries)   |
| `apps/api/plane/app/urls/views.py`  | 7 (lines 17–65)  | `domains.views` (7 entries)   |

Canonicalization: `<str:slug>` → `:slug`, `<uuid:project_id>` → `:project_id`, `<uuid:pk>` → `:pk`, `<uuid:view_id>` → `:view_id`, `<uuid:work_item_id>` → `:work_item_id`. All paths already registered in `main.rs` (verified line-by-line below) → no `missing` status expected; every entry is `implemented` or `shape_mismatch` per the status recipe.

New domains need a `rust_module` string (gate requirement `route_inventory_test.rs:40`): `intake` → `routes/intake.rs`, `state` → `routes/state.rs`, `views` → `routes/view.rs`. (The 2 intake desc-versions handlers live in `versions.rs` — recorded per-entry via `rust_handler`; the domain-level `rust_module` is just the primary module.)

## Status recipe (from Batch G/H, unchanged)

1. Path not in `main.rs` (exact string) → `missing`. (None here.)
2. Else compare Django view vs Rust handler(s) on (a) status codes, (b) response JSON keys, (c) error strings, (d) permission gates.
3. Any delta → `shape_mismatch` (`constraint_mismatch` if validation-only); `notes` MUST describe it with `file:line` both sides.
4. No delta → `implemented`, `notes` = `"verified <View> vs <handler>"` with line refs.
5. `methods` reflects DJANGO-served methods (parity target). Unhandled Django method → record fully + `shape_mismatch`, never scope down. Extra Rust-only methods (Rust superset) → keep Django methods in `methods`, describe the extras in `notes`.
6. FE-variant lesson: if FE calls a URL variant present in NEITHER Django NOR Rust (e.g. `views/:pk/issues/`), do NOT invent an entry — record the observation in the matching entry's `notes`.
7. `fe_pages`: default `[]`; fill ONLY with single-page verification via grep.

## Pre-mined deltas (verify, don't blindly copy)

| path                                                               | expected status             | delta hint                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------------------------------------ | --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/intakes/` GET                                                    | `shape_mismatch`            | Django returns a SINGLE object (`IntakeSerializer(intake).data`, `views/intake/base.py:74-76`) vs Rust `[{id,name}]` array (`intake.rs:68-84`). POST: Django 201 full shape vs Rust 201 `{id,name}` + extra 409 CONFLICT dup (`intake.rs:101-106`; Django dup → IntegrityError → 400 via `views/base.py:92-97`).                                                                    |
| `/intakes/:pk/`                                                    | `shape_mismatch`            | Django retrieve/patch return full `IntakeSerializer` shape; Rust `{id,name}` / `{id}` trimmed (`intake.rs:250-253, 280`). DELETE: default-intake guard string identical both sides → likely `implemented` (`intake.rs:223-231` = `"You cannot delete the default intake"`, `views/intake/base.py:86-90`).                                                                           |
| `/intake-issues/`                                                  | `shape_mismatch`            | GET: Django paginated full `IntakeIssueSerializer` + 404 `{"error": "Intake not found"}` (`base.py:179-181`) vs Rust `[{id,status}]` (`intake.rs:120-136`). POST: Django **200** full `IntakeIssueDetailSerializer` (`base.py:330`) vs Rust **201** `{id,status,issue_id}` (`intake.rs:217-221`).                                                                                   |
| `/intake-issues/:pk/`                                              | `shape_mismatch`            | Django detail/patch full detail-serializer shape; Rust `{id,status}` (`intake.rs:320-322`) / patch_issue 200. DELETE: Django 204 cascades issue delete when status ∈ [-2,-1,0,2] (`base.py:563-569`); Rust `destroy_issue` mirrors (`intake.rs:326-332+`) → verify.                                                                                                                 |
| `/inboxes/`, `/inboxes/:pk/`                                       | same as intakes twins       | Identical viewset wiring (`IntakeViewSet`), identical Rust handlers (`intake.rs` list/create/detail/patch/destroy) — copy the intakes status + notes, adjust names.                                                                                                                                                                                                                 |
| `/inbox-issues/`, `/inbox-issues/:pk/`                             | same as intake-issues twins | Same handlers.                                                                                                                                                                                                                                                                                                                                                                      |
| `/intake-work-items/:work_item_id/description-versions/` (+`:pk/`) | verify                      | Django `IntakeWorkItemDescriptionVersionEndpoint` GET-only, guest gate `base.py:581-600`; Rust `versions::intake_desc_versions_list` / `intake_desc_version_detail` (`versions.rs:749,793`; wired `main.rs:335-341`). Compare with the G T9 `work-items/.../description-versions/` entries already inventoried (same serializer).                                                   |
| `/states/`                                                         | `shape_mismatch` likely     | GET: Django full `StateSerializer` list + `grouped=true` dict mode + per-group `order` field (`views/state/base.py:78-102`) vs Rust `[{id,name,group}]` ordered by sequence (`state.rs:56-76`). POST: Django 201 full + dup-name 400 `{"name": "The state name is already taken"}` (`base.py:55-59`) vs Rust 201 `StateOut{id,name,group}` (`state.rs:94-101`) + `validate_create`. |
| `/states/:pk/`                                                     | `shape_mismatch` likely     | Django retrieve/patch full shape (dup-name 400 both sides); Rust `{id,name,group}` / `{id}` (`state.rs:138-145, 155+`). DELETE: guard strings verbatim-identical (`"Default state cannot be deleted"`, `"The state is not empty, only empty states can be deleted"` — `base.py:117-130` vs `state.rs:106-114`) → destroy likely `implemented`.                                      |
| `/intake-state/`                                                   | verify                      | Django GET 200 full `StateSerializer` / 404 `{"error": "Triage state not found"}` (`base.py:139-144`); Rust `state::intake_state` (`state.rs:359+`, `guard_intake_state`, wired `main.rs:647-648`).                                                                                                                                                                                 |
| `/states/:pk/mark-default/`                                        | verify                      | Django POST-only `mark_as_default` → 204, ADMIN gate (`base.py:104-110`); Rust `state::mark_default` (`state.rs:245+`, role guard `guard_mark_default`).                                                                                                                                                                                                                            |
| `/projects/:project_id/views/`                                     | verify                      | GET: Django full list w/ `fields` param + guest filter; Rust `view::list` returns `[{id,name}]` (`view.rs:159-165`) → likely `shape_mismatch`. POST: Django 201 full `IssueViewSerializer` vs Rust 201 `{id,name}` → delta.                                                                                                                                                         |
| `/projects/:project_id/views/:pk/`                                 | `shape_mismatch`            | **Django serves PUT (`"put": "update"`, `views.py:27`) — Rust has NO PUT** (`main.rs:838-843` get/patch/delete only). Also Django detail full shape w/ locked/owner checks (`view/base.py:349-369`) vs Rust `{id,name}` (`view.rs:337-338`) / patch `{id}` + `"Invalid name"/"Invalid access"` errors.                                                                              |
| `/workspaces/:slug/views/`                                         | verify                      | Django list/create (`WorkspaceViewViewSet`, ws-level, `level="WORKSPACE"` gates) vs Rust `view::list_global/create_global` — same `[{id,name}]`/`{id,name}` trim.                                                                                                                                                                                                                   |
| `/workspaces/:slug/views/:pk/`                                     | `shape_mismatch`            | **Django serves GET+PUT+PATCH+DELETE (`views.py:41-47`) — Rust wires GET ONLY** (`main.rs:849-851` `get(routes::view::detail_global)`; no patch_global/destroy_global exist in `view.rs`). Record Django PUT/PATCH/DELETE fully; `shape_mismatch` notes must list the 3 missing methods with `main.rs:849-851`.                                                                     |
| `/workspaces/:slug/issues/`                                        | verify                      | Django `WorkspaceViewIssuesViewSet.list` gzip + ComplexFilter + paginated `ViewIssueListSerializer` (`view/base.py:221-259`) vs Rust `issue_lists::workspace_issues` (`issue_lists.rs:616`, wired `main.rs:74`) — compare carefully; likely `shape_mismatch`.                                                                                                                       |
| `/projects/:project_id/user-favorite-views/`                       | `shape_mismatch`            | **Quirk:** Django maps `"get": "list"` but `IssueViewFavoriteViewSet` has NO `serializer_class` (`view/base.py:407-417`) → DRF `get_serializer_class()` AssertionError → GET 500. Rust pre-existing `view::list_favorites` returns `[{id,view}]` (`view.rs:223-239`) — Rust superset; `methods` = GET, POST; POST 204 (`base.py:419-427`).                                          |
| `/projects/:project_id/user-favorite-views/:view_id/`              | verify                      | Django DELETE-only `destroy` → 204 hard-delete (`view/base.py:429-439`); Rust `favorite::view_fav_destroy` (`favorite.rs:973`, wired `main.rs:926-927`).                                                                                                                                                                                                                            |

## FE evidence (pre-mined, verify service+method exist before listing)

| path                                                       | fe_evidence hint                                                                                                                                                                                                           |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/intakes/`, `/intakes/:pk/`, `/inboxes/`, `/inboxes/:pk/` | NO FE caller found (grep `intakes/` + `inboxes/` across `apps/web/` and `packages/services/` → 0 hits; FE only ever calls the `*-issues` forms) → `fe_evidence: []`, record grep in notes                                  |
| `/intake-issues/`, `/intake-issues/:pk/`                   | NO FE caller (FE uses `inbox-issues` alias) → `fe_evidence: []`, record grep                                                                                                                                               |
| `/inbox-issues/`                                           | `apps/web/core/services/inbox/inbox-issue.service.ts` `list`, `create`; `packages/services/src/intake/issue.service.ts` `list`                                                                                             |
| `/inbox-issues/:pk/`                                       | `apps/web/core/services/inbox/inbox-issue.service.ts` `retrieve`, `update`, `updateIssue`, `destroy`                                                                                                                       |
| `/intake-work-items/:work_item_id/description-versions/`   | `apps/web/core/services/inbox/intake-work_item_version.service.ts` `listDescriptionVersions`                                                                                                                               |
| `.../description-versions/:pk/`                            | `apps/web/core/services/inbox/intake-work_item_version.service.ts` `retrieveDescriptionVersion`                                                                                                                            |
| `/states/`                                                 | `apps/web/core/services/project/project-state.service.ts` `getStates`, `createState`                                                                                                                                       |
| `/states/:pk/`                                             | `project-state.service.ts` `getState`, `updateState`, `patchState`, `deleteState`                                                                                                                                          |
| `/intake-state/`                                           | `project-state.service.ts` `getIntakeState`                                                                                                                                                                                |
| `/states/:pk/mark-default/`                                | `project-state.service.ts` `markDefault`                                                                                                                                                                                   |
| `/projects/:project_id/views/`                             | `apps/web/core/services/view.service.ts` `createView`, `getViews`                                                                                                                                                          |
| `/projects/:project_id/views/:pk/`                         | `view.service.ts` `patchView`, `deleteView`, `getViewDetails` — NOTE `getViewIssues` (:58) calls a `views/:pk/issues/` variant present in NEITHER Django NOR Rust → do NOT invent an entry; record in this entry's `notes` |
| `/workspaces/:slug/views/`                                 | `apps/web/core/services/workspace.service.ts` `createView` (:232), `getAllViews` (:256)                                                                                                                                    |
| `/workspaces/:slug/views/:pk/`                             | `workspace.service.ts` `updateView` (:240), `deleteView` (:248), `getViewDetails` (:264)                                                                                                                                   |
| `/workspaces/:slug/issues/`                                | `workspace.service.ts` `getViewIssues` (:272); `packages/services/src/workspace/view.service.ts` `getViewIssues` (:61)                                                                                                     |
| `/projects/:project_id/user-favorite-views/`               | `view.service.ts` `addViewToFavorites`                                                                                                                                                                                     |
| `/projects/:project_id/user-favorite-views/:view_id/`      | `view.service.ts` `removeViewFromFavorites`                                                                                                                                                                                |

## Overlap / dup risk with existing inventory (all verified distinct — no dups, but sibling-name confusion is real)

- `/api/workspaces/:slug/states/` (Batch H T5, `routes::workspace::ws_states`) ≠ `/api/workspaces/:slug/projects/:project_id/states/` (this batch). Both serve state lists; do NOT conflate.
- `/api/workspaces/:slug/projects/:project_id/project-views/` (Batch F T6, `project.py:93`) ≠ `/api/workspaces/:slug/projects/:project_id/views/` (this batch, `views.py:17`).
- `/api/workspaces/:slug/workspace-views/` (Batch H T8, `workspace.py:118`, POST-only) ≠ `/api/workspaces/:slug/views/` (this batch, GET+POST).
- `/api/workspaces/:slug/projects/:project_id/user-favorite-projects/` (Batch I T4) ≠ `.../user-favorite-views/` (this batch).
- `/api/workspaces/:slug/projects/:project_id/work-items/:work_item_id/description-versions/` (Batch G T9) is the ISSUE twin of `/intake-work-items/:work_item_id/description-versions/` (this batch) — same serializer family, different entity prefix; keep separate.
- The gate test `implemented_paths_are_registered_in_main_rs` (`route_inventory_test.rs:68`) asserts exact string match against `main.rs` — all 21 paths below were verified present verbatim.

## Tasks

### L1 — intake core: intakes + intake-issues + inboxes (6 entries) → new domain `intake`

| path                                                            | methods (Django)   | domains.\* key | django_source           | view                 | rust_handler hint                                                                  | fe_evidence hint                  | known risk                                                                                           |
| --------------------------------------------------------------- | ------------------ | -------------- | ----------------------- | -------------------- | ---------------------------------------------------------------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/intakes/`           | GET, POST          | `intake`       | `app/urls/intake.py:16` | `IntakeViewSet`      | `routes::intake::list` / `create` (main.rs:725-727)                                | none (grep)                       | list returns single object vs Rust array; POST 201 full vs `{id,name}` + 409 dup                     |
| `/api/workspaces/:slug/projects/:project_id/intakes/:pk/`       | GET, PATCH, DELETE | `intake`       | `app/urls/intake.py:21` | `IntakeViewSet`      | `routes::intake::detail` / `patch` / `destroy` (main.rs:729-733)                   | none (grep)                       | full-shape retrieve/patch vs trimmed Rust; DELETE guard identical                                    |
| `/api/workspaces/:slug/projects/:project_id/intake-issues/`     | GET, POST          | `intake`       | `app/urls/intake.py:26` | `IntakeIssueViewSet` | `routes::intake::list_issues` / `create_issue` (main.rs:745-747)                   | none (grep: FE uses inbox-issues) | POST 200 Django vs 201 Rust; pagination vs plain array                                               |
| `/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/` | GET, PATCH, DELETE | `intake`       | `app/urls/intake.py:31` | `IntakeIssueViewSet` | `routes::intake::detail_issue` / `patch_issue` / `destroy_issue` (main.rs:749-753) | none (grep)                       | patch guard strings `"Only admin or creator..."` / `"You cannot edit intake issues"`; delete cascade |
| `/api/workspaces/:slug/projects/:project_id/inboxes/`           | GET, POST          | `intake`       | `app/urls/intake.py:36` | `IntakeViewSet`      | `routes::intake::list` / `create` (main.rs:735-737)                                | none (grep)                       | same as intakes twin — status/notes must match intakes entries                                       |
| `/api/workspaces/:slug/projects/:project_id/inboxes/:pk/`       | GET, PATCH, DELETE | `intake`       | `app/urls/intake.py:41` | `IntakeViewSet`      | `routes::intake::detail` / `patch` / `destroy` (main.rs:739-743)                   | none (grep)                       | same as intakes/:pk/ twin                                                                            |

- [ ] **Step 1: Append the 6 entries** to a new `domains.intake` object (`rust_module: "routes/intake.rs"`), `batch_task: "Batch L T1"`, `out_scope: false`, `fe_pages: []`. Status/notes via the recipe using the deltas above (`routes/intake.rs` + `views/intake/base.py` line refs).
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/intakes/',
  '/api/workspaces/:slug/projects/:project_id/intakes/:pk/',
  '/api/workspaces/:slug/projects/:project_id/intake-issues/',
  '/api/workspaces/:slug/projects/:project_id/intake-issues/:pk/',
  '/api/workspaces/:slug/projects/:project_id/inboxes/',
  '/api/workspaces/:slug/projects/:project_id/inboxes/:pk/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('L1 present: 6/6')
"
```

Expected: `L1 present: 6/6`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory intake core (Batch L T1)"
```

### L2 — intake sub: inbox-issues + desc-versions (4 entries) → `intake` domain

| path                                                                                                   | methods (Django)   | domains.\* key | django_source           | view                                       | rust_handler hint                                                                  | fe_evidence hint                                                                                 | known risk                                                                             |
| ------------------------------------------------------------------------------------------------------ | ------------------ | -------------- | ----------------------- | ------------------------------------------ | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/inbox-issues/`                                             | GET, POST          | `intake`       | `app/urls/intake.py:46` | `IntakeIssueViewSet`                       | `routes::intake::list_issues` / `create_issue` (main.rs:755-757)                   | `inbox-issue.service.ts` `list`/`create`; `packages/services/src/intake/issue.service.ts` `list` | only the FE-facing alias — status/notes mirror intake-issues entries                   |
| `/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/`                                         | GET, PATCH, DELETE | `intake`       | `app/urls/intake.py:51` | `IntakeIssueViewSet`                       | `routes::intake::detail_issue` / `patch_issue` / `destroy_issue` (main.rs:759-762) | `inbox-issue.service.ts` `retrieve`/`update`/`updateIssue`/`destroy`                             | `?expand=issue_inbox` query on FE retrieve — tripwire strips query strings, fine       |
| `/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/`     | GET                | `intake`       | `app/urls/intake.py:56` | `IntakeWorkItemDescriptionVersionEndpoint` | `routes::versions::intake_desc_versions_list` (main.rs:335-337)                    | `intake-work_item_version.service.ts` `listDescriptionVersions`                                  | cursor pagination; guest gate `base.py:581-600`; compare vs Batch G T9 work-items twin |
| `/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/:pk/` | GET                | `intake`       | `app/urls/intake.py:61` | `IntakeWorkItemDescriptionVersionEndpoint` | `routes::versions::intake_desc_version_detail` (main.rs:339-341)                   | `intake-work_item_version.service.ts` `retrieveDescriptionVersion`                               | same                                                                                   |

- [ ] **Step 1: Append the 4 entries** to `domains.intake` (`batch_task: "Batch L T2"`, `out_scope: false`, `fe_pages: []`). Status/notes via recipe.
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/inbox-issues/',
  '/api/workspaces/:slug/projects/:project_id/inbox-issues/:pk/',
  '/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/',
  '/api/workspaces/:slug/projects/:project_id/intake-work-items/:work_item_id/description-versions/:pk/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('L2 present: 4/4')
"
```

Expected: `L2 present: 4/4`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory intake inbox issues + versions (Batch L T2)"
```

### L3 — state (4 entries) → new domain `state`

| path                                                                  | methods (Django)   | domains.\* key | django_source          | view                           | rust_handler hint                                               | fe_evidence hint                                                               | known risk                                                                                                    |
| --------------------------------------------------------------------- | ------------------ | -------------- | ---------------------- | ------------------------------ | --------------------------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/states/`                  | GET, POST          | `state`        | `app/urls/state.py:12` | `StateViewSet`                 | `routes::state::list` / `create` (main.rs:625-627)              | `project-state.service.ts` `getStates`/`createState`                           | `grouped=true` mode + per-group `order` in Django; dup-name 400 `{"name": "The state name is already taken"}` |
| `/api/workspaces/:slug/projects/:project_id/states/:pk/`              | GET, PATCH, DELETE | `state`        | `app/urls/state.py:17` | `StateViewSet`                 | `routes::state::detail` / `patch` / `destroy` (main.rs:629-633) | `project-state.service.ts` `getState`/`updateState`/`patchState`/`deleteState` | destroy guard strings identical; NOT the ws-level `/states/` (Batch H T5)                                     |
| `/api/workspaces/:slug/projects/:project_id/intake-state/`            | GET                | `state`        | `app/urls/state.py:22` | `IntakeStateEndpoint`          | `routes::state::intake_state` (main.rs:647-648)                 | `project-state.service.ts` `getIntakeState`                                    | 404 `{"error": "Triage state not found"}`; triage-state lookup quirk (group='triage')                         |
| `/api/workspaces/:slug/projects/:project_id/states/:pk/mark-default/` | POST               | `state`        | `app/urls/state.py:27` | `StateViewSet.mark_as_default` | `routes::state::mark_default` (main.rs:638-639)                 | `project-state.service.ts` `markDefault`                                       | POST-only action; 204 both sides; ADMIN gate Django vs role guard Rust                                        |

- [ ] **Step 1: Append the 4 entries** to a new `domains.state` object (`rust_module: "routes/state.rs"`), `batch_task: "Batch L T3"`, `out_scope: false`, `fe_pages: []`. Status/notes via recipe (`routes/state.rs` + `views/state/base.py` line refs).
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/states/',
  '/api/workspaces/:slug/projects/:project_id/states/:pk/',
  '/api/workspaces/:slug/projects/:project_id/intake-state/',
  '/api/workspaces/:slug/projects/:project_id/states/:pk/mark-default/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('L3 present: 4/4')
"
```

Expected: `L3 present: 4/4`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory states + intake-state (Batch L T3)"
```

### L4 — views (7 entries) → new domain `views`

| path                                                                       | methods (Django)        | domains.\* key | django_source          | view                         | rust_handler hint                                                    | fe_evidence hint                                                                                          | known risk                                                                                                                                                    |
| -------------------------------------------------------------------------- | ----------------------- | -------------- | ---------------------- | ---------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/views/`                        | GET, POST               | `views`        | `app/urls/views.py:17` | `IssueViewViewSet`           | `routes::view::list` / `create` (main.rs:835-837)                    | `view.service.ts` `createView`/`getViews`                                                                 | NOT `project-views/` (Batch F T6); guest `owned_by` filter + `fields` param                                                                                   |
| `/api/workspaces/:slug/projects/:project_id/views/:pk/`                    | GET, PUT, PATCH, DELETE | `views`        | `app/urls/views.py:22` | `IssueViewViewSet`           | `routes::view::detail` / `patch` / `destroy` (main.rs:838-843)       | `view.service.ts` `patchView`/`deleteView`/`getViewDetails`                                               | **Django PUT (`views.py:27`) has NO Rust handler** → `shape_mismatch`; FE `getViewIssues` calls `views/:pk/issues/` — NEITHER Django NOR Rust: note, no entry |
| `/api/workspaces/:slug/views/`                                             | GET, POST               | `views`        | `app/urls/views.py:34` | `WorkspaceViewViewSet`       | `routes::view::list_global` / `create_global` (main.rs:845-847)      | `workspace.service.ts` `createView`/`getAllViews`                                                         | NOT `workspace-views/` (Batch H T8); WORKSPACE-level gates                                                                                                    |
| `/api/workspaces/:slug/views/:pk/`                                         | GET, PUT, PATCH, DELETE | `views`        | `app/urls/views.py:39` | `WorkspaceViewViewSet`       | `routes::view::detail_global` ONLY (main.rs:849-851)                 | `workspace.service.ts` `updateView`/`deleteView`/`getViewDetails`                                         | **Django PUT/PATCH/DELETE have NO Rust handlers** (no patch_global/destroy_global in `view.rs`) → `shape_mismatch`, list all 3 missing methods                |
| `/api/workspaces/:slug/issues/`                                            | GET                     | `views`        | `app/urls/views.py:51` | `WorkspaceViewIssuesViewSet` | `routes::issue_lists::workspace_issues` (main.rs:74)                 | `workspace.service.ts` `getViewIssues`; `packages/services/src/workspace/view.service.ts` `getViewIssues` | gzip + pagination + permission filters vs Rust shape — likely `shape_mismatch`                                                                                |
| `/api/workspaces/:slug/projects/:project_id/user-favorite-views/`          | GET, POST               | `views`        | `app/urls/views.py:56` | `IssueViewFavoriteViewSet`   | `routes::view::list_favorites` / `create_favorite` (main.rs:916-917) | `view.service.ts` `addViewToFavorites`                                                                    | **Django GET → 500** (no `serializer_class`, `view/base.py:407-417`); Rust GET is a superset → record GET+POST, note the 500                                  |
| `/api/workspaces/:slug/projects/:project_id/user-favorite-views/:view_id/` | DELETE                  | `views`        | `app/urls/views.py:61` | `IssueViewFavoriteViewSet`   | `routes::favorite::view_fav_destroy` (main.rs:926-927)               | `view.service.ts` `removeViewFromFavorites`                                                               | DELETE-only; 204 hard (`soft=False`)                                                                                                                          |

- [ ] **Step 1: Append the 7 entries** to a new `domains.views` object (`rust_module: "routes/view.rs"`), `batch_task: "Batch L T4"`, `out_scope: false`, `fe_pages: []`. Status/notes via recipe (`routes/view.rs`, `routes/issue_lists.rs`, `routes/favorite.rs` + `views/view/base.py` line refs).
- [ ] **Step 2: Validate JSON syntax**

Run: `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null`
Expected: no output, exit 0.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test`
Expected: PASS (3 + 3).

- [ ] **Step 4: Presence check**

```bash
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/views/',
  '/api/workspaces/:slug/projects/:project_id/views/:pk/',
  '/api/workspaces/:slug/views/',
  '/api/workspaces/:slug/views/:pk/',
  '/api/workspaces/:slug/issues/',
  '/api/workspaces/:slug/projects/:project_id/user-favorite-views/',
  '/api/workspaces/:slug/projects/:project_id/user-favorite-views/:view_id/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('L4 present: 7/7')
"
```

Expected: `L4 present: 7/7`.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(rs-api): inventory views domain (Batch L T4)"
```

### L5 — final audit + full suite

**Files:** none (verification only; fix forward with a new commit only if the audit finds a gap)

- [ ] **Step 1: Full domain audit — every intake/state/views.py path inventoried**

```bash
python3 -c "
import json, re
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
have = set(ep['path'] for d in inv['domains'].values() for ep in d['endpoints'])
def canon(s):
    return '/api/' + re.sub(r'<(?:str|uuid|int):(\w+)>', r':\1', s)
total = 0
for fn in ['intake.py', 'state.py', 'views.py']:
    src = open(f'apps/api/plane/app/urls/{fn}').read()
    dpaths = set()
    for m in re.finditer(r'path\(\s*(?:[ru]?\"([^\"]+)\"|ru?\'([^\']+)\')', src):
        dpaths.add(canon(m.group(1) or m.group(2)))
    missing = sorted(dpaths - have)
    assert not missing, f'{fn}: missing {missing}'
    total += len(dpaths)
    print(f'{fn}: {len(dpaths)} paths, all inventoried')
print('total paths:', total)
for dom in ['intake', 'state', 'views']:
    n = len(inv['domains'][dom]['endpoints'])
    print(f'domain {dom}: {n} entries')
    assert n > 0
assert total == 21
print('L5 audit: OK')
"
```

Expected: `intake.py: 10 paths, all inventoried`, `state.py: 4 paths, all inventoried`, `views.py: 7 paths, all inventoried`, `total paths: 21`, `domain intake: 10 entries`, `domain state: 4 entries`, `domain views: 7 entries`, `L5 audit: OK`.

- [ ] **Step 2: Run the full api test suite**

Run: `cargo test -p api`
Expected: 0 failed.

- [ ] **Step 3: Report**

No commit unless the audit forces a fix (then `fix(rs-api): inventory audit gap (Batch L T5)`). Report audit output + suite result.

## Final state after Batch L

- `intake` domain: 10 entries, `state`: 4, `views`: 7 — every path in the 3 urls files tracked.
- 3 new domains (`rust_module`: `routes/intake.rs`, `routes/state.rs`, `routes/view.rs`); total domains 7 → 10.
- Any `shape_mismatch` found is RECORDED with notes, NOT fixed (follow-up Batch L-fix plan) — expected: all intakes/intake-issues twins, states list/detail/patch, project-views detail (PUT), workspace-views detail (PUT/PATCH/DELETE), user-favorite-views GET 500.

## Out of scope

- Fixing handlers for any `shape_mismatch` found (follow-up plan).
- Other domains (`cycle`, `module`, `page`, `user`, …) and urls files (`issue.py`, `cycle.py`, `module.py`, `page.py`, `estimate.py`, `inbox.py`-adjacent files, `search.py`, `webhook.py`, …) — later batches.
- `project-views/` (Batch F T6), `workspace-views/` (Batch H T8), `/api/workspaces/:slug/states/` (Batch H T5) — already inventoried, out of scope here.
- Changing gate tests, shadow.sh, or FE code.

## Self-review notes

- Spec coverage: 21/21 paths across the 3 urls files appear in exactly one task table; canonicalization matches the gate's exact-string compare (verified each against `main.rs` verbatim). Standing rules (status recipe, methods never scope-down, `fe_pages: []`, commit format) carried over from Batch H/I.
- Placeholder scan: every table cell carries a concrete hint (view class, `main.rs` line, handler name, FE service+method or "none (grep)"); no TBD. Pre-mined deltas tell the executor where the recipe will land but the recipe still decides `rust_status`.
- Type consistency: `batch_task` `"Batch L T1"`–`"Batch L T5"` unique; 3 new domain keys (`intake`, `state`, `views`) consistent across tables, audits, and presence checks; domain counts 10+4+7=21.
- Count check: L1 6 + L2 4 + L3 4 + L4 7 = 21 new entries; `domains` 7 → 10. FE evidence never references methods/files that don't exist (all verified by grep at research time; tripwire re-verifies).
