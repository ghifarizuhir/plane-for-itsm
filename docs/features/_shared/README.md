# Shared Concerns — Plane for ITSM

Cross-cutting behavior dipakai > 1 halaman. Adaptasi dari `terra/docs/features/_shared/README.md` — ramping dari 19 file Terra → 5 file Plane Phase 1.

---

## Inventory

| Concern                   | File                                     | Status   | Used by                                          |
| ------------------------- | ---------------------------------------- | -------- | ------------------------------------------------ |
| List & filter/sort/export | [`list.md`](./list.md)                   | 📝 Draft | Work Items, Cycles, Modules, Views, + ITSM lists |
| Detail page               | [`detail-page.md`](./detail-page.md)     | 📝 Draft | Issue detail, Cycle/Module detail, ITSM detail   |
| Routing                   | [`routing.md`](./routing.md)             | 📝 Draft | URL pattern `/:workspaceSlug/projects/:id/...`   |
| Comments & activity       | [`comments.md`](./comments.md)           | 📝 Draft | Issue comments, Pages comments                   |
| Global search             | [`global-search.md`](./global-search.md) | 📝 Draft | Cmd+K palette                                    |

> **Jangan pre-build 19 file Terra** (`terra/docs/features/_shared:19` — termasuk `entity-linking`, `versioning`, `reviews`, `rail`, `report`, `attachments`, `app-selector`). Tambah bila ITSM benar-benar butuh (rule-of-3: dipakai 3× baru ekstrak).

---

## Template (ringkas — lihat `../README.md`)

```markdown
# <Concern>

Status: **Draft** | **Approved**
Used by: ...

## Purpose

## Behavior

## States

## Edge Cases

## API Touchpoints (ref design/03-api-contract.md)
```

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
