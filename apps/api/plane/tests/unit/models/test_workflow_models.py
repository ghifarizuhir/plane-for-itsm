# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

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
