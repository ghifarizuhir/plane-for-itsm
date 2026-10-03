# Service Peek Overview — Design

Date: 2026-10-03
Status: Approved (pending user review of this spec)
Scope: Frontend-only (`apps/web`). Backend and API unchanged.

## Goal

Clicking a service in the Services list (board) or graph must open a **peek overview** panel over the current view — the same behavior as clicking a work item — instead of navigating to the detail page. It applies to both the board and graph views of services.

Full parity with the work-item peek: side-peek by default, modal and full-screen modes, close via button / ESC / outside click. The existing detail route (`/services/:serviceId`) stays for direct URLs and "open full screen". The URL is not modified while peeking (pure in-memory state, matching `apps/web` work-item behavior). On mobile, clicking a service navigates to the detail page.

## Decisions (brainstormed & approved)

1. **Parity** — same modes and dismissal behavior as `IssuePeekOverview`; mobile navigates to the full page.
2. **Content** — the full service detail: title, description, work items, and the sidebar (health, status, criticality, type, owner, URLs, dependencies).
3. **URL** — in-memory only; no query param, no history entry. Back does not close the peek.
4. **Detail page** — kept for direct URLs and "open full screen".
5. **Architecture** — store-based global peek in `ServicesStore`, mirroring `IssuePeekOverview` (observable + action + root-rendered overlay).

## Architecture

### State (`apps/web/core/store/service.store.ts`)

- `type TServicePeek = { workspaceSlug: string; projectId: string; serviceId: string }`
- `peekService: TServicePeek | undefined` — `observable.ref`
- `setPeekService(peek?: TServicePeek)` — `action`
- `getIsServicePeeked = computedFn((serviceId) => this.peekService?.serviceId === serviceId)`
- `deleteService`: after removing the service from the maps, clear `peekService` when it targets the deleted service.

### Triggers

New hook `apps/web/core/hooks/use-service-peek-overview-redirection.ts`, mirroring `use-issue-peek-overview-redirection.tsx`:

- Signature: `(workspaceSlug, service, isMobile)` → `{ handleRedirection }`.
- Builds `/{workspaceSlug}/projects/{service.project_id}/services/{service.id}`.
- Mobile → `router.push(link)`; desktop → `setPeekService` (skip when already peeked).

Call sites:

- `services/board/services-board-row.tsx` — replace the `next/link` row `<Link>` with `ControlLink` from `@plane/ui`: plain left click peeks, Cmd/Ctrl+click opens the detail route in a new tab. Add `id={`service-${service.id}`}` for the outside-click exemption.
- `services/graph/service-graph.tsx` — `handleNodeClick` calls the hook instead of `router.push`.

### Render

- New `ServicePeekOverview` is rendered in `services/services-list-view.tsx` (present for both board and graph) and portaled into `#full-screen-portal` (declared in `app/(all)/[workspaceSlug]/(projects)/layout.tsx`) when the element exists; otherwise rendered inline.
- The root observes `peekService`; it renders `null` when absent and clears the state from an effect when the peeked `workspaceSlug`/`projectId` no longer matches the route params (no stale panel after switching project/workspace).

### Components

New folder `apps/web/core/components/services/peek-overview/`:

| File         | Responsibility                                                                                                                                                  |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `root.tsx`   | `ServicePeekOverview`: route guard, data ensure (`fetchServices` when `fetchedMap[projectId]` is unset), loading / error / not-found states, renders `view.tsx` |
| `view.tsx`   | Peek mode state, outside-click + ESC handling, portal, panel chrome, content composition                                                                        |
| `header.tsx` | Close, open full page, mode dropdown (Side peek / Modal / Full screen), `ServiceDetailQuickActions`                                                             |
| `index.ts`   | Barrel export                                                                                                                                                   |

Reused detail sections: `ServiceTitleInput`, `ServiceDescription`, `ServiceWorkItems`, `ServiceDetailSidebar`.

Panel layout (mirrors the issue peek):

- **side-peek** (default): `absolute z-[25] top-0 right-0 bottom-0 w-full border-l md:w-[50%]`; one scroll column — title → description → work items → sidebar stacked below.
- **modal**: `top/left-[8.33%] size-5/6`; same single column.
- **full-screen**: `absolute inset-0 m-4`; two columns, left main content, right `md:!w-[400px] md:border-l` sidebar.

Supporting changes:

- `services/detail/sidebar.tsx` — add optional `layout?: "sidebar" | "stacked"` (default `"sidebar"`). In `"stacked"` mode the `md:h-full md:overflow-y-auto` is dropped so the side-peek column scrolls as one, without changing the detail page.
- `hooks/use-peek-overview-outside-click.tsx` — add an optional `targetElementId` parameter; when provided it replaces the hardcoded `issue-{id}` element id comparison. Existing issue callers keep the current behavior.
- `services/index.ts` — export `ServicePeekOverview`.

i18n: reuse `common.side_peek`, `common.modal`, `common.full_screen`, `common.close_peek_view`. No new keys.

### Close behavior

- Close button, ESC (`useKeypress`), outside click (`usePeekOverviewOutsideClickDetector` with `targetElementId = service-{id}`).
- Close is suppressed while a dialog is open (`[role="dialog"]`, which covers `CreateUpdateServiceModal`, `DeleteServiceModal`, and `ExistingIssuesListModal`), so interacting with those modals does not dismiss the peek.
- ESC returns focus to `#service-{id}` when that element exists (best effort, mirroring work items).
- Deleting the peeked service clears `peekService` in the store; `DeleteServiceModal`'s existing redirect to the list route is harmless because the peek is only opened from that route.

## Edge cases & error handling

- **Data loading** — `fetchServices` runs when `fetchedMap[projectId]` is unset; loading state until the service is available.
- **Fetch error** — `ServiceLoadErrorState` with retry inside the panel body; header stays so the panel can be closed.
- **Service not found** — `service.detail.not_found_*` copy with a close action.
- **Project/workspace switch** — the root effect clears `peekService`; no stale reopen when returning.
- **Same trigger click while open** — the `service-{id}` exemption prevents close-and-reopen flicker on the originating row / node.
- **Cmd/Ctrl+click** — board row opens the detail page in a new tab; graph nodes only peek (no anchor).

## Testing & verification

- New unit test `apps/web/core/store/service.store.test.ts` (pattern and mocks mirror `war-room.store.test.ts`):
  - `setPeekService` sets and clears the observable; `getIsServicePeeked` returns true/false accordingly.
  - `deleteService` clears the peek when it targets the peeked service and leaves it untouched otherwise.
- Manual verification: board row click → peek; Cmd/Ctrl+click → new tab; graph node click → peek; mode switch; ESC / X / outside click; peek stays open while edit / delete / work-item-link modals are open; mobile viewport → navigates to the detail page; delete while peeked → closes; project switch → no stale panel; Re-layout and node drag unaffected.
- Commands: `pnpm --filter=web check:lint`, `pnpm --filter=web check:types`, `pnpm --filter=web test`. Per `AGENTS.md`, rebuild (`pnpm --filter=web build`) and restart `plane-web-prod` for the tunnel.

## Out of scope

- War-room service map (keeps `window.open` behavior).
- URL/deep-link peek (`?peekService=`).
- Backend/API changes.
- Removing or restructuring the detail route.

## Files touched

- `apps/web/core/store/service.store.ts` — peek state, actions, delete cleanup.
- `apps/web/core/hooks/use-service-peek-overview-redirection.ts` — new.
- `apps/web/core/hooks/use-peek-overview-outside-click.tsx` — optional `targetElementId`.
- `apps/web/core/components/services/peek-overview/{root,view,header,index}.tsx` — new.
- `apps/web/core/components/services/services-list-view.tsx` — render `ServicePeekOverview`.
- `apps/web/core/components/services/board/services-board-row.tsx` — `ControlLink` trigger.
- `apps/web/core/components/services/graph/service-graph.tsx` — node click peeks.
- `apps/web/core/components/services/detail/sidebar.tsx` — `layout` prop.
- `apps/web/core/components/services/index.ts` — export.
- `apps/web/core/store/service.store.test.ts` — new tests.
