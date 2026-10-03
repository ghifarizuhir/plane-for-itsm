# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

import importlib

import pytest
from django.apps import apps as django_apps

from plane.db.models import DraftIssue, Issue, IssueVersion, Project, State
from plane.tests.factories import ProjectFactory, WorkspaceFactory

flatten = importlib.import_module("plane.db.migrations.0126_remove_workflows_and_flatten_states")


def make_state(project, name, group, default=False, **kwargs):
    return State.objects.create(project=project, name=name, color="#60646C", group=group, default=default, **kwargs)


@pytest.mark.unit
class TestFlattenProjectStates:
    @pytest.mark.django_db
    def test_reset_recreates_defaults_and_remaps(self):
        workspace = WorkspaceFactory()
        project = ProjectFactory(workspace=workspace)
        old_default = make_state(project, "New", "backlog", default=True)
        make_state(project, "Triage", "triage")
        issue = Issue.objects.create(project=project, name="Incident 1", state=old_default)
        draft = DraftIssue.objects.create(project=project, name="Draft 1", state=old_default)
        version = IssueVersion.objects.create(
            project=project,
            issue=issue,
            owned_by=workspace.owner,
            name=issue.name,
            state=old_default.id,
        )
        Project.objects.filter(id=project.id).update(default_state_id=old_default.id)

        flatten.flatten_project_states(django_apps, None)

        states = State.all_state_objects.filter(project=project)
        assert states.count() == 6
        assert set(states.values_list("group", flat=True)) == {
            "backlog",
            "unstarted",
            "started",
            "completed",
            "cancelled",
            "triage",
        }
        backlog = states.get(group="backlog")
        assert backlog.default is True
        assert states.filter(default=True).count() == 1
        project.refresh_from_db()
        assert project.default_state_id == backlog.id
        issue.refresh_from_db()
        draft.refresh_from_db()
        version.refresh_from_db()
        assert issue.state_id == backlog.id
        assert draft.state_id == backlog.id
        assert version.state is None

    @pytest.mark.django_db
    def test_reset_is_idempotent(self):
        project = ProjectFactory()
        make_state(project, "New", "backlog", default=True)

        flatten.flatten_project_states(django_apps, None)
        flatten.flatten_project_states(django_apps, None)

        states = State.all_state_objects.filter(project=project)
        assert states.count() == 6
        assert states.filter(default=True).count() == 1
