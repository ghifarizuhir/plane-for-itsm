# Work Item Types & Workflows (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Admin workspace bisa mendefinisikan work item type (Incident, Problem, Change, Improvement) dengan workflow state + transisi sendiri; project opt-in lewat `project_issue_types`; perpindahan state work item divalidasi di api-rs.

**Architecture:** Schema dimiliki Django (`apps/api`, migrator-only) — model baru `Workflow`, `WorkflowState`, `WorkflowTransition`, plus kolom `IssueType.workflow`, `State.type`, `State.workflow_state`. Serving di api-rs: state workflow di-materialize jadi row `states` project saat type di-import, lalu validasi transisi memakai mapping `workflow_state`. Web plan terpisah: `docs/superpowers/plans/2026-09-25-work-item-types-workflows-web.md`.

**Tech Stack:** Django 5.2 + PostgreSQL (migrasi), Rust (axum + sqlx, runtime query), Docker compose local/test.

**Spec:** `docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md`

---

## File Structure

| File                                                          | Aksi               | Tanggung jawab                                                                             |
| ------------------------------------------------------------- | ------------------ | ------------------------------------------------------------------------------------------ |
| `apps/api/plane/db/models/workflow.py`                        | Create             | Model `Workflow`, `WorkflowState`, `WorkflowTransition`                                    |
| `apps/api/plane/db/models/__init__.py`                        | Modify             | Ekspor model baru                                                                          |
| `apps/api/plane/db/models/issue_type.py`                      | Modify             | `IssueType.workflow` FK                                                                    |
| `apps/api/plane/db/models/state.py`                           | Modify             | `State.type`, `State.workflow_state`, constraint baru                                      |
| `apps/api/plane/db/models/issue.py`                           | Modify             | `_ensure_default_state` type-aware                                                         |
| `apps/api/plane/db/models/draft.py`                           | Modify             | Default state draft type-aware                                                             |
| `apps/api/plane/db/migrations/0123_*.py`                      | Create (generated) | Schema migration                                                                           |
| `apps/api/plane/db/migrations/0124_seed_default_workflows.py` | Create             | Data migration seed workflow/type default                                                  |
| `apps/api/plane/tests/unit/models/test_workflow_models.py`    | Create             | Test model + constraint + default state                                                    |
| `apps/api-rs/crates/api/src/routes/workflow.rs`               | Create             | CRUD workflow/state/transisi + materialization + workflow-map + guard + enforcement helper |
| `apps/api-rs/crates/api/src/routes/issue_common.rs`           | Modify             | `resolve_issue_state` type-aware                                                           |
| `apps/api-rs/crates/api/src/routes/issue_update.rs`           | Modify             | Enforcement transisi di PATCH                                                              |
| `apps/api-rs/crates/api/src/routes/issue_write.rs`            | Modify             | Enforcement + default type-aware di create                                                 |
| `apps/api-rs/crates/api/src/routes/draft.rs`                  | Modify             | Default type-aware di draft→issue                                                          |
| `apps/api-rs/crates/api/src/routes/v1/work_item.rs`           | Modify             | Enforcement di v1 write                                                                    |
| `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`      | Modify             | Tambah `workflow` di payload, pakai helper shared                                          |
| `apps/api-rs/crates/api/src/seed.rs`                          | Modify             | Seed workflow/type default untuk workspace baru                                            |
| `apps/api-rs/crates/api/src/routes/mod.rs`                    | Modify             | `pub mod workflow; pub mod work_item_type_shared;`                                         |
| `apps/api-rs/crates/api/src/main.rs`                          | Modify             | Registrasi route baru                                                                      |
| `apps/api-rs/crates/api/tests/workflow_test.rs`               | Create             | Test validators + materialization + workflow-map                                           |
| `apps/api-rs/crates/api/tests/workflow_transition_test.rs`    | Create             | Test enforcement                                                                           |
| `apps/api-rs/crates/api/parity-inventory.json`                | Modify             | Entry route baru (route inventory test)                                                    |

**Urutan milestone:** A (schema) → B (api-rs CRUD + materialization) → C (enforcement) → D (seed, route inventory, E2E smoke). Setiap milestone menghasilkan software yang bisa dites.

---

## Milestone A — Schema Django

Semua command dijalankan dari root repo. Test Django butuh stack `docker-compose-test.yml`; migrasi lokal butuh `migrator`.

### Task A1: Model Workflow, WorkflowState, WorkflowTransition

**Files:**

- Create: `apps/api/plane/db/models/workflow.py`
- Modify: `apps/api/plane/db/models/__init__.py`
- Test: `apps/api/plane/tests/unit/models/test_workflow_models.py`

- [ ] **Step 1: Tulis test yang gagal**

```python
# apps/api/plane/tests/unit/models/test_workflow_models.py
import pytest
from django.db import IntegrityError

from plane.db.models import Workflow, WorkflowState, WorkflowTransition
from plane.tests.factories import WorkspaceFactory


def make_workflow():
    workspace = WorkspaceFactory()
    workflow = Workflow.objects.create(workspace=workspace, name="Incident Workflow")
    return workspace, workflow


@pytest.mark.unit
class TestWorkflowModels:
    @pytest.mark.django_db
    def test_state_default_unique_per_workflow(self):
        _, workflow = make_workflow()
        WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True
        )
        with pytest.raises(IntegrityError):
            WorkflowState.objects.create(
                workflow=workflow, name="Other", color="#60646C", group="backlog", is_default=True
            )

    @pytest.mark.django_db
    def test_state_name_unique_per_workflow(self):
        _, workflow = make_workflow()
        WorkflowState.objects.create(workflow=workflow, name="New", color="#60646C", group="backlog")
        with pytest.raises(IntegrityError):
            WorkflowState.objects.create(workflow=workflow, name="New", color="#F59E0B", group="started")

    @pytest.mark.django_db
    def test_state_slug_is_slugified(self):
        _, workflow = make_workflow()
        state = WorkflowState.objects.create(
            workflow=workflow, name="In Progress", color="#F59E0B", group="started"
        )
        assert state.slug == "in-progress"

    @pytest.mark.django_db
    def test_transition_pair_unique_and_self_rejected(self):
        _, workflow = make_workflow()
        new = WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True
        )
        progress = WorkflowState.objects.create(
            workflow=workflow, name="In Progress", color="#F59E0B", group="started"
        )
        WorkflowTransition.objects.create(workflow=workflow, from_state=new, to_state=progress)
        with pytest.raises(IntegrityError):
            WorkflowTransition.objects.create(workflow=workflow, from_state=new, to_state=progress)

    @pytest.mark.django_db
    def test_transition_to_self_rejected(self):
        _, workflow = make_workflow()
        new = WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True
        )
        with pytest.raises(IntegrityError):
            WorkflowTransition.objects.create(workflow=workflow, from_state=new, to_state=new)

    @pytest.mark.django_db
    def test_workflow_name_unique_per_workspace(self):
        workspace, _ = make_workflow()
        with pytest.raises(IntegrityError):
            Workflow.objects.create(workspace=workspace, name="Incident Workflow")
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `docker compose -f docker-compose-test.yml run --rm --build api-tests pytest plane/tests/unit/models/test_workflow_models.py -vv`

Expected: FAIL — `ImportError: cannot import name 'Workflow' from 'plane.db.models'`.

- [ ] **Step 3: Tulis model**

```python
# apps/api/plane/db/models/workflow.py
# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

# Django imports
from django.db import models
from django.db.models import Q
from django.template.defaultfilters import slugify

# Module imports
from .base import BaseModel
from .state import StateGroup


class Workflow(BaseModel):
    workspace = models.ForeignKey("db.Workspace", related_name="workflows", on_delete=models.CASCADE)
    name = models.CharField(max_length=255)
    description = models.TextField(blank=True)
    is_active = models.BooleanField(default=True)
    external_source = models.CharField(max_length=255, null=True, blank=True)
    external_id = models.CharField(max_length=255, blank=True, null=True)

    class Meta:
        verbose_name = "Workflow"
        verbose_name_plural = "Workflows"
        db_table = "workflows"
        ordering = ("-created_at",)
        constraints = [
            models.UniqueConstraint(
                fields=["workspace", "name"],
                condition=Q(deleted_at__isnull=True),
                name="workflow_unique_name_workspace_when_deleted_at_null",
            )
        ]

    def __str__(self):
        return f"{self.name} <{self.workspace.name}>"


class WorkflowState(BaseModel):
    workflow = models.ForeignKey("db.Workflow", related_name="states", on_delete=models.CASCADE)
    name = models.CharField(max_length=255)
    description = models.TextField(blank=True)
    color = models.CharField(max_length=255)
    slug = models.SlugField(max_length=100, blank=True)
    sequence = models.FloatField(default=65535)
    group = models.CharField(choices=StateGroup.choices, default=StateGroup.BACKLOG, max_length=20)
    is_default = models.BooleanField(default=False)
    external_source = models.CharField(max_length=255, null=True, blank=True)
    external_id = models.CharField(max_length=255, blank=True, null=True)

    class Meta:
        verbose_name = "Workflow State"
        verbose_name_plural = "Workflow States"
        db_table = "workflow_states"
        ordering = ("sequence",)
        constraints = [
            models.UniqueConstraint(
                fields=["workflow", "name"],
                condition=Q(deleted_at__isnull=True),
                name="workflow_state_unique_name_workflow_when_deleted_at_null",
            ),
            models.UniqueConstraint(
                fields=["workflow", "slug"],
                condition=Q(deleted_at__isnull=True),
                name="workflow_state_unique_slug_workflow_when_deleted_at_null",
            ),
            models.UniqueConstraint(
                fields=["workflow"],
                condition=Q(is_default=True, deleted_at__isnull=True),
                name="workflow_state_unique_default_workflow_when_deleted_at_null",
            ),
        ]

    def save(self, *args, **kwargs):
        self.slug = slugify(self.name)
        return super().save(*args, **kwargs)

    def __str__(self):
        return f"{self.name} <{self.workflow.name}>"


class WorkflowTransition(BaseModel):
    workflow = models.ForeignKey("db.Workflow", related_name="transitions", on_delete=models.CASCADE)
    from_state = models.ForeignKey("db.WorkflowState", related_name="outgoing_transitions", on_delete=models.CASCADE)
    to_state = models.ForeignKey("db.WorkflowState", related_name="incoming_transitions", on_delete=models.CASCADE)

    class Meta:
        verbose_name = "Workflow Transition"
        verbose_name_plural = "Workflow Transitions"
        db_table = "workflow_transitions"
        ordering = ("-created_at",)
        constraints = [
            models.UniqueConstraint(
                fields=["workflow", "from_state", "to_state"],
                condition=Q(deleted_at__isnull=True),
                name="workflow_transition_unique_pair_when_deleted_at_null",
            ),
            models.CheckConstraint(
                condition=~Q(from_state=models.F("to_state")),
                name="workflow_transition_from_state_differs_from_to_state",
            ),
        ]

    def __str__(self):
        return f"{self.from_state.name} -> {self.to_state.name}"
```

Tambahkan ekspor di akhir `apps/api/plane/db/models/__init__.py`:

```python
from .workflow import Workflow, WorkflowState, WorkflowTransition
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/models/test_workflow_models.py -vv`

Expected: PASS (6 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/db/models/workflow.py apps/api/plane/db/models/__init__.py apps/api/plane/tests/unit/models/test_workflow_models.py
git commit -m "feat(db): add workflow models"
```

---

### Task A2: `IssueType.workflow`, `State.type`, `State.workflow_state`, constraint baru

**Files:**

- Modify: `apps/api/plane/db/models/issue_type.py`
- Modify: `apps/api/plane/db/models/state.py`
- Modify: `apps/api/plane/tests/unit/models/test_workflow_models.py`

- [ ] **Step 1: Tambah test yang gagal**

Tambahkan import dan class berikut ke `test_workflow_models.py`:

```python
from plane.db.models import IssueType, State
from plane.tests.factories import ProjectFactory


def make_project():
    workspace = WorkspaceFactory()
    project = ProjectFactory(workspace=workspace)
    return workspace, project


@pytest.mark.unit
class TestStateTypeScoping:
    @pytest.mark.django_db
    def test_same_state_name_allowed_across_types(self):
        workspace, project = make_project()
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        problem = IssueType.objects.create(workspace=workspace, name="Problem")
        State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=incident)
        State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=problem)

    @pytest.mark.django_db
    def test_same_state_name_rejected_within_type(self):
        workspace, project = make_project()
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=incident)
        with pytest.raises(IntegrityError):
            State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=incident)

    @pytest.mark.django_db
    def test_legacy_state_name_still_unique_per_project(self):
        _, project = make_project()
        State.objects.create(project=project, name="Backlog", color="#60646C", group="backlog", default=True)
        with pytest.raises(IntegrityError):
            State.objects.create(project=project, name="Backlog", color="#60646C", group="backlog")

    @pytest.mark.django_db
    def test_only_one_default_per_type_per_project(self):
        workspace, project = make_project()
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        State.objects.create(
            project=project, name="New", color="#60646C", group="backlog", type=incident, default=True
        )
        with pytest.raises(IntegrityError):
            State.objects.create(
                project=project, name="Other", color="#60646C", group="backlog", type=incident, default=True
            )

    @pytest.mark.django_db
    def test_workflow_state_mirror_unique_per_project(self):
        workspace, project = make_project()
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        workflow = Workflow.objects.create(workspace=workspace, name="Incident Workflow")
        wf_state = WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True
        )
        State.objects.create(
            project=project,
            name="New",
            color="#60646C",
            group="backlog",
            type=incident,
            workflow_state=wf_state,
            default=True,
        )
        with pytest.raises(IntegrityError):
            State.objects.create(
                project=project, name="New Mirror", color="#60646C", group="backlog", type=incident, workflow_state=wf_state
            )
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/models/test_workflow_models.py::TestStateTypeScoping -vv`

Expected: FAIL — `TypeError`/`FieldError` karena `State.type` belum ada.

- [ ] **Step 3: Implementasi**

Di `apps/api/plane/db/models/issue_type.py`, tambahkan field `workflow` ke `IssueType` (setelah `external_id`):

```python
    workflow = models.ForeignKey(
        "db.Workflow",
        on_delete=models.SET_NULL,
        related_name="issue_types",
        null=True,
        blank=True,
    )
```

Di `apps/api/plane/db/models/state.py`, tambahkan dua field ke `State` (setelah `external_id`):

```python
    type = models.ForeignKey(
        "db.IssueType",
        on_delete=models.SET_NULL,
        related_name="states",
        null=True,
        blank=True,
    )
    workflow_state = models.ForeignKey(
        "db.WorkflowState",
        on_delete=models.SET_NULL,
        related_name="state_mirrors",
        null=True,
        blank=True,
    )
```

Ganti blok `Meta` `State` (baris `unique_together` + `constraints`) menjadi:

```python
    class Meta:
        verbose_name = "State"
        verbose_name_plural = "States"
        db_table = "states"
        ordering = ("sequence",)
        constraints = [
            models.UniqueConstraint(
                fields=["project", "name"],
                condition=Q(deleted_at__isnull=True, type__isnull=True),
                name="state_unique_legacy_name_project_when_deleted_at_null",
            ),
            models.UniqueConstraint(
                fields=["project", "type", "name"],
                condition=Q(deleted_at__isnull=True, type__isnull=False),
                name="state_unique_name_project_type_when_deleted_at_null",
            ),
            models.UniqueConstraint(
                fields=["project", "workflow_state"],
                condition=Q(deleted_at__isnull=True, workflow_state__isnull=False),
                name="state_unique_project_workflow_state_when_deleted_at_null",
            ),
            models.UniqueConstraint(
                fields=["project", "type"],
                condition=Q(deleted_at__isnull=True, default=True, type__isnull=False),
                name="state_unique_default_project_type_when_deleted_at_null",
            ),
        ]
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/models/test_workflow_models.py -vv`

Expected: PASS (11 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/db/models/issue_type.py apps/api/plane/db/models/state.py apps/api/plane/tests/unit/models/test_workflow_models.py
git commit -m "feat(db): scope states by work item type"
```

---

### Task A3: Schema migration 0123

**Files:**

- Create: `apps/api/plane/db/migrations/0123_*.py` (generated)

- [ ] **Step 1: Generate migrasi**

Run: `docker compose -f docker-compose-local.yml run --rm migrator python manage.py makemigrations db`

Expected: file baru `apps/api/plane/db/migrations/0123_<autoname>.py` berisi `CreateModel` × 3, `AddField` × 3 (`issuetype.workflow`, `state.type`, `state.workflow_state`), `RemoveConstraint` constraint lama, `AlterUniqueTogether` (hapus) dan `AddConstraint` × 4.

- [ ] **Step 2: Periksa migrasi**

Run: `git diff --stat apps/api/plane/db/migrations/`

Pastikan migrasi memuat operasi berikut (nama boleh beda, isi harus ada):

- `CreateModel` untuk `Workflow`, `WorkflowState`, `WorkflowTransition`
- `AddField` `issue_type.workflow`, `state.type`, `state.workflow_state`
- `RemoveConstraint` `state_unique_name_project_when_deleted_at_null`
- `AlterUniqueTogether(name="state", unique_together=set())`
- `AddConstraint` untuk keempat constraint baru

Jika ada `AlterField` yang tidak relevan (mis. `logo_props`), revert file itu dan jalankan ulang makemigrations — jangan commit perubahan yang tidak disengaja.

- [ ] **Step 3: Jalankan migrasi di stack lokal**

Run: `docker compose -f docker-compose-local.yml run --rm migrator python manage.py migrate db`

Expected: `Applying db.0123_...` OK.

- [ ] **Step 4: Verifikasi tabel**

Run:

```bash
docker compose -f docker-compose-local.yml exec plane-db psql -U plane -d plane -c "\d workflows" -c "\d workflow_states" -c "\d workflow_transitions" -c "\d states" | head -80
```

Expected: tiga tabel baru ada; `states` punya kolom `type_id`, `workflow_state_id` dan keempat index unique parsial.

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/db/migrations/0123_*.py
git commit -m "feat(db): migration for workflows and typed states"
```

---

### Task A4: Default state type-aware (`Issue` + `DraftIssue`)

**Files:**

- Modify: `apps/api/plane/db/models/issue.py`
- Modify: `apps/api/plane/db/models/draft.py`
- Modify: `apps/api/plane/tests/unit/models/test_workflow_models.py`

- [ ] **Step 1: Tambah test yang gagal**

```python
from plane.db.models import DraftIssue, Issue


@pytest.mark.unit
class TestTypeAwareDefaultState:
    @pytest.mark.django_db
    def test_issue_default_state_follows_type(self):
        workspace, project = make_project()
        State.objects.create(project=project, name="Backlog", color="#60646C", group="backlog", default=True)
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        State.objects.create(
            project=project, name="New", color="#60646C", group="backlog", type=incident, default=True
        )
        issue = Issue.objects.create(project=project, name="Server down", type=incident)
        assert issue.state.name == "New"

    @pytest.mark.django_db
    def test_issue_without_type_uses_legacy_default(self):
        _, project = make_project()
        State.objects.create(project=project, name="Backlog", color="#60646C", group="backlog", default=True)
        issue = Issue.objects.create(project=project, name="Generic task")
        assert issue.state.name == "Backlog"

    @pytest.mark.django_db
    def test_issue_epic_type_uses_legacy_default(self):
        workspace, project = make_project()
        State.objects.create(project=project, name="Backlog", color="#60646C", group="backlog", default=True)
        epic = IssueType.objects.create(workspace=workspace, name="Epic", is_epic=True)
        issue = Issue.objects.create(project=project, name="Big rock", type=epic)
        assert issue.state.name == "Backlog"

    @pytest.mark.django_db
    def test_draft_default_state_follows_type(self):
        workspace, project = make_project()
        State.objects.create(project=project, name="Backlog", color="#60646C", group="backlog", default=True)
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        State.objects.create(
            project=project, name="New", color="#60646C", group="backlog", type=incident, default=True
        )
        draft = DraftIssue.objects.create(project=project, name="Draft", type=incident)
        assert draft.state.name == "New"
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/models/test_workflow_models.py::TestTypeAwareDefaultState -vv`

Expected: FAIL — `test_issue_default_state_follows_type` dapat state `Backlog` (bukan `New`).

- [ ] **Step 3: Implementasi**

Ganti `_ensure_default_state` di `apps/api/plane/db/models/issue.py` (baris 228-238) dengan:

```python
    def _ensure_default_state(self):
        """Assign a default state when none is set (type-aware)."""
        if self.state is not None:
            return
        try:
            from plane.db.models import State

            states = State.objects.filter(~models.Q(is_triage=True), project=self.project)
            if self.type_id is not None and not self.type.is_epic:
                typed_states = states.filter(type_id=self.type_id)
                self.state = typed_states.filter(default=True).first() or typed_states.first()
                return
            self.state = states.filter(default=True).first() or states.first()
        except ImportError as e:
            log_exception(e)
```

Ganti blok `if self.state is None:` di `DraftIssue.save()` (`apps/api/plane/db/models/draft.py:84-98`) dengan:

```python
        if self.state is None:
            try:
                from plane.db.models import State

                states = State.objects.filter(~models.Q(is_triage=True), project=self.project)
                if self.type_id is not None and not self.type.is_epic:
                    typed_states = states.filter(type_id=self.type_id)
                    self.state = typed_states.filter(default=True).first() or typed_states.first()
                else:
                    self.state = states.filter(default=True).first() or states.first()
            except ImportError:
                pass
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/models/test_workflow_models.py -vv`

Expected: PASS (15 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/db/models/issue.py apps/api/plane/db/models/draft.py apps/api/plane/tests/unit/models/test_workflow_models.py
git commit -m "feat(db): resolve default state by work item type"
```

---

### Task A5: Data migration seed workflow + type default

**Files:**

- Create: `apps/api/plane/db/migrations/0124_seed_default_workflows.py`

- [ ] **Step 1: Buat migrasi kosong**

Run: `docker compose -f docker-compose-local.yml run --rm migrator python manage.py makemigrations db --empty --name seed_default_workflows`

Expected: `apps/api/plane/db/migrations/0124_seed_default_workflows.py` dengan `dependencies = [("db", "0123_...")]`.

- [ ] **Step 2: Isi migrasi**

```python
# apps/api/plane/db/migrations/0124_seed_default_workflows.py
from django.db import migrations

# (name, group, is_default) — urutan = sequence (15000, 30000, ...)
WORKFLOW_SEEDS = [
    {
        "type": "Incident",
        "workflow": "Incident Workflow",
        "states": [
            ("New", "backlog", True),
            ("In Progress", "started", False),
            ("On Hold", "started", False),
            ("Resolved", "completed", False),
            ("Closed", "completed", False),
        ],
        "transitions": [
            ("New", "In Progress"),
            ("In Progress", "On Hold"),
            ("In Progress", "Resolved"),
            ("On Hold", "In Progress"),
            ("Resolved", "Closed"),
            ("Resolved", "In Progress"),
        ],
    },
    {
        "type": "Problem",
        "workflow": "Problem Workflow",
        "states": [
            ("New", "backlog", True),
            ("Investigating", "started", False),
            ("Known Error", "started", False),
            ("Resolved", "completed", False),
            ("Closed", "completed", False),
        ],
        "transitions": [
            ("New", "Investigating"),
            ("Investigating", "Known Error"),
            ("Investigating", "Resolved"),
            ("Known Error", "Resolved"),
            ("Known Error", "Investigating"),
            ("Resolved", "Closed"),
            ("Resolved", "Investigating"),
        ],
    },
    {
        "type": "Change",
        "workflow": "Change Workflow",
        "states": [
            ("New", "backlog", True),
            ("Assessment", "started", False),
            ("Approval", "started", False),
            ("Implementation", "started", False),
            ("Review", "started", False),
            ("Closed", "completed", False),
        ],
        "transitions": [
            ("New", "Assessment"),
            ("Assessment", "Approval"),
            ("Assessment", "Closed"),
            ("Approval", "Implementation"),
            ("Approval", "Assessment"),
            ("Implementation", "Review"),
            ("Review", "Closed"),
            ("Review", "Implementation"),
        ],
    },
    {
        "type": "Improvement",
        "workflow": "Improvement Workflow",
        "states": [
            ("New", "backlog", True),
            ("In Progress", "started", False),
            ("Done", "completed", False),
        ],
        "transitions": [
            ("New", "In Progress"),
            ("In Progress", "Done"),
            ("In Progress", "New"),
            ("Done", "In Progress"),
        ],
    },
]

GROUP_COLORS = {
    "backlog": "#60646C",
    "unstarted": "#60646C",
    "started": "#F59E0B",
    "completed": "#46A758",
    "cancelled": "#9AA4BC",
}


def seed_default_workflows(apps, schema_editor):
    Workspace = apps.get_model("db", "Workspace")
    IssueType = apps.get_model("db", "IssueType")
    Workflow = apps.get_model("db", "Workflow")
    WorkflowState = apps.get_model("db", "WorkflowState")
    WorkflowTransition = apps.get_model("db", "WorkflowTransition")

    for workspace in Workspace.objects.filter(deleted_at__isnull=True).iterator():
        for seed in WORKFLOW_SEEDS:
            issue_type, _ = IssueType.objects.get_or_create(
                workspace_id=workspace.id,
                name=seed["type"],
                deleted_at__isnull=True,
                defaults={"is_epic": False, "is_default": False, "is_active": True},
            )
            workflow, _ = Workflow.objects.get_or_create(
                workspace_id=workspace.id,
                name=seed["workflow"],
                deleted_at__isnull=True,
                defaults={"is_active": True},
            )
            if issue_type.workflow_id != workflow.id:
                issue_type.workflow_id = workflow.id
                issue_type.save(update_fields=["workflow"])

            state_map = {}
            for index, (name, group, is_default) in enumerate(seed["states"]):
                wf_state, _ = WorkflowState.objects.get_or_create(
                    workflow_id=workflow.id,
                    name=name,
                    deleted_at__isnull=True,
                    defaults={
                        "group": group,
                        "color": GROUP_COLORS[group],
                        "sequence": (index + 1) * 15000,
                        "is_default": is_default,
                    },
                )
                state_map[name] = wf_state.id

            for from_name, to_name in seed["transitions"]:
                WorkflowTransition.objects.get_or_create(
                    workflow_id=workflow.id,
                    from_state_id=state_map[from_name],
                    to_state_id=state_map[to_name],
                    deleted_at__isnull=True,
                )


class Migration(migrations.Migration):
    dependencies = [("db", "0123_workflow_workflowstate_workflowtransition_and_more")]

    operations = [
        migrations.RunPython(seed_default_workflows, reverse_code=migrations.RunPython.noop),
    ]
```

Penting: ganti string dependency `0123_workflow_workflowstate_workflowtransition_and_more` dengan nama file yang benar-benar dihasilkan di Task A3.

- [ ] **Step 3: Jalankan migrasi**

Run: `docker compose -f docker-compose-local.yml run --rm migrator python manage.py migrate db`

Expected: `Applying db.0124_seed_default_workflows` OK.

- [ ] **Step 4: Verifikasi seed**

Run:

```bash
docker compose -f docker-compose-local.yml exec plane-db psql -U plane -d plane -c "SELECT t.name AS type, w.name AS workflow, count(ws.id) AS states FROM issue_types t JOIN workflows w ON w.id = t.workflow_id JOIN workflow_states ws ON ws.workflow_id = w.id WHERE t.deleted_at IS NULL GROUP BY 1,2 ORDER BY 1;"
```

Expected: 4 baris — Change/6, Improvement/3, Incident/5, Problem/5.

- [ ] **Step 5: Verifikasi idempotensi (jalankan ulang)**

Run: `docker compose -f docker-compose-local.yml run --rm migrator python manage.py migrate db 0123`

Lalu: `docker compose -f docker-compose-local.yml run --rm migrator python manage.py migrate db`

Expected: 0124 applied ulang tanpa error dan tanpa duplikat (count tetap sama).

- [ ] **Step 6: Commit**

```bash
git add apps/api/plane/db/migrations/0124_seed_default_workflows.py
git commit -m "feat(db): seed default ITSM workflows and types"
```

---

## Milestone B — api-rs: workflow CRUD + materialization

Semua test Rust dijalankan dari `apps/api-rs`. Test murni tidak butuh DB; test ber-tag DB butuh `DATABASE_URL` (default `postgres://plane:plane@localhost:5432/plane`). Jalankan Postgres lokal via stack yang sudah ada atau container `plane-db`.

### Task B1: Validator murni `workflow.rs`

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Test: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Daftarkan modul**

Di `apps/api-rs/crates/api/src/routes/mod.rs`, sisipkan sebelum `pub mod work_item;`:

```rust
pub mod workflow;
```

- [ ] **Step 2: Tulis test yang gagal**

```rust
// apps/api-rs/crates/api/tests/workflow_test.rs
use api::routes::workflow::{allowed_target_state_ids, validate_name, validate_state_group};
use uuid::Uuid;

#[test]
fn name_required_and_max_255() {
    assert!(validate_name("  ", "name").is_err());
    assert!(validate_name(&"x".repeat(256), "name").is_err());
    assert_eq!(validate_name(" Incident ", "name").unwrap(), "Incident");
}

#[test]
fn group_must_be_known() {
    assert!(validate_state_group("backlog").is_ok());
    assert!(validate_state_group("triage").is_err());
    assert!(validate_state_group("bogus").is_err());
}

#[test]
fn allowed_targets_follow_transitions() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let mirror_closed = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_closed = Uuid::new_v4();
    let pairs = vec![
        (mirror_new, wf_new),
        (mirror_progress, wf_progress),
        (mirror_closed, wf_closed),
    ];
    let transitions = vec![(wf_new, wf_progress), (wf_progress, wf_closed)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
    assert_eq!(
        allowed_target_state_ids(mirror_progress, &pairs, &transitions),
        vec![mirror_closed]
    );
    assert!(allowed_target_state_ids(mirror_closed, &pairs, &transitions).is_empty());
    assert!(allowed_target_state_ids(Uuid::new_v4(), &pairs, &transitions).is_empty());
}
```

- [ ] **Step 3: Jalankan test untuk memastikan gagal**

Run: `cargo test -p api --test workflow_test`

Expected: FAIL — module `workflow` tidak ada.

- [ ] **Step 4: Implementasi validator**

```rust
// apps/api-rs/crates/api/src/routes/workflow.rs
//! Workflows workspace-level (state + transisi) dan materialization state ke
//! project. Spec: docs/superpowers/specs/2026-09-25-work-item-types-workflows-design.md

use uuid::Uuid;

/// Group valid untuk workflow state; `triage` bukan bagian workflow
/// (`StateGroup` di `apps/api/plane/db/models/state.py:14-20`).
pub const STATE_GROUPS: [&str; 5] = ["backlog", "unstarted", "started", "completed", "cancelled"];

/// Validasi nama ala DRF (required, <=255). Mengembalikan nama ter-trim.
pub fn validate_name(name: &str, field: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.chars().count() > 255 {
        return Err(format!("Ensure {field} has no more than 255 characters."));
    }
    Ok(trimmed.to_string())
}

pub fn validate_state_group(group: &str) -> Result<(), String> {
    if STATE_GROUPS.contains(&group) {
        Ok(())
    } else {
        Err("\"group\" is not a valid choice.".to_string())
    }
}

/// Target state (mirror) yang diizinkan dari `current_state`, dihitung murni
/// dari pasangan `(state_id, workflow_state_id)` dan daftar transisi
/// `(from_workflow_state_id, to_workflow_state_id)`.
pub fn allowed_target_state_ids(
    current_state: Uuid,
    mirror_pairs: &[(Uuid, Uuid)],
    transitions: &[(Uuid, Uuid)],
) -> Vec<Uuid> {
    let Some(current_workflow_state) = mirror_pairs
        .iter()
        .find(|(state_id, _)| *state_id == current_state)
        .map(|(_, workflow_state_id)| *workflow_state_id)
    else {
        return Vec::new();
    };
    let allowed_workflow_states: Vec<Uuid> = transitions
        .iter()
        .filter(|(from, _)| *from == current_workflow_state)
        .map(|(_, to)| *to)
        .collect();
    mirror_pairs
        .iter()
        .filter(|(_, workflow_state_id)| allowed_workflow_states.contains(workflow_state_id))
        .map(|(state_id, _)| *state_id)
        .collect()
}
```

- [ ] **Step 5: Jalankan test untuk memastikan lulus**

Run: `cargo test -p api --test workflow_test`

Expected: PASS (3 passed).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): workflow validators"
```

---

### Task B2: Materialization state ke project

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tambah test DB yang gagal**

Tambahkan ke `workflow_test.rs`:

```rust
use api::middleware::auth::AuthUser;
use api::routes::workflow::materialize_type_states;
use api::routes::workspace::create;
use api::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::json;
use sqlx::PgPool;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
}

async fn app_state() -> AppState {
    AppState {
        pool: pool().await,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) {
    sqlx::query(
        "INSERT INTO users (id, password, username, email, first_name, last_name, avatar, \
         date_joined, created_at, updated_at, last_location, created_location, is_superuser, \
         is_managed, is_password_expired, is_active, is_staff, is_email_verified, \
         is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip, \
         last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid, \
         is_password_reset_required) \
         VALUES ($1, '', $2, $3, '', '', '', now(), now(), now(), '', '', false, false, \
         false, true, false, false, true, $4, 'UTC', '', '', '', '', false, $2, true, false)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
}

async fn purge(pool: &PgPool, slug: &str) {
    let ws: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .expect("purge lookup");
    let Some((ws_id,)) = ws else { return };
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT member_id FROM workspace_members WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_all(pool)
            .await
            .expect("purge members");
    for stmt in [
        "DELETE FROM workflow_transitions WHERE workspace_id = $1",
        "DELETE FROM workflow_states WHERE workspace_id = $1",
        "DELETE FROM workflows WHERE workspace_id = $1",
        "DELETE FROM project_issue_types WHERE workspace_id = $1",
        "DELETE FROM issue_types WHERE workspace_id = $1",
        "DELETE FROM states WHERE workspace_id = $1",
        "DELETE FROM project_members WHERE workspace_id = $1",
        "DELETE FROM projects WHERE workspace_id = $1",
        "DELETE FROM workspace_members WHERE workspace_id = $1",
        "DELETE FROM workspaces WHERE id = $1",
    ] {
        sqlx::query(stmt).bind(ws_id).execute(pool).await.expect("purge");
    }
    for (member_id,) in members {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(member_id)
            .execute(pool)
            .await
            .expect("purge user");
    }
}

async fn make_workspace(st: &AppState, prefix: &str) -> (String, Uuid, Uuid) {
    let slug = format!("{prefix}-{}", Uuid::new_v4().simple());
    let owner = Uuid::new_v4();
    insert_user(&st.pool, owner, &slug).await;
    let (status, _) = create(
        State(st.clone()),
        AuthUser(owner),
        Json(json!({"name": "Acme IT", "slug": slug})),
    )
    .await
    .expect("workspace create");
    assert_eq!(status, StatusCode::CREATED);
    let (ws_id,): (Uuid,) = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("workspace row");
    let (project_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM projects WHERE workspace_id = $1 AND deleted_at IS NULL")
            .bind(ws_id)
            .fetch_one(&st.pool)
            .await
            .expect("project row");
    (slug, ws_id, project_id)
}

#[tokio::test]
async fn materialize_creates_and_updates_mirrors() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfm").await;

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("issue type");

    let (workflow_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workflows (id, name, description, is_active, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident Workflow', '', true, $1, now(), now()) RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("workflow");

    sqlx::query(
        "INSERT INTO workflow_states (id, name, description, color, slug, sequence, \"group\", \
         is_default, workflow_id, created_at, updated_at) VALUES \
         (gen_random_uuid(), 'New', '', '#60646C', 'new', 15000, 'backlog', true, $1, now(), now()), \
         (gen_random_uuid(), 'In Progress', '', '#F59E0B', 'in-progress', 30000, 'started', false, $1, now(), now())",
    )
    .bind(workflow_id)
    .execute(&st.pool)
    .await
    .expect("workflow states");

    sqlx::query("UPDATE issue_types SET workflow_id = $1 WHERE id = $2")
        .bind(workflow_id)
        .bind(type_id)
        .execute(&st.pool)
        .await
        .expect("attach workflow");

    sqlx::query(
        "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, level, is_default, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 0, false, now(), now())",
    )
    .bind(type_id)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("project type link");

    let written = materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize");
    assert_eq!(written, 2);

    let (count, defaults): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE \"default\") FROM states \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("state count");
    assert_eq!(count, 2);
    assert_eq!(defaults, 1);

    // Idempotent: panggilan kedua tidak menambah row.
    materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize again");
    let (count_again,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("state count 2");
    assert_eq!(count_again, 2);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test materialize_creates_and_updates_mirrors`

Expected: FAIL — `materialize_type_states` belum ada.

- [ ] **Step 3: Implementasi materialization**

Tambahkan ke `workflow.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::bad;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// Materialize seluruh state workflow milik `type_id` ke project (idempotent).
/// Return jumlah mirror yang ditulis (update + insert).
pub async fn materialize_type_states(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let workflow_id: Option<Uuid> =
        sqlx::query_scalar("SELECT workflow_id FROM issue_types WHERE id = $1 AND deleted_at IS NULL")
            .bind(type_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let Some(workflow_id) = workflow_id else {
        return Ok(0);
    };
    materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await
}

/// Inti materialization: turunkan default lama, revive + update mirror,
/// insert mirror baru, soft-delete mirror yatim. Satu transaksi.
pub(crate) async fn materialize_workflow_for_project(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
    workflow_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE states SET \"default\" = false, updated_at = now() \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL AND \"default\" = true",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;

    let updated = sqlx::query(
        "UPDATE states s SET name = ws.name, description = ws.description, color = ws.color, \
         slug = ws.slug, sequence = ws.sequence, \"group\" = ws.\"group\", \"default\" = ws.is_default, \
         deleted_at = NULL, updated_at = now() \
         FROM workflow_states ws \
         WHERE s.workflow_state_id = ws.id AND s.project_id = $1 AND s.type_id = $2 \
           AND ws.workflow_id = $3 AND ws.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let inserted = sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, workflow_state_id, created_at, updated_at) \
         SELECT gen_random_uuid(), ws.name, ws.description, ws.color, ws.slug, ws.sequence, ws.\"group\", \
         false, ws.is_default, $1, p.workspace_id, $2, ws.id, now(), now() \
         FROM workflow_states ws JOIN projects p ON p.id = $1 \
         WHERE ws.workflow_id = $3 AND ws.deleted_at IS NULL \
         AND NOT EXISTS (SELECT 1 FROM states s WHERE s.project_id = $1 AND s.workflow_state_id = ws.id)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(workflow_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    sqlx::query(
        "UPDATE states s SET deleted_at = now(), updated_at = now() \
         WHERE s.project_id = $1 AND s.type_id = $2 AND s.deleted_at IS NULL \
         AND s.workflow_state_id IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM workflow_states ws \
                         WHERE ws.id = s.workflow_state_id AND ws.deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(updated + inserted)
}

/// Sync satu workflow ke semua project hidup yang mengaktifkan type-nya.
pub(crate) async fn sync_workflow_to_projects(
    pool: &sqlx::PgPool,
    workflow_id: Uuid,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT DISTINCT pit.project_id, t.id FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         JOIN projects p ON p.id = pit.project_id \
         WHERE t.workflow_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND p.deleted_at IS NULL AND p.archived_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_all(pool)
    .await?;
    for (project_id, type_id) in rows {
        materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await?;
    }
    Ok(())
}

/// Pastikan semua type yang aktif di project sudah ter-materialize.
pub(crate) async fn ensure_project_workflows(
    pool: &sqlx::PgPool,
    project_id: Uuid,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT pit.issue_type_id, t.workflow_id FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND t.workflow_id IS NOT NULL",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    for (type_id, workflow_id) in rows {
        materialize_workflow_for_project(pool, project_id, type_id, workflow_id).await?;
    }
    Ok(())
}
```

Catatan: `let workflow_id: Option<Uuid> = sqlx::query_scalar(...).fetch_optional(pool).await?.flatten();` valid — `query_scalar` meng-infer `Option<Uuid>` sebagai tipe scalar sehingga `fetch_optional` menghasilkan `Option<Option<Uuid>>`.

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test`

Expected: PASS (4 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): materialize workflow states per project"
```

---

### Task B3: CRUD workflow

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tambah test DB yang gagal**

```rust
use api::routes::workflow::{create_workflow, delete_workflow, list_workflows, WorkflowBody};
use axum::extract::Path;

#[tokio::test]
async fn workflow_crud_roundtrip() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfcrud").await;

    let (status, created) = create_workflow(
        State(st.clone()),
        AuthUser(Uuid::new_v4()),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: Some("ITIL incident".into()),
            is_active: None,
        }),
    )
    .await
    .expect("create");
    // Pemanggil bukan member workspace → 403; test kepemilikan admin ada di
    // test terpisah. Untuk roundtrip, pakai owner dari make_workspace.
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (owner,): (Uuid,) = sqlx::query_as(
        "SELECT owner_id FROM workspaces WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_one(&st.pool)
    .await
    .expect("owner");

    let (status, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: Some("ITIL incident".into()),
            is_active: None,
        }),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);
    let workflow_id = created["id"].as_str().expect("id").to_string();

    let (status, list) = list_workflows(State(st.clone()), AuthUser(owner), Path(slug.clone()))
        .await
        .expect("list");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().expect("array").len(), 1);
    assert_eq!(list[0]["name"], "Incident Workflow");

    let (status, _) = delete_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), Uuid::parse_str(&workflow_id).unwrap())),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = list_workflows(State(st.clone()), AuthUser(owner), Path(slug.clone()))
        .await
        .expect("list after delete");
    assert!(list.as_array().expect("array").is_empty());

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test workflow_crud_roundtrip`

Expected: FAIL — `WorkflowBody`/handler belum ada.

- [ ] **Step 3: Implementasi**

Tambahkan ke `workflow.rs`:

```rust
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, is_integrity_error, missing, ws_role},
    state::AppState,
};

use super::issue_common::bad;

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

fn is_integrity_err(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| is_integrity_error(c.as_ref()))
        .unwrap_or(false)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub workspace_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub fn workflow_json(row: &WorkflowRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "description": row.description,
        "is_active": row.is_active,
        "workspace_id": row.workspace_id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkflowBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

async fn is_ws_admin(st: &AppState, user: Uuid, slug: &str) -> Result<bool, sqlx::Error> {
    Ok(matches!(ws_role(&st.pool, user, slug).await?, Some(r) if r >= 20))
}

async fn workflow_in_workspace(
    pool: &sqlx::PgPool,
    slug: &str,
    workflow_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let (ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflows w JOIN workspaces ws ON ws.id = w.workspace_id \
         WHERE w.id = $1 AND ws.slug = $2 AND w.deleted_at IS NULL AND ws.deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .bind(slug)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// GET `/api/workspaces/:slug/workflows/`
pub async fn list_workflows(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    let rows: Vec<WorkflowRow> = sqlx::query_as(
        "SELECT w.id, w.name, w.description, w.is_active, w.workspace_id, w.created_at, w.updated_at \
         FROM workflows w JOIN workspaces ws ON ws.id = w.workspace_id \
         WHERE ws.slug = $1 AND w.deleted_at IS NULL ORDER BY w.created_at",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(workflow_json).collect::<Vec<_>>())),
    ))
}

/// POST `/api/workspaces/:slug/workflows/`
pub async fn create_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<WorkflowBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    let name = match validate_name(body.name.as_deref().unwrap_or(""), "name") {
        Ok(name) => name,
        Err(e) => return Ok(bad(&e)),
    };
    let inserted: Result<WorkflowRow, sqlx::Error> = sqlx::query_as(
        "INSERT INTO workflows (id, name, description, is_active, workspace_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, COALESCE($2, ''), COALESCE($3, true), w.id, $4, $4, now(), now() \
         FROM workspaces w WHERE w.slug = $5 AND w.deleted_at IS NULL \
         RETURNING id, name, description, is_active, workspace_id, created_at, updated_at",
    )
    .bind(&name)
    .bind(&body.description)
    .bind(body.is_active)
    .bind(auth.0)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await;
    match inserted {
        Ok(row) => Ok((StatusCode::CREATED, Json(workflow_json(&row)))),
        Err(e) if is_integrity_err(&e) => Ok(bad("Workflow with this name already exists")),
        Err(e) => Err(e.into()),
    }
}

/// GET `/api/workspaces/:slug/workflows/:workflow_id/`
pub async fn retrieve_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    let row: Option<WorkflowRow> = sqlx::query_as(
        "SELECT w.id, w.name, w.description, w.is_active, w.workspace_id, w.created_at, w.updated_at \
         FROM workflows w JOIN workspaces ws ON ws.id = w.workspace_id \
         WHERE w.id = $1 AND ws.slug = $2 AND w.deleted_at IS NULL",
    )
    .bind(workflow_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(row) => Ok((StatusCode::OK, Json(workflow_json(&row)))),
        None => Ok(missing()),
    }
}

/// PATCH `/api/workspaces/:slug/workflows/:workflow_id/`
pub async fn patch_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
    Json(body): Json<WorkflowBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let name = match body.name.as_deref() {
        Some(raw) => match validate_name(raw, "name") {
            Ok(name) => Some(name),
            Err(e) => return Ok(bad(&e)),
        },
        None => None,
    };
    let updated: Result<WorkflowRow, sqlx::Error> = sqlx::query_as(
        "UPDATE workflows SET name = COALESCE($3, name), description = COALESCE($4, description), \
         is_active = COALESCE($5, is_active), updated_by_id = $6, updated_at = now() \
         WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL \
         RETURNING id, name, description, is_active, workspace_id, created_at, updated_at",
    )
    .bind(workflow_id)
    .bind(&slug)
    .bind(&name)
    .bind(&body.description)
    .bind(body.is_active)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await;
    match updated {
        Ok(row) => Ok((StatusCode::OK, Json(workflow_json(&row)))),
        Err(e) if is_integrity_err(&e) => Ok(bad("Workflow with this name already exists")),
        Err(e) => Err(e.into()),
    }
}

/// DELETE `/api/workspaces/:slug/workflows/:workflow_id/`
pub async fn delete_workflow(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issue_types WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Workflow is in use by a work item type"));
    }
    sqlx::query(
        "UPDATE workflows SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test`

Expected: PASS (5 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): workflow CRUD routes"
```

---

### Task B4: CRUD workflow state + sync mirror

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tambah test DB yang gagal**

```rust
use api::routes::workflow::{create_state, list_states, WorkflowStateBody};

#[tokio::test]
async fn state_create_syncs_mirror_and_default() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfstate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let workflow_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(workflow_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    sqlx::query(
        "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, level, is_default, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 0, false, now(), now())",
    )
    .bind(type_id)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("link");

    let (status, _) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            description: None,
            color: None,
            group: None,
            sequence: None,
            is_default: None,
        }),
    )
    .await
    .expect("state");
    assert_eq!(status, StatusCode::CREATED);

    let (name, is_default, type_match): (String, bool, Uuid) = sqlx::query_as(
        "SELECT s.name, s.\"default\", s.type_id FROM states s \
         WHERE s.project_id = $1 AND s.workflow_state_id IS NOT NULL AND s.deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await
    .expect("mirror");
    assert_eq!(name, "New");
    assert!(is_default, "state pertama otomatis jadi default");
    assert_eq!(type_match, type_id);

    let (_, list) = list_states(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("list");
    assert_eq!(list.as_array().unwrap().len(), 1);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test state_create_syncs_mirror_and_default`

Expected: FAIL — handler belum ada.

- [ ] **Step 3: Implementasi**

Tambahkan ke `workflow.rs`:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowStateRow {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub slug: String,
    pub sequence: f64,
    pub group: String,
    pub is_default: bool,
}

pub fn workflow_state_json(row: &WorkflowStateRow) -> Value {
    json!({
        "id": row.id,
        "workflow_id": row.workflow_id,
        "name": row.name,
        "description": row.description,
        "color": row.color,
        "slug": row.slug,
        "sequence": row.sequence,
        "group": row.group,
        "is_default": row.is_default,
    })
}

const WF_STATE_COLS: &str = "id, workflow_id, name, description, color, slug, sequence, \"group\", is_default";

#[derive(Debug, Deserialize, Default)]
pub struct WorkflowStateBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub sequence: Option<f64>,
    #[serde(default)]
    pub is_default: Option<bool>,
}

fn default_color(group: &str) -> &'static str {
    match group {
        "started" => "#F59E0B",
        "completed" => "#46A758",
        "cancelled" => "#9AA4BC",
        _ => "#60646C",
    }
}

/// GET `/api/workspaces/:slug/workflows/:workflow_id/states/`
pub async fn list_states(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let rows: Vec<WorkflowStateRow> = sqlx::query_as(&format!(
        "SELECT {WF_STATE_COLS} FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL ORDER BY sequence"
    ))
    .bind(workflow_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(workflow_state_json).collect::<Vec<_>>())),
    ))
}

/// POST `/api/workspaces/:slug/workflows/:workflow_id/states/`
pub async fn create_state(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
    Json(body): Json<WorkflowStateBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let name = match validate_name(body.name.as_deref().unwrap_or(""), "name") {
        Ok(name) => name,
        Err(e) => return Ok(bad(&e)),
    };
    let group = body.group.clone().unwrap_or_else(|| "backlog".to_string());
    if let Err(e) = validate_state_group(&group) {
        return Ok(bad(&e));
    }
    let color = body.color.clone().unwrap_or_else(|| default_color(&group).to_string());

    let mut tx = st.pool.begin().await?;
    let (state_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_one(&mut *tx)
    .await?;
    let make_default = body.is_default.unwrap_or(state_count == 0) || state_count == 0;
    if make_default {
        sqlx::query(
            "UPDATE workflow_states SET is_default = false, updated_at = now() \
             WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default = true",
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
    }
    let inserted: Result<WorkflowStateRow, sqlx::Error> = sqlx::query_as(&format!(
        "INSERT INTO workflow_states (id, workflow_id, name, description, color, slug, sequence, \
         \"group\", is_default, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, COALESCE($3, ''), $4, \
         regexp_replace(lower($2), '[^a-z0-9]+', '-', 'g'), \
         COALESCE($5, (SELECT COALESCE(MAX(sequence), 0) + 15000 FROM workflow_states WHERE workflow_id = $1)), \
         $6, $7, $8, $8, now(), now()) RETURNING {WF_STATE_COLS}"
    ))
    .bind(workflow_id)
    .bind(&name)
    .bind(&body.description)
    .bind(&color)
    .bind(body.sequence)
    .bind(&group)
    .bind(make_default)
    .bind(auth.0)
    .fetch_one(&mut *tx)
    .await;
    let row = match inserted {
        Ok(row) => row,
        Err(e) if is_integrity_err(&e) => {
            tx.rollback().await?;
            return Ok(bad("Workflow state with this name already exists"));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;

    sync_workflow_to_projects(&st.pool, workflow_id).await?;
    Ok((StatusCode::CREATED, Json(workflow_state_json(&row))))
}

/// PATCH `/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/`
pub async fn patch_state(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id, state_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<WorkflowStateBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let current: Option<WorkflowStateRow> = sqlx::query_as(&format!(
        "SELECT {WF_STATE_COLS} FROM workflow_states WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL"
    ))
    .bind(state_id)
    .bind(workflow_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok(missing());
    };
    let name = match body.name.as_deref() {
        Some(raw) => match validate_name(raw, "name") {
            Ok(name) => Some(name),
            Err(e) => return Ok(bad(&e)),
        },
        None => None,
    };
    let group = body.group.clone().unwrap_or_else(|| current.group.clone());
    if let Err(e) = validate_state_group(&group) {
        return Ok(bad(&e));
    }
    if body.is_default == Some(false) && current.is_default {
        return Ok(bad("A workflow must have a default state"));
    }

    let mut tx = st.pool.begin().await?;
    if body.is_default == Some(true) && !current.is_default {
        sqlx::query(
            "UPDATE workflow_states SET is_default = false, updated_at = now() \
             WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default = true",
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
    }
    let updated: Result<WorkflowStateRow, sqlx::Error> = sqlx::query_as(&format!(
        "UPDATE workflow_states SET name = COALESCE($3, name), description = COALESCE($4, description), \
         color = COALESCE($5, color), \"group\" = $6, sequence = COALESCE($7, sequence), \
         is_default = COALESCE($8, is_default), updated_by_id = $9, updated_at = now() \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL RETURNING {WF_STATE_COLS}"
    ))
    .bind(state_id)
    .bind(workflow_id)
    .bind(&name)
    .bind(&body.description)
    .bind(&body.color)
    .bind(&group)
    .bind(body.sequence)
    .bind(body.is_default)
    .bind(auth.0)
    .fetch_one(&mut *tx)
    .await;
    let row = match updated {
        Ok(row) => row,
        Err(e) if is_integrity_err(&e) => {
            tx.rollback().await?;
            return Ok(bad("Workflow state with this name already exists"));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;

    sync_workflow_to_projects(&st.pool, workflow_id).await?;
    Ok((StatusCode::OK, Json(workflow_state_json(&row))))
}

/// DELETE `/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/`
pub async fn delete_state(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id, state_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflow_states WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL)",
    )
    .bind(state_id)
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await?;
    if !exists {
        return Ok(missing());
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues i JOIN states s ON s.id = i.state_id \
         WHERE s.workflow_state_id = $1 AND i.deleted_at IS NULL)",
    )
    .bind(state_id)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Workflow state is in use by work items"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE workflow_transitions SET deleted_at = now(), updated_at = now() \
         WHERE workflow_id = $1 AND (from_state_id = $2 OR to_state_id = $2) AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .bind(state_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE workflow_states SET deleted_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(state_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    sync_workflow_to_projects(&st.pool, workflow_id).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test`

Expected: PASS (6 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): workflow state CRUD with project sync"
```

---

### Task B5: CRUD workflow transition

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tambah test DB yang gagal**

```rust
use api::routes::workflow::{create_transition, list_transitions, TransitionBody};

#[tokio::test]
async fn transition_crud_validates_states() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wftr").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let (_, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let workflow_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (_, new) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state new");
    let (_, progress) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("In Progress".into()),
            group: Some("started".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state progress");
    let new_id = Uuid::parse_str(new["id"].as_str().unwrap()).unwrap();
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();

    let (status, _) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("transition");
    assert_eq!(status, StatusCode::CREATED);

    // Duplikat → 400.
    let (status, _) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("dup");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Self transition → 400.
    let (status, _) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(new_id),
        }),
    )
    .await
    .expect("self");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, list) = list_transitions(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("list");
    assert_eq!(list.as_array().unwrap().len(), 1);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test transition_crud_validates_states`

Expected: FAIL — handler belum ada.

- [ ] **Step 3: Implementasi**

Tambahkan ke `workflow.rs`:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowTransitionRow {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub from_state_id: Uuid,
    pub to_state_id: Uuid,
}

pub fn transition_json(row: &WorkflowTransitionRow) -> Value {
    json!({
        "id": row.id,
        "workflow_id": row.workflow_id,
        "from_state_id": row.from_state_id,
        "to_state_id": row.to_state_id,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct TransitionBody {
    #[serde(default)]
    pub from_state_id: Option<Uuid>,
    #[serde(default)]
    pub to_state_id: Option<Uuid>,
}

async fn state_in_workflow(
    pool: &sqlx::PgPool,
    workflow_id: Uuid,
    state_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let (ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflow_states WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL)",
    )
    .bind(state_id)
    .bind(workflow_id)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// GET `/api/workspaces/:slug/workflows/:workflow_id/transitions/`
pub async fn list_transitions(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let rows: Vec<WorkflowTransitionRow> = sqlx::query_as(
        "SELECT id, workflow_id, from_state_id, to_state_id FROM workflow_transitions \
         WHERE workflow_id = $1 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(workflow_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(transition_json).collect::<Vec<_>>())),
    ))
}

/// POST `/api/workspaces/:slug/workflows/:workflow_id/transitions/`
pub async fn create_transition(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id)): Path<(String, Uuid)>,
    Json(body): Json<TransitionBody>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let (Some(from_state_id), Some(to_state_id)) = (body.from_state_id, body.to_state_id) else {
        return Ok(bad("from_state_id and to_state_id are required"));
    };
    if from_state_id == to_state_id {
        return Ok(bad("A transition cannot start and end on the same state"));
    }
    if !state_in_workflow(&st.pool, workflow_id, from_state_id).await?
        || !state_in_workflow(&st.pool, workflow_id, to_state_id).await?
    {
        return Ok(bad("States must belong to this workflow"));
    }
    let inserted: Result<WorkflowTransitionRow, sqlx::Error> = sqlx::query_as(
        "INSERT INTO workflow_transitions (id, workflow_id, from_state_id, to_state_id, \
         created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now()) \
         RETURNING id, workflow_id, from_state_id, to_state_id",
    )
    .bind(workflow_id)
    .bind(from_state_id)
    .bind(to_state_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await;
    match inserted {
        Ok(row) => Ok((StatusCode::CREATED, Json(transition_json(&row)))),
        Err(e) if is_integrity_err(&e) => Ok(bad("This transition already exists")),
        Err(e) => Err(e.into()),
    }
}

/// DELETE `/api/workspaces/:slug/workflows/:workflow_id/transitions/:transition_id/`
pub async fn delete_transition(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, workflow_id, transition_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !is_ws_admin(&st, auth.0, &slug).await? {
        return Ok(deny());
    }
    if !workflow_in_workspace(&st.pool, &slug, workflow_id).await? {
        return Ok(missing());
    }
    let rows = sqlx::query(
        "UPDATE workflow_transitions SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND workflow_id = $2 AND deleted_at IS NULL",
    )
    .bind(transition_id)
    .bind(workflow_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if rows == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test`

Expected: PASS (7 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): workflow transition CRUD"
```

---

### Task B6: Type internal, import + materialize, un-enable, workflow-map

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`
- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tambah kolom `workflow` di payload + body v1**

Di `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`:

1. Tambahkan `t.workflow_id AS workflow,` ke `TYPE_COLS` (setelah `t.level::int AS level,`).
2. Tambahkan field ke `V1WorkItemTypeRow`:

```rust
    pub workflow: Option<uuid::Uuid>,
```

3. Tambahkan ke `v1_work_item_type_json`:

```rust
        "workflow": row.workflow,
```

4. Tambahkan `workflow` ke `V1CreateWorkItemType` dan `V1UpdateWorkItemType`:

```rust
    #[serde(default)]
    pub workflow: Option<uuid::Uuid>,
```

5. Di `create_type` (INSERT `issue_types`), tambahkan kolom + bind `workflow_id`. Ubah daftar kolom dan `VALUES` agar memuat `workflow_id` (bind `body.workflow`). Pastikan urutan bind konsisten dengan placeholder `$N`.
6. Di `update_type` (UPDATE kolom-by-kolom), tambahkan:

```rust
        workflow_id = COALESCE($N, workflow_id),
```

sesuai nomor bind berikutnya, dan bind `body.workflow` di posisi yang sama.

- [ ] **Step 2: Materialize saat import**

Di `import_to_project` (akhir fungsi, setelah semua link di-insert), tambahkan:

```rust
    for type_id in ids {
        crate::routes::workflow::materialize_type_states(&st.pool, project_id, type_id).await?;
    }
```

Sesuaikan nama variabel dengan kode aktual (fungsi saat ini meloop `body.get("work_item_types")`); kumpulkan dulu id-nya ke `Vec<uuid::Uuid>`, lalu loop setelah link.

- [ ] **Step 3: Guard delete type**

Di `delete_workspace` (v1), sebelum soft-delete, tambahkan guard:

```rust
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE type_id = $1 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(crate::routes::issue_common::bad(
            "Type is in use by work items",
        ));
    }
```

Tambahkan guard yang sama di `delete_project` (scope project) dengan tambahan `AND project_id = $2`.

- [ ] **Step 4: Tulis test yang gagal untuk import/un-enable/workflow-map**

```rust
use api::routes::v1::work_item_type::import_to_project;
use api::routes::workflow::{unlink_type, workflow_map};

#[tokio::test]
async fn import_materializes_and_unlink_guards() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wftype").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Buat type + workflow + 2 states lewat helper handler.
    let (_, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let workflow_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state");
    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(workflow_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");

    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(json!({"work_item_types": [type_id]})),
    )
    .await
    .expect("import");
    assert_eq!(status, StatusCode::OK);

    let (mirrors,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("mirrors");
    assert_eq!(mirrors, 1);

    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(map["types"].as_array().unwrap().len(), 1);
    assert_eq!(map["types"][0]["type_id"], type_id.to_string());
    assert_eq!(map["types"][0]["states"].as_array().unwrap().len(), 1);

    // Un-enable saat belum ada issue → 204.
    let (status, _) = unlink_type(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, type_id)),
    )
    .await
    .expect("unlink");
    assert_eq!(status, StatusCode::NO_CONTENT);

    purge(&st.pool, &slug).await;
}
```

Catatan: `workflow_map` tidak punya body; hapus import `WorkflowMapBody` dari test (tidak dipakai).

- [ ] **Step 5: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test import_materializes_and_unlink_guards`

Expected: FAIL — `unlink_type`/`workflow_map` belum ada.

- [ ] **Step 6: Implementasi `unlink_type` + `workflow_map`**

Tambahkan ke `workflow.rs`:

```rust
use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// DELETE `/api/workspaces/:slug/projects/:project_id/work-item-types/:pk/`
/// (un-enable type dari project).
pub async fn unlink_type(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, type_id)): Path<(String, Uuid, Uuid)>,
) -> R {
    if !super::v1::work_item_type::can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Type is in use by work items"));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE project_issue_types SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE states SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// GET `/api/workspaces/:slug/projects/:project_id/workflow-map/`
pub async fn workflow_map(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> R {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(role.is_some(), role.is_some(), ws_admin) {
        return Ok(deny());
    }
    ensure_project_workflows(&st.pool, project_id).await?;

    let types: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT t.id, t.workflow_id, t.name FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         JOIN projects p ON p.id = pit.project_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
           AND t.workflow_id IS NOT NULL AND p.deleted_at IS NULL \
         ORDER BY t.name",
    )
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;

    let mut out = Vec::new();
    for (type_id, workflow_id, type_name) in types {
        let states: Vec<(Uuid, String, String, String, f64, bool)> = sqlx::query_as(
            "SELECT id, name, color, \"group\", sequence, \"default\" FROM states \
             WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL ORDER BY sequence",
        )
        .bind(project_id)
        .bind(type_id)
        .fetch_all(&st.pool)
        .await?;
        let default_state_id = states.iter().find(|s| s.5).map(|s| s.0);
        let transitions: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT mf.id, mt.id FROM workflow_transitions tr \
             JOIN states mf ON mf.workflow_state_id = tr.from_state_id AND mf.project_id = $1 AND mf.deleted_at IS NULL \
             JOIN states mt ON mt.workflow_state_id = tr.to_state_id AND mt.project_id = $1 AND mt.deleted_at IS NULL \
             WHERE tr.workflow_id = $2 AND tr.deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(workflow_id)
        .fetch_all(&st.pool)
        .await?;
        out.push(json!({
            "type_id": type_id,
            "type_name": type_name,
            "workflow_id": workflow_id,
            "default_state_id": default_state_id,
            "states": states.iter().map(|(id, name, color, group, sequence, is_default)| json!({
                "id": id, "name": name, "color": color, "group": group,
                "sequence": sequence, "is_default": is_default,
            })).collect::<Vec<_>>(),
            "transitions": transitions.iter().map(|(from, to)| json!({
                "from_state_id": from, "to_state_id": to,
            })).collect::<Vec<_>>(),
        }));
    }
    Ok((StatusCode::OK, Json(json!({ "types": out }))))
}
```

- [ ] **Step 7: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test`

Expected: PASS (8 passed). Juga jalankan test v1 lama: `cargo test -p api --test v1_work_item_type_test` — pastikan masih PASS setelah `workflow` ditambahkan.

- [ ] **Step 8: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): internal type routes, import materialization, workflow map"
```

---

### Task B7: Registrasi route + route inventory

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/parity-inventory.json`

- [ ] **Step 1: Registrasi route**

Di `main.rs`, setelah blok route states project (sekitar baris 644-656), tambahkan:

```rust
        // Work item types + workflows (workspace-level, opt-in per project).
        .route(
            "/api/workspaces/:slug/workflows/",
            get(routes::workflow::list_workflows).post(routes::workflow::create_workflow),
        )
        .route(
            "/api/workspaces/:slug/workflows/:workflow_id/",
            get(routes::workflow::retrieve_workflow)
                .patch(routes::workflow::patch_workflow)
                .delete(routes::workflow::delete_workflow),
        )
        .route(
            "/api/workspaces/:slug/workflows/:workflow_id/states/",
            get(routes::workflow::list_states).post(routes::workflow::create_state),
        )
        .route(
            "/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/",
            patch(routes::workflow::patch_state).delete(routes::workflow::delete_state),
        )
        .route(
            "/api/workspaces/:slug/workflows/:workflow_id/transitions/",
            get(routes::workflow::list_transitions).post(routes::workflow::create_transition),
        )
        .route(
            "/api/workspaces/:slug/workflows/:workflow_id/transitions/:transition_id/",
            delete(routes::workflow::delete_transition),
        )
        .route(
            "/api/workspaces/:slug/work-item-types/",
            get(routes::v1::work_item_type::list_workspace)
                .post(routes::v1::work_item_type::create_workspace),
        )
        .route(
            "/api/workspaces/:slug/work-item-types/:pk/",
            get(routes::v1::work_item_type::retrieve_workspace)
                .patch(routes::v1::work_item_type::update_workspace)
                .delete(routes::v1::work_item_type::delete_workspace),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/import-work-item-types/",
            post(routes::v1::work_item_type::import_to_project),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/work-item-types/:pk/",
            delete(routes::workflow::unlink_type),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/workflow-map/",
            get(routes::workflow::workflow_map),
        )
```

- [ ] **Step 2: Tambah domain inventory**

Tambahkan domain `"workflow"` ke `apps/api-rs/crates/api/parity-inventory.json` (setelah domain terakhir):

```json
    "workflow": {
      "rust_module": "routes/workflow.rs",
      "endpoints": [
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/workflows/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::list_workflows",
          "out_scope": false
        },
        {
          "methods": ["GET", "PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/workflows/:workflow_id/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::retrieve_workflow",
          "out_scope": false
        },
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/workflows/:workflow_id/states/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::list_states",
          "out_scope": false
        },
        {
          "methods": ["PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/workflows/:workflow_id/states/:state_id/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::patch_state",
          "out_scope": false
        },
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/workflows/:workflow_id/transitions/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::list_transitions",
          "out_scope": false
        },
        {
          "methods": ["DELETE"],
          "path": "/api/workspaces/:slug/workflows/:workflow_id/transitions/:transition_id/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::delete_transition",
          "out_scope": false
        },
        {
          "methods": ["GET", "POST"],
          "path": "/api/workspaces/:slug/work-item-types/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::v1::work_item_type::list_workspace",
          "out_scope": false
        },
        {
          "methods": ["GET", "PATCH", "DELETE"],
          "path": "/api/workspaces/:slug/work-item-types/:pk/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::v1::work_item_type::retrieve_workspace",
          "out_scope": false
        },
        {
          "methods": ["POST"],
          "path": "/api/workspaces/:slug/projects/:project_id/import-work-item-types/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::v1::work_item_type::import_to_project",
          "out_scope": false
        },
        {
          "methods": ["DELETE"],
          "path": "/api/workspaces/:slug/projects/:project_id/work-item-types/:pk/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::unlink_type",
          "out_scope": false
        },
        {
          "methods": ["GET"],
          "path": "/api/workspaces/:slug/projects/:project_id/workflow-map/",
          "django_source": "n/a (fitur baru)",
          "rust_status": "implemented",
          "rust_handler": "routes::workflow::workflow_map",
          "out_scope": false
        }
      ]
    }
```

- [ ] **Step 3: Jalankan route inventory + build**

Run: `cargo test -p api --test route_inventory_test`

Expected: PASS. Jika gagal karena path baru tidak ada di `main.rs`, perbaiki registrasi; jika karena duplikasi, cek `seen_paths`.

Run: `cargo build -p api`

Expected: build sukses (compile check route + handler).

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/parity-inventory.json
git commit -m "feat(api-rs): register workflow routes and inventory"
```

---

### Task B8: Seed workflow/type default di workspace baru

**Files:**

- Modify: `apps/api-rs/crates/api/src/seed.rs`
- Modify: `apps/api-rs/crates/api/tests/workspace_seed_test.rs`

- [ ] **Step 1: Tambah assertion test yang gagal**

Di `workspace_seed_test.rs`, setelah assertion `project_user_properties`, tambahkan:

```rust
    assert_eq!(count("workflows").await, 4);
    assert_eq!(count("issue_types").await, 4);
    assert_eq!(count("workflow_states").await, 19);
    assert_eq!(count("workflow_transitions").await, 25);
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workspace_seed_test`

Expected: FAIL — count 0.

- [ ] **Step 3: Implementasi seed**

Di `seed.rs`, tambahkan konstanta seed dan fungsi:

```rust
const WORKFLOW_SEEDS: &[(&str, &str, &[(&str, &str, bool)], &[(&str, &str)])] = &[
    (
        "Incident",
        "Incident Workflow",
        &[
            ("New", "backlog", true),
            ("In Progress", "started", false),
            ("On Hold", "started", false),
            ("Resolved", "completed", false),
            ("Closed", "completed", false),
        ],
        &[
            ("New", "In Progress"),
            ("In Progress", "On Hold"),
            ("In Progress", "Resolved"),
            ("On Hold", "In Progress"),
            ("Resolved", "Closed"),
            ("Resolved", "In Progress"),
        ],
    ),
    (
        "Problem",
        "Problem Workflow",
        &[
            ("New", "backlog", true),
            ("Investigating", "started", false),
            ("Known Error", "started", false),
            ("Resolved", "completed", false),
            ("Closed", "completed", false),
        ],
        &[
            ("New", "Investigating"),
            ("Investigating", "Known Error"),
            ("Investigating", "Resolved"),
            ("Known Error", "Resolved"),
            ("Known Error", "Investigating"),
            ("Resolved", "Closed"),
            ("Resolved", "Investigating"),
        ],
    ),
    (
        "Change",
        "Change Workflow",
        &[
            ("New", "backlog", true),
            ("Assessment", "started", false),
            ("Approval", "started", false),
            ("Implementation", "started", false),
            ("Review", "started", false),
            ("Closed", "completed", false),
        ],
        &[
            ("New", "Assessment"),
            ("Assessment", "Approval"),
            ("Assessment", "Closed"),
            ("Approval", "Implementation"),
            ("Approval", "Assessment"),
            ("Implementation", "Review"),
            ("Review", "Closed"),
            ("Review", "Implementation"),
        ],
    ),
    (
        "Improvement",
        "Improvement Workflow",
        &[
            ("New", "backlog", true),
            ("In Progress", "started", false),
            ("Done", "completed", false),
        ],
        &[
            ("New", "In Progress"),
            ("In Progress", "Done"),
            ("In Progress", "New"),
            ("Done", "In Progress"),
        ],
    ),
];

fn group_color(group: &str) -> &'static str {
    match group {
        "started" => "#F59E0B",
        "completed" => "#46A758",
        "cancelled" => "#9AA4BC",
        _ => "#60646C",
    }
}

/// Seed workflow + type default workspace (parity migrasi Django
/// `0124_seed_default_workflows`). Tidak mengaktifkan type di project mana pun.
async fn insert_workflows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    bot_id: Uuid,
) -> Result<(), sqlx::Error> {
    for (type_name, workflow_name, states, transitions) in WORKFLOW_SEEDS {
        let (workflow_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO workflows (id, name, description, is_active, workspace_id, created_by_id, \
             updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', true, $2, $3, $3, now(), now()) RETURNING id",
        )
        .bind(workflow_name)
        .bind(workspace_id)
        .bind(bot_id)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
             level, workflow_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', '{}', false, false, true, 0, $2, $3, $4, $4, now(), now())",
        )
        .bind(type_name)
        .bind(workflow_id)
        .bind(workspace_id)
        .bind(bot_id)
        .execute(&mut **tx)
        .await?;

        let mut state_ids = std::collections::HashMap::new();
        for (index, (name, group, is_default)) in states.iter().enumerate() {
            let (state_id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO workflow_states (id, workflow_id, name, description, color, slug, sequence, \
                 \"group\", is_default, created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, '', $3, $4, $5, $6, $7, $8, $8, now(), now()) \
                 RETURNING id",
            )
            .bind(workflow_id)
            .bind(name)
            .bind(group_color(group))
            .bind(slugify(name))
            .bind(((index as f64) + 1.0) * 15000.0)
            .bind(group)
            .bind(is_default)
            .bind(bot_id)
            .fetch_one(&mut **tx)
            .await?;
            state_ids.insert(*name, state_id);
        }

        for (from_name, to_name) in transitions.iter() {
            sqlx::query(
                "INSERT INTO workflow_transitions (id, workflow_id, from_state_id, to_state_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now())",
            )
            .bind(workflow_id)
            .bind(state_ids[from_name])
            .bind(state_ids[to_name])
            .bind(bot_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
```

Panggil `insert_workflows(&mut tx, workspace_id, bot_id).await?;` di `seed_workspace` setelah project/state/label/issue seed (di dalam transaksi yang sama, sebelum `tx.commit()`).

Catatan: `state_ids[from_name]` — `HashMap<&str, Uuid>`; indeks dengan `&str` butuh `*from_name`. Gunakan `state_ids[*from_name]` / `state_ids[*to_name]`. Pastikan `slugify` sudah diimpor di `seed.rs` (sudah dipakai `insert_states`).

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workspace_seed_test`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/seed.rs apps/api-rs/crates/api/tests/workspace_seed_test.rs
git commit -m "feat(api-rs): seed default workflows for new workspaces"
```

---

### Task B9: Expose `type_id` + `workflow_state_id` di API states project

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/state.rs`

- [ ] **Step 1: Ubah query + row + serializer**

Di `state.rs`:

1. `STATE_FULL_SELECT_SQL` (baris ~94) — tambahkan kolom:

```rust
const STATE_FULL_SELECT_SQL: &str = "SELECT s.id, s.project_id, s.workspace_id, s.name, s.color, s.\"group\", s.\"default\" AS is_default, s.description, s.sequence, s.type_id, s.workflow_state_id FROM states s";
```

2. `StateFullRow` (baris ~468-479) — tambahkan field:

```rust
    pub type_id: Option<uuid::Uuid>,
    pub workflow_state_id: Option<uuid::Uuid>,
```

3. `state_serializer_json` (baris ~488-504) — tambahkan key:

```rust
        "type_id": row.type_id,
        "workflow_state_id": row.workflow_state_id,
```

- [ ] **Step 2: Jalankan test state**

Run: `cargo test -p api --lib state`

Expected: PASS. Jika ada test in-file yang membandingkan exact JSON, perbarui ekspektasinya dengan dua key baru.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/state.rs
git commit -m "feat(api-rs): expose state type mapping"
```

---

## Milestone C — Enforcement transisi

### Task C1: Konteks + evaluasi transisi

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs`
- Create: `apps/api-rs/crates/api/tests/workflow_transition_test.rs`

- [ ] **Step 1: Tulis test murni yang gagal**

```rust
// apps/api-rs/crates/api/tests/workflow_transition_test.rs
use api::routes::workflow::{evaluate_transition, TransitionContext};
use uuid::Uuid;

#[test]
fn same_state_is_noop() {
    let ctx = TransitionContext {
        pairs: vec![],
        transitions: vec![],
        default_state_id: None,
    };
    let state = Uuid::new_v4();
    assert!(evaluate_transition(state, state, &ctx).is_ok());
}

#[test]
fn transition_must_exist() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let mirror_closed = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_closed = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![
            (mirror_new, wf_new),
            (mirror_progress, wf_progress),
            (mirror_closed, wf_closed),
        ],
        transitions: vec![(wf_new, wf_progress), (wf_progress, wf_closed)],
        default_state_id: Some(mirror_new),
    };
    assert!(evaluate_transition(mirror_new, mirror_progress, &ctx).is_ok());
    assert!(evaluate_transition(mirror_progress, mirror_closed, &ctx).is_ok());
    assert_eq!(
        evaluate_transition(mirror_new, mirror_closed, &ctx),
        Err(vec![mirror_progress])
    );
    assert_eq!(evaluate_transition(mirror_closed, mirror_new, &ctx), Err(vec![]));
    // Target di luar workflow type → tidak ada yang diizinkan.
    assert_eq!(
        evaluate_transition(mirror_new, Uuid::new_v4(), &ctx),
        Err(vec![])
    );
}

#[test]
fn legacy_current_state_only_moves_to_default() {
    let mirror_new = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, wf_new)],
        transitions: vec![],
        default_state_id: Some(mirror_new),
    };
    assert!(evaluate_transition(legacy_state, mirror_new, &ctx).is_ok());
    assert_eq!(
        evaluate_transition(legacy_state, Uuid::new_v4(), &ctx),
        Err(vec![mirror_new])
    );
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `cargo test -p api --test workflow_transition_test`

Expected: FAIL — `evaluate_transition`/`TransitionContext` belum ada.

- [ ] **Step 3: Implementasi**

Tambahkan ke `workflow.rs`:

```rust
/// Konteks transisi satu type di satu project.
#[derive(Debug, Clone, Default)]
pub struct TransitionContext {
    /// (state_id, workflow_state_id) — mirror typed state di project.
    pub pairs: Vec<(Uuid, Uuid)>,
    /// (from_workflow_state_id, to_workflow_state_id).
    pub transitions: Vec<(Uuid, Uuid)>,
    pub default_state_id: Option<Uuid>,
}

/// Pure: `Ok(())` bila transisi boleh; `Err(allowed_state_ids)` bila ditolak.
pub fn evaluate_transition(
    current_state: Uuid,
    target_state: Uuid,
    ctx: &TransitionContext,
) -> Result<(), Vec<Uuid>> {
    if current_state == target_state {
        return Ok(());
    }
    if !ctx.pairs.iter().any(|(state_id, _)| *state_id == target_state) {
        return Err(Vec::new());
    }
    let current_workflow_state = ctx
        .pairs
        .iter()
        .find(|(state_id, _)| *state_id == current_state)
        .map(|(_, workflow_state_id)| *workflow_state_id);
    let Some(current_workflow_state) = current_workflow_state else {
        return match ctx.default_state_id {
            Some(default_state_id) if default_state_id == target_state => Ok(()),
            Some(default_state_id) => Err(vec![default_state_id]),
            None => Err(Vec::new()),
        };
    };
    let allowed = allowed_target_state_ids(current_state, &ctx.pairs, &ctx.transitions);
    if allowed.contains(&target_state) {
        Ok(())
    } else {
        Err(allowed)
    }
}

/// DB: konteks untuk (project, type). `None` = legacy (type tanpa workflow /
/// epic / tidak aktif di project).
pub(crate) async fn fetch_transition_context(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<Option<TransitionContext>, sqlx::Error> {
    let workflow_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT t.workflow_id FROM issue_types t \
         JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.project_id = $2 \
           AND pit.deleted_at IS NULL \
         WHERE t.id = $1 AND t.deleted_at IS NULL AND t.is_epic = false",
    )
    .bind(type_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let Some(workflow_id) = workflow_id else {
        return Ok(None);
    };
    let pairs: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, workflow_state_id FROM states \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL \
           AND workflow_state_id IS NOT NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_all(pool)
    .await?;
    let transitions: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT from_state_id, to_state_id FROM workflow_transitions \
         WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_all(pool)
    .await?;
    let default_state_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND type_id = $2 AND \"default\" = true \
         AND deleted_at IS NULL ORDER BY sequence LIMIT 1",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_optional(pool)
    .await?;
    Ok(Some(TransitionContext {
        pairs,
        transitions,
        default_state_id,
    }))
}

/// Validasi transisi untuk satu issue. `Ok(Ok(()))` lolos/legacy,
/// `Ok(Err(allowed))` ditolak.
pub(crate) async fn validate_state_transition(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Option<Uuid>,
    current_state: Uuid,
    target_state: Uuid,
) -> Result<Result<(), Vec<Uuid>>, sqlx::Error> {
    let Some(type_id) = type_id else {
        return Ok(Ok(()));
    };
    let Some(ctx) = fetch_transition_context(pool, project_id, type_id).await? else {
        return Ok(Ok(()));
    };
    Ok(evaluate_transition(current_state, target_state, &ctx))
}

/// 400 untuk transisi ditolak (dipakai semua jalur ubah state).
pub(crate) fn transition_denied(allowed: Vec<Uuid>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "Invalid state transition", "allowed_state_ids": allowed })),
    )
}
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `cargo test -p api --test workflow_transition_test`

Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_transition_test.rs
git commit -m "feat(api-rs): transition evaluation helpers"
```

---

### Task C2: Default state type-aware di resolver bersama

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_common.rs`
- Modify: `apps/api-rs/crates/api/src/routes/issue_write.rs`
- Modify: `apps/api-rs/crates/api/src/routes/draft.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_transition_test.rs`

- [ ] **Step 1: Cari semua pemanggil**

Run: `rg -n "resolve_issue_state|resolve_default_state" apps/api-rs/crates/api/src`

Expected: pemanggil di `issue_write.rs` (create) dan `draft.rs` (`resolve_default_state` lokal). Catat semua lokasi.

- [ ] **Step 2: Tulis test DB yang gagal**

```rust
use api::routes::issue_common::resolve_issue_state;

#[tokio::test]
async fn typed_default_beats_legacy_default() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfdefault").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    let (typed_state,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'New', '', '#60646C', 'new', 15000, 'backlog', false, true, \
         $1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .bind(ws_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("typed state");

    let resolved = resolve_issue_state(&st.pool, project_id, Some(type_id), None)
        .await
        .expect("resolve");
    assert_eq!(resolved, Some(typed_state));

    let _ = owner;
    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 3: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test typed_default_beats_legacy_default`

Expected: FAIL — signature lama hanya 3 argumen.

- [ ] **Step 4: Implementasi**

Ubah `resolve_issue_state` di `issue_common.rs`:

```rust
pub(crate) async fn resolve_issue_state(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Option<Uuid>,
    explicit: Option<Uuid>,
) -> Result<Option<Uuid>, sqlx::Error> {
    if explicit.is_some() {
        return Ok(explicit);
    }
    if let Some(type_id) = type_id {
        let typed: Option<Uuid> = sqlx::query_scalar(
            "SELECT s.id FROM states s JOIN issue_types t ON t.id = s.type_id \
             WHERE s.project_id = $1 AND s.type_id = $2 AND s.deleted_at IS NULL \
               AND t.deleted_at IS NULL AND t.is_epic = false \
             ORDER BY s.\"default\" DESC, s.sequence ASC, s.created_at ASC LIMIT 1",
        )
        .bind(project_id)
        .bind(type_id)
        .fetch_optional(pool)
        .await?;
        if typed.is_some() {
            return Ok(typed);
        }
    }
    let default_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    if default_id.is_some() {
        return Ok(default_id);
    }
    let first_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(resolve_effective_state(explicit, default_id, first_id))
}
```

Perbarui pemanggil:

- `issue_write.rs`: `resolve_issue_state(&st.pool, project_id, body.type_id, body.state_id)`.
- `draft.rs::resolve_default_state` (lokal, baris ~475-505): tambahkan parameter `type_id: Option<Uuid>` dan query typed yang sama seperti di atas sebelum fallback legacy; perbarui pemanggilnya di `create_draft_to_issue` (baris ~1259) dengan `d.type_id`.
- Jalankan `rg -n "resolve_issue_state|resolve_default_state" apps/api-rs/crates/api/src` sekali lagi dan pastikan semua pemanggil sudah memakai signature baru.

- [ ] **Step 5: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test`

Expected: PASS (4 passed).

Run: `cargo test -p api --test issue_create_test`

Expected: PASS (regresi create tetap hijau).

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_common.rs apps/api-rs/crates/api/src/routes/issue_write.rs apps/api-rs/crates/api/src/routes/draft.rs apps/api-rs/crates/api/tests/workflow_transition_test.rs
git commit -m "feat(api-rs): type-aware default state resolution"
```

---

### Task C3: Enforcement di PATCH issue

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_update.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_transition_test.rs`

- [ ] **Step 1: Tulis test DB yang gagal**

Tambahkan test yang membuat workspace + type/workflow/state/transisi, lalu panggil handler `patch_issue` dengan `state_id` yang tidak diizinkan dan assert 400 + `allowed_state_ids`.

Gunakan pola `make_workspace` + insert rows seperti Task C2, lalu:

```rust
use api::routes::issue_update::{patch_issue, PatchIssue};

#[tokio::test]
async fn patch_rejects_disallowed_transition() {
    // ... setup workspace/type/workflow/states/transitions + 1 issue dengan state New
    // transitions: New → In Progress (bukan New → Closed)
    let (status, body) = patch_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, issue_id)),
        Json(PatchIssue {
            state_id: Some(Some(closed_state_id)),
            ..Default::default()
        }),
    )
    .await
    .expect("patch");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(
        body["allowed_state_ids"].as_array().unwrap(),
        &vec![json!(progress_state_id.to_string())]
    );
}
```

Catatan: `PatchIssue` harus punya `Default` (semua field `Option`) — jika belum, tambahkan `#[derive(Default)]` pada struct. Setup issue lewat `INSERT INTO issues (...)` langsung (kolom minimal: `id, name, description_html, description_json, priority, is_draft, sort_order, sequence_id, state_id, project_id, workspace_id, created_at, updated_at`).

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test patch_rejects_disallowed_transition`

Expected: FAIL — patch saat ini mengizinkan pindah state.

- [ ] **Step 3: Implementasi**

Di `issue_update.rs`, tepat setelah blok yang menghitung `new_state_id` dan `state_changed` (sekitar baris 748-771), sisipkan:

```rust
    let type_changed = match body.type_id {
        Some(Some(new_type)) => Some(new_type) != current.type_id,
        Some(None) => current.type_id.is_some(),
        None => false,
    };
    let effective_type_id = match body.type_id {
        Some(Some(new_type)) => Some(new_type),
        Some(None) => None,
        None => current.type_id,
    };
    let new_state_id = if type_changed {
        // Ganti type: state wajib milik type baru; tanpa cek transisi.
        match body.state_id {
            Some(Some(explicit)) => {
                let (ok,): (bool,) = sqlx::query_as(
                    "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
                     AND deleted_at IS NULL AND type_id IS NOT DISTINCT FROM $3)",
                )
                .bind(explicit)
                .bind(project_id)
                .bind(effective_type_id)
                .fetch_one(&st.pool)
                .await?;
                if !ok {
                    return Ok(bad("State is not valid for this work item type"));
                }
                Some(explicit)
            }
            _ => {
                resolve_issue_state(&st.pool, project_id, effective_type_id, None).await?
            }
        }
    } else {
        new_state_id
    };
    let state_changed = new_state_id != current.state_id;
    if state_changed && !type_changed {
        if let (Some(current_state), Some(target_state)) = (current.state_id, new_state_id) {
            match validate_state_transition(
                &st.pool,
                project_id,
                effective_type_id,
                current_state,
                target_state,
            )
            .await?
            {
                Ok(()) => {}
                Err(allowed) => return Ok(transition_denied(allowed)),
            }
        }
    }
```

Perhatikan: `new_state_id` yang lama dideklarasikan `let new_state_id = ...`; ganti deklarasi itu (jangan mendeklarasikan dua kali) — jadikan blok di atas sebagai pengganti, dan pastikan `new_state_group` dihitung setelah `state_changed` final.

Verifikasi `CurrentIssue` (struct snapshot di file yang sama) sudah memuat `state_id` dan `type_id`; jika belum, tambahkan field `type_id: Option<Uuid>` dan sertakan di query snapshot (SELECT ... `type_id`).

Tambahkan import:

```rust
use super::workflow::{transition_denied, validate_state_transition};
```

dan pastikan `resolve_issue_state`/`bad` sudah diimpor di file (keduanya dipakai patch saat ini).

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test`

Expected: PASS (5 passed).

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test issue_patch_test`

Expected: PASS (regresi patch hijau; jika ada test yang memindah state pada issue ber-type workflow, sesuaikan setup test dengan menambah transisi).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_update.rs apps/api-rs/crates/api/tests/workflow_transition_test.rs
git commit -m "feat(api-rs): enforce state transitions on issue patch"
```

---

### Task C4: Enforcement + default type di create issue

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/issue_write.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_transition_test.rs`

- [ ] **Step 1: Tulis test DB yang gagal**

```rust
use api::routes::issue_write::{create, CreateIssue};

#[tokio::test]
async fn create_rejects_state_from_other_type() {
    // setup workspace + type A (state New) + type B (state TriageB) + import keduanya
    let (status, body) = create(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(CreateIssue {
            name: "Server down".into(),
            state_id: Some(state_of_type_b),
            type_id: Some(type_a),
            ..Default::default()
        }),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");
}
```

Catatan: jika `CreateIssue` belum `Default`, tambahkan `#[derive(Default)]` (semua field Option kecuali `name` — buat `name` di-set eksplisit di test; derive Default tetap bisa selama `String` punya Default).

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test create_rejects_state_from_other_type`

Expected: FAIL.

- [ ] **Step 3: Implementasi**

Di `issue_write.rs::validate_create_refs`, ganti pengecekan state (baris ~103-114) menjadi type-aware:

```rust
    if let Some(state_id) = body.state_id {
        let (ok,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM states WHERE id = $1 AND project_id = $2 \
             AND deleted_at IS NULL AND \"group\" != 'triage' \
             AND type_id IS NOT DISTINCT FROM $3)",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(body.type_id)
        .fetch_one(pool)
        .await?;
        if !ok {
            return Err(bad("State is not valid for this work item type"));
        }
    }
```

Jika signature `validate_create_refs` belum menerima `pool`, sesuaikan pemanggil (fungsi sudah melakukan query refs, jadi pool tersedia).

Di handler `create` (baris ~276), ubah pemanggilan menjadi:

```rust
    let state_id = resolve_issue_state(&st.pool, project_id, body.type_id, body.state_id).await?;
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test`

Expected: PASS (6 passed).

Run: `cargo test -p api --test issue_create_test`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/issue_write.rs apps/api-rs/crates/api/tests/workflow_transition_test.rs
git commit -m "feat(api-rs): validate typed state on issue create"
```

---

### Task C5: Enforcement di v1 write

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item.rs`
- Modify: `apps/api-rs/crates/api/tests/workflow_transition_test.rs`

- [ ] **Step 1: Cari jalur tulis v1**

Run: `rg -n "pub async fn (create|update)|resolve_effective_state|state" apps/api-rs/crates/api/src/routes/v1/work_item.rs | head -40`

Expected: handler `create` (sekitar baris 747) dan handler update; catat keduanya.

- [ ] **Step 2: Tulis test DB yang gagal**

Tambahkan test yang memanggil handler update v1 (nama sesuai hasil Step 1) untuk memindah state ke state yang tidak diizinkan, assert 400 `{"error": "Invalid state transition", ...}`. Ikuti pola setup Task C3.

- [ ] **Step 3: Implementasi**

Di handler `create` v1:

- Validasi state vs type: tambahkan `AND type_id IS NOT DISTINCT FROM $3` pada query state di `validate_write` (baris ~721-727), bind `body.type_id`.
- Default: sebelum `resolve_effective_state(body.state, default_state, first_state)`, jika `body.type_id` ada, ambil typed default dulu:

```rust
    let typed_state: Option<uuid::Uuid> = match body.type_id {
        Some(type_id) => {
            sqlx::query_scalar(
                "SELECT s.id FROM states s JOIN issue_types t ON t.id = s.type_id \
                 WHERE s.project_id = $1 AND s.type_id = $2 AND s.deleted_at IS NULL \
                   AND t.deleted_at IS NULL AND t.is_epic = false \
                 ORDER BY s.\"default\" DESC, s.sequence ASC LIMIT 1",
            )
            .bind(project_id)
            .bind(type_id)
            .fetch_optional(&st.pool)
            .await?
        }
        None => None,
    };
    let state = resolve_effective_state(body.state, typed_state.or(default_state), first_state);
```

Di handler update v1: jika `state` berubah, panggil `validate_state_transition(&st.pool, project_id, current.type_id, current.state_id, target_state)`; tolak dengan `transition_denied(allowed)`. Gunakan `type_id` efektif setelah patch (jika body mengganti type, perlakukan seperti Task C3: tanpa cek transisi, state wajib milik type baru).

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test`

Expected: PASS (7 passed).

Run: `cargo test -p api --test v1_work_item_test`

Expected: PASS (atau test lain yang menyentuh v1 write; cek daftar test dengan `ls apps/api-rs/crates/api/tests | rg v1`).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item.rs apps/api-rs/crates/api/tests/workflow_transition_test.rs
git commit -m "feat(api-rs): enforce transitions on v1 work item writes"
```

---

### Task C6: Accept intake memindahkan state ke default type

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/intake.rs`
- Modify: `apps/api-rs/crates/api/tests/intake_test.rs` (atau test baru `workflow_transition_test.rs`)

- [ ] **Step 1: Temukan handler accept**

Run: `rg -n "status" apps/api-rs/crates/api/src/routes/intake.rs | rg -n "UPDATE intake_issues|accepted|1" | head -20`

Expected: handler `patch_issue` (~baris 1362) menulis `UPDATE intake_issues SET status = ...`. Catat blok tepatnya.

- [ ] **Step 2: Tulis test DB yang gagal**

Buat workspace + project + type/workflow (state default `New`) + intake issue di state Triage; panggil handler `patch_issue` dengan `status: Some(1)`; assert `issues.state_id` sekarang = mirror default type, bukan triage.

- [ ] **Step 3: Implementasi**

Setelah blok `UPDATE intake_issues ... status ...` dan di dalam transaksi yang sama, tambahkan:

```rust
    if body.status == Some(1) {
        sqlx::query(
            "UPDATE issues i SET state_id = COALESCE( \
               (SELECT s.id FROM states s WHERE s.project_id = i.project_id AND s.type_id = i.type_id \
                  AND s.deleted_at IS NULL AND s.\"default\" = true ORDER BY s.sequence LIMIT 1), \
               (SELECT s.id FROM states s WHERE s.project_id = i.project_id AND s.type_id IS NULL \
                  AND s.deleted_at IS NULL AND s.\"default\" = true ORDER BY s.sequence LIMIT 1) \
             ), completed_at = NULL, updated_at = now() \
             WHERE i.id = $1 AND i.state_id IN \
               (SELECT id FROM states WHERE project_id = i.project_id AND \"group\" = 'triage' AND deleted_at IS NULL)",
        )
        .bind(issue_id)
        .execute(&mut *tx)
        .await?;
    }
```

Sesuaikan nama variabel (`body.status`, `issue_id`, `tx`) dengan kode aktual; jika handler tidak memakai transaksi, bungkus kedua statement dalam satu `st.pool.begin()`.

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test intake_test`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/intake.rs apps/api-rs/crates/api/tests/intake_test.rs
git commit -m "feat(api-rs): move accepted intake items to typed default state"
```

---

## Milestone D — Verifikasi end-to-end

### Task D1: Smoke E2E + rebuild

**Files:** tidak ada perubahan kode.

- [ ] **Step 1: Jalankan seluruh test backend**

Run:

```bash
cargo test -p api --lib
cargo test -p api --test workflow_test --test workflow_transition_test --test route_inventory_test --test v1_work_item_type_test
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test --test workflow_transition_test --test workspace_seed_test --test issue_create_test --test issue_patch_test --test intake_test
```

Expected: semua PASS. Test DB tanpa `DATABASE_URL` boleh di-skip dengan catatan; test murni wajib hijau.

- [ ] **Step 2: Rebuild api-rs (detached + log, sesuai AGENTS.md)**

Run:

```bash
docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build.log 2>&1 &
```

Poll sampai selesai (build Rust LTO bisa 10+ menit tanpa output — jangan abort):

```bash
tail -5 /tmp/plane-api-build.log
```

Expected: container `api`/`worker`/`beat-worker` started; tidak ada error build.

- [ ] **Step 3: Verifikasi health + live**

Run:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8000/health
systemctl --user restart plane-live.service
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `200` dan `200`.

- [ ] **Step 4: Smoke API (butuh kredensial workspace)**

```bash
export PLANE_EMAIL="<email-admin>"
export PLANE_PASSWORD="<password>"
COOKIE=/tmp/plane-cookies.txt
BASE=http://localhost:8000
SLUG=<workspace-slug>
PROJECT=<project-id>

curl -s -c $COOKIE -X POST $BASE/api/auth/sign-in/ -H 'Content-Type: application/json' \
  -d "{\"email\":\"$PLANE_EMAIL\",\"password\":\"$PLANE_PASSWORD\"}" -o /dev/null

# 1. Workflow default hasil seed harus ada.
curl -s -b $COOKIE $BASE/api/workspaces/$SLUG/workflows/ | jq 'length'   # expect 4

# 2. Buat workflow smoke + 3 state + transisi New → In Progress.
WF=$(curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/workflows/ \
  -H 'Content-Type: application/json' -d '{"name":"Smoke Workflow"}' | jq -r .id)
S1=$(curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/workflows/$WF/states/ \
  -H 'Content-Type: application/json' -d '{"name":"New","group":"backlog"}' | jq -r .id)
S2=$(curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/workflows/$WF/states/ \
  -H 'Content-Type: application/json' -d '{"name":"In Progress","group":"started"}' | jq -r .id)
S3=$(curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/workflows/$WF/states/ \
  -H 'Content-Type: application/json' -d '{"name":"Closed","group":"completed"}' | jq -r .id)
curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/workflows/$WF/transitions/ \
  -H 'Content-Type: application/json' -d "{\"from_state_id\":\"$S1\",\"to_state_id\":\"$S2\"}" | jq .id

# 3. Buat type dengan workflow lalu import ke project.
TYPE=$(curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/work-item-types/ \
  -H 'Content-Type: application/json' \
  -d "{\"name\":\"Smoke Incident\",\"workflow\":\"$WF\",\"project_ids\":[\"$PROJECT\"]}" | jq -r .id)
curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/projects/$PROJECT/import-work-item-types/ \
  -H 'Content-Type: application/json' -d "{\"work_item_types\":[\"$TYPE\"]}" -o /dev/null

# 4. workflow-map: default state + transisi (id mirror, bukan workflow_state id).
MAP=$(curl -s -b $COOKIE $BASE/api/workspaces/$SLUG/projects/$PROJECT/workflow-map/)
echo "$MAP" | jq --arg t "$TYPE" '.types[] | select(.type_id==$t) | {default_state_id, states: (.states|length), transitions: (.transitions|length)}'
# expect default_state_id = mirror "New", states 3, transitions 1
MIRROR_NEW=$(echo "$MAP" | jq -r --arg t "$TYPE" '.types[] | select(.type_id==$t) | .default_state_id')
MIRROR_PROGRESS=$(echo "$MAP" | jq -r --arg t "$TYPE" '.types[] | select(.type_id==$t) | .states[] | select(.name=="In Progress") | .id')
MIRROR_CLOSED=$(echo "$MAP" | jq -r --arg t "$TYPE" '.types[] | select(.type_id==$t) | .states[] | select(.name=="Closed") | .id')

# 5. Buat issue bertipe Smoke Incident (state default terisi otomatis).
ISSUE=$(curl -s -b $COOKIE -X POST $BASE/api/workspaces/$SLUG/projects/$PROJECT/issues/ \
  -H 'Content-Type: application/json' \
  -d "{\"name\":\"Server down\",\"type_id\":\"$TYPE\"}" | jq -r .id)
curl -s -b $COOKIE $BASE/api/workspaces/$SLUG/projects/$PROJECT/issues/$ISSUE/ | jq -r .state_id
# expect = $MIRROR_NEW

# 6. Transisi terlarang New → Closed harus 400 + allowed_state_ids.
curl -s -b $COOKIE -X PATCH $BASE/api/workspaces/$SLUG/projects/$PROJECT/issues/$ISSUE/ \
  -H 'Content-Type: application/json' -d "{\"state_id\":\"$MIRROR_CLOSED\"}" \
  | jq '{error, allowed_state_ids}'
# expect error="Invalid state transition", allowed_state_ids=[$MIRROR_PROGRESS]

# 7. Transisi valid New → In Progress harus sukses.
curl -s -o /dev/null -w "%{http_code}\n" -b $COOKIE -X PATCH \
  $BASE/api/workspaces/$SLUG/projects/$PROJECT/issues/$ISSUE/ \
  -H 'Content-Type: application/json' -d "{\"state_id\":\"$MIRROR_PROGRESS\"}"
# expect 204
```

Expected: langkah 1 `4`; langkah 4 `states 3, transitions 1`; langkah 5 state = mirror New; langkah 6 400 dengan `allowed_state_ids` berisi mirror In Progress; langkah 7 `204`.

- [ ] **Step 5: Update AGENTS.md bila perlu**

Jika ada command/verifikasi baru yang perlu diingat (mis. `cargo test -p api --test workflow_test`), tambahkan satu baris ke `AGENTS.md` bagian backend. Jika tidak, lewati.

- [ ] **Step 6: Commit sisa perubahan**

```bash
git status --short
git add <file-yang-tersisa>
git commit -m "chore: backend workflow verification"
```

---

## Catatan self-review

- **Deviasi dari spec:** helper type CRUD tidak dipindah ke modul baru; route internal memakai ulang handler `v1::work_item_type` (yang sudah `pub`) + `workflow.rs` untuk guard/materialization. Ini memenuhi "tidak duplikasi" tanpa refactor besar.
- **Bulk operation:** endpoint `bulk-operation-issues` belum ada di api-rs (web mengirim ke endpoint yang tidak dilayani). Enforcement bulk ditunda sampai endpoint itu diport; `validate_state_transition` sudah siap dipakai.
- **Web admin UI + board hybrid** ada di plan terpisah: `docs/superpowers/plans/2026-09-25-work-item-types-workflows-web.md`.
- **`work_item_type_shared.rs` tidak dibuat** — abaikan referensi lama di File Structure/B1 bila masih ada; gunakan `workflow.rs` + reuse `v1::work_item_type`.
