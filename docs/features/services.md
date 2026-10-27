---
status: current
last_verified: 2026-10-27
source_hash: sha256:865e8a3cb48145a92bb0dc33c17b500e99b78e4e0e4cdedcd8ffdd0da1a99bc0
source_paths:
  - apps/api-rs/crates/api/src/routes/service.rs
  - apps/api-rs/crates/api/src/routes/service_health.rs
  - apps/api-rs/crates/api/src/routes/service_metrics.rs
  - apps/api-rs/crates/api/src/routes/service_access.rs
  - apps/api-rs/migrations
  - apps/web/core/store/service.store.ts
  - apps/web/core/components/services
  - apps/web/app/routes/core.ts
---

# Services

Status: **Current.**

## Intent

Service catalog adalah daftar layanan IT tingkat workspace: apa yang
disediakan, siapa pemiliknya, bagaimana dependensinya, dan bagaimana
kesehatannya. Service dapat dipakai banyak application.

## Routes

| Surface                           | Route                                                                        |
| --------------------------------- | ---------------------------------------------------------------------------- |
| Service list                      | `/:workspaceSlug/services`                                                   |
| Service detail                    | `/:workspaceSlug/services/:serviceId`                                        |
| Legacy application service list   | `/:workspaceSlug/projects/:projectId/services` → workspace catalog           |
| Legacy application service detail | `/:workspaceSlug/projects/:projectId/services/:serviceId` → workspace detail |

## Catalog model

Field utama:

- name, description, status, criticality, type;
- owner dan documentation;
- repository dan link operasional;
- owner application dan consumer application;
- work item terkait.

Service adalah entitas workspace. Application tidak memiliki service; application
dapat menjadi owner atau consumer.

## Typed dependencies

Dependency memiliki tipe:

- `runs_on`
- `connects_to`
- `replicates_to`
- `depends_on`
- `uses`
- `other`

Graph menampilkan arah dan tipe dependency. Hover dan focus card membantu
membaca blast radius tanpa membuka detail.

## Layouts

- **List/board**: katalog dengan filter status, criticality, type, dan
  application.
- **Graph**: dependency graph dengan grouping berdasarkan kind dan reset layout.
- **Detail**: overview, dependency, work item terkait, health, dan metrics.

## Health

Health adalah indikator katalog yang diturunkan dari data service, insiden, dan
war room yang tertaut. Health bukan live monitoring atau APM. Untuk integrasi
metrics eksternal, gunakan metric source workspace.

## Metrics ingest

- Metrics source dikelola di workspace settings.
- Ingest menerima payload eksternal dan menyimpan metrik terkait service.
- Service detail menampilkan metrik yang tersedia; jangan mengklaim real-time
  monitoring jika source belum dikonfigurasi.

## API touchpoints

- `service.rs` — CRUD dan katalog.
- `service_health.rs` — health indicator.
- `service_metrics.rs` — metrics ingest/read.
- `service_access.rs` — akses service per application.
- Migrasi `0003`, `0004`, `0020`, `0023`, `0024`, `0025`.

Daftar method/path: [`../_generated/api-routes.md`](../_generated/api-routes.md).

## Permissions

| Role              | Create                       | Read     | Update                    | Delete |
| ----------------- | ---------------------------- | -------- | ------------------------- | ------ |
| Member            | Tidak                        | Ya       | Tidak                     | Tidak  |
| Application admin | Ya, jika diizinkan workspace | Ya       | Service milik application | Tidak  |
| Workspace admin   | Ya                           | Ya       | Ya                        | Ya     |
| Guest             | Tidak                        | Terbatas | Tidak                     | Tidak  |

## Limitations

- Belum ada live monitoring/APM bawaan.
- Health bukan pengganti alerting.
- Dependency graph tidak melakukan auto-discovery; dependency diisi manual atau
  lewat API.
- Service metrics bergantung pada source yang dikonfigurasi.

## In-app guide

`/:workspaceSlug/docs/services` dan `/:workspaceSlug/docs/metric-sources`.

## Changelog

| Tanggal    | Perubahan                                                                   |
| ---------- | --------------------------------------------------------------------------- |
| 2026-10-27 | new — menggantikan proposal CMDB, katalog workspace, graph, health, metrics |
