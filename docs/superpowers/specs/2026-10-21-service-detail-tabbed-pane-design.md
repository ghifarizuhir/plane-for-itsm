# Service Detail Tabbed Pane — Design

Date: 2026-10-21
Status: Approved (pending user review of this spec)
Scope: Frontend-only (`apps/web`). Backend, API, and data model unchanged.

## Goal

Restructure the Services detail page to match the Knowledge Base detail pattern:

- The main column is dedicated to the service title and description.
- The right side is a collapsible pane with three tabs: **Properties**, **Relationships**, and
  **Tickets**.

The pane behavior mirrors the KB navigation pane: query-param state, collapse/expand animation, and
a close control inside the pane. The existing service peek overview is intentionally left unchanged.

## Current State

- `apps/web/core/components/services/detail/root.tsx` renders a two-column layout. The main column
  contains title, description, project links, relationships, and tickets. The right column is the
  static `ServiceDetailSidebar` with properties only.
- `apps/web/core/components/pages/navigation-pane/root.tsx` implements the KB pane: a 294px aside
  with `outline`, `info`, and `assets` tabs. Open/closed state and the active tab come from the
  `paneTab` query param. Closing removes the param; opening defaults to `outline`.
- There is no shared tabbed pane abstraction between KB and Services.

## Decisions (brainstormed & approved)

1. **Layout** — main column becomes title + description only. Project links, relationships, and
   tickets move out of the main column.
2. **Pane content** — three tabs: `properties`, `relationships`, `tickets`.
3. **Project links** — move into the bottom of the **Properties** tab, separated by a divider.
4. **State** — stored in the URL query param `paneTab`, matching KB. No param (or an invalid value)
   means the pane is closed. Opening defaults to `properties`. Tab changes push a new history entry.
5. **Width** — 320px on desktop (`md:w-80`). KB keeps its 294px width.
6. **Open control** — an icon button in the services header, right side, shown only on the detail
   route while the pane is closed. Close control lives inside the pane.
7. **Responsive** — below `md`, the open pane renders as a full-width section below the description
   instead of a side column.
8. **Peek overview** — unchanged. It keeps the current stacked layout through the default component
   props.
9. **Architecture** — a service-specific pane component modeled on the KB pane. Do not generalize the
   KB pane; it is tightly coupled to page instances, editor versions, and asset state.

## URL Contract

Param name: `paneTab` (same name as KB, but scoped to the service detail route).

| URL state               | Result                       |
| ----------------------- | ---------------------------- |
| no `paneTab`            | Pane closed                  |
| `paneTab=properties`    | Pane open, Properties tab    |
| `paneTab=relationships` | Pane open, Relationships tab |
| `paneTab=tickets`       | Pane open, Tickets tab       |
| any other value         | Pane closed                  |

- Open: add `paneTab=properties` while preserving unrelated query params.
- Change tab: replace `paneTab` with the selected key; preserve unrelated params.
- Close: remove `paneTab`; preserve unrelated params.
- Use the existing `useQueryParams` helper and `router.push` so browser back/forward and refresh
  restore the pane state.
- The invalid-value fallback is closed, matching KB's `PAGE_NAVIGATION_PANE_TAB_KEYS` check.

## Layout

### Desktop (`md` and up)

```
+------------------------------------------+----------------------+
| Title (inline editable)                  | Close                |
| Description (rich text)                  | [Props][Rel][Tickets]|
|                                          |                      |
| scrollable                               | scrollable           |
+------------------------------------------+----------------------+
```

- Root: `flex h-full w-full flex-col overflow-y-auto md:flex-row md:overflow-hidden`.
- Main column: `w-full ... md:h-full md:min-w-0 md:flex-1 md:overflow-y-auto`. Retain the current
  `px-9 py-5 space-y-6` spacing.
- Pane: `md:h-full md:w-80 md:shrink-0 border-l border-subtle bg-surface-1`.
- Collapse animation mirrors KB: animate `width` and `marginRight` with
  `transition-all duration-300 ease-out`; when closed use `marginRight: -320px`.
- Pane content scrolls independently on desktop.

### Mobile (below `md`)

- The page scrolls as one column.
- When open, the pane renders full-width below the description, with a top border instead of a left
  border.
- When closed, the pane is not rendered.

### Header Control

- Add `Header.RightItem` to `WorkspaceServicesBreadcrumbs` (or a sibling header-actions component)
  in `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx`.
- Render only when `serviceId` is present and the pane is closed.
- Icon: `RightSidePaneOutline` from `@makeplane/propel/icons`.
- Tooltip/aria label: reuse `page_navigation_pane.open_button`.
- The pane close button uses `ArrowRightCircle` and reuses `page_navigation_pane.close_button`.

## Components

### New Files

| File                                                                | Responsibility                                                     |
| ------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `core/components/services/detail/pane/index.ts`                     | Barrel exports                                                     |
| `core/components/services/detail/pane/helpers.ts`                   | Pane width, query-param constant, tab metadata, validation helpers |
| `core/components/services/detail/pane/helpers.test.ts`              | Unit tests for tab validation and default-open behavior            |
| `core/components/services/detail/pane/root.tsx`                     | Aside chrome, close button, `Tabs`, open/close animation           |
| `core/components/services/detail/pane/tabs-list.tsx`                | Three `Tabs.Trigger` items with existing i18n labels               |
| `core/components/services/detail/pane/tab-panels/root.tsx`          | `Tabs.Content` switch                                              |
| `core/components/services/detail/pane/tab-panels/properties.tsx`    | `ServiceDetailSidebar` + project links section                     |
| `core/components/services/detail/pane/tab-panels/relationships.tsx` | `ServiceRelationships` with pane layout                            |
| `core/components/services/detail/pane/tab-panels/tickets.tsx`       | `ServiceWorkItems` with pane layout                                |
| `core/hooks/services/use-service-detail-pane.ts`                    | Read `paneTab`, validate, open/tab-change/close handlers, `isOpen` |

### Modified Files

| File                                                                | Change                                                                                        |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `core/components/services/detail/root.tsx`                          | Keep title + description in the main column; render the pane; remove the three moved sections |
| `core/components/services/detail/sidebar.tsx`                       | Add `layout="pane"` support: no internal heading, no horizontal padding, no own scroll        |
| `core/components/services/detail/relationships.tsx`                 | Add `layout?: "page" \| "pane"`; pane layout stacks the add form and uses two-line rows       |
| `core/components/services/detail/work-items.tsx`                    | Add `layout?: "page" \| "pane"`; pane layout hides the redundant heading and compacts rows    |
| `apps/web/app/(all)/[workspaceSlug]/(projects)/services/layout.tsx` | Add the right-side open button, only on the detail route                                      |
| `core/components/services/detail/index.ts`                          | Export the pane barrel                                                                        |

No i18n keys are added. Tab labels reuse:

- Properties: `common.properties`
- Relationships: `service.relationships.title`
- Tickets: `service.tabs.work_items` (already "Tickets")

### Pane Root

- Uses `Tabs` from `@plane/propel/tabs`, matching KB.
- Reads the active tab from `useServiceDetailPane`.
- Renders `null`/nothing when closed on mobile; on desktop keeps the aside mounted with negative
  margin so the collapse animates.
- Close button sits at the top, same placement as KB.
- Tabs list sits below the close button; active tab uses the Propel indicator.

### Properties Tab

- Renders `ServiceDetailSidebar` with `layout="pane"` so the internal "Properties" heading and
  `px-6` padding are dropped (the tab label provides the heading).
- Appends `ServiceProjectLinks` below a `border-t border-subtle` divider.
- The sidebar keeps all existing controls: health, last deploy, lifecycle status, business
  criticality, service type, owner, repository URL, documentation URL.
- Project links keep their current add/remove/transfer behavior and permission checks.

### Relationships Tab

- Wrapper renders `ServiceRelationships` with `layout="pane"`.
- In pane layout:
  - The section heading is omitted (tab label is enough).
  - Add form stacks vertically: type select, service select, full-width Add button.
  - Each relationship row becomes two lines:
    - Line 1: service name, lite lock, status badge, criticality badge, project identifier.
    - Line 2: inline type selector and Remove.
  - Group labels and the empty state remain.
- All store actions, validation, and toasts are unchanged.

### Tickets Tab

- Wrapper renders `ServiceWorkItems` with `layout="pane"`.
- In pane layout:
  - The section heading is omitted.
  - The "Link tickets" button sits at the top, right-aligned or full-width.
  - Rows keep identifier, title, and Remove with compact padding.
- `ExistingIssuesListModal` behavior is unchanged.

## Non-Goals

- No changes to the service peek overview (`side-peek`, `modal`, `full-screen`).
- No changes to the KB navigation pane or its tab registry.
- No backend, API, store, or data-model changes.
- No new i18n keys.
- No new pane tabs beyond the three approved.

## Edge Cases

- `paneTab` has an unknown value: pane is closed, no crash.
- Navigating to another service through a relationship link keeps the current pane tab because the
  query param persists; this is acceptable and consistent with KB.
- Closing the pane on mobile unmounts the pane section; desktop animates the margin.
- Pane content taller than the viewport scrolls inside the pane on desktop.
- The existing unsaved-changes guard for title/description is untouched.
- If `serviceId` is absent (services list route), the header open button is not rendered.
- Permissions for project links, relationship editing, and ticket linking remain enforced by the
  existing components.

## Verification

1. Unit test `pane/helpers.test.ts`:
   - valid tab keys resolve to themselves;
   - missing, empty, and unknown values resolve to closed;
   - opening defaults to `properties`.
2. `pnpm --filter=web test`
3. `pnpm check:types`
4. `pnpm check:lint`
5. Manual checks:
   - Open/close the pane from the header; URL updates and refresh preserves state.
   - Each tab renders and scrolls correctly.
   - Project links add/remove/transfer still work inside Properties.
   - Relationship add/type-change/remove still work inside the narrow pane.
   - Ticket link/unlink and the picker modal still work.
   - Mobile layout stacks the pane below the description.
   - Peek overview still renders the old stacked layout with no tabs.
