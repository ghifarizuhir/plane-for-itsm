# Docs Current Corpus — Design

- Date: 2026-10-27
- Status: Approved design
- Scope: `docs/` corpus, docs tooling, and docs drift guardrails
- Audience: engineers, contributors, and AI coding agents

## 1. Problem

`docs/` describes the repository as it existed around the Rust cutover in
September 2026. Since then the product moved to workspace-level surfaces and
gained services, tickets, intake, knowledge, war rooms, Galileo, scheduler,
metrics, and application-scoped navigation. Several current documents now make
claims that are directly false:

- `docs/PRODUCT.md` calls ITSM a future direction; it is implemented.
- `docs/design/02-data-model.md` says ITSM models do not exist; migrations
  `0003_services`, `0011_war_rooms`, `0016_remove_workflows`,
  `0020_workspace_services`, `0024_service_metrics`, and `0025_service_kind`
  exist.
- `docs/features/*` mostly documents project-scoped routes that have been
  replaced by workspace-level routes.
- `docs/design/06-ops-runbook.md` describes `docker-compose-local.yml` as the
  legacy Django stack; it is the main local api-rs stack.
- `docs/design/README.md` says the monorepo has six apps; it has seven.
- `docs/features/README.md` marks ITSM as future and lists shared docs that do
  not exist.

The corpus is also unprotected: there is no link checker, no route/model/API
inventory, and no way to tell whether a document still matches the code.

## 2. Goals

1. Make `docs/` a small, current engineering handbook with no known false
   claims.
2. Make every current document traceable to the source files it describes.
3. Generate route, API, and schema inventories from code instead of maintaining
   them by hand.
4. Detect drift automatically in CI with a lightweight check.
5. Keep product-facing guidance in the in-app docs and keep `docs/` focused on
   implementation, contracts, and contributor context.

## 3. Non-goals

- Building a documentation website or changing the in-app docs content.
- Rewriting `docs/superpowers/specs`, `plans`, `decisions`, or `smoke` history.
- Documenting every route, model column, or API field in prose; generated
  inventories cover exhaustive lists.
- Changing application behavior, database schema, or API contracts.
- Translating the corpus to English. Existing `docs/` documents stay in
  Indonesian; README and CONTRIBUTING remain English.
- Preserving stale documents as current. Git history is the archive for deleted
  files.

## 4. Decisions

| Topic                       | Decision                                                                                                                                                                      |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Corpus boundary             | `docs/` contains current implementation docs. `docs/superpowers/` is historical archive only and is excluded from the main reading path.                                      |
| Source of truth             | Code is authoritative. Documents link to source paths and generated inventories instead of copying exhaustive lists.                                                          |
| Product vs engineering docs | In-app docs (`/:workspaceSlug/docs`) explain user flows. `docs/features/*` explains implementation: routes, components, stores, API touchpoints, permissions, and edge cases. |
| Metadata                    | Every current document has machine-readable frontmatter. `source_hash` is authoritative for staleness; `last_verified` records the verification date.                         |
| Why not `verified_commit`   | A document cannot contain its own commit hash before the commit exists. Source content hashes work for same-commit doc and code updates.                                      |
| Generated inventories       | Route, API, and schema inventories are generated, committed, and checked in CI. They are never edited by hand.                                                                |
| Line references             | Hand-written docs use file paths and symbol names, not line numbers, because line numbers drift on every edit.                                                                |
| Stale documents             | Delete superseded docs. Do not keep stale files with a warning banner in the current corpus.                                                                                  |
| Language                    | `docs/` stays Indonesian. Paths, symbols, API names, and frontmatter keys stay English.                                                                                       |
| Guardrail strength          | CI runs `pnpm docs:check` on docs and source changes. It does not require every code PR to edit a document.                                                                   |
| Product terminology         | Use product terms with route mappings: Tickets ↔ work items, Sprints ↔ cycles, Tracks ↔ modules, Knowledge ↔ pages, Applications ↔ projects.                                  |

## 5. Target Structure

```text
docs/
├── README.md
├── PRODUCT.md
├── CHEAT-SHEET.md
├── linting.md
├── design/
│   ├── README.md
│   ├── 01-architecture.md
│   ├── 02-data-model.md
│   ├── 03-api-contract.md
│   ├── 04-design-system.md
│   ├── 05-testing-strategy.md
│   ├── 06-ops-runbook.md
│   └── 07-routing-and-navigation.md
├── features/
│   ├── README.md
│   ├── work-items.md
│   ├── services.md
│   ├── intake.md
│   ├── knowledge.md
│   ├── sprints.md
│   ├── tracks.md
│   ├── war-rooms.md
│   ├── ai.md
│   ├── analytics.md
│   ├── notifications.md
│   ├── applications.md
│   ├── settings.md
│   ├── archives.md
│   ├── drafts.md
│   ├── stickies.md
│   ├── views.md
│   ├── browse.md
│   └── _shared/
│       ├── README.md
│       ├── list.md
│       ├── detail-page.md
│       ├── comments.md
│       └── search.md
├── ui/
│   ├── README.md
│   ├── design-tokens.md
│   ├── shell.md
│   └── editor.md
├── _generated/
│   ├── routes.md
│   ├── api-routes.md
│   └── schema.md
└── superpowers/
    └── README.md
```

## 6. Metadata Contract

Every file in the current corpus (`docs/README.md`, `PRODUCT.md`,
`CHEAT-SHEET.md`, `linting.md`, `design/`, `features/`, and `ui/`) starts with:

```yaml
---
status: current
last_verified: 2026-10-27
source_hash: sha256:853923d339a4bd50230878de625ab5b5b22f1deff5bef783b8c927646d6ee47e
source_paths:
  - apps/web/app/routes/core.ts
---
```

Rules:

- `status`: `current`, `proposal`, `generated`, or `historical`.
- `last_verified`: `YYYY-MM-DD`.
- `source_hash`: SHA-256 computed by `pnpm docs:verify`. The script sorts the
  expanded source paths, then hashes `path + "\0" + content + "\0"` for each
  path in order.
- `source_paths`: repo-relative files or directories that the document
  describes. Directories expand recursively in sorted order.
- `generated` documents are under `docs/_generated/` only and use an HTML
  comment banner instead of frontmatter.
- A current document with a missing or invalid field fails `docs:check`.
- A document is stale when the recomputed `source_hash` differs from the value
  in frontmatter.

`docs/superpowers/README.md` explains that the directory is historical, is not
part of the current corpus, and must not be used as a description of current
behavior.

## 7. Generated Inventories

### 7.1 `routes.md`

Source: `apps/web/app/routes/core.ts`.

Extract every `route("<path>", "<file>")` call, including multiline calls.
Group rows by:

1. Workspace-level surfaces.
2. Application-scoped surfaces.
3. Settings.
4. Docs and Galileo.
5. Legacy redirects.

Each row contains path, source file, and canonical/redirect status. Redirect
rows include the destination when the destination is statically readable from
the redirect module.

### 7.2 `api-routes.md`

Source: `apps/api-rs/crates/api/src/main.rs` and the route modules it mounts.

Extract `.route("<path>", <handlers>)` entries and the HTTP methods applied to
each path. Group by path prefix (`/api/workspaces/`, `/api/v1/`, auth, health,
internal ingest, and so on). Each row contains method, path, handler symbol, and
source file.

### 7.3 `schema.md`

Source: `apps/api-rs/migrations/*.sql`.

Extract `CREATE TABLE`, `ALTER TABLE`, and `DROP TABLE` statements. Each table
row contains the table name, migration that created it, latest migration that
changed it, and whether it is currently present. Column-level details remain in
the migration files and `design/02-data-model.md`; the generated inventory is a
table-level map.

### 7.4 Generation and checking

New scripts:

- `scripts/docs-inventory.mjs` — generates the three inventory files.
- `scripts/docs-check.mjs` — verifies frontmatter, links, hashes, generated
  files, and prohibited stale claims.
- `scripts/docs-verify.mjs` — recomputes and writes `source_hash` and
  `last_verified` for one or more documents.

Root scripts:

```json
{
  "docs:inventory": "node scripts/docs-inventory.mjs",
  "docs:verify": "node scripts/docs-verify.mjs",
  "docs:check": "node scripts/docs-check.mjs",
  "docs:test": "node --test scripts/docs-*.test.mjs"
}
```

The scripts use only Node built-ins. They must be deterministic and sort all
output. `docs:check` regenerates inventories in memory and fails when committed
files differ. It also fails on these explicit stale claims in current documents:
`Plane for ITSM`, `ITSM is future`, `ITSM remains future`, `6 apps`, and
`six apps`.

## 8. Content Rules

### 8.1 Current documents

- Start with a one-sentence purpose and a "Source of truth" list.
- Link to generated inventories instead of listing every route or endpoint.
- Reference files by path and symbol name; avoid line numbers.
- Distinguish canonical routes from legacy redirects.
- State implemented behavior and explicit non-goals; do not mix roadmap items
  into current behavior.
- Use the product terms above and include the code term on first use.

### 8.2 Feature documents

Each feature document answers:

1. What user problem does this surface solve?
2. Which routes and layouts implement it?
3. Which stores, services, and API routes carry the state?
4. Which permissions and scopes apply?
5. What are the important edge cases, empty states, and known limitations?
6. Where is the in-app user guide for this topic?

### 8.3 Design documents

Each design document keeps the existing `design/` role: cross-cutting
engineering contracts. `07-routing-and-navigation.md` owns route families,
workspace/application scope, the app rail, and legacy redirects. `01` through
`06` keep their current topics but are rewritten from code.

## 9. File Disposition

| Path                                       | Action                       | Destination / note                                                     |
| ------------------------------------------ | ---------------------------- | ---------------------------------------------------------------------- |
| `docs/README.md`                           | Rewrite                      | Index, status table, reading paths, generated inventory links          |
| `docs/PRODUCT.md`                          | Rewrite                      | Current product, users, scope, non-goals, terminology                  |
| `docs/CHEAT-SHEET.md`                      | Rewrite                      | Current architecture decisions; keep the filename                      |
| `docs/business-capabilities.md`            | Delete                       | Content merges into `PRODUCT.md` and `features/*`                      |
| `docs/linting.md`                          | Keep                         | Verify commands; update only if stale                                  |
| `docs/design/README.md`                    | Rewrite                      | Reading order, status, source map                                      |
| `docs/design/01-architecture.md`           | Rewrite                      | Seven apps, workspace/application model, current stack                 |
| `docs/design/02-data-model.md`             | Rewrite                      | Current migrations, services, war rooms, AI, workspace settings        |
| `docs/design/03-api-contract.md`           | Rewrite                      | Current api-rs routes, auth, scopes, errors                            |
| `docs/design/04-design-system.md`          | Verify and rewrite as needed | Tokens, primitives, shell changes                                      |
| `docs/design/05-testing-strategy.md`       | Rewrite                      | Rust, Django fallback, Vitest, docs checks                             |
| `docs/design/06-ops-runbook.md`            | Rewrite                      | Correct api-rs local stack, live server, tunnels, rebuilds             |
| `docs/design/07-routing-and-navigation.md` | New                          | Route families, scope, rail, redirects, generated routes link          |
| `docs/features/README.md`                  | Rewrite                      | Current inventory only; no missing files                               |
| `docs/features/work-items.md`              | Rewrite                      | Tickets plus application work items                                    |
| `docs/features/services.md`                | New                          | Replaces `cmdb.md`; catalog, graph, health, metrics                    |
| `docs/features/intake.md`                  | Rewrite                      | Workspace and application intake, sources, triage                      |
| `docs/features/knowledge.md`               | New                          | Replaces `pages.md`; pages, KB, Space                                  |
| `docs/features/sprints.md`                 | New                          | Replaces `cycles.md`; route remains `/cycles`                          |
| `docs/features/tracks.md`                  | New                          | Replaces `modules.md`; route remains `/modules`                        |
| `docs/features/war-rooms.md`               | Verify and rewrite           | Current incident room behavior                                         |
| `docs/features/ai.md`                      | New                          | Galileo assistant, full page, schedules                                |
| `docs/features/analytics.md`               | Rewrite                      | Current workspace reports                                              |
| `docs/features/applications.md`            | New                          | Applications/projects, scope selector, membership                      |
| `docs/features/settings.md`                | Rewrite                      | Workspace, application, profile, god-mode                              |
| `docs/features/archives.md`                | Verify and rewrite           | Current archive routes                                                 |
| `docs/features/drafts.md`                  | Verify and rewrite           | Workspace drafts                                                       |
| `docs/features/stickies.md`                | Verify and rewrite           | Personal stickies                                                      |
| `docs/features/views.md`                   | Rewrite                      | Queues, workspace views, application views                             |
| `docs/features/browse.md`                  | Rewrite                      | Canonical work item URL                                                |
| `docs/features/_backlog.md`                | Delete                       | Implemented items move to docs; remaining non-goals go to `PRODUCT.md` |
| `docs/features/_shared/README.md`          | Rewrite                      | Only files that exist                                                  |
| `docs/features/_shared/list.md`            | Rewrite                      | Shared list/filter/sort behavior                                       |
| `docs/features/_shared/detail-page.md`     | Rewrite                      | Shared detail layout                                                   |
| `docs/features/_shared/comments.md`        | New                          | Comments, activity, versions                                           |
| `docs/features/_shared/search.md`          | New                          | Global search and command palette                                      |
| `docs/features/_shared/routing.md`         | Delete                       | Replaced by `design/07-routing-and-navigation.md`                      |
| `docs/features/cmdb.md`                    | Delete                       | Replaced by `features/services.md`                                     |
| `docs/features/cycles.md`                  | Delete                       | Replaced by `features/sprints.md`                                      |
| `docs/features/modules.md`                 | Delete                       | Replaced by `features/tracks.md`                                       |
| `docs/features/pages.md`                   | Delete                       | Replaced by `features/knowledge.md`                                    |
| `docs/features/inbox.md`                   | Rewrite and rename           | Destination `features/notifications.md`                                |
| `docs/ui/README.md`                        | Verify                       | Update links and status                                                |
| `docs/ui/design-tokens.md`                 | Verify                       | Update only if tokens changed                                          |
| `docs/ui/shell.md`                         | Rewrite                      | App rail, sidebar modes, responsive shell                              |
| `docs/ui/editor.md`                        | Verify                       | Update only if editor contract changed                                 |
| `docs/superpowers/README.md`               | New                          | Historical archive policy                                              |

Renaming `inbox.md` to `notifications.md` requires updating every relative link
in the corpus. The old inbox page is notifications; intake has its own document.

## 10. Execution Phases

### Phase 0 — Baseline

- Confirm a clean working tree before edits.
- Snapshot route, API, and schema inventories as scratch output.
- Record the current doc inventory and disposition from section 9.
- No content edits in this phase.

### Phase 1 — Tooling

- Add the three docs scripts and their Node tests.
- Add `docs:inventory`, `docs:verify`, `docs:check`, and `docs:test` to the root
  `package.json`.
- Generate and commit `docs/_generated/{routes,api-routes,schema}.md`.
- Add `docs/superpowers/README.md`.
- Verify that the generated route count matches `core.ts`, the API count matches
  `main.rs`, and the table count matches the migrations.

### Phase 2 — Core Documents

- Rewrite `docs/README.md`, `PRODUCT.md`, `CHEAT-SHEET.md`, and
  `design/01` through `design/07`.
- Add frontmatter with accurate `source_paths`.
- Run `pnpm docs:verify` for each rewritten document.
- Update root `README.md` links only if paths changed.

### Phase 3 — Feature Documents

- Create the new capability documents and shared docs.
- Delete superseded feature documents.
- Update `features/README.md` so every listed file exists and every existing
  file is listed.
- Link each feature doc to its in-app docs topic and generated route entries.
- Run `pnpm docs:verify` for each rewritten or new document.

### Phase 4 — UI and Archive

- Verify `ui/design-tokens.md` and `ui/editor.md`.
- Rewrite `ui/shell.md` for the app rail and workspace modes.
- Add `docs/superpowers/README.md` and confirm no current document points to
  superpowers as current behavior.

### Phase 5 — Guardrail

- Add `.github/workflows/docs-check.yml`.
- The workflow runs on changes to `docs/**`, `scripts/docs-*`,
  `apps/web/app/routes/core.ts`, `apps/api-rs/crates/api/src/main.rs`,
  `apps/api-rs/migrations/**`, and the root `package.json`.
- Add an `AGENTS.md` rule: after changing routes, API routes, migrations, or
  documented behavior, run `pnpm docs:verify <doc>` and `pnpm docs:check`.
- Run `pnpm check:format`, `pnpm docs:test`, and `pnpm docs:check`.

## 11. Validation

The work is complete when all of the following pass:

1. `pnpm docs:test` passes.
2. `pnpm docs:check` passes with no stale documents.
3. `pnpm exec oxfmt --check docs scripts` passes.
4. `git diff --check` passes.
5. Every file listed in section 9 exists at its destination, and every file
   marked delete is absent.
6. Every document in `docs/features` has frontmatter, valid source paths, and
   no reference to a deleted file.
7. Generated inventories contain the same route, API, and table counts as a
   fresh parse of their sources.
8. No current document contains `Plane for ITSM`, describes ITSM as future, or
   says the monorepo has six apps.
9. A manual spot-check of five documents confirms their route and symbol
   references exist.
10. The CI workflow is valid YAML and runs `pnpm docs:check`.

## 12. Risks and Mitigations

| Risk                                          | Mitigation                                                                                           |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Regex parsers miss routes or migrations       | Keep the supported syntax narrow, add parser tests, fail loudly on zero matches or unexpected counts |
| Source hashes change on formatting-only edits | Expected; `docs:verify` followed by a review confirms the document still matches                     |
| Large rewrite creates review fatigue          | Land in five phases; each phase is independently reviewable                                          |
| Deleting documents breaks external links      | `docs/` is not a public site; update all in-repo links and rely on git history                       |
| Feature docs duplicate in-app docs            | Feature docs focus on implementation and link to in-app topics for user flows                        |
| CI is noisy on unrelated PRs                  | Trigger the docs workflow only on docs, route, API, migration, and docs-script paths                 |

## 13. Out of Scope

- In-app docs content under `apps/web/core/components/docs/content`.
- Translation of `docs/` to English.
- A static documentation site.
- API reference generation beyond route lists.
- Refactoring application code to make documentation easier.
- Updating historical `docs/superpowers` files.
