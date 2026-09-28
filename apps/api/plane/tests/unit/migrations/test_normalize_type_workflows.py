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
