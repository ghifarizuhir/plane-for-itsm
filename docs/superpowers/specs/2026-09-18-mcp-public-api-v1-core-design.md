# MCP Public API v1 (Core Resources) — Design

Date: 2026-09-18
Status: Draft (pending user review)
Scope: Backend (`apps/api-rs`) only. New `/api/v1/...` surface consumed by `plane-sdk`
(used by `makeplane/plane-mcp-server`). Core resources only.
Predecessor: none. Related: `docs/superpowers/plans/2026-09-06-batch-e-workspace-platform-parity.md:16`
which explicitly scoped out `plane/api/*` (public v1).

## Goal

Make the official Plane MCP server usable against this fork. The MCP server talks to the
backend through `plane-sdk==0.2.23`, which hardcodes the base path `.../api/v1` and a cursor
pagination envelope. This fork only serves the internal app API under `/api/...`, so every MCP
tool call currently returns 404.

Deliver the `/api/v1` endpoints for the two resources selected for the first iteration —
**project** and **work item** (including comments, links, relations, attachments, activities
and work-item-types) — by reusing the existing Rust query/response layer wherever possible.

## Context

- `plane-sdk` builds URLs as `{base_url}/api/v1` + resource path + trailing slash
  (`.../plane/api/base_resource.py`, `.../plane/config.py`).
- Django upstream serves this surface at `/api/v1/` (`apps/api/plane/urls.py:22`,
  `plane/api/urls/*`). The Rust port covered only `plane/app/*` (`/api/...`).
- The SDK targets a **newer** Plane API than this fork's Django: some endpoints it calls do
  not exist in this version's Django v1 either (`dependencies`, `work-item-relations`,
  `work-item-types`, `work-items/count`, `projects-lite`, `*/features`, `*/total-worklogs`).
  The contract to satisfy is therefore the **SDK's expected shapes**, not Django's.
- Existing Rust cursor helpers already reproduce Django's 12-key envelope byte-for-byte:
  `DetailEnvelope` (`apps/api-rs/crates/api/src/routes/issue_common.rs:434`),
  `build_ungrouped_envelope` (`.../issue_query.rs:42`), `parse_per_page`/`parse_cursor`
  (`issue_common.rs:172,187`).

## Decisions (brainstormed & approved)

1. **Target contract** — the HTTP calls made by `plane-sdk==0.2.23` for the scoped tools,
   not a faithful port of Django `plane/api/*`.
2. **Scope** — core only: `project`, `workitem`, `workitem_comment`, `workitem_link`,
   `workitem_relation`, `workitem_attachment`, `workitem_activity`, `workitem_type`.
   Release/customer/initiative/milestone/collection/template/work-log tools stay out (their
   tables do not exist in this fork).
3. **Implementation home** — Rust `apps/api-rs`, mounted at `/api/v1`, reusing the existing
   handlers' SQL and JSON shapers. No Django runtime introduced.
4. **Auth** — the same middleware as `/api` (`X-Api-Key` and session cookies) applies; no new
   auth path.
5. **Phasing** — 5 phases (below); each phase is independently testable and verifiable
   through the MCP server.

## Architecture

```
main.rs Router
  ├── /api/...            existing app API (unchanged)
  ├── /api/v1/...         NEW: v1 compatibility layer
  └── /:bucket, /health   existing (unchanged)
```

- New module tree: `apps/api-rs/crates/api/src/routes/v1/` with `mod.rs` plus one file per
  resource (`project.rs`, `work_item.rs`, `workitem_comment.rs`, `workitem_link.rs`,
  `workitem_relation.rs`, `workitem_attachment.rs`, `workitem_activity.rs`,
  `workitem_type.rs`).
- Shared helpers in `routes/v1/common.rs`:
  - `v1_envelope(rows, count, total, per_page, page, is_prev)` → the 12-key envelope. It
    delegates to the existing `DetailEnvelope`/cursor helpers rather than re-implementing.
  - `iso(dt)`, `page_params(query)` wrappers.
- Response shaping: each resource gets pure `v1_<resource>_json(...)` functions returning
  `serde_json::Value`. These are unit-tested (repo convention: pure functions, see
  `misc_test.rs`) because they encode the SDK contract.
- Reuse map (from research):

  | v1 need               | Reuse source                                                                                                               |
  | --------------------- | -------------------------------------------------------------------------------------------------------------------------- |
  | Envelope + cursor     | `issue_common.rs:172-447`, `issue_query.rs:42-66`                                                                          |
  | Project object        | `project.rs:346-437` (`fetch_project_full`, `project_full_json`) + count annotations                                       |
  | Project-lite          | derive from project row (`id, identifier, name, cover_image, icon_prop, emoji, description, cover_image_url, archived_at`) |
  | Work item list/detail | `LIST_SELECT_SQL` (`issue_query.rs:70`), `DETAIL_SELECT_SQL` (`:1639`), `IssueListRow`/`IssueDetailRow`                    |
  | Comments              | `CommentRow`/`comment_json` (`work_item.rs:179-398`) — superset, filter keys                                               |
  | Links                 | `LinkRow`/`link_json` (`work_item.rs:740-795`) — drop `created_by_detail`                                                  |
  | Relations             | bucket logic `work_item.rs:1207-1267`; pair shape `{project_id, issue_id}`                                                 |
  | Activities            | widen query `work_item.rs:1522-1554` to read all serialized columns                                                        |
  | Attachments           | `asset.rs` presign (`1184-1192`), `full_asset_json` (`713-740`)                                                            |

## Endpoint contract (scoped)

All paths relative to `/api/v1`. `{slug}` = workspace slug, `{pid}` = project uuid.

### Project

| Method | Path                                               | Response                       |
| ------ | -------------------------------------------------- | ------------------------------ |
| GET    | `workspaces/{slug}/projects-lite/`                 | `PaginatedProjectLiteResponse` |
| GET    | `workspaces/{slug}/projects/{pid}/`                | `Project`                      |
| POST   | `workspaces/{slug}/projects/`                      | `Project` (201)                |
| PATCH  | `workspaces/{slug}/projects/{pid}/`                | `Project`                      |
| DELETE | `workspaces/{slug}/projects/{pid}/`                | 204                            |
| POST   | `workspaces/{slug}/projects/{pid}/archive/`        | 204                            |
| DELETE | `workspaces/{slug}/projects/{pid}/archive/`        | 204                            |
| GET    | `workspaces/{slug}/projects/{pid}/total-worklogs/` | `list[ProjectWorklogSummary]`  |
| GET    | `workspaces/{slug}/projects/{pid}/features/`       | `ProjectFeature`               |
| PATCH  | `workspaces/{slug}/projects/{pid}/features/`       | `ProjectFeature`               |

`projects-lite` query params: `cursor`, `per_page` (the SDK always sends its own
default of 100; the server falls back to 1000 and caps at 1000 per `parse_per_page`),
`order_by` (accepted, currently ignored — fixed `name ASC` order), `include_archived`
(default excludes archived; case-insensitive `true`/`1`).

### Work item

| Method | Path                                                          | Response                             |
| ------ | ------------------------------------------------------------- | ------------------------------------ |
| GET    | `workspaces/{slug}/projects/{pid}/work-items/`                | `PaginatedWorkItemResponse`          |
| GET    | `workspaces/{slug}/work-items/`                               | `PaginatedWorkItemResponse`          |
| GET    | `workspaces/{slug}/projects/{pid}/archived-work-items/`       | `PaginatedWorkItemResponse`          |
| GET    | `workspaces/{slug}/projects/{pid}/work-items/{id}/`           | `WorkItemDetail`                     |
| GET    | `workspaces/{slug}/work-items/{IDENT}-{seq}/`                 | `WorkItemDetail`                     |
| GET    | `workspaces/{slug}/work-items/search/?search=`                | `WorkItemSearch` (`{issues: [...]}`) |
| GET    | `workspaces/{slug}/work-items/count/`                         | `WorkItemGroupedCountResponse`       |
| POST   | `workspaces/{slug}/projects/{pid}/work-items/`                | `WorkItem` (201)                     |
| PATCH  | `workspaces/{slug}/projects/{pid}/work-items/{id}/`           | `WorkItem`                           |
| DELETE | `workspaces/{slug}/projects/{pid}/work-items/{id}/`           | 204                                  |
| POST   | `workspaces/{slug}/projects/{pid}/work-items/{id}/archive/`   | 204                                  |
| DELETE | `workspaces/{slug}/projects/{pid}/work-items/{id}/unarchive/` | 204                                  |

### Sub-resources (all under `.../work-items/{workitem_id}/`)

| Resource         | Endpoints                                                                   | Response                     |
| ---------------- | --------------------------------------------------------------------------- | ---------------------------- |
| comments         | GET/POST `comments/`; GET/PATCH/DELETE `comments/{cid}/`                    | paginated / object / 204     |
| links            | GET/POST `links/`; GET/PATCH/DELETE `links/{lid}/`                          | paginated / object / 204     |
| activities       | GET `activities/`; GET `activities/{aid}/`                                  | paginated / object           |
| attachments      | GET/POST `attachments/`; GET/PATCH/DELETE `attachments/{aid}/`              | array / presign / 302 / 204  |
| dependencies     | GET/POST `dependencies/`; DELETE `dependencies/{related_id}/`               | grouped object / array / 204 |
| custom relations | GET/POST `work-item-relations/`; DELETE `work-item-relations/{related_id}/` | grouped object / array / 204 |

### Work-item types

| Method           | Path                                                                    |
| ---------------- | ----------------------------------------------------------------------- |
| GET/POST         | `workspaces/{slug}/projects/{pid}/work-item-types/`                     |
| GET/PATCH/DELETE | `workspaces/{slug}/projects/{pid}/work-item-types/{tid}/`               |
| GET/POST         | `workspaces/{slug}/work-item-types/`                                    |
| GET/PATCH/DELETE | `workspaces/{slug}/work-item-types/{tid}/`                              |
| POST             | `workspaces/{slug}/projects/{pid}/import-work-item-types/`              |
| GET/PATCH        | `workspaces/{slug}/features/` (workspace features; needed by `resolve`) |

Relation **definitions** (`workspaces/{slug}/work-item-relation-definitions/`) require a new
table that does not exist in this fork. They are **deferred** (see Out of scope); relation
create/delete via `relation_type` (dependency) is supported.

## Pagination envelope

Every paginated response returns the 12-key envelope, matching `plane/api/paginator.py:728-743`
and required by `plane.models.pagination.PaginatedResponse`:

```json
{
  "grouped_by": null,
  "sub_grouped_by": null,
  "total_count": 0,
  "next_cursor": "100:1:0",
  "prev_cursor": "100:-1:1",
  "next_page_results": false,
  "prev_page_results": false,
  "count": 0,
  "total_pages": 0,
  "total_results": 0,
  "extra_stats": null,
  "results": []
}
```

Required (SDK will fail validation if absent): `total_count`, `next_cursor`, `prev_cursor`,
`next_page_results`, `prev_page_results`, `count`, `total_pages`, `total_results`, `results`.

## Error handling

- Reuse the existing `AppError`/`missing`/`deny` helpers so status codes and bodies match the
  app API (`404 Not Found`, `403 Forbidden`, `400` validation).
- Unknown `per_page`/`cursor` follow `parse_per_page`/`parse_cursor` byte-exact DRF messages.
- 204 responses carry no body; DELETE is soft-delete (`deleted_at`) where the app API is.
- Unsupported query params are ignored (the SDK only sends documented ones).

## Auth & permissions

- Routes inherit the global auth middleware (`middleware/auth.rs`), accepting `X-Api-Key` or
  session. Workspace membership checks reuse the app API's `ws_role`/permission helpers.
- v1 project reads (`projects-lite`, retrieve, features, work-logs) mirror `detail`
  (`project.rs:693-727`) exactly: non-member of a SECRET (`network=0`) project → 403,
  non-member of a public project → 409, missing/archived → 404.
- v1 project writes (`create`, `update`, `delete`, `archive`, `features` update) mirror
  `patch` (`project.rs:738-761`): ws-ADMIN (`>=20`) or project-ADMIN required, otherwise
  403; archived → 400.
- API key callers resolve to their owning user; workspace scoping is enforced by the same SQL
  filters as the app API.

## Testing strategy

- **Unit (Rust, `#[test]`)** — pure shapers and helpers: envelope keys, cursor round-trips,
  `v1_project_json`/`v1_project_lite_json` key sets, `v1_workitem_json` required fields,
  comment/link/activity key filtering. Follows `misc_test.rs` / `webhook_test.rs` style.
- **Contract smoke (manual/CI script)** — start the API, insert a temporary `api_tokens` row,
  and drive the real `plane-sdk` client against `http://localhost:8000/api/v1` for each scoped
  method. This proves the SDK's pydantic models accept our payloads. Script lives under
  `apps/api-rs/scripts/` (e.g. `v1-mcp-smoke.py`) and is not part of `cargo test`.
- **End-to-end MCP check** — run `plane-mcp-server` tools against the rebuilt container with a
  real token (as done for the token-shape fix).

## Phases

1. **Infrastructure** — mount `/api/v1` router; `routes/v1/common.rs` envelope/cursor helpers;
   auth wiring; unit tests.
2. **Project** — `projects-lite`, retrieve/create/update/delete, archive/unarchive, features,
   total-worklogs. Completes the MCP `project` tool.
3. **Work item core** — list (project/workspace/archived), retrieve, by-identifier, search,
   count, create/update/delete, archive/unarchive. Completes the MCP `workitem` core actions.
4. **Sub-resources** — comments, links, activities, attachments, dependencies/custom
   relations, work-item-types (+ import, workspace features).
5. **Verification & docs** — SDK smoke script, live MCP run of every scoped action, document
   the v1 layer and its reuse points.

## Out of scope / deferred

- Relation **definitions** CRUD (needs new `work_item_relation_definitions` schema).
- `expand`/`fields` full Django semantics: accepted and ignored unless trivially supported.
- Release, customer, initiative, milestone, collection, project-template, work-log resources
  (no tables in this fork).
- `plane/space/*` (anchor/public board) and remaining `plane/api/*` resources.
- Adding `/api/v1` to the parity inventory gate (follow-up once the surface stabilises).

## Risks

- **SDK drift** — upstream MCP may call endpoints/fields beyond this spec; mitigated by the
  contract smoke test against the pinned SDK version.
- **Shape reuse** — Rust app rows are supersets in some cases (comments) and subsets in others
  (activities); each shaper must be asserted against the SDK model's required fields.
- **PQL/filters** — the work-item list may receive `pql`; phase 3 must confirm the app API's
  existing PQL handling is reachable from v1 and mirror its errors when not.
- **Schema gaps** — work-item-types CRUD depends only on the existing `issue_types` table;
  confirm column coverage before phase 4.
