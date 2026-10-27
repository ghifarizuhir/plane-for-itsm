# Terraline — Dokumentasi

Terraline adalah **fork Plane** untuk **IT service management (ITSM)**. Workspace menjadi tenant, application mengelompokkan pekerjaan, service membentuk katalog bersama, dan ticket membawa incident, request, problem, change, serta improvement melalui praktik ITIL4.

Dokumen di folder ini menjelaskan implementasi yang ada di repo, bukan rencana jangka panjang. [`../README.md`](../README.md) adalah sumber kebenaran untuk positioning produk dan fitur yang sudah berjalan. Beberapa dokumen di `design/` dan `features/` masih berupa snapshot dari fase cutover Rust (September 2026) dan belum diperbarui untuk seluruh permukaan ITSM; statusnya ditandai di bawah.

## Status dokumen

| Dokumen                    | Status      | Catatan                                                          |
| -------------------------- | ----------- | ---------------------------------------------------------------- |
| `../README.md`             | ✅ Current  | Positioning produk, capability, arsitektur, self-hosting         |
| `PRODUCT.md`               | ⚠️ Snapshot | Masih menyebut ITSM sebagai backlog; perlu refresh               |
| `business-capabilities.md` | ⚠️ Snapshot | Potret kode saat cutover; perlu verifikasi ulang                 |
| `CHEAT-SHEET.md`           | ⚠️ Snapshot | Ringkasan keputusan; cek versi stack                             |
| `design/`                  | ⚠️ Snapshot | Arsitektur, data model, API, design system, testing, ops         |
| `features/`                | ⚠️ Sebagian | `war-rooms.md` dan `cmdb.md` sudah ada; fitur ITSM lain menyusul |
| `ui/`                      | ✅ Spec     | Token, shell, editor                                             |
| `superpowers/`             | 📚 Arsip    | Spec, plan, decision, dan smoke historis                         |

---

## Struktur

```text
docs/
├── README.md                   ← ini
├── PRODUCT.md                  ← platform, users, positioning, constraints
├── business-capabilities.md    ← snapshot kode (apps/, packages/)
├── CHEAT-SHEET.md              ← rangkuman keputusan (stack, layout, auth, URL, testing)
├── linting.md                  ← aturan lint/format
│
├── design/                     ← engineering cross-cutting (kontrak hidup)
│   ├── README.md               ← reading order & status
│   ├── 01-architecture.md      # monorepo pnpm+turbo, 7 apps, 15 packages
│   ├── 02-data-model.md        # Postgres per-table + ITSM extensions
│   ├── 03-api-contract.md      # Rust Axum + X-Api-Key/Bearer
│   ├── 04-design-system.md     # @plane/ui + tailwind-config + editor
│   ├── 05-testing-strategy.md  # cargo test + pytest baseline + vitest
│   └── 06-ops-runbook.md       # docker-compose + deployments + env
│
├── features/                   ← product per halaman + shared
│   ├── README.md               ← inventory + template
│   ├── _backlog.md             ← ide yang diparkir
│   ├── _shared/                ← concern cross-cutting
│   └── <page>.md               ← 1 file per halaman
│
├── ui/                         ← spesifikasi komponen
│   ├── README.md
│   ├── design-tokens.md
│   ├── shell.md
│   └── editor.md
│
└── superpowers/                ← arsip desain & implementasi
    ├── decisions/
    ├── plans/
    ├── smoke/
    └── specs/
```

---

## Kapan baca yang mana

| Kamu adalah…                    | Mulai dari                                                                   |
| ------------------------------- | ---------------------------------------------------------------------------- |
| **New dev / onboarding**        | [`../README.md`](../README.md) → status dokumen di atas → `design/README.md` |
| **Sedang implementasi halaman** | `features/<page>.md` → `design/03-api-contract.md` untuk endpoint            |
| **Architect / reviewer**        | `design/*` penuh; cross-check `business-capabilities.md`                     |
| **Designer / UX**               | `design/04-design-system.md` + `ui/*` + `features/<page>.md`                 |
| **DevOps / infra**              | `design/06-ops-runbook.md` + [`../AGENTS.md`](../AGENTS.md)                  |
| **QA / test**                   | `design/05-testing-strategy.md` + `AGENTS.md` §Backend tests                 |

---

## Update policy

| Path           | Update kapan?                                 | Siapa boleh ubah?                      |
| -------------- | --------------------------------------------- | -------------------------------------- |
| `../README.md` | Positioning produk atau capability berubah    | Maintainer                             |
| `design/`      | Setiap keputusan arsitektur berubah           | Dengan review — kontrak hidup teknikal |
| `features/`    | Setiap perubahan spec produk                  | Dengan review — kontrak hidup produk   |
| `ui/`          | Setiap perubahan komponen shell/editor        | Dengan review                          |
| `superpowers/` | Arsip historis; tambah file baru, jangan edit | —                                      |

Perubahan kode di `apps/`/`packages/` wajib disertai update doc yang relevan dalam commit yang sama (toleransi: `docs/linting.md` untuk lint-only).

---

## Prinsip dokumentasi

1. **Design preskriptif, features per-halaman.** Concern terpisah, template konsisten (lihat `features/README.md`).
2. **Jujur soal status.** Stub / draft / deferred ditandai eksplisit.
3. **Referensi ke file + line-range**, bukan copy paste kode.
4. **Cross-reference link relatif** (`./design/01-architecture.md`).
5. **Content boundary tegas** — lihat `features/README.md` §Content Boundary.
6. **README root menang saat konflik.** Kalau dokumen di folder ini bertentangan dengan [`../README.md`](../README.md), README adalah sumber kebenaran dan dokumen tersebut perlu diperbarui.

---

## Changelog

| Date       | Change                                                                       |
| ---------- | ---------------------------------------------------------------------------- |
| 2026-09-03 | init — adaptasi dari terra `docs/README.md:2`                                |
| 2026-09-05 | cutover `rust-cutover-v1`: stack → Rust Axum                                 |
| 2026-10-27 | rapikan index: status snapshot, struktur aktual, dan sumber kebenaran README |
