# 05 — Testing Strategy

Status: **Draft.**

References: [`01-architecture.md`](./01-architecture.md), `AGENTS.md:26` (Backend tests Docker).

---

## Targets

| Layer                           | Target (line) | Tool                                                  | Catatan                             |
| ------------------------------- | ------------- | ----------------------------------------------------- | ----------------------------------- |
| `apps/api-rs` routes/handlers   | **100%**      | `cargo test --workspace` — TDD wajib, gate 0 failed   | Kontrak 1:1 Django                  |
| `apps/api-rs` gates             | —             | shadow (`scripts/shadow.sh`) + parity + cutover (TCP) | 55+ path, no-404, no-reqwest        |
| `apps/api/plane` services       | **70%**       | `pytest` via `docker-compose-test.yml`                | Baseline kontrak Django (573 green) |
| `apps/api` middleware/throttles | 80%           | pytest (Django) + Rust `rate_limit_test.rs`           | Auth, rate-limit critical           |
| `apps/api` views/serializers    | 50%           | pytest + APIClient (Django); Rust via integration     | Thin — via integration              |
| `apps/web/core/store`           | 50%           | Vitest + RTL + MSW                                    | MobX stores                         |
| `packages/ui`                   | 60%           | Vitest + Storybook + RTL                              | Component contract                  |
| Global minimum                  | 55%           | —                                                     | —                                   |

---

## Tooling

| Concern           | Library                    | Versi                               | Pemakaian                                             |
| ----------------- | -------------------------- | ----------------------------------- | ----------------------------------------------------- |
| Backend (Rust)    | `cargo test` workspace     | Rust 1.96                           | `apps/api-rs`: unit + integration + gates             |
| Backend (Django)  | `pytest` + `pytest-django` | —                                   | `apps/api/pytest.ini`, baseline kontrak, bukan target |
| Backend isolation | `docker-compose-test.yml`  | —                                   | `api-tests` service — `AGENTS.md:30`                  |
| Frontend          | `Vitest 4` + RTL + MSW     | `4.1.8` (`pnpm-workspace.yaml:183`) | `pnpm turbo run test --filter=web`                    |
| Coverage          | `@vitest/coverage-v8`      | `4.1.8`                             | —                                                     |
| Lint gate         | `Oxlint 1.51 + oxfmt 0.35` | —                                   | `pnpm check` sebelum test                             |
| E2E (deferred)    | Playwright                 | —                                   | Phase 2 — 6 golden paths                              |

---

## Commands

```bash
# Rust (utama — TDD Iron Law: test dulu, watch fail, GREEN, 0 failed)
DATABASE_URL=postgres://plane:plane@<plane-db-ip>:5432/plane_test cargo test --workspace
cargo test -p api --test cutover_test                                  # guard cutover :8000
RUST_API_URL=127.0.0.1:8000 cargo test -p api --test parity_gate_test -- --ignored  # live no-404
bash apps/api-rs/scripts/shadow.sh                                     # shadow Django-vs-Rust

# Django baseline (kontrak referensi — full suite, ~6-8 mnt)
docker compose -f docker-compose-test.yml up --build --abort-on-container-exit --exit-code-from api-tests
docker compose -f docker-compose-test.yml down -v

# Subset
docker compose -f docker-compose-test.yml run --rm api-tests pytest -m unit
docker compose -f docker-compose-test.yml run --rm api-tests pytest apps/api/tests/test_incidents.py -v

# Frontend (local — pnpm + turbo)
pnpm check:types && pnpm check:lint
pnpm turbo run test --filter=web
pnpm turbo run test --filter=@plane/ui
pnpm --filter=@plane/ui storybook  # port 6006 — visual regression
```

Prereq sekali: `./setup.sh` — generate `apps/api/.env` dari `.env.example`.

---

## Patterns per Layer

### Rust routes/handlers (TDD wajib — Iron Law)

```rust
// crates/api/tests/detail_cycle_test.rs (pola — RED dulu: could not find X in routes)
#[tokio::test]
async fn patch_completed_cycle_sort_only() { /* 400 untuk field selain sort_order */ }
```

- Test dulu sebelum handler; komit hanya saat `cargo test` 0 failed.
- Validator `validate_*` murni → unit test langsung; kontrak HTTP → integration test + shadow + gate.

### Django services/views (baseline kontrak)

```python
# tests/test_issue_service.py (contoh actual)
import pytest

@pytest.mark.django_db
def test_create_issue_in_project(workspace, project, user):
    from plane.db.models import Issue
    issue = Issue.objects.create(workspace=workspace, project=project, name="DB down", priority="urgent", created_by=user)
    assert issue.pk is not None
    assert issue.project_id == project.id
```

- Factories/fixtures di `apps/api/tests/` — reuse `conftest.py` untuk `workspace`, `project`, `user`, `api_client`.
- Isolation: `pytest-django` transaction rollback per test; Docker image migrate-once.
- Hasil baseline: 573 passed — acuan parity Rust, bukan target pengembangan baru.

### MobX stores (test-after boleh, tapi coverage 50%)

```ts
// apps/web/core/store/issue.store.test.ts
import { IssueStore } from "./issue.store";
test("createIssue invalidates list", async () => {
  const store = new IssueStore(rootStore);
  await store.createIssue({ title: "x", priority: "urgent" });
  expect(store.issues).toContainEqual(expect.objectContaining({ title: "x" }));
});
```

### Components (`packages/ui` — Storybook + Vitest)

- Stories colocated `*.stories.tsx` di `packages/ui/src/*` — bukan `*.stories` terpisah.
- A11y via `jsx-a11y` (Oxlint category) — `docs/linting.md:8`.

---

## TDD Discipline

- **Rust route/handler/validator/middleware** = TDD wajib (Iron Law).
- **Django models/services** = baseline kontrak (pytest green dipertahankan, bukan target baru).
- **UI component + MobX store + hook** = test-after (tapi tetap 50%/60% gate).

---

## Detail di

- `apps/api/tests/RUNNING_TESTS.md` — walkthrough + troubleshooting Docker test stack.
- `apps/api/tests/TESTING_GUIDE.md` — conventions + fixtures.

---

---

## Changelog

| Date       | Change                                                                            |
| ---------- | --------------------------------------------------------------------------------- |
| 2026-09-03 | adaptasi dari terra `05-testing-strategy.md` — Django/pytest vs Vitest-only Terra |
| 2026-09-05 | cutover `rust-cutover-v1`: cargo test utama + gates, pytest jadi baseline kontrak |
