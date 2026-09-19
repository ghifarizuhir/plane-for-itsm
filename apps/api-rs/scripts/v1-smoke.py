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

    # ---- sub-resources ------------------------------------------------------
    from plane.models.work_items import CreateWorkItemComment  # noqa: E402

    comment = client.work_items.comments.create(
        workspace_slug=WS,
        project_id=pid,
        work_item_id=created.id,
        data=CreateWorkItemComment(comment_html="<p>v1-smoke comment</p>"),
    )
    assert comment.id, "comment create missing id"
    print(f"comment create OK: {comment.id}")

    comments = client.work_items.comments.list(
        workspace_slug=WS, project_id=pid, work_item_id=created.id
    )
    assert comments.total_count >= 1, "comment list missing created comment"
    print(f"comment list OK: total_count={comments.total_count}")

    client.work_items.comments.delete(
        workspace_slug=WS,
        project_id=pid,
        work_item_id=created.id,
        comment_id=comment.id,
    )
    print("comment delete OK")

    links = client.work_items.links.list(
        workspace_slug=WS, project_id=pid, work_item_id=created.id
    )
    assert hasattr(links, "total_count") and isinstance(links.results, list)
    print(f"links list OK: total_count={links.total_count}")

    acts = client.work_items.activities.list(
        workspace_slug=WS, project_id=pid, work_item_id=created.id
    )
    assert all(a.project and a.workspace for a in acts.results)
    print(f"activities list OK: {len(acts.results)} rows")

    atts = client.work_items.attachments.list(
        workspace_slug=WS, project_id=pid, work_item_id=created.id
    )
    assert isinstance(atts, list), "attachments list must be a list"
    print(f"attachments list OK: {len(atts)} rows")

    deps = client.work_items.dependencies.list(
        workspace_slug=WS, project_id=pid, work_item_id=created.id
    )
    assert hasattr(deps, "blocking"), "dependencies missing blocking"
    print("dependencies list OK: has blocking")

    crels = client.work_items.custom_relations.list(
        workspace_slug=WS, project_id=pid, work_item_id=created.id
    )
    assert isinstance(crels, dict), "custom relations must be a dict"
    print(f"custom relations list OK: {len(crels)} groups")

    # Archive/unarchive: archive only accepts completed/cancelled states, but
    # v1 exposes no states-listing endpoint to move the item there
    # deterministically. Exercise the route and report the state-group 400
    # rather than skipping silently; run unarchive only on success.
    from plane.errors.errors import HttpError  # noqa: E402

    try:
        client.work_items.archive(WS, pid, created.id)
    except HttpError as e:
        print(f"workitem archive reported HTTP {e.status_code} (state group not terminal)")
    else:
        archived = client.work_items.list_archived(WS, pid)
        assert any(i.id == created.id for i in archived.results), "archived item not listed"
        print("workitem archive OK")
        client.work_items.unarchive(WS, pid, created.id)
        print("workitem unarchive OK")
finally:
    client.work_items.delete(WS, pid, created.id)
    print("workitem delete OK")

# ---- Phase-4b: work item types + workspace features --------------------------
from plane.models.work_item_types import CreateWorkItemType  # noqa: E402

ws_feats = client.workspaces.get_features(WS)
assert ws_feats.work_item_types is False, (
    f"workspace features work_item_types expected False, got {ws_feats.work_item_types}"
)
print(f"workspace features OK: work_item_types={ws_feats.work_item_types}")

ws_types = client.workspace_work_item_types.list(WS)
assert isinstance(ws_types, list), "workspace type list must be a list"
print(f"workspace type list OK: {len(ws_types)} rows")

smoke_type = client.workspace_work_item_types.create(
    WS, CreateWorkItemType(name="Smoke Type")
)
assert smoke_type.id and smoke_type.name == "Smoke Type", "workspace type create mismatch"
print(f"workspace type create OK: {smoke_type.id}")

try:
    fetched = client.workspace_work_item_types.retrieve(WS, smoke_type.id)
    assert fetched.id == smoke_type.id, "workspace type retrieve id mismatch"
    print("workspace type retrieve OK")

    client.work_item_types.import_to_project(WS, pid, [smoke_type.id])
    proj_types = client.work_item_types.list(WS, pid)
    assert any(t.id == smoke_type.id for t in proj_types), (
        "imported type not in project list"
    )
    print(f"project type import OK: {len(proj_types)} rows")

    client.work_item_types.delete(WS, pid, smoke_type.id)
    print("project type delete (detach) OK")
finally:
    try:
        client.workspace_work_item_types.delete(WS, smoke_type.id)
        print("workspace type delete OK")
    except Exception as e:
        print(f"workspace type delete skipped: {e}")

print("v1 smoke passed")
