---
status: current
last_verified: 2026-10-27
source_hash: sha256:ca593de5aecacfd73d979c88e0a36888d49d55fc4d4c12caf64964bc22231c7e
source_paths:
  - apps/api-rs/crates/api/src/routes/intake.rs
  - apps/api-rs/crates/api/src/routes/intake_source.rs
  - apps/api-rs/crates/api/src/routes/inbound.rs
  - apps/api-rs/migrations/0014_intake_service_gate.sql
  - apps/api-rs/migrations/0015_intake_sources.sql
  - apps/web/core/store/inbox
  - apps/web/core/components/inbox
---

# Intake

Status: **Current.**

## Intent

Intake adalah pintu masuk laporan sebelum menjadi ticket. Laporan ditampung,
ditriase, lalu di-accept, decline, snooze, atau ditandai duplicate.

## Routes

| Surface              | Route                                        |
| -------------------- | -------------------------------------------- |
| Workspace intake     | `/:workspaceSlug/intake`                     |
| Application intake   | `/:workspaceSlug/projects/:projectId/intake` |
| Legacy project inbox | `.../inbox` → `.../intake`                   |
| In-app guide         | `/:workspaceSlug/docs/intake`                |

## Workspace versus application intake

- **Workspace intake** mengagregasi laporan lintas application dan menyediakan
  filter application untuk triage.
- **Application intake** hanya menampilkan laporan untuk application tersebut.
- Accept dari workspace tetap membutuhkan pemilihan application tujuan.

## Sources

- Form in-app.
- Webhook source bertoken, termasuk payload alert gaya Alertmanager.
- Source dapat diaktifkan/nonaktifkan dan memiliki token sendiri.
- Intake source dikonfigurasi dari application settings.

## Series deduplication

Laporan berulang dengan series yang sama tidak membuat ticket baru. Sistem
menaikkan `occurrence_count` dan memperbarui data laporan. Ini mencegah alert
storm membanjiri triage.

## AI triage suggestions

- Saran triage dihasilkan oleh AI berdasarkan isi laporan.
- Saran tidak diterapkan otomatis; manusia harus accept atau menolak.
- Jangan menganggap saran AI sebagai klasifikasi final.

## Service gate

Work item type Request dapat dikonfigurasi `requires service`. Saat accept,
service harus dipilih sebelum laporan menjadi ticket. Gate ini mencegah request
masuk tanpa service context.

## Triage actions

| Action    | Efek                                       |
| --------- | ------------------------------------------ |
| Accept    | Membuat ticket di application tujuan       |
| Decline   | Menutup laporan tanpa ticket               |
| Snooze    | Menunda laporan dari queue aktif           |
| Duplicate | Menautkan laporan ke ticket yang sudah ada |

## API touchpoints

- `intake.rs` — list, detail, triage, accept.
- `intake_source.rs` — source CRUD dan token.
- `inbound.rs` — ingest webhook.
- Tabel: `intake_issues`, `intake_sources`, `intake_triage_suggestions`.

## Permissions

| Role              | Read         | Triage            | Configure source |
| ----------------- | ------------ | ----------------- | ---------------- |
| Member            | Sesuai scope | Jika diberi akses | Tidak            |
| Application admin | Ya           | Ya                | Ya               |
| Workspace admin   | Ya           | Ya                | Ya               |
| Guest             | Tidak        | Tidak             | Tidak            |

## Edge cases

- Webhook dengan token salah harus ditolak.
- Series yang sama tidak boleh membuat duplikat.
- Accept tanpa service saat gate aktif harus gagal.
- Laporan yang sudah di-accept tidak boleh di-triage ulang.
- Application filter harus tetap konsisten saat pindah tab.

## In-app guide

`/:workspaceSlug/docs/intake`.

## Changelog

| Tanggal    | Perubahan                                                                             |
| ---------- | ------------------------------------------------------------------------------------- |
| 2026-10-27 | rewrite — workspace/application intake, sources, dedupe, AI suggestions, service gate |
