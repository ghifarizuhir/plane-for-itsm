# List — Filter / Sort / Export / Pagination

Status: **Draft.**

Used by: Work Items (`issues`), Cycles, Modules, Views, Pages (actual)

Adaptasi dari `terra/docs/features/_shared/list.md` + `filter-sort-export.md` — Plane pakai `apps/web/core/store/*` + `@plane/ui` tables (`@tanstack/react-table`), bukan custom `DataTable` Terra.

---

## Purpose

Semua list pages (Plane + ITSM) pakai pola yang sama: **table/board/grid + filter + sort + search + pagination + export** — URL-driven, shareable, persist.

---

## Behavior

### Layout

```
Tier-1: Breadcrumbs + Filters (search, state/priority/label, date) + Sort + View toggle
Tier-2: Page header (title + count chip "N total" dari pagination)
Primary: DataTable (Work Items) / Board (Cycles) / Card grid (Pages) / Graph (Service Map)
Footer: Pagination mono "1–N of M" + per-page selector
```

Tier headers via `apps/web/core/layouts` + `packages/ui` — bukan `ListPageHeader` custom Terra (tapi pola sama: sticky `border-b /70`, `px-6 sm:px-8`).

### Filter

| Filter                | Type                 | Default | Persist                    |
| --------------------- | -------------------- | ------- | -------------------------- |
| State / Status        | multi-select         | all     | `?state=backlog,started`   |
| Priority              | multi-select         | all     | `?priority=urgent,high`    |
| Assignee / Created by | single               | —       | `?assignee=USR-1`          |
| Label                 | multi-select         | —       | `?label=LAB-1`             |
| Search                | text debounced 300ms | —       | `?q=payment`               |
| Date range            | picker               | —       | `?createdFrom=&createdTo=` |

**Chips:** active filters render sebagai chips di atas table — klik `×` remove. `Clear all` push history.

### Sort

- Klik column header → `asc/desc/off` cycle.
- Multi-sort: Shift+click (opsional — Terra pakai, Plane Work Items belum; tambah bila ITSM butuh).
- Persist URL: `?sort=created_at:desc,priority:asc`.

### Pagination

- Default `pageSize=50` (25/50/100 options). Footer bar monokrom.
- Persist URL: `?page=2&pageSize=50` → DRF `?page=2` + `count` di response.

### Export

- Actual: `exporter` model (`plane/db/models/exporter.py`) + `plane.bgtasks.exporter_expired_task` — CSV/XLSX export Work Items (cek `apps/api/plane/api/` actual sebelum tulis detail).
- Propose ITSM (mis. `incidents/export` cap 5000) diparkir di `_backlog.md`.

---

## States

- **Default:** table rows + filter chips.
- **Empty (0 items, no filter):** `h2` + CTA "Create work item" / "Report incident".
- **Empty (filter 0):** "No results for filters. [Clear filters]".
- **Loading initial:** skeleton rows 8 (shimmer).
- **Loading filter:** top progress bar 2px + dim rows 0.6.
- **Error:** banner "Failed to load. [Retry]".

---

## Edge Cases

- URL param rusak → auto-clear + toast "Invalid filter, reset to default".
- `page` out of range → clamp ke `1` atau `last`.

---

## API Touchpoints

- `GET /api/workspaces/:slug/projects/:id/issues/?state=&priority=&q=&sort=&page=` — Work Items (`plane/api/views/issue.py`)
- (ITSM `incidents/` belum ada — lihat `_backlog.md`)

Ref: [`../../design/03-api-contract.md`](../../design/03-api-contract.md).

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
