# 03 — API Contract

Status: **Draft.**

Base: [`01-architecture.md`](./01-architecture.md), [`02-data-model.md`](./02-data-model.md).

Backend: `apps/api-rs` Rust Axum + SQLx — kontrak 1:1 Django (`plane/settings/common.py:138` = referensi). Session/CSRF Django hanya berlaku di fallback `api-legacy`.

Base URL: `http://localhost:8000` (Rust `api`, via `docker-compose.yml`). Client: `packages/services` (axios) + `SWR` di `apps/web`.

---

## Design Principles

1. **REST over RPC.** Resource-oriented, HTTP verb standard (GET/POST/PATCH/DELETE).
2. **Workspace-scoped.** Semua endpoint di bawah `/api/workspaces/:slug/` atau `/api/workspaces/:slug/projects/:id/` — filter `workspace_id` selalu.
3. **Thin handlers, pure validators.** Handler hanya parse + `validate_*` + query SQLx; logic async di worker via Stream.
4. **Predictable errors.** Shape konsisten + HTTP status bermakna.
5. **Soft delete invisible.** Default `deleted_at__isnull=True`; `?includeDeleted=true` untuk admin/audit.

---

## Conventions

### Authentication

- **API Key utama:** `X-Api-Key` — token dicek ke tabel `api_tokens` → owner `user_id`; tanpa token valid → 401 `missing or invalid bearer` (`middleware/auth.rs:12`).
- **Bearer:** didukung sebagai alternatif API key (bukan JWT session Terra).
- **Legacy (Django `api-legacy` saja):** session cookie `session-id` + CSRF + DRF `SessionAuthentication`. Login/social (GitHub/GitLab/Google/Gitea) masih via Django — belum di-port.
- Public: `/api/auth/` + `/api/workspaces/` list (tergantung permission). Lainnya butuh auth.

### Authorization

| Role (Plane)              | Access                                            |
| ------------------------- | ------------------------------------------------- |
| Instance admin (god-mode) | Full (`apps/admin`, `ADMIN_BASE_PATH=/god-mode/`) |
| Workspace admin           | CRUD workspace + projects + members               |
| Project admin             | CRUD project + issues/cycles/modules              |
| Member                    | CRUD own + read others dalam workspace            |
| Guest (space)             | Read-only via `apps/space`                        |

Enforcement di Axum extractor `AuthUser` + guard per handler (owner/self/higher-role) — bukan di SQL scope utility Terra (`buildScopeCondition`).

### Request / Response Format

- JSON, `Content-Type: application/json`, timestamps ISO 8601 UTC, IDs string.
- Pagination: `?page=1&pageSize=50` (default 50, max 200) → `{results, count, next, previous, hasMore}` atau `{items,total,hasMore}` — konsistenkan ke `CHEAT-SHEET.md` §URL & Routing (shape parity Django `PageNumberPagination`).
- Filter: `?state=backlog,started&priority=urgent&assignee=USR-1` (CSV multi-value).
- Sort: `?sort=created_at:desc,priority:asc`.

### Error Shape

```json
{ "error": "Human readable", "code": "ERR_SLUG", "details": { "field": "info" }, "requestId": "req-abc123" }
```

HTTP status: 200 OK, 201 Created, 204 No Content, 400 Validation, 401 Unauth, 403 Forbidden, 404 Not Found, 409 Conflict, 429 Rate Limited (token-bucket per-key 600/mnt, 429 langsung — `middleware/rate_limit.rs`), 500 Internal.

Error Rust: plain string validasi (parity pesan Django, mis. `PROJECT_NAME_ALREADY_EXIST`) atau `missing or invalid bearer` untuk 401 — kontrak diverifikasi shadow + parity gate, bukan wrapper DRF.

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

| #   | Topik          | Keputusan                                                                                                                        |
| --- | -------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Auth           | **Session cookie + CSRF** (Plane) — bukan JWT bearer Terra                                                                       |
| 2   | Scoping        | **Workspace FK filter** — bukan `org_id` orgMiddleware                                                                           |
| 3   | Pagination     | **Shape parity Django PageNumberPagination** — `{count,next,previous,results}` atau normalisasi ke `{items,total,hasMore}` di FE |
| 4   | ITSM endpoints | **Nested `/workspaces/:slug/projects/:id/incidents/`** — reuse Issue infra, bukan `/api/incidents` global                        |
| 5   | Error shape    | **Parity pesan Django** via validator Rust + shadow/parity gate (bukan wrapper DRF)                                              |

---

---

## Changelog

| Date       | Change                                                                             |
| ---------- | ---------------------------------------------------------------------------------- |
| 2026-09-03 | —                                                                                  |
| 2026-09-05 | cutover `rust-cutover-v1`: auth X-Api-Key/Bearer, rate-limit 600/mnt, error parity |
