# 02 — Data Model

Status: **Draft.**

References: [`01-architecture.md`](./01-architecture.md), [`03-api-contract.md`](./03-api-contract.md).

Database: **Postgres** via `DATABASE_URL` — skema didefinisikan Django ORM (`plane.db.models.*`, referensi), **dilayani Rust SQLx** (`apps/api-rs`, migrasi `migrations/`). Actual-only: snapshot model Plane upstream, bukan propose ITSM. Model di bawah tidak berubah saat cutover (kontrak 1:1).

---

## Design Principles

1. **Per-table (skema Django, dilayani Rust).** `Workspace → Project → Issue/Cycle/Module/Page` sebagai tabel terpisah (`plane/db/models/workspace.py`, `project.py`, `issue.py`, `cycle.py`, `module.py`, `page.py`, `state.py`, `label.py`, `view.py`, `issue_type.py`).
2. **Workspace-scoped multi-tenant.** Domain tabel FK ke `Workspace` (via `Project.workspace`); query filter `workspace_id`.
3. **Soft delete via `deleted_at`.** `HARD_DELETE_AFTER_DAYS=60` (`plane/settings/common.py:420`) + `plane.bgtasks.cleanup_task`.
4. **Enums via Django choices + `IssueType` table.** `issue_type.py` (`IssueType`, `ProjectIssueType`) adalah tipe Work Item upstream — bukan ITSM propose.

---

## Domain Layering

```
FOUNDATION   workspace, project, state, label, estimate, user, member, issue_type
CORE         issue (Work Item: `issue.py` — `get_default_properties()` + `get_default_filters()`), description (`description.py`), draft (`draft.py`), intake (`intake.py`)
CYCLES       cycle (`cycle.py`), cycle_issue
MODULES      module (`module.py`), module_issue
PAGES        page (`page.py`)
VIEWS        view (`view.py`), global-view, project-view
CROSS        activity, notification (`notification.py`), favorite (`favorite.py`), inbox, sticky (`sticky.py`), recent_visit, webhook, asset (`asset.py`), exporter, importer, analytic, api (`api.py`: API tokens), device, session (`session.py`), deploy_board
```

Semua layer di atas delivered upstream (actual). Propose ITSM (incident/problem/change/CI) tidak ada di sini — lihat `features/_backlog.md`.

---

## ERD (ringkas — tabel Postgres, definisi Django)

```
WORKSPACE ||--o{ PROJECT : contains
PROJECT   ||--o{ ISSUE : contains
PROJECT   ||--o{ STATE : defines
PROJECT   ||--o{ LABEL : defines
PROJECT   ||--o{ CYCLE : contains
PROJECT   ||--o{ MODULE : contains
PROJECT   ||--o{ VIEW : contains
WORKSPACE ||--o{ USER (via Member) : has
ISSUE     ||--o{ ISSUE_DESCRIPTION : has
ISSUE     ||--o{ ISSUE_VERSION : versions
ISSUE     ||--o{ ISSUE_RELATION : relates
CYCLE     ||--o{ CYCLE_ISSUE : contains
MODULE    ||--o{ MODULE_ISSUE : contains
PAGE      }o--|| PROJECT : belongs
ISSUE     }o--|| ISSUE_TYPE (via `ProjectIssueType`): typed (`issue_type.py:11` `IssueType`, `project_issue_types` unique `project+issue_type`)
WORKSPACE ||--o{ VIEW : contains
WORKSPACE ||--o{ DRAFT / INTAKE / STICKY : contains
```

---

## Table Definitions

### FOUNDATION (Plane upstream — `plane/db/models/`)

```python
# workspace.py — multi-tenant root
class Workspace(models.Model):
    name = models.CharField(max_length=255)
    slug = models.SlugField(unique=True)  # URL: /:workspaceSlug
    owner = models.ForeignKey(User, on_delete=models.CASCADE)
    created_at = models.DateTimeField(auto_now_add=True)
    deleted_at = models.DateTimeField(null=True)

# project.py
class Project(models.Model):
    workspace = models.ForeignKey(Workspace, on_delete=models.CASCADE)
    name = models.CharField(max_length=255)
    identifier = models.CharField(max_length=12)  # e.g. "WEB", prefix untuk Issue ID
    created_at = models.DateTimeField(auto_now_add=True)

# issue.py — Work Item inti Plane
class Issue(models.Model):
    workspace = models.ForeignKey(Workspace, on_delete=models.CASCADE)
    project = models.ForeignKey(Project, on_delete=models.CASCADE)
    name = models.CharField(max_length=255)
    description = models.JSONField(default=dict)  # TipTap JSON
    state = models.ForeignKey(State, on_delete=models.SET_NULL, null=True)
    priority = models.CharField(choices=Priority.choices)
    assignee = models.ForeignKey(User, null=True, on_delete=models.SET_NULL)
    created_by = models.ForeignKey(User, on_delete=models.SET_NULL, null=True)
    created_at = models.DateTimeField(auto_now_add=True)
    deleted_at = models.DateTimeField(null=True)  # soft delete (HARD_DELETE_AFTER_DAYS)

# issue_type.py — tipe Work Item upstream (bukan ITSM propose)
class IssueType(models.Model):  # `issue_type.py:11`, db_table `issue_types`
    workspace = models.ForeignKey("db.Workspace", on_delete=models.CASCADE)
    name = models.CharField(max_length=255)
    logo_props = models.JSONField(default=dict)
    is_epic / is_default / is_active = models.BooleanField(...)
    level = models.FloatField(default=0)

class ProjectIssueType(models.Model):  # db_table `project_issue_types`
    project / issue_type = models.ForeignKey(...)
    unique: (project, issue_type) when deleted_at null
```

---

## Soft Delete

`deleted_at` dipakai di: `Workspace`, `Project`, `Issue`, `Cycle`, `Module`, `Page`, `IssueType`/`ProjectIssueType` (unique `...when deleted_at null`). Query default `filter(deleted_at__isnull=True)` — cleanup via `plane.bgtasks.cleanup_task` setelah `HARD_DELETE_AFTER_DAYS=60`. Jangan hard delete di view layer.

---

## Seed Requirements

- `plane/seeds/` (`plane/settings/common.py:559`) — fixture untuk State, Label, Estimate per workspace.

---

## Resolved Decisions

| #   | Topik        | Keputusan                                                                                  |
| --- | ------------ | ------------------------------------------------------------------------------------------ |
| 1   | Lookup enums | **Django choices + `IssueType`** — actual upstream                                         |
| 2   | Soft delete  | **`deleted_at` + `HARD_DELETE_AFTER_DAYS=60`** — actual upstream                           |
| 3   | ID format    | **Plane `PROJECT-IDENTIFIER + sequence`** (e.g. `WEB-123`) — actual (`project.identifier`) |

---

## Open Items

(none — actual-only; propose ITSM di `features/_backlog.md`)

---

---

## Changelog

| Date       | Change |
| ---------- | ------ |
| 2026-09-03 | —      |
