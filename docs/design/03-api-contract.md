# 03 — API Contract

Status: **Draft.**

Base: [`01-architecture.md`](./01-architecture.md), [`02-data-model.md`](./02-data-model.md).

Backend: `apps/api` Django 5 + DRF (`plane/settings/common.py:138`). Adaptasi dari `terra/docs/design/02-api-contract.md:6` (Terra: Express+Drizzle+JWT bearer — **jangan copy**; Plane: Django SessionAuthentication).

Base URL: `http://localhost:8000` (dev, via `docker-compose-local.yml`). Client: `packages/services` (axios) + `SWR` di `apps/web`.

---

## Design Principles

1. **REST over RPC.** Resource-oriented, HTTP verb standard (GET/POST/PATCH/DELETE).
2. **Workspace-scoped.** Semua endpoint di bawah `/api/workspaces/:slug/` atau `/api/workspaces/:slug/projects/:id/` — filter `workspace_id` selalu.
3. **Thin views, fat serializers/services.** View hanya parse + validate + delegate; logic di `plane/db/models` + `plane/bgtasks`.
4. **Predictable errors.** Shape konsisten + HTTP status bermakna.
5. **Soft delete invisible.** Default `deleted_at__isnull=True`; `?includeDeleted=true` untuk admin/audit.

---

## Conventions

### Authentication

- **Session cookie** `session-id` (`plane/settings/common.py:374`, age 604800) — `SessionAuthentication` (`plane/settings/common.py:139`). Login via `plane.authentication` (Django auth + social GitHub/GitLab/Google/Gitea).
- **CSRF:** `CsrfViewMiddleware` (`plane/settings/common.py:127`) + `CSRF_COOKIE_NAME` — frontend wajib `X-CSRFToken` header.
- **API Key (opsional):** `X-API-Key` (`plane/settings/common.py:192` `CORS_ALLOW_HEADERS`, `API_KEY_RATE_LIMIT=60/minute:154`).
- Public: `/api/auth/` + `/api/workspaces/` list (tergantung permission). Lainnya `IsAuthenticated` (`plane/settings/common.py:145`).
- **Belum JWT bearer** seperti Terra — kalau ITSM butuh service-to-service token, tambahkan DRF `TokenAuthentication` terpisah (jangan ganti Session).

### Authorization

| Role (Plane)              | Access                                            |
| ------------------------- | ------------------------------------------------- |
| Instance admin (god-mode) | Full (`apps/admin`, `ADMIN_BASE_PATH=/god-mode/`) |
| Workspace admin           | CRUD workspace + projects + members               |
| Project admin             | CRUD project + issues/cycles/modules              |
| Member                    | CRUD own + read others dalam workspace            |
| Guest (space)             | Read-only via `apps/space`                        |

Enforcement di DRF `permission_classes` per view — bukan di SQL scope utility Terra (`buildScopeCondition`).

### Request / Response Format

- JSON, `Content-Type: application/json`, timestamps ISO 8601 UTC, IDs string.
- Pagination: `?page=1&pageSize=50` (default 50, max 200) → `{results, count, next, previous, hasMore}` atau `{items,total,hasMore}` — konsistenkan ke `CHEAT-SHEET.md` §URL & Routing (Plane pakai DRF `PageNumberPagination`).
- Filter: `?state=backlog,started&priority=urgent&assignee=USR-1` (CSV multi-value).
- Sort: `?sort=created_at:desc,priority:asc`.

### Error Shape

```json
{ "error": "Human readable", "code": "ERR_SLUG", "details": { "field": "info" }, "requestId": "req-abc123" }
```

HTTP status: 200 OK, 201 Created, 204 No Content, 400 Validation, 401 Unauth, 403 Forbidden, 404 Not Found, 409 Conflict, 429 Rate Limited (`429 ERR_RATE_LIMITED` dari throttle `plane/settings/common.py:141`), 500 Internal.

DRF default error adalah `{"field": ["message"]}` — ITSM fork **bungkus** jadi shape `code` di `plane.authentication.adapter.exception.auth_exception_handler` (`plane/settings/common.py:148`).

---

## Endpoint Inventory

### Plane Upstream (existing — `plane/api/urls/*`)

| Method           | Path                                             | Auth | Deskripsi                                                                    |
| ---------------- | ------------------------------------------------ | ---- | ---------------------------------------------------------------------------- |
| GET/POST         | `/api/workspaces/`                               | Auth | List/create workspace                                                        |
| GET/PATCH        | `/api/workspaces/:slug/`                         | Auth | Detail/update workspace                                                      |
| GET/POST         | `/api/workspaces/:slug/projects/`                | Auth | List/create project                                                          |
| GET/PATCH/DELETE | `/api/workspaces/:slug/projects/:id/`            | Auth | Project detail                                                               |
| GET/POST         | `/api/workspaces/:slug/projects/:id/issues/`     | Auth | Issues (Work Items) — list/create dengan filter `?state=&priority=&q=&sort=` |
| GET/PATCH        | `/api/workspaces/:slug/projects/:id/issues/:id/` | Auth | Issue detail/update                                                          |
| GET/POST         | `/api/workspaces/:slug/projects/:id/cycles/`     | Auth | Cycles                                                                       |
| GET/POST         | `/api/workspaces/:slug/projects/:id/modules/`    | Auth | Modules                                                                      |
| GET/POST         | `/api/workspaces/:slug/projects/:id/views/`      | Auth | Views (saved filters)                                                        |
| GET/POST         | `/api/workspaces/:slug/projects/:id/pages/`      | Auth | Pages (TipTap)                                                               |
| GET              | `/api/workspaces/:slug/projects/:id/analytics/`  | Auth | Analytics                                                                    |
| POST             | `/api/auth/sign-in/`                             | No   | Login (session cookie)                                                       |
| POST             | `/api/auth/sign-out/`                            | Auth | Logout                                                                       |
| GET              | `/api/users/me/`                                 | Auth | Current user                                                                 |

> Inventory lengkap: `apps/api/plane/api/urls` + `plane/settings/common.py:160` `ROOT_URLCONF=plane.urls`.

> ITSM endpoints belum ada (actual-only) — propose di `features/_backlog.md`.

### Versions / Timeline (fork — reuse Plane pattern)

| Method | Path                                                                     | Deskripsi                |
| ------ | ------------------------------------------------------------------------ | ------------------------ |
| GET    | `/api/workspaces/:slug/projects/:id/issues/:id/versions/`                | List versions (desc)     |
| GET    | `/api/workspaces/:slug/projects/:id/issues/:id/versions/:vn/`            | Single snapshot          |
| GET    | `/api/workspaces/:slug/projects/:id/issues/:id/versions/:vn/diff?from=N` | Diff fields+items        |
| POST   | `/api/workspaces/:slug/projects/:id/issues/:id/versions/:vn/revert`      | Revert (buat versi baru) |

Implement di `plane/bgtasks/issue_version_sync` + `description_version.py` — jangan copy `VersionService` Terra (`terra/02-api-contract.md:554`).

---

## Resolved Decisions

| #   | Topik          | Keputusan                                                                                                              |
| --- | -------------- | ---------------------------------------------------------------------------------------------------------------------- |
| 1   | Auth           | **Session cookie + CSRF** (Plane) — bukan JWT bearer Terra                                                             |
| 2   | Scoping        | **Workspace FK filter** — bukan `org_id` orgMiddleware                                                                 |
| 3   | Pagination     | **DRF PageNumberPagination** — shape `{count,next,previous,results}` atau normalisasi ke `{items,total,hasMore}` di FE |
| 4   | ITSM endpoints | **Nested `/workspaces/:slug/projects/:id/incidents/`** — reuse Issue infra, bukan `/api/incidents` global              |
| 5   | Error shape    | **Wrap DRF errors** jadi `{error,code,details}` via `auth_exception_handler`                                           |

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
