# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

# Django imports
from django.db import models

# Module imports
from .base import BaseModel


class WorkspaceDocumentSettings(BaseModel):
    workspace = models.ForeignKey("db.Workspace", related_name="document_settings", on_delete=models.CASCADE)
    header_logo_mode = models.CharField(max_length=16, default="none")
    header_logo_asset = models.ForeignKey(
        "db.FileAsset", related_name="+", on_delete=models.SET_NULL, null=True, blank=True
    )
    header_text_lines = models.JSONField(default=list)
    footer_text_lines = models.JSONField(default=list)
    show_page_numbers = models.BooleanField(default=True)
    show_generated_by = models.BooleanField(default=True)

    class Meta:
        verbose_name = "Workspace Document Settings"
        verbose_name_plural = "Workspace Document Settings"
        db_table = "workspace_document_settings"

    def __str__(self):
        return f"{self.workspace} - document settings"
