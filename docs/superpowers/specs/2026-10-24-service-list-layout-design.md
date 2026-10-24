# Service Catalog: Board → List Layout — Design

Date: 2026-10-24
Status: Approved (brainstormed & reviewed with user)
Scope: apps/web service catalog only. No backend, DB, or API changes.

## Goal

Replace the service catalog `board` layout with a `list` layout modeled on work items list conventions: grouped rows, rich columns, and collapse/expand — while keeping the existing peek interaction and health/incident data flow untouched.

## Non-goals

- Work-items parity beyond grouping (no bulk-select, no quick actions, no inline property editing, no sub-item nesting).
- Changes to the graph layout or the war-room service map.
- Backend or schema changes; grouping is computed client-side from already-fetched data.
- Persisting group collapse state.

## Decisions (brainstormed & approved)

1. **Replace, don't add.** The `board` layout is removed; layouts are `list` + `graph`.
2. **Rename the layout key.** `TServiceLayoutOptions` becomes `"list" | "graph"` (approach A over keeping a stale `board` key or carrying a deprecated layout).
3. **Group by status/criticality/health.** `group_by: "status" | "criticality" | "health" | null`, default `"status"`.
4. **Columns = board + Status.** Name, Status, Criticality, Health, Incidents, Last deploy, Owner.
5. **Filter first, then group.** Groups are computed from the filtered service list, matching work items behavior.

## Layout model & migration

- `TServiceLayoutOptions` in `packages/types/src/service/filters.ts`: `"board" | "graph"` → `"list" | "graph"`.
- `SERVICE_VIEW_LAYOUTS` in `service-layout-icon.tsx`: List entry (row icon) + Graph entry. All `layout === "board"` checks and `displayFilters.layout` reads updated.
- `ServiceFilterStore.initProjectServiceFilters`: legacy persisted values (`board`, `list`, `grid`, `undefined`) collapse to `"list"`; `"graph"` stays `"graph"`. This mirrors the existing `list/grid → board` migration precedent already in the store.
- Default `order_by` remains `"health"`.
- New display-filter field `group_by` with default `"status"`. Persisted in the same localStorage bucket as other display filters.

## Grouping

- Groups are derived from the filtered service ids (never from the unfiltered set).
- Group dimensions and order:
  - `status`: active, maintenance, planned, deprecated, retired (lifecycle order).
  - `criticality`: critical, high, medium, low.
  - `health`: down, degraded, healthy, unknown (reuse existing `HEALTH_WEIGHT` ordering).
  - `null`: flat list, no group headers.
- Health for grouping reads `healthMap` (same source as filters and the board today); missing snapshots count as `unknown`.
- Group header shows the dimension label + service count and a collapse chevron. Collapse state is component-local, default expanded; empty groups are never rendered.
- A filter equal to the group dimension (e.g. Status = active while grouping by status) yields a single group. This is correct behavior.

## List UI

- New components `ServicesList` / `ServicesListRow` replace `ServicesBoard` / `ServicesBoardRow`; old files are deleted.
- Columns (with existing responsive hiding): Service (name + owner-app chip + health dot) | Status (new pill) | Criticality (new pill) | Health (existing pill) | Incidents (existing cell + tooltip) | Last deploy (existing cell) | Owner (existing avatar).
- Status/Criticality pills reuse the codebase's badge/pill tokens and colors.
- The left health rail is removed; the health dot next to the name is kept.
- Row click opens the peek (unchanged), keeping `id={`service-${id}`}` for the outside-click detector.
- Within-group row order follows the active `order_by`; groups follow the dimension order above.
- Empty states unchanged: no services → title + description + Add CTA; no matches → clear-filters CTA.

## i18n

- Add: `service.layout.list` ("List"), `service.list.group_by.*` (status/criticality/health/none labels), group collapse/expand accessibility labels. `service.status_values.*` and `service.criticality_values.*` already exist and are reused.
- Remove: `service.layout.board` from every locale.
- Use the `translate` skill when implementing.

## Testing

- Unit tests for the grouping helper: dimension grouping, group order per dimension, filter-before-group, health-missing → unknown, empty input.
- `pnpm turbo run check:types check:lint --filter=web`.
- Manual smoke: switch list/graph, change `group_by`, collapse/expand, combine filters with grouping, resize to small breakpoints, open peek from a row.

## Rollout

1. Web only: no DB migration, no api-rs rebuild.
2. `pnpm --filter=web build` + `systemctl --user restart plane-web-prod.service`; verify `curl localhost:3000` → 200.
3. Old `board` localStorage values migrate to `list` on store init; no user action needed.

## Edge cases

- Service with `healthMap` miss groups under `unknown` when grouping by health.
- `planned`/`retired` services keep real incident tooltips; grouping by status shows them under their own groups.
- Workspace "All applications" scoping and project scoping reuse the existing id selectors; grouping applies on top.
- Mobile: group headers remain; columns hide per existing breakpoints.

## Changelog

| Date       | Change                                                 |
| ---------- | ------------------------------------------------------ |
| 2026-10-24 | Initial design: board replaced by grouped list layout. |
