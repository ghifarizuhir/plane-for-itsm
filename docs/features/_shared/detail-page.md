# Detail Page

Status: **Draft.**

Used by: Issue detail (`issues/:issueId`), Cycle (`cycles/:cycleId`), Module (`modules/:moduleId`), View (`views/:viewId`), Page (`pages/:pageId`) — actual

Adaptasi dari `terra/docs/features/_shared/entity-detail-page.md` — Plane pakai `apps/web/core/layouts/default-layout` + MobX store (`core/store/*`). Rail kanan opsional, bukan `EntityRail` Terra.

---

## Purpose

Satu detail page untuk view + edit semua field entity — bukan list+side-peek combo Terra Phase-1 awal, tapi **standalone page** atau **overlay panel** tergantung entity (Plane Issues = overlay/side panel; ITSM bisa standalone bila konten berat).

---

## Behavior

### Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ Breadcrumb: Workspace > Project > Issues > ISSUE-123        [⋯] │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Issue Title h2                         Badge: state/priority   │
│  Description (TipTap editor) — packages/editor                  │
│                                                                 │
│  ─────────── Metadata grid (140px | 1fr) ────────────────       │
│    State: Backlog  Priority: Urgent  Assignee: Alice            │
│    Created: 2026-09-03  Labels: [bug, api]                      │
│  ─────────── Sections (ITSM: goals/checkpoints/RCA) ──────     │
│  ─────────── Comments + Activity timeline ────────────────      │
│  ─────────── Versions / History (ITSM) ──────────────────       │
└─────────────────────────────────────────────────────────────────┘
Rail kanan (desktop, resizable): Properties / Activity / History tabs — optional
```

- **Main:** title + description (TipTap `packages/editor`) + metadata grid + sections.
- **Rail/side panel (opsional, desktop):** Properties (state/priority/assignee/labels) + Activity (comments+timeline) — actual Plane Issues pakai side panel/modal, bukan rail Terra.

### Interaction

- Title click → inline edit (`Input` + save on blur/Enter).
- Description → TipTap editor (inline atau modal) — mention `@` untuk cross-issue ref.
- Metadata row → dropdown/picker (State, Priority, Assignee — via `packages/ui`).
- Comments → composer pinned bottom + `Cmd+Enter` send.
- History → list versions + Compare (diff) + Revert.

### Create Page

Actual: `drafts` (`plane/db/models/draft.py`) + inline create di list; belum ada route `/issues/new` terpisah — cek actual sebelum tulis.

---

## States

- **Loading:** skeleton (title + editor + metadata rows).
- **Not found:** "Issue not found." + back to list.
- **Error:** banner + retry.
- **Permission denied:** "You don't have access." (403 dari `plane/api`).

---

## API Touchpoints

| Hook (MobX store)                   | Endpoint                                                           | Catatan                         |
| ----------------------------------- | ------------------------------------------------------------------ | ------------------------------- |
| `issueStore.getIssue(id)`           | `GET /api/workspaces/:slug/projects/:id/issues/:id/`               | detail + description + activity |
| `issueStore.updateIssue(id, patch)` | `PATCH /api/workspaces/:slug/projects/:id/issues/:id/`             | partial update                  |
| `issueStore.createIssue(data)`      | `POST /api/workspaces/:slug/projects/:id/issues/`                  | create                          |
| `commentStore`                      | `GET/POST /api/workspaces/:slug/projects/:id/issues/:id/comments/` | comments                        |
| `versionStore`                      | `GET /api/workspaces/:slug/projects/:id/issues/:id/versions/`      | ITSM fork                       |

Ref: [`../../design/03-api-contract.md`](../../design/03-api-contract.md).

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
