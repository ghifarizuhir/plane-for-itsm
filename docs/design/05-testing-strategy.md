---
status: current
last_verified: 2026-10-27
source_hash: sha256:a578c965c9eeeee7fa7a7974247a1d37b103dedfc2490842f0f890b63fbfecb8
source_paths:
  - package.json
  - apps/api-rs/Cargo.toml
  - docker-compose-test.yml
  - AGENTS.md
  - .github/workflows
---

# 05 — Testing Strategy

Status: **Current.**

## Prinsip

- Perubahan perilaku harus disertai test di layer yang paling dekat dengan
  perubahan.
- Bug fix dimulai dari test yang gagal, lalu implementasi minimal, lalu test
  hijau.
- Test tidak boleh bergantung pada state global atau urutan eksekusi.
- Contract API Rust dijaga terhadap parity Django; perubahan kontrak harus
  disengaja dan terdokumentasi.

## Layer dan command

| Layer                 | Tool                               | Command                                                                                                     |
| --------------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Rust unit/integration | `cargo test`                       | `cd apps/api-rs && cargo test --workspace`                                                                  |
| Rust scratch suites   | `cargo test --test <name>`         | Tambahkan `-- --test-threads=1` untuk suite yang memakai helper purge                                       |
| Django baseline       | pytest + `docker-compose-test.yml` | `docker compose -f docker-compose-test.yml up --build --abort-on-container-exit --exit-code-from api-tests` |
| Web/packages          | Vitest + RTL                       | `pnpm turbo run test --filter=web` dan filter package terkait                                               |
| Docs tooling          | `node:test`                        | `pnpm docs:test`                                                                                            |
| Docs integrity        | custom scripts                     | `pnpm docs:check`                                                                                           |

Container test membutuhkan file env. Prasyarat dan troubleshooting ada di
[`AGENTS.md`](../../AGENTS.md) serta `apps/api/tests/RUNNING_TESTS.md`.

## Rust tests

- Workspace test berjalan dari `apps/api-rs`.
- DB-backed integration test memakai `DATABASE_URL`.
- Suite scratch tertentu (`issue_create_test`, `issue_patch_test`, `issue_test`,
  `workflow_test`, `workflow_transition_test`) harus serial karena helper purge
  menghapus fixture dengan prefix slug.
- Jangan mengurangi cakupan test untuk membuat CI hijau; perbaiki test atau
  perilaku yang salah.

## Django fallback

- Suite Django adalah baseline kontrak dan fallback, bukan target fitur baru.
- Jalankan full suite lewat Docker Compose test.
- Subset dapat dijalankan dengan `docker compose -f docker-compose-test.yml run --rm api-tests pytest -m unit`.

## Web and packages

- Test store/komponen memakai Vitest dan RTL.
- Mock network memakai MSW bila tersedia.
- Test komponen UI harus fokus pada perilaku yang terlihat user, bukan snapshot
  markup yang rapuh.
- Storybook dipakai untuk verifikasi visual manual, bukan pengganti test.

## Docs tests

- `pnpm docs:test` menguji parser frontmatter, hash, link, dan inventory.
- `pnpm docs:check` memvalidasi seluruh dokumen current, link, hash, dan inventory.
- Setelah mengubah route, API, migrasi, atau perilaku yang didokumentasikan,
  jalankan `pnpm docs:verify <doc>` lalu `pnpm docs:check`.

## CI

- Workflow web menjalankan format, build, lint, dan type check.
- Workflow API menjalankan build dan lint API.
- Workflow docs menjalankan `pnpm docs:test` dan `pnpm docs:check` untuk perubahan
  docs, route web, route API, migrasi, atau script docs.

## Changelog

| Tanggal    | Perubahan                                        |
| ---------- | ------------------------------------------------ |
| 2026-09-03 | fork init                                        |
| 2026-09-05 | cutover Rust                                     |
| 2026-10-27 | rewrite — layer test, serial suites, docs checks |
