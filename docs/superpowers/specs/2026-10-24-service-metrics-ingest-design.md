# External Service Metrics Ingest — Design

**Date:** 2026-10-24
**Status:** Approved design; pending written-spec review
**Scope:** `apps/api-rs` (Rust + migrasi sqlx), `apps/web`, `packages/types`, `packages/i18n`, docs.
**Related:** [`2026-10-24-service-health-real-design.md`](./2026-10-24-service-health-real-design.md), [`2026-10-04-intake-webhook-source-design.md`](./2026-10-04-intake-webhook-source-design.md), [`2026-10-21-service-detail-tabbed-pane-design.md`](./2026-10-21-service-detail-tabbed-pane-design.md).

## Context

Service catalog sudah workspace-level (`services`, `service_project_links`, dependencies, work-item links) dan health-nya diturunkan saat baca dari open Incident + war room aktif. Intake eksternal yang ada **hanya** untuk alert: `POST /api/inbound/alertmanager/:token/` membuat item triage Incident dengan mapping label → service/priority, fingerprint dedupe, dan auto-resolve. Belum ada penyimpanan metrik numerik sama sekali.

Tujuan fase ini: service catalog bisa menerima **metrik monitoring eksternal yang sudah teragregasi per bucket** (1m/5m/10m/30m/1h), menyimpannya, dan menampilkannya di detail service.

## Keputusan yang dikunci

1. **Batas domain** — metrik numerik langsung ke service catalog; alert tetap ke intake. Intake adalah antrean triage manusia, metrik high-volume tanpa workflow accept/decline.
2. **Jalur ingest** — push JSON pre-agregat via HTTP. Plane tidak pull dan tidak menerima remote-write.
3. **Identitas** — satu seri per `service + metric`; tanpa dimensi/labels.
4. **Statistik per bucket** — `count`, `sum`, `min`, `max`, `avg`, `last`, semuanya opsional tetapi minimal satu terisi.
5. **Rollup** — pengirim menghitung semua resolusi yang dikirim. Plane tidak melakukan rollup.
6. **Resolusi** — `60`, `300`, `600`, `1800`, `3600` detik.
7. **Retensi** — bertingkat: 1m 48 jam, 5m 14 hari, 10m 30 hari, 30m 90 hari, 1h 1 tahun.
8. **Source** — workspace-level, token per integrasi, mapping key eksternal → service id.
9. **Katalog metrik** — auto-register saat ingest pertama; admin bisa edit/hapus.
10. **Konflik** — upsert per `(metric, resolution, bucket_start)`, kiriman terakhir menang.
11. **UI fase 1** — tab **Monitoring** di pane detail service; list/board/health tidak berubah.
12. **Health** — metrik tidak mengubah status health di fase ini.

## Non-goals

- Threshold, SLO, alerting, atau perubahan status health dari metrik.
- Korelasi alert intake ↔ metrik.
- Labels/dimensi bebas (endpoint, region, instance).
- Penyimpanan sampel mentah; hanya bucket teragregasi.
- Pull dari Prometheus/VictoriaMetrics dan Prometheus remote-write.
- Adapter per-vendor (Grafana, Zabbix, Datadog); kontrak JSON bersifat generik.
- Rate limit per-source, HMAC signature, IP allowlist.
- Retensi yang bisa diatur per source.
- Sparkline/kolom metrik di list/board/graph.
- Partisi tabel dan downsampling server-side.

## Arsitektur

```
Agent eksternal
  │  POST /api/inbound/metrics/:token/   (JSON pre-agregat)
  ▼
api-rs
  ├─ validasi token + seluruh batch (all-or-nothing)
  ├─ resolve service (service_id → service_map → fallback)
  ├─ auto-register service_metrics (first non-empty unit/display_name)
  └─ upsert service_metric_buckets (last write wins)
        │
        ▼
Postgres
        │  GET .../services/:pk/metrics[/:metric_id/buckets/]
        ▼
Web — tab Monitoring (Recharts)

beat 03:45 ──► worker: cleanup.service_metric_buckets (hapus per resolusi)
```

Semua tabel baru milik api-rs; tidak ada perubahan model/migrasi Django.

## Data model

Migrasi baru: `apps/api-rs/migrations/0024_service_metrics.sql`. Diterapkan otomatis saat boot oleh `common::db::migrate`.

### `metric_sources`

```sql
CREATE TABLE IF NOT EXISTS public.metric_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    name character varying(255) NOT NULL,
    token character varying(64) NOT NULL,
    is_active boolean NOT NULL DEFAULT true,
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_by_id uuid,
    updated_by_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS metric_sources_token_uniq
    ON public.metric_sources (token) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS metric_sources_name_uniq
    ON public.metric_sources (workspace_id, lower(btrim(name))) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS metric_sources_workspace_idx
    ON public.metric_sources (workspace_id) WHERE deleted_at IS NULL;
```

- Token dibuat server-side: `plane_ms_` + 32 hex (`replace(gen_random_uuid()::text, '-', '')`), pola sama dengan `intake_sources`.
- Token disimpan plaintext (parity `intake_sources`); blast radius hanya ingest metrik.
- Soft delete menjaga provenance bucket lama.

`config` yang valid:

```json
{
  "service_map": { "payment-api": "<service uuid>" },
  "fallback_service_id": "<service uuid>|null"
}
```

- Hanya dua key itu; key lain ditolak.
- Maksimal 500 entri `service_map`; key 1–200 karakter, tidak boleh kosong; value harus UUID service live di workspace yang sama.
- `fallback_service_id` opsional, harus service live di workspace yang sama.

### `service_metrics`

```sql
CREATE TABLE IF NOT EXISTS public.service_metrics (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES public.workspaces(id) ON DELETE CASCADE,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    metric_key character varying(200) NOT NULL,
    display_name character varying(255) NOT NULL DEFAULT '',
    unit character varying(50) NOT NULL DEFAULT '',
    description text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS service_metrics_service_key_uniq
    ON public.service_metrics (service_id, metric_key);
CREATE INDEX IF NOT EXISTS service_metrics_workspace_idx
    ON public.service_metrics (workspace_id);
```

- Definisi per service, bukan global; unit/nama tampilan bisa berbeda antar service.
- Auto-register memakai `ON CONFLICT (service_id, metric_key)`.
- Aturan konflik: `display_name` dan `unit` hanya diisi bila nilai lama kosong (first non-empty wins). Kiriman berikutnya tidak menimpa metadata yang sudah ada.
- `description` hanya diubah lewat PATCH admin.

### `service_metric_buckets`

```sql
CREATE TABLE IF NOT EXISTS public.service_metric_buckets (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    service_id uuid NOT NULL REFERENCES public.services(id) ON DELETE CASCADE,
    metric_id uuid NOT NULL REFERENCES public.service_metrics(id) ON DELETE CASCADE,
    source_id uuid REFERENCES public.metric_sources(id) ON DELETE SET NULL,
    resolution smallint NOT NULL CHECK (resolution IN (60, 300, 600, 1800, 3600)),
    bucket_start timestamptz NOT NULL,
    count bigint CHECK (count IS NULL OR count >= 0),
    sum double precision,
    min double precision,
    max double precision,
    avg double precision,
    last_value double precision,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT service_metric_buckets_stats_present CHECK (
        count IS NOT NULL OR sum IS NOT NULL OR min IS NOT NULL OR max IS NOT NULL
        OR avg IS NOT NULL OR last_value IS NOT NULL
    ),
    CONSTRAINT service_metric_buckets_min_max CHECK (
        min IS NULL OR max IS NULL OR min <= max
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS service_metric_buckets_series_uniq
    ON public.service_metric_buckets (metric_id, resolution, bucket_start);
CREATE INDEX IF NOT EXISTS service_metric_buckets_service_time_idx
    ON public.service_metric_buckets (service_id, bucket_start DESC);
CREATE INDEX IF NOT EXISTS service_metric_buckets_retention_idx
    ON public.service_metric_buckets (resolution, bucket_start);
```

- Satu seri = `(metric_id, resolution, bucket_start)`; `service_id` denormalized untuk query dan scoping.
- `source_id` hanya provenance penulis terakhir, bukan bagian identitas. Jika dua source mengirim metrik yang sama, yang terakhir menang.
- `avg` disimpan apa adanya; tidak dihitung ulang dari `sum/count`.
- Nilai non-finite (NaN/Infinity) ditolak di validasi; JSON tidak membawa NaN.

## Ingest API

### Endpoint

`POST /api/inbound/metrics/:token/`

Tanpa session. Path `/api/inbound/` sudah dikecualikan origin middleware, jadi tidak perlu perubahan middleware.

### Payload

```json
{
  "metrics": [
    {
      "service": "payment-api",
      "service_id": null,
      "key": "http.server.duration.p95",
      "display_name": "HTTP latency p95",
      "unit": "ms",
      "resolution": 60,
      "bucket_start": "2026-10-24T10:00:00Z",
      "count": 60,
      "sum": 7200,
      "min": 80,
      "max": 250,
      "avg": 120,
      "last": 130
    }
  ]
}
```

| Field                  | Wajib      | Aturan                                                                                                |
| ---------------------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| `metrics`              | ya         | array 1–1000 item                                                                                     |
| `service_id`           | salah satu | UUID service live di workspace source; menang atas `service`                                          |
| `service`              | salah satu | key eksternal 1–200 karakter, di-resolve via `config.service_map` → `fallback_service_id`             |
| `key`                  | ya         | `^[A-Za-z0-9][A-Za-z0-9._:-]{0,199}$`                                                                 |
| `display_name`         | tidak      | ≤255 karakter, di-trim; string kosong dianggap tidak ada                                              |
| `unit`                 | tidak      | ≤50 karakter, di-trim; string kosong dianggap tidak ada                                               |
| `resolution`           | ya         | salah satu `60/300/600/1800/3600`                                                                     |
| `bucket_start`         | ya         | RFC3339; harus align ke resolusi; tidak >5 menit di masa depan; tidak lebih tua dari retensi resolusi |
| `count`                | tidak      | integer ≥ 0                                                                                           |
| `sum/min/max/avg/last` | tidak      | angka finite; minimal satu statistik terisi                                                           |

Validasi tambahan:

- `min <= max`.
- Bila `avg` dan `min`/`max` ada: `avg` harus dalam `[min, max]` dengan toleransi `1e-9`.
- `bucket_start` align: `unix_timestamp % resolution == 0` (pakai `rem_euclid`).
- Service yang di-resolve harus live (`services.deleted_at IS NULL`) dan berada di workspace source.
- Jika `service_id` dan `service` sama-sama ada, `service_id` yang dipakai; `service` diabaikan.

### Semantik batch

- **All-or-nothing**: seluruh batch divalidasi lebih dulu. Jika ada satu item invalid, tidak ada baris yang ditulis dan respons `400` berisi daftar error.
- Setelah valid, satu transaksi:
  1. bulk auto-register definisi metrik,
  2. bulk upsert bucket.
- Upsert `ON CONFLICT (metric_id, resolution, bucket_start) DO UPDATE` mengganti **seluruh** statistik (termasuk null), `source_id`, dan `ingested_at`.
- Retry agent aman dan idempoten.

### Respons

| Kondisi                              | Status | Body                                                                    |
| ------------------------------------ | ------ | ----------------------------------------------------------------------- |
| Sukses                               | `202`  | `{"accepted": N, "metrics_created": N}`                                 |
| Token tidak dikenal / source deleted | `404`  | `{"error": "Unknown token"}`                                            |
| Source inactive                      | `403`  | `{"error": "Metric source is inactive"}`                                |
| Payload invalid                      | `400`  | `{"errors": [{"index": 0, "field": "bucket_start", "message": "..."}]}` |
| DB error                             | `500`  | `{"error": "..."}`                                                      |

`metrics_created` = jumlah definisi `(service_id, key)` yang baru dibuat oleh request ini. Error validasi dikembalikan untuk semua item yang gagal (maksimal 50 error, disertai `"truncated": true` bila dipotong).

### Contoh

```bash
curl -X POST "$API/api/inbound/metrics/plane_ms_<token>/" \
  -H 'Content-Type: application/json' \
  -d '{
    "metrics": [{
      "service": "payment-api",
      "key": "http.server.duration.p95",
      "unit": "ms",
      "resolution": 60,
      "bucket_start": "2026-10-24T10:00:00Z",
      "count": 60,
      "avg": 120,
      "max": 250
    }]
  }'
```

## Source management API

Session + workspace admin. Non-admin → `403` dengan pesan permission yang sudah dipakai service API.

| Method   | Path                                               | Fungsi                               |
| -------- | -------------------------------------------------- | ------------------------------------ |
| `GET`    | `/api/workspaces/:slug/metric-sources/`            | list (token disertakan)              |
| `POST`   | `/api/workspaces/:slug/metric-sources/`            | create + generate token              |
| `PATCH`  | `/api/workspaces/:slug/metric-sources/:id/`        | `name`, `is_active`, `config`        |
| `DELETE` | `/api/workspaces/:slug/metric-sources/:id/`        | soft delete                          |
| `POST`   | `/api/workspaces/:slug/metric-sources/:id/rotate/` | token baru; token lama langsung mati |

Respons source:

```json
{
  "id": "...",
  "workspace_id": "...",
  "name": "Prometheus production",
  "token": "plane_ms_...",
  "is_active": true,
  "config": { "service_map": {}, "fallback_service_id": null },
  "created_by": "...",
  "created_at": "...",
  "updated_at": "..."
}
```

Validasi:

- `name` wajib, 1–255 karakter, unik per workspace (case-insensitive, trim).
- `config` divalidasi seperti di atas; service harus live dan satu workspace.
- Rotate hanya untuk source live; source inactive tetap bisa dirotasi.
- Delete soft; bucket lama tetap ada (`source_id` menjadi tidak relevan tapi tidak ikut terhapus).

## Read API

Session. Akses memakai `gate_member` + service visibility yang sudah ada: workspace admin melihat semua service; member hanya service di `visible_service_ids` (owner/consumer project yang bisa diakses). Guest/non-member → `403`.

| Method   | Path                                                             | Fungsi                                             |
| -------- | ---------------------------------------------------------------- | -------------------------------------------------- |
| `GET`    | `/api/workspaces/:slug/services/:pk/metrics/`                    | daftar definisi + ringkasan bucket terakhir        |
| `GET`    | `/api/workspaces/:slug/services/:pk/metrics/:metric_id/buckets/` | deret bucket                                       |
| `PATCH`  | `/api/workspaces/:slug/services/:pk/metrics/:metric_id/`         | edit `display_name`, `unit`, `description` (admin) |
| `DELETE` | `/api/workspaces/:slug/services/:pk/metrics/:metric_id/`         | hapus definisi + bucket (admin)                    |

### Daftar metrik

```json
{
  "metrics": [
    {
      "id": "...",
      "service_id": "...",
      "key": "http.server.duration.p95",
      "display_name": "HTTP latency p95",
      "unit": "ms",
      "description": "",
      "last_bucket": {
        "bucket_start": "2026-10-24T10:59:00Z",
        "resolution": 60,
        "count": 60,
        "sum": 7200,
        "min": 80,
        "max": 250,
        "avg": 120,
        "last": 130
      }
    }
  ]
}
```

- Urut `metric_key` ascending.
- `last_bucket` = bucket dengan `bucket_start` terbaru lintas resolusi; `null` bila belum ada.
- Implementasi: `LEFT JOIN LATERAL (... ORDER BY bucket_start DESC LIMIT 1)`.

### Deret bucket

Query params:

| Param        | Default     | Aturan                                                |
| ------------ | ----------- | ----------------------------------------------------- |
| `from`       | `now - 24h` | RFC3339                                               |
| `to`         | `now`       | RFC3339, harus > `from`                               |
| `resolution` | auto        | salah satu resolusi; harus tersedia untuk rentang tsb |

Rentang maksimum 90 hari. Resolusi eksplisit ditolak (`400`) bila `from` lebih tua dari retensi resolusi tersebut.

Auto-resolution:

| Rentang   | Resolusi |
| --------- | -------- |
| ≤ 6 jam   | `60`     |
| ≤ 24 jam  | `300`    |
| ≤ 7 hari  | `600`    |
| ≤ 30 hari | `1800`   |
| ≤ 90 hari | `3600`   |

Respons:

```json
{
  "metric": { "id": "...", "key": "...", "display_name": "...", "unit": "ms", "description": "" },
  "resolution": 300,
  "from": "2026-10-23T10:00:00Z",
  "to": "2026-10-24T10:00:00Z",
  "buckets": [
    {
      "bucket_start": "2026-10-24T09:55:00Z",
      "count": 60,
      "sum": 7200,
      "min": 80,
      "max": 250,
      "avg": 120,
      "last": 130
    }
  ]
}
```

- Bucket kosong tidak dikirim; UI yang mengisi gap.
- `Cache-Control: no-store` (konsisten dengan health).
- `metric_id` harus milik `:pk`; jika tidak → `404`.

## UI

### Tab Monitoring di detail service

- Tambah tab keempat `monitoring` pada pane (`paneTab=monitoring`), setelah `tickets`:
  - `apps/web/core/components/services/detail/pane/helpers.ts` — tambah key + label `service.tabs.monitoring`.
  - `.../tab-panels/monitoring.tsx` — panel baru.
  - `.../tab-panels/root.tsx` — render panel.
- Komponen baru di `apps/web/core/components/services/metrics/`:
  - `monitoring-panel.tsx` — orchestration selector + chart + summary.
  - `metric-selector.tsx` — daftar `display_name || key` + unit.
  - `range-selector.tsx` — `1h / 6h / 24h / 7d / 30d / 90d`.
  - `metric-chart.tsx` — Recharts.
  - `metric-summary.tsx` — `last/avg/min/max/count`.
  - `helpers.ts` + `helpers.test.ts` — pemilihan resolusi, formatting, gap filling.
- State: `selectedMetricId` dan `range` adalah local state; default metric = definisi pertama yang punya `last_bucket`, else yang pertama; default range `24h`.
- Chart:
  - `ComposedChart` dengan garis `avg` (solid) dan `max` (dashed); tooltip menampilkan semua statistik + `bucket_start` lokal.
  - Helper membangun timeline kontinu per resolusi dan menyisipkan `null` untuk bucket yang hilang; `connectNulls={false}` agar gap terlihat.
  - Sumbu Y memakai unit metrik; sumbu X format waktu menyesuaikan range.
- Empty state:
  - Belum ada definisi: "No metrics received yet" + tombol ke settings source (hanya admin).
  - Ada definisi tapi rentang kosong: "No data in this range".
- Error state: pesan + retry; kegagalan metrik tidak memblokir tab lain.
- Mobile: pane full-width di bawah deskripsi; selector/range stack; tinggi chart ±220px.

### Data fetching

- Tipe baru di `packages/types/src/service/metrics.ts` (export dari `packages/types/src/service/index.ts`).
- `apps/web/core/services/service-metrics.service.ts` — HTTP client untuk definitions, buckets, source CRUD, rotate.
- `apps/web/core/hooks/use-service-metrics.ts` — SWR key `service+metric+from/to+resolution`, refresh 60 detik saat tab aktif, refetch on focus.
- Tidak menyentuh store health/service yang ada.

### Workspace settings — metric sources

- Route baru: `/[workspaceSlug]/settings/metric-sources` (kategori `SERVICE_MANAGEMENT`, admin-only), mengikuti pola halaman Webhooks.
- Daftarkan item `metric_sources` di `packages/constants/src/settings/workspace.ts` (`WORKSPACE_SETTINGS` + `GROUPED_WORKSPACE_SETTINGS`) dengan akses `ADMIN`.
- File:
  - `apps/web/app/(all)/[workspaceSlug]/(settings)/settings/(workspace)/metric-sources/{page,header}.tsx`
  - `apps/web/core/components/services/metrics/sources/*`
- Daftar: nama, status aktif, jumlah service ter-mapping, token masked + reveal/copy, dibuat oleh, aksi `Rotate / Disable / Delete`.
- Modal create/edit: nama, baris mapping `label eksternal → service` (service picker workspace), fallback service opsional, toggle aktif.
- Snippet `curl` siap copy dengan URL ingest + token + contoh payload.
- Rotate dan delete memakai modal konfirmasi; non-admin melihat `NotAuthorizedView`.

### i18n

- `service.tabs.monitoring`
- `service.monitoring.*` (judul, selector, range, summary, empty/error, unit fallback)
- `workspace_settings.settings.metric_sources.*` (judul, deskripsi, form, aksi, token, snippet)
- `en` ditulis pertama, lalu 19 locale lewat skill `translate`; `generate:types` wajib.
- Namespace lain tidak disentuh.

### Tidak berubah

- List/board, filter, grouping, layout toggle, health cell, graph, dan peek.

## Retensi & operasional

### Job retensi

- Beat: `cleanup.service_metric_buckets`, cron `0 45 3 * * *` (03:45 harian).
- Worker: `handlers::cleanup::service_metric_buckets(pool)`.
- Tambahkan job ke `is_enabled_job` dan arm `handle_by_id` (saat ini allowlist hanya job AI; cleanup lain belum aktif).
- Hapus per resolusi dengan cutoff:

| Resolusi | Retensi  |
| -------- | -------- |
| 60       | 48 jam   |
| 300      | 14 hari  |
| 600      | 30 hari  |
| 1800     | 90 hari  |
| 3600     | 365 hari |

- Hapus bertahap: `DELETE ... WHERE id IN (SELECT id FROM ... WHERE resolution = $1 AND bucket_start < $2 LIMIT 10000)`, ulangi sampai 0 atau batas putaran; log jumlah per resolusi.
- Definisi `service_metrics` tidak dihapus oleh retensi.

### Operasional

- Ingest satu transaksi per batch; error DB → 500 agar agent retry.
- Trace log: `source_id`, `accepted`, `metrics_created`, durasi. Nilai metrik **tidak** dicatat.
- Rate limit: limiter global 600/menit yang ada; batch 1000 item dihitung satu request.
- Body limit 5 MB yang ada cukup (1000 item ±300 KB).
- Timezone: `bucket_start` UTC; UI memformat lokal. Penyimpangan >5 menit ke depan ditolak.
- Estimasi storage: ±24 ribu baris per metric per service untuk seluruh retensi (2.880 + 4.032 + 4.320 + 4.320 + 8.760). 100 service × 20 metric ≈ 48 juta baris; masih wajar dengan index di atas. Jika tumbuh, opsi berikutnya partisi bulanan pada `bucket_start`.
- Backup PG yang ada otomatis mencakup tabel baru; ukuran dibatasi retensi.

## Permission matrix

| Aksi                       | Workspace admin | Member              | Guest      |
| -------------------------- | --------------- | ------------------- | ---------- |
| Ingest via token           | ✅ (token)      | ✅ (token)          | ✅ (token) |
| List/rotate/delete source  | ✅              | ❌                  | ❌         |
| Lihat metrik service       | ✅ semua        | ✅ visible services | ❌         |
| Edit/hapus definisi metrik | ✅              | ❌                  | ❌         |

Token ingest tidak terikat role; siapa pun yang memegang token aktif bisa mengirim.

## Testing

### Rust

`apps/api-rs/crates/api/tests/service_metrics_test.rs` (DB-backed, `--test-threads=1`):

- ingest via `service_map`, `fallback_service_id`, dan `service_id` langsung;
- auto-register definisi + first non-empty `unit`/`display_name`;
- upsert last-write-wins, termasuk menghapus statistik dengan `null`;
- all-or-nothing saat satu item invalid;
- validasi alignment, future skew, too old, `min > max`, `avg` di luar rentang;
- token unknown/inactive;
- read scoping admin vs member (`visible_service_ids`);
- auto-resolution dan penolakan resolusi yang tidak tersedia untuk rentang;
- `metric_id` bukan milik service → 404.

Unit test di `routes/service_metrics.rs`:

- validasi payload (key, unit, statistik, alignment);
- perhitungan cutoff retensi;
- pemilihan resolusi otomatis.

### Web

Vitest:

- `service-metrics.service.test.ts` — URL/query params definitions, buckets, source CRUD, rotate.
- `metrics/helpers.test.ts` — auto-resolution, formatting nilai/unit, gap filling timeline.
- Tidak ada component test baru; verifikasi visual lewat smoke.

### Smoke manual

1. Buat source di settings, copy token.
2. Kirim dua bucket via curl (1m dan 5m), lalu kirim ulang salah satu dengan nilai berbeda.
3. Buka detail service → tab Monitoring → pilih metric/range; chart dan summary sesuai.
4. Rotate token → request dengan token lama `404`.
5. Non-admin membuka settings → `NotAuthorizedView`.
6. Jalankan job retensi manual di lingkungan test; bucket lama terhapus, definisi tetap ada.

### Perintah verifikasi

```bash
# Rust
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test service_metrics_test -- --test-threads=1

# Web
pnpm --filter=web check:types
pnpm --filter=web check:lint
pnpm --filter=web test -- core/services/service-metrics.service.test.ts \
  core/components/services/metrics/helpers.test.ts
pnpm --filter=web build
```

## Rollout

- Migrasi additive, tanpa backfill; diterapkan saat boot.
- Deploy: rebuild `api`, `worker`, `beat-worker` (Docker), lalu build web + restart prod.
- Tidak ada feature flag; tab Monitoring menampilkan empty state sampai ingest pertama.
- Intake/alert, health, list, dan graph yang ada tidak berubah.

## Risiko & mitigasi

| Risiko                                           | Mitigasi                                                                                                                             |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| Dua source mengirim metrik sama → saling menimpa | Didokumentasikan; `source_id` menyimpan penulis terakhir. Jika perlu multi-source, tambah `source_id` ke unique key di fase berikut. |
| Typo metric key membuat katalog kotor            | Admin bisa edit `display_name`/`unit` atau hapus definisi (cascade bucket).                                                          |
| Token di URL tercatat di log proxy               | Parity intake; HMAC/header auth ditunda. Token hanya bisa ingest metrik.                                                             |
| Storage tumbuh                                   | Retensi bertingkat + index; opsi partisi bulanan bila volume naik.                                                                   |
| Data telat/duplikat                              | Upsert idempoten; validasi jendela retensi dan skew.                                                                                 |
| Metrik berhenti masuk tanpa alert                | UI menampilkan `last_bucket.bucket_start`; alerting metrik di luar scope.                                                            |
