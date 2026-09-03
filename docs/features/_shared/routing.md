# Routing

Status: **Draft.**

Used by: semua pages (Plane upstream + ITSM fork)

Adaptasi dari `terra/docs/features/_shared/routing.md` — Terra pakai `/o/:orgId/<type>/:id` (org UUID); Plane pakai `/:workspaceSlug/projects/:id/issues/:id` (workspace slug + React Router 8 file-based).

---

## Purpose

URL adalah source of truth untuk navigation + filter state + deep link. Semua list filter & detail selection persist di URL supaya shareable + back/forward konsisten.

---

## Behavior

### Plane URL Pattern (actual — `apps/web/app/routes.ts` + `react-router.config.ts`)

```
/                           → redirect → /workspaces/:slug atau /workspaces
/workspaces                 → workspace list
/:workspaceSlug             → workspace home (dashboard)
/:workspaceSlug/projects/:projectId/issues           → Work Items list
/:workspaceSlug/projects/:projectId/issues/:issueId → Issue detail
/:workspaceSlug/projects/:projectId/cycles           → Cycles
/:workspaceSlug/projects/:projectId/modules          → Modules
/:workspaceSlug/projects/:projectId/views/:viewId    → Views (saved filter)
/:workspaceSlug/settings    → workspace settings
/god-mode/*                 → admin (ADMIN_BASE_PATH=/god-mode/ — plane/settings/common.py:395)
/spaces/:slug               → space public
/live/:id                   → live collaboration
```

### ITSM Pattern (belum ada)

Tidak ada route ITSM di `apps/web/app/routes/core.ts` — propose diparkir di `_backlog.md`. Jangan pakai `/o/:orgId` Terra — Plane pakai `:workspaceSlug`.

### Query Params (URL-driven state)

| Param                | Example                  | Persist    | History        |
| -------------------- | ------------------------ | ---------- | -------------- |
| `q`                  | `?q=payment+timeout`     | search     | `replaceState` |
| `state` / `priority` | `?state=backlog,started` | filter     | `replaceState` |
| `sort`               | `?sort=created_at:desc`  | sort       | `replaceState` |
| `page` / `pageSize`  | `?page=2&pageSize=50`    | pagination | `pushState`    |
| `assignee` / `label` | `?assignee=abc`          | filter     | `replaceState` |

- Filter change → `replaceState` (tidak bikin history entry baru).
- Pagination / Clear all → `pushState`.
- `withWorkspaceScope` helper (mirip Terra `withAppScope`) — wrap `navigate()` untuk preserve `workspaceSlug`.

### Navigation Patterns

- **List → Detail:** klik row → `navigate('/:workspaceSlug/projects/:id/issues/:issueId')`.
- **Detail → List (back):** `navigate(parentListPath)` deterministic — **bukan** `navigate(-1)` (external deep-link safe).
- **Scroll restoration:** `react-router` `<ScrollRestoration>` — list scroll position + filter preserved saat back dari detail.

---

## Edge Cases

- Workspace slug tidak ada → `404` + "Workspace not found" + link ke `/workspaces`.
- Project tidak dalam workspace → `403` (scope check di loader).
- Query param invalid → auto-clear + toast.

---

## API Touchpoints

Tidak ada — routing adalah frontend concern. Workspace/project validation via `GET /api/workspaces/:slug` + `GET /api/workspaces/:slug/projects/:id`.

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
