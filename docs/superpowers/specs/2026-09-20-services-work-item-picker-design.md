# Service Detail — Work Item Picker

Date: 2026-09-20
Status: Approved (pending user review of this spec)
Scope: Frontend-only (`apps/web/core/components/services`, `apps/web/core/services`,
`apps/web/core/store`, `packages/i18n`). No backend, DB, or API change.

## Goal

Replace the raw-UUID linking flow in the service detail **Work items** tab with the
existing multi-select issue picker, and make a failed services fetch show a recoverable
error state instead of an infinite loading spinner.

## Decisions (brainstormed & approved)

1. **Picker UX** — reuse `ExistingIssuesListModal` (multi-select modal), triggered by an
   **Add work items** button. No new picker component.
2. **Scope** — picker swap **plus** the fetch-failure loading fix. Nothing else.
3. **Project-scoped** — the picker searches only the current project
   (`workspace_search: false`). The Rust backend rejects an `issue_id` that does not belong
   to the same project (`service.rs` `issues_create`), so workspace-level search is not
   offered.
4. **Already-linked issues are hidden** in the picker via `shouldHideIssue`, in addition to
   being preselected via `selectedWorkItemIds`.
5. **Bulk link** — one submit links every selected issue; the backend `service-issues/`
   POST is idempotent, so re-linking is safe.
6. **Backend unchanged** — the Rust API already implements `service-issues/`
   list/create/delete with the `identifier`/`name` join. No gap on that side.

## Components

### `apps/web/core/components/services/detail/work-items.tsx`

- Remove the `<Input name="service-work-item-id">` + **Add** row and the `issueId` state.
- Add a single **Add work items** button that opens `ExistingIssuesListModal` with:
  - `workspaceSlug`, `projectId={pid}`;
  - `searchParams={{ workspace_search: false }}`;
  - `selectedWorkItemIds` = issue ids of the current service's links;
  - `shouldHideIssue={(issue) => linkedSet.has(issue.id)}`;
  - `handleOnSubmit` mapping each `ISearchIssueResponse` to
    `{ id, identifier: `${project__identifier}-${sequence_id}`, name }` and calling the new
    store `linkWorkItems`.
- Keep the existing linked list (identifier, name, remove button) and `handleUnlink`.
- One `isLinking`/submitting state covers the whole batch; a single error toast on failure.

### `ExistingIssuesListModal` (`core/modals/existing-issues-list-modal.tsx`)

Reused as-is. Its own search/select/submit UI and `issue.select.*` i18n keys are unchanged.

## Data flow

```
Add work items button
  └─ ExistingIssuesListModal (projectIssuesSearch → ISearchIssueResponse[])
       └─ handleOnSubmit(selected[])
            └─ ServicesStore.linkWorkItems(slug, wsId, pid, serviceId, issues[])
                 └─ ServiceService.linkWorkItems(...)
                      └─ Promise.all(linkWorkItem) → POST service-issues/
                           └─ set workItemLinkMap[link.id] = link   (MobX reactive)
```

- `ServiceService.linkWorkItems` maps over the existing `linkWorkItem` POST
  (`POST .../service-issues/`, `{ service_id, issue_id }`), `Promise.all`s the calls, and
  returns the created links.
- `ServicesStore.linkWorkItems` is an `action` that awaits the service call and writes each
  returned link into `workItemLinkMap`. Existing `linkWorkItem` / `unlinkWorkItem` stay for
  the issue-detail `ServiceSelect`.
- `IServiceStore` gains the `linkWorkItems` signature.

## Error handling (fetch loading fix)

The current `fetchServices` catch swallows the error, leaves `fetchedMap[projectId]` unset
and `loader = false`; `getProjectServiceIds` then returns `null` forever, so the list and
detail render `common.loading` permanently.

- Add `errorMap: Record<string, boolean>` (observable) to `ServicesStore`.
- `fetchServices`: clear `errorMap[projectId]` at the start; on failure set
  `errorMap[projectId] = true` and `loader = false`, and leave `fetchedMap` untouched so a
  retry can re-fetch.
- Add a `retry`/`clearError` path — Retry simply calls `fetchServices` again (the existing
  list/detail `useEffect` already re-fetches when `hasFetched` is falsy).
- `ServicesListView`: when `errorMap[projectId]` is set, render an error state
  (title + description + **Retry** button) instead of the loading branch.
- `ServiceDetailRoot`: when `errorMap[pid]` is set and the service is still absent, render the
  same error state + Retry instead of `common.loading`.
- Health fetch stays supplementary; its failure is still swallowed and never blocks the page.

## i18n (`packages/i18n/src/locales/*/service.json`)

- Add `service.detail.add_work_items`, `service.detail.load_error_title`,
  `service.detail.load_error_description`, and `service.detail.retry`.
- Remove the now-dead `service.detail.issue_id_placeholder` from all locales.
- All locale edits go through the `translate` skill (do-not-translate terms, placeholders
  preserved).

## Testing & verification

- `apps/web` has no configured test runner (unchanged; adding one is out of scope). Pure
  logic added here is a thin loop, not unit-testable in isolation.
- No backend change; the Rust `service.rs` tests already cover the link endpoints.
- Static checks: `pnpm --filter=web check:types`, `pnpm --filter=web check:lint`,
  `pnpm fix:format` (or `pnpm fix`).
- Manual checklist against the dev server:
  1. Open a service → Work items → **Add work items** opens the picker.
  2. Selecting one/multiple issues links them; the list shows identifier + name.
  3. Re-opening the picker hides the already-linked issues.
  4. Unlink removes a link.
  5. With the API unreachable, the services list and service detail show the error state and
     **Retry** recovers once the API is back (no infinite spinner).

## Out of scope (YAGNI)

- No backend, migration, or API change.
- No workspace-level issue search; no cross-project linking.
- No change to `ServiceSelect` on the issue-detail sidebar, the dependencies tab, the create/
  edit form, or the delete flow.
- No new frontend test runner or E2E automation.
- No change to the mocked service health model.
