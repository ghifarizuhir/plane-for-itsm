# Plane for ITSM — Dokumentasi

Plane for ITSM adalah **fork Plane** (project management open-source). **Actual-only:** docs ini snapshot Plane upstream (Work Items, Cycles, Modules, Views, Pages, Analytics) — disesuaikan ke stack Plane: **pnpm + Turbo + Rust Axum (API, sejak `rust-cutover-v1`) + React Router + MobX**. Skema data referensi Django (`apps/api`, fallback opt-in). Arah ITSM (incidents/problems/changes/Service Map) adalah future, diparkir di `features/_backlog.md`. Dokumentasi mengikuti pola `terra` (referensi) untuk struktur `design/` + `features/` + `ui/`, tapi isi actual Plane.

- **`PRODUCT.md`** — platform, users, capabilities, constraints (actual Plane upstream) — menjawab "produk ini untuk siapa & bisa apa?"
- **`business-capabilities.md`** — snapshot observasional berbasis kode (bukan rencana) — menjawab "apa yang benar-benar ada di `apps/` + `packages/`?"
- **`CHEAT-SHEET.md`** — 1-page rangkuman keputusan arsitektur + product — checklist sebelum coding
- **`design/`** — engineering cross-cutting (arsitektur, data model, API, design system, testing, ops) — menjawab "bagaimana kita bangun?"
- **`features/`** — product per halaman + cross-cutting interaksi — menjawab "apa yang user lihat & lakukan di halaman X?"
- **`ui/`** — spesifikasi teknis komponen shell & editor — menjawab "bagaimana komponen dirender?"

> Referensi asal: `../terra-service-management/docs/` — struktur `design/` + `features/` + `ui/` dicontek, isi diadaptasi ke Rust Axum/MobX/multi-app Plane. Jangan lock 1:1 (Terra = Express+Drizzle, Plane = Axum+Postgres).

---

## Struktur

```
docs/
├── README.md                   ← ini
├── PRODUCT.md                  ← platform, users, positioning, constraints
├── business-capabilities.md    ← snapshot kode (apps/api/plane, apps/web, packages/*)
├── CHEAT-SHEET.md              ← rangkuman keputusan (stack, layout, auth, URL, testing)
│
├── design/                     ← engineering cross-cutting (kontrak hidup)
│   ├── README.md               ← reading order & status
│   ├── 01-architecture.md      # monorepo pnpm+turbo, 6 apps, 15 packages
│   ├── 02-data-model.md        # Postgres per-table (skema Django) + Postgres
│   ├── 03-api-contract.md      # Rust Axum + X-Api-Key/Bearer (kontrak 1:1 Django)
│   ├── 04-design-system.md     # @plane/ui + tailwind-config + editor
│   ├── 05-testing-strategy.md  # cargo test (api-rs) + pytest baseline + vitest (web/packages)
│   └── 06-ops-runbook.md       # docker-compose + deployments + env
│
├── features/                   ← product per halaman + shared
│   ├── README.md               ← inventory + template + writing order
│   ├── _shared/
│   │   ├── README.md
│   │   └── <concern>.md        ← list, detail-page, routing, filter-sort, dll
│   └── <page>.md               ← 1 file per halaman (work-items, cycles, incidents, ...)
│
└── ui/                         ← spesifikasi komponen
    ├── README.md
    ├── design-tokens.md        ← token warna/radius/font/icon (actual CSS)
    ├── shell.md                ← AppShell + Sidebar + Topbar
    └── editor.md               ← TipTap editor (packages/editor)
```

---

## Kapan baca yang mana

| Kamu adalah…                    | Mulai dari                                                                 |
| ------------------------------- | -------------------------------------------------------------------------- |
| **New dev / onboarding**        | `PRODUCT.md` (15 menit) → `design/README.md` → `design/01-architecture.md` |
| **Sedang implementasi halaman** | `features/<page>.md` → `design/03-api-contract.md` untuk endpoint          |
| **Architect / reviewer**        | `design/*` penuh; cross-check `business-capabilities.md`                   |
| **Designer / UX**               | `design/04-design-system.md` + `ui/*` + `features/<page>.md`               |
| **DevOps / infra**              | `design/06-ops-runbook.md`                                                 |
| **QA / test**                   | `design/05-testing-strategy.md` + `AGENTS.md` §Backend tests               |

---

## Update policy

| Folder      | Update kapan?                          | Siapa boleh ubah?                      |
| ----------- | -------------------------------------- | -------------------------------------- |
| `design/`   | Setiap keputusan arsitektur berubah    | Dengan review — kontrak hidup teknikal |
| `features/` | Setiap perubahan spec produk           | Dengan review — kontrak hidup produk   |
| `ui/`       | Setiap perubahan komponen shell/editor | Dengan review                          |

Perubahan kode di `apps/`/`packages/` wajib disertai update doc yang relevan dalam commit yang sama (toleransi: `docs/linting.md` untuk lint-only).

---

## Prinsip dokumentasi

1. **Design preskriptif, features per-halaman.** Concern terpisah, template konsisten (lihat `features/README.md`).
2. **Jujur soal status.** Stub / draft / deferred ditandai eksplisit.
3. **Referensi ke file + line-range**, bukan copy paste kode.
4. **Cross-reference link relatif** (`./design/01-architecture.md`).
5. **Content boundary tegas** — lihat `features/README.md` §Content Boundary.

---

---

## Changelog

| Date       | Change                                        |
| ---------- | --------------------------------------------- |
| 2026-09-03 | init — adaptasi dari terra `docs/README.md:2` |
| 2026-09-05 | cutover `rust-cutover-v1`: stack → Rust Axum  |
