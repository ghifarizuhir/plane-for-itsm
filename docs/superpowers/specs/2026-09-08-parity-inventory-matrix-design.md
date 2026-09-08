# Parity Inventory Matrix Design — per-domain source of truth

**Date:** 2026-09-08
**Status:** Approved (user: JSON source of truth, semua route + FE evidence, JSON jadi baseline test + tripwire FE)
**Goal:** Parity tracking yang tidak drift — satu file JSON per-domain yang dibaca test CI, menggantikan `BASELINE` hardcoded di `route_parity_test.rs`, dan menangkap bukan hanya route yang hilang tapi juga disparitas _shape_ (fungsi API yang masih perlu ditambahkan).
**Scope:** Inventory backend parity (Django ↔ api-rs) ditambah FE evidence; tidak mengubah handler/route backend. Murni penataan tracking + gate.

---

## 1. Context

- Frontend (`apps/web` + `packages/`) terstruktur **per domain/feature**, bukan per page:
  - Services (API calls): `apps/web/core/services/*.service.ts` + `packages/services/*`
  - Stores (MobX): `apps/web/core/store/*`
  - Routes/pages (React Router): `apps/web/app/routes/core.ts` — lapisan consumer yang compose banyak domain.
- Backend api-rs juga **per domain**: `apps/api-rs/crates/api/src/routes/*` (cycle.rs, module.rs, issue\_\*.rs, project.rs, workspace.rs, dst) — mapping 1:1 dengan domain frontend.
- Tracking parity saat ini:
  - `apps/api-rs/crates/api/tests/route_parity_test.rs` — `BASELINE` const hardcoded (16 path Batch F), cek `main.rs` `.route()`.
  - `apps/api-rs/crates/api/tests/parity_gate_test.rs` — hitung `scripts/shadow.sh` paths (≥55).
  - `apps/api-rs/scripts/shadow.sh` — shadow test Django :8000 vs Rust :8001.
- Granularity yang benar: **per domain** (track), **per page** hanya untuk prioritisasi (user-visible impact). Per-page sebagai unit tracking salah karena satu halaman memanggil banyak domain.

## 2. Artefak

Satu file JSON source of truth: `apps/api-rs/crates/api/parity-inventory.json`.

- Diletakkan di root crate (`crates/api`) agar terbaca test via `CARGO_MANIFEST_DIR` (pola yang sudah dipakai `route_parity_test.rs`).
- Tidak ada markdown yang diedit manual; report markdown boleh di-generate dari JSON.
- Lokasi alternatif yang ditimbang dan ditolak: root `apps/api-rs/` (jauh dari test), `docs/` (tidak dibaca kode).

## 3. Schema

```json
{
  "schema_version": 1,
  "domains": {
    "issue": {
      "rust_module": "routes/issue_common.rs",
      "endpoints": [
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/projects/:project_id/issues/",
          "django_source": "app/urls/issue.py:64",
          "rust_status": "implemented",
          "rust_handler": "routes::issue_common::list_issues",
          "fe_evidence": [
            {
              "service": "apps/web/core/services/issue/issue.service.ts",
              "method": "getIssuesFromServer"
            }
          ],
          "fe_pages": ["issues-list", "issue-detail"],
          "batch_task": "Batch C T1",
          "out_scope": false,
          "notes": "bare-array tolerated (c17379e18)"
        }
      ]
    }
  }
}
```

Aturan field:

| Field           | Wajib | Nilai                                                                | Keterangan                                                               |
| --------------- | ----- | -------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `methods`       | ya    | `["GET", "POST", ...]`                                               | Semua method yang dilayani route ini                                     |
| `path`          | ya    | string kanonik Django (`:param`)                                     | Konsisten dengan `BASELINE` yang sudah ada                               |
| `django_source` | ya    | `app/urls/*.py:line`                                                 | Jejak audit ke Django                                                    |
| `rust_status`   | ya    | `implemented` / `missing` / `shape_mismatch` / `constraint_mismatch` | Beda status = beda penanganan                                            |
| `rust_handler`  | tidak | `routes::<mod>::<fn>`                                                | Nama handler di api-rs bila sudah ada                                    |
| `fe_evidence`   | tidak | array `{service, method}`                                            | Service + method (bukan page) karena service = unit API call yang stabil |
| `fe_pages`      | tidak | array string                                                         | Untuk prioritisasi saja, bukan unit tracking                             |
| `batch_task`    | tidak | string                                                               | Batch + task yang mengimplementasikannya                                 |
| `out_scope`     | ya    | boolean                                                              | Route yang sengaja tidak dimigrasi (Batch E §16)                         |
| `notes`         | tidak | string                                                               | Deviasi / perilaku khusus                                                |

Semantik `rust_status`:

- `implemented` — route ada dan shape sesuai Django.
- `missing` — route tidak ter-register di `main.rs` (404/405).
- `shape_mismatch` — route ada tapi shape JSON/response beda (fungsi API yang masih perlu ditambahkan/diperbaiki).
- `constraint_mismatch` — route ada tapi validasi/constraint beda (mis. T10 identifier).
- Route dengan `out_scope: true` tidak dihitung gagal di gate, tapi tetap dicatat agar inventory "lengkap".

## 4. Gate tests

Dua test Rust baru di `apps/api-rs/crates/api/tests/`, menggantikan peran `BASELINE` di `route_parity_test.rs`:

### 4.1 `route_inventory_test.rs`

- Baca `parity-inventory.json`.
- Refactor logika ekstraksi `rust_routes()` (dari `route_parity_test.rs`) menjadi helper bersama yang dipakai kedua test.
- Untuk tiap entry dengan `rust_status != "missing" && out_scope == false`: pastikan path ter-register di `main.rs`.
- Tripwire: path yang diklaim implemented tapi hilang dari router → CI gagal.
- `route_parity_test.rs` lama di-migrasi isinya (BASELINE Batch F) ke JSON, lalu file lama dihapus agar tidak dobel sumber kebenaran.

### 4.2 `fe_tripwire_test.rs`

- Untuk tiap entry dengan `fe_evidence`: verifikasi file service yang dirujuk ada.
- Normalisasi path: ganti `:param` di kanonik dengan wildcard dan `${...}` di template literal TS dengan wildcard; cek kecocokan substring antara URL FE dan `path` matrix.
- Tripwire: FE service memanggil endpoint yang tidak ada entry-nya di matrix → CI gagal (loud failure, perbaikan manual).
- Keterbatasan yang diterima: matching bersifat pendekatan karena template literal + `serviceType` dinamis; false negative mungkin, tapi arahnya selalu "loud", bukan silent.

## 5. Workflow batch berikutnya (Batch G+)

1. Pilih satu domain yang belum parity penuh (prioritas: yang paling sering dipanggil FE → `issue`, `project`, `cycle`, `workspace`).
2. Isi/update entry `rust_status` dari hasil audit + shadow test.
3. Satu task T# = satu entry `missing`/`shape_mismatch`/`constraint_mismatch`: implement, flip status di JSON, commit.
4. Gate test (`route_inventory_test` + `fe_tripwire_test`) harus tetap hijau.

## 6. Rollout (seeding)

1. Buat `parity-inventory.json` berisi schema + domain `issue` dengan 16 path baseline Batch F sebagai seed pertama.
2. Migrasi `route_parity_test.rs` → `route_inventory_test.rs` yang baca JSON; hapus BASELINE const.
3. Tambah `fe_tripwire_test.rs`; seed FE evidence untuk path yang sudah ada caller-nya (mis. `issue-links`/`issue-relation` → `issue.service.ts`).
4. Jalankan `cargo test -p api` → hijau.
5. Perluas per domain secara bertahap (prioritas FE-impact).

## 7. Testing

- `cargo test -p api --test route_inventory_test` — unit test ekstraksi path + baca JSON.
- `cargo test -p api --test fe_tripwire_test` — unit test normalisasi `${...}` ↔ `:param`.
- `cargo test -p api` — seluruh suite tetap hijau (termasuk parity gate lama).
- `bash apps/api-rs/scripts/shadow.sh` — shadow test tidak berubah, tetap dipakai untuk verifikasi shape di runtime.

## 8. Out of scope

- Implementasi handler/route backend baru (di luar menyalakan status di JSON).
- Auto-scan script yang mengekstrak route dari Django urls / main.rs / FE services secara penuh (ditunda; cukup seeding manual + tripwire untuk saat ini).
- Report UI/dashboard parity (markdown/HTML generate menyusul bila dibutuhkan).
