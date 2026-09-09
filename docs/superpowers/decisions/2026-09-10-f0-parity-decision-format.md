# ADR F0-1: Parity decision format (schema v2)

Date: 2026-09-10
Status: accepted

## Context

`parity-inventory.json` v1 hanya punya `implemented|missing|shape_mismatch|constraint_mismatch`.
Akibatnya temuan "Django bug, Rust sengaja beda" terpaksa dicatat sebagai `shape_mismatch`
(TODO) atau di-flip diam-diam ke `implemented` (metrik bohong).

## Decision

Schema v2 tambah dua status keputusan:

- `deviation_accepted`: route ADA di Rust, perilaku SENGAJA beda dari Django
  (bug-for-bug tidak direplikasi). Syarat: `rust_handler` non-empty,
  terdaftar di `main.rs`, wajib `adr` + `notes` memuat `DECISION`.
- `stays_on_django`: SENGAJA tidak dibangun di Rust (LLM proxy, celery,
  legacy). Syarat: `rust_handler == ""`, tidak perlu route, wajib `adr`.

Gate:

- `implemented|shape_mismatch|constraint_mismatch|deviation_accepted` wajib terdaftar
  di `main.rs` (kecuali composite `:a-:b` + `out_scope=true`).
- `missing|stays_on_django` boleh tanpa route.
- Setiap `adr` pointer wajib resolve ke file nyata (dicek `decision_records_are_valid`).

## Consequences

- Metrik jujur: `implemented` = sama persis, `deviation_accepted` = beda disengaja,
  `shape_mismatch` = TODO beneran.
- Aturan flip ketat: entri campuran (bug + KEYS/GATE) TIDAK boleh flip sampai
  semua delta lain tuntas. F0 hanya migrasi entri murni.
