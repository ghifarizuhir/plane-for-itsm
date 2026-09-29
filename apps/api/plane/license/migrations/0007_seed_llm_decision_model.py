# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

# Django imports
from django.db import migrations


def seed_llm_decision_model(apps, schema_editor):
    InstanceConfiguration = apps.get_model("license", "InstanceConfiguration")
    InstanceConfiguration.objects.get_or_create(
        key="LLM_DECISION_MODEL",
        defaults={"value": "", "category": "AI", "is_encrypted": False},
    )


def unseed_llm_decision_model(apps, schema_editor):
    InstanceConfiguration = apps.get_model("license", "InstanceConfiguration")
    InstanceConfiguration.objects.filter(key="LLM_DECISION_MODEL").delete()


class Migration(migrations.Migration):
    dependencies = [("license", "0006_instance_is_current_version_deprecated")]

    operations = [
        migrations.RunPython(seed_llm_decision_model, unseed_llm_decision_model),
    ]
