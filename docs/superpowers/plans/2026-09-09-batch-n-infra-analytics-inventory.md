# Batch N — infra & analytics inventory expansion (asset / webhook / analytics)

Date: 2026-09-09 | Branch: `preview` | Label: `Batch N Tn`

## Source

- `apps/api/plane/app/urls/asset.py` — 18 `path()` entries (lines 27–113). **0 inventoried** → new `domains.asset` (18 entries).
- `apps/api/plane/app/urls/webhook.py` — 4 `path()` entries (lines 15–30). **0 inventoried** → new `domains.webhook` (4 entries).
- `apps/api/plane/app/urls/analytic.py` — 13 `path()` entries (lines 25–89). **4 already inventoried** (Batch F T9, `domains.analytics`) → **9 new entries** → `domains.analytics` grows 4 → 13.

**Already inventoried (do NOT re-add):**

analytic.py, Batch F T9 in `domains.analytics`:
| path | methods | inventory rust_handler |
|---|---|---|
| `/api/workspaces/:slug/analytics/` | GET | `routes::analytic::workspace_analytics` |
| `/api/workspaces/:slug/saved-analytic-view/:analytic_id/` | GET | `routes::analytic::saved_analytic` |
| `/api/workspaces/:slug/export-analytics/` | POST | `routes::analytic::export_analytics` |
| `/api/workspaces/:slug/analytic-view/:pk/` | GET, PATCH, DELETE | `routes::analytic::analytic_view_detail` |

asset.py: **no overlap**. The `/api/assets/v2/workspaces/:slug/projects/:project_id/issues/:issue_id/attachments/` + `/:pk/` pair lives in `apps/api/plane/app/urls/issue.py:137-146` (already in `domains.issue`, Batch G T10) — NOT in asset.py. All 18 asset.py paths are new.

Canonicalization: `<str:slug>` → `:slug`, `<uuid:asset_id>` → `:asset_id`, `<uuid:workspace_id>` → `:workspace_id`, `<uuid:project_id>` → `:project_id`, `<uuid:pk>` → `:pk`, `<uuid:webhook_id>` → `:webhook_id`, `<uuid:analytic_id>` → `:analytic_id`, `<uuid:entity_id>` → `:pk` (bulk only — see below).

**Param-name deviations — record main.rs names, NOT Django names** (the gate `route_inventory_test.rs:68-89` does an exact string match against `main.rs` routes; this batch is inventory-only, no router rename):

| Django url param          | main.rs route param | paths affected                                                                                                                               |
| ------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `<str:asset_key>`         | `:key`              | `/api/workspaces/file-assets/:workspace_id/:key/`, `/api/workspaces/file-assets/:workspace_id/:key/restore/`, `/api/users/file-assets/:key/` |
| `<uuid:entity_id>` (bulk) | `:pk`               | `/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/bulk/`                                                                             |

New domain objects (create at `domains` level, alongside `issue`/`project`/…): `"asset": { "rust_module": "routes/asset.rs", "endpoints": [...] }` and `"webhook": { "rust_module": "routes/webhook.rs", "endpoints": [...] }`. The gate test iterates `domains` generically (`route_inventory_test.rs:36-40`) — new keys are safe (Batch K precedent with `cycle`/`module`).

**Research findings (pre-verified):** 29 of 31 paths have an exact route string in `main.rs`. **2 paths have NO Rust handler** → `rust_status: "missing"` (NOT `shape_mismatch` — the gate only skips `missing` for the main.rs presence check):

- `/api/workspaces/:slug/file-assets/` (POST-only) — main.rs:1054-1057 deliberately excludes the legacy POST-create ("no FE caller").
- `/api/users/file-assets/` (POST-only) — same exclusion.

FE evidence tripwire (`fe_tripwire_test.rs:39-67`): every `fe_evidence` entry must point to a file containing BOTH the method name AND a URL template that matches the inventory path. The `advance-analytics*` (×6) endpoints have FE callers (`apps/web/core/components/analytics/*` via `AnalyticsService.getAdvanceAnalytics…`) but the URLs are built dynamically in `AnalyticsService.processUrl` (`apps/web/core/services/analytics.service.ts:91-111`) — NO literal URL exists anywhere in FE → **keep `fe_evidence: []`** and document the callers in `notes`. Do NOT cite `analytics.service.ts` as evidence (tripwire would fail).

## Tasks

### N1 — asset v2 core (10 entries) → `domains.asset`

| path                                                             | methods (Django)   | domains.\* key | django_source             | view                       | rust_handler hint                                                                        | fe_evidence hint                                                                                                                                                    | known risk                                                                                                                                                                                                                                                      |
| ---------------------------------------------------------------- | ------------------ | -------------- | ------------------------- | -------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/assets/v2/workspaces/:slug/`                               | POST               | asset          | `app/urls/asset.py:49-53` | WorkspaceFileAssetEndpoint | `routes::asset::ws_presign` (main.rs:986)                                                | `uploadWorkspaceAsset` @ apps/web/core/services/file.service.ts:72                                                                                                  | 200 `{upload_data, asset_id, asset_url}`; workspace AMG gate; WORKSPACE_LOGO → admin-only 403 "Only workspace admins can upload a workspace logo."; size clamp FILE_SIZE_LIMIT; entity_type must be in FileAsset.EntityTypeContext else 400                     |
| `/api/assets/v2/workspaces/:slug/:asset_id/`                     | GET, PATCH, DELETE | asset          | `app/urls/asset.py:54-58` | WorkspaceFileAssetEndpoint | `routes::asset::ws_get` / `mark_uploaded` / `soft_delete` (main.rs:987-992)              | `updateWorkspaceAssetUploadStatus` (PATCH), `deleteWorkspaceAsset` (DELETE) @ file.service.ts; GET via `getEditorAssetSrc` @ packages/utils/src/editor/common.ts:26 | GET = 302 redirect to presigned URL (not-uploaded → 404 "The requested asset could not be found."); PATCH 204 + Celery metadata task (skipped); DELETE 204 soft; project-bound asset → 403 "You don't have access to this asset."                               |
| `/api/assets/v2/user-assets/`                                    | POST               | asset          | `app/urls/asset.py:59-63` | UserAssetsV2Endpoint       | `routes::asset::user_presign` (main.rs:993)                                              | `uploadUserAsset` @ file.service.ts:184                                                                                                                             | entity_type must be USER_AVATAR\|USER_COVER else 400 "Invalid entity type."; 200 `{upload_data, asset_id, asset_url}`; **IsAuthenticated only — no workspace scope**                                                                                            |
| `/api/assets/v2/user-assets/:asset_id/`                          | PATCH, DELETE      | asset          | `app/urls/asset.py:64-68` | UserAssetsV2Endpoint       | `routes::asset::user_complete` / `user_delete` (main.rs:994-997)                         | `updateUserAssetUploadStatus`, `deleteUserAsset` @ file.service.ts                                                                                                  | scoped `user_id=request.user`; PATCH 204 marks uploaded + avatar/cover unlink; DELETE 204 soft + deleted_at; miss → Django 500 (Rust sane 404)                                                                                                                  |
| `/api/assets/v2/workspaces/:slug/restore/:asset_id/`             | POST               | asset          | `app/urls/asset.py:69-73` | AssetRestoreEndpoint       | `routes::asset::restore` (main.rs:1074-1077)                                             | `restoreNewAsset` @ file.service.ts:236                                                                                                                             | 204; `all_objects` (soft-deleted incl.) + workspace slug; miss → 404                                                                                                                                                                                            |
| `/api/assets/v2/static/:asset_id/`                               | GET                | asset          | `app/urls/asset.py:74-78` | StaticFileAssetEndpoint    | `routes::asset::static_get` (main.rs:998)                                                | — (no FE caller; URLs server-generated in avatar payloads)                                                                                                          | **AllowAny — public, no auth**; 404 not-uploaded; 400 invalid entity_type; 302 redirect w/ attachment disposition for script-capable MIME (XSS guard)                                                                                                           |
| `/api/assets/v2/workspaces/:slug/projects/:project_id/`          | POST               | asset          | `app/urls/asset.py:79-83` | ProjectAssetEndpoint       | `routes::asset::project_presign` (main.rs:999-1002)                                      | `uploadProjectAsset` @ file.service.ts:148                                                                                                                          | project AMG gate; `entity_identifier` binds issue/page/comment/draft; 200 `{upload_data, asset_id, asset_url}`                                                                                                                                                  |
| `/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/`      | GET, PATCH, DELETE | asset          | `app/urls/asset.py:84-88` | ProjectAssetEndpoint       | `routes::asset::project_get` / `project_complete` / `project_delete` (main.rs:1003-1008) | `updateProjectAssetUploadStatus` (PATCH) @ file.service.ts:107; GET via `getEditorAssetSrc` @ packages/utils/src/editor/common.ts:24; DELETE: no FE caller          | GET 302 (404 not-uploaded); PATCH 204; DELETE 204 soft; scoped `workspace__slug`+`project_id`; Django `pk` == main.rs `:pk` ✓                                                                                                                                   |
| `/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/bulk/` | POST               | asset          | `app/urls/asset.py:89-93` | ProjectBulkAssetEndpoint   | `routes::asset::bulk` (main.rs:1009-1012)                                                | `updateBulkProjectAssetsUploadStatus` @ file.service.ts:133                                                                                                         | **Django param `entity_id` vs main.rs `:pk`** → record `:pk`; 204; entity_type-driven association; IntegrityError swallowed (issue/comment/draft delete races); no asset → 404 "The requested asset could not be found."; no ids → 400 "No asset ids provided." |
| `/api/assets/v2/workspaces/:slug/check/:asset_id/`               | GET                | asset          | `app/urls/asset.py:94-98` | AssetCheckEndpoint         | `routes::asset::check` (main.rs:1070-1073)                                               | `checkIfAssetExists` @ file.service.ts:246                                                                                                                          | 200 `{"exists": bool}` over `all_objects` + `deleted_at IS NULL` (is_deleted-only rows still count); workspace AMG gate                                                                                                                                         |

- [ ] Append 10 entries (`batch_task: "Batch N T1"`, 10 keys, `fe_pages: []` default) to new `domains.asset.endpoints`; create the `domains.asset` object (`rust_module: "routes/asset.rs"`)
- [ ] `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 10/10:

```python
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/assets/v2/workspaces/:slug/',
  '/api/assets/v2/workspaces/:slug/:asset_id/',
  '/api/assets/v2/user-assets/',
  '/api/assets/v2/user-assets/:asset_id/',
  '/api/assets/v2/workspaces/:slug/restore/:asset_id/',
  '/api/assets/v2/static/:asset_id/',
  '/api/assets/v2/workspaces/:slug/projects/:project_id/',
  '/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/',
  '/api/assets/v2/workspaces/:slug/projects/:project_id/:pk/bulk/',
  '/api/assets/v2/workspaces/:slug/check/:asset_id/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('N1 present: 10/10')
"
```

- [ ] Commit `feat(rs-api): inventory asset v2 core (Batch N T1)`

### N2 — asset legacy + duplicate/downloads (8 entries) → `domains.asset`

| path                                                                       | methods (Django) | domains.\* key | django_source               | view                           | rust_handler hint                                                                              | fe_evidence hint                                                            | known risk                                                                                                                                                                                         |
| -------------------------------------------------------------------------- | ---------------- | -------------- | --------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/file-assets/`                                       | POST             | asset          | `app/urls/asset.py:27-31`   | FileAssetEndpoint              | **NO RUST HANDLER** → `rust_status: "missing"` (main.rs:1054-1057 excludes legacy POST-create) | — (no FE caller)                                                            | POST-only (GET/DELETE need `workspace_id`+`asset_key` kwargs → 500 on this path); multipart upload; WorkspaceMemberPermission                                                                      |
| `/api/workspaces/file-assets/:workspace_id/:key/`                          | GET, DELETE      | asset          | `app/urls/asset.py:32-36`   | FileAssetEndpoint              | `routes::asset::legacy_ws_get` / `legacy_ws_delete` (main.rs:1058-1061)                        | `deleteOldWorkspaceAsset` (DELETE) @ file.service.ts:218; GET: no FE caller | **Django `<str:asset_key>` vs main.rs `:key`** → record `:key`; GET miss → 200 `{error,status:False}` quirk (NOT 404); DELETE 204 soft (is_deleted only, no deleted_at); WorkspaceMemberPermission |
| `/api/users/file-assets/`                                                  | POST             | asset          | `app/urls/asset.py:37`      | UserAssetsEndpoint             | **NO RUST HANDLER** → `rust_status: "missing"`                                                 | — (no FE caller)                                                            | POST-only; **IsAuthenticated default — no workspace scope**; multipart; 201 serializer.data / 400 errors                                                                                           |
| `/api/users/file-assets/:key/`                                             | GET, DELETE      | asset          | `app/urls/asset.py:38-42`   | UserAssetsEndpoint             | `routes::asset::legacy_user_get` / `legacy_user_delete` (main.rs:1066-1069)                    | `deleteOldUserAsset` (DELETE) @ file.service.ts:227; GET: no FE caller      | **`asset_key` → `:key`**; GET 200 miss quirk; DELETE 204 soft; scoped `created_by=request.user`                                                                                                    |
| `/api/workspaces/file-assets/:workspace_id/:key/restore/`                  | POST             | asset          | `app/urls/asset.py:43-47`   | FileAssetViewSet               | `routes::asset::legacy_ws_restore` (main.rs:1062-1065)                                         | `restoreOldEditorAsset` @ file.service.ts:259                               | **`asset_key` → `:key`**; 204; `{post: restore}` action; WorkspaceMemberPermission                                                                                                                 |
| `/api/assets/v2/workspaces/:slug/duplicate-assets/:asset_id/`              | POST             | asset          | `app/urls/asset.py:99-103`  | DuplicateAssetEndpoint         | `routes::asset::duplicate` (main.rs:1013-1016)                                                 | `duplicateAsset` @ file.service.ts:284                                      | 200 `{asset_id}`; AssetRateThrottle (skipped per contract); 400 "Invalid entity type or entity id"; 404 "Project not found"/"Asset not found"; cross-workspace copy blocked                        |
| `/api/assets/v2/workspaces/:slug/download/:asset_id/`                      | GET              | asset          | `app/urls/asset.py:104-108` | WorkspaceAssetDownloadEndpoint | `routes::asset::ws_download` (main.rs:1017-1020)                                               | `getEditorAssetDownloadSrc` @ packages/utils/src/editor/common.ts:41        | 302 redirect, attachment disposition + filename; miss/not-uploaded → 404; workspace AMG gate                                                                                                       |
| `/api/assets/v2/workspaces/:slug/projects/:project_id/download/:asset_id/` | GET              | asset          | `app/urls/asset.py:109-113` | ProjectAssetDownloadEndpoint   | `routes::asset::project_download` (main.rs:1021-1024)                                          | `getEditorAssetDownloadSrc` @ packages/utils/src/editor/common.ts:39        | 302 redirect; 404 miss; **project AMG gate** (Django `@allow_permission(..., level="PROJECT")`)                                                                                                    |

- [ ] Append 8 entries (`batch_task: "Batch N T2"`, 10 keys, `fe_pages: []` default) after N1 entries
- [ ] `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 8/8:

```python
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/file-assets/',
  '/api/workspaces/file-assets/:workspace_id/:key/',
  '/api/users/file-assets/',
  '/api/users/file-assets/:key/',
  '/api/workspaces/file-assets/:workspace_id/:key/restore/',
  '/api/assets/v2/workspaces/:slug/duplicate-assets/:asset_id/',
  '/api/assets/v2/workspaces/:slug/download/:asset_id/',
  '/api/assets/v2/workspaces/:slug/projects/:project_id/download/:asset_id/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('N2 present: 8/8')
"
```

- [ ] Commit `feat(rs-api): inventory asset legacy + downloads (Batch N T2)`

### N3 — webhooks (4 entries) → `domains.webhook`

| path                                              | methods (Django)   | domains.\* key | django_source               | view                            | rust_handler hint                                                   | fe_evidence hint                                                                 | known risk                                                                                                                                                                                                                                                                                   |
| ------------------------------------------------- | ------------------ | -------------- | --------------------------- | ------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/webhooks/`                 | GET, POST          | webhook        | `app/urls/webhook.py:15-20` | WebhookEndpoint                 | `routes::webhook::list` / `create` (main.rs:1078-1081)              | `fetchWebhooksList`, `createWebhook` @ apps/web/core/services/webhook.service.ts | ADMIN-only gate; GET fields subset (id,url,is_active,created_at,updated_at,project,issue,cycle,module,issue_comment — NO secret); POST 201 w/ secret; dup (workspace,url) → **409** "URL already exists for the workspace"; URL validation http/https only, no localhost/127.0.0.1, max 1024 |
| `/api/workspaces/:slug/webhooks/:pk/`             | GET, PATCH, DELETE | webhook        | `app/urls/webhook.py:16-20` | WebhookEndpoint                 | `routes::webhook::detail` / `patch` / `destroy` (main.rs:1082-1087) | `fetchWebhookDetails`, `updateWebhook`, `deleteWebhook` @ webhook.service.ts     | ADMIN; GET/PATCH fields subset, no secret; DELETE 204; miss → 404                                                                                                                                                                                                                            |
| `/api/workspaces/:slug/webhooks/:pk/regenerate/`  | POST               | webhook        | `app/urls/webhook.py:21-25` | WebhookSecretRegenerateEndpoint | `routes::webhook::regenerate` (main.rs:1088-1091)                   | `regenerateSecretKey` @ webhook.service.ts:59                                    | ADMIN; 200 full serializer **with** secret*key (`show_secret_key: True`); secret format `plane_wh*<32hex>` (`generate_token`)                                                                                                                                                                |
| `/api/workspaces/:slug/webhook-logs/:webhook_id/` | GET                | webhook        | `app/urls/webhook.py:26-30` | WebhookLogsEndpoint             | `routes::webhook::list_logs` (main.rs:1092)                         | — (no FE caller found)                                                           | ADMIN; 200 array (WebhookLogSerializer); Django `webhook_id` == main.rs `:webhook_id` ✓                                                                                                                                                                                                      |

- [ ] Append 4 entries (`batch_task: "Batch N T3"`, 10 keys, `fe_pages: []` default) to new `domains.webhook.endpoints`; create the `domains.webhook` object (`rust_module: "routes/webhook.rs"`)
- [ ] `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 4/4:

```python
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/webhooks/',
  '/api/workspaces/:slug/webhooks/:pk/',
  '/api/workspaces/:slug/webhooks/:pk/regenerate/',
  '/api/workspaces/:slug/webhook-logs/:webhook_id/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('N3 present: 4/4')
"
```

- [ ] Commit `feat(rs-api): inventory webhooks (Batch N T3)`

### N4 — analytics advance + views (9 entries) → `domains.analytics` (existing)

| path                                                                   | methods (Django) | domains.\* key | django_source                | view                                 | rust_handler hint                                                  | fe_evidence hint                                                                                                                                          | known risk                                                                                                                                                                                                                                                                                                                   |
| ---------------------------------------------------------------------- | ---------------- | -------------- | ---------------------------- | ------------------------------------ | ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/workspaces/:slug/analytic-view/`                                 | GET, POST        | analytics      | `app/urls/analytic.py:30-34` | AnalyticViewViewset                  | `routes::analytic::list_views` / `create_view` (main.rs:1426-1429) | — (no FE caller found)                                                                                                                                    | GET list paginated (BasePaginator) + POST 201; WorkSpaceAdminPermission = workspace ADMIN+MEMBER; `perform_create` binds workspace_id                                                                                                                                                                                        |
| `/api/workspaces/:slug/default-analytics/`                             | GET              | analytics      | `app/urls/analytic.py:50-54` | DefaultAnalyticsEndpoint             | `routes::analytic::default_analytics` (main.rs:1424)               | — (no FE caller found)                                                                                                                                    | workspace AMG gate; 200 10-key payload (total_issues, total_issues_classified, open_issues, open_issues_classified, issue_completed_month_wise, most_issue_created_user, most_issue_closed_user, pending_issue_user, open_estimate_sum, total_estimate_sum); `issue_filters` query params; `timezone.now().year` (Django tz) |
| `/api/workspaces/:slug/project-stats/`                                 | GET              | analytics      | `app/urls/analytic.py:55-59` | ProjectStatsEndpoint                 | `routes::analytic::project_stats` (main.rs:1425)                   | `getProjectAnalyticsCount` @ apps/web/core/services/project/project.service.ts:70                                                                         | workspace AMG gate; `fields` + `project_ids` query params; no valid field → all 5 defaults; 200 array of `{id, ...requested_fields}`                                                                                                                                                                                         |
| `/api/workspaces/:slug/advance-analytics/`                             | GET              | analytics      | `app/urls/analytic.py:60-64` | AdvanceAnalyticsEndpoint             | `routes::analytic::advance_overview` (main.rs:1458)                | — (FE calls `AnalyticsService.getAdvanceAnalytics` @ analytics.service.ts:23 but URL built dynamically via `processUrl` — NO literal → `fe_evidence: []`) | workspace ADMIN+MEMBER gate; `?tab=overview\|work-items` (default overview; invalid → 400 `{"message": "Invalid tab"}`); `date_filter`/`project_ids` params; `get_analytics_filters` date conversions                                                                                                                        |
| `/api/workspaces/:slug/advance-analytics-stats/`                       | GET              | analytics      | `app/urls/analytic.py:65-69` | AdvanceAnalyticsStatsEndpoint        | `routes::analytic::advance_stats` (main.rs:1463-1466)              | — (same dynamic-URL note; caller `getAdvanceAnalyticsStats`)                                                                                              | ADMIN+MEMBER gate; `?type=work-items` else 400 `{"message": "Invalid type"}`; per-project state-group counts                                                                                                                                                                                                                 |
| `/api/workspaces/:slug/advance-analytics-charts/`                      | GET              | analytics      | `app/urls/analytic.py:70-74` | AdvanceAnalyticsChartEndpoint        | `routes::analytic::advance_charts` (main.rs:1471-1474)             | — (same; caller `getAdvanceAnalyticsCharts`)                                                                                                              | ADMIN+MEMBER gate; `?type=projects\|custom-work-items\|work-items` (default projects); `group_by`, `x_axis` (default PRIORITY); monthly zero-fill `{data, schema}`                                                                                                                                                           |
| `/api/workspaces/:slug/projects/:project_id/advance-analytics/`        | GET              | analytics      | `app/urls/analytic.py:75-79` | ProjectAdvanceAnalyticsEndpoint      | `routes::analytic::project_advance` (main.rs:1480-1483)            | — (same; FE peek view — `processUrl` `isPeekView` prefix, analytics.service.ts:99-109)                                                                    | **project ADMIN+MEMBER gate**; `?cycle_id\|module_id` id\_\_in scoping; unknown ids → zero counts, NO 404                                                                                                                                                                                                                    |
| `/api/workspaces/:slug/projects/:project_id/advance-analytics-stats/`  | GET              | analytics      | `app/urls/analytic.py:80-84` | ProjectAdvanceAnalyticsStatsEndpoint | `routes::analytic::project_advance_stats` (main.rs:1488-1491)      | — (same)                                                                                                                                                  | project ADMIN+MEMBER gate; `?type=work-items` else 400; per-assignee counts + `avatar_url`                                                                                                                                                                                                                                   |
| `/api/workspaces/:slug/projects/:project_id/advance-analytics-charts/` | GET              | analytics      | `app/urls/analytic.py:85-89` | ProjectAdvanceAnalyticsChartEndpoint | `routes::analytic::project_advance_charts` (main.rs:1496-1499)     | — (same)                                                                                                                                                  | **project ADMIN+MEMBER+GUEST gate (GUEST allowed — differs from the two above)**; `?type=custom-work-items\|work-items` (default projects → 400 "Invalid type"); cycle/module scoping; monthly or daily zero-fill                                                                                                            |

- [ ] Append 9 entries (`batch_task: "Batch N T4"`, 10 keys, `fe_pages: []` default) to existing `domains.analytics.endpoints` (rust_module `routes/analytic.rs` already set)
- [ ] `python3 -m json.tool apps/api-rs/crates/api/parity-inventory.json > /dev/null` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 9/9:

```python
python3 -c "
import json
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
paths = [ep['path'] for d in inv['domains'].values() for ep in d['endpoints']]
want = [
  '/api/workspaces/:slug/analytic-view/',
  '/api/workspaces/:slug/default-analytics/',
  '/api/workspaces/:slug/project-stats/',
  '/api/workspaces/:slug/advance-analytics/',
  '/api/workspaces/:slug/advance-analytics-stats/',
  '/api/workspaces/:slug/advance-analytics-charts/',
  '/api/workspaces/:slug/projects/:project_id/advance-analytics/',
  '/api/workspaces/:slug/projects/:project_id/advance-analytics-stats/',
  '/api/workspaces/:slug/projects/:project_id/advance-analytics-charts/',
]
missing = [p for p in want if p not in paths]
assert not missing, missing
print('N4 present: 9/9')
"
```

- [ ] Commit `feat(rs-api): inventory analytics advance + views (Batch N T4)`

### N5 — final audit + full suite

- [ ] Audit: all 31 new paths inventoried; `domains.asset` = 18, `domains.webhook` = 4, `domains.analytics` = 13; no dups; `cargo test -p api` 0 failed. No commit (verification only).

```python
python3 -c "
import json, re
inv = json.load(open('apps/api-rs/crates/api/parity-inventory.json'))
have = set(ep['path'] for d in inv['domains'].values() for ep in d['endpoints'])
assert len(have) == sum(len(d['endpoints']) for d in inv['domains'].values()), 'dup paths across domains'
for f, dom in [('apps/api/plane/app/urls/asset.py', 'asset'), ('apps/api/plane/app/urls/webhook.py', 'webhook')]:
    src = open(f).read()
    def canon(s):
        s = re.sub(r'<(?:str|uuid|int):(\w+)>', r':\1', s)
        s = s.replace(':asset_key', ':key').replace(':entity_id', ':pk')
        return '/api/' + s
    dpaths = {canon(m.group(1) or m.group(2)) for m in re.finditer(r'path\(\s*(?:[ru]?\"([^\"]+)\"|ru?\'([^\']+)\')', src)}
    missing = sorted(dpaths - have)
    assert not missing, (f, missing)
    n = len(inv['domains'][dom]['endpoints'])
    print(f.split('/')[-1], 'paths:', len(dpaths), '| inventoried:', len(dpaths & have), '|', dom, 'entries:', n)
    assert n == len(dpaths), (dom, n, len(dpaths))
dom = 'analytics'
src = open('apps/api/plane/app/urls/analytic.py').read()
def canon(s):
    return '/api/' + re.sub(r'<(?:str|uuid|int):(\w+)>', r':\1', s)
dpaths = {canon(m.group(1) or m.group(2)) for m in re.finditer(r'path\(\s*(?:[ru]?\"([^\"]+)\"|ru?\'([^\']+)\')', src)}
missing = sorted(dpaths - have)
assert not missing, ('analytic.py', missing)
n = len(inv['domains'][dom]['endpoints'])
print('analytic.py paths:', len(dpaths), '| inventoried:', len(dpaths & have), '| analytics entries:', n)
assert n == len(dpaths), (dom, n, len(dpaths))
print('N5 audit: OK')
"
```

- [ ] `cargo test -p api` — 0 failed. No commit (verification only).

## Standing rules (from Batch G/H/I/K)

- One task = one subagent → controller verifies (presence check + gates) → quality-review subagent → next task.
- Status recipe: (a) status codes, (b) response keys, (c) error strings, (d) permission gates. Any delta → `shape_mismatch` with `notes` citing `file:line` both sides; match → `implemented`, `notes` = `"verified <View> vs <handler>"`. Path absent from `main.rs` → `missing` (NOT `shape_mismatch` — the gate only skips `missing` in the main.rs presence check).
- `methods` = Django-served; never scope down (unhandled Django method → record fully + `shape_mismatch`; extra Rust-only methods → keep Django methods in `methods`, describe extras in `notes`).
- `fe_pages: []` default; fill ONLY with single-page verification via grep.
- Entry keys (10): `methods`, `path`, `django_source`, `rust_status`, `rust_handler`, `fe_evidence`, `fe_pages`, `batch_task`, `out_scope: false`, `notes`.
- Commit format: `feat(rs-api): inventory <scope> (Batch N Tn)`; single-file commits (`apps/api-rs/crates/api/parity-inventory.json` only).
- FE-only URL variants (e.g. `/api/assets/v2/workspaces/:slug/:entity_id/bulk/` in `updateBulkWorkspaceAssetsUploadStatus`, file.service.ts:126 — exists in NEITHER Django NOR Rust) → record in `notes` of the matching entry; never invent entries.
- FE evidence must stay tripwire-safe (`fe_tripwire_test.rs:39-67`): file must contain both the method name AND a URL template matching the inventory path. Dynamically-built URLs (`AnalyticsService.processUrl`) → `fe_evidence: []` + note.
