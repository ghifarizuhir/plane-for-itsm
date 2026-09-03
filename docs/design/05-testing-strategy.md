# 05 — Testing Strategy

Status: **Draft.**

References: [`01-architecture.md`](./01-architecture.md), `AGENTS.md:26` (Backend tests Docker).

---

## Targets

| Layer                           | Target (line) | Tool                                   | Catatan                   |
| ------------------------------- | ------------- | -------------------------------------- | ------------------------- |
| `apps/api/plane` services       | **70%**       | `pytest` via `docker-compose-test.yml` | Hard gate CI              |
| `apps/api` middleware/throttles | 80%           | pytest                                 | Auth, rate-limit critical |
| `apps/api` views/serializers    | 50%           | pytest + APIClient                     | Thin — via integration    |
| `apps/web/core/store`           | 50%           | Vitest + RTL + MSW                     | MobX stores               |
| `packages/ui`                   | 60%           | Vitest + Storybook + RTL               | Component contract        |
| Global minimum                  | 55%           | —                                      | —                         |

---

## Tooling

| Concern           | Library                    | Versi                               | Pemakaian                                             |
| ----------------- | -------------------------- | ----------------------------------- | ----------------------------------------------------- |
| Backend           | `pytest` + `pytest-django` | —                                   | `apps/api/pytest.ini`, `run_tests.py`, `run_tests.sh` |
| Backend isolation | `docker-compose-test.yml`  | —                                   | `api-tests` service — `AGENTS.md:30`                  |
| Frontend          | `Vitest 4` + RTL + MSW     | `4.1.8` (`pnpm-workspace.yaml:183`) | `pnpm turbo run test --filter=web`                    |
| Coverage          | `@vitest/coverage-v8`      | `4.1.8`                             | —                                                     |
| Lint gate         | `Oxlint 1.51 + oxfmt 0.35` | —                                   | `pnpm check` sebelum test                             |
| E2E (deferred)    | Playwright                 | —                                   | Phase 2 — 6 golden paths                              |

---

## Commands

```bash
# Full suite (isolated Docker — AGENTS.md:30)
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

### Django services/views (TDD wajib)

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

- **Service + middleware + utils + Django models** = TDD wajib.
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
