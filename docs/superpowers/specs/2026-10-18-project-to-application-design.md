# Project → Application & Workspace-Level Configuration — Master Design

Date: 2026-10-18
Status: Approved in brainstorming (6 clarifying decisions, 3 design sections, 2026-10-18); awaiting written-spec review.
Scope: Program-level master design. Projects are **renamed to Applications** (identity + membership containers) while **all configuration moves to workspace level**: states/workflow, labels, estimates, work item types, views, feature settings, intake, cycles & modules. Issues stay owned by exactly one application — no flat pool, no identifier renumbering. RBAC keeps both layers. Execution follows the additive `0020_workspace_services.sql` precedent, in waves. Each sub-project (S1–S9) gets its own spec → plan → implementation; this document is not itself an implementation plan.
Related: `2026-10-17-workspace-level-services-design.md` (same additive pattern; services already workspace-level), `2026-10-18-workspace-tickets-design.md`, `2026-10-18-workspace-sprints-tracks-design.md`, `2026-10-18-workspace-kb-design.md`, `2026-10-18-workspace-intake-design.md` (workspace aggregate pages that become the primary IA), `docs/features/cmdb.md`.

## Background

**Scale of the entanglement** (live backend is Rust, `apps/api-rs`; Django `apps/api` is test-only and follows the established Rust-only schema pattern, as with services/KB/intake):

- ~74 tables carry `project_id` (~34 NOT NULL) — `apps/api-rs/migrations/0001_initial.sql` passim. Config subset: `states` (recreated in `0016_remove_workflows.sql:14-17`), `labels` (already dual-level: partial uniques at `0001_initial.sql:7186,7200`), `estimates`/`estimate_points` (`:763,:783`), `project_issue_types` + workspace-level `issue_types` (`:1376,:1872`), `issue_views` (nullable; workspace views already exist via `project_id IS NULL` — `routes/view.rs:324-326`), `intakes`/`intake_sources`/`intake_triage_suggestions` (`0001:1061`, `0015:5`, `0010:4`), `cycles`/`cycle_issues` (`:223,:263`), `modules` + children (`:1549-1641`), and the feature-settings columns on `projects` (`:2014-2052`: `cycle_view`, `module_view`, `issue_views_view`, `page_view`, `intake_view`, `is_issue_type_enabled`, `is_time_tracking_enabled`, `timezone`, `default_assignee_id`, `archive_in`, `close_in`, `network`, `guest_view_all_features`).
- Content subset stays project-scoped: `issues` + 17 child tables, `draft_issues*` (nullable already), war rooms (`0011_war_rooms.sql`), service links (`0020`), integrations (`github_*`, `slack_project_syncs`, `importers`, `project_webhooks`).
- Issue identity: display ID `PROJ-123` is derived as `projects.identifier || '-' || issues.sequence_id` (e.g. `apps/api-rs/crates/ai/src/tools/mod.rs:268`); sequences are allocated per project under row lock (`routes/issue_write.rs:186-254`); partial unique `(project_id, sequence_id)` (`0005_fix_issue_sequences.sql:59-61`).
- API: 201 of 369 route registrations are project-scoped (`apps/api-rs/crates/api/src/main.rs`). Workspace-level read endpoints already exist for issues, cycles, modules, labels, states, pages, intake-issues, and services.
- RBAC: dual-layer (workspace + project membership, roles `20/15/5` — `routes/member.rs:123`); guests scoped per project via `projects.guest_view_all_features` (`routes/issue_common.rs:122-138`).
- Web: 540 files touch `projectId`; 95 files destructure `useParams().projectId`; 128 files use `useProject()`; project nav + settings trees in `apps/web/app/routes/core.ts:167-407`; favorites/recent are workspace tables with a nullable project FK (`favorite.py:14-31`, `recent_visit.py:13-30`); the in-app user guide documents "Projects" (`apps/web/app/(all)/[workspaceSlug]/(docs)/`).
- Live data (this deployment): workspace `itsm` — 2 active projects (PREPAID, 61 issues; CARDLINKAPP, 0 issues), 6 members; `terraline-demo` — 1 project. Soft-deleted projects retain membership rows; migration must skip all `deleted_at` rows.

**Why now:** projects-as-config-containers is Plane legacy. In this ITSM fork the workspace is the tenant and the workspace-level aggregate pages (tickets/sprints/tracks/KB/intake/services) already exist. What remains per-project is exactly the friction: every new project re-creates states/labels/estimates/settings, and workspace pages duplicate config. De-projecting the config while keeping projects as lighter "Application" groupings removes that friction without invalidating identifiers, memberships, or deep links.

## Locked decisions (brainstorming 2026-10-18)

1. **Audience: internal-first, multi-tenant later.** The migration logic is designed generally (per-workspace, multi-project safe, idempotent) but execution is staged and focused on this instance. No customer-facing backward-compat guarantees yet.
2. **Application = renamed Project.** Keeps identity (name, icon, description, issue prefix), membership, and its detail pages. It no longer owns configuration.
3. **All configuration moves to workspace level** (one set per workspace): states/workflow, labels, estimates, work item types, views, feature settings, intake, and cycles & modules.
4. **RBAC keeps both layers** (workspace roles + per-application membership). Invite flows stay as they are (workspace email/link invite plus bulk-add of existing members to an application). Guest scoping stays per-application; `guest_view_all_features` becomes a workspace setting.
5. **Application pages minimal:** Overview, Tickets (filtered), Members, Settings (identity only). Sprints/Tracks/Views/Intake/KB/Services live in workspace navigation only, filterable by application through links/query params.
6. **Issues remain owned by exactly one application** (`issues.project_id` unchanged). No flat pool, no renumbering. Workspace pages are cross-application aggregates (already built).

Rejected during brainstorming: flat content pool with optional application link; folding Application into Service; workspace-only RBAC with requester guests; keeping per-application membership configuration; full "delete the projects table" reading.

## Terminology

- **Application** — user-facing name for the renamed project concept. Internally the `projects` row, `project` code identifiers, `project_id` columns, and `/projects/` API paths stay (until an optional S9 rename).
- **Config** — workspace-wide definitional data (states, labels, estimates, work item types, views, feature settings, intake, cycles, modules).
- **Content** — user-produced data (issues + children, drafts, pages content, war rooms, services, intake submissions).
- **Dual-level table** — table with `workspace_id` and a **nullable** `project_id`, following the services/KB precedent.

## Target model

| Layer                           | Now                                                     | Target                                                                                                                |
| ------------------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Workspace                       | config fragmented per project                           | one config set: states/workflow, labels, estimates, work item types, views, feature settings, intake, cycles, modules |
| Application (= renamed Project) | container + own config                                  | identity container: name, icon, description, issue prefix + members + issues + war rooms + service links              |
| Application pages               | issues, cycles, modules, views, pages, intake, settings | Overview, Tickets (filtered), Members, Settings (identity only)                                                       |
| Issues                          | owned by a project                                      | owned by exactly one application (`project_id` kept)                                                                  |
| Issue identifier                | `PROJ-123` from project prefix + per-project sequence   | unchanged — no renumbering; git/email references stay valid                                                           |
| RBAC                            | workspace + application membership, roles 20/15/5       | unchanged mechanics; copy renamed; `guest_view_all_features` becomes a workspace setting                              |
| War rooms                       | per-project                                             | per-application (entry via the `war_room=on` filter on Tickets)                                                       |
| Services                        | workspace-level catalog, project links                  | unchanged (already done in `0020`)                                                                                    |
| Integrations/importers          | per-project                                             | deferred — decided in S9                                                                                              |
| Pages (KB)                      | workspace-level `pages` + `project_pages` join          | workspace KB (already built); app nav drops Pages; join table retired in S9 if unused                                 |

**Creation flow:** unchanged shape (name + prefix + icon modal) but now trivial — a new application inherits the workspace config immediately (no states/labels/estimates setup). Application settings are reduced to identity, members, and archive/delete.

**RBAC / invites:** flows unchanged — workspace invite by email/link, bulk-add existing members to an application, guest = application member with role 5 and per-application guest scoping. UI copy renamed.

## Impact inventory

- **DB (~12 config tables + `projects` settings columns):** become dual-level (`workspace_id` + nullable `project_id`), backfilled and deduped per workspace; drops happen in S9. Content tables, war rooms, and services links are untouched.
- **Rust API:** canonical workspace endpoints added/completed per domain. Project-scoped endpoints remain live as compat. Seeder (`crates/api/src/seed.rs`) switches to workspace-level config. AI crate SQL reading per-project feature flags (`crates/ai/src/tools/mod.rs:311-314`, sprints/tracks SQL) switches to workspace settings.
- **Web:** config stores switch ownership from per-project to workspace (several already expose `fetchWorkspace*`); application nav trimmed; workspace settings pages added; deep links, favorites/recent link builders, and redirects updated; PowerK commands updated (create cycle/module at workspace level; create ticket picks an application).
- **i18n:** "Application" copy across ~20 locales via the `translate` skill; in-app user guide updated in S8.
- **Django:** no model changes (Rust-only schema precedent).
- **Testing:** Rust integration tests per domain with a deliberate 2-application collision fixture (same state names, same label names, same cycle names); idempotent-backfill test (run twice); web vitest for nav/links; prod smoke per wave.

## Migration rules per config

Rules apply per workspace, over live (`deleted_at IS NULL`) rows only.

| Config                                 | Merge rule                                                                                                                                                                                                                                                                                                                                                                                            |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **States**                             | Dedupe by `(group, lower(btrim(name)))`; canonical = oldest live row (colors follow canonical). Remap `issues.state_id`, workspace default state (canonical Todo-group state), intake state (canonical backlog-group state). Renumber `sequence` by group order. Rewrite state references inside saved JSON filters (`issue_views.filters` and any analytics/AI schedule specs that embed state ids). |
| **Labels**                             | Dedupe by `lower(btrim(name))`; canonical = oldest; remap `issue_labels` and label references inside saved filters. New unique `(workspace_id, lower(btrim(name)))`.                                                                                                                                                                                                                                  |
| **Estimates**                          | Merge estimates by name; merge points by value (dedupe); canonical estimate = one of the application with the most issues; remap `issues` estimate points. Workspace keeps the canonical estimate active; others retained inactive for manual re-activation.                                                                                                                                          |
| **Work item types**                    | Union of per-project enablement → active workspace-wide; `project_issue_types` becomes vestigial (dropped in S9).                                                                                                                                                                                                                                                                                     |
| **Views**                              | `project_id → NULL` (all views become workspace views); filters preserved verbatim.                                                                                                                                                                                                                                                                                                                   |
| **Cycles/Modules**                     | `project_id → NULL`; name collisions get suffix `" (APP)"` (renamable by admins); join tables (`cycle_issues`, `module_issues`) unchanged; ids unchanged so no reference remap.                                                                                                                                                                                                                       |
| **Intake**                             | One intake front door per workspace; `intake_sources` merged workspace-level with tokens preserved; `intake_issues` keep their target application; triage flow and the `requires_service` gate unchanged.                                                                                                                                                                                             |
| **Feature settings**                   | Union for booleans (enabled-if-any-ever-enabled; fresh workspaces default all-on); `timezone`, `archive_in`, `close_in`, `default_assignee_id`, `is_time_tracking_enabled`, `guest_view_all_features` → workspace settings. `network` (public/private) **stays per-application** — it is access control, not config.                                                                                  |
| **War rooms, integrations, importers** | Not merged — stay per-application; integrations disposition deferred to S9.                                                                                                                                                                                                                                                                                                                           |

**Safety:** each domain ships a dry-run report first (counts of merged/deduped rows, name collisions, remap counts, referential-integrity checks) and the backfill is idempotent. Single-application workspaces get a pure `project_id → NULL` no-op (still runs for uniformity).

## Operating principles

- **Additive-only until S9** → rollback is redeploying the previous web/API build; new columns are unread by old code.
- **No dual-write.** Web and API are deployed together in this stack, so per-domain cutover is atomic enough; backfill runs before the web switch.
- **Idempotent, per-workspace backfills** with dry-run reports.
- **Single-application workspaces are no-ops** (e.g. `terraline-demo`).
- **Verification per domain:** Rust tests (cargo) + SQL count checks before/after + web build + production smoke.
- **Mandatory rehearsal before touching live data:** `pg_dump` → restore into a scratch database → run backfill → verify counts → only then run on the live database.

## Execution waves

- **W0 — Rename UI (S1):** copy, icons, i18n "Application" everywhere user-facing. Zero schema. Quick win. Copy scope = EN only; non-EN locales ship as S1b (user decision 2026-10-18: all active profiles use en).
- **W1 — Config to workspace, per domain (S2→S7), smallest risk first:**
  1. S2 — workspace settings + feature flags (foundation: storage + endpoint + backfill)
  2. S3 — taxonomy: labels, estimates, work item types
  3. S4 — views
  4. S5 — states/workflow (most entangled: issue create, filters, intake, AI)
  5. S6 — cycles + modules
  6. S7 — intake
- **W2 — IA final (S8):** application nav minimal (Overview/Tickets/Members/Settings), complete workspace settings pages, web URL rename `/projects/` → `/applications/` with redirects, PowerK and create-flow updates, in-app guide `/docs` rewrite, removal of project config pages/stores.
- **W3 — Cleanup (S9):** drop per-project config columns/tables/joins, decide integrations/importers/project webhooks disposition, optional internal code/API/DB rename.

## Sub-project decomposition

Each sub-project gets its own spec → plan → implementation. Execution order equals S order.

| #   | Sub-project                                  | Wave | Scope highlights                                                                                                                                       | Exit criteria                                                                               |
| --- | -------------------------------------------- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| S1  | Rename UI                                    | W0   | Sidebar/nav/modals/settings/empty states copy → "Application"; icons; i18n EN + copy-guard (19 locale lain = S1b, plan terpisah); no schema/API change | Typecheck, lint, build, prod smoke; no "project" user-facing copy left in the main surfaces |
| S2  | Workspace settings + feature flags           | W1①  | Workspace settings storage + GET/PATCH endpoint; backfill union of per-project flags; nav gating reads workspace settings                              | Tests (union backfill, defaults), web reads workspace, prod smoke                           |
| S3  | Taxonomy: labels, estimates, work item types | W1②③ | Workspace uniques + dedupe/remap backfill; workspace pickers in web; type enablement workspace-wide                                                    | Collision-fixture tests; dry-run report; issue label/estimate/create flows smoke            |
| S4  | Views                                        | W1④  | `project_id → NULL` backfill; workspace views already readable                                                                                         | Tests; views list/detail smoke                                                              |
| S5  | States/workflow                              | W1⑤  | Dedupe/remap backfill incl. saved-filter rewrites; default + intake state; seeder and AI updates                                                       | Collision-fixture tests; create/filter/intake smoke; AI tool smoke                          |
| S6  | Cycles + modules                             | W1⑥  | Unnest backfill with name-collision suffixing; workspace pages are canonical (read endpoints exist)                                                    | Tests; sprint/track pages + issue cycle/module pickers smoke                                |
| S7  | Intake                                       | W1⑦  | Single workspace front door; sources merge; form application picker; triage assigns application                                                        | Tests; intake submit/accept smoke incl. `requires_service`                                  |
| S8  | IA final + redirects + docs                  | W2   | App nav minimal; workspace settings complete; `/applications/` URLs + redirects; PowerK; favorites/recent link builders; guide rewrite                 | Vitest, build, full smoke of both URL families (old → redirect)                             |
| S9  | Cleanup                                      | W3   | Drop per-project config columns/tables/joins; integrations/importers decision; optional internal rename                                                | Tests green after drops; smoke; no dead code                                                |

## Open decisions delegated to sub-specs (with defaults)

- **S2:** storage shape for workspace settings — default: single-row `workspace_settings` table keyed by workspace. Boolean union = enabled-if-any.
- **S3:** estimates multiplicity — default: multiple workspace estimates allowed, exactly one active; canonical pick = application with most issues (admin can re-point later). Work item types: union enablement.
- **S4:** views unnest — default: no renames; filters untouched (state/label ids inside filters are rewritten by S3/S5 respectively).
- **S5:** states canonical default — default: canonical Todo-group state becomes the workspace default; intake state = backlog-group canonical.
- **S6:** collision suffix format — default `"Name (APP)"`, applied only to colliding names.
- **S7:** intake form application picker — default: optional; empty means the item waits in a workspace triage bucket until assigned.
- **S8:** URL policy — default: web URLs canonicalize to `/:workspaceSlug/applications/...`, old `/projects/...` redirect client-side; API paths unchanged.
- **S9:** integrations (GitHub/Slack sync, importers, `project_webhooks`) — decide move-to-workspace vs drop; internal code/API rename optional at this point.

## Risks & mitigations

- **Losing custom workflow/states semantics in the merge** → conservative dedupe (only exact `(group, name)` collisions merge; every custom state survives as a workspace state), dry-run report, rehearsal on a restored dump.
- **Deep-link breakage** (favorites, recents, notifications, PowerK, guide) → identifier stability removes the worst class; S8 does a full link inventory + redirects.
- **Hidden SQL surface** (analytics, AI tools, worker, seed) reads config per project → per-domain grep + tests are part of each sub-project's definition of done.
- **i18n volume** (~20 locales) → `translate` skill workflow with native-review flags.
- **Mistake on live data** → rehearsal and count checks are mandatory per domain; backfills are idempotent so re-runs are safe.
- **Scope creep toward a full "delete projects" rewrite** → locked decision 6 (issues keep `project_id`) and the deferred S9 cleanup bound the work.

## Verification strategy

- **Per sub-project:** Rust integration tests (scratch-workspace suites run serially per `AGENTS.md`), web vitest + typecheck + lint, web build, API/worker/beat rebuild, prod smoke via the tunnel.
- **Migration-specific:** collision fixture (two applications with identical state/label/cycle names), idempotence (run backfill twice → identical state), dry-run report reviewed before live execution, dump-restore rehearsal for the `itsm` workspace.
- **Program-level:** after S8, every legacy URL class (project pages, favorites links, PowerK results, notifications) resolves correctly; after S9, schema contains no per-project config columns.
