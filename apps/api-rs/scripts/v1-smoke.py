#!/usr/bin/env python3
"""Contract smoke: drive plane-sdk against the local Rust /api/v1.

Usage:
  V1_TOKEN=plane_api_... V1_WS=itsm /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
Reads the SDK from plane-mcp-server/.venv (PLANE_SDK_PATH can override).
"""
import os
import sys

SDK = os.environ.get("PLANE_SDK_PATH", "/home/ghifari/plane-mcp-server/.venv/lib/python3.11/site-packages")
sys.path.insert(0, SDK)

from plane import PlaneClient  # noqa: E402

BASE = os.environ.get("V1_BASE", "http://localhost:8000")
TOKEN = os.environ["V1_TOKEN"]
WS = os.environ["V1_WS"]

client = PlaneClient(base_url=BASE, api_key=TOKEN)

lite = client.projects.list_lite(WS)
assert lite.total_count >= 0, "projects-lite missing envelope fields"
print(f"projects-lite OK: total_count={lite.total_count}")

if lite.results:
    pid = lite.results[0].id
    project = client.projects.retrieve(WS, pid)
    assert project.name and project.identifier, "retrieve missing required fields"
    print(f"project retrieve OK: {project.identifier} {project.name}")
    feats = client.projects.get_features(WS, pid)
    print(f"project features OK: work_item_types={feats.work_item_types}")
    wl = client.projects.get_worklog_summary(WS, pid)
    assert isinstance(wl, list), "worklog summary must be a list"
    print(f"worklog summary OK: {len(wl)} rows")

# ---- work items -----------------------------------------------------------
from plane.models.work_items import CreateWorkItem, UpdateWorkItem  # noqa: E402

pid = lite.results[0].id
page = client.work_items.list(WS, pid)
assert hasattr(page, "total_count"), "work item list missing envelope"
print(f"workitem list OK: total_count={page.total_count}")

created = client.work_items.create(
    WS, pid, CreateWorkItem(name="v1-smoke item", priority="low")
)
assert created.id and created.name == "v1-smoke item"
print(f"workitem create OK: {created.id}")

try:
    detail = client.work_items.retrieve(WS, pid, created.id)
    assert detail.id == created.id
    assert detail.description_html is not None
    print("workitem retrieve OK")

    updated = client.work_items.update(WS, pid, created.id, UpdateWorkItem(priority="high"))
    print(f"workitem update OK: priority={updated.priority}")

    found = client.work_items.search(WS, "v1-smoke")
    assert any(i.id == created.id for i in found.issues), "search did not find created item"
    print(f"workitem search OK: {len(found.issues)} hits")

    cnt = client.work_items.count_workspace(WS)
    assert cnt.total_count >= 1
    print(f"workitem count OK: total_count={cnt.total_count}")

    proj_ident = client.projects.retrieve(WS, pid).identifier
    by_ident = client.work_items.retrieve_by_identifier(WS, proj_ident, detail.sequence_id)
    assert by_ident.id == created.id
    print("workitem retrieve_by_identifier OK")
finally:
    client.work_items.delete(WS, pid, created.id)
    print("workitem delete OK")

print("v1 smoke passed")
