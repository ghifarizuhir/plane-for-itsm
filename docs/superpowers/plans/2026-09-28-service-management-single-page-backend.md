# Service Management Satu Halaman — Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Setiap work item type non-epic memiliki tepat satu workflow — dibuat otomatis saat type dibuat, ikut rename saat type di-rename, ikut aktif/nonaktif, dan terhapus bersama type; data existing dinormalisasi; endpoint workflow standalone ditutup.

**Architecture:** Django data migration `0125_normalize_type_workflows` menormalkan data lama (adopsi orphan, clone workflow yang dipakai bersama, healing default state, soft-delete sisa orphan). api-rs memiliki helper baru di `routes/workflow.rs` (`ensure_workflow_for_type`, `sync_type_workflow`, `soft_delete_workflow_cascade`, dll) yang dipanggil dari create/update/delete type di `routes/v1/work_item_type.rs`; route `POST/PATCH/DELETE /workflows/...` dilepas dari router (handler tetap ada sebagai fixture test).

**Tech Stack:** Rust (axum, sqlx, tokio) di `apps/api-rs`; Django migrations + pytest di `apps/api`; Postgres.

**Spec:** `docs/superpowers/specs/2026-09-28-service-management-single-page-design.md`

**Execution notes (baca sebelum mulai):**

- Perintah Rust dijalankan dari `apps/api-rs`. Test DB-backed butuh `DATABASE_URL` (default `postgres://plane:plane@localhost:5432/plane`).
- Suite scratch-workspace (`workflow_test`, `workflow_transition_test`, dll) WAJIB serial: tambahkan `-- --test-threads=1` (helper `purge` menghapus by slug prefix).
- Working tree punya banyak file unrelated yang termodifikasi — **jangan** `git add -A`; commit hanya file yang disebut task.
- Jangan mengubah `seed.rs` dan migrasi `0124`: keduanya sudah 1:1.
- Handler `create_workflow`/`patch_workflow`/`delete_workflow` TETAP ada (dipakai test sebagai fixture) — hanya route-nya yang dilepas.
- Django test jalan di Docker: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/migrations/test_normalize_type_workflows.py -vv` (working dir container `/code`).

---

### Task 1: Django data migration `0125_normalize_type_workflows`

**Files:**

- Create: `apps/api/plane/db/migrations/0125_normalize_type_workflows.py`
- Create: `apps/api/plane/tests/unit/migrations/__init__.py` (file kosong)
- Create: `apps/api/plane/tests/unit/migrations/test_normalize_type_workflows.py`

- [ ] **Step 1: Tulis test yang gagal**

Buat `apps/api/plane/tests/unit/migrations/__init__.py` kosong, lalu tulis `apps/api/plane/tests/unit/migrations/test_normalize_type_workflows.py`:

```python
# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

import importlib

from datetime import timedelta

import pytest
from django.apps import apps as django_apps
from django.utils import timezone

from plane.db.models import IssueType, State, Workflow, WorkflowState, WorkflowTransition
from plane.tests.factories import ProjectFactory, WorkspaceFactory

normalize = importlib.import_module("plane.db.migrations.0125_normalize_type_workflows")


@pytest.mark.unit
class TestNormalizeTypeWorkflows:
    @pytest.mark.django_db
    def test_adopts_orphan_and_heals_default_state(self):
        workspace = WorkspaceFactory()
        orphan = Workflow.objects.create(workspace=workspace, name="Request Workflow", is_active=True)
        issue_type = IssueType.objects.create(workspace=workspace, name="Request", is_active=False)

        normalize.normalize_type_workflows(django_apps, None)

        issue_type.refresh_from_db()
        orphan.refresh_from_db()
        assert issue_type.workflow_id == orphan.id
        assert orphan.is_active is False
        states = WorkflowState.objects.filter(workflow=orphan, deleted_at__isnull=True)
        assert states.count() == 1
        assert states.get().name == "New"
        assert states.get().is_default is True

    @pytest.mark.django_db
    def test_creates_derived_workflow_with_default_state(self):
        workspace = WorkspaceFactory()
        issue_type = IssueType.objects.create(workspace=workspace, name="Incident")

        normalize.normalize_type_workflows(django_apps, None)

        issue_type.refresh_from_db()
        workflow = Workflow.objects.get(id=issue_type.workflow_id, deleted_at__isnull=True)
        assert workflow.name == "Incident Workflow"
        assert (
            WorkflowState.objects.filter(workflow=workflow, deleted_at__isnull=True, is_default=True).count() == 1
        )

    @pytest.mark.django_db
    def test_clones_shared_workflow_and_repoints_mirrors(self):
        workspace = WorkspaceFactory()
        project = ProjectFactory(workspace=workspace)
        shared = Workflow.objects.create(workspace=workspace, name="Shared Workflow")
        wf_new = WorkflowState.objects.create(
            workflow=shared, name="New", color="#60646C", group="backlog", is_default=True
        )
        wf_done = WorkflowState.objects.create(workflow=shared, name="Done", color="#46A758", group="completed")
        WorkflowTransition.objects.create(workflow=shared, from_state=wf_new, to_state=wf_done)
        type_t = IssueType.objects.create(workspace=workspace, name="Type T", workflow=shared)
        type_u = IssueType.objects.create(workspace=workspace, name="Type U", workflow=shared)
        # Deterministik: `Type T` harus jadi pemilik workflow (type tertua).
        IssueType.objects.filter(id=type_t.id).update(created_at=timezone.now() - timedelta(days=1))
        mirror_u = State.objects.create(
            project=project,
            name="New",
            color="#60646C",
            group="backlog",
            type=type_u,
            workflow_state=wf_new,
            default=True,
        )

        normalize.normalize_type_workflows(django_apps, None)

        type_t.refresh_from_db()
        type_u.refresh_from_db()
        assert type_t.workflow_id == shared.id
        clone = Workflow.objects.get(id=type_u.workflow_id, deleted_at__isnull=True)
        assert clone.name == "Type U Workflow"
        clone_states = {
            state.name: state
            for state in WorkflowState.objects.filter(workflow=clone, deleted_at__isnull=True)
        }
        assert set(clone_states) == {"New", "Done"}
        assert WorkflowTransition.objects.filter(workflow=clone, deleted_at__isnull=True).count() == 1
        mirror_u.refresh_from_db()
        assert mirror_u.workflow_state_id == clone_states["New"].id

    @pytest.mark.django_db
    def test_soft_deletes_leftover_orphans(self):
        workspace = WorkspaceFactory()
        orphan = Workflow.objects.create(workspace=workspace, name="Old Workflow")
        orphan_state = WorkflowState.objects.create(
            workflow=orphan, name="New", color="#60646C", group="backlog", is_default=True
        )

        normalize.normalize_type_workflows(django_apps, None)

        orphan.refresh_from_db()
        orphan_state.refresh_from_db()
        assert orphan.deleted_at is not None
        assert orphan_state.deleted_at is not None

    @pytest.mark.django_db
    def test_seed_rows_keep_identity(self):
        workspace = WorkspaceFactory()
        workflow = Workflow.objects.create(
            workspace=workspace,
            name="Incident Workflow",
            external_source="plane-default-itsm",
            external_id="workflow:incident",
        )
        WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True
        )
        issue_type = IssueType.objects.create(
            workspace=workspace,
            name="Incident",
            workflow=workflow,
            external_source="plane-default-itsm",
            external_id="issue-type:incident",
        )

        normalize.normalize_type_workflows(django_apps, None)

        issue_type.refresh_from_db()
        workflow.refresh_from_db()
        assert issue_type.workflow_id == workflow.id
        assert workflow.name == "Incident Workflow"
        assert Workflow.objects.filter(workspace=workspace, deleted_at__isnull=True).count() == 1
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/migrations/test_normalize_type_workflows.py -vv`

Expected: FAIL/ERROR — `ModuleNotFoundError: No module named 'plane.db.migrations.0125_normalize_type_workflows'`.

- [ ] **Step 3: Tulis migrasi**

Buat `apps/api/plane/db/migrations/0125_normalize_type_workflows.py`:

```python
# Generated by Django 5.2.15 on 2026-09-28

from django.db import migrations
from django.db.models import Count, Q
from django.utils import timezone
from django.utils.text import slugify


WORKFLOW_SUFFIX = " Workflow"
DEFAULT_STATE_NAME = "New"
DEFAULT_STATE_GROUP = "backlog"
DEFAULT_STATE_COLOR = "#60646C"


def derived_workflow_name(type_name):
    return f"{type_name}{WORKFLOW_SUFFIX}"


def live_workflows(Workflow, workspace_id):
    return Workflow.objects.filter(workspace_id=workspace_id, deleted_at__isnull=True)


def name_is_free(Workflow, workspace_id, name, exclude_id=None):
    query = live_workflows(Workflow, workspace_id).filter(name=name)
    if exclude_id is not None:
        query = query.exclude(id=exclude_id)
    return not query.exists()


def unique_workflow_name(Workflow, workspace_id, base_name):
    if name_is_free(Workflow, workspace_id, base_name):
        return base_name
    index = 2
    while not name_is_free(Workflow, workspace_id, f"{base_name} ({index})"):
        index += 1
    return f"{base_name} ({index})"


def ensure_default_state(WorkflowState, workflow, user_id):
    states = list(
        WorkflowState.objects.filter(workflow_id=workflow.id, deleted_at__isnull=True).order_by("sequence", "id")
    )
    if not states:
        WorkflowState.objects.create(
            workflow_id=workflow.id,
            name=DEFAULT_STATE_NAME,
            group=DEFAULT_STATE_GROUP,
            color=DEFAULT_STATE_COLOR,
            slug=slugify(DEFAULT_STATE_NAME),
            sequence=15000,
            is_default=True,
            created_by_id=user_id,
            updated_by_id=user_id,
        )
        return
    if not any(state.is_default for state in states):
        first = states[0]
        first.is_default = True
        first.save(update_fields=["is_default", "updated_at"])


def adopt_or_create_workflow(Workflow, workspace, issue_type):
    if issue_type.workflow_id:
        workflow = live_workflows(Workflow, workspace.id).filter(id=issue_type.workflow_id).first()
        if workflow is not None:
            return workflow
    target_name = derived_workflow_name(issue_type.name)
    orphan = (
        live_workflows(Workflow, workspace.id)
        .filter(name=target_name)
        .annotate(live_types=Count("issue_types", filter=Q(issue_types__deleted_at__isnull=True)))
        .filter(live_types=0)
        .order_by("created_at", "id")
        .first()
    )
    if orphan is not None:
        return orphan
    return Workflow.objects.create(
        workspace_id=workspace.id,
        name=unique_workflow_name(Workflow, workspace.id, target_name),
        is_active=issue_type.is_active,
        created_by_id=workspace.owner_id,
        updated_by_id=workspace.owner_id,
    )


def clone_workflow(Workflow, WorkflowState, WorkflowTransition, source, workspace, issue_type):
    clone = Workflow.objects.create(
        workspace_id=workspace.id,
        name=unique_workflow_name(Workflow, workspace.id, derived_workflow_name(issue_type.name)),
        description=source.description,
        is_active=issue_type.is_active,
        created_by_id=workspace.owner_id,
        updated_by_id=workspace.owner_id,
    )
    state_map = {}
    for state in WorkflowState.objects.filter(workflow_id=source.id, deleted_at__isnull=True).order_by(
        "sequence", "id"
    ):
        new_state = WorkflowState.objects.create(
            workflow_id=clone.id,
            name=state.name,
            description=state.description,
            color=state.color,
            slug=state.slug,
            sequence=state.sequence,
            group=state.group,
            is_default=state.is_default,
            created_by_id=workspace.owner_id,
            updated_by_id=workspace.owner_id,
        )
        state_map[state.id] = new_state.id
    for transition in WorkflowTransition.objects.filter(workflow_id=source.id, deleted_at__isnull=True):
        if transition.from_state_id in state_map and transition.to_state_id in state_map:
            WorkflowTransition.objects.create(
                workflow_id=clone.id,
                from_state_id=state_map[transition.from_state_id],
                to_state_id=state_map[transition.to_state_id],
                created_by_id=workspace.owner_id,
                updated_by_id=workspace.owner_id,
            )
    return clone, state_map


def repoint_project_mirrors(State, type_id, state_map):
    for old_state_id, new_state_id in state_map.items():
        State.objects.filter(
            type_id=type_id,
            workflow_state_id=old_state_id,
            deleted_at__isnull=True,
        ).update(workflow_state_id=new_state_id, updated_at=timezone.now())


def soft_delete_orphan_workflows(Workflow, WorkflowState, WorkflowTransition, workspace):
    now = timezone.now()
    orphans = (
        live_workflows(Workflow, workspace.id)
        .annotate(live_types=Count("issue_types", filter=Q(issue_types__deleted_at__isnull=True)))
        .filter(live_types=0)
    )
    for workflow in orphans.iterator():
        WorkflowState.objects.filter(workflow_id=workflow.id, deleted_at__isnull=True).update(
            deleted_at=now, updated_at=now
        )
        WorkflowTransition.objects.filter(workflow_id=workflow.id, deleted_at__isnull=True).update(
            deleted_at=now, updated_at=now
        )
        Workflow.objects.filter(id=workflow.id).update(deleted_at=now, updated_at=now)


def normalize_type_workflows(apps, schema_editor):
    Workspace = apps.get_model("db", "Workspace")
    IssueType = apps.get_model("db", "IssueType")
    Workflow = apps.get_model("db", "Workflow")
    WorkflowState = apps.get_model("db", "WorkflowState")
    WorkflowTransition = apps.get_model("db", "WorkflowTransition")
    State = apps.get_model("db", "State")

    for workspace in Workspace.objects.filter(deleted_at__isnull=True).iterator():
        issue_types = list(
            IssueType.objects.filter(
                workspace_id=workspace.id, deleted_at__isnull=True, is_epic=False
            ).order_by("created_at", "id")
        )
        # 1. Tiap type non-epic punya workflow (adopsi/ciptakan) + default state.
        for issue_type in issue_types:
            workflow = adopt_or_create_workflow(Workflow, workspace, issue_type)
            if issue_type.workflow_id != workflow.id:
                issue_type.workflow_id = workflow.id
                issue_type.save(update_fields=["workflow_id", "updated_at"])
            ensure_default_state(WorkflowState, workflow, workspace.owner_id)
        # 2. Workflow yang dipakai beberapa type: sisakan untuk type tertua, clone
        #    untuk sisanya + re-point mirror project milik type itu.
        by_workflow = {}
        for issue_type in issue_types:
            by_workflow.setdefault(issue_type.workflow_id, []).append(issue_type)
        for workflow_id, linked_types in by_workflow.items():
            if len(linked_types) < 2:
                continue
            source = live_workflows(Workflow, workspace.id).filter(id=workflow_id).first()
            if source is None:
                continue
            for extra_type in linked_types[1:]:
                clone, state_map = clone_workflow(
                    Workflow, WorkflowState, WorkflowTransition, source, workspace, extra_type
                )
                extra_type.workflow_id = clone.id
                extra_type.save(update_fields=["workflow_id", "updated_at"])
                repoint_project_mirrors(State, extra_type.id, state_map)
        # 3. Rename ke nama derived (hanya bila bebas) + sync is_active.
        for issue_type in issue_types:
            workflow = live_workflows(Workflow, workspace.id).filter(id=issue_type.workflow_id).first()
            if workflow is None:
                continue
            changed = False
            target_name = derived_workflow_name(issue_type.name)
            if workflow.name != target_name and name_is_free(
                Workflow, workspace.id, target_name, exclude_id=workflow.id
            ):
                workflow.name = target_name
                changed = True
            if workflow.is_active != issue_type.is_active:
                workflow.is_active = issue_type.is_active
                changed = True
            if changed:
                workflow.save(update_fields=["name", "is_active", "updated_at"])
        # 4. Orphan yang tersisa di-soft-delete.
        soft_delete_orphan_workflows(Workflow, WorkflowState, WorkflowTransition, workspace)


class Migration(migrations.Migration):

    dependencies = [
        ("db", "0124_seed_default_workflows"),
    ]

    operations = [
        migrations.RunPython(normalize_type_workflows, reverse_code=migrations.RunPython.noop),
    ]
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `docker compose -f docker-compose-test.yml run --rm api-tests pytest plane/tests/unit/migrations/test_normalize_type_workflows.py -vv`

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add apps/api/plane/db/migrations/0125_normalize_type_workflows.py \
  apps/api/plane/tests/unit/migrations/__init__.py \
  apps/api/plane/tests/unit/migrations/test_normalize_type_workflows.py
git commit -m "feat(api): normalize work item type workflows in migration"
```

### Task 2: Helper workflow 1:1 di `routes/workflow.rs`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/workflow.rs` (tambah helper setelah `validate_state_group`, sekitar baris 46)
- Test: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Di `apps/api-rs/crates/api/tests/workflow_test.rs`, tambahkan `derived_workflow_name`, `ensure_workflow_for_type`, `sync_type_workflow`, `soft_delete_workflow_cascade` ke daftar import dari `api::routes::workflow` (baris 6-12), lalu tambahkan test berikut. Test pure diletakkan setelah `group_error_names_offending_value` (setelah baris 63); test DB di akhir file (sebelum test terakhir yang sudah ada boleh, yang penting sebelum penutup file):

```rust
#[test]
fn derived_workflow_name_appends_suffix() {
    assert_eq!(derived_workflow_name("Incident"), "Incident Workflow");
    assert_eq!(derived_workflow_name("  Incident  "), "Incident Workflow");
}
```

```rust
#[tokio::test]
async fn ensure_workflow_for_type_adopts_and_defaults() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfwf").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let mut conn = st.pool.acquire().await.expect("conn");

    let first = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("ensure workflow");
    let (name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(first)
        .fetch_one(&mut *conn)
        .await
        .expect("workflow row");
    assert_eq!(name, "Incident Workflow");
    let (defaults,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default",
    )
    .bind(first)
    .fetch_one(&mut *conn)
    .await
    .expect("default state");
    assert_eq!(defaults, 1);

    // Idempotent: panggilan kedua mengadopsi workflow yang sama (masih orphan).
    let second = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("ensure again");
    assert_eq!(first, second);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn sync_type_workflow_renames_when_free_and_syncs_active() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfsync").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let mut conn = st.pool.acquire().await.expect("conn");

    let incident_wf = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("incident workflow");
    sync_type_workflow(&mut conn, ws_id, "Major Incident", false, incident_wf)
        .await
        .expect("sync");
    let (name, active): (String, bool) =
        sqlx::query_as("SELECT name, is_active FROM workflows WHERE id = $1")
            .bind(incident_wf)
            .fetch_one(&mut *conn)
            .await
            .expect("workflow row");
    assert_eq!(name, "Major Incident Workflow");
    assert!(!active);

    // Nama derived sudah dipakai workflow lain → nama lama dipertahankan.
    let request_wf = ensure_workflow_for_type(&mut conn, ws_id, "Request", true, owner)
        .await
        .expect("request workflow");
    sync_type_workflow(&mut conn, ws_id, "Major Incident", true, request_wf)
        .await
        .expect("sync collision");
    let (request_name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(request_wf)
        .fetch_one(&mut *conn)
        .await
        .expect("request row");
    assert_eq!(request_name, "Request Workflow");

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn soft_delete_workflow_cascade_clears_states_and_transitions() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfdel").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let mut conn = st.pool.acquire().await.expect("conn");

    let workflow_id = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("workflow");
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
    .expect("state");
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();
    let (_, states) = list_states(State(st.clone()), AuthUser(owner), Path((slug.clone(), workflow_id)))
        .await
        .expect("states");
    let default_id = Uuid::parse_str(states[0]["id"].as_str().unwrap()).unwrap();
    let _ = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: default_id,
            to_state_id: progress_id,
        }),
    )
    .await
    .expect("transition");

    soft_delete_workflow_cascade(&mut conn, workflow_id)
        .await
        .expect("cascade");
    let (workflow_live, states_live, transitions_live): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&mut *conn)
    .await
    .expect("cascade counts");
    assert_eq!(workflow_live, 0);
    assert_eq!(states_live, 0);
    assert_eq!(transitions_live, 0);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 derived_workflow_name ensure_workflow_for_type sync_type_workflow soft_delete_workflow_cascade`

Expected: FAIL compile — fungsi belum ada di `api::routes::workflow`.

- [ ] **Step 3: Implementasi helper**

Di `apps/api-rs/crates/api/src/routes/workflow.rs`, tambahkan setelah `validate_state_group` (baris 46):

```rust
/// Nama workflow derived dari nama type. Kontrak 1:1: workflow tidak punya
/// nama sendiri di UI; nama mengikuti type (spec
/// 2026-09-28-service-management-single-page-design.md).
pub fn derived_workflow_name(type_name: &str) -> String {
    format!("{} Workflow", type_name.trim())
}

/// Nama workflow unik di workspace: `base`, `base (2)`, `base (3)`, ...
pub async fn unique_workflow_name(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    base: &str,
) -> Result<String, sqlx::Error> {
    let mut candidate = base.to_string();
    let mut index = 1;
    loop {
        let (taken,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM workflows WHERE workspace_id = $1 AND name = $2 AND deleted_at IS NULL)",
        )
        .bind(workspace_id)
        .bind(&candidate)
        .fetch_one(&mut *conn)
        .await?;
        if !taken {
            return Ok(candidate);
        }
        index += 1;
        candidate = format!("{base} ({index})");
    }
}

/// Pastikan workflow punya tepat satu default state: tambah `New` (backlog)
/// bila belum ada state, atau promosikan state paling awal bila default hilang.
pub async fn ensure_workflow_default_state(
    conn: &mut sqlx::PgConnection,
    workflow_id: Uuid,
    user: Uuid,
) -> Result<(), sqlx::Error> {
    let (has_default,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default)",
    )
    .bind(workflow_id)
    .fetch_one(&mut *conn)
    .await?;
    if has_default {
        return Ok(());
    }
    let (has_states,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&mut *conn)
    .await?;
    if has_states {
        sqlx::query(
            "UPDATE workflow_states SET is_default = true, updated_at = now() WHERE id = ( \
             SELECT id FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL \
             ORDER BY sequence, id LIMIT 1)",
        )
        .bind(workflow_id)
        .execute(&mut *conn)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO workflow_states (id, workflow_id, name, description, color, slug, sequence, \
             \"group\", is_default, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, 'New', '', '#60646C', 'new', 15000, 'backlog', true, $2, $2, now(), now())",
        )
        .bind(workflow_id)
        .bind(user)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Workflow milik satu type: adopsi orphan `{Type} Workflow` bila ada, jika
/// tidak buat baru. Selalu memastikan default state ada dan `is_active`
/// mengikuti type.
pub async fn ensure_workflow_for_type(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    type_name: &str,
    is_active: bool,
    user: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let base = derived_workflow_name(type_name);
    let orphan: Option<Uuid> = sqlx::query_scalar(
        "SELECT w.id FROM workflows w \
         WHERE w.workspace_id = $1 AND w.name = $2 AND w.deleted_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM issue_types t WHERE t.workflow_id = w.id AND t.deleted_at IS NULL) \
         ORDER BY w.created_at, w.id LIMIT 1",
    )
    .bind(workspace_id)
    .bind(&base)
    .fetch_optional(&mut *conn)
    .await?;
    let workflow_id = match orphan {
        Some(id) => id,
        None => {
            let name = unique_workflow_name(&mut *conn, workspace_id, &base).await?;
            sqlx::query_scalar(
                "INSERT INTO workflows (id, name, description, is_active, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, '', $2, $3, $4, $4, now(), now()) RETURNING id",
            )
            .bind(&name)
            .bind(is_active)
            .bind(workspace_id)
            .bind(user)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    ensure_workflow_default_state(&mut *conn, workflow_id, user).await?;
    sqlx::query("UPDATE workflows SET is_active = $2, updated_at = now() WHERE id = $1 AND is_active <> $2")
        .bind(workflow_id)
        .bind(is_active)
        .execute(&mut *conn)
        .await?;
    Ok(workflow_id)
}

/// Rename workflow ke nama derived type bila nama itu bebas, lalu sync
/// `is_active`. Nama yang sudah dipakai workflow lain dibiarkan.
pub async fn sync_type_workflow(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    type_name: &str,
    is_active: bool,
    workflow_id: Uuid,
) -> Result<(), sqlx::Error> {
    let target = derived_workflow_name(type_name);
    let (taken,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflows WHERE workspace_id = $1 AND name = $2 \
         AND deleted_at IS NULL AND id <> $3)",
    )
    .bind(workspace_id)
    .bind(&target)
    .bind(workflow_id)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE workflows SET name = CASE WHEN $2 AND name <> $3 THEN $3 ELSE name END, \
         is_active = $4, updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .bind(!taken)
    .bind(&target)
    .bind(is_active)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Soft-delete workflow beserta state + transition-nya (dipakai delete type).
pub async fn soft_delete_workflow_cascade(
    conn: &mut sqlx::PgConnection,
    workflow_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE workflow_transitions SET deleted_at = now(), updated_at = now() \
         WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE workflow_states SET deleted_at = now(), updated_at = now() \
         WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE workflows SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 derived_workflow_name ensure_workflow_for_type sync_type_workflow soft_delete_workflow_cascade`

Expected: 4 passed (1 pure + 3 DB).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/workflow.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): add 1:1 type workflow helpers"
```

### Task 3: Create type auto-create workflow

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs:277-354` (`create_type`)
- Test: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan test berikut di `apps/api-rs/crates/api/tests/workflow_test.rs` (setelah test `soft_delete_workflow_cascade_clears_states_and_transitions`):

```rust
#[tokio::test]
async fn create_type_autocreates_workflow_and_rejects_explicit() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfcreate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (status, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create type");
    assert_eq!(status, StatusCode::CREATED);
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().expect("workflow id")).unwrap();
    let (name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow row");
    assert_eq!(name, "Incident Workflow");
    let (defaults,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("default state");
    assert_eq!(defaults, 1);

    // Body `workflow` eksplisit ditolak dan tidak meninggalkan type orphan.
    let (status, body) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Problem".into()),
            workflow: Some(workflow_id),
            ..Default::default()
        }),
    )
    .await
    .expect("reject explicit workflow");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflows are managed through work item types");
    let (problem_types,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issue_types WHERE name = 'Problem' AND deleted_at IS NULL",
    )
    .fetch_one(&st.pool)
    .await
    .expect("rejected type rows");
    assert_eq!(problem_types, 0);

    // Epic tetap tanpa workflow.
    let (status, epic) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Epic Thing".into()),
            is_epic: Some(true),
            ..Default::default()
        }),
    )
    .await
    .expect("create epic");
    assert_eq!(status, StatusCode::CREATED);
    assert!(epic["workflow"].is_null());

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 create_type_autocreates_workflow_and_rejects_explicit`

Expected: FAIL — create dengan body `workflow` masih diterima / `workflow` null untuk non-epic.

- [ ] **Step 3: Implementasi**

Di `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`, ganti blok dari `if body.is_epic.unwrap_or(false) && body.workflow.is_some() {` (baris 304) sampai akhir `create_type` (baris 353) dengan:

```rust
    if body.is_epic.unwrap_or(false) && body.workflow.is_some() {
        return Ok(bad("Epic types cannot have a workflow"));
    }
    if body.workflow.is_some() {
        return Ok(bad("Workflows are managed through work item types"));
    }
    let is_epic = body.is_epic.unwrap_or(false);
    let is_active = body.is_active.unwrap_or(true);
    let mut tx = st.pool.begin().await?;
    // Type non-epic memiliki tepat satu workflow; dibuat/diadopsi dalam
    // transaksi yang sama supaya create yang gagal tidak meninggalkan orphan.
    let workflow_id: Option<uuid::Uuid> = if is_epic {
        None
    } else {
        Some(
            crate::routes::workflow::ensure_workflow_for_type(
                &mut tx,
                ws,
                name,
                is_active,
                user,
            )
            .await?,
        )
    };
    let row: V1WorkItemTypeRow = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
         is_active, level, workflow_id, external_id, external_source, workspace_id, created_by_id, \
         updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, false, $5, $6, $7, $8, $9, $10, $11, $11, now(), now()) \
         RETURNING id, name, description, logo_props, is_epic, is_default, is_active, \
         level::int AS level, workflow_id AS workflow, external_id, external_source, \
         created_by_id AS created_by, updated_by_id AS updated_by, workspace_id AS workspace, \
         created_at, updated_at, deleted_at, ARRAY[]::uuid[] AS project_ids",
    )
    .bind(name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.logo_props.clone().unwrap_or_else(|| json!({})))
    .bind(is_epic)
    .bind(is_active)
    .bind(body.level.unwrap_or(0) as f64)
    .bind(workflow_id)
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(ws)
    .bind(user)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    let linked = link_projects(st, &ws, row.id, user, &link_ids).await?;
    for pid in &linked {
        crate::routes::workflow::materialize_type_states(&st.pool, *pid, row.id).await?;
    }

    let row = reload(st, &ws, row.id)
        .await?
        .ok_or_else(|| common::errors::AppError::internal())?;
    Ok((StatusCode::CREATED, Json(v1_work_item_type_json(&row))))
}
```

Catatan: guard `workflow_link_conflict` lama dihapus dari blok ini; fungsinya baru dihapus di Task 6 setelah update_type juga berhenti memakainya.

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 create_type_autocreates_workflow_and_rejects_explicit`

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): auto-create one workflow per work item type"
```

### Task 4: Update type — tolak `workflow`, sync nama derived + active

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs:501-641` (`update_type`)
- Test: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan test berikut di `apps/api-rs/crates/api/tests/workflow_test.rs`:

```rust
#[tokio::test]
async fn update_type_rejects_workflow_and_syncs_derived_name() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfupdate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create");
    let type_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().unwrap()).unwrap();

    // Body `workflow` (null eksplisit) ditolak.
    let (status, body) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            workflow: Some(None),
            ..Default::default()
        }),
    )
    .await
    .expect("reject workflow body");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflows are managed through work item types");

    // Rename type → workflow ikut rename.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            name: Some("Major Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("rename");
    assert_eq!(status, StatusCode::OK);
    let (renamed,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow name");
    assert_eq!(renamed, "Major Incident Workflow");

    // Toggle active → workflow.is_active ikut.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            is_active: Some(false),
            ..Default::default()
        }),
    )
    .await
    .expect("deactivate");
    assert_eq!(status, StatusCode::OK);
    let (active,): (bool,) = sqlx::query_as("SELECT is_active FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow active");
    assert!(!active);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 update_type_rejects_workflow_and_syncs_derived_name`

Expected: FAIL — body `workflow: null` masih diproses dan rename belum sync.

- [ ] **Step 3: Implementasi**

Di `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`, ganti seluruh blok dari komentar `// Workflow: omitted (None) = unchanged...` (baris 501) sampai akhir `update_type` (baris 641) dengan:

```rust
    if body.workflow.is_some() {
        return Ok(bad("Workflows are managed through work item types"));
    }
    let current: Option<(Option<uuid::Uuid>, bool, bool, String)> = sqlx::query_as(
        "SELECT workflow_id, is_epic, is_active, name FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some((current_workflow, current_is_epic, current_is_active, current_name)) = current else {
        return Ok(missing());
    };
    let effective_is_epic = body.is_epic.unwrap_or(current_is_epic);
    let effective_is_active = body.is_active.unwrap_or(current_is_active);
    let effective_name = body
        .name
        .as_deref()
        .map(str::trim)
        .unwrap_or(current_name.as_str())
        .to_string();

    let mut tx = st.pool.begin().await?;
    // Column-by-column COALESCE so omitted fields are untouched; workflow_id
    // tidak lagi bagian dari payload.
    let updated = sqlx::query(
        "UPDATE issue_types SET \
         name = COALESCE($2, name), \
         description = COALESCE($3, description), \
         logo_props = COALESCE($4, logo_props), \
         is_epic = COALESCE($5, is_epic), \
         is_active = COALESCE($6, is_active), \
         level = COALESCE($7, level), \
         external_id = COALESCE($8, external_id), \
         external_source = COALESCE($9, external_source), \
         updated_by_id = $10, updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.description.clone())
    .bind(body.logo_props.clone())
    .bind(body.is_epic)
    .bind(body.is_active)
    .bind(body.level.map(|l| l as f64))
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(user)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(missing());
    }
    // Workflow mengikuti type: nama derived + is_active. Epic tidak punya.
    if let Some(workflow_id) = current_workflow {
        if !effective_is_epic {
            crate::routes::workflow::sync_type_workflow(
                &mut tx,
                ws,
                &effective_name,
                effective_is_active,
                workflow_id,
            )
            .await?;
        }
    }
    tx.commit().await?;

    // Materialize hanya untuk project yang di-link lewat request ini.
    if let Some(ids) = body.project_ids.as_ref() {
        let linked = link_projects(st, &ws, pk, user, ids).await?;
        for pid in &linked {
            crate::routes::workflow::materialize_type_states(&st.pool, *pid, pk).await?;
        }
    }
    match reload(st, &ws, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}
```

Pertahankan validasi nama di atas blok ini (baris 493-500) apa adanya.

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 update_type_rejects_workflow_and_syncs_derived_name`

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): sync workflow name and active state on type update"
```

### Task 5: Delete type cascade workflow

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs:255-272` (`delete_workspace`)
- Test: `apps/api-rs/crates/api/tests/workflow_test.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan test berikut di `apps/api-rs/crates/api/tests/workflow_test.rs`:

```rust
#[tokio::test]
async fn delete_type_cascades_workflow() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfdel2").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create");
    let type_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().unwrap()).unwrap();

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
    .expect("state");
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();
    let (_, states) = list_states(State(st.clone()), AuthUser(owner), Path((slug.clone(), workflow_id)))
        .await
        .expect("states");
    let default_id = Uuid::parse_str(states[0]["id"].as_str().unwrap()).unwrap();
    let _ = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: default_id,
            to_state_id: progress_id,
        }),
    )
    .await
    .expect("transition");

    let (status, _) = delete_workspace(State(st.clone()), AuthUser(owner), Path((slug.clone(), type_id)))
        .await
        .expect("delete type");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (workflow_live, states_live, transitions_live): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("cascade counts");
    assert_eq!(workflow_live, 0);
    assert_eq!(states_live, 0);
    assert_eq!(transitions_live, 0);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 2: Jalankan test untuk memastikan gagal**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 delete_type_cascades_workflow`

Expected: FAIL — workflow masih hidup setelah type dihapus.

- [ ] **Step 3: Implementasi**

Di `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`, ganti blok `let affected = sqlx::query(` sampai `Ok((StatusCode::NO_CONTENT, Json(Value::Null)))` di `delete_workspace` (baris 255-272) dengan:

```rust
    let workflow_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT workflow_id FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?
    .flatten();
    let mut tx = st.pool.begin().await?;
    let affected = sqlx::query(
        "UPDATE issue_types SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected == 0 {
        return Ok(missing());
    }
    sqlx::query("UPDATE project_issue_types SET deleted_at = now(), updated_at = now() WHERE issue_type_id = $1 AND deleted_at IS NULL")
        .bind(pk)
        .execute(&mut *tx)
        .await?;
    // Workflow dimiliki type: ikut ter-soft-delete beserta states + transitions.
    if let Some(workflow_id) = workflow_id {
        crate::routes::workflow::soft_delete_workflow_cascade(&mut tx, workflow_id).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 4: Jalankan test untuk memastikan lulus**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 delete_type_cascades_workflow`

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "feat(api-rs): cascade workflow delete with work item type"
```

### Task 6: Update test `import_materializes_and_unlink_guards`

**Files:**

- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs` (fungsi `import_materializes_and_unlink_guards`)

- [ ] **Step 1: Ganti blok policy `workflow: null` + konflik sharing**

Hapus blok mulai komentar `// Policy: explicit \`workflow: null\` pada type yang masih enabled → 400.`sampai`assert_eq!(links, 1, "import yang ditolak tidak boleh menambah link");` (dua blok: policy null + type kedua sharing workflow), ganti dengan:

```rust
    // Body `workflow` eksplisit ditolak (workflow dikelola lewat type).
    let explicit_workflow: V1UpdateWorkItemType =
        serde_json::from_value(json!({"workflow": null})).unwrap();
    let (status, body) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(explicit_workflow),
    )
    .await
    .expect("patch workflow");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflows are managed through work item types");

    // Type kedua dengan workflow sendiri boleh di-import ke project yang sama.
    let (status, second_type) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Problem".into()),
            project_ids: vec![project_id],
            ..Default::default()
        }),
    )
    .await
    .expect("second type");
    assert_eq!(status, StatusCode::CREATED);
    let second_type_id = Uuid::parse_str(second_type["id"].as_str().unwrap()).unwrap();
    let (links,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_issue_types WHERE project_id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await
    .expect("links after second import");
    assert_eq!(links, 2);
    // Bersihkan lagi supaya assertion map berikutnya (project kosong) tetap valid.
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, second_type_id)),
    )
    .await
    .expect("detach second");
    assert_eq!(status, StatusCode::NO_CONTENT);
```

- [ ] **Step 2: Ganti blok clear workflow + konflik batch dengan create API**

Hapus blok mulai komentar `// Explicit null saat tidak ada link hidup → 200 dan workflow dikosongkan.` sampai `assert_eq!(links, 1, "import batch yang ditolak tidak boleh menambah link");` (termasuk setup workflow kedua dan konflik batch), ganti dengan:

```rust
    // Create via API tanpa `workflow`: workflow derived + materialize.
    let (status, created_type) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Problem Type".into()),
            project_ids: vec![project_id],
            ..Default::default()
        }),
    )
    .await
    .expect("create with project_ids");
    assert_eq!(status, StatusCode::CREATED);
    let created_type_id = Uuid::parse_str(created_type["id"].as_str().unwrap()).unwrap();
    let created_workflow_id = Uuid::parse_str(created_type["workflow"].as_str().unwrap()).unwrap();
    let (workflow_name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(created_workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("derived workflow");
    assert_eq!(workflow_name, "Problem Type Workflow");
    let (created_mirrors,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(created_type_id)
    .fetch_one(&st.pool)
    .await
    .expect("created type mirrors");
    assert_eq!(
        created_mirrors, 1,
        "create dengan project_ids harus materialize"
    );

    // Update project_ids (workflow diwarisi) tetap idempotent.
    let (status, patched_type) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), created_type_id)),
        Json(V1UpdateWorkItemType {
            project_ids: Some(vec![project_id]),
            ..Default::default()
        }),
    )
    .await
    .expect("update with project_ids");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched_type["workflow"], created_workflow_id.to_string());
```

- [ ] **Step 3: Jalankan test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 import_materializes_and_unlink_guards`

Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "test(api-rs): cover derived workflow on type create and import"
```

### Task 7: Ganti test switch workflow dengan test lifecycle 1:1

**Files:**

- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs` (hapus `workflow_switch_guard_and_map_details`, tambah `type_workflow_lifecycle`)

- [ ] **Step 1: Hapus test lama**

Hapus seluruh fungsi `workflow_switch_guard_and_map_details` — dari `#[tokio::test]` tepat sebelum `async fn workflow_switch_guard_and_map_details() {` sampai `}` penutup fungsinya, tepat sebelum `#[tokio::test]` milik `async fn workflow_map_hides_inactive_types() {`. Test ini menguji switch/clear workflow yang sudah tidak ada.

- [ ] **Step 2: Tulis test pengganti (gagal sampai Task 9? tidak — sudah lulus dengan Task 3-5)**

Tambahkan test berikut di posisi yang sama:

```rust
#[tokio::test]
async fn type_workflow_lifecycle_syncs_name_active_and_delete() {
    let st = app_state().await;
    let (slug, _ws_id, project_id) = make_workspace(&st, "wflife").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Create type via API → workflow derived + default state.
    let (status, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create type");
    assert_eq!(status, StatusCode::CREATED);
    let type_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().unwrap()).unwrap();
    let (name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow name");
    assert_eq!(name, "Incident Workflow");

    // Tambah state + transisi lalu import ke project; cek detail workflow-map.
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
    .expect("state");
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();
    let (_, states) = list_states(State(st.clone()), AuthUser(owner), Path((slug.clone(), workflow_id)))
        .await
        .expect("states");
    let default_id = Uuid::parse_str(states[0]["id"].as_str().unwrap()).unwrap();
    let _ = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(default_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("transition");
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(json!({"work_item_types": [type_id]})),
    )
    .await
    .expect("import");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(map["types"][0]["type_name"], "Incident");
    assert_eq!(map["types"][0]["states"].as_array().unwrap().len(), 2);
    assert_eq!(map["types"][0]["transitions"].as_array().unwrap().len(), 1);

    // Rename type → workflow ikut rename.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            name: Some("Major Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("rename");
    assert_eq!(status, StatusCode::OK);
    let (renamed,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("renamed workflow");
    assert_eq!(renamed, "Major Incident Workflow");

    // Deactivate type → workflow.is_active ikut + map menyembunyikan type.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            is_active: Some(false),
            ..Default::default()
        }),
    )
    .await
    .expect("deactivate");
    assert_eq!(status, StatusCode::OK);
    let (active,): (bool,) = sqlx::query_as("SELECT is_active FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow active");
    assert!(!active);
    let (_, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map inactive");
    assert!(map["types"].as_array().unwrap().is_empty());

    // Delete type (detach dulu) → workflow + states + transitions soft-delete.
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, type_id)),
    )
    .await
    .expect("detach");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = delete_workspace(State(st.clone()), AuthUser(owner), Path((slug.clone(), type_id)))
        .await
        .expect("delete type");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (workflow_live, states_live, transitions_live): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("cascade counts");
    assert_eq!(workflow_live, 0);
    assert_eq!(states_live, 0);
    assert_eq!(transitions_live, 0);

    purge(&st.pool, &slug).await;
}
```

- [ ] **Step 3: Jalankan test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 type_workflow_lifecycle_syncs_name_active_and_delete`

Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "test(api-rs): cover type workflow lifecycle end to end"
```

### Task 8: Update test `unlink_cross_workspace_and_switch_with_issues`

**Files:**

- Modify: `apps/api-rs/crates/api/tests/workflow_test.rs` (fungsi `unlink_cross_workspace_and_switch_with_issues`)

- [ ] **Step 1: Rename + hapus setup workflow kedua**

Ganti nama fungsi menjadi `unlink_cross_workspace_and_project_delete_guards`. Hapus blok pembuatan `wf_a2` — dari `let (_, wf_a2) = create_workflow(` sampai `.expect("A2 state");`.

- [ ] **Step 2: Hapus blok switch dengan live issue**

Hapus blok mulai komentar `// Live issue bertipe T memblokir switch workflow.` — namun **pertahankan** pembuatan `state_id` dan `issue_id` (dipakai assertion project-delete di bawah). Hapus hanya blok `let (status, body) = update_workspace(...)` beserta dua assertion-nya:

```rust
    let (status, body) = update_workspace(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), type_t)),
        Json(V1UpdateWorkItemType {
            workflow: Some(Some(wf_a2_id)),
            ..Default::default()
        }),
    )
    .await
    .expect("switch with live issues");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Cannot change the workflow while the type has live work items"
    );
```

Ganti komentar blok yang tersisa (`// Live issue bertipe T memblokir switch workflow.`) dengan `// Live issue bertipe T memblokir project-scope delete.`

- [ ] **Step 3: Jalankan test**

Run: `DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1 unlink_cross_workspace_and_project_delete_guards`

Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/tests/workflow_test.rs
git commit -m "test(api-rs): drop workflow switch coverage from unlink guards test"
```

### Task 9: Hapus dead code + tutup route workflow standalone

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs` (hapus `workflow_link_conflict`, guard import)
- Modify: `apps/api-rs/crates/api/src/main.rs:657-683` (route workflow)

- [ ] **Step 1: Hapus `workflow_link_conflict` + guard import**

Di `apps/api-rs/crates/api/src/routes/v1/work_item_type.rs`:

1. Hapus fungsi `workflow_link_conflict` — dari doc comment `/// Guard invariant "satu workflow per type per project" untuk jalur link` sampai `}` penutupnya, tepat sebelum doc comment `/// \`true\` bila \`workflow_id\` adalah workflow hidup di \`workspace_id\`.`.
2. Di `import_to_project`, hapus blok mulai komentar `// Guard sebelum link: satu workflow hanya boleh dipakai satu type hidup` sampai blok `if batch_conflict || project_conflict { ... }` (termasuk kedua query `batch_conflict`/`project_conflict`).

- [ ] **Step 2: Tutup route workflow standalone di `main.rs`**

Di `apps/api-rs/crates/api/src/main.rs`, ganti dua blok route workflow (baris 658-667) dengan:

```rust
        // Work item types + workflows (internal cookie-auth). Sejak kontrak 1:1
        // (spec 2026-09-28-service-management-single-page-design.md), workflow
        // dibuat/diubah/dihapus lewat work item type; hanya GET yang di-route.
        // Handler create/patch/delete tetap ada sebagai fixture test.
        .route(
            "/api/workspaces/:slug/workflows/",
            get(routes::workflow::list_workflows),
        )
        .route(
            "/api/workspaces/:slug/workflows/:workflow_id/",
            get(routes::workflow::retrieve_workflow),
        )
```

- [ ] **Step 3: Jalankan seluruh suite terkait (serial)**

Run:

```bash
cd apps/api-rs
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workflow_transition_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test v1_work_item_type_test
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test workspace_seed_test -- --test-threads=1
```

Expected: semua PASS, tanpa warning dead code baru.

- [ ] **Step 4: Build + lint Rust**

Run:

```bash
cd apps/api-rs
cargo build
cargo clippy -p api --all-targets -- -D warnings
```

Expected: sukses; tidak ada warning unused/dead code baru (hapus import yang jadi tidak terpakai bila ada).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/v1/work_item_type.rs apps/api-rs/crates/api/src/main.rs
git commit -m "refactor(api-rs): close standalone workflow routes"
```

### Task 10: Verifikasi end-to-end backend

- [ ] **Step 1: Apply migrasi Django ke DB dev**

```bash
docker compose -f docker-compose-local.yml run --rm migrator
docker compose -f docker-compose-local.yml exec -T plane-db psql -U plane -d plane -tAc \
  "SELECT name FROM django_migrations WHERE app='db' ORDER BY id DESC LIMIT 1"
```

Expected: baris terakhir `0125_normalize_type_workflows`.

- [ ] **Step 2: Verifikasi data hasil migrasi (read-only)**

```bash
docker exec -i plane-for-itsm-plane-db-1 psql -U plane -d plane -P pager=off -c "
SELECT it.name AS type, wf.name AS workflow,
       (SELECT COUNT(*) FROM workflow_states ws WHERE ws.workflow_id = wf.id
          AND ws.deleted_at IS NULL AND ws.is_default) AS defaults
FROM issue_types it
LEFT JOIN workflows wf ON wf.id = it.workflow_id AND wf.deleted_at IS NULL
WHERE it.workspace_id = (SELECT id FROM workspaces WHERE slug='itsm')
  AND it.deleted_at IS NULL ORDER BY it.name;"
```

Expected: `Request` → `Request Workflow` dengan `defaults = 1`; semua type non-epic punya workflow.

- [ ] **Step 3: Rebuild api-rs**

Ikuti `AGENTS.md` (build Rust LTO bisa 10+ menit tanpa output — jangan abort):

```bash
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-singlepage.log 2>&1 < /dev/null &
```

Poll `docker compose -f docker-compose-local.yml ps` sampai `api` sehat, lalu:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8000/health
```

Expected: `200`.

- [ ] **Step 4: Smoke route workflow**

```bash
curl -s -X POST -o /dev/null -w '%{http_code}\n' http://localhost:8000/api/workspaces/itsm/workflows/
```

Expected: `404`/`405` (route POST ditutup), bukan `201`. `GET` yang sama boleh `401` tanpa cookie — yang penting route-nya masih ada untuk read-only.

- [ ] **Step 5: Restart live + lanjut ke plan web**

```bash
systemctl --user restart plane-live.service
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3100/live/health/
```

Expected: `200` (cold start ~8 detik).

Backend selesai. Plan frontend: `docs/superpowers/plans/2026-09-28-service-management-single-page-web.md`.
