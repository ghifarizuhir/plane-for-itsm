# Intake ITSM: Type Gate + Service Binding + Saran Service Jev — Design (2026-10-04)

Status: disetujui user saat brainstorming (slice penuh), menunggu review spec tertulis.
Scope: `apps/api-rs` (Rust + migrasi sqlx), model/migrasi Django `apps/api`, web (`apps/web`, `packages/types`, `packages/services`, `packages/i18n`).
Terkait: [`docs/features/intake.md`](../../features/intake.md), [`2026-09-29-jev-intake-triage-design.md`](./2026-09-29-jev-intake-triage-design.md), `packages/types/src/service/core.ts`, `apps/api-rs/migrations/0003_services.sql`.

## Latar

Intake sudah punya: satu antrean default per project, bridge `IntakeIssue`, aksi accept/decline/snooze/duplicate, dan saran triage Jev (category/severity/needs_human, suggest-only). Fitur Services sudah ada (`services`, `service_issues`) dan work item sudah punya type (Incident/Problem/Change/Improvement di seed; `Request` di data nyata).

Untuk kebutuhan IT Application Services (Service Request, Incident, dst.), intake dipakai sebagai **satu front-door per project dengan klasifikasi**: tiap item ditriase ke sebuah **work item type**, dan tipe yang ber-flag `requires_service` wajib punya **service** sebelum di-accept. Kanal v1 hanya form in-app; AI (Jev) menyarankan, manusia yang apply.

## Keputusan yang dikunci saat brainstorming

1. **Front-door:** satu antrean per project + klasifikasi type. Entry point boleh banyak nanti; landing tetap satu antrean.
2. **Service binding:** wajib untuk jenis tertentu, ditentukan per tipe lewat flag `requires_service` (bukan hardcode nama tipe). Seed/backfill: `Incident`, `Request`/`Service Request`.
3. **Channel v1:** manual in-app saja. Email/webhook/portal = non-goal.
4. **Peran AI:** suggest-only. Jev menyarankan category + service; triager apply manual satu klik. Tidak ada auto-apply.
5. **Field v1:** minimal — title, description, priority (severity), type, service. Field ITSM lain (impact/urgency, requested-for, detection source, needed-by) menyusul.
6. **Strategi:** slice penuh dalam satu spec/plan (gate + AI suggest type & service). Apply category kini aman karena workflow sudah dihapus & state di-flatten (`0002_remove_workflows.sql`).
7. **Aturan accept gate:** **type wajib** + **service kondisional** (wajib bila `type.requires_service`). Fallback: project tanpa tipe live non-epic → gate type off.

## Non-goals (v1)

- Kanal email/webhook/portal eksternal; inbound auto-create.
- SLA, approval chain, multi-stage approval.
- Kolom ITSM baru (impact, urgency, detection source, requested_for, needed_by).
- Auto-apply AI / confidence threshold; apply `needs_human`; re-classify manual di luar picker.
- Multi-antrean per jenis/channel; routing lintas project; bulk triage; badge di list/sidebar intake.
- Tipe seed baru (`Service Request`); flag hanya disediakan untuk tipe yang ada.
- Riwayat perubahan service link; perubahan kontrak endpoint intake existing selain yang disebut di spec ini.

## Desain

### 1. Data model & migrasi

**1.1 Flag tipe** — `issue_types.requires_service boolean NOT NULL DEFAULT false`:

- Django: field di `apps/api/plane/db/models/issue_type.py:14-29`; migrasi baru `apps/api/plane/db/migrations/0128_issue_type_requires_service.py` (AddField + RunPython backfill, reverse noop).
- Backfill: `requires_service = true` untuk baris live dengan `is_epic = false` dan `lower(name) IN ('incident','request','service request')`.
- Epic tidak boleh `requires_service=true` (guard API; backfill mengecualikan epic).
- sqlx: migrasi baru `apps/api-rs/migrations/0014_intake_service_gate.sql` (`ALTER TABLE issue_types ADD COLUMN IF NOT EXISTS requires_service ...` + kolom saran di bawah).
- Seed workspace baru (`apps/api-rs/crates/api/src/seed.rs:296`, `TYPE_SEEDS = ["Incident", "Problem", "Change", "Improvement"]`): `Incident` dibuat dengan `requires_service = true`; tipe lain `false`. Tipe seed baru tidak ditambah.
- Ekspos di API type: `TYPE_COLS` + response struct + create/patch body di `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs` (baris 26-102, 289-305, 430); tipe FE `packages/types/src/work-item-type.ts` ikut.

**1.2 Saran service** — kolom baru di `intake_triage_suggestions` (migrasi sqlx 0014):

| Kolom                | Tipe             | Catatan                                                               |
| -------------------- | ---------------- | --------------------------------------------------------------------- |
| `service_id`         | uuid NULL        | FK logis ke `services(id)` (tanpa FK constraint, mengikuti pola JSON) |
| `service_label`      | varchar(255)     | nama service; sentinel `__none__` bila Jev abstain                    |
| `service_confidence` | double precision | confidence Choice                                                     |

Semantik: question tidak ditanya (project tanpa service) → ketiganya NULL; Jev abstain → `service_label = '__none__'`, `service_id = NULL`, confidence diisi; Jev memilih → ketiganya terisi. `applied_fields`/`dismissed_fields` (sudah `text[]`) menerima nilai `service`; `category` kini applyable.

**1.3 Service link** — reuse `service_issues` (sudah ada, unique live pair `(service_id, issue_id)`, soft-delete). Tidak ada perubahan schema.

### 2. API (Rust)

#### 2.1 Accept gate (`apps/api-rs/crates/api/src/routes/intake.rs`, jalur accept ~1588-1660)

Dievaluasi di dalam transaksi yang sama, sebelum write accept:

1. **Type gate:** bila `issues.type_id IS NULL` **dan** project punya minimal satu tipe live non-epic (join `project_issue_types` + `issue_types` live, `is_epic = false`) → `400 {"error": "Select a work item type before accepting"}`.
2. **Service gate:** bila `issue_types.requires_service = true` untuk `type_id` item dan tidak ada baris `service_issues` live (`deleted_at IS NULL`) untuk issue → `400 {"error": "Select a service before accepting this work item"}`.
3. Project tanpa tipe live non-epic → gate type off (fallback); service gate tidak relevan karena `type_id` tetap NULL.

FE men-disable tombol Accept + hint; server tetap sumber kebenaran.

#### 2.2 Set type manual di intake (`patch_issue`, `intake.rs:1375`)

- `InboxIssueFields` (`intake.rs:1219-1228`) tambah `type_id: Option<uuid::Uuid>`; body: `{"issue": {"type_id": "..."}}`.
- Validasi: tipe live (`deleted_at IS NULL`), non-epic, `workspace_id` cocok (parity `v1/work_item.rs:696-705` + guard epic).
- Write: `UPDATE issues SET type_id = ...` **langsung, tanpa state resolution** — state triage tidak berpindah sebelum accept. Ini alasan utama tidak memakai PATCH work item v1 (`v1/work_item.rs:923` auto-resolve ke project default saat type berubah).
- `requires_service` tidak ditulis di sini; hanya type.

#### 2.3 Apply & dismiss saran (`apply_triage_suggestion` `intake.rs:1967`, `dismiss_triage_suggestion` `intake.rs:2037`)

Field yang didukung v1: `category`, `service`, `severity` (`needs_human` tetap dismiss-only; apply-nya 400 seperti sekarang).

- **`category`:** butuh `category_type_id` non-null; validasi tipe live, non-epic, workspace sama; `UPDATE issues SET type_id = category_type_id` langsung (bukan v1 PATCH); idempotent bila sudah applied; tolak bila sudah dismissed.
- **`service`:** butuh `service_id` non-null (sentinel `__none__` → `400 {"error": "No service to apply"}`); validasi service live di project & workspace sama, `status <> 'retired'`; insert link `service_issues` bila belum ada link live untuk pasangan itu (idempotent, satu transaksi); bila item sudah punya link service lain, link lama **tidak** dihapus.
- **`severity`:** tidak berubah (tulis `issues.priority`).
- `dismiss`: menerima `category`, `service`, `severity`, `needs_human`; field yang sudah `applied` tetap tidak bisa di-dismiss; idempotent.
- Apply/dismiss menolak row yang belum `ready` (existing).

#### 2.4 Response suggestion (`get_triage_suggestion` `intake.rs:1938`)

Tambah objek `service`, sejajar `category`:

```json
{
  "service": {
    "id": "uuid | null",
    "label": "Payment Gateway | __none__",
    "confidence": 0.74,
    "probabilities": { "Payment Gateway": 0.74, "API Gateway": 0.11 }
  }
}
```

`null` bila question tidak ditanya / masih `pending`.

### 3. Perluasan Jev

- **`apps/api-rs/crates/ai/src/decision.rs`** — builder questions menambah `service` (Choice) bila project punya service "live":
  - Sumber: `services` project, `deleted_at IS NULL AND status <> 'retired'`, order `name`, cap 20; criteria `"{name} — {type}, criticality {criticality}, status {status}"`.
  - Opsi abstain `"No service / unsure"` → mapping handler ke sentinel `__none__`.
  - Bila tidak ada service live → question `service` dihilangkan (perilaku sama dengan `category` saat project tanpa tipe).
- **`apps/api-rs/crates/worker/src/handlers/intake_triage.rs`** — map label Choice → `service_id` (lookup by exact name dalam daftar yang dikirim); tulis `service_id`, `service_label`, `service_confidence` saat sukses; sentinel tidak di-map ke service.
- State Jev (`work_item.name`, `description`, `project.name`, `source`) dan question `category`/`severity`/`needs_human` tidak berubah; truncation 4.000 char tetap.
- Usage/budget dan alur push/sweep tidak berubah.

### 4. UI

**4.1 Detail intake** (`apps/web/core/components/inbox/content/issue-properties.tsx`):

- Baris **Type**: reuse `apps/web/core/components/dropdowns/work-item-type/dropdown.tsx`; write lewat PATCH intake (2.2), bukan update generik.
- Baris **Service**: reuse `apps/web/core/components/services/select/service-select.tsx` (sudah dipakai di `issues/peek-overview/properties.tsx` dan `issues/issue-detail/sidebar.tsx`). Bila `type.requires_service = true`, beri penanda "Required".
- Keduanya editable selama belum accepted; disembunyikan setelah accepted (work item penuh punya picker sendiri).

**4.2 Gate di FE** (`apps/web/core/components/inbox/content/inbox-issue-header.tsx`, accept di baris ~140):

- Hitung `missingType` (project punya tipe live non-epic && `issue.type_id` null) dan `missingService` (`type.requires_service` && tidak ada link service live); tombol Accept disabled + tooltip/hint.
- Modal konfirmasi accept menampilkan checklist syarat yang belum terpenuhi.

**4.3 Panel saran** (`apps/web/core/components/inbox/content/triage-suggestion.tsx`):

- Category: `Apply` + `Dismiss` (sebelumnya informasional).
- Service (baris baru): label + confidence + distribusi, `Apply` + `Dismiss`; label `__none__` → "No service suggested", hanya `Dismiss`.
- Severity dan `needs_human` tidak berubah. Tetap suggest-only; accept tidak auto-apply.

**4.4 Tipe & store:**

- `packages/types/src/inbox.ts:101-125`: `TInboxIssueTriageField` tambah `"service"`; `TInboxIssueTriageSuggestion` tambah `service`; `TWorkItemType` (`packages/types/src/work-item-type.ts`) tambah `requires_service`.
- `apps/web/core/store/inbox/inbox-issue.store.ts` (suggestion state + apply/dismiss existing, baris 36-62, 242-259): payload apply mengikuti field baru; tidak ada state baru.

**4.5 Settings type:** toggle "Requires a service" di `apps/web/core/components/work-item-types/type-form-modal.tsx`; list/detail menampilkan badge kecil (opsional, non-blocking).

**4.6 i18n:** key baru di `packages/i18n/src/locales/en/inbox.json` (gate hints, label service) dan mengikuti skill translate untuk locale lain.

### 5. Edge cases

- Project tanpa tipe live non-epic → gate type off; accept seperti sekarang.
- Type `requires_service` tapi project belum punya service → accept tetap diblok (disengaja); hint mengarahkan membuat service dulu. Item tetap bisa decline/snooze.
- Service di-retire/soft-delete setelah saran → apply 400; picker & saran exclude `retired`/deleted. Gate accept hanya mengecek link hidup (`service_issues.deleted_at IS NULL`), jadi link ke service yang lalu di-retire tetap lolos accept — disengaja agar tidak deadlock.
- Tipe dinonaktifkan/dihapus setelah saran → apply category 400; saran tetap bisa dismiss.
- Apply category lalu triager ganti type manual → `applied_fields` tetap menandai `category`; apply ulang no-op.
- Ganti type dari `requires_service` → non-required: link service lama tetap tersimpan, gate lepas. Unlink service sebelum accept → gate aktif lagi.
- Saran `__none__` → apply 400, hanya dismiss.
- Accept/apply concurrent → aman via transaksi + idempotensi (pola existing).
- Epic: tidak bisa dipilih sebagai type intake (guard non-epic); tidak bisa `requires_service`.

## Testing

- **Rust unit:** matriks gate (type null dengan/tanpa tipe project; requires_service dengan/tanpa/multi link; fallback tanpa tipe), validasi apply category (live/non-epic/workspace), apply service (insert, idempotent, sentinel 400, service retired), state item tetap `triage` setelah apply category, builder question service Jev (cap 20, exclude retired/deleted, sentinel abstain, tanpa service project).
- **Rust integration** (`apps/api-rs/crates/api/tests/`): accept 400 type/service + happy path; apply `category`/`service` mengubah DB yang benar; GET suggestion memuat objek `service`; migration 0014 backfill (`Incident` true, epic false). Ikuti aturan scratch-suite seri (`-- --test-threads=1`) dari `AGENTS.md`.
- **FE vitest:** kalkulasi gate (`missingType`/`missingService`), store apply field baru, render baris service + sentinel, type picker menulis lewat PATCH intake.
- **Manual/live:** nyalakan flag Incident, buat intake item, cek saran Jev memuat service, apply, accept blocked sampai lengkap, accept lolos; cek state tetap triage setelah apply category.
- **Docs:** update snapshot `docs/features/intake.md` (+ changelog) dan catatan flag di docs terkait work item types bila ada.

## File yang disentuh (ringkas)

- **Rust:** `apps/api-rs/migrations/0014_intake_service_gate.sql` (baru), `crates/api/src/routes/intake.rs`, `crates/api/src/routes/v1/work_item_type.rs`, `crates/api/src/seed.rs`, `crates/ai/src/decision.rs`, `crates/worker/src/handlers/intake_triage.rs`, test di `crates/api/tests/` + unit di `crates/ai`.
- **Django:** `apps/api/plane/db/models/issue_type.py`, `apps/api/plane/db/migrations/0128_issue_type_requires_service.py` (baru).
- **Web/types:** `apps/web/core/components/inbox/content/issue-properties.tsx`, `.../inbox-issue-header.tsx`, `.../triage-suggestion.tsx`, `apps/web/core/components/work-item-types/type-form-modal.tsx`, `apps/web/core/store/inbox/inbox-issue.store.ts`, `packages/types/src/inbox.ts`, `packages/types/src/work-item-type.ts`, `packages/i18n/src/locales/*/inbox.json`.
- **Docs:** `docs/features/intake.md`.

## Changelog

| Date       | Change                                                                                    |
| ---------- | ----------------------------------------------------------------------------------------- |
| 2026-10-04 | init — hasil brainstorming: type gate + service binding per tipe + saran service Jev (v1) |
