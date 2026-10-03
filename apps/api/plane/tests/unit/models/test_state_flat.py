# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

import pytest
from django.db import IntegrityError

from plane.db.models import State
from plane.tests.factories import ProjectFactory, WorkspaceFactory


def make_state(project, name, group, default=False):
    return State.objects.create(project=project, name=name, color="#60646C", group=group, default=default)


@pytest.mark.unit
class TestStateFlatConstraints:
    @pytest.mark.django_db
    def test_name_unique_per_project(self):
        project = ProjectFactory()
        make_state(project, "Backlog", "backlog", default=True)
        with pytest.raises(IntegrityError):
            make_state(project, "Backlog", "started")

    @pytest.mark.django_db
    def test_one_default_per_project(self):
        project = ProjectFactory()
        make_state(project, "Backlog", "backlog", default=True)
        with pytest.raises(IntegrityError):
            make_state(project, "Other", "backlog", default=True)

    @pytest.mark.django_db
    def test_same_name_allowed_across_projects(self):
        # NOTE: ProjectFactory memakai django_get_or_create; Sequence tidak
        # selalu advance antar build dalam satu test, jadi pakai nama eksplisit.
        workspace = WorkspaceFactory()
        first = ProjectFactory(workspace=workspace, name="Project Alpha", identifier="PA")
        second = ProjectFactory(workspace=workspace, name="Project Beta", identifier="PB")
        make_state(first, "Backlog", "backlog", default=True)
        make_state(second, "Backlog", "backlog", default=True)
        assert State.objects.filter(name="Backlog").count() == 2


@pytest.mark.unit
class TestDefaultStateResolution:
    @pytest.mark.django_db
    def test_issue_uses_project_default_state(self):
        from plane.db.models import Issue

        project = ProjectFactory()
        backlog = make_state(project, "Backlog", "backlog", default=True)
        make_state(project, "Todo", "unstarted")
        issue = Issue.objects.create(project=project, name="No state")
        assert issue.state_id == backlog.id

    @pytest.mark.django_db
    def test_draft_uses_project_default_state(self):
        from plane.db.models import DraftIssue

        project = ProjectFactory()
        backlog = make_state(project, "Backlog", "backlog", default=True)
        make_state(project, "Todo", "unstarted")
        draft = DraftIssue.objects.create(project=project, name="Draft no state")
        assert draft.state_id == backlog.id
