# Workspace-level KB — Design

**Date:** 2026-10-18
**Status:** Approved design; pending written-spec review

## Context

This is the third sub-project of the workspace-level aggregation series
(Sprints & Tracks and Tickets shipped first). Goal: a read-only workspace page
listing knowledge-base Pages across the user's member projects, with a project
filter and search, grouped per project inside access tabs (All / Public /
Private / Archived), with click-through to the existing project page detail.

Notes:

- The existing workspace `/docs` route is static product documentation — not
  the KB. The KB (`Pages`) lives per project today (`/[slug]/projects/:pid/pages`)
  with `public | private | archived` tabs.
- The `pages` data model is already workspace-level (`pages.workspace_id`),
  with projects attached via the `project_pages` join. Pages nest through
  `parent`. `access` is 0 (public) / 1 (private).
- Permission semantics to mirror (all verified in `routes/page.rs`):
  owner bypasses everything; private non-owner is always denied; public pages
  are readable by project members 20/15/5; restricted guests
  (`guest_view_all_features = false`) only see pages they own
  (`page_perm_decision`, `guest_restricted`, project `list` SQL).
- Unlike Sprints/Tracks/Tickets, no workspace-level pages endpoint exists —
  **this sub-project adds a backend endpoint** (`GET /api/workspaces/:slug/pages/`),
  plus the web page. No Django changes.

## Goals

- Workspace route `/[workspaceSlug]/kb` (short for Knowledge Base; fall back to
  `/knowledge-base` if `kb` collides — to verify in implementation).
- Tabs All (default, unarchived) / Public / Private / Archived via `?type=`,
  mirroring the project UI; search (`?search=`); project filter (`?project=`),
  all server-side.
- Pagination: 100 per page + Load more (cursor), like Tickets.
- Click-through to the existing project page detail
  (`/[workspaceSlug]/projects/:projectId/pages/:pageId`).
- Reuse the `WorkspaceAggregateList` shell (filter header, groups, Load more
  footer).

## Non-goals (v1)

- Creating/editing/locking/moving pages, versions, labels, favorites actions,
  tree navigation — all stay in the project context.
- A workspace page tree or parent-chain display; the flat list is for
  discovery, trees stay per project.
- Django changes.

## Terminology

KB = Pages (knowledge base). Nav label reuses `sidebar.pages`
("Knowledge base"). Tabs mirror `TPageNavigationTabs` plus an "All" default.

## UX design

### Routes

Following the tickets pattern:

- `:workspaceSlug/kb` → `(projects)/kb/{layout,page}.tsx` with `AppHeader` +
  breadcrumb ("Knowledge base", `PagesOutline`), and `PageHead`.

### Sidebar

In `packages/constants/src/workspace.ts`:

- Add `kb` (`/kb/`) to `WORKSPACE_SIDEBAR_STATIC_NAVIGATION_ITEMS` and insert
  into `..._LINKS` after Services:
  Home → Tickets → Sprints → Tracks → Services → **KB** → Scheduler → Docs
  (follows the project sidebar's relative order).
- Label `sidebar.pages`, icon `PagesOutline`, access `ADMIN, MEMBER, GUEST`.
- Register in `sidebar-item.tsx` `staticItems` and `helper.tsx`.

### Page layout

- `page.tsx` reads `?type` (default `all`), `?project`, and renders the
  workspace KB list view.
- Tabs row (All / Public / Private / Archived) above the shared shell header,
  following the project pages UX; changing tab keeps `project`/`search` params.
- The list view renders the shared `WorkspaceAggregateList` shell: project
  filter, search input, count (`total_results`), groups, loading/empty states,
  Load more footer.
- Empty state keys added to the existing `workspace_empty_state` namespace
  (do NOT create a second namespace — lesson from earlier work).

### Rows

Each row (existing `ListItem` component):

- Prepend: small page icon — reuse the project's page row icon treatment
  (logo/color dot if cheap; otherwise a static pages icon), plus a lock icon
  when `is_locked`.
- Title: page name.
- Append: access chip (Public/Private; Archived badge inside the archived
  tab), owner (avatar + name via member store), updated date.
- Project badges for all linked member projects (second badge onwards in
  muted style).
- Click: `itemLink` →
  `/[workspaceSlug]/projects/{pid}/pages/{pageId}` where `pid` is the `?project=`
  filter when it links the page, else the row's first project id.

### Grouping

- Within a tab, group by project (sections ordered alphabetically by project
  name) using the row's `project_ids`.
- Multi-linked pages appear **once**, under their alphabetically-first member
  project group; badges show all linked member projects.
- Empty groups are not rendered.

## Data flow

### Backend — new endpoint `GET /api/workspaces/:slug/pages/`

- Gate: active workspace member (roles 20/15/5); others 403.
- Query params:
  - `type`: `all` (default, unarchived) | `public` | `private` | `archived`.
  - `project_id`: optional, filters `project_pages.project_id`.
  - `search`: name `ILIKE` (same semantics as the project list `search`).
  - `per_page` (default 100), `cursor`, `order_by` (reuse
    `sanitize_page_order_by` allowlist; default `-updated_at`).
- Scope: pages in the workspace, not deleted, linked via non-deleted
  `project_pages` to a project where the user has an active membership;
  excluded when the linked project is archived.
- Visibility per row — mirror the project `list` SQL:
  `(owned_by_id = me) OR (access = 0 AND NOT restricted-guest-on-that-project)`;
  restricted guests only ever see their own pages.
- Tab filter: all → `archived_at IS NULL`; public → `access = 0` unarchived;
  private → `access = 1` unarchived (visibility already scopes to own);
  archived → `archived_at IS NOT NULL` (visibility still applies).
- No `parent_id` filter: parents and children are all returned as flat rows
  (`parent` field kept for context; the web does not render the tree in v1).
- One row per page (DISTINCT over pages); `project_ids` contains **only
  projects the user is a member of** (no leaking non-member project links).
- Row shape: exact reuse of the project `page_list_json` fields
  (`id`, `name`, `owned_by`, `access`, `color`, `parent`, `is_favorite`,
  `is_locked`, `archived_at`, `workspace`, `created_at`, `updated_at`,
  `created_by`, `updated_by`, `view_props`, `logo_props`, `label_ids`,
  `project_ids`).
- Envelope: same style as the workspace issues endpoints —
  `{ results, next_cursor, next_page_results, total_results }`.

### Web

- New service method `getWorkspacePages(slug, params, config)` in
  `apps/web/core/services/page/project-page.service.ts` (page domain, so it
  lives with the page service).
- New components: `core/components/pages/workspace/workspace-kb-list-view.tsx`
  (+ pure helpers `core/lib/workspace-kb.ts` with Vitest tests) and
  `app/(all)/[workspaceSlug]/(projects)/kb/{layout,page}.tsx`.
- Local React state in the list view (mirror tickets; do NOT touch the project
  page store): `results`, `nextCursor`, `totalResults`, `isLoading`,
  `isLoadingMore`, tab, debounced search; abort in-flight requests; Load more
  appends.
- Server params from UI state: `type` (tab), `project_id` (normalized against
  `workspaceProjectIds` — invalid ids fall back to all), `search` (trimmed),
  `per_page=100`, `cursor`, default `order_by=-updated_at`.
- Lookups: project names (`getProjectById`), owner (`getUserDetails`),
  date (`renderFormattedDate`).
- Click target: filtered project when linked there, else first `project_ids` entry.

### Permissions

No web gates beyond the sidebar entry (visibility is server-side). The
endpoint enforces member-project scoping, owner/private rules, and restricted
guest rules. Sidebar access: `ADMIN, MEMBER, GUEST`.

### i18n

- Reuse `sidebar.pages`, catalog header keys, `common.load_more`, `common.loading`.
- New empty-state keys merged into the existing `workspace_empty_state`
  namespace in `packages/i18n/src/locales/en/empty-state.json`:

```json
"kb": {
  "title": "No pages yet",
  "description": "Knowledge base pages from projects you are a member of will show up here."
}
```

## Files

Backend (new + modified):

- New: `GET /api/workspaces/:slug/pages/` route in `main.rs` (+ handler in
  `apps/api-rs/crates/api/src/routes/page.rs`, reusing the list-SQL
  visibility shape, `page_list_json`, order allowlist, and issue-style cursor
  pagination).
- New: DB-backed integration tests alongside the existing page suites
  (`page_test.rs`, `page_parent_filter_test.rs`, `page_desc_patch_test.rs`
  conventions; serial execution with `DATABASE_URL`).
- No Django changes.

Web (new + modified):

- `apps/web/app/(all)/[workspaceSlug]/(projects)/kb/layout.tsx`, `page.tsx`
- `apps/web/core/components/pages/workspace/workspace-kb-list-view.tsx`
- `apps/web/core/lib/workspace-kb.ts` + `workspace-kb.test.ts`
- `apps/web/app/routes/core.ts` (one route block)
- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`
  (tabs are rendered by the list view above the shell — no shell change unless
  the tab row turns out to be reusable; implementation may add a small
  optional `tabs` slot)
- `packages/constants/src/workspace.ts`, sidebar `sidebar-item.tsx`, `helper.tsx`
- `packages/i18n/src/locales/en/empty-state.json` (merged keys)

## Testing

- Backend (Rust, DB-backed, serial): visibility matrix (public readable by
  member + non-restricted guest; private visible only to owner; restricted
  guest sees only own pages; non-member projects invisible), tabs
  (all/public/private/archived), search, `project_id` filter, cursor
  pagination (`next_page_results`/`total_results`), row `project_ids` scoped to
  member projects only, invalid `type` falls back to `all`.
- Web (Vitest): helpers (`workspace-kb.ts`) — query-param builder
  (`type`/`project`/`search` composition, invalid values defaulted),
  project-grouping with dedupe by page id, merge of multi-link badges,
  pagination reducer (reuse the proven tickets pattern).
- No Django tests affected.

## Verification

1. `cargo test -p api --test <pages suite(s)>` with `DATABASE_URL` (serial).
2. `pnpm check:types` and targeted Vitest + full `pnpm --filter=web test`.
3. `pnpm --filter=@plane/constants build` (sidebar entry) then
   `pnpm --filter=web build`.
4. `systemctl --user restart plane-web-prod.service` and health check.
5. Smoke on the tunnel: sidebar shows KB after Services; All/Public/Private/
   Archived tabs load; project filter + search work server-side; Load more;
   rows show owner/access/updated; click opens project page detail; private
   pages of others never appear; multi-linked pages appear once with badges.

## Risks and open questions

- **Pagination and grouping:** groups and per-project headers are computed
  over loaded rows only (same caveat as Tickets). If a filter matches rows
  beyond loaded pages, headers undercount — acceptable for v1.
- **No tree/parent context in v1:** children appear as flat rows; hierarchy
  stays in the project context.
- **Favorites:** rows carry `is_favorite` but the v1 page has no favorite
  actions or favorite-first ordering; add later if needed.

## Out of scope (v1)

Create/edit pages, favorites/labels/versions actions, tree navigation,
lock/move/archive actions, edit access — all stay in the project context.
