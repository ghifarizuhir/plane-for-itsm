# Services List — Health-First Ops Board Redesign

Date: 2026-09-17
Status: Approved (pending user review of this spec)
Scope: Frontend-only, project Services **list page** (`apps/web/core/components/services`,
`apps/web/core/store`, `apps/web/core/services`, `packages/types/src/service`, `packages/i18n`).
No backend, no detail-page, no form changes.

## Goal

Turn the project Services list page from a generic catalog list into a **health-first
operations board**. A user lands on the page and immediately sees which services are down or
degraded, how many active incidents they carry, and how recently they changed — then drills
into detail from there.

## Decisions (brainstormed & approved)

1. **Scope** — the list page only: toolbar/header, view modes, filters, ordering, empty
   states, and the dependency graph's node rendering. The service detail page, create/edit
   form, work-item linking, and graph interaction model are unchanged.
2. **Primary job** — service health / ops status at a glance.
3. **Health data** — mocked in the frontend service layer with **deterministic, seeded values
   per service**. No DB migration, no API change. The mock sits behind a swappable service
   class so a future backend endpoint drops in without touching stores or components.
4. **Signals shown on the board** — overall health state, active incidents (count + highest
   severity), last deploy recency.
5. **View modes** — two: **Board** (default, replaces the old list + grid) and **Graph**.
   The `grid` layout is removed.
6. **Board layout** — Option A, "status rail rows": one dense list; a colored left rail
   encodes health; health, incidents, deploy recency and owner align in columns.
7. **Board behavior** — health-first default sort (down → degraded → unknown → healthy, then
   criticality, then name); new **Health** and **Has active incidents** filters; existing
   status/criticality/type filters and name/created/updated/criticality/status ordering stay
   available.
8. **Graph** — nodes become health-colored (border/ring + incident badge), consistent with
   the board. No impact-highlighting in this iteration.
9. **Aesthetic** — "refined ops console": Plane semantic tokens, tighter density, `font-code`
   (`IBM Plex Mono`) for numeric columns, a status rail plus one status color per row. Must
   read correctly in both light and dark themes.

Reference mockup (approved): `.superpowers/brainstorm/6213-1789605300/content/health-board-refined.html`.

## Health model (frontend mock)

The backend `services` table has no health data, so health is derived on the client and is
**not persisted**. Derivation is deterministic from `service.id`, so values are stable across
reloads, list order, and filters.

### Types

Added to `packages/types/src/service/core.ts` and exported via the service barrel:

```ts
export type TServiceHealth = "healthy" | "degraded" | "down" | "unknown";
export type TServiceIncidentSeverity = "sev1" | "sev2" | "sev3" | "sev4";

export interface IServiceIncident {
  id: string;
  service_id: string;
  severity: TServiceIncidentSeverity;
  opened_at: string; // ISO
}

export interface IServiceHealthSnapshot {
  service_id: string;
  health: TServiceHealth;
  incidents: IServiceIncident[]; // open only, for this iteration
  last_deployed_at: string | null; // ISO; null = never / not applicable
}
```

Incident `title` is deliberately omitted: the board only renders count + severity, so no
mock English strings leak into i18n.

### Derivation rules

New pure module `apps/web/core/services/service-health.helpers.ts`:

- `fnv1aHash(value: string): number` and `mulberry32(seed: number): () => number` — small,
  dependency-free seeded PRNG.
- `buildHealthSnapshot(service: IService): IServiceHealthSnapshot`:
  - `status === "planned" || status === "retired"` → `health: "unknown"`, `incidents: []`,
    `last_deployed_at: null`. (Lifecycle states with no runtime signal.)
  - Otherwise draw from the seeded PRNG and classify with criticality-weighted thresholds:
    `critical` → down 12% / degraded 34%; `high` → 10% / 30%; `medium` → 8% / 26%;
    `low` → 6% / 22%. Remainder is `healthy`.
  - Incidents, correlated with health: `down` → exactly 1 open `sev1`; `degraded` → 1–2 open
    from `{sev2, sev3}`; `healthy` → none.
  - `last_deployed_at` → `updated_at` minus a seeded 0–14 day offset. Never in the future.
  - The PRNG is seeded from `service.id` and consumed in a fixed order (health, then
    incidents, then deploy offset), so snapshots are stable across reloads and filter changes.
- `HEALTH_WEIGHT: Record<TServiceHealth, number> = { down: 0, degraded: 1, unknown: 2, healthy: 3 }`
  for ordering.
- `INCIDENT_SEVERITY_RANK: Record<TServiceIncidentSeverity, number> = { sev1: 0, sev2: 1, sev3: 2, sev4: 3 }`
  so "highest severity" means the lowest rank. The board renders the highest-severity open
  incident plus the open count.

### Service-layer seam

New `apps/web/core/services/service-health.service.ts`:

```ts
export class ServiceHealthService {
  async getHealth(
    workspaceSlug: string,
    workspaceId: string,
    projectId: string,
    services: IService[]
  ): Promise<IServiceHealthSnapshot[]>;
}
```

For now it maps `buildHealthSnapshot` over the already-fetched `services` (no extra request).
The class exists so the future backend call (`GET .../services/health/`) is a one-file swap.

## Architecture & files

### Types

- `packages/types/src/service/core.ts` — health types above.
- `packages/types/src/service/filters.ts`:
  - `TServiceLayoutOptions = "board" | "graph"` (was `"list" | "grid" | "graph"`).
  - `TServiceOrderByOptions` — add `"health"` (new default).
  - `TServiceIncidentFilter = "active"`.
  - `TServiceFilters` — add `health?: TServiceHealth[]` and `incidents?: TServiceIncidentFilter[]`.
- `packages/types/src/service/index.ts` — export the new types.

### Store

`apps/web/core/store/service.store.ts`:

- Add observable `healthMap: Record<string, IServiceHealthSnapshot>`.
- Instantiate `ServiceHealthService` in the constructor.
- `fetchServices` — after services resolve, call `getHealth(...)`, then
  `set(this.healthMap, [snapshot.service_id], snapshot)` for each. Health fetch failure is
  non-fatal: log and leave health `null` (board falls back to `unknown` styling).
- Add `getServiceHealth = computedFn((serviceId) => this.healthMap[serviceId] ?? null)`.
- Add `getProjectHealthSummary = computedFn((projectId) => …)` returning
  `{ down, degraded, healthy, unknown, criticalImpacted }`, computed over **all** services in
  the project (not the filtered subset), where `criticalImpacted` counts services with
  `criticality === "critical"` and `health` in `{down, degraded}`.
- `getFilteredServiceIds` — pass `healthMap` into `filterServices` / `orderServices`, default
  `order_by` `"health"`.
- `getGraphData` — unchanged (nodes read health from the store by id).
- `healthMap` needs no explicit reset: `root.store.ts` `resetOnSignOut` re-instantiates
  `ServicesStore`.

`apps/web/core/store/service_filter.store.ts`:

- `initProjectServiceFilters` — coerce legacy persisted layouts: `list`/`grid` → `board`;
  anything not `board|graph` → `board`. Default `order_by` becomes `"health"`.
- `updateFilters` / `clearAllFilters` unchanged (array-valued keys keep working).

### Helpers

`apps/web/core/services/service.helpers.ts`:

- `matchesFilters(service, filters, health)` — add `health` and `incidents` predicates
  (`incidents` includes `"active"` ⇒ keep services with ≥1 open incident).
- `filterServices(services, filters, searchQuery, healthMap)` — thread health through.
- `orderServices(services, orderBy, healthMap)` — add the `"health"` case using
  `HEALTH_WEIGHT`, tie-broken by criticality then name. Existing cases unchanged.

### Components

New under `apps/web/core/components/services/`:

- `health/service-health-dot.tsx` — colored dot per `TServiceHealth`.
- `health/service-health-pill.tsx` — pill with localized health label.
- `health/service-health-summary.tsx` — summary strip: `N down · N degraded · N healthy`
  plus a `N critical impacted` chip.
- `health/service-incident-cell.tsx` — highest severity + count, or a muted `—`.
- `health/service-deploy-cell.tsx` — relative time via `calculateTimeAgo` (`@plane/utils`).
- `board/services-board.tsx` — column header + rows; consumes filtered ids + health summary.
- `board/services-board-row.tsx` — status rail, health dot + name + criticality chip, health
  pill, incidents, deploy, owner avatars, chevron; row links to service detail.
- `filters/health.tsx`, `filters/incidents.tsx` — mirror `filters/status.tsx` structure using
  `FilterHeader` / `FilterOption`.

Removed:

- `service-list-item.tsx`, `service-card-item.tsx` (and their barrel exports).

Changed:

- `services-list-view.tsx` — render **Board** or **Graph**; keep loading / empty-catalog /
  no-matches states; surface `ServiceHealthSummary` above the board. The no-matches state
  gains a **Clear filters** action (the empty-catalog state keeps **Add service**).
- `service-view-header.tsx` — layout toggle now Board | Graph.
- `service-layout-icon.tsx` — `SERVICE_VIEW_LAYOUTS` becomes `board` (`ListOutline`) +
  `graph` (`WorkgraphOutline`); `grid` removed.
- `service-mobile-header.tsx` — Board | Graph.
- `filters/root.tsx` — mount `FilterServiceHealth` and `FilterServiceIncidents`.
- `applied-filters/root.tsx` — extend `SERVICE_FILTER_VALUE_I18N` with `health` and
  `incidents` value-key mappings (and `service.fields.health` / `service.fields.incidents`).
- `dropdowns/order-by.tsx` — add the `health` option and list it first.
- `graph/service-node.tsx` — health-colored border/ring + incident count badge; keep
  status/criticality secondary text.
- `graph/service-graph.tsx` — inject `health` into each node's `data` (alongside the existing
  translated labels) so nodes re-render on health changes.
- `index.ts` — barrel updates.

### i18n

New keys under `packages/i18n/src/locales/en/service.json` (namespace `service`):

- `fields.health`, `fields.incidents`, `fields.deploy`
- `health_values.{healthy,degraded,down,unknown}`
- `incident_severity.{sev1,sev2,sev3,sev4}` and `incidents.active` (filter label)
- `order_by.health`
- `layout.board` (replaces `layout.list` / `layout.grid`, which are removed)
- `board.service`, `board.health`, `board.incidents`, `board.deploy`, `board.owner`
  (column headers), and summary-strip labels such as `summary.down`, `summary.degraded`,
  `summary.healthy`, `summary.critical_impacted`, `summary.none`
- `empty_state.*` — reuse existing keys; only adjust copy if needed.

Every new/changed string is authored in `en` and propagated to the other locales by following
the **translate** skill (do-not-translate terms, plural forms, placeholder preservation).

## Data flow

```
fetchServices(slug, wsId, projectId)
  ├─ GET services            → serviceMap
  ├─ GET service-dependencies→ dependencyMap
  ├─ GET service-issues      → workItemLinkMap
  └─ ServiceHealthService.getHealth(services)   [mock, in-memory]
        └─ buildHealthSnapshot(service) per service (seeded by id)
             → healthMap

getFilteredServiceIds(projectId)
  serviceMap ─ filterServices(filters + search + healthMap) ─ orderServices(order_by="health", healthMap)
    → ids

getProjectHealthSummary(projectId) → counts  ──▶ ServiceHealthSummary
getServiceHealth(id)               → snapshot ──▶ Board row / Graph node
```

Health is derived once per `fetchServices` and stored; it is not recomputed per render and
not persisted.

## Error handling

- **Health fetch fails** — non-fatal. The board renders with the `unknown` presentation for
  services lacking a snapshot; log the error, no toast (health is supplementary).
- **Service fetch fails** — unchanged store behavior (loader cleared, list stays empty).
- **Filter/ordering with a missing snapshot** — `matchesFilters` treats missing health as
  `unknown`; `orderServices` sorts `undefined` with `unknown`.
- **Relative time** — `calculateTimeAgo(null)` renders an em dash; `last_deployed_at: null`
  shows `—` rather than "NaN".
- **Graph delete/connect** — untouched; existing toasts remain.

## Testing & verification

`apps/web` has no configured test runner, so verification is:

1. `pnpm --filter=web check:types` (runs `react-router typegen` first).
2. `pnpm --filter=web check:lint` and `check:format` (or `pnpm fix`).
3. `pnpm --filter=web build` then `systemctl --user restart plane-web-prod.service`
   (per `AGENTS.md` — prod serves the tunnel; dev/prod must not share port 3000).
4. Manual checklist:
   - Board is the default view; the old grid is gone; Graph still works.
   - Rails/pills/dots use the correct health color in light and dark.
   - Health-first sort is the default; other order-by options still work.
   - Health and "has active incidents" filters apply, appear in the applied-filters bar with
     readable labels, and clear correctly.
   - Summary strip counts are computed over **all** services in the project (a fleet overview),
     independent of active filters, and match the seeded health values.
   - Deploy recency renders as relative time; missing values show `—`.
   - Empty catalog and no-matches states still render.
   - Legacy persisted layout (`list`/`grid`) opens the Board without a broken state.
   - Graph nodes reflect health and show incident badges; drag/connect/delete still work.

Pure logic (`buildHealthSnapshot` determinism, filter/order predicates) is isolated so it can
be unit-tested if a runner is added later; adding one is out of scope.

## Out of scope (YAGNI)

- No backend migration, API fields, or endpoint for health.
- No detail-page redesign, no change to the create/edit form or delete flow.
- No incident list/detail UI (counts + severity only).
- No SLO, error-budget, latency, uptime, or trend graphs.
- No graph "impact highlighting" or blast-radius mode.
- No saved views, favorites, bulk actions, archive, or real-time health updates.
- No new frontend test runner or E2E automation.
- No project-settings toggle for the feature.
