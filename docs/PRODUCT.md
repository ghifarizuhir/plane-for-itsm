# Product — Plane for ITSM

## Platform

Fork dari **Plane** (open-source project management — `README.md:26`). Actual-only: docs ini snapshot Plane upstream (Work Items, Cycles, Modules, Views, Pages, Analytics). Arah ITSM (incidents/problems/changes/Service Map) adalah future — diparkir di `features/_backlog.md`, belum ada kode/route/model.

## Users

Hierarki Plane tetap, ditambah peran ITSM:

- **Members** (engineer, helpdesk L1/L2) — daily operators: triage incidents, log requests, update work items, capture knowledge.
- **Team leads / Project admins** — own workload + applications/infrastruktur: approve requests, review changes, publish RCA.
- **Workspace admins / Instance admins (god-mode)** — oversight + governance: `apps/admin` (`ADMIN_BASE_PATH=/god-mode/` — `plane/settings/common.py:395`), manage workspaces, users, auth providers.
- **Instance = multi-tenant** — setiap workspace terisolasi; instance admin punya override lintas-workspace.

Access scope mengikuti hierarki Plane (member ⊂ project admin ⊂ workspace admin ⊂ instance admin).

## Product Purpose

**Plane** = track issues, run cycles, manage modules roadmaps tanpa chaos (`README.md:28`). **Plane for ITSM** = satukan day-to-day IT operations (incident→problem→change→knowledge→asset→Service Map) dengan delivery work (issues/cycles/modules) dalam satu sistem, satu data model — supaya ops dan product tidak juggling tool terpisah.

Sukses = seberapa cepat work bergerak `open → done` dengan trace lengkap (comments, history, versions).

## Positioning

Plane = **project delivery** (issues, sprints/cycles, roadmaps). Referensi Terra = **entity-graph unification**, tapi itu arsitektur Terra (Express+Drizzle `entities` JSONB) — bukan actual Plane (Postgres per-table, skema `plane/db/models/`, dilayani Rust Axum). Fork ITSM future (lihat `_backlog.md`) akan reuse Work Items + `IssueType` (`issue_type.py:11`) bila memungkinkan, bukan copy `entities` JSONB.

## Operating Context

- Operators kerja di browser SPA: **React Router 8** (`apps/web/package.json:67`, `apps/web/react-router.config.ts`), keyboard-first (Cmd+K global search), URL-driven filters (`?q=&status=&sort=&page=`), detail via page/overlay.
- State: **MobX** (`packages/shared-state`, `apps/web/core/store/root.store.ts`) + SWR (`swr:2.4.2` — `pnpm-workspace.yaml:170`). Bukan TanStack Query only seperti Terra.
- Editor: **TipTap 2** (`packages/editor`, `@tiptap/*` — `pnpm-workspace.yaml:52`) + Yjs collaboration (`yjs`, `y-prosemirror`).
- Realtime: `apps/live` (Express + express-ws — `pnpm-workspace.yaml:109`) + Hocuspocus (`@hocuspocus/server:2.15.2`) untuk Pages collaboration; bukan SSE ticket-based Terra.
- Home/dashboard: work items stats + Cycles/Modules/Views/Pages/Analytics.
- Entity pages: rich sections, comments, timeline, versions, attachments.

## Capabilities and Constraints

**Confirmed capabilities (Plane base):**

- Work Items — rich text editor + file uploads, sub-properties, cross-issue refs.
- Cycles — burn-down charts, momentum tracking.
- Modules — divide complex projects.
- Views — saved filters, shareable.
- Pages — TipTap + AI capabilities, convert notes → actionable items.
- Analytics — realtime insights, trends, blockers.
- Workspaces/Projects/States/Labels/Estimates/Members — fondasi multi-tenant Plane.

**ITSM extensions (fork):**

- Incident, Problem (+RCA), Change (+goals/checkpoints), Improvement, Request, Knowledge, Asset — CRUD + filters + export.
- Service Map — `configuration_items` + dependencies + app links (impact analysis).
- Cross-entity refs via `@`-mention di description/sections.

**Constraints:**

- UI bahasa Inggris (product), docs Indonesia.
- Deploy: **Docker Compose** (`docker-compose.yml`) + Postgres + Valkey Redis (Stream `plane:jobs`, ganti RabbitMQ/Celery) — bukan Render free-tier Terra. API: Rust `api:8000`; Django hanya fallback opt-in.
- Auth: `X-Api-Key` + `Bearer` (Rust) + social OAuth via Django fallback (GitHub/GitLab/Google/Gitea) + god-mode — bukan JWT bearer Terra.
- Package manager: **pnpm** (`pnpm@11.10.0`, `pnpm-workspace.yaml:1`) + **Turbo** (`turbo.json:1`) — bukan npm workspaces.

## Brand Commitments

- Nama: **Plane for ITSM** (fork Plane). Plane tetap brand upstream (AGPL-3.0 — `package.json:6`).
- Entity color ITSM (Terra: Incident=red, Problem=purple, dst.) belum dipakai — actual Plane pakai `project.identifier` + `IssueType.logo_props`.
- UI voice Inggris, concise, operational.

## Evidence on Hand

- `apps/web` + `apps/admin` + `apps/live` + `apps/space` + `apps/api-rs` (Rust Axum — API utama) + `apps/api` (Django, fallback) + `packages/*` (15 packages) — working implementation Plane.
- `docs/design/` — engineering cross-cutting (lihat `design/README.md`).
- `docs/features/` — per-page specs (lihat `features/README.md`).
- Upstream: `https://docs.plane.so` + `https://developers.plane.so`.

## Product Principles

1. **Actual first** — docs gambarkan Plane apa adanya; ITSM hanya sebagai backlog sampai ada kode.
2. **Scope-aware operations** — access mengikuti workspace/project hierarchy; UI harus komunikasikan apa yang visible.
3. **ITIL4 as methodology, not ceremony** — proses ada untuk mempercepat traceability, bukan paperwork.
4. **Speed for operators** — keyboard-first, realtime, low-friction capture > dashboard polish.
5. **Everything is traceable** — comments, timeline, versions, audit logs.

---

## Changelog

| Date       | Change                                                    |
| ---------- | --------------------------------------------------------- |
| 2026-09-03 | fork init — adaptasi dari terra `PRODUCT.md:6`            |
| 2026-09-05 | cutover `rust-cutover-v1`: backend + deploy + auth → Rust |
