# Workspace-level Intake — Design

**Date:** 2026-10-18
**Status:** Approved design; pending written-spec review

## Context

This is the fourth and last sub-project of the workspace-level aggregation
series (Sprints & Tracks, Tickets, KB shipped first). Goal: a read-only
workspace page listing incoming service requests (intake/inbox issues) across
the user's member projects, with Open/Closed tabs, project filter and search,
Load more pagination, and click-through to the existing project inbox with the
item open.

Terminology and domain facts (verified in the codebase):

- Intake = Inbox: per-project triage of incoming requests. The project page
  renders `InboxIssueRoot` (`apps/web/app/.../projects/[projectId]/intake/page.tsx`).
- Intake-issue statuses: Pending `-2`, Rejected `-1`, Snoozed `0`,
  Accepted `1`, Duplicate `2` (`EInboxIssueStatus`, Django
  `IntakeIssueStatus`).
- Tabs: `?currentTab=open|closed` (default open). Open = pending; closed =
  accepted/rejected/duplicate. Snoozed stays visible under Open.
- Request sources: `IN_APP | FORMS | EMAIL | WEBHOOK`.
- Accept converts the request into a ticket (needs per-project context:
  work item type, service, default state, duplicate check). Decline, snooze
  (with date), and mark-duplicate are project writes.
- Row click in the project UI navigates to
  `/[slug]/projects/{pid}/intake?currentTab=<tab>&inboxIssueId=<issueId>` —
  note `inboxIssueId` is the **issue** id.

**Decision: v1 is read-only aggregation.** Triage actions (accept / decline /
snooze / duplicate), creating intake, and AI triage suggestions all stay in
the project inbox, where the required context lives. The workspace page is the
dispatcher's queue: what's pending across projects, jump in to triage.

Unlike Tickets (existing workspace endpoint), Intake has no workspace-level
read endpoint — **this adds one backend endpoint**
(`GET /api/workspaces/:slug/intake-issues/`), plus the web page and a small
shared-shell addition (flat mode). No Django changes.

## Goals

- Workspace route `/[workspaceSlug]/intake` streaming intake requests across
  member projects, newest first.
- Tabs Open (pending + snoozed) / Closed (accepted, rejected, duplicate) via
  `?currentTab`, mirroring project semantics.
- Project filter via `?project=` and name search (server-side).
- Cursor-based Load more (100 per page) reusing the project endpoint's
  envelope.
- Click-through to the project inbox with the tab and the item open.

## Non-goals (v1)

- Inline triage (accept/decline/snooze/duplicate), creating intake requests,
  AI triage suggestions — all stay in the project context.
- Saved filters, unread badges/counts in the sidebar, grouping, kanban/peek.

## Terminology

Intake = Inbox (service-desk triage). Row status chip reuses `INBOX_STATUS`
(`@plane/constants`) and `InboxStatusIcon` (`@plane/propel/icons`).

## UX design

### Routes

Following the tickets/KB pattern:

- `:workspaceSlug/intake` → `(projects)/intake/{layout,page}.tsx` with
  `AppHeader` + breadcrumb and `PageHead`.

### Sidebar

After KB (end of the entity list, mirroring the project sidebar where intake
is last):

Home → Tickets → Sprints → Tracks → Services → KB → **Intake** → Scheduler → Docs

- Item `intake` (`/intake/`), icon `IntakeOutline` (same as project sidebar).
- Access: `ADMIN, MEMBER` (triage visibility is operator-only; guests are
  excluded from the entry even though the endpoint still scopes server-side).
- Register in `sidebar-item.tsx` `staticItems` and `helper.tsx`.
- Label: short **"Intake"** (project nav uses the longer "Service desk
  intake", which is too long here). This needs a new i18n key defaulting to
  en (`sidebar.intake_short` or a workspace-scoped label); other locales via
  the translate skill at implementation time.

### Page layout

- `page.tsx` reads `?currentTab` (default `open`) and `?project`, renders the
  workspace intake list view.
- Tab row above the shared shell (Open / Closed), preserving `?project` when
  switching (same pattern as the KB tabs).
- The list view renders the shared `WorkspaceAggregateList` shell in a new
  **flat mode** (no group header when there is a single unlabeled group):
  project filter (`?project=`), search input, count (`total_results`), flat
  rows newest-first, loading/empty states, Load more footer.
- Empty-state keys merged into the existing `workspace_empty_state` namespace
  (`intake.title/description`), en first.

### Rows

Each row (existing `ListItem` component):

- Prepend: small request icon (reuse the project inbox row treatment).
- Title: `{IDENTIFIER}-{sequence_id}` muted + request name.
- Append: project badge, source badge (`IN_APP | FORMS | EMAIL | WEBHOOK`),
  priority icon, creator (avatar + name), created date; status chip for
  non-pending rows (via `INBOX_STATUS` + `InboxStatusIcon`; snoozed shows the
  snooze date like the project row does).
- Click: `ListItem itemLink` →
  `/[workspaceSlug]/projects/{pid}/intake?currentTab=open|closed&inboxIssueId={issueId}`.

### Filters and pagination

- Tabs: `open` → statuses `[-2, 0]` (pending + snoozed); `closed` →
  `[1, -1, 2]` (accepted, rejected, duplicate). Invalid `?currentTab` falls
  back to `open`. The Open tab matches the project's _effective_ behavior
  (snoozed items stay actionable there).
- Project filter `?project=<id>` server-side; unknown ids fall back to all
  (normalized client-side against `workspaceProjectIds`).
- Search: issue name (`ILIKE`) server-side, debounced 300ms, request abort on
  change (same pattern as Tickets/KB).
- `per_page=100`, cursor from `next_cursor`; Load more appends;
  `total_results` drives the header count; empty state when total is 0.
- Flat order: newest first (`issue.created_at` desc by default; `order_by`
  allowlist identical to the project endpoint).

## Data flow

### Backend — new endpoint `GET /api/workspaces/:slug/intake-issues/`

- Gate: any active workspace member (20/15/5, mirroring `list_issues`); 403
  otherwise. Per-row project scoping is stricter than the project endpoint:
  **active membership in the row's project is required** (consistent with all
  other workspace aggregate endpoints; the project endpoint's ws-only gate is
  a quirk we do not copy).
- Params: `tab` (`open | closed`, default `open`; invalid → `open`),
  `project_id` (optional filter), `search` (**addition** vs the project
  endpoint: issue-name `ILIKE`), `per_page` (default 100), `cursor`,
  `order_by` (reuse the project endpoint's allowlist and default
  `-issue__created_at`).
- Scope: for each member project, that project's first intake by name
  (mirror `list_issues`); rows are the intake's un-deleted issues; projects
  without an intake contribute nothing. No `inbox_view` feature-flag check
  (mirrors the project API; the flag only gates the project UI).
- Visibility per row — mirror `list_issues` guest scoping per project:
  active membership required; restricted guests (role 5 on a
  `guest_view_all_features=false` project) only see requests they created
  (the project check keys on the intake-issue row's `created_by_id` — reuse
  it verbatim).
- Row shape: **exactly reuse the project `inbox_list_json` fields** (which
  already include the issue's `project_id`): `id, status, duplicate_to_id,
snoozed_till, source, created_by_id, issue_id, issue_name, issue_priority,
issue_sequence_id, issue_project_id, issue_created_at, issue_label_ids,
issue_created_by_id`. Flat rows — no grouping server-side.
- Envelope: `DetailEnvelope` (`results, next_cursor, prev_cursor,
next_page_results, prev_page_results, counts`), identical to the issues
  workspace endpoints.

### Web

- New service method in the domain service (`InboxIssueService`), e.g.
  `listWorkspace(slug, params, config)` → the new endpoint.
- New components: `core/components/inbox/workspace/workspace-intake-list-view.tsx`
  (+ pure helpers `core/lib/workspace-intake.ts` with Vitest:
  tab↔status mapping, query builder (`tab/search/project`, invalid-tab
  fallback), envelope normalize, flat pagination reducer,
  `issueKey` + click-link builder).
- List view local state (mirror Tickets; do NOT touch the project inbox
  store): `results`, `nextCursor`, `totalResults`, loading flags; debounce +
  abort; tab/filter changes reset to page one.
- Lookups: project (`getProjectById` → name + `identifier` for the key),
  creator (`getUserDetails`), date (`renderFormattedDate`), priority icon,
  source badge, status chip data (`INBOX_STATUS`, `InboxStatusIcon`).
- The shell's single group carries an empty label; the new `flat` mode hides
  group headers in that case.
- Click target: issue id (`issue_id` from the row), tab param preserved.

### Permissions

No new web gates beyond the sidebar entry (visibility is server-side; the
member-project requirement and guest scoping are enforced per row). Sidebar
access `ADMIN, MEMBER`.

### i18n

- Reuse `common.load_more`, `common.loading`, `service.catalog_all_projects`,
  `service.catalog_search_placeholder`, project-row labels where they exist.
- New keys: sidebar label "Intake" (new key, en first) and
  `workspace_empty_state.intake.{title,description}` merged into the existing
  `workspace_empty_state` namespace.

## Files

Backend (new + modified):

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` — query type, scope
  helper, `workspace_intake_issues` handler (reuse `page_window`,
  `parse_*`, `next_cursor_str`, `DetailEnvelope` from `issue_common`, and the
  project row JSON builder).
- Modify: `apps/api-rs/crates/api/src/main.rs` — one route registration.
- New: `apps/api-rs/crates/api/tests/intake_workspace_test.rs` (mirror intake
  test conventions; run serially with `DATABASE_URL`).
- No Django changes.

Web (new + modified):

- `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/layout.tsx`, `page.tsx`.
- `apps/web/core/components/inbox/workspace/workspace-intake-list-view.tsx`.
- `apps/web/core/lib/workspace-intake.ts` + `workspace-intake.test.ts`.
- `apps/web/core/services/inbox/inbox-issue.service.ts` (new `listWorkspace` method).
- `apps/web/app/routes/core.ts` (one route block).
- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`
  (optional `flat` mode).
- `packages/constants/src/workspace.ts`, `sidebar-item.tsx`, `helper.tsx`.
- `packages/i18n/src/locales/en/empty-state.json` (merged keys) + new sidebar
  label key (other locales via the translate skill).

## Testing

- Backend (Rust, DB-backed, serial): visibility matrix (member sees member
  projects only; non-member project invisible; restricted guest sees only own
  requests; outsider 403), tab mapping (open = pending+snoozed; closed =
  accepted/rejected/duplicate; invalid tab → open), project filter, name
  search, pagination (`next_page_results`/`total_results`, second page
  disjoint), default order newest-first, multi-project aggregate without
  duplicates, envelope key shape.
- Web (Vitest): helpers (`workspace-intake.test.ts`) — query builder (tab,
  search trim, project, invalid tab), envelope normalize, append reducer,
  flat-group builder, key/link builders; sibilings from Tickets/KB stay green.
- Full suites stay green.

## Verification

1. `cargo check -p api` + new `cargo test -p api --test intake_workspace_test -- --test-threads=1`.
2. `pnpm check:types` and targeted + full `pnpm --filter=web test`.
3. `pnpm --filter=@plane/constants build` then `pnpm --filter=web build`.
4. Backend rebuild per AGENTS.md (new Rust code → Docker rebuild
   `api worker beat-worker`, detached, poll) + health check + live restart.
5. `systemctl --user restart plane-web-prod.service` + health check + smoke
   on the tunnel: entry appears after KB; Open/Closed tabs; filter + search
   server-side; Load more; click opens project inbox with the item selected;
   private/restricted scoping holds.

## Risks and open questions

- **Tab semantics for snoozed:** Open includes snoozed (`[-2,0]`) to match
  the project's effective behavior; snooze dates are shown in the row.
- **Flat queue:** groups are a single unlabeled list; the shell `flat` mode
  exists only to support this.
- **First-intake-by-name rule** is inherited from the project endpoint; a
  project with multiple intake configs still contributes its rows under one
  intake only.
- **Search covers the issue name only**, not keys like `ENG-123` (same v1
  limitation as Tickets).

## Out of scope (v1)

Inline triage (accept/decline/snooze/mark-duplicate), creating intake
requests, AI triage suggestions, unread badges/sidebar counts, saved
filters, kanban/peek, grouping.
