# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

import pytest
from django.db import IntegrityError
from django.utils import timezone

from plane.db.models import IssueType, State, StateGroup, Workflow, WorkflowState, WorkflowTransition
from plane.tests.factories import ProjectFactory, WorkspaceFactory


def make_workflow():
    workspace = WorkspaceFactory()
    workflow = Workflow.objects.create(workspace=workspace, name="Incident Workflow")
    return workspace, workflow


def make_project():
    workspace = WorkspaceFactory()
    project = ProjectFactory(workspace=workspace)
    return workspace, project


@pytest.mark.unit
class TestWorkflowModels:
    @pytest.mark.django_db
    def test_state_default_unique_per_workflow(self):
        _, workflow = make_workflow()
        WorkflowState.objects.create(workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True)
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
        state = WorkflowState.objects.create(workflow=workflow, name="In Progress", color="#F59E0B", group="started")
        assert state.slug == "in-progress"

    @pytest.mark.django_db
    def test_transition_pair_unique_and_self_rejected(self):
        _, workflow = make_workflow()
        new = WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group="backlog", is_default=True
        )
        progress = WorkflowState.objects.create(workflow=workflow, name="In Progress", color="#F59E0B", group="started")
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

    @pytest.mark.django_db
    def test_state_name_reusable_after_soft_delete(self):
        _, workflow = make_workflow()
        state = WorkflowState.objects.create(workflow=workflow, name="New", color="#60646C", group=StateGroup.BACKLOG)
        state.deleted_at = timezone.now()
        state.save()
        WorkflowState.objects.create(workflow=workflow, name="New", color="#F59E0B", group=StateGroup.STARTED)

    @pytest.mark.django_db
    def test_state_default_and_name_scoped_per_workflow(self):
        workspace = WorkspaceFactory()
        workflow_a = Workflow.objects.create(workspace=workspace, name="Workflow A")
        workflow_b = Workflow.objects.create(workspace=workspace, name="Workflow B")
        WorkflowState.objects.create(
            workflow=workflow_a, name="New", color="#60646C", group=StateGroup.BACKLOG, is_default=True
        )
        WorkflowState.objects.create(
            workflow=workflow_b, name="New", color="#60646C", group=StateGroup.BACKLOG, is_default=True
        )

    @pytest.mark.django_db
    def test_transition_pair_reusable_after_soft_delete(self):
        _, workflow = make_workflow()
        new = WorkflowState.objects.create(
            workflow=workflow, name="New", color="#60646C", group=StateGroup.BACKLOG, is_default=True
        )
        progress = WorkflowState.objects.create(
            workflow=workflow, name="In Progress", color="#F59E0B", group=StateGroup.STARTED
        )
        transition = WorkflowTransition.objects.create(workflow=workflow, from_state=new, to_state=progress)
        transition.deleted_at = timezone.now()
        transition.save()
        WorkflowTransition.objects.create(workflow=workflow, from_state=new, to_state=progress)


@pytest.mark.unit
class TestStateTypeScoping:
    @pytest.mark.django_db
    def test_same_state_name_allowed_across_types(self):
        workspace, project = make_project()
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        problem = IssueType.objects.create(workspace=workspace, name="Problem")
        State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=incident)
        State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=problem)
        State.objects.create(project=project, name="Closed", color="#46A758", group="completed", type=None)

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
        problem = IssueType.objects.create(workspace=workspace, name="Problem")
        State.objects.create(project=project, name="New", color="#60646C", group="backlog", type=incident, default=True)
        State.objects.create(project=project, name="New", color="#60646C", group="backlog", type=problem, default=True)
        with pytest.raises(IntegrityError):
            State.objects.create(
                project=project, name="Other", color="#60646C", group="backlog", type=incident, default=True
            )

    @pytest.mark.django_db
    def test_workflow_state_mirror_unique_per_project(self):
        workspace, project = make_project()
        incident = IssueType.objects.create(workspace=workspace, name="Incident")
        problem = IssueType.objects.create(workspace=workspace, name="Problem")
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
                project=project,
                name="New Mirror",
                color="#60646C",
                group="backlog",
                type=problem,
                workflow_state=wf_state,
            )
