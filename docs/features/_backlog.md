# Backlog — Parked Ideas (belum commit phase)

> Actual-only: file `features/incidents.md` + `features/services.md` (propose ITSM) dihapus 2026-09-03 — isinya diparkir di sini sampai implementasi ITSM dimulai. Jangan buat `features/<itsm-page>.md` baru sebelum ada kode + route + model.

## ITSM Proposals (ex-`features/incidents.md` + `features/services.md` — ringkas)

- **Incident Management** (referensi: `terra/docs/features/incidents.md:1`): list `:workspaceSlug/projects/:projectId/incidents` + detail `:id` — kolom Priority/State/Created + War Room (`isWarRoom`)/Detection Source bila jadi ITSM; reuse `Issue` + `IssueType` vs tabel baru (belum diputuskan); export CSV cap 5000.
- **Service Map / CI** (referensi: `terra/docs/features/services.md:1`): list `:workspaceSlug/configuration-items` + detail `:id` — butuh model Django baru (`ConfigurationItem` + `CIDependency` + `AppCILink`, belum ada di `plane/db/models/`); filter Kind/Env/Status; graph `@xyflow/react` Phase 2.
- **Problem+RCA / Change+goals / Request / Knowledge / Improvement / Asset**: belum ada model/route/store — propose menyusul setelah Incident + Service Map terbukti.

Ide cross-feature yang belum ada timeline — bukan Open Items blocking. Review quarterly; idle > 1 tahun → hapus atau commit. Adaptasi dari `terra/docs/features/_backlog.md`.

| Idea                    | Deskripsi                                                | Sumber                       |
| ----------------------- | -------------------------------------------------------- | ---------------------------- |
| Multi-stage approvals   | Approval chains parallel/sequential (Request→Change)     | Terra Phase 2+ backlog       |
| SLA timers              | Auto-calc time-in-state vs target, badge breach          | Terra Phase 2 deferred       |
| Auto-discovery CI       | Dynatrace/Prometheus → auto-register `ConfigurationItem` | Terra Service Map Phase 2    |
| Impact graph recursive  | `GET /configuration-items/:id/impact?depth=3` (CTE)      | Terra Service Map Phase 2    |
| Report export           | DOCX/PDF per entity (mirip Terra `report.md`)            | Terra                        |
| Webhook integrations    | Generic inbound webhook → auto-create Incident           | Terra integrations (dropped) |
| Dashboard customization | Custom layout Widgets                                    | Plane Pages/Analytics        |
| Comment threading       | Threaded comments + @mentions                            | Terra Phase 3+               |
| Collaborative edit      | OT/CRDT (Yjs sudah ada di Live — extend ke Issues)       | Terra Phase 3+               |

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
