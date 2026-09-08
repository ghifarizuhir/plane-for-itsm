# parity-audit.py — how to & untuk apa

Audit human-readable untuk `parity-inventory.json`: menjawab **"apa yang perlu
diperbaiki / ditambahkan"** sebelum merencanakan batch berikutnya.

## Untuk apa

| Laporan                 | Menjawab                                                                                         | Dipakai untuk                                       |
| ----------------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------------- |
| `[1] status per domain` | Endpoint mana yang `missing` / `shape_mismatch` / `constraint_mismatch`, beserta notes delta-nya | Bahan **Batch-fix** (satu task per mismatch)        |
| `[2] empty fe_evidence` | Entry mana yang belum ketahuan caller FE-nya                                                     | **Evidence mining** (grep service FE, isi evidence) |
| `[3] coverage`          | File Django urls mana yang path-nya belum masuk inventory                                        | **Expansion Batch H+** (satu batch per domain)      |

Bedanya dengan gate test (`route_inventory_test`, `fe_tripwire_test`): gate dipakai CI
untuk _mencegah drift_ (pass/fail). Script ini untuk _manusia merencanakan kerja_
(membaca daftar + notes).

## Cara menggunakan

```bash
# Laporan penuh (semua domain)
python3 apps/api-rs/scripts/parity-audit.py

# Fokus satu domain
python3 apps/api-rs/scripts/parity-audit.py --domain issue

# Tampilkan juga daftar path Django yang belum terinventaris (backlog expansion)
python3 apps/api-rs/scripts/parity-audit.py --coverage
python3 apps/api-rs/scripts/parity-audit.py --domain workspace --coverage
```

Exit code: `0` = tidak ada action item dalam scope; `1` = ada perbaikan/penambahan
yang outstanding (atau argumen salah, pesan error ke stderr).

## Cara membaca output (contoh ringkas)

```
== [1] status per domain (fix candidates) ==
issue: 39 entries (implemented=32, shape_mismatch=7)

[shape_mismatch] /api/.../issues/:pk/
  methods=GET,PATCH,DELETE handler=routes::work_item::get_issue src=app/urls/issue.py:59
  notes: GET returns {id,name} only (work_item.rs:1133) vs IssueDetailSerializer ...
```

- Baris `[status] path` = satu kandidat fix. `methods` = method yang dilayani Django
  (target paritas), `handler` = handler Rust saat ini, `src` = lokasi kontrak Django.
- `notes` = delta yang ditemukan saat audit (dengan `file:line` kedua sisi).
  Ini yang diubah menjadi task Batch-fix.

```
== [2] empty fe_evidence (evidence-mining candidates) ==
issue: /api/.../versions/  [GET]
```

- Entry tanpa caller FE yang tercatat. Cara mengisi: `grep -rn "<segmen-khas>" apps/web/core/services packages/services/src`,
  verifikasi template URL-nya cocok dengan path, lalu tambah ke `fe_evidence`.

```
== [3] Django-vs-inventory coverage (expansion candidates) ==
issue.py: 40/40 inventoried
workspace.py: 2/41 inventoried
```

- `x/y inventoried` per file Django urls. File dengan coverage parsial = kandidat
  batch expansion berikutnya. Dengan `--coverage`, tiap path yang hilang
  dicetak sebagai `MISSING <path>` (siap disalin jadi tabel plan).
- Catatan: beberapa file memang OUT of scope migrasi (`api.py`, `external.py`,
  `exporter.py`, `timezone.py`, ai-assistant, file-assets POST) — abaikan dari
  daftar MISSING sesuai keputusan Batch E §16, jangan dimasukkan ke inventory.

## Kapan dijalankan

- **Sebelum merencanakan batch** — untuk mengambil daftar fix/expansion.
- **Sesudah mengedit JSON** — pastikan entry baru tidak menambah action item
  yang tidak disengaja (atau justru menutup yang lama).
- **Opsional di CI** — exit code 1 = ada PR yang menambah gap; tetapi gate
  Rust test tetap menjadi penegak utama (script ini tidak menggantikannya).

## Cara extend

- Alias bentuk route (`ALIASES` di script): untuk path yang Axum tidak bisa
  ekspresikan persis seperti Django (contoh Batch F T10 `:ident`).
- Status baru: tambah ke `ACTION_STATUSES` bila skema inventory menambah status
  non-`implemented` baru.
- Sumber Django lain: `URLS_DIR` menunjuk `apps/api/plane/app/urls/`; file
  `__init__.py` dilewati otomatis.
