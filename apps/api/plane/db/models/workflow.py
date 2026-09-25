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
