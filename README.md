<div align="center">

# Terraline

**Open-source IT service management, built on Plane and shaped by ITIL4.**

Tickets, services, knowledge, and delivery work in one workspace — so incidents, requests, problems, and changes stay connected to the services they affect.

[ITIL4 practice map](#itil4-practice-map) · [Capabilities](#capabilities) · [Architecture](#architecture) · [Self-hosting](#self-hosting) · [Local development](#local-development) · [Documentation](#documentation)

</div>

---

## Why Terraline

Terraline is a fork of [Plane](https://plane.so) that moves the product from project delivery into **IT service management**. The workspace is the tenant. Applications group the work. Services form a shared catalog. Tickets carry incidents, requests, problems, changes, and improvements through ITIL4 practices.

The result is one operational system for support and delivery teams:

- Service desk intake and triage instead of scattered request channels.
- A service catalog and dependency map instead of a spreadsheet.
- Major-incident coordination instead of a chat thread and a bridge call.
- Knowledge capture that stays linked to the tickets that produced it.
- Sprints, tracks, and reports for the delivery work behind the services.

## ITIL4 practice map

| ITIL4 practice             | What the platform provides                                                                      | Main surfaces                                               |
| -------------------------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Incident management        | Report, triage, resolve, and review disruptions; run major incidents from a dedicated war room. | Intake, Tickets, Queues, Services, War Rooms, KB, Reports   |
| Problem management         | Link recurring incidents, investigate root causes, and publish known errors.                    | Tickets (Problem), Tracks, Knowledge base, Reports          |
| Service request management | Move standard requests through a configurable Request type with an optional service gate.       | Intake, Tickets (Request), Services, Queues, Knowledge base |
| Change enablement          | Plan changes, link affected services, and coordinate rollout and rollback.                      | Tickets (Change), Sprints, Tracks, Services                 |
| Continual improvement      | Turn review findings into Improvement tickets and measure the outcome.                          | Tickets (Improvement), Tracks, Reports                      |
| Knowledge management       | Capture solutions, runbooks, and known errors; publish selected knowledge externally.           | Knowledge base, Space, Galileo                              |

ITIL4 here is an operating model, not a certification claim. Two deliberate boundaries today: there is no SLA engine or CAB approval board, and service health is a catalog indicator rather than live monitoring. The in-app guide at `/:workspaceSlug/docs` walks through each practice with the exact product flow.

## Capabilities

### Tickets

Tickets are the unit of operational work. The seed ships four work item types — **Incident**, **Problem**, **Change**, and **Improvement**. **Request** is opt-in: create a Request work item type and enable `requires service` to enforce a service link during intake triage.

- Five layouts: list, kanban, calendar, gantt, and spreadsheet.
- Rich text descriptions with TipTap, versions, comments, activity, attachments, sub-items, and relations.
- Saved filters, queues, export, keyboard-first navigation, and global search.
- Workspace-wide and application-scoped views, with a persistent application scope selector.

### Intake

Intake is the service desk front door. Reports arrive in a workspace triage queue before they become tickets.

- In-app forms and token-authenticated webhook sources (for example, Alertmanager-style alerts).
- Series deduplication: repeated alerts update `occurrence_count` instead of creating duplicate tickets.
- AI triage suggestions with a human accept/decline gate.
- Accept-time classification by work item type, with a required service link when the type demands it.
- Workspace intake aggregates reports across applications, with an application filter for triage.

### Services

Services form a workspace-level service catalog shared across applications.

- Owner and consumer application links, so one service can be supported by one team and consumed by many.
- Status, criticality, type, owner, repository, and documentation fields.
- Typed dependencies: `runs_on`, `connects_to`, `replicates_to`, `depends_on`, `uses`, and `other`.
- Board and dependency-graph layouts, work-item links, and incident/health indicators.
- Health is currently derived from catalog data, not a live monitoring feed; the replacement seam is isolated in `apps/web/core/services/service-health.service.ts`.

### War Rooms

A War Room turns an Incident ticket into a coordinated major-incident response.

- Toggle the War Room property on an Incident to create or reopen a room; toggle it off to resolve.
- Realtime chat with presence, typing indicators, mentions, and message history.
- Runbook, participants, activity timeline, and blast-radius service map.
- Severity follows ticket priority; one active room per incident.
- Realtime events relay through `apps/live` over Redis.

### Sprints and Tracks

- **Sprints** (cycles) provide a recurring cadence with start and end dates, progress, and burn-down.
- **Tracks** (modules) group tickets toward a larger outcome or phase.
- Both are workspace-level aggregates with application scope, so the same planning surfaces work across teams.

### Queues, Knowledge base, and Reports

- **Queues** are saved views over tickets: triage, active incidents, problem review, per-service requests, and upcoming changes.
- **Knowledge base** is a workspace page tree with a TipTap editor, versions, comments, and Yjs collaboration for concurrent editing. Selected pages can be published through **Space**.
- **Reports** provide workspace analytics for trends and distribution across applications.

### AI

- **Galileo** is a context-aware assistant that can summarize the open ticket, service, sprint, track, or knowledge article.
- **Scheduler** runs recurring AI recipes such as daily reports, weekly summaries, and reminders, with run history.
- Intake triage suggestions keep a human in the loop before classification is applied.

### Applications and workspace configuration

- **Applications** are the user-facing name for projects: identity, members, issue prefix, tickets, and war rooms.
- Configuration lives at workspace level: states, labels, estimates, work item types, views, intake, sprints, and tracks.
- Tickets remain owned by exactly one application; application identifiers and deep links stay stable.
- RBAC keeps both layers: workspace roles plus per-application membership, with guest scoping per application.
- The application scope selector is persisted per workspace and shared by every aggregate page.

## Architecture

Terraline is a pnpm + Turbo monorepo with a Rust API at its center.

| Path              | Role                                                               | Stack                                                           |
| ----------------- | ------------------------------------------------------------------ | --------------------------------------------------------------- |
| `apps/web`        | Main SPA: tickets, services, intake, KB, reports, docs             | React 19, React Router 8, Vite, Tailwind 4, MobX, SWR, TipTap 2 |
| `apps/admin`      | Instance administration (`/god-mode`)                              | React Router, Vite                                              |
| `apps/space`      | Public pages and knowledge publishing                              | React Router, Vite                                              |
| `apps/live`       | Realtime collaboration and war-room events                         | Express, Hocuspocus, Yjs, Redis                                 |
| `apps/api-rs`     | Primary API, worker, and beat                                      | Rust, Axum, SQLx, Tokio, Redis Streams                          |
| `apps/api`        | Django fallback for un-ported boundaries                           | Django 5, Django REST Framework                                 |
| `apps/proxy`      | Reverse proxy and TLS termination                                  | Caddy                                                           |
| `packages/ui`     | Design system and Storybook                                        | React, Radix, Tailwind                                          |
| `packages/editor` | Rich-text editor                                                   | TipTap, Yjs                                                     |
| `packages/*`      | Shared types, MobX stores, API clients, constants, i18n, utilities | TypeScript                                                      |

Data and infrastructure:

- **PostgreSQL 15** with one table per entity, not a single JSONB document store.
- **Valkey/Redis** for caching and the `plane:jobs` job stream used by the Rust worker and beat.
- **MinIO/S3** for uploads.
- **Hocuspocus + Yjs** for real-time document collaboration.

The Django app (`apps/api`) remains as an opt-in fallback for boundaries that have not been ported to Rust: asset upload/download, Unsplash, analytics export, notification sending, and OAuth. The Rust service is the default API.

## Self-hosting

Prerequisites: Docker with Compose, Node.js >= 22.22, and pnpm 11.10 (enabled through Corepack).

```bash
./setup.sh
docker compose up -d --build
```

`setup.sh` copies the env files, generates a `SECRET_KEY`, enables pnpm, and installs dependencies. The compose stack starts Postgres, Valkey, MinIO, the Rust API/worker/beat, the web, admin, space, and live apps, and Caddy. Caddy serves the stack on `LISTEN_HTTP_PORT` (default `80`).

Verify the API:

```bash
curl http://localhost:8000/health
```

For production, set `SECRET_KEY`, `WEB_URL`, `CORS_ALLOWED_ORIGINS`, database, Redis, and storage values in the env files. The full runbook is in [`docs/design/06-ops-runbook.md`](docs/design/06-ops-runbook.md).

## Local development

Run the backend in Docker and the frontends with Vite:

```bash
./setup.sh
docker compose -f docker-compose-local.yml up -d --build api worker beat-worker
pnpm dev
```

- Web: `http://localhost:3000`
- Admin: `http://localhost:3001`
- API: `http://localhost:8000`
- Live: `http://localhost:3100`

Useful commands:

| Command                             | Purpose                           |
| ----------------------------------- | --------------------------------- |
| `pnpm dev`                          | Start all dev servers             |
| `pnpm build`                        | Build all packages and apps       |
| `pnpm check`                        | Run format, lint, and type checks |
| `pnpm fix`                          | Auto-fix format and lint issues   |
| `pnpm --filter=@plane/ui storybook` | Start Storybook on port 6006      |

Service-specific workflows, migration rules, and backend test commands are in [`AGENTS.md`](AGENTS.md).

## Repository layout

```text
apps/
  web/           main SPA
  admin/         instance admin (god-mode)
  space/         public pages
  live/          realtime collaboration and war-room events
  api-rs/        Rust API, worker, beat, sqlx migrations
  api/           Django fallback
  proxy/         Caddy reverse proxy
packages/
  ui/            design system + Storybook
  editor/        TipTap + Yjs editor
  types/         shared TypeScript types
  shared-state/  MobX stores
  services/      API clients
  constants/     enums, prefixes, navigation
  i18n/          locales
docs/
  design/        architecture, data model, API contract, testing, ops
  features/      product specifications
  superpowers/   design history and implementation plans
```

## Documentation

- **Docs index** — [`docs/README.md`](docs/README.md) lists which documents are current and which are historical snapshots.
- **In-app guide** — open `/:workspaceSlug/docs` in a running instance. The **ITIL4** group covers incident, problem, service request, change and improvement, and an end-to-end operating flow.
- **Architecture** — [`docs/design/01-architecture.md`](docs/design/01-architecture.md)
- **API contract** — [`docs/design/03-api-contract.md`](docs/design/03-api-contract.md)
- **Testing strategy** — [`docs/design/05-testing-strategy.md`](docs/design/05-testing-strategy.md)
- **Ops runbook** — [`docs/design/06-ops-runbook.md`](docs/design/06-ops-runbook.md)
- **Design history** — [`docs/superpowers/specs/`](docs/superpowers/specs/)

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening a pull request, and run `pnpm check` before pushing. Bug reports and feature requests can follow the repository's issue templates.

## Security

Report vulnerabilities responsibly instead of opening a public issue. See [`SECURITY.md`](SECURITY.md) for the disclosure process.

## License

Terraline is licensed under the [GNU Affero General Public License v3.0 only](LICENSE.txt) (`AGPL-3.0-only`). It is a modified fork of [Plane](https://github.com/makeplane/plane), which is also licensed under AGPL-3.0-only.

Upstream copyright notices and license terms remain in effect. Fork attribution, the copyright for modifications, and a summary of changes are recorded in [`NOTICE`](NOTICE).

If you run a modified version of Terraline as a network service, AGPL section 13 requires you to offer users of that service the Corresponding Source of the version you run.
