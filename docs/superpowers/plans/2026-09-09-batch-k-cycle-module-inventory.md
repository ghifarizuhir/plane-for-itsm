# Batch K — cycle & module domain inventory expansion

Date: 2026-09-09 | Branch: `preview` | Label: `Batch K Tn`

## Source

- `apps/api/plane/app/urls/cycle.py` — 14 `path()` entries (lines 23–102). **0 inventoried** → new `domains.cycle` (14 entries).
- `apps/api/plane/app/urls/module.py` — 13 `path()` entries (lines 20–101). **0 inventoried** → new `domains.module` (13 entries).

Canonicalization: `<str:slug>` → `:slug`, `<uuid:project_id>` → `:project_id`, `<uuid:pk>` → `:pk`, `<uuid:cycle_id>` → `:cycle_id`, `<uuid:issue_id>` → `:issue_id`, `<uuid:module_id>` → `:module_id`. No type-less params in either file. Canonical paths match the exact strings already registered in `main.rs` (verified — no router rename needed).

New domain objects (create at `domains` level, alongside `issue`/`project`/…): `"cycle": { "rust_module": "routes/cycle.rs", "endpoints": [...] }` and `"module": { "rust_module": "routes/module.rs", "endpoints": [...] }`. The gate test iterates `domains` generically (`route_inventory_test.rs:36-40`) — new keys are safe.

**Research findings (pre-verified):** every one of the 27 paths has at least a partial Rust route — **0 paths with NO Rust handler**. 5 paths are partial-coverage `shape_mismatch` candidates (Django serves methods Rust does not wire): cycle detail (PUT), cycle-issue detail (GET/PUT/PATCH), favorite-cycles list (GET), module-issue detail (GET/PUT/PATCH), favorite-modules list (GET). None of the 27 overlap existing inventory entries (verified against all 7 existing domains). FE service evidence: `cycle.service.ts`, `module.service.ts`, `cycle_archive.service.ts`, `module_archive.service.ts`, `issue_filter.service.ts` + `packages/services` twins.

## Tasks

### K1 — cycle core (4 entries) → `domains.cycle`

| path                                                                           | methods (Django)        | domains.\* key | django_source          | view                       | rust_handler hint                                                                    | fe_evidence hint                                                                                                                        | known risk                                                                                                  |
| ------------------------------------------------------------------------------ | ----------------------- | -------------- | ---------------------- | -------------------------- | ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/cycles/`                           | GET, POST               | cycle          | `app/urls/cycle.py:23` | CycleViewSet               | `routes::cycle::list` / `create` (main.rs:423-425)                                   | `createCycle`, `getCyclesWithParams` @ apps/web/core/services/cycle.service.ts (+ `packages/services/src/cycle/cycle.service.ts` twins) | list 200 shape + started/unstarted/backlog counts; create 201 serializer                                    |
| `/api/workspaces/:slug/projects/:project_id/cycles/:pk/`                       | GET, PUT, PATCH, DELETE | cycle          | `app/urls/cycle.py:28` | CycleViewSet               | `routes::cycle::detail` / `patch` / `destroy` (main.rs:427-431) — **NO PUT handler** | `getCycleDetails`, `patchCycle`, `deleteCycle` @ cycle.service.ts (PUT has no FE caller)                                                | **PUT missing in Rust** → record method + shape_mismatch; retrieve detail shape (distribution)              |
| `/api/workspaces/:slug/projects/:project_id/cycles/date-check/`                | POST                    | cycle          | `app/urls/cycle.py:57` | CycleDateCheckEndpoint     | `routes::cycle::date_check` (main.rs:454-456)                                        | `cycleDateCheck` @ cycle.service.ts                                                                                                     | overlap → **200** `{status:false}` verbatim (NOT 4xx); AM gate                                              |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/transfer-issues/` | POST                    | cycle          | `app/urls/cycle.py:72` | TransferCycleIssueEndpoint | `routes::cycle::transfer` (main.rs:476-478)                                          | `transferIssues` @ cycle.service.ts                                                                                                     | 200 `{"message":"Success"}`; progress-snapshot written on SOURCE cycle; backlog/unstarted/started-only move |

- [ ] Append 4 entries (`batch_task: "Batch K T1"`, 10 keys, `fe_pages: []` default) to new `domains.cycle.endpoints`; create the `domains.cycle` object (`rust_module: "routes/cycle.rs"`)
- [ ] `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 4/4:

```python
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/projects/:project_id/cycles/',
  '/api/workspaces/:slug/projects/:project_id/cycles/:pk/',
  '/api/workspaces/:slug/projects/:project_id/cycles/date-check/',
  '/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/transfer-issues/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('K1 present: 4/4')
"
```

- [ ] Commit `feat(rs-api): inventory cycle core (Batch K T1)`

### K2 — cycle issues + favorites (4 entries) → `domains.cycle`

| path                                                                                  | methods (Django)        | domains.\* key | django_source          | view                 | rust_handler hint                                                                       | fe_evidence hint                                                | known risk                                                                                                                                                                                                        |
| ------------------------------------------------------------------------------------- | ----------------------- | -------------- | ---------------------- | -------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/cycle-issues/`           | GET, POST               | cycle          | `app/urls/cycle.py:40` | CycleIssueViewSet    | `routes::cycle::cycle_issues_list` / `cycle_issues_create` (main.rs:439-441)            | `getCycleIssues` @ cycle.service.ts (+ packages twin, GET+POST) | grouped pagination: `group_by==sub_group_by` → 400 verbatim; Rust flat envelope only (grouped 200 dict OUT — same class as archived-issues Batch G T8); POST 201 `{"message":"success"}`                          |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/cycle-issues/:issue_id/` | GET, PUT, PATCH, DELETE | cycle          | `app/urls/cycle.py:45` | CycleIssueViewSet    | `routes::cycle::cycle_issue_destroy` only (main.rs:446-448) — **GET/PUT/PATCH missing** | DELETE via cycle-issue remove callers (verify grep)             | **methods never scope down**: record GET/PUT/PATCH/DELETE + shape_mismatch; DELETE 204 always even 0 rows (soft-delete); no FE GET/PUT caller expected                                                            |
| `/api/workspaces/:slug/projects/:project_id/user-favorite-cycles/`                    | GET, POST               | cycle          | `app/urls/cycle.py:62` | CycleFavoriteViewSet | `routes::cycle::fav_create` only (main.rs:463-465) — **GET missing**                    | `addCycleToFavorites` @ cycle.service.ts                        | **GET list served by Django (default DRF list) but not wired in Rust** (documented E2 contract, main.rs:457-462) → shape_mismatch; POST 204, dup → 400 `{"error":"The payload is not valid"}`, NO existence check |
| `/api/workspaces/:slug/projects/:project_id/user-favorite-cycles/:cycle_id/`          | DELETE                  | cycle          | `app/urls/cycle.py:67` | CycleFavoriteViewSet | `routes::cycle::fav_destroy` (main.rs:467-469)                                          | `removeCycleFromFavorites` @ cycle.service.ts                   | DELETE 204, miss → 404; AM gate                                                                                                                                                                                   |

- [ ] Append 4 entries (`batch_task: "Batch K T2"`) after K1 entries
- [ ] `python3 -m json.tool` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 4/4 (paths: `.../cycles/:cycle_id/cycle-issues/`, `.../cycles/:cycle_id/cycle-issues/:issue_id/`, `.../user-favorite-cycles/`, `.../user-favorite-cycles/:cycle_id/`)
- [ ] Commit `feat(rs-api): inventory cycle issues/favorites (Batch K T2)`

### K3 — cycle archive + analytics + prefs (6 entries) → `domains.cycle`

| path                                                                           | methods (Django) | domains.\* key | django_source           | view                          | rust_handler hint                                                            | fe_evidence hint                                                                                    | known risk                                                                                                                                                                                                                                        |
| ------------------------------------------------------------------------------ | ---------------- | -------------- | ----------------------- | ----------------------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/user-properties/` | GET, PATCH       | cycle          | `app/urls/cycle.py:77`  | CycleUserPropertiesEndpoint   | `routes::userprops::cycle_props_get` / `cycle_props_patch` (main.rs:279-281) | `fetchCycleIssueFilters`, `patchCycleIssueFilters` @ apps/web/core/services/issue_filter.service.ts | GET 200 get_or_create; PATCH **201** semantics, missing row → 404; PATCH merges only 4 filter keys; AMG gate                                                                                                                                      |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/archive/`         | POST, DELETE     | cycle          | `app/urls/cycle.py:82`  | CycleArchiveUnarchiveEndpoint | `routes::cycle::archive` / `unarchive` (main.rs:497-499)                     | `archiveCycle`, `restoreCycle` @ cycle_archive.service.ts                                           | POST 200 `{"archived_at"}` (non-completed → 400 verbatim); DELETE 204; NO GET on this path (Django defines none — do not add)                                                                                                                     |
| `/api/workspaces/:slug/projects/:project_id/archived-cycles/`                  | GET              | cycle          | `app/urls/cycle.py:87`  | CycleArchiveUnarchiveEndpoint | `routes::cycle::archived_list` (main.rs:485-487)                             | `getArchivedCycles` @ cycle_archive.service.ts (+ packages twin)                                    | archived-only scope; list OMITS logo_props/version/created_by; GET on this class serves `pk=None` branch (archive.py:272)                                                                                                                         |
| `/api/workspaces/:slug/projects/:project_id/archived-cycles/:pk/`              | GET              | cycle          | `app/urls/cycle.py:92`  | CycleArchiveUnarchiveEndpoint | `routes::cycle::archived_detail` (main.rs:489-491)                           | `getArchivedCycleDetails` @ cycle_archive.service.ts (+ packages twin)                              | detail + distribution/estimate_distribution; miss 404                                                                                                                                                                                             |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/progress/`        | GET              | cycle          | `app/urls/cycle.py:97`  | CycleProgressEndpoint         | `routes::cycle::progress` (main.rs:505-507)                                  | `workspaceActiveCyclesProgress` @ cycle.service.ts                                                  | 12 keys, snapshot counts win, total may be null; AMG gate; **FE-only variant `/cycles/:id/cycle-progress/`** (cycle.service.ts:57 `workspaceActiveCyclesProgressPro`) exists in NEITHER Django nor Rust — record in notes, do NOT invent an entry |
| `/api/workspaces/:slug/projects/:project_id/cycles/:cycle_id/analytics/`       | GET              | cycle          | `app/urls/cycle.py:102` | CycleAnalyticsEndpoint        | `routes::cycle::analytics` (main.rs:513-515)                                 | `workspaceActiveCyclesAnalytics` @ cycle.service.ts (`?type=` param)                                | GET `{assignees,labels,completion_chart}` snapshot-or-live; points branch only with points estimate; AMG gate                                                                                                                                     |

- [ ] Append 6 entries (`batch_task: "Batch K T3"`) after K2 entries
- [ ] `python3 -m json.tool` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 6/6 (paths: `.../cycles/:cycle_id/user-properties/`, `.../cycles/:cycle_id/archive/`, `.../archived-cycles/`, `.../archived-cycles/:pk/`, `.../cycles/:cycle_id/progress/`, `.../cycles/:cycle_id/analytics/`)
- [ ] Commit `feat(rs-api): inventory cycle archive/analytics (Batch K T3)`

### K4 — module core (5 entries) → `domains.module`

| path                                                                              | methods (Django)        | domains.\* key | django_source           | view               | rust_handler hint                                                                           | fe_evidence hint                                                                                               | known risk                                                                                                                                                                                             |
| --------------------------------------------------------------------------------- | ----------------------- | -------------- | ----------------------- | ------------------ | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/api/workspaces/:slug/projects/:project_id/modules/`                             | GET, POST               | module         | `app/urls/module.py:20` | ModuleViewSet      | `routes::module::list` / `create` (main.rs:524-526)                                         | `getModules`, `createModule` @ module.service.ts (+ packages twins)                                            | list 200 shape; create 201 serializer                                                                                                                                                                  |
| `/api/workspaces/:slug/projects/:project_id/modules/:pk/`                         | GET, PUT, PATCH, DELETE | module         | `app/urls/module.py:25` | ModuleViewSet      | `routes::module::detail` / `update` / `patch` / `destroy` (main.rs:528-533) — **PUT wired** | `updateModule` (PUT!), `getModuleDetails`, `patchModule`, `deleteModule` @ module.service.ts (+ packages twin) | full method parity; verify update (PUT) 200 vs partial_update semantics                                                                                                                                |
| `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/modules/`            | POST                    | module         | `app/urls/module.py:37` | ModuleIssueViewSet | `routes::module::issue_modules_create` (main.rs:550-551)                                    | `addModulesToIssue` @ module.service.ts (+ packages twin)                                                      | POST 201 `{"message":"success"}` always even empty lists; added modules NOT scoped (replicated as-is); AM gate                                                                                         |
| `/api/workspaces/:slug/projects/:project_id/modules/:module_id/issues/`           | GET, POST               | module         | `app/urls/module.py:42` | ModuleIssueViewSet | `routes::module::issues_list` / `issues_create` (main.rs:542-544)                           | `getModuleIssues`, `addIssuesToModule` @ module.service.ts (+ packages twins)                                  | grouped pagination same quirk as cycle-issues (`group_by==sub_group_by` → 400 verbatim; Rust flat only); POST 201 `{"message":"success"}`, issues re-scoped to ws+project, unknown silently dropped    |
| `/api/workspaces/:slug/projects/:project_id/modules/:module_id/issues/:issue_id/` | GET, PUT, PATCH, DELETE | module         | `app/urls/module.py:47` | ModuleIssueViewSet | `routes::module::issue_destroy` only (main.rs:559-561) — **GET/PUT/PATCH missing**          | DELETE via `removeIssuesFromModuleBulk` @ module.service.ts (+ packages twin)                                  | **methods never scope down**: record GET/PUT/PATCH/DELETE + shape_mismatch; DELETE 204 always even 0 rows (Django `.first().module` crash normalized to idempotent 204 in Rust — documented deviation) |

- [ ] Append 5 entries (`batch_task: "Batch K T4"`) to new `domains.module.endpoints`; create the `domains.module` object (`rust_module: "routes/module.rs"`)
- [ ] `python3 -m json.tool` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 5/5 (paths: `.../modules/`, `.../modules/:pk/`, `.../issues/:issue_id/modules/`, `.../modules/:module_id/issues/`, `.../modules/:module_id/issues/:issue_id/`)
- [ ] Commit `feat(rs-api): inventory module core (Batch K T4)`

### K5 — module sub-resources (8 entries) → `domains.module`

| path                                                                              | methods (Django)        | domains.\* key | django_source            | view                           | rust_handler hint                                                                            | fe_evidence hint                                                                                                  | known risk                                                                                                                                                                                                               |
| --------------------------------------------------------------------------------- | ----------------------- | -------------- | ------------------------ | ------------------------------ | -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/api/workspaces/:slug/projects/:project_id/modules/:module_id/module-links/`     | GET, POST               | module         | `app/urls/module.py:59`  | ModuleLinkViewSet              | `routes::module::links_list` / `links_create` (main.rs:568-570)                              | `createModuleLink` @ module.service.ts; `createLink` @ packages/services/src/module/link.service.ts               | GET order `-created_at`; POST 201 prepends `http://` when scheme missing; SAFE gate = any active member incl GUEST, unsafe = AM                                                                                          |
| `/api/workspaces/:slug/projects/:project_id/modules/:module_id/module-links/:pk/` | GET, PUT, PATCH, DELETE | module         | `app/urls/module.py:64`  | ModuleLinkViewSet              | `routes::module::link_detail` / `link_put` / `link_patch` / `link_destroy` (main.rs:576-581) | `updateModuleLink`, `deleteModuleLink` @ module.service.ts; `updateLink`, `deleteLink` @ packages link.service.ts | dup url on update → 400 sic `"URL already exists for this Issue"`; bad url → 400 field errors                                                                                                                            |
| `/api/workspaces/:slug/projects/:project_id/user-favorite-modules/`               | GET, POST               | module         | `app/urls/module.py:76`  | ModuleFavoriteViewSet          | `routes::module::fav_create` only (main.rs:589-591) — **GET missing**                        | `addModuleToFavorites` @ module.service.ts (+ packages twin)                                                      | **GET list served by Django (default DRF list) but not wired in Rust** (documented E3 contract, main.rs:582-587) → shape_mismatch; POST 204, dup → 400 `{"error":"The payload is not valid"}`, NO module-existence check |
| `/api/workspaces/:slug/projects/:project_id/user-favorite-modules/:module_id/`    | DELETE                  | module         | `app/urls/module.py:81`  | ModuleFavoriteViewSet          | `routes::module::fav_destroy` (main.rs:593-595)                                              | `removeModuleFromFavorites` @ module.service.ts (+ packages twin)                                                 | DELETE 204, miss → 404; Lite gate                                                                                                                                                                                        |
| `/api/workspaces/:slug/projects/:project_id/modules/:module_id/user-properties/`  | GET, PATCH              | module         | `app/urls/module.py:86`  | ModuleUserPropertiesEndpoint   | `routes::userprops::module_props_get` / `module_props_patch` (main.rs:287-289)               | `fetchModuleIssueFilters`, `patchModuleIssueFilters` @ issue_filter.service.ts                                    | same 200/201/404 semantics as cycle twin; AMG gate                                                                                                                                                                       |
| `/api/workspaces/:slug/projects/:project_id/modules/:module_id/archive/`          | POST, DELETE            | module         | `app/urls/module.py:91`  | ModuleArchiveUnarchiveEndpoint | `routes::module::archive` / `unarchive` (main.rs:601-603)                                    | `archiveModule`, `restoreModule` @ module_archive.service.ts                                                      | POST 200 `{"archived_at"}` (wrong status → 400 verbatim); DELETE 204 (no status check); NO GET on this path                                                                                                              |
| `/api/workspaces/:slug/projects/:project_id/archived-modules/`                    | GET                     | module         | `app/urls/module.py:96`  | ModuleArchiveUnarchiveEndpoint | `routes::module::archived_list` (main.rs:611-613)                                            | `getArchivedModules` @ module_archive.service.ts                                                                  | archived-only scope; list OMITS logo_props/estimate_points; GET serves `pk=None` branch (archive.py:258)                                                                                                                 |
| `/api/workspaces/:slug/projects/:project_id/archived-modules/:pk/`                | GET                     | module         | `app/urls/module.py:101` | ModuleArchiveUnarchiveEndpoint | `routes::module::archived_detail` (main.rs:615-617)                                          | `getArchivedModuleDetails` @ module_archive.service.ts                                                            | detail + link/sub-issues/distribution/estimate_distribution; miss 404; SAFE gate any active member                                                                                                                       |

- [ ] Append 8 entries (`batch_task: "Batch K T5"`) after K4 entries
- [ ] `python3 -m json.tool` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 8/8 (paths: `.../module-links/`, `.../module-links/:pk/`, `.../user-favorite-modules/`, `.../user-favorite-modules/:module_id/`, `.../modules/:module_id/user-properties/`, `.../modules/:module_id/archive/`, `.../archived-modules/`, `.../archived-modules/:pk/`)
- [ ] Commit `feat(rs-api): inventory module sub-resources (Batch K T5)`

### K6 — final audit + full suite

- [ ] Audit: all 27 paths inventoried (14 cycle.py + 13 module.py); `domains.cycle` has exactly 14 entries; `domains.module` has exactly 13 entries; no dup across domains:

```python
python3 -c "
import json, re
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
have = set(ep['path'] for d in inv['domains'].values() for ep in d['endpoints'])
assert len(have) == sum(len(d['endpoints']) for d in inv['domains'].values()), 'dup paths across domains'
for f, dom in [('apps/api/plane/app/urls/cycle.py', 'cycle'), ('apps/api/plane/app/urls/module.py', 'module')]:
    src = open(f).read()
    def canon(s):
        return '/api/' + re.sub(r'<(?:str|uuid|int):(\w+)>', r':\1', s)
    dpaths = {canon(m.group(1) or m.group(2)) for m in re.finditer(r'path\(\s*(?:[ru]?\"([^\"]+)\"|ru?\'([^\']+)\')', src)}
    missing = sorted(dpaths - have)
    assert not missing, (f, missing)
    n = len(inv['domains'][dom]['endpoints'])
    print(f.split('/')[-1], 'paths:', len(dpaths), '| inventoried:', len(dpaths & have), '|', dom, 'entries:', n)
    assert n == len(dpaths), (dom, n, len(dpaths))
print('K6 audit: OK')
"
```

- [ ] `cargo test -p api` — 0 failed. No commit (verification only).

## Standing rules (from Batch G/H/I)

- One task = one subagent → controller verifies (presence check + gates) → quality-review subagent → next task.
- Status recipe: (a) status codes, (b) response keys, (c) error strings, (d) permission gates. Any delta → `shape_mismatch` with `notes` citing `file:line` both sides; match → `implemented`, `notes` = `"verified <View> vs <handler>"`.
- `methods` = Django-served; never scope down (Django-only PUT/GET on cycle-detail, cycle-issue/module-issue detail, favorite-list paths → record fully + `shape_mismatch`, do NOT trim).
- `fe_pages: []` default; fill ONLY with single-page verification via grep.
- Entry keys (10): `methods`, `path`, `django_source`, `rust_status`, `rust_handler`, `fe_evidence`, `fe_pages`, `batch_task`, `out_scope: false`, `notes`.
- Commit format: `feat(rs-api): inventory <scope> (Batch K Tn)`; single-file commits (`apps/api-rs/crates/api/parity-inventory.json` only).
- FE-only URL variants (`/cycle-progress/`) → record in `notes` of the matching entry; never invent entries.
