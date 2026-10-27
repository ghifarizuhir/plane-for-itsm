---
status: current
last_verified: 2026-10-27
source_hash: sha256:a3c0aee78922ec89e83669255eda6eda8b6bd3e1b6ce1e6b88c4a73287c75c55
source_paths:
  - README.md
  - apps/web/app/routes/core.ts
  - apps/api-rs/migrations
---

# Product — Terraline

## Ringkasan

Terraline adalah fork dari [Plane](https://github.com/makeplane/plane) yang
mengarahkan produk dari project delivery ke **IT service management (ITSM)**.
Workspace menjadi tenant, application mengelompokkan pekerjaan, service
membentuk katalog bersama, dan ticket membawa incident, request, problem,
change, serta improvement melalui praktik ITIL4.

Dokumen ini menjelaskan produk yang sudah berjalan di repo. Untuk alur pemakaian
per fitur, lihat in-app docs di `/:workspaceSlug/docs`. Untuk kontrak teknis,
lihat [`design/`](design/README.md) dan inventory yang dihasilkan di
[`_generated/`](_generated/routes.md).

## Platform

- **Runtime utama:** SPA React Router 8 di `apps/web`.
- **API utama:** Rust Axum + SQLx di `apps/api-rs`.
- **Fallback:** Django + DRF di `apps/api`, hanya untuk boundary yang belum
  dipindahkan.
- **Realtime:** `apps/live` dengan Hocuspocus/Yjs untuk kolaborasi dokumen.
- **Penyimpanan:** PostgreSQL per-table; Redis untuk cache, job stream, dan
  realtime.
- **Monorepo:** pnpm workspaces + Turbo.

## Users

| Persona                 | Kebutuhan utama                                                                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------- |
| Member / helpdesk L1-L2 | Triage intake, membuat dan memperbarui ticket, mencari knowledge, berkoordinasi di war room.         |
| Application admin       | Mengelola anggota, state, label, work item type, dan konfigurasi application.                        |
| Workspace admin         | Mengelola service catalog, intake, sprint, track, knowledge, view, dan pengaturan workspace.         |
| Instance admin          | Mengelola workspace, user, autentikasi, dan konfigurasi instance lewat `apps/admin` di `/god-mode/`. |

Scope akses mengikuti hierarki workspace dan application. Guest dapat diberi
akses terbatas per application.

## Product purpose

Menyatukan pekerjaan operasional IT dan delivery dalam satu sistem:

- laporan masuk lewat intake;
- triage mengubah laporan menjadi ticket;
- ticket dapat terhubung ke service, sprint, track, knowledge, dan war room;
- pekerjaan delivery tetap memakai work item, sprint, track, view, dan analytics
  yang sama.

## ITIL4 map

| Praktik ITIL4              | Yang disediakan platform                                                           | Surface utama                                                         |
| -------------------------- | ---------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Incident management        | Lapor, triage, resolve, dan review gangguan; major incident ditangani di war room. | Intake, Tickets, Queues, Services, War Rooms, Knowledge base, Reports |
| Problem management         | Kaitkan insiden berulang, investigasi akar masalah, dan publikasikan known error.  | Tickets (Problem), Tracks, Knowledge base, Reports                    |
| Service request management | Alur request standar dengan work item type Request dan service gate opsional.      | Intake, Tickets (Request), Services, Queues, Knowledge base           |
| Change enablement          | Rencanakan perubahan, kaitkan service terdampak, dan koordinasikan rollout.        | Tickets (Change), Sprints, Tracks, Services                           |
| Continual improvement      | Ubah temuan review menjadi Improvement ticket dan ukur hasilnya.                   | Tickets (Improvement), Tracks, Reports                                |
| Knowledge management       | Simpan solusi, runbook, dan known error; publikasikan lewat Space.                 | Knowledge base, Space, Galileo                                        |

ITIL4 di sini adalah operating model, bukan klaim sertifikasi. Dua batas yang
disengaja saat ini:

- belum ada SLA engine atau CAB approval board;
- service health adalah indikator katalog, bukan live monitoring.

## Current capabilities

### Tickets

Ticket adalah unit kerja operasional. Seed menyediakan work item type
**Incident**, **Problem**, **Change**, dan **Improvement**. **Request** bersifat
opt-in dan dapat mewajibkan service link saat triage.

- Lima layout: list, kanban, calendar, gantt, dan spreadsheet.
- Deskripsi TipTap, versi, komentar, activity, attachment, sub-item, dan relasi.
- Saved filter, queue, export, keyboard-first navigation, dan global search.
- Tampilan workspace-wide dan application-scoped dengan application scope
  selector.

### Intake

Intake adalah pintu masuk service desk. Laporan masuk ke triage queue workspace
sebelum menjadi ticket.

- Form in-app dan webhook source bertoken, termasuk alert gaya Alertmanager.
- Deduplikasi series: alert berulang menaikkan `occurrence_count`, bukan membuat
  ticket baru.
- Saran triage AI dengan gate accept/decline manusia.
- Klasifikasi work item type saat accept, termasuk service gate untuk Request.
- Intake workspace mengagregasi laporan lintas application dengan filter
  application.

### Services

Service membentuk katalog workspace yang dipakai bersama lintas application.

- Relasi owner dan consumer application.
- Status, criticality, type, owner, repository, dan documentation.
- Dependency bertipe: `runs_on`, `connects_to`, `replicates_to`, `depends_on`,
  `uses`, dan `other`.
- Layout board dan dependency graph, link work item, serta indikator insiden dan
  health.
- Health diturunkan dari data katalog, bukan feed monitoring langsung.

### War Rooms

War room mengubah Incident ticket menjadi respons major incident terkoordinasi.

- Toggle War Room pada Incident untuk membuat atau membuka kembali room.
- Chat realtime dengan presence, typing indicator, mention, dan message history.
- Runbook, participant, activity timeline, dan blast-radius service map.
- Severity mengikuti priority ticket; satu active room per incident.
- Event realtime diteruskan melalui `apps/live` lewat Redis.

### Sprints and Tracks

- **Sprints** (cycles) menyediakan cadence berulang dengan start/end date,
  progress, dan burn-down.
- **Tracks** (modules) mengelompokkan ticket menuju outcome atau fase yang lebih
  besar.
- Keduanya adalah agregat workspace dengan application scope.

### Queues, Knowledge base, and Reports

- **Queues** adalah saved view atas ticket: triage, active incident, problem
  review, request per service, dan upcoming change.
- **Knowledge base** adalah page tree workspace dengan TipTap, versi, komentar,
  dan kolaborasi Yjs. Page terpilih dapat dipublikasikan lewat **Space**.
- **Reports** menyediakan analytics workspace untuk tren dan distribusi lintas
  application.

### AI

- **Galileo** adalah asisten kontekstual untuk ticket, service, sprint, track,
  atau knowledge article yang sedang dibuka.
- **Scheduler** menjalankan recipe AI berulang seperti laporan harian, rangkuman
  mingguan, dan pengingat, dengan run history.
- Saran triage intake tetap mempertahankan human-in-the-loop.

### Applications

- **Application** adalah nama user-facing untuk project: identity, member,
  issue prefix, ticket, dan war room.
- Konfigurasi berada di level workspace: state, label, estimate, work item type,
  view, intake, sprint, dan track.
- Ticket dimiliki tepat satu application; identifier dan deep link tetap stabil.
- RBAC mempertahankan dua lapisan: workspace role dan application membership.

## Terminology

| Istilah produk | Istilah kode            | Route utama                                                    |
| -------------- | ----------------------- | -------------------------------------------------------------- |
| Tickets        | Work items / issues     | `/:workspaceSlug/work-items`, `.../projects/:projectId/issues` |
| Sprints        | Cycles                  | `/:workspaceSlug/cycles`                                       |
| Tracks         | Modules                 | `/:workspaceSlug/modules`                                      |
| Knowledge      | Pages                   | `/:workspaceSlug/pages`                                        |
| Applications   | Projects                | `/:workspaceSlug/projects`                                     |
| Queues         | Workspace/project views | `/:workspaceSlug/workspace-views`, `.../views`                 |

## Non-goals

- SLA engine, escalation policy, dan CAB approval board.
- Live monitoring atau APM; service health saat ini adalah indikator katalog.
- Modul Problem, Change, dan Asset terpisah di luar work item type dan surface
  yang sudah ada.
- Billing dan subscription model baru.
- Mobile native app.

## Operating context

- Operator bekerja di browser SPA dengan URL-driven state.
- Scope utama adalah workspace; application adalah filter dan batas kepemilikan
  ticket.
- State frontend memakai MobX dan SWR.
- Dokumen memakai TipTap + Yjs + Hocuspocus.
- API dan worker berjalan sebagai service Rust terpisah.

## Source of truth

- Route web: [`_generated/routes.md`](_generated/routes.md)
- Route API: [`_generated/api-routes.md`](_generated/api-routes.md)
- Tabel database: [`_generated/schema.md`](_generated/schema.md)
- Arsitektur: [`design/01-architecture.md`](design/01-architecture.md)
- Data model: [`design/02-data-model.md`](design/02-data-model.md)
- API contract: [`design/03-api-contract.md`](design/03-api-contract.md)

## Changelog

| Tanggal    | Perubahan                                                         |
| ---------- | ----------------------------------------------------------------- |
| 2026-09-03 | fork init — adaptasi dari dokumen produk Terra                    |
| 2026-09-05 | cutover backend ke Rust                                           |
| 2026-10-27 | rewrite — produk current, capability ITSM, terminology, non-goals |
