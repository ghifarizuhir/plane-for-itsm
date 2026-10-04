# Intake Webhook Source: Alertmanager → Incident — Design (2026-10-04)

Status: disetujui user saat brainstorming (4 bagian), menunggu review spec tertulis.
Scope: `apps/api-rs` (Rust + migrasi sqlx), web (`apps/web`, `packages/types`, `packages/i18n`), docs. Tanpa perubahan model/migrasi Django (kolom & tabel baru murni milik api-rs).
Terkait: [`docs/features/intake.md`](../../features/intake.md), [`2026-10-04-intake-itsm-type-service-gate-design.md`](./2026-10-04-intake-itsm-type-service-gate-design.md), `apps/api-rs/crates/api/src/routes/intake.rs`, `apps/api-rs/crates/api/src/routes/webhook.rs` (outbound, tidak diubah), `docs/features/_backlog.md` (item "Generic inbound webhook → auto-create Incident").

## Latar

Intake v1 sudah menjadi front-door ITSM: satu antrean per project, klasifikasi type + service (gate accept), saran triage Jev (suggest-only), kanal manual in-app. Kanal otomatis (webhook/email/portal) adalah non-goal v1.

Fase berikutnya: **event intake dari monitoring** — alert Prometheus Alertmanager masuk otomatis sebagai item intake Incident. Karena tidak ada triager di depan saat event masuk, klasifikasi harus deterministik: mapping label → service + severity → priority, dikonfigurasi per source. Ini menghidupkan kembali keputusan auto-apply yang ditunda di v1, tetapi secara terbatas: mapping rule (bukan AI), dengan toggle auto-accept per source dan default tetap pending untuk direview manusia.

Infrastruktur webhook yang ada (`routes/webhook.rs`) seluruhnya **outbound** (Terraline → URL luar) dan tidak berubah. Endpoint inbound ini adalah arah sebaliknya dan belum ada.

## Keputusan yang dikunci saat brainstorming

1. **Skenario:** alert monitoring → auto-create item intake Incident (bukan service request dari aplikasi lain).
2. **Klasifikasi:** mapping label per source — `service_map` (nilai label → service), `severity_map` (nilai label → priority), plus fallback service dan default priority. Deterministik, tanpa AI. Type **tetap per source** (default Incident), bukan per-label.
3. **Landing:** item mendarat di intake **pending**; toggle `auto_accept` per source. Auto-accept hanya jalan bila klasifikasi lengkap (type + service bila `requires_service`).
4. **Dedup:** fingerprint Alertmanager sebagai identitas seri — firing berulang meng-update item yang sama (`occurrence_count`, `last_seen`), resolved meng-auto-resolve, refire membuka lagi.
5. **Auth:** token acak di URL (`plane_is_<32hex>`) + rotate/revoke. HMAC & IP allowlist ditunda.
6. **Payload:** format native Prometheus Alertmanager v4; Grafana Alerting ikut jalan gratis via contact point Alertmanager-compatible.
7. **Resolved saat pending:** auto-decline. Item snoozed dibiarkan (aksi sadar triager).
8. **Pendekatan:** rules dikelola di Plane + UI settings project (pendekatan A), bukan convention labels atau API-only.

## Non-goals (fase ini)

- Format payload selain Alertmanager (native Grafana/Zabbix/Dynatrace, generic schema).
- HMAC signature & IP allowlist — di balik Cloudflare Tunnel IP klien tidak reliabel.
- Email/portal eksternal (tetap ditunda).
- Auto-apply Jev saat ingest; Jev tidak dipanggil untuk item webhook.
- Mapping type per-label; cross-project routing; multi-antrean; bulk triage.
- SLA timer & approval chain.
- Rate limit per-source (memakai limiter global 600/menit yang ada).
- Realtime push ke UI (list memakai refetch yang ada).

## Desain

### 1. Data model & migrasi

**1.1 Tabel `intake_sources`** — migrasi baru `apps/api-rs/migrations/0015_intake_sources.sql`:

```sql
CREATE TABLE IF NOT EXISTS public.intake_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id uuid NOT NULL,
    name character varying(255) NOT NULL,
    token character varying(64) NOT NULL,
    is_active boolean NOT NULL DEFAULT true,
    auto_accept boolean NOT NULL DEFAULT false,
    type_id uuid,
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);
CREATE UNIQUE INDEX IF NOT EXISTS intake_sources_token_uniq
    ON public.intake_sources (token) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS intake_sources_project_idx
    ON public.intake_sources (project_id) WHERE deleted_at IS NULL;
```

- Token dibuat server-side, format `plane_is_` + 32 hex (pola `generate_token` di `db/models/webhook.py`), disimpan **plaintext** — parity dengan `secret_key` webhook outbound, blast radius hanya create intake item, dan URL bisa disalin kapan saja dari UI. Rotate mengganti token; token lama langsung mati.
- Soft delete (`deleted_at`) menjaga atribusi issue lama; lookup token hanya baris live.
- `config` jsonb (validasi di kode saat create/patch):

```json
{
  "service_label_key": "service",
  "service_map": { "payment-api": "<service uuid>" },
  "fallback_service_id": "<service uuid>|null",
  "severity_label_key": "severity",
  "severity_map": { "critical": "urgent", "warning": "high", "info": "low" },
  "default_priority": "none"
}
```

**1.2 Seri alert di `issues`** (migrasi yang sama):

```sql
ALTER TABLE public.issues
    ADD COLUMN IF NOT EXISTS intake_source_id uuid,
    ADD COLUMN IF NOT EXISTS intake_fingerprint text,
    ADD COLUMN IF NOT EXISTS intake_occurrence_count integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS intake_last_seen_at timestamptz;
CREATE UNIQUE INDEX IF NOT EXISTS issues_intake_series_uniq
    ON public.issues (intake_source_id, intake_fingerprint)
    WHERE intake_source_id IS NOT NULL;
```

Satu seri `(source, fingerprint)` = satu issue selamanya; upsert aman dari race & retry Alertmanager.

**1.3 Status triage & atribusi** — tidak ada kolom baru:

- `intake_issues.source = 'WEBHOOK'` (CharField bebas, sudah ada; `external_source` tidak dipakai).
- Atribusi nama source dibaca dari join `issues.intake_source_id` → `intake_sources.name`; respons detail issue intake menyertakan `{id, name, occurrence_count, last_seen_at}`.
- Status: pending `-2`, declined `-1`, snoozed `0`, accepted `1`, duplicate `2` (nilai v1).
- FE: tambah `WEBHOOK = "WEBHOOK"` ke `EInboxIssueSource` (`packages/types/src/inbox.ts`).

### 2. Endpoint ingest (publik)

`POST /api/inbound/alertmanager/:token/`

**2.1 Auth & middleware**

- Handler tanpa extractor `AuthUser`; token di path → lookup `intake_sources` live & project live. Token tak dikenal → 404; `is_active = false` → 403.
- Origin middleware (`middleware/origin.rs`) diberi pengecualian prefix path `/api/inbound/` — sender server-to-server tidak mengirim Origin/Referer. Mutasi web lain tidak terpengaruh.
- Rate limit global 600/menit yang ada dipakai apa adanya; limiter per-source non-goal.
- Body limit 5 MB existing cukup untuk batch Alertmanager.

**2.2 Parsing payload** (Alertmanager v4)

- `alerts[]` diproses **satu per satu, satu transaksi per alert** (tidak all-or-nothing). `commonLabels`/`commonAnnotations` menjadi fallback per alert.
- `fingerprint` absen → sha256 dari label terurut (deterministik).
- `status` per alert: `firing` | `resolved`.

**2.3 Klasifikasi** (per alert)

- Service: `service_map[labels[service_label_key]]` → `fallback_service_id` → null.
- Priority: `severity_map[labels[severity_label_key]]` → `default_priority` (`urgent|high|medium|low|none`).
- Title: `annotations.summary` → `labels.alertname` → `"Alert"`. Deskripsi: `annotations.description` + baris `generatorURL`.
- Type: `source.type_id` (default Incident). Bila type sudah tidak live (dihapus setelah source dibuat) → type null; item tetap dibuat, gate accept memblokir hingga triager memilih type.

**2.4 Dedup & resolve** — cari issue by `(intake_source_id, intake_fingerprint)`:

| Kondisi item                                  | Event `firing`                                                                               | Event `resolved`                                                         |
| --------------------------------------------- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| belum ada                                     | create issue + intake pending, `occurrence_count=1`, service link bila terpetakan            | ignore (counter `ignored`)                                               |
| pending (`-2`)                                | `occurrence_count+1`, `last_seen=now`                                                        | auto-decline (`-1`)                                                      |
| snoozed (`0`)                                 | update metadata saja                                                                         | biarkan (tidak menyentuh aksi triager)                                   |
| declined (`-1`)                               | update metadata + kembali pending (`-2`)                                                     | no-op                                                                    |
| duplicate (`2`)                               | update metadata saja                                                                         | no-op                                                                    |
| accepted (`1`), state issue open              | update metadata                                                                              | set state `completed` pertama project + `completed_at`, comment INTERNAL |
| accepted (`1`), state `completed`/`cancelled` | update metadata + reopen ke state default project (`resolve_issue_state`) + comment INTERNAL | no-op                                                                    |

- Pencarian series memfilter `issues.deleted_at IS NULL`; issue yang dihapus manual → event berikutnya membuat item baru.
- Hitungan di respons saling eksklusif per alert (satu alert masuk tepat satu kategori).
- Project tanpa state `completed` → resolve di-skip + WARN (item dibiarkan open), bukan error.
- Service link: saat create, mapping yang ada langsung di-insert ke `service_issues` (bukan suggestion). Mapping kosong → biarkan null, triager isi manual.

**2.5 Auto-accept per source** (default off)

- Syarat: `auto_accept = true`, event firing, item pending, dan gate accept lulus (`accept_gate`: type ada + service bila `requires_service`).
- Eksekusi memakai **helper accept bersama** yang diekstrak dari `patch_issue` (status → 1 + pindah issue keluar dari triage) agar PATCH manual dan ingest tidak divergen.
- Klasifikasi tidak lengkap → item tetap pending + WARN; tidak ada accept parsial.

**2.6 Respons & error**

- Sukses → `202 {created, updated, reopened, accepted, declined, resolved, ignored}`.
- JSON malformed / `alerts` bukan array → 400.
- Error internal (DB) pada alert mana pun → **500** agar Alertmanager me-retry seluruh batch; idempotensi dijamin upsert fingerprint (retry aman, tidak menggandakan).
- Label tak dikenal / mapping kosong bukan error — fallback/null.

**2.7 Konsekuensi lain**

- Jev **tidak** di-queue untuk item webhook (mapping deterministik; hemat AI & hindari saran yang melawan mapping).
- Intake default project dipakai; bila hilang, buat ulang dengan SQL yang sama seperti create project (`project.rs:1053`), bukan 404.
- `created_by_id`/`updated_by_id` item = user pembuat source (`intake_sources.created_by_id`); helper `insert_issue` yang ada mewajibkan user, dan atribusi asli tetap terlihat dari badge source.

### 3. CRUD source (auth)

Semua route terautentikasi, scope project, mirror pola intake lain (`missing()`/`ws_role`):

| Method & path                                                                     | Fungsi                         |
| --------------------------------------------------------------------------------- | ------------------------------ |
| `GET /api/workspaces/:slug/projects/:project_id/intake-sources/`                  | list (deleted_at null)         |
| `POST /api/workspaces/:slug/projects/:project_id/intake-sources/`                 | create (generate token)        |
| `GET/PATCH/DELETE /api/workspaces/:slug/projects/:project_id/intake-sources/:pk/` | detail / update / soft delete  |
| `POST /api/workspaces/:slug/projects/:project_id/intake-sources/:pk/rotate/`      | token baru, kembalikan `token` |

Validasi:

- `name` wajib non-kosong ≤ 255.
- `type_id` wajib, harus type live non-epic yang ter-link ke project (`project_issue_types`); default UI Incident.
- `config`: `service_map`/`fallback_service_id` harus service live milik project; nilai `severity_map`/`default_priority` ∈ {urgent, high, medium, low, none}; label key non-kosong.
- Respons snake_case seperti endpoint lain; `token` ikut di list/detail (plaintext, untuk membentuk URL).

### 4. UI

**Halaman baru**: project settings → _Intake sources_ (`settings/projects/[projectId]/intake-sources/`), pola work-item-types (header + page + entri sidebar settings).

- **List**: nama, badge aktif/nonaktif, badge auto-accept, type, tanggal; aksi salin webhook URL, rotate (konfirmasi; token lama mati), edit, toggle aktif, hapus (soft).
- **Create modal**: nama, type (default Incident), toggle auto-accept.
- **Mapping editor** (modal edit): key label service (default `service`) + baris nilai→service (select dari services project, tambah/hapus) + fallback service; key label severity (default `severity`) + baris nilai→priority (prefill critical/warning/info) + default priority. Validasi klien: baris ganda ditolak, service harus dipilih.
- **URL**: absolut dibentuk di FE dari `VITE_API_BASE_URL` + `/api/inbound/alertmanager/<token>/`.

**Di item intake**: badge `via <nama source>` + jumlah kejadian + terakhir terlihat di detail. Transisi reopen/auto-resolve pada issue accepted dicatat sebagai comment INTERNAL. Tidak ada push realtime.

**Data layer FE**: tipe `TIntakeSource` (`packages/types/src/intake-source.ts`), service + MobX store CRUD baru, reuse services store untuk select, i18n di `workspace-settings.json` (+ label source di `inbox.json` bila perlu).

### 5. Testing

**Rust integration** (`apps/api-rs/crates/api/tests/inbound_intake_test.rs`, DB-backed serial seperti `intake_triage_test.rs`):

1. Token valid → item pending, type/service/priority terpetakan, title dari annotation, service link terpasang.
2. Token salah → 404; source nonaktif → 403.
3. Dedup: firing kedua → `occurrence_count` naik, tidak ada issue baru.
4. Resolved saat pending → declined; resolved lagi → no-op.
5. Accepted + resolved → state completed; refire setelah completed → reopen ke state default.
6. Declined + refire → pending lagi.
7. Auto-accept: klasifikasi lengkap → accepted; service tak terpetakan → tetap pending.
8. Batch campuran firing/resolved + fallback `commonLabels`/`commonAnnotations` + fingerprint absen.
9. CRUD: validasi mapping menolak service asing, rotate mematikan token lama.
10. Retry idempotensi: payload yang sama dikirim dua kali tidak menggandakan.

**Unit**: origin exemption path; fingerprint hash; severity mapping. **Web**: tes helper murni (validasi mapping, builder URL). **Manual E2E**: curl payload Alertmanager ke localhost:8000 → item muncul, resolved menutup, refire membuka.

### 6. Observability

`tracing` WARN: service label tak dikenal (nilai label + source id), project tanpa state completed saat resolve, auto-accept dengan klasifikasi tak lengkap. Respons batch memuat hitungan per kategori; error per-alert di-log dengan fingerprint.

## File yang disentuh (ringkas)

- `apps/api-rs/migrations/0015_intake_sources.sql` (baru)
- `apps/api-rs/crates/api/src/routes/inbound.rs` (baru — ingest publik)
- `apps/api-rs/crates/api/src/routes/intake_source.rs` (baru — CRUD)
- `apps/api-rs/crates/api/src/routes/intake.rs` (helper accept bersama, detail memuat source)
- `apps/api-rs/crates/api/src/routes/mod.rs`, `main.rs` (mount + modul)
- `apps/api-rs/crates/api/src/middleware/origin.rs` (pengecualian `/api/inbound/`)
- `apps/api-rs/crates/api/tests/inbound_intake_test.rs`, `intake_source_routes_test.rs` (baru)
- `packages/types/src/intake-source.ts` (baru), `packages/types/src/inbox.ts` (enum `WEBHOOK`, detail source)
- `apps/web/core/services/intake/` + `apps/web/core/store/` (store CRUD)
- `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/projects/[projectId]/intake-sources/` (halaman + modal + sidebar)
- `apps/web/core/components/inbox/` (badge source + occurrence)
- `packages/i18n/src/locales/en/workspace-settings.json`, `inbox.json`
- `docs/features/intake.md` (bagian channel webhook + changelog)

## Changelog

| Date       | Change                                             |
| ---------- | -------------------------------------------------- |
| 2026-10-04 | Spec awal disetujui saat brainstorming (4 bagian). |
