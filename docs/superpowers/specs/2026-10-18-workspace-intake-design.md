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

Terminology and facts (verified in the codebase):

- Intake = Inbox: per-project triage. The project page renders
  `InboxIssueRoot` (`apps/web/app/.../projects/[projectId]/intake/page.tsx`).
- Intake-issue statuses: Pending `-2`, Rejected `-1`, Snoozed `0`,
  Accepted `1`, Duplicate `2` (`EInboxIssueStatus`, Django `IntakeIssueStatus`).
- Tabs: `?currentTab=open|closed` (project default: open when unset).
- Request sources: `IN_APP | FORMS | EMAIL | WEBHOOK`.
- Row click in the project UI navigates to
  `/[slug]/projects/{pid}/intake?currentTab=<tab>&inboxIssueId=<issueId>` —
  note `inboxIssueId` is the **issue** id, not the intake-issue id.
- Accept converts the request into a ticket (needs per-project context:
  work item type, service, default state, duplicate check). Decline, snooze
  (with date), and mark-duplicate are project writes.
- Row JSON shape and envelope are reusable verbatim: the project
  `inbox_list_json` row (incl. nested `issue` with `project_id`) and
  `DetailEnvelope`.

**Decision: v1 is read-only aggregation.** Triage actions, creating intake
requests, and AI triage suggestions all stay in the project context, where the
required per-project data lives. The workspace page is the dispatcher's queue:
what's pending across projects, jump in to triage.

Unlike Tickets (existing workspace endpoint), Intake has no workspace-level
read endpoint — **this adds a backend endpoint**
(`GET /api/workspaces/:slug/intake-issues/`), plus the web page and a small
shared-shell addition (flat mode). No Django changes.

## Goals

- Workspace route `/[workspaceSlug]/intake` streaming intake requests newest
  first across member projects.
- Tabs Open (pending + snoozed) / Closed (accepted, rejected, duplicate) via
  `?currentTab`, mirroring project semantics.
- Project filter via `?project=` and name search, both server-side.
- Load more (100 per page) with the project endpoint's cursor envelope.
- Click-through to the project inbox with the tab and the item open.

## Non-goals (v1)

- Inline triage (accept/decline/snooze/mark-duplicate), creating intake
  requests, AI triage suggestions.
- Unread badges/counts in the sidebar, saved filters, kanban/peek, grouping.

## Terminology

Intake = Inbox (service-desk triage). Status chip reuses `INBOX_STATUS`
(`@plane/constants`) and `InboxStatusIcon`
(`apps/web/core/components/inbox/inbox-status-icon.tsx`).

## UX design

### Routes

Following the tickets/KB pattern:

- `:workspaceSlug/intake` → `(projects)/intake/{layout,page}.tsx` with
  `AppHeader` + breadcrumb ("Intake", `InboxOutline`) and `PageHead`.

### Sidebar

At the end of the entity list (mirroring the project sidebar, where intake is
last):

Home → Tickets → Sprints → Tracks → Services → KB → **Intake** → Scheduler → Docs

- Item `intake` (`/intake/`), icon `IntakeOutline` (same as project sidebar),
  access: `ADMIN, MEMBER` (triage is operator-only; guests are excluded from
  the entry even though the endpoint still scopes server-side).
- New short label **"Intake"** (project nav uses the longer "Service desk
  intake"). New i18n key defaulting to en; other locales via the translate
  skill at implementation time.
- Register in `sidebar-item.tsx` `staticItems` and `helper.tsx`.

### Page layout

- `page.tsx` reads `?currentTab` (default `open`) and `?project`, renders the
  workspace intake list view.
- Tab row above the shared shell (Open / Closed), preserving `?project=`.
- The list view renders the shared `WorkspaceAggregateList` shell in a new
  **flat mode** (no group header when there is a single unlabeled group):
  project filter, search input, count (`total_results`), flat rows newest
  first, loading/empty states, Load more footer.
- Empty-state keys merged into the existing `workspace_empty_state` namespace
  (`intake.title/description`), en first.

### Rows

Each row (existing `ListItem` component):

- Prepend: small request icon (reuse the project row treatment where cheap,
  else a neutral icon).
- Title: `{IDENTIFIER}-{sequence_id}` (muted) + request name.
- Append: project badge, source badge (`IN_APP | FORMS | EMAIL | WEBHOOK`),
  priority icon, creator (avatar + name), created date; status chip for
  non-pending rows (`INBOX_STATUS` lookup + `InboxStatusIcon`); snoozed rows
  show the snooze date like the project row does.
- Click: `ListItem itemLink` →
  `/[workspaceSlug]/projects/{pid}/intake?currentTab=open|closed&inboxIssueId={issueId}`
  with the **issue** id (matches project behavior).

### Filters and pagination

- Tabs: open → statuses `[-2, 0]` (pending + snoozed); closed → `[1, -1, 2]`
  (accepted, rejected, duplicate). Invalid `?currentTab` falls back to open.
  Open intentionally includes snoozed to match the project's effective
  behavior (snoozed items stay actionable).
- Project filter `?project=<id>` server-side; invalid ids fall back to all
  (normalized client-side against `workspaceProjectIds`).
- Search: issue-name `ILIKE` server-side, debounced 300ms, request aborted on
  change. (Covers names only, not keys like `ENG-123` — same v1 limitation
  as Tickets.)
- `per_page=100`, `cursor` for Load more; total from `total_results`; empty
  state when total is 0.
- Fixed newest-first order (`issue.created_at` desc). No `order_by` param in
  v1 — an explicit, documented simplification (a triage queue is always
  newest-first).

## Data flow

### Backend — new endpoint `GET /api/workspaces/:slug/intake-issues/`

- Gate: any active workspace member (20/15/5); otherwise 403 (same shape as
  the project endpoint, `member::deny_detail`).
- Params: `tab` (`open | closed`, default `open`; invalid → `open`),
  `project_id` (optional filter; malformed UUID → axum 422, same as existing
  typed query params), `search` (issue-name `ILIKE` — **addition** vs the
  project endpoint: the project list has no text search), `per_page` (default
  100, cap 1000, invalid → 400), `cursor` (invalid → 400). No `order_by` in
  v1 — fixed newest-first order.
- **Union across all intakes** of each member project (explicit deviation
  from the project endpoint, which uses the first intake by name): a
  dispatcher queue must not hide requests from a second channel. If the same
  issue is linked through two intakes it can appear twice (distinct
  intake-issue rows, each with its own triage status) — recorded as a known
  v1 edge case.
- Scope: member projects only (active membership required —
  **explicitly stricter** than the project endpoint's workspace-member-only
  gate, consistent with every other workspace aggregate); non-member projects
  contribute nothing; archived/deleted rows excluded.
- Visibility per project — mirror the project `guest_only` logic exactly:
  `(NOT restricted-guest-on-that-project) OR intake-issue created_by = me`,
  where restricted = role 5 + project `guest_view_all_features = false`.
- Row shape: **exactly reuse** `inbox_list_json` (project nested `issue`,
  incl. `issue_project_id`); envelope: `DetailEnvelope` (same fields as the
  workspace issues endpoint: `results`, cursors, `next_page_results`,
  `prev_page_results`, `total_count`, `total_pages`, `total_results`).
- No `inbox_view` feature-flag check (mirrors the project API — the flag
  gates the project UI only).

### Web

- New service method `listWorkspace(slug, params, config)` in
  `apps/web/core/services/inbox/inbox-issue.service.ts` →
  `/api/workspaces/:slug/intake-issues/`.
- New components: `core/components/inbox/workspace/workspace-intake-list-view.tsx`
  (domain folder mirrors the inbox feature area, like the project inbox UI)
  (+ pure helpers `core/lib/workspace-intake.ts` with Vitest tests: query
  builder, envelope normalize, pagination reducer, flat-group builder,
  key/link builder).
- List view local state (mirror Tickets; do NOT touch the project inbox
  stores): `results`, `nextCursor`, `totalResults`, loading flags; search
  debounce + abort; reset on tab/filter change; single flat group.
- Lookups: project (`getProjectById` → name + `identifier` for the key/link),
  creator (`getUserDetails`), date (`renderFormattedDate`), priority icon,
  source badge (raw string), status chip.
- Click target: issue id from the row + active tab (mirrors project
  deep-link semantics).
- Permissions server-side; sidebar entry is `ADMIN, MEMBER`.

### Permissions

No new web gates beyond the sidebar entry. The endpoint enforces
member-project scoping plus the per-project restricted-guest rule. Guests get
no sidebar entry (operator-only surface).

### i18n

- Reuse `common.load_more`, `common.loading`, `service.catalog_all_projects`,
  `service.catalog_search_placeholder`, `sidebar.work_items`-style existing keys.
- New sidebar label "Intake" (new key, en first, other locales via the
  translate skill).
- `workspace_empty_state.intake.{title,description}` merged into the existing
  `workspace_empty_state` namespace.

## Files

Backend (new + modified):

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs` — query type, scope
  helper, `workspace_intake_issues` handler (reuse `parse_per_page`,
  `parse_cursor`, `page_window`, `next_cursor_str`, `DetailEnvelope` from
  `issue_common`; reuse `inbox_list_json` + `InboxListRow` in place).
- Modify: `apps/api-rs/crates/api/src/main.rs` — one route registration.
- New: `apps/api-rs/crates/api/tests/intake_workspace_test.rs` (mirror intake
  test fixture conventions; run serially with `DATABASE_URL`).
- No Django changes.

Web (new + modified):

- `apps/web/app/(all)/[workspaceSlug]/(projects)/intake/layout.tsx`, `page.tsx`.
- `apps/web/core/components/inbox/workspace/workspace-intake-list-view.tsx`.
- `apps/web/core/lib/workspace-intake.ts` + `workspace-intake.test.ts`.
- `apps/web/core/services/inbox/inbox-issue.service.ts` (new `listWorkspace` method).
- `apps/web/app/routes/core.ts` (one route block).
- `apps/web/core/components/workspace/aggregate-list/workspace-aggregate-list.tsx`
  (optional flat mode — no group headers for a single unlabeled group).
- `packages/constants/src/workspace.ts`, `sidebar-item.tsx`, `helper.tsx`.
- `packages/i18n/src/locales/en/empty-state.json` (merged keys) + new sidebar
  label key.

## Testing

- Backend (Rust, DB-backed, serial): visibility matrix (member sees member
  projects only; restricted guest sees only own requests; non-member project
  invisible; outsider 403), tab mapping (open/closed/invalid), project
  filter, name search, cursor pagination + `total_results`, newest-first
  order, multi-intake union, flat row shape (`issue` nested + project id).
- Web (Vitest): helpers (`workspace-intake.test.ts`) — query builder
  (tab/project/search + invalid tab fallback), envelope normalize,
  pagination reducer, flat-group builder, key/link builder.
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
   restricted-guest scoping holds.

## Risks and open questions

- **Tab semantics for snoozed:** Open includes snoozed (`[-2,0]`) to match
  the project's effective behavior; snooze dates are shown in the row.
- **Flat queue with `flat` shell mode:** the shell gains an optional mode for
  headerless single-group lists; reusable by future simple views.
- **Multi-intake union:** differs from the project endpoint's first-intake
  rule; possible duplicate rows if one issue is bridged to two intakes
  (distinct triage statuses make this defensible, not a bug for v1).
- **Search covers the issue name only**, not keys like `ENG-123`.

## Out of scope (v1)

Inline triage (accept/decline/snooze/mark-duplicate), creating intake
requests, AI triage suggestions, unread badges/sidebar counts, saved
filters, kanban/peek, grouping.
