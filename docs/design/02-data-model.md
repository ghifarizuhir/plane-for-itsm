---
status: current
last_verified: 2026-10-27
source_hash: sha256:49f80e2da239d5fa1cda9bf28e03c303fc2f2e3fc93bf4907a0e7baf259417a5
source_paths:
  - apps/api-rs/migrations
  - apps/api/plane/db/models
---

# 02 — Data Model

Status: **Current.**

Database adalah PostgreSQL. Schema operasional didefinisikan oleh migrasi SQLx
di `apps/api-rs/migrations/`; model Django di `apps/api/plane/db/models/`
menjadi referensi historis dan fallback. Daftar tabel aktual ada di
[`../_generated/schema.md`](../_generated/schema.md).

## Design principles

1. **Per-table.** Setiap entity memiliki tabel sendiri; tidak ada satu tabel
   `entities` JSONB.
2. **Workspace-scoped.** Tabel domain memiliki `workspace_id` atau dapat
   dijangkau melalui relasi application.
3. **Application ownership.** Ticket dan konfigurasi application terikat pada
   satu application.
4. **Soft delete.** Entity memakai `deleted_at`; archive dan restore tersedia
   untuk entity yang mendukungnya.
5. **Migrasi append-only.** Perubahan schema dibuat sebagai migrasi baru.
6. **ID string/UUID.** ID tidak boleh diasumsikan auto-increment di API publik.

## Migration baseline

| Migrasi            | Isi                                                   |
| ------------------ | ----------------------------------------------------- |
| `0001_initial.sql` | Baseline schema Plane hasil squash 123 migrasi Django |
| `0002`             | Index archive issue per project                       |
| `0003`             | Services, dependencies, dan link application          |
| `0004`             | Index dependency service                              |
| `0005`             | Perbaikan sequence issue                              |
| `0006`–`0009`      | AI schedules, review fixes, dan spec                  |
| `0010`             | Saran triage intake                                   |
| `0011`             | War rooms                                             |
| `0012`–`0013`      | Release review control dan briefing                   |
| `0014`             | Service gate intake                                   |
| `0015`             | Intake sources                                        |
| `0016`             | Penghapusan workflows                                 |
| `0017`             | Index primary issue war room                          |
| `0018`             | Penghapusan release review                            |
| `0019`             | Workspace document settings                           |
| `0020`             | Workspace-level services                              |
| `0021`             | Workspace settings                                    |
| `0022`             | Taxonomy workspace                                    |
| `0023`             | Tipe dependency service                               |
| `0024`             | Service metrics                                       |
| `0025`             | Service kind                                          |

## Core domains

| Domain        | Tabel utama                                          | Catatan                                      |
| ------------- | ---------------------------------------------------- | -------------------------------------------- |
| Workspace     | `workspaces`, `workspace_members`                    | Tenant root dan membership                   |
| Application   | `projects`, `project_members`                        | Application adalah project user-facing       |
| Ticket        | `issues`, `issue_*`                                  | Work item, type, relation, activity, version |
| Planning      | `cycles`, `cycle_issues`, `modules`, `module_issues` | Sprint dan track                             |
| Knowledge     | `pages`, `descriptions`, `description_versions`      | Page tree dan versi                          |
| Views         | `views`, `global_views`                              | Queue dan saved filter                       |
| Configuration | `states`, `labels`, `estimates`, `issue_types`       | Konfigurasi workspace/application            |
| Intake        | `intake_issues`, `intake_sources`                    | Triage dan webhook source                    |
| Collaboration | `notifications`, `favorites`, `stickies`, `drafts`   | Notifikasi dan personal surface              |
| Assets        | `assets`                                             | Attachment dan upload                        |

## ITSM extensions

| Domain             | Tabel / migrasi                                                       | Perilaku                                              |
| ------------------ | --------------------------------------------------------------------- | ----------------------------------------------------- |
| Services           | `services`, `service_dependencies`, `service_metrics`                 | Katalog workspace, dependency bertipe, metrics ingest |
| War rooms          | `war_rooms`, `war_room_*`                                             | Major incident room, chat, participant, activity      |
| AI                 | `ai_schedules`, `ai_schedule_runs`, `ai_conversations`, `ai_messages` | Scheduler dan Galileo                                 |
| Intake sources     | `intake_sources`, `intake_triage_suggestions`                         | Webhook source dan saran triage                       |
| Workspace settings | `workspace_settings`, `workspace_document_settings`                   | Feature flag dan konfigurasi dokumen                  |
| Taxonomy           | `workspace_*` taxonomy tables                                         | State/label/type workspace-level                      |

Service health bukan tabel monitoring. Nilainya diturunkan dari data katalog
dan insiden/war room yang tertaut.

## Removed concepts

- **Workflows** dihapus pada `0016`; state sekarang dikelola langsung per
  application/workspace.
- **Release review** dihapus pada `0018`; route dan tabel review tidak lagi
  menjadi bagian produk current.
- Workflow dan release review hanya boleh muncul di arsip historis, bukan di
  dokumentasi current.

## Soft delete and retention

- Entity utama memakai `deleted_at` dan query default menyaring data terhapus.
- Archive menyimpan entity dengan `archived_at` untuk issue, cycle, module, dan
  application.
- Restore tersedia dari halaman archive terkait.
- Hard delete mengikuti kebijakan retensi worker; jangan menghapus row langsung
  dari database produksi.

## Source of truth

- Schema aktual: [`../_generated/schema.md`](../_generated/schema.md)
- Arsitektur: [`01-architecture.md`](./01-architecture.md)
- API contract: [`03-api-contract.md`](./03-api-contract.md)

## Changelog

| Tanggal    | Perubahan                                                                |
| ---------- | ------------------------------------------------------------------------ |
| 2026-09-03 | fork init                                                                |
| 2026-09-05 | cutover Rust                                                             |
| 2026-10-27 | rewrite — migrasi 0001–0025, services, war rooms, AI, workspace settings |
