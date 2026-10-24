# Real Service Health & Active Incidents — Design

Date: 2026-10-24
Status: Approved (brainstormed & reviewed with user)
Scope: api-rs read endpoint + web service catalog health/incidents. Deployment postponed.

## Goal

Replace the deterministic client-side mock in `apps/web/core/services/service-health.helpers.ts` with real service health and incident data derived from Plane's existing ITSM records:

- Incident work items linked to a service (`service_issues`).
- War rooms linked to a service (`war_rooms`, `war_room_services`).

No new tables, migrations, or workers. Health is computed on read by api-rs and consumed by the existing `healthMap` flow.

## Non-goals

- Deployment tracking / real `last_deployed_at`. Postponed until a deployment feature (likely work-item based) exists.
- External monitoring (Prometheus, Alertmanager, uptime probes) as a direct health source.
- Dependency health propagation (a service is not marked degraded because its dependency is down).
- Realtime health over WebSocket; v1 uses polling + focus refetch.
- New incident pages/tabs; incident details stay in a tooltip on the catalog.

## Decisions (brainstormed & approved)

1. **Health source** — derived from active incidents in Plane, not external monitoring.
2. **Active incident** — union of active/monitoring war rooms and open Incident work items.
3. **Severity** — war room severity wins when present; otherwise priority mapping.
4. **Health rule** — any active incident ⇒ `degraded`; none ⇒ `healthy`; planned/retired ⇒ `unknown`. `down` is not produced in this phase.
5. **Propagation** — none; only incidents directly linked to the service (or explicitly linked via a war room).
6. **UI depth** — count + highest severity + tooltip listing active incidents. No new page.
7. **Last deploy** — fake timestamps removed; `last_deployed_at = null` and UI copy "No deployment".
8. **Approach** — read-time aggregation in api-rs (chosen over materialized snapshots and frontend computation).

## Semantics

### Active incident sources

An incident is active for a service when either:

1. It is an open Incident work item linked directly to the service:
   - `service_issues.deleted_at IS NULL`
   - `issues.deleted_at IS NULL`, `issues.archived_at IS NULL`
   - `lower(btrim(issue_types.name)) = 'incident'` and `issue_types.deleted_at IS NULL`
   - `states.group` is not `completed` or `cancelled` (missing/unknown state counts as active)
2. It is the primary issue of an active war room:
   - `war_rooms.status IN ('active', 'monitoring')`, `war_rooms.deleted_at IS NULL`
   - The service is linked to the room either through the issue's `service_issues` rows or through `war_room_services` (`deleted_at IS NULL`)

Resolved/archived war rooms and completed/cancelled issues are not active.

### Deduplication

Deduplicate by `(service_id, issue_id)`. A single issue can reach a service through both `service_issues` and `war_room_services`; it must appear once.

When both sources exist, the war room record wins for:

- `severity` (war room severity, e.g. `sev2`)
- `war_room_id`, `war_room_sequence_id`, `war_room_name`
- `opened_at` (war room `started_at`, fallback issue `created_at`)

Issue-only incidents use priority mapping via `severity_from_priority`:

| priority | severity |
| -------- | -------- |
| `urgent` | `sev1`   |
| `high`   | `sev2`   |
| `medium` | `sev3`   |
| `low`    | `sev4`   |
| `none`   | `sev4`   |

### Health

| Condition                             | Health     |
| ------------------------------------- | ---------- |
| Service status `planned` or `retired` | `unknown`  |
| At least one active incident          | `degraded` |
| No active incidents                   | `healthy`  |

Incidents are still returned for planned/retired services even though health is `unknown`.

`TServiceHealth` keeps `down` for a future monitoring source; this phase never emits it.

### Incident fields

```ts
interface IServiceIncident {
  id: string; // issue id
  service_id: string;
  severity: "sev1" | "sev2" | "sev3" | "sev4";
  opened_at: string; // war room started_at, else issue created_at
  identifier?: string; // e.g. PREPAID-123 (project.identifier + '-' + sequence_id)
  name?: string; // issue title
  war_room_id?: string | null;
  war_room_sequence_id?: number | null;
  war_room_name?: string | null;
}
```

Sort incidents by severity rank (`sev1` first), then `opened_at` descending. The catalog count uses the array length; the tooltip shows up to 5 and appends `+N more`.

### Last deploy

`last_deployed_at` is always `null` in this phase. The field stays in the snapshot for forward compatibility. UI copy: "No deployment".

## Backend API

### Endpoints

| Scope     | Route                                                             | Auth                  |
| --------- | ----------------------------------------------------------------- | --------------------- |
| Workspace | `GET /api/workspaces/:slug/services/health/`                      | session (`AuthUser`)  |
| Project   | `GET /api/workspaces/:slug/projects/:project_id/services/health/` | session + member gate |

Both return `200` with an array of `IServiceHealthSnapshot`-shaped objects, one per visible service in scope. Static `health` segments take priority over `:pk` routes.

Project compat reuses `gate_member` and filters to services linked to the project through `service_project_links`.

### Visibility & security

- Workspace admins see all workspace services and all incidents.
- Other callers see only `visible_service_ids` (owner/consumer project membership).
- Incidents are additionally filtered to projects the caller can access (`project_members` with `is_active = true` and `deleted_at IS NULL`) unless the caller is a workspace admin. This prevents leaking identifiers/titles/war rooms from inaccessible projects.
- Health may therefore differ per viewer, consistent with the per-viewer service visibility model.
- Read-only endpoint; no new write permission surface.
- Response headers: `Cache-Control: no-store`.

### Query plan

At most three queries per request, no N+1:

1. Visible services in scope: `id`, `status`, `name`, scoped by `visible_service_ids` and optional project link.
2. Issue-based incidents for the service ids:
   - `service_issues` → `issues` → `projects` → `states` → `issue_types` → active `war_rooms` by `primary_issue_id`.
   - Select service id, issue id, priority, created_at, identifier parts, state group, war room fields.
3. War-room-linked incidents for the service ids:
   - `war_room_services` → `war_rooms` → `issues` → `projects`.
   - Filter active/monitoring rooms and accessible projects.

Merge in Rust: dedupe by `(service_id, issue_id)`, prefer war-room fields, map severity, sort, compute health, and emit a snapshot for every visible service. `last_deployed_at: null`.

Suggested response:

```json
[
  {
    "service_id": "b0c...",
    "health": "degraded",
    "incidents": [
      {
        "id": "4d1...",
        "service_id": "b0c...",
        "severity": "sev2",
        "opened_at": "2026-10-24T03:12:00Z",
        "identifier": "PREPAID-123",
        "name": "Payment gateway timeout",
        "war_room_id": "c2f...",
        "war_room_sequence_id": 42,
        "war_room_name": "Payment gateway timeout"
      }
    ],
    "last_deployed_at": null
  }
]
```

### Tests (api-rs)

New DB-backed integration test (scratch workspace; run serially with other scratch suites):

- Open Incident issue linked via `service_issues` ⇒ `degraded`, severity from priority.
- Completed/cancelled/archived incident ⇒ `healthy`.
- Active war room linked through `war_room_services` ⇒ incident appears on that service even without a direct `service_issues` row.
- Issue + war room duplicate ⇒ one incident, war room severity and fields win.
- `war_room_services` + `service_issues` same service/issue ⇒ deduped.
- Planned/retired service ⇒ `unknown` with incidents preserved.
- Project-scoped endpoint excludes services not linked to the project.
- Non-member/project-inaccessible incidents do not leak (count and fields filtered); workspace admin sees all.
- Unknown/missing state counts as active.

## Frontend

### Data layer

- Delete `buildHealthSnapshot`, `fnv1aHash`, and `mulberry32`; keep `HEALTH_WEIGHT`, `INCIDENT_SEVERITY_RANK`, `getHighestSeverityIncident`.
- `ServiceHealthService` becomes an `APIService` HTTP client exposing `getHealth(slug, projectId | null)`. `projectId = null` calls the workspace endpoint; a non-null id calls the project compat endpoint.
- `ServicesStore`:
  - `fetchServices` and `fetchWorkspaceServices` call the matching health endpoint after loading services.
  - New `refreshHealth(slug, projectId)` action for polling, without toggling the page loader.
  - On success, replace snapshots for returned services; on failure, keep existing data and log. First-load failure leaves services `unknown`; health never blocks the page.
- Remove mock-derived `last_deployed_at`; endpoint returns null.

### Polling

- While the Services list view is mounted, refresh health every 60 seconds and on `visibilitychange`/window focus.
- Clean up timers/listeners on unmount. No new realtime dependency.

### UI

- `ServiceIncidentCell`: keep count + highest-severity label. Add a keyboard-focusable tooltip listing active incidents:
  - `identifier — name` and `WR-{sequence_id}` when present
  - max 5 visible, `+N more`
- `ServiceDeployCell`: `null` renders the translated "No deployment" copy instead of an em dash.
- Graph node: when `lastDeployedAt` is null, render the same "No deployment" copy (not an empty slot).
- Sidebar deploy cell automatically picks up the new copy.
- `IServiceIncident` gains the optional fields listed above.

### i18n

Add keys (all active locales; use the `translate` skill):

- `service.deploy.no_deployment` — "No deployment"
- `service.incidents.tooltip_title` — e.g. "Active incidents"
- `service.incidents.more` — ICU plural, e.g. "{count, plural, one {# more} other {# more}}"

### Frontend tests

- `ServiceHealthService`: workspace vs project URL selection and error normalization.
- Hover-card content builder (pure helper): ordering, max 5, `+N more`, war room label.
- `ServiceDeployCell`: null renders "No deployment"; non-null still renders relative time.
- Existing `workspace-services.test.ts` health/filter/order cases remain valid; remove mock-generator-specific tests if any.

## Rollout

1. No DB migration; no backup required for schema.
2. Rebuild api-rs detached per `AGENTS.md`; poll the log (Rust LTO link can take 10+ minutes); wait for `curl http://localhost:8000/health` → 200.
3. Restart `plane-live.service` and verify `http://localhost:3100/live/health/` (API container rebuild may recreate shared containers).
4. Build web (`pnpm --filter=web build`) and restart `plane-web-prod.service`; verify `:3000` → 200.
5. Smoke: open Services, confirm incidents reflect linked Incident work items/war rooms, filters `Has active incidents` and health work, tooltip lists incidents, last deploy shows "No deployment".
6. Verify permissions with a non-admin member: no incidents from inaccessible projects.

## Edge cases

- User with no visible services ⇒ `[]`.
- Service with only completed/cancelled incidents ⇒ `healthy`, empty incidents.
- Active war room whose primary issue is soft-deleted/archived ⇒ skipped.
- Multiple active war rooms for one issue cannot happen (partial unique index); resolved rooms are ignored.
- Same issue linked to multiple services ⇒ one incident per service, each counted separately.
- War room linked to a retired service ⇒ incident returned, health `unknown`.
- Intake-pending incidents (triage) count as active because their state group is not completed/cancelled.
- Services with `state_id = NULL` are treated as active incidents if the rest matches.
- Health endpoint failure does not break the catalog; UI degrades to `unknown`/stale data.

## Changelog

| Date       | Change                                                                                   |
| ---------- | ---------------------------------------------------------------------------------------- |
| 2026-10-24 | Initial design: real health/incidents from work items + war rooms; deployment postponed. |
